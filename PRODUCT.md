# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

- **Power users, developers, and writers**: Need instantaneous, private, zero-latency desktop dictation directly injected into whatever application or window is active.
- **Privacy-first knowledge workers**: Need a 100% offline, local alternative to cloud transcription services (Whisper API, Otter, etc.).
- **Voice-first typists & accessibility users**: Depend on high-accuracy, continuous speech recognition without cloud fatigue, subscription lock-in, or network latency.

## Product Purpose

ZER0 is an ultra-low-latency, private, desktop speech-to-text platform built for real-time dictation, model orchestration, and audio intelligence. It serves as both an everyday voice interface and the architectural root foundation for a future unified all-in-one STT + TTS + local Brain platform.

Success means speech converts into text instantaneously with zero perceived lag, running completely on-device without cloud telemetry, in a focused, distraction-free environment.

## Positioning

Unlike cloud transcription wrappers or heavy electron utilities:

1. **True Local-First Privacy**: Runs completely offline using native GGUF models (`transcribe.cpp`) and local LLMs (`llama-server`). Audio never touches a remote server.
2. **Multi-STT Parallel Consensus**: Capable of running multiple STT models concurrently in real-time, feeding their intermediate or completed hypotheses into a local LLM to produce clean, high-accuracy consensus text.
3. **Multi-Layer UI Experience**:
   - **Deep Configuration & Management Surface**: Comprehensive control over model weights, quantizations, audio devices, VAD thresholds, benchmarks, and recall vault notes.
   - **Minimalist "Épuré" Live Canvas**: Once configured, the interface recedes into an ultra-minimal, dark blank canvas showcasing only the crisp white transcription paired with circular FFT spectrum and raw audio visualizers.
4. **Sub-Millisecond System Feel**: Built with Tauri 3 and SolidJS backed by an allocation-free Rust audio pipeline and lock-free thread coordination.

## Operating Context

- **Desktop Global Utility**: Resides in the system tray and runs via global hotkeys (push-to-talk or toggle recording).
- **Active Window Injection**: Directly types or pastes transcribed text into IDEs, text editors, browsers, chat apps, and terminals.
- **Hardware Acceleration**: Scales from pure CPU execution to local GPU acceleration (CUDA, Metal, Vulkan) depending on user hardware.

## Capabilities and Constraints

- **Engine Support**: GGUF speech models via `transcribe.cpp` with native streaming (Whisper, Nemotron, R2T2) and local LLM post-processing via supervised `llama-server`.
- **Audio Processing**: High-speed voice activity detection (VAD), real-time live FFT spectral analysis (8 spectral features, linear/mel/bark scales, circular sweep scope).
- **Geometric / Aesthetic Constraint**: Sharp corners for containers and panels across the interface; avoid excessive rounding or bubbly pill aesthetics. Dark, high-contrast, distraction-free palette.
- **Identity & Renaming Architecture**: Product identity is strictly centralized in `scripts/app-meta.ts` and mirrored code constants. The codebase is architected for easy identity rebranding and extension into future speech/thought capabilities.
- **Future Scope**: Serves as the foundational base for unified Speech-to-Text (STT), Text-to-Speech (TTS), and local Brain AI memory.

## Brand Commitments

- **Product Name**: Managed exclusively through `scripts/app-meta.ts` (currently ZER0); never hardcoded as static strings in source code.
- **Visual Personality**: Sharp-edged, precision-engineered, dark, épuré, technical, and unobtrusive.
- **Attribution & Provenance**: MIT-licensed upstream project lineage preserved in license notices; distinct identity and architecture maintained in all product surfaces.

## Evidence on Hand

- Shipped live application code with complete Rust engine, custom FFT DSP pipelines, and SolidJS UI.
- Model catalog in `src-tauri/src/catalog/catalog.json`.
- Comprehensive performance and latency budgets documented in `docs/PERFORMANCE.md`.

## Product Principles

1. **Performance is a Feature**: Real-time voice requires strict latency budgets. Zero allocations on audio threads, atomic hot-toggles, and non-blocking webviews.
2. **Precision over Fluff**: Sharp edges, clean contrast, exact metrics, and instant visual feedback (spectral meters and scopes) over decorative ornamentation.
3. **Radical Privacy**: Zero cloud dependencies, zero data leakage. User voice and transcripts never leave the device.
4. **Layered Disclosure**: Deep, comprehensive controls when tuning models and hardware; invisible, razor-thin presence when doing the actual work of speaking and writing.
