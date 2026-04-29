//! Session daemon — wires hotkey → audio capture → STT → UI events.
//!
//! ## Event contract (FE / UI agent)
//!
//! - `dictation:start` — hotkey press; pill should appear
//! - `dictation:state` — `{ state, at_ms }` lifecycle transitions
//!   (listening → transcribing → cleaning → pasting → idle).
//!   `at_ms` is monotonic millis since this dictation began.
//!   Drives the pill state machine.
//! - `dictation:summary` — capture stats (frames, seconds, peak dBFS)
//! - `dictation:partial` — `{ committed, tentative, at_ms }` while the user
//!   is still holding the key. Local-Agreement-2 stabilization: `committed`
//!   is stable text the FE can render solid; `tentative` is still in flux
//!   (render dimmed). Multiple events fire over the lifetime of one press.
//! - `dictation:result` — raw STT transcript + metadata (immediately after Whisper)
//! - `dictation:formatted` — LLM-cleaned text (only if it differs from raw)
//! - `dictation:done` — `{ capture_ms, stt_ms, llm_ms, paste_ms, total_ms, formatted }`
//!   emitted once paste completes; carries the full timing breakdown
//! - `dictation:error` — generic STT/audio/LLM/inject failure
//! - `dictation:model-missing` — Whisper model not at the expected path
//! - `dictation:llm-missing` — LLM endpoint unreachable (degraded but functional)
//!
//! The pill window stays visible from `listening` through `pasting`, then
//! hides on `idle`. UI components subscribe to `dictation:state` to render
//! per-stage visuals (listening pulse, transcribing spinner, etc.).

#[cfg(feature = "real-engines")]
mod real {
    use std::thread;
    use std::time::{Duration, Instant};

    use serde::Serialize;
    use tauri::{AppHandle, Emitter};

    use std::sync::Arc;

    use crate::audio::{AudioFrame, AudioSource, CpalAudioSource};
    use crate::history::{HistoryStore, RecordRequest};
    use crate::hotkey::{HotkeyEvent, HotkeySource, RdevHotkeySource};
    use crate::injector::{ClipboardInjector, Injector};
    use crate::llm::{should_skip_llm, CleanupRequest, LlmCleanup, OpenAiCompatLlmCleanup};
    use crate::overlay;
    use crate::settings;
    use crate::stt::{StreamingTranscriber, SttEngine, SttResult, WhisperSttEngine};
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

    /// Lifecycle state transitions emitted on `dictation:state`. The FE
    /// pill subscribes and renders per-stage visuals.
    #[derive(Debug, Clone, Serialize)]
    struct DictationState {
        state: &'static str,
        at_ms: u64,
    }

    /// Final telemetry payload on `dictation:done` — emitted after paste
    /// completes (or on hard failure with whatever timings we have).
    #[derive(Debug, Clone, Serialize)]
    struct DictationDone {
        formatted: String,
        capture_ms: u64,
        stt_ms: u64,
        llm_ms: u64,
        paste_ms: u64,
        total_ms: u64,
    }

    fn emit_stage(app: &AppHandle, state: &'static str, started: Instant) {
        let _ = app.emit(
            "dictation:state",
            &DictationState {
                state,
                at_ms: started.elapsed().as_millis() as u64,
            },
        );
    }

    pub fn spawn_session_daemon(app: AppHandle, history: Option<Arc<HistoryStore>>) {
        thread::Builder::new()
            .name("boothrflow-session".into())
            .spawn(move || {
                if let Err(e) = run(app, history) {
                    tracing::error!("session daemon errored: {e}");
                }
            })
            .ok();
    }

    fn run(app: AppHandle, history: Option<Arc<HistoryStore>>) -> crate::error::Result<()> {
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
                HotkeyEvent::QuickPasteOpen => {
                    // Capture the currently-focused window so we can paste
                    // back into it after the user picks an entry.
                    crate::quickpaste::capture_target_window();
                    if let Err(e) = crate::quickpaste::show(&app) {
                        tracing::warn!("quickpaste show failed: {e}");
                    }
                }
                HotkeyEvent::Press => {
                    if tray::is_paused() {
                        tracing::info!("hotkey: press ignored (paused)");
                        continue;
                    }

                    // Per-press monotonic clock: stage at_ms timestamps and
                    // the final timing breakdown all reference this.
                    let dictation_started = Instant::now();

                    let _ = overlay::show(&app);
                    tray::set_listening(&app, true);
                    let _ = app.emit("dictation:start", ());
                    emit_stage(&app, "listening", dictation_started);

                    let frame_rx = match audio.start() {
                        Ok(rx) => rx,
                        Err(e) => {
                            tracing::error!("audio start failed: {e}");
                            let _ = app.emit("dictation:error", e.to_string());
                            let _ = overlay::hide(&app);
                            tray::set_listening(&app, false);
                            emit_stage(&app, "idle", dictation_started);
                            continue;
                        }
                    };

                    // Streaming partials. Optional — if the worker fails to
                    // spawn we just don't emit partials; the final pass still
                    // produces the same transcript on release.
                    let streaming = stt.as_ref().and_then(|engine| {
                        match StreamingTranscriber::spawn(
                            engine.shared_context(),
                            engine.initial_prompt().map(|s| s.to_string()),
                            dictation_started,
                        ) {
                            Ok(s) => Some(s),
                            Err(e) => {
                                tracing::warn!("streaming disabled: {e}");
                                None
                            }
                        }
                    });

                    let mut frames: Vec<AudioFrame> = Vec::new();
                    let mut released = false;

                    while !released {
                        while let Ok(frame) = frame_rx.try_recv() {
                            if let Some(s) = streaming.as_ref() {
                                s.push_audio(&frame);
                            }
                            frames.push(frame);
                        }
                        if let Some(s) = streaming.as_ref() {
                            s.maybe_tick();
                            while let Ok(partial) = s.partial_rx.try_recv() {
                                let _ = app.emit("dictation:partial", &partial);
                            }
                        }
                        match hotkey_rx.recv_timeout(Duration::from_millis(20)) {
                            Ok(HotkeyEvent::Release) => released = true,
                            Ok(HotkeyEvent::Press) => {} // duplicate, ignore
                            Ok(HotkeyEvent::QuickPasteOpen) => {} // ignore mid-dictation
                            Err(_) => {}                          // timeout
                        }
                    }

                    let _ = audio.stop();
                    while let Ok(frame) = frame_rx.try_recv() {
                        if let Some(s) = streaming.as_ref() {
                            s.push_audio(&frame);
                        }
                        frames.push(frame);
                    }
                    // Drop streaming: the worker thread sees the channel
                    // close and exits. We don't await it — the final pass
                    // produces the authoritative transcript.
                    drop(streaming);

                    let capture_ms = dictation_started.elapsed().as_millis() as u64;
                    let summary = summarize(&frames, dictation_started.elapsed());
                    tracing::info!(
                        "captured: {} frames, {:.2}s, peak {:.1} dBFS",
                        summary.frames,
                        summary.seconds,
                        summary.peak_dbfs
                    );
                    let _ = app.emit("dictation:summary", &summary);

                    // Pill stays visible — we transition through transcribing →
                    // cleaning → pasting → idle inside `transcribe_and_emit`.
                    tray::set_listening(&app, false);
                    emit_stage(&app, "transcribing", dictation_started);

                    // STT on the captured audio, optional LLM cleanup, then paste, then persist.
                    match &stt {
                        Some(engine) => {
                            let pcm: Vec<f32> =
                                frames.iter().flat_map(|f| f.iter().copied()).collect();
                            transcribe_and_emit(
                                &app,
                                engine,
                                llm.as_ref(),
                                injector.as_ref(),
                                history.as_deref(),
                                dictation_started,
                                capture_ms,
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

                    let _ = overlay::hide(&app);
                    emit_stage(&app, "idle", dictation_started);
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
        history: Option<&HistoryStore>,
        dictation_started: Instant,
        capture_ms: u64,
        pcm: &[f32],
    ) {
        let stt_started = Instant::now();
        let stt_result = match engine.transcribe(pcm) {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("stt error: {e}");
                let _ = app.emit("dictation:error", e.to_string());
                return;
            }
        };
        let stt_ms = stt_started.elapsed().as_millis() as u64;

        tracing::info!(
            "transcript ({} ms): \"{}\"",
            stt_result.duration_ms,
            stt_result.text
        );
        emit_result(app, &stt_result);

        if stt_result.text.is_empty() {
            // Still emit done so the FE pill leaves transcribing state with a
            // full timing breakdown (paste_ms = 0, formatted empty).
            let total_ms = dictation_started.elapsed().as_millis() as u64;
            let _ = app.emit(
                "dictation:done",
                &DictationDone {
                    formatted: String::new(),
                    capture_ms,
                    stt_ms,
                    llm_ms: 0,
                    paste_ms: 0,
                    total_ms,
                },
            );
            return;
        }

        // Optional LLM cleanup pass. Skip for short / opted-out cases; fall
        // back to raw transcript on any LLM failure.
        emit_stage(app, "cleaning", dictation_started);
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

        emit_stage(app, "pasting", dictation_started);
        let paste_started = Instant::now();
        if let Some(inj) = injector {
            if let Err(e) = inj.inject(&formatted) {
                tracing::error!("inject failed: {e}");
                let _ = app.emit("dictation:error", e.to_string());
            }
        }
        let paste_ms = paste_started.elapsed().as_millis() as u64;

        // Persist to history. Embedding fires-and-forgets in a background
        // thread inside record(); this call returns in <1ms after the SQL
        // insert, so it stays out of the dictation hot path.
        if let Some(hist) = history {
            let req = RecordRequest {
                raw: stt_result.text.clone(),
                formatted: formatted.clone(),
                style,
                app_exe: None, // populated when W5 wires context detection
                window_title: None,
                duration_ms: capture_ms,
                llm_ms,
            };
            if let Err(e) = hist.record(req) {
                tracing::warn!("history record failed: {e}");
            }
        }

        let total_ms = dictation_started.elapsed().as_millis() as u64;
        tracing::info!(
            "dictation:done capture={capture_ms}ms stt={stt_ms}ms llm={llm_ms}ms paste={paste_ms}ms total={total_ms}ms"
        );
        let _ = app.emit(
            "dictation:done",
            &DictationDone {
                formatted,
                capture_ms,
                stt_ms,
                llm_ms,
                paste_ms,
                total_ms,
            },
        );
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
