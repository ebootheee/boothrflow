//! NVIDIA Nemotron Speech Streaming en-0.6B engine.
//!
//! Why this exists: Parakeet-TDT-0.6B-v3 in sherpa-onnx is officially
//! not designed for streaming (see [`sherpa-onnx#2918`]), and its
//! NeMo model card explicitly warns "not recommended for
//! word-for-word / incomplete sentences" — consistent with the
//! pronoun-swap behavior the 2026-05-05 bench captured. Nemotron
//! Speech Streaming is the cache-aware FastConformer-RNNT NVIDIA
//! published as the native-streaming successor, with sherpa-onnx
//! graphs already shipping for the 80/160/560/1120 ms chunk targets
//! Wave 6 Phase 1 was spec'd against. LibriSpeech 2.32 / 4.84 WER at
//! the 1120 ms chunk is _better_ than `whisper-base.en` and natively
//! streaming.
//!
//! Model layout — same shape as the Parakeet bundle, populated by
//! `pnpm download:model:mac nemotron`:
//!
//! ```text
//! ${models_dir}/nemotron-speech-streaming-en-0.6b/
//!   encoder.onnx           ~250 MB int8
//!   decoder.onnx           ~30  MB
//!   joiner.onnx            ~5   MB
//!   tokens.txt             ~few KB
//! ```
//!
//! License: NVIDIA Open Model License Agreement — commercial OK, but
//! not Apache/MIT. Flagged in NOTICE for the OSS distribution.
//!
//! [`sherpa-onnx#2918`]: https://github.com/k2-fsa/sherpa-onnx/issues/2918

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::error::{BoothError, Result};
use crate::stt::online_transducer::{OnlineTransducerConfig, OnlineTransducerRecognizer};
use crate::stt::{SttEngine, SttResult};

/// 16 kHz matches what every sherpa-onnx ASR model we ship expects.
const NEMOTRON_SAMPLE_RATE: i32 = 16_000;
/// 80-dim mel filterbank, matches NeMo Cache-Aware FastConformer
/// preprocessing config.
const NEMOTRON_FEATURE_DIM: i32 = 80;

pub struct NemotronStreamingSttEngine {
    // The sherpa-onnx C++ recognizer keeps its own per-stream state,
    // so the outer Mutex isn't strictly required for correctness —
    // but `transcribe` creates a fresh stream each call and we don't
    // want two callers to step on each other's create/destroy. Same
    // discipline ParakeetSttEngine uses.
    recognizer: Mutex<OnlineTransducerRecognizer>,
    name: String,
    model_dir: PathBuf,
}

impl NemotronStreamingSttEngine {
    pub fn from_model_dir(model_dir: impl Into<PathBuf>) -> Result<Self> {
        let model_dir = model_dir.into();
        let encoder = require_file(&model_dir, "encoder.onnx")?;
        let decoder = require_file(&model_dir, "decoder.onnx")?;
        let joiner = require_file(&model_dir, "joiner.onnx")?;
        let tokens = require_file(&model_dir, "tokens.txt")?;

        let config = OnlineTransducerConfig {
            encoder: encoder.to_string_lossy().into_owned(),
            decoder: decoder.to_string_lossy().into_owned(),
            joiner: joiner.to_string_lossy().into_owned(),
            tokens: tokens.to_string_lossy().into_owned(),
            sample_rate: NEMOTRON_SAMPLE_RATE,
            feature_dim: NEMOTRON_FEATURE_DIM,
            num_threads: num_cpus_clamped(),
            debug: std::env::var("BOOTHRFLOW_NEMOTRON_DEBUG").is_ok(),
        };

        let name = format!(
            "nemotron:{}",
            model_dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
        );

        let recognizer = OnlineTransducerRecognizer::new(config, name.clone())
            .map_err(|e| BoothError::internal(format!("nemotron init: {e}")))?;

        tracing::info!("nemotron: loaded from {}", model_dir.display());

        Ok(Self {
            recognizer: Mutex::new(recognizer),
            name,
            model_dir,
        })
    }

    pub fn model_dir(&self) -> &Path {
        &self.model_dir
    }
}

impl SttEngine for NemotronStreamingSttEngine {
    fn transcribe(&self, audio: &[f32]) -> Result<SttResult> {
        let started = std::time::Instant::now();
        let recognizer = self
            .recognizer
            .lock()
            .map_err(|_| BoothError::internal("nemotron: recognizer mutex poisoned"))?;
        let text = recognizer.transcribe(NEMOTRON_SAMPLE_RATE as u32, audio)?;
        let duration_ms = started.elapsed().as_millis() as u64;
        Ok(SttResult {
            text: text.trim().to_string(),
            language: Some("en".into()),
            duration_ms,
        })
    }

    fn name(&self) -> &str {
        &self.name
    }
}

fn require_file(dir: &Path, name: &str) -> Result<PathBuf> {
    let path = dir.join(name);
    if !path.exists() {
        return Err(BoothError::internal(format!(
            "nemotron: missing model file {} (expected at {})",
            name,
            path.display()
        )));
    }
    Ok(path)
}

/// Same thread-count heuristic as Parakeet — clamp to 4 so the LLM
/// cleanup pass running in parallel via Ollama isn't starved.
fn num_cpus_clamped() -> i32 {
    std::thread::available_parallelism()
        .map(|n| (n.get() as i32).clamp(1, 4))
        .unwrap_or(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_model_dir_returns_err() {
        match NemotronStreamingSttEngine::from_model_dir("/nonexistent/nemotron") {
            Ok(_) => panic!("expected an error for a nonexistent dir"),
            Err(e) => {
                let msg = format!("{e}");
                assert!(msg.contains("missing model file"), "got: {msg}");
            }
        }
    }

    #[test]
    fn num_cpus_clamps_within_range() {
        let n = num_cpus_clamped();
        assert!((1..=4).contains(&n));
    }
}
