# Vox — Independent TTS App (Category B)

> Repo: `mrtozner/vox` · Version: `0.6.0` · License: MIT OR Apache-2.0 · Author: Mert Ozoner · Platforms: macOS (Apple Silicon + Intel), Linux (x86_64), Windows (x86_64), Raspberry Pi 4/5
> Nature: independent — local-first voice AI framework built from scratch
> Role for S2B2S: Reference for Qwen3-TTS engine integration, streaming TTS architecture, trait-based backend abstraction, and Raspberry Pi optimizations

---

## 1. What Vox Is

Vox is an open-source, local-first voice AI framework written entirely in Rust. It provides a complete voice pipeline — microphone capture, voice activity detection (Silero VAD), speech-to-text (Whisper/Distil-Whisper/Sherpa-ONNX), optional speaker diarization, and text-to-speech (Kokoro/Piper/Pocket/Chatterbox/Qwen3) — all running on-device with zero cloud dependencies. It is delivered as a Rust library crate, a CLI binary (`vox listen`, `vox speak`, `vox chat`, `vox serve`), and Python bindings via PyO3/maturin.

The project targets a broad spectrum of hardware: from Apple Silicon desktops with Metal GPU acceleration all the way down to Raspberry Pi 4/5 single-board computers with INT8 quantized models. It includes an HTTP REST + WebSocket server with a 5-tab browser UI (Listen, Speak, Chat, Live Talk, Dashboard), an LRU model cache to eliminate cold-start latency, a capability registry for LLM-friendly environment introspection, and a hardware-aware `SystemProfile` that auto-selects model recommendations based on detected RAM and CPU.

---

## 2. Tech Stack

### 2.1 Core Framework

| Layer | Choice | Purpose |
|-------|--------|---------|
| Language | Rust (edition 2024, MSRV 1.85) | Full-stack systems language for ML inference + I/O |
| Async runtime | Tokio 1.x (full features) | Async audio capture, multi-backend concurrency |
| ML framework (STT) | whisper-rs (whisper.cpp bindings) | On-device Whisper transcription |
| ML framework (STT alt) | sherpa-sys (vendored C bindings) | Sherpa-ONNX SenseVoice/Zipformer streaming STT |
| ML framework (TTS) | candle-core 0.9, kokoro-tts, pocket-tts, piper-rs, chatterbox-rs, qwen3-tts | Multi-backend TTS via Candle + ONNX Runtime |
| ONNX Runtime | ort =2.0.0-rc.11 (optional) | Kokoro, Piper, Chatterbox TTS backends |
| Audio capture | cpal 0.15 | Cross-platform microphone capture |
| Audio playback | rodio 0.20 | Cross-platform speaker output |
| Resampling | rubato 0.16 | Sample rate conversion |
| WAV I/O | hound 3.5 | Audio file read/write |
| Web server | axum 0.8 + tower-http 0.6 | HTTP REST + WebSocket API |
| HTTP client | reqwest 0.12 | Ollama connectivity, model downloads |
| CLI | clap 4.5 (derive) | Command-line interface |
| Error handling | anyhow 1, thiserror 2 | Error propagation and typed errors |
| Serialization | serde 1, serde_json 1, toml 0.8 | Config, API, model metadata |
| Speaker DB | sqlx 0.8 (SQLite, runtime-tokio) | Persistent speaker embeddings for diarization |
| Profiling | tracing 0.1, tracing-subscriber 0.3 | Structured logging |
| Python bindings | PyO3 / maturin | Python package `vox_voice` |

### 2.2 Key Dependencies (non-obvious)

| Dependency | Purpose |
|------------|---------|
| `qwen3-tts` (vendored at `vendor/qwen3-tts/`) | Pure Rust Qwen3-TTS inference via Candle. NOT a wrapper — full re-implementation of the 3-stage TTS pipeline (TalkerModel, CodePredictor, Decoder12Hz) |
| `candle-core` 0.9 | Tensor operations, GPU backends (CUDA/Metal/CPU). Shared between Pocket and Qwen3 TTS backends |
| `pocket-tts` 0.6 | Kyutai Pocket TTS (100M params). Rust-native, no ONNX dependency |
| `kokoro-tts` 0.3.2 | Kokoro-82M TTS via ONNX Runtime. 57 voices across 9 languages |
| `chatterbox-rs` (vendored at `vendor/cbx/`) | Voice cloning TTS (350M params) via ONNX Runtime + optional CoreML |
| `piper-rs` (vendored at `vendor/piper-rs/`) | VITS-based multilingual TTS via ONNX Runtime. 35+ languages |
| `sherpa-sys` (vendored at `vendor/sherpa-sys/`) | C FFI bindings for Sherpa-ONNX. Auto-downloads native libs at build time via `build.rs` |
| `rustfft` 6.2 | FFT for diarization mel-spectrogram computation |
| `hf-hub` 0.4 (optional) | HuggingFace Hub model downloads with ureq sync client |

---

## 3. Architecture & Source Map

### 3.1 Top-Level Project Layout

```
vox/
├── Cargo.toml              # v0.6.0, Rust 2024 edition, ~20 feature flags
├── build.rs                # Sherpa-ONNX auto-download for Linux (incl. aarch64/Pi)
├── src/
│   ├── lib.rs              # Crate root — module declarations + "prelude" re-exports (136 lines)
│   ├── traits.rs           # Trait definitions for pluggable backends (138 lines)
│   ├── types.rs            # Core types (AudioChunk, Utterance, SttResult, TtsOutput, VoiceInfo) (87 lines)
│   ├── engine.rs           # Vox pipeline orchestrator: VoxBuilder → Vox event loop (449 lines)
│   ├── error.rs            # VoxError enum with 10 variants (51 lines)
│   ├── model_cache.rs      # LRU model cache with hit/miss tracking (355 lines)
│   ├── streaming_pipeline.rs  # Parallel STT processing with JoinSet (267 lines)
│   ├── system_profile.rs   # Hardware detection: OS/arch/RAM/Pi model detection → HardwareClass (440 lines)
│   ├── streaming_chat.rs   # Voice chat with Ollama: VAD+STT+LLM+TTS in a loop (conditional on cli/server)
│   ├── bin/vox.rs           # CLI binary entry point (208 lines)
│   ├── audio/              # Audio capture, buffer, playback, resampling
│   │   ├── mod.rs          # Module declarations (34 lines)
│   │   ├── capture.rs      # cpal microphone capture → mpsc channel
│   │   ├── buffer.rs       # Ring buffer for frame-based processing
│   │   ├── playback.rs     # rodio AudioPlayer with gapless append streaming (171 lines)
│   │   └── resampler.rs    # rubato sample rate conversion
│   ├── vad/                # Voice Activity Detection
│   │   ├── mod.rs          # Feature-gated module (10 lines)
│   │   └── silero.rs       # Silero VAD v5 via ONNX Runtime (ort crate)
│   ├── stt/                # Speech-to-Text backends
│   │   ├── mod.rs          # Feature-gated re-exports (29 lines)
│   │   ├── whisper.rs      # whisper-rs / whisper.cpp
│   │   ├── distil_whisper.rs  # Distil-Whisper (6x faster)
│   │   ├── sherpa.rs       # Sherpa-ONNX SenseVoice (batch)
│   │   └── sherpa_streaming.rs  # Sherpa Zipformer (streaming with partial results)
│   ├── tts/                # Text-to-Speech backends
│   │   ├── mod.rs          # Feature-gated module declarations (40 lines)
│   │   ├── kokoro.rs       # Kokoro-82M ONNX TTS (626 lines)
│   │   ├── piper.rs        # Piper VITS ONNX TTS (238 lines)
│   │   ├── pocket.rs       # Pocket TTS (Candle, 225 lines)
│   │   ├── chatterbox.rs   # Voice cloning TTS (ONNX)
│   │   ├── qwen3.rs        # Qwen3-TTS backend integration (603 lines)
│   │   └── streaming.rs    # SentenceStreamingAdapter for sentence-level streaming (182 lines)
│   ├── server/             # HTTP + WebSocket server (conditional on `server` feature)
│   │   ├── mod.rs          # Axum router, ServerState, backend loading (707 lines)
│   │   ├── handlers.rs     # REST endpoints
│   │   ├── ws.rs           # WebSocket endpoints (listen, speak, converse)
│   │   ├── live_talk.rs    # Barge-in voice chat WebSocket
│   │   ├── models.rs       # Server request/response types
│   │   ├── error.rs        # Server error types
│   │   └── ui.html         # Single-file embedded web UI (5 tabs)
│   ├── cli/                # CLI command handlers (conditional on `cli` feature)
│   │   ├── mod.rs          # Module declarations (12 lines)
│   │   ├── listen.rs       # `vox listen` — VAD + STT pipeline
│   │   ├── speak.rs        # `vox speak` — TTS synthesis + playback
│   │   ├── chat.rs         # `vox chat` — voice chat with Ollama
│   │   ├── serve.rs        # `vox serve` → delegates to server::run
│   │   ├── models.rs       # `vox models` subcommands
│   │   ├── benchmark.rs    # `vox benchmark`
│   │   ├── config.rs       # `vox config` interactive wizard
│   │   └── test.rs         # `vox test` audio I/O diagnostics
│   ├── capabilities/       # Environment-aware capability registry
│   │   ├── mod.rs          # CapabilityRegistry, FeatureFlags, inject_into_prompt (305 lines)
│   │   ├── hardware.rs     # GPU facts detection
│   │   ├── models.rs       # Model inventory
│   │   └── ollama.rs       # Ollama model probing
│   ├── diarization/        # Speaker identification (conditional on `diarization`)
│   ├── intelligence/       # Semantic caching, voice memory (conditional)
│   └── prompts/            # Voice-optimized LLM system prompts
├── vendor/                 # Vendored Rust crates (workspace members)
│   ├── qwen3-tts/          # Pure Rust Qwen3-TTS inference library (2208-line lib.rs)
│   │   ├── src/
│   │   │   ├── lib.rs      # Qwen3TTS facade, StreamingSession, SynthesisOptions, all 3 variants
│   │   │   ├── models/
│   │   │   │   ├── talker.rs        # TalkerModel — 28-layer transformer, MRoPE, KV cache (1030 lines)
│   │   │   │   ├── code_predictor.rs  # CodePredictor — 5-layer decoder, 15 acoustic heads (566 lines)
│   │   │   │   ├── transformer.rs    # Shared building blocks: DecoderLayer, MRoPE, Attention, MLP
│   │   │   │   ├── kv_cache.rs       # KVCache + PreAllocKVCache (InplaceOp2 zero-copy) (422 lines)
│   │   │   │   ├── fused_ops.rs      # Fused residual+RMSNorm CUDA kernel + CPU fallback (328 lines)
│   │   │   │   ├── codec/            # Audio codec: Decoder12Hz, Encoder12Hz, ConvNeXt, quantizer (10 files)
│   │   │   │   ├── speaker.rs        # ECAPA-TDNN speaker encoder (voice cloning)
│   │   │   │   ├── config.rs         # Model config, ParsedModelConfig, QuantizationConfig (875 lines)
│   │   │   │   ├── quantized.rs      # INT8 QuantizedLinear layers (conditional)
│   │   │   │   └── mod.rs            # Module exports (30 lines)
│   │   │   ├── generation/           # Generation loop support
│   │   │   │   ├── mod.rs            # GenerationConfig, SamplingContext
│   │   │   │   ├── sampling.rs       # Top-k, top-p, repetition penalty
│   │   │   │   └── tts.rs            # Token suppression mask (170 lines)
│   │   │   ├── audio/                # AudioBuffer, mel spectrogram, resampling
│   │   │   ├── tokenizer/            # HuggingFace TextTokenizer (Qwen2-0.5B)
│   │   │   ├── platform/             # ARM NEON, Pi thread pool tuning (78 lines)
│   │   │   ├── hub.rs                # HuggingFace Hub model downloads
│   │   │   └── profiling.rs          # Chrome tracing spans
│   │   ├── RASPBERRY_PI.md           # Pi deployment guide (149 lines)
│   │   ├── IMPLEMENTATION_SUMMARY.md # Pi optimizations summary (251 lines)
│   │   └── OPTIMIZATIONS.md          # Vox-specific qwen3-tts optimizations (191 lines)
│   ├── cbx/               # chatterbox-rs crate
│   ├── piper-rs/          # piper-rs crate
│   └── sherpa-sys/        # sherpa-sys FFI bindings
├── examples/              # 18 example programs (qwen3: test_streaming, test_non_streaming, test_qwen3_play)
├── tests/                 # 17 test files including qwen3_tests.rs, tts_e2e.rs
├── benches/               # 9 benchmarks including tts_comparison_bench.rs
├── scripts/               # build_for_raspberry_pi.sh, build_static.sh, download_models.sh
├── assets/                # Logo, images
└── python/                # PyO3/maturin Python bindings (workspace member)
```

### 3.2 Subsystem: Qwen3-TTS Vendor Integration

The Qwen3-TTS vendor crate (`vendor/qwen3-tts/`) is a full Rust re-implementation of Alibaba's Qwen3-TTS model using the Candle ML framework. It is NOT a thin wrapper — it implements the complete 3-stage pipeline:

1. **TalkerModel** (`models/talker.rs`, 1030 lines): 28-layer transformer decoder that generates semantic tokens from text autoregressively. Uses MRoPE (multimodal rotary position encoding), dual embeddings (text 151936-vocab → 2048-dim + codec 3072-vocab → 1024-dim), KV caching, and variant-specific prefill formats (CustomVoice, Base/voice-clone, VoiceDesign). 0.6B models have hidden=1024; 1.7B models have hidden=2048.

2. **CodePredictor** (`models/code_predictor.rs`, 566 lines): 5-layer transformer decoder that generates 15 acoustic codes per semantic token. Always 1024 hidden dim; 1.7B models add a `small_to_mtp_projection` layer to bridge from talker's 2048-dim space. Has 15 separate lm_heads and 15 separate codec embeddings (one per acoustic group).

3. **Decoder12Hz** (`models/codec/decoder_12hz.rs`): Converts 16-codebook tokens (1 semantic + 15 acoustic) to 24kHz audio via ConvNeXt blocks and transposed convolution upsampling. Shared across all model variants. Always F32.

The generation loop (`lib.rs::generate_codes`, ~100 lines) ties them together with sophisticated GPU optimizations:
- Pre-allocated KV caches with InplaceOp2 (zero-copy CUDA writes, no Tensor::cat)
- GPU-side repetition penalty mask (incremental slice_assign, eliminates growing CPU transfer)
- Deferred acoustic codes transfer (single bulk GPU→CPU at end of generation)
- Fused residual + RMSNorm CUDA kernel (`models/fused_ops.rs`)
- GPU→CPU syncs reduced from 3/frame to 1/frame (4-byte EOS check)
- Pre-built token suppression mask reused every frame

---

## 4. Feature Inventory

### 4.1 TTS Pipeline

| Feature | Implementation | Files |
|---------|---------------|-------|
| **Kokoro TTS** | 82M params, ONNX Runtime, 57 voices × 9 languages, 24kHz mono output. Uses `kokoro-tts` crate with async synth call. | `src/tts/kokoro.rs` (626 lines) |
| **Piper TTS** | VITS-based, ONNX Runtime, 35+ languages, 15-100MB per voice, 22050Hz output. Multi-speaker support via numeric ID or name lookup. | `src/tts/piper.rs` (238 lines) |
| **Pocket TTS** | 100M params, Pure Rust via Candle (no ONNX). 8 built-in voices (alba, marius, etc.). Voice cloning from WAV or pre-computed `.safetensors` embeddings. Metal/CUDA support. | `src/tts/pocket.rs` (225 lines) |
| **Chatterbox TTS** | 350M params, ONNX Runtime + optional CoreML. Voice cloning from reference audio. | `src/tts/chatterbox.rs` |
| **Qwen3 TTS** | 0.6B and 1.7B models, 20 voices × 10 languages, streaming synthesis. CPU/Metal/CUDA. | `src/tts/qwen3.rs` (603 lines) |
| **Sentence Streaming Adapter** | Wraps any batch TtsBackend to provide sentence-level streaming. Splits on `.!?;` followed by whitespace, preserves decimals. Uses `std::thread::spawn` + `handle.block_on()` for parallel sentence synthesis. | `src/tts/streaming.rs` (182 lines) |
| **Audio Playback** | rodio-based `AudioPlayer` with dedicated thread. Supports fire-and-forget play, blocking play, and **gapless append** via persistent `rodio::Sink` (chunks queued back-to-back). | `src/audio/playback.rs` (171 lines) |
| **Model Cache** | LRU cache (`ModelCache`) wraps `Arc<Mutex<HashMap>>` with hit/miss tracking and LRU eviction. Shared between STT and TTS backends. | `src/model_cache.rs` (355 lines) |
| **TTS Comparison Benchmarks** | Criterion benchmarks for RTF (real-time factor) across Kokoro, Pocket, Chatterbox with short and long text. | `benches/tts_comparison_bench.rs` (271 lines) |

### 4.2 Qwen3-TTS Specific Features

| Feature | Implementation | Files |
|---------|---------------|-------|
| **Voice selection** | 20 custom voice IDs mapped to `(Speaker, Language)` pairs. US Male voices mapped to Aiden (NOT Ryan) due to upstream EOS generation bug in 0.6B model. Full `voice_list()` with gender, language, accent metadata. | `src/tts/qwen3.rs` lines 181-379 |
| **Streaming synthesis** | `synthesize_with_streaming()` wraps `Qwen3TTS::synthesize_streaming()` behind a callback API. Uses `spawn_blocking` for CPU-bound work, yields ~800ms chunks. | `src/tts/qwen3.rs` lines 382-482 |
| **Device auto-detection** | Metal → CUDA → CPU priority chain. Configurable via `VOX_QWEN3_DEVICE` env var. Metal auto-available on macOS, CUDA checked via compile-time feature flag. | `src/tts/qwen3.rs` lines 40-77 |
| **Model path resolution** | Tries 3 HF cache formats (new `snapshots/main`, old `hub/Org/Repo`, and `VOX_QWEN3_MODEL_PATH` env override). Shows actionable error message with download command on failure. | `src/tts/qwen3.rs` lines 119-151 |
| **GPU optimizations** (vendored) | Pre-allocated KV cache (InplaceOp2), GPU-side repetition penalty mask, Fused residual+RMSNorm CUDA kernel, single bulk GPU→CPU transfer for all acoustic codes, pre-built suppression mask. Achieves 0.48-0.67 RTF on NVIDIA DGX Spark (non-streaming). | `vendor/qwen3-tts/src/lib.rs` + `models/kv_cache.rs` + `models/fused_ops.rs` |
| **Model variant auto-detection** | Parses `config.json` to detect model type (Base/CustomVoice/VoiceDesign), size (0.6B/1.7B), talker config, and code predictor config. Falls back to weight shape inspection. | `vendor/qwen3-tts/src/models/config.rs` (875 lines) |
| **StreamingSession** | Iterator that yields `AudioBuffer` chunks. Maintains KV cache, penalty mask, suppression mask, code predictor KV caches across chunk boundaries. Full barge-in support potential. | `vendor/qwen3-tts/src/lib.rs` lines 1484-1786 |
| **INT8 quantization** (future) | `QuantizationConfig`, `QuantizedLinear` layer, `quantized` feature flag. Targets 50% memory reduction (1.7GB → 850MB for 0.6B). Pi-specific config via `QuantizationConfig::for_raspberry_pi()`. | `vendor/qwen3-tts/src/models/quantized.rs` + `config.rs` |

### 4.3 STT Pipeline

| Feature | Implementation | Files |
|---------|---------------|-------|
| Whisper STT | whisper-rs bindings. Models: tiny.en (75MB), base.en (142MB), small.en (466MB), medium.en (1.5GB). | `src/stt/whisper.rs` |
| Distil-Whisper STT | 6x faster than Whisper with minimal accuracy loss. Same model sizes. | `src/stt/distil_whisper.rs` |
| Sherpa STT | SenseVoice (multilingual zh/en/ja/ko/yue) at 230MB. | `src/stt/sherpa.rs` |
| Sherpa Streaming STT | Zipformer transducer (27MB) with partial results. | `src/stt/sherpa_streaming.rs` |
| StreamingSttBackend trait | `create_session()` → `SttSession` with `push_audio()` + `finish()`. | `src/traits.rs` lines 84-114 |

### 4.4 Voice Activity Detection

| Feature | Implementation | Files |
|---------|---------------|-------|
| Silero VAD v5 | ONNX Runtime (ort 2.0.0-rc.11). 2MB model. Frame size ~512 samples at 16kHz. Emits `SpeechStart`, `SpeechEnd(Utterance)`, `Silence` events. | `src/vad/silero.rs` |

### 4.5 Server / Web Features

| Feature | Implementation | Files |
|---------|---------------|-------|
| REST API | 9 endpoints: transcribe, synthesize, voices, ollama-models, capabilities, models, stats, cache/stats, health. 50MB body limit. | `src/server/mod.rs` + `handlers.rs` |
| WebSocket /v1/listen | Real-time STT with VAD. Sends speech_start, partial, transcript, speech_end JSON events. Optional speaker diarization. | `src/server/ws.rs` |
| WebSocket /v1/speak | Streaming TTS. Receive JSON, send chunked PCM audio. | `src/server/ws.rs` |
| WebSocket /v1/converse | Continuous voice chat (VAD+STT+LLM+TTS loop). | `src/server/ws.rs` |
| WebSocket /v1/live-talk | Barge-in voice chat. Full-duplex — interrupt LLM mid-response. | `src/server/live_talk.rs` |
| Web UI | Single embedded `ui.html` served by Rust binary. 5 tabs. No separate frontend build. | `src/server/ui.html` |
| Model cache API | GET /v1/cache/stats returns hits, misses, entries, hit_rate. | `src/model_cache.rs` |
| Capability registry | GET /v1/capabilities returns hardware, models, Ollama models, feature flags. Injected into LLM system prompts. | `src/capabilities/mod.rs` (305 lines) |

### 4.6 Configuration & Platform Features

| Feature | Implementation | Files |
|---------|---------------|-------|
| SystemProfile | Auto-detects OS, arch, Raspberry Pi model (via `/proc/device-tree/model`), RAM (via `/proc/meminfo` or `sysctl`), CPU count. Classifies into 5 tiers: Tiny, Constrained, Small, Medium, Large. Recommends Ollama model, Whisper model, inference threads per tier. | `src/system_profile.rs` (440 lines) |
| Feature flags | 21 compile-time flags: whisper, distil-whisper, silero, sherpa, kokoro, piper, pocket, chatterbox, qwen3, qwen3-metal, qwen3-cuda, pocket-metal, chatterbox-coreml, diarization, intelligence, cli, server, tts. Auto-GPU: on macOS, `qwen3` or `pocket` features enable Metal automatically. | `Cargo.toml` lines 73-102 |
| Build script | Auto-download Sherpa-ONNX native libs for Linux (including aarch64/Pi). Respects `VOX_SHERPA_NO_AUTODOWNLOAD` and `CARGO_NET_OFFLINE`. | `build.rs` (338 lines) |
| Static musl build | Dockerfile producing fully static binary for scratch/distroless containers. | `Dockerfile.static` (133 lines) |
| Cross-compilation | `scripts/build_for_raspberry_pi.sh` sets up aarch64-unknown-linux-gnu toolchain and builds with `server,qwen3,quantized` features. | `scripts/build_for_raspberry_pi.sh` (47 lines) |

### 4.7 Speaker Diarization (Experimental)

| Feature | Implementation | Files |
|---------|---------------|-------|
| ECAPA-TDNN encoder | 512-dim voice embeddings from 24kHz audio. CMVN normalization, EMA embedding adaptation. | `src/diarization/` |
| SQLite speaker DB | Persistent speaker storage with embedding version tracking. Auto-clears stale embeddings on preprocessing changes. | `src/server/mod.rs` lines 536-663 |
| Cosine similarity matching | Threshold 0.35 for speaker recognition. Tableau 10 colorblind-friendly palette. | `src/diarization/` |

---

## 5. Key Code Patterns & Techniques

### 5.1 Trait-Based Backend Abstraction (like embedded-hal)

Vox defines clean, minimal traits in `src/traits.rs` (138 lines):

```rust
#[async_trait]
pub trait VadBackend: Send + Sync { ... }
pub trait SttBackend: Send + Sync { ... }
pub trait TtsBackend: Send + Sync { ... }
pub trait StreamingSttBackend: Send + Sync { ... }
pub trait StreamingTtsBackend: Send + Sync { ... }
```

Key design decisions:
- All traits require `Send + Sync` — backends are shared via `Arc<dyn Trait>` across threads.
- `SttBackend` and `TtsBackend` take `&self` (not `&mut self`), enabling concurrent calls from multiple tokio tasks.
- `VadBackend` takes `&mut self` (stateful VAD state per frame).
- Backends use `spawn_blocking` internally for CPU-bound inference, keeping the async runtime responsive.
- `StreamingTtsBackend` creates `TtsSession` objects that yield `TtsChunk` structs with `samples`, `sample_rate`, and `progress` (0.0-1.0).

### 5.2 Builder Pattern for Pipeline Configuration

`VoxBuilder` (`src/engine.rs`) uses a fluent builder pattern:
- `config()`, `vad()`, `stt()`, `tts()`, `streaming_stt()`, `on_partial()`, `on_utterance()`, `build()`
- `VoxContext` passed to callbacks provides `speak()`, `speak_and_play()`, and `stats()` methods.

### 5.3 Streaming TTS via Sentence-Level Adapter

`SentenceStreamingAdapter` (`src/tts/streaming.rs`):
- Splits text on `.!?;` followed by whitespace (preserves decimals like "3.50").
- Uses `std::thread::spawn` (NOT `spawn_blocking`) because `handle.block_on()` panics inside tokio runtime.
- Uses `sync_channel` with capacity 2 for backpressure.
- Known limitation: prosody breaks at sentence boundaries since each sentence is synthesized independently.

### 5.4 Gapless Audio Playback

`AudioPlayer` (`src/audio/playback.rs`):
- Dedicated thread owns `rodio::OutputStream` + persistent `rodio::Sink`.
- `append()` queues audio chunks to the same sink — no gap between chunks.
- `wait_until_done()` blocks until all queued audio finishes.
- Channel-based communication with `mpsc::Sender<PlaybackCommand>` for thread safety.

### 5.5 GPU Optimization Techniques (Qwen3-TTS vendor)

The vendored `qwen3-tts` library implements production-grade GPU optimizations:

1. **Pre-allocated KV Cache** (`models/kv_cache.rs`): `PreAllocKVCache` uses `InplaceOp2` + `copy2d` on CUDA for zero-allocation generation. On Metal/CPU, falls back to `slice_set`.

2. **GPU-side Repetition Penalty Mask** (`lib.rs` lines 539-674): A `[1, vocab]` F32 tensor updated incrementally via `slice_assign` with pre-built scalar ones. Eliminates the O(n) GPU→CPU transfer that grows with each frame.

3. **Fused Residual + RMSNorm CUDA Kernel** (`models/fused_ops.rs`): Custom PTX kernel that fuses `rms_norm(x + residual)` into a single GPU kernel launch. Falls back to sequential `add` + `rms_norm` on CPU/Metal.

4. **Deferred Acoustic Codes Transfer** (`lib.rs` lines 574-610, 656-657): Accumulates all frame codes as GPU tensors during generation, then does a single `Tensor::stack` + `to_vec1` at the end. Eliminates 15 per-frame GPU→CPU transfers.

5. **Pre-built Suppression Mask** (`generation/tts.rs`): Boolean mask `[1, 3072]` that suppresses reserved control tokens (range 2048-3071 except EOS 2150). Built once, applied cheaply each frame via `where_cond`.

6. **BF16 compute on GPU**: Talker + CodePredictor run in BF16 on CUDA/Metal (2x memory savings vs F32). Decoder and speaker encoder always F32 (convolutional, no attention).

### 5.6 Hardware-Aware Defaults

`SystemProfile` (`src/system_profile.rs`):
- Detects Raspberry Pi model via `/proc/device-tree/model`.
- RAM via `/proc/meminfo` (Linux) or `sysctl hw.memsize` (macOS).
- Maps to 5 hardware classes with per-class model recommendations.
- `recommended_ollama_model()`: None for Tiny, qwen2.5:0.5b for Constrained, up to llama3.2:3b for Large.
- `recommended_whisper_model()`: tiny.en for Tiny/Constrained, base.en for Small/Medium, small.en for Large.
- `recommended_inference_threads()`: leaves 1 core free on systems with >2 CPUs.

### 5.7 Error Handling

- `VoxError` enum (`src/error.rs`) with 10 variants: Audio, Vad, Stt, Tts, Diarization, IntegrityCheckFailed, NoStt, NoVad, ModelNotFound(PathBuf), Pipeline, Io.
- Uses `thiserror` derive for Display and `anyhow` for propagation.
- Server module has separate `error.rs` with HTTP-specific error types.

---

## 6. Relation to S2B2S

S2B2S is a Tauri 2.x desktop app (Rust backend + React/TypeScript frontend) with 9 TTS backends. Vox is a pure-Rust library/framework with 5 TTS backends and a web interface. Both are local-first voice applications.

### 6.1 Comparison Table

| Aspect | Vox (This Project) | S2B2S | Verdict |
|--------|-------------------|-------|---------|
| **Framework** | Pure Rust library + CLI binary | Tauri 2.x (Rust + React/TS + Vite) | S2B2S has full desktop UI; Vox is embeddable library |
| **TTS Backends** | Kokoro, Piper, Pocket, Chatterbox, **Qwen3** | Piper (persistent HTTP server), Kokoro, Kitten, Pocket, SAPI (stub), OpenAI, ElevenLabs, Cartesia | Vox has unique Qwen3 integration; S2B2S has cloud backends + SAPI |
| **TTS Engine Abstraction** | `TtsBackend` trait with `synthesize()` + `StreamingTtsBackend` for chunked output | `TtsBackend` trait with `synthesize()`, `list_voices()`, warm-persistent lifecycle | Very similar — both use async traits + list_voices |
| **TTS Streaming** | `StreamingSession` iterator from Qwen3 (native), `SentenceStreamingAdapter` for batch backends, `AudioPlayer.append()` for gapless | Sentence splitter in brain `client.rs`, fragment queue (unused), streaming gapless `Player` (rodio) | Both have sentence-level streaming + gapless rodio playback |
| **Qwen3-TTS** | Full integration with 0.6B/1.7B, voice cloning, streaming, Metal/CUDA | Not available | Vox is the reference for adding Qwen3 to S2B2S |
| **Architecture** | Trait-based `embedded-hal` style. Builder pattern. `Arc<dyn Trait>` sharing. | Manager pattern (AudioManager, ModelManager, etc.). Tauri state. WarmEngine trait for model lifecycle. | Vox simpler/fewer abstractions; S2B2S more production-ready with lifecycle |
| **Audio Pipeline** | VAD → STT → callback → optional TTS. Single event loop. | TripleVAD (RMS→RNNoise→Silero), ITN/TN text processing, LLM "brain", 5-stage sanitize pipeline | S2B2S has much more sophisticated text processing |
| **Model Management** | Auto-download on first use, LRU cache, `vox models` CLI | Model download scripts, download progress in UI, model store (Zustand) | S2B2S has richer model UX |
| **Hardware Awareness** | `SystemProfile` with 5-tier hardware classification, auto model recommendations | Platform detection via `utils.rs`, cfg-gated code paths | Vox's hardware classification is more systematic |
| **Raspberry Pi** | Dedicated Pi docs, INT8 quantization, NEON optimizations (WIP), cross-compilation scripts | Not a target | Vox leads on edge deployment |
| **Text Normalization** | None (raw text to TTS) | ITN (inverse text normalization), TN (text normalization), markdown strip, regex cleanup — full 5-stage pipeline | S2B2S vastly more sophisticated |
| **WebSocket API** | 4 channels (listen, speak, converse, live-talk) + 9 REST endpoints | axum control server with a few endpoints | Vox has richer API surface |
| **Frontend** | Single embedded HTML file, no build step | Full React 19 + Vite SPA with Tailwind, Three.js, Zustand, i18n (20 languages) | S2B2S much richer frontend |
| **Error Handling** | `VoxError` enum (10 variants), anyhow, thiserror | anyhow, thiserror, managers use `Arc<Mutex<T>>` | Similar patterns |
| **Line Count** | ~12,000+ lines (including vendored crates) | ~50,000+ lines (Rust + TypeScript) | S2B2S is a larger, more mature project |
| **Platform Support** | macOS, Linux, Windows (CI tested), Raspberry Pi | Windows 11 (top priority), macOS, Linux | Similar cross-platform commitment |
| **Voice Cloning** | Chatterbox (350M ONNX) + Qwen3 Base (ECAPA-TDNN) + Pocket (WAV cloning) | None | Vox has unique voice cloning capability |

### 6.2 What S2B2S Does Better

1. **Text processing pipeline**: S2B2S's 5-stage sanitize pipeline (ITN, TN, markdown strip, fuzzy correction, regex cleanup) is far more sophisticated than Vox's raw-text-to-TTS approach. Vox has no text normalization whatsoever.

2. **VAD sophistication**: S2B2S's TripleVAD (RMS → RNNoise probability → Silero) with tunable RNNoise threshold (0.05-0.9) is more robust than Vox's single Silero VAD.

3. **Desktop UI**: S2B2S has a full React/TypeScript desktop app with settings panels, conversation window, onboarding flow, 20-language i18n, and Three.js animations. Vox's web UI is a single embedded HTML file.

4. **Cloud TTS backends**: S2B2S supports OpenAI, ElevenLabs, and Cartesia cloud TTS for users who want higher quality than local models can provide.

5. **Model lifecycle**: S2B2S's `WarmEngine` trait (Loading → WarmingUp → Ready → Error) provides proper async model lifecycle management. Vox loads models synchronously in `spawn_blocking`.

6. **Persistent settings**: S2B2S uses tauri-plugin-store for reactive settings persistence. Vox has no settings store — configuration is CLI-only or hardcoded.

7. **Security**: S2B2S has crash logging, secret scanning (GitHub), and proper API key management. Vox has no security features.

### 6.3 What Vox Does Better

1. **Qwen3-TTS integration**: Complete, production-ready Pure Rust implementation with streaming, voice cloning, GPU acceleration for Metal/CUDA/CPU. S2B2S has no equivalent.

2. **Trait abstraction purity**: Vox's traits are cleaner and simpler — `TtsBackend` with just `synthesize()` and `list_voices()`. S2B2S's `TtsBackend` has warm-up lifecycle and engine status that adds complexity.

3. **Streaming TTS adapter**: `SentenceStreamingAdapter` wraps any batch backend for sentence-level streaming. S2B2S has `SentenceSplitter` in the brain module but it's tied to LLM output, not a general TTS adapter.

4. **Gapless playback**: `AudioPlayer.append()` with persistent `rodio::Sink` is simpler and more correct than S2B2S's approach.

5. **Hardware detection**: `SystemProfile` is more comprehensive and useful than S2B2S's platform detection.

6. **Edge deployment**: Raspberry Pi support with INT8 quantization, NEON optimizations, cross-compilation scripts. S2B2S doesn't target edge devices.

7. **Library-first design**: Vox is a proper Rust library crate usable by other projects. S2B2S is a Tauri app first, library second (though `audio_toolkit` is a reusable module).

8. **LRU model cache**: Eliminates cold-start latency by keeping models warm in memory. S2B2S relies on persistent HTTP servers for model lifecycle but doesn't cache loaded models.

9. **Capability registry**: Injects hardware facts into LLM prompts so the assistant knows what it can do. S2B2S has no equivalent.

---

## 7. Harvest List (Features Worth Copying)

| Feature to harvest | From file | Effort (XS/S/M/L/XL) | Why valuable for S2B2S |
|-------------------|-----------|----------------------|------------------------|
| **Qwen3-TTS backend** | `vendor/qwen3-tts/` (whole crate) | L | State-of-the-art TTS quality, 10 languages, streaming, voice cloning. Would be S2B2S's highest-quality local TTS engine. |
| **Qwen3 TTS adapter for S2B2S** | `src/tts/qwen3.rs` (603 lines) | M | Already implemented the integration pattern — `TtsBackend` trait impl, voice mapping, streaming adapter. Can adapt directly. |
| **SentenceStreamingAdapter** | `src/tts/streaming.rs` (182 lines) | S | Generic wrapper to make any batch TTS backend streaming. Useful for S2B2S's Piper/Kokoro/Pocket backends. |
| **Gapless AudioPlayer with append()** | `src/audio/playback.rs` (171 lines) | S | Vox's gapless playback via persistent sink is simpler and more correct. Could replace or supplement S2B2S's `player.rs`. |
| **SystemProfile hardware classification** | `src/system_profile.rs` (440 lines) | M | Auto-detect Raspberry Pi model, RAM class, and recommend models. Useful for S2B2S's onboarding/setup wizard. |
| **ModelCache with LRU eviction** | `src/model_cache.rs` (355 lines) | S | LRU cache for loaded models. Reduces cold-start latency. Could supplement S2B2S's persistent HTTP server approach. |
| **CapabilityRegistry** | `src/capabilities/mod.rs` (305 lines) | S | Hardware + model + feature introspection injected into LLM prompts. Useful for S2B2S's brain/conversation feature. |
| **Build-time Sherpa-ONNX auto-download** | `build.rs` (338 lines) | M | Auto-downloads native Sherpa-ONNX libs for Linux/aarch64. S2B2S could use this pattern for other native deps. |
| **GPU optimization patterns** | `vendor/qwen3-tts/src/models/kv_cache.rs`, `fused_ops.rs` | L | Pre-allocated KV caches, fused kernels, GPU-side penalty masks. Advanced — could apply to S2B2S's llama.cpp integration. |
| **INT8 quantization config** | `vendor/qwen3-tts/src/models/quantized.rs`, `config.rs` | M | Quantization infrastructure for 50% memory reduction. Valuable if S2B2S ever targets edge devices. |

---

## 8. Known Issues, Caveats & Limitations

| Issue | Severity | Impact |
|-------|----------|--------|
| **Ryan speaker EOS bug in 0.6B CustomVoice** | High | Ryan speaker generates 163+ seconds of audio instead of stopping. Mitigated by safety limit (512 frames / 40s max) and mapping US Male voices to Aiden instead. Root cause not fixed. |
| **No text normalization** | Medium | Vox passes raw text to TTS without any preprocessing. Numbers, abbreviations, symbols may produce unnatural speech. S2B2S's ITN/TN pipeline would improve quality significantly. |
| **Qwen3 INT8 quantization incomplete** | Medium | `quantized` feature flag and `QuantizationConfig` exist, but actual model loading path from quantized weights is not yet implemented. Pi INT8 estimates are projections, not measurements. |
| **Sentence streaming prosody breaks** | Low | `SentenceStreamingAdapter` synthesizes each sentence independently. Cross-sentence intonation context is lost. Documented limitation with workaround (both chunks synthesize fine). |
| **No settings persistence** | Medium | Vox has no config file or persistent settings. All configuration is CLI arguments or environment variables. Inconvenient for server deployments. |
| **Single-model-at-a-time TTS** | Low | Only one TTS backend is loaded at a time (the "primary" detected at startup). Users can't switch between Kokoro and Qwen3 without restarting. S2B2S supports multiple simultaneously loaded backends. |
| **`is_cuda_available()` stub** | Low | `Qwen3Config::is_cuda_available()` returns `false` unconditionally — it's a TODO stub. Real CUDA detection would require probing the Candle device. |
| **ICL mode ICL_MIN_REPETITION_PENALTY high** | Low | ICL voice cloning uses a minimum repetition penalty of 1.5 (matching mlx-audio reference). This may produce unnatural prosody. |
| **Speaker diarization experimental** | Medium | Marked `(experimental)` in docs. V4: 5+ IDs for 2 people, CMVN fix applied. Still in active development. |

---

## 9. Strengths & Weaknesses

### Strengths

1. **Clean, minimal trait-based architecture** — 138 lines of trait definitions cover all backend types. Easy to understand, easy to extend.

2. **Qwen3-TTS integration is best-in-class** — Full Rust re-implementation with streaming, voice cloning, GPU optimizations, and 5 model variants auto-detected from config.json. Achieves 0.48-0.67 RTF on NVIDIA hardware.

3. **Raspberry Pi first-class citizen** — Dedicated hardware detection, INT8 quantization path, NEON optimizations, cross-compilation scripts, thread pool tuning for 3-core inference.

4. **Library-first design** — Usable as a Rust crate with clean public API (`Vox::builder().vad().stt().on_utterance().build()`). Also provides Python bindings.

5. **Comprehensive testing** — 17 test files, 9 benchmarks, test coverage for safety limits, voice mapping, suppression masks, KV cache, quantization config, and hardware profile.

6. **Gapless playback via persistent sink** — `AudioPlayer.append()` with single `rodio::Sink` is the correct approach for streaming chunked audio.

7. **No Python dependency for TTS** — Everything in Rust via Candle or ONNX Runtime. No subprocess management, no venv. S2B2S requires Python venv for Piper/Kokoro/Kitten/Pocket.

8. **Embedded web UI** — Single HTML file, no separate frontend build. Functional for testing and demos.

9. **Rich WebSocket API** — 4 WebSocket channels covering real-time STT, streaming TTS, continuous voice chat, and barge-in conversation.

### Weaknesses

1. **No text preprocessing** — Raw text to TTS with no normalization. Numbers ("42" not "forty-two"), abbreviations, and special characters produce unpredictable output.

2. **No persistent configuration** — CLI-only or env-var configuration. No settings file makes server deployment awkward.

3. **Single TTS backend at a time** — Can't hot-switch between Kokoro and Qwen3. The server loads one primary TTS backend and optionally a secondary Piper for conversation.

4. **Experimental features** — Diarization, Live Talk, and Intelligence are all marked experimental. Some have known bugs (speaker fragmentation).

5. **SAPI backend missing** — Unlike S2B2S which plans to support SAPI (stub), Vox has no platform-native TTS fallback. On Windows without models downloaded, TTS is simply unavailable.

6. **No cancel/interrupt for TTS** — Once synthesis starts, it runs to completion. No barge-in at the TTS level (only at the conversation level via Live Talk WebSocket).

7. **Qwen3 model download UX poor** — The error message tells users to run `huggingface-cli download` manually. No automatic download integration (unlike Pocket which downloads from HF on first use).

8. **Limited cloud integration** — No OpenAI, ElevenLabs, or Cartesia backends. For users who want cloud TTS quality, Vox offers no path. S2B2S supports all three.

---

## 10. Bottom Line / Verdict

Vox is an impressive local-first voice AI framework that punches above its weight, particularly in TTS. Its Qwen3-TTS integration is the most complete open-source Rust implementation available — a full re-implementation of the 3-stage TalkerModel/CodePredictor/Decoder12Hz pipeline with production-grade GPU optimizations. For S2B2S, the single most valuable takeaway is the **Qwen3-TTS vendor crate** (`vendor/qwen3-tts/`) and the **integration adapter** (`src/tts/qwen3.rs`), which together provide a ready-made blueprint for adding state-of-the-art 24kHz streaming TTS with voice cloning and multilingual support. The second most valuable pattern is the `SentenceStreamingAdapter` — a 182-line generic wrapper that converts any batch TTS backend into a sentence-level streaming backend, directly applicable to S2B2S's existing Piper/Kokoro/Pocket backends. Vox is well worth studying as a reference for clean trait design, gapless audio playback, and edge deployment strategies, even though S2B2S is objectively more mature in its text processing pipeline, desktop UI, and production feature set.
