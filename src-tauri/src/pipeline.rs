//! The hot path: hotkey press → audio capture → VAD-gated buffer → STT →
//! optional LLM cleanup → injector → history.
//!
//! In v0 only the synchronous, single-utterance flow is wired. Streaming
//! partials and overlapping windows land in Phase 2.

use crate::audio::{AudioFrame, AudioSource};
use crate::context::ContextDetector;
use crate::error::Result;
use crate::injector::Injector;
use crate::llm::{should_skip_llm, CleanupRequest, LlmCleanup};
use crate::settings::Style;
use crate::stt::SttEngine;

#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct DictateOutcome {
    pub raw: String,
    pub formatted: String,
    pub duration_ms: u64,
    pub app_exe: Option<String>,
    pub skipped_llm: bool,
}

pub struct Pipeline<'a> {
    pub audio: &'a dyn AudioSource,
    pub stt: &'a dyn SttEngine,
    pub llm: &'a dyn LlmCleanup,
    pub injector: &'a dyn Injector,
    pub context: &'a dyn ContextDetector,
}

impl Pipeline<'_> {
    /// Run a single push-to-talk utterance: capture every available frame,
    /// transcribe it, format it, paste it. Returns the outcome for UI display
    /// and history persistence.
    pub fn dictate_once(&self, style: Style, llm_enabled: bool) -> Result<DictateOutcome> {
        let started = std::time::Instant::now();
        let frames = drain_audio(self.audio)?;
        let pcm: Vec<f32> = frames.into_iter().flatten().collect();

        let stt = self.stt.transcribe(&pcm)?;

        let app_exe = self.context.detect().map(|c| c.app_exe);
        let skipped_llm = should_skip_llm(&stt.text, llm_enabled);

        let formatted = if skipped_llm {
            stt.text.clone()
        } else {
            let app_context = app_exe.as_deref().map(|exe| crate::context::AppContext {
                app_exe: exe.to_string(),
                app_name: exe.trim_end_matches(".exe").to_string(),
                window_title: None,
                control_role: None,
            });
            self.llm
                .cleanup(CleanupRequest {
                    raw_text: &stt.text,
                    style,
                    app_context,
                    ..Default::default()
                })?
                .text
        };

        self.injector.inject(&formatted)?;

        Ok(DictateOutcome {
            raw: stt.text,
            formatted,
            duration_ms: started.elapsed().as_millis() as u64,
            app_exe,
            skipped_llm,
        })
    }
}

fn drain_audio(audio: &dyn AudioSource) -> Result<Vec<AudioFrame>> {
    let rx = audio.start()?;
    let mut frames = Vec::new();
    while let Ok(f) = rx.recv() {
        frames.push(f);
    }
    audio.stop()?;
    Ok(frames)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::FakeAudioSource;
    use crate::context::FixedContextDetector;
    use crate::injector::RecordingInjector;
    use crate::llm::FakeLlmCleanup;
    use crate::stt::FakeSttEngine;

    #[test]
    fn end_to_end_with_fakes() {
        let audio = FakeAudioSource::silence(0.5);
        let stt = FakeSttEngine::canned("uh hello world this is a test");
        let llm = FakeLlmCleanup;
        let injector = RecordingInjector::new();
        let context = FixedContextDetector::slack();

        let pipeline = Pipeline {
            audio: &audio,
            stt: &stt,
            llm: &llm,
            injector: &injector,
            context: &context,
        };

        let outcome = pipeline.dictate_once(Style::Casual, true).unwrap();

        assert!(!outcome.skipped_llm);
        assert!(!outcome.formatted.contains("uh"));
        assert_eq!(outcome.app_exe.as_deref(), Some("slack.exe"));
        assert_eq!(injector.calls(), vec![outcome.formatted.clone()]);
    }

    #[test]
    fn short_utterance_skips_llm() {
        let audio = FakeAudioSource::silence(0.1);
        let stt = FakeSttEngine::canned("ok");
        let llm = FakeLlmCleanup;
        let injector = RecordingInjector::new();
        let context = FixedContextDetector::none();

        let pipeline = Pipeline {
            audio: &audio,
            stt: &stt,
            llm: &llm,
            injector: &injector,
            context: &context,
        };

        let outcome = pipeline.dictate_once(Style::Formal, true).unwrap();
        assert!(outcome.skipped_llm);
        assert_eq!(outcome.formatted, "ok");
    }

    #[test]
    fn llm_disabled_passes_raw_through() {
        let audio = FakeAudioSource::silence(0.1);
        let stt = FakeSttEngine::canned("uh this would normally be cleaned up");
        let llm = FakeLlmCleanup;
        let injector = RecordingInjector::new();
        let context = FixedContextDetector::none();

        let pipeline = Pipeline {
            audio: &audio,
            stt: &stt,
            llm: &llm,
            injector: &injector,
            context: &context,
        };

        let outcome = pipeline.dictate_once(Style::Casual, false).unwrap();
        assert!(outcome.skipped_llm);
        assert!(outcome.formatted.contains("uh"));
    }
}
