# sherpa-onnx -- Speech AI Framework (Category C)

> Repo: `k2-fsa/sherpa-onnx` . License: Apache-2.0 . Author: Xiaomi Corporation . Platforms: Linux, macOS, Windows, Android, WearOS, iOS, HarmonyOS, Node.js, WebAssembly, RISC-V, NVIDIA Jetson, Raspberry Pi, RK3588
> Nature: **framework** -- universal ONNX-based speech AI engine backed by C++ core with 12 language bindings
> Role for S2B2S: sherpa-onnx is the heaviest and most complete speech framework in this reference set. S2B2S already uses transcribe-rs (which wraps ONNX models with a similar philosophy but much narrower scope). sherpa-onnx offers capabilities S2B2S could adopt: TTS engine selection, KWS, speaker diarization, multi-NPU acceleration, WebSocket server mode, and a truly comprehensive model zoo. However, integrating sherpa-onnx directly would be a major migration away from transcribe-rs.

---

## 1. What sherpa-onnx Is

sherpa-onnx is a production-grade speech AI inference framework built by the "next-gen Kaldi" team at Xiaomi. It wraps the ONNX Runtime to run speech models **100% locally, offline, with no Internet connection**. It is the swiss-army knife of ONNX speech inference: speech-to-text (ASR), text-to-speech (TTS), voice activity detection (VAD), keyword spotting (KWS), speaker diarization/identification/verification, spoken language identification, audio tagging, punctuation restoration, speech enhancement/denoising, source separation, and diacritization -- all run through a single unified C API with language bindings for C++, C, Python, Java, Kotlin, C#, Go, Dart, Swift, Rust, Pascal, and JavaScript (Node.js).

The project is actively maintained with version 1.13.2 (June 2026), 1398+ lines of changelog, and a massive community of downstream projects including Open-LLM-VTuber, BreezeApp (MediaTek), MentraOS (smart glasses), Speed of Sound (Linux voice typing), VoxSherpa TTS, and many more. Pre-built APKs, Flutter apps, HuggingFace Spaces, and WebAssembly demos allow zero-install trials of every feature.

The key differentiator from other ONNX speech frameworks is breadth: no other single library provides ASR + TTS + KWS + diarization + punctuation + audio tagging + denoising + source separation across 12 programming languages and 4 NPU backends.

## 2. Tech Stack

### 2.1 Backend / Core

| Layer | Choice | Purpose |
|-------|--------|---------|
| Core language | C++17 | All inference logic, model implementations, feature extraction |
| Inference runtime | ONNX Runtime (v1.24.4) | CPU, CUDA, TensorRT, DirectML, CoreML execution |
| Build system | CMake 3.15+ | Cross-platform build with 50+ configurable options |
| Feature extraction | kaldi-native-fbank | Mel filterbank features (from Kaldi ecosystem) |
| Decoding graph | kaldi-decoder (openfst) | WFST-based CTC decoding, rule FSTs |
| Text processing | kaldifst TextNormalizer | ITN (inverse text normalization) |
| Phonemization | piper-phonemize + espeak-ng | TTS phonemization for Piper models |
| Clustering | hclust-cpp | Hierarchical clustering for speaker diarization |
| Audio I/O | portaudio, ALSA | Microphone capture for demo binaries only (core library is audio-agnostic) |
| WebSocket | websocketpp + asio | ASR/TTS WebSocket server mode |
| Python bindings | pybind11 | Python C++ extension module |
| Rust bindings | Cargo build.rs auto-download | Prebuilt static/shared libs from GitHub Releases |

### 2.2 Key Dependencies

- **sentencepiece**: BPE tokenization used by most ASR models
- **kissfft**: Lightweight FFT used in kaldi-native-fbank (instead of larger FFTW)
- **Eigen v5.0.1**: Linear algebra for various models (e.g., Moonshine)
- **ureq**: HTTP downloader in Rust build.rs for auto-fetching prebuilt libs (with proxy-from-env support)
- **bzip2 + tar**: Decompression of prebuilt archives in Rust build.rs
- **openfst v1.8.5**: Finite-state transducer library for WFST decoding

### 2.3 NPU Backend Stacks

| NPU | Directory | Model Coverage |
|-----|-----------|----------------|
| Rockchip NPU (RKNN) | `sherpa-onnx/csrc/rknn/` (29 files) | Zipformer transducer/CTC, Paraformer, SenseVoice, Silero VAD, KWS, CTC greedy search |
| Qualcomm NPU (QNN) | `sherpa-onnx/csrc/qnn/` (20 files) | Zipformer transducer/CTC, Paraformer, SenseVoice (offline + online) |
| Ascend NPU | `sherpa-onnx/csrc/ascend/` (12 files) | Zipformer CTC, Paraformer, SenseVoice, Whisper |
| Axera NPU | `sherpa-onnx/csrc/axera/`, `axcl/` | Separate build scripts available |

## 3. Architecture & Source Map

```
sherpa-onnx/
├── CMakeLists.txt                    # Top-level build (646 lines, 50+ options)
├── setup.py                          # Python wheel packaging
├── README.md                         # Massive reference doc (661 lines)
├── CHANGELOG.md                      # 1398 lines of detailed history
│
├── sherpa-onnx/                      # CORE SOURCE (~1000 files)
│   ├── csrc/                         # C++ implementation (327 .cc + 355 .h = 682 files)
│   │   ├── online-recognizer*.{h,cc} # Streaming ASR: Transducer, Paraformer, CTC, NeMo
│   │   ├── offline-recognizer*.{h,cc}# Non-streaming ASR: ~19 model families
│   │   ├── online-model-config.h     # Online model dispatch (6 model families)
│   │   ├── offline-model-config.h    # Offline model dispatch (19 model families)
│   │   ├── offline-tts*.{h,cc}       # TTS: VITS, Matcha, Kokoro, ZipVoice, Kitten, Pocket, Supertonic
│   │   ├── keyword-spotter*.{h,cc}  # KWS: Transducer-based streaming keyword detection
│   │   ├── voice-activity-detector*  # VAD: Silero + TenVAD wrapper
│   │   ├── silero-vad-model*         # Silero VAD ONNX model implementation
│   │   ├── offline-speaker-diarization* # Pyannote segmentation + embedding + clustering
│   │   ├── speaker-embedding-*       # Speaker embedding extraction + manager
│   │   ├── spoken-language-id*       # Spoken language ID via Whisper
│   │   ├── audio-tagging*            # Audio tagging: CED + Zipformer
│   │   ├── offline-speech-denoiser*  # Speech enhancement: GTCRN, DPDFNet
│   │   ├── offline-source-separation*# Source separation: Spleeter, UVR
│   │   ├── offline-punctuation*      # Offline punctuation (CT Transformer)
│   │   ├── online-punctuation*       # Online punctuation (CNN-BiLSTM)
│   │   ├── offline-diacritization*   # Arabic diacritization (CATT)
│   │   ├── offline-websocket-*       # Offline WebSocket server
│   │   ├── online-websocket-*        # Online WebSocket server + client
│   │   ├── endpoint.{h,cc}           # Endpoint detection for streaming ASR
│   │   ├── features.{h,cc}           # Feature extraction config
│   │   ├── provider-config.{h,cc}    # CUDA/TensorRT/CPU provider config
│   │   ├── session.{h,cc}            # ONNX Runtime session management
│   │   ├── onnx-utils.{h,cc}         # ONNX tensor helpers
│   │   ├── sherpa-onnx*.cc           # CLI entry points (30+ demo binaries)
│   │   ├── parse-options.{h,cc}      # CLI argument parsing
│   │   ├── wave-reader/writer.*      # WAV I/O
│   │   ├── resample.{h,cc}           # Audio resampling (linear)
│   │   ├── circular-buffer.*         # Lock-free ring buffer for streaming
│   │   ├── rknn/                     # Rockchip NPU backend (29 files)
│   │   ├── qnn/                      # Qualcomm NPU backend (20 files)
│   │   └── ascend/                   # Ascend NPU backend (12 files)
│   │
│   ├── c-api/                        # C API (stable ABI across all bindings)
│   │   ├── c-api.h                   # 4324 lines -- the complete public surface
│   │   ├── c-api.cc                  # Implementation (~5000+ lines)
│   │   ├── cxx-api.h/.cc             # C++ convenience wrappers
│   │   └── Doxyfile, mainpage.md     # API documentation generation
│   │
│   ├── python/                       # Python bindings
│   │   ├── csrc/                     # pybind11 bridge (164 files -- mirrors csrc/)
│   │   ├── sherpa_onnx/              # Python package
│   │   └── tests/                    # Python test suite
│   │
│   └── rust/                         # Rust crates
│       ├── sherpa-onnx/              # Safe Rust wrapper (18 modules in src/)
│       │   ├── src/lib.rs            # Module declarations + doc examples (239 lines)
│       │   ├── src/online_asr.rs     # Streaming ASR
│       │   ├── src/offline_asr.rs    # Non-streaming ASR
│       │   ├── src/tts.rs            # TTS
│       │   ├── src/kws.rs            # Keyword spotting (252 lines)
│       │   ├── src/vad.rs            # VAD
│       │   ├── src/offline_speaker_diarization.rs
│       │   ├── src/speaker_embedding.rs
│       │   └── src/audio_tagging.rs
│       └── sherpa-onnx-sys/          # Raw FFI bindings
│           ├── build.rs              # Auto-download prebuilt libs (444 lines)
│           └── src/                  # 15 FFI modules
│
├── android/                          # 19 Android demo apps (APKs)
├── ios-swift/ + ios-swiftui/         # iOS Swift/SwiftUI demo apps
├── flutter/                          # Flutter plugin (pub.dev package sherpa_onnx)
├── tauri-examples/                   # Tauri v2 desktop apps (2 examples)
├── harmony-os/                       # HarmonyOS native apps (6 demos)
├── wasm/                             # WebAssembly builds (9 specialized configs)
├── rust-api-examples/                # Rust example programs (30+ examples)
├── python-api-examples/              # Python example scripts
├── c-api-examples/ + cxx-api-examples/ # C/C++ API examples
├── go-api-examples/ + dart-api-examples/ + dotnet-examples/
├── nodejs-examples/ + nodejs-addon-examples/
├── swift-api-examples/ + kotlin-api-examples/ + java-api-examples/
├── pascal-api-examples/ + lazarus-examples/ + mfc-examples/
├── ffmpeg-examples/                  # FFmpeg integration examples
├── scripts/ + cmake/ + toolchains/   # Build infrastructure
└── .github/                          # CI, issue templates
```

**Scale**: ~1000 C++ source files, 4324-line C API header, 19 Android demos, 8 Flutter plugins, 6 HarmonyOS demos, 9 WASM configs, 12 language binding packages, 4 NPU backends.

## 4. Feature Inventory

### 4.1 Speech-to-Text (ASR) -- Streaming

**Online (streaming) ASR** supports processing audio chunk-by-chunk with incremental results. 6 model families:

| Model Family | Config Struct | Decoder Types | Notes |
|-------------|---------------|---------------|-------|
| Transducer (Zipformer, Conformer, LSTM) | `OnlineTransducerModelConfig` | Greedy search, Modified beam search | encoder + decoder + joiner ONNX. Hotwords via contextual biasing |
| Paraformer | `OnlineParaformerModelConfig` | Greedy search | encoder + decoder. Alibaba non-autoregressive |
| NeMo CTC | `OnlineNeMoCtcModelConfig` | Greedy search, WFST | NVIDIA NeMo CTC models |
| NeMo Transducer | `OnlineTransducerModelConfig` (nemo variant) | Greedy search, Modified beam search | Includes Parakeet Unified with RNNT buffered path |
| Zipformer2 CTC | `OnlineZipformer2CtcModelConfig` | Greedy search, WFST | Zipformer2 architecture, CTC output |
| Wenet CTC | `OnlineWenetCtcModelConfig` | Greedy search, WFST | WeNet CTC models |
| T-One CTC | `OnlineToneCtcModelConfig` | Greedy search | Specialized Chinese model |

**Shared streaming infrastructure** (`online-recognizer-impl.cc`):
- `OnlineStream`: Accumulates audio samples, computes features in chunks, manages decoder state
- `OnlineRecognizer`: Creates streams, feeds audio, runs `DecodeStreams()` (parallel multi-stream), returns `OnlineRecognizerResult` with text, tokens, timestamps, log-probs
- `Endpoint`: Rule-based endpoint detection (silence duration thresholds) with `IsEndpoint()` and `Reset()`
- Hotwords: Per-stream contextual biasing via `CreateStream(hotwords)` -- modifies beam search scores
- Homophone replacement: Configurable fuzzy correction via `HomophoneReplacer`
- ITN (Inverse Text Normalization): Applied post-decode via `kaldifst::TextNormalizer`
- LM integration: Optional RNN LM rescoring via `online-lm.{h,cc}`

**Key files**: `online-recognizer.h` (231 lines), `online-recognizer-impl.h`, `online-recognizer-transducer-impl.h` (main streaming impl), `online-recognizer-transducer-nemo-impl.h`, `online-recognizer-transducer-nemo-parakeet-unified-impl.h`, `online-recognizer-paraformer-impl.h`, `online-recognizer-ctc-impl.h`, `online-stream.{h,cc}`.

### 4.2 Speech-to-Text (ASR) -- Non-Streaming

**Offline (non-streaming) ASR** processes complete audio files in one pass. **19 model families** as of v1.13.2:

Transducer (Zipformer), Paraformer, NeMo CTC, Whisper (tiny to large), FireRed ASR, FireRed ASR CTC, TDNN, Zipformer CTC, Wenet CTC, SenseVoice, Moonshine (v1 + v2), Dolphin, Canary, Cohere Transcribe, Omnilingual ASR, FunASR Nano, MedASR CTC, Qwen3 ASR, TeleSpeech CTC.

Plus NeMo Parakeet TDT 0.6B (Parakeet V3 from NVIDIA), Whisper timestamp rules with DTW alignment.

**Key files**: `offline-model-config.h` (129 lines, 19 model families dispatch), `offline-recognizer.h`, `offline-recognizer-impl.h` (65 lines, base impl with ITN/homophone replacement), `offline-recognizer-ctc-impl.h`, `offline-recognizer-transducer-impl.h`, `offline-recognizer-whisper-impl.h`, `offline-transducer-greedy-search-decoder.*`, `offline-transducer-modified-beam-search-decoder.*`.

### 4.3 Text-to-Speech (TTS)

**7 model families**: VITS, Matcha, Kokoro, ZipVoice (voice cloning CN+EN), Kitten (v0.8), Pocket (voice cloning EN, flow matching, 5 model files), Supertonic (v3, added v1.13.2).

**TTS Pipeline** (`offline-tts.h`, 164 lines): `OfflineTtsConfig` (model config + rule FSTs + silence scale + batch size), `GenerationConfig` (speed, speaker ID, reference audio for cloning, flow steps, model-specific extras), `GeneratedAudio` (float samples + sample rate), `GeneratedAudioCallback` (streaming per-batch callback with progress), `Generate()` unified method, `NumSpeakers()`, `SampleRate()`.

**TTS Frontend**: Text-to-phoneme via Piper phonemize lexicon, Kokoro multi-lang lexicon, Matcha-TTS lexicon, Melo-TTS lexicon, Supertonic Unicode processor, character frontend.

**Key files**: `offline-tts-model-config.h` (64 lines, dispatch hub), `offline-tts-impl.cc`, `offline-tts-vits-impl.h`, `offline-tts-kokoro-impl.h`, `offline-tts-pocket-impl.h`, `offline-tts-zipvoice-impl.h`, `offline-tts-kitten-impl.h`, `offline-tts-matcha-impl.h`, `offline-tts-supertonic-impl.{h,cc}`, `hifigan-vocoder.{h,cc}`, `vocos-vocoder.{h,cc}`, `vocoder.{h,cc}`.

### 4.4 Voice Activity Detection (VAD)

Two models: **Silero VAD** (`silero-vad-model.{h,cc}`) and **TenVAD** (`ten-vad-model.{h,cc}`).

**VAD API** (`voice-activity-detector.h`, 63 lines): `AcceptWaveform()`, `Compute()` (returns probability), `Empty()`/`Front()`/`Pop()` (queue-based segment access), `IsSpeechDetected()`, `CurrentSpeechSegment()`, `Flush()`, `Reset()`, configurable 60-second buffer.

### 4.5 Keyword Spotting (KWS)

Streaming keyword detection using transducer ASR models with keyword-specific decoding.

**KWS API** (`keyword-spotter.h`, 150 lines): `KeywordSpotterConfig` (feature config, model config, keywords file/buffer, score threshold), `KeywordSpotter` (Create stream, IsReady, Decode, Reset, GetResult), `KeywordResult` (triggered keyword, tokens, timestamps, JSON). Per-stream keyword override via `CreateStream(keywords)` for dynamic keyword lists.

**Implementation**: Transducer greedy search with keyword-specific scoring. `TransducerKeywordDecoder` uses modified beam search biased toward keyword paths. Works with same online transducer models as ASR.

**Key files**: `keyword-spotter.{h,cc}`, `keyword-spotter-impl.{h,cc}`, `keyword-spotter-transducer-impl.h`, `transducer-keyword-decoder.{h,cc}`, `kws.rs` (252 lines, Rust binding).

### 4.6 Speaker Diarization

3-stage pipeline: **Speaker segmentation** (Pyannote-based, `min_duration_on` 0.3s, `min_duration_off` 0.5s), **Speaker embedding extraction** (Wespeaker-style or NeMo TitaNet-style), **Fast clustering** (hierarchical agglomerative).

**API** (`offline-speaker-diarization.h`, 84 lines): `Process(audio, n, callback)` end-to-end, `SetConfig()` runtime update, `SampleRate()`, progress callback.

**Key files**: `offline-speaker-diarization-impl.cc`, `offline-speaker-diarization-pyannote-impl.h`, `offline-speaker-segmentation-pyannote-model.*`, `speaker-embedding-extractor.{h,cc}`, `fast-clustering.{h,cc}`.

### 4.7 Speaker Identification & Verification

**Speaker Embedding Extractor** (`speaker-embedding-extractor.h`, 71 lines): Extracts fixed-dimensional embeddings. `CreateStream()`, `IsReady()`, `Compute()` (embedding vector), `Dim()`.

**Speaker Embedding Manager** (`speaker-embedding-manager.{h,cc}`): Enroll by name, search/identify nearest, verify match. Persistent enrollment database.

### 4.8 Additional Features

- **Spoken Language Identification**: Via multilingual Whisper. Returns language code. `spoken-language-identification.h` (99 lines).
- **Audio Tagging**: CED (CNN14) + Zipformer models. Returns top-k acoustic events with probabilities. `audio-tagging.h` (74 lines).
- **Punctuation Restoration**: Offline (CT Transformer, `offline-punctuation.h` 50 lines) + Online (CNN-BiLSTM, `online-punctuation.h` 51 lines).
- **Speech Enhancement**: GTCRN + DPDFNet, each with offline + online variants. `offline-speech-denoiser.h` (61 lines).
- **Source Separation**: Spleeter (2/4/5-stem) + UVR. `offline-source-separation.h` (78 lines).
- **Diacritization**: Arabic text via CATT model. `offline-diacritization.{h,cc}`.
- **WebSocket Server/Client**: Offline (205 lines) + Online server, Online client. Binary protocol (sample_rate + byte_size + float32 data). No TLS.

### 4.9 Platform Features

| Platform | Binding | Demo Apps | Notes |
|----------|---------|-----------|-------|
| **Android** | JNI + Java/Kotlin API | 13 APKs (ASR, KWS, TTS, VAD, diarization, audio tagging, etc.) | TTS Engine plugin (system-wide TTS), WearOS support |
| **iOS** | Swift API | Swift + SwiftUI demo apps | Build from source (no pre-built IPA) |
| **Flutter** | Dart FFI plugin | pub.dev package (`sherpa_onnx` v1.13.2) | 8 platform plugins (Android 4 archs, iOS, macOS, Linux, Windows) |
| **Desktop (Tauri)** | Rust via C API | 2 Tauri v2 apps | Non-streaming ASR from file/mic, 62 models, SRT export |
| **HarmonyOS** | Native C++ | 6 demos | Full OHOS support |
| **WebAssembly** | Emscripten + JS | 9 browser demos on HuggingFace | ASR, TTS, VAD, KWS, diarization, enhancement, Node.js |
| **Desktop Native** | C/C++/Python/Rust/Go/etc. | 30+ CLI binaries | microphone, file, ALSA, parallel decoding |

### 4.10 NPU Acceleration

Unique among open-source speech frameworks: **Rockchip RKNN** (29 files: Zipformer transducer/CTC, Paraformer, SenseVoice, Silero VAD, KWS), **Qualcomm QNN** (20 files: Zipformer transducer/CTC, Paraformer, SenseVoice), **Ascend NPU** (12 files: Zipformer CTC, Paraformer, SenseVoice, Whisper), **Axera NPU**. Each NPU has its own model implementation -- re-implementing models for each NPU SDK.

## 5. Key Code Patterns & Techniques

### 5.1 Model Dispatch Architecture

Each feature has a config struct containing all supported model families as optional sub-structs. Exactly one must be non-empty. The impl factory checks which is set and creates the appropriate implementation. Adding a new model family requires creating impl files and adding one `else if` branch -- the public API is unchanged. This is how the project grew from ~3 to 19+ models without breaking changes.

### 5.2 C API as Stable ABI

The 4324-line `c-api.h` is the single source of truth for ALL 12 language bindings. Opaque handles with Create/Destroy pairs, `typedef struct` configs, heap-allocated result structs with matching Destroy functions, JSON string alternatives, Android `AAssetManager*` constructor variants.

### 5.3 Platform Abstraction via Manager Template

`template <typename Manager> OnlineRecognizer(Manager *mgr, const Config &config)` allows loading ONNX models from Android assets (read-only compressed storage) instead of filesystem, keeping the same C++ implementation for all platforms.

### 5.4 Streaming Decoding State Machine

`OnlineStream` accumulates audio in a circular buffer, extracts features in chunks. `IsReady()` checks for sufficient frames. `DecodeStreams(ss, n)` runs parallel multi-stream decode. `GetResult()` returns incremental results. `IsEndpoint()`/`Reset()` for utterance segmentation. `InputFinished()` for tail processing.

### 5.5 Feature Extraction Pipeline

`features.{h,cc}`: 16000 Hz sample rate, 80-dim mel bins, 10ms frame shift, 25ms frame length. Features flow through ONNX encoder -> decoder/joiner.

### 5.6 Rust Auto-Download Build Script

`build.rs` (444 lines): Auto-detects target OS/arch, downloads prebuilt libs from GitHub Releases, caches in target dir, handles bzip2+tar extraction, sets rpath on Linux/macOS, copies DLLs on Windows. Supports `SHERPA_ONNX_LIB_DIR` override. Single `Cargo.toml` dependency with zero manual setup.

### 5.7 TTS Generation Pipeline

Text -> frontend (phonemization) -> ONNX model -> vocoder -> audio samples. `max_num_sentences` batch control to avoid OOM. `GeneratedAudioCallback` for streaming per-batch. `silence_scale` controls pause duration. `GenerationConfig.extra` for model-specific settings. Voice cloning: reference audio + text + flow matching steps.

### 5.8 ITN (Inverse Text Normalization)

Post-ASR: "one hundred and twenty three" -> "123". Rule FSTs applied left-to-right via `kaldifst::TextNormalizer`. Built into `OfflineRecognizerImpl::ApplyInverseTextNormalization()`.

## 6. Relation to S2B2S

S2B2S uses **transcribe-rs** for STT and its own TTS subsystem via HTTP servers. sherpa-onnx is a superset of both.

| Aspect | sherpa-onnx | transcribe-rs | S2B2S | Verdict |
|--------|------------|---------------|-------|---------|
| **STT engines** | 19 model families (streaming + non-streaming) | 9 engines | Uses transcribe-rs (7 engines) | sherpa-onnx has more models |
| **Streaming ASR** | First-class (Zipformer, Paraformer, NeMo, CTC) | Moonshine streaming only | None | sherpa-onnx enables real-time dictation |
| **TTS** | 7 model families, phonemization, voice cloning | None | 8 backends via HTTP servers | S2B2S broader TTS but HTTP-based |
| **VAD** | Silero, TenVAD, queue-based | Silero (vad-silero) | TripleVAD (RMS+RNNoise+Silero) | Equivalent |
| **KWS** | Full transducer-based keyword spotting | None | Wake word (not fully connected) | sherpa-onnx KWS is production-ready |
| **Speaker diarization** | Pyannote segmentation+embedding+clustering | None | None | Could add "who said what" |
| **Speaker ID/verification** | Embedding extractor + enrollment manager | None | None | Potential voice login |
| **Audio tagging** | CED + Zipformer | None | None | Auto-tag recordings |
| **Punctuation** | Online + offline | None | ITN only | Improve ASR readability |
| **Speech enhancement** | GTCRN, DPDFNet (offline+online) | None | RNNoise | More advanced denoising |
| **Language bindings** | 12 languages | Rust only | Rust + TypeScript | sherpa-onnx polyglot |
| **Platform support** | 7 OSes + 4 NPUs + WASM | Rust targets | Win/Mac/Linux | sherpa-onnx embedded/IoT |
| **API style** | C ABI, Create/Destroy | Rust trait SpeechModel | Manager via Tauri state | Different philosophies |
| **License** | Apache-2.0 | MIT | MIT | Compatible with patent grant |
| **Codebase size** | ~1000 C++ files, 12 binding languages | ~50 Rust files | ~200 Rust + ~100 TS files | 10x larger |

**Comparison Summary**: sherpa-onnx is the "do everything" framework. For KWS, diarization, streaming ASR, punctuation, enhancement, or separation, sherpa-onnx provides them all. However, switching S2B2S from transcribe-rs would mean replacing the `SpeechModel` trait with C FFI, managing new model downloads, larger binary size (~23 static libs), and licensing (MIT->Apache-2.0). The pragmatic approach: use sherpa-onnx for features transcribe-rs does not have (KWS, diarization) while keeping transcribe-rs for core STT.

## 7. Harvest List (Features Worth Copying)

| Feature to harvest | From file | Effort | Why valuable for S2B2S |
|-------------------|-----------|--------|------------------------|
| **Keyword spotting (KWS)** | `keyword-spotter-transducer-impl.h`, `transducer-keyword-decoder.{h,cc}` | L | Replace incomplete wake word. Production-ready transducer KWS with per-stream keyword override. Could power "Hey S2B2S" voice commands |
| **Speaker diarization** | `offline-speaker-diarization-pyannote-impl.h`, `fast-clustering.{h,cc}` | L | Add "who said what" to conversation history. Pyannote segmentation + embedding + clustering pipeline |
| **Streaming ASR** | `online-recognizer-transducer-impl.h`, `online-stream.{h,cc}` | L | Real-time dictation with incremental results. S2B2S currently processes complete utterances only |
| **Online punctuation** | `online-punctuation-cnn-bilstm-impl.h` | M | Real-time punctuation insertion during streaming ASR |
| **TTS model dispatch pattern** | `offline-tts-model-config.h` (64 lines) | S | Unify S2B2S 9 TTS backends: one config struct, factory picks the right impl |
| **C API as stable ABI** | `c-api.h` (4324 lines) | M | If S2B2S wants language bindings. sherpa-onnx is a masterclass in C API design |
| **Auto-download build.rs** | `sherpa-onnx-sys/build.rs` (444 lines) | M | Auto-download prebuilt native libs during Cargo build. Elegant pattern for S2B2S deps |
| **NPU acceleration abstractions** | `rknn/`, `qnn/`, `ascend/` | XL | Only open-source examples of speech models on edge NPUs |
| **Speech enhancement** | `offline-speech-denoiser-gtcrn-impl.h` | L | Upgrade from RNNoise to GTCRN/DPDFNet for better pre-ASR noise suppression |
| **WebSocket server protocol** | `online-websocket-server-impl.h` | S | Simple binary protocol for remote ASR/TTS. Enable S2B2S to serve LAN devices |
| **ITN via kaldifst** | `offline-recognizer-impl.h` (lines 50-53) | S | kaldifst FST-based ITN may be more robust than text-processing-rs for complex rules |

## 8. Known Issues, Caveats & Limitations

| Issue | Severity | Impact |
|-------|----------|--------|
| **No TLS in WebSocket servers** | Medium | Both online/offline servers use `asio_no_tls.hpp`. TODO exists but TLS not implemented. Requires reverse proxy |
| **WebSocket inactivity timeout not implemented** | Low | `online-websocket-server-impl.h` line 40 TODO. Long-lived connections could leak |
| **Modified beam search for offline ASR not implemented** | Low | `offline-recognizer.h` line 47 TODO. Only greedy search for offline |
| **Paraformer streaming batch > 1 not supported** | Low | `online-recognizer-paraformer-impl.h` line 176 TODO. Single-stream only |
| **Hardcoded constants in several model impls** | Low | Padding/chunk constants with TODO comments to make configurable |
| **No checksum verification for Rust prebuilt lib downloads** | Medium | MITM risk during build. Uses ureq without hash verification |
| **Static CRT requirement on Windows** | Low | Default `/MT` linking. Mixing with `/MD` causes linker errors |
| **Large binary size with static linking** | Medium | ~23 static libraries including ONNX Runtime. Shared lib recommended for production |
| **iOS requires source build** | Low | No pre-built iOS framework |
| **RISC-V / niche NPU testing gaps** | Low | Less test coverage than main x64/ARM paths |

## 9. Strengths & Weaknesses

### Strengths

1. **Unmatched breadth**: 12+ speech AI functions in one framework. ASR, TTS, VAD, KWS, diarization, speaker ID, language ID, audio tagging, punctuation, denoising, source separation, diacritization.
2. **Model zoo depth**: 19 ASR model families, 7 TTS families. New models added monthly.
3. **Platform reach**: 7 OSes + 4 NPUs + WASM + embedded SBCs (Jetson, Raspberry Pi, RK3588).
4. **NPU support unique among open-source**: Rockchip, Qualcomm, Ascend, Axera.
5. **12 language bindings**: From a single stable C ABI. Rust bindings with auto-download.
6. **Production-ready**: Used by MediaTek, MentraOS, Open-LLM-VTuber, Speed of Sound, etc.
7. **Stable ABI**: 100+ releases without breaking changes.
8. **Excellent documentation**: Doxygen, HF Spaces, WASM demos, pre-built APKs, Flutter packages.
9. **Consistent design**: Config -> Create -> Stream -> Decode -> Result -> Destroy pattern everywhere.
10. **Apache-2.0 license**: Permissive with patent grant.

### Weaknesses

1. **C++ monolith**: ~1000 C++ files. Requires deep C++17 and ONNX Runtime knowledge to contribute.
2. **Large dependency tree**: 23 static libraries. Complex linking.
3. **No Rust-native models**: All inference bridges through C ABI.
4. **WebSocket no TLS**: Production requires reverse proxy.
5. **TTS is batch-based**: No true streaming TTS (generating audio as text arrives).
6. **Limited testing on niche platforms**: NPU backends, HarmonyOS, RISC-V.
7. **No unified feature pipeline**: Combining VAD+ASR+punctuation+diarization requires app-level orchestration.
8. **iOS source-build only**: No pre-built framework.
9. **Heavy maintenance burden**: Supporting 12 languages, 4 NPUs, 7 OSes, 26+ model families.

## 10. Bottom Line / Verdict

sherpa-onnx is the undisputed heavyweight champion of open-source ONNX speech AI frameworks. Its breadth is staggering: 12 speech functions, 19 ASR model families, 7 TTS families, 12 language bindings, 4 NPU backends, and 7 operating systems -- all from a single C API. For S2B2S, it represents both an aspirational benchmark and a practical resource. The single most valuable idea to copy is the **model-family dispatch pattern** (one config struct, factory picks the right impl), which could cleanly unify S2B2S fragmented TTS backend system. The most impactful feature to harvest is the **keyword spotting module**, which directly addresses S2B2S incomplete wake word functionality. However, a full migration from transcribe-rs to sherpa-onnx would be a major undertaking and is likely not justified given transcribe-rs already covers S2B2S core STT needs well. The pragmatic path: cherry-pick KWS, diarization, and punctuation features from sherpa-onnx while retaining transcribe-rs for the main ASR pipeline.

---

*Analysis generated from deep reading: README (661 lines), CMakeLists.txt (646 lines), c-api.h (4324 lines), offline-model-config.h (129 lines, 19 model families), online-model-config.h (90 lines, 6 model families), offline-tts-model-config.h (64 lines, 7 TTS families), keyword-spotter.h (150 lines), voice-activity-detector.h (63 lines), offline-speaker-diarization.h (84 lines), speaker-embedding-extractor.h (71 lines), spoken-language-identification.h (99 lines), audio-tagging.h (74 lines), offline-speech-denoiser.h (61 lines), offline-source-separation.h (78 lines), offline-punctuation.h (50 lines), online-punctuation.h (51 lines), provider-config.h (93 lines), offline-recognizer-impl.h (65 lines), Rust lib.rs (239 lines), kws.rs (252 lines), build.rs (444 lines), CHANGELOG.md (1398 lines). Directory exploration: 327 .cc + 355 .h C++ source files, 19 Android demos, 8 Flutter plugins, 6 HarmonyOS demos, 9 WASM configs, 12 language binding packages, 4 NPU backends.*
