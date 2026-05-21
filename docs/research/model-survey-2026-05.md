# boothrflow Model Survey — May 2026

Research report for STT and LLM cleanup model upgrades. Survey date: **2026-05-21**.
Numbers cited are from public model cards, papers, and benchmarks dated 2025–2026.
Where a number was not findable, it is marked **N/F**. Nothing in this doc was invented.

---

## Section 1 — STT candidates

### Baseline: whisper-base.en (current default on macOS)

- 74M params, GGML via whisper.cpp, ANE/Metal via CoreML.
- Known regression in our own bench: "Mementis" / "Momentus" proper-noun substitution.
- Public LibriSpeech numbers for `base.en` are not surfaced cleanly in 2026 sources — the OpenAI paper is the authoritative reference, with `base.en` historically reported in the 4–6% test-clean range. Real-time-factor on M-series via whisper.cpp+CoreML is in the tens of x ([Voicci M1–M4 benchmarks](https://www.voicci.com/blog/apple-silicon-whisper-performance.html)).

### Parakeet-TDT-0.6B-v3 (current alternate engine)

- Released **2025-08-14**, 600M params, **CC-BY-4.0** license, FastConformer-TDT.
- LibriSpeech **test-clean: 1.93% WER**, **test-other: 3.59% WER** ([HF model card](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3)).
- Avg 6.34% across the HF Open ASR leaderboard, top-tier throughput ([HF model card](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3)).
- Known model-card limitations: "not recommended for word-for-word/incomplete sentences" and "out-of-vocabulary words…not likely to be recognized" — i.e., NVIDIA itself warns the model is weak on short fragments and proper nouns. This is consistent with the pronoun-swap behavior we hit in our 64s capture.
- **No native streaming in sherpa-onnx today.** ([k2-fsa/sherpa-onnx#2918](https://github.com/k2-fsa/sherpa-onnx/issues/2918)). NeMo has a chunked-inference script, but the int8 export sherpa-rs uses is offline only.
- Multilingual (25 European languages), still the latest in the v3 family (no 0.6B v4 yet).

**Status: this is still the latest 0.6B Parakeet as of 2026-05.**

### Parakeet 1.1B (TDT / RNNT / CTC)

- No "v3" 1.1B exists. The current 1.1B SKUs are `parakeet-rnnt-1.1b` (English, multi/Indic variants on NGC) and `parakeet-ctc-1.1b-asr` ([NGC catalog](https://catalog.ngc.nvidia.com/orgs/nvidia/collections/parakeet-1-1b-ctc-en-us), [NVIDIA build page](https://build.nvidia.com/nvidia/parakeet-1_1b-rnnt-multilingual-asr/modelcard)).
- These predate v3 — same FastConformer-Conformer family, larger but trained on less diverse data than v3.
- **For dictation, 1.1B is not a clear win over the v3 0.6B**: more compute, similar-or-worse English WER, no recent leaderboard wins.

**Expected delta vs current default (`whisper-base.en`):** WER ~2–4 points better on clean, ~3-5× more compute. Not recommended.

### parakeet.cpp (Frikallo, MIT)

- Active project, 273 stars, **MIT licensed** ([Frikallo/parakeet.cpp](https://github.com/Frikallo/parakeet.cpp)).
- Ships five models: TDT-CTC 110M (offline EN), TDT 600M (offline multi), **EOU 120M (streaming EN with end-of-utterance)**, **Nemotron 600M (streaming multilingual)**, Sortformer 117M (diarization).
- Built on **Axiom** tensor lib → MPSGraph fused encoder on Apple Silicon, plus CPU and CUDA paths. Models are `.nemo` → `safetensors`.
- M3 benchmark from the repo: 110M encoder, 10s audio → **27 ms GPU** vs 2,581 ms CPU (**96× speedup**); 600M TDT → 520 ms GPU vs 10,779 ms CPU (**21× speedup**); RTF ≈ 0.003, "~370× real-time" claimed.
- Comparison numbers from independent Mac speedtests: parakeet-mlx ran the 0.6B-v2 at 0.4995s avg; whisper.cpp large-v3-turbo-q5_0 with CoreML at 0.1935s — i.e., for the same audio, **Parakeet on MLX is ~2.6× slower than Whisper-turbo on CoreML** ([Mac whisper speedtest discussion](https://modelslab.com/blog/audio-generation/parakeet-cpp-vs-whisper-self-hosted-asr-comparison-2026)). Parakeet.cpp's MPSGraph path is younger than MLX and the Frikallo numbers suggest it is at least competitive with MLX, but we have no third-party reproduction yet.

**Expected delta vs current default (`whisper-base.en`):** Big WER win (Parakeet 600M scores ~1.93/3.59 vs base's ~4–6% test-clean), and on the Frikallo numbers ~10–30× faster on encoder for a similar-sized model — but the comparable head-to-head says whisper-turbo is still faster end-to-end on CoreML. Worth a measured trial; do not assume the speedup.

### NVIDIA Nemotron Speech Streaming (en-0.6B)

- Released **2026-01-05**, updated **2026-03-12** ([HF model card](https://huggingface.co/nvidia/nemotron-speech-streaming-en-0.6b)). Older Jan checkpoint is in a branch.
- 600M params, **NVIDIA Open Model License Agreement** (commercial OK, but not a permissive OSS license — flag for our license note in §4).
- Cache-Aware FastConformer-RNNT. Configurable chunk sizes via `att_context_size`: **80 ms, 160 ms, 560 ms, 1120 ms** — exact targets you spec'd in Wave 6 Phase 1.
- LibriSpeech at 1.12s chunk: **test-clean 2.32% WER, test-other 4.84% WER**.
- **sherpa-onnx has explicit graphs**: `sherpa-onnx-nemotron-speech-streaming-en-0.6b-int8-2026-01-14` and a newer `560ms-int8-2026-04-25` variant exist on HF ([csukuangfj's HF account](https://huggingface.co/csukuangfj/sherpa-onnx-nemotron-speech-streaming-en-0.6b-2026-01-14)). sherpa-onnx is shipping ONNX graphs with encoder/decoder/joiner split for per-component quantization ([sherpa-onnx#3408](https://github.com/k2-fsa/sherpa-onnx/issues/3408)).
- Native streaming — eliminates the buffered-replay overhead Parakeet v3 has.

**Expected delta vs current default:** Best-in-class streaming WER on English; lower CPU cost per chunk than buffered Parakeet; not directly comparable to base.en on M-series RTFx, but designed for low-latency CPU deployment.

### NVIDIA Canary-1B-Flash + Canary-Qwen-2.5B

- **Canary-Qwen-2.5B** currently tops the HF Open ASR leaderboard with **5.63% avg WER** and RTFx **418×** ([HF model card](https://huggingface.co/nvidia/canary-qwen-2.5b)).
- Canary-Qwen is a SALM (speech-augmented LM), not a thin ASR — it carries an LM head, larger memory footprint, and is _not_ streaming.
- **Canary-1B-Flash**: fast batch transcription, ~1B params; English-only flash version is the latency variant of canary-1b-v2 ([HF model card](https://huggingface.co/nvidia/canary-1b-flash)).
- License: NVIDIA Open Model License (same caveat as Nemotron).

**Expected delta vs current default:** WER significantly better, but model size and offline-only design makes these the wrong shape for short PTT dictations. **Skip.**

### Whisper-large-v3-turbo and "Whisper v4"

- As of April 2026, **no Whisper v4 has shipped**. `whisper-large-v3-turbo` (Oct 2024) is still the head of the OpenAI line ([HF turbo card](https://huggingface.co/openai/whisper-large-v3-turbo)).
- Turbo is the pruned-decoder Whisper (32 → 4 decoder layers), ~48% faster than large-v3, minor WER cost.
- whisper.cpp+CoreML on M-series runs `large-v3-turbo-q5_0` at ~0.19s avg per 10s sample, beating Parakeet-MLX on the same machine ([speedtest summary](https://modelslab.com/blog/audio-generation/parakeet-cpp-vs-whisper-self-hosted-asr-comparison-2026)).

### Distil-whisper-large-v3.5 (Mar 2025)

- Trained on 4× more diverse data than v3, "patient teacher" + SpecAugment ([HF model card](https://huggingface.co/distil-whisper/distil-large-v3.5)).
- Claims ~1.5× faster than `whisper-large-v3-turbo` on short-form with **slightly better** WER; ~2× faster than large-v3 on long form, same outputs.
- ONNX export available ([distil-large-v3.5-ONNX](https://huggingface.co/distil-whisper/distil-large-v3.5-ONNX)).

**Expected delta vs current default:** WER big improvement (close to large-v3); on short PTT clips it should still be slower than `base.en` in absolute ms, but quality jump is meaningful. ASR streaming is not the strong suit.

### WhisperKit / Argmax Open-Source SDK

- WhisperKit graduated to "Argmax OSS SDK" 1.0 ([releases](https://github.com/argmaxinc/argmax-oss-swift/releases)). MIT-licensed, Swift 6 concurrency. Argmax Pro SDK adds advanced models on top.
- Benchmark: **2.2% WER at 0.46s latency** on the standard server-vs-client comparison ([WhisperKit paper](https://arxiv.org/html/2507.10860v1)).
- **macOS-only** — not portable to our Windows build, so it can never be the _only_ default, but it could be the macOS-preferred runtime.

### Moonshine v2 / Moonshine Voice (Feb 2026)

- 245M params largest, **6.65% WER on HF Open ASR Leaderboard**, beats Whisper-large-v3 at 7.44% with ~6× fewer params ([UsefulSensors blog](https://huggingface.co/blog/UsefulSensors/announcing-moonshine-voice)).
- Streaming Moonshine WER (LibriSpeech, separate measurement): tiny streaming **12.00%**, base **10.07%** ([Moonshine v2 paper](https://arxiv.org/html/2602.12241)).
- Prebuilt for iOS, Android, macOS, Windows, Linux, Pi.
- License: MIT.

**Expected delta vs current default:** Streaming yes. WER better than `base.en` on the Open ASR leaderboard (6.65% vs ~10–15% on `base.en` per typical Whisper-base reports); ~3× faster than `whisper-tiny` per UsefulSensors. **Strong candidate for a streaming default.**

### Mistral Voxtral

- **Voxtral-Mini-3B-2507**: Apache 2.0, 3B, beats Whisper-large-v3 on FLEURS / Common Voice / Multilingual LibriSpeech ([HF model card](https://huggingface.co/mistralai/Voxtral-Mini-3B-2507)).
- **Voxtral Transcribe 2 (Feb 2026)**: sub-200ms configurable latency, Voxtral Realtime is the open real-time variant ([Mistral news](https://mistral.ai/news/voxtral-transcribe-2)).
- **Voxtral-Mini-4B-Realtime-2602**: realtime variant (~4B) on HF.

**Expected delta vs current default:** Significant WER win at 3B–4B param scale; but model size pushes RAM usage well past whisper-base (~74M) and Parakeet-0.6B. License is genuinely permissive (Apache 2.0), but no sherpa-onnx graph today. Plausible "stretch" pick if we want a Whisper-class quality bump on macOS, not Wave-6-ready.

### NeMo Sortformer / streaming

Sortformer is diarization, not transcription. Not relevant to single-speaker PTT dictation. **Skip.**

### Summary table

| Model                             | Params | License                      | Stream?                      | LibriSpeech clean/other           | Sherpa-onnx graph?                                                                                         | Best fit                         |
| --------------------------------- | ------ | ---------------------------- | ---------------------------- | --------------------------------- | ---------------------------------------------------------------------------------------------------------- | -------------------------------- |
| whisper-base.en (current)         | 74M    | MIT                          | No (chunked)                 | ~4–6% / ~10–15%                   | n/a (ggml)                                                                                                 | Status quo macOS default         |
| parakeet-tdt-0.6b-v3              | 600M   | CC-BY-4.0                    | No native                    | **1.93% / 3.59%**                 | Yes (offline only)                                                                                         | Quality-first offline            |
| parakeet.cpp / TDT 600M           | 600M   | MIT runtime, model CC-BY-4.0 | EOU 120M streams             | same as above                     | n/a                                                                                                        | macOS Wave 6 Phase 2             |
| nemotron-speech-streaming-en-0.6b | 600M   | NVIDIA Open                  | **Native** 80/160/560/1120ms | **2.32% / 4.84%** @ 1120ms        | **Yes** ([HF](https://huggingface.co/csukuangfj/sherpa-onnx-nemotron-speech-streaming-en-0.6b-2026-01-14)) | Wave 6 Phase 1 streaming default |
| canary-qwen-2.5b                  | 2.5B   | NVIDIA Open                  | No                           | 5.63% leaderboard avg             | No                                                                                                         | Skip (too heavy)                 |
| whisper-large-v3-turbo            | 809M   | MIT                          | No (chunked)                 | ~2.7% on clean                    | n/a                                                                                                        | macOS quality stretch            |
| distil-large-v3.5                 | 756M   | MIT                          | No native                    | ~3% on clean, near v3             | n/a (ONNX exists)                                                                                          | Cross-platform quality bump      |
| WhisperKit (CoreML)               | varies | MIT                          | Yes (sub-100ms)              | 2.2% WER on bench                 | macOS only                                                                                                 | macOS premium path               |
| Moonshine-base streaming          | 245M   | MIT                          | **Yes**                      | 10.07% streaming / 6.65% Open ASR | n/a (own runtime)                                                                                          | Streaming + Windows portable     |
| Voxtral-Mini-3B-2507              | 3B     | Apache-2.0                   | Voxtral Realtime variant     | beats large-v3                    | No                                                                                                         | Stretch goal                     |

---

## Section 2 — LLM cleanup candidates

### Baseline: qwen2.5:7b and qwen2.5:1.5b

Tight prompt adherence is what we actually need. The "Moderate" style is format-only, no paraphrasing, no fragment completion — i.e., the model has to _resist_ the temptation to "improve" text. The same applies, more or less, to all the candidates below.

Qwen2.5-7B-Instruct historically underperformed its peers specifically on IFEval ([Qwen2.5 tech report](https://arxiv.org/pdf/2412.15115)). That's the exact axis we care about.

### Qwen3 family (April 29, 2025 release)

- Sizes: 0.6B / 1.7B / 4B / 8B / 14B / 32B + MoE 30B-A3B and 235B-A22B ([Qwen3 blog](https://qwenlm.github.io/blog/qwen3/)).
- **Apache 2.0** for the dense 0.6B–32B models.
- Ollama tags confirmed live: `qwen3:0.6b` (523MB), `qwen3:1.7b` (1.4GB), `qwen3:4b` (2.5GB), `qwen3:8b` (5.2GB), all also in `q4_K_M`, `q8_0`, `fp16` variants ([ollama tags](https://ollama.com/library/qwen3/tags)).
- **IFEval scores**:
  - Qwen3-8B: **85.0** strict-prompt IFEval (per technical report search).
  - Qwen3-4B-Instruct-2507: **83.4** IFEval ([artificialanalysis.ai](https://artificialanalysis.ai/models/qwen3-4b-2507-instruct)).
  - Qwen3-1.7B: documented to match Qwen2.5-3B base on average benchmarks ([Qwen3 blog](https://qwenlm.github.io/blog/qwen3/)).
- Hybrid Instruct/Thinking modes — for dictation cleanup you'd use Instruct/non-thinking and lock the temperature low; thinking mode is dangerous because it'll happily generate `<think>` paraphrasing of the transcript.
- Known caveat: open GH issue [QwenLM/Qwen3#1442](https://github.com/QwenLM/Qwen3/issues/1442) on Qwen3-32B hallucinating its own model name/cutoff. Not directly about dictation, but a signal that prompt-faithful behavior still requires temperature and system-prompt discipline.
- Tokens/sec: a quoted user benchmark of **Qwen3-8B Q4 on M2 Air 16GB via Ollama = 16.34 tok/s** ([codersera Qwen3-8B Mac guide](https://codersera.com/blog/run-qwen-3-8b-on-mac-an-installation-guide/)). The 4B and 1.7B should be roughly 1.5–3× faster.

**Score vs qwen2.5:7b:** Qwen3-8B is a clear upgrade — same Apache license, smaller-or-equal RAM, materially higher IFEval, identical Ollama runtime. **Recommended swap.**

**Score vs qwen2.5:1.5b:** Qwen3-1.7B is the direct successor, slightly larger but matches Qwen2.5-3B base. Apache. Strict drop-in. **Recommended swap.**

### Qwen3-Coder

- Code-specialist branch; for "code mode" dictation it can format code blocks faithfully, but it is not the right default for prose cleanup. **Skip as default; possibly an extension lane later.**

### Llama 3.3 (8B) and Llama 4 Scout

- Llama 3.3 officially only shipped 70B. The frequently-quoted "Llama 3.3 8B" referenced in [sitepoint best-local-LLM 2026](https://www.sitepoint.com/best-local-llm-models-2026/) appears to be community shorthand for Llama-3.1-8B inheriting 3.3 chat template — not an official Meta 3.3-8B drop.
- Llama 4 Scout: 17B active / 109B total MoE, 10M context ([Meta blog](https://ai.meta.com/blog/llama-4-multimodal-intelligence/)). Too heavy for our local-first envelope on a 16GB Mac.

**Score vs qwen2.5:7b:** No clean "small Llama 4" equivalent landed. Llama-3.1-8B is fine but not better than Qwen3-8B on IFEval-class tasks. **Skip.**

### IBM Granite 3.3 (2B / 8B)

- Granite 3.1 was the IFEval-focused release; Granite 3.3 continues it ([IBM Granite 3.1 announcement](https://www.ibm.com/new/announcements/ibm-granite-3-1-powerful-performance-long-context-and-more)).
- Ollama: `granite3.3:8b`, `granite3.3:2b` ([ollama page](https://ollama.com/library/granite3.3:8b)).
- Apache 2.0. Strong on structured instruction following (IBM's pitch is specifically RAG and instruction adherence).
- Specific IFEval numbers for 3.3 not surfaced in the search but the 3.1 release notes called out the IFEval improvement explicitly.

**Score vs qwen2.5:7b:** Plausible peer for prompt-faithful cleanup. Less community vetting than Qwen3 in voice/dictation tooling. **Stretch candidate**, not a default swap.

### Gemma 3 (1B / 4B / 12B / 27B)

- Announced March 12, 2025 ([Gemma 3 technical report](https://arxiv.org/pdf/2503.19786)).
- **Gemma 3-12B IFEval: 88.9** ([artificialanalysis.ai](https://artificialanalysis.ai/models/gemma-3-12b)). One of the highest open-weights IFEval scores at the 12B class.
- License: Gemma terms (not Apache — usable but with Google's acceptable-use policy attached; not CC-BY-NC, so OK for us).
- Multimodal (text + image), 128K context, supports 140+ languages.

**Score vs qwen2.5:7b:** Gemma 3-4B is a credible 1.5b-class replacement on capability per byte; Gemma 3-12B beats Qwen2.5-7B on IFEval but is bigger. Strong "stretch" pick for 7B-class swap if quality > footprint.

### Phi-4 / Phi-4-mini

- **Phi-4-mini**: 3.8B params, MIT, 128K context ([HF Phi-4-mini-instruct](https://huggingface.co/microsoft/Phi-4-mini-instruct/discussions/20)).
- MMLU 73% vs Llama 3.2 3B's 65%; MATH 62% vs 48% — strong reasoning per param.
- Ollama: `phi4-mini`, ~2.2GB Q4 ([ollama phi4-mini](https://ollama.com/library/phi4-mini)).
- Phi family is famously over-trained on synthetic data → can be over-confident in answer mode, and has historically been prompt-rigid (which is good for our use case).

**Score vs qwen2.5:1.5b:** Phi-4-mini is a fair Qwen3-1.7B alternative — smaller-ish, MIT, strong reasoning. **Stretch fallback**, not a default swap unless we run our own IFEval-on-dictation bench.

### Mistral Small 3 / Ministral 3

- Mistral Small 3 (24B): "knowledge-dense", runs on a single 32GB MacBook ([cometapi mistral 3 guide](https://www.cometapi.com/how-to-run-mistral-3-locally/)). Too big for our 7B-class default.
- Ministral 3 line: 3B / 8B / 14B, edge-tuned, 256K context. Ollama: `ministral-3:8b` ([ollama ministral-3](https://ollama.com/library/ministral-3)).
- License: Mistral Research / Apache mix depending on the SKU — check per-tag.

**Score vs qwen2.5:7b:** Ministral-3-8B is a credible peer; benchmarks aren't as well-circulated as Qwen3-8B's. **Stretch**, not default.

### DeepSeek R1 distills

- 1.5B / 7B / 8B / 14B / 32B / 70B distill checkpoints based on Qwen2.5 and Llama3 ([Fireworks DeepSeek guide](https://fireworks.ai/blog/deepseek-models)).
- These are _reasoning_ distills — they emit `<think>` traces. **Wrong shape for dictation cleanup**: the whole point of our prompt is to _not_ paraphrase.
- DeepSeek-R1-0528-Qwen3-8B is the most recent 8B distill, again reasoning-oriented.

**Score:** Skip — reasoning distills will explode our hallucination axis exactly like Assertive style did.

### Hermes 4

- Sizes: 14B, 70B, 405B — **no 8B variant** ([MarkTechPost Hermes 4 launch](https://www.marktechpost.com/2025/08/27/nous-research-team-releases-hermes-4-a-family-of-open-weight-ai-models-with-hybrid-reasoning/)).
- "Hybrid reasoning" with togglable `<think>` blocks.
- 14B smallest is too heavy for our 1.5B-class slot; the 14B for 7B-class is plausible but again the reasoning-mode default biases toward elaboration.

**Score:** Skip.

### Summary table

| Model                      | Params  | License           | Ollama tag           | IFEval                 | Best fit                  |
| -------------------------- | ------- | ----------------- | -------------------- | ---------------------- | ------------------------- |
| qwen2.5:7b (current)       | 7B      | Apache-2.0        | `qwen2.5:7b`         | underperforms peers    | baseline                  |
| qwen2.5:1.5b (current)     | 1.5B    | Apache-2.0        | `qwen2.5:1.5b`       | weak                   | baseline fallback         |
| **qwen3:8b**               | 8B      | Apache-2.0        | `qwen3:8b` (5.2GB)   | **85.0**               | **7B-class swap**         |
| **qwen3:4b-instruct-2507** | 4B      | Apache-2.0        | `qwen3:4b`           | **83.4**               | mid-tier sweet spot       |
| **qwen3:1.7b**             | 1.7B    | Apache-2.0        | `qwen3:1.7b` (1.4GB) | matches Qwen2.5-3B     | **1.5B-class swap**       |
| gemma3:12b                 | 12B     | Gemma terms       | `gemma3:12b`         | **88.9**               | quality stretch           |
| gemma3:4b                  | 4B      | Gemma terms       | `gemma3:4b`          | high                   | 1.5B/4B stretch           |
| granite3.3:8b              | 8B      | Apache-2.0        | `ibm/granite3.3:8b`  | strong, exact N/F      | structured-prompt stretch |
| phi4-mini                  | 3.8B    | MIT               | `phi4-mini`          | N/F (strong reasoning) | 1.5B-class stretch        |
| ministral-3:8b             | 8B      | Mistral mix       | `ministral-3:8b`     | N/F                    | stretch                   |
| DeepSeek R1 distills       | 1.5–70B | open              | various              | reasoning bias         | **skip**                  |
| Hermes 4 (14B+)            | 14B+    | Llama 3.1 license | n/a in our class     | reasoning bias         | **skip**                  |

---

## Section 3 — Recommendations

### STT default (macOS): currently `whisper-base.en`

- **Keep what we have**: only if we can't fund a one-week bench. The "Mementis"/"Momentus" class of error is a real product blocker, and `base.en` is the cheapest model that will hit it.
- **Recommended swap**: nothing yet on the macOS _default_ axis — finish Wave 6 Phase 2 (parakeet.cpp) and bench it head-to-head before switching the default. The single most credible alternative is `whisper-large-v3-turbo` via whisper.cpp+CoreML (it beat Parakeet-MLX in the one cross-tool Mac speedtest we found, and large-v3-turbo's WER is ~2-3% on clean — 2–4 points better than base). Risk: ~5× larger download (~800MB GGUF), more RAM.
- **Stretch swap**: parakeet.cpp with the 600M TDT model on Metal, _if_ the Wave 6 Phase 2 bench confirms the 2×+ win the project claims. Migration is bigger because it's a new C++ engine, not a ggml swap.
- **Do nothing yet** verdict on this axis is honestly defensible — Wave 6 Phase 2 was explicitly written to gate this swap on bench numbers we don't have. Don't preempt it.

### STT default (Windows): currently `whisper-base.en`

- **Keep what we have**: defensible. Whisper.cpp is the most cross-platform of the candidates.
- **Recommended swap**: **Moonshine-base streaming** (245M, MIT, prebuilt for Windows, 6.65% Open ASR avg, ~3× faster than whisper-tiny). It's the only candidate that beats `base.en` on quality _and_ runs natively on Windows without a CUDA assumption.
- **Stretch swap**: distil-whisper-large-v3.5 ONNX (~756M, better WER than turbo on short-form, ONNX export exists). Larger download, but Windows users are typically less RAM-constrained than M-series users.

### STT streaming engine: currently sherpa-onnx + Parakeet-TDT-0.6B-v3 (no native streaming → simulated)

- **Keep what we have**: bad option. Parakeet v3 in sherpa-onnx is officially not designed for streaming ([sherpa-onnx#2918](https://github.com/k2-fsa/sherpa-onnx/issues/2918)), and our LocalAgreement2 layer is papering over a model that wasn't built for the chunk shape we want.
- **Recommended swap**: **NVIDIA Nemotron-Speech-Streaming-en-0.6b** via sherpa-onnx. This is literally what Wave 6 Phase 1 is targeting — and the sherpa-onnx graphs (`sherpa-onnx-nemotron-speech-streaming-en-0.6b-int8-2026-01-14` and `560ms-int8-2026-04-25`) are already published. Configurable chunk sizes 80/160/560/1120 ms match the architecture exactly. WER 2.32% clean / 4.84% other at 1120 ms is _better_ than `whisper-base.en` and natively streaming.
- **Stretch swap**: Moonshine v2 streaming (also native streaming, MIT, ~245M). Lower WER ceiling than Nemotron but smaller download and permissive license.

### LLM 7B-class default: currently `qwen2.5:7b`

- **Keep what we have**: tolerable but eats the same Qwen2.5 IFEval weakness that contributed to "Assertive style" hallucinations. Don't keep.
- **Recommended swap**: **`qwen3:8b`** (Apache-2.0, 5.2GB Q4 on Ollama, **IFEval 85.0**, drop-in chat template). Same prompt format, same memory footprint, materially better instruction adherence. Lock the temperature low and use Instruct (non-thinking) mode. ~10–15 tok/s on M2 Air 16GB based on the cited benchmark.
- **Stretch swap**: `qwen3:4b` (`qwen3:4b-instruct-2507`, Apache, IFEval 83.4). Smaller, faster, only ~1.6 IFEval points behind 8B. If our cleanup task is genuinely "fix disfluencies + format" and not "reason about anything," 4B is plausibly enough. The biggest risk is content tasks where 4B will be less robust to ambiguous transcripts.

### LLM 1.5B-class fallback: currently `qwen2.5:1.5b`

- **Keep what we have**: weak but functional. Auto-upgrade-on-Assertive is gone, so this is the genuine low-resource path.
- **Recommended swap**: **`qwen3:1.7b`** (Apache, 1.4GB Q4, matches Qwen2.5-3B-base in capability). Same family, same prompt template, slight memory bump, real quality jump. Strictly better.
- **Stretch swap**: `phi4-mini` (3.8B, MIT, ~2.2GB Q4). It's bigger than the slot, but if "low-resource" means "no GPU" rather than "tiny RAM," Phi-4-mini's instruction following is a known strong axis.

---

## Section 4 — Migration cost notes

| Swap                                                                   | sherpa-onnx graph?                                                                                                    | Ollama tag?                | License risk                                                                                                                                      | Approx download  |
| ---------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------- | -------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------- |
| **STT streaming**: Parakeet v3 → Nemotron-streaming-0.6b               | **Yes** — `sherpa-onnx-nemotron-speech-streaming-en-0.6b-int8-2026-01-14`, plus `560ms-int8-2026-04-25`               | n/a                        | **NVIDIA Open Model License**, not Apache/MIT — commercial OK, but it is a custom license; verify against the project's OSS policy before pinning | ~150-300 MB int8 |
| **STT macOS stretch**: whisper-base.en → whisper-large-v3-turbo (ggml) | n/a (whisper.cpp engine)                                                                                              | n/a                        | MIT                                                                                                                                               | ~800 MB GGUF Q5  |
| **STT macOS stretch**: parakeet.cpp 600M TDT                           | n/a — separate engine                                                                                                 | n/a                        | **MIT runtime, model is CC-BY-4.0** (attribution required, no NC)                                                                                 | ~1.2 GB          |
| **STT Windows**: whisper-base.en → Moonshine-base streaming            | No — Moonshine ships its own ONNX/optimized binaries ([moonshine-ai repo](https://github.com/moonshine-ai/moonshine)) | n/a                        | MIT                                                                                                                                               | ~250 MB          |
| **STT cross**: distil-large-v3.5-ONNX                                  | n/a (ONNX runtime direct)                                                                                             | n/a                        | MIT                                                                                                                                               | ~750 MB          |
| **LLM 7B-class**: qwen2.5:7b → qwen3:8b                                | n/a                                                                                                                   | **Yes** `qwen3:8b` (5.2GB) | **Apache-2.0** ✅                                                                                                                                 | 5.2 GB Q4        |
| **LLM 7B-class stretch**: qwen3:4b                                     | n/a                                                                                                                   | Yes `qwen3:4b` (2.5GB)     | Apache-2.0 ✅                                                                                                                                     | 2.5 GB Q4        |
| **LLM 1.5B-class**: qwen2.5:1.5b → qwen3:1.7b                          | n/a                                                                                                                   | Yes `qwen3:1.7b` (1.4GB)   | Apache-2.0 ✅                                                                                                                                     | 1.4 GB Q4        |
| **LLM stretch**: gemma3:12b                                            | n/a                                                                                                                   | Yes `gemma3:12b`           | **Gemma terms** (not Apache, but OSS-compatible)                                                                                                  | ~8 GB Q4         |
| **LLM stretch**: phi4-mini                                             | n/a                                                                                                                   | Yes `phi4-mini`            | MIT ✅                                                                                                                                            | ~2.2 GB Q4       |
| **LLM stretch**: ibm/granite3.3:8b                                     | n/a                                                                                                                   | Yes `ibm/granite3.3:8b`    | Apache-2.0 ✅                                                                                                                                     | ~5 GB Q4         |

**License watch-outs for an OSS app:**

- **Parakeet-TDT-0.6B-v3 model weights are CC-BY-4.0.** Attribution required in the app's about/credits screen. Not a blocker.
- **Nemotron uses NVIDIA Open Model License.** Commercial use allowed, but it is _not_ a standard OSS license. If boothrflow is being positioned as a fully OSS project, vendor-distributing the Nemotron weights is fine but worth a sentence in NOTICE.
- **Gemma terms** include an acceptable-use policy. Compatible with OSS but adds a clause beyond Apache-2.0.
- **None of the recommended swaps are CC-BY-NC.** No NC blockers.

---

## Short verdict

1. **Ship Wave 6 Phase 1 — Nemotron streaming via sherpa-onnx — as soon as bench passes.** This is the single highest-value swap on the board: graphs already exist, the WER and latency story is clean, it directly fixes the simulated-streaming hack on Parakeet v3.
2. **Swap LLM defaults to Qwen3 (`qwen3:8b` and `qwen3:1.7b`) on the next LLM-touching wave.** Same chat template, materially better IFEval, same Apache license, immediate. The Qwen2.5 IFEval gap is the same axis that drove the Assertive-style hallucination problem.
3. **Hold the macOS STT default at `whisper-base.en` until Wave 6 Phase 2 (parakeet.cpp) lands its bench.** The cross-tool Mac speedtest evidence suggests parakeet.cpp's quality-per-millisecond claim is not yet a slam-dunk vs whisper-turbo on CoreML; don't preempt the bench.
4. **Add Moonshine-base streaming as the Windows STT candidate** in the next research-spike issue. It's the only "better than `base.en`, native Windows, streaming, MIT" candidate that survived the survey.
5. **Skip Canary-Qwen, Hermes 4, DeepSeek R1 distills** — wrong shape (LM-augmented ASR / reasoning-traced LLMs) for short PTT dictation cleanup.
