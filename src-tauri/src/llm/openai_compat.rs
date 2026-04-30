//! `LlmCleanup` impl backed by an OpenAI-compatible HTTP API.
//!
//! Default target: Ollama on `http://localhost:11434/v1/chat/completions`
//! (Ollama's chat-completions endpoint is OpenAI-compatible). Also works
//! with `llama-server` (llama.cpp), LM Studio, vLLM, OpenRouter, and the
//! OpenAI/Anthropic/Groq APIs themselves — set the env vars to switch.
//!
//! Why HTTP instead of in-process: `whisper-rs-sys` and `llama-cpp-sys-2`
//! both statically link different versions of `ggml`, which produces
//! hundreds of duplicate-symbol linker errors on Windows. Running the LLM
//! out-of-process sidesteps that, and conveniently lets the user's existing
//! Ollama install do the heavy lifting (with GPU offload).
//!
//! ## Configuration (env vars)
//!
//! - `BOOTHRFLOW_LLM_ENDPOINT` — full URL to the chat-completions endpoint.
//!   Default: `http://localhost:11434/v1/chat/completions`
//! - `BOOTHRFLOW_LLM_MODEL` — model name as the endpoint expects it.
//!   Default: `qwen2.5:1.5b` (works with `ollama pull qwen2.5:1.5b`).
//! - `BOOTHRFLOW_LLM_API_KEY` — optional bearer token. Required for cloud
//!   providers (OpenAI, Anthropic, Groq); not needed for local Ollama.
//! - `BOOTHRFLOW_LLM_DISABLED=1` — skip LLM cleanup entirely; equivalent
//!   to setting Style → Raw at runtime.
//!
//! ## Latency
//!
//! Local Ollama with GPU offload on a 1.5B model: typically 100–400ms for
//! a ~50-token cleanup. Cloud BYOK adds ~50–150ms of network. The session
//! daemon falls back to the raw transcript if the request fails or times
//! out (5s), so a missing/down LLM degrades gracefully.

use std::time::{Duration, Instant};

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

use crate::error::{BoothError, Result};
use crate::llm::{CleanupRequest, LlmCleanup};
use crate::settings::Style;

pub const DEFAULT_ENDPOINT: &str = "http://localhost:11434/v1/chat/completions";
pub const DEFAULT_MODEL: &str = "qwen2.5:1.5b";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

pub struct OpenAiCompatLlmCleanup {
    endpoint: String,
    model: String,
    api_key: Option<String>,
    client: Client,
}

impl OpenAiCompatLlmCleanup {
    pub fn new(endpoint: String, model: String, api_key: Option<String>) -> Result<Self> {
        let client = Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|e| BoothError::Formatting(format!("reqwest client: {e}")))?;
        Ok(Self {
            endpoint,
            model,
            api_key,
            client,
        })
    }

    /// Build from `BOOTHRFLOW_LLM_*` env vars with sensible defaults.
    /// Returns `None` if `BOOTHRFLOW_LLM_DISABLED` is set.
    pub fn from_env() -> Option<Result<Self>> {
        if std::env::var("BOOTHRFLOW_LLM_DISABLED").is_ok() {
            return None;
        }
        let endpoint =
            std::env::var("BOOTHRFLOW_LLM_ENDPOINT").unwrap_or_else(|_| DEFAULT_ENDPOINT.into());
        let model = std::env::var("BOOTHRFLOW_LLM_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.into());
        let api_key = std::env::var("BOOTHRFLOW_LLM_API_KEY").ok();
        Some(Self::new(endpoint, model, api_key))
    }

    /// Build from on-disk settings if present, falling back to env vars and
    /// then defaults. Precedence (highest first): saved settings → env →
    /// `DEFAULT_*`. Returns `None` if the saved settings or env disable LLM.
    pub fn from_settings_or_env(saved: &crate::app_settings::LlmSettings) -> Option<Result<Self>> {
        if saved.disabled || std::env::var("BOOTHRFLOW_LLM_DISABLED").is_ok() {
            return None;
        }
        let endpoint = if !saved.endpoint.is_empty() {
            saved.endpoint.clone()
        } else {
            std::env::var("BOOTHRFLOW_LLM_ENDPOINT").unwrap_or_else(|_| DEFAULT_ENDPOINT.into())
        };
        let model = if !saved.model.is_empty() {
            saved.model.clone()
        } else {
            std::env::var("BOOTHRFLOW_LLM_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.into())
        };
        let api_key = if !saved.api_key.is_empty() {
            Some(saved.api_key.clone())
        } else {
            std::env::var("BOOTHRFLOW_LLM_API_KEY").ok()
        };
        Some(Self::new(endpoint, model, api_key))
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Send a tiny dummy request so the backend loads the model into VRAM
    /// before the user's first dictation. Failures are silently logged —
    /// the daemon will still try the real call when the user dictates.
    pub fn prewarm(&self) {
        let body = ChatRequest {
            model: &self.model,
            messages: [
                ChatMessage {
                    role: "system",
                    content: "respond with the word 'ok'",
                },
                ChatMessage {
                    role: "user",
                    content: "warmup",
                },
            ],
            temperature: 0.0,
            stream: false,
        };
        let started = Instant::now();
        let mut req = self.client.post(&self.endpoint).json(&body);
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }
        match req.send() {
            Ok(res) if res.status().is_success() => {
                tracing::info!(
                    "llm prewarm: {} ms (endpoint={})",
                    started.elapsed().as_millis(),
                    self.endpoint
                );
            }
            Ok(res) => tracing::warn!("llm prewarm: http {}", res.status()),
            Err(e) => tracing::warn!("llm prewarm failed: {e}"),
        }
    }
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: [ChatMessage<'a>; 2],
    temperature: f32,
    stream: bool,
}

#[derive(Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatMessageOwned,
}

#[derive(Deserialize)]
struct ChatMessageOwned {
    content: String,
}

impl LlmCleanup for OpenAiCompatLlmCleanup {
    fn cleanup(&self, request: CleanupRequest<'_>) -> Result<String> {
        let started = Instant::now();
        let system = build_system_prompt(&request.style);

        let body = ChatRequest {
            model: &self.model,
            messages: [
                ChatMessage {
                    role: "system",
                    content: &system,
                },
                ChatMessage {
                    role: "user",
                    content: request.raw_text,
                },
            ],
            temperature: 0.0,
            stream: false,
        };

        let mut req = self.client.post(&self.endpoint).json(&body);
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }

        let res = req
            .send()
            .map_err(|e| BoothError::Formatting(format!("llm http: {e}")))?;

        let status = res.status();
        if !status.is_success() {
            let body = res.text().unwrap_or_default();
            return Err(BoothError::Formatting(format!("llm http {status}: {body}")));
        }

        let parsed: ChatResponse = res
            .json()
            .map_err(|e| BoothError::Formatting(format!("llm json: {e}")))?;

        let text = parsed
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content.trim().to_string())
            .ok_or_else(|| BoothError::Formatting("llm: empty response".into()))?;

        tracing::info!(
            "llm http cleanup: {} ms (endpoint={}, model={})",
            started.elapsed().as_millis(),
            self.endpoint,
            self.model
        );
        Ok(text)
    }

    fn name(&self) -> &str {
        "openai-compat"
    }
}

fn build_system_prompt(style: &Style) -> String {
    let style_instr = match style {
        Style::Raw => "",
        Style::Formal => "\nStyle: formal — full sentences with proper punctuation, no slang, no contractions where avoidable.",
        Style::Casual => "\nStyle: casual — keep contractions, conversational tone.",
        Style::Excited => "\nStyle: excited — exclamation marks where natural, energetic tone.",
        Style::VeryCasual => "\nStyle: very casual — lowercase first letters, minimal punctuation.",
    };

    format!(
        "You are a post-processor for voice dictation. Your job is to add proper punctuation \
         and capitalization to a raw spoken transcript while preserving the speaker's words.\n\
         \n\
         Rules:\n\
         - Add periods, commas, question marks, exclamation marks where natural.\n\
         - Capitalize the first word of each sentence and proper nouns.\n\
         - Split run-on sentences into separate sentences.\n\
         - Do NOT change words, paraphrase, or remove filler words.\n\
         - Output ONLY the cleaned text. No preamble, no explanation, no quotes around the output.\
         {style_instr}"
    )
}
