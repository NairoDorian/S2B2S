# <img src="logo.png" alt="S2B2S" width="48" height="48" style="vertical-align: middle" /> S2B2S — SpeechToBrainToSpeech

**Local-first STT → Brain → TTS desktop app for Windows 11. Dictate anywhere, read anything aloud, and talk naturally with a local AI — almost keyboard-free.**

S2B2S is a cross-platform desktop application that combines speech-to-text (STT), a local or cloud "Brain" (LLM), and text-to-speech (TTS) into one unified voice-native experience. Based on the battle-tested [Handy](https://github.com/cjpais/Handy) skeleton (MIT), rebranded and extended toward a full voice conversation pipeline.

## Why S2B2S?

- **Local-first**: Everything works offline. Parakeet V3 for STT, Piper for TTS, Ollama/LM Studio for the Brain. No cloud required.
- **Open Source (MIT)**: Forkable, inspectable, extendable.
- **Private**: Your voice, text, and conversations stay on your machine.
- **Voice-native**: Designed for spoken interaction — not a text chat with voice bolted on.

## How It Works

1. **Dictate Anywhere** — press a hotkey, speak, and polished text lands at your cursor. Powered by **Parakeet V3** (default, local, 25 languages with auto-detection).
2. **Read Aloud** — select text anywhere, press a hotkey, and a local voice reads it instantly with pause/resume.
3. **Talk to the Brain** — the Conversation window: speak naturally to a local LLM (Ollama/LM Studio) or any cloud LLM. Real-time STT in, streaming tokens out, TTS reads the reply aloud (toggleable, default ON).

## Quick Start

### Installation

1. Download the latest release from the [releases page](https://github.com/NairoDorian/S2B2S/releases)
2. Install and grant microphone permissions
3. On first run, download **Parakeet V3** (~0.6 GB) — the default and recommended STT model
4. Configure your hotkeys and start transcribing!

### Development Setup

```bash
# Prerequisites: Rust, Bun
bun install
bun run tauri dev

# Build
bun run tauri build
```

For detailed build instructions, see [BUILD.md](BUILD.md).

## Architecture

S2B2S is built as a Tauri 2 application:

- **Frontend**: React + TypeScript + Tailwind CSS
- **Backend**: Rust for system integration, audio processing, and ML inference
- **Core Libraries**: transcribe-rs (Parakeet V3 + Whisper), cpal (audio I/O), vad-rs (Voice Activity Detection)

### Default STT Model: Parakeet V3

Parakeet TDT 0.6B V3 is the default and recommended STT engine:
- CPU-optimized, ~5x real-time on mid-range hardware
- 25 languages with automatic detection
- No GPU required (DirectML/CUDA optional for larger models)

### The Pipeline

```
Microphone → VAD (Silero) → Parakeet V3 STT → LLM Post-Processing (Brain) → Clipboard/Paste
                                                                           → TTS (Piper) Read Aloud
```

### CLI Parameters

```bash
s2b2s --toggle-transcription    # Toggle recording on/off
s2b2s --toggle-post-process     # Toggle with post-processing
s2b2s --cancel                  # Cancel current operation
s2b2s --start-hidden            # Start minimized to tray
s2b2s --no-tray                 # Start without tray icon
s2b2s --debug                   # Enable debug logging
```

## Platform Support

- **Windows 11** (primary)
- Windows 10, macOS, Linux

## Roadmap

S2B2S is the foundation of the SpeechToBrainToSpeech vision:

- **Now**: Full STT dictation with Parakeet V3 (from Handy base)
- **Next**: TTS read-aloud (Piper/Kokoro integration)
- **Then**: Conversation mode with local Brain (Ollama/LM Studio/llama.cpp streaming)
- **Later**: Multi-OS polish, streaming live captions, mobile companion

See the full planning documents in the `docs/` directory of the planning repository.

## License

MIT License — see [LICENSE](LICENSE) file.

## Acknowledgments

Built on [Handy](https://github.com/cjpais/Handy) by CJ Pais (MIT). Uses Parakeet V3 (CC-BY-4.0), Silero VAD, transcribe-rs, and the excellent Tauri framework. Inspired by the open-source voice tools community.
