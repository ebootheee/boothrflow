//! End-to-end pipeline integration test running fully on fakes.
//!
//! This is the canonical "shape" test for the hot path. Unlike unit tests
//! inside the lib crate, integration tests live in their own binary and
//! exercise the public API only — keeping our trait boundaries honest.

use boothrflow_lib::audio::FakeAudioSource;
use boothrflow_lib::context::FixedContextDetector;
use boothrflow_lib::injector::RecordingInjector;
use boothrflow_lib::llm::FakeLlmCleanup;
use boothrflow_lib::pipeline::Pipeline;
use boothrflow_lib::settings::Style;
use boothrflow_lib::stt::FakeSttEngine;

#[test]
fn full_pipeline_runs_with_fakes() {
    let audio = FakeAudioSource::silence(0.5);
    let stt = FakeSttEngine::canned("uh basically the integration test runs cleanly");
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

    assert!(outcome.duration_ms < 5_000, "fake pipeline must be fast");
    assert!(!outcome.formatted.contains("uh"));
    assert!(!outcome.formatted.contains("basically"));
    assert_eq!(injector.calls().len(), 1);
    assert_eq!(injector.calls()[0], outcome.formatted);
    assert_eq!(outcome.app_exe.as_deref(), Some("slack.exe"));
}

// Input is intentionally >= 6 words so `should_skip_llm` lets the LLM run.
// Update both `RAW_INPUT` and the per-style expectations together if either
// the threshold (`should_skip_llm`) or the FakeLlmCleanup behavior changes.
const RAW_INPUT: &str = "this is another regular test sentence";

#[rstest::rstest]
#[case(Style::Raw, "this is another regular test sentence")]
#[case(Style::Formal, "This is another regular test sentence.")]
#[case(Style::Casual, "this is another regular test sentence")]
#[case(Style::Excited, "This is another regular test sentence!")]
#[case(Style::VeryCasual, "this is another regular test sentence")]
fn style_shapes_output(#[case] style: Style, #[case] expected: &str) {
    let audio = FakeAudioSource::silence(0.5);
    let stt = FakeSttEngine::canned(RAW_INPUT);
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

    let outcome = pipeline.dictate_once(style, true).unwrap();
    assert_eq!(outcome.formatted, expected);
}
