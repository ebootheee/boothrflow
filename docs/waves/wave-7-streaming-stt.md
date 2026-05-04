# Wave 7 — Streaming STT + Native Runtime

**Goal:** keep Parakeet's transcription quality (which we just confirmed
beats Whisper-tiny + Whisper-base on the Lysara capture in Wave 5
benchmarking) while closing two gaps the offline 0.6B-v2 ONNX bundle
opened up:

1. **No live preview while talking.** Users type-pause-watch on
   Whisper; with Parakeet the pill stays empty until release. UX
   regression we papered over by emitting a synthesized partial on
   `dictation:result`, but that's a fake — there's no real
   incremental output to stream.
2. **Cold-start latency is structural.** Sherpa-onnx + 3 ONNX files
   loaded on every dictation → ~13.5s for a 116s capture in our
   bench, and that number was _consistent across runs_, so it's not
   a warm-up artifact. The ONNX runtime overhead is real.

Two parallel tracks address each. Both can ship independently; we'll
pick one as default after benchmarking against the existing captures
under the new dev-mode flow.

---

## Why this matters

Wave 5 made Parakeet usable but exposed two reasons we can't ship it
as the _only_ engine without losing the live-preview UX users got
from Whisper. After this wave we have a path to **Parakeet quality +
Whisper-style streaming + faster than either**, all on local
inference, no cloud round-trip.

Plus: the `BOOTHRFLOW_DEV=1` flag landed in Wave 5 makes the bench
harness a permanent part of how we evaluate engine swaps. Every
candidate goes through the same `bench:replay` + grading UI on the
same captured wavs. No more "trust me bro, X is faster."

---

## Phase 1 — Nemotron Speech Streaming via sherpa-onnx (3–5 days)

The strategically obvious upgrade. Same parameter scale as our
current offline Parakeet (~600M), already exported to ONNX by NVIDIA
([nvidia/nemotron-speech-streaming-en-0.6b](https://huggingface.co/nvidia/nemotron-speech-streaming-en-0.6b)),
encoder.onnx + decoder_joint.onnx with int8/int4 quantization options.
Cache-aware streaming with configurable chunk sizes (80–1120 ms).

### Deliverables

- **New STT model entry**: `nemotron-speech-streaming-0.6b` in
  `settings::whisper_models()`, available under the `parakeet-engine`
  feature (sherpa-onnx already linked). Different model dir, separate
  download script entry.
- **Download script extension**: `scripts/download-model.sh` learns a
  `nemotron-streaming` argument that pulls encoder.onnx +
  decoder_joint.onnx + tokens.txt from HuggingFace, runs the same
  metadata-propagation pass we built for Parakeet (one of those ONNX
  files probably also needs `vocab_size` propagated — check before
  shipping).
- **Streaming integration in `src-tauri/src/stt/`**: new
  `nemotron_streaming.rs` that wraps sherpa-onnx's streaming
  recognizer (`OnlineRecognizer` rather than `OfflineRecognizer`).
  Emits incremental tokens via the same `dictation:partial` event the
  Whisper streaming path uses, so the pill picks them up with no FE
  changes.
- **Engine routing**: `session.rs`'s STT selection branches on
  `model_value == "nemotron-speech-streaming-0.6b"` and constructs
  the streaming engine. Whisper streaming, Parakeet offline, Nemotron
  streaming — three real paths.
- **Settings UI label**: `NVIDIA Nemotron Speech Streaming 0.6B —
live preview (preview)` to match our existing Whisper labels.

### Open questions

- Does sherpa-onnx 0.6.8 (the version we pinned for offline Parakeet)
  support `OnlineRecognizer` for the Nemotron config, or do we need a
  newer release? Validate before commitiing to a sherpa-rs version
  bump.
- Chunk size tuning: 80 ms is the absolute lowest; latency-vs-context
  trade-off should be measured against real captures, not picked
  blind.

### Acceptance

- Pill shows live transcript during PTT (LA2-style finalize behavior
  fine; whatever sherpa-onnx hands us).
- Final transcript graded against the Lysara capture's existing
  Parakeet variant. Target: ≥ same grade as offline Parakeet.

---

## Phase 2 — parakeet.cpp evaluation (2–3 days)

[Frikallo/parakeet.cpp](https://github.com/Frikallo/parakeet.cpp) is
a C++ implementation of Parakeet with Metal acceleration via the
Axiom tensor library (not GGML). Supports TDT 0.6B + EOU 120M +
Nemotron 600M from the same family. Promises native Apple Silicon
performance vs the ONNX runtime layer we currently wear.

This is an _evaluation_ phase — we don't commit to swapping until
the bench numbers prove it.

### Deliverables

- **Vendored fork or git submodule** of parakeet.cpp at a pinned
  commit. Build from source as part of the macOS build.
- **`stt/parakeet_cpp.rs`** module behind a new feature flag
  `parakeet-cpp-engine` (parallel to existing `parakeet-engine`).
  FFI wrapper over parakeet.cpp's C API, same `SttEngine` trait
  impl as the sherpa-onnx-backed `ParakeetSttEngine`.
- **Bench integration**: `bench_replay` discovers the parakeet.cpp
  variant alongside the others when its model files are present.
  Same captures, same LLM cleanup pass, third row in the variants
  table.
- **Decision doc**: a short `docs/notes/parakeet-cpp-vs-onnx.md`
  recording the bench numbers and the call. If sherpa-onnx + Metal
  is within 20% of parakeet.cpp + Metal, stick with sherpa-onnx
  (one less dep). If parakeet.cpp wins by a meaningful margin
  (>2x?) on the load+decode time we measured (13.5s on the Lysara
  wav), swap it in for macOS production builds.

### Open questions

- Linux + Windows story for parakeet.cpp. If Metal is the only
  acceleration backend, we still need sherpa-onnx for non-Apple
  builds. Two STT backends to maintain is a real cost.
- Build complexity: parakeet.cpp pulls Axiom which may have its
  own build dance. Compare to "sherpa-rs just downloads a binary."

### Acceptance

- Same Lysara capture grades ≥ same as sherpa-onnx Parakeet on raw
  text quality.
- Median STT latency ≤ 50% of sherpa-onnx Parakeet on the same
  captures (post-warmup, N=3 runs, median).

---

## Phase 3 — Bench harness hardening (1 day)

Eric flagged in Wave 5 that the harness was confounded by cold-start
ordering — first-tested engine ate the model-load cost. Fix that as
part of this wave so the Phase 1 + 2 numbers are trustworthy.

### Deliverables

- **Warmup pass**: each STT engine gets one throwaway transcribe
  call (1-second silence buffer is fine) before the timed runs.
- **N=3 with median**: each timed run repeats 3 times, harness
  records the median rather than the single sample. Variance also
  recorded so we know if a config is unstable.
- **Aggregate grading view**: in the Benchmarks tab, add a "across
  all captures" leaderboard alongside the per-capture one. Helps
  pick the default once we have ≥ 5 graded captures.

### Acceptance

- Cold-start asymmetry < 10% between first-tested and
  third-tested STT engines.
- The variants JSON schema doesn't break — backwards-compatible
  evolution. (Add `stt_ms_runs: [u64; 3]` alongside the existing
  `stt_ms`; old files without the new field still load.)

---

## Risks + mitigations

| Risk                                                                                    | Impact                | Mitigation                                                                                         |
| --------------------------------------------------------------------------------------- | --------------------- | -------------------------------------------------------------------------------------------------- |
| Nemotron Streaming ONNX has a vocab-size or context-size metadata gap like Parakeet did | Med                   | Apply the same `parakeet-propagate-metadata.py` script; we already know that pattern               |
| sherpa-rs 0.6.8 doesn't support OnlineRecognizer for Nemotron                           | High (blocks Phase 1) | Pin a newer sherpa-rs first, validate offline Parakeet still loads, _then_ layer streaming         |
| parakeet.cpp build is fragile or unmaintained                                           | Med                   | Pin a specific commit; keep sherpa-onnx as the production default until parakeet.cpp wins on bench |
| Bench harness changes break existing variants files                                     | Low                   | Additive schema (`stt_ms_runs` is new); existing `grade` / `notes` fields preserved                |

---

## Out of scope (revisited later)

- Realtime EOU 120M as a VAD replacement. Worth a side-eval after
  Phase 1 lands — pairs with streaming for the "always-listening"
  story but isn't required for this wave's quality+streaming goal.
- Multitalker Parakeet. Different use case (meetings); the dictation
  product doesn't need speaker labels.
- Canary translation models. Future "Translate to Spanish" preset, not
  blocking.

---

## Bench-driven default selection

After Phase 1 + Phase 2 land, the default STT for `parakeet-engine`
production builds gets re-decided based on graded variants across
≥ 3 captures. Candidates:

- **Nemotron Speech Streaming 0.6B** (Phase 1) — adds streaming
- **parakeet.cpp + TDT 0.6B v2** (Phase 2) — adds latency win
- **sherpa-onnx + TDT 0.6B v2** (current Wave 5 default)

The leaderboard mean grade picks the winner. Tie goes to whichever
is simpler to maintain.
