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

Every architecturally-significant choice goes through an ADR ([`DECISIONS.md`](./DECISIONS.md), 14 entries so far). UATs after each phase ([`docs/uat/`](./docs/uat/)) capture what shipped, what got deferred, and why.

If you want a specific feature, open an issue with the use case. Concrete user friction beats theoretical architecture in our prioritization.
