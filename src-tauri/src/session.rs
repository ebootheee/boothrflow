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
    use crate::context::{ContextDetector, RealContextDetector};
    use crate::history::{HistoryStore, RecordRequest};
    use crate::hotkey::{HotkeyEvent, HotkeySource, RdevHotkeySource};
    use crate::injector::{ClipboardInjector, Injector};
    use crate::learning::{FocusedTextReader, LearningCoordinator, PasteSnapshot};
    use crate::llm::{should_skip_llm, CleanupRequest, LlmCleanup, OpenAiCompatLlmCleanup};
    use crate::overlay;
    use crate::settings::{self, SettingsStore};
    use crate::stt::{StreamingTranscriber, SttEngine, SttResult, WhisperSttEngine};
    use crate::tray;

    /// STT engine selected at runtime. Wave 5c added Parakeet alongside
    /// the existing Whisper path; the variant determines which
    /// `transcribe()` impl runs and whether the streaming partial
    /// pipeline can be wired (whisper-only today — Parakeet streaming
    /// is a Wave 5d enhancement against `stt::streaming::LocalAgreement2`).
    enum LoadedStt {
        Whisper(WhisperSttEngine),
        #[cfg(feature = "parakeet-engine")]
        Parakeet(crate::stt::ParakeetSttEngine),
    }

    impl LoadedStt {
        fn transcribe(&self, audio: &[f32]) -> crate::error::Result<SttResult> {
            match self {
                Self::Whisper(e) => e.transcribe(audio),
                #[cfg(feature = "parakeet-engine")]
                Self::Parakeet(e) => e.transcribe(audio),
            }
        }

        /// Whisper-typed handle, for streaming partials. Returns `None`
        /// when Parakeet (or any future non-streaming engine) is the
        /// active variant — the session loop interprets that as
        /// "no partials this dictation".
        fn as_whisper(&self) -> Option<&WhisperSttEngine> {
            match self {
                Self::Whisper(e) => Some(e),
                #[cfg(feature = "parakeet-engine")]
                Self::Parakeet(_) => None,
            }
        }
    }

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
    #[derive(Debug, Clone, Serialize, Default)]
    struct DictationDone {
        formatted: String,
        capture_ms: u64,
        stt_ms: u64,
        llm_ms: u64,
        paste_ms: u64,
        total_ms: u64,
        // Cleanup-pass throughput, populated when the LLM backend reports
        // a `usage` block (Ollama always does; some compat servers don't).
        // `None` when the LLM was skipped or didn't report — the FE
        // distinguishes "no data" from "0".
        llm_prompt_tokens: Option<u32>,
        llm_completion_tokens: Option<u32>,
        llm_tok_per_sec: Option<f32>,
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

    pub fn spawn_session_daemon(
        app: AppHandle,
        history: Option<Arc<HistoryStore>>,
        settings_store: SettingsStore,
    ) {
        thread::Builder::new()
            .name("boothrflow-session".into())
            .spawn(move || {
                if let Err(e) = run(app, history, settings_store) {
                    tracing::error!("session daemon errored: {e}");
                }
            })
            .ok();
    }

    fn run(
        app: AppHandle,
        history: Option<Arc<HistoryStore>>,
        settings_store: SettingsStore,
    ) -> crate::error::Result<()> {
        let hotkey = RdevHotkeySource::new();
        let hotkey_rx = hotkey.start()?;
        tracing::info!(
            "session daemon ready — {} to dictate",
            settings::current_hotkeys().ptt
        );

        let audio = CpalAudioSource::new();
        let context_detector = RealContextDetector::new();
        let learning = build_learning_coordinator(settings_store.clone());

        // STT engine is hot-swappable from Settings. Load once at startup,
        // then reload on the next press if the configured model file
        // changes. Variant (Whisper vs. Parakeet) is determined per-load
        // based on the current `whisper_model` setting value.
        let mut stt: Option<LoadedStt> = None;
        let mut stt_model_file: Option<String> = None;
        ensure_stt_loaded(&app, &mut stt, &mut stt_model_file);

        prewarm_llm_from_settings();

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
                HotkeyEvent::Press | HotkeyEvent::ToggleDictation => {
                    if tray::is_paused() {
                        tracing::info!("hotkey: press ignored (paused)");
                        continue;
                    }

                    // Whether this session was started by hold-PTT (Press) or
                    // tap-to-toggle (ToggleDictation). Doesn't change the hot
                    // path; only used in logs + to decide which keypaths can
                    // terminate the session in the inner loop below.
                    let toggle_session = matches!(event, HotkeyEvent::ToggleDictation);
                    if toggle_session {
                        tracing::info!(
                            "hotkey: toggle-on ({})",
                            settings::current_hotkeys().toggle
                        );
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
                    ensure_stt_loaded(&app, &mut stt, &mut stt_model_file);

                    // Streaming partials are whisper-only today —
                    // `as_whisper()` returns `None` when Parakeet is
                    // active, which the loop interprets as
                    // "no partials this dictation".
                    let streaming = stt
                        .as_ref()
                        .and_then(LoadedStt::as_whisper)
                        .and_then(|engine| {
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
                            // Either modality terminates the session: Release
                            // ends a hold-PTT, ToggleDictation ends a toggle.
                            // We also accept Release in toggle sessions and
                            // ToggleDictation in PTT sessions — preferable to
                            // a wedged state if the user mixes chords.
                            Ok(HotkeyEvent::Release) | Ok(HotkeyEvent::ToggleDictation) => {
                                released = true;
                            }
                            Ok(HotkeyEvent::Press) => {} // duplicate, ignore
                            Ok(HotkeyEvent::QuickPasteOpen) => {} // ignore mid-dictation
                            Err(_) => {}                 // timeout
                        }
                    }
                    if toggle_session {
                        tracing::info!("hotkey: toggle-off");
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
                                injector.as_ref(),
                                history.as_deref(),
                                &context_detector,
                                learning.as_ref(),
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

    fn ensure_stt_loaded(
        app: &AppHandle,
        stt: &mut Option<LoadedStt>,
        loaded_model_file: &mut Option<String>,
    ) {
        let model_file = settings::current_whisper_model_file();
        if stt.is_some() && loaded_model_file.as_deref() == Some(model_file.as_str()) {
            return;
        }

        *stt = None;
        *loaded_model_file = None;

        let models_dir = match crate::stt::default_models_dir() {
            Some(dir) => dir,
            None => {
                let msg = "could not resolve user data directory";
                tracing::warn!("stt not available: {msg}");
                let _ = app.emit("dictation:model-missing", msg);
                return;
            }
        };

        // Parakeet vs. Whisper: the current settings value carries the
        // model identifier, not a flag. We dispatch by name. The
        // settings option for Parakeet is gated on the
        // `parakeet-engine` cargo feature, so we only see the
        // identifier here when the engine is actually compiled in.
        let model_value = settings::current_app_settings().whisper.model;

        if model_value == "parakeet-tdt-0.6b-v3" {
            #[cfg(feature = "parakeet-engine")]
            {
                let dir = models_dir.join(&model_file);
                match crate::stt::ParakeetSttEngine::from_model_dir(&dir) {
                    Ok(engine) => {
                        tracing::info!("parakeet: loaded from {}", dir.display());
                        *stt = Some(LoadedStt::Parakeet(engine));
                        *loaded_model_file = Some(model_file);
                        return;
                    }
                    Err(e) => {
                        tracing::warn!("parakeet not available: {e}");
                        let _ = app.emit("dictation:model-missing", e.to_string());
                        return;
                    }
                }
            }
            #[cfg(not(feature = "parakeet-engine"))]
            {
                let msg = "Parakeet selected but binary built without `parakeet-engine` feature";
                tracing::warn!("{msg}");
                let _ = app.emit("dictation:model-missing", msg);
                return;
            }
        }

        let path = models_dir.join(&model_file);
        let name = std::path::Path::new(&model_file)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("whisper")
            .to_string();

        match WhisperSttEngine::from_path(&path, name) {
            Ok(engine) => {
                tracing::info!("whisper: model loaded ({model_file})");
                *stt = Some(LoadedStt::Whisper(engine));
                *loaded_model_file = Some(model_file);
            }
            Err(e) => {
                tracing::warn!("whisper not available: {e}");
                let _ = app.emit("dictation:model-missing", e.to_string());
            }
        }
    }

    fn prewarm_llm_from_settings() {
        let settings = settings::current_app_settings();
        let Some(llm) = OpenAiCompatLlmCleanup::from_settings(&settings.llm) else {
            tracing::info!("llm: disabled via settings");
            return;
        };
        let engine = match llm {
            Ok(engine) => engine,
            Err(e) => {
                tracing::warn!("llm: client init failed, falling back to raw: {e}");
                return;
            }
        };
        tracing::info!(
            "llm: openai-compat HTTP (endpoint={}, model={})",
            engine.endpoint(),
            engine.model()
        );
        std::thread::Builder::new()
            .name("boothrflow-llm-prewarm".into())
            .spawn(move || engine.prewarm())
            .ok();
    }

    // Pulling these into a struct doesn't buy clarity — every collaborator
    // is conceptually independent (engine, injector, history, context,
    // learning), and the function is the single coordination point for
    // the post-STT path. Suppress the lint locally.
    #[allow(clippy::too_many_arguments)]
    fn transcribe_and_emit(
        app: &AppHandle,
        engine: &LoadedStt,
        injector: Option<&ClipboardInjector>,
        history: Option<&HistoryStore>,
        context_detector: &dyn ContextDetector,
        learning: Option<&LearningCoordinator>,
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
                    capture_ms,
                    stt_ms,
                    total_ms,
                    ..DictationDone::default()
                },
            );
            return;
        }

        // Capture the focused-app context once (cheap on Mac/Win — a few
        // syscalls). The result feeds both the cleanup prompt's app-aware
        // hints and the history record (so per-app filtering / counts in
        // the UI work without re-walking the OS later).
        let app_context = context_detector.detect();
        if let Some(ctx) = &app_context {
            tracing::debug!("context: app={} window={:?}", ctx.app_exe, ctx.window_title);
        }

        // Window OCR: opt-in via Settings + skipped when privacy mode is
        // on. Wave 5 ships the structure; the actual Vision-framework
        // call lives in `crate::ocr` and is gated by the Screen Recording
        // TCC permission. None on platforms without an OCR backend.
        let window_ocr = if !settings::privacy_mode_enabled()
            && settings::current_app_settings().cleanup_window_ocr
        {
            crate::ocr::capture_focused_window_text(app_context.as_ref()).ok()
        } else {
            None
        };

        // Optional LLM cleanup pass. Skip for short / opted-out cases; fall
        // back to raw transcript on any LLM failure.
        emit_stage(app, "cleaning", dictation_started);
        let style = settings::current_style();
        let cleanup = run_llm_cleanup(&stt_result.text, style, app_context.clone(), window_ocr);
        if let Some(err) = cleanup.error {
            // Surface so the UI can show "Cleanup unavailable — using raw"
            // instead of silently displaying 0 ms.
            let _ = app.emit("dictation:llm-missing", err);
        }
        let formatted = cleanup.text;
        let llm_ms = cleanup.elapsed_ms;

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
        let mut paste_succeeded = false;
        if let Some(inj) = injector {
            match inj.inject(&formatted) {
                Ok(()) => {
                    paste_succeeded = true;
                }
                Err(e) => {
                    tracing::error!("inject failed: {e}");
                    let _ = app.emit("dictation:error", e.to_string());
                }
            }
        }
        let paste_ms = paste_started.elapsed().as_millis() as u64;

        // Post-paste learning observation: if the user enables auto-learn,
        // a background thread samples the focused field after a short
        // settling window and looks for a small single-word edit. The
        // coordinator no-ops when auto-learn is off, so the gate is here
        // (settings load + opt-in check) rather than inside the spawn.
        if paste_succeeded
            && !formatted.is_empty()
            && !settings::privacy_mode_enabled()
            && settings::current_app_settings().auto_learn_corrections
        {
            if let Some(coord) = learning {
                coord.observe(PasteSnapshot {
                    pasted_text: formatted.clone(),
                });
            }
        }

        // Persist to history. Embedding fires-and-forgets in a background
        // thread inside record(); this call returns in <1ms after the SQL
        // insert, so it stays out of the dictation hot path.
        if let Some(hist) = history {
            let req = RecordRequest {
                raw: stt_result.text.clone(),
                formatted: formatted.clone(),
                style,
                app_exe: app_context.as_ref().map(|c| c.app_exe.clone()),
                window_title: app_context
                    .as_ref()
                    .and_then(|c| c.window_title.clone()),
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
                llm_prompt_tokens: cleanup.prompt_tokens,
                llm_completion_tokens: cleanup.completion_tokens,
                llm_tok_per_sec: cleanup.tok_per_sec,
            },
        );
    }

    /// Bundle of cleanup outcome + telemetry the session daemon needs to
    /// build the `dictation:done` event. `error` is `Some` only when the
    /// cleanup pass tried to run and the call failed — so the UI can
    /// distinguish "0 ms because LLM was skipped" (short utterance / Raw
    /// style / disabled) from "0 ms because Ollama is down".
    struct CleanupOutcome {
        text: String,
        elapsed_ms: u64,
        prompt_tokens: Option<u32>,
        completion_tokens: Option<u32>,
        tok_per_sec: Option<f32>,
        error: Option<String>,
    }

    impl CleanupOutcome {
        fn passthrough(raw: &str) -> Self {
            Self {
                text: raw.to_string(),
                elapsed_ms: 0,
                prompt_tokens: None,
                completion_tokens: None,
                tok_per_sec: None,
                error: None,
            }
        }
    }

    fn run_llm_cleanup(
        raw: &str,
        style: settings::Style,
        app_context: Option<crate::context::AppContext>,
        window_ocr: Option<String>,
    ) -> CleanupOutcome {
        // Hard skips: explicit raw style, no LLM loaded, very short utterance.
        if matches!(style, settings::Style::Raw) || settings::privacy_mode_enabled() {
            return CleanupOutcome::passthrough(raw);
        }
        let settings = settings::current_app_settings();
        let llm = match OpenAiCompatLlmCleanup::from_settings(&settings.llm) {
            None => return CleanupOutcome::passthrough(raw),
            Some(Ok(llm)) => llm,
            Some(Err(e)) => {
                tracing::error!("llm client init failed, falling back to raw: {e}");
                return CleanupOutcome {
                    error: Some(e.to_string()),
                    ..CleanupOutcome::passthrough(raw)
                };
            }
        };
        if should_skip_llm(raw, settings.llm.enabled) {
            return CleanupOutcome::passthrough(raw);
        }

        // Wave 5: pull the user's vocab + corrections out of settings and
        // hand them to the cleanup prompt. The prompt builder emits
        // <USER-CORRECTIONS> + <OCR-RULES> blocks ghost-pepper-style.
        let preferred = parse_vocabulary(&settings.vocabulary);
        let commonly_misheard = settings.commonly_misheard.clone();

        match llm.cleanup(CleanupRequest {
            raw_text: raw,
            style,
            app_context,
            window_ocr,
            preferred_transcriptions: preferred,
            commonly_misheard,
        }) {
            Ok(out) => {
                let tok_per_sec = out.tokens_per_second();
                tracing::info!(
                    "llm cleanup ({} ms{}): \"{raw}\" → \"{}\"",
                    out.elapsed_ms,
                    tok_per_sec
                        .map(|t| format!(", {t:.1} tok/s"))
                        .unwrap_or_default(),
                    out.text,
                );
                CleanupOutcome {
                    text: out.text,
                    elapsed_ms: out.elapsed_ms,
                    prompt_tokens: out.prompt_tokens,
                    completion_tokens: out.completion_tokens,
                    tok_per_sec,
                    error: None,
                }
            }
            Err(e) => {
                tracing::error!("llm cleanup failed, falling back to raw: {e}");
                CleanupOutcome {
                    error: Some(e.to_string()),
                    ..CleanupOutcome::passthrough(raw)
                }
            }
        }
    }

    fn emit_result(app: &AppHandle, result: &SttResult) {
        let _ = app.emit("dictation:result", result);
    }

    /// Build the platform-appropriate learning coordinator, or `None` on
    /// platforms without an accessibility reader. The coordinator is
    /// always constructed when the platform supports it — the gating on
    /// `auto_learn_corrections` happens per-paste, so flipping the
    /// settings toggle takes effect immediately.
    fn build_learning_coordinator(settings_store: SettingsStore) -> Option<LearningCoordinator> {
        let reader: Arc<dyn FocusedTextReader> = build_focused_text_reader()?;
        Some(LearningCoordinator::new(reader, Arc::new(settings_store)))
    }

    #[cfg(target_os = "macos")]
    fn build_focused_text_reader() -> Option<Arc<dyn FocusedTextReader>> {
        Some(Arc::new(crate::learning::MacosFocusedTextReader::new()))
    }

    #[cfg(not(target_os = "macos"))]
    fn build_focused_text_reader() -> Option<Arc<dyn FocusedTextReader>> {
        // Windows: TODO(wave-5b) — UIAutomation reader.
        // Linux: AT-SPI bridge, deferred.
        None
    }

    /// Parse the user's free-text vocabulary setting into a normalized
    /// list of distinct terms. Splits on commas / newlines, trims, drops
    /// blanks, dedupes (case-sensitive — proper-noun casing matters).
    fn parse_vocabulary(text: &str) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for raw in text.split([',', '\n', ';']) {
            let term = raw.trim();
            if term.is_empty() {
                continue;
            }
            if seen.insert(term.to_string()) {
                out.push(term.to_string());
            }
        }
        out
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
