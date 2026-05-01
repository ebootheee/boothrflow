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
pub fn set_dictation_style(style: Style) {
    tracing::info!("settings: style → {style:?}");
    settings::set_current_style(style);
}

#[tauri::command]
pub fn settings_get(
    store: tauri::State<'_, settings::SettingsStore>,
) -> Result<settings::AppSettings, BoothError> {
    store.load()
}

#[tauri::command]
pub fn settings_update(
    store: tauri::State<'_, settings::SettingsStore>,
    patch: settings::SettingsPatch,
) -> Result<settings::AppSettings, BoothError> {
    store.update(patch)
}

#[tauri::command]
pub fn settings_options() -> settings::SettingsOptions {
    settings::settings_options()
}

#[tauri::command]
pub fn settings_export(
    store: tauri::State<'_, settings::SettingsStore>,
) -> Result<String, BoothError> {
    store.export_json()
}

#[tauri::command]
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
}

/// Open the relevant Privacy & Security pane in System Settings. No-op on
/// non-macOS platforms (returns Ok). The OS handles the rest — once the
/// user toggles the relevant switch, the next dictation should work.
#[tauri::command]
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
pub fn microphone_available() -> bool {
    use cpal::traits::HostTrait;
    let host = cpal::default_host();
    host.default_input_device().is_some()
}

#[cfg(not(feature = "real-engines"))]
#[tauri::command]
pub fn microphone_available() -> bool {
    true
}

/// Report which Whisper model file the daemon will load (or already loaded).
/// The UI uses this to show "tiny.en" / "base.en" / "small.en" rather than a
/// hardcoded label, so the chip stays honest after the user swaps models via
/// `BOOTHRFLOW_WHISPER_MODEL_FILE`. Returns the file stem ("ggml-small.en").
#[cfg(feature = "real-engines")]
#[tauri::command]
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
pub async fn history_recent(
    history: tauri::State<'_, Arc<HistoryStore>>,
    limit: Option<usize>,
) -> Result<Vec<HistoryEntry>, BoothError> {
    history.recent(limit.unwrap_or(50))
}

#[cfg(feature = "real-engines")]
#[tauri::command]
pub async fn history_search(
    history: tauri::State<'_, Arc<HistoryStore>>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<SearchResult>, BoothError> {
    history.search(&query, limit.unwrap_or(20))
}

#[cfg(feature = "real-engines")]
#[tauri::command]
pub async fn history_delete(
    history: tauri::State<'_, Arc<HistoryStore>>,
    id: i64,
) -> Result<(), BoothError> {
    history.delete(id)
}

#[cfg(feature = "real-engines")]
#[tauri::command]
pub async fn history_clear(history: tauri::State<'_, Arc<HistoryStore>>) -> Result<(), BoothError> {
    history.clear()
}

#[cfg(feature = "real-engines")]
#[tauri::command]
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
