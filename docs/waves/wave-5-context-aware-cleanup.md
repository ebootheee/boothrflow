# Wave 5 — Context-aware cleanup + post-paste learning

**Goal:** lift cleanup quality above what Whisper-tiny + a generic prompt
can produce by feeding the LLM three new signals at cleanup time:

1. **Foreground app + window title** so tone matches the destination
   (Slack vs. Pages vs. an IDE).
2. **A best-effort OCR snapshot of the focused window's contents** so the
   LLM can correct names, jargon, model IDs, file names that Whisper
   mishears but that are visible on screen.
3. **A user-curated correction list** (vocabulary terms + wrong→right
   pairs) that gets injected as authoritative spellings, both from
   Settings and — eventually — auto-learned from the edits the user
   makes after pasting.

The first wave of work in this branch lands the **structure** for all
three: a new prompt builder that emits `<USER-CORRECTIONS>`,
`<APP-CONTEXT>`, `<OCR-RULES>`, and `<WINDOW-OCR-CONTENT>` blocks; a
real `ContextDetector` for foreground-app introspection on macOS and
Windows; and the settings + UI surfaces for the correction list and the
OCR opt-in.

The two parts that are **hard to validate from a dev session** —
runtime OCR capture (requires Screen Recording TCC and a focused
content window to test against) and the Parakeet TDT 0.6B engine
(multi-file ONNX bundle, sherpa-onnx C++ build, streaming refactor) —
are explicitly handed off here. Read the rest of this doc end-to-end
before you start; the pieces depend on each other.

## What's already shipped on this branch

- `src-tauri/src/llm/prompt.rs` — pure builder with unit tests covering
  legacy match, corrections-block emission, OCR truncation, the
  Captain's Log path, and app-context formatting. Order is stable so
  Ollama prompt-prefix caching has a long shared prefix.
- `src-tauri/src/llm/openai_compat.rs` — sends `keep_alive: "5m"` so the
  KV cache + model weights stay warm across consecutive dictations
  (Ollama-specific extra; OpenAI-compat layer ignores fields it doesn't
  recognize, harmless on cloud BYOK).
- `src-tauri/src/context/real.rs` — production foreground-app detector.
  macOS: `NSWorkspace::sharedWorkspace().frontmostApplication()` →
  bundle ID + localized name. Windows: `GetForegroundWindow` +
  `GetWindowTextW` + `K32GetModuleFileNameExW`. Linux: stub.
- `src-tauri/src/session.rs` — wires the detector + a privacy-mode-gated
  call to `crate::ocr::capture_focused_window_text` into the cleanup
  request, parses `vocabulary` into `preferred_transcriptions`, and
  passes `commonly_misheard` straight through.
- `src-tauri/src/settings.rs` — `MisheardReplacement` struct,
  `commonly_misheard: Vec<MisheardReplacement>`, `cleanup_window_ocr:
bool`, both wired through `SettingsPatch` + `Default` + `load` +
  `save_all`.
- `src/App.svelte` — Misheard-replacements editor in the Recognition
  section; OCR opt-in toggle in the LLM section.
- TS bindings regenerated (`pnpm gen` /
  `cargo run --example export_bindings --features real-engines`).

## What this doc hands off

### 1. macOS Vision OCR (the actual capture)

`src-tauri/src/ocr.rs::capture_focused_window_text` is a stub on every
platform — returns `Err(BoothError::internal(...))`. Session daemon
silently treats that as "no OCR available" and runs cleanup without
the `<WINDOW-OCR-CONTENT>` block, which is the right fallback. To wire
the real capture path on macOS:

1. **Crates** (add to `src-tauri/Cargo.toml` under
   `target.'cfg(target_os = "macos")'.dependencies`):

   ```toml
   objc2-vision = { version = "0.3", features = ["VNRequest", "VNRecognizeTextRequest", "VNTextObservation"] }
   objc2-screen-capture-kit = { version = "0.3", features = ["SCStream", "SCShareableContent", "SCContentFilter", "SCStreamConfiguration"] }
   objc2-core-graphics = { version = "0.3", features = ["CGImage"] }
   ```

   These pin to the same generation as the existing `objc2-app-kit`
   dep used by the context detector. If a newer family has shipped,
   bump them all together — the bindings are versioned in lockstep.

2. **Permission row.** Add a "Screen Recording" entry to the
   Permissions card (`src/lib/components/PermissionsCard.svelte` or
   wherever the existing Microphone/Accessibility/Input-Monitoring
   rows live). Tauri command name suggestion:
   `permission_screen_recording_status` returning the same
   `PermissionStatus` shape the others use. The detection call is
   `CGRequestScreenCaptureAccess()` (returns `false` if not granted)
   plus `CGPreflightScreenCaptureAccess()` for a non-prompting probe.
   Open-System-Settings deep link:
   `x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture`.

3. **Capture flow.** Inside `capture_focused_window_text`:
   - Use `SCShareableContent::getShareableContentExcludingDesktopWindows`
     and filter to the window that matches `app_context.app_exe` (bundle
     ID on macOS) plus `app_context.window_title`. Title match is
     fuzzy — apps tend to suffix " — Document name" / " · Channel
     name" / similar; do a `contains` after trimming common separators.
   - Build an `SCContentFilter` from that single `SCWindow`, set
     `SCStreamConfiguration::captures_audio = false`, `pixel_format =
BGRA`, and request a single frame via the `SCStream` one-shot
     pattern (`SCStream::startCapture` → first sample buffer →
     `SCStream::stopCapture`).
   - Convert the `CMSampleBuffer` → `CGImage` and feed it to
     `VNImageRequestHandler::initWithCGImage:options:` with a
     `VNRecognizeTextRequest` configured for `.fast` recognition level
     (latency over precision — the OCR is supporting context, not the
     primary signal). Set `usesLanguageCorrection = false` so we don't
     double-correct against the LLM.
   - Return the concatenated `topCandidates(1)` strings, joined with
     `\n`. Cap at the `MAX_OCR_CHARS` constant the prompt builder
     enforces (4000) so we never spend serialization time on data the
     prompt drops.

4. **Failure modes.** All of these MUST log via `tracing::warn!` and
   return `Err(BoothError::internal(...))` — the session daemon
   already swallows the error and proceeds without OCR:
   - Permission denied → users sees no banner; session continues
     without OCR. Expose state via the new permission row instead.
   - No matching window → stream-out of focus during the capture
     window. Just return the error; cleanup falls back gracefully.
   - Vision request returns no observations → return `Ok("")`. Empty
     strings are filtered out by the prompt builder.

5. **Testing strategy.** End-to-end testing requires:
   - A bundled or `tauri dev` build with the crate added.
   - First-run TCC prompt accepted (Screen Recording).
   - A focused content window with known text (e.g. open this file in
     a text editor with "Wave 5 hand-off — known marker phrase" near
     the top). Trigger a dictation, then assert the marker phrase
     appears somewhere in the system prompt logs.
   - Run with `RUST_LOG=boothrflow=trace` so the session daemon
     dumps the system prompt that goes to Ollama. Add a temporary
     `tracing::trace!("system prompt: {system}")` in
     `openai_compat.rs::cleanup` for the validation pass; remove
     before merging.

### 2. Windows OCR

`windows::Media::Ocr::OcrEngine` against a `SoftwareBitmap` from a
focused-window `BitBlt` capture. Crates:

```toml
windows = { version = "0.58", features = ["Media_Ocr", "Graphics_Imaging", "Win32_Graphics_Gdi", "Win32_UI_WindowsAndMessaging"] }
```

Rough flow:

- `GetForegroundWindow()` (already used by the context detector) gives
  an `HWND`.
- `GetWindowDC(hwnd)` + `BitBlt` into a `CreateCompatibleBitmap` of the
  window's client size. Convert to `SoftwareBitmap` via
  `WinRT.Graphics.Imaging.SoftwareBitmap.CreateCopyFromBuffer`.
- `OcrEngine::TryCreateFromUserProfileLanguages()` →
  `RecognizeAsync(softwareBitmap)`. Concatenate
  `result.Lines().Select(l => l.Text)` joined with `\n`.
- No TCC equivalent on Windows — capture works without a permission
  prompt. UAC matters only for elevated processes (we're not).

### 3. Parakeet TDT 0.6B v3 STT engine

The settings option is already there (`parakeet-tdt-0.6b-v3` in
`SettingsOptions::whisper_models`, marked `available: false`). The
Whisper model option list literally exposes the choice; selecting it
today fails the model resolution check and falls back. The handoff:

- **Crate:** `sherpa-rs = "0.7"` (sherpa-onnx Rust bindings) or wrap
  `sherpa-onnx-c-api` directly. The Rust crate is easier; the C wrap
  is more flexible if you want to support custom model bundles.
- **Model bundle.** Parakeet TDT 0.6B v3 ships as a multi-file ONNX
  set: `encoder.onnx`, `decoder.onnx`, `joiner.onnx`, plus a
  `tokens.txt`. Treat the directory as the unit, not a single file —
  this is the first time we'll have a non-Whisper engine, so factor
  the resolution layer (`commands::whisper_download_model`,
  `settings::whisper_model_path`) to handle either a single file or a
  directory bundle. Suggest a `ModelKind` enum:
  `WhisperGgml(PathBuf) | ParakeetOnnx(PathBuf /* dir */)`.
- **Streaming.** Parakeet supports streaming via sherpa-onnx's
  `OnlineRecognizer` API. Plug into the existing
  `stt::streaming::LocalAgreement2` aggregator unchanged — Parakeet
  will produce partials at a higher rate and shorter latency, but the
  LA2 dedupe layer is engine-agnostic. Initial commit can be
  non-streaming (offline transcribe only, like Whisper today) and we
  layer streaming on after.
- **Trait impl.** `ParakeetSttEngine: SttEngine` lives in
  `src-tauri/src/stt/parakeet.rs`. Mirror `whisper_real::WhisperEngine`
  — same constructor signature, same `transcribe` method, same
  `name()`. The `SttEngine` factory in `pipeline.rs` /
  `session.rs::spawn_session_daemon` switches on `ModelKind`.
- **Download.** Hosted on Hugging Face under
  `nvidia/parakeet-tdt-0.6b-v3-onnx`. Wire a tracker against the
  existing `whisper_download_model` (rename to `model_download` since
  it's no longer Whisper-specific). Same progress events the FE
  already consumes for Whisper.
- **License.** Parakeet TDT 0.6B v3 is CC-BY-4.0. Add an attribution
  line to `README.md` — the existing licensing block in there already
  has a Whisper acknowledgment to mirror.
- **Testing.** Snapshot test against a vendored 5-second WAV
  (`testdata/parakeet/the_quick_brown_fox.wav`) → assert the
  transcription contains "quick brown fox". Same shape as the existing
  Whisper snapshot test.

### 4. PostPasteLearningCoordinator — focused-field accessibility read

The Wave 5b commit lands the **structure** for auto-learning:

- `learning::detect_correction(pasted, current)` — pure function,
  unit-tested for: single-word swaps, capitalization-only rejects,
  short-word rejects, multi-word rejects, high-distance rejects,
  long-word rejects, word-boundary safety on punctuated edits.
- `learning::LearningCoordinator` — spawns a one-shot observation
  thread per paste; sleeps 8s, calls the focused-text reader, runs
  `detect_correction`, appends to `commonly_misheard` on a hit.
- `auto_learn_corrections: bool` setting + Settings UI toggle.
- `learning::FocusedTextReader` trait + macOS stub
  (`MacosFocusedTextReader::read_focused_text` returns `None`).

What's still stubbed: the actual `AXUIElement` call. The structure
mirrors the OCR stub pattern — coordinator gracefully treats `None`
as "no edit observable" and bails. To finish the wiring on macOS:

1. **Crate** — easiest path is the `accessibility` crate (high-level
   `AXUIElement` wrapper over ApplicationServices). Add to
   `[target.'cfg(target_os = "macos")'.dependencies]`:

   ```toml
   accessibility = "0.4"
   accessibility-sys = "0.1"
   core-foundation = "0.10"
   ```

2. **Call sequence** — replace the stub body in
   `src-tauri/src/learning/macos.rs::read_focused_text`:
   - `AXUIElement::system_wide()` — get the system-wide AX root.
   - `system_wide.attribute(&AXAttribute::new(&CFString::new("AXFocusedUIElement")))`
     → an `AXUIElement` for the currently-focused UI element.
   - `focused.attribute(&AXAttribute::value())` → the value, which is
     `AXValue::String` for plain text fields, `AXValue::AttributedString`
     for rich text. Coerce both via `to_string()`.
   - Return `Ok(string)` on success; `None` on any access error so
     the coordinator falls through silently.

3. **Edge cases** —
   - Web inputs (Chrome, Safari WKWebView) return AX values for
     standard `<input>`/`<textarea>` but not for `contenteditable`
     div hierarchies. Accept the loss; they're a minority of dictation
     destinations.
   - Electron apps vary: VS Code exposes AX values for the editor
     pane; Slack / Discord do not. Test against each manually.
   - macOS Sequoia / 15+: AX reads from a non-frontmost app return
     null. We always read post-paste (we just injected via `enigo`,
     so the target is frontmost). Should be fine.

4. **Permission** — Accessibility (already granted for paste
   injection via `enigo`). No new permission row needed.

5. **Testing** — manual UAT against:
   - macOS TextEdit (NSTextView, the easy case).
   - Notes (NSTextView with attributed text).
   - VS Code editor (Electron).
   - Chrome `<textarea>` (WKWebView).
   - Slack message field (Electron, _expected to fail_ — record to
     surface as a known limitation in the help text).

   For each, dictate something that contains a misrecognized word,
   correct it within 8 seconds, then trigger another dictation and
   verify the correction is in the `<USER-CORRECTIONS>` block via
   `RUST_LOG=boothrflow=trace`.

### 5. PostPasteLearningCoordinator — Windows UIAutomation read

Same shape, different API. The Rust binding crate is the existing
`windows = "0.58"` (already in deps for accessibility). The flow:

- `CoCreateInstance(&CUIAutomation, …)` to get an `IUIAutomation`.
- `GetFocusedElement(&mut element)` → `IUIAutomationElement`.
- `GetCurrentPattern(UIA_ValuePatternId, &mut pattern)` → cast to
  `IUIAutomationValuePattern`.
- `get_CurrentValue(&mut bstr)` → the focused field's text.

Caveats:

- COM apartment — UIAutomation requires MTA on the calling thread,
  which is what we want for a coordinator background thread.
  `CoInitializeEx(COINIT_MULTITHREADED)` once at coordinator startup.
- Some apps (legacy WPF / Win32) expose `TextPattern` instead of
  `ValuePattern`; fall through to that if `Value` returns null.
- Web browsers expose AX through UIAutomation reasonably well —
  Chrome and Edge both surface `<input>` / `<textarea>` values.

## Open questions

- **OCR cost vs. value.** A 1080p screen captures ~3500 chars of OCR
  text, which is ~900 tokens — about 8% of a 7B model's context. On
  Qwen 2.5 7B at the local Ollama settings, that's ~150ms additional
  cleanup latency. Worth the extra time? Initial guess: yes for
  short transcripts (where the OCR is signal-dense relative to the
  user input), no for long ones. Consider a length-based gate
  ("skip OCR when raw_text > 500 chars").
- **Parakeet model size.** 0.6B is the smallest variant; 0.6B-v3 is
  faster and more accurate than Whisper small.en in benchmarks but
  slightly slower than Whisper tiny.en. We may want to ship both
  Parakeet 0.6B and the larger 1.1B variant.
- **Auto-learned correction noise.** First few sessions of using the
  PostPasteLearningCoordinator will likely produce a lot of noise
  (typo edits, intentional rewrites). Surfacing them with a one-click
  "discard" UI is the right minimum. Two-corroboration gating helps
  but only after the second occurrence — a high-noise first session
  would still pollute the list.

## Smoke checklist for the next agent

- [ ] `cargo check --features real-engines` clean.
- [ ] `cargo test --features real-engines` — at minimum the prompt
      builder's 6 tests pass and the pipeline e2e tests still pass.
- [ ] `pnpm exec svelte-check --tsconfig ./tsconfig.json` clean.
- [ ] `pnpm gen` regenerates bindings without diffs (idempotent).
- [ ] Manual: macOS bundled build → grant Screen Recording → dictate
      against a window with known text → verify the OCR appears in
      tracing logs (with the temporary `tracing::trace!` mentioned
      above).
- [ ] Manual: toggle `cleanup_window_ocr` off → dictate → confirm no
      OCR block appears in the system prompt.
- [ ] Manual: toggle `privacy_mode` on → confirm OCR is suppressed
      regardless of `cleanup_window_ocr`.
