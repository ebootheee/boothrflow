# boothrflow — Open-Source, Local-First Wispr Flow Replica

> Push-to-talk dictation that runs entirely on your machine: streaming STT → LLM cleanup with styles → paste into any app → searchable history. Fast, embedded, private.

> **PLAN.md is the original engineering plan from project inception.** It documents the architectural decisions and the original spec. For _current state_ (what's actually shipped, what's queued, what changed), use [`ROADMAP.md`](./ROADMAP.md) + [`CHANGELOG.md`](./CHANGELOG.md). Some examples here (e.g., the `casual / formal / excited` style names) reflect the original design — Wave 6 Phase 0 has since replaced the tone-based style system with a structuring-aggressiveness axis (raw / light / moderate / assertive). The architecture sections (audio path, STT/LLM dispatch, paste injection, storage) remain authoritative.

---

## 0. TL;DR — the decisions

| Question     | Answer                                                                                                             | Why                                                                                       |
| ------------ | ------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------- |
| Foundation   | **Greenfield Tauri 2 + Rust + Svelte 5** app, with patterns lifted from `cjpais/Handy` and `EpicenterHQ/epicenter` | Own brand, own license, no inherited scope debt; OSS prior art proves the architecture    |
| STT primary  | **NVIDIA Parakeet-TDT-0.6B-v3** via `sherpa-onnx`                                                                  | 1.93/3.59% WER on LibriSpeech, RTFx 3332x, native streaming RNN-T, 25 EU languages        |
| STT fallback | **whisper-large-v3-turbo** Q5_K_M via `whisper.cpp` (CUDA / Vulkan / Metal)                                        | Universal Windows GPU coverage, 99 langs, drop-in for languages Parakeet can't do         |
| VAD          | **TEN-VAD** primary, **Silero VAD** fallback                                                                       | TEN-VAD has the fastest speech→non-speech transition (decisive for dictation endpointing) |
| LLM cleanup  | **Qwen 2.5 3B Instruct Q4_K_M** via `llama.cpp` (in-process, not Ollama)                                           | Best instruction-following at 2GB, speed/quality sweet spot                               |
| Embeddings   | **bge-small-en-v1.5** via ONNX Runtime                                                                             | 33M params, 384-dim, sub-50ms CPU inference                                               |
| Storage      | **SQLite + FTS5 + sqlite-vec**                                                                                     | Hybrid lexical + semantic search in one file                                              |
| Hotkey       | `tauri-plugin-global-shortcut` + `rdev` (hold-to-talk) + `win-hotkeys` (Win-key cases)                             | Layered: simple path covers 80%, fallbacks for the rest                                   |
| Audio        | `cpal` 0.15+ (WASAPI shared mode) → `rubato` resample to 16kHz mono                                                | Doesn't kill other audio apps, latency floor is STT not capture                           |
| Paste        | Hybrid: clipboard-write + `SendInput Ctrl+V` (default), `enigo` typing fallback, opportunistic UIAutomation        | What every dictation app actually does, including Wispr                                   |
| LLM target   | Run on CPU; local LLM is optional / off by default                                                                 | Lots of users won't have a GPU; CPU 3B Q4 is fast enough on modern Ryzen/Intel            |

VibeVoice is text-to-speech (synthesis) — dead end for dictation. Microsoft's `VibeVoice-ASR` (Jan 2026) exists but is 9B params, way too heavy for sub-500ms PTT. We're not building on either.

---

## 1. Vision & Positioning

**boothrflow is to Wispr Flow what Obsidian is to Notion.** Local, free, owns-your-data, extensible, opinionated.

**Three things we must absolutely beat Wispr on:**

1. **Privacy** — 100% local by default, zero network calls in the hot path. BYOK cloud STT/LLM is opt-in only.
2. **Footprint** — Tauri 2 build under 30MB, ~80MB resident idle. (Wispr Electron build = ~250MB install, ~800MB RAM under load on Windows.)
3. **Persistence** — searchable, semantic history of every dictation, with cross-app recall. Wispr has no real "memory" feature in 2026.

**Three things we must reach parity on:**

1. **Course-correction handling** — "I mean..." / "actually..." → final intent only. The single most-loved Wispr behavior.
2. **App-context-aware formatting** — Slack vs Gmail vs Cursor get different tone/punctuation automatically.
3. **Sub-500ms perceived latency** — from key release to text on screen.

**Things we don't need to ship in v1:**

- Mobile (Wispr's edge is iOS/Android; we're desktop-first)
- Cloud sync of dictionary/snippets across devices
- Team features
- Voice-control automation (Talon's territory)

---

## 2. Foundation Decision — Three Paths

| Path                                         | Effort                    | License                         | Brand           | Recommendation          |
| -------------------------------------------- | ------------------------- | ------------------------------- | --------------- | ----------------------- |
| **A. Greenfield Tauri 2 + Rust + Svelte**    | High (8-12 wk to v1)      | Yours to choose                 | Yours           | **★ Recommended**       |
| B. Fork `cjpais/Handy`                       | Medium (5-7 wk to v1)     | Inherits Handy's (GPL-flavored) | Forked identity | Solid plan B            |
| C. Fork `EpicenterHQ/epicenter` (Whispering) | Medium-Low (4-6 wk to v1) | AGPL-3.0 inherited              | Forked identity | Only if you accept AGPL |

**Why greenfield wins for boothrflow specifically:**

- You own the brand and the license decision. AGPL is fine for some intents, blocking for others.
- The plumbing surface is small enough that copying patterns from Handy gets you 70% of the speed advantage of a fork without inheriting any scope.
- Our feature set (history-as-first-class, semantic memory, app-context routing) doesn't map cleanly onto Whispering's data model. We'd be ripping a lot out anyway.
- The hot loop (audio → VAD → STT → LLM → paste) is ~600 lines of Rust. Not the bottleneck.

**Day 1 sanity check before committing:** spend half a day cloning and running both Handy and Whispering. If either of them already feels 80% of the way to your vision, fork. If you find yourself wanting to delete more than you keep, greenfield. Decide by end of Day 1.

---

## 3. Architecture (mental model)

```
┌──────────────────────────────────────────────────────────────────────┐
│                         boothrflow process                            │
│                                                                       │
│   ┌────────────────────────────────────────────────────────────┐     │
│   │  Frontend (Svelte 5 in WebView2/WKWebView)                  │     │
│   │  - Settings UI    - Flow Bar overlay    - History browser   │     │
│   │  - Style picker   - Dictionary editor   - Onboarding         │     │
│   └────────────────────────────────────────────────────────────┘     │
│                              ↕ Tauri IPC                              │
│   ┌────────────────────────────────────────────────────────────┐     │
│   │  Rust core (src-tauri/)                                      │     │
│   │                                                               │     │
│   │   ┌──────────┐   ┌──────────┐   ┌──────────┐                 │     │
│   │   │  Hotkey  │ → │  Audio   │ → │   VAD    │                 │     │
│   │   │  daemon  │   │  capture │   │ (TEN/    │                 │     │
│   │   │ (rdev)   │   │  (cpal)  │   │  Silero) │                 │     │
│   │   └──────────┘   └──────────┘   └────┬─────┘                 │     │
│   │                                       ↓                       │     │
│   │   ┌──────────────────────────────────────┐                   │     │
│   │   │  STT engine (transcribe-rs trait)     │                   │     │
│   │   │  - Parakeet via sherpa-onnx (default) │                   │     │
│   │   │  - whisper.cpp (fallback / non-EU)    │                   │     │
│   │   │  - WhisperKit (Mac, future)           │                   │     │
│   │   │  - BYOK Deepgram/Groq (opt-in)        │                   │     │
│   │   └──────────────┬───────────────────────┘                   │     │
│   │                  ↓                                             │     │
│   │   ┌──────────────────────────────────────┐                   │     │
│   │   │  Context router                       │                   │     │
│   │   │  - GetForegroundWindow → app exe      │                   │     │
│   │   │  - UIAutomation focused control       │                   │     │
│   │   │  - Per-app profile lookup             │                   │     │
│   │   └──────────────┬───────────────────────┘                   │     │
│   │                  ↓                                             │     │
│   │   ┌──────────────────────────────────────┐                   │     │
│   │   │  LLM formatter (llama-cpp-2)          │                   │     │
│   │   │  - Style + app-context aware system   │                   │     │
│   │   │    prompt (KV-cached)                 │                   │     │
│   │   │  - Skip for short utterances / raw    │                   │     │
│   │   │    mode / IDE code-mode               │                   │     │
│   │   └──────────────┬───────────────────────┘                   │     │
│   │                  ↓                                             │     │
│   │   ┌──────────────────────────────────────┐                   │     │
│   │   │  Injector                             │                   │     │
│   │   │  - Save clipboard → write text →      │                   │     │
│   │   │    SendInput Ctrl+V → restore clip    │                   │     │
│   │   │  - Fallback: enigo typing             │                   │     │
│   │   │  - Fallback: UIA SetValue             │                   │     │
│   │   └──────────────┬───────────────────────┘                   │     │
│   │                  ↓                                             │     │
│   │   ┌──────────────────────────────────────┐                   │     │
│   │   │  History (rusqlite + sqlite-vec)      │                   │     │
│   │   │  - utterances + sessions + embeddings │                   │     │
│   │   │  - bge-small-en-v1.5 (ONNX)           │                   │     │
│   │   │  - FTS5 hybrid search (BM25 + cosine, │                   │     │
│   │   │    fused with RRF)                    │                   │     │
│   │   └──────────────────────────────────────┘                   │     │
│   └────────────────────────────────────────────────────────────┘     │
└──────────────────────────────────────────────────────────────────────┘
```

The hot path latency budget (target 500ms p50, 700ms p99 from key release to text on screen):

| Stage                                               | Budget             | How                                                                                |
| --------------------------------------------------- | ------------------ | ---------------------------------------------------------------------------------- |
| End-of-utterance detection (VAD silence hangover)   | 200ms              | TEN-VAD with 700ms hangover, but emit on key-release, so this is mostly preemptive |
| STT (Parakeet TDT 0.6B v3)                          | 50-100ms           | RTFx > 1000 on RTX 4070; ~15s utterance decoded in ~15ms                           |
| Context detection                                   | <5ms               | Cached per-foreground-window                                                       |
| LLM cleanup (Qwen 2.5 3B Q4_K_M, ~50 output tokens) | 200-300ms          | TTFT ~80ms cached prefix, ~4ms/tok = ~280ms for 50 tok                             |
| Clipboard-paste-restore                             | 80-120ms           | 50ms paste latency + 50ms restore delay                                            |
| **Total**                                           | **~530-625ms p50** |                                                                                    |

To hit Wispr's 500ms p50: skip the LLM for short utterances (<6 words), aggressive prompt-caching, async clipboard restore.

---

## 4. Tech Stack — Concrete Crate List

### Rust core (`src-tauri/Cargo.toml`)

```toml
# App framework
tauri = { version = "2", features = ["tray-icon"] }
tauri-plugin-global-shortcut = "2"
tauri-plugin-autostart = "2"
tauri-plugin-single-instance = "2"
tauri-plugin-updater = "2"
tauri-plugin-clipboard-manager = "2"
tauri-plugin-store = "2"             # settings persistence
tauri-plugin-log = "2"
tauri-plugin-window-state = "2"

# Audio + DSP
cpal = "0.15"                        # mic capture (WASAPI shared)
rubato = "0.15"                      # resample → 16kHz mono
hound = "3"                          # WAV writes for debugging
dasp = "0.11"                        # peak-normalize, AGC
rnnoise-c = "0.4"                    # optional noise suppression

# VAD
ten-vad-rs = { git = "https://github.com/TEN-framework/ten-vad" }   # primary
voice_activity_detector = "0.2"      # Silero ONNX wrapper, fallback

# STT
sherpa-rs = "0.6"                    # for Parakeet TDT
whisper-rs = "0.13"                  # for whisper.cpp
# (Optionally use `transcribe-rs` 0.x as a unifying trait layer — Handy's pattern)

# LLM
llama-cpp-2 = "0.1"                  # in-process llama.cpp bindings
tokenizers = "0.20"                  # HF tokenizers for prompt mgmt

# Embeddings
ort = "2"                            # ONNX Runtime
fastembed = "4"                      # bge-small-en-v1.5 wrapper

# Storage
rusqlite = { version = "0.32", features = ["bundled", "fts5"] }
sqlite-vec = "0.1"

# Input / hotkeys / paste
rdev = "0.5"                         # raw key events for hold-to-talk
win-hotkeys = "0.4"                  # WH_KEYBOARD_LL on Windows
enigo = "0.2"                        # cross-platform keyboard sim
arboard = "3"                        # clipboard
clipboard-win = "5"                  # Windows-specific (preserve all formats)

# Windows-specific
windows = { version = "0.58", features = [
    "Win32_UI_Accessibility",
    "Win32_UI_Input_KeyboardAndMouse",
    "Win32_System_Threading",
    "Win32_System_ProcessStatus",
] }

# Utility
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
toml = "0.8"
tracing = "0.1"
tracing-subscriber = "0.3"
anyhow = "1"
thiserror = "1"
parking_lot = "0.12"
crossbeam-channel = "0.5"
```

### Frontend (`apps/ui/package.json`)

- Svelte 5 + SvelteKit (SPA mode) — Whispering's pick, mature
- Vite
- Tailwind v4 + shadcn-svelte
- `@tauri-apps/api` v2

### Models (downloaded to `%APPDATA%\boothrflow\models\` on first run)

| Model                              | File                              | Size   | Why                           |
| ---------------------------------- | --------------------------------- | ------ | ----------------------------- |
| Parakeet TDT 0.6B v3 (ONNX int8)   | `parakeet-tdt-v3-int8.onnx`       | ~600MB | Default English+EU STT        |
| Whisper large-v3-turbo (GGUF Q5)   | `whisper-large-v3-turbo-q5.gguf`  | ~600MB | Universal-language fallback   |
| Qwen 2.5 3B Instruct (GGUF Q4_K_M) | `qwen2.5-3b-instruct-q4_k_m.gguf` | ~2.0GB | Default LLM cleanup           |
| bge-small-en-v1.5 (ONNX int8)      | `bge-small-en-v1.5-int8.onnx`     | ~33MB  | Embeddings for history search |
| TEN-VAD (ONNX)                     | `ten-vad.onnx`                    | ~1MB   | Endpoint detection            |
| Silero VAD (ONNX)                  | `silero-vad.onnx`                 | ~2MB   | Fallback VAD                  |

Download wizard at first run; models can also be swapped in Settings → Models.

---

## 5. Feature Spec — Parity & Differentiator Matrix

Source: Wispr Flow product breakdown (April 2026). Key: ✅ ship in v1 / 🟡 v1.x / 🟢 v2 / ⛔ skip.

| #   | Feature                                            | Wispr                             | boothrflow plan                                                                       |
| --- | -------------------------------------------------- | --------------------------------- | ------------------------------------------------------------------------------------- |
| 1   | Push-to-talk hotkey, customizable, mouse buttons   | ✅                                | ✅ default `Ctrl+Win` (Win) / `Fn` (Mac); rebindable; mouse btn 4/5                   |
| 2   | Hands-free toggle mode                             | ✅                                | ✅ `Ctrl+Win+Space`                                                                   |
| 3   | Cancel via Esc                                     | ✅                                | ✅                                                                                    |
| 4   | Floating overlay during capture                    | ✅ Flow Bar                       | ✅ "Listen Pill" overlay (positioned near caret if obtainable)                        |
| 5   | Sub-500ms perceived latency                        | ✅ ~500ms                         | ✅ target 500ms p50                                                                   |
| 6   | Streaming STT with partial hypotheses              | partial                           | ✅ partials shown in scratchpad UI (we beat this — Wispr doesn't show real-time text) |
| 7   | Auto-edit (filler, course-correction, punctuation) | ✅                                | ✅ via LLM pass with explicit prompt                                                  |
| 8   | Cleanup levels (None/Light/Medium/High)            | ✅                                | ✅ same UX                                                                            |
| 9   | Style presets per app category                     | ✅ Formal/Casual/Excited/V.Casual | ✅ same four + custom user styles                                                     |
| 10  | Per-app rules (Slack vs Email vs IDE)              | ✅                                | ✅ TOML profiles in `%APPDATA%\boothrflow\profiles\`                                  |
| 11  | Code-aware dictation (camelCase, snake_case, CLI)  | ✅                                | ✅ via "code mode" prompt + IDE detection                                             |
| 12  | Personal Dictionary (manual + auto-learn)          | ✅                                | ✅ auto-learn on user post-edit (we'll log the diff)                                  |
| 13  | Snippets / voice-activated text expanders          | ✅                                | 🟡 v1.1                                                                               |
| 14  | Command Mode (highlight + speak transform)         | ✅ Pro                            | ✅ free                                                                               |
| 15  | Searchable history                                 | ✅                                | ✅ + semantic search (we beat this)                                                   |
| 16  | "Memory" — recall past dictations as paste options | ⛔                                | ✅ killer differentiator                                                              |
| 17  | Privacy Mode (zero retention)                      | ✅ Pro                            | ✅ free, default-on                                                                   |
| 18  | Local mode                                         | ⛔                                | ✅ default. Cloud is opt-in BYOK                                                      |
| 19  | Active app detection                               | ✅                                | ✅ `GetForegroundWindow` + UIA                                                        |
| 20  | Multi-monitor / DPI                                | ✅                                | ✅ via Tauri                                                                          |
| 21  | Auto-launch                                        | ✅ aggressive                     | ✅ opt-in, clearly disabled by default                                                |
| 22  | iOS / Android                                      | ✅                                | ⛔ desktop only                                                                       |
| 23  | Whisper Mode (sub-audible)                         | ✅                                | 🟢 v2 (needs custom acoustic model)                                                   |
| 24  | View diff after Command Mode transform             | ✅                                | ✅                                                                                    |
| 25  | Voice command "press enter"                        | ✅                                | ✅ + we extend: "new line", "delete that", "select all", "press tab"                  |
| 26  | File tagging in Cursor/Windsurf                    | ✅                                | 🟡 v1.1                                                                               |
| 27  | Banking-app auto-pause                             | ✅ Android                        | ✅ optional safelist of foreground apps that suppress dictation                       |
| 28  | HIPAA / SOC 2 / ISO 27001                          | ✅ Enterprise                     | ⛔ N/A — local-only sidesteps most of this                                            |
| 29  | Insights dashboard                                 | ✅                                | 🟡 v1.1 (just words/day, accuracy delta after edits, top apps)                        |
| 30  | Custom AI transformations chain                    | partial                           | ✅ pluggable post-processors (à la Whispering)                                        |

**Our additions (no Wispr equivalent):**

- A1: **Semantic history recall** — search past dictations by meaning, not just keyword.
- A2: **Quick-paste palette** — hold modifier, type a fuzzy query, paste from history.
- A3: **Memory threading** — group consecutive utterances in the same app/control as a "session" for replay.
- A4: **Plugin API** — pre-STT (audio filters), post-STT (text transforms), pre-paste (re-write) hooks.
- A5: **Open data** — export to JSONL / Markdown anytime.
- A6: **No telemetry** ever. Period.

---

## 6. STT Strategy in Detail

### Why Parakeet over Whisper as the default

|                       | Parakeet TDT 0.6B v3           | Whisper large-v3-turbo  |
| --------------------- | ------------------------------ | ----------------------- |
| Architecture          | FastConformer + RNN-T          | Transformer enc-dec     |
| Streaming-native      | **Yes** (RNN-T trained for it) | No (chunked workaround) |
| LibriSpeech clean WER | **1.93%**                      | 2.10%                   |
| LibriSpeech other WER | **3.59%**                      | 4.24%                   |
| TED-LIUM3 WER         | **2.75%**                      | 3.57%                   |
| RTFx (consumer GPU)   | **3332x**                      | 200x                    |
| Disk size             | ~600MB int8                    | ~1.6GB fp16 / ~600MB Q5 |
| Languages             | 25 EU                          | 99                      |
| License               | CC-BY-4.0                      | MIT                     |

Parakeet is faster, more accurate, and natively streaming. Whisper covers the language tail. Ship both, default to Parakeet, fall through to Whisper for unsupported languages.

### Streaming pipeline

```
mic stream (16kHz mono, 20ms frames)
  → ring buffer (last 30s)
  → TEN-VAD per-frame speech probability
  → on key-press: start recording
  → during recording: every 250ms, dispatch overlapping 1s chunk to STT
      → emit partial hypothesis to UI scratchpad
  → on key-release OR (silence > 700ms AND >2s of audio captured): finalize
      → run full Local-Agreement-2 reconciliation across chunks
      → produce final transcript
  → ship to LLM cleanup
```

**Why Local-Agreement-2** (`ufal/whisper_streaming` algorithm): re-decode overlapping chunks, commit the longest common prefix between the last two consecutive hypotheses. Gives streaming UX with non-streaming model behavior. Parakeet doesn't need this (true streaming) but the same code path handles Whisper.

### Custom vocabulary

Two layers, both runtime-mutable from Settings → Dictionary:

1. **Whisper `initial_prompt`** — pack up to 224 tokens of user dictionary terms before each session. Cheap, weak (fades after 30s).
2. **Hot-word boosting in `sherpa-onnx`** — Parakeet TDT in sherpa-onnx supports keyword boost during RNN-T beam search. Build a runtime trie of `["Anthropic", "Kubernetes", "kubectl", "Eric Boothe", "boothrflow", ...]` with a +10 dB log-prob boost. This is the right primary mechanism.

V2: LoRA fine-tuning on the user's own dictation history, opt-in.

### Honest cloud gap

Where local still loses to Wispr/Deepgram in 2026 — and what we do about it:

| Gap                            | Severity            | Mitigation                                                                   |
| ------------------------------ | ------------------- | ---------------------------------------------------------------------------- |
| Heavy non-native accents       | High                | Cloud-only training data tail. BYOK Deepgram fallback for users who need it. |
| Code-switching mid-sentence    | Medium              | Multilingual Parakeet helps; not perfect.                                    |
| Custom vocab handling polish   | Low                 | Hot-word boost gets us 90% of cloud quality.                                 |
| Punctuation / smart formatting | Zero gap            | This is the LLM pass, which is local.                                        |
| Latency                        | Zero gap on RTX 40+ | Slower CPU machines fall back to whisper.cpp medium                          |

---

## 7. LLM Formatting Strategy

### Model choice

**Default: Qwen 2.5 3B Instruct Q4_K_M** (~2GB, llama.cpp).

Reasoning: best instruction-following at the size, multilingual, Apache 2 license. Llama 3.2 3B is faster but less reliable on style instructions. Phi-4 mini (3.8B) is stronger on reasoning but 30% slower. Gemma 3 1B is too small for our task — it occasionally drops user intent.

Tiers exposed in Settings:

- **Off** — pass-through raw STT
- **Fast** — Llama 3.2 3B Q4 (~1.6GB, fastest TTFT)
- **Balanced** — Qwen 2.5 3B Q4_K_M (default)
- **Quality** — Qwen 2.5 7B Q4_K_M (~5GB, requires 16GB RAM)
- **BYOK Cloud** — Anthropic Claude Haiku / OpenAI 4o-mini / Groq Llama 3.3 70B (opt-in)

### Prompt design

System prompt is **identical across utterances within a style**, so llama.cpp's KV cache stays warm. Style switch invalidates the cache but only once.

```
<|im_start|>system
You rewrite raw spoken-dictation transcripts into clean written text.

STYLE: {{style}}
APP_CONTEXT: {{app_context}}
USER_NAME: {{user_name}}

Rules:
- Preserve the speaker's meaning exactly. Do not add information.
- Remove fillers ("um", "uh", "like", "you know") unless they're meaningful.
- Resolve self-corrections — if the speaker says "go to the store, I mean the office",
  output only "go to the office".
- Apply punctuation, capitalization, and paragraph breaks per the STYLE.
- Do NOT translate. Keep the speaker's language.
- Do NOT add greetings, sign-offs, or framing. Output only the cleaned text.
- For code/CLI contexts, preserve identifiers verbatim (camelCase, snake_case, kebab-case).

STYLE_GUIDE for "{{style}}":
{{style_guide_text}}

APP_GUIDE for "{{app_context}}":
{{app_guide_text}}
<|im_end|>
<|im_start|>user
{{raw_transcript}}
<|im_end|>
<|im_start|>assistant
```

Style guides are short (50-100 tokens each), shipped as TOML in `apps/core/styles/`:

```toml
# styles/formal.toml
[style.formal]
description = "Formal written English. Full punctuation, full capitalization, no contractions."
example_in = "yeah so basically what I want to say is uh we're moving forward with the plan"
example_out = "We are moving forward with the plan."
```

```toml
# profiles/slack.toml
[app.slack]
exes = ["slack.exe", "Slack.app"]
default_style = "casual"
overrides = "Lowercase first letter unless proper noun. Periods optional."
```

### When to skip the LLM

- Utterance < 6 words AND no obvious disfluency
- "Raw mode" hotkey held (`Ctrl+Win+R`)
- App-context = IDE code editor (configurable per-IDE)
- Style = "passthrough"

### Streaming caveat

Streaming output into the **target app** via SendInput typing breaks IMEs, jitters caret, and feels janky. We stream into our own **scratchpad UI** (a ghost overlay near the caret) so the user can see progress, then commit the final result to the target app via clipboard-paste.

---

## 8. Text Injection — The Killer Move

This is where most clones fail. Plan for it.

### Default path (works ~95% of the time)

```rust
fn paste(text: &str) -> Result<()> {
    let saved = clipboard::snapshot_all_formats()?;     // CF_UNICODETEXT, CF_HTML, CF_BITMAP, ...
    clipboard::write_text(text)?;
    sleep(Duration::from_millis(15));                    // give Windows time to acknowledge
    sendinput::ctrl_v()?;
    sleep(Duration::from_millis(80));                    // give target app time to read
    clipboard::restore(saved)?;
    Ok(())
}
```

### Failure modes & fallbacks

| Failure                                      | Detection                                     | Fallback                                                            |
| -------------------------------------------- | --------------------------------------------- | ------------------------------------------------------------------- |
| Target app is elevated, we are not           | `IsTokenElevated` + foreground window check   | Show toast: "Run boothrflow as admin to dictate into elevated apps" |
| Clipboard write fails (locked)               | `OpenClipboard` retries fail                  | Type via `enigo::text(text)`                                        |
| Password field (paste blocked)               | UIA control type = `Edit` + `IsPassword=true` | Either skip entirely (privacy) or type via `enigo` (config)         |
| IME composition active                       | UIA + `ITextInputProvider`                    | Wait for composition end, then paste                                |
| App that strips clipboard formatting (Excel) | Known-app rule                                | Force CF_UNICODETEXT only                                           |

### UIA opportunistic mode (advanced)

When the focused control is a known UIA edit (`IUIAutomation::GetFocusedElement` → `ValuePattern`), we can `SetValue(text)` directly without clipboard juggling. This is faster and doesn't trip clipboard listeners. Falls back to clipboard for anything that isn't a clean UIA edit (Electron apps = single canvas, Chrome web = partial).

Architecture: `Injector` is a trait with `ClipboardInjector`, `TypingInjector`, `UIAInjector` implementations and a `HybridInjector` that tries them in order based on the detected target.

### Clipboard preservation gotcha

Capture **all** formats, not just CF_UNICODETEXT. Users will have images, files, rich-text on their clipboard. Whispering and Wispr both occasionally lose user clipboard contents — we shouldn't.

---

## 9. History, Memory & Search

### Schema

```sql
CREATE TABLE sessions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    started_at TEXT NOT NULL,
    ended_at TEXT,
    app_exe TEXT,
    window_title TEXT,
    style TEXT
);

CREATE TABLE utterances (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id INTEGER REFERENCES sessions(id) ON DELETE CASCADE,
    ts TEXT NOT NULL,
    raw_text TEXT NOT NULL,
    formatted_text TEXT NOT NULL,
    style TEXT,
    app_exe TEXT,
    window_title TEXT,
    control_role TEXT,
    duration_ms INTEGER,
    user_edited INTEGER DEFAULT 0,         -- did user post-edit?
    final_text TEXT                          -- post-edit final, if known
);

CREATE VIRTUAL TABLE utterance_fts USING fts5(
    raw_text, formatted_text, content='utterances', content_rowid='id'
);

CREATE VIRTUAL TABLE utterance_vec USING vec0(
    embedding float[384]
);

-- triggers to keep FTS5 + vec in sync with utterances
```

### Embedding pipeline

After each finalized utterance: embed `formatted_text` with bge-small-en-v1.5 (ONNX, ~30ms CPU). Insert into `utterance_vec` keyed on utterance id.

### Hybrid search

```rust
fn search(query: &str, limit: usize) -> Vec<Utterance> {
    let bm25_results = fts5_search(query, limit * 4);            // top 4N by lexical
    let query_emb = embed(query);
    let vec_results = sqlite_vec_search(query_emb, limit * 4);   // top 4N by cosine
    rrf_merge(bm25_results, vec_results, k=60, limit)            // Reciprocal Rank Fusion
}
```

### Quick-paste palette UX

Hotkey: `Ctrl+Win+H` → small floating palette → fuzzy-search box → top 10 history hits → arrow keys → Enter pastes the formatted text into the focused app, ghost-restoring after.

### Privacy & controls

Settings → Data:

- "Save history" toggle (default ON, but "Save audio" default OFF)
- "Auto-delete after [N] days" (default 30)
- "Delete all" button + "Export to JSONL" button
- Per-app exclusion list ("Never log in 1Password.exe, KeePass.exe, banking apps")
- Privacy Mode (zero retention, reversible per-session)

---

## 10. UI/UX Spec

### Surfaces

1. **Tray icon** — only persistent surface. Three states: idle (gray), listening (pulsing red), processing (spinning). Right-click → menu (Open, History, Settings, Pause, Quit).
2. **Listen Pill** — ~280×40 floating overlay, appears near caret if we can resolve it, otherwise bottom-center of focused monitor. Shows: waveform + partial transcript + timer + cancel hint. Hides on completion. CSS: dark glass, subtle pulse.
3. **Scratchpad ghost** — semi-transparent panel showing partial STT output and post-LLM text. Slides up from Listen Pill on long utterances (>3s).
4. **Quick-paste palette** — `Ctrl+Win+H`. ~480×360 floating window, search input + result list. ESC dismisses.
5. **Settings window** — full Tauri window, tabs: General, Hotkeys, Models, Styles, Profiles, Dictionary, History, Privacy, About.
6. **Onboarding** — first-run wizard: pick mic, test mic + waveform meter, pick hotkey, download models (with progress + size), grant accessibility (Mac), Windows SmartScreen explainer.
7. **Flow Hub** — lightweight stats page: words today, words/min, accuracy delta after edits, top apps. Behind a menu item; not a primary surface.

### Visual feedback timing

- Hotkey press → Listen Pill appears within 30ms (pre-warm overlay window)
- First word transcribed → appears in scratchpad ghost within 250ms of speech start
- Hotkey release → "processing" animation, then text pasted within 500ms p50
- On error → toast notification with link to log

### Accessibility

- All interactive elements keyboard-navigable
- High-contrast mode toggle
- Configurable Listen Pill opacity / position
- Audible cue toggle (default off, optional "ping" on start/end)

---

## 11. Phased Roadmap

> **Note (May 2026):** this section is the original 12-week plan as
> drafted at project start. Phases 1, 2 (intelligence layer), and 3
> (memory + differentiators) shipped on `main` between April–May 2026
> across Waves 1–4b; the Wave 5 work on `feat/wave-5` adds the
> context-aware-cleanup features that were originally listed in
> Phase 2 (OCR window context, auto-learn correction store, app-
> context detection) plus a Parakeet engine implementation that the
> original plan had slotted for Phase 4. **Current state and the
> next-up work live in [`ROADMAP.md`](./ROADMAP.md) and
> [`CHANGELOG.md`](./CHANGELOG.md);** this section is preserved as
> the original spec / decision record.

### Phase 0 — Decide & Scaffold (Week 0, 3 days)

- Day 1: Spike — clone Handy and Whispering, run both, decide foundation. Write `DECISIONS.md`.
- Day 2: `pnpm create tauri-app`, set up workspace (`src-tauri/`, `apps/ui/`), hook up CI (GitHub Actions, `cargo check` + `pnpm build`), set up `cargo-deny`, configure tracing.
- Day 3: Decide license (MIT for permissive / Apache 2 for patent grant / GPL-3 for copyleft). Write `LICENSE`, `README.md` stub.

### Phase 1 — Hot Path MVP (Weeks 1-3)

Goal: hold hotkey, speak, get clean text pasted into Notepad.

- W1: Audio capture (cpal WASAPI shared) → 16kHz mono ring buffer. Mic enumeration + hot-swap. CLI test harness.
- W1: Global hotkey (rdev + tauri-plugin-global-shortcut) with hold-to-talk semantics. Listen Pill overlay (basic).
- W2: VAD integration (Silero first — easier ONNX path; TEN-VAD when bindings stabilize).
- W2: STT integration: whisper.cpp via whisper-rs first (simpler than sherpa-onnx), with whisper-large-v3-turbo Q5_K_M GGUF. End-to-end "speak → transcribed text in console".
- W2-3: Text injection: clipboard-write + SendInput Ctrl+V + restore. Test against Notepad, VS Code, Slack, Chrome, Word, Excel.
- W3: First-run model download wizard. Settings persistence (tauri-plugin-store). Tray icon.

**Deliverable:** Functional dictation app, no LLM cleanup, no history, no styles. Works in ~5 apps.

### Phase 2 — Intelligence Layer (Weeks 4-6)

- W4: llama.cpp integration (llama-cpp-2). Default Qwen 2.5 3B Q4_K_M. System prompt + style system. KV cache reuse.
- W4: Style presets (Formal/Casual/Excited/Very Casual + raw passthrough). Settings UI.
- W5: App-context detection (GetForegroundWindow + UIA focused control). Profile system in `%APPDATA%\boothrflow\profiles\*.toml`. Per-app default styles.
- W5: Skip-LLM heuristics (short utterance, raw mode, code-context).
- W6: Personal Dictionary (UI + auto-learn from user post-edit detection). Hot-word boost wiring.
- W6: Custom AI transformations (post-STT chain, à la Whispering — pluggable shell-out + LLM transforms).

**Deliverable:** Wispr-parity formatting, app-aware, dictionary-aware.

### Phase 3 — Memory & Differentiators (Weeks 7-9)

- W7: SQLite + FTS5 schema, history backend. Save every utterance. Settings → Data tab with retention controls.
- W7: bge-small-en-v1.5 ONNX. Embedding pipeline + sqlite-vec.
- W8: Hybrid search (FTS5 + vec, RRF fusion).
- W8: Quick-paste palette (`Ctrl+Win+H`). Fuzzy search UI.
- W9: Command Mode (highlight + speak transform). UIA `GetSelection` → LLM rewrite → SendInput replace.
- W9: Voice commands ("press enter", "new line", "delete that", "select all") via small finite parser.

**Deliverable:** Searchable history, semantic recall, command mode. All differentiators live.

### Phase 4 — Production Polish (Weeks 10-12)

- W10: Parakeet TDT integration via sherpa-onnx (default). Whisper becomes fallback for non-EU langs.
- W10: TEN-VAD swap-in.
- W11: Onboarding wizard. SmartScreen / Defender testing. Code signing (Azure Trusted Signing → cheap path).
- W11: Auto-update (tauri-plugin-updater + GitHub Releases).
- W12: Stability week. Crash reporting (sentry-rust optional, off by default). Logging UX. Bug bash.
- W12: 1.0 release.

### Beyond v1 (v1.x and v2)

- Mac port (WhisperKit integration, AXUIElement injection, Notarization)
- Linux port (X11 + Wayland injection, sherpa-onnx works fine on Linux)
- Snippets feature (voice-activated text expanders)
- Plugin API (pre-STT, post-STT, pre-paste hooks; WASM-sandboxed)
- LoRA fine-tuning on user's history (opt-in)
- "Whisper Mode" (sub-audible) — needs custom acoustic model training
- Insights dashboard
- File tagging in Cursor/Windsurf
- Mobile (probably not — outside scope)

---

## 12. Risk Register

| Risk                                                              | Likelihood | Impact | Mitigation                                                                                                                    |
| ----------------------------------------------------------------- | ---------- | ------ | ----------------------------------------------------------------------------------------------------------------------------- |
| Antivirus / EDR flags low-level keyboard hook as keylogger        | High       | Med    | Code-sign immediately, submit to MS reputation portal, document the hook clearly in source/UI. Don't install hook from a DLL. |
| Tauri WH_KEYBOARD_LL hook bug (#13919) bites us                   | Med        | High   | Run hook on a dedicated thread before window creation. Have rdev as Plan B.                                                   |
| Bluetooth headsets force HFP 8/16kHz codec → garbage audio        | High       | Med    | Detect at capture, surface a warning toast, suggest wired/2.4GHz dongle.                                                      |
| Paste fails in elevated apps (UIPI)                               | High       | Med    | Document; offer optional "Run as admin" mode.                                                                                 |
| Local LLM cleanup takes >300ms on weak hardware                   | Med        | High   | Allow LLM=Off mode; auto-detect and suggest based on hw probe.                                                                |
| Parakeet license (CC-BY-4.0) creates attribution burden           | Low        | Low    | Add NOTICE file, OK for OSS app.                                                                                              |
| GPU detection wrong → wrong model loaded                          | Med        | Med    | Test on AMD/Intel/NV. Manual override in Settings → Models.                                                                   |
| Whisper-cpp Vulkan path crashes on some Windows iGPUs             | Med        | Low    | Auto-fallback to CPU on init failure.                                                                                         |
| User's clipboard contents lost during paste                       | Low        | Med    | Comprehensive format snapshot/restore. Test against Excel, Photoshop, Figma.                                                  |
| Code signing cost / SmartScreen blocks installs                   | Cert       | High   | Budget Azure Trusted Signing ($10/mo) from day 1. Plan for SmartScreen warning during initial reputation building.            |
| Microsoft re-launches a free Windows-native dictation that's good | Med        | High   | Differentiate on local LLM, history/memory, OSS extensibility.                                                                |
| Hot-word boost in sherpa-onnx flaky for some terms                | Med        | Low    | Layer initial_prompt + LM rescoring as defense in depth.                                                                      |
| Wispr Flow ships local mode                                       | Low        | High   | They're cloud-by-design; their LLM trick requires their server. We're already there.                                          |

---

## 13. Repo Layout

```
boothrflow/
├── README.md
├── PLAN.md                         # this file
├── DECISIONS.md                    # ADRs as we make them
├── LICENSE
├── NOTICE                          # third-party model attributions
├── package.json                    # workspace root
├── pnpm-workspace.yaml
├── .github/
│   ├── workflows/
│   │   ├── ci.yml                  # lint + check + test on push
│   │   ├── release.yml             # tag → build → notarize → release
│   │   └── codeql.yml
│   └── ISSUE_TEMPLATE/
├── docs/
│   ├── architecture.md
│   ├── injection.md                # the paste pipeline in painful detail
│   ├── stt-stack.md
│   ├── llm-prompts.md
│   ├── memory-schema.md
│   └── plugin-api.md               # v2
├── apps/
│   ├── ui/                         # Svelte 5 frontend
│   │   ├── src/
│   │   │   ├── routes/
│   │   │   │   ├── +layout.svelte
│   │   │   │   ├── +page.svelte    # main settings
│   │   │   │   ├── history/
│   │   │   │   ├── styles/
│   │   │   │   ├── profiles/
│   │   │   │   ├── dictionary/
│   │   │   │   └── onboarding/
│   │   │   ├── lib/
│   │   │   │   ├── components/
│   │   │   │   ├── stores/
│   │   │   │   └── ipc.ts
│   │   │   └── overlays/
│   │   │       ├── listen-pill/
│   │   │       └── quick-paste/
│   │   ├── static/
│   │   ├── package.json
│   │   └── vite.config.ts
│   └── core/                       # shared TS types & style/profile assets
│       ├── styles/
│       │   ├── formal.toml
│       │   ├── casual.toml
│       │   ├── excited.toml
│       │   └── very-casual.toml
│       └── profiles/
│           ├── slack.toml
│           ├── outlook.toml
│           ├── gmail-chrome.toml
│           ├── code.toml           # VS Code / Cursor / Windsurf
│           └── default.toml
├── src-tauri/
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── build.rs
│   ├── icons/
│   ├── capabilities/               # Tauri 2 capability files
│   └── src/
│       ├── main.rs
│       ├── app.rs                  # Tauri builder, plugins, command registration
│       ├── audio/
│       │   ├── capture.rs          # cpal stream
│       │   ├── resample.rs         # rubato
│       │   ├── ring_buffer.rs
│       │   └── devices.rs          # enumeration, hot-swap
│       ├── vad/
│       │   ├── ten.rs
│       │   ├── silero.rs
│       │   └── mod.rs              # trait
│       ├── stt/
│       │   ├── whisper.rs
│       │   ├── parakeet.rs
│       │   ├── byok_cloud.rs
│       │   └── mod.rs              # trait + Local-Agreement-2
│       ├── llm/
│       │   ├── llama.rs            # llama-cpp-2 wrapper
│       │   ├── prompts.rs          # style + profile composer
│       │   └── byok_cloud.rs
│       ├── context/
│       │   ├── win_foreground.rs   # GetForegroundWindow + exe
│       │   ├── uia.rs              # UI Automation queries
│       │   └── profile_loader.rs
│       ├── injector/
│       │   ├── clipboard.rs
│       │   ├── typing.rs           # enigo
│       │   ├── uia.rs              # ValuePattern.SetValue
│       │   └── hybrid.rs           # strategy selector
│       ├── hotkey/
│       │   ├── global.rs           # tauri plugin
│       │   ├── rdev_listener.rs    # hold-to-talk
│       │   └── win_low_level.rs    # WH_KEYBOARD_LL escape hatch
│       ├── history/
│       │   ├── db.rs               # rusqlite + FTS5 + sqlite-vec
│       │   ├── embed.rs            # bge-small-en-v1.5 ONNX
│       │   ├── search.rs           # hybrid + RRF
│       │   └── retention.rs
│       ├── dictionary/
│       │   ├── store.rs
│       │   └── auto_learn.rs       # diff user post-edits
│       ├── pipeline.rs             # the hot loop
│       ├── overlay/
│       │   ├── listen_pill.rs
│       │   ├── scratchpad.rs
│       │   └── quick_paste.rs
│       ├── tray.rs
│       ├── ipc.rs                  # commands exposed to UI
│       └── settings.rs
├── models/                         # gitignored; populated at first run
│   └── .gitkeep
├── scripts/
│   ├── download-models.ps1
│   ├── download-models.sh
│   └── package-windows.ps1
└── tests/
    ├── integration/
    │   ├── injector_smoke.rs
    │   ├── e2e_dictation.rs
    │   └── apps/                   # known-app paste tests
    └── fixtures/
        └── audio/                  # test wavs
```

---

## 14. Day 1 — Concrete First Steps

```bash
# 1. Initialize repo
cd "C:\Users\ericb\Coding Projects\boothrflow"
git init
echo "node_modules\ntarget\n.DS_Store\nmodels/\n*.gguf\n*.onnx\ndist/\nbuild/" > .gitignore

# 2. Sanity-check spike — clone references
mkdir _spike && cd _spike
git clone https://github.com/cjpais/Handy
git clone https://github.com/EpicenterHQ/epicenter
# spend an hour in each. Pick foundation. Write DECISIONS.md.

# 3. Scaffold Tauri 2 app
cd ..
pnpm create tauri-app@latest .
# - name: boothrflow
# - identifier: dev.booth.boothrflow
# - language: TypeScript / JavaScript
# - package manager: pnpm
# - UI template: Svelte
# - flavor: TypeScript

# 4. First commit
git add .
git commit -m "scaffold: tauri 2 + svelte"

# 5. Boot it
pnpm install
pnpm tauri dev
```

After that, the Phase 1 W1 tasks are independent and good first-issues:

- (a) cpal mic capture → log RMS to console
- (b) global hotkey via tauri-plugin-global-shortcut → log to console
- (c) Listen Pill overlay window (always-on-top, click-through, transparent)

Stitch them together by end of W1.

---

## 15. References (the homework that produced this)

### OSS we're learning from

- Handy — `https://github.com/cjpais/Handy` (Tauri+Rust+React, closest reference)
- Whispering / Epicenter — `https://github.com/EpicenterHQ/epicenter`
- VoiceInk — `https://github.com/Beingpax/VoiceInk` (Mac, Power Mode pattern)
- VoiceTypr — `https://github.com/moinulmoin/voicetypr`
- Tambourine Voice — `https://github.com/kstonekuan/tambourine-voice`
- Whisper Writer — `https://github.com/savbell/whisper-writer` + LLM PR #102
- Buzz — `https://github.com/chidiwilliams/buzz` (batch but good Whisper patterns)
- nerd-dictation — `https://github.com/ideasman42/nerd-dictation` (minimal reference)

### Models

- Parakeet TDT 0.6B v3 — `https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3`
- Whisper large-v3-turbo — `https://huggingface.co/openai/whisper-large-v3-turbo`
- distil-large-v3.5 — `https://huggingface.co/distil-whisper/distil-large-v3.5`
- Moonshine — `https://github.com/moonshine-ai/moonshine`
- Qwen 2.5 3B Instruct — `https://huggingface.co/Qwen/Qwen2.5-3B-Instruct`
- bge-small-en-v1.5 — `https://huggingface.co/BAAI/bge-small-en-v1.5`

### Runtimes

- sherpa-onnx — `https://github.com/k2-fsa/sherpa-onnx`
- whisper.cpp — `https://github.com/ggml-org/whisper.cpp`
- faster-whisper — `https://github.com/SYSTRAN/faster-whisper`
- WhisperKit — `https://github.com/argmaxinc/WhisperKit`
- TEN-VAD — `https://github.com/TEN-framework/ten-vad`
- whisper_streaming (Local-Agreement) — `https://github.com/ufal/whisper_streaming`

### Rust crates

- cpal — `https://github.com/RustAudio/cpal`
- rdev — `https://github.com/Narsil/rdev`
- whisper-rs — `https://github.com/tazz4843/whisper-rs`
- llama-cpp-2 — `https://crates.io/crates/llama-cpp-2`
- transcribe-rs — `https://crates.io/crates/transcribe-rs`
- enigo — `https://github.com/enigo-rs/enigo`
- arboard / clipboard-win
- sqlite-vec — `https://github.com/asg017/sqlite-vec`
- rusqlite + FTS5

### Wispr Flow primary sources

- `https://wisprflow.ai/` + features + pricing + what's-new + data-controls
- `https://docs.wisprflow.ai/` (shortcuts, hands-free, command mode, context awareness)
- Baseten case study (Llama post-processing, latency targets) — `https://www.baseten.co/resources/customers/wispr-flow/`

### Benchmarks & analysis

- HF Open ASR Leaderboard — `https://huggingface.co/spaces/hf-audio/open_asr_leaderboard`
- Open ASR Leaderboard paper — `https://arxiv.org/abs/2510.06961`
- Best OSS STT 2026 (Northflank) — `https://northflank.com/blog/best-open-source-speech-to-text-stt-model-in-2026-benchmarks`

### Tauri docs

- Tauri 2 — `https://v2.tauri.app/`
- Plugins: global-shortcut, autostart, single-instance, updater, clipboard-manager, store
- Code signing on Windows — `https://v2.tauri.app/distribute/sign/windows/`

---

**Next action:** open `_spike/Handy` and `_spike/epicenter`, run them, write `DECISIONS.md` by end of day. That's where the project actually starts.
