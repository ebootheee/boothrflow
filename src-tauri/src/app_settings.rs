//! On-disk app settings persisted as JSON in the app's data directory.
//!
//! Currently scoped to LLM endpoint config so the user can point boothrflow
//! at any OpenAI-compatible server (Ollama, llama-server, LM Studio,
//! OpenAI/Anthropic/Groq/OpenRouter, Switchboard) without setting env vars.
//! Read once at session-daemon startup; writes from the Settings UI prompt
//! a "restart to apply" notice.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const FILENAME: &str = "settings.json";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AppSettings {
    pub llm: LlmSettings,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LlmSettings {
    /// Full chat-completions URL. Empty string means "use default".
    pub endpoint: String,
    /// Model name as the endpoint expects it. Empty string means "use default".
    pub model: String,
    /// Optional bearer token. Empty string means "no auth".
    pub api_key: String,
    /// When true, skip LLM cleanup entirely.
    pub disabled: bool,
}

pub fn settings_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join(FILENAME)
}

pub fn load(app_data_dir: &Path) -> AppSettings {
    let path = settings_path(app_data_dir);
    let Ok(bytes) = fs::read(&path) else {
        return AppSettings::default();
    };
    serde_json::from_slice(&bytes).unwrap_or_else(|e| {
        tracing::warn!("settings: parse failed at {}: {e}", path.display());
        AppSettings::default()
    })
}

pub fn save(app_data_dir: &Path, settings: &AppSettings) -> std::io::Result<()> {
    fs::create_dir_all(app_data_dir)?;
    let path = settings_path(app_data_dir);
    let json = serde_json::to_vec_pretty(settings)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    fs::write(path, json)
}
