# Reference GitHub Links

Curated list of STT, TTS, and voice-related open-source projects referenced by the S2B2S ecosystem.

## STT (Speech-to-Text)

- <https://github.com/cjpais/Handy/>
- <https://github.com/Melvynx/Parler>
- <https://github.com/MaxITService/AIVORelay>
- <https://github.com/TheSethRose/Parakeet-Realtime-Transcriber/>
- <https://github.com/homelab-00/TranscriptionSuite>
- <https://github.com/EpicenterHQ/epicenter/tree/main/apps/whispering>
- <https://github.com/asrjs/speech-recognition>
- <https://github.com/cjpais/transcribe-rs>
- <https://github.com/KoljaB/RealtimeSTT>
- <https://github.com/NairoDorian/S2B2S>
- <https://github.com/istupakov/onnx-asr>
- <https://github.com/k2-fsa/sherpa-onnx>
- <https://github.com/speechbrain/speechbrain>

## TTS (Text-to-Speech)

- <https://github.com/NairoDorian/copyspeak-tts/>
- <https://github.com/rishiskhare/parrot/>
- <https://github.com/mrtozner/vox>
- <https://github.com/ai-joe-git/pocket-tts-server>
- <https://github.com/cool-japan/voirs>
- <https://github.com/danielclough/vibevoice-rs>
- <https://github.com/diodiogod/TTS-Audio-Suite>

## LLM / Brain / Local AI

- <https://github.com/mudler/LocalAI>

## Voice I/O (STT + TTS)

- <https://github.com/jamiepine/voicebox>

## Utility / Other

- <https://github.com/NairoDorian/Cross_Platform_Rust_WebGPU_CursorFX>
- <https://github.com/NairoDorian/TD_Web_Trail>

---

## Android Section

### Android TTS Engines

- [SherpaTTS — System TTS Engine](https://github.com/woheller69/ttsEngine) — Piper/Kokoro/VITS as Android system TTS
- [Kokoro-82M-Android](https://github.com/puff-dayo/Kokoro-82M-Android) — Native Kokoro on Android
- [VoxSherpa-TTS](https://github.com/CodeBySonu95/VoxSherpa-TTS) — React Native TTS with sherpa-onnx
- [pocket-tts-unity](https://github.com/lookbe/pocket-tts-unity) — Pocket TTS in Unity (Android/iOS)

### Android Voice Assistants

- [NekoSpeak](https://github.com/siva-sub/NekoSpeak) — Android voice assistant
- [speech-android](https://github.com/soniqo/speech-android) — Speech pipeline for Android

### Sherpa-onnx (Unified TTS/STT C API)

- [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx) — C++ inference for Piper, Kokoro, VITS, Whisper, Zipformer
- [sherpa-onnx Android demo](https://github.com/k2-fsa/sherpa-onnx/tree/master/android) — Official Android examples

### Desktop Integration

- [S2B2S control_server.rs](src-tauri/src/control_server.rs) — Existing local HTTP server (base for mobile API)
- [Vox WebSocket protocol](https://github.com/mrtozner/vox) — Reference for voice-over-WebSocket protocol design

---

## Recently Added (On-Device Mobile AI)

- [Llamatik](https://github.com/ferranpons/Llamatik) — On-device AI for Kotlin Multiplatform (LLM, STT, and Image Generation via llama.cpp/whisper.cpp).
- [llmedge](https://github.com/Aatricks/llmedge) — Android native AI inference library for GGUF and SD models using llama.cpp.
- [maise](https://github.com/Mobile-Artificial-Intelligence/maise) — Open-source Android speech engine for edge synthesis.
- [LiteRT-LM](https://github.com/google-ai-edge/LiteRT-LM) — Google's high-performance inference framework for LLMs on edge devices.
- [Box](https://github.com/jegly/Box) — Fully offline client-side AI suite on Android.
- [react-native-litert-lm](https://github.com/hung-yueh/react-native-litert-lm) — High-performance React Native wrapper for Google's LiteRT-LM.
- [mllm](https://github.com/UbiquitousLearning/mllm) — Fast Multimodal LLM execution framework on mobile devices.
- [executorch](https://github.com/pytorch/executorch) — Meta's official on-device runtime for PyTorch (supports Qualcomm QNN delegation).
- [onnxruntime-qnn](https://github.com/onnxruntime/onnxruntime-qnn) — Qualcomm AI Runtime (QAIRT) execution provider for ONNX Runtime.
