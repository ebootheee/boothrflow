# Wave 4b — In-app Settings Panel

**Goal:** replace every env-var-only knob with a UI surface that persists.
After this wave, a non-technical user can switch models, rebind hotkeys,
configure their LLM endpoint, and edit their vocabulary without ever
opening a terminal.

## Strategy

- Single source of truth: `tauri-plugin-store` keyed by domain (whisper,
  llm, embed, hotkeys, styles, privacy). Already in the dep tree.
- Typed command surface (Rust → Specta → TS) so the FE doesn't mirror
  struct shapes by hand. ~15 commands total — the right scale to finally
  retire the deferred ADR-007 work.
- New "Settings" route or modal in `App.svelte`, separate from the
  topbar Permissions panel.
- Defaults pulled from existing `DEFAULT_*` constants so an unset store
  behaves identically to current env-var-driven behavior.

## Items

1. **Settings persistence layer** — `tauri-plugin-store` schema, defaults,
   migrations stub.
2. **Tauri command surface** — typed get/set for every setting via
   `tauri-specta` so the FE imports Rust types directly.
3. **Whisper model picker** — tiny.en / base.en / small.en / medium.en
   / large-v3-turbo, with download-on-select tied into
   `pnpm download:model:mac`.
4. **LLM endpoint + model + API key** — endpoint URL, model name,
   optional API key (stored via OS keychain when available, fallback to
   the encrypted `tauri-plugin-store` backend).
5. **Embed endpoint + model** — same shape as LLM, separate domain.
6. **Hotkey rebind UI** — PTT chord, toggle chord, quick-paste chord.
   Capture key chord live, validate against a small blocklist (no
   single Cmd / Ctrl, no system shortcuts).
7. **Per-app style overrides** — stub the data model now; populates
   once app-context detection lands in Wave 5.
8. **Privacy-mode toggle** — surface the existing `settings.svelte.ts`
   field. No new logic, just a switch.
9. **Vocabulary editor** — multi-line text area that overrides
   `BOOTHRFLOW_WHISPER_PROMPT`. Persist verbatim, no parsing.
10. **Settings UI shell** — route or modal, sectioned by domain, save
    on change (no global Save button — every flip is independent).
11. **Bump-default escape hatch** — flipping the LLM model picker
    rewrites the stored default and unsets `BOOTHRFLOW_LLM_MODEL` from
    the in-process env. Confirms the user's choice survives restart.
12. **Settings export / import** — `boothrflow.settings.json` round-trip
    so users can move their config between machines. Cheap to add
    while we're already serializing.

## Model labels — UX guideline (applies to every picker in this wave)

Always show the **specific parameter count** in dropdowns, never just
the family name. Helps the user predict latency / memory / quality at a
glance without a separate help link.

| ✅ Good                                        | ❌ Bad        |
| ---------------------------------------------- | ------------- |
| `Whisper small.en (244M, 466MB)`               | `Whisper`     |
| `Qwen 2.5 7B Instruct (~5GB, ~80 tok/s on M4)` | `Qwen 2.5`    |
| `nomic-embed-text v1.5 (137M, 274MB)`          | `nomic-embed` |

For models the user is currently running, append a `(active)` suffix
so the dropdown doubles as a status indicator. Tok/s figures come from
the same `dictation:done.llm_tok_per_sec` field Wave 4a wired up — when
present, show them inline; when absent, omit rather than show `?`.

## Out of scope

- App-context detection — Wave 5.
- OCR window context — Wave 5.
- Auto-learning correction store — Wave 5.
- Push connectors — Wave 6.
- Cloud BYOK config UX beyond a single API-key field — Wave 6 polish.
- Onboarding wizard that walks users through Settings on first run —
  Wave 7 production polish.

## UAT

A single Mac dictation session should cover:

1. Open Settings, switch Whisper model from `tiny.en` to `small.en`,
   confirm the model downloads and the next dictation uses it.
2. Open Settings, switch LLM model from `Qwen 2.5 7B` to `Qwen 2.5 1.5B`,
   confirm tok/s on the cleanup chip drops accordingly.
3. Rebind PTT from `Ctrl + Cmd` to `Ctrl + Shift + Space`, confirm the
   new chord triggers dictation and the old one does not.
4. Add `kubernetes, terraform, GraphQL` to the vocabulary editor,
   confirm those words come through cleanly on the next dictation.
5. Toggle Privacy mode on, confirm the cleanup pass is bypassed.
6. Quit and relaunch — every setting persists.

If all six pass, Wave 4b is done. Wave 5 (context-aware cleanup, OCR,
auto-learning) gets the green light.
