//! Tauri commands exposed to the frontend.
//!
//! v0 only wires `dictate_once`, which currently runs against fakes. The real
//! pipeline lands when the engine deps are uncommented in Cargo.toml.

use serde::{Deserialize, Serialize};

use crate::audio::FakeAudioSource;
use crate::context::FixedContextDetector;
use crate::error::BoothError;
use crate::injector::RecordingInjector;
use crate::llm::FakeLlmCleanup;
use crate::pipeline::Pipeline;
use crate::settings::{self, Style};
use crate::stt::FakeSttEngine;

#[cfg(feature = "real-engines")]
use crate::history::{HistoryEntry, HistoryStats, HistoryStore, SearchResult};
#[cfg(feature = "real-engines")]
use crate::injector::{ClipboardInjector, Injector};
#[cfg(feature = "real-engines")]
use std::sync::Arc;

/// Update the active style. Called by the FE whenever the user changes the
/// dropdown; the session daemon reads `settings::current_style()` before
/// each LLM cleanup call.
#[tauri::command]
#[specta::specta]
pub fn set_dictation_style(style: Style) {
    tracing::info!("settings: style → {style:?}");
    settings::set_current_style(style);
}

#[tauri::command]
#[specta::specta]
pub fn settings_get(
    store: tauri::State<'_, settings::SettingsStore>,
) -> Result<settings::AppSettings, BoothError> {
    store.load()
}

#[tauri::command]
#[specta::specta]
pub fn settings_update(
    store: tauri::State<'_, settings::SettingsStore>,
    patch: settings::SettingsPatch,
) -> Result<settings::AppSettings, BoothError> {
    store.update(patch)
}

#[tauri::command]
#[specta::specta]
pub fn settings_options() -> settings::SettingsOptions {
    settings::settings_options()
}

#[tauri::command]
#[specta::specta]
pub fn settings_export(
    store: tauri::State<'_, settings::SettingsStore>,
) -> Result<String, BoothError> {
    store.export_json()
}

#[tauri::command]
#[specta::specta]
pub fn settings_import(
    store: tauri::State<'_, settings::SettingsStore>,
    json: String,
) -> Result<settings::AppSettings, BoothError> {
    store.import_json(&json)
}

#[cfg(feature = "real-engines")]
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct WhisperDownloadResult {
    pub model: String,
    pub file: String,
    pub path: String,
    pub already_present: bool,
}

#[cfg(feature = "real-engines")]
#[tauri::command]
#[specta::specta]
pub async fn whisper_download_model(model: String) -> Result<WhisperDownloadResult, BoothError> {
    let model_info = settings::whisper_model_for(&model)
        .ok_or_else(|| BoothError::internal(format!("unsupported Whisper model: {model}")))?;
    let dest_dir = crate::stt::default_models_dir()
        .ok_or_else(|| BoothError::internal("could not resolve user data directory"))?;
    let dest = dest_dir.join(model_info.file);

    if dest.exists() {
        return Ok(WhisperDownloadResult {
            model: model_info.value.into(),
            file: model_info.file.into(),
            path: dest.display().to_string(),
            already_present: true,
        });
    }

    std::fs::create_dir_all(&dest_dir)
        .map_err(|e| BoothError::internal(format!("create models dir: {e}")))?;

    let repo_script = std::env::current_dir()
        .map(|cwd| cwd.join("scripts").join("download-model.sh"))
        .ok()
        .filter(|path| path.exists());

    let status = if let Some(script) = repo_script {
        std::process::Command::new(script)
            .arg(model_info.download_arg)
            .status()
            .map_err(|e| BoothError::internal(format!("run model downloader: {e}")))?
    } else {
        let url = format!(
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{}",
            model_info.file
        );
        #[cfg(windows)]
        {
            let ps_command = format!(
                "Invoke-WebRequest -Uri '{}' -OutFile '{}'",
                url,
                dest.display()
            );
            std::process::Command::new("powershell")
                .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command"])
                .arg(ps_command)
                .status()
                .map_err(|e| BoothError::internal(format!("download model: {e}")))?
        }
        #[cfg(not(windows))]
        {
            std::process::Command::new("curl")
                .args(["-L", "--fail", "-o"])
                .arg(&dest)
                .arg(url)
                .status()
                .map_err(|e| BoothError::internal(format!("download model: {e}")))?
        }
    };

    if !status.success() || !dest.exists() {
        return Err(BoothError::internal(format!(
            "model download failed for {}",
            model_info.file
        )));
    }

    Ok(WhisperDownloadResult {
        model: model_info.value.into(),
        file: model_info.file.into(),
        path: dest.display().to_string(),
        already_present: false,
    })
}

#[cfg(not(feature = "real-engines"))]
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct WhisperDownloadResult {
    pub model: String,
    pub file: String,
    pub path: String,
    pub already_present: bool,
}

#[cfg(not(feature = "real-engines"))]
#[tauri::command]
#[specta::specta]
pub async fn whisper_download_model(model: String) -> Result<WhisperDownloadResult, BoothError> {
    let _ = model;
    Err(BoothError::internal(
        "Whisper model downloads require the real-engines build",
    ))
}

// ────────────────────────────────────────────────────────────────────────
// macOS permissions helpers
//
// On macOS we need three distinct privileges, each gated by its own TCC
// pane: Microphone (cpal capture), Accessibility (enigo paste), Input
// Monitoring (rdev's global keyboard hook). The OS prompts on first use
// only when the bundled Info.plist carries the matching usage string —
// in dev mode, the prompt is attributed to the parent process (Terminal,
// VS Code, etc.) and the user has to relaunch that process after
// granting. The UI uses these commands to (a) probe whether mic capture
// works right now, and (b) open the relevant System Settings pane.
// ────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum MacPermissionPane {
    Microphone,
    Accessibility,
    InputMonitoring,
    /// Required for the Wave 5 focused-window OCR capture path. The
    /// runtime call site is still a stub on every platform — see
    /// `docs/waves/wave-5-context-aware-cleanup.md` — but exposing the
    /// pane lets users pre-grant the permission so the first OCR
    /// capture after wiring doesn't fail with a denied prompt.
    ScreenRecording,
}

/// Open the relevant Privacy & Security pane in System Settings. No-op on
/// non-macOS platforms (returns Ok). The OS handles the rest — once the
/// user toggles the relevant switch, the next dictation should work.
#[tauri::command]
#[specta::specta]
pub fn open_macos_setting(pane: MacPermissionPane) -> Result<(), BoothError> {
    #[cfg(target_os = "macos")]
    {
        let url = match pane {
            MacPermissionPane::Microphone => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"
            }
            MacPermissionPane::Accessibility => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
            }
            MacPermissionPane::InputMonitoring => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent"
            }
            MacPermissionPane::ScreenRecording => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"
            }
        };
        std::process::Command::new("open")
            .arg(url)
            .status()
            .map_err(|e| BoothError::internal(format!("open settings: {e}")))?;
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = pane;
        Ok(())
    }
}

/// Probe whether the OS is currently letting us read the default input
/// device. A `false` result means either the mic was denied at the TCC
/// layer or no input device is connected — both of which prevent
/// dictation. Cheap; safe to call on app start. Real-engines only since
/// the cpal probe lives behind that feature.
#[cfg(feature = "real-engines")]
#[tauri::command]
#[specta::specta]
pub fn microphone_available() -> bool {
    use cpal::traits::HostTrait;
    let host = cpal::default_host();
    host.default_input_device().is_some()
}

#[cfg(not(feature = "real-engines"))]
#[tauri::command]
#[specta::specta]
pub fn microphone_available() -> bool {
    true
}

/// Probe whether Screen Recording TCC is currently granted, without
/// triggering the OS prompt. Returns `true` on non-macOS (no-op) so
/// the FE doesn't need to branch — the OCR feature itself is gated
/// at runtime by the actual capture call.
#[tauri::command]
#[specta::specta]
pub fn screen_recording_available() -> bool {
    #[cfg(target_os = "macos")]
    {
        use objc2_core_graphics::CGPreflightScreenCaptureAccess;
        CGPreflightScreenCaptureAccess()
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

/// Eagerly request Screen Recording permission so the OS prompt
/// fires now (rather than the first time the cleanup pass tries an
/// OCR capture mid-dictation, which is the worst possible moment).
/// Returns whether access is granted afterwards. The OS prompt only
/// appears once per app per macOS reset; subsequent calls just
/// return the current state. Calls `CGRequestScreenCaptureAccess()`
/// which both prompts and registers the app in System Settings →
/// Privacy & Security → Screen Recording.
#[tauri::command]
#[specta::specta]
pub fn request_screen_recording_permission() -> bool {
    #[cfg(target_os = "macos")]
    {
        use objc2_core_graphics::CGRequestScreenCaptureAccess;
        CGRequestScreenCaptureAccess()
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

/// Report which Whisper model file the daemon will load (or already loaded).
/// The UI uses this to show "tiny.en" / "base.en" / "small.en" rather than a
/// hardcoded label, so the chip stays honest after the user swaps models via
/// `BOOTHRFLOW_WHISPER_MODEL_FILE`. Returns the file stem ("ggml-small.en").
#[cfg(feature = "real-engines")]
#[tauri::command]
#[specta::specta]
pub fn whisper_model_name() -> String {
    let file = settings::current_whisper_model_file();
    std::path::Path::new(&file)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("whisper")
        .to_string()
}

#[cfg(not(feature = "real-engines"))]
#[tauri::command]
#[specta::specta]
pub fn whisper_model_name() -> String {
    "fake".into()
}

// ────────────────────────────────────────────────────────────────────────
// History commands (P3 W7-D)
//
// These form the IPC contract for the UI agent. Frontend types are derived
// via `specta::Type` on HistoryEntry / SearchResult / HistoryStats, so the
// UI side gets typed shapes directly out of bindings.
// ────────────────────────────────────────────────────────────────────────

#[cfg(feature = "real-engines")]
#[tauri::command]
#[specta::specta]
pub async fn history_recent(
    history: tauri::State<'_, Arc<HistoryStore>>,
    limit: Option<usize>,
) -> Result<Vec<HistoryEntry>, BoothError> {
    history.recent(limit.unwrap_or(50))
}

#[cfg(feature = "real-engines")]
#[tauri::command]
#[specta::specta]
pub async fn history_search(
    history: tauri::State<'_, Arc<HistoryStore>>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<SearchResult>, BoothError> {
    history.search(&query, limit.unwrap_or(20))
}

#[cfg(feature = "real-engines")]
#[tauri::command]
#[specta::specta]
pub async fn history_delete(
    history: tauri::State<'_, Arc<HistoryStore>>,
    id: i64,
) -> Result<(), BoothError> {
    history.delete(id)
}

#[cfg(feature = "real-engines")]
#[tauri::command]
#[specta::specta]
pub async fn history_clear(history: tauri::State<'_, Arc<HistoryStore>>) -> Result<(), BoothError> {
    history.clear()
}

#[cfg(feature = "real-engines")]
#[tauri::command]
#[specta::specta]
pub async fn history_stats(
    history: tauri::State<'_, Arc<HistoryStore>>,
) -> Result<HistoryStats, BoothError> {
    history.stats()
}

/// Paste the formatted text of a history entry. Called from the main
/// settings UI; pastes wherever focus currently sits. For paste-back-into-
/// the-previously-focused-app behavior, use [`quickpaste_paste`] instead.
#[cfg(feature = "real-engines")]
#[tauri::command]
#[specta::specta]
pub async fn history_paste(
    history: tauri::State<'_, Arc<HistoryStore>>,
    id: i64,
) -> Result<HistoryPasteResult, BoothError> {
    let Some(text) = history.get_formatted(id)? else {
        return Err(BoothError::internal(format!("history id {id} not found")));
    };
    let injector = ClipboardInjector::new()?;
    injector.inject(&text)?;
    Ok(HistoryPasteResult {
        id,
        len: text.len(),
    })
}

/// Quick-paste palette flow: hide the palette, restore focus to the
/// previously-captured target window, then paste. Called by the palette
/// UI when the user picks an entry (Enter or click).
#[cfg(feature = "real-engines")]
#[tauri::command]
#[specta::specta]
pub async fn quickpaste_paste(
    app: tauri::AppHandle,
    history: tauri::State<'_, Arc<HistoryStore>>,
    id: i64,
) -> Result<HistoryPasteResult, BoothError> {
    let Some(text) = history.get_formatted(id)? else {
        return Err(BoothError::internal(format!("history id {id} not found")));
    };

    // Hide first so focus can return to the target window. SetForegroundWindow
    // works best when no window is actively claiming focus.
    let _ = crate::quickpaste::hide(&app);
    crate::quickpaste::restore_target_window();
    // A small beat lets the OS settle focus before the SendInput Ctrl+V.
    std::thread::sleep(std::time::Duration::from_millis(40));

    let injector = ClipboardInjector::new()?;
    injector.inject(&text)?;
    Ok(HistoryPasteResult {
        id,
        len: text.len(),
    })
}

/// Hide the quick-paste palette without pasting (e.g. user pressed Esc).
#[cfg(feature = "real-engines")]
#[tauri::command]
#[specta::specta]
pub async fn quickpaste_close(app: tauri::AppHandle) -> Result<(), BoothError> {
    crate::quickpaste::hide(&app)?;
    crate::quickpaste::restore_target_window();
    Ok(())
}

#[cfg(feature = "real-engines")]
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct HistoryPasteResult {
    pub id: i64,
    pub len: usize,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct DictateResult {
    pub raw: String,
    pub formatted: String,
    pub duration_ms: u64,
}

#[tauri::command]
#[specta::specta]
pub async fn dictate_once(style: Style) -> Result<DictateResult, BoothError> {
    // v0: still using fakes end-to-end. When real engines are wired in, this
    // body becomes the only place that swaps the trait objects.
    let audio = FakeAudioSource::silence(0.5);
    let stt = FakeSttEngine::canned("uh basically hello there from the fake stt");
    let llm = FakeLlmCleanup;
    let injector = RecordingInjector::new();
    let context = FixedContextDetector::none();

    let pipeline = Pipeline {
        audio: &audio,
        stt: &stt,
        llm: &llm,
        injector: &injector,
        context: &context,
    };

    let outcome = pipeline
        .dictate_once(style, true)
        .map_err(|e| BoothError::Internal(e.to_string()))?;

    Ok(DictateResult {
        raw: outcome.raw,
        formatted: outcome.formatted,
        duration_ms: outcome.duration_ms,
    })
}

// ────────────────────────────────────────────────────────────────────────
// Wave 4b polish — incoming from Casper's PR ideas
// ────────────────────────────────────────────────────────────────────────

/// 1-token "are you there" probe against the configured LLM endpoint.
/// Returns latency on success; bubbles up the error string on failure so
/// the UI can render either case inline. Uses the user's current settings,
/// not env vars — so it tests what the user sees in the panel, not what
/// the binary launched with.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct LlmTestResult {
    pub ok: bool,
    pub latency_ms: u64,
    pub error: Option<String>,
}

#[cfg(feature = "real-engines")]
#[tauri::command]
#[specta::specta]
pub async fn llm_test_connection(
    store: tauri::State<'_, settings::SettingsStore>,
) -> Result<LlmTestResult, BoothError> {
    use crate::llm::{CleanupRequest, LlmCleanup, OpenAiCompatLlmCleanup};
    use std::time::Instant;
    let app_settings = store.load()?;
    let llm = app_settings.llm;
    // The reqwest::blocking::Client owns an internal tokio runtime; if
    // we construct it on the async caller's runtime worker thread and
    // then drop it on that same thread, tokio panics with "Cannot drop
    // a runtime in a context where blocking is not allowed". Construct
    // AND drop the probe entirely inside spawn_blocking so the Client's
    // runtime lives + dies on a blocking-allowed thread.
    let join_result = tokio::task::spawn_blocking(move || -> (bool, u64, Option<String>) {
        let started = Instant::now();
        let probe = match OpenAiCompatLlmCleanup::new(
            llm.endpoint.clone(),
            llm.model.clone(),
            llm.api_key
                .as_ref()
                .filter(|k| !k.trim().is_empty())
                .cloned(),
        ) {
            Ok(p) => p,
            Err(e) => {
                return (
                    false,
                    started.elapsed().as_millis() as u64,
                    Some(e.to_string()),
                );
            }
        };
        let result = probe
            .cleanup(CleanupRequest {
                raw_text: "ping",
                style: crate::settings::Style::Raw,
                ..Default::default()
            })
            .map(|_| ());
        let latency_ms = started.elapsed().as_millis() as u64;
        match result {
            Ok(()) => (true, latency_ms, None),
            Err(e) => (false, latency_ms, Some(e.to_string())),
        }
    })
    .await;

    match join_result {
        Ok((ok, latency_ms, error)) => Ok(LlmTestResult {
            ok,
            latency_ms,
            error,
        }),
        Err(e) => Ok(LlmTestResult {
            ok: false,
            latency_ms: 0,
            error: Some(format!("test task failed: {e}")),
        }),
    }
}

#[cfg(not(feature = "real-engines"))]
#[tauri::command]
#[specta::specta]
pub async fn llm_test_connection() -> Result<LlmTestResult, BoothError> {
    // Fakes-only build doesn't have the real HTTP client; pretend the
    // probe worked so the FE smoke path still exercises this command.
    Ok(LlmTestResult {
        ok: true,
        latency_ms: 0,
        error: None,
    })
}

/// App version reported by the Cargo manifest. Used by the About section.
#[tauri::command]
#[specta::specta]
pub fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Open a path (file or directory) in the OS file browser. Used by the
/// About section's "reveal in Finder" links — model dir, history db.
/// Falls through to `tauri-plugin-opener` so the same command works
/// across macOS / Windows / Linux.
#[tauri::command]
#[specta::specta]
pub async fn reveal_path(app: tauri::AppHandle, path: String) -> Result<(), BoothError> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .reveal_item_in_dir(&path)
        .map_err(|e| BoothError::internal(format!("reveal {path}: {e}")))
}
