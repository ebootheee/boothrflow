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
//! - Whisper uses 30s context internally; we keep the live buffer below
//!   that with a "commit-and-roll" loop. When the buffer crosses
//!   [`ROLL_THRESHOLD_SAMPLES`], the LA2-stable prefix is moved to a
//!   separate `frozen_text` field and the audio is trimmed to a small
//!   overlap (`ROLL_KEEP_SAMPLES`). The worker prepends `frozen_text` to
//!   every emitted partial, with a suffix-prefix dedup against the
//!   overlap. Net effect: indefinitely-long dictations stay responsive
//!   and accurate without exceeding Whisper's context window.
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

/// When the streaming buffer crosses this length, perform a "commit-and-roll":
/// freeze the most recent stable LA2 prefix into `frozen_text`, trim the live
/// audio buffer to `ROLL_KEEP_SAMPLES` of overlap, and continue ticking. This
/// keeps per-tick Whisper compute bounded while supporting indefinitely long
/// dictations.
const ROLL_THRESHOLD_SAMPLES: usize = 16_000 * 20; // 20s at 16kHz

/// How much audio to retain after a roll, as overlap context for the next
/// transcription window. ~3s gives Whisper enough surrounding context to
/// produce coherent output at the boundary; less risks a stutter, more
/// inflates the post-roll prompt-eval cost. The matching tail of
/// `frozen_text` is then de-duplicated against the next pass's head — see
/// `dedupe_suffix_prefix`.
const ROLL_KEEP_SAMPLES: usize = 16_000 * 3; // 3s at 16kHz

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
    /// Cumulative PCM since the most recent roll (or dictation start, if no
    /// roll has happened yet). The live transcription window operates on
    /// this buffer alone; older audio's transcript lives in `frozen_text`.
    buffer: Vec<f32>,
    /// Most recent two partial transcripts' word tokens, used for LA2.
    /// Cleared on every roll so the LA2 algorithm restarts cleanly with
    /// the post-roll buffer.
    last_two: Option<(Vec<String>, Vec<String>)>,
    /// Time of the last `maybe_tick` flush — gates [`PARTIAL_INTERVAL`].
    last_tick: Instant,
    /// LA2-stable prefix accumulated across all prior rolls. Each roll
    /// appends the most recent committed string to this buffer, then
    /// resets `last_two` and trims `buffer`. The worker prepends this to
    /// every emitted partial so the FE pill renders the entire dictation
    /// from start to current.
    frozen_text: String,
    /// Most recent committed string from the worker. Captured here so the
    /// `maybe_tick` rolling logic can freeze it without coordinating
    /// directly with the worker thread.
    last_committed: String,
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
            buffer: Vec::with_capacity(ROLL_THRESHOLD_SAMPLES + ROLL_KEEP_SAMPLES),
            last_two: None,
            last_tick: Instant::now(),
            frozen_text: String::new(),
            last_committed: String::new(),
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
        g.buffer.extend_from_slice(frame);
    }

    /// Try to dispatch a partial pass. Returns true if a request was sent.
    /// Cheap when no work is due (just a clock check).
    ///
    /// Performs the commit-and-roll inline: when the live buffer exceeds
    /// [`ROLL_THRESHOLD_SAMPLES`], the most recent committed text is
    /// appended to `frozen_text`, the LA2 window is reset, and the buffer
    /// is trimmed to the last [`ROLL_KEEP_SAMPLES`] of audio (~3s overlap).
    /// The next tick re-transcribes the overlap as fresh audio; the worker
    /// de-dupes the suffix-prefix so the displayed prefix doesn't double
    /// up on the boundary words.
    pub fn maybe_tick(&self) -> bool {
        let snapshot: Option<Vec<f32>> = {
            let mut g = self.inner.lock();

            // Roll the buffer forward when it crosses the threshold. We do
            // this BEFORE the time/length gates so a long quiet stretch
            // doesn't accumulate unbounded audio between ticks.
            if g.buffer.len() >= ROLL_THRESHOLD_SAMPLES {
                let frozen_chunk = std::mem::take(&mut g.last_committed);
                if !frozen_chunk.is_empty() {
                    if !g.frozen_text.is_empty() {
                        g.frozen_text.push(' ');
                    }
                    g.frozen_text.push_str(&frozen_chunk);
                }
                let drop_to = g.buffer.len().saturating_sub(ROLL_KEEP_SAMPLES);
                g.buffer.drain(..drop_to);
                g.last_two = None;
                tracing::info!(
                    "streaming: rolled buffer (frozen len = {} chars, kept {} samples of audio)",
                    g.frozen_text.len(),
                    g.buffer.len(),
                );
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

        // Compute LA2 commit + cache it for the next roll, all under a single
        // lock so the rolling logic sees a consistent view.
        let (display_committed, tentative) = {
            let mut g = inner.lock();
            let prev = g.last_two.as_ref().map(|(_, p1)| p1.as_slice());
            let (committed, tentative) = local_agreement_2(prev, &tokens);
            // Slide the LA2 window forward.
            g.last_two = Some(match g.last_two.take() {
                Some((_, prev1)) => (prev1, tokens.clone()),
                None => (Vec::new(), tokens.clone()),
            });
            // Cache the committed prefix so `maybe_tick` can freeze it on
            // the next roll without re-running Whisper.
            g.last_committed = committed.clone();

            // Build the display prefix: prior frozen text + new committed,
            // with suffix-prefix de-dup so the audio overlap kept after the
            // last roll doesn't double-print boundary words.
            let display = if g.frozen_text.is_empty() {
                committed
            } else {
                let deduped = dedupe_suffix_prefix(&g.frozen_text, &committed);
                if deduped.is_empty() {
                    g.frozen_text.clone()
                } else {
                    format!("{} {}", g.frozen_text, deduped)
                }
            };
            (display, tentative)
        };

        tracing::debug!(
            "streaming partial ({elapsed}ms): committed=\"{display_committed}\" tentative=\"{tentative}\""
        );

        let _ = partials.try_send(StreamingPartial {
            committed: display_committed,
            tentative,
            at_ms: req.at_ms,
        });
    }
}

/// Trim the prefix of `fresh` that already appears as a suffix of `frozen`.
///
/// After a roll, the streaming buffer keeps ~3s of overlap audio whose
/// transcription approximately matches the tail of `frozen_text`. Without
/// this dedup, the display would read "...quick brown fox brown fox jumped"
/// — the same words show up at the end of frozen and the start of the new
/// committed string. We greedily find the longest suffix-of-frozen / prefix-
/// of-fresh match (case-insensitive, punctuation-stripped) up to a small
/// window, and strip that prefix from `fresh`.
fn dedupe_suffix_prefix(frozen: &str, fresh: &str) -> String {
    if frozen.is_empty() || fresh.is_empty() {
        return fresh.to_string();
    }
    let frozen_tokens: Vec<&str> = frozen.split_whitespace().collect();
    let fresh_tokens: Vec<&str> = fresh.split_whitespace().collect();
    // The audio overlap is ~3s; rarely more than ~10 words. Capping at 15
    // bounds the comparison cost and avoids spurious matches deeper in.
    let max_overlap = frozen_tokens.len().min(fresh_tokens.len()).min(15);

    let normalize = |t: &str| -> String {
        t.chars()
            .filter(|c| c.is_alphanumeric())
            .map(|c| c.to_ascii_lowercase())
            .collect()
    };

    for k in (1..=max_overlap).rev() {
        let frozen_tail: Vec<String> = frozen_tokens[frozen_tokens.len() - k..]
            .iter()
            .map(|t| normalize(t))
            .collect();
        let fresh_head: Vec<String> = fresh_tokens[..k].iter().map(|t| normalize(t)).collect();
        if frozen_tail == fresh_head {
            return fresh_tokens[k..].join(" ");
        }
    }
    fresh.to_string()
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
    use super::{dedupe_suffix_prefix, local_agreement_2};

    fn toks(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn dedupe_strips_overlap() {
        // Frozen tail "fox jumped over" matches the head of fresh.
        let out = dedupe_suffix_prefix(
            "the quick brown fox jumped over",
            "fox jumped over the lazy dog",
        );
        assert_eq!(out, "the lazy dog");
    }

    #[test]
    fn dedupe_handles_full_overlap() {
        // The entire fresh string is already in the frozen tail.
        let out = dedupe_suffix_prefix("alpha bravo charlie", "bravo charlie");
        assert_eq!(out, "");
    }

    #[test]
    fn dedupe_no_overlap_passes_through() {
        let out = dedupe_suffix_prefix("the quick brown", "fox jumped over");
        assert_eq!(out, "fox jumped over");
    }

    #[test]
    fn dedupe_is_case_and_punct_insensitive() {
        // Whisper sometimes flips capitalization between passes; the dedupe
        // should still find the overlap.
        let out = dedupe_suffix_prefix("ran the deploy", "Ran the deploy, then waited");
        assert_eq!(out, "then waited");
    }

    #[test]
    fn dedupe_caps_lookback() {
        // Beyond the 15-token cap we don't try to match — keeps the helper
        // O(window²) at most.
        let frozen = (0..30)
            .map(|i| format!("w{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        let fresh = format!("{frozen} extra");
        // The full-string overlap is 30 tokens but we only check 15; with
        // no match found within the window, fresh passes through unchanged.
        let out = dedupe_suffix_prefix(&frozen, &fresh);
        assert_eq!(out, fresh);
    }

    #[test]
    fn dedupe_empty_inputs_are_safe() {
        assert_eq!(dedupe_suffix_prefix("", "fresh text"), "fresh text");
        assert_eq!(dedupe_suffix_prefix("frozen text", ""), "");
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
