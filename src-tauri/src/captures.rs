//! Optional capture-to-disk for benchmarking.
//!
//! When `BOOTHRFLOW_SAVE_CAPTURES=1` is set in the environment, every
//! successful dictation writes its audio buffer + metadata pair to
//! the user's data directory:
//!
//! ```text
//! ~/Library/Application Support/boothrflow/captures/
//!   2026-05-03T18-35-48Z-claudefordesktop.wav   ← 16kHz mono i16 PCM
//!   2026-05-03T18-35-48Z-claudefordesktop.json  ← raw + cleaned + timings
//! ```
//!
//! The JSON sidecar carries the engine name, raw + cleaned transcript,
//! style, app context, and timing breakdown. Use it to hand-edit a
//! `ground_truth` field in for later WER computation:
//!
//! ```json
//! {
//!   "engine": "parakeet:parakeet-tdt-0.6b-v3",
//!   "stt_ms": 3033,
//!   "raw": "I have two questions...",
//!   "ground_truth": "I have two questions..."   ← edit by hand
//! }
//! ```
//!
//! Sized for the future benchmark harness (Wave 7) — it'll pull from
//! both `testdata/benchmark/` (vendored, hand-curated) and any
//! captures directory the user has populated this way.
//!
//! Does NOT trigger on STT failures or empty transcripts — the
//! captures dir stays clean.

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use serde::Serialize;

use crate::error::{BoothError, Result};

const ENV_FLAG: &str = "BOOTHRFLOW_SAVE_CAPTURES";
const SAMPLE_RATE_HZ: u32 = 16_000;

#[derive(Serialize)]
pub struct CaptureMetadata<'a> {
    pub captured_at: String,
    pub engine: &'a str,
    pub style: &'a str,
    pub app_exe: Option<&'a str>,
    pub window_title: Option<&'a str>,
    pub audio_samples: usize,
    pub audio_seconds: f32,
    pub raw: &'a str,
    pub formatted: &'a str,
    /// Empty by default — fill in by hand for benchmark WER targets.
    pub ground_truth: &'a str,
    pub stt_ms: u64,
    pub llm_ms: u64,
    pub llm_prompt_tokens: Option<u32>,
    pub llm_completion_tokens: Option<u32>,
}

/// Returns true iff captures are enabled (env var set + non-empty/non-zero).
pub fn enabled() -> bool {
    matches!(
        std::env::var(ENV_FLAG).ok().as_deref(),
        Some("1") | Some("true") | Some("yes")
    )
}

/// Write `audio` + a JSON sidecar to the user's captures directory.
/// Caller chooses when to call (typically right after STT, before
/// paste, so a paste failure doesn't suppress the capture).
pub fn save(audio: &[f32], meta: CaptureMetadata<'_>) -> Result<PathBuf> {
    let dir = captures_dir()?;
    fs::create_dir_all(&dir)
        .map_err(|e| BoothError::internal(format!("mkdir {}: {e}", dir.display())))?;

    let stem = filename_stem(&meta.captured_at, meta.app_exe);
    let wav_path = dir.join(format!("{stem}.wav"));
    let json_path = dir.join(format!("{stem}.json"));

    write_wav(&wav_path, audio)?;
    write_json(&json_path, &meta)?;

    tracing::info!(
        "captures: wrote {} ({} samples, {:.2}s)",
        wav_path.display(),
        audio.len(),
        meta.audio_seconds,
    );
    Ok(wav_path)
}

fn captures_dir() -> Result<PathBuf> {
    let base =
        dirs::data_dir().ok_or_else(|| BoothError::internal("could not resolve user data dir"))?;
    Ok(base.join("boothrflow").join("captures"))
}

/// Filename stem in the form `YYYY-MM-DDTHH-MM-SSZ-<app-or-na>` so
/// listings sort chronologically and the focused-app hint is visible.
fn filename_stem(captured_at: &str, app_exe: Option<&str>) -> String {
    let safe_ts = captured_at.replace(':', "-");
    let app = app_exe
        .and_then(|a| a.rsplit('.').next())
        .unwrap_or("na")
        .replace(|c: char| !c.is_ascii_alphanumeric(), "");
    format!("{safe_ts}-{app}")
}

/// Write a 16 kHz mono i16 PCM WAV. f32 input clamped to i16 range.
/// We use a hand-rolled writer rather than pulling `hound` directly
/// from this module to keep the dependency surface explicit.
fn write_wav(path: &PathBuf, audio: &[f32]) -> Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: SAMPLE_RATE_HZ,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)
        .map_err(|e| BoothError::internal(format!("wav create {}: {e}", path.display())))?;
    for &s in audio {
        let i = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        writer
            .write_sample(i)
            .map_err(|e| BoothError::internal(format!("wav write: {e}")))?;
    }
    writer
        .finalize()
        .map_err(|e| BoothError::internal(format!("wav finalize: {e}")))?;
    Ok(())
}

fn write_json(path: &PathBuf, meta: &CaptureMetadata<'_>) -> Result<()> {
    let json = serde_json::to_vec_pretty(meta)
        .map_err(|e| BoothError::internal(format!("json serialize: {e}")))?;
    let mut f = fs::File::create(path)
        .map_err(|e| BoothError::internal(format!("json create {}: {e}", path.display())))?;
    f.write_all(&json)
        .map_err(|e| BoothError::internal(format!("json write: {e}")))?;
    f.write_all(b"\n").ok();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enabled_responds_to_env_truthy_values() {
        let prev = std::env::var(ENV_FLAG).ok();

        std::env::remove_var(ENV_FLAG);
        assert!(!enabled());

        std::env::set_var(ENV_FLAG, "0");
        assert!(!enabled());

        std::env::set_var(ENV_FLAG, "1");
        assert!(enabled());

        std::env::set_var(ENV_FLAG, "true");
        assert!(enabled());

        // Restore so concurrent tests aren't affected.
        match prev {
            Some(v) => std::env::set_var(ENV_FLAG, v),
            None => std::env::remove_var(ENV_FLAG),
        }
    }

    #[test]
    fn filename_stem_strips_unsafe_chars() {
        let stem = filename_stem("2026-05-03T18:35:48Z", Some("com.apple.TextEdit"));
        assert_eq!(stem, "2026-05-03T18-35-48Z-TextEdit");

        let stem_no_app = filename_stem("2026-05-03T18-35-48Z", None);
        assert_eq!(stem_no_app, "2026-05-03T18-35-48Z-na");

        let stem_messy = filename_stem("2026-05-03T18:35:48Z", Some("Slack/Beta!"));
        // Unsafe chars in app exe collapse; rsplit('.') keeps the leaf.
        assert!(stem_messy.starts_with("2026-05-03T18-35-48Z-"));
        assert!(!stem_messy.contains('!'));
        assert!(!stem_messy.contains('/'));
    }
}
