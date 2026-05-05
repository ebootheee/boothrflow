# Changelog

User-facing changes per session, most recent at the top. Engineering
detail and rationale lives in commits + the per-wave docs under
`docs/waves/`. This file is for humans skimming "what shipped".

## 2026-05-05 — Wave 6 Phase 0 + small-fixes sweep

### Added

- **Wave 6 Phase 0 — structure-aggressiveness style overhaul** (commit `d71cb90`). New `Style` enum: **Raw / Light / Moderate / Assertive** + Captain's Log retained as orthogonal preset. Replaces the tone-based system (casual / formal / very-casual / excited). Old persisted settings auto-migrate via serde aliases (no manual fix-up needed). New cleanup prompts per level. Settings UI shows a 4-option segmented control with help text per level. Captain's Log under "Fun presets" disclosure. `bench:replay` fans out across all 4 structure styles + raw.
- **History detail → inline expand-under-row** (commit `60bb2b0`). Old side-by-side detail panel could overflow viewport at narrow widths. Now: click a row, detail expands beneath; click same row to collapse; click another to swap. Caret glyph (▸ / ▾) signals state.
- **Cleanup chip tok/s fallback** (commit `60bb2b0`). When the LLM backend reports `completion_tokens` + `llm_ms` but skips the explicit `tok_per_sec` field, the FE now derives tok/s from those instead of silently dropping it.
- **Bluetooth-aware mic default + manual override** (commit `a7302de`). When system default input is a Bluetooth mic (AirPods / Beats / Sony WH/WF / Bose), boothrflow now silently uses the built-in mic instead — avoids the macOS HFP downgrade that dims music for ~30 seconds. New Settings → General → **Microphone** section with a device-picker dropdown + "Use built-in mic when Bluetooth headphones are connected" toggle (default on). User can pin any specific device explicitly via the dropdown to override the auto-pick.
- **Assertive prompt tightening + small-LLM auto-upgrade** (commit `4ba7e95`). First bench grading exposed three failure modes: invented `### Section` headers when the speaker had no transitions, fake "Hi <name>... Best, [Your Name]" Mail signatures in non-Mail contexts, and "Sure, here is the formatted text:" preambles from qwen 1.5b. New prompt makes every structuring permission CONDITIONAL on its trigger being present (transition cues for headers, listing cues for bullets, Mail-app context for greetings) + explicit anti-pattern bans (no preambles, no `[Your Name]` placeholders, no horizontal rules, no closing summaries). Inline backticks on filenames also banned across Light / Moderate / Assertive — Claude Code's chat input was rewriting `` `devops.md` `` into `[devops.md](http://devops.md)` markdown links on paste.
- **Auto-upgrade qwen 0.5b/1.5b/3b → 7b for Assertive only** (commit `4ba7e95`). Small models can't follow Assertive's nuanced rules. The user's configured default stays unchanged for the other styles; only Assertive routes through the upgrade. Bench `bench:replay` does NOT upgrade (preserves the 1.5b-on-Assertive variant for grading).
- **Bench harness warmup pass** (commit `4ba7e95`). `bench:replay` now loads the engine once per config and runs a throwaway 1-second silence pass before timing the real transcription. Without this, first-tested engine paid model load + GPU context init, inflating its `stt_ms` ~10x (whisper-tiny.en went 6.3s → 770ms between two runs of identical audio in the first round).

### Fixed

- **Honest tok/s display** — Cleanup chip now shows tok/s in more cases (any LLM that reports `completion_tokens` will get a derived tok/s, not just ones that ship the explicit field).
- **Music-dimming on Bluetooth output** — see Bluetooth mic default above. Was a real macOS HFP behavior, not a boothrflow bug, but the default behavior now sidesteps it.

### Changed

- **Settings → General → Style** picker is now a segmented control (4 buttons) rather than a 6-option dropdown. Visual reinforcement of "level" mental model. Help text per option.
- **Defaults for new installs**: structure-aggressiveness = `Light` (default in the migration map for legacy `casual` / `very-casual` / `excited`). Mic = "Auto" (system default with Bluetooth-aware fallback). Toggle = on.

### Lessons learned (carried into Phase 1+ planning)

- **Bench cold-start ordering bias was real and significant** — first-tested engine ate the model-load cost, looked 10x slower than the others. Phase 3 `bench harness hardening` partially shipped (warmup); N=3 + median + aggregate-leaderboard still pending.
- **Small LLMs can't follow Assertive's nuanced rules** — qwen 1.5b on long Assertive prompts emits placeholders, fake signatures, preambles. The auto-upgrade is the right pattern; we'll likely need similar guardrails for any future structure-heavy style.
- **Plain-prose backticks on filenames cause more problems than they solve** — even when the destination renders markdown (Slack, Notion, Obsidian), apps like Claude Code interpret them as auto-link triggers. Plain text is the safe default; code fences for actual code only.

## 2026-05-04 — wave reordering (Wave 6 ↔ 7 swap, Wave 8 added)

### Changed

- **Swapped Wave 6 and Wave 7.** New ordering: **Wave 6 = engine + formatting** (was old Wave 7), **Wave 7 = production polish** (was old Wave 6). Reasoning: dial in the engine and the cleanup pass _before_ packaging it into a signed installer. Better one user (Eric) on a fast iteration loop with the right engine than ten users on a polished installer of a placeholder.
- **Branch + doc renames** — `feat/wave-7-streaming-stt` → `feat/wave-6-engine-and-formatting`. `docs/waves/wave-7-streaming-stt.md` → `docs/waves/wave-6-engine-and-formatting.md`. `docs/waves/wave-6-production-polish.md` → `docs/waves/wave-7-production-polish.md`.

### Added

- **New Phase 0 in Wave 6: style overhaul.** Replace the tone-based `Style` enum (casual / formal / very-casual / excited / raw / captains-log) with a single **structuring-aggressiveness axis**: raw / light / moderate / assertive. Tone variation turned out to be noise; users actually vary how aggressively the LLM should structure output. `Assertive` adopts Wispr's auto-format playbook (bullets when listing, paragraph breaks at sentence-boundary pauses, code fences for "in code" cues, greeting + sign-off when focused app is Mail). Captain's Log retained as an orthogonal fun preset. Day-one work, testable immediately.
- **New Wave 8 — Connectors + UI rebuild + privacy audit.** Pulls forward three items from Future Ideas into a dedicated wave: (1) Connector trait + Obsidian vault push + custom HTTP webhook + Slack incoming webhooks + voice-triggered routing + History row push action; (2) hyper-modern UI rebuild (visual language refresh, pill redesign, Liquid Glass / NSVisualEffectView vibrancy on macOS, Cmd-K command palette, keyboard shortcuts); (3) `PRIVACY_AUDIT.md` with pre-written AI-assistant verification prompt + default-features checklist + BYOK callouts + telemetry confirmation + pass/fail table. Plan: [`docs/waves/wave-8-connectors-ui-privacy.md`](./docs/waves/wave-8-connectors-ui-privacy.md).

### Removed

- **From "Future Ideas":** Connectors section, Hyper-modern UI rebuild section, Privacy audit doc section (all promoted to Wave 8). "Parakeet → default engine" candidate (already done — landed as part of the Wave 5 → main merge).

## 2026-05-04 (Wave 5 → main, Wave 7 plan)

### Added

- **Wave 5 merged to main** (`feat/wave-5` → `main` via `--no-ff`, commit `763d370`). 20 commits covering context-aware cleanup, Parakeet TDT 0.6B v2 engine, post-paste learning coordinator, macOS Vision OCR, focused-AX read, captures-to-disk, bench replay tool, in-app grading UI.
- **Developer mode flag (`BOOTHRFLOW_DEV=1`)** replaces `BOOTHRFLOW_SAVE_CAPTURES`. Single umbrella that gates capture saving + Benchmarks tab visibility + future dev-only surfaces. Production builds default the tab off; devs flip the env var to unlock.
- **Multi-LLM bench fan-out.** `bench:replay` now iterates across qwen2.5:7b + qwen2.5:1.5b (configurable list) instead of just the user's currently-configured model. Raw style emits one variant per STT (no LLM dependency) instead of one per (STT × LLM).
- **Wave 7 plan committed** — [`docs/waves/wave-7-streaming-stt.md`](./docs/waves/wave-7-streaming-stt.md). Two parallel tracks targeting the streaming + cold-start gaps offline Parakeet exposed: Phase 1 Nemotron Speech Streaming via sherpa-onnx (3-5d), Phase 2 parakeet.cpp evaluation on Apple Silicon (2-3d), Phase 3 bench harness hardening — warmup pass + N=3 median (1d). Default STT for production gets re-decided from leaderboard grades after both phases land.

### Changed

- **Defaults: Parakeet TDT 0.6B v2 + qwen2.5:7b for production builds.** Inner-loop dev (no `parakeet-engine` feature) still defaults to whisper tiny.en so the dev loop stays light. Driven by Wave 5 bench results: on a 116s Lysara dictation, Parakeet was the only engine that got the named entity right and avoided "paste" → "pay" semantic substitution.

### First-run benchmark findings (Lysara capture, 116s)

- whisper:tiny.en → "LISAR", "pay" (semantically wrong); STT 770ms (post-warmup); LLM 7b 8.8s, 1.5b 3.1s.
- whisper:base.en → "Lysara" ✓, "pay" (still wrong); STT 851ms; LLM 7b 4.0s, 1.5b 1.5s.
- parakeet:0.6b-v2-int8 → all entities + verbs correct; STT 13.5s (load + decode, consistent across runs); LLM 7b 4.2s, 1.5b 1.4s.
- qwen2.5:1.5b is ~3× faster than 7b across every variant but **dropped trailing content** on the parakeet+1.5b cleanup (cut last sentence + middle phrase). Suggests a "1.5b for short utterances, 7b for long" heuristic for a future setting.

## 2026-05-03 (planning)

### Added

- **Performance baseline + benchmark harness** added to Wave 7 candidates as the recommended first pick. Vendored test-wav set, `cargo run --example bench` binary that loops the set through each (engine × LLM-config × style) combo and emits a CSV, markdown report generator, `docs/benchmarks/baseline-YYYY-MM-DD.md` snapshots for trend tracking. Gates every subsequent engine swap — without numbers, "is engine X better?" stays a vibes call.
- **STT engine evaluations subsection** in Future Ideas. Captures the NVIDIA NeMo model family worth measuring against our baseline once it exists:
  - Parakeet TDT 0.6B v3 (multilingual; 25 EU languages with auto language detection)
  - Nemotron Speech Streaming (low-latency streaming with native punctuation — most strategic option, could replace Whisper streaming AND skip the LLM cleanup pass)
  - Multitalker Parakeet (multi-speaker ASR for meeting mode — collapses STT + diarization)
  - Parakeet Realtime EOU (120M streaming model with end-of-utterance detection — Silero VAD upgrade for tap-to-toggle)
  - Canary multilingual translation (powers a "Translate to English / Spanish / etc" Style preset)
- **Wave 7 candidates: multilingual Whisper variants + Parakeet 1.1B English** added. Both small. Multilingual Whisper unblocks non-English without forcing BYOK; Parakeet 1.1B is a power-user precision option for M-series Pro/Max.
- **"Parakeet → default engine" candidate gated on benchmark numbers** explicitly. No swap without measurement.

### Changed

- **Honesty pass on Parakeet labeling.** The bundle we ship is the v2 ONNX export of NeMo Parakeet TDT 0.6B (English only) — the directory and settings identifier were aspirationally named `parakeet-tdt-0.6b-v3` but the actual model files inside are v2. User-facing strings (download-script messages, picker label, ROADMAP entries) now say "Parakeet TDT 0.6B (preview)" without claiming v3. Internal identifiers (`parakeet-tdt-0.6b-v3` directory + settings value) preserved so existing installs don't break; v3 multilingual moves to Future Ideas as a follow-up bundle swap.

## 2026-05-02 (planning)

### Added

- **Wave 6 plan committed** — [`docs/waves/wave-6-production-polish.md`](./docs/waves/wave-6-production-polish.md). Six phases (release infrastructure → macOS signing → auto-update → Windows signing → onboarding wizard → beta/stable channels), 6-9 days total. Each phase independently shippable. After Wave 6 the project moves to a staging → stable release cadence.
- **Future-ideas section in ROADMAP.md** — Obsidian + custom connectors (push dictations / embeddings to a vault, voice-trigger routing, history-row push action), hyper-modern UI rebuild (Settings + pill redesign, Liquid Glass / Vibrancy on macOS, command palette, keyboard shortcuts), meeting transcription mode, plugin API, insights dashboard, snippets, voice commands, privacy audit doc, Linux port. Captured so they don't get lost; not committed.
- **iOS mobile companion** added to Future Ideas. Not a Wispr-clone — a private capture-and-sync surface for the same searchable corpus the desktop owns. On-device STT (WhisperKit / sherpa-onnx via CoreML) + on-device cleanup (Apple Intelligence FoundationModels or MLX-hosted Qwen 1.5B Q4) + end-to-end encrypted sync with user-hosted keys (Signal-style trust model). Two flavors: standard tier (iCloud-Drive E2E sync), hardcore-privacy tier (LAN-only sync + MLX cleanup + Whisper/Parakeet only). Pairs natively with the Obsidian connector idea.

### Changed

- **Wave 6 plan: auto-update pulled into the early-Wave-6 bundle** (Phase 3 instead of Phase 4). Unsigned auto-update is broken UX — every update re-triggers Gatekeeper's "Open Anyway" dance. Pairing auto-update directly with macOS signing means the first three phases together ship a working release loop on Eric's daily driver. Windows signing slips to Phase 4 — can lag a release.
- **"Deliberately not building" list refined** — removed the blanket "Mobile" exclusion in favor of "Wispr-clone on iOS" specifically (the mobile companion is a different product). Added "Vendor-controlled cloud sync" — any sync we ship is E2E with user-hosted keys, or we don't ship sync.

## 2026-05-02

### Added

- `feat/wave-5` branch is in UAT. Six commits covering
  context-aware cleanup, auto-learning corrections, focused-window
  OCR, and the Parakeet TDT 0.6B engine. Walk-through in
  [`docs/uat/wave-5-checklist.md`](./docs/uat/wave-5-checklist.md).
- **App-context detection** — cleanup prompt now carries an
  `<APP-CONTEXT>` block with the foreground app's bundle ID
  (macOS) or executable name (Windows). LLM tone-matches the
  destination.
- **Common mishearings editor** — Settings → Voice → Recognition
  has a wrong → right substitution editor. Pairs land in the
  cleanup prompt's `<USER-CORRECTIONS>` block as authoritative.
- **Auto-learn corrections after paste** — opt-in Settings toggle.
  After pasting, watches the focused field for ~8 seconds (via
  macOS Accessibility API). On a small single-word edit, records
  the pair into `commonly_misheard` automatically. Privacy-mode
  suppressed; capped at 50 entries.
- **Focused-window OCR cleanup context** — opt-in Settings toggle.
  macOS Vision (`CGDisplayCreateImage` + `VNRecognizeTextRequest`)
  reads on-screen text and feeds it to the cleanup prompt as
  supporting context for disambiguating names / model IDs / file
  names. Eager Screen Recording permission prompt at toggle time.
  OCR text is sanitized against prompt-injection (`<` / `>`
  neutralized) before it lands in the prompt.
- **NVIDIA Parakeet TDT 0.6B engine** behind the `parakeet-engine`
  Cargo feature. `pnpm dev:parakeet` builds with it; `pnpm
download:model:mac parakeet` fetches and prepares the bundle
  (auto-runs a Python metadata-propagation step because the
  published `v2-int8` bundle ships ASR metadata only on
  encoder.onnx and sherpa-onnx 1.10+ wants it on the decoder
  too). Selectable in Settings → Voice → Recognition once built
  with the feature.
- **Prompt prefix caching** via Ollama `keep_alive: 5m`. Stable
  prefix order in the cleanup prompt makes second-and-later
  dictations within a 5-minute window noticeably faster (KV cache
  - model weights stay resident). Heuristic-gated to port 11434
    so LM Studio / llama-server users aren't broken by the
    unknown-field rejection some compat layers do.
- **Screen Recording row** in Settings → General → Permissions.
- New Tauri commands: `screen_recording_available`,
  `request_screen_recording_permission`.
- New Cargo feature: `parakeet-engine` (off by default — opts into
  the sherpa-onnx prebuilt download).
- New scripts: `scripts/parakeet-propagate-metadata.py`,
  `cargo run --example parakeet_probe`.
- New docs: `docs/uat/wave-5-checklist.md`,
  `docs/waves/wave-5-context-aware-cleanup.md`.

### Changed

- **Quick-paste hotkey default** changed from `Option + Cmd + H`
  / `Alt + Win + H` to `Ctrl + Option + H` / `Ctrl + Alt + H`.
  The legacy default conflicted with the macOS system-wide
  `Cmd + H` "Hide app" shortcut — AppKit intercepted the
  keypress before our `rdev` listener could see it. Migration
  rewrites the saved default on next load; user-chosen chords
  are preserved.
- **"Whisper model" picker** renamed to "Speech-to-text model" —
  Parakeet is now a peer engine.
- **Test connection** button moved above the OCR toggle in
  Settings → Voice → LLM (it was previously below an unrelated
  cleanup-context feature).
- **Privacy mode** now suppresses three new context channels
  beyond the existing LLM-cleanup gate: focused-window OCR
  capture, app-context propagation, and the auto-learn
  coordinator.

### Fixed

- **Test connection no longer panics with "Cannot drop a runtime
  in a context where blocking is not allowed".** The
  `reqwest::blocking::Client` was being constructed and dropped
  on the async caller's tokio worker thread. Construction +
  drop now happens entirely inside `spawn_blocking`.
- **Quick-paste palette fires on first press.** `rdev` was
  losing modifier-down events on macOS for tap-style chords
  (the heartbeat resync only fired every 150ms, which a fast
  user beats). Eagerly resync from `CGEventSourceFlagsState`
  on every fresh non-modifier KeyPress.
- **Quick-paste palette no longer has white corners.** The shared
  `app.css` paints `#app` with the light app background; the
  quickpaste window's transparent override now also covers
  `#app`.
- **`isMac` detection in Settings** falls back to `userAgent`
  when `navigator.platform` is empty (deprecated in modern
  WebKit and intermittent in Tauri WKWebView).
- **Parakeet model-load no longer crashes the app** when given
  a bundle that's missing the `vocab_size` metadata field.
  Engine pre-checks for it and returns a graceful
  `BoothError` instead of letting sherpa-onnx C++ call
  `exit(-1)` during decode.

### Security

- OCR prompt-injection defense: `<` / `>` in OCR'd text are
  replaced with U+2039 / U+203A so an attack string containing
  `</WINDOW-OCR-CONTENT>` can't close the block and inject fake
  follow-on instructions.

## Earlier sessions

For the full session-by-session history before Wave 5, see
[`docs/uat/`](./docs/uat/) (one report per phase / wave) and the
git log:

```bash
git log --oneline --no-merges
```

Notable prior landings:

- **Wave 4b polish** (April 2026): Keychain for API keys, sidebar
  nav, LLM endpoint presets, Test connection button, autostart
  toggle, About section, Permissions moved into Settings, equal-
  width workspace grid, full Tauri-Specta TS-binding generation.
- **Wave 4B** (April 2026): in-app Settings panel — model pickers
  with parameter-count labels, hotkey rebind UI, vocabulary
  editor, privacy toggle, settings export/import.
- **Wave 4a** (April 2026): cleanup quality (per-style
  aggressiveness flag), `tok/s` telemetry from Ollama's `usage`
  field, streaming-partial commit-and-roll past the 25 s cap,
  Captain's Log style, expanded Whisper initial-prompt vocab.
- **Wave 3** (April 2026): macOS port — Apple Silicon Metal
  default, tap-to-toggle hotkey, hotkey-resync heartbeat for
  Cmd-Tab races, two-line pill, permissions card.
- **Phase 1 W1–W3**: audio + hotkey + pill, VAD + Whisper STT,
  paste injection + tray.
- **P3 W7–W8**: persistent history (SQLite + FTS5 +
  nomic-embed-text), quick-paste palette.
