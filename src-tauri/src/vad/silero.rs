//! Silero VAD via the [`voice_activity_detector`] crate (ONNX Runtime).
//!
//! Silero v4 is the production default — best speech/non-speech accuracy
//! at this size class. Under the hood the crate ships the model weights
//! embedded; no model download needed at runtime.

use parking_lot::Mutex;
use voice_activity_detector::VoiceActivityDetector;

use crate::error::{BoothError, Result};
use crate::vad::Vad;

const SAMPLE_RATE: i64 = 16_000;
/// Silero's expected chunk size for 16kHz inputs.
const CHUNK_SIZE: usize = 512;

pub struct SileroVad {
    detector: Mutex<VoiceActivityDetector>,
}

impl SileroVad {
    pub fn new() -> Result<Self> {
        let detector = VoiceActivityDetector::builder()
            .sample_rate(SAMPLE_RATE)
            .chunk_size(CHUNK_SIZE)
            .build()
            .map_err(|e| BoothError::internal(format!("silero vad init: {e}")))?;
        Ok(Self {
            detector: Mutex::new(detector),
        })
    }
}

impl Vad for SileroVad {
    fn score(&self, frame: &[f32]) -> Result<f32> {
        // Silero is sized for exactly CHUNK_SIZE samples per call. cpal_source
        // emits frames at this size; if a smaller frame slips through (e.g.
        // a flush on stream stop), pad with zeros.
        if frame.len() == CHUNK_SIZE {
            Ok(self.detector.lock().predict(frame.iter().copied()))
        } else if frame.len() < CHUNK_SIZE {
            let mut buf = vec![0.0f32; CHUNK_SIZE];
            buf[..frame.len()].copy_from_slice(frame);
            Ok(self.detector.lock().predict(buf))
        } else {
            // Too big — score the first CHUNK_SIZE samples; caller should
            // chunk before calling for stable framing.
            Ok(self
                .detector
                .lock()
                .predict(frame[..CHUNK_SIZE].iter().copied()))
        }
    }
}
