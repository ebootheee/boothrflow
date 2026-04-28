# Roadmap

> Where we are and where we're going. The detailed engineering plan lives in [`PLAN.md`](./PLAN.md); this is the user-facing summary.

## Current state — Phase 1 complete (April 2026)

The core push-to-talk dictation loop works end-to-end on Windows.

- Hold `Ctrl + Win`, speak, release → transcript pastes into the focused app
- Whisper-tiny-en STT (~75MB local model)
- Tray icon + pause toggle
- Live transcript panel in the main window
- 100% local: no audio or transcripts leave your machine

**What's missing:** LLM cleanup (filler removal, punctuation), styles, app-context-aware formatting, history, multilingual STT, polish on edge cases. See below.

---

## Phase 2 — Intelligence layer (weeks 4–6)

Goal: feels like Wispr Flow.

- **LLM cleanup pass** — Qwen 2.5 3B running locally via `llama-cpp-2`. Strips fillers, fixes punctuation, handles course-correction ("go to the store, I mean the office" → "go to the office").
- **Style presets** — Formal, Casual, Excited, Very Casual + custom.
- **App-context detection** — `GetForegroundWindow` + UI Automation to detect Slack vs Gmail vs IDE, applies the right style automatically.
- **Personal dictionary** — manual add + auto-learn from your post-edits. Hot-word boost via Whisper's `initial_prompt` trick.
- **Skip-LLM hotkey** — explicit "raw mode" for code dictation.

## Phase 3 — Memory & differentiators (weeks 7–9)

Goal: beats Wispr Flow on memory.

- **Searchable history** — every dictation persisted in SQLite + FTS5.
- **Semantic recall** — `bge-small-en-v1.5` embeddings + `sqlite-vec` for hybrid lexical + semantic search.
- **Quick-paste palette** — `Ctrl+Win+H` opens a fuzzy-search overlay; pick a past dictation and paste it.
- **Command Mode** — highlight text + hold-to-speak a transformation ("make this more concise", "translate to Spanish").
- **Voice commands** — "press enter", "new line", "delete that", "select all".

## Phase 4 — Production polish (weeks 10–12)

Goal: 1.0.

- **NVIDIA Parakeet TDT 0.6B v3** as default STT (faster, more accurate, native streaming) via `sherpa-onnx`. Whisper becomes the multilingual fallback.
- **TEN-VAD** swap-in (faster endpoint detection than Silero).
- **Onboarding wizard** — model download, mic test, hotkey config, accessibility permissions (macOS), Windows SmartScreen explainer.
- **Code signing** — Azure Trusted Signing on Windows, Developer ID + notarization on macOS.
- **Auto-update** — `tauri-plugin-updater` + GitHub Releases.
- **macOS port** — WhisperKit on Apple Neural Engine, AXUIElement for paste injection.
- **Linux port** — sherpa-onnx works the same; X11 + Wayland injection paths.

## Beyond v1

- **Snippets** — voice-activated text expanders.
- **Plugin API** — pre-STT, post-STT, pre-paste hooks (WASM-sandboxed).
- **LoRA fine-tuning** on your own dictation history (opt-in).
- **"Whisper Mode"** — sub-audible speech (custom acoustic model required).
- **Insights dashboard** — words/day, accuracy delta, top apps.
- **File tagging in Cursor / Windsurf** — `@file` syntax injection when you mention a filename.

## What we are deliberately not building

- Mobile (Wispr Flow's edge; we're desktop-first).
- Cloud sync of dictionary/snippets across devices (local-first means the data stays here).
- Team features.
- Voice-control automation (Talon's territory; different problem).

## How feature decisions get made

Every architecturally-significant choice goes through an ADR ([`DECISIONS.md`](./DECISIONS.md), 13 entries so far). UATs after each phase ([`docs/uat/`](./docs/uat/)) capture what shipped, what got deferred, and why.

If you want a specific feature, open an issue with the use case. Concrete user friction beats theoretical architecture in our prioritization.
