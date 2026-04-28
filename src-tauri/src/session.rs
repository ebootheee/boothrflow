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
    use crate::injector::{ClipboardInjector, Injector};
    use crate::llm::{should_skip_llm, CleanupRequest, LlmCleanup, OpenAiCompatLlmCleanup};
    use crate::overlay;
    use crate::settings;
    use crate::stt::{SttEngine, SttResult, WhisperSttEngine};
    use crate::tray;

    #[derive(Debug, Clone, Serialize)]
    struct DictationFormatted {
        raw: String,
        formatted: String,
        style: settings::Style,
        llm_ms: u64,
    }

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

        // LLM cleanup via OpenAI-compatible HTTP. Ollama on localhost:11434
        // by default; configurable via BOOTHRFLOW_LLM_* env vars. Failures
        // (server down, model missing, timeout) fall back to raw transcript.
        let llm: Option<OpenAiCompatLlmCleanup> = match OpenAiCompatLlmCleanup::from_env() {
            None => {
                tracing::info!("llm: disabled via BOOTHRFLOW_LLM_DISABLED");
                None
            }
            Some(Ok(engine)) => {
                tracing::info!(
                    "llm: openai-compat HTTP (endpoint={}, model={})",
                    engine.endpoint(),
                    engine.model()
                );
                // Pre-warm in a background thread so the first user dictation
                // doesn't pay the model-load tax (typically 3-5s the first
                // time Ollama touches a freshly-pulled model).
                let prewarm_endpoint = engine.endpoint().to_string();
                let prewarm_model = engine.model().to_string();
                let prewarm_key = std::env::var("BOOTHRFLOW_LLM_API_KEY").ok();
                std::thread::Builder::new()
                    .name("boothrflow-llm-prewarm".into())
                    .spawn(move || {
                        if let Ok(warm) = OpenAiCompatLlmCleanup::new(
                            prewarm_endpoint,
                            prewarm_model,
                            prewarm_key,
                        ) {
                            warm.prewarm();
                        }
                    })
                    .ok();
                Some(engine)
            }
            Some(Err(e)) => {
                tracing::warn!("llm: client init failed, falling back to raw: {e}");
                let _ = app.emit("dictation:llm-missing", e.to_string());
                None
            }
        };

        // Injector. ClipboardInjector init failure is rare (would mean
        // OS-level clipboard access denied); we still keep the daemon
        // running and emit errors per-attempt.
        let injector: Option<ClipboardInjector> = match ClipboardInjector::new() {
            Ok(inj) => Some(inj),
            Err(e) => {
                tracing::error!("injector init failed: {e}");
                let _ = app.emit("dictation:error", e.to_string());
                None
            }
        };

        for event in hotkey_rx.iter() {
            match event {
                HotkeyEvent::Press => {
                    if tray::is_paused() {
                        tracing::info!("hotkey: press ignored (paused)");
                        continue;
                    }
                    let _ = overlay::show(&app);
                    tray::set_listening(&app, true);
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
                    tray::set_listening(&app, false);

                    // STT on the captured audio, optional LLM cleanup, then paste.
                    match &stt {
                        Some(engine) => {
                            let pcm: Vec<f32> =
                                frames.iter().flat_map(|f| f.iter().copied()).collect();
                            transcribe_and_emit(
                                &app,
                                engine,
                                llm.as_ref(),
                                injector.as_ref(),
                                &pcm,
                            );
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

    fn transcribe_and_emit(
        app: &AppHandle,
        engine: &WhisperSttEngine,
        llm: Option<&OpenAiCompatLlmCleanup>,
        injector: Option<&ClipboardInjector>,
        pcm: &[f32],
    ) {
        let stt_result = match engine.transcribe(pcm) {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("stt error: {e}");
                let _ = app.emit("dictation:error", e.to_string());
                return;
            }
        };

        tracing::info!(
            "transcript ({} ms): \"{}\"",
            stt_result.duration_ms,
            stt_result.text
        );
        emit_result(app, &stt_result);

        if stt_result.text.is_empty() {
            return;
        }

        // Optional LLM cleanup pass. Skip for short / opted-out cases; fall
        // back to raw transcript on any LLM failure.
        let style = settings::current_style();
        let (formatted, llm_ms) = run_llm_cleanup(&stt_result.text, style, llm);

        if formatted != stt_result.text {
            let _ = app.emit(
                "dictation:formatted",
                &DictationFormatted {
                    raw: stt_result.text.clone(),
                    formatted: formatted.clone(),
                    style,
                    llm_ms,
                },
            );
        }

        if let Some(inj) = injector {
            if let Err(e) = inj.inject(&formatted) {
                tracing::error!("inject failed: {e}");
                let _ = app.emit("dictation:error", e.to_string());
            }
        }
    }

    /// Returns the post-cleanup string and elapsed ms (0 if LLM was skipped).
    fn run_llm_cleanup(
        raw: &str,
        style: settings::Style,
        llm: Option<&OpenAiCompatLlmCleanup>,
    ) -> (String, u64) {
        // Hard skips: explicit raw style, no LLM loaded, very short utterance.
        if matches!(style, settings::Style::Raw) {
            return (raw.to_string(), 0);
        }
        let Some(llm) = llm else {
            return (raw.to_string(), 0);
        };
        if should_skip_llm(raw, true) {
            return (raw.to_string(), 0);
        }

        let started = std::time::Instant::now();
        match llm.cleanup(CleanupRequest {
            raw_text: raw,
            style,
            app_context: None,
        }) {
            Ok(text) => {
                let ms = started.elapsed().as_millis() as u64;
                tracing::info!("llm cleanup ({ms} ms): \"{raw}\" → \"{text}\"");
                (text, ms)
            }
            Err(e) => {
                tracing::error!("llm cleanup failed, falling back to raw: {e}");
                (raw.to_string(), 0)
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
