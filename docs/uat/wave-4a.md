# Wave 4a UAT — Polish + Telemetry + Captain's Log

**Date:** 2026-04-29
**Branch:** `main` (committed direct, no separate feature branch)
**Reviewer:** Claude
**Verdict:** **Ready for on-Mac UAT.** Five features landed in commit `28d4f52` on top of the Wave 3 polish. Awaiting Eric's runtime verification before kicking off Wave 4b (Settings panel).

---

## What landed

### 1. Whisper vocabulary expansion (`stt/whisper.rs`)

Appended project-specific proper nouns to `DEFAULT_INITIAL_PROMPT`:
Qwen, Wispr, Boothe, boothrflow, Whisper, FluidAudio, Parakeet,
sherpa-onnx, llama.cpp, MTLDevice, Metal, CoreML, Apple Silicon,
M-series, Apple Vision, ScreenCaptureKit, WhisperKit, RNNoise,
DeepFilterNet, sqlite-vec, FTS5, nomic-embed, stardate.

Whisper now has a prior on these; the "Qwen → kWEN" class of
recognition misses from Wave 3 should drop substantially.

### 2. Cleanup aggressiveness flag (`settings.rs`, `llm/openai_compat.rs`)

`Style::aggressiveness()` returns 0 for Raw, 1 for everything else.
The cleanup system prompt now contains graded instructions:

- **0 (Raw):** "Preserve every word the speaker said exactly. Do not drop fillers, do not paraphrase."
- **1 (default):** "Drop disfluencies (uh, um, you know, I mean, like as filler), false starts, and self-corrections — when the speaker says 'go to the store, I mean the office', output 'go to the office'. Do not paraphrase or shorten otherwise."

This closes the Wave 3 "mumbling / rambling came through" UAT gap.
The `<corrected>` reflection tier we previously planned is folded in
as a one-liner in the prompt: _"If a transcribed word is acoustically
plausible but semantically nonsensical given the surrounding context,
replace it with the most likely intended word."_ — full reflection /
OCR-window-context lands in Wave 4b.

### 3. tok/s telemetry (`llm/{mod,openai_compat}.rs`, `session.rs`, FE)

`LlmCleanup` trait was changed to return `CleanupOutput { text,
prompt_tokens, completion_tokens, elapsed_ms }` instead of bare
`String`. The OpenAI-compat backend parses Ollama's `usage` field;
the fake leaves token counts as `None`.

Three new optional fields on `dictation:done`:
`llm_prompt_tokens`, `llm_completion_tokens`, `llm_tok_per_sec`.
The cleanup chip in the topbar shows `350 ms · 100 tok/s` when
present, falling back to bare ms when the backend doesn't report
usage. `null` means "no data" (distinct from `0`).

### 4. Captain's Log style (cross-cutting)

Sixth Style enum variant, sixth dropdown entry. Bespoke cleanup
prompt that:

- prepends `Captain's log, stardate <X>` (TNG-era approximation
  computed from current real-world date)
- rewrites the body in 24th-century formal tone
- ends with `End log.`
- is bounded against inventing ship names / canon characters /
  numeric stardate prefixes

The fake LLM gets the same opener/closer for the web smoke path so
non-Tauri users see the format too.

### 5. Streaming partial continuation (`stt/streaming.rs`)

The Wave 3 UAT carry-over. Replaces the 25 s hard cap with a
commit-and-roll loop:

- When the live audio buffer crosses `ROLL_THRESHOLD_SAMPLES` (20s),
  the most recent LA2-stable prefix is moved from `last_committed`
  into `Inner.frozen_text`.
- The audio buffer trims to `ROLL_KEEP_SAMPLES` (3s) of overlap.
- LA2 state resets so the new transcription window starts fresh.
- Worker prepends `frozen_text` to every emitted partial.
- A `dedupe_suffix_prefix` helper strips the audio overlap from the
  new pass's output so "fox jumped" doesn't show up twice at the
  boundary. Case- and punctuation-insensitive matching covers
  Whisper's habit of re-capitalizing between passes.

Six new tests cover the dedup helper's overlap, full-overlap,
no-overlap, case-insensitive, lookback-cap, and empty-input cases.

## Test matrix

| Check                                                           | Result   |
| --------------------------------------------------------------- | -------- |
| `pnpm check` (types / lint / format / clippy fakes + real)      | ✅       |
| `pnpm test:fe`                                                  | ✅ 7/7   |
| `cargo nextest run --no-default-features --features test-fakes` | ✅ 23/23 |
| `cargo nextest run --features real-engines`                     | ✅ 42/42 |

## What's still on the human

1. **On-Mac UAT** — exercise each of the five items:
   - Dictate a sentence with "Qwen", "boothrflow", "Apple Silicon" — should render correctly without spelling them out.
   - Dictate with rambling / "uh" / "you know" — Casual style should drop them; Raw should keep them.
   - Watch the cleanup chip during a normal dictation — should see `Xms · Y tok/s`.
   - Pick "Captain's Log" from the Style dropdown, dictate something normal, see the log-entry rewrite.
   - Hold dictation past the 25 s mark — pill should keep scrolling instead of freezing.

2. **Cross-platform sanity** (Windows whenever convenient) — none of the Wave 4a code is platform-gated, so all five features should work identically on Windows.

## What's next: Wave 4b

In-app Settings panel (Whisper model picker, LLM endpoint/model/key,
hotkey rebind, vocabulary editor, privacy mode toggle). Persists via
`tauri-plugin-store`. The Qwen 7B default bump rides on top of this
once it lands. ~2-3 days of focused work.

After Wave 4b ships: Wave 5 — context-aware cleanup (OCR window
context, auto-learning correction store, prompt prefix caching). The
big leverage round.
