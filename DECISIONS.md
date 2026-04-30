# Architecture Decision Records — boothrflow

Each entry: **Context → Decision → Consequences.** Short on purpose. Deep rationale lives in `PLAN.md` and the linked research.

> Process: append-only. New decisions get the next number. If a decision is reversed, append a new ADR titled `Reverse ADR-NNN: …`. Do not edit accepted ADRs.

---

## ADR-001 — Foundation: greenfield, not fork

**Status:** Accepted, 2026-04-27.

**Context.** Two strong OSS dictation apps already exist in our exact target stack (Tauri 2 + Rust): `cjpais/Handy` (closer to our scope, dictation-only, MIT) and `EpicenterHQ/epicenter`'s `whispering` app (broader monorepo, AGPL-3.0, Svelte 5). Day-1 spike: clone both, read structure + Cargo.toml + CI + key Rust files.

**Decision.** Greenfield, with explicit pattern lifts:

- **From Handy:** STT abstraction via `transcribe-rs` (multi-engine: whisper-cpp, parakeet, moonshine, canary, cohere, gigaam, sense_voice), input.rs's VK-code paste pattern (keyboard-layout-independent), `rusqlite_migration`, manager-per-domain Rust layout.
- **From Whispering:** three-layer frontend architecture (pure services → TanStack Query → Svelte UI), `wellcrafted` Result types for error handling, build-time platform detection pattern.

**Consequences.**

- Slower out of the gate (8-12 weeks to v1 vs 5-6 for a fork) but no inherited scope debt, no AGPL constraint, our brand and license to choose.
- Every architectural pattern has a known-working precedent we can cross-reference.
- Spike repos remain in `_spike/` (gitignored) for ongoing reference.

**Rejected alternatives.**

- Fork Handy: forces us to either inherit MIT (fine) and merge upstream forever, or diverge fast and waste the inheritance. Their `managers/transcription_mock.rs` CI swap is a smell we want to fix from day one, not inherit.
- Fork Whispering: AGPL-3.0 viral license rules out a permissive boothrflow. Also a much larger codebase to navigate.

---

## ADR-002 — Stack: Tauri 2 + Rust + Svelte 5 + pnpm

**Status:** Accepted, 2026-04-27.

**Context.** App framework, runtime, package manager.

**Decision.**

- **Tauri 2** for the desktop shell (~30MB installer, ~80MB resident).
- **Rust** for the audio/STT/LLM/injection hot path.
- **Svelte 5** with runes for the UI (Whispering's choice; React would also work — Svelte wins on SSR-isolated reactivity for our overlay windows).
- **pnpm** for JS package management (mature on Windows; user has it installed). Note: Handy and Whispering both use `bun`; we deviate because pnpm has better Windows compatibility for our target users.
- **TypeScript strict mode** everywhere on the frontend.
- **Tailwind CSS v4** + **shadcn-svelte** for UI primitives.

**Consequences.**

- Cross-platform reach (Windows primary, Mac second, Linux third).
- Smaller install + RAM than Electron.
- Native Rust FFI for whisper.cpp / sherpa-onnx / llama-cpp-2 with no IPC tax.
- Svelte's smaller community than React for shadcn-style components, but shadcn-svelte covers our needs.

---

## ADR-003 — STT abstraction: use `transcribe-rs`, don't reinvent

**Status:** Accepted, 2026-04-27.

**Context.** We need a unified Rust trait for the STT engine that supports whisper-cpp (universal-language fallback) and Parakeet TDT (default English+EU streaming) at minimum, with engine-swap exposed in settings.

**Decision.** Adopt the `transcribe-rs` crate (currently 0.3.8 — the one Handy uses) as our STT trait layer. It already supports whisper-cpp + ONNX (parakeet, moonshine, canary, cohere, gigaam, sense_voice) under one `SpeechModel` trait, with feature flags `whisper-cpp`, `whisper-vulkan`, `whisper-cuda`, `onnx`, `ort-directml`.

We wrap it in our own `SttEngine` trait so we can:

1. Inject a `FakeSttEngine` for tests (`#[cfg(feature = "test-fakes")]`).
2. Layer in BYOK cloud STT (Deepgram, Groq) without polluting the local engine layer.
3. Decouple from `transcribe-rs`'s API churn (still pre-1.0).

**Consequences.**

- Massive head-start: we don't author whisper bindings or NeMo ONNX integration.
- Couples us to a small-maintained crate. Mitigation: pin a specific version; vendor a fork if upstream stalls.
- License: `transcribe-rs` is MIT — compatible with anything we pick.

---

## ADR-004 — Frontend architecture: three layers (Service / Query / UI)

**Status:** Accepted, 2026-04-27.

**Context.** Desktop apps quickly grow tangled when business logic, reactivity, and UI all share files. Whispering's pattern handles this cleanly and demonstrably enables 97% code reuse if we ever ship a web version.

**Decision.** Organize the frontend in three layers:

1. **Service layer** (`src/lib/services/`): pure functions, no Svelte, no `$state`, no UI knowledge. Each service exposes a set of typed functions that return `Result<T, E>` (via `wellcrafted`). Platform variants via build-time detection: `*.desktop.ts` calls Tauri APIs, `*.web.ts` calls browser APIs, the index file picks at build via `window.__TAURI_INTERNALS__`.
2. **Query layer** (`src/lib/query/`): TanStack Query Svelte. Wraps services with reactivity, runtime dependency injection (e.g., switching STT provider based on settings), cache invalidation. This is the only layer that knows about user settings.
3. **UI layer** (`src/routes/` + `src/lib/components/`): Svelte 5 components. Reads from query layer + reactive state stores. No business logic.

Errors flow up via `Result<T, E>` until the query layer, where they're either auto-toasted (`toastOnError`) or handed to UI for explicit handling.

**Consequences.**

- Tests are trivial: services are pure functions, just pass fakes.
- Refactoring is bounded: changing STT provider doesn't ripple into UI.
- Initial overhead: a 3-layer architecture for a Hello World feels heavy. We accept the cost up-front; v0 wires through all three.

---

## ADR-005 — Test strategy: traits + fakes via feature flag, nextest, browser-mode Vitest

**Status:** Accepted, 2026-04-27.

**Context.** Dictation apps have nightmare testability: audio I/O, FFI to whisper, Win32 SendInput, non-deterministic LLM. Handy's CI hack (`cp transcription_mock.rs transcription.rs`) is a code smell; we want a real abstraction from day one.

**Decision.**

**Rust:**

- Every cross-cutting subsystem (`SttEngine`, `Vad`, `Injector`, `LlmCleanup`, `ContextDetector`, `AudioSource`) is a trait.
- Production impls behind `#[cfg(feature = "real-engines")]` (default).
- Fake impls behind `#[cfg(feature = "test-fakes")]`.
- CI runs `cargo nextest run --no-default-features --features test-fakes` for the fast tier (no whisper/llama compile, ~10s).
- A nightly job runs `--features real-engines` end-to-end with a tiny model.
- **Runner:** `cargo-nextest` (process-per-test isolation matters — FFI panics shouldn't take the whole binary down). `cargo test --doc` for doctests.
- **Coverage:** `cargo-llvm-cov` (works on Windows; tarpaulin doesn't).
- **Property tests:** `proptest` for resampler, VAD frame slicing, clipboard chunk boundaries. Not for STT/LLM (their outputs aren't a function we can assert properties of cheaply).
- **Snapshot tests:** `insta` for prompt templates and SQL migration outputs, NOT for LLM completions or STT transcripts (drift).
- **LLM tests:** stub the `LlmCleanup` trait with `ScriptedLlm` fake (canned outputs keyed by input hash) for unit tests. One nightly real-LLM smoke test with Qwen 0.5B asserts properties (length, no fillers, ends-with-period), not exact strings.
- **Win32 smoke tests:** marker `#[cfg_attr(not(feature = "win32-headed"), ignore)]`. Run on a dedicated Windows runner serially via `serial_test::serial`. Not on every PR.

**Frontend:**

- **Unit:** Vitest in `node` environment for pure-function services.
- **Component:** Vitest browser mode with `vitest-browser-svelte` (Playwright provider, headless Chromium). Naming convention: `*.svelte.test.ts` so Vite preprocesses runes correctly.
- **E2E:** Defer to Phase 4. Use WebDriverIO + `tauri-driver` for one or two Windows-CI smoke tests (app launches, hotkey registers, settings persist).

**Cross-cutting:**

- Pre-commit: `lefthook` (single Go binary, parallel hooks, cross-OS, no Node bootstrap on Rust contributor's clone).
- Task runner: pnpm scripts at root + cargo subcommands inside `src-tauri`. No `just` (extra dep), no Makefile (Windows pain). Pnpm scripts call `cd src-tauri && cargo …` like Handy does.

**Consequences.**

- Testable from line 1: write a test before the production impl exists; the trait + fake exists from scaffold time.
- CI is fast (~3 min for the lint+test matrix, no whisper compile in the default path).
- Some Win32 corners stay manually tested. Documented and accepted.

---

## ADR-006 — Workflow: conventional commits, small PRs, no stacked-PR tooling

**Status:** Accepted, 2026-04-27.

**Context.** "Read up on gstack." Surveyed Graphite's `gt`, Meta's `ghstack`, Linux `gstack` (latter is a thread-stack diagnostic, irrelevant). Stacked-PR tooling solves "I have 8 dependent PRs blocked on each other in review" — a problem 1-2-contributor OSS projects don't have.

**Decision.**

- **Conventional Commits** (`feat:`, `fix:`, `refactor:`, `chore:`, `ci:`, `docs:`, `test:`, `perf:`) — gives us automated changelogs and `release-please` later.
- **Small PRs** (~200-400 LoC) merged via squash with `gh pr merge --auto --squash`.
- **Branch protection on `main`** with required green CI.
- **No stacked-PR tooling.** If we ever scale to ≥3 active contributors with overlapping work, revisit.

**Principles we ARE absorbing from stacked-PR culture:**

- Small, single-purpose changes. Easier to review, easier to bisect, easier to revert.
- `main` always green. CI failures block merge — non-negotiable.
- Fast local feedback. `cargo nextest run` < 30s, `vitest --run` < 15s. Watch mode (`bacon` for Rust, `vitest --watch` for FE) reruns on save.
- Atomic commits with informative messages. Even one-person history matters when you `git bisect` six months from now.

**Consequences.**

- No fancy tools to install or teach. Standard GitHub flow.
- No risk of stack-rewrite chaos when CI breaks mid-stack.
- Lose the marginal benefit of "start work on N+1 before N lands." For a 1-2 person team this is theoretical.

---

## ADR-007 — Type sharing: specta + tauri-specta

**Status:** Accepted, 2026-04-27.

**Context.** Tauri commands cross a Rust ↔ TS boundary. Manual type duplication invites drift; we want compile-time guarantees on the contract.

**Decision.** Adopt `specta` + `tauri-specta` (both v2). Annotate Rust types with `#[derive(specta::Type)]`, register commands with `tauri_specta::collect_commands![…]`, emit a `bindings.ts` at build time. UI imports the generated client.

**Consequences.**

- One source of truth for command signatures, command payloads, and emitted-event payloads.
- Pre-1.0 churn (`tauri-specta = "=2.0.0-rc.21"`); pin exact version.
- Adds a generation step to the dev loop. Mitigated by triggering `cargo run --bin gen-bindings` from a `pnpm gen` script.

---

## ADR-008 — License: Apache 2.0

**Status:** Accepted, 2026-04-27.

**Context.** Pick a license before committing line one. Considered: MIT, Apache 2.0, GPL-3.0, AGPL-3.0, BSL.

**Decision.** **Apache 2.0.**

**Why.**

- Permissive enough for individuals and corporates to adopt.
- Includes a **patent grant** that MIT lacks — non-trivial in audio/ML space where adversarial patents exist.
- Compatible with all our deps' licenses (MIT, BSD, Apache 2, CC-BY-4.0 for Parakeet).
- Doesn't create the AGPL "must-open-source-the-server" friction that blocks corporate users from contributing.

**Rejected:**

- MIT — no patent grant.
- GPL-3.0 / AGPL-3.0 — copyleft chills corporate contributors.
- BSL — non-OSI, scares contributors.

**Consequences.**

- Anyone can fork and ship a closed-source product. We accept this; community + brand wins this market, not license-based moats.
- Need an `ATTRIBUTION.md` / `NOTICE` file for downstream license preservation, especially for CC-BY-4.0 Parakeet model files.

---

## ADR-009 — STT default model: Parakeet TDT 0.6B v3, with whisper-large-v3-turbo Q5 fallback

**Status:** Accepted, 2026-04-27. See `PLAN.md §6` for benchmark detail.

**Context.** Need a default that handles English+EU well at sub-500ms p50, plus a fallback for Asian/African/etc. languages.

**Decision.**

- **Default:** Parakeet-TDT-0.6B v3 (CC-BY-4.0, 1.93/3.59% LS WER, RTFx 3332x, native streaming) via sherpa-onnx (or transcribe-rs's parakeet feature, depending on which integrates more cleanly — try transcribe-rs first since we're already on it).
- **Fallback:** Whisper-large-v3-turbo Q5_K_M GGUF via whisper.cpp (CUDA / Vulkan / Metal feature-gated). 99 languages.
- **CPU-only path:** distil-large-v3.5 Q5 via whisper.cpp CPU.
- **Mac-only future:** WhisperKit + large-v3-turbo CoreML on ANE.

**Consequences.**

- ~1.2GB extra model download for both Parakeet + Whisper. Acceptable; users can choose one in onboarding.
- CC-BY-4.0 attribution requirement on Parakeet → satisfied by `NOTICE` file.

---

## ADR-010 — VAD: TEN-VAD primary, Silero fallback

**Status:** Accepted, 2026-04-27.

**Context.** Endpoint-detection latency is decisive for "feels instant." Compared TEN-VAD, Silero, WebRTC, NeMo MarbleNet.

**Decision.** TEN-VAD as default (faster speech → non-speech transition), Silero as fallback for compatibility/troubleshooting. Both ship as ONNX models, both via `voice_activity_detector` crate or direct ONNX Runtime.

**Consequences.**

- ~2-3MB extra model files. Trivial.
- TEN-VAD bindings less mature than Silero — ship Silero v1.0, swap default to TEN-VAD v1.1 once vetted.

---

## ADR-011 — LLM cleanup: Qwen 2.5 3B Q4_K_M via llama-cpp-2 in-process

**Status:** Accepted, 2026-04-27.

**Context.** Per-utterance formatting pass needs <300ms TTFT, 50-token output. Local options: Ollama (extra daemon), llama.cpp (in-process), MLX (Mac-only), ONNX-Runtime-GenAI.

**Decision.** `llama-cpp-2` crate, in-process, GGUF model in `%APPDATA%\boothrflow\models\`. Default model: Qwen 2.5 3B Instruct Q4_K_M (~2GB, Apache 2 license, best instruction-following at this size). Tiers: Off, Fast (Llama 3.2 3B), Balanced (Qwen 2.5 3B), Quality (Qwen 2.5 7B), BYOK Cloud.

**Consequences.**

- ~2GB extra disk by default; users can disable LLM entirely (raw mode toggle).
- KV cache reuse across utterances within a style → ~80ms TTFT after first call.
- `llama-cpp-2` is well-maintained but not 1.0; pin version, vendor if needed.

---

## ADR-012 — v0 scope deferrals

**Status:** Accepted, 2026-04-27.

**Context.** First `cargo check` and `pnpm check` on a fresh Windows install surfaced a handful of integration friction points. Resolving each properly would have stretched the v0 scaffold into v0.2 territory. Deferring them lets the spine ship today.

**Decision.** The following are deferred and tracked here so we don't lose the thread:

1. **tauri-specta + auto TS bindings (was ADR-007, target Phase 1 W3).** Cross-binary macro resolution (`__cmd__*`, `__specta__fn__*`) is finicky between `lib.rs` and a sibling `bin/`. The canonical fix is to hoist the `specta_builder()` into `lib.rs` and have the bin call it; we do that when we have ≥ 2 commands and the typed-binding payoff is concrete. Until then `specta::Type` derives stay (so payload types are documented) and the frontend talks to Rust through plain `invoke<T>()` calls with manually-mirrored types.
2. **Vitest browser-mode component tests.** `vitest-browser-svelte 0.1.0` predates Svelte 5 runes; the upstream API churns. Component tests via `render(...).getByTestId(...).toContainText(...)` were drafted then deleted. The infrastructure (vitest project for `*.svelte.test.ts` + `@vitest/browser` + `playwright` Chromium) is wired and ready; we just don't ship a passing component test until vitest-browser-svelte 2.x stabilises.
3. **Lefthook Rust pre-commit hooks.** Lefthook's YAML `run:` field on Windows can't reliably wrap commands in `bash -c '...'` because of shell-quoting interactions. We dropped `cargo fmt --check` and `cargo clippy` from pre-commit. CI still gates on them via GitHub Actions. Local contributors run `pnpm check:rust` manually.
4. **Local commit-msg hook for Conventional Commits.** Same shell-quoting problem on Windows. We enforce the convention via PR title (planned GitHub Action), not via lefthook.
5. **`tauri.conf.json` `protocol-asset`.** Removed for v0; re-add when we serve images/audio from app-managed paths (likely Phase 3 for history audio playback).

**Consequences.** v0 ships with a smaller surface than ADR-005/007 envisioned. The traits and CI gates that matter most are in place. Each deferred item has a known re-enable path tracked in this ADR; if any becomes blocking earlier than its target Phase, append a Reverse ADR.

---

## ADR-013 — Windows native build env via wrapper script

**Status:** Accepted, 2026-04-27.

**Context.** Bindgen-using crates (`whisper-rs`, `sherpa-rs`, `llama-cpp-2`) need libclang + MSVC/SDK `INCLUDE` paths to parse C headers. Rust's MSVC integration sets these up for `cc` but **not** for `bindgen`. Plain `cargo build` from a non-VS-dev shell silently produces empty bindings (`pub struct foo { _address: u8 }`) and the build fails with cryptic size-assertion errors.

**Decision.** Ship `scripts/cargo-msvc.bat` that:

1. Locates `libclang.dll` (defaults: `C:\Program Files\LLVM\bin`; honors `BOOTHRFLOW_LLVM_PATH`).
2. Locates `vcvars64.bat` (BuildTools / Community / Professional / Enterprise).
3. Sources `vcvars64.bat` to set `INCLUDE`, `LIB`, `PATH`, etc.
4. Sets `LIBCLANG_PATH`.
5. Forwards remaining args to `cargo`.

Hooked into `package.json`:

- `pnpm dev:msvc` → `tauri dev` with env loaded
- `pnpm build:msvc` → `tauri build` with env loaded
- `pnpm test:rust:real` → `cargo nextest run --features real-engines` with env

The fakes-only inner loop (`pnpm test:rust`, `pnpm test:fe`) still works in any shell because `test-fakes` doesn't compile the heavy native deps.

**Consequences.**

- Windows contributors: install `Microsoft.VisualStudio.2022.BuildTools` + `LLVM.LLVM` (both `winget install`); use the `:msvc` pnpm scripts.
- Mac/Linux: unaffected — clang and SDK headers are auto-discovered.
- CI: GitHub Actions Windows job uses the wrapper; same code path as local dev.
- Tradeoff: one extra script-name to remember on Windows. Mitigated by README docs and pnpm script aliases.

**Rejected alternatives.**

- `.cargo/config.toml [env]` with hardcoded paths: brittle (breaks across MSVC/SDK version updates).
- Require contributors to use Developer Command Prompt: invisible UX, easy to forget which shell they're in.
- Switch to a non-bindgen STT crate: doesn't exist for whisper.cpp; compromises the architecture.

---

## ADR-014 — Whisper default stays at `tiny.en`; `small.en` is the recommended upgrade

**Status:** Accepted, 2026-04-28.

**Context.** Wave 3 UAT raised "is `tiny.en` the right default? quality feels light." The honest answer is "no, but the right answer is per-machine, not a single global default."

| Model            | Disk  | Params | CPU RTF (M-series)      | Notes                                                        |
| ---------------- | ----- | ------ | ----------------------- | ------------------------------------------------------------ |
| `tiny.en`        | 75MB  | 39M    | ~0.10                   | Current default. Fast, error-prone on proper nouns.          |
| `base.en`        | 142MB | 74M    | ~0.18                   | Noticeably better; minor latency cost.                       |
| `small.en`       | 466MB | 244M   | ~0.50 CPU / ~0.10 Metal | Sweet spot for accuracy. Realtime on Apple Silicon w/ Metal. |
| `medium.en`      | 1.5GB | 769M   | ~1.5 CPU / ~0.30 Metal  | Approaches SOTA; slow on CPU.                                |
| `large-v3-turbo` | 1.6GB | ~800M  | ~0.40 Metal             | Best multilingual; assumes GPU.                              |

**Decision.**

- Keep `ggml-tiny.en.bin` as the bundled-default download path. Smallest disk footprint, fastest cold-start, makes "first run" cheap.
- The UI's STT chip now reads the active model name from the daemon (`whisper_model_name`), so users who upgrade actually see it.
- Recommend `small.en` for users who care about quality. The tooltip on the chip and a note in the README point at `pnpm download:model:mac small` plus `BOOTHRFLOW_WHISPER_MODEL_FILE=ggml-small.en.bin`.
- A real "Model picker" UI + Metal-by-default on Apple Silicon is deferred until ADR-009's Parakeet path lands — that work supersedes this one.

**Consequences.**

- Users on `tiny.en` get a discoverable upgrade path without a disk-heavy default.
- Telemetry / chip honesty: the UI no longer lies "Whisper tiny.en" when the daemon loaded `small.en`.
- Doesn't fix Whisper's true ceiling; ADR-009 (Parakeet TDT) is still the long-term answer.

---

## ADR-015 — Cleanup LLM default: Qwen 2.5 7B (with 1.5B as a fallback knob)

**Status:** Accepted, 2026-04-29. Refines the implementation choice made at ADR-011 (which targeted Qwen 2.5 3B in-process via `llama-cpp-2`) and the OpenAI-compat HTTP pivot in commit `ca00e48` (which shipped with Qwen 2.5 1.5B as the default model).

**Context.** Wave 3 UAT showed the 1.5B model running clean cleanup in 150–300 ms but with borderline quality — it preserved mumbling-grade disfluencies even after the cleanup-aggressiveness flag landed in Wave 4a, and it occasionally botched proper nouns even when the Whisper initial-prompt vocabulary was extended. The user's direct feedback after dictating the Phase 2 backlog: "the 7 billion parameter model for Qwen is a little bit better… but the latency is quite a bit longer, maybe double. But for cleanup, having 350-400 milliseconds is no big deal."

7B inference on Apple Silicon Metal is well below the "feels instant" cleanup threshold because the user has already finished speaking and is watching the paste land — perceived latency is the paste, not the cleanup pass.

**Decision.**

- `DEFAULT_MODEL` in `src-tauri/src/llm/openai_compat.rs` flips from `qwen2.5:1.5b` to **`qwen2.5:7b`**.
- `pnpm ollama:pull` now pulls both 7B and 1.5B (plus `nomic-embed-text` for history embeddings) so users can swap without a re-pull. `pnpm ollama:pull:fast` skips the 7B for tight-disk machines.
- The escape hatch is `BOOTHRFLOW_LLM_MODEL=qwen2.5:1.5b` until the in-app Settings panel (Wave 4b) exposes a UI picker and persists the choice via `tauri-plugin-store`.

**Consequences.**

- ~5 GB extra model on disk by default. Acceptable given the user has explicitly asked for the upgrade and Ollama can free unused models on demand.
- ~2-3× cleanup latency (300 ms → 350-400 ms). Stays comfortably below the perceived-latency threshold; tok/s telemetry on the cleanup chip lets users diagnose if their machine is slower.
- Slower boxes (Intel Mac, low-VRAM Linux) can drop back to 1.5B via env var. Until Settings ships, this is a manual flip.

**Reverses?** Not strictly. ADR-011 was about an in-process llama-cpp-2 path that didn't ship (we pivoted to HTTP). ADR-015 stacks on top of the HTTP pivot.

---

## How to add a new ADR

1. Append below the last one with the next number.
2. Keep it short — Context, Decision, Consequences, optional Rejected Alternatives.
3. Include date and status (Accepted / Superseded / Reversed).
4. Cross-reference `PLAN.md` sections or external docs for depth; don't re-explain.
5. Never edit accepted ADRs in place. To reverse: append a new ADR titled `Reverse ADR-NNN`.
