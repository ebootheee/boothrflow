# Wave 2 — Speed + Pill UX (status: shipped, awaiting UAT)

Wave 2 picks up after the W1–W8 phases delivered the working dictation
pipeline (audio → whisper → llm → paste → searchable history). It is
focused on two complaints that surfaced during real usage:

1. **The pill disappears the instant the user releases the key**, even
   though Whisper + LLM cleanup can take 1–2 s before paste lands. The
   user is left staring at nothing, unsure if the system is still working.
2. **No streaming feedback**. Wispr Flow shows partial text while you
   speak. We were silent until the final transcript landed.

Wave 2 has four work items. All four are merged on the `feat/wave-2`
branch. Wave 2 UAT (the human-in-the-loop validation pass) is the
remaining gate before merging to `main` and starting Wave 3.

---

## #1 — Pill persists past key release + state machine event contract

**File:** `src-tauri/src/session.rs`

The session daemon's press loop now emits a `dictation:state` event at
each lifecycle transition. The pill window stays visible from
`listening` through `pasting` and only hides when `idle` fires.

```
listening → transcribing → cleaning → pasting → idle
```

Each event carries `{ state, at_ms }`. `at_ms` is monotonic milliseconds
since the _current_ dictation began (resets on every key press). The FE
pill subscribes to this single event to drive its visual state machine
(pulsing dot for listening, spinner for transcribing, sparkles for
cleaning, paste-icon for pasting).

The `cleaning` and `pasting` stages are skipped if the relevant
subsystem is disabled (no LLM configured → no `cleaning`; injector
init failed → no `pasting`). `idle` always fires last.

## #2 — Per-stage latency telemetry on `dictation:done`

**File:** `src-tauri/src/session.rs`

After paste completes (or on early empty-transcript exit), the daemon
emits one `dictation:done` event with the full timing breakdown:

```ts
{
  formatted: string; // final pasted text
  capture_ms: number; // press → release
  stt_ms: number; // Whisper full pass
  llm_ms: number; // LLM cleanup (0 if skipped)
  paste_ms: number; // clipboard injection
  total_ms: number; // press → idle
}
```

Also logged at INFO so a single dictation produces one timing line in
the daemon log:

```
dictation:done capture=1820ms stt=380ms llm=510ms paste=18ms total=2731ms
```

The Settings page can graph these over time to show the user where
their wall-clock time is going. The FE store exposes them as
`dictationStore.lastDone`.

## #3 — GPU Whisper feature flags (`gpu-vulkan` / `gpu-cuda` / `gpu-metal`)

**File:** `src-tauri/Cargo.toml`

Three opt-in features forward to the matching `whisper-rs` /
`whisper.cpp` GPU backend:

| Feature      | Backend | Targets         | SDK requirement               |
| ------------ | ------- | --------------- | ----------------------------- |
| `gpu-vulkan` | Vulkan  | Windows + Linux | `VULKAN_SDK` env var at build |
| `gpu-cuda`   | CUDA    | Windows + Linux | CUDA Toolkit at build         |
| `gpu-metal`  | Metal   | macOS only      | None (built into Xcode CLT)   |

Build example (Windows, Vulkan, NVIDIA / AMD / Intel Arc):

```
scripts\cargo-msvc.bat cargo build --release \
  --features "real-engines gpu-vulkan"
```

These are **not** in `default` because they require an SDK. The default
build stays self-contained and runs CPU-only Whisper. Users with the
SDK installed flip a feature flag and get a 2–10× speedup on the STT
stage with no code change.

The Vulkan path is also the right one for Wave 3's Linux port — same
flag works there. Metal is the macOS path (Wave 3).

## #4 — Streaming Whisper with Local-Agreement-2

**Files:** `src-tauri/src/stt/streaming.rs` (new),
`src-tauri/src/stt/whisper.rs`, `src-tauri/src/stt/mod.rs`,
`src-tauri/src/session.rs`

While the user holds push-to-talk, the press loop now pushes captured
PCM into a `StreamingTranscriber` that runs Whisper on the cumulative
buffer every 800 ms on a worker thread. Each pass produces a partial
transcript; consecutive passes are compared word-by-word and the
longest common prefix is _committed_ (will not change). The remaining
suffix is _tentative_ (subject to revision on the next pass).

```ts
// emitted multiple times per dictation
'dictation:partial' { committed: string, tentative: string, at_ms: number }
```

The FE pill renders `committed` solid and `tentative` dimmed; users see
text appear ~800ms behind their voice instead of waiting for the full
release-then-decode round-trip.

The final pass on release still produces the authoritative transcript
via the existing path — partials are advisory. If the streaming worker
fails to spawn (rare), partials silently disable and the rest of the
pipeline is unaffected.

**Constraints baked in:**

- Below 1 s of audio: no partial fires (Whisper output is too unstable).
- Above 25 s of audio: partials disable (Whisper's 30 s context window).
- Worker thread uses `n_cores - 1` threads to leave headroom for the
  capture thread on lower-core machines.
- The partial channel uses `try_send`; if the FE is back-pressured,
  older partials drop. The next tick catches up.

**Tests:** 7 unit tests in `streaming::tests` cover the LA2 prefix
logic (no-prev, full agreement, partial overlap, divergence, current
shorter than previous, case sensitivity).

---

## Verification (already passed before commit)

- `cargo nextest run --features real-engines --lib` → 26 / 26 pass
- `cargo clippy --features real-engines --lib -- -D warnings` → clean
- `pnpm exec tsc --noEmit` → clean (excluding `_spike/`)

## Wave 2 UAT — what to test on the running app

Pre-flight:

1. `pnpm tauri dev` — launches with the daemon spinning up.
2. `ollama serve` running with `qwen2.5:3b-instruct-q4_K_M` (or your
   preferred LLM) pulled.
3. Whisper model (`ggml-tiny.en.bin`) at
   `%APPDATA%/boothrflow/models/`.

UAT flow:

1. **Pill persistence.** Hold Ctrl+Win, dictate a sentence, release.
   The pill should stay visible through transcription + cleanup +
   paste, then hide. Confirm it doesn't blink off mid-flow.
2. **State transitions.** Open DevTools on the pill window. Listen on
   `dictation:state` and confirm you see all five states fire in order:
   `listening → transcribing → cleaning → pasting → idle`.
3. **Done telemetry.** Confirm one `dictation:done` event per dictation
   with reasonable timings — capture matches your hold duration; stt
   in the 200–800 ms range for tiny.en CPU on a short utterance; llm
   under 1 s after first dictation (prewarm); paste under 100 ms.
4. **Streaming partials.** Hold the key longer (3–5 s). Watch the pill.
   Text should start appearing ~1 s after you start speaking, growing
   as you continue. The committed prefix should stop changing once
   it's been seen on two consecutive passes.
5. **GPU build (optional).** If you have Vulkan SDK installed:
   ```
   scripts\cargo-msvc.bat cargo build --release \
     --features "real-engines gpu-vulkan"
   ```
   Compare `stt_ms` in `dictation:done` before/after. Expect 2–10×
   reduction.

## Known Wave 2 limitations (deferred)

- Streaming caps at 25 s of audio. Long utterances stop emitting
  partials past that point but the final pass still works on the full
  buffer. A sliding-window strategy is a Wave 4+ refinement.
- The LA2 algorithm does word-level matching. Whisper occasionally
  flips capitalization or punctuation between passes; those are treated
  as different tokens, killing the commit at that point. Acceptable
  for now — the next pass typically resolves it.
- No partial-text rendering inside the _target_ application. Wispr
  also doesn't do this; SendInput streaming breaks IMEs and feels
  janky. Partials live in the pill only.
