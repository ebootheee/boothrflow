//! Tauri commands exposed to the frontend.
//!
//! v0 only wires `dictate_once`, which currently runs against fakes. The real
//! pipeline lands when the engine deps are uncommented in Cargo.toml.

use serde::Serialize;

use crate::audio::FakeAudioSource;
use crate::context::FixedContextDetector;
use crate::error::BoothError;
use crate::injector::RecordingInjector;
use crate::llm::FakeLlmCleanup;
use crate::pipeline::Pipeline;
use crate::settings::Style;
use crate::stt::FakeSttEngine;

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct DictateResult {
    pub raw: String,
    pub formatted: String,
    pub duration_ms: u64,
}

#[tauri::command]
pub async fn dictate_once(style: Style) -> Result<DictateResult, BoothError> {
    // v0: still using fakes end-to-end. When real engines are wired in, this
    // body becomes the only place that swaps the trait objects.
    let audio = FakeAudioSource::silence(0.5);
    let stt = FakeSttEngine::canned("uh basically hello there from the fake stt");
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

    let outcome = pipeline
        .dictate_once(style, true)
        .map_err(|e| BoothError::Internal(e.to_string()))?;

    Ok(DictateResult {
        raw: outcome.raw,
        formatted: outcome.formatted,
        duration_ms: outcome.duration_ms,
    })
}
