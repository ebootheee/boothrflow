use std::sync::atomic::{AtomicU8, Ordering};

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub enum Style {
    Raw = 0,
    Formal = 1,
    #[default]
    Casual = 2,
    Excited = 3,
    VeryCasual = 4,
}

impl Style {
    fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Raw,
            1 => Self::Formal,
            3 => Self::Excited,
            4 => Self::VeryCasual,
            _ => Self::Casual,
        }
    }
}

/// Runtime-mutable current style. Updated by the `set_dictation_style`
/// Tauri command on every dropdown change in the UI; read by the session
/// daemon before each LLM cleanup call. Persistence across restarts is a
/// later milestone (Phase 3 settings.toml).
static CURRENT_STYLE: AtomicU8 = AtomicU8::new(Style::Casual as u8);

pub fn current_style() -> Style {
    Style::from_u8(CURRENT_STYLE.load(Ordering::Relaxed))
}

pub fn set_current_style(style: Style) {
    CURRENT_STYLE.store(style as u8, Ordering::Relaxed);
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct Settings {
    pub style: Style,
    pub hotkey: String,
    pub llm_enabled: bool,
    pub privacy_mode: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            style: Style::default(),
            hotkey: "Ctrl+Win".into(),
            llm_enabled: true,
            privacy_mode: false,
        }
    }
}
