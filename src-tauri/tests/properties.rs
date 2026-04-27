//! Property tests for pure helpers in the pipeline. Stays focused on
//! shapes-not-strings — never assert exact LLM/STT outputs in proptest.

use boothrflow_lib::llm::should_skip_llm;
use proptest::prelude::*;

proptest! {
    /// LLM-disabled always skips, regardless of input.
    #[test]
    fn llm_disabled_always_skips(s in ".{0,200}") {
        prop_assert!(should_skip_llm(&s, false));
    }

    /// Empty string with LLM enabled also skips (zero words < 6).
    #[test]
    fn empty_skips_when_enabled(_dummy in any::<u8>()) {
        prop_assert!(should_skip_llm("", true));
    }

    /// Six-or-more whitespace-separated tokens with LLM enabled never skip.
    #[test]
    fn six_plus_words_never_skip(words in proptest::collection::vec("[a-z]{1,8}", 6..16)) {
        let s = words.join(" ");
        prop_assert!(!should_skip_llm(&s, true));
    }
}
