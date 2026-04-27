use std::collections::HashMap;

use crate::error::Result;
use crate::stt::{SttEngine, SttResult};

/// Scripted STT — keys input audio length to canned transcripts.
/// In real tests you'll usually want [`FakeSttEngine::canned`].
pub struct FakeSttEngine {
    by_len: HashMap<usize, String>,
    default: String,
}

impl FakeSttEngine {
    /// Returns the same string regardless of input.
    pub fn canned(text: impl Into<String>) -> Self {
        Self {
            by_len: HashMap::new(),
            default: text.into(),
        }
    }

    /// Builder-style — register a canned response keyed by audio sample count.
    pub fn with_response(mut self, sample_count: usize, text: impl Into<String>) -> Self {
        self.by_len.insert(sample_count, text.into());
        self
    }
}

impl SttEngine for FakeSttEngine {
    fn transcribe(&self, audio: &[f32]) -> Result<SttResult> {
        let text = self
            .by_len
            .get(&audio.len())
            .cloned()
            .unwrap_or_else(|| self.default.clone());
        Ok(SttResult {
            text,
            language: Some("en".into()),
            duration_ms: 0,
        })
    }

    fn name(&self) -> &str {
        "fake-stt"
    }
}
