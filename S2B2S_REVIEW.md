# S2B2S — Complete Project Review & Analysis

> **SpeechToBrainToSpeech**: A local-first, cross-platform, open-source desktop application that turns voice into text, text into speech, and enables natural spoken conversation with AI — all offline by default.
>
> **Last audited:** June 2026. This document reflects the verified state of the codebase (105 Rust source files, 113+ TypeScript/React source files, 20 locale files, 9 CI workflows).

---

## Table of Contents

1. [Project Overview](#1-project-overview)
2. [Vision & Philosophy](#2-vision--philosophy)
3. [Architecture Deep Dive](#3-architecture-deep-dive)
4. [The Three Pipelines](#4-the-three-pipelines)
5. [STT Subsystem — Speech-to-Text](#5-stt-subsystem)
6. [TTS Subsystem — Text-to-Speech](#6-tts-subsystem)
7. [Brain Subsystem — The LLM](#7-brain-subsystem)
8. [Voice Activity Detection (VAD)](#8-voice-activity-detection)
9. [Text Normalization Pipeline](#9-text-normalization-pipeline)
10. [Audio Processing Toolkit](#10-audio-processing-toolkit)
11. [Model Management](#11-model-management)
12. [Settings & Persistence](#12-settings--persistence)
13. [Frontend Architecture](#13-frontend-architecture)
14. [Internationalization (i18n)](#14-internationalization)
15. [CI/CD & Build System](#15-cicd--build-system)
16. [Project Lineage & Donor Map](#16-project-lineage--donor-map)
17. [Dependency Analysis](#17-dependency-analysis)
18. [File Structure Map](#18-file-structure-map)
19. [Roadmap & Future Work](#19-roadmap--future-work)
20. [Known Issues & Limitations](#20-known-issues--limitations)
21. [Diagrams](#21-diagrams)

---

## 1. Project Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                             S2B2S                                            │
│                    Speech → Brain → Speech                                   │
│                                                                              │
│    ┌─────────────┐    ┌─────────────┐    ┌─────────────┐                    │
│    │   DICTATE    │    │   READ       │    │   CONVERSE   │                   │
│    │   Anywhere   │    │   Aloud      │    │   with AI    │                   │
│    └──────┬───────┘    └──────┬───────┘    └──────┬───────┘                   │
│           │                  │                    │                          │
│           ▼                  ▼                    ▼                          │
│    ┌─────────────────────────────────────────────────────┐                  │
│    │              Shared Engine Room                       │                  │
│    │  STT (Parakeet V3 / Whisper)                        │                  │
│    │  TTS (Piper / Kokoro / Kitten / SAPI / Cloud)      │                  │
│    │  Brain (Ollama / LM Studio / OpenAI / Anthropic)    │                  │
│    │  VAD (TripleVAD: RMS → RNNoise → Silero)           │                  │
│    │  Text Pipeline (ITN → TN → Markdown Strip)          │                  │
│    └─────────────────────────────────────────────────────┘                  │
└─────────────────────────────────────────────────────────────────────────────┘
```

| Attribute           | Value                                                                |
| ------------------- | -------------------------------------------------------------------- |
| **Name**            | S2B2S (SpeechToBrainToSpeech)                                        |
| **Author**          | NairoDorian                                                          |
| **Version**         | 0.1.0 (working title v0.10)                                          |
| **License**         | MIT                                                                  |
| **Platform**        | Windows 11 (primary), macOS (first-class), Linux (first-class)       |
| **Framework**       | Tauri 2.x (Rust backend + React/TypeScript frontend)                 |
| **Package Manager** | Bun                                                                  |
| **Rust MSRV**       | 1.87                                                                 |
| **Repository**      | [github.com/NairoDorian/S2B2S](https://github.com/NairoDorian/S2B2S) |
| **Base Project**    | [Handy](https://github.com/cjpais/Handy) by CJ Pais (MIT)            |

### What S2B2S Does

1. **Dictate Anywhere** — Press a hotkey, speak, and polished text lands at your cursor in any application. Powered by Parakeet V3 (default, local, 25 languages with auto-detection). Works fully offline.

2. **Read Aloud** — Select text anywhere in any app, press a hotkey (or double-copy the same text within 1.5s), and a local voice reads it instantly. Features streaming playback, pause/resume, and 8 TTS backends (Piper, Kokoro, Kitten, Pocket, SAPI, OpenAI, ElevenLabs, Cartesia).

3. **Talk to the Brain** — The Conversation window: speak naturally to a local LLM (Ollama/LM Studio) or any cloud LLM (OpenAI, Anthropic, Gemini). Real-time STT in, streaming tokens out, sentence-by-sentence TTS reads the reply aloud (toggleable, default ON). Interruptible mid-sentence (barge-in).

---

## 2. Vision & Philosophy

### Core Principles

1. **Local-first, cloud-optional.** Every pillar works fully offline. A visible global "Offline mode" switch hard-blocks all egress.

2. **BYOK, no accounts, no middleman.** API keys live in the OS keychain (Windows Credential Manager, macOS Keychain). Audio goes user → provider directly — no proxy.

3. **Privacy is a feature with receipts.** Per-feature network indicators; retention sliders for audio and text; persist-before-deliver so crashes never eat work.

4. **Keyboard-optional, never keyboard-hostile.** Everything voice-reachable also has a hotkey and a click path.

5. **Fast beats fancy.** Piper-by-default for TTS speed, Parakeet V3 for STT speed. Warm models (pre-loaded in RAM) eliminate loading times. Latency budgets measured in CI.

6. **Cross-platform from day one.** Windows 11 excellence first, but macOS and Linux kept building and functional. No platform-specific lock-in.

7. **Complexity is opt-in.** Ship many features, default to few. Dangerous features (voice commands) ship OFF behind explicit consent.

### Target Audience

- **Privacy-conscious users** who want their voice data to stay on their machine
- **Developers** who read a lot of code/docs and want text read aloud
- **Power users** who want to dictate anywhere (emails, documents, chat)
- **AI enthusiasts** who want to speak naturally with local LLMs
- **Accessibility users** who benefit from voice input/output
- **French-speaking users** (and 20+ other languages) with full i18n

### Differentiators vs. Alternatives

| vs.                                             | S2B2S Advantage                                                                             |
| ----------------------------------------------- | ------------------------------------------------------------------------------------------- |
| **Wispr Flow / SuperWhisper**                   | Open source, free, BYOK, cross-platform, adds TTS read-aloud + Brain conversation           |
| **ChatGPT Voice / Copilot Voice**               | Fully local option (data never leaves machine), any model, inspectable code                 |
| **Open WebUI / LM Studio chat**                 | Voice-native (endpointing, barge-in, streaming TTS), lives system-wide, not in one window   |
| **Individual tools** (Handy, Parrot, CopySpeak) | One app with shared audio stack, model manager, key vault, history across all three pillars |

---

## 3. Architecture Deep Dive

### High-Level Architecture

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                           Tauri 2 Application                                  │
│                                                                                │
│  ┌────────────────────────────────────────────────────────────────────────┐   │
│  │  React/TypeScript Frontend (WebView)                                    │   │
│  │                                                                          │   │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────────┐ │   │
│  │  │ Settings │ │ Overlay  │ │Conversa- │ │ History  │ │  Her Loading  │ │   │
│  │  │  Panels  │ │  Window  │ │tion View │ │  Panel   │ │  Animation   │ │   │
│  │  └──────────┘ └──────────┘ └──────────┘ └──────────┘ └──────────────┘ │   │
│  │                                                                          │   │
│  │  ┌────────────────────────────────────────────────────────────────────┐ │   │
│  │  │ Zustand Stores │ i18n (20 langs) │ tauri-specta typed IPC bindings │ │   │
│  │  └────────────────────────────────────────────────────────────────────┘ │   │
│  └────────────────────────────────────────────────────────────────────────┘   │
│                           │  Tauri Commands + Events                           │
│                           ▼                                                    │
│  ┌────────────────────────────────────────────────────────────────────────┐   │
│  │  Rust Backend Core                                                      │   │
│  │                                                                          │   │
│  │  ┌─────────────┐ ┌──────────────────┐ ┌────────────────────────────┐   │   │
│  │  │  managers/  │ │    audio_toolkit │ │       settings.rs           │   │   │
│  │  │  ├ audio.rs │ │    ├ audio/      │ │  (Tauri plugin store)       │   │   │
│  │  │  ├ model.rs │ │    │ ├ device.rs │ │                             │   │   │
│  │  │  ├ trans-   │ │    │ ├ recorder  │ │  ┌───────────────────────┐  │   │   │
│  │  │  │ cription │ │    │ │ .rs       │ │  │  commands/            │  │   │   │
│  │  │  ├ history  │ │    │ ├ resampler │ │  │  ├ tts.rs             │  │   │   │
│  │  │  │ .rs      │ │    │ │ .rs       │ │  │  ├ brain.rs           │  │   │   │
│  │  │  └───────────│ │    │ ├ visual-  │ │  │  └ audio.rs           │  │   │   │
│  │  │              │ │    │ │ izer.rs   │ │  └───────────────────────┘  │   │   │
│  │  │  ┌──────────┐│ │    │ ├ noise_   │ │                             │   │   │
│  │  │  │  tts/    ││ │    │ │ suppres-  │ │  ┌───────────────────────┐  │   │   │
│  │  │  │ ├ back-  ││ │    │ │ sion.rs   │ │  │  shortcut/            │  │   │   │
│  │  │  │ │ ends/  ││ │    │ └ utils.rs  │ │  │  ├ mod.rs             │  │   │   │
│  │  │  │ │ ├ piper││ │    │ └ vad/      │ │  │  └ handy_keys.rs      │  │   │   │
│  │  │  │ │ ├ koko-││ │    │   ├ silero  │ │  └───────────────────────┘  │   │   │
│  │  │  │ │ │ ro   ││ │    │   ├ smoothed│ │                             │   │   │
│  │  │  │ │ ├ kit- ││ │    │   └ triple_ │ │  ┌───────────────────────┐  │   │   │
│  │  │  │ │ │ ten  ││ │    │     vad.rs   │ │  │  overlay.rs           │  │   │   │
│  │  │  │ │ ├ sapi ││ │    └──────────────┘ │  │  tray.rs              │  │   │   │
│  │  │  │ │ ├ open-││ │                     │  │  tray_i18n.rs         │  │   │   │
│  │  │  │ │ │ ai   ││ │  ┌──────────────┐  │  │  clipboard.rs          │  │   │   │
│  │  │  │ │ ├ elev-││ │  │  brain/      │  │  │  input.rs              │  │   │   │
│  │  │  │ │ │ en-  ││ │  │  ├ client.rs │  │  │  audio_feedback.rs     │  │   │   │
│  │  │  │ │ │ labs ││ │  │  └ manager.rs│  │  │  control_server.rs     │  │   │   │
│  │  │  │ │ └ cart-││ │  └──────────────┘  │  │  crash_logging.rs      │  │   │   │
│  │  │  │ │  esia  ││ │                     │  │  portable.rs           │  │   │   │
 │  │  │  │ ├ san-   ││ │  ┌──────────────┐  │  │  active_app.rs        │  │   │   │
│  │  │  │ │ itize/ ││ │  │  llm_client  │  │  │  apple_intelligence    │  │   │   │
│  │  │  │ │ ├ itn  ││ │  │  .rs         │  │  │  .rs                  │  │   │   │
│  │  │  │ │ ├ tn   ││ │  └──────────────┘  │  │  wake_word.rs          │  │   │   │
│  │  │  │ │ ├ mark-││ │                     │  │  cli.rs               │  │   │   │
│  │  │  │ │ │ down ││ │  ┌──────────────┐  │  │  signal_handle.rs     │  │   │   │
│  │  │  │ │ └ clea-││ │  │transcription_│  │  │  actions.rs           │  │   │   │
│  │  │  │ │  nup   ││ │  │coordinator.rs│  │  │  utils.rs             │  │   │   │
│  │  │  │ ├ mana-  ││ │  └──────────────┘  │                             │   │   │
│  │  │  │ │ ger.rs ││ │                     │                             │   │   │
│  │  │  │ ├ player ││ │                     │                             │   │   │
│  │  │  │ │ .rs    ││ │                     │                             │   │   │
│  │  │  │ ├ pagina-││ │                     │                             │   │   │
│  │  │  │ │ tion.rs││ │                     │                             │   │   │
│  │  │  │ ├ frag-  ││ │                     │                             │   │   │
│  │  │  │ │ ment_  ││ │                     │                             │   │   │
│  │  │  │ │ queue  ││ │                     │                             │   │   │
│  │  │  │ │ .rs    ││ │                     │                             │   │   │
│  │  │  │ └ clip-  ││ │                     │                             │   │   │
│  │  │  │   board_ ││ │                     │                             │   │   │
│  │  │  │   watch  ││ │                     │                             │   │   │
│  │  │  │   .rs    ││ │                     │                             │   │   │
│  │  │  └──────────┘│ │                     │                             │   │   │
│  │  └──────────────┘ └─────────────────────┘                             │   │
│  └────────────────────────────────────────────────────────────────────────┘   │
└──────────────────────────────────────────────────────────────────────────────┘
```

### Manager Pattern

The application uses a **Manager Pattern** where core functionality is organized into independent managers, initialized at startup and managed via Tauri state:

| Manager                 | File                        | Responsibility                                        |
| ----------------------- | --------------------------- | ----------------------------------------------------- |
| `AudioRecordingManager` | `managers/audio.rs`         | Audio capture, device management, recording lifecycle |
| `ModelManager`          | `managers/model.rs`         | Model downloads, verification, lifecycle              |
| `TranscriptionManager`  | `managers/transcription.rs` | STT processing pipeline                               |
| `HistoryManager`        | `managers/history.rs`       | SQLite persistence for transcriptions and TTS         |
| `TTS Manager`           | `tts/manager.rs`            | TTS orchestration: sanitize → paginate → synthesize   |
| `Brain Manager`         | `brain/manager.rs`          | LLM conversation state, turn history, barge-in        |

### Command-Event Architecture

```
Frontend ──tauri command──► Backend (request)
Backend  ──tauri event────► Frontend (notification)
```

- **Frontend → Backend**: Typed IPC via `tauri-specta` generates `src/bindings.ts` from Rust command definitions
- **Backend → Frontend**: Events emitted via `app_handle.emit("event-name", payload)` and listened with `listen()` in React
- **Type Safety**: Both directions are fully typed — if the backend changes a command signature, the TypeScript bindings regenerate with `cargo test export_bindings`

### State Flow

```
User Action → React Component → Zustand Store Mutation
                                    │
                                    ▼
                             Tauri Command (invoke)
                                    │
                                    ▼
                             Rust State Manager
                                    │
                                    ▼
                             Persistence (tauri-plugin-store / SQLite)
                                    │
                                    ▼
                             Event emitted to Frontend
                                    │
                                    ▼
                             Zustand Store Update → React Re-render
```

### Single Instance Architecture

The app uses `tauri_plugin_single_instance` to enforce single-instance behavior:

- Second launch → brings existing window to front
- Remote control via CLI flags: `--toggle-transcription`, `--cancel`, etc.
- Second instance sends args to running instance, then exits
- `signal_handle.rs` provides `send_transcription_input()` for both CLI and Unix signals

---

## 4. The Three Pipelines

### Pipeline 1: Dictation

```
┌──────────┐    ┌──────────────────────────┐    ┌──────────────┐    ┌──────────┐
│ Hotkey   │───►│ Audio Recording           │───►│ TripleVAD    │───►│ Parakeet │
│ Pressed  │    │ (cpal + always-on mic)    │    │ RMS→RNNoise │    │ V3 STT   │
└──────────┘    └──────────────────────────┘    │ →Silero      │    └──────────┘
                                                └──────────────┘         │
                                                                         ▼
┌──────────┐    ┌──────────────────────────┐    ┌──────────────────────┐ │
│ Text     │◄───│ Paste at Cursor          │◄───│ ITN Normalization    │◄┘
│ Pasted!  │    │ (clipboard save/restore, │    │ spoken→written       │
└──────────┘    │  auto-submit optional)   │    └──────────────────────┘
                └──────────────────────────┘
```

**Latency target:** Hotkey release → text pasted < 1.5s for a 10s utterance on mid-range CPU.

### Pipeline 2: Read Aloud

```
┌──────────────┐    ┌──────────────────┐    ┌──────────────────┐    ┌──────────────┐
│ Select Text  │───►│ Capture Text     │───►│ Sanitize:        │───►│ Paginate     │
│ (any app)    │    │ (AX API / Ctrl+C │    │ • Markdown strip │    │ (UTF-8-safe   │
└──────────────┘    │  / Double-copy)  │    │ • TN (written→   │    │  sentence     │
                    └──────────────────┘    │   spoken)        │    │  chunks)      │
                                            │ • Regex cleanup  │    └──────┬───────┘
                                            └──────────────────┘           │
                                                                           ▼
┌──────────────┐    ┌──────────────────┐    ┌──────────────────────────────┐
│ Speaker      │◄───│ Streaming Gapless │◄───│ Synthesize i+1 while i plays│
│ Output       │    │ Playback (rodio)  │    │ (Piper / Kokoro / Engine)   │
└──────────────┘    │ Pause / Resume    │    └──────────────────────────────┘
                    │ Speed / Volume    │
                    └──────────────────┘
```

**Latency target:** Selection → first audio < 700ms (warm Piper/Kokoro).

### Pipeline 3: Conversation (Speech → Brain → Speech)

```
┌──────────┐    ┌──────────────┐    ┌──────────────┐    ┌────────────────┐
│ Hotkey   │───►│ Record       │───►│ VAD           │───►│ Parakeet V3    │
│ / Voice  │    │ (microphone) │    │ (endpoint     │    │ STT            │
│ ────────────────────────────────────────────────────────────────────── │
│                                                                         │
│  IDLE ──► LISTENING ──► TRANSCRIBING ──► THINKING ──► SPEAKING ──► IDLE│
│          (recording)    (STT running)   (LLM)        (TTS playing)      │
│                                                                         │
│  ▲  Barge-in: new speech during SPEAKING → STOP TTS → restart LISTENING│
└─────────────────────────────────────────────────────────────────────────┘
         │                    │                       │
         ▼                    ▼                       ▼
┌────────────────┐    ┌────────────────┐    ┌────────────────────────┐
│ ITN            │───►│ Brain (LLM)    │───►│ Sentence Splitter      │
│ (spoken→       │    │ • Ollama/LM    │    │ (on token stream at    │
│  written)      │    │   Studio       │    │  . ? ! and length      │
└────────────────┘    │ • OpenAI/      │    │  heuristics)           │
                       │   Anthropic   │    └───────────┬────────────┘
                       │ • Streaming   │                │
                       │   SSE tokens  │                ▼
                       └────────────────┘    ┌────────────────────────┐
                                              │ TTS Queue             │
                                              │ • Synthesize sentence │
                                              │   n+1 while n plays   │
                                              │ • Crossfade (planned) │
                                              │ • Barge-in abort      │
                                              └───────────┬────────────┘
                                                           │
                                                           ▼
                                                  ┌────────────────┐
                                                  │ Speaker Output │
                                                  │ (streaming)    │
                                                  └────────────────┘
```

**Latency target:** End-of-speech → first audible reply < 1.5s (local 8B + mid GPU).

---

## 5. STT Subsystem

### Speech-to-Text Engines

| Engine | Type | Languages | Size | Default? | Notes |
| ------------------------ | ---------- | ---------------- | ------- | -------------- | ----------------------------------------- |
| **Parakeet TDT 0.6B V3** | Local ONNX | 25 (auto-detect) | ~456 MB | ✅ **Default** | CPU-optimized, ~5x real-time on mid-range |
| Whisper Small | Local GGML | 99 | ~465 MB | Optional | Fast and fairly accurate |
| Whisper Medium | Local GGML | 99 | ~469 MB | Optional | Good accuracy, medium speed |
| Whisper Turbo | Local GGML | 99 | ~1.5 GB | Optional | Balanced accuracy and speed |
| Whisper Large | Local GGML | 99 | ~1.0 GB | Optional | Good accuracy, but slow |
| Breeze ASR | Local GGML | 99 | ~1.0 GB | Optional | Optimized for Taiwanese Mandarin |
| Moonshine Base | Local ONNX | 1 (en) | ~55 MB | Optional | Very fast, English only, handles accents well |
| SenseVoice | Local ONNX | 5 | ~152 MB | Optional | Very fast; Chinese, English, Japanese, Korean, Cantonese |
| GigaAM v3 | Local ONNX | 1 (ru) | ~151 MB | Optional | Russian speech recognition. Fast and accurate |
| Canary 180M Flash | Local ONNX | 4 | ~146 MB | Optional | Very fast; English, German, Spanish, French. Supports translation |
| Canary 1B v2 | Local ONNX | 25 | ~691 MB | Optional | Accurate multilingual; 25 European languages. Supports translation |
| Cohere | Local ONNX | 16 | ~1.7 GB | Optional | Large, slower, but very accurate multilingual model |
| Nemotron 3.5 ASR | Local ONNX | 36 | ~680 MB | Optional | Multilingual streaming ASR via sherpa-onnx. 80ms chunks |
| Parakeet EOU 120M | Local ONNX | 1 (en) | ~250-500MB| Optional | Streaming RNN-T with end-of-utterance detection |

### STT Implementation

All STT engines are accessed through the `transcribe-rs` crate (and `unified_parakeet.rs` helper), which provides:

- **Parakeet V3**: ONNX-based, NVIDIA NeMo architecture, auto language detection, CPU-optimized quantized (int8).
- **Whisper**: GGML-based through whisper.cpp, supports GPU acceleration (Metal/Vulkan/DirectML/CUDA).
- **Moonshine**: ONNX-based, lightweight, streaming-capable.
- **SenseVoice**: ONNX-based, optimized for fast multilingual voice detection and speech-to-text.
- **GigaAM**: CTC-based Russian speech recognition, highly optimized.
- **Canary**: Transformer-based encoder-decoder model supporting English, Spanish, German, French, and speech-to-text translation to English.
- **Cohere**: High-accuracy large multilingual transcription model.
- **UnifiedParakeet / Nemotron**: RNN-T streaming engines wrapping sherpa-onnx for low-latency end-of-utterance detection and real-time streaming transcripts.


### Key STT Features

- **25 language auto-detection** (Parakeet V3 default) — speak in any supported language, no manual switching
- **GPU acceleration**: DirectML (Windows), Metal (macOS), Vulkan/CUDA (Linux)
- **Model manager**: Download, verify (SHA2), activate, unload
- **Custom words**: Fuzzy correction via `strsim` + `natural` crates for domain jargon
- **TripleVAD**: 3-stage voice activity detection before STT reduces hallucinations
- **ITN post-processing**: Spoken numbers/dates/currency → written form

---

## 6. TTS Subsystem

### TTS Engine Overview

> **Note:** The `WarmEngine` trait in `tts/status.rs` is defined and implemented by all local TTS backends (`PiperBackend`, `KokoroBackend`, `KittenBackend`, `PocketBackend`) to support pre-warming and switch unloads. However, the orchestration layer (`commands/tts.rs` / `manager.rs`) currently interacts with these engines' lifecycles via direct server utilities (`local_tts_server` and `piper_server`) rather than using dynamic/polymorphic dispatch (`&dyn WarmEngine`). Telemetry (`tts/telemetry.rs`) is registered as Tauri state but `record()` is never called — the `chars_per_ms` adaptive sizing infrastructure is in place but not wired to any pipeline.

```
┌─────────────────────────────────────────────────────────────────────┐
│                        TTS Subsystem                                │
│                                                                      │
│  ┌─────────┐    ┌─────────────┐    ┌──────────────────────────────┐ │
│  │ Trigger  │───►│ Sanitize    │───►│ Paginate                     │ │
│  │ • Hotkey │    │ • Markdown  │    │ • UTF-8-safe sentence split  │ │
│  │ • Double-│    │   strip     │    │ • Telemetry-driven adaptive  │ │
│  │   copy   │    │ • TN (writ- │    │   sizing (chars_per_ms)     │ │
│  │ • Manual │    │   ten→spoken│    │ • Shorten first chunk for   │ │
│  └─────────┘    │ • Regex     │    │   low TTFA                   │ │
│                  └─────────────┘    └──────────────┬───────────────┘ │
│                                                     │                │
│                                                     ▼                │
│  ┌──────────────────────────────────────────────────────────────┐   │
│  │  TTS Backend (TtsBackend trait + WarmEngine trait)            │   │
│  │                                                               │   │
│  │  ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌────────┐   │   │
│  │  │ Piper   │ │ Kokoro  │ │ Kitten  │ │ SAPI   │ │ Cloud  │   │   │
│  │  │ (local, │ │ (local, │ │ (local, │ │ (local, │ │ (OpenAI,│   │   │
│  │  │ warm    │ │ 54 voic-│ │ 8 voic- │ │ zero    │ │ Eleven- │   │   │
│  │  │ HTTP    │ │ es, 9   │ │ es, 3   │ │ down-   │ │ Labs,   │   │   │
│  │  │ server) │ │ langs)  │ │ sizes)  │ │ load)   │ │ Carte-  │   │   │
│  │  └─────────┘ └─────────┘ └─────────┘ └─────────┘ │ sia)    │   │   │
│  │                                                   └────────┘   │   │
│  └──────────────────────────────────────────────────────────────┘   │
│                     │                                               │
│                     ▼                                               │
│  ┌──────────────────────────────────────────────────────────────┐   │
│  │  Player (rodio)                                              │   │
│  │  • Streaming gapless playback                                │   │
│  │  • Synthesize chunk i+1 while chunk i plays                  │   │
│  │  • Pause / Resume / Stop                                     │   │
│  │  • Speed (engine-native length_scale)                        │   │
│  │  • Volume control                                            │   │
│  │  • Output device selection                                   │   │
│  └──────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────┘
```

### TTS Backends Comparison

| Backend            | Type                    | Voices       | Languages                              | Quality       | Speed                    | RAM                    | Setup             |
| ------------------ | ----------------------- | ------------ | -------------------------------------- | ------------- | ------------------------ | ---------------------- | ----------------- |
| **Piper**          | Local (persistent HTTP) | 20+ EN, 7 FR | ~20                                    | Good          | **Fastest**              | ~100-200 MB            | Auto-download     |
| **Kokoro-82M**     | Local (in-process ONNX) | 54           | 9 (EN, ES, FR, HI, IT, JA, PT, ZH, KO) | **Excellent** | Fast                     | ~115 MB + 50 MB/worker | Auto-download     |
| **Kitten TTS**     | Local (persistent HTTP) | 8            | EN only                                | Good          | Medium                   | ~25-200 MB             | Auto-download     |
| **Pocket TTS**     | Local (persistent HTTP) | 8 + cloned   | EN                                     | Good          | Medium                   | ~100 MB                | Auto-download     |
| **SAPI**           | OS (Windows)            | Multiple (system) | Multiple (system)                       | Standard      | Fast                     | ~0 MB                  | **Zero download** (fully functional Windows fallback) |
| **OpenAI TTS**     | Cloud API               | 9            | Multiple                               | **Excellent** | Fast (network)           | ~0 MB                  | API key           |
| **ElevenLabs**     | Cloud API               | Many         | 29+                                    | **Excellent** | Fast (network)           | ~0 MB                  | API key           |
| **Cartesia Sonic** | Cloud API               | Many         | Multiple                               | Excellent     | **Lowest latency cloud** | ~0 MB                  | API key           |

**Total: 8 TTS backends** (5 local: Piper, Kokoro, Kitten, Pocket, SAPI; 3 cloud: OpenAI, ElevenLabs, Cartesia).

### TTS Backend Trait

```rust
pub trait TtsBackend: Send + Sync {
    /// Human-readable name for settings UI / logs.
    fn name(&self) -> &str;

    /// Synthesize `text` with `voice` at `speed` into audio bytes.
    fn synthesize(&self, text: &str, voice: &str, speed: f32) -> Result<Vec<u8>, String>;

    /// Check that the engine/server is reachable.
    fn health_check(&self) -> Result<(), String>;

    /// File extension for the bytes returned by [`Self::synthesize`].
    fn file_extension(&self) -> &str {
        "wav"
    }
}

pub trait WarmEngine {
    /// Load the model and run a warm-up inference sentence.
    fn warm(&self) -> Result<(), String>;

    /// Free the model from RAM/VRAM. Called on engine switch or manual unload.
    fn unload(&self) -> Result<(), String>;

    /// Current lifecycle status.
    fn status(&self) -> EngineStatus;

    /// Whether the engine is ready for synthesis.
    fn is_ready(&self) -> bool {
        matches!(self.status(), EngineStatus::Ready)
    }
}
```

### TTS Lifecycle States

> **Note:** The `WarmEngine` trait and `EngineStatus` enum in `tts/status.rs` are fully implemented by all local backends (`PiperBackend`, `KokoroBackend`, `KittenBackend`, `PocketBackend`) to manage model and server lifecycle stages (Stopped → Loading → WarmingUp → Ready). Although the traits are implemented, the main orchestration layer (`commands/tts.rs` / `manager.rs`) communicates directly with the lower-level process and HTTP server helpers (`local_tts_server` and `piper_server`) rather than via dynamic/polymorphic `dyn WarmEngine` dispatch.

```
                    ┌──────────────────────────────────┐
                    │         Engine Status             │
                    │                                  │
                    │    Stopped                        │
                    │       │                           │
                    │       ▼                           │
                    │    Loading ◄── (download if       │
                    │       │        needed)            │
                    │       ▼                           │
                    │    WarmingUp (warm-up synthesis)  │
                    │       │                           │
                    │    ┌──┴──┐                        │
                    │    ▼     ▼                        │
                    │  Ready  Error                     │
                    │    │     │                        │
                    │    ▼     │                        │
                    │  Stopped◄┘                        │
                    └──────────────────────────────────┘
```

### Piper Backend Details

- **Implementation**: Persistent Python `piper.http_server` process, launched via project-local venv Python
- **Warmth**: Model stays in RAM; warm-up synthesis at startup
- **CUDA**: Automatic NVIDIA DLL path discovery, CUDA Execution Provider toggle
- **Voices**: 26 English voices (7 French voices available via manual download)
- **Parameters**: `noise_scale` (0-1.5), `noise_w_scale` (0-1.5) — configurable in settings
- **Edge case**: Child process stdio drained to prevent pipe buffer freeze

### Kokoro Backend Details

- **Implementation**: Persistent HTTP server backend via `kokoro_server.py` — voice listing (54 voices, 9 languages) works; synthesis fully operational via `kokoro_tts` Python API with venv-based Python resolution
- **Pool**: Configurable worker count (auto-tuned from CPU count, 1-4 range)
- **Voices**: 54 voices across 9 languages (US/UK English, Spanish, French, Hindi, Italian, Japanese, Portuguese, Mandarin Chinese)
- **Voice-per-language**: Auto-selection based on detected language
- **Shorten-first-chunk**: Clause-split for fast time-to-first-audio (planned)
- **Crossfade**: 10ms @ 24kHz between chunks (in progress)

### SAPI Backend Details

- **Windows-only** fallback backend (requires zero downloads)
- `list_voices()` retrieves list of installed system SAPI voices dynamically via COM
- **Fully functional** — COM interop fully implemented using `windows-rs`:
  - Allocates global memory for streams (`CreateStreamOnHGlobal`)
  - Instantiates `ISpVoice` and `ISpStream` objects
  - Performs text-to-speech rendering into the memory stream using the correct format GUID (`C31ADBAE-527F-4FF5-A230-F62BB61FF70C`)
  - Packages raw PCM samples into standard 44-byte WAV bytes using an in-memory `pcm_to_wav` helper
- Serves as the primary zero-download local fallback on Windows platforms

### Cloud TTS Backends

- **OpenAI**: `tts-1` and `tts-1-hd` models, 9 voices (alloy, echo, fable, onyx, nova, shimmer, etc.)
- **ElevenLabs**: 11+ formats, voice settings, voice cloning
- **Cartesia Sonic 3.5**: Ultra-low-latency cloud TTS

All cloud backends use pooled `reqwest::Client` with keepalive (60s, ≤2 idle/host).

---

## 7. Brain Subsystem

### What the Brain Does

The Brain is the LLM component that powers the Conversation mode. It:

1. Receives transcribed user speech as text
2. Streams LLM tokens back via SSE (Server-Sent Events)
3. Splits the token stream into sentences at punctuation boundaries
4. Feeds sentences to TTS for real-time read-aloud
5. Maintains multi-turn conversation history
6. Supports barge-in (new speech interrupts TTS)

### Supported Providers

| Provider                   | Type  | Default?   | Auto-discover?          |
| -------------------------- | ----- | ---------- | ----------------------- |
| **Ollama**                 | Local | ✅ Default | ✅ Auto-detect `:11434` |
| **LM Studio**              | Local | ✅         | ✅ Auto-detect `:1234`  |
| llama.cpp server           | Local | Optional   | Via custom URL          |
| OpenAI                     | Cloud | Optional   | API key                 |
| Anthropic                  | Cloud | Optional   | API key                 |
| Gemini                     | Cloud | Optional   | API key                 |
| Groq                       | Cloud | Optional   | API key                 |
| OpenRouter                 | Cloud | Optional   | API key                 |
| Cerebras                   | Cloud | Optional   | API key                 |
| Custom (OpenAI-compatible) | Any   | Optional   | Custom base URL         |

### Brain Implementation

- **Client**: `brain/client.rs` (495 lines) — SSE streaming chat client with token delta accumulation and `SentenceSplitter`
- **Manager**: `brain/manager.rs` (314 lines) — Turn history, abort (barge-in), sentence → TTS bridge, multimodal support (audio + image for Gemma 4)
- **Llama Manager**: `brain/llama_manager.rs` (356 lines) — Llama.cpp server process management, Gemma-4 GGUF model download, MTP speculative decoding (n=13, ~216 tok/s), multimodal projector conditionally loaded
- **Sentence splitter**: Accumulates tokens, emits at `. ? !` + length heuristics
- **Multi-turn memory**: Configurable context window with oldest-first trimming (default: 20 turns)
- **System prompt**: Configurable per conversation + separate `speakable_output_prompt` when read-aloud toggled
- **10 LLM providers**: openai, z.ai, google_ai_studio, openrouter, anthropic, groq, cerebras, bedrock_mantle, llama_cpp, custom (+ Apple Intelligence on macOS aarch64)

### Conversation Mode Features

- **Push-to-talk**: Hold hotkey to record, release to transcribe
- **Toggle mode**: Press to start recording, press again to stop
- **Barge-in**: New speech during TTS playback → stop playback → start new turn
- **TTS toggle**: Per-conversation, default ON
- **Text fallback**: Type your message if voice isn't feasible
- **Live transcript**: Streaming tokens visible in conversation view
- **History**: All turns persisted to SQLite

---

## 8. Voice Activity Detection

### TripleVAD Architecture

```
                    TripleVAD — 3-Stage Voice Activity Detection

Microphone (16kHz, 480-sample frames @ 30fps)
         │
         ▼
 ┌─────────────────┐
 │ Stage 1: RMS    │  Energy Gate
 │ Energy Gate     │  • Threshold: 0.002
 │                 │  • Purpose: Reject absolute silence immediately
 │                 │  • Cost: < 0.01ms per frame
 └────────┬────────┘
          │ (passed if RMS > 0.002)
          ▼
 ┌─────────────────┐
 │ Stage 2: RNNoise│  Voice Probability Gate
 │ Voice Prob.     │  • Threshold: configurable 0.05-0.9 (default 0.2)
 │                 │  • Denoises audio (ns_enabled=true)
 │                 │  • Passes voice probability to Stage 3
 │                 │  • Cost: ~1ms per frame
 └────────┬────────┘
          │ (passed if voice_prob > threshold)
          ▼
 ┌─────────────────┐
 │ Stage 3: Silero │  Neural VAD Confirmation
 │ VAD (ONNX)      │  • Threshold: 0.3
 │                 │  • Final arbiter — must confirm speech
 │                 │  • Cost: ~1ms per frame
 │                 │  • Avoids false positives from non-speech
 └────────┬────────┘
          │ (confirmed speech)
          ▼
 ┌─────────────────┐
 │ VAD Decision    │
 │ Speech / Noise  │  Total pipeline: ~2ms per frame
 └─────────────────┘
```

### VAD Modes

| Mode                    | Pipeline               | Latency    | Use Case                            |
| ----------------------- | ---------------------- | ---------- | ----------------------------------- |
| **TripleVAD** (default) | RMS → RNNoise → Silero | ~2ms/frame | Best noise rejection, conversation  |
| Silero only             | Silero VAD             | ~1ms/frame | Lower latency, cleaner environments |
| Push-to-talk            | None (manual)          | 0          | User controls start/stop            |

### RNNoise Integration

- **Crate**: `nnnoiseless` 0.5.2
- **Purpose**: Real-time noise suppression before VAD and STT
- **Architecture**: 16kHz input → upsample to 48kHz → RNNoise process → downsample to 16kHz
- **Toggle**: Configurable in Settings → Advanced → Audio Enhancements
- **Threshold slider**: RNNoise voice probability threshold (0.05–0.9, default 0.2)

---

## 9. Text Normalization Pipeline

### 5-Stage Text Normalization Pipeline

```
┌─────────────────────────────────────────────────────────────────────┐
│                    Text Normalization Pipeline                       │
│                                                                      │
│  Post-STT Path (Dictation / Conversation):                          │
│                                                                      │
│  STT Output (spoken form)                                            │
│         │                                                            │
│         ▼                                                            │
│  ┌─────────────────┐                                                │
│  │ Pass 1: ITN     │  spoken → written                              │
│  │ text-processing │  "two hundred" → "200"                         │
│  │ -rs             │  "january fifth" → "January 5"                 │
│  └────────┬────────┘  "$5.50" → "$5.50" (already correct)          │
│           │                                                          │
│           ▼                                                          │
│  ┌─────────────────┐                                                │
│  │ Pass 2: Custom  │  Fuzzy word correction                         │
│  │ Words           │  "open ai" → "OpenAI"                          │
│  │ strsim + natural│  Domain-specific terms                         │
│  └────────┬────────┘                                                │
│           │                                                          │
│           ▼                                                          │
│       Brain (LLM) — only in Conversation mode                      │
│           │                                                          │
│           ▼                                                          │
│                                                                      │
│  Pre-TTS Path (Read Aloud / Conversation reply):                   │
│                                                                      │
│  Brain Output (written form, may contain markdown)                  │
│         │                                                            │
│         ▼                                                            │
│  ┌─────────────────┐                                                │
│  │ Pass 3: Markdown│  Regex-based stripping                         │
│  │ Strip (regex)   │  # Title → "Title."                            │
│  │                 │  **bold** → "bold"                             │
│  └────────┬────────┘  `code` → "code: code"                         │
│           │           URLs simplified                                │
│           ▼                                                          │
│  ┌─────────────────┐                                                │
│  │ Pass 4: TN      │  written → spoken                              │
│  │ text-processing │  "123" → "one hundred twenty three"            │
│  │ -rs             │  "$5.50" → "five dollars and fifty cents"     │
│  └────────┬────────┘  "Dr. Smith" → "doctor Smith"                  │
│           │                                                          │
│           ▼                                                          │
│  ┌─────────────────┐                                                │
│  │ Pass 5: Regex   │  Final artifact scrub                          │
│  │ Cleanup         │  Remove extra whitespace                       │
│  │ cleanup.rs      │  Handle edge cases                             │
│  └────────┬────────┘                                                │
│           │                                                          │
│           ▼                                                          │
│      TTS Engine (clean, speakable text)                             │
└─────────────────────────────────────────────────────────────────────┘
```

### Pass Details

**Pass 1: ITN (Inverse Text Normalization)**

- **Crate**: `text-processing-rs` (Apache 2.0)
- **Purpose**: Convert ASR-style spoken text to proper written form
- **Toggle**: Settings → Dictation → "Normalize spoken numbers (ITN)"
- **Coverage**: 98.6% of NVIDIA NeMo test suite (1200/1217 tests)
- **Categories**: cardinal, ordinal, decimal, money, measurements, dates, time, email/URL, telephone/IP, whitelist

**Pass 2: Custom Words (Fuzzy Correction)**

- **Purpose**: Domain-specific term correction
- **Libraries**: `strsim` (string similarity), `natural` (NLP utilities)
- **Use case**: Correct "handee" → "Handy", "pie per" → "Piper", "ollamaa" → "Ollama"

**Pass 3: Markdown Strip**

- **Method**: Regex-based stripping (replaced `pulldown-cmark`)
- **Purpose**: Convert markdown to natural spoken text
- **Toggle**: Settings → Voice → "Strip markdown before speaking"
- **Handles**: Headings, bold/italic, lists, links, code blocks, HTML entities, URLs

**Pass 4: TN (Text Normalization)**

- **Crate**: `text-processing-rs` (same crate, different mode)
- **Purpose**: Convert written text to spoken form before TTS
- **Toggle**: Settings → Voice → "Normalize written text (TN)"
- **Handles**: Numbers, dates, currency, time, measurements, abbreviations

**Pass 5: Regex Cleanup (Fallback)**

- **Purpose**: Final scrub pass — remove artifacts, normalize whitespace
- **Kept from**: Original CopySpeak sanitizer + S2B2S extensions

---

## 10. Audio Processing Toolkit

The `audio_toolkit` module provides low-level audio processing:

### Audio Capture (`audio_toolkit/audio/`)

| Component         | File                   | Purpose                                   |
| ----------------- | ---------------------- | ----------------------------------------- |
| `Device`          | `device.rs`            | Microphone/output device enumeration      |
| `Recorder`        | `recorder.rs`          | Real-time audio capture via cpal callback |
| `Resampler`       | `resampler.rs`         | Sample rate conversion (rubato)           |
| `Visualizer`      | `visualizer.rs`        | FFT-based audio visualization (rustfft)   |
| `NoiseSuppressor` | `noise_suppression.rs` | RNNoise denoising (nnnoiseless)           |
| `Utils`           | `utils.rs`             | Audio format helpers                      |

### Voice Activity Detection (`audio_toolkit/vad/`)

| Component     | File            | Purpose                                 |
| ------------- | --------------- | --------------------------------------- |
| `SileroVad`   | `silero.rs`     | Neural VAD via ONNX (vad-rs)            |
| `SmoothedVad` | `smoothed.rs`   | Temporal smoothing of VAD output        |
| `TripleVad`   | `triple_vad.rs` | 3-stage cascade: RMS → RNNoise → Silero |

### Audio Playback (`tts/player.rs`)

- Uses `rodio` 0.22 for streaming audio output
- Gapless playback: synthesize chunk i+1 while chunk i plays
- Pause / Resume / Stop controls
- Speed control (engine-native length_scale)
- Volume control
- Output device selection
- Windows 200ms preroll for smooth startup

---

## 11. Model Management

### Model Manager (`managers/model.rs`)

The ModelManager handles:

- **Download**: Resumable HTTP downloads with progress events
- **Verification**: SHA2 checksum validation after download
- **Extraction**: tar.gz extraction for Parakeet models
- **Persistence**: Models stored in app data directory
- **Lifecycle**: Load, warm-up, unload, timeout
- **Discovery**: Auto-detection of downloaded models
- **Custom models**: User-placed Whisper GGML files auto-discovered

### Models Available

**STT Models:**
| Model | File | Size | Download URL |
|-------|------|------|-------------|
| Silero VAD | `silero_vad_v4.onnx` | ~5 MB | `https://blob.handy.computer/silero_vad_v4.onnx` |
| Parakeet V2 | `parakeet-v2-int8.tar.gz` | 473 MB | `https://blob.handy.computer/parakeet-v2-int8.tar.gz` |
| Parakeet V3 | `parakeet-v3-int8.tar.gz` | 478 MB | `https://blob.handy.computer/parakeet-v3-int8.tar.gz` |
| Whisper Small | `ggml-small.bin` | 487 MB | `https://blob.handy.computer/ggml-small.bin` |
| Whisper Medium | `whisper-medium-q4_1.bin` | 492 MB | `https://blob.handy.computer/whisper-medium-q4_1.bin` |
| Whisper Turbo | `ggml-large-v3-turbo.bin` | 1600 MB | `https://blob.handy.computer/ggml-large-v3-turbo.bin` |
| Whisper Large | `ggml-large-v3-q5_0.bin` | 1100 MB | `https://blob.handy.computer/ggml-large-v3-q5_0.bin` |

**TTS Models:** (Auto-downloaded on first TTS use)
| Model | Components | Size |
|-------|-----------|------|
| Kokoro-82M | ONNX model + voice data | ~115 MB |
| Piper voices | per-voice ONNX + config | ~50-200 MB per voice |
| Kitten TTS | ONNX model | ~25-200 MB |

### Model Directory Structure

```
{app_data_dir}/models/
├── silero_vad_v4.onnx
├── ggml-small.bin (optional Whisper)
├── parakeet-tdt-0.6b-v3-int8/   (extracted Parakeet V3)
│   ├── model.onnx
│   ├── config.json
│   └── tokenizer.json
└── piper-voices/                 (Piper voice files)
    ├── en_US-lessac-medium.onnx
    ├── en_US-lessac-medium.onnx.json
    ├── fr_FR-siwis-medium.onnx
    └── ...
```

---

## 12. Settings & Persistence

### Settings System

- **Backend**: `settings.rs` manages the full settings struct with `TtsConfig`, `BrainConfig`, `SanitizeConfig`
- **Storage**: `tauri-plugin-store` for JSON persistence
- **Reactivity**: Zustand store manages frontend state; Tauri commands sync to Rust
- **Backfill**: Settings-bindings backfill on read — new settings get defaults automatically

### Major Setting Categories

| Category               | Settings                                                                       |
| ---------------------- | ------------------------------------------------------------------------------ |
| **General**            | Language, Start hidden, Auto-start, Minimize to tray                           |
| **Sound**              | Microphone, Output device, Always-on mic, Mute while recording                 |
| **Shortcuts**          | Dictation, Read aloud, Conversation, Cancel, Toggle post-processing            |
| **Models**             | STT engine (Parakeet/Whisper/Moonshine), GPU acceleration                      |
| **TTS**                | Engine, Voice, Speed, Volume, Workers, Shorten first chunk, Piper noise params |
| **Brain**              | Provider, Endpoint URL, Model, System prompt, Memory length, Read-aloud toggle |
| **Advanced**           | VAD mode, RNNoise threshold, Noise suppression, Paste method, Overlay position |
| **Audio Enhancements** | TripleVAD toggle, RNNoise threshold slider, NS toggle                          |
| **History**            | Retention policy (time-based, count-based)                                     |

### History Database (SQLite)

- **Library**: `rusqlite` 0.40 with `rusqlite_migration`
- **Schema**: Entries with `entry_type` (STT / TTS), `model_name`, `model_info`, `duration_ms`
- **Features**: Delete All, type badges, per-entry audio replay
- **Storage**: Transcription text, TTS audio files, metadata

---

## 13. Frontend Architecture

### Technology Stack

| Technology   | Version | Purpose                 |
| ------------ | ------- | ----------------------- |
| React        | 19      | UI framework            |
| TypeScript   | 6       | Type safety             |
| Vite         | 8       | Build tool / dev server |
| Tailwind CSS | 4       | Styling                 |
| Zustand      | 5       | State management        |
| i18next      | 26      | Internationalization    |
| Three.js     | 0.184   | 3D loading animation    |
| Lucide React | Latest  | Icons                   |
| Sonner       | 2       | Toast notifications     |
| Zod          | 4       | Schema validation       |

### UI Components

```
src/
├── components/             # React UI components
│   ├── conversation/       # Conversation mode UI
│   │   └── ConversationView.tsx
│   ├── settings/           # ~45 components across 10 subdirs
│   │   ├── about/          # About page (version, credits)
│   │   ├── advanced/       # Audio enhancements, long audio
│   │   ├── brain/          # Brain/LLM settings
│   │   ├── debug/          # Log viewer, path display
│   │   ├── general/        # General settings
│   │   ├── history/        # History panel (infinite scroll)
│   │   ├── models/         # Model management
│   │   ├── post-processing/# PP settings
│   │   ├── PostProcessingSettingsApi/  # API config
│   │   ├── speech/         # TTS settings
│   │   └── *Settings.tsx   # ~30 individual setting components
│   ├── model-selector/     # Model management UI
│   ├── onboarding/         # First-run (Onboarding, ModelCard, Access)
│   ├── update-checker/     # Update notifications
│   ├── shared/             # Shared components (ProgressBar)
│   ├── ui/                 # 17 UI primitives (Button, Input, Select, etc.)
│   ├── icons/              # 6 SVG icon components
│   ├── footer/             # Footer bar (Brain, TTS, STT, GPU VRAM)
│   ├── Sidebar.tsx         # Navigation sidebar
│   ├── HerLoading.tsx      # 3D loading animation
│   └── AccessibilityPermissions.tsx
├── overlay/                # Recording overlay window (separate entry)
│   ├── main.tsx
│   ├── RecordingOverlay.tsx
│   └── RecordingOverlay.css
│
├── hooks/                  # React hooks
│   ├── useSettings.ts      # Settings state hook
│   ├── useOsType.ts        # OS detection hook
│   └── useProviderState.ts # Shared provider state hook
│
├── stores/                 # Zustand state stores
│   ├── settingsStore.ts    # Application settings
│   └── modelStore.ts       # Model lifecycle state
│
├── i18n/                   # Internationalization (20 languages)
│   ├── index.ts
│   ├── languages.ts
│   └── locales/{lang}/translation.json
│
├── lib/                    # Utilities and types
│   ├── constants/languages.ts
│   ├── types/events.ts
│   └── utils/ (rtl, keyboard, format, modelTranslation)
│
└── utils/dateFormat.ts     # Date formatting utilities
```

### Theme

- **Background**: Pure black (#000000)
- **Primary accent**: Purple (#7c3aed)
- **Secondary accent**: Gold (#f59e0b)
- **Loading animation**: Her-style 3D tube geometry (lissajous curve) with ring-reveal transition
- **Minimum splash time**: 3 seconds
- **Startup greeting**: Played at 0.9x speed

---

## 14. Internationalization

### Supported Languages (20)

| Code | Language         | Code    | Language              |
| ---- | ---------------- | ------- | --------------------- |
| `ar` | Arabic           | `ko`    | Korean                |
| `bg` | Bulgarian        | `pl`    | Polish                |
| `cs` | Czech            | `pt`    | Portuguese            |
| `de` | German           | `ru`    | Russian               |
| `en` | English (source) | `sv`    | Swedish               |
| `es` | Spanish          | `tr`    | Turkish               |
| `fr` | French           | `uk`    | Ukrainian             |
| `he` | Hebrew           | `vi`    | Vietnamese            |
| `it` | Italian          | `zh`    | Chinese (Simplified)  |
| `ja` | Japanese         | `zh-TW` | Chinese (Traditional) |

### i18n Architecture

- **Library**: i18next 26 + react-i18next 17
- **Enforcement**: ESLint plugin prevents hardcoded strings in JSX
- **Validation**: `bun run check:translations` verifies completeness
- **RTL support**: Arabic and Hebrew with proper layout direction
- **Tray labels**: i18n-generated from translation files (build-time)

---

## 15. CI/CD & Build System

### GitHub Actions Workflows

| Workflow      | File                                  | Purpose                              |
| ------------- | ------------------------------------- | ------------------------------------ |
| Test          | `.github/workflows/test.yml`          | Unit tests + lint on push/PR         |
| Build         | `.github/workflows/build.yml`         | Build on Windows/macOS/Linux         |
| Build Test    | `.github/workflows/build-test.yml`    | Build + test                         |
| Release       | `.github/workflows/release.yml`       | Manual release with platform bundles |
| Playwright    | `.github/workflows/playwright.yml`    | E2E tests                            |
| Code Quality  | `.github/workflows/code-quality.yml`  | ESLint + Prettier + Clippy           |
| PR Test Build | `.github/workflows/pr-test-build.yml` | PR verification                      |
| Main Build    | `.github/workflows/main-build.yml`    | Main branch build                    |
| Nix Check     | `.github/workflows/nix-check.yml`     | Nix flake validation                 |

### Build Targets

| Platform                      | Installer          | Notes                             |
| ----------------------------- | ------------------ | --------------------------------- |
| Windows x64                   | NSIS (.exe), MSI   | Common-Controls v6 manifest       |
| macOS (Intel + Apple Silicon) | DMG                | Hardened runtime, signed          |
| Linux x64                     | deb, rpm, AppImage | Nix flake for reproducible builds |

### Nix Support

A `flake.nix` provides reproducible builds on Linux (NixOS):

- Uses `bun2nix` for dependency management
- Declares common native dependencies (webkitgtk, gtk3, alsa-lib, onnxruntime, etc.)
- Supports x86_64-linux and aarch64-linux

---

## 16. Project Lineage & Donor Map

### Where S2B2S Comes From

```
                    cjpais/Handy (MIT)
                  STT skeleton, Tauri 2
                  ~~~~~~~~~~~~~~~~~~~~~~
                         │
            ┌────────────┼────────────────┐
            │            │                │
            ▼            ▼                ▼
       AIVORelay      Parler           Parrot
       (MaxITService) (Melvynx)        (Rishi Khare)
       Windows++,     Gemini STT,     STT → TTS inversion
       streaming,     long-audio,     Kokoro in-process,
       profiles,      crash logging   54 voices, crossfade
       browser relay  settings exp/imp
            │
            │             independent
            │             ~~~~~~~~~~~~
            │             CopySpeak TTS (MIT)
            │             (ilyaizen → NairoDorian)
            │             6 TTS engines, Piper server,
            │             double-copy, HUD, control API
            │
            └──────────┬──────────────┘
                       │
                       ▼
            ┌─────────────────────┐
            │       S2B2S         │
            │ SpeechToBrainToSpeech│
            │ STT + Brain + TTS   │
            │ All three pipelines │
            └─────────────────────┘
```

### Feature Donor Map

| Feature                                          | Donor                      | How Used                  |
| ------------------------------------------------ | -------------------------- | ------------------------- |
| Tauri skeleton, managers, audio_toolkit          | Handy                      | Core architecture         |
| Streaming STT, profiles, RNNoise, keyring        | AIVORelay                  | Infrastructure            |
| Gemini STT, long-audio routing, crash logging    | Parler                     | Ported features           |
| Kokoro TTS, crossfade, markdown stripping (regex) | Parrot                     | Core TTS engine           |
| TtsBackend trait, Piper server, double-copy, HUD | CopySpeak                  | TTS product patterns      |
| ITN/TN (text-processing-rs)                      | FluidInference             | Text normalization        |
| Provider matrix, transformations concept         | Whispering **AGPL**        | Concept only — no code    |
| Persist-before-deliver, LAN GPU pattern          | TranscriptionSuite **GPL** | Concept only — no code    |
| Streaming params (0.8s pause, 3s overlap)        | Parakeet-RT **no license** | Parameters only — no code |

### License Compliance

- All code contributions are MIT (Handy, AIVORelay, Parler, Parrot, CopySpeak)
- AGPL (Whispering) and GPL (TranscriptionSuite) projects are **concept donors only** — zero code crosses the license boundary
- Unlicensed (Parakeet-RT) — parameters and ideas only, no code
- Model licenses: Parakeet V3 (CC-BY-4.0), Kokoro-82M (Apache 2.0), Piper voices (various)

---

## 17. Dependency Analysis

### Rust Dependencies (Key Crates)

| Crate                         | Version | Purpose                   | License    |
| ----------------------------- | ------- | ------------------------- | ---------- |
| `tauri` (+ tray-icon, image)  | 2.11    | Desktop framework + tray  | MIT/Apache |
| `tauri-specta` / `specta`     | rc.25   | Typed IPC                 | MIT        |
| `transcribe-rs`               | 0.3.11  | STT inference (Parakeet)  | MIT        |
| `cpal`                        | 0.17    | Audio I/O                 | MIT/Apache |
| `rodio`                       | 0.22    | Audio playback            | MIT        |
| `rubato`                      | 3.0     | Audio resampling          | MIT        |
| `nnnoiseless`                 | 0.5.2   | RNNoise noise suppression | MIT/Apache |
| `vad-rs`                      | —       | Silero VAD                | MIT        |
| `rdev`                        | —       | Global shortcuts          | MIT        |
| `reqwest`                     | 0.13    | HTTP client               | MIT/Apache |
| `rusqlite` + `rusqlite_migration` | 0.40 | SQLite + migrations       | MIT        |
| `text-processing-rs`          | 0.2.2   | ITN/TN normalization      | Apache 2.0 |
| `regex`                       | 1.12    | Markdown stripping        | MIT/Apache |
| `enigo`                       | 0.6     | Keyboard simulation       | MIT        |
| `serde` + `serde_json`        | 1.0     | Serialization             | MIT        |
| `tokio`                       | 1.52    | Async runtime             | MIT        |
| `windows`                     | 0.62    | Win32 API (Win only)      | MIT        |
| `anyhow`                      | 1.0     | Error handling            | MIT        |
| `log` + `env_filter`          | 0.4/1.0 | Logging                   | MIT        |
| `strsim`                      | 0.11    | String similarity         | MIT        |
| `natural`                     | 0.5     | NLP utilities             | MIT        |
| `chrono`                      | 0.4     | Date/time handling        | MIT/Apache |
| `hound`                       | 3.5     | WAV audio I/O             | MIT        |
| `flate2` + `tar`              | 1.1/0.4 | GZIP archive extraction   | MIT/Apache |
| `sha2`                        | 0.11    | SHA-256 model verification | MIT        |
| `rustfft`                     | 6.4     | FFT-based visualization   | MIT        |
| `ferrous-opencc`              | 0.4     | Chinese text conversion   | MIT        |
| `clap`                        | 4.6     | CLI argument parsing      | MIT/Apache |
| `handy-keys`                  | 0.2     | Keyboard key maps         | MIT        |
| `audioadapter` (+ buffers)    | 3.0     | Audio buffer utilities    | MIT        |
| `once_cell`                   | 1.21    | Lazy statics              | MIT        |
| `futures-util`                | 0.3     | Async stream utilities    | MIT/Apache |

**Tauri Plugin Suite** (14 plugins, all MIT):

| Plugin                        | Purpose                               |
| ----------------------------- | ------------------------------------- |
| `tauri-plugin-log`            | Rust log forwarding to frontend       |
| `tauri-plugin-store`          | Settings persistence                  |
| `tauri-plugin-opener`         | Open URLs/files                      |
| `tauri-plugin-os`             | OS type detection                     |
| `tauri-plugin-clipboard-manager` | Clipboard read/write              |
| `tauri-plugin-dialog`         | Native file dialogs                   |
| `tauri-plugin-fs`             | Filesystem access                     |
| `tauri-plugin-process`        | Process management                    |
| `tauri-plugin-global-shortcut`| Tauri-managed global shortcuts        |
| `tauri-plugin-autostart`      | Launch on login (Unix)               |
| `tauri-plugin-single-instance`| Single-instance enforcement           |
| `tauri-plugin-updater`        | App update mechanism                  |
| `tauri-plugin-macos-permissions` | macOS permission APIs (macOS only) |
| `tauri-nspanel`               | macOS panel window support (macOS)    |

### JavaScript/TypeScript Dependencies (Key)

| Package                     | Version | Purpose              | License    |
| --------------------------- | ------- | -------------------- | ---------- |
| `@tauri-apps/api`           | 2.11    | Tauri frontend API   | MIT        |
| `react` / `react-dom`       | 19      | UI framework         | MIT        |
| `i18next` / `react-i18next` | 26/17   | Internationalization | MIT        |
| `zustand`                   | 5       | State management     | MIT        |
| `zod`                       | 4       | Schema validation    | MIT        |
| `three`                     | 0.184   | 3D graphics          | MIT        |
| `tailwindcss` / `@tailwindcss/vite` | 4 | CSS framework   | MIT        |
| `lucide-react`              | 1       | Icons                | ISC        |
| `sonner`                    | 2       | Toasts               | MIT        |
| `immer`                     | 11      | Immutable state      | MIT        |
| `react-select`              | 5       | Enhanced dropdowns   | MIT        |
| `vite`                      | 8       | Build tool           | MIT        |
| `typescript`                | 6       | TypeScript           | Apache 2.0 |
| `eslint`                    | 10      | Linting              | MIT        |
| `prettier`                  | 3       | Formatting           | MIT        |
| `playwright`                | 1.60    | E2E testing          | Apache 2.0 |

**Tauri Plugin JS Bindings:** `@tauri-apps/plugin-{autostart,clipboard-manager,dialog,fs,global-shortcut,opener,os,process,sql,store,updater}` plus `tauri-plugin-macos-permissions-api`.

---

## 18. File Structure Map

```
S2B2S/
│
├── 📄 README.md                  # Project overview & quick start
├── 📄 S2B2S_REVIEW.md            # THIS FILE — complete analysis
├── 📄 AGENTS.md                  # AI coding assistant guidance
├── 📄 BUILD.md                   # Build instructions
├── 📄 CHANGELOG.md               # Version history
├── 📄 CONTRIBUTING.md            # Contribution guide
├── 📄 CONTRIBUTING_TRANSLATIONS.md # Translation guide
├── 📄 CRUSH.md                   # Dev commands quick reference
├── 📄 CLAUDE.md                  # AI entry point
├── 📄 LICENSE                    # MIT
├── 📄 package.json               # JS deps & scripts
├── 📄 index.html                 # HTML entry point
├── 📄 vite.config.ts             # Vite configuration
├── 📄 tailwind.config.js         # Tailwind CSS
├── 📄 tsconfig.json              # TypeScript config
├── 📄 tsconfig.node.json         # Node TypeScript config
├── 📁 analysys/                  # Evolution planning documents
│   ├── 📄 00_OVERVIEW.md          # Vision: GPU overlay, Conversation 2.0, Avatar "Orbi"
│   ├── 📄 01_REPO_REVIEW.md       # Current codebase audit for overlay/avatar work
│   ├── 📄 02_GPU_OVERLAY_ARCHITECTURE.md  # Cross-platform transparent overlay design
│   ├── 📄 03_CONVERSATION_MODE_2.md  # Conversation Mode 2.0 UX spec
│   ├── 📄 04_AVATAR_SPEC.md       # Avatar "Orbi" visual design & state machine
│   └── 📄 05_IMPLEMENTATION_ROADMAP.md  # Phased implementation plan (Phases 0-4)
│
├── 📄 playwright.config.ts       # E2E test config
├── 📄 .gitignore                 # Git ignore
├── 📄 .prettierrc / .prettierignore  # Prettier config
├── 📄 flake.nix / flake.lock     # Nix flake (Linux builds)
│
├── 📁 src/                       # Frontend (React/TypeScript)
│   ├── 📄 App.tsx                # Main app component
│   ├── 📄 main.tsx               # Entry point
│   ├── 📄 App.css                # Global styles
│   ├── 📄 bindings.ts            # Auto-generated Tauri bindings
│   │
│   ├── 📁 components/
│   │   ├── 📁 conversation/      # Conversation mode UI
│   │   ├── 📁 settings/          # Settings panels
│   │   ├── 📁 model-selector/    # Model management
│   │   ├── 📁 onboarding/        # First-run experience
│   │   ├── 📁 overlay/           # Recording overlay
│   │   ├── 📁 update-checker/    # Update notifications
│   │   ├── 📁 shared/            # Shared utilities
│   │   ├── 📁 ui/                # UI primitives
│   │   ├── 📁 icons/             # Icons
│   │   ├── 📁 footer/            # Status footer
│   │   ├── 📄 Sidebar.tsx        # Navigation
│   │   ├── 📄 HerLoading.tsx     # 3D loading
│   │   └── 📄 AccessibilityPermissions.tsx
│   │
│   ├── 📁 hooks/                 # React hooks
│   │   ├── 📄 useSettings.ts     # Settings state hook
│   │   ├── 📄 useOsType.ts       # OS detection hook
│   │   ├── 📄 useProviderState.ts # Shared provider state hook
│   │   └── 📄 useLlamaState.ts   # Llama.cpp server & VRAM state
│   │
│   ├── 📁 stores/                # Zustand stores
│   │   ├── 📄 settingsStore.ts   # App settings (785 lines)
│   │   └── 📄 modelStore.ts      # Model lifecycle state
│   │
│   ├── 📁 i18n/                  # Internationalization
│   │   ├── 📄 index.ts           # i18n setup
│   │   ├── 📄 languages.ts       # Language metadata
│   │   └── 📁 locales/           # 20 translation files
│   │       ├── 📁 en/
│   │       ├── 📁 fr/
│   │       └── ...
│   │
│   ├── 📁 lib/                   # Utilities & types
│   │   ├── 📁 constants/
│   │   │   └── 📄 languages.ts  # STT language constants
│   │   ├── 📁 types/
│   │   │   └── 📄 events.ts     # Tauri event type defs
│   │   └── 📁 utils/
│   │       ├── 📄 rtl.ts        # RTL text direction
│   │       ├── 📄 keyboard.ts   # Keyboard event normalization
│   │       ├── 📄 format.ts     # Model size formatting
│   │       └── 📄 modelTranslation.ts  # Model name i18n
│   │
│   ├── 📁 overlay/               # Overlay window (separate entry)
│   │   ├── 📄 main.tsx           # Overlay entry point
│   │   ├── 📄 index.html         # Overlay HTML
│   │   ├── 📄 RecordingOverlay.tsx # Recording/speaking overlay
│   │   └── 📄 RecordingOverlay.css # Overlay styles
│   │
│   ├── 📁 assets/                # Static assets
│   │   ├── 📄 logo.png           # App logo
│   │   └── 📄 icon.png           # App icon
│   │
│   └── 📁 utils/
│       └── 📄 dateFormat.ts     # Date formatting utilities
│
├── 📁 src-tauri/                 # Backend (Rust)
│   ├── 📁 src/                   # Rust source code
│   │   ├── 📄 lib.rs             # Main entry, Tauri setup
│   │   ├── 📄 main.rs            # Binary entry
│   │   ├── 📄 actions.rs         # Shortcut actions
│   │   ├── 📄 cli.rs             # CLI argument definitions
│   │   ├── 📄 settings.rs        # App settings
│   │   ├── 📄 signal_handle.rs   # Shared IPC logic
│   │   ├── 📄 utils.rs           # Platform helpers
│   │   ├── 📄 overlay.rs         # Recording overlay
│   │   ├── 📄 clipboard.rs       # Clipboard ops
│   │   ├── 📄 input.rs           # Keyboard input
│   │   ├── 📄 audio_feedback.rs  # Sound effects
│   │   ├── 📄 control_server.rs  # Local HTTP API
│   │   ├── 📄 crash_logging.rs   # Panic capture
│   │   ├── 📄 portable.rs        # Portable mode
│   │   ├── 📄 tray.rs            # System tray
│   │   ├── 📄 tray_i18n.rs       # Tray i18n
│   │   ├── 📄 llm_client.rs      # Multi-provider LLM
│   │   ├── 📄 transcription_coordinator.rs  # Record→paste orchestration
│   │   ├── 📄 active_app.rs      # Foreground app detection
│   │   ├── 📄 apple_intelligence.rs  # macOS Apple Intel.
│   │   │
│   │   ├── 📁 managers/          # Core business logic
│   │   │   ├── 📄 mod.rs         # Module declarations
│   │   │   ├── 📄 audio.rs       # Audio recording, device mgmt
│   │   │   ├── 📄 model.rs       # Model download, verification
│   │   │   ├── 📄 transcription.rs  # STT pipeline (886 lines)
│   │   │   ├── 📄 transcription_mock.rs  # CI mock
│   │   │   ├── 📄 history.rs     # SQLite persistence
│   │   │   └── 📄 continuous_voice.rs  # Voice mode mgmt
│   │   │
│   │   ├── 📁 tts/               # TTS subsystem
│   │   │   ├── 📄 mod.rs         # TtsBackend trait + Voice
│   │   │   ├── 📄 manager.rs     # Sanitize→Paginate→Synthesize
│   │   │   ├── 📄 player.rs      # Streaming gapless playback
│   │   │   ├── 📄 pagination.rs  # UTF-8-safe text chunking
│   │   │   ├── 📄 fragment_queue.rs  # ⚠️ 306 lines dead code (preserved for future use)
│   │   │   ├── 📄 clipboard_watch.rs  # Double-copy trigger
│   │   │   ├── 📄 audio_format.rs # WAV→MP3/OGG/FLAC conversion
│   │   │   ├── 📄 telemetry.rs   # TTS perf (chars_per_ms)
│   │   │   ├── 📄 status.rs      # Engine status reporting
│   │   │   ├── 📁 backends/      # 9 engine implementations (piper, piper_server, kokoro, kitten, pocket, sapi, openai, elevenlabs, cartesia)
│   │   │   │   ├── 📄 piper.rs        # Piper HTTP client
│   │   │   │   ├── 📄 piper_server.rs # Piper server lifecycle
│   │   │   │   ├── 📄 kokoro.rs       # Kokoro-82M ONNX TTS
│   │   │   │   ├── 📄 kitten.rs       # Kitten TTS (persistent HTTP server)
│   │   │   │   ├── 📄 sapi.rs         # Windows SAPI
│   │   │   │   ├── 📄 openai.rs       # OpenAI TTS cloud
│   │   │   │   ├── 📄 elevenlabs.rs   # ElevenLabs TTS cloud
│   │   │   │   └── 📄 cartesia.rs     # Cartesia Sonic cloud
│   │   │   └── 📁 sanitize/      # 5-stage text normalization
│   │   │       ├── 📄 mod.rs     # Pipeline orchestrator
│   │   │       ├── 📄 itn.rs     # Inverse Text Normalization
│   │   │       ├── 📄 tn.rs      # Text Normalization
│   │   │       ├── 📄 markdown.rs # Regex markdown stripping
│   │   │       ├── 📄 tts_normalize.rs  # Legacy rules
│   │   │       └── 📄 cleanup.rs # Final regex scrub
│   │   │
│   │   ├── 📁 brain/             # LLM subsystem
│   │   │   ├── 📄 mod.rs         # Module declarations
│   │   │   ├── 📄 client.rs      # SSE streaming client
│   │   │   ├── 📄 manager.rs     # Turn history, barge-in
│   │   │   └── 📄 llama_manager.rs  # Llama.cpp server orchestration
│   │   │
│   │   ├── 📁 audio_toolkit/     # Audio processing
│   │   │   ├── 📄 mod.rs         # Module declarations
│   │   │   ├── 📄 constants.rs   # Sample rates, frame sizes
│   │   │   ├── 📄 text.rs        # Text processing utils
│   │   │   ├── 📁 audio/
│   │   │   │   ├── 📄 device.rs  # Device enumeration (cpal)
│   │   │   │   ├── 📄 recorder.rs # Audio recording
│   │   │   │   ├── 📄 resampler.rs # rubato resampling
│   │   │   │   ├── 📄 visualizer.rs # rustfft FFT
│   │   │   │   ├── 📄 noise_suppression.rs  # RNNoise
│   │   │   │   └── 📄 utils.rs   # Audio utility functions
│   │   │   └── 📁 vad/           # Voice Activity Detection
│   │   │       ├── 📄 silero.rs  # Silero ONNX VAD
│   │   │       ├── 📄 smoothed.rs # Smoothed VAD output
│   │   │       └── 📄 triple_vad.rs  # RMS→RNNoise→Silero
│   │   │   └── 📁 bin/
│   │   │       └── 📄 cli.rs     # Audio toolkit CLI test
│   │   │
│   │   ├── 📁 commands/          # 10 Tauri command modules
│   │   │   ├── 📄 mod.rs         # Module declarations
│   │   │   ├── 📄 audio.rs       # Audio device/recording cmds
│   │   │   ├── 📄 brain.rs       # Brain/LLM commands
│   │   │   ├── 📄 discovery.rs   # Ollama/LM Studio discovery
│   │   │   ├── 📄 history.rs     # History CRUD commands
│   │   │   ├── 📄 llama_server.rs  # Llama.cpp server management
│   │   │   ├── 📄 models.rs      # Model management cmds
│   │   │   ├── 📄 transcription.rs  # STT pipeline cmds
│   │   │   ├── 📄 tts.rs         # TTS engine commands
│   │   │   └── 📄 wake_word.rs   # Wake word commands
│   │   │
│   │   ├── 📁 shortcut/          # 4 modules for shortcuts
│   │   │   ├── 📄 mod.rs         # Shortcut manager
│   │   │   ├── 📄 handler.rs     # Shortcut event handler
│   │   │   ├── 📄 key_listener.rs # Low-level key listener
│   │   │   └── 📄 tauri_impl.rs  # Tauri plugin wrapper
│   │   │
│   │   ├── 📁 helpers/           # Helper utilities
│   │   │   ├── 📄 mod.rs         # Module declarations
│   │   │   └── 📄 clamshell.rs  # Clamshell mode detection
│   │   │
│   │   ├── 📁 llama_server/       # Pre-compiled llama.cpp manager
│   │   │   ├── 📄 mod.rs         # Module declarations
│   │   │   └── 📄 manager.rs     # Server lifecycle, download, GPU offload
│   │
│   ├── 📄 Cargo.toml             # Rust dependencies (60+ crates)
│   ├── 📄 Cargo.lock             # Locked dependency versions
│   ├── 📄 tauri.conf.json        # Tauri application config
│   ├── 📁 capabilities/          # Tauri capability permissions
│   ├── 📁 resources/             # Static resources
│   │   ├── 📄 default_settings.json  # Platform defaults
│   │   ├── 📁 models/            # VAD + Kokoro models
│   │   ├── *.wav / *.png         # Sound effects, tray icons
│   │   └── 📁 ...                # Platform-specific resources
│   ├── 📁 icons/                 # Platform icon set
│   ├── 📁 nsis/                  # NSIS installer script
│   ├── 📁 gen/                   # Auto-generated schemas
│   └── 📁 target/                # Build output (gitignored)
│
├── 📁 models/                    # STT/TTS model files
│   ├── 📄 silero_vad_v4.onnx
│   └── 📁 piper-voices/          # Piper voice models
│
├── 📁 scripts/                   # Utility scripts
├── 📁 tests/                     # E2E tests
├── 📁 .github/                   # CI/CD configuration
│   ├── 📁 workflows/             # 9 GitHub Actions workflows
│   ├── 📁 ISSUE_TEMPLATE/        # Bug report template
│   └── 📄 PULL_REQUEST_TEMPLATE.md
│
├── 📁 .vscode/                   # VS Code settings
├── 📁 .nix/                      # Nix scripts
├── 📄 flake.nix                  # Nix flake
├── 📄 flake.lock                 # Nix lock file
├── 📄 eslint.config.js           # ESLint config
├── 📄 playwright.config.ts       # Playwright config
└── 📄 bun.lock                   # Dependency lock file
```

---

## 19. Roadmap & Future Work

### Completed (✅)

| Feature                                               | Details                                                        |
| ----------------------------------------------------- | -------------------------------------------------------------- |
| STT (9 engine types, 11 variants: Parakeet V3/V2/Unified/EOU, Whisper, Moonshine, Nemotron 3.5, SenseVoice, GigaAM, Canary, Cohere) | Multi-engine STT with auto language detection, GPU acceleration |
| TTS read-aloud (8 backends)                           | Piper, Kokoro, Kitten, Pocket, SAPI, OpenAI, ElevenLabs, Cartesia |
| Conversation mode                                     | Streaming LLM + streaming TTS                                  |
| Double-copy clipboard trigger                         | Copy same text twice within 1.5s                               |
| Text normalization pipeline                           | ITN + TN + markdown stripping (5 stages)                       |
| TripleVAD                                             | RMS → RNNoise → Silero cascade                                 |
| Crash logging                                         | Full backtraces to s2b2s-crash.log                             |
| Her 3D loading animation                              | Three.js tube geometry with ring reveal                        |
| 20-language i18n                                      | Full translation coverage                                      |
| WarmEngine trait lifecycle                            | Stopped → Loading → WarmingUp → Ready → Error (implemented in backends, direct-managed in orchestrator) |
| TTS performance telemetry                             | chars_per_ms adaptive fragment sizing                          |
| Piper persistent HTTP server with CUDA auto-discovery | Warm model, CUDA EP auto-detection                             |
| cpal → rodio streaming playback                       | Gapless chunk synthesis                                        |
| Wake word audio pipeline                              | Connected `recorder.rs` audio callback to `WakeWordDetector::feed_audio()` for local VAD energy check |
| Specta typed bindings                                 | cargo test export_bindings (headless)                          |
| RAM-persistent warm model lifecycle                   | Model unload timeout with idle watcher                         |
| Save-to-file                                          | MP3/OGG/FLAC export via ffmpeg                                 |
| Waveform HUD                                          | AmplitudeEnvelope from TTS playback signal                     |
| AI Replace Selection                                  | Voice-edit selected text via LLM                               |
| Wake word (VAD-based)                                 | Simple RMS energy-threshold detection (0.03); KWS blocked on CRT linking; audio pipeline is connected via recorder.rs callback |
| Ollama/LM Studio/llama.cpp auto-discovery             | Probes :11434, :1234, :8080 for running servers                |
| Latency HUD                                           | Per-stage timestamps (EP, STT, TTFT, TTFA) color-coded display |
| Pre-compiled llama.cpp server                         | Drop-in CUDA/Vulkan/CPU GPU acceleration, auto-download, VRAM offloading |
| Llama.cpp settings tab                                | Manage server binaries, GPU detection, backend switching        |
| Performance metrics                                   | Tokens/sec, STT/TTS latency per message                         |
| GPU VRAM usage indicator                              | Green/yellow/red with hover tooltip, per-second polling         |
| Log viewer console                                    | Level filter, search, auto-refresh, copy to clipboard           |
| Footer status indicators                              | STT 🟢, Brain 🟢, TTS 🟢 with hover tooltips                   |
| Hands-free auto-listen / continuous voice             | Auto rearms mic after Brain+TTS finishes                        |
| Voice barge-in                                        | Interrupt TTS with new speech in continuous voice mode          |
| Brain overlay (3D avatar + reply bubble)               | ✅ Complete — 8-phase state machine, `brain-overlay/` React entry + `overlay_fx/` Rust module |
| Overlay Window settings (Tauri/OS-Native mode toggle) | ✅ Complete — `OverlayWindowConfig`, WGPU trail config |
| GPU overlay cursor trail physics                      | ✅ Complete — spring-friction chain, Catmull-Rom splines in `overlay_fx/trail.rs` |
| GPU overlay wgpu native rendering (Track B)           | 🚧 Placeholder — `overlay_fx/native/mod.rs` is a no-op; physics engine done, surface integration pending |
| Analysys/ evolution plans                             | 📋 Superseded — replaced by `futuristic_analysis/` (9 docs, 1,867 lines) which corrects CursorFX assumptions |

### In Progress (🚧)

| Feature                        | Details                                             |
| ------------------------------ | --------------------------------------------------- |
| Kokoro worker pool + crossfade | Multi-worker parallel synthesis with 10ms crossfade |
| Engine-switch cleanup          | Unload previous engine when switching               |
| Test suite (text pipeline)     | ITN 1217 tests + markdown + integration             |

### Planned (📋)

| Phase                | Features                                            |
| -------------------- | --------------------------------------------------- |
| **Streaming STT**    | OpenAI Realtime WS, Deepgram WS, MoonshineStreaming |
| **Pocket TTS**       | ✅ Complete — 8 character voices + voice cloning from WAV via Python HTTP server |
| **Profiles**         | Per-mode language/prompt/model/hotkey configuration |
| **Audio cache**      | Hash-keyed audio replay from history                |
| **Control HTTP API** | axum server with crypto auth                        |
| **Effects**          | WalkieTalkie/GameBoy audio DSP                      |
| **Prompt variables** | `${current_app}`, `${time_local}`, etc.             |

### Later (Post-1.0)

- Full-duplex conversation with AEC
- Local speaker diarization
- Long-form model routing (big model for long recordings)
- MCP tool use for Brain (file ops, web, app control)
- Chat memory/RAG over history (FTS5 → embeddings)
- Mobile companion app
- Plugin/API ecosystem

---

## 20. Known Issues & Limitations

### Current Known Issues

| Issue                                               | Severity | Status                          |
| --------------------------------------------------- | -------- | ------------------------------- |
| **WarmEngine trait not dynamically dispatched**     | Low      | Trait is implemented by all local backends but not dynamically used in the manager; lifecycle handled directly |
| **TTS telemetry wired**                             | Low      | ✅ Complete — `telemetry.rs` registered as state, recording synthesis speed, dynamically driving adaptive sizing |
| **Model definitions hardcoded**                     | Low      | 20+ model entries hardcoded in `model.rs` (2,224 lines); not JSON-driven as planned |
| **overlay_fx/native wgpu is placeholder**           | Low      | `NativeTrailOverlay::start()` is a no-op; wgpu surface integration pending |
| **Whisper model crashes** on some Win/Linux configs | High     | Parakeet V3 default avoids this |
| **Wayland paste** needs wtype/dotool                | Medium   | Documented workaround           |
| **Overlay focus-stealing** on Linux                 | Medium   | Disable overlay as workaround   |
| **AppImage build** fails on rolling distros         | Low      | Use deb/rpm instead             |
| **macOS Intel** needs manual ONNX Runtime           | Low      | Documented in BUILD.md          |

### Architecture Limitations

| Limitation                    | Explanation                                                             |
| ----------------------------- | ----------------------------------------------------------------------- |
| **Half-duplex only**          | Mic muted while TTS plays; barge-in via hotkey, not VAD. Voice barge-in works in continuous voice mode. |
| **Wake word (VAD-based)**     | RMS energy threshold (0.03) with 3-frame debounce; audio feed-in is connected via `recorder.rs` callback. KWS blocked on CRT linking conflict (sherpa-onnx /MT vs transcribe-rs /MD on Windows) |
| **No streaming STT defaults** | Conversation uses final-shot STT (lower total latency for short turns). Streaming STT available via WebSocket for EOU 120M model. |
| **Pocket voice cloning**      | Implemented via Python server — clone from 5-20s WAV, persistent storage in `models/TTS/pocket-cloned-voices/` |
| **No profiles**               | Per-context presets planned                                             |
| **No remote LAN-GPU support** | For heavy models on a separate machine (backlog)                        |
| **settings.rs is 1,803 lines** | Monolithic settings file; config fields spread across single `AppSettings` struct |
| **models.rs is 2,224 lines**  | 20+ model entries hardcoded; no JSON/external config-driven model loading yet |
| **`tts/fragment_queue.rs` is dead code** | 306 lines preserved for future use; marked `#![allow(dead_code)]` |

### Security Considerations

- API keys stored in OS keychain (Windows Credential Manager, macOS Keychain)
- Local HTTP control server is off by default
- Voice commands (PowerShell) off by default with explicit consent
- All network calls attributable to a visible setting
- Offline mode hard-blocks all egress

---

## 21. Diagrams

### Diagram 1: System Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                        Tauri 2 Application Shell                       │
│  ┌──────────────────────────────────────────────────────────────────┐  │
│  │                    React/TypeScript WebView                      │  │
│  │                                                                  │  │
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐  │  │
│  │  │ Main Window  │  │    Overlay   │  │  Conversation View   │  │  │
│  │  │ ┌──────────┐ │  │ ┌──────────┐ │  │ ┌─────────────────┐ │  │  │
│  │  │ │ Sidebar  │ │  │ │ Recording│ │  │ │ Message List    │ │  │  │
│  │  │ │ Settings │ │  │ │ /Speaking│ │  │ │ Streaming Tokens│ │  │  │
│  │  │ │ History  │ │  │ │ Indicator│ │  │ │ Text Input      │ │  │  │
│  │  │ │ Onboard  │ │  │ └──────────┘ │  │ └─────────────────┘ │  │  │
│  │  │ └──────────┘ │  └──────────────┘  └──────────────────────┘  │  │
│  │  └──────────────┘                                               │  │
│  │                                                                  │  │
│  │  ┌──────────────────────────────────────────────────────────┐  │  │
│  │  │            IPC Layer (tauri-specta typed bindings)        │  │  │
│  │  │  invoke("command", args) → Result  |  listen("event")    │  │  │
│  │  └──────────────────────────────────────────────────────────┘  │  │
│  └──────────────────────────────────────────────────────────────────┘  │
│                               │                                         │
│                               ▼                                         │
│  ┌──────────────────────────────────────────────────────────────────┐  │
│  │                        Rust Backend Core                        │  │
│  │                                                                  │  │
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐  │  │
│  │  │  managers/   │  │ audio_toolkit│  │      settings.rs     │  │  │
│  │  │  ┌─────────┐ │  │  ┌───────┐  │  │  (tauri-plugin-store)│  │  │
│  │  │  │ Audio   │ │  │  │ Audio │  │  └──────────────────────┘  │  │  │
│  │  │  │ Model   │ │  │  │ VAD   │  │                             │  │  │
│  │  │  │ Transcr │ │  │  │ Noise │  │  ┌──────────────────────┐  │  │  │
│  │  │  │ History │ │  │  └───────┘  │  │      commands/       │  │  │  │
│  │  │  └─────────┘ │  └──────────────┘  │  tts.rs, brain.rs   │  │  │  │
│  │  └──────────────┘                    └──────────────────────┘  │  │  │
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐  │  │  │
│  │  │    tts/      │  │   brain/     │  │     shortcut/        │  │  │  │
│  │  │  ┌───────┐  │  │  ┌────────┐  │  │  Tauri + rdev        │  │  │  │
│  │  │  │ Piper │  │  │  │ Client │  │  └──────────────────────┘  │  │  │
│  │  │  │ Kokoro│  │  │  │Manager│  │                             │  │  │
│  │  │  │ Cloud │  │  │  └────────┘  │  ┌──────────────────────┐  │  │  │
│  │  │  │ Sanit │  │  └──────────────┘  │     overlay.rs       │  │  │  │
│  │  │  │ Player│  │                     │     tray.rs          │  │  │  │
│  │  │  └───────┘  │                     │     clip.rs          │  │  │  │
│  │  └──────────────┘                     └──────────────────────┘  │  │  │
│  └──────────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────┘
```

### Diagram 2: Data Flow — Dictation

```
User Presses Shortcut
        │
        ▼
[1] AudioRecordingManager.start_recording()
        │
        ▼
[2] cpal captures 16kHz mono PCM frames
        │
        ▼
[3] TripleVAD processes each frame (30fps)
    ├── RMS Energy Gate (0.002 threshold)
    ├── RNNoise Voice Probability (0.2 threshold)
    └── Silero VAD Confirmation (0.3 threshold)
        │
        ▼ (speech detected)
[4] VAD accumulates audio until silence > threshold
        │
        ▼ (end of speech)
[5] Parakeet V3 STT transcribes audio buffer
        │
        ▼
[6] ITN normalizes spoken→written text
        │
        ▼
[7] Paste at cursor (clipboard save/restore)
        │
        ▼
[8] HistoryManager.save() → SQLite
        │
        ▼
[9] Event emitted: transcription complete
```

### Diagram 3: Data Flow — Conversation

```
User presses "Talk to Brain" shortcut
        │
        ▼
[1] Audio capture → TripleVAD → STT (same as dictation steps 1-5)
        │
        ▼ (text: "What is the capital of France?")
[2] ITN normalizes: "what is the capital of france" → "What is the capital of France?"
        │
        ▼
[3] Brain.client.send_message(messages, stream=true)
        │
        ▼ (SSE stream starts)
[4] Tokens arrive: "The" " capital" " of" " France" " is" " Paris" "."
        │
        ▼
[5] Sentence splitter accumulates tokens
    ┌── "The capital of France is Paris." ── emits complete sentence
        │
        ▼
[6] TTS Manager orchestrates:
    ├── Sanitize: markdown strip → TN → regex cleanup
    ├── Paginate: split into speakable chunks
    └── Synthesize: Piper/Kokoro generates audio for chunk 1
        │
        ▼
[7] Player starts streaming chunk 1
    └── While chunk 1 plays, chunk 2 synthesizes in background
        │
        ▼
[8] User interrupts (barge-in): presses shortcut again
    ├── TTS playback stops immediately
    ├── Any pending LLM tokens cancelled
    └── Recording starts for new turn
        │
        ▼
[9] Both user turn and assistant reply persisted to SQLite
```

### Diagram 4: Text Normalization Flow

```
STT Text: "meeting is at two thirty pm on january fifth"
         │
         ▼
┌─────────────────────┐
│ ITN (text-processing)│  "meeting is at 2:30 p.m. on January 5"
└─────────────────────┘
         │
         ▼
┌─────────────────────┐
│ Custom Words        │  (no changes — text is clean)
└─────────────────────┘
         │
         ▼
—> To Brain (in Conversation mode) or directly to TTS
         │
         ▼ (Brain may generate markdown)

LLM Text: "*The* meeting is at **2:30 PM** on **January 5th**."
         │
         ▼
┌─────────────────────┐
│ Regex Markdown Strip│  "The meeting is at 2:30 PM on January 5th."
└─────────────────────┘
         │
         ▼
┌─────────────────────┐
│ TN (text-processing)│  "the meeting is at two thirty p m on january fifth"
└─────────────────────┘
         │
         ▼
┌─────────────────────┐
│ Regex Cleanup       │  (remove artifacts, normalize whitespace)
└─────────────────────┘
         │
         ▼
TTS speaks: "the meeting is at two thirty p m on january fifth"
```

### Diagram 5: TTS Engine Lifecycle

```
                           ┌──────────┐
                           │  Stopped  │
                           └─────┬────┘
                                 │ warm() called
                                 ▼
                           ┌──────────┐
                    ┌──────│ Loading  │──────┐
                    │      └─────┬────┘      │
                    │            │ loaded     │
                    │            ▼           │
                    │      ┌──────────┐      │
                    │      │WarmingUp │      │
                    │      └─────┬────┘      │
                    │            │ warm-up    │
                    │            ▼           │
                    │      ┌──────────┐      │
              unload│      │  Ready   │      │ error
              called│      └────┬─────┘      │
                    │           │            │
                    │           │ synthesize │
                    │           ▼            │
                    │    ┌──────────────┐    │
                    │    │ Synthesizing │    │
                    │    └──────┬───────┘    │
                    │           │ done       │
                    │           ▼            │
                    │    ┌──────────────┐    │
                    └────│   Stopped    │◄───┘
                         └──────────────┘
                              ▲
                              │ idle timeout
                              │ (configurable)
                              │
                         ┌──────────┐
                         │   Idle   │
                         └──────────┘
```

### Diagram 6: Platform Matrix

```
                    WINDOWS 11          macOS              Linux
                    ──────────        ──────            ──────
                    ✅ PRIMARY        ✅ FIRST-CLASS    ✅ FIRST-CLASS

STT Engines:        Parakeet V3       Parakeet V3       Parakeet V3
                    Whisper GGML      Whisper GGML      Whisper GGML
                    Moonshine ONNX    Moonshine ONNX    Moonshine ONNX

GPU Acceleration:   DirectML          Metal             Vulkan
                    CUDA (optional)                     OpenBLAS
                                                        CUDA (optional)

TTS Backends:       Piper             Piper             Piper
                    Kokoro            Kokoro            Kokoro
                    Kitten            Kitten            Kitten
                    SAPI ✅            —                 —
                    OpenAI            OpenAI            OpenAI
                    ElevenLabs        ElevenLabs        ElevenLabs
                    Cartesia          Cartesia          Cartesia

VAD:                TripleVAD         TripleVAD         TripleVAD
                    Silero            Silero            Silero

Audio Capture:      cpal (WASAPI)     cpal (CoreAudio)  cpal (ALSA/Pulse)

Keyboard:           rdev + Tauri      rdev + Tauri      rdev + Tauri
                                                    (Wayland: wtype/dotool)

Paste:              enigo + clip      enigo + clip      enigo + clip
                                                    (Wayland: wtype)

Overlay:            Win32             NSPanel           GTK Layer Shell
                                                    (S2B2S_NO_GTK_LAYER_SHELL)

Installer:          NSIS + MSI        DMG               deb + rpm + AppImage

Secrets:            Credential Mgr    Keychain          keyring (DBus)

TTS Voice Setup:    project venv      project venv      project venv
                    Kokoro (complete)  Kokoro (complete) Kokoro (complete)
```

---

## Quick Reference

### Key CLI Commands

```bash
# Development
bun install                          # Install JS deps
bun run tauri dev                    # Dev mode full app
bun run tauri build                  # Production build
cargo test export_bindings           # Regenerate TS bindings

# Frontend only
bun run dev                          # Vite dev server
bun run build                        # Build frontend

# Quality
bun run lint                         # ESLint
bun run format                       # Prettier + cargo fmt
bunx tsc --noEmit                    # TS type check

# Testing
bun run test:playwright              # E2E tests
cargo test                           # Rust unit tests
bun run check:translations           # i18n validation
```

### Key Files Quick Reference

| File                                            | What it contains                       |
| ----------------------------------------------- | -------------------------------------- |
| `src-tauri/src/lib.rs`                          | App entry point, setup, event handlers |
| `src-tauri/src/tts/manager.rs`                  | TTS orchestration                      |
| `src-tauri/src/brain/client.rs`                 | SSE streaming LLM client               |
| `src-tauri/src/llama_server/manager.rs`         | Pre-compiled llama.cpp server lifecycle, auto-download, GPU offloading |
| `src-tauri/src/brain/llama_manager.rs`          | Llama.cpp server process management + Gemma-4 model orchestration |
| `src-tauri/src/tts/backends/pocket.rs`          | Pocket TTS (Python server) + voice cloning from WAV |
| `src-tauri/src/audio_toolkit/vad/triple_vad.rs` | 3-stage VAD                            |
| `src-tauri/src/settings.rs`                     | All settings definitions               |
| `src-tauri/src/actions.rs`                      | Pipeline triggers                      |
| `src/App.tsx`                                   | Main frontend component                |
| `src/components/HerLoading.tsx`                 | 3D loading animation                   |
| `src/bindings.ts`                               | Auto-generated typed IPC               |
| `src/i18n/locales/en/translation.json`          | English translations                   |

### Key Environment Variables

| Variable                           | Purpose                         |
| ---------------------------------- | ------------------------------- |
| `S2B2S_NO_GTK_LAYER_SHELL=1`       | Disable GTK layer shell (Linux) |
| `WEBKIT_DISABLE_DMABUF_RENDERER=1` | Fix WebKit rendering (Linux)    |
| `CMAKE_POLICY_VERSION_MINIMUM=3.5` | Fix cmake on macOS              |
| `ORT_LIB_LOCATION`                 | ONNX Runtime path (Intel Mac)   |
| `RUST_LOG=debug`                   | Verbose logging                 |

---

_This document serves as the definitive reference for the S2B2S project — for users, developers, and AI agents. Last audited June 2026._

_See [README.md](README.md) for quick start and [AGENTS.md](AGENTS.md) for AI development guidance._

### Evolution Documents

S2B2S maintains two sets of evolution planning documents:

- **`futuristic_analysis/`** (9 files, 1,867 lines) — **Active, current plan.** Supersedes `analysys/`. Corrects critical CursorFX assumptions (uses Tauri V2 + Vulkan, not winit + DX12). Covers: GPU overlay architecture (two-track: webview + native wgpu), Conversation Mode 2.0 UX spec, screen vision pipeline, 3D cybernetic avatar (Four Senses), concrete implementation plan with code-level specifics.
- **`analysys/`** (6 files) — **Superseded.** Original evolution plan from earlier audit. Contains the now-corrected CursorFX assumptions (DX12/DirectComposition recommendation). Excluded from git and Repomix. Preserved on disk for reference but `futuristic_analysis/00_README_START_HERE.md` explicitly states it is superseded.
- **`references_comparative_analysis_md/`** (27 files, ~10,300 lines) — Complete comparative analysis of all 23 reference projects. Includes individual reviews, fork lineage, license compatibility matrix, and architecture pattern catalog.
- **`gemma_4_qat_mtp_e2b/`** (2 files) — Reference benchmarks and configuration for Gemma 4 E2B brain model (MTP n=13 at ~216 tok/s, multimodal API formats).

---

## Related Documentation

| Document | Contents |
|----------|----------|
| `references_comparative_analysis_md/00_COMPARATIVE_ANALYSIS.md` | Cross-project comparison of all 22 reference projects, feature matrices, harvest priorities |
| `references_comparative_analysis_md/README.md` | Folder index and reading guide for all 22 individual project reviews |
| `references_comparative_analysis_md/FORK_LINEAGE.md` | Complete Handy family genealogy — what S2B2S inherited from each project |
| `references_comparative_analysis_md/LICENSE_COMPATIBILITY.md` | Which projects' code can be reused (MIT) vs concept-only (GPL/AGPL) |
| `reference_github_links.md` | Curated list of all reference project GitHub repos |
| `futuristic_analysis/00_README_START_HERE.md` | S2B2S evolution vision — GPU overlay, conversation 2.0, screen vision, 3D avatar |
