# boothrflow

> Local-first voice dictation. Push-to-talk, transcribe, paste anywhere. All on your machine.

An open-source replacement for [Wispr Flow](https://wisprflow.ai/), built around three rules:

1. **100% local by default.** Audio and transcripts never leave your machine unless you explicitly turn on a cloud BYOK provider.
2. **Tiny footprint.** Tauri 2 + Rust. Target: ~30MB installer, ~80MB RAM idle.
3. **Persistent, searchable memory.** Every dictation goes into a local SQLite store with both lexical and semantic search _(landing in Phase 3)_.

**Status:** pre-alpha. Hot path (mic → Whisper / Parakeet STT → LLM cleanup → paste) works on Windows and macOS, with persistent history, quick-paste palette, and an in-app Settings panel. Wave 5 (context-aware cleanup: app-context, focused-window OCR, auto-learning correction store, Parakeet TDT 0.6B engine) is in UAT on [`feat/wave-5`](https://github.com/ebootheee/boothrflow/tree/feat/wave-5). **Next: Wave 6 — production polish** (code signing, auto-update, onboarding wizard, beta/stable channels). Detailed plan at [`docs/waves/wave-6-production-polish.md`](./docs/waves/wave-6-production-polish.md). After Wave 6 the project moves to a staging → stable release cadence — see [`ROADMAP.md`](./ROADMAP.md).

## Try it (macOS)

Apple Silicon first; Intel macOS best-effort.

```bash
# 1. Install dev dependencies (one-time)
xcode-select --install
# Install Rust stable via https://rustup.rs and Node 22+ via nvm, mise, or Homebrew.
brew install cmake ollama
brew services start ollama

# 2. Clone + install JS deps
git clone https://github.com/ebootheee/boothrflow
cd boothrflow
corepack enable
pnpm install

# 3. Download local models
pnpm download:model:mac   # Whisper tiny.en, ~75MB. For better quality:
                          #   pnpm download:model:mac small  (≈466MB)
                          # then `export BOOTHRFLOW_WHISPER_MODEL_FILE=ggml-small.en.bin`
pnpm ollama:pull          # qwen2.5:7b (default) + qwen2.5:1.5b (fallback) + nomic-embed-text
                          # `pnpm ollama:pull:fast` skips the 7B if disk is tight

# 4. Boot
pnpm dev
```

First boot compiles whisper.cpp from C++ source. On Apple Silicon the
Metal backend is **auto-enabled** (target.cfg in `Cargo.toml`), which
costs an extra ~1–2 minutes on the first compile but produces a 5–15×
faster STT runtime. Subsequent dev runs are much faster.

If you're on an Intel Mac, the build defaults to CPU. To force-enable
Metal on Intel (or anywhere else), opt-in via the feature flag:

```bash
cargo build --manifest-path src-tauri/Cargo.toml --features "real-engines gpu-metal"
```

### macOS permissions

Settings → General → Permissions has one-click links to each pane:

- **Microphone** — required for audio capture (`cpal`).
- **Accessibility** — required for paste injection (`enigo`) and
  the Wave 5 auto-learn coordinator's focused-field read.
- **Input Monitoring** — required for the global hotkey (`rdev` /
  `CGEventTap`) to fire when boothrflow isn't focused.
- **Screen Recording** _(optional, Wave 5)_ — only needed when the
  "focused-window OCR cleanup context" toggle is on. Requested
  eagerly via `CGRequestScreenCaptureAccess()` at the moment the
  toggle is flipped, so the OS prompt fires from a clear UX moment.

In dev mode (`pnpm dev` / `pnpm dev:parakeet`), macOS attributes the
prompts to the parent terminal — after granting, **quit and relaunch
the terminal** so the new permissions are inherited. Production
bundles ship with the matching `Info.plist` usage strings and prompt
against the boothrflow app itself, so notarized installs don't need
the relaunch dance.

**Hold to dictate**: hold `Ctrl + Cmd`, speak into TextEdit, release. Text pastes.

**Tap to toggle (hands-free)**: tap `Ctrl + Option + Space` to start a
hands-free dictation session, tap again to stop.

Open quick-paste with `Ctrl + Option + H`. _(The legacy default
`Option + Cmd + H` migrated automatically — `Cmd + H` is the
system-wide "Hide app" shortcut and macOS intercepts it before our
hotkey listener can see it.)_

## Try it (Windows, ~5 min setup)

```powershell
# 1. Install dev dependencies (one-time)
winget install Rustlang.Rustup
rustup toolchain install stable
winget install Microsoft.VisualStudio.2022.BuildTools
winget install LLVM.LLVM
winget install OpenJS.NodeJS

# 2. Clone + install JS deps
git clone https://github.com/ebootheee/boothrflow
cd boothrflow
pnpm install

# 3. Download the Whisper model (~75MB)
pnpm download:model

# 4. (Optional) Set up the LLM cleanup pass via Ollama
ollama pull qwen2.5:7b        # ~5GB, the default cleanup model
ollama pull qwen2.5:1.5b      # ~1GB, optional fallback for slow boxes

# 5. Boot
pnpm dev:msvc
```

First boot compiles whisper.cpp from C++ source (~5–10 min). Subsequent dev runs <30s.

**Hold to dictate**: hold `Ctrl + Win`, speak into Notepad, release. Text pastes.

**Tap to toggle (hands-free)**: tap `Ctrl + Alt + Space` to start a
hands-free dictation session, tap again to stop.

## Status

| Area                                                                                                                                                 | Status                                                                                                                                                              |
| ---------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Plan + 15 ADRs                                                                                                                                       | Done                                                                                                                                                                |
| Scaffold + green test suite                                                                                                                          | Done — 47 Rust + 7 FE tests passing                                                                                                                                 |
| **P1 W1**: audio + hotkey + pill                                                                                                                     | Done                                                                                                                                                                |
| **P1 W2**: VAD + Whisper STT                                                                                                                         | Done — needs ggml-tiny.en.bin                                                                                                                                       |
| **P1 W3**: paste injection + tray                                                                                                                    | Done                                                                                                                                                                |
| **P2 W4**: LLM cleanup (OpenAI-compat HTTP) + style picker                                                                                           | Done — needs Ollama or compat server                                                                                                                                |
| **Wave 3**: macOS port                                                                                                                               | Done                                                                                                                                                                |
| **Wave 4a**: cleanup quality + tok/s + streaming roll + Captain's Log                                                                                | Done                                                                                                                                                                |
| Memory / history                                                                                                                                     | Done                                                                                                                                                                |
| **LLM default**: Qwen 2.5 7B (1.5B fallback via env var or Settings)                                                                                 | Done — needs `pnpm ollama:pull`                                                                                                                                     |
| **Wave 4B**: in-app Settings panel                                                                                                                   | Done                                                                                                                                                                |
| **Wave 4b polish**: Keychain, sidebar nav, presets, Test connection, autostart, About, Permissions-in-Settings, equal-width grid, Specta TS bindings | Done                                                                                                                                                                |
| **Wave 5**: context-aware cleanup (app-context, OCR, correction store, auto-learn) + Parakeet TDT 0.6B engine                                        | In UAT on [`feat/wave-5`](https://github.com/ebootheee/boothrflow/tree/feat/wave-5) — checklist at [`docs/uat/wave-5-checklist.md`](./docs/uat/wave-5-checklist.md) |
| **Wave 6**: production polish — code signing, auto-update, onboarding wizard, beta/stable channels                                                   | Next — detailed plan at [`docs/waves/wave-6-production-polish.md`](./docs/waves/wave-6-production-polish.md) (6 phases, 6-9 days)                                   |
| **Wave 7**: pick from auto-format style / noise suppression / cleanup quality follow-ups / Wave 5 carry-overs / Parakeet-as-default                  | Queued — see [ROADMAP](./ROADMAP.md#after-wave-6--wave-7-candidates-queued)                                                                                         |
| Future ideas: Obsidian + custom connectors, hyper-modern UI rebuild, meeting mode, **iOS mobile companion (E2E + on-device local)**, plugin API      | Captured — see [ROADMAP](./ROADMAP.md#future-ideas-post-wave-7)                                                                                                     |
| **Wave 5d (carry-over)**: Windows UIA focused-field read, Windows OCR, Parakeet streaming partials, ScreenCaptureKit pivot                           | Tracked in [`docs/waves/wave-5-context-aware-cleanup.md`](./docs/waves/wave-5-context-aware-cleanup.md)                                                             |
| Linux port                                                                                                                                           | Phase 4                                                                                                                                                             |

## Documentation

- [`ROADMAP.md`](./ROADMAP.md) — what's coming, when
- [`CHANGELOG.md`](./CHANGELOG.md) — what shipped each session
- [`PLAN.md`](./PLAN.md) — full engineering plan with feature parity matrix vs Wispr Flow, latency budget, repo layout, risk register
- [`DECISIONS.md`](./DECISIONS.md) — Architecture Decision Records (15 entries)
- [`docs/uat/`](./docs/uat/) — phase-by-phase UAT reports, including manual test plans
- [`docs/waves/`](./docs/waves/) — per-wave design docs and handoff plans

## Architecture (mental model)

```
hotkey press (Ctrl+Win)
        │
        ▼
  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐
  │ Listen Pill │ ←─ │ tray status │    │ cpal capture│
  │   shown     │    │  → listening│    │ (16kHz mono)│
  └─────────────┘    └─────────────┘    └──────┬──────┘
                                               ▼
                                    ┌─────────────────┐
                                    │ Whisper (tiny.en)│
                                    └────────┬────────┘
                                             ▼
                                    ┌─────────────────┐
                                    │ ClipboardInjector│
                                    │  snapshot+paste  │
                                    │   +restore       │
                                    └─────────────────┘
                                             │
                                             ▼
                                  ─── focused app ───
```

Every cross-cutting subsystem (`AudioSource`, `Vad`, `SttEngine`, `LlmCleanup`, `Injector`, `ContextDetector`) is a Rust trait with a fake impl behind `--features test-fakes` (default, fast inner loop) and a real impl behind `--features real-engines`. Testing doesn't depend on the Windows Audio stack or Whisper being installed.

## Prerequisites (developers)

### Both platforms

- **Node 22+** and **pnpm 9+**
- **Rust stable** (`rustup` from rustup.rs or `winget install Rustlang.Rustup`)
- **cargo-nextest** — `cargo install cargo-nextest --locked`
- **lefthook** — installed automatically by `pnpm install`

### Windows-specific (for the `real-engines` feature)

`whisper-rs` and other native ML deps use `bindgen` which needs libclang and Windows SDK headers. Plain `cargo build` from a non-VS-dev shell doesn't get them, so we ship `scripts/cargo-msvc.bat` to wrap cargo with the env pre-loaded.

```powershell
winget install Microsoft.VisualStudio.2022.BuildTools     # MSVC + Win SDK
winget install LLVM.LLVM                                   # libclang for bindgen
```

Then any tool via the wrapper:

```bat
scripts\cargo-msvc.bat cargo build --features real-engines
scripts\cargo-msvc.bat cargo nextest run --features real-engines
scripts\cargo-msvc.bat pnpm exec tauri dev
```

`pnpm dev:msvc`, `pnpm build:msvc`, `pnpm test:rust:real` use the wrapper. The fast inner-loop fakes-only path (`pnpm test:rust`, `pnpm test:fe`) works in any shell because `test-fakes` doesn't compile the heavy native deps.

### macOS

Install Xcode Command Line Tools, `cmake`, and Ollama:

```bash
xcode-select --install
brew install cmake ollama
brew services start ollama
pnpm download:model:mac
pnpm ollama:pull
```

`pnpm dev`, `pnpm test:rust:real`, and plain `cargo check --features
real-engines` work directly — clang and SDK headers are auto-discovered.
If bindgen cannot find libclang, `brew install llvm` and set
`LIBCLANG_PATH=$(brew --prefix llvm)/lib`.

macOS uses `Ctrl + Cmd` for hold-to-talk and `Ctrl + Option + H` for
quick-paste. If the hotkey or paste does nothing, grant Accessibility
permission in `System Settings → Privacy & Security → Accessibility`.
If audio capture fails, grant Microphone permission in the same Privacy
& Security panel.

### Linux

`pnpm dev`, `pnpm test:rust:real`, and `pnpm download:model:linux`
are the intended bring-up path. Linux-specific focus restore and tray
polish land in Wave 4.

## Contributing

[Conventional Commits](https://www.conventionalcommits.org/). Small PRs (~200–400 LoC). Branch protection on `main` requires green CI.

See [ADR-006](./DECISIONS.md#adr-006--workflow-conventional-commits-small-prs-no-stacked-pr-tooling) for the full workflow.

## LLM cleanup pass

The transcript is run through a small LLM (Qwen 2.5 7B by default) for
punctuation, capitalization, run-on-sentence splitting, disfluency
removal, and contextual word correction. We talk to it over the
**OpenAI-compatible chat-completions API** so it can be backed by
whatever you already have running:

| Backend                    | Endpoint                                     | Notes                                                                                                   |
| -------------------------- | -------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| **Ollama** (default)       | `http://localhost:11434/v1/chat/completions` | `ollama pull qwen2.5:7b` and you're done. GPU offload automatic. ~350-400 ms per dictation on M-series. |
| `llama-server` (llama.cpp) | `http://localhost:8080/v1/chat/completions`  | Set `BOOTHRFLOW_LLM_ENDPOINT`                                                                           |
| LM Studio                  | `http://localhost:1234/v1/chat/completions`  | same                                                                                                    |
| OpenAI / Anthropic / Groq  | their cloud URL                              | set `BOOTHRFLOW_LLM_API_KEY` (BYOK)                                                                     |

Override defaults with environment variables:

```
BOOTHRFLOW_LLM_ENDPOINT=http://localhost:11434/v1/chat/completions
BOOTHRFLOW_LLM_MODEL=qwen2.5:7b              # or qwen2.5:1.5b on slow boxes
BOOTHRFLOW_LLM_API_KEY=...                   # only for cloud
BOOTHRFLOW_LLM_DISABLED=1                    # skip cleanup entirely
```

If the LLM server is down or the model isn't loaded, the pipeline falls
back to the raw Whisper transcript with a `tracing::warn` — you stay
unblocked even when the LLM isn't available.

## License

[Apache 2.0](./LICENSE) — permissive, with patent grant. See [`NOTICE`](./NOTICE) for third-party model and library attributions.

---

_Built by [Eric Boothe](https://github.com/ebootheee) with [Claude Code](https://claude.com/claude-code)._
