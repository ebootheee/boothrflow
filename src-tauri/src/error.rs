use serde::Serialize;
use thiserror::Error;

/// Crate-wide error. Tagged enum is what crosses the Tauri boundary, so the
/// frontend can pattern-match on `kind`.
#[derive(Debug, Error, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum BoothError {
    #[error("audio capture failed: {0}")]
    AudioCapture(String),

    #[error("transcription failed: {0}")]
    Transcription(String),

    #[error("formatting failed: {0}")]
    Formatting(String),

    #[error("injection failed: {0}")]
    Injection(String),

    #[error("internal error: {0}")]
    Internal(String),
}

pub type Result<T, E = BoothError> = std::result::Result<T, E>;

impl BoothError {
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }
}
