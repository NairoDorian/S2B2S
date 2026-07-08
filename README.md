# <img src="src/assets/logo.png" alt="S2B2S" width="48" height="48" style="vertical-align: middle" /> S2B2S — SpeechToBrainToSpeech

**Local-first STT → Brain → TTS desktop app for Windows 11, macOS, and Linux. Dictate anywhere, read anything aloud, and talk naturally with a local AI — almost keyboard-free.**

S2B2S is a cross-platform desktop application that combines speech-to-text (STT), a local or cloud "Brain" (LLM), and text-to-speech (TTS) into one unified voice-native experience. Built on the [Handy](https://github.com/cjpais/Handy) skeleton (MIT), S2B2S has evolved far beyond its origins — adding TTS read-aloud with 8 backends (Piper, Kokoro, Kitten, Pocket, SAPI, OpenAI, ElevenLabs, Cartesia) with RAM-persistent warm model lifecycle, a streaming LLM conversation mode with 20-turn memory and 10 providers, pre-compiled llama.cpp CUDA/Vulkan/CPU server with GPU VRAM offloading, per-message performance metrics (tokens/sec, STT/TTS latency), sentence streaming for fast time-to-first-audio, double-copy clipboard trigger, system RAM/VRAM footer indicators, Pocket voice cloning, a full text normalization pipeline (ITN + TN + markdown stripping), and a brain overlay with 3D avatar.

---

## Table of Contents

- [Why S2B2S?](#why-s2b2s)
- [How It Works](#how-it-works)
- [Quick Start](#quick-start)
- [Architecture](#architecture)
- [Default Stack](#default-stack)
- [The Three Pipelines](#the-three-pipelines)
- [CLI Parameters](#cli-parameters)
- [Platform Support](#platform-support)
- [System Requirements](#system-requirements)
- [Roadmap & Active Development](#roadmap--active-development)
- [Debug Mode](#debug-mode)
- [Troubleshooting](#troubleshooting)
- [How to Contribute](#how-to-contribute)
- [Related Projects](#related-projects)
- [License & Attribution](#license--attribution)

---

## Why S2B2S?

- **Local-first**: Everything works offline. Parakeet V3 for STT, Piper/Kokoro/Kitten/Pocket for TTS, pre-compiled llama.cpp (CUDA/Vulkan/CPU) or Ollama/LM Studio for the Brain. No cloud required.
- **Open Source (MIT)**: Forkable, inspectable, extendable.
- **Private**: Your voice, text, and conversations stay on your machine. Keys stored in OS keychain.
- **Voice-native**: Designed for spoken interaction — not a text chat with voice bolted on.
- **Three superpowers in one app**: Dictate anywhere, read anything aloud, talk to a local brain.

---

## How It Works

1. **Dictate Anywhere** — press a hotkey, speak, and polished text lands at your cursor. Powered by **Parakeet V3** (default, local, 25 languages with auto-detection).
2. **Read Aloud** — select text anywhere, press a hotkey, and a local voice reads it instantly with pause/resume. Also triggered by **double-copy** (copy same text twice within 1.5s).
3. **Talk to the Brain** — the Conversation window: speak naturally to a local LLM (Ollama/LM Studio/llama.cpp with GPU offload) or any cloud LLM. Real-time STT in, streaming tokens out, TTS reads the reply aloud (toggleable, default ON). Per-message performance metrics (tokens/sec, latency) displayed in-line.

---

## Quick Start

### Installation

1. Download the latest release from the [releases page](https://github.com/NairoDorian/S2B2S/releases)
2. Install and grant microphone permissions
3. On first run, download **Parakeet V3** (~478 MB) — the default and recommended STT model
4. Configure your hotkeys and start transcribing!

### Development Setup

```bash
# Prerequisites: Rust (latest stable), Bun

# 1. Install frontend dependencies
bun install

# 2. Install standalone speech runtime (portable uv + Python 3.12 + venv)
#    Windows: .\scripts\install-speech-runtime.ps1
#    macOS/Linux: bash scripts/install-speech-runtime.sh
#    This provisions everything needed for local TTS engines (Piper, Kokoro, Kitten, Pocket).

# 3. Download model files (organized as models/STT/, models/Brain/, models/TTS/)
#    Windows: .\models\download_models.ps1 -Model all
#    macOS/Linux: bash models/download_models.sh --model all

# 4. Run in development mode
bun run tauri dev

# On macOS if you encounter cmake errors:
CMAKE_POLICY_VERSION_MINIMUM=3.5 bun run tauri dev

# Build for production
bun run tauri build

# Regenerate typed bindings (frontend ↔ backend)
cargo test export_bindings

# Lint & Format
bun run lint
bun run format
```

For detailed platform-specific build instructions, see [BUILD.md](BUILD.md).

---

## Architecture

S2B2S is built as a **Tauri 2 application** with a Rust backend and React/TypeScript frontend:
```
┌─────────────────────────────────────────────────────────────────┐
│                     Tauri App (single process)                    │
│                                                                   │
│  ┌─────────────┐     ┌──────────────────────────────────────────┐│
│  │  React/TS   │◄───►│              Rust Core                    ││
│  │  Frontend   │ IPC │  (tauri-specta typed bindings)            ││
│  │             │     │                                            ││
│  │  Settings   │     │  managers/                                ││
│  │  Overlay    │     │   ├─ audio.rs (recording)                 ││
│  │  Conversation│     │   ├─ model.rs (downloads)                ││
│  │  History    │     │   ├─ transcription.rs (STT pipeline)      ││
│  │  Onboarding │     │   ├─ history.rs (SQLite)                  ││
│  │  Her Loading│     │   └─ continuous_voice.rs (hands-free)     ││
│  └─────────────┘     │                                            ││
│                       │  tts/ (Text-to-Speech subsystem)          ││
│                       │   ├─ backends/ (Piper, Kokoro, Kitten,   ││
│                       │   │   SAPI, OpenAI, ElevenLabs, Cartesia)││
│                       │   ├─ manager.rs (orchestration)          ││
│                       │   ├─ sanitize/ (ITN, TN, markdown strip) ││
│                       │   ├─ pagination.rs / fragment_queue.rs   ││
│                       │   ├─ player.rs (rodio playback)          ││
│                       │   ├─ status.rs / telemetry.rs            ││
│                       │   └─ clipboard_watch.rs (double-copy)    ││
│                       │                                            ││
│                       │  brain/ (Streaming LLM)                  ││
│                       │   ├─ client.rs (SSE streaming)           ││
│                       │   ├─ manager.rs (turn history, barge-in) ││
│                       │   └─ llama_manager.rs (llama.cpp bridge) ││
│                       │                                            ││
│                       │  llama_server/ (llama.cpp GPU)           ││
│                       │   └─ manager.rs (auto-download, launch,  ││
│                       │       GPU VRAM offloading, health check) ││
│                       │                                            ││
│                       │  audio_toolkit/                          ││
│                       │   ├─ audio/ (cpal capture, resample)     ││
│                       │   └─ vad/ (TripleVAD: RMS→RNNoise→Silero)││
│                       │                                            ││
│                       │  commands/ (Tauri command handlers)      ││
│                       │  shortcut/ (global hotkeys)              ││
│                       │  overlay.rs (recording/speaking overlay) ││
│                       │  settings.rs (persistence)               ││
│                       └──────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────────┘```

### Frontend (React)

- **110+ components**: settings (11 subdirectories, 60+ files), model-selector, onboarding, conversation, overlay, footer, sidebar, icons, shared utils, update-checker
- **20-language i18n** via i18next
- **Zustand** state management with typed bindings
- **Her-style 3D loading animation** (Three.js)
- Dark theme with purple (#7c3aed) + gold (#f59e0b) accents

### Backend (Rust)

- **Manager pattern**: Audio, Model, Transcription, History, TTS, Brain
- **TTS Backend trait**: 8 engines (5 local, 3 cloud) with `WarmEngine` lifecycle
- **TripleVAD**: 3-stage voice activity detection (RMS → RNNoise → Silero)
- **Normalization pipeline**: ITN → Custom Words → Markdown Strip → TN → Regex Cleanup
- **Single instance** architecture with CLI remote control

### Model Directory Structure

```
models/
├── STT/         # Speech-to-text (Parakeet V3, Silero VAD, Whisper)
├── Brain/       # LLM models (llama.cpp GGUF)
└── TTS/         # Text-to-speech engines
    ├── kokoro/        # Kokoro-82M ONNX + voices
    ├── piper-voices/  # Piper voice files (.onnx + .json)
    ├── pocket/        # Pocket TTS (auto-downloaded)
    └── kitten/        # Kitten TTS (auto-downloaded)
```

All models, voices, and Brain GGUF files are organized under a master `models/` folder with three category subdirectories. The app resolves paths project-local first (`S2B2S/models/`) and falls back to the OS app data directory for installed builds. A portable Python virtual environment is at `venv/` (provisioned by `scripts/install-speech-runtime.ps1`/`.sh`) and used by all local TTS engines.

### Core Libraries

| Crate                | Version | Purpose                                                 |
| -------------------- | ------- | ------------------------------------------------------- |
| `transcribe-rs`      | 0.3.11  | Local STT (Parakeet V3 + Whisper) with GPU acceleration |
| `cpal`               | 0.17    | Cross-platform audio I/O                                |
| `nnnoiseless`        | 0.5.2   | RNNoise-based noise suppression                         |
| `vad-rs`             | —       | Silero VAD (ONNX)                                       |
| `rdev`               | —       | Global keyboard shortcuts                               |
| `rubato`             | 3.0     | Audio resampling                                        |
| `rodio`              | 0.22    | Audio playback                                          |
| `text-processing-rs` | 0.2.2   | ITN + TN normalization                                  |
| `regex`              | 1.12    | Markdown stripping for TTS pipeline                     |
| `rusqlite`           | 0.40    | SQLite persistence                                      |
| `reqwest`            | 0.13    | HTTP client                                             |
| `tauri-specta`       | —       | Typed IPC bindings                                      |

---

## Default Stack

S2B2S works fully offline with no configuration. The defaults are chosen for speed, privacy, and broad hardware compatibility:

| Layer                 | Default                                                                                                      | Alternatives                                                                                                            |
| --------------------- | ------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------- |
| **STT**               | **Parakeet TDT 0.6B V3** (auto language, 25 langs, CPU-fast)                                                 | Whisper (Small/Medium/Turbo/Large), Moonshine                                                                           |
| **VAD**               | **TripleVAD** (RMS→RNNoise→Silero)                                                                           | Silero only, Push-to-talk                                                                                               |
| **Noise Suppression** | RNNoise (toggleable, triple mode default)                                                                    | Off                                                                                                                     |
| **TTS**               | **Piper** persistent HTTP server (speed-first, warm)                                                         | Kokoro-82M (quality-first), Kitten, Pocket (voice cloning), SAPI, OpenAI, ElevenLabs, Cartesia                          |
| **Brain**             | **llama.cpp** (pre-compiled CUDA/Vulkan/CPU) / **Ollama** auto-detected (`:11434`) / **LM Studio** (`:1234`) | 9 other providers: OpenAI, Anthropic, Gemini, Groq, Cerebras, OpenRouter, Z.ai, AWS Bedrock, Apple Intelligence (macOS) |
| **Storage**           | SQLite (rusqlite + migrations)                                                                               | —                                                                                                                       |
| **Secrets**           | OS keychain (Windows Credential Manager, macOS Keychain)                                                     | —                                                                                                                       |

---

## The Three Pipelines

### Dictation Pipeline

```
Microphone → TripleVAD (RMS→RNNoise→Silero) → Parakeet V3 STT → ITN Normalization → Clipboard/Paste
```

### Conversation Pipeline (Speech → Brain → Speech)

```
Microphone → TripleVAD → Parakeet V3 STT → ITN Normalization → LLM (Brain) → Markdown Strip → TN Normalization → TTS (Piper/Kokoro) → Speaker
```

### Read Aloud Pipeline

```
Selected Text (or double-copy clipboard) → Markdown Strip → TN Normalization → TTS → Speaker
```

### Text Normalization Pipeline (5-Stage)

```
Post-STT:  ITN (text-processing-rs) → Custom Words (fuzzy correction)
Pre-TTS:   Markdown strip (regex) → TN (text-processing-rs) → Regex Cleanup
```

| Pass                   | Direction         | Example Input            | Example Output                 |
| ---------------------- | ----------------- | ------------------------ | ------------------------------ |
| ITN                    | Spoken → Written  | `two hundred thirty two` | `232`                          |
| ITN                    | Spoken → Written  | `january fifth`          | `January 5, 2025`              |
| Markdown strip (regex) | Markdown → Speech | `**bold**`               | `bold`                         |
| TN                     | Written → Spoken  | `$5.50`                  | `five dollars and fifty cents` |
| TN                     | Written → Spoken  | `Dr. Smith`              | `doctor Smith`                 |

---

## CLI Parameters

```bash
s2b2s --toggle-transcription    # Toggle recording on/off
s2b2s --toggle-post-process     # Toggle with post-processing
s2b2s --cancel                  # Cancel current operation
s2b2s --start-hidden            # Start minimized to tray
s2b2s --no-tray                 # Start without tray icon
s2b2s --debug                   # Enable debug logging
```

Unix signals (Linux/macOS):
| Signal | Action |
|--------|--------|
| `SIGUSR2` | Toggle transcription |
| `SIGUSR1` | Toggle transcription with post-processing |

---

## Platform Support

| Platform                          | Status         | Notes                                                       |
| --------------------------------- | -------------- | ----------------------------------------------------------- |
| **Windows 11**                    | ✅ Primary     | Full support, NSIS/MSI installers                           |
| **Windows 10**                    | ✅ Supported   | Tested                                                      |
| **macOS** (Intel + Apple Silicon) | ✅ First-class | Metal acceleration, accessibility permissions required      |
| **Linux** (x64)                   | ✅ First-class | Ubuntu 22.04/24.04, Arch, Fedora; Wayland with wtype/dotool |

### Linux Notes

**Text Input Tools:**
| Display Server | Tool | Install |
|---------------|------|---------|
| X11 | `xdotool` | `sudo apt install xdotool` |
| Wayland | `wtype` | `sudo apt install wtype` |
| Both | `dotool` | `sudo apt install dotool` (+ `input` group) |

---

### Linux Environment Variables

| Variable                           | Purpose                                        |
| ---------------------------------- | ---------------------------------------------- |
| `S2B2S_NO_GTK_LAYER_SHELL=1`       | Skip GTK layer shell on Linux                  |
| `WEBKIT_DISABLE_DMABUF_RENDERER=1` | Fix WebKit rendering on some GPU/driver combos |

---

## System Requirements

**Parakeet V3 (default STT):**

- CPU-only operation (no GPU required)
- Minimum: Intel Skylake (6th gen) or equivalent AMD
- Performance: ~5x real-time on mid-range hardware (tested on i5)
- 25 languages with automatic detection

**Whisper Models:**

- macOS: M series or Intel Mac
- Windows/Linux: Intel, AMD, or NVIDIA GPU recommended

**TTS Backends:**

- Piper: CPU/CUDA, ~100-200 MB RAM per voice. Runs via portable Python venv (provisioned by `scripts/install-speech-runtime.ps1`/`.sh`).
- Kokoro-82M: CPU-only, ~115 MB ONNX model. Runs via portable Python venv. 54 voices across 9 languages.
- Kitten: CPU-only, ~25-80 MB ONNX models. Runs via portable Python venv. 8 English voices.
- Pocket: CPU/GPU (PyTorch), ~100 MB. Runs via portable Python venv. 8 character voices + voice cloning from WAV.
- SAPI: Windows-only voice API. Fully implemented local fallback using windows-rs COM interop.
- All local TTS engines use the project `venv/` — provisioned automatically during onboarding, no system Python required.
- Cloud engines: Requires internet connection and API key.

---

## Roadmap & Active Development

S2B2S is the foundation of the SpeechToBrainToSpeech vision. The core STT → Brain → TTS pipeline is feature-complete. Current work focuses on performance, stability, code quality, and polish.

| Feature                                                                                                                                              | Status                                                            |
| ---------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------- |
| STT dictation (Parakeet V3, Whisper, Moonshine, Nemotron 3.5, SenseVoice, GigaAM, Canary, Cohere)                                                    | ✅ Complete                                                       |
| TTS read-aloud (8 backends: Piper, Kokoro, Kitten, Pocket, SAPI, OpenAI, ElevenLabs, Cartesia)                                                       | ✅ Complete                                                       |
| Conversation mode with streaming LLM (10 providers: Ollama/LM Studio/llama.cpp/OpenAI/Anthropic/Gemini/Groq/Cerebras/OpenRouter/Z.ai/Bedrock/custom) | ✅ Complete                                                       |
| Pre-compiled llama.cpp CUDA/Vulkan/CPU server with GPU VRAM offloading                                                                               | ✅ Complete                                                       |
| Performance metrics (tokens/sec, STT/TTS latency, per-message timing)                                                                                | ✅ Complete                                                       |
| Llama.cpp settings tab (manage server binaries, GPU detection, backend switching)                                                                    | ✅ Complete                                                       |
| VRAM usage indicator (green/yellow/red with hover tooltip)                                                                                           | ✅ Complete                                                       |
| System RAM indicator (used/total percentage with hover tooltip)                                                                                      | ✅ Complete                                                       |
| Log viewer console (level filter, search, auto-refresh)                                                                                              | ✅ Complete                                                       |
| Double-copy clipboard trigger for speak-selection                                                                                                    | ✅ Complete                                                       |
| Text normalization pipeline (ITN + TN + markdown stripping)                                                                                          | ✅ Complete                                                       |
| TripleVAD (RMS → RNNoise → Silero) with tunable threshold                                                                                            | ✅ Complete                                                       |
| Crash logging with full backtraces                                                                                                                   | ✅ Complete                                                       |
| Her-style 3D loading animation                                                                                                                       | ✅ Complete                                                       |
| 20-language i18n (ar, bg, cs, de, en, es, fr, he, it, ja, ko, pl, pt, ru, sv, tr, uk, vi, zh, zh-TW)                                                 | 🟡 Partial (72% complete, missing keys in 19 non-English locales) |
| Conversation memory (context_turns, default 20 turns)                                                                                                | ✅ Complete                                                       |
| WarmEngine trait lifecycle (implemented by local backends, direct-managed in orchestrator)                                                           | ✅ Complete                                                       |
| Sentience streaming (3-fragment pattern: sentence 1 → sentence 2 → rest)                                                                             | ✅ Complete                                                       |
| TTS performance telemetry (chars_per_ms adaptive sizing)                                                                                             | ✅ Complete                                                       |
| Piper persistent HTTP server with CUDA auto-discovery                                                                                                | ✅ Complete                                                       |
| Kokoro/Kitten/Pocket persistent HTTP server with RAM persistency                                                                                     | ✅ Complete                                                       |
| Headless typed bindings export (`cargo test export_bindings`)                                                                                        | ✅ Complete                                                       |
| Engine descriptions, badges, links, test button, command preview                                                                                     | ✅ Complete                                                       |
| Process cleanup on shutdown (Drop impls, Exit handler)                                                                                               | ✅ Complete                                                       |
| Model download resilience (HTTP 416 auto-retry)                                                                                                      | ✅ Complete                                                       |
| AI Replace Selection                                                                                                                                 | ✅ Complete                                                       |
| Latency HUD (per-stage timestamps)                                                                                                                   | ✅ Complete                                                       |
| Wake word detection (VAD-based)                                                                                                                      | ✅ Complete                                                       |
| Save-to-file (MP3/OGG/FLAC)                                                                                                                          | ✅ Complete                                                       |
| Waveform HUD                                                                                                                                         | ✅ Complete                                                       |
| Ollama/LM Studio/llama.cpp auto-discovery                                                                                                            | ✅ Complete                                                       |
| Footer status indicators (STT 🟢, Brain 🟢, TTS 🟢) with hover tooltips                                                                              | ✅ Complete                                                       |
| GPU VRAM usage indicator with per-second polling                                                                                                     | ✅ Complete                                                       |
| Hands-free auto-listen / continuous voice                                                                                                            | ✅ Complete                                                       |
| Brain overlay (3D avatar + reply bubble)                                                                                                             | ✅ Complete                                                       |
| Overlay Window settings (Tauri/OS-Native mode toggle)                                                                                                | ✅ Complete                                                       |
| GPU overlay cursor trail physics (spring-friction chain, Catmull-Rom)                                                                                | ✅ Complete                                                       |
| GPU overlay wgpu native rendering (Track B)                                                                                                          | 🚧 Placeholder                                                    |
| Evolution planning (`futuristic_analysis/` supersedes `analysys/`)                                                                                   | 📋 Ongoing                                                        |
| Pocket TTS backend (voice cloning)                                                                                                                   | ✅ Complete                                                       |
| Voice barge-in (continuous voice mode)                                                                                                               | ✅ Complete                                                       |
| Streaming STT (WebSocket, EOU 120M via unified_parakeet server)                                                                                      | ✅ Partial                                                        |
| SAPI TTS backend                                                                                                                                     | ✅ Complete                                                       |
| Wake word detection (VAD-based)                                                                                                                      | ✅ Complete                                                       |
| Engine-switch cleanup (graceful unload/reload)                                                                                                       | ✅ Complete                                                       |
| Profiles (per-application settings)                                                                                                                  | 📋 Planned                                                        |
| Full-duplex conversation with acoustic echo cancellation                                                                                             | 📋 Later                                                          |
| Multi-OS polish, mobile companion                                                                                                                    | 📋 Later                                                          |
| Local speaker diarization                                                                                                                            | 📋 Later                                                          |
| MCP tool use for Brain                                                                                                                               | 📋 Later                                                          |
| Plugin/API ecosystem                                                                                                                                 | 📋 Later                                                          |

---

## Debug Mode

Press `Ctrl+Shift+D` (Windows/Linux) or `Cmd+Shift+D` (macOS) to toggle debug overlay. Also available in Advanced settings.

---

## Troubleshooting

| Issue                            | Solution                                                                                                                                    |
| -------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| piper-server not starting        | Ensure CUDA/Vulkan runtime is installed; check Debug → Log Viewer for errors                                                                |
| llama.cpp server fails to launch | Verify GPU drivers; select CPU backend in Llama.cpp settings if GPU not detected                                                            |
| STT model download stalls        | Download manually from [Hugging Face](https://huggingface.co/tdt-ai/TDT) and place in `src-tauri/resources/models/`                         |
| macOS accessibility permissions  | Grant permissions in System Preferences → Privacy & Security → Accessibility                                                                |
| Linux Wayland overlay issues     | Set `S2B2S_NO_GTK_LAYER_SHELL=1` environment variable; install `wtype` or `dotool` for text input                                           |
| Crash on startup                 | Check `s2b2s-crash.log` in app log directory; report with backtrace                                                                         |
| No audio output                  | Verify output device selection in Settings → Audio; test with Play Greeting button in TTS settings. Note: SAPI requires a Windows platform. |

---

## How to Contribute

1. **Check existing issues** at [github.com/NairoDorian/S2B2S/issues](https://github.com/NairoDorian/S2B2S/issues)
2. **Fork the repository** and create a feature branch
3. **Test thoroughly** on your target platform
4. **Submit a pull request** with clear description of changes
5. **Join the discussion** — reach out at [contact@s2b2s.computer](mailto:contact@s2b2s.computer)

See [CONTRIBUTING.md](CONTRIBUTING.md) for detailed contribution guidelines and [AGENTS.md](AGENTS.md) for AI assistant guidance.

---

## Related Projects

- **[Handy](https://github.com/cjpais/Handy)** — The original speech-to-text desktop app (MIT) that S2B2S is built upon
- **[Handy CLI](https://github.com/cjpais/handy-cli)** — Original Python command-line version
- **[Parakeet V3](https://github.com/nvidia/NeMo)** — 25-language STT model by NVIDIA (CC-BY-4.0)
- **[CopySpeak](https://github.com/yourfriendoss/copyspeak)** — TTS read-aloud patterns and warm-engine lifecycle
- **[Parrot](https://github.com/cjpais/parrot)** — Kokoro worker pool, crossfade, and markdown sanitization patterns
- **[AIVORelay](https://github.com/MaxITService/AIVORelay)** — Fork with streaming STT, profiles, and browser relay
- **[Parler](https://github.com/Melvynx/Parler)** — Fork with Gemini STT and long-audio routing

---

## License & Attribution
**S2B2S** — MIT License — see [LICENSE](LICENSE) file.

Built on [Handy](https://github.com/cjpais/Handy) by CJ Pais (MIT). Uses Parakeet V3 (CC-BY-4.0), Silero VAD, Kokoro-82M (Apache 2.0), text-processing-rs (Apache 2.0), Piper TTS, transcribe-rs, and the excellent Tauri framework.

Inspired by and incorporating patterns from: AIVORelay by MaxITService (MIT), Parler by Melvynx (MIT), Parrot by Rishi Khare (MIT), CopySpeak by ilyaizen & NairoDorian (MIT). Concepts from Whispering (AGPL-3.0), TranscriptionSuite (GPL-3.0), and Parakeet-Realtime-Transcriber (concepts only).

See [STATUS.md](STATUS.md) for the complete project status scorecard and [AGENTS.md](AGENTS.md) for AI assistant guidance.
Handy is open-source software, but the Handy name, logo, icon, and brand assets are not open-source. Unofficial forks, rewrites, and redistributions must use their own branding and must not imply endorsement or affiliation.
## Acknowledgments
- **Whisper** by OpenAI for the speech recognition model
- **ggml and transcribe.cpp** for amazing cross-platform speech-to-text inference/acceleration
- **Silero** for great lightweight VAD
- **Tauri** team for the excellent Rust-based app framework
- **Community contributors** helping make Handy better