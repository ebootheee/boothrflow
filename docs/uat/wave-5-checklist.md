# Wave 5 UAT Checklist

Walk through top to bottom. Mark each box as you go. Notes column for
anything unexpected — those go straight into round-2 fixes.

Companion to `wave-5.md` (design + context). This file is just the
walk-through.

## Pre-flight

- [x] **Build with both engines.** From the repo root: `pnpm dev:parakeet`.
      This compiles with `--features parakeet-engine` so Parakeet is
      selectable. First build pulls a ~150MB sherpa-onnx prebuilt + ONNX
      runtime download; subsequent builds are fast.
      _Expected:_ Tauri dev window opens, no compile errors, the
      pill/tray launch normally.
      _Observed 2026-05-01:_ `pnpm dev:parakeet` compiled and launched
      the Tauri window with `--features parakeet-engine`.

- [x] **Download the Parakeet model bundle:** `pnpm download:model:mac parakeet`.
      _Expected:_ Files land in
      `~/Library/Application Support/boothrflow/models/parakeet-tdt-0.6b-v3/`
      (encoder.onnx, decoder.onnx, joiner.onnx, tokens.txt).
      _Observed 2026-05-01:_ bundle was already present; all four expected
      files exist.

- [ ] **Confirm Accessibility permission** is already granted (it
      should be, from earlier waves). Settings → General → Permissions →
      Accessibility row should not say "blocked".
      _Observed 2026-05-01:_ current Settings → General UI has no
      Permissions/Accessibility row to inspect.

- [x] **Don't pre-grant Screen Recording.** We test the eager prompt
      separately in Part 4. Skip this one.
      _Observed 2026-05-01:_ I did not grant or change Screen Recording
      permission during pre-flight.

---

## Round-1 regression sweep

These are the things round-1 (`781eb07`) fixed. Quick verification
that nothing got re-broken.

### R1.1 — Test connection no longer panics

- [x] Settings → Voice → LLM. Click **Test connection**.
- [x] _Expected:_ "OK · NN ms" green text appears within ~5s. No app
      crash, no terminal panic about "Cannot drop a runtime".
      _Observed 2026-05-01:_ actual Tauri Settings returned
      `OK · 1870 ms`; app stayed alive.

### R1.2 — Test button is above the OCR toggle

- [x] Settings → Voice → LLM. The button order should be: LLM cleanup toggle + help text; endpoint preset chips; endpoint / model / API key fields;
      **Test connection** button; **Use focused-window OCR** toggle + help text.
      _Observed 2026-05-01:_ verified in actual Tauri screenshot.

### R1.3 — Speech-to-text label

- [x] Settings → Voice → Recognition. Topmost field should read
      "Speech-to-text model" (not "Whisper model"), since Parakeet is
      now a peer.
      _Observed 2026-05-01:_ verified in localhost DOM for the same
      Settings implementation.

### R1.4 — Quickpaste fires on first press

- [ ] Press `Ctrl+Option+H` cold (haven't fired any other hotkey yet).
- [ ] _Expected:_ palette pops up immediately. No "press 3 times to
      get it" behavior.
      _Observed 2026-05-01:_ synthetic macOS `Option+Cmd+H` hid the app
      and no palette appeared. The legacy default conflicted with macOS
      `Cmd + H` "Hide app". Round-2 (`fix(wave-5e)`) changes the default
      to `Ctrl + Option + H` and migrates existing settings off the
      legacy default. Re-test with the new chord.

### R1.5 — Quickpaste corners are transparent

- [ ] With the palette open, look at the four rounded corners.
- [ ] _Expected:_ corners reveal whatever's behind the palette
      (desktop, app underneath). NOT a white square poking out.
      _Observed 2026-05-01:_ not runtime-verified because R1.4 did not
      open the palette. Code/browser review shows `transparent(true)` and
      transparent `html/body/#app`, but the actual window still needs a
      physical hotkey pass.

---

## Part 1 — App-context detection

### 1.1 — App context flows into the cleanup prompt

- [ ] Open TextEdit, place the cursor in a document.
- [ ] Hold `Ctrl+Cmd`, say "this is a quick test", release.
- [ ] In the dev terminal, look for: `context: app=com.apple.TextEdit window=...`
- [ ] _Expected:_ log line appears with a non-empty `app` value.
      _Observed 2026-05-01:_ not executed in this automation pass; needs
      live mic/manual app-focus dictation.

### 1.2 — Different app, different identifier

- [ ] Switch focus to Slack (or any other app).
- [ ] Trigger another dictation.
- [ ] _Expected:_ `context: app=com.tinyspeck.slackmacgap` (or the
      relevant bundle ID).
      _Observed 2026-05-01:_ not executed in this automation pass; needs
      live mic/manual app-focus dictation.

---

## Part 2 — Common mishearings editor

### 2.1 — Add a manual correction

- [ ] Settings → Voice → Recognition → "Common mishearings".
- [ ] Click **Add correction**. Type `kwen` → `Qwen`.
- [ ] Close + reopen Settings.
- [ ] _Expected:_ row persists.
      _Observed 2026-05-01:_ not executed in actual Tauri UI; browser DOM
      confirmed the Common mishearings editor is present.

### 2.2 — Cleanup applies the substitution

- [ ] Dictate: "I'm using kwen seven b for cleanup."
- [ ] _Expected:_ pasted text contains "Qwen" (not "kwen").
      _Observed 2026-05-01:_ not executed via live dictation.

### 2.3 — Whitespace tolerance

- [ ] Add another row with leading whitespace: `"  paython "` → `" Python "`.
- [ ] Dictate: "I'm scripting in paython."
- [ ] _Expected:_ pasted text contains "Python" (the whitespace
      shouldn't have broken the substitution).
      _Observed 2026-05-01:_ covered by Rust prompt-builder tests in
      `pnpm test:rust`; not executed via live dictation.

### 2.4 — Empty rows are ignored

- [ ] Add an empty row (Add correction → leave both fields blank).
- [ ] Dictate any phrase.
- [ ] _Expected:_ no errors. The empty row doesn't break cleanup.
      _Observed 2026-05-01:_ covered by Rust prompt-builder tests in
      `pnpm test:rust`; not executed via live dictation.

---

## Part 3 — Auto-learn corrections

### 3.1 — Toggle on auto-learn

- [ ] Settings → Voice → Recognition. Toggle on **Auto-learn
      corrections after paste (preview)**.
      _Observed 2026-05-01:_ not executed in actual Tauri UI; browser DOM
      confirmed the toggle is present.

### 3.2 — Auto-learn captures a correction

- [ ] Open TextEdit. Dictate something Whisper-tiny will likely
      mishear: "I'm using kwen for cleanup."
- [ ] Wait for the paste, then **manually edit one word** within
      ~7 seconds (e.g. fix "kwen" → "Qwen").
- [ ] Wait another 5 seconds (total ~12s post-paste).
- [ ] Settings → Voice → Recognition → Common mishearings.
- [ ] _Expected:_ a new auto-learned row appears with your edit.
      _Observed 2026-05-01:_ not executed; needs live paste/edit flow.

### 3.3 — Apps without AX values silently no-op

- [ ] Toggle auto-learn on. Open Slack message field. Dictate +
      paste + edit.
- [ ] _Expected:_ no auto-learned row (Slack doesn't expose AX
      values). No errors in the dev terminal — the coordinator just
      logs "focused-field read returned None" at trace level.
      _Observed 2026-05-01:_ not executed; needs live Slack/paste/edit flow.

### 3.4 — Privacy mode suppresses auto-learn

- [ ] With auto-learn on, toggle **Privacy mode** on.
- [ ] Dictate + paste + edit a word in TextEdit.
- [ ] _Expected:_ no auto-learned row. Cleanup also skipped (no LLM
      ms in the timing).
      _Observed 2026-05-01:_ not executed; needs live paste/edit flow.

---

## Part 4 — OCR-aware cleanup

### 4.1 — Eager Screen Recording permission prompt

- [ ] Privacy mode OFF. Settings → Voice → LLM.
- [ ] Toggle ON **Use focused-window OCR as cleanup context (preview)**.
- [ ] _Expected:_ macOS Screen Recording permission prompt appears
      immediately ("Terminal" or "boothrflow" wants to record screen).
- [ ] Click **Open System Settings** → grant Screen Recording for
      the entry that just appeared (Terminal in dev, boothrflow in a
      bundled build).
- [ ] **Quit and relaunch the dev server** (`pnpm dev:parakeet`)
      after granting (TCC attribution refresh).
      _Observed 2026-05-01:_ not executed. The OCR toggle was already on
      when I inspected the Tauri Settings UI, so this run could not verify
      the first-toggle eager prompt path cleanly.

### 4.2 — App now appears in System Settings → Screen Recording

- [ ] After granting in 4.1: System Settings → Privacy & Security
      → Screen Recording.
- [ ] _Expected:_ Terminal (or boothrflow) is listed and toggled on.
      _Observed 2026-05-01:_ not executed; depends on 4.1.

### 4.3 — OCR text reaches the cleanup prompt

- [ ] Re-enable the OCR toggle if it got cleared by the relaunch.
- [ ] Open a document containing a known marker phrase
      (e.g. "Wave 5 hand-off — known marker phrase").
- [ ] Run with verbose logs: kill `pnpm dev:parakeet`, restart with
      `RUST_LOG=boothrflow=trace pnpm dev:parakeet`.
- [ ] Dictate: "let me update the qwen config".
- [ ] In the trace logs, look for a system prompt containing a
      `<WINDOW-OCR-CONTENT>` block.
- [ ] _Expected:_ the block contains text from the document on
      screen, including the marker phrase.
      _Observed 2026-05-01:_ not executed; needs trace-mode live dictation
      with controlled foreground document.

### 4.4 — Cleanup uses OCR for disambiguation

- [ ] With OCR on, dictate something Whisper-tiny will mangle but
      that's clearly visible on screen (e.g. an unusual file name in
      a Finder window).
- [ ] _Expected:_ the pasted output corrects toward the on-screen
      spelling.
      _Observed 2026-05-01:_ not executed; needs live dictation.

### 4.5 — Toggling OCR off removes the block

- [ ] Toggle OCR off. Dictate the same phrase.
- [ ] _Expected:_ no `<WINDOW-OCR-CONTENT>` block in the trace logs.
      _Observed 2026-05-01:_ not executed; needs trace-mode live dictation.

### 4.6 — Privacy mode suppresses OCR regardless

- [ ] Toggle OCR on. Toggle Privacy mode on. Dictate.
- [ ] _Expected:_ no OCR block (and no LLM call at all — privacy
      mode short-circuits cleanup).
      _Observed 2026-05-01:_ not executed; needs trace-mode live dictation.

### 4.7 — OCR cost is reasonable

- [ ] Dictate a 5-second utterance with OCR on. Note the LLM ms in
      the bottom-bar telemetry.
- [ ] Dictate the same with OCR off.
- [ ] _Expected:_ OCR adds ~200-400 ms on Apple Silicon.
      _Observed 2026-05-01:_ not executed; needs paired live dictations.

---

## Part 5 — Parakeet engine

### 5.1 — Parakeet is selectable

- [ ] Settings → Voice → Recognition → Speech-to-text model.
- [ ] _Expected:_ "NVIDIA Parakeet TDT 0.6B v3 (preview)" is in the
      dropdown AND selectable (not greyed out). If it's still
      greyed, you didn't run `pnpm dev:parakeet` — check pre-flight.
      _Observed 2026-05-01:_ `cargo check --features parakeet-engine`
      passed and Rust options gate Parakeet with
      `cfg!(feature = "parakeet-engine")`; actual dropdown selectability
      still needs a manual Tauri UI pass.

### 5.2 — Parakeet loads

- [ ] Pick Parakeet from the dropdown.
- [ ] In the dev terminal, look for:
      `parakeet: loaded from .../parakeet-tdt-0.6b-v3`
- [ ] _Expected:_ no error events; the next dictation will use it.
      _Observed 2026-05-01:_ not executed; needs actual dropdown selection.

### 5.3 — Parakeet transcribes accurately

- [ ] Dictate a phrase with technical jargon: "I'm calling the OpenAI
      compat layer with a bearer token in the Ollama endpoint."
- [ ] _Expected:_ most words right, including "Ollama", "OpenAI",
      "bearer". (Whisper-tiny mangles these.)
      _Observed 2026-05-01:_ not executed; needs live dictation.

### 5.4 — Parakeet has no streaming partials

- [ ] Hold dictation a long time (10+ seconds) with Parakeet
      selected. Watch the pill.
- [ ] _Expected:_ pill stays in "listening" state with no live text.
      Final transcript appears on release. (Streaming integration is
      Wave 5e.)
      _Observed 2026-05-01:_ not executed; needs live dictation.

### 5.5 — Switching back to Whisper restores partials

- [ ] Settings → Voice → Recognition. Pick a Whisper model again.
- [ ] Dictate for 5+ seconds.
- [ ] _Expected:_ pill shows live partials again.
      _Observed 2026-05-01:_ not executed; needs live dictation.

---

## Part 6 — Prompt prefix caching

### 6.1 — Cold first dictation

- [ ] Wait 5+ minutes since the last dictation, OR `ollama stop` the
      current model so the cache is cold.
- [ ] Dictate a 4-second phrase with Whisper. Note the LLM ms.
      _Observed 2026-05-01:_ not executed via live dictation.

### 6.2 — Warm second dictation is faster

- [ ] Immediately dictate another similar-length phrase.
- [ ] _Expected:_ LLM ms is noticeably lower on the second
      (~50% faster typical, depends on hardware).
- [ ] Run `ollama ps` in another terminal to confirm the model is
      resident with a 5m TTL.
      _Observed 2026-05-01:_ direct Ollama OpenAI-compatible probe with
      `keep_alive: "5m"` succeeded; `ollama ps` showed `qwen2.5:7b`
      resident on 100% GPU with ~4 minutes remaining. Live dictation
      latency comparison was not executed.

---

## Final verdict

- [ ] **All 7 parts pass.**
- [ ] **No regressions** in the existing dictation flow (Whisper +
      LLM cleanup + paste + history + quickpaste).
- [ ] **Total LLM latency** in normal mode (no OCR, no privacy) is
      within ~10% of pre-Wave-5 baseline.

### What to do with failures

For each failure, drop a one-liner here so I have a punch list:

- [ ] _failure 1:_ Settings → General currently has no
      Permissions/Accessibility row, so the pre-flight Accessibility check
      cannot be performed from the UI described here.
- [ ] _failure 2:_ Synthetic `Option+Cmd+H` cold quick-paste test hid the
      app and did not show the palette; needs round-2/manual verification
      of first-press hotkey delivery and transparent corners.
- [ ] _failure 3:_ Remaining live mic/paste checks (app context, manual
      corrections application, auto-learn, OCR prompt/latency, Parakeet
      transcription/partials, LLM warm-vs-cold latency) were not executable
      from this automation pass and still need a physical UAT pass or a
      dedicated harness.

Once this list is done and the verdict line is checked, Wave 5 is
ready to push to origin.
