# Wave 3 — macOS Port (status: UAT polish landed on `feat/wave-3-mac`, 2026-04-28)

> **Update 2026-04-28:** First on-Mac UAT pass surfaced eight issues (white
> pill corners, frozen elapsed clock, partial-text clipping, main-window
> scroll dead, hotkey desync after focus changes, ambiguous "0 ms" LLM
> cell, missing permission UX, Whisper model question). All addressed in
> this commit. Items #11 below ("Permission request UX") moved from
> "deferred to Wave 6" to "shipped"; everything else from Wave 3's
> original scope still applies. See `docs/uat/wave-3.md` for the report.

---

Wave 3 takes the Windows-working dictation pipeline and makes it run
natively on macOS (Apple Silicon priority; Intel best-effort). It is
the first cross-platform pass; Linux follows in Wave 4.

The principle: **port the platform-specific surfaces, keep everything
else identical**. The dictation hot loop, history, LLM, and FE are
already platform-agnostic. Most of Wave 3 is fixing the `cfg(windows)`
gates and the OS-specific shims behind them.

## Strategy: branch from `feat/wave-2`, port, validate, merge

The Mac development branch is `feat/wave-3-mac` (or `feat/wave-3` if
you want to keep options open for shared platform work). The user
clones to a Mac, sets up the toolchain, and walks the items below in
order. Each item is a checkpoint where the daemon should still build
and run — the goal is incremental working state, not a big-bang
port.

---

## #1 — Mac dev environment + first compile

**Goal:** `pnpm tauri dev` builds and launches the empty shell.

Toolchain:

- Xcode Command Line Tools: `xcode-select --install`
- Rust via rustup; nightly not required.
- Node 22+ via `nvm` or `mise`; pnpm via `corepack enable`.
- CMake for whisper.cpp native builds: `brew install cmake`.
- Ollama for the LLM cleanup and embedding endpoints:
  `brew install ollama && brew services start ollama`.
- LLVM (for whisper-rs bindgen). On macOS the system clang ships with
  CLT and works; if bindgen complains, `brew install llvm` and set
  `LIBCLANG_PATH=$(brew --prefix llvm)/lib`.

The Windows wrapper `scripts/cargo-msvc.bat` exists because MSVC needs
a vcvars64 environment. macOS doesn't need this — plain `cargo` works.
Add `scripts/cargo-mac.sh` if any macOS-specific env-priming is needed
(probably not). Otherwise document the difference in CONTRIBUTING.

Bring up `pnpm tauri dev`. Expected first errors:

- `windows` crate is in `[target.'cfg(windows)'.dependencies]`,
  which won't pull on macOS. Good.
- `rdev` builds on macOS but needs Accessibility permission (see #2).
- `enigo` builds on macOS but needs Accessibility permission too.
- Anything wrapped in `#[cfg(windows)]` will be missing on Mac and
  needs a sibling `#[cfg(target_os = "macos")]` impl.

## #2 — Hotkey hook (Ctrl+Cmd hold-to-talk on Mac)

**File:** `src-tauri/src/hotkey/global.rs`

`rdev` works on Mac via the CGEventTap API but the user must grant
Accessibility permission to the app bundle. First-launch UX: trap
the failure, prompt with a dialog explaining where to grant it
(`System Settings → Privacy & Security → Accessibility`), reopen the
listener after the user says they've enabled it.

Hotkey choice: Windows uses Ctrl+Win (Ctrl+Meta). On Mac the
equivalent is Ctrl+Cmd — Cmd is the natural primary-modifier on Mac
and Ctrl+Cmd doesn't collide with any system shortcut. Alt+Meta+H for
the quick-paste palette becomes Option+Cmd+H.

Two ways to handle the binding difference:

1. Detect platform at startup and pick the right combo. Document
   both in the README.
2. Make it configurable in Settings. Probably ship #1 first; #2 is
   already on the longer-term roadmap.

## #3 — Foreground-window capture + restore (paste-back)

**File:** `src-tauri/src/quickpaste.rs`,
`src-tauri/src/injector/clipboard.rs` (if it ends up needing it)

Windows uses `GetForegroundWindow` / `SetForegroundWindow` /
`AllowSetForegroundWindow` to remember which app the user was in,
then restore focus before pasting. Mac equivalent:

- `NSWorkspace.shared.frontmostApplication` returns the foreground
  app (an `NSRunningApplication`).
- `NSRunningApplication.activate(options:)` brings it back forward.
- This needs the `cocoa` or `objc2-app-kit` crate. `objc2-*` is
  preferred for new code (memory-safe, drop-in, well-maintained).

Module layout: rename or split out a `quickpaste/win.rs` and
`quickpaste/mac.rs` once both exist. Keep the cross-platform API
(`capture_target_window`, `restore_target_window`, `hide`) identical.

## #4 — Clipboard + paste injection

**File:** `src-tauri/src/injector/clipboard.rs`

`arboard` works on Mac out of the box. `enigo` simulates the
Cmd+V keystroke; on Mac this maps to `Modifier::Command` instead
of `Modifier::Control`. Pick the right one at compile time:

```rust
#[cfg(target_os = "macos")]
const PASTE_MODIFIER: Key = Key::Meta;
#[cfg(not(target_os = "macos"))]
const PASTE_MODIFIER: Key = Key::Control;
```

The 40 ms beat between focus-restore and paste should stay; macOS
focus transitions are fast but not instant.

## #5 — Tray icon + tray menu

**File:** `src-tauri/src/tray.rs`

Tauri 2's tray-icon plugin is cross-platform; the Windows tray code
should drop straight onto Mac. Two macOS-specific things to verify:

- The tray icon is in the menubar, not a system tray. Sizing may need
  a 22×22 PNG instead of the Windows 32×32 — tauri-plugin-image-png
  handles the resize but verify it looks right at retina scale.
- Right-click on Windows is the menu trigger; on Mac it's left-click
  too. Tauri exposes both — make sure both work.

## #6 — Multi-window (pill + quick-paste palette)

**File:** `src-tauri/src/lib.rs`, `src-tauri/src/overlay.rs`

The pill window is a borderless, transparent, always-on-top window
positioned near the screen bottom. On Mac:

- `set_decorations(false)` works.
- `set_always_on_top(true)` works.
- Transparency requires `transparent: true` in the window config
  _and_ the macOS-specific `macos-private-api` Tauri feature (already
  in our `Cargo.toml`).
- Click-through: Windows uses `WS_EX_TRANSPARENT`. Mac uses
  `setIgnoresMouseEvents(true)` on the NSWindow. Tauri exposes this
  via `set_ignore_cursor_events`. Verify the pill is click-through
  during dictation but the quick-paste palette is interactive.

Position on Mac: dock-aware. Use `NSScreen.visibleFrame` so the pill
sits above the dock, not under it.

## #7 — Whisper GPU on Mac (Metal)

Already wired in Wave 2 as a Cargo feature. Build with:

```
cargo build --release --features "real-engines gpu-metal"
```

`gpu-metal` forwards to `whisper-rs/metal`, which compiles
whisper.cpp's Metal backend. On Apple Silicon this auto-uses the
Neural Engine for the encoder where possible. Expect a 5–15× STT
speedup vs CPU.

Verify on first build:

- `MTLDevice` enumerates the GPU.
- `[ggml] using Metal backend` appears in the daemon log.
- `dictation:done` `stt_ms` drops dramatically vs the CPU build.

## #8 — Default model paths + first-run download

**File:** `src-tauri/src/stt/whisper.rs`

`dirs::data_dir()` returns the right thing on Mac
(`~/Library/Application Support`). The model goes to
`~/Library/Application Support/boothrflow/models/ggml-tiny.en.bin`.
The first-run download script is `scripts/download-model.bat` on
Windows; Wave 3 adds the parallel `scripts/download-model.sh` and
`pnpm download:model:mac`. It keeps the same model-name shortcuts
(tiny / base / small / medium / large-v3-turbo).

## #9 — LLM endpoint (Ollama on Mac)

No code change. Ollama on Mac listens on the same `localhost:11434`
by default. The `BOOTHRFLOW_LLM_*` env vars work identically. Just
confirm in the Wave 3 UAT that the OpenAI-compat path still hits the
prewarm + chat completions correctly through Mac's networking stack.

UAT setup should pull both local Ollama models used by the current
branch:

```bash
pnpm ollama:pull
```

That pulls `qwen2.5:1.5b` for cleanup and `nomic-embed-text` for
history embeddings.

## #10 — Code signing + notarization (basic)

**Goal:** the app bundle launches without "developer cannot be
verified" gates for the user, but full Apple Developer signing is
deferred to Wave 6 (distribution polish).

- Self-signed for local dev.
- `tauri-plugin-updater` works on Mac with no extra config but
  binaries it serves must be notarized. Skip the updater path until
  we have a proper Developer ID.
- Document the unsign workaround for users who want to try a build
  off our GitHub releases without a Dev ID:
  `xattr -dr com.apple.quarantine boothrflow.app`.

## #11 — Permission request UX

**Goal:** zero-mystery permission prompts on first launch.

Two prompts hit the user before the app can work:

1. **Accessibility** — for `rdev` (key listening) and `enigo`
   (paste injection).
2. **Microphone** — for `cpal`.

Both are macOS standard prompts, but the OS only shows them when the
relevant API is first called. The fix:

- On first launch, before any audio or input listening, walk a
  one-screen onboarding wizard.
- For each permission, trigger the OS prompt programmatically by
  making a no-op call (e.g. enumerate audio devices to trigger the
  mic prompt; install a no-op CGEventTap to trigger Accessibility).
- Block the rest of the app behind the wizard until both are granted.

Defer to Wave 6 if scope is tight; the dictation will work without
this if the user grants permissions on demand. UX is just rougher.

---

## Wave 3 UAT — what to test on the Mac

Same suite as Wave 2 plus the platform sanity items:

Pre-flight:

```bash
brew services start ollama
pnpm download:model:mac
pnpm ollama:pull
pnpm dev
```

Then:

1. Hold Ctrl+Cmd, dictate, release. Pill stays visible through paste.
2. State transitions fire in the same order; `dictation:done` carries
   sane timings (faster than Windows on the same hardware tier
   because Metal).
3. Streaming partials render in the pill ~800 ms after first audio.
4. Quick-paste palette opens on Option+Cmd+H, restores focus to the
   originating app on entry select.
5. Tray icon shows in menubar with correct retina sizing; right-click
   menu works; pause/resume toggles correctly.
6. Permissions: deny mic the first time, observe the failure path,
   re-grant in Settings, confirm dictation recovers.
7. App bundle survives a `xattr -dr com.apple.quarantine` (no sigsegv
   from missing signing).

## Risk register (Mac-specific)

| Risk                                                              | Likelihood | Impact | Mitigation                                                                            |
| ----------------------------------------------------------------- | ---------- | ------ | ------------------------------------------------------------------------------------- |
| `rdev` Accessibility permission UX is bad                         | High       | Med    | Onboarding wizard (#11) — defer to W6 if needed; fallback is to document in README.   |
| Window transparency renders incorrectly on Sonoma+                | Med        | Med    | `macos-private-api` flag is already on; test early, file Tauri issues if it breaks.   |
| Metal backend compiles slowly on M-series the first time          | Low        | Low    | One-time cost; document expected ~2 min first build.                                  |
| `enigo` paste keystroke conflicts with target app's Cmd+V binding | Low        | Low    | Same risk on Windows (Ctrl+V); we ship clipboard fallback if injection fails.         |
| HFS+ vs APFS path edge cases for model files                      | Low        | Low    | `dirs::data_dir()` already abstracts. Add a smoke test that round-trips a model path. |
| Tauri tray-icon retina scaling                                    | Med        | Low    | Render at 2x and let Tauri downsample; verify in #5 with the actual menubar.          |

## Out of scope for Wave 3

- Linux (Wave 4).
- Apple Developer ID signing + notarization for the App Store or for
  signed releases (Wave 6).
- Auto-update via tauri-plugin-updater (Wave 6).
- Dictation in non-English languages (any wave; tied to model choice
  and tested separately).
- Whisper CoreML backend (a separate `gpu-coreml` feature flag could
  land later for the Apple Neural Engine path; Metal already uses ANE
  opportunistically and is the simpler bring-up).
