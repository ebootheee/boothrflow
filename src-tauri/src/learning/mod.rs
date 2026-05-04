//! Post-paste correction learning.
//!
//! After we paste into the user's focused text field, sample its
//! contents a few seconds later. If a small single-word edit is
//! detected (Levenshtein ≤ 3, single token swap), record
//! `(original_word, edited_word)` as a `MisheardReplacement` on the
//! user's settings so the cleanup prompt's `<USER-CORRECTIONS>` block
//! starts applying it on subsequent dictations.
//!
//! The structure here is deliberately split:
//!
//! - `detect_correction` is a pure function over `(pasted, current)` —
//!   fully unit-testable, no I/O, no platform code.
//! - `FocusedTextReader` is the trait that the platform-specific AX /
//!   UIAutomation implementations satisfy. Stubs return `None`, which
//!   the coordinator silently treats as "user didn't edit" — same as
//!   the OCR path's graceful fallback.
//! - `LearningCoordinator` glues them together and runs in a
//!   background thread per dictation.
//!
//! Opt-in: `auto_learn_corrections: bool` on `AppSettings`. Off by
//! default — auto-edits a settings field, which is the kind of thing
//! that needs explicit consent.

use crate::settings::MisheardReplacement;

#[cfg(any(test, feature = "test-fakes"))]
pub mod fake;

pub mod coordinator;
pub use coordinator::{LearningCoordinator, PasteSnapshot};

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "macos")]
pub use macos::MacosFocusedTextReader;

/// Trait satisfied by platform-specific accessibility readers. Returns
/// `None` when the focused element doesn't expose its text value (web
/// browsers, some Electron apps, screen-locked sessions). Coordinator
/// treats `None` as "no correction available — drop this paste".
pub trait FocusedTextReader: Send + Sync {
    fn read_focused_text(&self) -> Option<String>;

    /// Identifier for diagnostics ("macos-ax", "stub", …).
    fn name(&self) -> &str;
}

/// Maximum allowed Levenshtein distance for a single-word edit to
/// count as a recognition correction. 3 is a conservative cap —
/// catches typos like `kwen` → `qwen` and `paython` → `python`,
/// rejects whole-word replacements that are usually intentional
/// rewrites rather than recognition fixes.
pub const MAX_EDIT_DISTANCE: usize = 3;

/// Maximum word length for either side of a candidate edit. Long
/// strings are usually URLs, file paths, or model IDs that the user
/// typed by hand — not recognition fixes.
const MAX_WORD_LEN: usize = 32;

/// Minimum word length for either side of a candidate edit. One- and
/// two-letter edits are dominated by punctuation and articles
/// ("a"/"the") which produce noise rather than signal.
const MIN_WORD_LEN: usize = 3;

/// Detect a likely single-word correction between the pasted text and
/// the current contents of the focused field. Returns `None` when the
/// edit doesn't look like a recognition correction (multi-word edit,
/// distance too high, words too short, identical text, etc.).
///
/// Heuristic intentionally aggressive on the rejection side: we'd
/// rather miss real corrections than persist a noisy rule that the
/// user has to manually delete from the corrections list.
pub fn detect_correction(pasted: &str, current: &str) -> Option<MisheardReplacement> {
    let pasted_trimmed = pasted.trim();
    let current_trimmed = current.trim();

    if pasted_trimmed.is_empty() || current_trimmed.is_empty() {
        return None;
    }
    if pasted_trimmed == current_trimmed {
        return None;
    }

    // Tokenize both sides. The user may have typed before / after the
    // pasted region; locate the edited tokens by stripping the longest
    // common prefix + suffix in *whole tokens* (not characters — that
    // would peel word interiors and hide the real edit, e.g. "kwen."
    // vs "qwen." would shrink to "k" vs "q" and fail the length gate).
    let pasted_words: Vec<&str> = pasted_trimmed.split_whitespace().collect();
    let current_words: Vec<&str> = current_trimmed.split_whitespace().collect();

    let prefix = pasted_words
        .iter()
        .zip(current_words.iter())
        .take_while(|(a, b)| a == b)
        .count();
    let suffix = pasted_words
        .iter()
        .rev()
        .zip(current_words.iter().rev())
        .take_while(|(a, b)| a == b)
        .count();

    // After stripping common prefix + suffix tokens, we should be left
    // with a single replaced token on each side. Anything else (multi-
    // word edit, deletion, insertion) is treated as a rewrite, not a
    // recognition correction.
    let p_mid_end = pasted_words.len().saturating_sub(suffix);
    let c_mid_end = current_words.len().saturating_sub(suffix);
    if prefix > p_mid_end || prefix > c_mid_end {
        return None;
    }
    let p_mid = &pasted_words[prefix..p_mid_end];
    let c_mid = &current_words[prefix..c_mid_end];

    if p_mid.len() != 1 || c_mid.len() != 1 {
        return None;
    }

    // Strip surrounding punctuation but keep the inner form. "Qwen,"
    // and "Qwen" should be treated as the same correction target.
    let wrong = strip_outer_punct(p_mid[0]);
    let right = strip_outer_punct(c_mid[0]);

    if wrong.is_empty() || right.is_empty() || wrong == right {
        return None;
    }
    if wrong.len() < MIN_WORD_LEN || right.len() < MIN_WORD_LEN {
        return None;
    }
    if wrong.len() > MAX_WORD_LEN || right.len() > MAX_WORD_LEN {
        return None;
    }

    let dist = levenshtein(wrong, right);
    if dist == 0 || dist > MAX_EDIT_DISTANCE {
        return None;
    }

    // Reject capitalization-only edits — the LLM cleanup should already
    // handle these via the style + the preferred-spellings vocab.
    if wrong.eq_ignore_ascii_case(right) {
        return None;
    }

    Some(MisheardReplacement::new(wrong, right))
}

/// Strip leading/trailing punctuation from a word. Keeps internal
/// punctuation (apostrophes, hyphens) intact.
fn strip_outer_punct(word: &str) -> &str {
    word.trim_matches(|c: char| !c.is_alphanumeric() && c != '\'' && c != '-')
}

/// Classic dynamic-programming Levenshtein. O(n*m) over chars, fine
/// for the short words we operate on (capped at `MAX_WORD_LEN`).
fn levenshtein(a: &str, b: &str) -> usize {
    if a == b {
        return 0;
    }
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let n = a_chars.len();
    let m = b_chars.len();
    if n == 0 {
        return m;
    }
    if m == 0 {
        return n;
    }

    let mut prev_row: Vec<usize> = (0..=m).collect();
    let mut curr_row: Vec<usize> = vec![0; m + 1];

    for i in 1..=n {
        curr_row[0] = i;
        for j in 1..=m {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };
            curr_row[j] = (curr_row[j - 1] + 1)
                .min(prev_row[j] + 1)
                .min(prev_row[j - 1] + cost);
        }
        std::mem::swap(&mut prev_row, &mut curr_row);
    }
    prev_row[m]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levenshtein_basics() {
        assert_eq!(levenshtein("", ""), 0);
        assert_eq!(levenshtein("kitten", "sitting"), 3);
        assert_eq!(levenshtein("kwen", "qwen"), 1);
        assert_eq!(levenshtein("python", "paython"), 1);
        assert_eq!(levenshtein("abc", "xyz"), 3);
    }

    #[test]
    fn detects_single_word_correction() {
        let out = detect_correction("Install kwen.", "Install qwen.").unwrap();
        assert_eq!(out.wrong, "kwen");
        assert_eq!(out.right, "qwen");
    }

    #[test]
    fn ignores_unchanged_text() {
        assert!(detect_correction("Hello world.", "Hello world.").is_none());
    }

    #[test]
    fn ignores_capitalization_only() {
        // Style/preferred-spelling territory, not a recognition
        // correction. The cleanup pass already handles casing.
        assert!(detect_correction("Use Python.", "Use python.").is_none());
        assert!(detect_correction("call API.", "call api.").is_none());
    }

    #[test]
    fn ignores_short_words() {
        // "a" → "the" is a rewrite, not a recognition fix.
        assert!(detect_correction("Sip a tea.", "Sip the tea.").is_none());
    }

    #[test]
    fn ignores_multi_word_edits() {
        // Two words changed = rewrite, not a recognition fix.
        assert!(
            detect_correction("Fix the kwen model bug.", "Fix the qwen model issue.").is_none()
        );
    }

    #[test]
    fn ignores_high_distance_edits() {
        // 4+ char edit distance is more rewrite than recognition.
        assert!(detect_correction("Install elephantine.", "Install programmable.").is_none());
    }

    #[test]
    fn ignores_long_words() {
        // Likely a model ID, file path, or URL — user typed by hand.
        let long_a = "abcdefghijklmnopqrstuvwxyz0123456789";
        let long_b = "abcdefghijklmnopqrstuvwxyz012345678X";
        let pasted = format!("Use {long_a}.");
        let current = format!("Use {long_b}.");
        assert!(detect_correction(&pasted, &current).is_none());
    }

    #[test]
    fn detects_correction_with_surrounding_text() {
        // The user's field had "Hello, " before paste. We pasted
        // "install kwen for the cleanup pass." and they corrected
        // "kwen" to "qwen". The surrounding context shouldn't confuse
        // the matcher.
        let pasted = "install kwen for the cleanup pass.";
        let current = "install qwen for the cleanup pass.";
        let out = detect_correction(pasted, current).unwrap();
        assert_eq!(out.wrong, "kwen");
        assert_eq!(out.right, "qwen");
    }

    #[test]
    fn strips_outer_punct() {
        assert_eq!(strip_outer_punct("\"Qwen,\""), "Qwen");
        assert_eq!(strip_outer_punct("end."), "end");
        assert_eq!(strip_outer_punct("don't"), "don't");
        assert_eq!(strip_outer_punct("self-host"), "self-host");
    }

    #[test]
    fn ignores_empty_or_whitespace_only() {
        assert!(detect_correction("", "").is_none());
        assert!(detect_correction("   ", "hello world").is_none());
    }
}
