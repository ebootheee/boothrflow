use serde::Serialize;
use thiserror::Error;

/// Crate-wide error. Adjacently-tagged enum so the FE can pattern-match
/// on `kind` while specta-serde stays happy (newtype variants like
/// `AudioCapture(String)` can't coexist with an internally-tagged enum
/// because the payload can't be merged with the tag at the same level).
#[derive(Debug, Error, Serialize, specta::Type)]
#[serde(tag = "kind", content = "message", rename_all = "kebab-case")]
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
