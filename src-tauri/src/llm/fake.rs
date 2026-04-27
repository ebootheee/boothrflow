use crate::error::Result;
use crate::llm::{CleanupRequest, LlmCleanup};
use crate::settings::Style;

/// Deterministic LLM stand-in. Strips a fixed set of fillers, applies trivial
/// style-shaped formatting. NOT a real LLM — just enough to exercise the
/// pipeline + assert "the cleanup ran" in tests.
pub struct FakeLlmCleanup;

impl LlmCleanup for FakeLlmCleanup {
    fn cleanup(&self, req: CleanupRequest<'_>) -> Result<String> {
        let cleaned = strip_fillers(req.raw_text);
        Ok(apply_style(&cleaned, &req.style))
    }

    fn name(&self) -> &str {
        "fake-llm"
    }
}

fn strip_fillers(input: &str) -> String {
    const FILLERS: &[&str] = &["uh", "um", "like", "basically", "you know"];
    input
        .split_whitespace()
        .filter(|word| {
            let lower = word.to_lowercase();
            !FILLERS.contains(&lower.trim_end_matches(|c: char| !c.is_alphanumeric()))
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
        assert!(!out.contains("uh"));
        assert!(!out.contains("basically"));
        assert!(!out.contains("you"));
        assert!(out.contains("this"));
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
        assert!(out.ends_with('.'));
        assert!(out.starts_with("S"));
    }
}
