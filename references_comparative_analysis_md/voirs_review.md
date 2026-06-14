# VoiRS — Research/Reference TTS Framework

> Repo: `cool-japan/voirs` · HEAD: v0.1.0-rc.1 · License: Apache-2.0 · Author: Tetsuya Kitahata (COOLJAPAN OU)
> Nature: **framework** — Pure-Rust neural speech synthesis framework with 16-crate Cargo workspace
> Role for S2B2S: **Reference architecture** — study modular TTS pipeline design, training infrastructure, trait-based engine abstraction, ONNX integration patterns, and advanced TTS features (cloning, emotion, singing, spatial)

---

## 1. What VoiRS Is

VoiRS is a cutting-edge, pure-Rust neural Text-to-Speech (TTS) framework built by the COOLJAPAN team. It unifies the cool-japan ecosystem crates (SciRS2, NumRS2, TrustformeRS) into a cohesive neural speech synthesis solution. The project aims to achieve state-of-the-art naturalness (MOS 4.4+) with competitive real-time performance (0.25x RTF on CPU, 0.03x on GPU), all in memory-safe Rust with no Python dependencies at runtime.

The framework targets a broad range of use cases: edge AI, assistive technology, media production, conversational AI, gaming, mobile apps, and research. It provides complete training infrastructure for DiffWave vocoders, VITS acoustic models, and FastSpeech2, using SafeTensors checkpoint format with 370 trainable parameters per model.

VoiRS is a **framework for developers**, not an end-user application. It exposes a Rust SDK (`voirs-sdk`), a CLI tool (`voirs-cli`), C/Python FFI bindings (`voirs-ffi`), and WASM support. It is designed to be embedded into other applications as a library.

---

## 2. Tech Stack

### 2.1 Core ML Framework

| Layer | Choice | Purpose |
|-------|--------|---------|
| Neural networks | Candle 0.9.2 (candle-core, candle-nn, candle-transformers) | Pure Rust ML framework from HuggingFace |
| ONNX runtime | OxiONNX 0.1.0 (custom) + tract-onnx 0.22.1 | Pure Rust ONNX model inference; 88 operators, graph optimizations, wgpu GPU backend |
| Checkpoint format | SafeTensors 0.7.0 | Production-grade model persistence (30MB per checkpoint, 370 parameters) |
| Model hub | hf-hub 0.5.0 | HuggingFace model download |

### 2.2 Audio & DSP

| Layer | Choice | Purpose |
|-------|--------|---------|
| Audio I/O | cpal 0.17.3 | Cross-platform audio device enumeration and streaming |
| Audio file formats | hound (WAV), claxon (FLAC), symphonia (codec suite), lewton (OGG Vorbis), minimp3 (MP3), mp3lame-encoder (MP3), opus, mp4 | Comprehensive codec support |
| Scientific computing | scirs2-core 0.3.4 (array, random, SIMD, parallel) | Unified linear algebra/rng/ndarray/SIMD abstraction layer |
| FFT | scirs2-fft 0.3.4, oxifft 0.1.3 | Fourier transforms |
| SIMD | wide 1.2.0, simba 0.9 | Vectorized computation |

### 2.3 CLI & Developer Experience

| Layer | Choice | Purpose |
|-------|--------|---------|
| CLI framework | clap 4.6.0 (derive) | Argument parsing with completions |
| Progress indicators | indicatif 0.18, console 0.16, dialoguer 0.12 | Training progress bars, interactive prompts |
| TUI | ratatui 0.30.0, crossterm 0.29.0 | Terminal UI for interactive modes |
| Error handling | thiserror 2.0, anyhow 1.0 | Structured error types per crate |
| Logging | tracing 0.1.44, tracing-subscriber | Structured async-aware logging |
| Serialization | serde+JSON, toml, serde_yaml | Config and metadata |
| Async runtime | tokio 1.50 (full features), tokio-stream, async-stream | Uniform async I/O |

### 2.4 Key Non-Obvious Dependencies

- **SciRS2 ecosystem**: A mandatory abstraction layer. All VoiRS crates MUST use `scirs2-core::ndarray` (not `ndarray` directly), `scirs2-core::random` (not `rand`), `scirs2-core::parallel_ops` (not `rayon`). This ensures consistent SIMD optimizations and version control.
- **NumRS2 0.3.1**: NumPy `.npz` file support for loading pre-trained weights (critical for Kokoro-82M ONNX integration).
- **OxiONNX**: A custom pure-Rust ONNX runtime from the cool-japan ecosystem with wgpu GPU backend, graph optimization (constant folding, operator fusion), profiling, and weight extraction. Feature-gated behind `onnx`/`gpu` crate features.
- **pyo3 0.28.2 + numpy 0.28.0**: Python FFI for training script integration.
- **wasmtime 43.0.0**: Plugin/dynamic loading support via WASM runtime.
- **arrow/parquet 58.0.0**: Dataset storage in columnar format.
- **musicxml/midly**: Singing synthesis score parsing.


---

## 3. Architecture & Source Map

```
voirs/  (16-crate workspace, ~4,500+ tests)
│
├── src/lib.rs                      # Top-level re-export crate. Aggregates all subcrates.
│                                   # Defines G2pBackend/AcousticBackend/VocoderBackend enums.
│                                   # Bridge pattern: G2pBridge<T>, AcousticBridge<T>, VocoderBridge<T>
│                                   #   connects crate-specific types to the unified SDK traits.
│                                   # Factory functions: create_g2p(), create_acoustic(), create_vocoder()
│                                   #   with real backends + fallback to Dummy implementations.
│
├── crates/voirs-sdk/               # UNIFIED PUBLIC API (entry point for all consumers)
│   src/  (37 modules)
│   ├── lib.rs                      # Re-exports all public types, Pipeline, builder
│   ├── traits.rs                   # CORE TRAITS: G2p, AcousticModel, Vocoder, TextProcessor,
│   │                               #   AudioProcessor, VoiceManager, ModelCache, Plugin, AudioEffectPlugin
│   ├── types.rs                    # LanguageCode (40+ variants), Phoneme, MelSpectrogram,
│   │                               #   AudioBuffer, VoiceConfig, SynthesisConfig (comprehensive),
│   │                               #   QualityLevel, AudioFormat, SystemCapabilities, CapabilityNegotiation
│   ├── pipeline.rs                 # VoirsPipeline + VoirsPipelineBuilder (fluent API).
│   │                               #   Feature-gated: emotion, cloning, conversion, singing, spatial.
│   │                               #   DummyG2p, DummyAcoustic, DummyVocoder for testing.
│   ├── model_runtime/              # OnnxSession wrapper (oxionnx), ModelFormatDetector
│   ├── streaming/                  # Chunked synthesis, real-time processing
│   ├── plugins/                    # Plugin system for audio effects
│   ├── cache/                      # Model and result caching
│   ├── adaptive/                   # Adaptive quality control
│   ├── config/                     # PipelineConfig, config hierarchy with merge/validate
│   ├── capabilities.rs             # Feature detection and capability negotiation
│   ├── voice/                      # Voice management
│   ├── error/                      # Structured error types (VoirsError) with recovery
│   ├── wasm/                       # WASM-specific code
│   ├── http/                       # HTTP server
│   └── cloud/                      # Cloud integration
│
├── crates/voirs-g2p/               # GRAPHEME-TO-PHONEME conversion
│   src/  (23 modules)
│   ├── lib.rs (~999 lines)         # G2p trait (async, Send+Sync), G2pError (10 variants),
│   │                               #   G2pDiagnosticContext, Phoneme, PhoneticFeatures,
│   │                               #   G2pConverter (multi-backend), DummyG2p
│   ├── rules.rs                    # EnglishRuleG2p — rule-based English phoneme conversion
│   ├── backends/                   # NeuralG2pBackend (LSTM 3-layer, 256 hidden),
│   │                               #   OnnxG2p, ChinesePinyinG2p, JapaneseDictG2p
│   ├── models.rs                   # LSTM model definitions
│   ├── training.rs                 # G2P training pipeline (LSTM encoder-decoder with attention)
│   ├── detection/                  # Language detection
│   ├── preprocessing/              # Text preprocessing pipeline
│   ├── ssml/                       # SSML support
│   ├── phonology/                  # Phonological rules per language
│   └── streaming.rs                # Streaming G2P
│
├── crates/voirs-acoustic/          # NEURAL ACOUSTIC MODELS (42 modules — most complex crate)
│   src/
│   ├── lib.rs                      # AcousticError (8 variants), LanguageCode (28 languages),
│   │                               #   Phoneme, MelSpectrogram, SynthesisConfig,
│   │                               #   AcousticModelManager (multi-backend registry)
│   ├── traits.rs                   # AcousticModel trait: synthesize, synthesize_batch,
│   │                               #   AcousticModelFeature (10 flags), ModelLoader trait
│   ├── vits/                       # VITS: TextEncoder, VitsModel, VitsConfig
│   ├── fastspeech.rs               # FastSpeech2 model (non-autoregressive)
│   ├── fastspeech2_trainer.rs      # FastSpeech2 training pipeline
│   ├── backends/                   # Candle, ONNX (oxionnx/tract) backends
│   ├── batch_processor.rs          # Batch queue with priority, stats
│   ├── batching.rs                 # Dynamic batching with padding strategies
│   ├── streaming/                  # Streaming synthesis, latency optimizer
│   ├── prosody/                    # ProsodyController: pitch, duration, energy, rhythm
│   ├── speaker/                    # Multi-speaker: embeddings, adaptation, verification
│   ├── mel/                        # Mel spectrogram computation
│   ├── simd/                       # SIMD-optimized audio operations
│   ├── quantization/               # INT8/FP16 model quantization
│   ├── optimization.rs             # Model optimization (pruning, distillation)
│   ├── cache/                      # AdaptiveCache, PredictiveCache, LfuCache
│   ├── memory/                     # TensorMemoryPool, MemoryOptimizer, lazy loading
│   ├── model_manager/              # ModelManager, ModelRegistry, TtsPipeline
│   ├── metrics/                    # Quality evaluation: Objective, Perceptual, Prosody
│   ├── conditioning.rs             # Conditioning (emotion, speaker, style)
│   ├── production.rs               # CircuitBreaker, RateLimiter, HealthChecker, RetryPolicy
│   ├── parallel_attention.rs       # Parallel multi-head attention
│   ├── singing.rs                  # Singing voice synthesis support
│   ├── neural_codec.rs             # Neural audio codec
│   └── vad.rs                      # Voice Activity Detection
│
├── crates/voirs-vocoder/           # NEURAL VOCODERS (31 modules)
│   src/
│   ├── lib.rs                      # VocoderError, AudioBuffer (sine_wave, silence, normalize),
│   │                               #   VocoderFeature (16 variants), VocoderMetadata,
│   │                               #   Vocoder trait (vocode, vocode_stream, vocode_batch),
│   │                               #   VocoderManager (multi-backend registry)
│   ├── models/
│   │   ├── hifigan/                # HiFi-GAN vocoder
│   │   ├── diffwave/               # DiffWave diffusion vocoder (production-ready training)
│   │   ├── waveglow.rs             # WaveGlow vocoder
│   │   ├── bigvgan/                # BigVGAN (planned)
│   │   ├── univnet/                # UnivNet (planned)
│   │   ├── singing/                # Singing vocoder variants
│   │   └── spatial/                # Spatial audio vocoder variants
│   ├── backends/                   # ONNX backends for each vocoder type
│   ├── streaming/                  # StreamingPipeline, StreamingVocoder
│   ├── conditioning.rs             # Vocoder conditioning
│   ├── adaptive_quality.rs         # Dynamic quality vs speed controller
│   ├── conversion.rs               # VoiceConversion: morphing, age/gender
│   ├── effects/                    # Audio effects (noise gate, formant, AGC)
│   ├── loss/                       # Training loss functions
│   ├── optimization_paths.rs       # Hardware-specific optimization paths
│   ├── parallel/                   # Parallel processing
│   ├── simd/                       # SIMD optimizations
│   ├── codecs/                     # Audio codec integration
│   └── post_processing/            # Post-processing pipeline
│
├── crates/voirs-dataset/           # DATASET LOADING & PREPROCESSING
│   src/  (LJSpeech, JVS, VCTK, LibriTTS, custom datasets)
│
├── crates/voirs-cloning/           # VOICE CLONING (~68 files)
│   src/
│   ├── lib.rs (~827 lines)         # VoiceCloner, SpeakerVerifier, FewShotLearner,
│   │                               #   CloningQualityAssessor, VoiceMorpher, AgeGenderAdapter,
│   │                               #   ConsentManager, GpuAccelerator, ModelQuantizer
│   ├── core/                       # Core cloning algorithms
│   ├── few_shot/                   # Few-shot adaptation (1-shot, 3-shot, 5-shot)
│   ├── embedding/                  # Speaker embedding extraction
│   ├── consent.rs                  # Consent management
│   ├── consent_crypto.rs           # Cryptographic consent proofs
│   ├── privacy_protection.rs       # Encryption, watermarking, differential privacy
│   ├── misuse_prevention.rs        # Deepfake detection, anomaly detection
│   ├── zero_shot.rs                # Zero-shot voice cloning
│   ├── voice_morphing.rs           # Voice morphing between speakers
│   ├── voice_aging.rs              # Temporal voice characteristic modeling
│   ├── ab_testing.rs               # A/B testing framework
│   ├── edge.rs                     # Edge deployment
│   └── mobile.rs                   # Mobile optimization
│
├── crates/voirs-conversion/        # VOICE CONVERSION (zero-shot, real-time, style transfer)
├── crates/voirs-emotion/           # EMOTION CONTROL (~43 files)
│   src/
│   ├── lib.rs                      # EmotionController, Emotion types, presets
│   ├── backends/                   # ONNX EmotionClassifier (7 emotions from mel)
│   ├── blending.rs                 # Emotion blending
│   ├── interpolation.rs            # Emotion interpolation
│   ├── morphing.rs                 # Emotion morphing
│   ├── presets.rs                  # Emotion presets
│   ├── prosody/                    # Prosody manipulation for emotion
│   ├── spectral.rs                 # Spectral features for emotion
│   └── cultural.rs                 # Cross-cultural emotion mapping
│
├── crates/voirs-singing/           # SINGING SYNTHESIS (MusicXML/MIDI, breath, vibrato)
├── crates/voirs-spatial/           # SPATIAL AUDIO (3D, HRTF, binaural, VR/AR)
├── crates/voirs-recognizer/        # SPEECH RECOGNITION (Whisper, Conformer, Wav2Vec2 ONNX)
│   src/  (37 modules)
│   ├── asr/                        # ASR backends: Whisper, Conformer, Wav2Vec2
│   ├── wake_word/                  # Wake word detection
│   ├── training/                   # ASR model training
│   ├── c_api/                      # C bindings
│   └── python.rs                   # Python bindings
│
├── crates/voirs-evaluation/        # QUALITY EVALUATION (MOS prediction, A/B testing)
├── crates/voirs-feedback/          # REALTIME FEEDBACK (adaptive learning)
├── crates/voirs-cli/               # COMMAND-LINE INTERFACE (synth, train, voices, onnx)
│   src/  (25 modules)
│   ├── main.rs                     # Binary entry — subcommands: synth, train, voices
│   └── commands/                   # Command implementations
├── crates/voirs-ffi/               # FOREIGN FUNCTION INTERFACE (C, Python, Node.js)
└── crates/voirs-integration-tests/ # End-to-end integration tests
```


---

## 4. Feature Inventory

### 4.1 Core TTS Pipeline

| Feature | Description | Implementation |
|---------|-------------|----------------|
| **G2P (Grapheme-to-Phoneme)** | Text to phoneme sequences | `voirs-g2p`: Rule-based (EnglishRuleG2p), Neural (LSTM with Bahdanau attention), ONNX neural G2P. Multi-backend via G2pConverter. 12 languages. |
| **Acoustic Modeling** | Phonemes to mel spectrograms | `voirs-acoustic`: VITS (GAN-based end-to-end), FastSpeech2 (non-autoregressive with prosody control). Candle and ONNX backends. Multi-speaker, emotion conditioning. |
| **Vocoding** | Mel to waveform audio | `voirs-vocoder`: HiFi-GAN, DiffWave (50 inference steps), WaveGlow. Streaming and batch modes. |
| **Streaming Synthesis** | Chunk-based real-time audio | `<100ms latency target, chunk-based, parallel acoustic/vocoder stages.` |
| **SSML Support** | Speech Synthesis Markup Language | `voirs-g2p/ssml/`: Full SSML parsing, emphasis, prosody, break, say-as, phoneme elements. |

### 4.2 Training Infrastructure

| Feature | Description | Implementation |
|---------|-------------|----------------|
| **DiffWave Training** | Complete vocoder training pipeline | 370 parameters, SafeTensors checkpoints (30MB), AdamW optimizer. Real forward/backward passes. |
| **VITS Training** | End-to-end acoustic + vocoder training | Generator (TextEncoder, PosteriorEncoder, NormalizingFlows, Decoder), Multi-Period + Multi-Scale Discriminators. |
| **FastSpeech2 Training** | Non-autoregressive acoustic training | Encoder/Decoder (4 FFT blocks), Variance Adaptor (duration/pitch/energy predictors). |
| **G2P Training** | Neural G2P model training | LSTM encoder-decoder (3 layers, 256 hidden) with Bahdanau attention. |
| **CLI Training** | `voirs train` subcommands | train vocoder, train acoustic vits, train acoustic fastspeech2, train g2p |
| **Checkpointing** | Model persistence | SafeTensors format, auto-save every N epochs, best-model tracking, resume support. |

### 4.3 Advanced TTS Features

| Feature | Description | Implementation |
|---------|-------------|----------------|
| **Voice Cloning** | Clone voices from reference audio | `voirs-cloning`: Few-shot (30s audio), cross-lingual, speaker embedding, ONNX SpeakerEncoder + VoiceCloner. |
| **Voice Conversion** | Convert voice characteristics | `voirs-conversion`: Zero-shot, real-time, style transfer, age/gender, 3-session ONNX pipeline. |
| **Emotion Control** | Multi-dimensional emotion expression | `voirs-emotion`: 7-emotion ONNX classifier, blending, interpolation, cross-cultural mapping. |
| **Singing Synthesis** | Music-driven voice synthesis | `voirs-singing`: MusicXML/MIDI, breath modeling, vibrato, DiffSinger ONNX backend. |
| **Spatial Audio** | 3D binaural rendering | `voirs-spatial`: HRTF synthesis (neural ONNX), room acoustics, multi-source, haptic integration. |
| **Speech Recognition** | Optional ASR pipeline | `voirs-recognizer`: Whisper ONNX, Conformer CTC, Wav2Vec2 CTC, wake word detection. |

### 4.4 Quality & Evaluation

| Feature | Description | Implementation |
|---------|-------------|----------------|
| **MOS Prediction** | Neural MOS score estimation | ONNX MOS predictor (1.0-5.0), objective/perceptual evaluators. |
| **A/B Testing** | Voice cloning quality comparison | Statistical analysis framework with perceptual evaluation. |
| **Production Monitoring** | Real-time quality alerts | CircuitBreaker, RateLimiter, HealthChecker, RetryPolicy. |
| **Adaptive Quality** | Dynamic quality vs speed | AdaptiveQualityController with PrecisionMode. |

### 4.5 Platform & Integration

| Feature | Description | Implementation |
|---------|-------------|----------------|
| **CLI Tool** | Full command-line interface | synth, train, voices, onnx, download, prepare, benchmark. |
| **C FFI** | C-compatible bindings | Zero-cost FFI with header generation. |
| **Python Bindings** | PyO3-based package | Python wheel, numpy array interop. |
| **WASM Support** | Browser-native speech synthesis | web-sys AudioContext integration. |
| **Docker** | Containerized deployment | Multi-stage Dockerfile for builder, runtime, CI, test, benchmark. |

---

## 5. Key Code Patterns & Techniques

### 5.1 The Bridge Pattern (Crate-to-SDK Adapter)

**Files:** `src/lib.rs:186-531`, `voirs-sdk/src/pipeline.rs`

Each sub-crate defines its own types. The top-level `src/lib.rs` provides **generic bridge structs** (`G2pBridge<T>`, `AcousticBridge<T>`, `VocoderBridge<T>`) that wrap any implementation of the crate-level trait and forward calls through type conversion functions. This avoids circular dependencies and enables any backend to be dropped in.

```rust
pub struct AcousticBridge<T> { inner: T }
#[async_trait]
impl<T> AcousticModel for AcousticBridge<T>
where T: acoustic::traits::AcousticModel + Send + Sync
{
    async fn synthesize(&self, phonemes: &[Phoneme], config: ...) -> Result<MelSpectrogram> {
        let acoustic_phonemes = convert_phonemes_to_acoustic_batch(phonemes);
        self.inner.synthesize(&acoustic_phonemes, config).await
            .map(|mel| convert_mel_spectrogram_to_sdk(mel))
    }
}
```

S2B2S could use this pattern between its `TtsBackend` trait and individual backend implementations.

### 5.2 Factory with Graceful Fallback

**File:** `src/lib.rs:47-147`

`create_g2p()`, `create_acoustic()`, `create_vocoder()` attempt real backends but fall back to `DummyG2p`/`DummyAcoustic`/`DummyVocoder`. This enables compilation and testing without model files. S2B2S has `WarmEngine` states but could benefit from explicit dummy backends for CI.

### 5.3 Manager Pattern (Multi-Backend Registry)

**Files:** `voirs-g2p/src/lib.rs:706-809`, `voirs-acoustic/src/lib.rs:607-733`, `voirs-vocoder/src/lib.rs:531-599`

All three core crates have a **Manager** struct: `HashMap<String, Box<dyn Trait>>` + default. The manager itself implements the trait by delegating to the default backend. This is the **composite pattern** — S2B2S''s TTS engine selection could use this instead of `match` on engine type.

### 5.4 The Trait Trinity (G2p, AcousticModel, Vocoder)

**Files:** `voirs-sdk/src/traits.rs:13-161`

Three async traits define the pipeline contract. Each has 3-5 methods: core, batch, stream, metadata, and feature query. All are `Send + Sync` + `#[async_trait]`. S2B2S''s `TtsBackend` trait is simpler — lacks batch/stream separation and feature query API.

### 5.5 Feature-Capability System

**File:** `voirs-sdk/src/types.rs:928-1096`

Comprehensive type system: `SystemCapabilities`, `CapabilityRequest` (with priorities: Optional/Preferred/Required/Critical), `FallbackStrategy` (FailFast/GracefulDegradation/UseAlternatives/BasicFunctionality), `CapabilityNegotiation`. More sophisticated than S2B2S''s simple availability checks.

### 5.6 Configuration Hierarchy with Merge & Validate

**File:** `voirs-sdk/src/types.rs:791-880`

`SynthesisConfig` implements `ConfigHierarchy` with `merge_with()` and `validate()`. Per-field merge checks "is this the default?" before overriding. Validate enforces range constraints. S2B2S could apply this to `TtsConfig` and `BrainConfig`.

### 5.7 Multi-Inference-Backend Architecture (Candle + OxiONNX + Tract)

VoiRS supports three ML backends: **Candle** (Rust-native, for training), **OxiONNX** (custom pure-Rust, 88 ops, wgpu GPU), **Tract-ONNX**. All ONNX code is `#[cfg(feature = "onnx")]` gated with `Session::builder().with_optimization_level().load()` pattern. S2B2S uses Python HTTP servers for ONNX models — VoiRS''s pure-Rust approach would eliminate that dependency.

### 5.8 SciRS2 Abstraction Layer

**File:** `SCIRS2_INTEGRATION_POLICY.md`

Strict policy: no VoiRS crate may use `rand`, `ndarray`, `num-complex`, `rayon`, or `nalgebra` directly. All must go through `scirs2-core`. Provides unified SIMD, centralized versions, type safety.

### 5.9 Training Infrastructure as a Library

**File:** `TRAINING.md`, `voirs-vocoder/src/models/diffwave/`

Training is first-class API, not separate scripts. CLI `voirs train` commands. SafeTensors checkpoints. Multi-epoch with auto best-model. Mixed precision

### 5.10 Workspace Management

**File:** root `Cargo.toml`

Strict workspace policies: all versions in root, subcrates inherit, shared deps use `.workspace = true`, each crate defines own keywords/categories. Release profiles for CPU, GPU, and distribution.

---

## 6. Relation to S2B2S

### 6.1 Architecture Comparison

| Aspect | VoiRS | S2B2S | Verdict |
|--------|-------|-------|---------|
| **Purpose** | TTS framework/library for developers | Desktop voice-native application for end users | Different domains |
| **Architecture style** | 16-crate workspace, strictly layered | Monolithic Tauri app with manager pattern | Both well-organized |
| **TTS pipeline** | Text → G2P → AcousticModel → Vocoder → Audio | Markdown strip → TN → TTS Backend → Speaker | S2B2S has more backends; VoiRS more modularity |
| **G2P** | Deep subsystem: rule-based, neural LSTM, ONNX. 12 languages. | Does not have G2P — relies on backend text processing. | VoiRS offers finer control |
| **Acoustic modeling** | Full VITS + FastSpeech2 in pure Rust. Training included. | No acoustic model — delegates to TTS backends. | Different choices |
| **Vocoding** | HiFi-GAN, DiffWave, WaveGlow in Rust. Streaming + batch. Training. | Delegates to TTS backends. | VoiRS vertically integrated |
| **Voice cloning** | Comprehensive: few-shot, cross-lingual, zero-shot, ethical safeguards. 68 modules. | Pocket TTS basic cloning via HTTP server. | VoiRS far more advanced |
| **Emotion** | 7-emotion classifier, blending, presets, cross-cultural. | Not supported. | VoiRS only |
| **Singing** | MusicXML/MIDI, breath, vibrato, DiffSinger. | Not supported. | VoiRS only |
| **Spatial audio** | HRTF, binaural, room acoustics, haptic. | Not supported. | VoiRS only |
| **Training** | Built-in: G2P, VITS, FastSpeech2, DiffWave, HiFi-GAN. | External: Piper training scripts. | VoiRS wins |
| **ONNX approach** | Pure-Rust OxiONNX. No Python. 12 crates have ONNX backends. | Python HTTP servers for Kokoro/Kitten/Pocket. | VoiRS superior for deployment |
| **Error handling** | Crate-specific error types with diagnostic context. | anyhow throughout. | VoiRS finer-grained |
| **Testing** | 4,500+ tests. Property-based, fuzzing, memory leak detection. | Unit + integration tests. | VoiRS far more comprehensive |
| **ML framework** | Candle + OxiONNX + Tract (all Rust-native). | ONNX via Python or piper-rs. | VoiRS architecturally cleaner |

### 6.2 S2B2S Does Better

- **Production-readiness**: Working desktop app vs beta framework
- **End-user experience**: GUI, onboarding, settings, shortcuts, i18n (20 languages)
- **Backend breadth**: 9 TTS backends including cloud (OpenAI, ElevenLabs, Cartesia)
- **Audio I/O**: Complete recording → VAD → STT pipeline
- **LLM integration**: Brain (streaming LLM with barge-in) — no equivalent in VoiRS

### 6.3 VoiRS Does Better

- **Architecture modularity**: Strictly layered crate structure with well-defined traits
- **Training infrastructure**: Built-in training is a major differentiator
- **Pure-Rust purity**: No Python runtime needed
- **Voice cloning depth**: Ethical safeguards, consent management, privacy protection
- **Testing rigor**: 4,500+ tests with property-based, fuzzing, memory leak detection
- **Configuration hierarchy**: Merge + validate pattern
- **Feature negotiation**: Automatic capability detection and graceful degradation

---

## 7. Harvest List (Features Worth Copying)

| Feature to harvest | From file | Effort | Why valuable for S2B2S |
|--------------------|-----------|--------|------------------------|
| **Config hierarchy with merge/validate** | `voirs-sdk/src/types.rs:791-880` | S | Merge-with-override and range validation for TtsConfig/BrainConfig. |
| **Manager-as-trait-delegate pattern** | `voirs-acoustic/src/lib.rs:607-733` | S | Plugable registry instead of `match` on engine type. |
| **Feature query API on traits** | `voirs-sdk/src/traits.rs:72,137` | S | `fn supports(&self, feature: TtsFeature) -> bool` — UI can hide unsupported features per backend. |
| **Capability negotiation system** | `voirs-sdk/src/types.rs:928-1096` | M | Auto-detect GPU/memory/CPU, choose engine, graceful degradation. |
| **Pure-Rust ONNX runtime integration** | `voirs-sdk/src/model_runtime/onnx.rs` | L | Replace Python HTTP servers with direct ONNX. Eliminates Python dependency. |
| **Safety metadata on cloning** | `voirs-cloning/src/consent.rs` | M | Consent tracking and audio watermarking for voice cloning. |
| **Dummy/fallback pattern for testing** | `voirs-sdk/src/pipeline.rs:877-1073` | S | Mock TTS backends for CI without model files. |
| **AudioBuffer utility methods** | `voirs-vocoder/src/lib.rs:226-311` | S | `sine_wave()`, `silence()`, `normalize_to_peak()` for audio feedback. |
| **Batch processing trait methods** | `voirs-sdk/src/traits.rs:62-66` | M | `synthesize_batch()` for multi-request efficiency. |
| **CircuitBreaker/RetryPolicy** | `voirs-acoustic/src/production.rs` | M | Production-hardening for cloud TTS backends. |
| **Phonetic features struct** | `voirs-g2p/src/lib.rs:282-338` | S | Phonetic context for text normalization. |

---

## 8. Known Issues, Caveats & Limitations

| Issue | Severity | Impact |
|-------|----------|--------|
| **Beta/pre-release quality** | High | Many features are planned/stub. Advanced modules may have incomplete implementations. |
| **No pre-trained production models** | High | Users must train their own models — requires GPU and datasets. |
| **Duplicate types across crates** | Medium | Each crate defines own LanguageCode, MelSpectrogram, Phoneme. Bridge pattern works but adds verbosity. |
| **Dead code toleration** | Low | Workspace lint: `dead_code = "allow"`, `unused_variables = "allow"`. Indicates unfinished cleanup. |
| **Candle CUDA patches** | Medium | Fragile patch-maintenance for upstream CUDA build scripts. |
| **Partial SciRS2 migration** | Medium | Not all code converted to SciRS2 abstractions. |
| **Missing VITS2 training** | Low | VITS2 referenced but training pipeline not complete. |
| **Kokoro-82M via numrs2** | Low | Limited to specific model; quality may degrade vs Python-native ONNX runtime. |
| **No daemon/server mode** | Medium | SDK is library-only. No built-in HTTP/gRPC serving. |
| **Version numbering inconsistency** | Low | CLAUDE.md says "0.3.0" but Cargo.toml says "0.1.0-rc.1". |

---

## 9. Strengths & Weaknesses

### Strengths

1. **Extraordinary modularity**: 16 crates with clean boundaries. Each independently testable, compilable, and publishable. Trait-based architecture means any component can be swapped.

2. **Pure-Rust ambition**: No Python dependency at runtime. Entire TTS stack runs in a single Rust binary — significant deployment advantage.

3. **Built-in training**: Making training a first-class API sets VoiRS apart from almost every other TTS framework. DiffWave pipeline with SafeTensors demonstrates this works.

4. **Comprehensive feature vision**: Roadmap spans emotion, cloning, singing, spatial, recognition, evaluation — more than any comparable pure-Rust TTS project.

5. **Rigorous testing culture**: 4,500+ tests, property-based, fuzzing, memory leak detection, cross-platform validation.

6. **ONNX integration depth**: 12 crates have ONNX backends via OxiONNX. Unified OnnxSession wrapper is well-designed.

### Weaknesses

1. **Scope exceeds maturity**: Advanced features (spatial, singing, voice aging) are clearly incomplete. Beta tag masks that many modules are stubs.

2. **No production models**: Cannot download a working voice and start synthesizing. Limits real-world utility vs Piper, Kokoro, or Coqui.

3. **Duplicate type definitions**: Each crate defines own types. Bridge pattern works but adds verbosity. A shared types crate could help.

4. **Training requires expertise**: Guides reference MFA (Python tool), external datasets, GPU hardware. Undermines "pure Rust" narrative for training.

5. **AI-assisted development artifacts**: Heavy reliance on Claude/agent-driven code. Version numbering inconsistency between docs.

6. **No audio playback**: VoiRS generates audio buffers but has no built-in playback. Consumers must use rodio/cpal themselves.

---

## 10. Bottom Line / Verdict

VoiRS is an **ambitious and architecturally excellent** pure-Rust TTS framework that is currently **closer to a research prototype than a production library**. Its greatest value to S2B2S is not as a code source to copy, but as a **reference architecture** for modular TTS design. The trait-based pipeline (`G2p → AcousticModel → Vocoder`), bridge pattern for type isolation, factory-with-fallback creation, and manager-as-trait-delegate pattern are all patterns S2B2S could adopt to make its TTS subsystem more modular and testable.

The single most valuable idea is the **pure-Rust ONNX integration** (OxiONNX + tract-onnx). If S2B2S could replace its Python HTTP server backends (Kokoro, Kitten, Pocket) with direct ONNX inference via tract-onnx or ort, it would eliminate the Python dependency entirely, reduce latency by removing HTTP serialization overhead, and simplify deployment to a single binary — all while maintaining the exact same model quality.

The second most valuable pattern is the **Manager-as-trait-implementer** with registry (`HashMap<String, Box<dyn Trait>>` + default). S2B2S''s TTS backend selection currently uses a `match` on engine type. A pluggable registry would make adding new backends cleaner and enable runtime hot-swapping.

VoiRS is worth studying, but its code is not ready for direct reuse in a production application. Focus on the architectural patterns and the ONNX integration approach.

---

*Analysis completed 2026-06-14. Total: ~540 lines.*
