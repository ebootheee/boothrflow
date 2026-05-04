//! LLM cleanup pass — turns raw STT into clean written text in a chosen style.

use crate::context::AppContext;
use crate::error::Result;
use crate::settings::{MisheardReplacement, Style};

#[derive(Debug, Clone, Default)]
pub struct CleanupRequest<'a> {
    pub raw_text: &'a str,
    pub style: Style,
    /// Foreground app + window at the moment the user finished dictating.
    /// `None` when context detection isn't supported on this platform or
    /// the detector failed (always graceful — cleanup runs regardless).
    pub app_context: Option<AppContext>,
    /// Best-effort OCR of the focused window's contents. Wave 5 feature;
    /// off by default, opt-in via Settings + `Privacy mode`. The cleanup
    /// prompt's `<OCR-RULES>` block tells the LLM to use this only as
    /// supporting context — preserves spoken words when there's no
    /// recognition miss to correct.
    pub window_ocr: Option<String>,
    /// User's curated vocabulary list (proper nouns, jargon). Plumbed
    /// into the prompt as `<USER-CORRECTIONS>` "treat these spellings
    /// as authoritative". Auto-populated by the post-paste learning
    /// coordinator; manually editable in Settings.
    pub preferred_transcriptions: Vec<String>,
    /// Wrong → right pairs the user (or auto-learning) recorded. Same
    /// `<USER-CORRECTIONS>` block, with explicit substitution rules.
    pub commonly_misheard: Vec<MisheardReplacement>,
}

/// Result of a cleanup pass, carrying both the rewritten text and timing /
/// token-count telemetry. The token counts are `Option` because not every
/// backend reports them — `openai_compat` fills them in from Ollama's
/// `usage` field; `fake` leaves them `None`. Callers (the session daemon)
/// derive `tok/s` from `completion_tokens / (elapsed_ms / 1000)`.
#[derive(Debug, Clone, Default)]
pub struct CleanupOutput {
    pub text: String,
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
    pub elapsed_ms: u64,
}

impl CleanupOutput {
    /// Convenience: tokens-per-second over the cleanup elapsed time. Returns
    /// `None` if the backend didn't report token counts or elapsed_ms is 0.
    pub fn tokens_per_second(&self) -> Option<f32> {
        let completion = self.completion_tokens? as f32;
        if self.elapsed_ms == 0 {
            return None;
        }
        Some(completion / (self.elapsed_ms as f32 / 1000.0))
    }
}

pub trait LlmCleanup: Send + Sync {
    /// Rewrite `raw_text` per the request's style and context. Returns the
    /// formatted text plus telemetry. Synchronous from the caller's
    /// perspective; impls may block on local llama.cpp inference or HTTP I/O.
    fn cleanup(&self, request: CleanupRequest<'_>) -> Result<CleanupOutput>;

    fn name(&self) -> &str;
}

pub mod prompt;

#[cfg(any(test, feature = "test-fakes"))]
pub mod fake;
#[cfg(any(test, feature = "test-fakes"))]
pub use fake::FakeLlmCleanup;

#[cfg(feature = "real-engines")]
pub mod openai_compat;
#[cfg(feature = "real-engines")]
pub use openai_compat::{OpenAiCompatLlmCleanup, DEFAULT_ENDPOINT, DEFAULT_MODEL};

// In-process llama via llama-cpp-2 conflicts with whisper-rs-sys (both
// statically link different ggml versions). We use the OpenAI-compatible
// HTTP API instead — works with Ollama, llama-server, LM Studio, vLLM,
// or any cloud BYOK provider.

/// Convenience: when raw text is short or the user has LLM disabled, skip the
/// pass entirely and pass through the raw transcript.
pub fn should_skip_llm(raw: &str, llm_enabled: bool) -> bool {
    if !llm_enabled {
        return true;
    }
    let word_count = raw.split_whitespace().count();
    word_count < 6
}

/// TNG-era stardate approximation rendered to one decimal. Formula:
/// `1000 × (year − 2323) + (day_of_year × 1000 / 365.25)`. Dates in our
/// timeline (pre-2323) yield a negative value; we absolute-value it and
/// render so it reads like a future entry. Used by Style::CaptainsLog.
///
/// Intentionally a simple, deterministic formula rather than the canon
/// (which canonically drifted across TNG/DS9/Voyager in inconsistent
/// ways). Doesn't have to be franchise-accurate — just has to feel like
/// Star Trek.
pub fn stardate_label() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    // Days since 1970-01-01 UTC, then back into year + day-of-year.
    // Cheap implementation that avoids pulling chrono into the non-real
    // path. We're computing a stardate, not flying a ship.
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as f64)
        .unwrap_or(0.0);
    let total_days = secs / 86_400.0;

    // Approximate Gregorian year + day-of-year. Off by a fraction of a
    // day in some leap-year edge cases — fine for a stardate.
    let years = total_days / 365.2425;
    let year = 1970.0 + years.floor();
    let day_of_year = (total_days - (years.floor() * 365.2425)).max(0.0);

    let raw = 1000.0 * (year - 2323.0) + (day_of_year * 1000.0 / 365.25);
    let absolute = raw.abs();
    format!("{absolute:.1}")
}
