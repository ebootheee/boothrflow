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

| Area                         | Status      |
| ---------------------------- | ----------- |
| Plan + ADRs                  | Done        |
| Scaffold                     | In progress |
| Hot path (mic → STT → paste) | Not started |
| LLM cleanup                  | Not started |
| Memory / history             | Not started |

## Prerequisites (developers)

- **Node 22+** and **pnpm 9+**
- **Rust stable** (install via [rustup](https://rustup.rs/) — `winget install Rustlang.Rustup` on Windows)
- **System deps for Tauri 2** — see [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)
- **lefthook** — `pnpm install` will set hooks up automatically
- **cargo-nextest** — `cargo install cargo-nextest` (used by `pnpm test:rust`)

Optional but recommended:

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
