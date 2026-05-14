# Windows port — buildability + test harness

Wave 5 + 6 added bindgen-heavy crates (`whisper-rs`, `sherpa-rs-sys`,
`ort` via `voice_activity_detector`) that all want MSVC + libclang +
cmake set up _just so_. This doc explains the prerequisites, the
single-entry-point test harness, and the small set of Windows-only
fixes that keep the tree compiling from a clean shell.

The hold-PTT + tap-to-toggle dictation pipeline itself is already
cross-platform on `main` (see `ROADMAP.md` → "Cross-platform status").
This is purely about keeping Windows building + green as new deps land.

## Prereqs (one-time)

```powershell
winget install Microsoft.VisualStudio.2022.BuildTools
winget install Rustlang.Rustup ; rustup toolchain install stable
winget install LLVM.LLVM
```

The VS BuildTools install needs the **Desktop development with C++**
workload — that's what ships `cl.exe`, the Windows 10 SDK headers, and
the cmake binary that `whisper-rs-sys` / `sherpa-rs-sys` consume. The
bundled cmake lives at:

```
C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\
  Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin\cmake.exe
```

LLVM ships `libclang.dll`, which bindgen needs to parse C headers.

## One entry point: `pnpm check:windows`

```powershell
pnpm check:windows
```

Runs, in order:

1. `pnpm check:types` — svelte-check + tsc (FE)
2. `pnpm check:lint` — eslint (FE)
3. `pnpm check:format` — prettier (whole tree)
4. `pnpm check:rust:real` — `cargo fmt --check` + clippy with the full
   feature set (`real-engines parakeet-engine`, `--all-targets`,
   `-D warnings`)
5. `pnpm test:rust:real:full` — nextest under the same feature set
6. `pnpm test:fe` — vitest unit suite

Steps 4–5 go through `scripts/cargo-msvc.bat`, which loads
vcvars64 + libclang env vars before invoking cargo. That wrapper is
the load-bearing piece: it lets you run from a plain PowerShell /
Git Bash session instead of having to open the "x64 Native Tools
Command Prompt for VS 2022" every time.

## What the wrapper sets up

`scripts/cargo-msvc.bat` does four things in order:

1. **Locates `libclang.dll`** at the canonical LLVM install paths
   and exports `LIBCLANG_PATH`. bindgen-using crates (whisper-rs,
   sherpa-rs, llama-cpp-2) fail without this.
2. **Locates and calls `vcvars64.bat`** for whichever VS 2022 edition
   is installed (BuildTools / Community / Pro / Enterprise). Prepends
   the shared Visual Studio Installer directory to PATH so vcvars64's
   internal `vswhere.exe` call resolves — otherwise vcvars prints
   "Environment initialized" but leaves PATH unchanged.
3. **Pins `CMAKE` to an absolute path** to VS's bundled cmake.exe.
   pnpm / tauri-cli on Windows have been observed to spawn child
   processes with a sanitized PATH that drops the vcvars-added MSVC
   bin dirs — so even though `where cmake` succeeds in the launching
   shell, cargo's build script subprocess fires from a PATH where
   cmake is gone. Pinning `CMAKE` bypasses the whole shell-env
   propagation chain.
4. **Forwards `%*`** — anything after `cargo-msvc.bat` runs in the
   prepared environment. Works for `cargo build`, `cargo nextest`,
   `pnpm exec tauri dev`, raw `cmake`, etc.

The per-package fallback for step 3 lives in `src-tauri/.cargo/config.toml`:

```toml
[env]
CMAKE_x86_64-pc-windows-msvc = "C:\\Program Files (x86)\\...\\cmake.exe"
```

That `CMAKE_<triple>` form is read by the `cmake-rs` crate's lookup
order **before** PATH, so even cargo invocations that bypass the
wrapper script (CI matrix runs, IDE rust-analyzer) get the right cmake.

## Line endings

`.gitattributes` pins the source tree to LF, with one important
exception: `.bat` / `.cmd` / `.ps1` files MUST stay CRLF. cmd.exe
silently misparses LF-only batch files — comments leak through as
commands, `goto` jumps fall through, and downstream "program not
found" errors get blamed on tools that are perfectly installed.
That's the failure mode the cargo-msvc.bat header references.

`.prettierrc.json` is set to `endOfLine: "auto"` so the Windows
check harness doesn't flag every file with `\r\n` on disk while
`.gitattributes` is still propagating into existing checkouts. Fresh
clones get LF source files; old checkouts get a graceful upgrade
path via `git add --renormalize .`.

## Known runtime gap: `pnpm gen` (binding regen) currently fails at startup

`pnpm gen` (and `cargo run --example export_bindings`) **build** fine
under `cargo-msvc.bat`, but the resulting exe exits immediately with
`STATUS_ENTRYPOINT_NOT_FOUND` (`0xc0000139`) on launch.

Root cause: `ort-sys` (pulled in via `voice_activity_detector` →
Silero VAD → onnxruntime) unconditionally emits
`cargo:rustc-link-lib=DirectML` on Windows. The example exe ends up
with `DirectML.dll` in its import table even though nothing at
runtime touches DML. The Windows loader resolves the DML imports
eagerly against the pyke-prebuilt onnxruntime's bundled DirectML.dll,
fails on a specific entry point, and aborts the process before main.

Workaround until fixed: regenerate `src/lib/ipc/bindings.ts` on macOS
(`pnpm gen` works there). The bindings file is checked in, so day-to-day
Windows dev doesn't actually need this — only landing a new
`#[tauri::command]` does.

Fixing this properly likely means either (a) running the example
with `LOAD_LIBRARY_SEARCH_DEFAULT_DIRS` + a curated DLL search path
that picks the system DirectML.dll, or (b) splitting the
`build_specta` command surface into a tiny crate without the VAD/
onnxruntime dep so the bindings example can compile clean.

## What the test harness will catch

| Symptom on Windows                                                            | Caught by                       | Notes                                                                             |
| ----------------------------------------------------------------------------- | ------------------------------- | --------------------------------------------------------------------------------- |
| `LIBCLANG_PATH not set`                                                       | `check:rust:real` (clippy step) | bindgen fails inside whisper-rs / sherpa-rs build                                 |
| `cmake.exe: program not found`                                                | `check:rust:real`               | sanitized-PATH issue, fixed by `.cargo/config.toml` + wrapper                     |
| `windows-0.58` API breaking change (e.g. `K32GetModuleFileNameExW` signature) | `check:rust:real`               | clippy with `-D warnings` catches all `unused import` / type mismatch             |
| Unused `Duration` import on non-mac targets                                   | `check:rust:real`               | cfg-gate the import                                                               |
| Tray icon `&Image` vs owned `Image` lifetime mismatch                         | `check:rust:real`               | tray.rs `tray_icon` on non-mac must `Image::new_owned(...)` instead of `cloned()` |
| Test regression                                                               | `test:rust:real:full`           | 89 tests across 4 binaries on Wave 6 tip                                          |
| Prettier CRLF noise on Windows checkouts                                      | `check:format`                  | only after `.prettierrc.json` got `endOfLine: "auto"`                             |
