//! Real `LlmCleanup` impl backed by [`llama_cpp_2`] (Rust wrapper around llama.cpp).
//!
//! Default model: Qwen 2.5 1.5B Instruct Q4_K_M (~1GB) — Apache 2.0,
//! fast on CPU, polished output for short rewrite tasks. Production should
//! offer a settings picker for 0.5B (faster) and 3B (higher quality).
//!
//! Lifecycle:
//! - `LlamaBackend` is process-global (only one allowed); cached in a `OnceLock`.
//! - `LlamaModel` is loaded once at engine construction (~1s).
//! - A fresh `LlamaContext` is created for each `cleanup()` call. Context
//!   creation is cheap (~ms); generation is the hot path. Avoids self-
//!   referential lifetimes.

use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Instant;

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;
use parking_lot::Mutex;

use crate::error::{BoothError, Result};
use crate::llm::{CleanupRequest, LlmCleanup};
use crate::settings::Style;

/// Default model file name. The user puts the GGUF here:
/// `%APPDATA%\boothrflow\models\qwen2.5-1.5b-instruct-q4_k_m.gguf`
pub const DEFAULT_MODEL_FILE: &str = "qwen2.5-1.5b-instruct-q4_k_m.gguf";

/// Where users get the file when it's missing.
pub const DEFAULT_MODEL_URL: &str = "https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF/resolve/main/qwen2.5-1.5b-instruct-q4_k_m.gguf";

/// Cap on output tokens. The cleanup task should rarely produce more than
/// ~1.2× the input length, so for typical 50-100 word transcripts this is
/// generous.
const MAX_OUTPUT_TOKENS: usize = 256;

/// Context window. 2048 tokens is plenty for our prompt (~200) + transcript
/// (~200) + output (~256). Larger windows blow KV-cache memory for no gain.
const N_CTX: u32 = 2048;

fn backend() -> Result<&'static LlamaBackend> {
    static BACKEND: OnceLock<std::result::Result<LlamaBackend, String>> = OnceLock::new();
    let init = BACKEND.get_or_init(|| {
        // Silence llama.cpp's chatty stdout/stderr logging once at init.
        // The helper exists in some versions; ignore if not present.
        LlamaBackend::init().map_err(|e| format!("backend init: {e}"))
    });
    match init {
        Ok(b) => Ok(b),
        Err(msg) => Err(BoothError::Formatting(msg.clone())),
    }
}

pub struct LlamaCppLlmCleanup {
    model: Mutex<LlamaModel>,
    name: String,
}

impl LlamaCppLlmCleanup {
    pub fn from_path(model_path: &Path, name: impl Into<String>) -> Result<Self> {
        if !model_path.exists() {
            return Err(missing_model_error(model_path));
        }
        let backend = backend()?;
        let params = LlamaModelParams::default();
        let model = LlamaModel::load_from_file(backend, model_path, &params)
            .map_err(|e| BoothError::Formatting(format!("load llama model: {e}")))?;
        Ok(Self {
            model: Mutex::new(model),
            name: name.into(),
        })
    }

    pub fn from_default_location() -> Result<Self> {
        let path = default_model_path()
            .ok_or_else(|| BoothError::Formatting("could not resolve user data dir".into()))?;
        Self::from_path(&path, "qwen-2.5-1.5b-q4-k-m")
    }
}

impl LlmCleanup for LlamaCppLlmCleanup {
    fn cleanup(&self, request: CleanupRequest<'_>) -> Result<String> {
        let backend = backend()?;
        let model = self.model.lock();
        let prompt = build_prompt(&request);
        run_inference(backend, &model, &prompt)
    }

    fn name(&self) -> &str {
        &self.name
    }
}

fn build_prompt(req: &CleanupRequest<'_>) -> String {
    let style_instr = style_instructions(&req.style);
    format!(
        "<|im_start|>system\n\
         You are a post-processor for voice dictation. Your job is to add proper punctuation \
         and capitalization to a raw spoken transcript while preserving the speaker's words.\n\
         \n\
         Rules:\n\
         - Add periods, commas, question marks, exclamation marks where natural.\n\
         - Capitalize the first word of each sentence and proper nouns.\n\
         - Split run-on sentences into separate sentences.\n\
         - Do NOT change words, paraphrase, or remove filler words.\n\
         - Output ONLY the cleaned text. No preamble, no explanation, no quotes around the output.\n\
         {style_instr}<|im_end|>\n\
         <|im_start|>user\n\
         {raw}<|im_end|>\n\
         <|im_start|>assistant\n",
        raw = req.raw_text
    )
}

fn style_instructions(style: &Style) -> &'static str {
    match style {
        Style::Raw => "",
        Style::Formal => "\nStyle: formal — full sentences with proper punctuation, no slang, no contractions where avoidable.\n",
        Style::Casual => "\nStyle: casual — keep contractions, conversational tone.\n",
        Style::Excited => "\nStyle: excited — exclamation marks where natural, energetic tone.\n",
        Style::VeryCasual => "\nStyle: very casual — lowercase first letters, minimal punctuation, contractions everywhere.\n",
    }
}

fn run_inference(backend: &LlamaBackend, model: &LlamaModel, prompt: &str) -> Result<String> {
    let started = Instant::now();

    let ctx_params = LlamaContextParams::default().with_n_ctx(NonZeroU32::new(N_CTX));

    let mut ctx = model
        .new_context(backend, ctx_params)
        .map_err(|e| BoothError::Formatting(format!("new context: {e}")))?;

    let tokens_in = model
        .str_to_token(prompt, AddBos::Always)
        .map_err(|e| BoothError::Formatting(format!("tokenize: {e}")))?;

    let n_prompt = tokens_in.len();
    if n_prompt as u32 > N_CTX - MAX_OUTPUT_TOKENS as u32 {
        return Err(BoothError::Formatting(format!(
            "prompt too long: {} tokens vs context {}",
            n_prompt, N_CTX
        )));
    }

    // Prefill: feed every prompt token, mark the last one for logit output.
    let mut batch = LlamaBatch::new(n_prompt.max(512), 1);
    for (i, token) in tokens_in.iter().enumerate() {
        let is_last = i == n_prompt - 1;
        batch
            .add(*token, i as i32, &[0], is_last)
            .map_err(|e| BoothError::Formatting(format!("batch.add prefill: {e}")))?;
    }
    ctx.decode(&mut batch)
        .map_err(|e| BoothError::Formatting(format!("decode prefill: {e}")))?;

    let mut sampler = LlamaSampler::greedy();
    let mut output_bytes: Vec<u8> = Vec::new();
    let mut n_cur = n_prompt as i32;
    let mut output_tokens: usize = 0;

    while output_tokens < MAX_OUTPUT_TOKENS {
        // Sample the most-likely next token from the logits at position n_cur-1.
        let new_token = sampler.sample(&ctx, batch.n_tokens() - 1);
        sampler.accept(new_token);

        if model.is_eog_token(new_token) {
            break;
        }

        // Render token to bytes (model output may not be valid UTF-8 mid-
        // token; we accumulate bytes and decode lossily at the end).
        // `special = true` so we still see EOS markers if the eog check
        // misses an edge variant; non-text artefacts get filtered by
        // String::from_utf8_lossy on the way out.
        match model.token_to_piece_bytes(new_token, 32, true, None) {
            Ok(bytes) => output_bytes.extend_from_slice(&bytes),
            Err(e) => tracing::debug!("llm: token_to_piece_bytes failed: {e}"),
        }

        // Feed the new token back as the next prefix.
        batch.clear();
        batch
            .add(new_token, n_cur, &[0], true)
            .map_err(|e| BoothError::Formatting(format!("batch.add gen: {e}")))?;
        ctx.decode(&mut batch)
            .map_err(|e| BoothError::Formatting(format!("decode gen: {e}")))?;

        n_cur += 1;
        output_tokens += 1;
    }

    let output = String::from_utf8_lossy(&output_bytes).trim().to_string();
    let elapsed_ms = started.elapsed().as_millis();
    tracing::info!(
        "llm: prompt={} tok, output={} tok, {} ms",
        n_prompt,
        output_tokens,
        elapsed_ms
    );

    Ok(output)
}

pub fn default_model_path() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("boothrflow").join("models").join(DEFAULT_MODEL_FILE))
}

fn missing_model_error(path: &Path) -> BoothError {
    BoothError::Formatting(format!(
        "Qwen LLM model not found at {}.\n\
         Download it once with:\n\
           pnpm download:llm\n\
         (fetches qwen2.5-1.5b-instruct-q4_k_m.gguf from HuggingFace, ~1GB)",
        path.display()
    ))
}
