//! Thin Rust wrapper over sherpa-onnx's online (streaming) transducer
//! recognizer.
//!
//! `sherpa-rs` 0.6.8 exposes only the OFFLINE transducer recognizer
//! (`sherpa_rs::transducer::TransducerRecognizer`, used by
//! `ParakeetSttEngine`). The online recognizer types exist in the
//! sherpa-onnx C API and are surfaced through `sherpa-rs-sys`'s
//! bindgen output, but there is no high-level Rust wrapper. This
//! module wraps just enough of them for `NemotronStreamingSttEngine`
//! to drive the C side from safe Rust.
//!
//! Why we need this at all: NemoTron Speech Streaming is exported as
//! a cache-aware streaming graph (the encoder takes streaming cache
//! tensors as inputs), so loading it through the offline recognizer
//! API blows up at graph-init time. The online recognizer is the
//! only path that understands the cache tensors.
//!
//! Scope: one Recognizer per loaded model, one per-call Stream, one
//! all-at-once `transcribe` entry point that feeds an entire utterance
//! buffer. Production streaming (chunked accept_waveform with partial
//! result polling) is intentionally NOT exposed yet — `bench_replay`
//! and the current `SttEngine` trait both want utterance-at-a-time
//! shapes. When the production hot path moves to true streaming, the
//! stream lifecycle will split out of `transcribe` and onto its own
//! type. For now this deliberately mirrors the offline shape so
//! Nemotron can slot into `SttEngine` unchanged.

use std::ffi::{CStr, CString};
use std::mem;
use std::os::raw::c_char;

use sherpa_rs_sys as sys;

use crate::error::{BoothError, Result};

/// Public configuration for an online transducer recognizer.
/// The four `*.onnx` paths plus the tokens file are the same shape as
/// the offline Parakeet bundle, just with different graph internals.
#[derive(Debug, Clone)]
pub struct OnlineTransducerConfig {
    pub encoder: String,
    pub decoder: String,
    pub joiner: String,
    pub tokens: String,
    /// 16000 for every sherpa-onnx ASR model we ship.
    pub sample_rate: i32,
    /// Mel filterbank dimensionality. 80 for FastConformer and
    /// Cache-Aware FastConformer (Nemotron streaming).
    pub feature_dim: i32,
    pub num_threads: i32,
    pub debug: bool,
}

pub struct OnlineTransducerRecognizer {
    recognizer: *const sys::SherpaOnnxOnlineRecognizer,
    name: String,
    // Keep the CString-backed config strings alive for the lifetime of
    // the recognizer. The sherpa-onnx C++ side copies the paths into
    // its own std::strings during create, so in practice these could be
    // dropped after the create call — but holding them is cheap and
    // makes the wrapper robust to any future ABI change that decides
    // to retain the pointers.
    _strings: Vec<CString>,
}

impl OnlineTransducerRecognizer {
    pub fn new(config: OnlineTransducerConfig, name: String) -> Result<Self> {
        let encoder = cstring(&config.encoder, "encoder")?;
        let decoder = cstring(&config.decoder, "decoder")?;
        let joiner = cstring(&config.joiner, "joiner")?;
        let tokens = cstring(&config.tokens, "tokens")?;
        // Hardcoded strings; CString::new on a bare literal can't fail.
        let provider = CString::new("cpu").expect("provider");
        let decoding = CString::new("greedy_search").expect("decoding");
        let empty = CString::new("").expect("empty");

        // SAFETY: every nullable / "unused" struct field is filled with
        // zeroed memory of its declared type. The C side checks pointers
        // against NULL before dereferencing, and reads ints/floats as
        // their natural zero default (disabled / unused). The pointers
        // we DO populate (encoder/decoder/joiner/tokens/provider/
        // decoding/empty for required string slots) are owned by the
        // CStrings we stash in `_strings` for the recognizer's lifetime.
        let model_config = unsafe {
            sys::SherpaOnnxOnlineModelConfig {
                transducer: sys::SherpaOnnxOnlineTransducerModelConfig {
                    encoder: encoder.as_ptr(),
                    decoder: decoder.as_ptr(),
                    joiner: joiner.as_ptr(),
                },
                paraformer: mem::zeroed::<_>(),
                zipformer2_ctc: mem::zeroed::<_>(),
                nemo_ctc: mem::zeroed::<_>(),
                tokens: tokens.as_ptr(),
                num_threads: config.num_threads,
                provider: provider.as_ptr(),
                debug: i32::from(config.debug),
                // Leave model_type empty so sherpa-onnx auto-detects
                // from encoder ONNX metadata. Setting it explicitly
                // sends the C++ side down the wrong loader path.
                model_type: empty.as_ptr(),
                modeling_unit: empty.as_ptr(),
                bpe_vocab: empty.as_ptr(),
                tokens_buf: mem::zeroed::<_>(),
                tokens_buf_size: 0,
            }
        };

        // Endpoint detection off — utterances are bounded by the
        // session daemon's PTT release signal, not by trailing silence.
        let recognizer_config = unsafe {
            sys::SherpaOnnxOnlineRecognizerConfig {
                feat_config: sys::SherpaOnnxFeatureConfig {
                    sample_rate: config.sample_rate,
                    feature_dim: config.feature_dim,
                },
                model_config,
                decoding_method: decoding.as_ptr(),
                max_active_paths: 4,
                enable_endpoint: 0,
                rule1_min_trailing_silence: 0.0,
                rule2_min_trailing_silence: 0.0,
                rule3_min_utterance_length: 0.0,
                hotwords_file: empty.as_ptr(),
                hotwords_score: 0.0,
                ctc_fst_decoder_config: mem::zeroed::<_>(),
                rule_fsts: empty.as_ptr(),
                rule_fars: empty.as_ptr(),
                blank_penalty: 0.0,
                hotwords_buf: mem::zeroed::<_>(),
                hotwords_buf_size: 0,
                hr: mem::zeroed::<_>(),
            }
        };

        let recognizer = unsafe { sys::SherpaOnnxCreateOnlineRecognizer(&recognizer_config) };
        if recognizer.is_null() {
            return Err(BoothError::internal(
                "online recognizer create returned null (check sherpa-onnx stderr for ONNX init errors)",
            ));
        }

        Ok(Self {
            recognizer,
            name,
            _strings: vec![encoder, decoder, joiner, tokens, provider, decoding, empty],
        })
    }

    /// Transcribe a whole utterance buffer. Creates a per-call stream,
    /// feeds the audio in one shot, signals input-finished, drains
    /// pending decode steps, reads the result, destroys the stream.
    ///
    /// `audio` must be 16 kHz mono f32 PCM in [-1.0, 1.0]. The
    /// recognizer is `Send + Sync` (we serialize streams behind a
    /// `&self` borrow plus the engine's outer Mutex), but a given
    /// stream is single-threaded by construction — we create and
    /// destroy it within this call.
    pub fn transcribe(&self, sample_rate: u32, audio: &[f32]) -> Result<String> {
        // SAFETY: every pointer crossing the FFI here is either a
        // freshly-created stream (checked for null on creation) or the
        // recognizer pointer we keep valid for our entire lifetime.
        // The audio slice is borrowed for the duration of the
        // `AcceptWaveform` call only; sherpa-onnx copies samples into
        // its internal buffer.
        unsafe {
            let stream = sys::SherpaOnnxCreateOnlineStream(self.recognizer);
            if stream.is_null() {
                return Err(BoothError::internal(
                    "online stream create returned null",
                ));
            }

            sys::SherpaOnnxOnlineStreamAcceptWaveform(
                stream,
                sample_rate as i32,
                audio.as_ptr(),
                audio.len() as i32,
            );
            sys::SherpaOnnxOnlineStreamInputFinished(stream);

            // Drain. `IsOnlineStreamReady` returns non-zero while there
            // are more frames to consume from the cache; `Decode` runs
            // one chunk. For a finite utterance this terminates after
            // a small bounded number of iterations.
            while sys::SherpaOnnxIsOnlineStreamReady(self.recognizer, stream) != 0 {
                sys::SherpaOnnxDecodeOnlineStream(self.recognizer, stream);
            }

            let result_ptr = sys::SherpaOnnxGetOnlineStreamResult(self.recognizer, stream);
            let text = if !result_ptr.is_null() && !(*result_ptr).text.is_null() {
                CStr::from_ptr((*result_ptr).text as *const c_char)
                    .to_string_lossy()
                    .into_owned()
            } else {
                String::new()
            };
            if !result_ptr.is_null() {
                sys::SherpaOnnxDestroyOnlineRecognizerResult(result_ptr);
            }

            sys::SherpaOnnxDestroyOnlineStream(stream);
            Ok(text)
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

// The C++ side guards its internal state with its own mutex per
// recognizer. We additionally serialize streams behind the outer
// engine's Mutex (see `NemotronStreamingSttEngine`) so concurrent
// transcribe calls can't race — but the recognizer pointer itself is
// safe to share, hence Send + Sync.
unsafe impl Send for OnlineTransducerRecognizer {}
unsafe impl Sync for OnlineTransducerRecognizer {}

impl Drop for OnlineTransducerRecognizer {
    fn drop(&mut self) {
        // SAFETY: `recognizer` is the pointer we got back from
        // `SherpaOnnxCreateOnlineRecognizer` and have not handed out
        // ownership of; the destroy pairing matches the create.
        unsafe {
            sys::SherpaOnnxDestroyOnlineRecognizer(self.recognizer);
        }
    }
}

fn cstring(value: &str, field: &str) -> Result<CString> {
    CString::new(value).map_err(|e| BoothError::internal(format!("{field} path is not a valid C string ({e})")))
}
