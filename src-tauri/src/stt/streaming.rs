//! Streaming Whisper with Local-Agreement-2 stabilization.
//!
//! As the user holds push-to-talk, audio frames arrive on a channel. Every
//! [`PARTIAL_INTERVAL`], we run a fast Whisper pass over the cumulative
//! buffer and emit a partial transcript. Word tokens that match the previous
//! pass are *committed* (will not change); the remaining suffix is the
//! *tentative* tail (subject to revision). This is the Local-Agreement-2
//! algorithm from Polák et al., "CUNI's submission to IWSLT 2022".
//!
//! Trade-offs:
//! - We re-run Whisper on the *entire* buffer each tick (not a sliding
//!   window), which costs O(N) per tick for an N-second utterance. With
//!   tiny.en on CPU this is ~150–250ms for ≤10s — fits in our 800ms tick.
//! - Whisper uses 30s context internally; longer utterances would need a
//!   sliding-window strategy. Defer to a follow-up if dictations regularly
//!   exceed ~25s.
//!
//! The press loop in `session.rs` owns the [`StreamingTranscriber`]; it
//! pushes frames synchronously and ticks `maybe_flush` on its own cadence.
//! The actual whisper call happens on a worker thread so we never block
//! frame intake (cpal's channel is bounded; back-pressure would drop audio).

use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::{bounded, Receiver, Sender};
use parking_lot::Mutex;
use serde::Serialize;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperState};

use crate::error::{BoothError, Result};

/// How often we kick a partial pass. Lower = more responsive, higher =
/// less CPU. 800ms balances both with tiny.en on a modern x86 CPU.
pub const PARTIAL_INTERVAL: Duration = Duration::from_millis(800);

/// Don't bother running a partial below this many samples — Whisper output
/// is unstable on sub-1s clips.
const MIN_PARTIAL_SAMPLES: usize = 16_000; // 1.0s at 16kHz

/// Cap streaming audio length. Beyond this we stop emitting partials —
/// quality degrades past Whisper's 30s context window. The final pass on
/// release still uses the full buffer.
const MAX_STREAMING_SAMPLES: usize = 16_000 * 25; // 25s at 16kHz

/// What the FE pill renders during capture. The committed prefix is fixed;
/// the tentative tail dims to indicate "may still change".
#[derive(Debug, Clone, Serialize)]
pub struct StreamingPartial {
    /// Word tokens both this pass *and* the previous pass agreed on. Stable.
    pub committed: String,
    /// New text since the last commit. May be revised on the next tick.
    pub tentative: String,
    /// Monotonic ms since the dictation started. Lets the FE drop stale
    /// partials if events arrive out of order.
    pub at_ms: u64,
}

/// Worker request: a frozen snapshot of the cumulative audio buffer plus
/// the timestamp to stamp on the resulting partial.
struct PartialRequest {
    audio: Vec<f32>,
    at_ms: u64,
}

/// Inner state shared between the press loop and the worker thread.
struct Inner {
    /// Cumulative PCM since dictation began. Cleared on `reset`.
    buffer: Vec<f32>,
    /// Most recent two partial transcripts' word tokens, used for LA2.
    last_two: Option<(Vec<String>, Vec<String>)>,
    /// Time of the last `maybe_tick` flush — gates [`PARTIAL_INTERVAL`].
    last_tick: Instant,
    /// Once the buffer exceeds [`MAX_STREAMING_SAMPLES`] we stop ticking.
    overlong: bool,
}

pub struct StreamingTranscriber {
    inner: Arc<Mutex<Inner>>,
    request_tx: Sender<PartialRequest>,
    /// Worker output. The press loop drains this each iteration and emits
    /// `dictation:partial` for whatever it finds.
    pub partial_rx: Receiver<StreamingPartial>,
    started: Instant,
}

impl StreamingTranscriber {
    /// Spawn the worker thread and return a handle. `started` is the
    /// monotonic clock zero for `at_ms` stamps in emitted partials.
    pub fn spawn(
        context: Arc<WhisperContext>,
        initial_prompt: Option<String>,
        started: Instant,
    ) -> Result<Self> {
        // Bounded so a slow worker can't memory-bomb us. If the queue fills
        // (unlikely — we only push one per tick), older requests drop.
        let (request_tx, request_rx) = bounded::<PartialRequest>(2);
        let (partial_tx, partial_rx) = bounded::<StreamingPartial>(8);

        let inner = Arc::new(Mutex::new(Inner {
            buffer: Vec::with_capacity(MAX_STREAMING_SAMPLES),
            last_two: None,
            last_tick: Instant::now(),
            overlong: false,
        }));
        let worker_inner = Arc::clone(&inner);

        std::thread::Builder::new()
            .name("boothrflow-stream-stt".into())
            .spawn(move || {
                worker_loop(
                    context,
                    initial_prompt,
                    request_rx,
                    partial_tx,
                    worker_inner,
                );
            })
            .map_err(|e| BoothError::Transcription(format!("stream worker spawn: {e}")))?;

        Ok(Self {
            inner,
            request_tx,
            partial_rx,
            started,
        })
    }

    /// Append captured PCM. Cheap — just copies into the cumulative buffer.
    pub fn push_audio(&self, frame: &[f32]) {
        let mut g = self.inner.lock();
        if g.overlong {
            return;
        }
        g.buffer.extend_from_slice(frame);
        if g.buffer.len() > MAX_STREAMING_SAMPLES {
            g.overlong = true;
            tracing::info!(
                "streaming: buffer over {MAX_STREAMING_SAMPLES} samples — partials disabled"
            );
        }
    }

    /// Try to dispatch a partial pass. Returns true if a request was sent.
    /// Cheap when no work is due (just a clock check).
    pub fn maybe_tick(&self) -> bool {
        let snapshot: Option<Vec<f32>> = {
            let mut g = self.inner.lock();
            if g.overlong {
                return false;
            }
            if g.last_tick.elapsed() < PARTIAL_INTERVAL {
                return false;
            }
            if g.buffer.len() < MIN_PARTIAL_SAMPLES {
                return false;
            }
            g.last_tick = Instant::now();
            Some(g.buffer.clone())
        };

        let Some(audio) = snapshot else {
            return false;
        };
        let at_ms = self.started.elapsed().as_millis() as u64;
        // try_send so we never block the press loop. If the worker is
        // backed up, drop this tick — the next one will catch us up.
        let _ = self.request_tx.try_send(PartialRequest { audio, at_ms });
        true
    }
}

/// Worker loop: receive snapshots, run Whisper, emit partials.
fn worker_loop(
    context: Arc<WhisperContext>,
    initial_prompt: Option<String>,
    requests: Receiver<PartialRequest>,
    partials: Sender<StreamingPartial>,
    inner: Arc<Mutex<Inner>>,
) {
    while let Ok(req) = requests.recv() {
        let started = Instant::now();
        let text = match run_partial(&context, initial_prompt.as_deref(), &req.audio) {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!("streaming whisper pass failed: {e}");
                continue;
            }
        };
        let elapsed = started.elapsed().as_millis() as u64;

        let tokens: Vec<String> = text.split_whitespace().map(|s| s.to_string()).collect();

        let (committed, tentative) = {
            let mut g = inner.lock();
            let prev = g.last_two.as_ref().map(|(_, p1)| p1.as_slice());
            let (c, t) = local_agreement_2(prev, &tokens);
            // Slide the LA2 window forward.
            g.last_two = Some(match g.last_two.take() {
                Some((_, prev1)) => (prev1, tokens.clone()),
                None => (Vec::new(), tokens.clone()),
            });
            (c, t)
        };

        tracing::debug!(
            "streaming partial ({elapsed}ms): committed=\"{committed}\" tentative=\"{tentative}\""
        );

        let _ = partials.try_send(StreamingPartial {
            committed,
            tentative,
            at_ms: req.at_ms,
        });
    }
}

/// Local-Agreement-2: given the previous pass's tokens (or `None` on the
/// first pass), return `(committed, tentative)` for the current pass.
/// The committed string is the longest common word-token prefix; the
/// tentative string is the remainder of the current pass.
fn local_agreement_2(prev: Option<&[String]>, current: &[String]) -> (String, String) {
    let n = match prev {
        Some(p) => current
            .iter()
            .zip(p.iter())
            .take_while(|(a, b)| a == b)
            .count(),
        None => 0,
    };
    let committed = current[..n]
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(" ");
    let tentative = current[n..]
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(" ");
    (committed, tentative)
}

/// Run a single Whisper pass tuned for partial latency rather than peak
/// quality. Greedy decode, single beam, suppress blank — same as the final
/// pass; whisper.cpp doesn't expose much else to tune for short partials.
fn run_partial(
    context: &WhisperContext,
    initial_prompt: Option<&str>,
    audio: &[f32],
) -> Result<String> {
    let mut state: WhisperState = context
        .create_state()
        .map_err(|e| BoothError::Transcription(format!("create_state: {e}")))?;

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    // Leave one core for the rest of the pipeline (capture + UI thread)
    // so partials don't starve frame intake on lower-core machines.
    let threads = std::thread::available_parallelism()
        .map(|n| (n.get() as i32).saturating_sub(1).max(1))
        .unwrap_or(2);
    params.set_n_threads(threads);
    params.set_translate(false);
    params.set_language(Some("en"));
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    params.set_suppress_blank(true);
    if let Some(prompt) = initial_prompt {
        params.set_initial_prompt(prompt);
    }

    state
        .full(params, audio)
        .map_err(|e| BoothError::Transcription(format!("full: {e}")))?;

    let mut text = String::new();
    for segment in state.as_iter() {
        text.push_str(&segment.to_string());
    }
    Ok(text.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::local_agreement_2;

    fn toks(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn first_pass_has_no_committed() {
        let (committed, tentative) = local_agreement_2(None, &toks("hello world"));
        assert_eq!(committed, "");
        assert_eq!(tentative, "hello world");
    }

    #[test]
    fn full_agreement_commits_everything() {
        let prev = toks("the quick brown");
        let cur = toks("the quick brown");
        let (committed, tentative) = local_agreement_2(Some(&prev), &cur);
        assert_eq!(committed, "the quick brown");
        assert_eq!(tentative, "");
    }

    #[test]
    fn shared_prefix_commits_only_prefix() {
        let prev = toks("the quick brown fox");
        let cur = toks("the quick brown dog jumps");
        let (committed, tentative) = local_agreement_2(Some(&prev), &cur);
        assert_eq!(committed, "the quick brown");
        assert_eq!(tentative, "dog jumps");
    }

    #[test]
    fn divergent_first_token_commits_nothing() {
        let prev = toks("hello world");
        let cur = toks("greetings world");
        let (committed, tentative) = local_agreement_2(Some(&prev), &cur);
        assert_eq!(committed, "");
        assert_eq!(tentative, "greetings world");
    }

    #[test]
    fn current_extends_previous_commits_overlap_only() {
        let prev = toks("hello there");
        let cur = toks("hello there friend");
        let (committed, tentative) = local_agreement_2(Some(&prev), &cur);
        assert_eq!(committed, "hello there");
        assert_eq!(tentative, "friend");
    }

    #[test]
    fn current_shorter_than_previous_still_works() {
        // Whisper occasionally truncates between passes — LA2 should not panic.
        let prev = toks("the quick brown fox jumps");
        let cur = toks("the quick");
        let (committed, tentative) = local_agreement_2(Some(&prev), &cur);
        assert_eq!(committed, "the quick");
        assert_eq!(tentative, "");
    }

    #[test]
    fn case_sensitive_matching() {
        // Whisper sometimes flips capitalization mid-decode. We treat
        // those as distinct tokens — first divergence kills the commit.
        let prev = toks("Hello world");
        let cur = toks("hello world");
        let (committed, tentative) = local_agreement_2(Some(&prev), &cur);
        assert_eq!(committed, "");
        assert_eq!(tentative, "hello world");
    }
}
