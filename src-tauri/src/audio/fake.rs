use parking_lot::Mutex;

use crate::audio::{AudioFrame, AudioSource};
use crate::error::Result;

/// Deterministic audio source for tests. Hand it a Vec of pre-baked frames
/// and it'll emit them once on `start()` then close the channel.
pub struct FakeAudioSource {
    frames: Mutex<Vec<AudioFrame>>,
}

impl FakeAudioSource {
    pub fn new(frames: Vec<AudioFrame>) -> Self {
        Self {
            frames: Mutex::new(frames),
        }
    }

    /// Build a fake source that yields N seconds of silence at 16 kHz mono.
    pub fn silence(seconds: f32) -> Self {
        let n = (seconds * 16_000.0).round() as usize;
        let frame_size = 1600; // 100ms frames
        let mut frames = Vec::new();
        let mut remaining = n;
        while remaining > 0 {
            let take = remaining.min(frame_size);
            frames.push(vec![0.0; take]);
            remaining -= take;
        }
        Self::new(frames)
    }
}

impl AudioSource for FakeAudioSource {
    fn start(&self) -> Result<crossbeam_channel::Receiver<AudioFrame>> {
        let (tx, rx) = crossbeam_channel::unbounded();
        let frames = std::mem::take(&mut *self.frames.lock());
        for f in frames {
            // ignore send errors — receiver may have dropped early
            let _ = tx.send(f);
        }
        Ok(rx)
    }

    fn stop(&self) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_produces_expected_sample_count() {
        let src = FakeAudioSource::silence(1.0);
        let rx = src.start().unwrap();
        let total: usize = rx.try_iter().map(|f| f.len()).sum();
        assert_eq!(total, 16_000);
    }

    #[test]
    fn empty_source_drops_immediately() {
        let src = FakeAudioSource::new(vec![]);
        let rx = src.start().unwrap();
        let total: usize = rx.try_iter().map(|f| f.len()).sum();
        assert_eq!(total, 0);
    }
}
