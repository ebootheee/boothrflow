//! NVIDIA Parakeet TDT 0.6B v3 STT engine, wrapped over sherpa-onnx.
//!
//! Why this exists: Parakeet TDT 0.6B v3 is faster and more accurate
//! than Whisper tiny.en on most benchmarks, with native streaming
//! support. The engine ships behind the `parakeet-engine` Cargo
//! feature so the baseline build stays small (no ONNX runtime
//! download for users on Whisper).
//!
//! Model layout — the engine resolves all four files from a
//! per-engine "models" directory; the user populates it via the
//! download script:
//!
//! ```text
//! ${models_dir}/parakeet-tdt-0.6b-v3/
//!   encoder.onnx           ~150 MB
//!   decoder.onnx           ~30  MB
//!   joiner.onnx            ~5   MB
//!   tokens.txt             ~few KB
//! ```
//!
//! Audio contract: 16kHz mono f32 PCM (same as Whisper's path).
//! Parakeet's feature extractor is fixed at 80-dim mel filterbanks;
//! we feed waveform directly and let sherpa-onnx do the framing.
//!
//! Streaming: Parakeet supports streaming via sherpa-onnx's online
//! recognizer API. This implementation is offline-only (single
//! transcribe-once call) — same shape as our current Whisper path.
//! Streaming integration with the existing
//! `stt::streaming::LocalAgreement2` aggregator is a Wave 5d
//! enhancement; the LA2 layer is already engine-agnostic so the
//! refactor will be mechanical.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use sherpa_rs::transducer::{TransducerConfig, TransducerRecognizer};

use crate::error::{BoothError, Result};
use crate::stt::{SttEngine, SttResult};

/// Sample rate sherpa-onnx expects for Parakeet's feature extractor.
const PARAKEET_SAMPLE_RATE: i32 = 16_000;
/// Mel filterbank dimensionality; matches Parakeet's training config.
const PARAKEET_FEATURE_DIM: i32 = 80;
/// Default decoding method: `greedy_search` is the fastest path and
/// the only one our offline use-case needs (no n-best diversity).
const DEFAULT_DECODING: &str = "greedy_search";

pub struct ParakeetSttEngine {
    recognizer: Mutex<TransducerRecognizer>,
    name: String,
    model_dir: PathBuf,
}

impl ParakeetSttEngine {
    /// Construct an engine pointing at a directory that contains the
    /// four expected files. Returns `Err(BoothError::Internal(...))`
    /// on missing files, missing ONNX metadata, or sherpa-onnx
    /// initialization failure.
    pub fn from_model_dir(model_dir: impl Into<PathBuf>) -> Result<Self> {
        let model_dir = model_dir.into();
        let encoder = require_file(&model_dir, "encoder.onnx")?;
        let decoder = require_file(&model_dir, "decoder.onnx")?;
        let joiner = require_file(&model_dir, "joiner.onnx")?;
        let tokens = require_file(&model_dir, "tokens.txt")?;

        // sherpa-onnx 1.10+ requires the decoder ONNX file to carry a
        // `vocab_size` metadata key. Older NeMo Parakeet exports
        // (sherpa-onnx-nemo-parakeet-tdt-0.6b-v2 / v2-int8 from the
        // `asr-models` GitHub release) don't include it, and the
        // sherpa-onnx C++ side calls `exit(-1)` during decode when
        // the field is missing — which takes the whole boothrflow
        // process down. Pre-check the decoder file here so we fail
        // gracefully (Err propagates up to `dictation:model-missing`,
        // session daemon stays alive, user can pick another engine).
        validate_decoder_metadata(&decoder)?;

        let config = TransducerConfig {
            encoder: encoder.to_string_lossy().into_owned(),
            decoder: decoder.to_string_lossy().into_owned(),
            joiner: joiner.to_string_lossy().into_owned(),
            tokens: tokens.to_string_lossy().into_owned(),
            sample_rate: PARAKEET_SAMPLE_RATE,
            feature_dim: PARAKEET_FEATURE_DIM,
            decoding_method: DEFAULT_DECODING.into(),
            num_threads: num_cpus_clamped(),
            // `transducer` is the right model_type for Parakeet TDT.
            // sherpa-onnx auto-detects but explicit is safer.
            model_type: "transducer".into(),
            ..Default::default()
        };

        let recognizer = TransducerRecognizer::new(config).map_err(|e| {
            BoothError::internal(format!("parakeet sherpa-onnx init: {e}"))
        })?;

        let name = format!(
            "parakeet:{}",
            model_dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
        );

        tracing::info!("parakeet: loaded from {}", model_dir.display());

        Ok(Self {
            recognizer: Mutex::new(recognizer),
            name,
            model_dir,
        })
    }

    /// Path the engine was loaded from. Useful for diagnostics in
    /// `whisper_model_name`-style commands when Parakeet is the
    /// active engine.
    pub fn model_dir(&self) -> &Path {
        &self.model_dir
    }
}

impl SttEngine for ParakeetSttEngine {
    fn transcribe(&self, audio: &[f32]) -> Result<SttResult> {
        let started = std::time::Instant::now();
        // sherpa-rs's TransducerRecognizer::transcribe takes &mut self
        // (the underlying C++ recognizer is reused per stream), so
        // the engine wraps it in a Mutex to satisfy `SttEngine`'s
        // `&self` API. No real contention — the session daemon
        // serializes dictations.
        let mut recognizer = self.recognizer.lock().map_err(|_| {
            BoothError::internal("parakeet: recognizer mutex poisoned")
        })?;
        let text = recognizer.transcribe(PARAKEET_SAMPLE_RATE as u32, audio);
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
            "parakeet: missing model file {} (expected at {})",
            name,
            path.display()
        )));
    }
    Ok(path)
}

/// Defensive pre-check: scan the decoder ONNX file for the literal
/// bytes `vocab_size`. ONNX metadata_props are protobuf-encoded but
/// the keys are stored as plain UTF-8 strings, so a substring search
/// is reliable. Files are ~30MB; the scan takes ~50ms on Apple
/// Silicon and only runs once at engine init.
///
/// Returns `Err` with a user-actionable message when the metadata is
/// missing — the failure mode that bricks the app when sherpa-onnx
/// hits it during decode.
fn validate_decoder_metadata(decoder_path: &Path) -> Result<()> {
    let bytes = std::fs::read(decoder_path).map_err(|e| {
        BoothError::internal(format!(
            "parakeet: read decoder.onnx for metadata check: {e}"
        ))
    })?;
    let needle = b"vocab_size";
    let found = bytes
        .windows(needle.len())
        .any(|window| window == needle);
    if !found {
        return Err(BoothError::internal(
            "parakeet: decoder.onnx is missing the `vocab_size` metadata \
             key required by sherpa-onnx 1.10+. The bundle published as \
             sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8 was built before \
             that requirement landed; sherpa-onnx will exit(-1) during \
             decode if loaded. See docs/waves/wave-5-context-aware-cleanup.md \
             for the bundle-rebuild plan. Falling back to Whisper.",
        ));
    }
    Ok(())
}

/// Pick a sensible thread count for sherpa-onnx. Default is 1 which
/// underutilizes modern hardware; cap at 4 because Parakeet's
/// transducer doesn't scale linearly past that and we don't want to
/// starve the LLM cleanup pass running in parallel via Ollama.
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
        // Use match instead of unwrap_err so we don't require
        // Debug on ParakeetSttEngine (Mutex<TransducerRecognizer>
        // isn't Debug).
        match ParakeetSttEngine::from_model_dir("/nonexistent/parakeet") {
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

    #[test]
    fn metadata_guard_rejects_files_missing_vocab_size() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("decoder.onnx");
        // Synthetic ONNX-shaped bytes without the vocab_size key.
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"\x08\x07\x12\x07pytorch\x52\x00")
            .unwrap();
        match validate_decoder_metadata(&path) {
            Ok(()) => panic!("expected vocab_size guard to fire"),
            Err(e) => {
                assert!(format!("{e}").contains("vocab_size"));
            }
        }
    }

    #[test]
    fn metadata_guard_accepts_files_with_vocab_size() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("decoder.onnx");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"\x08\x07\x12\nvocab_size\x52\x051024\x00")
            .unwrap();
        validate_decoder_metadata(&path).unwrap();
    }
}
