# S2B2S Android — Full On-Device Port: Architecture & Implementation Plan

**Goal:** Port the *complete* S2B2S experience — **STT → Brain → TTS, running locally on the phone** — not the thin-client companion. Offline-first, private, voice-native, on Android.

**Scope of this document:** the recommended stack and backend, why, the licensing landmines, a phased feature roadmap, the model strategy, the hardware realities, and a full survey of every referenced project (plus the ones the survey turned up). **Analysis and plan only — no code.**

**Sources reviewed:** `S2B2S_ANDROID_COMPANION.md` (the existing brainstorm), the S2B2S desktop tree (`gemma_4_qat_mtp_e2b/`, `control_server.rs`), `reference_links.md` (70+ projects across 16 categories), the cloned `k2-fsa/sherpa-onnx` Android suite, and live project pages for every GitHub link referenced (Kokoro-82M-Android, woheller69/ttsEngine, VoxSherpa-TTS, NekoSpeak, Maise, soniqo/speech-android, ToolNeuron, android_transcribe_app, local-whisper, Llamatik, llmedge, LiteRT-LM, google-ai-edge/gallery, mllm, ExecuTorch, MNN, plus earlier: pocket-tts-unity, NekoSpeak, soniqo, react-native-sherpa-onnx, BreezeApp, MediaPipe, llama.cpp Android).

---

## 0. The One-Paragraph Answer

Your instinct is right and it is the simplest viable stack: **lean on sherpa-onnx as the single native engine for STT *and* TTS *and* VAD *and* wake-word, with Kokoro + Piper as the TTS voices — exactly as SherpaTTS, VoxSherpa, NekoSpeak, Maise, local-whisper, and ToolNeuron already do in shipping apps.** Add **llama.cpp** (via **Llamatik** or direct JNI) for the Brain — the same Gemma 4 E2B GGUF family the desktop already runs, so models and behavior carry over; keep **LiteRT-LM** (Google's recommended successor to MediaPipe LLM Inference) as the optimized Android fast-path with native Gemma-4 MTP+QAT support. Build the shell in **native Kotlin + Jetpack Compose** for a full port (React Native is a credible second choice because of your React frontend; PWA and Tauri-mobile are not suitable for on-device inference). Steal the orchestration design from **soniqo/speech-core** (a clean STT→LLM→TTS pipeline with turn detection and barge-in). The one thing that will bite you is **licensing**: Piper/Kokoro phonemize through **eSpeak NG, which is GPLv3** — that is precisely why SherpaTTS and VoxSherpa are GPLv3 while S2B2S is MIT. Decide the G2P/license path on day one (NekoSpeak's answer: use the MIT **Misaki** G2P for English; Maise's answer: use **OpenPhonemizer**, also MIT-compatible, with broader language coverage — evaluate both).

---

## 1. Why sherpa-onnx Is the Backbone (and Why It's the *Easy* Path)

The temptation in a port is to integrate Kokoro, Piper, Whisper, and a VAD separately. Don't. **sherpa-onnx collapses all of speech into one dependency** and is purpose-built for Android:

- **One Gradle line.** It's published on JitPack: `implementation("com.github.k2-fsa:sherpa-onnx:v1.13.3")` — a prebuilt AAR with native `.so` for `arm64-v8a`/`armeabi-v7a`/`x86_64`. No NDK compile required to start.
- **It covers the whole speech layer:**
  - **TTS** — Kokoro-82M, Piper/VITS, Matcha, Kitten (the exact engines S2B2S uses on desktop).
  - **STT** — streaming Zipformer transducer, Parakeet, **Nemotron 3.5 streaming (80 ms)**, SenseVoice, Whisper, Moonshine (streaming *and* offline).
  - **VAD** — Silero.
  - **KWS** — keyword spotting (your "Hey S2B2S" wake word).
  - Bonus: speaker diarization, language ID, audio tagging, denoising — all on-device.
- **Official Android demo apps for literally every piece** ship in the repo (`android/`): `SherpaOnnxTts`, **`SherpaOnnxTtsEngine` (registers as the system TTS engine)**, `SherpaOnnxSimulateStreamingAsr`, `SherpaOnnxVadAsr`, `SherpaOnnx2Pass`, `SherpaOnnxKws`, `SherpaOnnxWebSocket`, plus **Wear OS** variants. These are starting templates, not just samples.
- **Proven in production across at least 7 shipping Android apps** (see §7): NekoSpeak (MIT), SherpaTTS (GPLv3, F-Droid), VoxSherpa (GPLv3, Play Store), **Maise** (dual TTS+ASR), **soniqo/speech-android** (SDK), **local-whisper** (multi-platform STT), and **ToolNeuron** (full AI suite with sherpa-onnx voice loop) — all are sherpa-onnx + Kokoro/Piper on Android.
- **It matches the desktop.** The desktop S2B2S already converged its STT onto sherpa-onnx (Nemotron, the exported Parakeet-Unified streaming model) and uses Kokoro/Piper for TTS. Reusing the same engine on mobile means shared model assets, shared quirks, and shared mental model. Note: the desktop's primary STT library is actually **[transcribe-rs](https://github.com/cjpais/transcribe-rs)** (Parakeet V3, Whisper, Moonshine, etc.), which also has an Android path via **[notune/android_transcribe_app](https://github.com/notune/android_transcribe_app)** — a production Android app running transcribe-rs + Parakeet TDT on-device in Rust. This creates a secondary continuity path: transcribe-rs models on both desktop and Android.

**Net:** sherpa-onnx is the difference between "integrate five C++ libraries and their build systems" and "add one AAR and call a Kotlin API." That is the whole reason it's the simplest solution.

---

## 2. The Brain (LLM) — llama.cpp First, LiteRT-LM as the Easy Alternative

sherpa-onnx does **not** do LLMs, so the Brain is a separate backend. The desktop already answers this: it runs **`gemma-4-E2B-it-qat-UD-Q4_K_XL.gguf` (2.44 GB)** + a 56 MB MTP draft model + a 940 MB vision projector, via **llama.cpp**. The on-device Android options, in order of fit:

| Framework | Model format | DX on Android | Streaming | Notes |
|---|---|---|---|---|
| **llama.cpp** (recommended) | **GGUF** (same as desktop) | Llamatik (KMP) / llmedge / JNI | Callback tokens | MIT. Broadest model + hardware support, grammar-constrained output & function calling. **Format continuity with desktop** — same Gemma 4 E2B Q4 files. |
| **LiteRT-LM** (google-ai-edge) | `.task` (FlatBuffers) | Native Kotlin/Java API, GPU accel | Yes | Google's recommended successor to MediaPipe LLM Inference. Supports **Gemma 4 E2B/E4B with MTP**, LoRA, image/audio input, agent skills, OpenAI-compatible server. Different model format from desktop. |
| **MLC LLM** | MLC-compiled | TVM runtime (Vulkan/Metal) | Yes | Fast native GPU; more build complexity. |
| **ExecuTorch** | PyTorch `.pte` | Kotlin | Yes | Meta-scale tooling; QAT export via torchao, Qualcomm QNN delegation, Android LlamaDemo. |
| **Cactus** | GGUF-ish | Cross-platform SDK | SSE tokens | Sub-120 ms TTFT, optional cloud fallback (breaks "pure offline" unless disabled). |

**Recommendation:** **llama.cpp** for v1 — it gives *model-format continuity* with the desktop (ship/download the same Gemma 4 E2B QAT GGUF), token streaming via a callback, and the largest community. Evaluate **[Llamatik](https://github.com/ferranpons/Llamatik)** (MIT, Kotlin Multiplatform) as the concrete Android binding — it bundles llama.cpp + whisper.cpp + stable-diffusion.cpp behind a single Kotlin API with MTP speculative drafting, concurrent sessions, and Maven Central distribution, potentially eliminating custom JNI work. Keep a **LiteRT-LM** adapter behind the same interface as a fast-path for the smoothest Android integration, GPU acceleration, and native Gemma-4 MTP+QAT support on supported devices. Study **[google-ai-edge/gallery](https://github.com/google-ai-edge/gallery)** (the canonical reference app for LiteRT-LM) as a blueprint — it already does on-device transcription, multimodal, and benchmarking.

**Mobile-specific Brain tuning:**
- **Evaluate MTP on-device** — desktop MTP (n=13, ~216 tok/s) is tuned for CUDA; on phones, MTP may still help (LiteRT-LM ships Gemma-4 MTP natively, and Llamatik supports it for Qwen3.5/GLM-4). Benchmark rather than pre-emptively dropping it.
- **Make the 940 MB vision projector optional** (Phase 4 multimodal), not default.
- Default to **Gemma 4 E2B-it QAT Q4_K_M** (~1.3–2.5 GB) to match desktop; offer **Qwen 3 1.5B** as a lighter alternative for mid-range devices.
- Cap context (e.g. 2–4 k) and conversation memory turns; run decode on a background thread and push tokens to the UI.

**The shared-model story (a real selling point):** *the same Gemma E2B family runs on both desktop and phone* — desktop via llama.cpp+CUDA, phone via llama.cpp+CPU/GPU (or LiteRT-LM as the optimized Android path). The **[Llamatik](https://github.com/ferranpons/Llamatik)** KMP library would make this story even cleaner by providing one Kotlin API that wraps llama.cpp on Android, Desktop, iOS, and WASM. One persona, one prompt, one model family across devices.

---

## 3. The Shell & Orchestration — Native Kotlin (with React Native as a Fallback)

The note's Options A/B/C were written for a *thin client*. For a **full on-device port**, re-evaluate:

| Option | Verdict for a full port | Why |
|---|---|---|
| **A. Native Kotlin + Jetpack Compose** | ✅ **Recommended** | Cleanest sherpa-onnx AAR + llama.cpp JNI integration; first-class `AudioRecord`/`AudioTrack`, foreground service, **system-TTS-engine** registration, **IME/voice-input** registration, Wear OS, Play Store. Every reference app (NekoSpeak, SherpaTTS, sherpa demos) is native Kotlin. |
| **B. React Native + `react-native-sherpa-onnx`** | ⚠️ Credible #2 | Reuses your React/TS skills and some desktop UI concepts. `XDcobra/react-native-sherpa-onnx` is a TurboModule for sherpa-onnx STT/TTS/VAD/diarization (Android+iOS). For the Brain: pair with **[react-native-litert-lm](https://github.com/hung-yueh/react-native-litert-lm)** (Gemma-4 + MTP + multimodal, demoed on Galaxy S22 4 GB) or **[react-native-executorch](https://github.com/software-mansion/react-native-executorch)** (Llama 3.x / Qwen 3 / Phi-4-mini). Some friction registering as a *system* TTS/IME service from RN. |
| **C. Tauri 2 Mobile (Rust core reuse)** | ❌ Not yet | Shares the desktop Rust core, but mobile Tauri is still maturing and the audio + JNI plugin story for sherpa + llama is immature/high-risk. Revisit later. |
| **D. PWA** | ❌ Only for the companion | No on-device ONNX/LLM inference, constrained mic/background audio. Fine for a remote thin client, not a full port. |

**Orchestration design — copy `soniqo/speech-core`.** It is, almost exactly, "S2B2S's conversation pipeline as a reusable engine": a `VoicePipeline` (STT→LLM→TTS), a `TurnDetector` (VAD-driven turn boundaries), a `SpeechQueue` (priority queue with cancel/resume → **barge-in**), a `StreamingVAD` (hysteresis state machine), an `AudioBuffer` (ring buffer + resampler), and abstract `STT/TTS/LLM/VAD` interfaces with the ML left to the consumer. Reimplement this state machine in Kotlin (or wrap the C++), and carry over the desktop's proven concepts: **TripleVAD** (RMS→RNNoise→Silero), **sentence/fragment streaming** for low time-to-first-audio, and the **ITN/TN/markdown-strip** normalization pipeline.

**Audio plumbing:** `AudioRecord` (16 kHz mono PCM) → ring buffer → VAD → streaming STT; LLM tokens → sentence streamer → sherpa TTS → `AudioTrack`. Handle **audio focus**, **Bluetooth SCO headset** routing, and a **foreground service** for hands-free/background use.

---

## 4. ⚠️ Licensing — Decide This First (It Shapes Distribution)

This is the single most important non-code decision, and the existing note doesn't address it.

| Component | License | Implication |
|---|---|---|
| **sherpa-onnx** | Apache-2.0 | ✅ Fine to bundle in an MIT app. |
| **Kokoro-82M weights** | Apache-2.0 | ✅ Redistributable. |
| **Piper voices** | per-voice (often MIT/CC-BY) | ✅ Mostly fine; check each voice. |
| **llama.cpp** | MIT | ✅ Fine. |
| **Gemma weights** | Gemma Terms | ⚠️ Permissive but has use terms; decide bundle-vs-download. |
| **eSpeak NG (G2P / phonemizer)** | **GPLv3** | 🚨 **The trap.** Piper/Kokoro use eSpeak NG for grapheme→phoneme. Bundling it makes the whole app GPLv3 — which is exactly why **SherpaTTS and VoxSherpa are GPLv3**, while S2B2S is MIT. |

**Options for the eSpeak/GPLv3 problem:**
- **(A) Misaki G2P (MIT) for English + eSpeak fallback for other languages** — this is **NekoSpeak's** answer, and NekoSpeak is **MIT**. Best precedent for keeping the core MIT. English uses Misaki; non-English may still pull eSpeak (handle as an optional/clearly-licensed module).
- **(B) Ship two builds** — an MIT core (no eSpeak; Misaki-only languages) and a GPLv3 "full languages" build (with eSpeak), like the F-Droid TTS-engine apps.
- **(C) Accept GPLv3** for the Android app if broad multilingual TTS out-of-the-box matters more than license purity.
- **(D) [OpenPhonemizer](https://github.com/NeuralVox/OpenPhonemizer)** — used by **[Maise](https://github.com/Mobile-Artificial-Intelligence/maise)** in production. An MIT-compatible G2P that covers more languages than Misaki (English-only). Evaluate as a potential single G2P that avoids the eSpeak trap entirely without sacrificing multilingual support.

**Action:** pick (A) or (D) — test OpenPhonemizer's language coverage against the target voice set; if it covers the needed languages, it's the cleanest single-license solution. Document the choice in the repo before writing TTS code.

---

## 5. Phased Feature Roadmap (Re-prioritized for a Full Port)

The existing note buries on-device inference in "Phase 2/Later." Flip it: **on-device is the product.** The remote companion becomes an *optional enhancement* at the end.

### Phase 0 — Foundation spike (prove the stack on a real device)
- New native module: Kotlin + Jetpack Compose; add the sherpa-onnx AAR.
- Run the official sherpa demos on target hardware: `SherpaOnnxTts`, `SherpaOnnxSimulateStreamingAsr`, `SherpaOnnxVadAsr`. Confirm Kokoro + Piper synth and streaming ASR work and measure latency/RTF.
- Add llama.cpp: evaluate **[Llamatik](https://github.com/ferranpons/Llamatik)** (Kotlin Multiplatform, one-API STT+LLM) or **[llmedge](https://github.com/Aatricks/llmedge)** (GGUF download + on-device quantization); load Gemma 4 E2B Q4 GGUF; measure **tok/s, TTFT, RAM, temperature**. Also benchmark a **LiteRT-LM** `.task` run (study `google-ai-edge/gallery`). Use **PolyEngineInfer** for side-by-side comparison.
- **Decide G2P/license path (§4).** Exit criteria: end-to-end "speak → text" and "text → speech" and "prompt → tokens" each working in isolation on-device.

### Phase 1 — On-device Dictation (STT only) → the mobile "Dictate Anywhere"
- `AudioRecord` (16 kHz mono) → sherpa **streaming ASR** (Nemotron 80 ms / streaming Zipformer / Qwen3-ASR) → **live partials** in the UI → commit text to a field/clipboard.
- Silero VAD for endpointing; partial-stabilization (grey unstable tail, commit stable prefix).
- Optional: register as an **Android IME (voice keyboard)** or **system Voice-Input service** so any app gets local dictation. Study **FUTO Keyboard** and **Transcribro** for IME registration approaches; study **Maise** for dual `RecognitionService` + `TextToSpeechService` registration.

### Phase 2 — On-device Read-Aloud (TTS only) → the mobile "Read Aloud"
- Text → sherpa **TTS** (Kokoro/Piper) → `AudioTrack`, with the desktop's **sentence/fragment streaming** for sub-second first audio.
- **Register as the Android system TTS engine** (`TextToSpeechService`) so Chrome, WhatsApp, TalkBack, e-readers use your voice — start from `SherpaOnnxTtsEngine` / study NekoSpeak & SherpaTTS.
- Voice/model manager: download Kokoro & Piper voices from Hugging Face on demand.

### Phase 3 — Full Conversation Loop (STT → Brain → TTS), all on-device
- Wire the soniqo-style `VoicePipeline`: streaming STT → endpoint (VAD/EOU) → **llama.cpp Gemma streaming** → sentence-stream into TTS → `AudioTrack`.
- **Barge-in** (SpeechQueue cancel/resume + VAD-driven interruption), conversation memory, persona/system prompt.
- Reuse desktop **normalization** (ITN/TN/markdown strip) — port the rule set or run a lightweight equivalent (note: desktop's `text-processing-rs` is Rust).
- **Foreground service** for hands-free; audio-focus + Bluetooth headset-button handling.

### Phase 4 — Mobile-native polish & extras
- **Wake word** via sherpa **KWS** ("Hey S2B2S").
- **Multimodal** (optional): image input to Gemma via mmproj; camera/screenshot Q&A.
- Home-screen **widget**, **Quick Settings tile**, **Wear OS** companion (sherpa has Wear OS demos), audiobook maker (PDF/text → MP3), notification reader, Car Mode.
- **Optional remote/hybrid mode:** on the home LAN, offload the Brain (or STT/TTS) to the desktop S2B2S. ⚠️ **Reality check:** the desktop `control_server.rs` is today a *hand-rolled single-threaded TCP/HTTP loop with no WebSocket* — the note's `axum` + `/v1/mobile/*` routes do **not** exist yet. A real remote mode means building the WebSocket server on the desktop first (mDNS discovery + the Vox-style protocol from the note). Treat this as a *bonus*, not the foundation.

---

## 6. Model Strategy (On-Device, Mobile-Tuned)

| Layer | Default | Lighter / fallback | Quality / extra | Engine |
|---|---|---|---|---|---|
| **STT** | **Nemotron 3.5 streaming INT8** (80 ms, 40 langs, downloadable) | Streaming Zipformer (small) | Parakeet Unified (EN quality); **Qwen3-ASR 0.6B** (multilingual, validated on Android by `local-whisper`); SenseVoice / Whisper-base (offline) | sherpa-onnx |
| **TTS** | **Piper** (tiny, fast — great default for low-end) | — | **Kokoro-82M int8** (quality, ~80–160 MB); Kitten/Pocket (voice cloning) | sherpa-onnx |
| **Brain** | **Gemma 4 E2B-it QAT Q4_K_M** (~1.3–2.5 GB; matches desktop) | **Qwen 3 1.5B** | Gemma E4B (if RAM allows); +mmproj for vision | llama.cpp (or LiteRT-LM) |
| **VAD** | Silero (sherpa) | RMS gate | TripleVAD concept from desktop | sherpa-onnx |
| **Wake word** | sherpa KWS | — | — | sherpa-onnx |

- **Quantization:** INT8 ONNX for speech (XNNPACK); Q4_K_M / Q4_0 GGUF for the LLM.
- **Tiers:** offer **STT-only**, **TTS-only**, and **full-loop** modes so 4–6 GB devices can still use the dictation/read-aloud halves even if the full conversation loop needs 8 GB+.

---

## 7. Reference Projects — Full Survey (every link in the note + discoveries)

### On-device TTS (the directly reusable ones)
| Project | Stack | License | Role for the port |
|---|---|---|---|
| **siva-sub/NekoSpeak** | Kotlin, sherpa-onnx, **Misaki G2P** | **MIT** ✅ | **The single best TTS reference.** Already runs **Kokoro + Kitten + Pocket + Piper** on-device, registers as system TTS, MIT-licensed, solves the G2P/license problem with Misaki+eSpeak-fallback. Study closely; closest to S2B2S's exact engine set. |
| **woheller69/ttsEngine (SherpaTTS)** | Kotlin, sherpa-onnx, Piper/Coqui | GPLv3 | Shipping F-Droid **system TTS engine**; built-in HF model downloader; clean `TextToSpeechService` integration. Great architecture ref; mind the GPLv3. |
| **CodeBySonu95/VoxSherpa-TTS** | Kotlin, sherpa-onnx, Kokoro+Piper+VITS, 50+ langs | GPLv3 | Listed in sherpa's official README; exposes all models to System TTS; APK via HF. Multilingual reference; GPLv3. |
| **Mobile-Artificial-Intelligence/maise** | Kotlin, ONNX Runtime, OpenPhonemizer, Kokoro | — | **System TTS + ASR in one app.** Registers both `TextToSpeechService` AND `RecognitionService`. Uses OpenPhonemizer (MIT-compatible G2P) instead of eSpeak. From the MAID team. Direct reference for dual system-service registration (Phase 1+2). |
| **soniqo/speech-android** | Kotlin SDK + JNI, ONNX Runtime, speech-core | (org) | **Packaged full-speech SDK**: Parakeet STT (114 langs), Kokoro TTS (8 langs), Silero VAD, DeepFilterNet3 noise cancellation. Clean engine abstractions. Study as a production SDK reference. |
| **puff-dayo/Kokoro-82M-Android** | Kotlin, ONNX Runtime (int8) | (archived) | Proof that Kokoro-82M int8 runs natively on Android; minimal/archived. Superseded by NekoSpeak. Same author's **Matcha-Chat** = local-LLM chat ref. |
| **lookbe/pocket-tts-unity** | Unity C#, ONNX | — | Shows Pocket TTS runs on mobile via ORT. Superseded by NekoSpeak's *native* Pocket support; relevant only if you ever go Unity. |

### On-device STT & Voice Input (Android IME/RecognitionService references)
| Project | Stack | License | Role |
|---|---|---|---|
| **notune/android_transcribe_app** | Rust, transcribe-rs, Parakeet TDT | — | **transcribe-rs on Android** — the STT library S2B2S desktop already uses. 25 languages, nothing leaves the phone. Direct STT model continuity with desktop (like llama.cpp for the Brain). Study for Phase 1 dictation. |
| **gabrimatic/local-whisper** | sherpa-onnx, Parakeet-TDT v3 + Qwen3-ASR 0.6B | — | Multi-platform STT using sherpa-onnx on Android. Validates Qwen3-ASR as a viable Android STT model. Clean architecture reference. |
| **futo-org/android-keyboard** | Whisper-based, offline | FUTO Source First | Production offline voice keyboard with IME registration. Study their `RECOGNIZE_SPEECH` intent and IME voice subtype approach. |
| **soupslurpr/Transcribro** | whisper.cpp + Silero VAD | — | Simpler reference for Phase 1 dictation — whisper.cpp Android voice keyboard with system-wide STT provider. Less complex than full sherpa-onnx pipeline. |

### Full On-Device AI Suites (closest existing S2B2S-on-Android)
| Project | Stack | License | Role |
|---|---|---|---|
| **Siddhesh2377/ToolNeuron** | llama.cpp (custom fork), sherpa-onnx voice loop, RAG, VLM | — | **The closest existing implementation to the plan's vision.** GGUF chat + vision + RAG + sherpa-onnx voice loop with sentence-chunked TTS + plugin runtime. Offline, no Google services. Install and study end-to-end. |
| **jegly/Box** | Offline AI suite | — | Separate builds for stock Android vs GrapheneOS/LineageOS/CalyxOS. Distribution/packaging reference for privacy-first app. |
| **Open-LLM-VTuber/Open-LLM-VTuber** | Live2D avatar + voice loop | — | Hands-free voice interaction + voice interruption + Live2D talking face. Reference for the "animated avatar + local voice loop" ambition. |
| **google-ai-edge/gallery** | LiteRT-LM, Gemma 4, on-device transcription | — | **Canonical LiteRT-LM reference app.** Already does on-device transcription ("Audio Scribe"), multimodal, thinking mode, and benchmarking. Blueprint for LiteRT-LM integration. |
| **alibaba/MNN TaoAvatar** | MNN engine, LLM+ASR+TTS+A2BS+NNR | — | **Full offline 3D-avatar conversation on Android.** Directly relevant to S2B2S's 3D avatar overlay ambitions.

### STT / voice-assistant / pipeline (architecture refs)
| Project | Stack | License | Role |
|---|---|---|---|
| **soniqo/speech-core** | C++ pipeline engine | (org) | **Best orchestration blueprint:** VoicePipeline (STT→LLM→TTS), TurnDetector, SpeechQueue (cancel/resume = barge-in), StreamingVAD, abstract STT/TTS/LLM/VAD interfaces. Reimplement this in Kotlin. |
| **soniqo/speech-android** | ONNX Runtime + Qualcomm NNAPI | (org) | On-device speech SDK (ASR/TTS/VAD/denoise) with NPU accel — the closest existing "S2B2S-on-Android" SDK. |
| **soniqo/speech-swift** | MLX/CoreML | (org) | iOS/Apple-Silicon sibling (full-duplex PersonaPlex, Qwen3-ASR/TTS) — reference for a future iOS port + SOTA models. |
| **XDcobra/react-native-sherpa-onnx** | RN TurboModule | — | If you choose React Native: sherpa-onnx STT/TTS/VAD/diarization for Android+iOS in one module. |
| **BreezeApp (MediaTek)** | Android+iOS | — | A full shipping mobile AI app: offline STT + TTS + chatbot + image Q&A. End-to-end product reference. |
| **siva-sub/NekoSpeak** (again) | — | MIT | Also a good "models on first run + bundled default voice" UX reference (135 MB universal APK pattern). |
| **notune/android_transcribe_app** | Rust, transcribe-rs, Parakeet TDT | — | **Desktop STT toolchain continuity.** transcribe-rs running on Android in production. 25 languages. Study for a secondary STT path that matches the desktop exactly. |

### Brain / LLM on Android
| Project / framework | Role |
|---|---|
| **ferranpons/Llamatik** (MIT, KMP) | **Top candidate for concrete Android binding.** One Kotlin API: llama.cpp (LLM) + whisper.cpp (STT) + stable-diffusion.cpp (image). MTP speculative drafting, concurrent sessions, Maven Central. Could eliminate custom JNI work entirely. |
| **Aatricks/llmedge** | GGUF download + caching + **on-device Safetensors→GGUF conversion with quantization** (Q8_0/Q4_K_M/IQ2_BN). Solves the "get models onto the phone" pipeline. |
| **shubham0204/SmolChat-Android** | Clean reference: llama.cpp JNI, Markdown rendering, chat history. Widely-cited production reference for GGUF on Android. |
| **Siddhesh2377/llama.cpp-android** (Tool-Neuron backend) | Production llama.cpp fork, CPU-only, ARM-optimized (NEON/i8mm/dotprod, KleidiAI). VLM + RAG engines, big.LITTLE-aware thread modes. |
| **FilipFan/PolyEngineInfer** | **Benchmarking harness** — runs llama.cpp, ExecuTorch, LiteRT, and ONNX side-by-side. Use for Phase 0 Brain spike to compare engines empirically. |
| **alibaba/MNN / MNN-LLM** | Production inference engine; FP16/Int8; CPU+GPU (Metal/OpenCL/Vulkan) on iOS 8+/Android 4.3+. Qwen3.5, Gemma, Llama, DeepSeek, Phi. Heavier but full-featured. |
| **mlc-ai/mlc-llm** | Universal LLM deployment via ML compilation (TVM). iOS/Android/WebGPU. |
| **pytorch/executorch** | PyTorch on-device runtime. QAT export via torchao, Qualcomm QNN delegation, LlamaDemo Android app. |
| **software-mansion/react-native-executorch** | RN wrapper for ExecuTorch (if RN path chosen). |
| **hung-yueh/react-native-litert-lm** | RN wrapper for LiteRT-LM + Gemma-4 MTP (if RN path chosen). Demoed E2B on Galaxy S22 (4 GB). |

### Engine & protocol
| Project | Role |
|---|---|
| **k2-fsa/sherpa-onnx** (Apache-2.0) | The backbone. AAR `com.github.k2-fsa:sherpa-onnx`; Android demos for TTS, system-TTS-engine, streaming ASR, VAD, KWS, WebSocket, Wear OS; recent **Qualcomm QNN** streaming-ASR Android demos. |
| **google-ai-edge/LiteRT-LM** (Apache-2.0) | Google's recommended on-device LLM runtime (successor to MediaPipe LLM Inference). Native Gemma-4 MTP+QAT, agent skills, OpenAI-compatible server, Android demo scaffold. |
| **google-ai-edge/litert** (Apache-2.0) | The general-purpose runtime under LiteRT-LM. Unified NPU acceleration, Compiled Model API. |
| **mrtozner/vox** | WebSocket voice protocol reference — only for the optional remote/hybrid mode. |
| **Vite PWA / Web Audio / MediaRecorder** | Only relevant if you also keep a PWA *companion*; irrelevant to the on-device port. |

---

## 8. Hardware & Performance Realities (plan around these)

- **Throughput:** LLM decode ≈ 10–30 tok/s for small models at INT4 on flagships (8 Gen 3 / 8 Elite class), much less on mid-range. Speech models are cheap by comparison (RTF ≪ 1). Keep the Brain at E2B and stream tokens to TTS sentence-by-sentence so *perceived* latency stays low.
- **Memory:** Gemma E2B Q4 ≈ 1.5 GB resident + ONNX speech models + app → **target 8 GB+ RAM** for the full loop; ship STT-only / TTS-only modes for smaller devices.
- **Thermals & battery:** sustained generation heats the SoC and drains battery; cap context, avoid MTP, and don't keep the LLM warm indefinitely.
- **Acceleration:** sherpa-onnx supports **NNAPI** and has **Qualcomm QNN** streaming-ASR Android demos; default speech to CPU/XNNPACK and use NPU opportunistically. **For the Brain:** llama.cpp NPU paths (QNN/MTK) are still experimental — but production NPU options exist via **LiteRT-LM** (unified NPU acceleration), **ONNX Runtime QNN EP** (`onnxruntime-qnn`, Qualcomm-maintained standalone plugin), and **mllm** (QNN AOT full-graph NPU execution). Default Brain to CPU/Vulkan GPU for v1; evaluate LiteRT-LM for NPU acceleration on supported devices.
- **APK size:** bundling Piper + one voice ≈ 135 MB (NekoSpeak). Keep the base APK small with **on-demand model downloads** + **Play Asset Delivery**; bundle only a tiny default voice.

---

## 9. Recommended "How to Start" (first two weeks)

1. **Day 1 — License decision (§4).** Choose Misaki-MIT-core + optional eSpeak (NekoSpeak pattern). Write it down.
2. **Day 1–3 — Stack spike (Phase 0).** New Kotlin/Compose app; add `com.github.k2-fsa:sherpa-onnx`; get the `SherpaOnnxTts` and `SherpaOnnxSimulateStreamingAsr` demos running on a real device with Kokoro + Nemotron-80ms. Measure RTF/latency.
3. **Day 4–6 — Brain spike.** Evaluate **[Llamatik](https://github.com/ferranpons/Llamatik)** as the concrete Kotlin binding for llama.cpp (adds whisper.cpp STT for free, Maven Central, MTP support). Load Gemma 4 E2B Q4 GGUF; measure tok/s, TTFT, RAM, temperature. Compare against a **LiteRT-LM** `.task` run (study the `google-ai-edge/gallery` app as blueprint).
4. **Day 7–10 — Vertical slice.** Build Phase 1 (streaming dictation with live partials) as the first shippable feature — it's the simplest end-to-end proof and immediately useful.
5. **Then** Phase 2 (system TTS engine) and Phase 3 (conversation loop with barge-in), reusing the soniqo pipeline design and the desktop's normalization/sentence-streaming concepts.
6. **Throughout — study the closest existing implementations:** install and reverse-engineer **ToolNeuron** (full offline AI suite with sherpa-onnx voice loop), **NekoSpeak** (MIT, system TTS with Misaki G2P), **Maise** (dual TTS+ASR system services with OpenPhonemizer), and **android_transcribe_app** (transcribe-rs on Android). Benchmark Brain engines empirically with **PolyEngineInfer**.

**Definition of done for v1.0:** offline dictation (IME/voice-input), offline read-aloud (system TTS engine), and an offline conversation loop (Gemma E2B) with barge-in — all running on the phone with no network, on an 8 GB-class device.

---

## 10. Open Decisions to Resolve Early

- **Shell:** Native Kotlin (recommended) vs React Native (skill reuse). Pick before Phase 1.
- **G2P/license:** Misaki-core vs OpenPhonemizer vs dual-build vs accept-GPLv3 (§4).
- **Brain runtime:** llama.cpp via **Llamatik** (KMP, one-API STT+LLM) vs direct JNI vs LiteRT-LM (Google-recommended, Gemma-4 MTP+QAT native) — or both behind one interface. Benchmark with **PolyEngineInfer**.
- **Model delivery:** bundle minimal + download, via Play Asset Delivery / Hugging Face.
- **Scope of "Brain":** text-only v1 vs multimodal (mmproj) later.
- **Remote mode:** build it at all? If yes, the desktop `control_server.rs` needs a real WebSocket server first.
- **Distribution channels:** Beyond Play Store, evaluate **F-Droid** (used by SherpaTTS), **Obtainium** (used by Box), and **Accrescent** (used by Transcribro) for privacy-conscious users who avoid Google services.
- **Google developer-identity deadline:** SherpaTTS README warns that certified Android devices may block unverified system TTS engines after Google's 2026/2027 requirement. System TTS/IME registration (Phase 1–2) may need identity verification or degrade gracefully on locked-down devices. Track and plan for this.

---

*Bottom line: the path you intuited is the right one. sherpa-onnx (Kokoro + Piper + streaming ASR + VAD + KWS) is the single backbone, llama.cpp carries the same Gemma E2B brain as the desktop, native Kotlin is the cleanest shell, and soniqo/speech-core is the orchestration template. The only thing that needs a decision before code is the eSpeak/GPLv3 question — and NekoSpeak (MIT) already shows the way through it.*
