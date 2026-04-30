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
use crate::llm::{stardate_label, CleanupOutput, CleanupRequest, LlmCleanup};
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

fn build_system_prompt(style: &Style) -> String {
    // Captain's Log gets its own bespoke prompt rather than retrofitting the
    // generic template — the canon-style rewrite is structurally different
    // (preserves meaning, transforms tone heavily) so a single set of rules
    // doesn't fit cleanly. We still constrain it tightly to avoid invented
    // ship names / characters / numeric stardate prefixes.
    if matches!(style, Style::CaptainsLog) {
        let stardate = stardate_label();
        return format!(
            "You are a post-processor for voice dictation, rewriting the speaker's words as a \
             Star-Trek-style Captain's Log entry.\n\
             \n\
             Rules:\n\
             - BEGIN your output with exactly this sentence: \"Captain's log, stardate {stardate}.\"\n\
             - END your output with exactly this sentence: \"End log.\"\n\
             - Between those, rewrite the speaker's content in formal, slightly archaic 24th-century \
               space-faring tone. Phrases like \"set course for\", \"we have detected\", \"the crew is \
               investigating\", \"long-range sensors indicate\", \"I have ordered\" are encouraged where \
               they fit.\n\
             - DO preserve all factual content the speaker said. The log should describe what they \
               actually said, not invent a sci-fi adventure.\n\
             - DO NOT invent ship names, crew names, characters from canon (Picard, Spock, Enterprise, \
               Federation, etc.), or any specific numeric details that weren't in the input.\n\
             - DO NOT add a stardate prefix anywhere except the opening sentence specified above.\n\
             - Drop disfluencies (\"uh\", \"um\", \"you know\") and false starts. Keep the meaning.\n\
             - Output ONLY the log entry. No preamble, no quotes around the output."
        );
    }

    let aggressiveness = style.aggressiveness();
    let aggressiveness_instr = match aggressiveness {
        0 => "Preserve every word the speaker said exactly. Do not drop fillers, do not paraphrase.",
        1 => "Drop disfluencies (\"uh\", \"um\", \"you know\", \"I mean\", \"like\" used as filler), false starts, and self-corrections — when the speaker says \"go to the store, I mean the office\", output \"go to the office\". Do not paraphrase or shorten otherwise. Keep all substantive content.",
        _ => "Drop disfluencies, false starts, and self-corrections. Light paraphrasing is allowed where it preserves the speaker's meaning and intent. Do not invent or add information.",
    };

    let style_instr = match style {
        Style::Raw => "",
        Style::Formal => "\nStyle: formal — full sentences with proper punctuation, no slang, no contractions where avoidable.",
        Style::Casual => "\nStyle: casual — keep contractions, conversational tone.",
        Style::Excited => "\nStyle: excited — exclamation marks where natural, energetic tone.",
        Style::VeryCasual => "\nStyle: very casual — lowercase first letters, minimal punctuation.",
        // Captain's Log handled in the early-return above.
        Style::CaptainsLog => unreachable!(),
    };

    format!(
        "You are a post-processor for voice dictation. Your job is to add proper punctuation \
         and capitalization to a raw spoken transcript and reshape it per the rules below.\n\
         \n\
         Rules:\n\
         - Add periods, commas, question marks, exclamation marks where natural.\n\
         - Capitalize the first word of each sentence and proper nouns.\n\
         - Split run-on sentences into separate sentences.\n\
         - {aggressiveness_instr}\n\
         - If a transcribed word is acoustically plausible but semantically nonsensical given the surrounding context, replace it with the most likely intended word. Do not over-correct content that simply seems unusual.\n\
         - Output ONLY the cleaned text. No preamble, no explanation, no quotes around the output.\
         {style_instr}"
    )
}
