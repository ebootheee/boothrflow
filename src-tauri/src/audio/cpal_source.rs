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
/// 30ms at 16kHz — matches Silero VAD frame size and is small enough that
/// VAD/STT consumers don't see noticeable batching latency.
const TARGET_FRAME_SAMPLES: usize = 480;

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
        let device_name = self.device_name.lock().clone();

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

    let device = match device_name {
        Some(name) => find_device_by_name(&host, &name)?,
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
