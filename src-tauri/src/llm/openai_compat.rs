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
//!   Default: `qwen2.5:7b` (works with `ollama pull qwen2.5:7b`). Set to
//!   `qwen2.5:1.5b` to fall back to the smaller / faster model on slower
//!   machines; that's the path until the in-app Settings panel ships.
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
use crate::llm::prompt::{build_system_prompt, CleanupPromptInputs};
use crate::llm::{CleanupOutput, CleanupRequest, LlmCleanup};
use crate::settings::LlmSettings;

pub const DEFAULT_ENDPOINT: &str = "http://localhost:11434/v1/chat/completions";
pub const DEFAULT_MODEL: &str = "qwen2.5:7b";
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

    pub fn from_settings(settings: &LlmSettings) -> Option<Result<Self>> {
        if !settings.enabled {
            return None;
        }
        let api_key = settings
            .api_key
            .as_ref()
            .filter(|key| !key.trim().is_empty())
            .cloned();
        Some(Self::new(
            settings.endpoint.clone(),
            settings.model.clone(),
            api_key,
        ))
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Heuristic: is the configured endpoint Ollama? Used to gate the
    /// `keep_alive` extra field — Ollama uses it to keep weights resident
    /// in VRAM across requests; cloud BYOK endpoints (OpenAI, Anthropic,
    /// Groq) generally ignore unknown fields but some strict gateways
    /// (LM Studio in particular) return 400 on unknown keys, so we only
    /// send `keep_alive` to endpoints that are clearly Ollama.
    ///
    /// Detection is conservative on purpose: we'd rather miss the
    /// VRAM-warming optimization for a non-default Ollama install than
    /// break LM Studio / llama-server / vLLM users with an unknown-field
    /// rejection. A future settings toggle can let advanced users opt
    /// in explicitly.
    fn looks_like_ollama(&self) -> bool {
        let lower = self.endpoint.to_lowercase();
        // Default Ollama port — covers the 99% case and our shipped
        // default endpoint. Bare `localhost` is too broad (matches LM
        // Studio :1234, llama-server :8080) so we don't include it.
        lower.contains(":11434")
    }

    fn keep_alive_for_endpoint(&self) -> &'static str {
        if self.looks_like_ollama() { "5m" } else { "" }
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
            keep_alive: self.keep_alive_for_endpoint(),
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
    /// Ollama-specific extra: how long to keep the model resident in
    /// VRAM after the request completes. The OpenAI-compat layer
    /// ignores fields it doesn't recognize, so this is harmless on
    /// non-Ollama backends. 5 minutes covers typical inter-dictation
    /// gaps so the KV cache + model weights stay warm — this is the
    /// "prompt prefix caching" win because Ollama can re-use the KV
    /// state for the stable system-prompt prefix across consecutive
    /// dictations within the keep_alive window.
    #[serde(skip_serializing_if = "str::is_empty")]
    keep_alive: &'a str,
}

#[derive(Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
    /// Ollama (and OpenAI itself) returns `usage` with prompt + completion
    /// token counts. Optional because some compat backends omit it on
    /// non-streaming responses; we'd rather log "unknown tok/s" than fail.
    #[serde(default)]
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatMessageOwned,
}

#[derive(Deserialize)]
struct ChatMessageOwned {
    content: String,
}

#[derive(Deserialize)]
struct Usage {
    #[serde(default)]
    prompt_tokens: Option<u32>,
    #[serde(default)]
    completion_tokens: Option<u32>,
}

impl LlmCleanup for OpenAiCompatLlmCleanup {
    fn cleanup(&self, request: CleanupRequest<'_>) -> Result<CleanupOutput> {
        let started = Instant::now();
        let inputs = CleanupPromptInputs {
            style: request.style,
            app_context: request.app_context.as_ref(),
            window_ocr: request.window_ocr.as_deref(),
            preferred_transcriptions: &request.preferred_transcriptions,
            commonly_misheard: &request.commonly_misheard,
        };
        let system = build_system_prompt(&inputs);

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
            keep_alive: self.keep_alive_for_endpoint(),
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

        let usage = parsed.usage;
        let text = parsed
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content.trim().to_string())
            .ok_or_else(|| BoothError::Formatting("llm: empty response".into()))?;

        let elapsed_ms = started.elapsed().as_millis() as u64;
        let prompt_tokens = usage.as_ref().and_then(|u| u.prompt_tokens);
        let completion_tokens = usage.as_ref().and_then(|u| u.completion_tokens);

        // Log tok/s when Ollama returned usage data — useful for tuning the
        // model picker. Skipped silently if usage is absent.
        if let (Some(pt), Some(ct)) = (prompt_tokens, completion_tokens) {
            let tps = if elapsed_ms > 0 {
                ct as f32 / (elapsed_ms as f32 / 1000.0)
            } else {
                0.0
            };
            tracing::info!(
                "llm http cleanup: {elapsed_ms} ms ({pt} prompt + {ct} completion = {} tok/s, endpoint={}, model={})",
                format!("{tps:.1}"),
                self.endpoint,
                self.model
            );
        } else {
            tracing::info!(
                "llm http cleanup: {elapsed_ms} ms (endpoint={}, model={})",
                self.endpoint,
                self.model
            );
        }

        Ok(CleanupOutput {
            text,
            prompt_tokens,
            completion_tokens,
            elapsed_ms,
        })
    }

    fn name(&self) -> &str {
        "openai-compat"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client(endpoint: &str) -> OpenAiCompatLlmCleanup {
        OpenAiCompatLlmCleanup::new(endpoint.into(), "test-model".into(), None).unwrap()
    }

    #[test]
    fn ollama_default_endpoint_gets_keep_alive() {
        let c = client(DEFAULT_ENDPOINT);
        assert!(c.looks_like_ollama());
        assert_eq!(c.keep_alive_for_endpoint(), "5m");
    }

    #[test]
    fn lm_studio_default_port_does_not_get_keep_alive() {
        // LM Studio runs on localhost:1234 by default and rejects the
        // `keep_alive` extra in some versions. The heuristic narrows
        // to port 11434 specifically so LM Studio users aren't broken.
        let c = client("http://localhost:1234/v1/chat/completions");
        assert!(!c.looks_like_ollama());
        assert_eq!(c.keep_alive_for_endpoint(), "");
    }

    #[test]
    fn llama_server_default_port_does_not_get_keep_alive() {
        // llama-server (llama.cpp) defaults to localhost:8080.
        let c = client("http://localhost:8080/v1/chat/completions");
        assert!(!c.looks_like_ollama());
    }

    #[test]
    fn ollama_alt_port_misses_optimization_acceptably() {
        // Trade-off: a user running Ollama on a non-default port misses
        // the VRAM-warming bonus. Worth it to avoid breaking LM Studio.
        let c = client("http://localhost:9999/v1/chat/completions");
        assert!(!c.looks_like_ollama());
    }

    #[test]
    fn openai_endpoint_skips_keep_alive() {
        let c = client("https://api.openai.com/v1/chat/completions");
        assert!(!c.looks_like_ollama());
        assert_eq!(c.keep_alive_for_endpoint(), "");
    }

    #[test]
    fn anthropic_endpoint_skips_keep_alive() {
        let c = client("https://api.anthropic.com/v1/messages");
        assert!(!c.looks_like_ollama());
    }
}
