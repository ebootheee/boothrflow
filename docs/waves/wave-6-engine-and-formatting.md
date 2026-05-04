# Wave 6 — Engine + formatting

**Goal:** dial in the engine _and_ the cleanup pass before we touch
production packaging. Two parallel tracks:

1. **Replace the tone-based style system** (casual / formal /
   very-casual / excited) with a single **structuring-aggressiveness
   axis** (raw / light / moderate / assertive). Tone variation
   turned out to be noise — what users actually vary is "leave my
   words alone" vs "organize them for me." Day-one work; testable
   immediately.
2. **Close the two gaps offline Parakeet exposed** in Wave 5
   benchmarking:
   - **No live preview while talking.** Users type-pause-watch on
     Whisper; with Parakeet the pill stays empty until release. UX
     regression we papered over by emitting a synthesized partial on
     `dictation:result`, but that's a fake — there's no real
     incremental output to stream.
   - **Cold-start latency is structural.** Sherpa-onnx + 3 ONNX files
     loaded on every dictation → ~13.5s for a 116s capture in our
     bench, and that number was _consistent across runs_, so it's not
     a warm-up artifact. The ONNX runtime overhead is real.

After this wave we have **Parakeet-quality transcription, Whisper-style
streaming, structure-aware cleanup, all on local inference**. Then
Wave 7 packages it into something the world can install.

---

## Why this matters

Wave 5 made Parakeet usable but the engine experience still has two
soft spots: no live preview and a tone-based style system that
nobody actually uses the way it's intended. The instinct from Wispr's
"auto-format" feature is right — long brain dumps want structure,
short utterances want to be left alone. That's a single knob, not a
tone wheel.

Plus: the `BOOTHRFLOW_DEV=1` flag landed in Wave 5 makes the bench
harness a permanent part of how we evaluate engine swaps. Every
candidate goes through the same `bench:replay` + grading UI on the
same captured wavs. No more "trust me bro, X is faster."

We finish all this _before_ Wave 7's signing + auto-update because
shipping a polished installer of an unpolished engine is the wrong
order. Better to have one user (Eric) on a fast iteration loop with
the engine we actually want to ship long-term than ten users on a
signed installer of a placeholder.

---

## Phase 0 — Style overhaul: structure-aggressiveness axis (1–2 days)

The current `Style` enum (`Casual`, `Formal`, `VeryCasual`, `Excited`,
`Raw`, `CaptainsLog`) mixes two axes:

- **Tone** (casual ↔ formal ↔ excited)
- **Structure** (raw ↔ cleaned-up)

Empirically, users don't switch tones — they pick one once and forget
the picker exists. What they _would_ switch is "this short Slack
message just needs grammar fixes" vs "this 5-minute brain dump should
come back as a bulleted memo." That's the single axis worth exposing.

### New style set

| Style         | What it does                                                                                                                                                                                                                                  | Use case                                      |
| ------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------- |
| **Raw**       | No cleanup, paste verbatim                                                                                                                                                                                                                    | Code dictation, exact-quote capture           |
| **Light**     | Grammar + light punctuation, paragraph kept as-is. Roughly what current `Casual` does.                                                                                                                                                        | Short utterances, Slack messages, quick notes |
| **Moderate**  | Light cleanup _plus_ paragraph splits at natural breaks, removes filler ("um," "you know," repeated false starts)                                                                                                                             | Medium-length thoughts, emails                |
| **Assertive** | LLM has full freedom: bullets when listing, paragraph breaks at sentence-boundary pauses, code fences for "in code" cues, greeting + signature for Mail context, `H1/H2` headers for explicit transitions ("first," "second," "next section") | Long brain dumps, board memos, meeting notes  |

`Captain's Log` stays as an orthogonal **fun preset** — it's a tone
gimmick that doesn't fit the structure axis. Surfaces under
"Presets" or similar in Settings, separate from the structure picker.

### Deliverables

- **`Style` enum rewrite** in `src-tauri/src/settings.rs` — replace
  current variants with `Raw`, `Light`, `Moderate`, `Assertive`,
  plus `CaptainsLog` retained as-is.
- **Cleanup prompt branches** in `src-tauri/src/llm/prompt.rs` —
  one prompt per structure level. The `Assertive` prompt is the new
  work; it gets the kind of permissions Wispr's auto-format does:
  _"Reorganize freely. Use bullets when the user lists items. Add
  paragraph breaks at natural sentence-boundary pauses. Use code
  fences if the user said 'in code' or the focused app is a code
  editor. Add a greeting + sign-off when the focused app is Mail.
  Preserve every fact; never invent. Strip filler words."_
- **Settings UI** (`src/App.svelte`):
  - Replace the 6-option select with a 4-option segmented control
    (Raw / Light / Moderate / Assertive) — visual reinforcement of
    the "level" mental model.
  - Help text under the picker explains what each level does, with
    an example sentence (or short paragraph) showing the difference.
  - Captain's Log moves to a "Fun presets" disclosure below.
- **Settings migration** — `migrate()` in `settings.rs` rewrites old
  values to new:
  - `Casual`, `VeryCasual`, `Excited` → `Light`
  - `Formal` → `Moderate`
  - `Raw` → `Raw`
  - `CaptainsLog` → `CaptainsLog`
  - Per-app overrides get the same treatment.
- **Per-app default suggestions**: when the focused-app context
  detects Slack/Discord, default to `Light`; when it detects
  Mail/Notion/Obsidian, default to `Moderate` (or `Assertive` if the
  utterance is > N seconds — see acceptance below).

### Open questions

- **Should `Assertive` trigger automatically for long utterances?**
  Empirically, users don't change the picker mid-flow. A "use
  Assertive when audio_seconds > 60s" auto-promote (revertable in
  Settings) might match user intent better than forcing them to
  pre-select. Decide after Phase 0 ships and we have grading data.
- **Streaming compatibility.** Whisper's LA2 streaming finalizes
  partials as the user speaks. The cleanup pass runs once at the end.
  Assertive is post-cleanup so no streaming impact, but
  documentation should be clear: live-preview shows raw transcript;
  cleanup style only affects the final pasted text.

### Acceptance

- Existing settings migrate cleanly (no broken installs).
- The Lysara capture, re-run through `Assertive`, comes back with
  paragraph breaks, the "what needs to happen between now and then"
  question split out, and structural markers ("First, ..." /
  "Second, ...") if the LLM finds them.
- `Light` graded ≥ current `Casual`'s grade on the same capture
  (i.e., the rename doesn't regress quality).

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
