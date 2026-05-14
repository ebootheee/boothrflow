# Roadmap

> Where we are and where we're going. The detailed engineering plan lives in [`PLAN.md`](./PLAN.md); this is the user-facing summary.

## Current state — Waves 1–3 + Wave 4a + Wave 4B landed on `main` (April 2026); Wave 5 in UAT on `feat/wave-5` (May 2026)

The core push-to-talk dictation loop works end-to-end on Windows _and_ macOS.

- **Hold to dictate**: `Ctrl + Win` (Windows) or `Ctrl + Cmd` (macOS); release to transcribe + paste.
- **Tap to toggle (hands-free)**: `Ctrl + Alt + Space` (Windows) or `Ctrl + Option + Space` (macOS); tap once to start, tap again to stop.
- Whisper-tiny-en STT (~75MB local model); **Metal auto-enabled on Apple Silicon** for ~5–15× CPU baseline. Initial-prompt vocabulary covers project-specific proper nouns so they don't get rendered phonetically.
- Local LLM cleanup via Ollama (**qwen2.5:7b** by default, fallback to qwen2.5:1.5b via env var or Settings, OpenAI-compat HTTP). Per-style aggressiveness flag drops disfluencies and self-corrections by default; `Raw` style preserves verbatim. **`tok/s` telemetry** in the cleanup chip alongside ms.
- **Five Style presets** plus the **Captain's Log** easter-egg style.
- **Streaming partials with commit-and-roll** — pill keeps updating indefinitely on long dictations (LA2-stable prefix freezes at the 20 s mark, audio buffer trims to a 3 s overlap, suffix-prefix dedup keeps boundary words clean).
- Persistent searchable history (SQLite + FTS5 + nomic-embed-text vectors).
- Quick-paste palette (`Ctrl+Alt+H` / `Ctrl+Option+H` post Wave 5e migration; legacy default `Alt+Win+H` / `Option+Cmd+H` migrated automatically).
- **In-app Settings** (Wave 4B): Whisper / LLM / embed model pickers with parameter-count labels, hotkey rebind UI, vocabulary editor, privacy toggle, settings export/import. Persists via `tauri-plugin-store`.
- macOS first-run permissions panel (Microphone / Accessibility / Input Monitoring + Screen Recording in Wave 5).
- 100% local: no audio or transcripts leave your machine.

**Wave 5 (in UAT on `feat/wave-5`, six commits 022876f → f1b4aaa):**

- **App-context detection** (`NSWorkspace::frontmostApplication` / `GetForegroundWindow`) → cleanup prompt's `<APP-CONTEXT>` block.
- **Common mishearings editor** + `<USER-CORRECTIONS>` prompt block.
- **Auto-learn correction coordinator** — post-paste settling window, macOS AX read of focused field, single-word edit detection, FIFO-capped at 50 entries.
- **Focused-window OCR** for cleanup context (macOS Vision via `CGDisplayCreateImage` + `VNRecognizeTextRequest`). Eager Screen-Recording permission prompt at toggle time.
- **OCR prompt-injection sanitizer** — neutralizes `<` / `>` so OCR'd text can't close `<WINDOW-OCR-CONTENT>` and inject fake instructions.
- **Parakeet TDT 0.6B engine** behind the `parakeet-engine` Cargo feature, via sherpa-rs 0.6.8. Auto-runs a metadata-propagation script during model download. Probe transcribes the bundle's test wav at ~470ms.
- **Prompt prefix caching** via Ollama `keep_alive: 5m` extra (heuristic-gated to port 11434 only so LM Studio / llama-server users don't get unknown-field rejections).
- **Privacy mode expansion** — now suppresses OCR + app context propagation + auto-learn in addition to the existing LLM-cleanup gate.

UAT checklist: [`docs/uat/wave-5-checklist.md`](./docs/uat/wave-5-checklist.md). Detail + handoff items: [`docs/waves/wave-5-context-aware-cleanup.md`](./docs/waves/wave-5-context-aware-cleanup.md).

**Wave 5 carry-overs (deferred from `feat/wave-5`):**

- Windows UIAutomation focused-field reader (mirror of macOS AX path).
- Windows OCR via `windows::Media::Ocr::OcrEngine`.
- Parakeet streaming partials integration with `LocalAgreement2` (currently no live partials when Parakeet is the active engine).
- ScreenCaptureKit pivot — `CGDisplayCreateImage` is deprecated as of macOS 14.4; works through 15.x but should move to `SCContentFilter` + `SCStream` before it actually disappears.

---

## Wave 6 — Engine + formatting (active: `feat/wave-6-engine-and-formatting`)

**Plan:** [`docs/waves/wave-6-engine-and-formatting.md`](./docs/waves/wave-6-engine-and-formatting.md). Get the engine and the cleanup pass right _before_ we package + ship. Better one user (Eric) on a fast iteration loop with the right engine than ten users on a signed installer of a placeholder.

**Phases (status as of 2026-05-05):**

0. ✅ **Style overhaul** _(shipped — `d71cb90` + `4ba7e95`)_. Replaced the tone-based system (casual / formal / very-casual / excited) with a **structuring-aggressiveness axis** (raw / light / moderate / assertive). Captain's Log retained as orthogonal fun preset. Old settings auto-migrate via serde aliases. Assertive prompt rewritten _twice_: first round invented headers + emitted fake Mail signatures; tightened version makes every structuring permission strictly conditional on its trigger + bans `[Your Name]` placeholders / preambles / inline-filename backticks. Auto-upgrade qwen 0.5b/1.5b/3b → qwen2.5:7b for Assertive only (small models can't follow the rules). Default for new installs: Light.
1. ⏳ **Nemotron Speech Streaming via sherpa-onnx** (3-5d) — NVIDIA already ships ONNX exports for cache-aware streaming at 80-1120ms chunks. Same param scale as our current 0.6B Parakeet, so quality should hold while gaining live preview.
2. ⏳ **parakeet.cpp evaluation** (2-3d) — C++ Parakeet impl with Metal acceleration via Axiom. Bench against sherpa-onnx Parakeet on the same wavs; swap on macOS only if it wins by >2× on load+decode.
3. ⏳ **Bench harness hardening** _(partially shipped — `4ba7e95`)_. Warmup pass landed (engine loaded once per config, throwaway 1-second silence transcription before timed run). N=3 + median + variance still pending. Aggregate "across all captures" leaderboard in Benchmarks tab still pending.

After this wave, the default STT + cleanup style for production is re-decided based on leaderboard mean grade across ≥ 3 captures.

### Wave 6 small-fixes (landed alongside Phase 0)

- ✅ **History detail → inline expand-under-row** (`60bb2b0`). No more off-screen detail panel.
- ✅ **Cleanup chip tok/s fallback** (`60bb2b0`). Derives tok/s from completion_tokens + llm_ms when the backend skips the explicit field.
- ✅ **Bluetooth-aware mic default + manual device picker** (`a7302de`). Switches to built-in mic when AirPods / Beats / Bose / Sony WH/WF are the system default — avoids macOS HFP downgrade that dims music for ~30 seconds. Override the auto-pick via the new dropdown in Settings → General → Microphone.

### Already-shipped Wave 6 prerequisites (Wave 5)

- ✅ **Performance baseline harness** — `BOOTHRFLOW_DEV=1` flag, captures-to-disk, `bench:replay` tool, in-app grading UI. Validated on the 116s Lysara capture: Parakeet beat Whisper-tiny + Whisper-base on raw fidelity (named entity, no semantic substitutions), at ~16× the STT load+decode cost.

---

## Wave 7 — Production polish (next, locked in)

**Plan:** [`docs/waves/wave-7-production-polish.md`](./docs/waves/wave-7-production-polish.md). Six phases, 6-9 focused days total, each independently shippable. Order swapped with the engine-and-formatting wave: dial in the engine first, _then_ package.

**Goal:** turn boothrflow from "works on Eric's laptop in dev mode" into "anyone can download a signed installer, get prompted by the real app for permissions, and stay current via auto-update." Without this, every dev-mode TCC permission is owned by the parent terminal, the app doesn't appear correctly in System Settings → Privacy & Security panes, and there's no way to give the build to anyone (multi-hour clone-and-compile onboarding).

**Phases:**

1. **Release infrastructure** — `VERSION` file, GitHub Actions matrix build (macOS-arm64, macOS-x64, Windows-x64), `RELEASING.md` playbook, CHANGELOG → release-notes mapping.
2. **macOS code signing + notarization** — Developer ID + `notarytool`. Replaces the dev-mode TCC dance with proper app attribution.
3. **Auto-update** — `tauri-plugin-updater` + GitHub Releases as the manifest server. Paired with Phase 2 because **unsigned auto-update is broken UX** — every update re-triggers Gatekeeper's "Open Anyway" dance. Sign first, auto-update second; the loop is real after Phase 3 lands. Phases 1+2+3 together = working release loop on macOS.
4. **Windows code signing** — Azure Trusted Signing (cheap path) or EV cert. Quiets SmartScreen for the `.msi`. Can lag by a release if onboarding drags.
5. **Onboarding wizard** — first-launch flow walking through privacy callout, mic permission, accessibility/input-monitoring/screen-recording permissions, model download (with progress), hotkey config, LLM endpoint check.
6. **Beta → Stable channels** — two release manifests (`latest-beta.json` / `latest.json`); promotion script; cadence rules in `RELEASING.md`.

**After Wave 7:** every subsequent feature follows a **staging → stable** cadence. Feature branch → local UAT → beta tag → 3-7 day soak on Eric's daily driver → promote to stable. Hot-fix path goes direct to stable.

---

## Wave 8 — Connectors, UI rebuild, privacy audit

**Plan:** [`docs/waves/wave-8-connectors-ui-privacy.md`](./docs/waves/wave-8-connectors-ui-privacy.md). The post-production wave — three independent tracks that turn boothrflow from "fast Wispr alternative" into "the local-first dictation tool with reasons-to-switch."

**Phases:**

1. **Connectors** (4-6d) — `Connector` trait + Obsidian vault push (markdown notes with frontmatter + embeddings) + custom HTTP webhook + Slack incoming webhooks. Voice-triggered routing detects "push this to Slack" inline and routes via the connector instead of pasting. History rows grow a "Push to…" dropdown.
2. **Hyper-modern UI rebuild** (5-8d) — visual language refresh (shadcn-svelte or hand-rolled), pill redesign (pulsing dot during listening, typewriter trail during cleanup), Liquid Glass / NSVisualEffectView vibrancy on macOS, Cmd-K command palette, keyboard shortcuts everywhere.
3. **Privacy audit doc** (1d) — `PRIVACY_AUDIT.md` with a pre-written AI-assistant prompt, default-features checklist, BYOK callouts, telemetry confirmation (none), pass/fail table. Settings → Privacy → "Run privacy audit" button. README badge.

---

## Smaller items still queued (post-Wave-8 unless promoted)

### Audio noise suppression `[low–medium]`

RNNoise (Rust binding via `nnnoiseless`, ~85KB model) or DeepFilterNet 3 (upgrade path) as an optional pre-VAD stage in `audio/cpal_source.rs`. Pairs with Silero VAD already there: cleaner input → tighter endpointing. Toggle in Settings, default off.

### Cleanup quality follow-ups `[low]` (stack-friendly)

- **Skip-LLM hotkey** — explicit "raw mode" for code dictation. One new hotkey row in Settings.
- **Spelled-out word detection** — pre-cleanup pass scans for `B-O-O-T-H-E` / `b o o t h e` / NATO phonetic / "spelled" cue phrases. Emits an authoritative-spelling marker the LLM is told to honor.
- **OCR length-based gate** — skip OCR when raw_text > 500 chars (OCR's marginal value drops as spoken text gets longer).

### Wave 5 carry-overs `[low–medium]`

- Windows UIAutomation focused-field reader.
- Windows OCR via `windows::Media::Ocr::OcrEngine`.
- Parakeet streaming partials integration with `LocalAgreement2`.
- ScreenCaptureKit pivot from the deprecated `CGDisplayCreateImage`.

### Multilingual Whisper variants `[low]`

Currently we ship the `.en` Whisper variants only (English-only, slightly more accurate on English than the multilingual same-size models). Add the multilingual rows (`tiny`, `base`, `small`, `medium`, `large-v3-turbo` without the `.en` suffix) to the picker so non-English users have an option without switching to BYOK. ~25 lines + a download-script alias each.

### Parakeet 1.1B English variant `[low]` (gated on benchmark numbers)

NVIDIA's larger Parakeet variant — same architecture as the 0.6B v2 we ship, ~2× memory, ~1.5-2× latency, but cleaner output on technical jargon. Worth offering as a third Parakeet row for power users with M-series Pro/Max chips. One-line bundle URL change in the download script + a row in `whisper_models()` once the sherpa-onnx ONNX export is published.

---

## Future ideas (post-Wave-8)

Things that have come up and are worth queuing — not committed, just captured so they don't get lost. (**Connectors**, **hyper-modern UI rebuild**, and **privacy audit doc** were promoted out of this list into Wave 8.)

### STT engine evaluations (NVIDIA model family)

Mostly absorbed by Wave 6 (which evaluates Nemotron Speech Streaming and parakeet.cpp). Remaining candidates worth queuing — all gated on the Wave 6 bench numbers existing first.

- **Parakeet TDT 0.6B v3 (multilingual).** Same architecture as the v2 we ship, but trained on 25 EU languages with automatic language detection. Direct upgrade once sherpa-onnx publishes its ONNX export. We could re-export from NeMo's checkpoint ourselves with sherpa-onnx's tooling (~1-2 days of model work) instead of waiting. Unblocks multilingual dictation without forcing users to switch to BYOK cloud.
- **Multitalker Parakeet.** Streaming multi-speaker ASR with speaker kernel injection (no enrollment audio required). Natural fit for the meeting transcription mode below — collapses STT + diarization into one model. Better latency and accuracy than the chained pyannote-onnx pipeline that meeting mode currently assumes.
- **Parakeet Realtime EOU.** 120M-param streaming ASR with built-in end-of-utterance detection at 80-160ms latency. Could replace Silero VAD as our endpoint detector — EOU is a stronger signal than "is the user speaking right now" because it knows when an utterance is _complete_. Tightens tap-to-toggle hands-free mode (auto-stop on real utterance end vs configurable silence timeout).
- **Canary multilingual translation.** Simultaneous translation + transcription across 25 languages via NVIDIA's Granary dataset. Different feature class — not a replacement for our current STT but the engine behind a "Translate to English" / "Translate to Spanish" Style preset. Speak French, paste English. Niche but cool for multilingual users.
- **Parakeet 1.1B English.** NVIDIA's larger Parakeet variant — same architecture as the 0.6B v2 we ship, ~2× memory, ~1.5-2× latency, but cleaner output on technical jargon. Worth offering as a third Parakeet row for power users with M-series Pro/Max chips. One-line bundle URL change in the download script + a row in `whisper_models()` once the sherpa-onnx ONNX export is published.

### Meeting transcription mode

Continuous-recording meetings produce transcript + summary as markdown automatically. Big enough to be its own product surface (Phase 5 in the original PLAN.md). Briefly:

- Dual-stream capture (mic + system audio) on macOS via ScreenCaptureKit, on Windows via WASAPI loopback.
- 30-second chunks with 1-second overlap; disk-spilled so memory stays bounded over multi-hour meetings.
- Speaker diarization post-meeting (pyannote-onnx or sherpa-onnx).
- Summary via local LLM with a meeting-summary prompt.
- Meeting markdowns get indexed by the same FTS5 + nomic-embed-text infra as dictations.

Pairs naturally with the Obsidian connector (meetings → notes folder).

### Plugin API

Pre-STT, post-STT, pre-paste hooks. WASM-sandboxed so plugins can't read arbitrary disk / make network calls without explicit permission. Lets users wire custom transformations (e.g. "always replace `lol` with `haha`," "convert numbers to digits in coding contexts") without forking the app.

### Insights dashboard

Words/day, accuracy delta over time, top apps used in, rating trend (depends on the rating tool from Phase 3). Local-only — never leaves the machine.

### Snippets / voice-activated text expanders

"Insert standup" → boothrflow expands to your standup template. Triggered by voice during dictation (rather than typing the trigger string). Adjacent to the connector idea: snippets as a special kind of insert.

### Voice commands in dictation

"press enter", "new line", "delete that", "select all" parsed mid-dictation and translated to keystroke sequences. Different from Command Mode (which is a separate hold-to-speak transformation gesture). Small finite parser.

### Linux port

X11 + Wayland clipboard injection paths, AppImage / deb / Flatpak packaging. rdev's Wayland coverage is the gating dependency. sherpa-onnx works the same on Linux as macOS / Windows.

### Mobile companion (iOS, capture-and-sync — Path B)

Not a Wispr-on-iOS clone. iOS doesn't allow the "push-to-talk → paste anywhere" magic that desktop boothrflow does (no global hotkeys, no paste-into-any-app, sandboxed app-context). Trying to compete with Wispr's iOS app or AudioPen on their turf is a losing fight against incumbent marketing budgets.

Instead: a **private mobile capture surface** for the same searchable corpus your desktop boothrflow owns. Phone is always with you; desktop is where you process + paste. Two paths into the same house.

**Architecture sketch:**

- **iOS app** (Tauri 2 mobile, sharing the Settings + history UI from desktop). Dictate via hold-to-record button; on-device STT (WhisperKit on the Apple Neural Engine, or sherpa-onnx → Parakeet via ONNX Runtime's CoreML provider); on-device cleanup via Apple Intelligence's `FoundationModels` framework on iOS 18+ (free, no API key) or MLX-hosted Qwen 1.5B Q4 (fits in iPhone 15 Pro+ memory budget). Optional BYOK cloud LLM cleanup for users who prefer cloud quality.
- **Sync layer** between phone + desktop, **end-to-end encrypted with user-hosted keys** (Signal-style trust model — server can store ciphertext but can't read it; key derivation lives on the user's devices, not on a central service). Three viable transports:
  - **iCloud Drive** as the dumb blob store — phone writes ciphertext, desktop reads it. Zero infra to run.
  - **LAN-only sync** when both devices are on the same WiFi (mDNS discovery + libp2p). Most private, no third-party at all.
  - **Self-hosted relay** (the user runs a Cloudflare Tunnel / Tailscale / etc.) for "public WiFi at a coffee shop" cases. Optional.
- **No central server boothrflow runs.** Same posture as Obsidian's Sync vs Sync Server — we don't see your data even if we wanted to.

**Two flavors of the mobile app:**

1. **Standard tier** — on-device STT, Apple Intelligence cleanup, sync-to-desktop via iCloud Drive E2E. Fits the existing privacy promise.
2. **Hardcore privacy tier** — same but: no iCloud, LAN-only sync, MLX-hosted Qwen instead of Apple Intelligence (Apple Intelligence has been audited as on-device but the trust model is "Apple says so" — Qwen via MLX is "you can read the model weights"), Whisper or Parakeet only (no fallback to Apple's SFSpeechRecognizer which routes through Apple servers on older devices). Toggleable in Settings.

**Differentiator:**

Nobody in the dictation space ships E2E-encrypted sync with user-hosted keys. AudioPen / Wispr / Cleft all hold your transcripts on their servers in plaintext. Even the "private" iOS dictation tools are private-from-other-users, not private-from-the-vendor. Signal-style trust ("we literally cannot read your data") is novel for this category.

**Pairs naturally with:**

- **Obsidian connector** (Future ideas above) — phone dictation → encrypted sync → desktop history → Obsidian vault. One unified note corpus.
- **Meeting transcription mode** (Future ideas above) — phone records the meeting, desktop processes + indexes it.
- **Privacy audit doc** (Future ideas above) — the iOS path needs its own audit chapter; the LAN-only / no-cloud variant is genuinely auditable.

**Realistic scope:** ~6-8 weeks of focused work for the standard tier; +2-3 weeks for the hardcore-privacy tier. App Store review, Apple Developer enrollment cost overlap with Wave 6's macOS code-signing work. Not a near-term wave — sit in Future Ideas until desktop is shipped + the connector story is proven.

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

> **Shipped in Wave 4a (April 2026):** vocabulary expansion in the Whisper initial prompt, per-style aggressiveness flag (mumbling / rambling cleanup), `tok/s` telemetry from the Ollama `usage` field, Captain's Log style, and streaming partial continuation (commit-and-roll). The bullets below for those items are kept as historical detail / spec; they correspond to code that's already on `main`.

- **LLM cleanup pass** — Qwen 2.5 3B running locally via `llama-cpp-2`. Strips fillers, fixes punctuation, handles course-correction ("go to the store, I mean the office" → "go to the office").
- **Audio-pipeline noise suppression** — preprocess captured audio _before_ VAD/STT to strip background noise (HVAC hum, keyboard clack, household chatter, lossy bluetooth artifacts). Two viable options to evaluate:
  - **RNNoise** (Xiph.org, BSD) — tiny GRU-based suppressor, ~85KB model, 10ms latency, runs on CPU at <1% on M-series. Mature, used by Discord/OBS/Mumble. Rust binding via `nnnoiseless` crate (pure-Rust port). Good baseline — the question is whether it's _good enough_ on modern noise vs. paying for the heavier option.
  - **DeepFilterNet 3** (Hendrik Schröter, MIT) — newer, ONNX-based, ~10MB model, ~5ms per frame on M-series Metal, noticeably cleaner than RNNoise on non-stationary noise (chatter, music). Heavier dep, but Metal-accelerated via `ort` is straightforward.

  Plumbing: insert as an optional stage in `audio/cpal_source.rs` between mono mixdown and the resampler — the suppressor wants 16 kHz mono input, which is exactly the post-resample format, so we'd actually run it _after_ resample for a single-rate path. Toggleable from Settings (off / RNNoise / DeepFilterNet). Default off in v0, evaluate against ground-truth dictations in noisy environments before flipping the default. Pairs naturally with the existing Silero VAD: cleaner input → fewer false-positive speech frames at the start/end of an utterance, which tightens endpointing.

- **Cleanup quality refinements** _(near-term, prompted by Wave 3 dictation UAT)_ — observed gaps from Eric's hands-free dictation pass:
  - **Mumbling / rambling removal.** Filler phrases ("you know", "I mean", "uh", "kind of"), false starts, restarts, and tangential half-sentences should be cleaned up by default in non-Raw styles. Today's prompt asks the model to preserve words exactly. Need a graded mode: keep the meaning, drop the disfluency. Plumbing: a per-style `aggressiveness` flag in the system prompt (0 = preserve verbatim, 1 = drop fillers, 2 = light paraphrase). Casual default = 1.
  - ✅ **Bumped default to Qwen 2.5 7B** (April 2026). Wave 3 UAT showed 1.5B is fast (~150-300ms typical) but borderline on quality. 7B costs ~350-400ms which is below the "feels instant" threshold for cleanup. Documented as ADR-015. Users on slower boxes can now switch back via the LLM picker in Settings (Wave 4B, `509e7a7`); env-var fallback still works for headless setups.
  - **Vocabulary expansion.** The Whisper `initial_prompt` doesn't currently list "Qwen" (or "Wispr", "Tauri-Specta", "boothrflow", "MTLDevice"…). Misses on those words ride through to the LLM, which can't always recover. Action: append a curated tech-vocab chunk to `DEFAULT_INITIAL_PROMPT` and let the (future) Personal Dictionary append user-specific terms on top.
  - **Spelled-out word detection** — when the user spells a name or technical term mid-sentence ("my last name is Boothe, B-O-O-T-H-E", "the project is called Q-W-E-N as in queen with a W"), the STT often produces a sequence of letter-tokens that the LLM doesn't know to treat as authoritative. Plumbing: a pre-cleanup pass that scans the raw transcript for spelling patterns —
    - Hyphen-joined uppercase runs: `B-O-O-T-H-E`, `Q-W-E-N`
    - Space-separated single-letter sequences: `b o o t h e`, `q w e n`
    - Letter-word sequences: `bee oh oh tee aitch ee`, `cue double-u ee en`
    - NATO phonetic: `bravo oscar oscar tango hotel echo`
    - Cue phrases: "spelled", "as in", "letters"

    — collapses each detected spelling to the literal word, and emits a `<spelling>BOOTHE</spelling>` marker that the LLM cleanup prompt is told to honor as the canonical spelling for the surrounding entity. Bonus: feed confirmed spellings back into the Personal Dictionary so the next dictation gets it right at the STT layer, not the cleanup layer. Reverse pipeline: STT misses → user spells → marker created → LLM applies → dictionary learns → STT no longer misses on subsequent dictations.

  - **Connect feedback ratings to model selection.** When the rating tool ships (Phase 3), use bad-rated transcripts to flag prompts that consistently underperform; auto-suggest model upgrades when accuracy drops below a threshold.

- ✅ **OCR the focused window as cleanup context** _(shipped in Wave 5 on `feat/wave-5`)._ macOS Vision via `CGDisplayCreateImage` + `VNRecognizeTextRequest`. Permission gate added to the macOS Permissions card (Screen Recording row). Eager `CGRequestScreenCaptureAccess()` on toggle so the OS prompt fires from a clear UX moment rather than mid-dictation. OCR text is sanitized (`<` / `>` neutralized) before landing in the prompt to prevent prompt-injection via on-screen text. Windows OCR (`Media::Ocr::OcrEngine`) deferred to Wave 5d carry-over. ScreenCaptureKit pivot from the deprecated `CGDisplayCreateImage` also tracked in Wave 5d.
- ✅ **Auto-learning correction store** _(shipped in Wave 5)._ `learning::detect_correction(pasted, current)` heuristic — token-level diff, single-word swaps only, capitalization-only / short / long / multi-word / high-Levenshtein rejects, 50-entry FIFO cap. macOS AX read of the focused text field via `AXUIElementCopyAttributeValue(kAXValueAttribute)` with fallback to `kAXSelectedTextAttribute`. Re-checks the opt-in flag after the 8-second settling window so toggling off mid-window is honored. Privacy mode suppresses both at spawn site and post-sleep. Settings UI exposes both manual editing of `commonly_misheard` and the auto-learn toggle. Windows UIAutomation reader deferred to Wave 5d.

  Also exposes two user-editable lists in Settings:
  - `preferredTranscriptions` (newline-separated vocabulary, augments the Whisper `initial_prompt`)
  - `commonlyMisheard` (`wrong -> right` lines, augments the cleanup prompt's `<USER-CORRECTIONS>` block)

  Both auto-populate from the learning coordinator and accept manual edits. Direct lift from ghost-pepper's `CorrectionStore` design — proven UX.

- ✅ **Prompt prefix caching** _(shipped in Wave 5)._ `CleanupPromptInputs` builder orders blocks stable-prefix-first (rules → corrections → app context → OCR last). Ollama `keep_alive: "5m"` extra field keeps weights + KV cache resident across consecutive dictations. Heuristic-gated to port 11434 only — LM Studio (1234) and llama-server (8080) sometimes return 400 on unknown JSON keys, so they skip the field. Cloud BYOK skips it too.

- **Streaming partial continuation past the 25 s cap** _(Wave 3 UAT carry-over)_ — the pill stops updating after ~25 s because `MAX_STREAMING_SAMPLES = 16_000 * 25` and Whisper's 30 s context window starts to drop early audio. Final transcript on release is still complete; only the live display freezes. Approach: a commit-and-roll loop in `streaming.rs`. When the buffer crosses ~20 s and LA2 has a long stable prefix, freeze that prefix into a separate `frozen_text: String` field on `Inner`, trim the buffer to the last ~5 s of audio (overlap), and continue ticking. Worker emits `StreamingPartial { committed: frozen + new_committed, tentative, … }`. Bounded per-tick cost, indefinite session length, minimal boundary-word risk. Same final-pass fallback semantics. ~half-day of work.
- **Style presets** — Formal, Casual, Excited, Very Casual + custom. Two extensions queued behind the Settings panel landing (so a new style is just a new dropdown entry + new prompt branch, not a structural change):
  - **Captain's Log** _(easter-egg, ships as a victory-lap commit right after the Settings panel)_ — rewrites dictation as a Star-Trek-style log entry. Prepends `Captain's log, stardate <X>` where stardate is computed from the current real-world date (TNG-era approximation: `1000 × (year − 2323) + (day_of_year × 1000 / 365.25)`, rendered to one decimal — for 2026-04-29 we'll absolute-value the negative result or pick a fixed forward-shift offset so it reads like a future entry). Rewrites the body in formal 24th-century space-faring tone — "Set course for…", "We have detected…", "The crew is investigating…", "End log." — without changing the underlying content. Same `aggressiveness` knob as other styles to keep it from hallucinating plot. Idiom whitelist prevents invented ship names / canon characters / numeric stardate prefixes. ~1-2 hours of work because all the structural pieces (Style enum, prompt branching, Settings dropdown) are already in place.
  - **Auto-format** — see "Structured formatting (app-aware)" below; the larger of the two style extensions.
- ✅ **App-context detection** _(shipped in Wave 5)._ macOS uses `NSWorkspace::frontmostApplication()` for bundle ID + localized name; Windows uses `GetForegroundWindow` + `K32GetModuleFileNameExW`. Plumbed into the cleanup prompt's `<APP-CONTEXT>` block. Per-app style overrides via the `auto-format` style remain queued (Wave 6 Option A).
- **Structured formatting (app-aware)** — beyond punctuation. Wispr Flow's superpower is that long dictations come back as actual _structure_: bullet lists when you spoke a list, paragraph breaks when you paused, a greeting + signature in Mail, code fenced when you said "in code". Plumbing: extend the cleanup prompt with a structure-detection pass keyed on app context (Mail / Slack / Notion / IDE / generic) plus heuristics on the raw transcript ("first… second… third" → numbered list; >25s of speech → paragraph splits at sentence-boundary pause markers). Surfaces as a sixth Style ("Auto-format") that overrides tone-only styles when the model has high confidence; falls back to plain casual cleanup otherwise.
- ✅ **In-app Settings panel** — shipped in Wave 4B (`509e7a7`). Whisper / LLM / embed pickers (with parameter counts in dropdown labels), hotkey rebind UI, vocabulary editor, privacy toggle, settings export/import, persisted via `tauri-plugin-store`. See `docs/waves/wave-4b-settings-panel.md`.

- **Wave 4b polish (mostly shipped April 2026)** — items that landed across `e42da2e` and `47b0eac`, with the Specta wiring still queued as the remaining structural piece:
  - ✅ **API keys → macOS Keychain** — `keyring` 3.x with apple-native, windows-native, sync-secret-service backends. Three-state availability probe (`Unknown` / `Available` / `Unavailable`), automatic migrate-on-save from prior plaintext-JSON stores. Falls back gracefully on platforms without a backend.
  - **Full `tauri-specta` TS-binding generation** — the remaining queued piece. Currently the FE manually mirrors Rust struct shapes. ADR-007 work, finally has the right scale (~20 typed commands) to be worth wiring. Eliminates the most common drift surface. Deferred to its own session because it touches every `#[tauri::command]` + the FE binding layer.
  - ✅ **"Test connection" button on the LLM section** — `llm_test_connection` Tauri command sends a 1-token cleanup probe; renders inline OK / Failed + latency in ms.
  - ✅ **Preset chips for common LLM endpoints** — Ollama / llama.cpp / LM Studio / OpenAI / OpenRouter. Selecting fills `endpoint` and suggests a matching `model` (preserves edits afterward).
  - ✅ **Autostart toggle** — `tauri-plugin-autostart` initialized in `lib.rs`; FE calls the plugin module directly. Lives in Settings → General → "Launch at login".
  - ✅ **About section** — app version (via `app_version` command reading `CARGO_PKG_VERSION`), repo link, license. Settings export/import moved here.
  - ✅ **Sidebar nav across Settings sections** — five-section layout (General · LLM · Whisper · History · About). `activeSettingsSection` persists across opens.
  - ✅ **Move Permissions into Settings → General** — topbar Permissions button retired. Card lives under General on macOS only. Dismissable mic-blocked dashboard notice still surfaces above the fold.
  - ✅ **Equal-width workspace grid** — `.workspace-grid` from `minmax(0, 1fr) 360px` to `minmax(0, 1fr) minmax(0, 1fr)`.
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

- ✅ **NVIDIA Parakeet TDT 0.6B engine** _(shipped in Wave 5 on `feat/wave-5`, behind the `parakeet-engine` Cargo feature)._ `stt/parakeet.rs` over `sherpa-rs 0.6.8` (with `sherpa-rs-sys` pinned to the same patch — caret-version drift was breaking struct layouts). Multi-file ONNX bundle (encoder/decoder/joiner.onnx + tokens.txt). Download script auto-runs a Python metadata-propagation step (`scripts/parakeet-propagate-metadata.py`) because the published `v2-int8` bundle ships ASR metadata only on encoder.onnx and sherpa-onnx 1.10+ wants it on the decoder too. `model_type=""` in the config defers to auto-detection from encoder metadata, which routes sherpa-onnx to its NeMo TDT loader. `session::LoadedStt` is a runtime engine enum; Parakeet is offline-only (no streaming partials yet — Wave 5d carry-over against `LocalAgreement2`). Probe: `cargo run --example parakeet_probe --features "real-engines parakeet-engine"`.
  - **Default flip pending:** the Rust default is still Whisper. Once Parakeet UAT confirms parity-or-better cleanup quality at lower latency on M-series, flip the default in `default_whisper_model()` and bundle Parakeet in the production installer (vs Whisper as the multilingual fallback). Tracked alongside Wave 7 production polish.
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

## What we are deliberately not building

- **Wispr-clone on iOS.** A push-to-talk-anywhere iOS app (their model — keyboard extension, paste-into-any-app) competes with Wispr + AudioPen on their turf. We're not chasing that fight. _The mobile companion sketch in Future Ideas (capture-and-sync with E2E + on-device local) is a different product — it doubles down on privacy + memory rather than copy Wispr._
- **Vendor-controlled cloud sync.** Any sync we ship is end-to-end encrypted with user-hosted keys (Signal-style); we never see plaintext, even if we run a relay. If we can't make E2E work, we don't ship sync.
- **Team features.** Multi-user shared corpora, collaborative dictation, etc. Single-user product.
- **Voice-control automation.** Talon's territory; different problem.

## How feature decisions get made

Every architecturally-significant choice goes through an ADR ([`DECISIONS.md`](./DECISIONS.md), 15 entries so far). UATs after each phase ([`docs/uat/`](./docs/uat/)) capture what shipped, what got deferred, and why.

If you want a specific feature, open an issue with the use case. Concrete user friction beats theoretical architecture in our prioritization.
