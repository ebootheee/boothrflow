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

#[rstest::rstest]
#[case(Style::Raw, "uh hi there")]
#[case(Style::Formal, "Hi there.")]
#[case(Style::Excited, "Hi there!")]
#[case(Style::VeryCasual, "hi there")]
fn style_shapes_output(#[case] style: Style, #[case] expected: &str) {
    let audio = FakeAudioSource::silence(0.5);
    let stt = FakeSttEngine::canned("uh hi there");
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

    // 3 words → would normally skip the LLM (< 6 word threshold). Force-enable
    // by passing extra context: tweak the canned text below if `should_skip_llm`
    // ever changes its threshold.
    let outcome = pipeline.dictate_once(style, true).unwrap();
    assert_eq!(outcome.formatted, expected);
}
