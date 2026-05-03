# Changelog

User-facing changes per session, most recent at the top. Engineering
detail and rationale lives in commits + the per-wave docs under
`docs/waves/`. This file is for humans skimming "what shipped".

## 2026-05-02 (planning)

### Added

- **Wave 6 plan committed** — [`docs/waves/wave-6-production-polish.md`](./docs/waves/wave-6-production-polish.md). Six phases (release infrastructure → macOS signing → Windows signing → auto-update → onboarding wizard → beta/stable channels), 6-9 days total. Each phase independently shippable. After Wave 6 the project moves to a staging → stable release cadence.
- **Future-ideas section in ROADMAP.md** — Obsidian + custom connectors (push dictations / embeddings to a vault, voice-trigger routing, history-row push action), hyper-modern UI rebuild (Settings + pill redesign, Liquid Glass / Vibrancy on macOS, command palette, keyboard shortcuts), meeting transcription mode, plugin API, insights dashboard, snippets, voice commands, privacy audit doc, Linux port. Captured so they don't get lost; not committed.

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
