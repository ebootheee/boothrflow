//! `audio_meter` — captures from the default microphone for 10 seconds and
//! prints a rolling RMS level in dBFS. The CLI sanity-test for [`CpalAudioSource`].
//!
//! Run with:
//!
//! ```text
//! cargo run --example audio_meter --features real-engines --no-default-features
//! ```
//!
//! Speak into your mic; you should see the dBFS rise from the noise floor
//! (around -60 to -40) up toward -10 to -3 when speaking near it. If it's
//! pinned at -inf, the mic isn't being captured at all.

use std::time::{Duration, Instant};

use boothrflow_lib::audio::{AudioSource, CpalAudioSource};

const CAPTURE_SECS: u64 = 10;
const PRINT_INTERVAL_MS: u64 = 200;
const ROLLING_WINDOW_SAMPLES: usize = 16_000; // 1s at 16kHz

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "boothrflow_lib=info".into()),
        )
        .with_target(false)
        .init();

    println!("audio_meter — capturing {CAPTURE_SECS}s from default mic, printing rolling RMS");
    println!(
        "            (speak into your mic; quiet floor ≈ -60 dBFS, normal speech ≈ -25 to -10)"
    );

    if let Ok(devices) = CpalAudioSource::list_devices() {
        println!();
        for d in &devices {
            let marker = if d.is_default { " (default)" } else { "" };
            println!(
                "  [{:>5} Hz, {} ch] {}{}",
                d.default_sample_rate, d.channels, d.name, marker
            );
        }
        println!();
    }

    let source = CpalAudioSource::new();
    let rx = source.start()?;

    let start = Instant::now();
    let mut sample_count = 0usize;
    let mut last_print = Instant::now();
    let mut window: Vec<f32> = Vec::with_capacity(ROLLING_WINDOW_SAMPLES);

    while start.elapsed() < Duration::from_secs(CAPTURE_SECS) {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(frame) => {
                sample_count += frame.len();
                window.extend_from_slice(&frame);
                if window.len() > ROLLING_WINDOW_SAMPLES {
                    let drop_n = window.len() - ROLLING_WINDOW_SAMPLES;
                    window.drain(..drop_n);
                }
                if last_print.elapsed() >= Duration::from_millis(PRINT_INTERVAL_MS) {
                    let rms = compute_rms(&window);
                    let db = 20.0 * rms.max(1e-9).log10();
                    let bar = level_bar(db);
                    println!("  {db:>7.2} dBFS  {bar}");
                    last_print = Instant::now();
                }
            }
            Err(_) => {
                // timeout — try again
            }
        }
    }

    source.stop()?;
    let secs_captured = sample_count as f32 / 16_000.0;
    println!();
    println!("done. captured {sample_count} samples (~{secs_captured:.2}s of 16kHz audio)");
    if sample_count == 0 {
        eprintln!();
        eprintln!("⚠  no samples received. Common causes:");
        eprintln!(
            "   - Microphone permission not granted (Windows Settings → Privacy → Microphone)"
        );
        eprintln!("   - Mic is in use by another app in exclusive mode");
        eprintln!("   - No default input device set in OS sound settings");
        std::process::exit(1);
    }
    Ok(())
}

fn compute_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f32 = samples.iter().map(|s| s * s).sum();
    (sum_sq / samples.len() as f32).sqrt()
}

/// Render a simple horizontal bar from -80 to 0 dBFS.
fn level_bar(db: f32) -> String {
    let pct = ((db + 80.0) / 80.0).clamp(0.0, 1.0);
    let width = 40;
    let filled = (pct * width as f32).round() as usize;
    let empty = width - filled;
    format!("[{}{}]", "█".repeat(filled), " ".repeat(empty))
}
