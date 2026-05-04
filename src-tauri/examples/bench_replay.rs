//! Benchmark replay tool.
//!
//! Reads every `*.wav` in the captures directory (`~/Library/Application
//! Support/boothrflow/captures` on macOS), runs each one through every
//! (STT engine × LLM model × style) combination it can find on disk +
//! talk to, and writes `<stem>.variants.json` alongside each wav.
//!
//! Lets you replay the same audio across configurations without
//! re-recording. Pair with the grading UI (next phase) to assign 1-5
//! ratings per variant; aggregate mean rating per config across all
//! graded wavs gives a defensible "is engine X actually better"
//! answer without verbatim ground-truth transcription work.
//!
//! Build:
//!   cargo run --example bench_replay --features "real-engines parakeet-engine"
//!
//! Configs auto-detected:
//!   - Every Whisper `.bin` in models dir → one variant
//!   - Parakeet model dir if present (and built with `parakeet-engine`)
//!   - Each variant runs the user's currently-configured LLM
//!   - Two styles: Casual + Raw (Raw skips LLM cleanup for the
//!     "what does the engine alone produce" data point)
//!
//! Output JSON shape per wav:
//!   {
//!     "wav": "2026-05-04T02-35-48Z-claudefordesktop.wav",
//!     "audio_seconds": 48.93,
//!     "variants": [
//!       {
//!         "config_id": "whisper:base.en + qwen2.5:7b + casual",
//!         "engine": "whisper:base.en",
//!         "llm_model": "qwen2.5:7b",
//!         "style": "casual",
//!         "raw": "...",
//!         "formatted": "...",
//!         "stt_ms": 856,
//!         "llm_ms": 4127,
//!         "grade": null,         ← fill in when grading
//!         "notes": null
//!       }
//!     ]
//!   }

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[cfg(feature = "real-engines")]
use boothrflow_lib::error::Result;
#[cfg(feature = "real-engines")]
use boothrflow_lib::llm::{CleanupRequest, LlmCleanup, OpenAiCompatLlmCleanup};
#[cfg(feature = "real-engines")]
use boothrflow_lib::settings::{self, Style};
#[cfg(feature = "real-engines")]
use boothrflow_lib::stt::{SttEngine, WhisperSttEngine};

#[cfg(feature = "parakeet-engine")]
use boothrflow_lib::stt::ParakeetSttEngine;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Variant {
    config_id: String,
    engine: String,
    llm_model: String,
    style: String,
    raw: String,
    formatted: String,
    stt_ms: u64,
    llm_ms: u64,
    /// Filled in by the grading UI (or by hand). 1-5; null until graded.
    #[serde(default)]
    grade: Option<u8>,
    /// Optional free-text note from the grading session.
    #[serde(default)]
    notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VariantsFile {
    wav: String,
    audio_seconds: f32,
    variants: Vec<Variant>,
}

#[cfg(not(feature = "real-engines"))]
fn main() {
    eprintln!("bench_replay requires the real-engines feature. Run with:");
    eprintln!("  cargo run --example bench_replay --features \"real-engines parakeet-engine\"");
    std::process::exit(1);
}

#[cfg(feature = "real-engines")]
fn main() -> Result<()> {
    let captures_dir = dirs::data_dir()
        .ok_or_else(|| {
            boothrflow_lib::error::BoothError::internal("could not resolve user data dir")
        })?
        .join("boothrflow")
        .join("captures");

    if !captures_dir.exists() {
        eprintln!("no captures directory at {}", captures_dir.display());
        eprintln!("run with `BOOTHRFLOW_SAVE_CAPTURES=1 pnpm dev:parakeet` first");
        std::process::exit(2);
    }

    eprintln!("bench_replay: scanning {}", captures_dir.display());
    let wavs = list_wavs(&captures_dir);
    if wavs.is_empty() {
        eprintln!("no .wav files in {}", captures_dir.display());
        std::process::exit(0);
    }
    eprintln!("found {} wav(s)", wavs.len());

    // Discover available STT engines from the models dir.
    let stt_configs = discover_stt_configs()?;
    if stt_configs.is_empty() {
        eprintln!("no STT models found — download at least one with");
        eprintln!("  pnpm download:model:mac base");
        std::process::exit(3);
    }
    eprintln!("STT configs to test:");
    for c in &stt_configs {
        eprintln!("  - {}", c.engine_label);
    }

    // Use the user's currently-configured LLM endpoint + model.
    let app_settings = settings::current_app_settings();
    let llm_endpoint = app_settings.llm.endpoint.clone();
    let llm_model = app_settings.llm.model.clone();
    let llm_api_key = app_settings.llm.api_key.clone();
    eprintln!(
        "LLM: {} (endpoint={})",
        llm_model, llm_endpoint
    );

    // Pre-build LLM client (reused across variants).
    let llm = OpenAiCompatLlmCleanup::new(
        llm_endpoint.clone(),
        llm_model.clone(),
        llm_api_key.filter(|k| !k.trim().is_empty()),
    )?;

    let styles = [
        ("casual", Style::Casual),
        ("raw", Style::Raw),
    ];

    let mut total_variants = 0usize;
    for wav_path in wavs {
        let stem = wav_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();
        let variants_path = wav_path.with_extension("variants.json");

        eprintln!("\n[{}]", wav_path.file_name().and_then(|s| s.to_str()).unwrap_or("?"));
        let audio = match read_wav(&wav_path) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("  read failed: {e}");
                continue;
            }
        };
        let audio_seconds = audio.len() as f32 / 16_000.0;
        eprintln!("  audio: {} samples, {:.2}s", audio.len(), audio_seconds);

        let mut variants = Vec::new();
        for stt_config in &stt_configs {
            // Load the engine for this config; drop after replay so memory
            // doesn't pile up across multiple Whisper variants.
            eprintln!("  ↳ {}", stt_config.engine_label);
            let stt_started = Instant::now();
            let raw = match stt_config.transcribe(&audio) {
                Ok(text) => text,
                Err(e) => {
                    eprintln!("    stt failed: {e}");
                    continue;
                }
            };
            let stt_ms = stt_started.elapsed().as_millis() as u64;
            eprintln!(
                "    raw ({} ms): {}",
                stt_ms,
                preview(&raw, 80)
            );

            for (style_name, style) in &styles {
                let formatted_started = Instant::now();
                let formatted = if matches!(style, Style::Raw) {
                    raw.clone()
                } else {
                    match llm.cleanup(CleanupRequest {
                        raw_text: &raw,
                        style: *style,
                        ..Default::default()
                    }) {
                        Ok(out) => out.text,
                        Err(e) => {
                            eprintln!("    llm failed: {e}");
                            raw.clone()
                        }
                    }
                };
                let llm_ms = if matches!(style, Style::Raw) {
                    0
                } else {
                    formatted_started.elapsed().as_millis() as u64
                };

                let config_id = format!(
                    "{} + {} + {}",
                    stt_config.engine_label, llm_model, style_name
                );
                eprintln!("    → {} → {}", style_name, preview(&formatted, 80));
                variants.push(Variant {
                    config_id,
                    engine: stt_config.engine_label.clone(),
                    llm_model: llm_model.clone(),
                    style: (*style_name).to_string(),
                    raw: raw.clone(),
                    formatted,
                    stt_ms,
                    llm_ms,
                    grade: None,
                    notes: None,
                });
                total_variants += 1;
            }
        }

        let payload = VariantsFile {
            wav: wav_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(&stem)
                .to_string(),
            audio_seconds,
            variants,
        };
        match serde_json::to_vec_pretty(&payload) {
            Ok(bytes) => {
                if let Err(e) = fs::write(&variants_path, bytes) {
                    eprintln!("  write failed: {e}");
                } else {
                    eprintln!("  → {}", variants_path.display());
                }
            }
            Err(e) => eprintln!("  serialize failed: {e}"),
        }
    }

    eprintln!(
        "\nbench_replay: wrote {} variant(s) across {} wav(s)",
        total_variants,
        list_wavs(&captures_dir).len()
    );
    eprintln!(
        "Grade by editing the `grade` field (1-5) and `notes` (string) in each .variants.json,"
    );
    eprintln!(
        "or wait for the in-app grading UI (Wave 7 candidate)."
    );
    Ok(())
}

#[cfg(feature = "real-engines")]
fn list_wavs(dir: &Path) -> Vec<PathBuf> {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return vec![],
    };
    let mut wavs: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("wav"))
        .collect();
    wavs.sort();
    wavs
}

#[cfg(feature = "real-engines")]
fn read_wav(path: &Path) -> Result<Vec<f32>> {
    let mut reader = hound::WavReader::open(path).map_err(|e| {
        boothrflow_lib::error::BoothError::internal(format!(
            "open wav {}: {e}",
            path.display()
        ))
    })?;
    let spec = reader.spec();
    if spec.channels != 1 {
        return Err(boothrflow_lib::error::BoothError::internal(format!(
            "expected mono wav, got {} channels",
            spec.channels
        )));
    }
    let samples: Result<Vec<f32>> = match spec.sample_format {
        hound::SampleFormat::Int => {
            let max = (1u32 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|s| {
                    s.map(|v| v as f32 / max).map_err(|e| {
                        boothrflow_lib::error::BoothError::internal(format!("read sample: {e}"))
                    })
                })
                .collect()
        }
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .map(|s| {
                s.map_err(|e| {
                    boothrflow_lib::error::BoothError::internal(format!("read sample: {e}"))
                })
            })
            .collect(),
    };
    let mut audio = samples?;
    // Resample if needed. The captures we save are 16 kHz already, so this
    // is a defensive path for stray external wavs.
    if spec.sample_rate != 16_000 {
        eprintln!(
            "    note: wav is {} Hz, resampling to 16000 Hz (naive)",
            spec.sample_rate
        );
        audio = naive_resample(&audio, spec.sample_rate, 16_000);
    }
    Ok(audio)
}

#[cfg(feature = "real-engines")]
fn naive_resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to {
        return input.to_vec();
    }
    let ratio = from as f32 / to as f32;
    let out_len = (input.len() as f32 / ratio) as usize;
    (0..out_len)
        .map(|i| {
            let src_idx = (i as f32 * ratio) as usize;
            input.get(src_idx).copied().unwrap_or(0.0)
        })
        .collect()
}

#[cfg(feature = "real-engines")]
struct SttConfig {
    engine_label: String,
    builder: SttBuilder,
}

#[cfg(feature = "real-engines")]
enum SttBuilder {
    Whisper(PathBuf, String),
    #[cfg(feature = "parakeet-engine")]
    Parakeet(PathBuf),
}

#[cfg(feature = "real-engines")]
impl SttConfig {
    fn transcribe(&self, audio: &[f32]) -> Result<String> {
        match &self.builder {
            SttBuilder::Whisper(path, name) => {
                let engine = WhisperSttEngine::from_path(path, name.clone())?;
                let result = engine.transcribe(audio)?;
                Ok(result.text)
            }
            #[cfg(feature = "parakeet-engine")]
            SttBuilder::Parakeet(dir) => {
                let engine = ParakeetSttEngine::from_model_dir(dir)?;
                let result = engine.transcribe(audio)?;
                Ok(result.text)
            }
        }
    }
}

#[cfg(feature = "real-engines")]
fn discover_stt_configs() -> Result<Vec<SttConfig>> {
    let models_dir = boothrflow_lib::stt::default_models_dir()
        .ok_or_else(|| {
            boothrflow_lib::error::BoothError::internal("could not resolve models dir")
        })?;

    let mut configs = Vec::new();

    // Whisper variants — every ggml-*.bin we find.
    let whisper_models = [
        ("ggml-tiny.en.bin", "tiny.en"),
        ("ggml-base.en.bin", "base.en"),
        ("ggml-small.en.bin", "small.en"),
        ("ggml-medium.en.bin", "medium.en"),
        ("ggml-large-v3-turbo.bin", "large-v3-turbo"),
    ];
    for (file, label) in whisper_models {
        let path = models_dir.join(file);
        if path.exists() {
            configs.push(SttConfig {
                engine_label: format!("whisper:{label}"),
                builder: SttBuilder::Whisper(path, format!("whisper:{label}")),
            });
        }
    }

    // Parakeet — directory with the four expected files.
    #[cfg(feature = "parakeet-engine")]
    {
        let dir = models_dir.join("parakeet-tdt-0.6b-v3");
        if dir.join("encoder.onnx").exists()
            && dir.join("decoder.onnx").exists()
            && dir.join("joiner.onnx").exists()
            && dir.join("tokens.txt").exists()
        {
            configs.push(SttConfig {
                engine_label: "parakeet:0.6b-v2-int8".into(),
                builder: SttBuilder::Parakeet(dir),
            });
        }
    }

    Ok(configs)
}

#[cfg(feature = "real-engines")]
fn preview(s: &str, max_chars: usize) -> String {
    let trimmed: String = s.chars().take(max_chars).collect();
    if s.chars().count() > max_chars {
        format!("{trimmed}…")
    } else {
        trimmed
    }
}
