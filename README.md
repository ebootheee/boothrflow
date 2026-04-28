# boothrflow

> Local-first voice dictation. Push-to-talk, transcribe, paste anywhere. All on your machine.

An open-source replacement for [Wispr Flow](https://wisprflow.ai/), built around three rules:

1. **100% local by default.** Audio and transcripts never leave your machine unless you explicitly turn on a cloud BYOK provider.
2. **Tiny footprint.** Tauri 2 + Rust. Target: ~30MB installer, ~80MB RAM idle.
3. **Persistent, searchable memory.** Every dictation goes into a local SQLite store with both lexical and semantic search _(landing in Phase 3)_.

**Status:** pre-alpha. Phase 1 hot path (mic → Whisper STT → paste) is working; Phase 2 (LLM cleanup, styles, app-context) is next. See [`ROADMAP.md`](./ROADMAP.md).

## Try it (Windows, ~5 min setup)

```powershell
# 1. Install dev dependencies (one-time)
winget install Rustlang.Rustup
rustup toolchain install stable
winget install Microsoft.VisualStudio.2022.BuildTools
winget install LLVM.LLVM
winget install OpenJS.NodeJS

# 2. Download the Whisper model (~75MB)
$dest = "$env:APPDATA\boothrflow\models\ggml-tiny.en.bin"
New-Item -ItemType Directory -Force (Split-Path $dest) | Out-Null
Invoke-WebRequest "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en.bin" -OutFile $dest

# 3. Clone + boot
git clone https://github.com/ebootheee/boothrflow
cd boothrflow
pnpm install
pnpm dev:msvc
```

First boot compiles whisper.cpp from C++ source (~5–10 min). Subsequent dev runs <30s.

Hold `Ctrl + Win`, speak into Notepad, release. Text pastes.

## Status

| Area                                 | Status                              |
| ------------------------------------ | ----------------------------------- |
| Plan + 13 ADRs                       | Done                                |
| Scaffold + green test suite          | Done — 22 Rust + 7 FE tests passing |
| **P1 W1**: audio + hotkey + pill     | Done                                |
| **P1 W2**: VAD + Whisper STT         | Done — needs ggml-tiny.en.bin       |
| **P1 W3**: paste injection + tray    | Done                                |
| **P2 W4**: LLM cleanup (Qwen 2.5 3B) | Next                                |
| Memory / history                     | Phase 3                             |
| Mac + Linux                          | Phase 4                             |

## Documentation

- [`ROADMAP.md`](./ROADMAP.md) — what's coming, when
- [`PLAN.md`](./PLAN.md) — full engineering plan with feature parity matrix vs Wispr Flow, latency budget, repo layout, risk register
- [`DECISIONS.md`](./DECISIONS.md) — Architecture Decision Records (13 entries)
- [`docs/uat/`](./docs/uat/) — phase-by-phase UAT reports, including manual test plans

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

### macOS / Linux

`pnpm dev`, `pnpm test:rust:real` work directly — clang and SDK headers are auto-discovered. (Windows is the awkward one because bindgen wants the SDK paths set up before invocation.)

## Contributing

[Conventional Commits](https://www.conventionalcommits.org/). Small PRs (~200–400 LoC). Branch protection on `main` requires green CI.

See [ADR-006](./DECISIONS.md#adr-006--workflow-conventional-commits-small-prs-no-stacked-pr-tooling) for the full workflow.

## License

[Apache 2.0](./LICENSE) — permissive, with patent grant. See [`NOTICE`](./NOTICE) for third-party model and library attributions.

---

_Built by [Eric Boothe](https://github.com/ebootheee) with [Claude Code](https://claude.com/claude-code)._
