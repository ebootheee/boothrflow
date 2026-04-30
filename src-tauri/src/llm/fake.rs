use std::time::Instant;

use crate::error::Result;
use crate::llm::{CleanupOutput, CleanupRequest, LlmCleanup};
use crate::settings::Style;

/// Deterministic LLM stand-in. Strips a fixed set of fillers, applies trivial
/// style-shaped formatting. NOT a real LLM — just enough to exercise the
/// pipeline + assert "the cleanup ran" in tests.
pub struct FakeLlmCleanup;

impl LlmCleanup for FakeLlmCleanup {
    fn cleanup(&self, req: CleanupRequest<'_>) -> Result<CleanupOutput> {
        let started = Instant::now();
        let cleaned = strip_fillers(req.raw_text);
        let text = apply_style(&cleaned, &req.style);
        Ok(CleanupOutput {
            text,
            // Fakes don't have real token counts; that's `openai_compat`'s
            // job. Leaving these `None` keeps the production tok/s display
            // honest about the lack of data.
            prompt_tokens: None,
            completion_tokens: None,
            elapsed_ms: started.elapsed().as_millis() as u64,
        })
    }

    fn name(&self) -> &str {
        "fake-llm"
    }
}

fn strip_fillers(input: &str) -> String {
    const MULTI_WORD: &[&str] = &["you know"];
    const SINGLE_WORD: &[&str] = &["uh", "um", "like", "basically"];

    // Multi-word fillers first — remove substring case-insensitively so that
    // "you know" is dropped before single-word filtering sees its tokens.
    let mut text = input.to_string();
    for filler in MULTI_WORD {
        while let Some(idx) = text.to_lowercase().find(filler) {
            text.replace_range(idx..idx + filler.len(), "");
        }
    }

    // Then single-word filtering, ignoring trailing punctuation.
    text.split_whitespace()
        .filter(|word| {
            let lower = word.to_lowercase();
            !SINGLE_WORD.contains(&lower.trim_end_matches(|c: char| !c.is_alphanumeric()))
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn apply_style(text: &str, style: &Style) -> String {
    if text.is_empty() {
        return String::new();
    }
    let mut chars = text.chars();
    let first = chars.next().unwrap();
    let rest: String = chars.collect();
    match style {
        Style::Raw => text.to_string(),
        Style::Formal => format!("{}{}.", first.to_uppercase(), rest),
        Style::Casual => format!("{}{}", first.to_lowercase(), rest),
        Style::Excited => format!("{}{}!", first.to_uppercase(), rest),
        Style::VeryCasual => text.to_lowercase(),
        Style::CaptainsLog => format!(
            "Captain's log, stardate {}. {}{}.  End log.",
            crate::llm::stardate_label(),
            first.to_uppercase(),
            rest
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::CleanupRequest;

    #[test]
    fn strips_fillers() {
        let llm = FakeLlmCleanup;
        let out = llm
            .cleanup(CleanupRequest {
                raw_text: "uh so basically this works, you know, like always",
                style: Style::Raw,
                app_context: None,
            })
            .unwrap();
        assert!(!out.text.contains("uh"));
        assert!(!out.text.contains("basically"));
        assert!(!out.text.contains("you"));
        assert!(out.text.contains("this"));
        // Fakes don't report token counts.
        assert!(out.prompt_tokens.is_none());
        assert!(out.completion_tokens.is_none());
    }

    #[test]
    fn formal_style_ends_with_period() {
        let llm = FakeLlmCleanup;
        let out = llm
            .cleanup(CleanupRequest {
                raw_text: "ship it",
                style: Style::Formal,
                app_context: None,
            })
            .unwrap();
        assert!(out.text.ends_with('.'));
        assert!(out.text.starts_with("S"));
    }

    #[test]
    fn captains_log_prepends_stardate() {
        let llm = FakeLlmCleanup;
        let out = llm
            .cleanup(CleanupRequest {
                raw_text: "we're approaching the asteroid field",
                style: Style::CaptainsLog,
                app_context: None,
            })
            .unwrap();
        assert!(out.text.starts_with("Captain's log, stardate "));
        assert!(out.text.contains("End log."));
    }
}
