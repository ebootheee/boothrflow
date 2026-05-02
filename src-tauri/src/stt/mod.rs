//! Speech-to-text engine.
//!
//! Production wraps `transcribe-rs` (multi-engine: whisper-cpp, parakeet,
//! moonshine, …). A scripted fake answers tests without any model files.

use serde::Serialize;

use crate::error::Result;

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct SttResult {
    pub text: String,
    pub language: Option<String>,
    pub duration_ms: u64,
}

pub trait SttEngine: Send + Sync {
    /// Transcribe a single utterance (16kHz mono PCM) end-to-end.
    fn transcribe(&self, audio: &[f32]) -> Result<SttResult>;

    /// Engine identifier for diagnostics ("parakeet-tdt-0.6b-v3", …).
    fn name(&self) -> &str;
}

#[cfg(any(test, feature = "test-fakes"))]
pub mod fake;
#[cfg(any(test, feature = "test-fakes"))]
pub use fake::FakeSttEngine;

#[cfg(feature = "real-engines")]
pub mod whisper;
#[cfg(feature = "real-engines")]
pub use whisper::{
    default_model_path, default_models_dir, SerializedWhisperSttEngine, WhisperSttEngine,
    DEFAULT_MODEL_FILE, DEFAULT_MODEL_URL,
};

#[cfg(feature = "real-engines")]
pub mod streaming;
#[cfg(feature = "real-engines")]
pub use streaming::{StreamingPartial, StreamingTranscriber};

#[cfg(feature = "parakeet-engine")]
pub mod parakeet;
#[cfg(feature = "parakeet-engine")]
pub use parakeet::ParakeetSttEngine;
