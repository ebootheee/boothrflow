//! Session daemon — wires hotkey → audio capture → pipeline.
//!
//! For W1, this is the smoke-level integration: hold key → pill appears,
//! audio is captured into a buffer, release → captured-frame-count is
//! emitted to the UI as a `dictation:summary` event. STT and injection
//! land in W2/W3.

#[cfg(feature = "real-engines")]
mod real {
    use std::thread;
    use std::time::{Duration, Instant};

    use serde::Serialize;
    use tauri::{AppHandle, Emitter};

    use crate::audio::{AudioFrame, AudioSource, CpalAudioSource};
    use crate::hotkey::{HotkeyEvent, HotkeySource, RdevHotkeySource};
    use crate::overlay;

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

        for event in hotkey_rx.iter() {
            match event {
                HotkeyEvent::Press => {
                    tracing::info!("hotkey: press");
                    let _ = overlay::show(&app);
                    let _ = app.emit("dictation:start", ());

                    // Open the audio stream and drain frames until release.
                    let frame_rx = match audio.start() {
                        Ok(rx) => rx,
                        Err(e) => {
                            tracing::error!("audio start failed: {e}");
                            let _ = overlay::hide(&app);
                            continue;
                        }
                    };

                    let started = Instant::now();
                    let mut frames: Vec<AudioFrame> = Vec::new();
                    let mut released = false;

                    while !released {
                        // Drain available frames non-blocking.
                        while let Ok(frame) = frame_rx.try_recv() {
                            frames.push(frame);
                        }
                        // Check for release without busy-looping.
                        match hotkey_rx.recv_timeout(Duration::from_millis(20)) {
                            Ok(HotkeyEvent::Release) => released = true,
                            Ok(HotkeyEvent::Press) => {
                                // duplicate press while already pressed — ignore
                            }
                            Err(_) => {
                                // timeout — keep draining
                            }
                        }
                    }

                    // Drain any remaining frames after release.
                    let _ = audio.stop();
                    while let Ok(frame) = frame_rx.try_recv() {
                        frames.push(frame);
                    }

                    let summary = summarize(&frames, started.elapsed());
                    tracing::info!(
                        "release: {} frames, {} samples, {:.2}s, peak {:.1} dBFS",
                        summary.frames,
                        summary.samples,
                        summary.seconds,
                        summary.peak_dbfs
                    );
                    let _ = overlay::hide(&app);
                    let _ = app.emit("dictation:summary", &summary);
                }
                HotkeyEvent::Release => {
                    // Lone release without a press — defensive no-op.
                    let _ = overlay::hide(&app);
                }
            }
        }

        Ok(())
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
