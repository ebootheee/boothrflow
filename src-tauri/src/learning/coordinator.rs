//! Background coordinator that observes a paste, waits a short
//! settling window, and looks for a small edit that signals a
//! recognition correction. When found, it appends the new pair to
//! `AppSettings.commonly_misheard` so the cleanup prompt's
//! `<USER-CORRECTIONS>` block starts applying it next time.
//!
//! The coordinator is a single fire-and-forget thread per paste —
//! cheaper than a long-lived worker because the only state that
//! crosses dictations is the persisted settings file. Per-paste
//! overhead: one thread spawn, one sleep, one AX read, one
//! Levenshtein. Negligible.

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crate::learning::{detect_correction, FocusedTextReader};
use crate::settings::SettingsStore;

/// One observation handed to the coordinator: what we pasted, when
/// we pasted it. The window-title hint gates the AX read — if the
/// user has switched to a different window during the settling
/// period, the read would observe an unrelated field's text.
pub struct PasteSnapshot {
    pub pasted_text: String,
}

/// How long to wait after a paste before sampling the focused field.
/// Long enough that fast edits (~1s reaction time) are caught, short
/// enough that the user has likely either accepted or rejected the
/// paste. A future polling variant could revisit at multiple points
/// to catch slower edits.
const SETTLING_DURATION: Duration = Duration::from_secs(8);

/// Most recent corrections to keep. The `<USER-CORRECTIONS>` block
/// in the cleanup prompt scales linearly with this count, and Qwen
/// 2.5 7B's context is finite — past ~50 entries the ROI drops as
/// the prompt gets noisy. Older entries are dropped FIFO when this
/// cap is hit.
const MAX_LEARNED_CORRECTIONS: usize = 50;

pub struct LearningCoordinator {
    reader: Arc<dyn FocusedTextReader>,
    settings_store: Arc<SettingsStore>,
}

impl LearningCoordinator {
    pub fn new(reader: Arc<dyn FocusedTextReader>, settings_store: Arc<SettingsStore>) -> Self {
        Self {
            reader,
            settings_store,
        }
    }

    /// Spawn a one-shot observation thread for the given paste. Returns
    /// immediately; the heavy work happens in the background.
    pub fn observe(&self, snapshot: PasteSnapshot) {
        let reader = Arc::clone(&self.reader);
        let store = Arc::clone(&self.settings_store);

        thread::Builder::new()
            .name("boothrflow-learning".into())
            .spawn(move || {
                if let Err(e) = run_observation(reader, store, snapshot) {
                    tracing::warn!("learning observation failed: {e}");
                }
            })
            .ok();
    }
}

fn run_observation(
    reader: Arc<dyn FocusedTextReader>,
    store: Arc<SettingsStore>,
    snapshot: PasteSnapshot,
) -> crate::error::Result<()> {
    thread::sleep(SETTLING_DURATION);

    // Re-check the opt-in flag after the sleep. The user may have
    // toggled auto-learn off — or flipped privacy mode on — during the
    // settling window. Either way, abort silently before sampling.
    let settings_after_sleep = crate::settings::current_app_settings();
    if !settings_after_sleep.auto_learn_corrections || crate::settings::privacy_mode_enabled() {
        return Ok(());
    }

    let current = match reader.read_focused_text() {
        Some(text) => text,
        None => {
            tracing::trace!(
                "learning: focused-field read returned None (reader={})",
                reader.name()
            );
            return Ok(());
        }
    };

    let Some(pair) = detect_correction(&snapshot.pasted_text, &current) else {
        tracing::trace!("learning: no single-word correction detected");
        return Ok(());
    };

    // Reject self-collisions: if `pair.wrong` already exists in the
    // user's preferred-spellings list, applying the substitution would
    // contradict it. Bail.
    let settings = crate::settings::current_app_settings();
    let already_preferred = settings
        .vocabulary
        .split([',', '\n', ';'])
        .map(|t| t.trim())
        .any(|t| t.eq_ignore_ascii_case(&pair.wrong));
    if already_preferred {
        tracing::trace!("learning: {} already in vocabulary, skipping", pair.wrong);
        return Ok(());
    }

    // Reject if the pair already exists. Don't promote a duplicate
    // entry; the LLM already gets the substitution.
    let exists = settings
        .commonly_misheard
        .iter()
        .any(|m| m.wrong.eq_ignore_ascii_case(&pair.wrong) && m.right == pair.right);
    if exists {
        tracing::trace!("learning: {} → {} already learned", pair.wrong, pair.right);
        return Ok(());
    }

    let mut next = settings.commonly_misheard.clone();
    next.push(pair.clone());
    if next.len() > MAX_LEARNED_CORRECTIONS {
        let drop = next.len() - MAX_LEARNED_CORRECTIONS;
        next.drain(..drop);
    }

    let patch = crate::settings::SettingsPatch {
        commonly_misheard: Some(next),
        ..Default::default()
    };
    if let Err(e) = store.update(patch) {
        tracing::warn!("learning: settings store update failed: {e}");
    } else {
        tracing::info!(
            "learning: recorded correction {} → {}",
            pair.wrong,
            pair.right
        );
    }

    Ok(())
}
