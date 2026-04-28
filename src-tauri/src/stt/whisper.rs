//! Whisper STT via [`whisper_rs`] (bindings to whisper.cpp).
//!
//! Default model: `ggml-tiny.en.bin` (~75MB) — fastest English-only model
//! that produces usable transcripts on CPU. Production should swap to
//! `ggml-large-v3-turbo.bin` (~1.6GB) when available, settable per-user.
//!
//! Model files live at `dirs::data_dir() / "boothrflow" / "models" /`.
//! Missing-model is a non-fatal error — the engine returns a [`BoothError`]
//! with the download URL, the session daemon emits it to the UI.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use parking_lot::Mutex;
use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState,
};

use crate::error::{BoothError, Result};
use crate::stt::{SttEngine, SttResult};

/// Whisper requires at least ~1s of audio to produce useful output. Below
/// this we short-circuit and emit an empty transcript — saves a noisy
/// model invocation.
const MIN_AUDIO_SAMPLES: usize = 16_000 / 2; // 0.5s at 16kHz

/// Default English-only tiny model. Smallest viable Whisper for dev iteration.
pub const DEFAULT_MODEL_FILE: &str = "ggml-tiny.en.bin";

/// Where users get the file when it's missing.
pub const DEFAULT_MODEL_URL: &str =
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en.bin";

pub struct WhisperSttEngine {
    context: Arc<WhisperContext>,
    name: String,
}

impl WhisperSttEngine {
    /// Load a Whisper model from the given path.
    pub fn from_path(model_path: &Path, name: impl Into<String>) -> Result<Self> {
        if !model_path.exists() {
            return Err(missing_model_error(model_path));
        }

        let path_str = model_path.to_str().ok_or_else(|| {
            BoothError::Transcription(format!(
                "model path is not valid UTF-8: {}",
                model_path.display()
            ))
        })?;

        let context =
            WhisperContext::new_with_params(path_str, WhisperContextParameters::default())
                .map_err(|e| {
                    BoothError::Transcription(format!(
                        "failed to load whisper model from {}: {e}",
                        model_path.display()
                    ))
                })?;

        Ok(Self {
            context: Arc::new(context),
            name: name.into(),
        })
    }

    /// Try to load the default model (`ggml-tiny.en.bin`) from the standard
    /// per-user models directory.
    pub fn from_default_location() -> Result<Self> {
        let path = default_model_path().ok_or_else(|| {
            BoothError::Transcription("could not resolve user data directory".into())
        })?;
        Self::from_path(&path, "whisper-tiny.en")
    }
}

impl SttEngine for WhisperSttEngine {
    fn transcribe(&self, audio: &[f32]) -> Result<SttResult> {
        if audio.len() < MIN_AUDIO_SAMPLES {
            tracing::warn!(
                "whisper: short audio ({} samples, < {MIN_AUDIO_SAMPLES}), skipping",
                audio.len()
            );
            return Ok(SttResult {
                text: String::new(),
                language: Some("en".into()),
                duration_ms: 0,
            });
        }

        let started = Instant::now();
        let mut state: WhisperState = self
            .context
            .create_state()
            .map_err(|e| BoothError::Transcription(format!("create_state: {e}")))?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        let threads = std::thread::available_parallelism()
            .map(|n| n.get() as i32)
            .unwrap_or(4);
        params.set_n_threads(threads);
        params.set_translate(false);
        params.set_language(Some("en"));
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_suppress_blank(true);

        state
            .full(params, audio)
            .map_err(|e| BoothError::Transcription(format!("full: {e}")))?;

        let mut text = String::new();
        for segment in state.as_iter() {
            // Segment's Display impl renders text, replacing invalid UTF-8
            // with U+FFFD — fine for our use.
            text.push_str(&segment.to_string());
        }

        Ok(SttResult {
            text: text.trim().to_string(),
            language: Some("en".into()),
            duration_ms: started.elapsed().as_millis() as u64,
        })
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// Sharded across processes: the shared lock is to serialise concurrent
/// `transcribe` calls per-engine. Whisper is heavy; we don't want overlap.
pub struct SerializedWhisperSttEngine {
    inner: Mutex<WhisperSttEngine>,
}

impl SerializedWhisperSttEngine {
    pub fn new(engine: WhisperSttEngine) -> Self {
        Self {
            inner: Mutex::new(engine),
        }
    }
}

impl SttEngine for SerializedWhisperSttEngine {
    fn transcribe(&self, audio: &[f32]) -> Result<SttResult> {
        self.inner.lock().transcribe(audio)
    }
    fn name(&self) -> &str {
        // can't borrow through Mutex without holding it; return a static label
        "whisper-tiny.en"
    }
}

pub fn default_model_path() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("boothrflow").join("models").join(DEFAULT_MODEL_FILE))
}

pub fn default_models_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("boothrflow").join("models"))
}

fn missing_model_error(path: &Path) -> BoothError {
    BoothError::Transcription(format!(
        "Whisper model not found at {}.\n\
         Download it once with:\n\
           curl -L {DEFAULT_MODEL_URL} -o \"{}\"\n\
         (creates the parent directory first if needed)",
        path.display(),
        path.display()
    ))
}
