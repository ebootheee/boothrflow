//! Probe: load Nemotron Speech Streaming en-0.6B from the user data
//! dir, decode a test wav, print the transcript. Catches FFI-level
//! breakage (config struct mismatches, feature-dim wrong, etc.) in
//! ~3 seconds without having to spin up the full Tauri app and hold
//! the PTT hotkey manually.
//!
//! Usage:
//!   # default: tries /tmp/nemotron-test.wav, then the bundle's own
//!   # test_wavs/0.wav, then a synthetic 3s sine wave
//!   cargo run --example nemotron_probe \
//!     --features "real-engines parakeet-engine"
//!
//!   # custom wav (16 kHz mono 16-bit PCM)
//!   cargo run --example nemotron_probe \
//!     --features "real-engines parakeet-engine" -- \
//!     /path/to/some.wav
//!
//! Sister probe to `parakeet_probe.rs`. Lives alongside it so the
//! same `cargo run --example <X>_probe` muscle memory works for both
//! sherpa-onnx-backed engines.

#[cfg(feature = "parakeet-engine")]
fn main() {
    use boothrflow_lib::stt::{NemotronStreamingSttEngine, SttEngine};
    use std::path::PathBuf;

    let model_dir = dirs::data_dir()
        .expect("data_dir")
        .join("boothrflow")
        .join("models")
        .join("nemotron-speech-streaming-en-0.6b");

    eprintln!("probe: loading from {}", model_dir.display());

    let engine = match NemotronStreamingSttEngine::from_model_dir(&model_dir) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("probe: from_model_dir Err: {e}");
            std::process::exit(2);
        }
    };
    eprintln!("probe: engine constructed");

    // Pick the wav. CLI arg wins; else the manually-staged /tmp wav;
    // else a synthetic sine wave (which won't decode to anything
    // meaningful but proves the FFI round-trip survives).
    let cli_wav: Option<PathBuf> = std::env::args().nth(1).map(PathBuf::from);
    let tmp_wav = PathBuf::from("/tmp/nemotron-test.wav");
    let chosen = cli_wav.as_deref().or_else(|| {
        if tmp_wav.exists() {
            Some(tmp_wav.as_path())
        } else {
            None
        }
    });

    let audio: Vec<f32> = if let Some(path) = chosen {
        eprintln!("probe: reading {}", path.display());
        let mut reader = hound::WavReader::open(path).expect("open wav");
        let spec = reader.spec();
        eprintln!(
            "probe: wav spec — {} Hz, {} ch, {}-bit {}",
            spec.sample_rate,
            spec.channels,
            spec.bits_per_sample,
            if matches!(spec.sample_format, hound::SampleFormat::Float) {
                "f32"
            } else {
                "int"
            }
        );
        if spec.sample_rate != 16_000 || spec.channels != 1 {
            eprintln!(
                "probe: WARNING — expected 16 kHz mono, got {} Hz / {} ch. Result will be garbage but the FFI round-trip is what we're testing.",
                spec.sample_rate, spec.channels
            );
        }
        reader
            .samples::<i16>()
            .map(|s| s.expect("read sample") as f32 / 32768.0)
            .collect()
    } else {
        eprintln!("probe: no wav available, using a 3s sine wave");
        let sample_rate = 16_000usize;
        let duration_s = 3usize;
        (0..(sample_rate * duration_s))
            .map(|i| {
                let t = i as f32 / sample_rate as f32;
                0.05 * (2.0 * std::f32::consts::PI * 220.0 * t).sin()
            })
            .collect()
    };
    eprintln!(
        "probe: audio samples = {} ({:.2}s)",
        audio.len(),
        audio.len() as f32 / 16_000.0
    );

    let started = std::time::Instant::now();
    match engine.transcribe(&audio) {
        Ok(out) => {
            eprintln!(
                "probe: transcribe ok in {} ms — text=\"{}\"",
                started.elapsed().as_millis(),
                out.text
            );
        }
        Err(e) => {
            eprintln!("probe: transcribe Err: {e}");
            std::process::exit(3);
        }
    }
    eprintln!("probe: ok");
}

#[cfg(not(feature = "parakeet-engine"))]
fn main() {
    eprintln!("probe: build with --features \"real-engines parakeet-engine\"");
    std::process::exit(1);
}
