# S2B2S Android Port Plan — On-Device Voice Assistant

This document merges the concepts from the original `S2B2S_ANDROID_COMPANION.md` and the detailed `android-port-plan.md` (v0.1.3) into a single master plan for running S2B2S locally on Android devices.

---

## 1. Architectural Strategy

S2B2S Android runs as a **fully local, on-device assistant** rather than a thin client, prioritizing privacy and offline availability.

```
+-------------------------------------------------------------+
|                       Android App Shell                      |
|                      (Kotlin / Compose)                     |
+-------------------------------------------------------------+
|  Speech Processing (sherpa-onnx)  |  Brain (llama.cpp)      |
|  - Whisper / Qwen3-ASR (STT)      |  - Gemma 4 2B (GGUF)     |
|  - Silero VAD (Voice Activity)    |  - Local KV Cache       |
|  - Kokoro / Piper (TTS)           |                         |
+-------------------------------------------------------------+
```

### Core Engine Choices

- **Speech Pipeline**: `sherpa-onnx` serves as the unified backbone for VAD (Voice Activity Detection), STT (Speech-to-Text), and TTS (Text-to-Speech). This eliminates complex Python/venv dependencies on Android.
- **LLM Brain**: Native `llama.cpp` Android integration runs GGUF models directly, ensuring complete format and feature parity with the desktop application.
- **User Interface**: Kotlin + Jetpack Compose form the native app shell, managing background audio capture and system-level overlay permissions.

---

## 2. Model Configuration

To ensure smooth performance on typical mobile chipsets (e.g., Snapdragon 8 Gen 1+), S2B2S Android uses a hardware-tuned model set:

| Task      | Model                    | Size        | Precision   | Note                         |
| --------- | ------------------------ | ----------- | ----------- | ---------------------------- |
| **VAD**   | Silero VAD v4            | ~1.7 MB     | ONNX        | Low memory, fast             |
| **STT**   | Qwen3-ASR 0.5B / Whisper | ~150-300 MB | INT8        | Streaming audio to text      |
| **Brain** | Gemma-4 2B (QAT)         | ~1.3 GB     | Q4_K_M GGUF | Speculative decoding enabled |
| **TTS**   | Kokoro-82M / Piper       | ~80-150 MB  | ONNX / VITS | Multi-voice, offline         |

---

## 3. Implementation Roadmap

### Phase 1: Native Audio & VAD Spike

- [ ] Implement audio recording in Kotlin utilizing Android's `AudioRecord` API.
- [ ] Integrate `sherpa-onnx` VAD and test silence detection in a background Service.
- [ ] Handle Android runtime audio recording permissions.

### Phase 2: Speech-to-Text & Text-to-Speech Integration

- [ ] Wire the `sherpa-onnx` speech recognition engine with Whisper or Qwen3-ASR.
- [ ] Implement offline TTS playback using Kokoro/Piper voices.
- [ ] Address Android text-to-speech output routing and focus policies.

### Phase 3: On-Device Brain (llama.cpp)

- [ ] Integrate the `llama.cpp` Android JNI bindings.
- [ ] Implement model file management (downloading and storing models in Android `filesDir`).
- [ ] Run inference test loop (STT -> llama.cpp -> TTS) on device.

### Phase 4: System Integration & Background Execution

- [ ] Create an Android Accessibility Service or Assist App integration for global system activation.
- [ ] Build background Service to maintain the voice conversation loop even when screen is locked.
- [ ] Optimize thermal/battery footprint with aggressive thread sleeping during idle states.
