//! Session daemon — wires hotkey → audio capture → STT → UI events.
//!
//! W2: STT now runs against captured audio. The Whisper model is loaded at
//! daemon startup; if missing, the daemon still runs and reports
//! `dictation:model-missing` on each invocation with download instructions.
//!
//! Events emitted to the frontend:
//! - `dictation:start`         — hotkey press; pill window shown
//! - `dictation:summary`       — capture stats (duration, peak dBFS)
//! - `dictation:result`        — transcript text + metadata
//! - `dictation:error`         — generic STT/audio failure
//! - `dictation:model-missing` — Whisper model not at the expected path

#[cfg(feature = "real-engines")]
mod real {
    use std::thread;
    use std::time::{Duration, Instant};

    use serde::Serialize;
    use tauri::{AppHandle, Emitter};

    use crate::audio::{AudioFrame, AudioSource, CpalAudioSource};
    use crate::hotkey::{HotkeyEvent, HotkeySource, RdevHotkeySource};
    use crate::overlay;
    use crate::stt::{SttEngine, SttResult, WhisperSttEngine};

    #[derive(Debug, Clone, Serialize)]
    struct DictationSummary {
        frames: usize,
        samples: usize,
        seconds: f32,
        peak_dbfs: f32,
    }

    pub fn spawn_session_daemon(app: AppHandle) {
        thread::Builder::new()
            .name("boothrflow-session".into())
            .spawn(move || {
                if let Err(e) = run(app) {
                    tracing::error!("session daemon errored: {e}");
                }
            })
            .ok();
    }

    fn run(app: AppHandle) -> crate::error::Result<()> {
        let hotkey = RdevHotkeySource::new();
        let hotkey_rx = hotkey.start()?;
        tracing::info!("session daemon ready — Ctrl+Meta to dictate");

        let audio = CpalAudioSource::new();

        // Whisper loads at daemon startup. tiny.en is ~75MB on disk and
        // ~300MB resident; load takes a few seconds. Failure is recoverable —
        // the daemon keeps running and reports model-missing on each press.
        let stt: Option<WhisperSttEngine> = match WhisperSttEngine::from_default_location() {
            Ok(engine) => {
                tracing::info!("whisper: model loaded");
                Some(engine)
            }
            Err(e) => {
                tracing::warn!("whisper not available: {e}");
                let _ = app.emit("dictation:model-missing", e.to_string());
                None
            }
        };

        for event in hotkey_rx.iter() {
            match event {
                HotkeyEvent::Press => {
                    let _ = overlay::show(&app);
                    let _ = app.emit("dictation:start", ());

                    let frame_rx = match audio.start() {
                        Ok(rx) => rx,
                        Err(e) => {
                            tracing::error!("audio start failed: {e}");
                            let _ = app.emit("dictation:error", e.to_string());
                            let _ = overlay::hide(&app);
                            continue;
                        }
                    };

                    let started = Instant::now();
                    let mut frames: Vec<AudioFrame> = Vec::new();
                    let mut released = false;

                    while !released {
                        while let Ok(frame) = frame_rx.try_recv() {
                            frames.push(frame);
                        }
                        match hotkey_rx.recv_timeout(Duration::from_millis(20)) {
                            Ok(HotkeyEvent::Release) => released = true,
                            Ok(HotkeyEvent::Press) => {} // duplicate, ignore
                            Err(_) => {}                 // timeout
                        }
                    }

                    let _ = audio.stop();
                    while let Ok(frame) = frame_rx.try_recv() {
                        frames.push(frame);
                    }

                    let captured_elapsed = started.elapsed();
                    let summary = summarize(&frames, captured_elapsed);
                    tracing::info!(
                        "captured: {} frames, {:.2}s, peak {:.1} dBFS",
                        summary.frames,
                        summary.seconds,
                        summary.peak_dbfs
                    );
                    let _ = app.emit("dictation:summary", &summary);
                    let _ = overlay::hide(&app);

                    // STT on the captured audio.
                    match &stt {
                        Some(engine) => {
                            // Flatten frames into one contiguous PCM buffer.
                            let pcm: Vec<f32> =
                                frames.iter().flat_map(|f| f.iter().copied()).collect();
                            transcribe_and_emit(&app, engine, &pcm);
                        }
                        None => {
                            let _ = app.emit(
                                "dictation:model-missing",
                                "Whisper model not loaded — see logs / settings",
                            );
                        }
                    }
                }
                HotkeyEvent::Release => {
                    // Lone release without a press — defensive no-op.
                    let _ = overlay::hide(&app);
                }
            }
        }

        Ok(())
    }

    fn transcribe_and_emit(app: &AppHandle, engine: &WhisperSttEngine, pcm: &[f32]) {
        match engine.transcribe(pcm) {
            Ok(result) => {
                tracing::info!(
                    "transcript ({} ms): \"{}\"",
                    result.duration_ms,
                    result.text
                );
                emit_result(app, &result);
            }
            Err(e) => {
                tracing::error!("stt error: {e}");
                let _ = app.emit("dictation:error", e.to_string());
            }
        }
    }

    fn emit_result(app: &AppHandle, result: &SttResult) {
        let _ = app.emit("dictation:result", result);
    }

    fn summarize(frames: &[AudioFrame], elapsed: Duration) -> DictationSummary {
        let samples: usize = frames.iter().map(|f| f.len()).sum();
        let peak = frames
            .iter()
            .flat_map(|f| f.iter().copied())
            .map(f32::abs)
            .fold(0.0f32, f32::max);
        let peak_dbfs = if peak > 0.0 {
            20.0 * peak.log10()
        } else {
            -120.0
        };
        DictationSummary {
            frames: frames.len(),
            samples,
            seconds: elapsed.as_secs_f32(),
            peak_dbfs,
        }
    }
}

#[cfg(feature = "real-engines")]
pub use real::spawn_session_daemon;
