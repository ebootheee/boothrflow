# boothrflow

> Local-first, open-source voice dictation. Push-to-talk → transcribe → format → paste anywhere → searchable history. All on your machine.

**Status:** pre-alpha. Scaffolding in place; hot path not yet wired.

## What is this?

A free and open replacement for [Wispr Flow](https://wisprflow.ai/), built around three rules:

1. **100% local by default.** Audio never leaves your machine unless you explicitly turn on a cloud BYOK provider.
2. **Tiny footprint.** Tauri + Rust. Target: ~30MB installer, ~80MB RAM idle.
3. **Persistent, searchable memory.** Every dictation goes into a local SQLite store with both lexical and semantic search.

See [`PLAN.md`](./PLAN.md) for the full architecture and roadmap.
See [`DECISIONS.md`](./DECISIONS.md) for ADRs.

## Status

| Area                         | Status                              |
| ---------------------------- | ----------------------------------- |
| Plan + ADRs (12 ADRs)        | Done                                |
| Scaffold + green test suite  | Done — 22 Rust + 7 FE tests passing |
| P1 W1: audio + hotkey + pill | Done                                |
| P1 W2: VAD + Whisper STT     | Done — needs ggml-tiny.en.bin       |
| P1 W3: paste injection       | Next                                |
| LLM cleanup                  | Fakes wired; real engine Phase 2 W4 |
| Memory / history             | Not started — Phase 3 W7            |

## Getting a Whisper model

The tiny English model (~75MB) is the dev default. After `pnpm install`:

```powershell
$dest = "$env:APPDATA\boothrflow\models\ggml-tiny.en.bin"
New-Item -ItemType Directory -Force (Split-Path $dest) | Out-Null
Invoke-WebRequest "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en.bin" -OutFile $dest
```

Or any equivalent `curl`. The app will report "Whisper model not loaded"
in the UI if the file is missing; transcription stays gracefully degraded
until the file appears.

## Prerequisites (developers)

### Both platforms

- **Node 22+** and **pnpm 9+**
- **Rust stable** — `winget install Rustlang.Rustup` (Windows) or rustup.rs
- **cargo-nextest** — `cargo install cargo-nextest --locked`
- **lefthook** — installed automatically by `pnpm install`

### Windows-specific (for the `real-engines` feature build)

`whisper-rs` and other native ML deps use `bindgen` which needs libclang and
the Windows SDK headers. Plain `cargo build` from a non-VS-dev shell doesn't
get these, so we ship `scripts/cargo-msvc.bat` to wrap cargo with the env
pre-loaded.

One-time install:

```powershell
winget install Microsoft.VisualStudio.2022.BuildTools     # MSVC + Win SDK
winget install LLVM.LLVM                                   # libclang for bindgen
```

Then run any cargo command via the wrapper:

```bat
scripts\cargo-msvc.bat build --features real-engines
scripts\cargo-msvc.bat nextest run --features real-engines
```

The `pnpm dev:msvc` and `pnpm build:msvc` and `pnpm test:rust:real` scripts
use the wrapper. For the fast inner-loop fakes-only path, `pnpm test:rust`
works in any shell because it doesn't compile the heavy native deps.

### macOS / Linux

`pnpm dev`, `pnpm test:rust:real`, etc. work directly — `whisper-rs`'s build
finds clang via the system toolchain. (Windows is the awkward one because
bindgen wants the SDK paths set up before invocation.)

### Optional but recommended

- `cargo-watch` or `bacon` for inner-loop test reruns
- `cargo-llvm-cov` for coverage

## Quick start

```bash
pnpm install
pnpm dev                # tauri dev: spins up vite + cargo run
pnpm test               # run the full local test tier
pnpm check              # cargo check + clippy + svelte-check + eslint + prettier
```

## Repo layout

```
boothrflow/
├── PLAN.md             # the canonical plan
├── DECISIONS.md        # ADRs
├── README.md           # you are here
├── LICENSE             # Apache 2.0
├── NOTICE              # third-party attributions
├── package.json        # root scripts + dev deps
├── pnpm-lock.yaml
├── lefthook.yml        # pre-commit hooks
├── deny.toml           # cargo-deny config
├── biome.json or eslint.config.js + .prettierrc
├── vite.config.ts
├── tsconfig.json
├── svelte.config.js
├── tailwind.config.js
├── index.html          # Vite entry
├── src/                # Svelte 5 frontend
│   ├── app.html
│   ├── app.css
│   ├── main.ts
│   ├── App.svelte
│   ├── lib/
│   │   ├── services/   # pure business logic, platform variants
│   │   ├── query/      # TanStack Query reactive layer
│   │   ├── state/      # Svelte 5 runes-based stores
│   │   ├── components/ # UI components
│   │   └── ipc/        # generated specta bindings
│   └── routes/         # pages (settings, history, onboarding, …)
├── src-tauri/
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── build.rs
│   ├── capabilities/
│   ├── icons/
│   ├── src/
│   │   ├── main.rs
│   │   ├── lib.rs       # tauri::Builder + commands registration
│   │   ├── audio/       # cpal capture, rubato resample
│   │   ├── vad/         # ten/silero adapters
│   │   ├── stt/         # transcribe-rs wrapper + cloud BYOK
│   │   ├── llm/         # llama-cpp-2 wrapper + prompts
│   │   ├── injector/    # clipboard, typing, UIA strategies
│   │   ├── hotkey/      # global shortcut + low-level hook
│   │   ├── context/     # foreground app + UIA detection
│   │   ├── history/     # rusqlite + FTS5 + sqlite-vec
│   │   ├── dictionary/  # personal dictionary + auto-learn
│   │   ├── overlay/     # listen-pill window
│   │   ├── pipeline.rs  # the hot loop
│   │   └── settings.rs
│   ├── tests/           # cargo integration tests
│   └── fixtures/audio/  # test WAVs (LibriSpeech derivatives)
├── tests/               # Playwright E2E (Phase 4+)
├── .github/workflows/
│   ├── lint.yml
│   ├── test.yml
│   └── release.yml
└── _spike/              # gitignored — reference clones of Handy / Whispering
```

## Contributing

Conventions: [Conventional Commits](https://www.conventionalcommits.org/). Small PRs (~200-400 LoC). Branch protection on `main` requires green CI.

See [`DECISIONS.md`](./DECISIONS.md#adr-006--workflow-conventional-commits-small-prs-no-stacked-pr-tooling) for the full workflow.

## License

[Apache 2.0](./LICENSE). See [`NOTICE`](./NOTICE) for third-party attributions.
