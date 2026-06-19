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





________

ANDROID SECTION 

## Links & Resources

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

_____
RECENTLY ADDED


https://github.com/ferranpons/Llamatik True on-device AI for Kotlin Multiplatform (Android, iOS, Desktop, JVM, WASM). LLM, Speech-to-Text and Image Generation — powered by llama.cpp, whisper.cpp and stable-diffusion.cpp. www.llamatik.com


https://github.com/Aatricks/llmedge Android native AI inference library, bringing gguf models and stable-diffusion inference on android devices, powered by llama.cpp and stable-diffusion.cpp aatricks.github.io/llmedge/

https://github.com/Mobile-Artificial-Intelligence/maise Maise is an open-source android speech engine designed to provide a powerful and flexible platform for speech sythesis on the edge.

https://github.com/google-ai-edge/LiteRT-LM LiteRT-LM is Google's production-ready, high-performance, open-source inference framework for deploying Large Language Models on edge devices. ai.google.dev/edge/litert-lm

https://github.com/jegly/Box The most advanced, fully offline client-side AI suite on Android today. jegly.xyz

https://github.com/hung-yueh/react-native-litert-lm  High-performance on-device LLM inference for React Native, powered by LiteRT-LM and Nitro Modules
What it does: A high-performance React Native wrapper around Google’s LiteRT-LM.
Key Features: Supports Speculative Decoding using built-in model MTP heads. It features a zero-copy multimodal API using direct JSI memory access, mapping image or audio inputs straight to the inference engine for responsive interactions

https://github.com/UbiquitousLearning/mllm Fast Multimodal LLM on Mobile Devices ubiquitouslearning.github.io/mllm/
What it does: An on-device multimodal LLM execution framework with a client-server architecture built using an in-app Go backend (aar) to separate UI from inference workloads.
Optimizations: Supports Speculative Execution, model pruning, and quantization-aware pipelines across ARM CPUs, OpenCL GPUs, and Qualcomm QNN NPUs

https://github.com/pytorch/executorch On-device AI across mobile, embedded and edge for PyTorch executorch.ai
What it does: Meta's official on-device runtime for PyTorch. It allows you to compile, quantize, and run PyTorch models directly on Android and iOS.
QAT & NPU Delegation: Supports exporting models with Quantization-Aware Training (QAT). It natively supports delegation to Qualcomm's QNN (Qualcomm AI Engine Direct) backend, allowing you to run 4-bit/8-bit quantized models directly on the Hexagon NPU of Snapdragon SoCs
Reference Demo: You can reference their built-in LlamaDemo Android App which showcases Llama 3.2 quantized execution over CPU (XNNPACK) or Qualcomm QNN backends.

https://github.com/onnxruntime/onnxruntime-qnn  onnxruntime-qnn is the Qualcomm AI Runtime (QAIRT) execution provider for onnxruntime. It provides onnxruntime hardware acceleration and advanced functionalities on Qualcomm devices.