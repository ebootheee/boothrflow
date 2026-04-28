//! Tauri commands exposed to the frontend.
//!
//! v0 only wires `dictate_once`, which currently runs against fakes. The real
//! pipeline lands when the engine deps are uncommented in Cargo.toml.

use serde::Serialize;

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
