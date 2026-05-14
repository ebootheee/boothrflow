//! Real `AudioSource` impl backed by `cpal` (WASAPI shared mode on Windows,
//! CoreAudio on macOS, ALSA/PulseAudio on Linux).
//!
//! Architecture:
//!
//! ```text
//! cpal callback (real-time) ──► mono mixdown ──► input ring buffer
//!                                                       │
//!                                                       ▼
//!                                            FftFixedOut resampler
//!                                                       │
//!                                                       ▼
//!                                       16kHz mono frame (480 samples)
//!                                                       │
//!                                                       ▼
//!                                          bounded crossbeam channel
//! ```
//!
//! The `cpal::Stream` is `!Send + !Sync`, so we hold it on a dedicated thread
//! that blocks on a `stop_rx` signal. Dropping the stream stops capture.

use std::sync::Arc;
use std::thread;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleFormat, StreamConfig};
use crossbeam_channel::{bounded, Receiver, Sender};
use parking_lot::Mutex;
use rubato::{FftFixedOut, Resampler};
use serde::Serialize;

use crate::audio::{AudioFrame, AudioSource};
use crate::error::{BoothError, Result};

const TARGET_RATE: u32 = 16_000;
/// 32ms at 16kHz. Matches Silero VAD's preferred chunk size for 16kHz
/// (512 samples), keeping the audio → VAD path zero-buffer.
const TARGET_FRAME_SAMPLES: usize = 512;

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct MicDevice {
    pub name: String,
    pub default_sample_rate: u32,
    pub channels: u16,
    pub is_default: bool,
}

pub struct CpalAudioSource {
    device_name: Mutex<Option<String>>,
    stop_tx: Mutex<Option<Sender<()>>>,
}

impl Default for CpalAudioSource {
    fn default() -> Self {
        Self::new()
    }
}

impl CpalAudioSource {
    pub fn new() -> Self {
        Self {
            device_name: Mutex::new(None),
            stop_tx: Mutex::new(None),
        }
    }

    /// Pin a specific input device by name. `None` means system default.
    pub fn set_device(&self, name: Option<String>) {
        *self.device_name.lock() = name;
    }

    /// Enumerate input devices for the Settings UI.
    pub fn list_devices() -> Result<Vec<MicDevice>> {
        let host = cpal::default_host();
        let default_name = host.default_input_device().and_then(|d| d.name().ok());

        let devices = host
            .input_devices()
            .map_err(|e| BoothError::AudioCapture(format!("input_devices: {e}")))?;

        let mut out = Vec::new();
        for d in devices {
            let Ok(name) = d.name() else { continue };
            let cfg = d.default_input_config().ok();
            out.push(MicDevice {
                is_default: default_name.as_ref() == Some(&name),
                name,
                default_sample_rate: cfg.as_ref().map(|c| c.sample_rate().0).unwrap_or(0),
                channels: cfg.as_ref().map(|c| c.channels()).unwrap_or(0),
            });
        }
        Ok(out)
    }
}

impl AudioSource for CpalAudioSource {
    fn start(&self) -> Result<Receiver<AudioFrame>> {
        if self.stop_tx.lock().is_some() {
            return Err(BoothError::AudioCapture("already capturing".into()));
        }

        let (frame_tx, frame_rx) = bounded::<AudioFrame>(128);
        let (stop_tx, stop_rx) = bounded::<()>(1);
        // Resolution order:
        //   1. `set_device(Some(name))` pin (rarely used outside tests).
        //   2. Settings override (`audio_input_device`).
        //   3. Bluetooth-aware fallback: if the system default input is a
        //      Bluetooth mic and the user hasn't disabled the toggle,
        //      switch to a built-in mic. Avoids the macOS HFP downgrade
        //      that dims any music playing through the same headphones.
        //   4. System default.
        let pinned = self.device_name.lock().clone();
        let device_name = pinned.or_else(resolve_input_device_from_settings);

        thread::Builder::new()
            .name("boothrflow-audio".into())
            .spawn(move || {
                if let Err(e) = run_capture(device_name, frame_tx, stop_rx) {
                    tracing::error!("audio capture thread errored: {e}");
                }
            })
            .map_err(|e| BoothError::AudioCapture(format!("spawn: {e}")))?;

        *self.stop_tx.lock() = Some(stop_tx);
        Ok(frame_rx)
    }

    fn stop(&self) -> Result<()> {
        if let Some(tx) = self.stop_tx.lock().take() {
            // Best-effort signal — receiver may have already exited.
            let _ = tx.send(());
        }
        Ok(())
    }
}

fn run_capture(
    device_name: Option<String>,
    frame_tx: Sender<AudioFrame>,
    stop_rx: Receiver<()>,
) -> Result<()> {
    let host = cpal::default_host();

    // If a specific device name was requested but isn't available
    // (unplugged AirPods, settings stale), fall through to system
    // default rather than failing the dictation outright. The user
    // gets a working capture; the log line surfaces the fallback so
    // we can debug if they report unexpected mic.
    let device = match device_name {
        Some(name) => match find_device_by_name(&host, &name) {
            Ok(d) => d,
            Err(_) => {
                tracing::warn!(
                    "audio: requested device {:?} not found — falling back to system default",
                    name
                );
                host.default_input_device()
                    .ok_or_else(|| BoothError::AudioCapture("no default input device".into()))?
            }
        },
        None => host
            .default_input_device()
            .ok_or_else(|| BoothError::AudioCapture("no default input device".into()))?,
    };

    let device_label = device.name().unwrap_or_else(|_| "<unnamed>".into());

    let config = device
        .default_input_config()
        .map_err(|e| BoothError::AudioCapture(format!("default_input_config: {e}")))?;

    let sample_rate = config.sample_rate().0;
    let channels = config.channels();
    let sample_format = config.sample_format();
    let stream_config: StreamConfig = config.into();

    tracing::info!(
        "audio: device={} rate={} channels={} format={:?}",
        device_label,
        sample_rate,
        channels,
        sample_format
    );

    let resampler = FftFixedOut::<f32>::new(
        sample_rate as usize,
        TARGET_RATE as usize,
        TARGET_FRAME_SAMPLES,
        1,
        1,
    )
    .map_err(|e| BoothError::AudioCapture(format!("resampler init: {e}")))?;

    let input_chunk = resampler.input_frames_next();
    let mono_buf: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::with_capacity(input_chunk * 4)));
    let resampler = Arc::new(Mutex::new(resampler));

    let mono_buf_cb = mono_buf.clone();
    let resampler_cb = resampler.clone();
    let frame_tx_cb = frame_tx.clone();
    let channels_us = channels as usize;

    let err_fn = |err| tracing::error!("cpal stream error: {err}");

    let stream = match sample_format {
        SampleFormat::F32 => device.build_input_stream(
            &stream_config,
            move |data: &[f32], _: &_| {
                process_input_f32(
                    data,
                    channels_us,
                    &mono_buf_cb,
                    &resampler_cb,
                    &frame_tx_cb,
                    input_chunk,
                );
            },
            err_fn,
            None,
        ),
        SampleFormat::I16 => device.build_input_stream(
            &stream_config,
            move |data: &[i16], _: &_| {
                let f32_data: Vec<f32> = data
                    .iter()
                    .map(|s| f32::from(*s) / f32::from(i16::MAX))
                    .collect();
                process_input_f32(
                    &f32_data,
                    channels_us,
                    &mono_buf_cb,
                    &resampler_cb,
                    &frame_tx_cb,
                    input_chunk,
                );
            },
            err_fn,
            None,
        ),
        SampleFormat::U16 => device.build_input_stream(
            &stream_config,
            move |data: &[u16], _: &_| {
                let f32_data: Vec<f32> = data
                    .iter()
                    .map(|s| (f32::from(*s) - 32_768.0) / 32_768.0)
                    .collect();
                process_input_f32(
                    &f32_data,
                    channels_us,
                    &mono_buf_cb,
                    &resampler_cb,
                    &frame_tx_cb,
                    input_chunk,
                );
            },
            err_fn,
            None,
        ),
        other => {
            return Err(BoothError::AudioCapture(format!(
                "unsupported sample format: {other:?}"
            )))
        }
    }
    .map_err(|e| BoothError::AudioCapture(format!("build_input_stream: {e}")))?;

    stream
        .play()
        .map_err(|e| BoothError::AudioCapture(format!("stream.play: {e}")))?;

    let _ = stop_rx.recv();
    drop(frame_tx);
    Ok(())
}

fn process_input_f32(
    data: &[f32],
    channels: usize,
    mono_buf: &Mutex<Vec<f32>>,
    resampler: &Mutex<FftFixedOut<f32>>,
    frame_tx: &Sender<AudioFrame>,
    input_chunk: usize,
) {
    let mut buf = mono_buf.lock();

    if channels <= 1 {
        buf.extend_from_slice(data);
    } else {
        let inv = 1.0 / channels as f32;
        for frame in data.chunks_exact(channels) {
            let sum: f32 = frame.iter().sum();
            buf.push(sum * inv);
        }
    }

    while buf.len() >= input_chunk {
        let chunk: Vec<f32> = buf.drain(..input_chunk).collect();
        let mut rs = resampler.lock();
        match rs.process(&[&chunk[..]], None) {
            Ok(mut out) => {
                if let Some(out_mono) = out.pop() {
                    if !out_mono.is_empty() && frame_tx.try_send(out_mono).is_err() {
                        tracing::warn!("audio: consumer slow, dropping a 30ms frame");
                    }
                }
            }
            Err(e) => tracing::error!("resample error: {e}"),
        }
    }
}

fn find_device_by_name(host: &cpal::Host, name: &str) -> Result<Device> {
    let devices = host
        .input_devices()
        .map_err(|e| BoothError::AudioCapture(format!("input_devices: {e}")))?;
    for d in devices {
        if d.name().map(|n| n == name).unwrap_or(false) {
            return Ok(d);
        }
    }
    Err(BoothError::AudioCapture(format!(
        "device not found: {name}"
    )))
}

/// Pick which input device to use given the user's settings. Returns
/// `Some(name)` to pin a specific device, `None` to fall through to the
/// system default. See `CpalAudioSource::start` for resolution order.
fn resolve_input_device_from_settings() -> Option<String> {
    let s = crate::settings::current_app_settings();
    let override_name = s.audio_input_device.trim();
    if !override_name.is_empty() {
        return Some(override_name.to_string());
    }
    if !s.prefer_builtin_mic_with_bluetooth {
        return None;
    }
    pick_builtin_when_default_is_bluetooth()
}

/// If the system default input device looks like a Bluetooth mic, find
/// a built-in mic to use instead. Returns `None` when the default is
/// fine (built-in / wired) or when no built-in alternative exists —
/// callers fall through to the system default in either case.
fn pick_builtin_when_default_is_bluetooth() -> Option<String> {
    let host = cpal::default_host();
    let default_name = host.default_input_device().and_then(|d| d.name().ok())?;
    if !is_bluetooth_input(&default_name) {
        return None;
    }
    let devices = host.input_devices().ok()?;
    for d in devices {
        let Ok(name) = d.name() else { continue };
        if is_builtin_input(&name) {
            tracing::info!(
                "audio: bluetooth default ({}) → switching to built-in mic ({}) to avoid HFP downgrade",
                default_name,
                name
            );
            return Some(name);
        }
    }
    tracing::warn!(
        "audio: bluetooth default ({}) but no built-in mic found — using bluetooth (HFP downgrade expected)",
        default_name
    );
    None
}

/// Heuristic: does this device name look like a Bluetooth headset/mic?
/// Matches the common consumer brands plus the generic Bluetooth /
/// Headset / Headphones tokens. Pure name match — fragile but cheap.
/// A more rigorous version would query
/// `kAudioDevicePropertyTransportType` via `coreaudio-sys`, which we
/// can swap in later if false positives become a problem.
fn is_bluetooth_input(name: &str) -> bool {
    let lc = name.to_ascii_lowercase();
    const NEEDLES: &[&str] = &[
        "airpods",
        "beats",
        "bluetooth",
        "headset",
        // Common brands when paired via BT (worth widening as users hit them):
        "sony wh-",
        "sony wf-",
        "bose quietcomfort",
        "powerbeats",
    ];
    NEEDLES.iter().any(|n| lc.contains(n))
}

/// Heuristic: does this device name look like a built-in MacBook mic?
/// macOS typically names it "MacBook Pro Microphone" /
/// "MacBook Air Microphone" / "Built-in Microphone."
fn is_builtin_input(name: &str) -> bool {
    let lc = name.to_ascii_lowercase();
    lc.contains("built-in") || lc.contains("macbook") || lc.contains("internal microphone")
}

#[cfg(test)]
mod device_resolution_tests {
    use super::*;

    #[test]
    fn classifies_bluetooth_names() {
        assert!(is_bluetooth_input("AirPods Max"));
        assert!(is_bluetooth_input("Eric's AirPods Pro"));
        assert!(is_bluetooth_input("Sony WH-1000XM5"));
        assert!(is_bluetooth_input("Beats Studio Buds"));
        assert!(is_bluetooth_input("Bluetooth Headset"));
        assert!(is_bluetooth_input("Bose QuietComfort 35"));
    }

    #[test]
    fn does_not_classify_wired_or_builtin_as_bluetooth() {
        assert!(!is_bluetooth_input("MacBook Pro Microphone"));
        assert!(!is_bluetooth_input("Built-in Microphone"));
        assert!(!is_bluetooth_input("Shure MV7"));
        assert!(!is_bluetooth_input("USB Audio Device"));
    }

    #[test]
    fn classifies_builtin_names() {
        assert!(is_builtin_input("MacBook Pro Microphone"));
        assert!(is_builtin_input("MacBook Air Microphone"));
        assert!(is_builtin_input("Built-in Microphone"));
    }

    #[test]
    fn does_not_classify_external_as_builtin() {
        assert!(!is_builtin_input("AirPods Max"));
        assert!(!is_builtin_input("USB Audio Device"));
        assert!(!is_builtin_input("Shure MV7"));
    }
}
