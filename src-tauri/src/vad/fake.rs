use crate::error::Result;
use crate::vad::Vad;

/// VAD fake that returns a fixed probability for every frame.
/// Use [`FakeVad::always_speech`] / [`FakeVad::always_silence`] for the
/// usual cases.
pub struct FakeVad {
    pub fixed_score: f32,
}

impl FakeVad {
    pub fn always_speech() -> Self {
        Self { fixed_score: 0.95 }
    }

    pub fn always_silence() -> Self {
        Self { fixed_score: 0.05 }
    }
}

impl Vad for FakeVad {
    fn score(&self, _frame: &[f32]) -> Result<f32> {
        Ok(self.fixed_score)
    }
}
