//! LLM cleanup pass — turns raw STT into clean written text in a chosen style.

use crate::error::Result;
use crate::settings::Style;

#[derive(Debug, Clone)]
pub struct CleanupRequest<'a> {
    pub raw_text: &'a str,
    pub style: Style,
    pub app_context: Option<&'a str>,
}

pub trait LlmCleanup: Send + Sync {
    /// Rewrite `raw_text` per the request's style and context. Returns the
    /// formatted text. Synchronous from the caller's perspective; impls may
    /// block on local llama.cpp inference internally.
    fn cleanup(&self, request: CleanupRequest<'_>) -> Result<String>;

    fn name(&self) -> &str;
}

#[cfg(any(test, feature = "test-fakes"))]
pub mod fake;
#[cfg(any(test, feature = "test-fakes"))]
pub use fake::FakeLlmCleanup;

/// Convenience: when raw text is short or the user has LLM disabled, skip the
/// pass entirely and pass through the raw transcript.
pub fn should_skip_llm(raw: &str, llm_enabled: bool) -> bool {
    if !llm_enabled {
        return true;
    }
    let word_count = raw.split_whitespace().count();
    word_count < 6
}
