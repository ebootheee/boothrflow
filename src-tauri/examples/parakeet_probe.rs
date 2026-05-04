//! Probe: try to load the Parakeet model from the user data dir
//! and run a tiny synthetic decode. If the sherpa-onnx version we
//! linked tolerates the existing v2-int8 bundle (no `vocab_size`
//! metadata), we'll see "ok" printed and the process will exit 0.
//! If it hits the metadata gate and calls exit(-1), the probe dies
//! with a non-zero exit code.
//!
//! Usage:
//!   cargo run --example parakeet_probe \
//!     --features "real-engines parakeet-engine"

#[cfg(feature = "parakeet-engine")]
fn main() {
    use boothrflow_lib::stt::{ParakeetSttEngine, SttEngine};

    let model_dir = dirs::data_dir()
        .expect("data_dir")
        .join("boothrflow")
        .join("models")
        .join("parakeet-tdt-0.6b-v3");

    eprintln!("probe: loading from {}", model_dir.display());

    let engine = match ParakeetSttEngine::from_model_dir(&model_dir) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("probe: from_model_dir Err: {e}");
            std::process::exit(2);
        }
    };
    eprintln!("probe: engine constructed");

    // Use the bundle's own test_wavs/0.wav if it's been preserved at
    // /tmp/parakeet-orig (the path the probe-time setup writes to).
    // Real speech audio gives the decoder something to chew on instead
    // of triggering NaN / empty-tensor paths in the encoder.
    let test_wav = std::path::Path::new(
        "/tmp/parakeet-orig/sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8/test_wavs/0.wav",
    );
    let audio: Vec<f32> = if test_wav.exists() {
        eprintln!("probe: using {}", test_wav.display());
        let mut reader = hound::WavReader::open(test_wav).expect("open wav");
        reader
            .samples::<i16>()
            .map(|s| s.expect("read sample") as f32 / 32768.0)
            .collect()
    } else {
        eprintln!(
            "probe: no test wav at {}, using a 3s sine wave",
            test_wav.display()
        );
        let sample_rate = 16_000usize;
        let duration_s = 3usize;
        (0..(sample_rate * duration_s))
            .map(|i| {
                let t = i as f32 / sample_rate as f32;
                0.05 * (2.0 * std::f32::consts::PI * 220.0 * t).sin()
            })
            .collect()
    };
    eprintln!("probe: audio samples = {}", audio.len());

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
    eprintln!("probe: build with --features parakeet-engine");
    std::process::exit(1);
}
