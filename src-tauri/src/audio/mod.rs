//! Audio capture subsystem.
//!
//! [`AudioSource`] is the trait every consumer (VAD, STT) takes by `&dyn`.
//! Production impl uses cpal in WASAPI shared mode + rubato resampling to
//! 16kHz mono. The fake impl yields canned PCM frames from a pre-loaded
//! buffer (handy for deterministic pipeline tests).

use crate::error::Result;

/// 16-bit signed PCM mono frame at 16 kHz.
pub type AudioFrame = Vec<f32>;

pub trait AudioSource: Send + Sync {
    /// Begin capturing. Returns immediately; frames are produced via the channel.
    fn start(&self) -> Result<crossbeam_channel::Receiver<AudioFrame>>;

    /// Stop capture and release the device.
    fn stop(&self) -> Result<()>;

    /// Sample rate of the frames produced. Always 16 000 in v0.
    fn sample_rate(&self) -> u32 {
        16_000
    }
}

#[cfg(any(test, feature = "test-fakes"))]
pub mod fake;
#[cfg(any(test, feature = "test-fakes"))]
pub use fake::FakeAudioSource;

#[cfg(feature = "real-engines")]
pub mod cpal_source;
#[cfg(feature = "real-engines")]
pub use cpal_source::{CpalAudioSource, MicDevice};
