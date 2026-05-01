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

/// Generic English tech-vocab seed prompt — biases Whisper toward
/// recognising common domain words and proper nouns. Override with
/// `BOOTHRFLOW_WHISPER_PROMPT` env var for per-user vocabulary.
///
/// Capped at 224 tokens by whisper.cpp; this string is well under that
/// even after the recent expansion, so we have headroom for a few more
/// terms before needing a per-user (Personal Dictionary) tier.
const DEFAULT_INITIAL_PROMPT: &str =
    "The transcript may include the following terms: Claude, Claude Code, GPT, OpenAI, \
     Anthropic, Ollama, Tauri, Rust, TypeScript, Svelte, GitHub, kubectl, Kubernetes, \
     Docker, latency, throughput, async, await, refactor, repository, debugger, \
     middleware, schema, deploy, payload, monorepo, namespace, Qwen, Wispr, Boothe, \
     boothrflow, Whisper, FluidAudio, Parakeet, sherpa-onnx, llama.cpp, MTLDevice, \
     Metal, CoreML, Apple Silicon, M-series, Apple Vision, ScreenCaptureKit, \
     WhisperKit, RNNoise, DeepFilterNet, sqlite-vec, FTS5, nomic-embed, stardate.";

pub struct WhisperSttEngine {
    context: Arc<WhisperContext>,
    name: String,
    initial_prompt: Option<String>,
}

impl WhisperSttEngine {
    /// Cheap-clone access to the shared context. Streaming uses this to
    /// spin up its own [`WhisperState`] without re-loading the model file.
    pub fn shared_context(&self) -> Arc<WhisperContext> {
        Arc::clone(&self.context)
    }

    pub fn initial_prompt(&self) -> Option<&str> {
        self.initial_prompt.as_deref()
    }
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

        // Resolve initial_prompt from Settings first, then env/default.
        let initial_prompt = crate::settings::current_whisper_prompt(DEFAULT_INITIAL_PROMPT);

        Ok(Self {
            context: Arc::new(context),
            name: name.into(),
            initial_prompt,
        })
    }

    /// Try to load the default model from the standard per-user models
    /// directory. Honors `BOOTHRFLOW_WHISPER_MODEL_FILE` for swapping
    /// tiny → base / small / large-v3-turbo without recompile.
    pub fn from_default_location() -> Result<Self> {
        let file = crate::settings::current_whisper_model_file();

        let path = default_models_dir()
            .ok_or_else(|| {
                BoothError::Transcription("could not resolve user data directory".into())
            })?
            .join(&file);

        // Use the file stem as the engine name ("ggml-small.en" etc.).
        let name = std::path::Path::new(&file)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("whisper")
            .to_string();
        Self::from_path(&path, name)
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

        // initial_prompt biases Whisper toward the supplied vocabulary.
        // Capped at 224 tokens by whisper.cpp; stays in scope for the
        // first 30s segment then is overwritten by rolling decode history.
        if let Some(prompt) = &self.initial_prompt {
            params.set_initial_prompt(prompt);
        }

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
    let file = crate::settings::current_whisper_model_file();
    default_models_dir().map(|d| d.join(file))
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
