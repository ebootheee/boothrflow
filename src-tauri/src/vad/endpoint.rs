//! Endpoint detection on top of [`Vad`] scores.
//!
//! Push frames in, get `SpeechStarted` / `SpeechEnded` events out. Used by
//! the session daemon to detect natural utterance boundaries within the
//! push-to-talk press window — for v0 we still rely on hotkey
//! press/release as the outer boundaries; this is the foundation for
//! "stream the partial transcript before the user releases."

use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointEvent {
    SpeechStarted,
    SpeechEnded,
}

pub struct EndpointDetector {
    threshold: f32,
    silence_hangover: Duration,
    in_speech: bool,
    last_speech: Option<Instant>,
}

impl EndpointDetector {
    /// `threshold` is the VAD probability above which a frame is "speech."
    /// `silence_hangover` is how long below threshold before we declare end.
    pub fn new(threshold: f32, silence_hangover: Duration) -> Self {
        Self {
            threshold,
            silence_hangover,
            in_speech: false,
            last_speech: None,
        }
    }

    pub fn default_for_dictation() -> Self {
        // 0.5 threshold + 700ms hangover is a reasonable default that lets
        // brief inter-word pauses through but ends sessions on real silence.
        Self::new(0.5, Duration::from_millis(700))
    }

    pub fn is_in_speech(&self) -> bool {
        self.in_speech
    }

    /// Process one VAD score for a frame timestamped at `now`. Returns an
    /// event only on transitions.
    pub fn observe(&mut self, score: f32, now: Instant) -> Option<EndpointEvent> {
        let above = score >= self.threshold;
        if above {
            self.last_speech = Some(now);
            if !self.in_speech {
                self.in_speech = true;
                return Some(EndpointEvent::SpeechStarted);
            }
        } else if self.in_speech {
            if let Some(last) = self.last_speech {
                if now.duration_since(last) >= self.silence_hangover {
                    self.in_speech = false;
                    return Some(EndpointEvent::SpeechEnded);
                }
            }
        }
        None
    }

    pub fn reset(&mut self) {
        self.in_speech = false;
        self.last_speech = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn started_event_on_first_speech() {
        let mut ep = EndpointDetector::new(0.5, Duration::from_millis(500));
        let t0 = Instant::now();

        assert_eq!(ep.observe(0.1, t0), None);
        assert_eq!(ep.observe(0.7, t0), Some(EndpointEvent::SpeechStarted));
        assert_eq!(ep.observe(0.8, t0), None); // still in speech
    }

    #[test]
    fn ended_event_after_hangover() {
        let mut ep = EndpointDetector::new(0.5, Duration::from_millis(500));
        let t0 = Instant::now();

        ep.observe(0.7, t0);
        // silence below threshold but within hangover
        assert_eq!(ep.observe(0.1, t0 + Duration::from_millis(100)), None);
        // still within hangover
        assert_eq!(ep.observe(0.1, t0 + Duration::from_millis(300)), None);
        // past hangover
        assert_eq!(
            ep.observe(0.1, t0 + Duration::from_millis(700)),
            Some(EndpointEvent::SpeechEnded)
        );
    }

    #[test]
    fn brief_dip_below_threshold_does_not_end() {
        let mut ep = EndpointDetector::new(0.5, Duration::from_millis(500));
        let t0 = Instant::now();

        ep.observe(0.7, t0);
        // brief silence
        ep.observe(0.1, t0 + Duration::from_millis(100));
        // back to speech before hangover expires — no SpeechEnded emitted
        assert_eq!(ep.observe(0.7, t0 + Duration::from_millis(200)), None);
        // still in speech
        assert!(ep.is_in_speech());
    }

    #[test]
    fn reset_clears_state() {
        let mut ep = EndpointDetector::new(0.5, Duration::from_millis(500));
        let t0 = Instant::now();
        ep.observe(0.7, t0);
        assert!(ep.is_in_speech());
        ep.reset();
        assert!(!ep.is_in_speech());
    }
}
