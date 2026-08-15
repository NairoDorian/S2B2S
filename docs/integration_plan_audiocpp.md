# Integration Plan — Native C++ Audio & TTS Engine via `audio.cpp`

> **Status:** Planning & Architecture Design Doc.
> **Repository:** [`0xShug0/audio.cpp`](https://github.com/0xShug0/audio.cpp)
> **Goal:** Integrate `audio.cpp` into S2B2S as a native C++ high-performance runtime for TTS, Voice Cloning, Voice Design, ASR, and Audio Workflows on GGUF with CUDA, Vulkan, Metal, and CPU acceleration.

---

## 1. Executive Summary

`audio.cpp` is a high-performance C++ audio inference framework built on `ggml`. It eliminates heavy Python/PyTorch/Conda runtimes and provides native C++ inference for **49 model families and 70+ model variants** as GGUF packages.

### Why `audio.cpp` is a Game-Changer for S2B2S:

1. **Massive TTS Performance & Efficiency:**
   - Benchmarks demonstrate **1.8x to 8x speedups** over Python reference paths while cutting end-to-end latency by **45%–85%**.
   - **Supertonic 3:** Up to **200x real-time on CUDA** (10 hours of audio in 3 minutes on RTX 5090; 6x+ real-time on CPU).
   - **VibeVoice 1.5B:** **5.15x real-time** (93.9 min podcast generated in 18.2 min).
   - **Qwen3-TTS & PocketTTS GGUF:** Native C++ GGUF inference with zero Python dependencies, instant startup, low memory footprint, and Q8_0/Q4_K quantization support.
2. **Standard OpenAI-Compatible HTTP Server (`audiocpp_server`):**
   - Implements `POST /v1/audio/speech` (TTS with WAV and SSE chunked PCM streaming).
   - Implements `POST /v1/audio/transcriptions` (STT multipart & file upload).
   - Implements `POST /v1/audio/transcriptions/live` (real-time chunked PCM streaming STT with SSE text deltas).
   - Implements `GET /v1/audio/voices` (voice discovery and presets).
3. **Cross-Platform & Multi-Backend:**
   - Supports **Windows 11 (CUDA & Vulkan)**, **macOS (Metal & CPU)**, and **Linux (CUDA, HIP/ROCm, Vulkan, CPU)**.

---

## 2. Core Model Families for S2B2S

| Family                 | Task               | Locales / Highlights                                     | S2B2S Target Role                                  |
| :--------------------- | :----------------- | :------------------------------------------------------- | :------------------------------------------------- |
| **`qwen3_tts`**        | TTS, Clone, Design | 10+ langs (zh, en, fr, de, it, ja, ko, pt, ru, es)       | Native C++ GGUF Q8_0 replacement for Python server |
| **`pocket_tts`**       | TTS, Clone         | en, de, it, pt, es (100M params, ultra-light)            | Native C++ GGUF replacement for Python Pocket      |
| **`supertonic`**       | TTS                | 30+ langs (en, ko, ja, ar, de, es, fr, hi, it, ru, etc.) | Ultra-low latency instant read-aloud (200x RTF)    |
| **`dots_tts`**         | TTS, Clone, Ctrl   | Multilingual, SOAR & MeanFlow with emotion control       | High-expressiveness emotional TTS                  |
| **`chatterbox`**       | TTS, Clone, VC     | 19 langs, 0.5B backbone                                  | Voice cloning and voice conversion                 |
| **`omnivoice`**        | TTS, Clone, Design | 646+ languages                                           | Massive multilingual voice synthesis               |
| **`voxcpm2`**          | TTS, Clone, Design | 30+ langs, 48 kHz, streaming                             | High-fidelity 48 kHz streaming TTS                 |
| **`voxtral_realtime`** | ASR                | Audio-LLM realtime streaming                             | Real-time dictation & speech-to-text               |
| **`qwen3_asr`**        | ASR                | 30+ languages, 0.6B / 1.7B                               | GGUF ASR backend                                   |
| **`silero_vad`**       | VAD                | Language agnostic, bundled                               | Zero-dependency native VAD                         |

---

## 3. Architecture & Integration Workstreams

```
┌────────────────────────────────────────────────────────────────────────┐
│                              S2B2S Frontend                            │
│           (React 19 + TypeScript + SpeechSettings + Dictation)         │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │ Tauri IPC Commands
┌───────────────────────────────────▼────────────────────────────────────┐
│                             S2B2S Backend                              │
│  ┌─────────────────────────┐         ┌──────────────────────────────┐  │
│  │   TtsManager (Rust)     │         │ TranscriptionManager (Rust)  │  │
│  └────────────┬────────────┘         └──────────────┬───────────────┘  │
│               │                                     │                  │
│  ┌────────────▼────────────┐         ┌──────────────▼───────────────┐  │
│  │ AudioCppBackend (TTS)   │         │ AudioCppBackend (STT)        │  │
│  └────────────┬────────────┘         └──────────────┬───────────────┘  │
│               │                                     │                  │
│  ┌────────────▼─────────────────────────────────────▼───────────────┐  │
│  │               AudioCppServerManager (Rust)                       │  │
│  │     • Lifecycle management (spawn, port, health check, kill)     │  │
│  │     • Generates server.json configuration                        │  │
│  │     • Model & GGUF path resolver (models/TTS/audiocpp)           │  │
│  └────────────────────────────────┬─────────────────────────────────┘  │
└───────────────────────────────────┼────────────────────────────────────┘
                                    │ HTTP / SSE Local Loopback
┌───────────────────────────────────▼────────────────────────────────────┐
│                  audiocpp_server.exe (Native C++)                      │
│            (GGML Engine: CUDA / Vulkan / Metal / CPU)                  │
│                                                                        │
│   • POST /v1/audio/speech          • POST /v1/audio/transcriptions/live│
│   • GET  /v1/audio/voices          • GET  /health                      │
│                                                                        │
│   [qwen3_tts] [pocket_tts] [supertonic] [dots_tts] [voxtral_realtime]  │
└────────────────────────────────────────────────────────────────────────┘
```

### WS-1: Build & Packaging Tooling

- Add `scripts/compile-audiocpp.ps1` to compile `audiocpp_server.exe` / `audiocpp_cli.exe` using CMake, MSVC, and CUDA / Vulkan.
- Update `scripts/sync-all-repos.ps1` and `package.json` (`sync:repos`) to automatically fetch and update `audio.cpp`.

### WS-2: `AudioCppServerManager` Lifecycle Controller

- Located in `src-tauri/src/audiocpp_server/manager.rs`.
- Spawns `audiocpp_server.exe` with dynamically allocated or configured port (e.g. `43120`).
- Manages `server.json` configuration file, defining active model IDs, GGUF paths, and device settings.
- Probes `/health` for server readiness and gracefully shuts down process on app exit.

### WS-3: AudioCpp TTS Backend (`TtsBackend` & `WarmEngine`)

- Located in `src-tauri/src/tts/backends/audiocpp.rs`.
- Sends POST requests to `http://127.0.0.1:{port}/v1/audio/speech`.
- Maps S2B2S text sanitization, voice selections, and speed parameters into the standard OpenAI speech schema.
- Supports voice cloning via `voice_ref` (file path or base64 WAV payload) and voice instructions (`instruct`).
- Dynamically queries `/v1/audio/voices` for available model voices.

### WS-4: AudioCpp STT & Live Transcription Backend

- Located in `src-tauri/src/stt/audiocpp.rs`.
- Connects to `/v1/audio/transcriptions` (file/multipart) and `/v1/audio/transcriptions/live` (raw PCM stream).
- Enables GGUF streaming STT models (`voxtral_realtime`, `nemotron_asr`, `qwen3_asr`).

### WS-5: Frontend & Settings UI

- Surfaced in `SpeechSettings.tsx` and `MultiSttSettings.tsx`.
- Option to select `Audio.cpp (Native C++ GGUF)` as the TTS Engine.
- Model picker for `audio.cpp` TTS models (Supertonic 3, Qwen3-TTS, PocketTTS, DotsTTS, Chatterbox, OmniVoice).
- Automatic model downloader integration in `models/download_models.ps1` fetching ready GGUF packages from `audio-cpp/audio.cpp-gguf`.

---

## 4. Phased Implementation Roadmap

1. **Phase 1: Repository Sync & Build Scripts**
   - Add `audio.cpp` to `sync-all-repos.ps1` and `package.json`.
   - Implement `scripts/compile-audiocpp.ps1` for Windows CUDA/Vulkan builds.
2. **Phase 2: Backend Server Controller & IPC**
   - Create `src-tauri/src/audiocpp_server/` module.
   - Implement `AudioCppServerManager` for binary lifecycle and config generation.
3. **Phase 3: TTS Backend Integration**
   - Implement `AudioCppBackend` in `src-tauri/src/tts/backends/audiocpp.rs`.
   - Register `AudioCpp` in `TtsEngine` enum, `manager.rs`, and commands.
4. **Phase 4: Settings UI & GGUF Model Downloads**
   - Add Audio.cpp settings to `SpeechSettings.tsx`.
   - Add download recipes to `models/download_models.ps1`.
5. **Phase 5: STT & Live Streaming Integration (Optional / Advanced)**
   - Connect live chunked PCM streaming to `/v1/audio/transcriptions/live`.
