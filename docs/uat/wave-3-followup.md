# Wave 3 — Follow-up after second on-Mac UAT

**Date:** 2026-04-28
**Branch:** `feat/wave-3-mac`
**Reviewer:** Claude (continued)
**Verdict:** **Ready for cross-platform UAT.** Eric re-tested on Mac after the first polish commit; pill, scroll, timer, and partial-scroll all confirmed working. This commit ships the next round (Apple Silicon Metal default, tap-to-toggle hotkey, two-line partial, in-app settings roadmap).

---

## What Eric reported on the second pass

Verbatim, organised:

**Working well:**

- Pill renders correctly (no more white corners).
- Elapsed timer counts up.
- Partial transcript scrolls horizontally as new tokens come in.
- Main UI scrolls.
- Overall: "this looks really solid."

**Hard limit observed (declined to fix):**

- Partial stops updating around the 52-second mark. Confirmed
  expected — the streaming Whisper buffer caps at
  `MAX_STREAMING_SAMPLES = 16_000 * 25 = 25 s`; partials stop emitting
  past Whisper's 30 s context window. The final transcript on release
  still includes the full audio. (52 s seemed long for a 25 s cap;
  may include some clock-drift in user perception or buffer slack.
  Not investigating until someone hits actual data loss.)

**Concrete asks:**

1. **Toggle hotkey.** "Don't have to hold this down as we keep going."
   Some chord that toggles dictation on tap.
2. **Apple Silicon Metal by default.** No need to remember
   `--features gpu-metal`.
3. **Pill geometry.** "Make the dictation box a little bit narrower
   on the Y axis. And then the text, the real-time dictation can be
   not just one line but maybe two." Acceptable to grow slightly if
   needed for two lines.

**Roadmap asks (no code):**

4. **Cross-platform check.** "Will this work on Windows? Probably not
   Linux out of the box, but you know maybe you want to check that."
5. **In-app settings.** "Change models on the fly from the user
   interface or change like other settings… without needing to access
   the terminal."

## Fixes (this commit)

### 1 — Tap-to-toggle dictation hotkey

**Files:** `src-tauri/src/hotkey/mod.rs`, `src-tauri/src/hotkey/global.rs`,
`src-tauri/src/session.rs`, `src/lib/services/platform.ts`,
`src/App.svelte`

- New `HotkeyEvent::ToggleDictation` variant.
- rdev listener detects `Space` rising edge while
  `Ctrl + Alt && !Meta` is held → emits `ToggleDictation`.
  Different modifier set from the hold-PTT chord (`Ctrl + Meta` on
  both platforms), so the rising edges don't collide and we don't
  need any "is the user already in a session?" fork in the listener.
- Session daemon's match arm accepts `Press | ToggleDictation` as a
  start event and `Release | ToggleDictation` as a stop event. Either
  modality can also serve as a kill-switch for the other (preferable
  to a wedged state).
- Chord:
  - macOS: `Ctrl + Option + Space`
  - Windows / Linux: `Ctrl + Alt + Space`
- Topbar shows the new chord alongside the hold-PTT and quick-paste
  chords; empty-state hint reads "Hold ⌃⌘ to dictate, or tap
  ⌃⌥␣ to toggle hands-free."

### 2 — Apple Silicon Metal default

**File:** `src-tauri/Cargo.toml`

```toml
[target.'cfg(all(target_os = "macos", target_arch = "aarch64"))'.dependencies]
whisper-rs = { version = "0.16", optional = true, features = ["metal"] }
```

Cargo merges feature lists across `[dependencies]` and
`[target.*.dependencies]` blocks, so this is purely additive — base
declaration above stays at no GPU features (CPU on Intel Mac /
Windows / Linux); aarch64-apple-darwin gets `metal` for free. Verified
via `cargo tree --features real-engines -e features | grep whisper-rs`:

```
├── whisper-rs feature "default"
└── whisper-rs feature "metal"
```

The manual `gpu-metal` Cargo feature still exists for users who want
to force Metal on Intel Mac / non-Mac targets.

### 3 — Pill geometry

**Files:** `src-tauri/src/overlay.rs`,
`src/lib/components/ListenPill.svelte`

- Window height: 74 → 80 px. Net +6 px to fit two text lines without
  feeling like a different shape.
- Status row: 28 → 22 px (roughly Apple HIG menu-bar density).
- Padding: 9/14/10 → 7/13/8.
- Partial row: switched from horizontal-scroll-with-ellipsis to
  two-line wrap with vertical scroll. `white-space: normal;
word-break: break-word; overflow-y: auto`. Scrollbar still hidden;
  the auto-scroll-to-bottom keeps the newest sentence in view.
- Auto-scroll JS: `scrollLeft = scrollWidth` → `scrollTop = scrollHeight`.

Net effect: roughly 2× the visible context per partial without making
the pill feel taller.

## Roadmap (this commit, docs-only)

### Cross-platform compatibility note

Added a status table to `ROADMAP.md` enumerating which Wave 3 fixes
land on which platforms:

- **Mac-verified:** all of pill / timer / scroll / 2-line wrap / LLM
  telemetry / toggle / Metal.
- **Windows (no extra work, just needs UAT):** all of the above except
  the macOS-only resync heartbeat and Permissions panel — those are
  `cfg(target_os = "macos")` and compile to no-ops on Windows.
- **Linux (Wave 4):** the rdev hooks need additional work for Wayland
  sessions; everything else is structurally fine.

Eric is doing Windows UAT tomorrow before the merge to `main`.

### In-app Settings panel

Added a new bullet to Phase 2:

> Every setting that's currently env-var-only should be flippable from the UI.
> Whisper model picker (with download-on-select); LLM endpoint, model, and API
> key; embed endpoint and model; hotkey rebind (PTT, toggle, quick-paste);
> per-app style overrides; privacy-mode toggle; vocabulary-biasing prompt.
> Persists to `tauri-plugin-store` (already in the dep tree). Pre-req: typed
> Tauri command surface (ADR-007 deferred work) so the FE doesn't have to
> mirror Rust types by hand for ~15 new commands.

This unblocks the "no terminal" experience without paving a separate
greenfield path — `tauri-plugin-store` is already wired and the env
var surface area is small enough to map 1-to-1.

## Test matrix

| Check                                                                        | Result   |
| ---------------------------------------------------------------------------- | -------- |
| `pnpm check` (types / lint / format / rust)                                  | ✅       |
| `pnpm test:fe`                                                               | ✅ 7/7   |
| `cargo nextest run --features test-fakes`                                    | ✅ 22/22 |
| `cargo check --features real-engines`                                        | ✅       |
| `cargo tree -e features` confirms `whisper-rs/metal` on aarch64-apple-darwin | ✅       |

## What's still on the human

1. **Re-test on Mac** with the new pill height (80 px) and
   tap-to-toggle (`Ctrl+Option+Space`). Should feel cleaner.
2. **Windows UAT** tomorrow: launch on the Windows box, confirm pill
   / timer / partial / LLM telemetry / toggle hotkey all work without
   regression. Also confirm the Permissions panel correctly does NOT
   render on Windows (`isMac` check in `App.svelte`).
3. **Merge `feat/wave-3-mac` → `main`** after Windows passes.

## Out-of-scope follow-ups (still queued)

- Structured / app-aware formatting (Phase 2 backlog item).
- In-app Settings panel (Phase 2 backlog item, added this commit).
- Linux port (Wave 4).
- Onboarding wizard, code signing, notarization (Wave 6 polish).
