# Wave 3 UAT — macOS Bring-Up Polish

**Date:** 2026-04-28
**Branch:** `feat/wave-3-mac`
**Reviewer:** Claude (continued from Codex's first pass after credit roll-over)
**Verdict:** **Ship — pending on-Mac re-test.** Eight UAT items addressed; full check suite green; hand-off back to the human for the round-trip dictation that still has to be tested in a real session.

---

## What Eric reported on the first pass

Verbatim:

1. Persistent hotkey sometimes lost when switching between windows.
2. Can't scroll through the main UI — have to enlarge the window to see lower features.
3. Wonders if Whisper Tiny is the right model or if a bigger one fits better.
4. Seeing **0 ms** for the LLM cleanup stage even when LLM should run.
5. Permissions should be requested up-front in a clean onboarding flow (had to reboot the terminal to pick them up).
6. Pill window has **white blocks at the rounded corners** — corners aren't transparent.
7. Pill text only shows the first sentence of a transcription — doesn't continue scrolling as the user keeps speaking.
8. Pill elapsed-time ticker shows **0.0s** even while the listening pulse is animating.

## Root causes

| Item              | Root cause                                                                                                                                                                                                                                                                                                                                                                           |
| ----------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 6 (white corners) | `ListenPill.svelte` had `:global(html)`/`:global(body) { background: transparent }` rules. Svelte 5 emits `:global` selectors into the **shared bundle CSS**, so the rules applied to both windows. But the main app's `#app { background: var(--color-background) }` was left alone, so the pill window's rounded corners revealed `#app`'s opaque grey behind the rounded `.root`. |
| 2 (no scroll)     | Same root cause. The pill's `:global(body) { overflow: hidden }` leaked into the main window's bundle and killed scroll there too.                                                                                                                                                                                                                                                   |
| 8 (frozen timer)  | `dictationStore.atMs` only updates on `dictation:state` events from the backend — and those only fire on stage transitions (listening → transcribing → cleaning → pasting → idle). During a long listening hold, `at_ms` was frozen at 0.                                                                                                                                            |
| 7 (clipped text)  | `.partial-row { white-space: nowrap; text-overflow: ellipsis; overflow: hidden }` clamped the LA2 partial to a single line that ran off the right with `…`. As the user kept speaking, more tokens appended but nothing was visible.                                                                                                                                                 |
| 4 (0 ms LLM)      | Two layers: (a) `App.svelte` derived `llmMs` from `selectedEntry`, which is a history row — history doesn't refresh after `dictation:done`, so the cell kept showing the previously-selected row. (b) When Ollama is down, the daemon silently falls back to raw with `llm_ms = 0` and emits no signal — UI couldn't tell "skipped" from "failed".                                   |
| 5 (permissions)   | No `Info.plist` (so prod bundles would crash on first cpal start), and no in-app helper to surface the three TCC panes (Microphone / Accessibility / Input Monitoring) the user has to flip in System Settings. In dev, macOS attributes prompts to the parent terminal — that's irreducible but at least documentable.                                                              |
| 1 (hotkey desync) | The rdev keyboard hook can miss release transitions when focus moves to another app mid-press (Cmd-Tab eats the Cmd-up). Our `both_was` atomic stayed `true` afterward, so the next legitimate Ctrl+Cmd press didn't produce a rising edge — hotkey appeared dead until the user explicitly cycled the modifiers.                                                                    |
| 3 (model size)    | `tiny.en` is the right _bundled_ default (75MB, single binary feels light), but it's too lossy to be the right _quality_ default. The chip in the toolbar also lied — hardcoded "Whisper tiny.en" regardless of `BOOTHRFLOW_WHISPER_MODEL_FILE`.                                                                                                                                     |

## Fixes

### Pill — white corners + no scroll + timer + partials (`src/lib/components/ListenPill.svelte`)

Removed the `:global(html), :global(body)` rules. They're now applied
imperatively in `onMount` to `html`, `body`, **and `#app`** (which was
the actually-opaque element behind the rounded corners), and reverted
in the cleanup. Side benefit: bundle CSS no longer carries pill-only
state — main window scroll restored.

Added a `setInterval` ticker plus a `$state` for `nowMs`. While
`activeLifecycle === "listening"`, the effect updates `nowMs` every
100ms; `displayMs` is `nowMs - listenStartedAt`. The pill counts up.

Partial-row CSS: dropped `text-overflow: ellipsis`; switched to
`overflow-x: auto; white-space: nowrap` with the scrollbar hidden. A
`$effect` watching `activePartial` calls `tick()` then sets
`scrollLeft = scrollWidth` — so the newest tokens stay visible at the
right edge as the user keeps talking.

### LLM cleanup display (`src-tauri/src/session.rs`, `src/lib/state/dictation.svelte.ts`, `src/App.svelte`)

`run_llm_cleanup` now returns `(formatted, ms, Option<error>)`. The
session daemon emits `dictation:llm-missing` on actual call failure
(previously only on client-init failure). The dictation store has a
new `llmMissing` field; `App.svelte` shows a red banner when set, and
the cleanup chip's small text now reads one of:

- the actual `ms` when the LLM ran;
- `off (raw)` when style is Raw;
- `skipped` when the utterance was too short for cleanup;
- `unreachable` when Ollama is down.

`App.svelte` also auto-refreshes history after each `dictation:done`
(via a `$effect` watching `lastDone.total_ms` as a change key) and
selects the new entry — so the "Current" panel reflects the dictation
you just made instead of an old selection.

### Hotkey re-sync (`src-tauri/src/hotkey/global.rs`)

Added a macOS-only `spawn_modifier_resync_macos` heartbeat. Polls
`CGEventSourceFlagsState(kCGEventSourceStateCombinedSessionState)`
every 150 ms via direct CoreGraphics FFI (no new crate dependency),
overwrites the rdev-tracked `ctrl/meta/alt` atomics with reality, and
emits a synthetic `Press`/`Release` if `both_was` drifted. This
self-heals the Cmd-Tab-during-hold case in <150ms without needing the
user to cycle modifiers.

### macOS permissions (`src-tauri/Info.plist` _new_, `src-tauri/tauri.conf.json`, `src-tauri/src/commands.rs`, `src/App.svelte`)

- New `Info.plist` with `NSMicrophoneUsageDescription`,
  `NSAccessibilityUsageDescription`, `NSAppleEventsUsageDescription`.
  Tauri 2 picks it up via `bundle.macOS.infoPlist` (also auto-discovered
  next to `tauri.conf.json`). Prod bundles will get the right OS prompts.
- Two new Tauri commands: `microphone_available()` (cheap cpal probe)
  and `open_macos_setting(pane)` (opens the right Privacy & Security
  pane via `x-apple.systempreferences:` URL).
- Topbar Permissions button + inline panel listing Microphone,
  Accessibility, Input Monitoring, each with an "Open" action.
- Auto-banner if `microphone_available()` returns `false`, with a
  dismiss + a "Open Microphone settings" shortcut.
- Panel copy explicitly notes the dev-mode terminal-relaunch quirk so
  the user isn't confused.

Item #11 in `docs/waves/wave-3-mac-port.md` was originally deferred to
Wave 6; this lands the in-app pieces now. The full scripted onboarding
wizard (block-the-app-until-granted) remains a Wave 6 polish item.

### Whisper model honesty (`src-tauri/src/commands.rs`, `src/App.svelte`, `DECISIONS.md`)

- New `whisper_model_name()` Tauri command returns the file stem the
  daemon will load (honors `BOOTHRFLOW_WHISPER_MODEL_FILE`).
- The STT chip in the toolbar now reads from this command rather than
  hardcoding "tiny.en". Tooltip points at `pnpm download:model:mac small`
  - the env-var override for users who want better quality.
- ADR-014 documents the recommendation: bundled default stays `tiny.en`,
  recommended upgrade is `small.en` (sweet spot for Apple Silicon
  with Metal). Full picker UI deferred to the ADR-009 Parakeet path.

## Pipeline / contract changes

- `dictation:llm-missing` now also fires on call-time failures (not
  just client-init). Frontend listens and surfaces it.
- `run_llm_cleanup` signature changed to a 3-tuple: `(formatted, ms, error)`.
  Internal-only — no public API change.
- New Tauri commands: `microphone_available`, `open_macos_setting`,
  `whisper_model_name`. All registered in both feature matrices.

## Test matrix

| Check                                                                     | Result   |
| ------------------------------------------------------------------------- | -------- |
| `pnpm check:types`                                                        | ✅ 0/0   |
| `pnpm check:lint`                                                         | ✅ clean |
| `pnpm check:format`                                                       | ✅ clean |
| `pnpm test:fe`                                                            | ✅ 7/7   |
| `cargo nextest run --no-default-features --features test-fakes`           | ✅ 22/22 |
| `cargo clippy --no-default-features --features test-fakes -- -D warnings` | ✅       |
| `cargo clippy --features real-engines --all-targets -- -D warnings`       | ✅       |
| `cargo fmt --check`                                                       | ✅       |

## What's left for the human (re-UAT)

I haven't actually launched `pnpm dev` on a Mac — that's the round-trip
that closes this UAT loop. Re-test checklist:

1. **Pill rendering.** Hold Ctrl+Cmd. The pill should render with clean
   rounded corners on _every_ background (Finder desktop, dark wallpaper,
   white app behind it). No white blocks.
2. **Main UI scrolls.** Resize the main window so the History grid is
   below the fold; verify mouse wheel + trackpad scroll reach it.
3. **Elapsed timer.** Hold Ctrl+Cmd silently for 5 seconds. Pill should
   read ~5.0s before transcription starts, not 0.0s.
4. **Partial keeps current.** Speak a long sentence. The most recently
   transcribed words should remain visible at the right edge of the
   partial row as new words flow in.
5. **Hotkey resync.** Hold Ctrl+Cmd. While holding, Cmd-Tab to another
   app and back. Release. Now press Ctrl+Cmd again — it should fire,
   not appear dead.
6. **LLM telemetry honesty.** With Ollama running, dictate a long
   sentence; the cleanup chip should show real ms. With Ollama stopped
   (`brew services stop ollama`), repeat — chip should read "unreachable"
   and a red banner should appear.
7. **Permissions panel.** Click "Permissions" in the topbar. Verify the
   three "Open" buttons launch the matching System Settings panes.
8. **Model honesty.** `export BOOTHRFLOW_WHISPER_MODEL_FILE=ggml-small.en.bin`
   then `pnpm dev`. The STT chip should read "Whisper small.en", not
   "Whisper tiny.en".

If all eight pass: the wave is done. If any regress, the fix points
are isolated to specific files and easy to bisect.

## Out-of-scope follow-ups (open as separate items)

- **Structured / app-aware formatting** — Eric called this out as a
  Wispr Flow feature he wants. Drafted into ROADMAP.md as a Phase 2
  backlog item; not landed here.
- **Whisper model picker UI** — currently env-var only. Defer until the
  Parakeet TDT pivot in ADR-009 lands; building a picker for whisper-only
  variants is throwaway work.
- **Onboarding wizard** — block-the-app-until-permissions-granted flow.
  Pre-work for prod bundles; polish for Wave 6.
- **GPU Metal as default on Apple Silicon** — currently `--features
"real-engines gpu-metal"` opt-in. Should auto-enable on `aarch64-apple-darwin`
  via Cargo `cfg`-gated default features.
