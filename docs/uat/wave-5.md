# Wave 5 UAT — Context-aware cleanup + auto-learning + Parakeet

**Date:** 2026-05-01
**Branch:** `main`
**Reviewer:** Eric
**Goal:** lift cleanup quality with three new signals (foreground app
context, focused-window OCR, user-curated + auto-learned correction
list) and ship a faster STT engine option (Parakeet TDT 0.6B v3 via
sherpa-onnx).

This UAT plan covers commits `022876f` (5a), `b367eb0` (5b), `966f85a`
(5c), and the runtime OCR / model download / engine switch landings
that follow. Read in order — sections build on each other.

---

## Pre-flight

1. **Build with both engines available.** Whisper is the safe default;
   Parakeet is opt-in via cargo feature. For UAT we want both:

   ```bash
   cd src-tauri
   cargo build --features "real-engines parakeet-engine"
   ```

   First-time build pulls a ~150MB sherpa-onnx prebuilt + ONNX runtime
   download. Subsequent builds are fast.

2. **Download the Parakeet model bundle.** Multi-file ONNX bundle from
   sherpa-onnx; the script normalizes the layout so the engine can
   find the four expected files:

   ```bash
   pnpm download:model:mac parakeet
   ```

   Installs to
   `~/Library/Application Support/boothrflow/models/parakeet-tdt-0.6b-v3/`
   with `encoder.onnx`, `decoder.onnx`, `joiner.onnx`, `tokens.txt`.

3. **Grant Screen Recording permission.** Required for the OCR cleanup
   context. macOS 14.4+ shows the Privacy & Security pane. From the
   app: **Settings → General → Permissions → Screen Recording → Open**.
   In dev mode the prompt is attributed to the parent terminal, so
   relaunch your terminal after granting.

4. **Confirm Accessibility is granted** (it should be, from earlier
   waves' paste injection). Auto-learn corrections need it for the
   AX-read of the focused field's contents post-paste. Same Settings
   panel.

---

## Part 1 — App-context detection (Wave 5a)

### What it does

The cleanup system prompt now carries an `<APP-CONTEXT>` block when
a foreground app is detected:

```
<APP-CONTEXT>
Active app: TextEdit
App identifier: com.apple.TextEdit
Window title: Untitled.txt
</APP-CONTEXT>
```

Lets the LLM tone-match the destination — Slack tends to short
sentences with contractions, Pages tends to formal prose.

### How to verify

- [ ] Open TextEdit, place the cursor in a document.
- [ ] Hold push-to-talk (`Ctrl+Cmd`), say a few sentences, release.
- [ ] In the terminal running `pnpm dev`, look for a debug log:
      `context: app=com.apple.TextEdit window=...`
- [ ] Repeat in Slack — the log should now show
      `app=com.tinyspeck.slackmacgap`. (Window title may be empty;
      that's expected — we don't pull AX-window-title via the
      detector, only the bundle ID + localized name.)

### Known limitations

- Window title is `None` on macOS by default. The detector reads
  `NSWorkspace::frontmostApplication()` for the bundle ID + localized
  name; window-title needs an AX call we deliberately skip in the
  detector to keep latency low. The AX read in the learning
  coordinator pulls field content, not window title.

---

## Part 2 — Common mishearings editor (Wave 5a)

### What it does

A new "Common mishearings" editor in **Settings → Voice → Recognition**
lets you maintain a list of `wrong → right` substitutions. The
cleanup prompt's `<USER-CORRECTIONS>` block injects these as
authoritative — the LLM applies them on every cleanup.

### How to verify

- [ ] Open Settings → Voice → Recognition → Common mishearings.
- [ ] Add a row: `kwen → Qwen`. Watch it persist (close + reopen
      Settings; the row should still be there).
- [ ] Add another: `paython → Python`.
- [ ] Dictate: "I'm using kwen for cleanup and paython for scripting"
      — the paste should read "I'm using Qwen for cleanup and Python
      for scripting".
- [ ] Try with leading/trailing whitespace (" kwen ") — the row
      should still apply (Wave 5b adds whitespace trimming).
- [ ] Empty rows are ignored: leave one row blank, add another with
      content; the blank one shouldn't show up in the prompt.

---

## Part 3 — Auto-learn corrections (Wave 5b + 5c)

### What it does

After a paste, the learning coordinator waits 8 seconds, reads the
focused text field via the macOS Accessibility API, and looks for a
small single-word edit. If found (Levenshtein ≤ 3, single-word swap,
not capitalization-only), it appends the pair to your
`commonly_misheard` list automatically.

### How to verify

- [ ] Settings → Voice → Recognition → toggle on **Auto-learn
      corrections after paste (preview)**.
- [ ] Open TextEdit. Dictate something Whisper will likely mishear —
      a name, a model ID, jargon. (Useful to dictate "I'm using kwen
      for cleanup" — Whisper-tiny often mishears Qwen.)
- [ ] After the paste, manually edit one word. Wait ~10 seconds.
- [ ] Open Settings → Voice → Recognition → Common mishearings. The
      auto-learned pair should appear in the list.
- [ ] Trigger a second dictation with the same word — the cleanup
      should apply the substitution.

### Known limitations

- **Apps that don't expose AX values:** Slack message field, some
  Discord / Electron app fields, `contenteditable` div hierarchies
  in some web apps. The coordinator silently no-ops for these.
- **macOS 15 (Sequoia) + non-frontmost apps:** AX reads from a
  non-frontmost app return null. We always read post-paste so the
  target is frontmost — should be fine.
- **Settings change mid-window:** If you toggle auto-learn off
  during the 8-second settling window, the coordinator re-checks
  the flag after sleeping and aborts.
- **Privacy mode:** auto-learn is suppressed when privacy mode is on
  (gated at both spawn site and post-sleep).

---

## Part 4 — OCR-aware cleanup (Wave 5a structure + 5d wiring)

### What it does

When enabled, the OCR pass captures the visible on-screen text via
`CGDisplayCreateImage` + Vision's `VNRecognizeTextRequest`, and
injects it as a `<WINDOW-OCR-CONTENT>` block in the cleanup prompt.
Used by the LLM as supporting context to disambiguate names, model
IDs, file names, jargon — Whisper sees a phonetic guess; the OCR
sees what the user is actually looking at.

### Pre-flight

1. **Grant Screen Recording.** Settings → General → Permissions →
   Screen Recording → Open. After granting, **relaunch the
   terminal** that started `pnpm dev` (dev-mode TCC attribution
   limitation).

2. **Enable the toggle.** Settings → Voice → LLM → toggle on **Use
   focused-window OCR as cleanup context (preview)**.

### How to verify

- [ ] Open a document containing a known marker phrase, e.g.
      "Wave 5 hand-off — known marker phrase".
- [ ] Run with verbose logging: `RUST_LOG=boothrflow=trace pnpm dev`.
- [ ] Dictate something ambiguous: "let me update the qwen config".
      Whisper-tiny will likely render "qwen" as "kwen" or similar.
- [ ] Look at the trace log for the system prompt — the
      `<WINDOW-OCR-CONTENT>` block should contain text from your
      open document, including the marker phrase.
- [ ] The pasted output should reflect the LLM disambiguating
      against the visible context (e.g. correcting "kwen" → "Qwen"
      because Qwen is on screen).
- [ ] Toggle OCR off → dictate the same phrase → confirm no OCR
      block in the system prompt.
- [ ] Privacy mode on (Settings → General → Style → Privacy mode)
      → confirm OCR is suppressed regardless of the OCR toggle.

### Latency

CGDisplayCreateImage + Vision Fast OCR on a 1080p screen: ~200-400ms
on Apple Silicon. Counts toward the cleanup pass timing on the
`dictation:done` event. Watch the LLM ms in the bottom-bar telemetry.

### Known limitations

- **Captures the whole main display, not just the focused window.**
  Full-display OCR is more disambiguation context per call, but
  costs more tokens. The `<OCR-RULES>` block tells the LLM to use
  it only as supporting context.
- **CGDisplayCreateImage is deprecated as of macOS 14.4** but still
  functional. Wave 5d should pivot to ScreenCaptureKit before the
  deprecated APIs are removed in a future macOS major.
- **Multi-monitor:** only the main display is captured. If you're
  dictating in a window on a secondary display, the OCR misses
  that context.
- **OCR sanitizer:** OCR'd text containing `<` / `>` is escaped
  to U+2039 / U+203A so an attack string with
  `</WINDOW-OCR-CONTENT>` can't close the block and inject fake
  instructions.

---

## Part 5 — Parakeet TDT engine (Wave 5c)

### What it does

NVIDIA Parakeet TDT 0.6B is a transformer-based STT model trained on
~36k hours of speech. Faster than Whisper tiny.en on Apple Silicon
(no Metal GPU dependency), more accurate on technical vocabulary.
Loaded via sherpa-onnx behind the `parakeet-engine` Cargo feature.

### Pre-flight

- Already built with `--features "real-engines parakeet-engine"` from
  Pre-flight step 1.
- Already downloaded via `pnpm download:model:mac parakeet`.

### How to verify

- [ ] Open Settings → Voice → Recognition → Whisper model.
- [ ] Pick **NVIDIA Parakeet TDT 0.6B v3 (preview)**.
- [ ] In the terminal running `pnpm dev`, look for:
      `parakeet: loaded from .../parakeet-tdt-0.6b-v3`
- [ ] Dictate a phrase with technical jargon: "I'm calling the
      OpenAI compat layer with a bearer token in the Ollama
      endpoint". Compare: - Whisper tiny.en → some words mangled - Parakeet → most words right
- [ ] STT latency should be lower than Whisper for a similar
      utterance length. Watch the `STT` ms in the bottom-bar
      telemetry.

### Known limitations

- **No streaming partials.** Parakeet supports streaming via
  sherpa-onnx's online recognizer, but the integration with
  `LocalAgreement2` is Wave 5d. While dictating, the pill won't
  show partial text — you'll only see the final transcript on
  release. Whisper streaming partials still work when Whisper is
  the active engine.
- **Multi-file model bundle.** Parakeet is 4 files in a directory,
  not a single .bin. The download script handles the layout; if you
  manually move things around, the engine init will fail with
  "missing model file: encoder.onnx".

---

## Part 6 — Privacy mode coverage (Wave 5b)

### What it does

Privacy mode now suppresses three new context channels in addition
to the LLM cleanup pass it already gated:

1. **OCR capture** — `cleanup_window_ocr` is checked AFTER the
   privacy-mode gate, so privacy mode wins.
2. **App context propagation to the LLM** — short-circuits the
   cleanup call entirely.
3. **Auto-learn coordinator** — gated at spawn site and re-checked
   post-settling.

### How to verify

- [ ] Toggle privacy mode on (Settings → General → Style).
- [ ] Dictate something. Confirm: - The `dictation:done` event shows `llm_ms = 0` (cleanup
      skipped). - No `<APP-CONTEXT>` / `<USER-CORRECTIONS>` /
      `<WINDOW-OCR-CONTENT>` blocks in the system prompt logs
      (the prompt isn't sent at all). - No auto-learn correction is recorded after a paste even
      if you make an edit.
- [ ] Toggle privacy mode off, retry — all three should re-engage.

---

## Part 7 — Prompt prefix caching (Wave 5a)

### What it does

The cleanup system prompt is structured with stable content first
(rules, corrections, app context) and dynamic content last (OCR).
Combined with Ollama's `keep_alive: "5m"` extra field, the KV cache

- model weights stay resident across consecutive dictations within a
  5-minute window — second dictation should be noticeably faster than
  the first.

### How to verify

- [ ] Wait at least 5 minutes since the last dictation (cache cold).
- [ ] Dictate a phrase. Note the LLM ms in the bottom-bar telemetry.
- [ ] Immediately dictate another phrase of similar length. The LLM
      ms on the second should be lower (cache warm).
- [ ] (Optional) Run `ollama ps` in another terminal — the model
      should show as resident with a 5m TTL.

### Known limitations

- **Heuristic gates `keep_alive` to Ollama only.** LM Studio
  (localhost:1234) and llama-server (:8080) skip the field — those
  endpoints sometimes 400 on unknown JSON keys. Cloud BYOK
  (OpenAI, Anthropic, Groq) also skips.

---

## Final checklist

- [ ] All 7 parts pass on a fresh build.
- [ ] No regressions in existing dictation flow (Whisper + LLM
      cleanup + paste + history).
- [ ] Total LLM latency in normal mode (no OCR, no privacy) is
      within ~10% of pre-Wave-5 baseline (~400-600ms on Qwen 2.5
      7B on M-series).
- [ ] Switching between Whisper and Parakeet via Settings reloads
      the engine without a daemon restart (next dictation uses the
      new engine).

## Verdict

_(filled in by Eric after running the checklist)_

---

## What's deferred to Wave 5d

- **Parakeet streaming partials** via `LocalAgreement2`.
- **Window-specific OCR** via ScreenCaptureKit (`SCContentFilter` +
  `SCStream`) — replaces the deprecated `CGDisplayCreateImage`.
- **Windows UIAutomation focused-field reader** — mirror of the
  macOS AX path, for the auto-learn coordinator.
- **Windows OCR** via `windows::Media::Ocr::OcrEngine`.
- **OCR length-based gate** — skip OCR when `raw_text > 500 chars`
  (the OCR's marginal value drops as the spoken text gets longer).
