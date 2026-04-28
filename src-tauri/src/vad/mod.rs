//! Voice Activity Detection.
//!
//! TEN-VAD or Silero in production; an "always speech" or "scripted" fake in
//! tests. The trait is intentionally tiny — one frame in, a probability out.

use crate::error::Result;

pub trait Vad: Send + Sync {
    /// Probability of speech in this frame, in `[0.0, 1.0]`.
    fn score(&self, frame: &[f32]) -> Result<f32>;

    /// Threshold above which we treat a frame as speech.
    fn threshold(&self) -> f32 {
        0.5
    }
}

#[cfg(any(test, feature = "test-fakes"))]
pub mod fake;
#[cfg(any(test, feature = "test-fakes"))]
pub use fake::FakeVad;

#[cfg(feature = "real-engines")]
pub mod silero;
#[cfg(feature = "real-engines")]
pub use silero::SileroVad;

pub mod endpoint;
pub use endpoint::{EndpointDetector, EndpointEvent};
