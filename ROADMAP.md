# Roadmap

> Where we are and where we're going. The detailed engineering plan lives in [`PLAN.md`](./PLAN.md); this is the user-facing summary.

## Current state — Waves 1–3 landed on `main` (April 2026)

The core push-to-talk dictation loop works end-to-end on Windows _and_ macOS.

- **Hold to dictate**: `Ctrl + Win` (Windows) or `Ctrl + Cmd` (macOS); release to transcribe + paste.
- **Tap to toggle (hands-free)**: `Ctrl + Alt + Space` (Windows) or `Ctrl + Option + Space` (macOS); tap once to start, tap again to stop. For dictations longer than you'd want to hold a key.
- Whisper-tiny-en STT (~75MB local model); **Metal auto-enabled on Apple Silicon** for ~5–15× CPU baseline.
- Local LLM cleanup via Ollama (qwen2.5:1.5b by default, OpenAI-compat HTTP)
- Persistent searchable history (SQLite + FTS5 + nomic-embed-text vectors)
- Quick-paste palette (Alt+Win+H / Option+Cmd+H)
- Streaming partials in the floating pill (Local-Agreement-2, two-line wrap)
- macOS first-run permissions panel (Microphone / Accessibility / Input Monitoring)
- 100% local: no audio or transcripts leave your machine

**What's missing:** structured/app-aware formatting (Phase 2), in-app Settings panel (Phase 2), Linux port, code-signing + notarization, onboarding wizard. See below.

### Cross-platform status (post Wave 3 polish)

Wave 3 polish lands across platforms as follows:

| Subsystem                              | macOS                    | Windows                                    | Linux                                                                      |
| -------------------------------------- | ------------------------ | ------------------------------------------ | -------------------------------------------------------------------------- |
| Pill rendering + scroll                | ✅ verified              | ✅ same code, no platform branches         | ✅ same                                                                    |
| Elapsed timer                          | ✅ verified              | ✅ same                                    | ✅ same                                                                    |
| 2-line partial wrap                    | ✅ verified              | ✅ same                                    | ✅ same                                                                    |
| LLM telemetry (skipped/unreachable/ms) | ✅ verified              | ✅ same                                    | ✅ same                                                                    |
| Tap-to-toggle hotkey                   | ✅ verified              | ✅ same chord (Ctrl+Alt+Space), rdev hooks | ⚠️ rdev needs X11 access on Wayland sessions — Wave 4 work                 |
| Hold-PTT hotkey                        | ✅ verified              | ✅ unchanged from Wave 1                   | ⚠️ same as above                                                           |
| Hotkey resync heartbeat                | ✅ macOS-only on purpose | n/a — `SetWindowsHookEx` is reliable       | n/a (Linux uses libevdev/uinput; resync would need its own implementation) |
| Permissions panel                      | ✅ macOS-only on purpose | n/a — Windows doesn't use TCC              | n/a — Linux distros vary too much to ship a single panel                   |
| Apple Silicon Metal default            | ✅ auto-enabled          | n/a                                        | n/a                                                                        |

What remains for Linux is _structurally_ a Wave 4 deliverable: rdev's
Wayland coverage, X11 vs Wayland clipboard injection, packaging
(AppImage / deb / Flatpak). None of the macOS-specific bits in Wave 3
block Linux; they sit behind `cfg(target_os = "macos")` and compile to
no-ops elsewhere.

---

## Phase 2 — Intelligence layer (weeks 4–6)

Goal: feels like Wispr Flow.

- **LLM cleanup pass** — Qwen 2.5 3B running locally via `llama-cpp-2`. Strips fillers, fixes punctuation, handles course-correction ("go to the store, I mean the office" → "go to the office").
- **Audio-pipeline noise suppression** — preprocess captured audio _before_ VAD/STT to strip background noise (HVAC hum, keyboard clack, household chatter, lossy bluetooth artifacts). Two viable options to evaluate:
  - **RNNoise** (Xiph.org, BSD) — tiny GRU-based suppressor, ~85KB model, 10ms latency, runs on CPU at <1% on M-series. Mature, used by Discord/OBS/Mumble. Rust binding via `nnnoiseless` crate (pure-Rust port). Good baseline — the question is whether it's _good enough_ on modern noise vs. paying for the heavier option.
  - **DeepFilterNet 3** (Hendrik Schröter, MIT) — newer, ONNX-based, ~10MB model, ~5ms per frame on M-series Metal, noticeably cleaner than RNNoise on non-stationary noise (chatter, music). Heavier dep, but Metal-accelerated via `ort` is straightforward.

  Plumbing: insert as an optional stage in `audio/cpal_source.rs` between mono mixdown and the resampler — the suppressor wants 16 kHz mono input, which is exactly the post-resample format, so we'd actually run it _after_ resample for a single-rate path. Toggleable from Settings (off / RNNoise / DeepFilterNet). Default off in v0, evaluate against ground-truth dictations in noisy environments before flipping the default. Pairs naturally with the existing Silero VAD: cleaner input → fewer false-positive speech frames at the start/end of an utterance, which tightens endpointing.

- **Cleanup quality refinements** _(near-term, prompted by Wave 3 dictation UAT)_ — observed gaps from Eric's hands-free dictation pass:
  - **Mumbling / rambling removal.** Filler phrases ("you know", "I mean", "uh", "kind of"), false starts, restarts, and tangential half-sentences should be cleaned up by default in non-Raw styles. Today's prompt asks the model to preserve words exactly. Need a graded mode: keep the meaning, drop the disfluency. Plumbing: a per-style `aggressiveness` flag in the system prompt (0 = preserve verbatim, 1 = drop fillers, 2 = light paraphrase). Casual default = 1.
  - **Bump default to Qwen 2.5 7B.** Wave 3 UAT showed 1.5B is fast (~150-300ms typical) but borderline on quality. 7B costs ~350-400ms which is below the "feels instant" threshold for cleanup (the user has already finished speaking and is watching the paste land). Move `DEFAULT_MODEL` from `qwen2.5:1.5b` to `qwen2.5:7b` once the in-app Settings panel ships so users can drop back to 1.5B if their LLM box is slow. Document as a Reverse-ADR-014 follow-up if we commit.
  - **Vocabulary expansion.** The Whisper `initial_prompt` doesn't currently list "Qwen" (or "Wispr", "Tauri-Specta", "boothrflow", "MTLDevice"…). Misses on those words ride through to the LLM, which can't always recover. Action: append a curated tech-vocab chunk to `DEFAULT_INITIAL_PROMPT` and let the (future) Personal Dictionary append user-specific terms on top.
  - **Spelled-out word detection** — when the user spells a name or technical term mid-sentence ("my last name is Boothe, B-O-O-T-H-E", "the project is called Q-W-E-N as in queen with a W"), the STT often produces a sequence of letter-tokens that the LLM doesn't know to treat as authoritative. Plumbing: a pre-cleanup pass that scans the raw transcript for spelling patterns —
    - Hyphen-joined uppercase runs: `B-O-O-T-H-E`, `Q-W-E-N`
    - Space-separated single-letter sequences: `b o o t h e`, `q w e n`
    - Letter-word sequences: `bee oh oh tee aitch ee`, `cue double-u ee en`
    - NATO phonetic: `bravo oscar oscar tango hotel echo`
    - Cue phrases: "spelled", "as in", "letters"

    — collapses each detected spelling to the literal word, and emits a `<spelling>BOOTHE</spelling>` marker that the LLM cleanup prompt is told to honor as the canonical spelling for the surrounding entity. Bonus: feed confirmed spellings back into the Personal Dictionary so the next dictation gets it right at the STT layer, not the cleanup layer. Reverse pipeline: STT misses → user spells → marker created → LLM applies → dictionary learns → STT no longer misses on subsequent dictations.

  - **Context-aware reflection ("does this make sense?")** — when a user dictates fast, Whisper can produce homophone-substitutions and acoustic mishears that survive cleanup because the LLM is told to preserve words exactly. Today's prompt: "Add periods, commas, … Do NOT change words". Wanted behavior: the LLM should be allowed to swap a transcribed word for the contextually-correct one when the original is _semantically nonsensical_ in context — "the patch landed in the rebase" stays as is, but "the patch landed in the bay sis" becomes "basis", and "we deployed to the cluster fluffer" becomes "cluster buffer" (or whichever fits surrounding context). Three implementation tiers, ship in order:
    1. **Single-pass with reflection in the system prompt** (cheap, ~no latency cost) — extend the prompt with: "If a word is acoustically plausible but semantically nonsensical in context, replace it with the most likely intended word and wrap your replacement in `<corrected from='X'>Y</corrected>` for the first dictation; once the correction tracker matures, drop the marker." Single LLM call, just a smarter prompt.
    2. **Confidence-flagged reflection** (medium, ~20% latency cost) — Whisper exposes per-token logprobs via `whisper-rs`'s segment iterator. Tokens with logprob below a threshold get tagged in the LLM prompt as "uncertain": "the cluster `<uncertain>fluffer</uncertain>` is down" → LLM is explicitly invited to consider alternatives at that position. More accurate than #1 because the LLM knows _where_ to focus.
    3. **Two-pass reflection** (heaviest, ~2× latency) — first pass does normal cleanup, second pass reads its own output and is asked "is anything off?". Drops to single-pass when first-pass output looks confident (no uncertainty markers, no flagged tokens). Reserve for a "Quality" preset that the user opts into when they don't care about latency.

    All three tiers feed `<corrected>` markers into the history record so the user can see what was changed; rating the result becomes a strong feedback signal for the self-learning loop in Phase 3. Prereq: aggressiveness flag must be ≥ 1 (otherwise we're contradicting the per-style "preserve verbatim" instruction).

  - **Connect feedback ratings to model selection.** When the rating tool ships (Phase 3), use bad-rated transcripts to flag prompts that consistently underperform; auto-suggest model upgrades when accuracy drops below a threshold.

- **OCR the focused window as cleanup context** _(pattern from [matthartman/ghost-pepper](https://github.com/matthartman/ghost-pepper))_ — before running cleanup, screenshot the frontmost window, run on-device OCR (macOS: Apple Vision; Windows: Win32 OCR or Tesseract), and feed the recognized text into the cleanup prompt under an `<OCR-RULES>` block: _"Use the window OCR only as supporting context. Prefer the spoken words, but if a spoken word is a recognition miss for a name, command, file, or jargon visible in the OCR, correct it."_ Two-pronged win: (a) proper nouns visible on screen get auto-corrected without the user maintaining a vocab list; (b) the cleanup pass becomes context-aware (replying to a Slack thread? Editing a doc? The model sees what you see). Plumbing: a new `Context/` Rust module mirroring ghost-pepper's `OCRContext` + `FrontmostWindowOCRService` shapes. Permission gate: macOS Screen Recording (in addition to existing Mic/Accessibility/Input Monitoring); Windows is unrestricted. Defer to a Phase 2 sub-feature; entirely optional via Settings toggle, with a privacy callout explaining what's captured (the OCR'd text never leaves the machine — feeds the local LLM and is then dropped).
- **Auto-learning correction store** _(pattern from ghost-pepper's `PostPasteLearningCoordinator`)_ — passive self-improvement loop. After every paste, poll the focused text field for ~15 s (1 s cadence). When the field stops changing for ~2 s, diff what we pasted against what the user kept. If the diff is a narrow correction (≤2 words), record `MisheardReplacement { wrong: "kwen", right: "Qwen" }` into a `CorrectionStore` SQLite table. Subsequent cleanup prompts include the user's top-N learned corrections as a `<USER-CORRECTIONS>` block: _"Treat these substitutions as authoritative."_ Closes the same loop as the planned spelling-detection feature but without requiring the user to spell anything — they just edit naturally and the system learns. Strictly better than rating-based feedback (passive, no UI friction) and complements it (ratings tell us when the cleanup _approach_ is wrong; this learns the _vocabulary_).

  Also exposes two user-editable lists in Settings:
  - `preferredTranscriptions` (newline-separated vocabulary, augments the Whisper `initial_prompt`)
  - `commonlyMisheard` (`wrong -> right` lines, augments the cleanup prompt's `<USER-CORRECTIONS>` block)

  Both auto-populate from the learning coordinator and accept manual edits. Direct lift from ghost-pepper's `CorrectionStore` design — proven UX.

- **Prompt prefix caching** _(pattern from ghost-pepper's `CleanupPromptPrefillPlan`)_ — split the cleanup prompt into a static prefix (system instructions + `<USER-CORRECTIONS>` + Style block) and a dynamic suffix (OCR context + the actual transcript). Send the prefix once per Settings change; subsequent dictations only re-encode the dynamic suffix. Ollama supports this via `keep_alive` + reusing context across requests, or via the `/api/generate` endpoint's `context` field. Latency cut for second-and-later dictations within a session — typically saves the entire prompt-eval portion (~150-300 ms on 1.5B Qwen, more on 7B).

- **Streaming partial continuation past the 25 s cap** _(Wave 3 UAT carry-over)_ — the pill stops updating after ~25 s because `MAX_STREAMING_SAMPLES = 16_000 * 25` and Whisper's 30 s context window starts to drop early audio. Final transcript on release is still complete; only the live display freezes. Approach: a commit-and-roll loop in `streaming.rs`. When the buffer crosses ~20 s and LA2 has a long stable prefix, freeze that prefix into a separate `frozen_text: String` field on `Inner`, trim the buffer to the last ~5 s of audio (overlap), and continue ticking. Worker emits `StreamingPartial { committed: frozen + new_committed, tentative, … }`. Bounded per-tick cost, indefinite session length, minimal boundary-word risk. Same final-pass fallback semantics. ~half-day of work.
- **Style presets** — Formal, Casual, Excited, Very Casual + custom.
- **App-context detection** — `GetForegroundWindow` + UI Automation to detect Slack vs Gmail vs IDE, applies the right style automatically.
- **Structured formatting (app-aware)** — beyond punctuation. Wispr Flow's superpower is that long dictations come back as actual _structure_: bullet lists when you spoke a list, paragraph breaks when you paused, a greeting + signature in Mail, code fenced when you said "in code". Plumbing: extend the cleanup prompt with a structure-detection pass keyed on app context (Mail / Slack / Notion / IDE / generic) plus heuristics on the raw transcript ("first… second… third" → numbered list; >25s of speech → paragraph splits at sentence-boundary pause markers). Surfaces as a sixth Style ("Auto-format") that overrides tone-only styles when the model has high confidence; falls back to plain casual cleanup otherwise.
- **In-app Settings panel** — every setting that's currently env-var-only should be flippable from the UI. Whisper model picker (tiny / base / small / medium / large-v3-turbo, with download-on-select), LLM endpoint + model + API key, embed endpoint + model, hotkey rebind (PTT chord, toggle chord, quick-paste chord), per-app style overrides, privacy-mode toggle, `BOOTHRFLOW_WHISPER_PROMPT` for vocabulary biasing. Persists to `tauri-plugin-store` (already in the dependency tree). Pre-req: typed Tauri command surface (ADR-007 deferred work) so the FE doesn't have to mirror Rust types by hand for ~15 new commands.
- **Personal dictionary** — manual add + auto-learn from your post-edits. Hot-word boost via Whisper's `initial_prompt` trick.
- **Skip-LLM hotkey** — explicit "raw mode" for code dictation.

## Phase 3 — Memory & differentiators (weeks 7–9)

Goal: beats Wispr Flow on memory.

- **Searchable history** — every dictation persisted in SQLite + FTS5.
- **Semantic recall** — `bge-small-en-v1.5` embeddings + `sqlite-vec` for hybrid lexical + semantic search.
- **Quick-paste palette** — `Ctrl+Win+H` opens a fuzzy-search overlay; pick a past dictation and paste it.
- **Command Mode** — highlight text + hold-to-speak a transformation ("make this more concise", "translate to Spanish").
- **Voice commands** — "press enter", "new line", "delete that", "select all".
- **Push connectors (voice + history routing)** — instead of pasting into the focused app, route the transcript to a configured destination. Two surfaces:
  1. **Voice trigger.** The cleanup pass detects routing instructions inline ("push this to slack", "send to email", "drop into the ops channel") and treats them as a `Connector::SendTo(target, payload)` call rather than a paste. The instruction itself is stripped from the body.
  2. **History row action.** Each row in the History panel grows a "Push to…" dropdown listing configured connectors. One click queues a background job that sends the formatted transcript and reports success/failure as a toast.

  Connector trait: `fn send(&self, payload: ConnectorPayload) -> Result<()>` — implementations land progressively. v0 set: Slack (incoming webhook), Email (SMTP), generic HTTP webhook (catch-all). v1 set: Notion (append-to-page), Linear (create-issue), Gmail (compose-and-send via OAuth). All connector configuration lives in the in-app Settings panel; secrets stored via `tauri-plugin-store`'s encrypted backend (or OS keychain via `keyring-rs`). Background queue lives in the session daemon — jobs survive app close and retry on next start.

- **Feedback / rating tool** — every history row gets a 1–5 thumb rating + optional free-text feedback. Stored alongside the transcript record. Two near-term uses:
  - Powers the **Insights dashboard** (Beyond v1) — words/day, accuracy delta, top apps, rating trend over time.
  - Lights up the **self-learning loop** below.
- **Self-learning loop** — once we have a corpus of rated transcripts (target: ≥500 entries with ≥1-star span), train a small **LoRA adapter** on top of the cleanup model using the user's own rating signal. Two paths to evaluate:
  1. **DPO over rated pairs** — when two transcripts share a similar raw input but get different ratings, the high-rated one is the chosen sample. Direct Preference Optimisation needs no reward model and runs on a 4090 / M-series in hours, not days.
  2. **Prompt-prefix tuning** — cheaper-but-cruder: extract the user's preferred output _patterns_ (favored tone, sentence length, list usage) into a learned prefix appended to the system prompt. No model weights touched.

  v0: ship the rating capture and the corpus export script. The adapter trainer is its own follow-up that can run offline / opt-in. v0 export format: `transcripts.parquet` with `{raw, formatted, style, app_context, rating, comment, ms_total}`. Anyone can pull this into their own training run; we provide a reference Colab.

## Phase 4 — Production polish (weeks 10–12)

Goal: 1.0.

- **NVIDIA Parakeet TDT 0.6B v3** as default STT (faster, more accurate, native streaming) via `sherpa-onnx`. Whisper becomes the multilingual fallback.
- **TEN-VAD** swap-in (faster endpoint detection than Silero).
- **Onboarding wizard** — model download, mic test, hotkey config, accessibility permissions (macOS), Windows SmartScreen explainer.
- **Code signing** — Azure Trusted Signing on Windows, Developer ID + notarization on macOS.
- **Auto-update** — `tauri-plugin-updater` + GitHub Releases.
- **macOS port** — WhisperKit on Apple Neural Engine, AXUIElement for paste injection.
- **Linux port** — sherpa-onnx works the same; X11 + Wayland injection paths.
- **Privacy audit doc** _(pattern from ghost-pepper)_ — ship `PRIVACY_AUDIT.md` containing (a) the exact prompt to feed Claude Code (or any AI assistant) to verify all default features are 100% local, (b) a checklist of subsystems with file pointers and pass/fail status, (c) an explicit list of opt-in cloud features (BYOK LLM endpoint, BYOK embed endpoint) showing they're disabled by default. Run the audit on every release and store dated results in the same file. Cheap to author (1-2 hours), high trust signal — and exactly the kind of thing reviewers and HN commenters notice immediately.

## Phase 5 — Meeting transcription mode

A new product surface beyond push-to-talk dictation, inspired by ghost-pepper.

Goal: continuous-recording meetings produce transcript + summary as markdown automatically.

- **Dual-stream capture** — concurrent mic (cpal / WASAPI) and system-audio (macOS ScreenCaptureKit, Windows WASAPI loopback, Linux PulseAudio monitor) capture, each tagged with source. Solves the "I can't hear the other side of the call" problem.
- **Chunked transcription pipeline** — 30 s chunks with 1 s overlap for dedup, disk-spilled after transcription so memory stays bounded over multi-hour meetings. Different pipeline from PTT (which is single-utterance + LA2 streaming); meeting mode does coarse-grained chunks + diarization.
- **Speaker diarization** — pyannote-onnx or sherpa-onnx speaker diarization on the chunk WAVs post-meeting; map clusters to known voices via a `RecognizedVoiceStore` (you tag a few seconds of "this is me" once, future meetings auto-label).
- **Summary generation** — feed the diarized transcript to the local LLM with a meeting-summary prompt; output as markdown (decisions, action items, open questions) saved to a user-chosen folder. Same model as cleanup but a different prompt.
- **Meeting detection** — heuristic (windowed: Zoom / Meet / Teams / FaceTime in foreground? + mic active? + camera active?) auto-suggests starting a recording; user accepts / dismisses. Privacy-conscious — never auto-records without consent.
- **Markdown library** — local folder of meeting markdowns becomes searchable via the same FTS5 + nomic-embed-text infra we already use for dictation history. One unified search across "things I dictated" and "things people said in meetings."
- **Imports** — Granola-style: pull existing meeting notes from another app and index them. Read-only, just augments the searchable corpus.

## Beyond v1

- **Snippets** — voice-activated text expanders.
- **Plugin API** — pre-STT, post-STT, pre-paste hooks (WASM-sandboxed).
- **Sound effects** _(ghost-pepper polish)_ — subtle "ding" on dictation start, softer "click" on paste-complete. Off by default to avoid surprising users; on by default for meeting recording start/stop where the audio cue is information, not noise.
- **Prompt editor window** _(ghost-pepper UI)_ — first-class UI for editing the cleanup system prompt, with reset-to-defaults, save, and live preview against a stored test transcript. Bigger than the in-app Settings field; lives in its own window like ghost-pepper's `PromptEditorWindow`.
- **Transcription Lab** _(ghost-pepper dev tooling, optionally exposed to power users)_ — a "lab" view where you can replay stored audio fixtures against the current STT/LLM stack and diff outputs across model + prompt configurations. Originally a developer tool for iterating on prompts; valuable enough to expose to users tuning their own setup.
- **LoRA fine-tuning** on your own dictation history (opt-in). See the self-learning loop in Phase 3 for the corpus + DPO sketch.
- **"Whisper Mode"** — sub-audible speech (custom acoustic model required).
- **Insights dashboard** — words/day, accuracy delta, top apps, rating trend.
- **File tagging in Cursor / Windsurf** — `@file` syntax injection when you mention a filename.
- **Captain's Log mode** _(easter-egg style)_ — a sixth Style ("Captain's Log") that rewrites dictation as a Star-Trek-style log entry. The cleanup prompt:
  - Prepends `Captain's log, stardate <X>` where stardate is computed from the current real-world date (TNG-era approximation: `1000 × (year − 2323) + (day_of_year × 1000 / 365.25)`, rendered to one decimal). Today (2026-04-29), that's a negative stardate; for fun we'll absolute-value it or pick a fixed forward-shift offset so it reads like a future entry.
  - Rewrites the body in formal 24th-century space-faring tone — "Set course for…", "We have detected…", "The crew is investigating…", "End log." — without changing the underlying content. Same `aggressiveness` knob as other styles, so it doesn't hallucinate plot.
  - Idiom whitelist: cleanup may add closing phrases like "End log." but won't invent ship names, stardate-numeric prefixes, or canon characters. Keeps it bounded.

  Selected from the same Style dropdown as Casual / Formal / Excited; per-app Style overrides apply. Goes in the joke-but-actually-useful column — same energy as the Pirate Mode that lots of code editors ship for talk-like-a-pirate day.

## What we are deliberately not building

- Mobile (Wispr Flow's edge; we're desktop-first).
- Cloud sync of dictionary/snippets across devices (local-first means the data stays here).
- Team features.
- Voice-control automation (Talon's territory; different problem).

## How feature decisions get made

Every architecturally-significant choice goes through an ADR ([`DECISIONS.md`](./DECISIONS.md), 14 entries so far). UATs after each phase ([`docs/uat/`](./docs/uat/)) capture what shipped, what got deferred, and why.

If you want a specific feature, open an issue with the use case. Concrete user friction beats theoretical architecture in our prioritization.
