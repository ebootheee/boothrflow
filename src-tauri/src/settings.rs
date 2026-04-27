use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub enum Style {
    Raw,
    Formal,
    Casual,
    Excited,
    VeryCasual,
}

impl Default for Style {
    fn default() -> Self {
        Self::Casual
    }
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
