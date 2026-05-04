//! Benchmark grading commands. Reads + writes the
//! `<stem>.variants.json` files produced by `examples/bench_replay.rs`.
//!
//! The replay tool is the heavy lift (loads engines, transcribes,
//! cleanup-passes). This module is just CRUD over the resulting JSON
//! sidecars + a small list view that the FE renders. No engines
//! loaded here.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{BoothError, Result};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct CaptureRow {
    pub wav_filename: String,
    pub captured_at: String,
    pub app_exe: Option<String>,
    pub audio_seconds: f32,
    /// Engine that produced the original capture (sidecar `engine` field).
    pub original_engine: String,
    pub raw: String,
    pub formatted: String,
    /// Has a sibling `<stem>.variants.json`?
    pub has_variants: bool,
    /// Number of variants in the variants file.
    pub variant_count: u32,
    /// Number of variants that have a `grade` field set.
    pub graded_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct Variant {
    pub config_id: String,
    pub engine: String,
    pub llm_model: String,
    pub style: String,
    pub raw: String,
    pub formatted: String,
    pub stt_ms: u64,
    pub llm_ms: u64,
    #[serde(default)]
    pub grade: Option<u8>,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct VariantsFile {
    pub wav: String,
    pub audio_seconds: f32,
    pub variants: Vec<Variant>,
}

/// Sidecar for the original capture (written by `captures.rs`).
#[derive(Debug, Clone, Deserialize)]
struct CaptureSidecar {
    captured_at: String,
    engine: String,
    app_exe: Option<String>,
    audio_seconds: f32,
    raw: String,
    formatted: String,
}

fn captures_dir() -> Result<PathBuf> {
    let base =
        dirs::data_dir().ok_or_else(|| BoothError::internal("could not resolve user data dir"))?;
    Ok(base.join("boothrflow").join("captures"))
}

pub fn list() -> Result<Vec<CaptureRow>> {
    let dir = captures_dir()?;
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut rows = Vec::new();
    let entries = fs::read_dir(&dir)
        .map_err(|e| BoothError::internal(format!("read_dir {}: {e}", dir.display())))?;
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("wav") {
            continue;
        }
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let json_path = dir.join(format!("{stem}.json"));
        let variants_path = dir.join(format!("{stem}.variants.json"));

        let sidecar: Option<CaptureSidecar> = fs::read_to_string(&json_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok());

        let (variant_count, graded_count) = match fs::read_to_string(&variants_path)
            .ok()
            .and_then(|s| serde_json::from_str::<VariantsFile>(&s).ok())
        {
            Some(v) => {
                let total = v.variants.len() as u32;
                let graded = v.variants.iter().filter(|x| x.grade.is_some()).count() as u32;
                (total, graded)
            }
            None => (0, 0),
        };

        rows.push(CaptureRow {
            wav_filename: path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string(),
            captured_at: sidecar
                .as_ref()
                .map(|s| s.captured_at.clone())
                .unwrap_or_default(),
            app_exe: sidecar.as_ref().and_then(|s| s.app_exe.clone()),
            audio_seconds: sidecar.as_ref().map(|s| s.audio_seconds).unwrap_or(0.0),
            original_engine: sidecar
                .as_ref()
                .map(|s| s.engine.clone())
                .unwrap_or_default(),
            raw: sidecar.as_ref().map(|s| s.raw.clone()).unwrap_or_default(),
            formatted: sidecar
                .as_ref()
                .map(|s| s.formatted.clone())
                .unwrap_or_default(),
            has_variants: variants_path.exists(),
            variant_count,
            graded_count,
        });
    }
    // Newest first by captured_at; falls back to filename if missing.
    rows.sort_by(|a, b| {
        let av = a.captured_at.as_str();
        let bv = b.captured_at.as_str();
        bv.cmp(av).then_with(|| b.wav_filename.cmp(&a.wav_filename))
    });
    Ok(rows)
}

pub fn load(wav_filename: &str) -> Result<Option<VariantsFile>> {
    let dir = captures_dir()?;
    let stem = wav_filename.strip_suffix(".wav").unwrap_or(wav_filename);
    let path = dir.join(format!("{stem}.variants.json"));
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(&path)
        .map_err(|e| BoothError::internal(format!("read {}: {e}", path.display())))?;
    let parsed: VariantsFile = serde_json::from_slice(&bytes)
        .map_err(|e| BoothError::internal(format!("parse {}: {e}", path.display())))?;
    Ok(Some(parsed))
}

pub fn save(wav_filename: &str, variants: VariantsFile) -> Result<()> {
    let dir = captures_dir()?;
    let stem = wav_filename.strip_suffix(".wav").unwrap_or(wav_filename);
    let path = dir.join(format!("{stem}.variants.json"));
    let bytes = serde_json::to_vec_pretty(&variants)
        .map_err(|e| BoothError::internal(format!("serialize: {e}")))?;
    fs::write(&path, bytes)
        .map_err(|e| BoothError::internal(format!("write {}: {e}", path.display())))?;
    Ok(())
}

/// Returns the absolute path of the wav so the FE can pass it to a
/// Tauri asset URL. Tauri's `convertFileSrc` requires an absolute
/// path.
pub fn wav_path(wav_filename: &str) -> Result<PathBuf> {
    let dir = captures_dir()?;
    Ok(dir.join(wav_filename))
}
