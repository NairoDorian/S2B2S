# @asrjs/speech-recognition -- Library/Reference (Category C/D)

> Repo: `asrjs/speech-recognition` · HEAD: `b38d7eb` · License: Apache-2.0 · Author: asrjs · Platforms: Browser + Node.js
> Nature: independent library · Role for S2B2S: reference API design patterns for multi-model STT abstraction layer, canonical transcript contracts, browser realtime helpers, IO/asset-loading architecture

---

## 1. What @asrjs/speech-recognition Is

`@asrjs/speech-recognition` (v0.2.0) is a speech-first TypeScript runtime for browser and local Node.js ONNX-based ASR inference. It is NOT a general-purpose task-pipeline framework -- it is intentionally scoped to speech recognition workloads, providing a layered API that separates runtime orchestration from model-family execution and branded presets.

The library solves the problem of running multiple ASR architectures (Conformer/TDT, CTC, Whisper seq2seq, AED) through ONNX Runtime with a unified API surface. It provides model discovery, asset loading (HuggingFace, URL, Blob, local files), IndexedDB caching, WebGPU/WASM backend selection with fallback, canonical transcript normalization across families, and browser realtime streaming helpers.

Target audience: TypeScript/JavaScript developers building browser-based or Node.js speech recognition applications who need to switch between model families (Parakeet, Whisper, MedASR, Canary, Wav2Vec2) without changing application code.

---

## 2. Tech Stack

### 2.1 Frontend (Browser)

| Layer | Choice | Purpose |
|-------|--------|---------|
| Language | TypeScript 5.9.3 (strict, ES2022 target) | Type-safe source |
| Inference | ONNX Runtime Web 1.27 (WebGPU/WASM backends) | Model execution |
| Audio capture | Web Audio API / MediaDevices | Microphone input |
| Caching | IndexedDB (custom wrapper) | Model asset persistence |
| Workers | Web Workers | Non-blocking transcription |
| Rendering | Canvas 2D (browser-waveform.ts) | Waveform visualization |

### 2.2 Backend / Core (Node.js)

| Layer | Choice | Purpose |
|-------|--------|---------|
| Runtime | Node.js (ESM via NodeNext) | Server/CLI inference |
| Inference | ONNX Runtime Node 1.25 (dev) | Model execution |
| Asset loading | `fetch`, `fs` (via IO handles) | Model file resolution |

### 2.3 Key Dependencies

| Dependency | Purpose |
|------------|---------|
| `onnxruntime-web` ^1.27.0 | Browser ONNX inference (WebGPU + WASM) |
| `onnxruntime-node` 1.25.0-dev | Node.js ONNX inference |
| `pako` ^2.1.0 | Gzip decompression (tokenizer assets) |
| `vitest` 4.0.18 | Testing framework (~100+ test files) |
| `typedoc` ^0.28.17 | API documentation generation |

No React, Vue, Svelte, or Solid dependencies. The architecture document explicitly recommends keeping framework bindings in separate packages (e.g., `@asrjs/speech-recognition-react`).

---

## 3. Architecture & Source Map

```
src/
├── types/                         # Stable contracts, zero execution code
│   ├── index.ts                   # Re-exports all type modules
│   ├── architecture.ts            # Model architecture descriptors
│   ├── audio.ts                   # AudioBufferLike, AudioInputLike
│   ├── backend.ts                 # ExecutionBackend, BackendCapabilities (57 lines)
│   ├── classification.ts          # ModelClassification
│   ├── io.ts                      # AssetRequest, AssetProvider, AssetCache
│   ├── latency.ts                 # Latency metrics types
│   ├── model.ts                   # TensorMap, AcousticFeatures (22 lines)
│   ├── runtime.ts                 # SpeechRuntime, SpeechModel, SpeechSession (268 lines)
│   ├── streaming.ts               # StreamingSessionOptions, StreamingTranscriber
│   └── transcript.ts              # TranscriptResult, PartialTranscript (203 lines)
│
├── runtime/                       # Orchestration, registration, lifecycle
│   ├── session.ts                 # DefaultSpeechRuntime, createSpeechRuntime() (351 lines)
│   ├── backend.ts                 # BackendCandidate, sorting/selection logic (142 lines)
│   ├── catalog.ts                 # listSpeechModels, getSpeechModelDescriptor (99 lines)
│   ├── load.ts                    # loadSpeechModel, transcribeSpeech, createSpeechPipeline (518 lines)
│   ├── transcripts.ts             # 7 transcript normalizers (469 lines)
│   ├── builtins.ts                # createBuiltInSpeechRuntime()
│   ├── huggingface.ts             # HuggingFace model file resolution (306 lines)
│   ├── capture.ts                 # Microphone capture (browser)
│   ├── browser-controller.ts      # Browser lifecycle controller
│   ├── browser-realtime.ts        # createBrowserRealtimeStarter()
│   ├── browser-transcription-worker.ts  # Worker thread orchestration
│   ├── local-browser.ts           # Local file/directory model loading
│   ├── media.ts                   # Audio decoding (decodeAudioSourceToMonoPcm)
│   ├── realtime.ts                # AudioRingBuffer, StreamingWindowBuilder, UtteranceTranscriptMerger (731 lines)
│   ├── noise-floor.ts             # Noise floor estimation
│   ├── rough-speech-gate.ts       # Speech gating
│   ├── streaming-config.ts        # Streaming configuration
│   ├── streaming-consumer.ts      # Consumer pattern for streaming
│   ├── streaming-controls.ts      # Playback/pause controls
│   ├── streaming-detector.ts      # Activity detection
│   ├── chunking.ts                # AudioChunker, LayeredAudioBuffer (165 lines)
│   ├── vad.ts                     # VAD adapter
│   ├── ten-vad-browser.ts         # Ten VAD browser integration
│   ├── firered-vad-browser.ts     # FireRed VAD browser integration
│   ├── audio-timeline.ts          # Voice activity timeline
│   ├── browser-waveform.ts        # Canvas waveform renderer
│   ├── browser-monitor.ts         # Streaming monitor
│   ├── browser-monitor-display.ts # Display metadata
│   ├── browser-compact-stats.ts   # Compact stats renderer
│   ├── timing.ts                  # nowMs(), roundMetric()
│   ├── logging.ts                 # Logger hooks
│   ├── errors.ts                  # BackendUnavailableError, ModelLoadError, etc.
│   ├── cache.ts                   # Cache helpers
│   ├── controller.ts              # Realtime controller
│   ├── segment-foreground-filter.ts # Foreground segment filtering
│   └── transcripts.ts             # Normalizer exports
│
├── io/                            # Asset resolution and caching
│   ├── index.ts                   # Re-exports
│   ├── providers.ts               # CompositeAssetProvider, HuggingFace, URL, Blob (159 lines)
│   ├── cache.ts                   # IndexedDbAssetCache, MemoryAssetCache (253 lines)
│   ├── handles.ts                 # BlobAssetHandle, UrlAssetHandle
│   ├── io.ts / io-node.ts         # Environment entry points
│   └── node-providers.ts          # Node filesystem provider
│
├── inference/                     # Shared descriptors, math, streaming
│   ├── descriptors.ts             # FASTCONFORMER_ENCODER, TDT_GREEDY_DECODING, etc. (169 lines)
│   ├── math.ts                    # argmax(), confidenceFromLogits() (46 lines)
│   ├── streaming/                 # Shared streaming primitives
│   │   ├── accumulator.ts         # Window accumulator
│   │   ├── long-audio.ts          # Long audio coordinator
│   │   ├── merge.ts               # Window merge strategies
│   │   ├── rolling-window.ts      # Rolling window logic
│   │   └── transcriber.ts         # DefaultStreamingTranscriber
│   └── backends/                  # Backend capability probes
│       ├── wasm/index.ts          # WASM backend (STUB - 75 lines)
│       ├── webgpu/index.ts        # WebGPU backend (STUB - 98 lines)
│       ├── webnn/index.ts         # WebNN backend
│       └── webgl/index.ts         # WebGL backend
│
├── models/                        # Architecture-based implementation families
│   ├── nemo-tdt/                  # NeMo TDT (Parakeet) -- ~866-line executor
│   │   ├── executor.ts            # OrtNemoTdtExecutor: preprocess->encode->decode loop (866 lines)
│   │   ├── model.ts               # createNemoTdtModelFamily()
│   │   ├── config.ts              # NemoTdtModelConfig
│   │   ├── mapping.ts             # Native->canonical mapping
│   │   ├── tokenizer.ts           # ParakeetTokenizer (SentencePiece BPE)
│   │   ├── preprocessor.ts        # JsNemoPreprocessor + OnnxNemoPreprocessor
│   │   ├── ort.ts                 # ORT session creation
│   │   ├── weights.ts             # Default weight setups
│   │   ├── transcript-details.ts  # Word/token detail reconstruction
│   │   └── types.ts               # NemoTdtNativeTranscript, etc.
│   │
│   ├── nemo-rnnt/                 # NeMo RNNT (Parakeet RNNT)
│   │   ├── executor.ts            # OrtNemoRnntExecutor
│   │   ├── model.ts               # createNemoRnntModelFamily()
│   │   └── ...                    # Similar structure to nemo-tdt
│   │
│   ├── nemo-aed/                  # NeMo AED (Canary)
│   │   ├── executor.ts            # CanaryAEDExecutor
│   │   ├── model.ts               # createNemoAedModelFamily()
│   │   └── ...
│   │
│   ├── nemo-common/               # Shared NeMo utilities
│   │   ├── mapping.ts             # mapNemoNativeToCanonical()
│   │   ├── classification.ts      # Model classification
│   │   ├── stub.ts                # Placeholder
│   │   └── types.ts
│   │
│   ├── lasr-ctc/                  # LASR CTC (MedASR / Google Health)
│   │   ├── executor.ts            # OrtLasrCtcExecutor: CTC greedy (870 lines)
│   │   ├── model.ts               # createLasrCtcModelFamily()
│   │   ├── mel.ts                 # MedAsrJsPreprocessor (128-bin Kaldi mel)
│   │   ├── tokenizer.ts           # MedAsrTextTokenizer
│   │   ├── ort.ts                 # ORT session creation
│   │   └── types.ts
│   │
│   ├── whisper-seq2seq/           # Whisper (23 exported modules)
│   │   ├── executor.ts            # WhisperOnnxExecutor (~2000+ lines)
│   │   ├── enhanced-executor.ts   # Enhanced executor with quality gates
│   │   ├── core.ts                # whisperDecode() unified decode loop
│   │   ├── model.ts               # createWhisperSeq2SeqModelFamily()
│   │   ├── config.ts              # WhisperSeq2SeqModelConfig
│   │   ├── generation-config.ts   # WhisperGenerationConfig
│   │   ├── beam-search.ts         # Beam search (WhisperBeamState)
│   │   ├── processors.ts          # WhisperTimestampLogitProcessor
│   │   ├── tokenizer.ts           # WhisperTokenizer (tiktoken-based BPE)
│   │   ├── ort.ts                 # ORT with split-graph support
│   │   ├── attention-alignment.ts # DTW-based cross-attention word timestamps
│   │   ├── word-timestamps.ts     # Word timestamp reconstruction
│   │   ├── chunking.ts            # Chunk-aware transcript merging
│   │   ├── chunk-context.ts       # Context preservation across chunks
│   │   ├── drift-handler.ts       # Timestamp drift correction
│   │   ├── segment-merger.ts      # Segment merging
│   │   ├── quality-gates.ts       # Quality validation gates
│   │   ├── temperature-fallback.ts # Temperature-based fallback
│   │   ├── vad-segmenter.ts       # VAD-based segmentation
│   │   ├── whisperx-options.ts    # WhisperX-compatible options
│   │   └── types.ts
│   │
│   ├── firered-llm/               # FireRed ASR-LLM (stub/early)
│   └── wav2vec2/                  # Wav2Vec2 CTC
│       ├── executor.ts
│       ├── config.ts
│       └── ...
│
├── presets/                       # Branded presets (thin wrappers)
│   ├── descriptors.ts             # 5 presets (903 lines of model metadata)
│   ├── index.ts                   # Re-exports
│   ├── parakeet/                  # Parakeet TDT/RNNT presets
│   │   ├── catalog.ts             # 8 Parakeet model variants
│   │   ├── factory.ts             # createParakeetPresetFactory()
│   │   ├── manifest.ts            # resolveParakeetPresetManifest()
│   │   └── ...
│   ├── canary/                    # Canary AED preset
│   ├── medasr/                    # MedASR CTC preset
│   ├── whisper/                   # Whisper seq2seq preset
│   └── wav2vec2/                  # Wav2Vec2 CTC preset
│
├── pipeline/                      # Long-audio windowing & post-processing
│   ├── index.ts                   # Re-exports (15 modules)
│   ├── composition.ts             # withResolvedTranscriptDetail()
│   ├── long-audio-windowing.ts    # planWindowedTranscription(), transcribeWithWindowing()
│   ├── window-policy.ts           # createDefaultModelInferenceLimits()
│   ├── windowed-metrics.ts        # Windowed perf metrics
│   ├── sentence-segmenter.ts      # Sentence boundary detection
│   ├── sentence-stage.ts          # Sentence-level post-processing
│   ├── vad-segments.ts            # VAD segment integration
│   ├── whisper-chunking.ts        # Whisper-specific chunk planning
│   ├── whisper-timestamps.ts      # Whisper timestamp processing
│   ├── whisper-production-pipeline.ts # Production Whisper pipeline
│   ├── output-options.ts          # Output format options
│   ├── output-sidecars.ts         # Sidecar output (SRT, VTT)
│   ├── subtitles.ts               # Subtitle generation
│   ├── types.ts                   # Pipeline types
│   └── windowing-stage.ts         # Windowing stage orchestration
│
├── audio/                         # Audio preprocessing
│   ├── audio.ts                   # PcmAudioBuffer, AudioChunk, normalizePcmInput (309 lines)
│   ├── base.ts                    # AudioBufferLike base
│   ├── js-mel.ts                  # JS mel spectrogram (NeMo)
│   ├── or-mel.ts                  # ONNX Runtime mel spectrogram
│   ├── kaldi-mel.ts               # Kaldi-style mel spectrogram
│   ├── whisper-mel.ts             # Whisper mel (log10, clamp, scale)
│   ├── wav2vec-conv.ts            # Wav2Vec2 conv feature extractor
│   ├── specs.ts                   # Spectrogram utilities
│   └── stub.ts                    # Stub audio implementations
│
├── ctc/                           # CTC decoding utilities
│   ├── decoder.ts                 # CTC greedy/beam decoder
│   └── types.ts
│
├── alignment/                     # Forced alignment
│   ├── ctc-viterbi.ts             # CTC Viterbi alignment
│   ├── cross-attention-dtw.ts     # DTW on cross-attention
│   └── wav2vec2-aligner.ts        # Wav2Vec2 alignment
│
├── post-processing/               # Post-processing pipeline
│   ├── segment-merger.ts          # Segment merging
│   └── extras.ts                  # Extra annotations
│
├── tokenizers/                    # Tokenizer implementations
│   ├── base.ts                    # Base tokenizer interface
│   ├── bpe.ts                     # BPE tokenizer
│   ├── tiktoken.ts                # tiktoken-based tokenizer
│   ├── utf8.ts                    # UTF-8 tokenizer
│   └── stub.ts
│
├── quality/                       # Quality assessment
│   ├── compression-ratio.ts       # Text compression ratio
│   ├── entropy.ts                 # Entropy metrics
│   ├── log-probability.ts         # Log probability analysis
│   ├── no-speech.ts               # No-speech probability
│   └── temperature-fallback.ts    # Temperature fallback
│
├── chunking/                      # Chunking strategies
│   ├── fixed-window.ts            # Fixed-size window chunking
│   ├── vad-segmenter.ts           # VAD-based chunking
│   └── backends/                  # VAD backends
│
├── runtime/assets/                # Pre-compiled WASM assets
│   └── ten-vad/                   # Ten VAD WASM + JS
│
├── index.ts                       # Root entry (13 re-exports)
├── builtins.ts                    # Built-in convenience entry
├── browser.ts                     # Browser-only entry (13 re-exports)
├── realtime.ts                    # Realtime helpers entry
├── io.ts                          # IO entry
├── io-node.ts                     # Node IO entry
├── inference.ts                   # Inference entry
├── bench.ts                       # Benchmark entry
├── datasets.ts                    # Datasets entry
├── alignment.ts                   # Alignment entry
├── tokenizers.ts                  # Tokenizers entry
├── pipeline.ts                    # Pipeline entry
├── post-processing.ts             # Post-processing entry
├── chunking.ts                    # Chunking entry
├── quality.ts                     # Quality entry
└── presets.ts                     # Presets entry (re-exports)
```

**Dependency direction** (enforced): `types -> audio/tokenizers/io/runtime -> inference -> models -> presets`

---

## 4. Feature Inventory

### 4.1 STT Pipeline (Core Inference)

| Feature | Description | Implementation |
|---------|-------------|----------------|
| **Multi-model runtime** | Single runtime hosts multiple model families | `DefaultSpeechRuntime` in `src/runtime/session.ts` (351 lines) |
| **Model family abstraction** | `SpeechModelFactory` interface for pluggable architectures | `src/types/runtime.ts` (lines 220-233) |
| **Preset->family resolution** | Branded presets resolve into technical family requests | `DefaultSpeechRuntime.resolvePresetRequest()` in `session.ts` (lines 233-259) |
| **NeMo TDT executor** | Preprocess (JS/ONNX mel) -> encoder -> autoregressive decoder | `OrtNemoTdtExecutor` in `src/models/nemo-tdt/executor.ts` (866 lines) |
| **NeMo RNNT executor** | RNNT transducer variant | `src/models/nemo-rnnt/executor.ts` |
| **NeMo AED (Canary) executor** | AED with prompt tokens (PnC, timestamps, language) | `src/models/nemo-aed/executor.ts` |
| **LASR CTC executor** | CTC greedy with frame collapse, token spans | `OrtLasrCtcExecutor` in `src/models/lasr-ctc/executor.ts` (870 lines) |
| **Whisper seq2seq executor** | Split-graph or merged decoder, beam search, forced alignment | `WhisperOnnxExecutor` in `src/models/whisper-seq2seq/executor.ts` (~2000+ lines) |
| **Whisper split-graph** | Separate encoder_init, decoder_step, decoder_align sessions | `src/models/whisper-seq2seq/executor.ts` (lines 944-1018) |
| **Wav2Vec2 CTC executor** | Raw waveform -> Wav2Vec2 encoder -> CTC decode | `src/models/wav2vec2/executor.ts` |

### 4.2 Canonical Transcript Contracts

| Feature | Description | Implementation |
|---------|-------------|----------------|
| **TranscriptResult** | Stable transcript shape: text, warnings, meta, segments, words, tokens | `src/types/transcript.ts` (lines 149-157) |
| **PartialTranscript** | Streaming: committedText + previewText + revision | `src/types/transcript.ts` (lines 165-177) |
| **TranscriptionEnvelope** | Canonical + optional native output | `src/types/transcript.ts` (lines 180-183) |
| **Response flavors** | `canonical`, `native`, `canonical+native` | `src/types/transcript.ts` (line 18) |
| **Detail levels** | text, segments, sentences, words, sentences+words, detailed | `src/types/transcript.ts` (lines 7-13) |
| **7 normalizers** | nemo-tdt, nemo-rnnt, nemo-aed, lasr-ctc, whisper, wav2vec2, legacy-parakeet | `src/runtime/transcripts.ts` (469 lines) |
| **Worker safety** | Canonical objects are structured-clone-safe POJOs | Enforced by design (no classes, Map, Set) |

### 4.3 Backend System

| Feature | Description | Implementation |
|---------|-------------|----------------|
| **Multi-backend probing** | WASM, WebGPU, WebNN, WebGL capability detection | `src/inference/backends/*/index.ts` |
| **Backend selection** | Priority-based sort: preferred->precision->acceleration->SAB | `src/runtime/backend.ts` (142 lines) |
| **FP16 fallback** | WebGPU FP16 fails -> auto fallback to FP32 or INT8 | `OrtNemoTdtExecutor.initialize()` in `executor.ts` (lines 269-344) |
| **External data fallback** | ONNX external data failure -> retry single-file | `OrtLasrCtcExecutor.initialize()` in `executor.ts` (lines 516-530) |
| **Quantization-aware** | Per-model per-backend default weights (fp16/int8/fp32) | `src/presets/descriptors.ts` (lines 695-708) |

### 4.4 Audio Preprocessing

| Feature | Description | Implementation |
|---------|-------------|----------------|
| **PcmAudioBuffer** | Float32 planar PCM with sample rate | `src/audio/audio.ts` (lines 28-105) |
| **AudioChunk** | Timestamped, sequenced audio chunks | `src/audio/audio.ts` (lines 114-131) |
| **normalizePcmInput** | Accept Float32Array, Float64Array, Int16Array, interleaved | `src/audio/audio.ts` (lines 171-203) |
| **Multiple mel frontends** | NeMo (80/128), Kaldi, Whisper, GigaAM, identity | `src/audio/js-mel.ts`, `kaldi-mel.ts`, `whisper-mel.ts` |
| **ONNX preprocessor** | ONNX-based mel for hardware-accelerated preprocessing | `src/models/nemo-tdt/preprocessor.ts` |

### 4.5 Long Audio Windowing

| Feature | Description | Implementation |
|---------|-------------|----------------|
| **Auto windowing** | `planWindowedTranscription()` decides if audio needs chunking | `src/pipeline/long-audio-windowing.ts` |
| **Per-model limits** | `ModelInferenceLimits` with min/max/recommended window, overlap | `src/types/runtime.ts` (lines 89-106) |
| **Multiple merge strategies** | word-dedupe, ctc-collapse, whisper-stride, concat | `src/types/runtime.ts` (line 85) |
| **Segmentation strategies** | word-punctuation, ctc-frame, whisper-token, vad, none | `src/types/runtime.ts` (line 84) |
| **Whisper chunk planning** | Stride-based overlapping windows | `src/pipeline/whisper-chunking.ts` |
| **Sentence boundary windowing** | Prefer sentence boundaries for window cuts | Parakeet TDT default |

### 4.6 Browser Realtime Features

| Feature | Description | Implementation |
|---------|-------------|----------------|
| **AudioRingBuffer** | Circular buffer with time-based read/write | `src/runtime/realtime.ts` (lines 21-229) |
| **StreamingWindowBuilder** | Builds transcription windows from ring buffer | `src/runtime/realtime.ts` (lines 256-423) |
| **UtteranceTranscriptMerger** | Merges overlapping window transcripts, deduplicates | `src/runtime/realtime.ts` (lines 557-731) |
| **Microphone capture** | `startMicrophoneCapture()` via MediaDevices | `src/runtime/capture.ts` |
| **Browser transcription worker** | Offload transcription to Web Worker | `src/runtime/browser-transcription-worker.ts` |
| **Audio decoding** | `decodeAudioSourceToMonoPcm()` for uploaded files | `src/runtime/media.ts` |
| **Waveform renderer** | Canvas-based min-max waveform | `src/runtime/browser-waveform.ts` |
| **Compact stats renderer** | Canvas-based latency/RTF display | `src/runtime/browser-compact-stats.ts` |
| **Ten VAD / FireRed VAD** | Browser VAD adapters with worker support | `src/runtime/ten-vad-browser.ts`, `firered-vad-browser.ts` |

### 4.7 IO / Asset Loading

| Feature | Description | Implementation |
|---------|-------------|----------------|
| **CompositeAssetProvider** | Chains multiple providers (Blob -> HuggingFace -> URL) | `src/io/providers.ts` (lines 50-159) |
| **HuggingFace provider** | Resolves `repoId/revision/filename` to HF CDN URLs | `src/io/providers.ts` (lines 117-127) |
| **IndexedDB cache** | Caches model files as Blobs in `asrjs-cache-db` | `src/io/cache.ts` (lines 78-198) |
| **Memory cache** | In-memory fallback when IndexedDB unavailable | `src/io/cache.ts` (lines 200-239) |
| **Stream-first handles** | `ResolvedAssetHandle.openStream()` for large assets | `src/io/handles.ts` |
| **Node filesystem provider** | Direct file access for Node.js | `src/io/node-providers.ts` |
| **Model file discovery** | `fetchModelFiles()` lists repo contents | `src/runtime/huggingface.ts` (306 lines) |
| **Quantization detection** | Auto-detect available fp16/int8/fp32 variants | `src/presets/descriptors.ts` (lines 710-742) |

### 4.8 Quality Assessment

| Feature | Description | Implementation |
|---------|-------------|----------------|
| **Compression ratio** | Text length / audio duration heuristic | `src/quality/compression-ratio.ts` |
| **Entropy** | Token-level entropy metrics | `src/quality/entropy.ts` |
| **No-speech probability** | Whisper no-speech token confidence | `src/quality/no-speech.ts` |
| **Temperature fallback** | Retry with higher temperature on low-quality output | `src/quality/temperature-fallback.ts` |

### 4.9 Tokenizers

| Feature | Description | Implementation |
|---------|-------------|----------------|
| **BPE tokenizer** | SentencePiece BPE (Parakeet, NeMo families) | `src/tokenizers/bpe.ts` |
| **Tiktoken tokenizer** | tiktoken-based BPE (Whisper) | `src/tokenizers/tiktoken.ts` |
| **UTF-8 tokenizer** | Simple character-level tokenizer | `src/tokenizers/utf8.ts` |

### 4.10 Pipeline & Post-Processing

| Feature | Description | Implementation |
|---------|-------------|----------------|
| **createSpeechPipeline** | Model-caching, multi-model transcription pipeline | `src/runtime/load.ts` (lines 317-517) |
| **Subtitle generation** | SRT/VTT subtitle output from segments/words | `src/pipeline/subtitles.ts` |
| **Sentence segmentation** | Punctuation-based sentence boundary detection | `src/pipeline/sentence-segmenter.ts` |
| **Output sidecars** | JSON/SRT/VTT sidecar file generation | `src/pipeline/output-sidecars.ts` |
| **Whisper production pipeline** | Full Whisper pipeline with quality gates | `src/pipeline/whisper-production-pipeline.ts` |

### 4.11 Built-in Model Registry (5 presets, 12+ models)

| Preset | Models | Topology |
|--------|--------|----------|
| `parakeet` | 8 variants (parakeet-tdt-0.6b-v2/v3, parakeet-rnnt-1.1b, etc.) | TDT / RNNT |
| `canary` | canary-180m-flash | AED |
| `medasr` | google/medasr | CTC |
| `whisper` | openai/whisper-base, openai/whisper-large-v3 | seq2seq |
| `wav2vec2` | facebook/wav2vec2-base-960h | CTC |

Full metadata in `src/presets/descriptors.ts` (903 lines): modelId, aliases, display name, description, classification, languages, capabilities, inference limits, loading descriptor, controls, warmup, docs.

---

## 5. Key Code Patterns & Techniques

### 5.1 Discriminated Union for Model Loading (model families vs presets)

```typescript
// File: src/types/runtime.ts (lines 144-170)
export type ModelLoadRequest<TLoadOptions = unknown> =
  | FamilyModelLoadRequest<TLoadOptions>   // { family, modelId, ... }
  | PresetModelLoadRequest<TLoadOptions>;  // { preset, modelId?, ... }
```

The runtime resolves preset requests into family requests via `DefaultSpeechRuntime.resolvePresetRequest()` (lines 233-259 in `session.ts`), which calls `preset.resolveModelRequest()` and merges backend/classification.

### 5.2 Factory Pattern for Model Families and Presets

- `SpeechModelFactory` (lines 220-233 in `runtime.ts`): has `family`, `supports()`, `createModel()`
- `SpeechPresetFactory` (lines 236-243): has `preset`, `supports()`, `resolveModelRequest()`
- Each concrete factory is created by a function like `createNemoTdtModelFamily()` or `createParakeetPresetFactory()`

### 5.3 Autoregressive TDT Decode Loop (NemoTdtExecutor)

The executor at `src/models/nemo-tdt/executor.ts` lines 586-720 runs a frame-by-frame autoregressive loop:
1. Copy encoder frame to `encoderFrameBuffer`
2. Run decoder session with encoder output + previous token + state
3. Argmax on logits for token ID (vocab_size entries) + duration (remaining entries)
4. If token != blank: emit token, swap decoder state
5. Advance frame by duration step (or 1 if blank)
6. Report progress every updated frame

### 5.4 Split-Graph Whisper Execution

For Whisper models, the executor supports two paths:
- **Merged**: Single decoder ONNX model handles all steps (KV cache managed internally)
- **Split-graph**: Three separate ONNX sessions -- `decoder_init` (prompt tokens + encoder hidden states -> first logits + present KV), `decoder_step` (single token + past KV -> next logits + updated KV), `decoder_align` (forced alignment for word timestamps)

### 5.5 Cross-Attention DTW for Word Timestamps

`src/models/whisper-seq2seq/executor.ts` lines 737-903:
1. Run forced alignment through the decoder with ground-truth text tokens
2. Extract cross-attention tensors from alignment heads
3. Compute DTW (Dynamic Time Warping) token timestamps via `computeWhisperDtwTokenTimestamps()`
4. Map token timestamps to word boundaries using the tokenizer

### 5.6 Fallback Chain for Backend Selection

`src/runtime/backend.ts` (142 lines) implements a multi-criteria sort:
1. Preferred backend IDs (user-specified order)
2. Precision match
3. Acceleration class (NPU > GPU > hybrid > CPU)
4. SharedArrayBuffer requirements
5. Fallback suitability
6. Priority score

### 5.7 Canonical Transcript Normalization Layer

Seven normalizers in `src/runtime/transcripts.ts` (469 lines) implement the `TranscriptNormalizer<TNative>` interface:
- Each maps family-specific native output -> `TranscriptResult` (stable POJO)
- Normalizers carry model classification metadata
- Metrics are remapped from native field names to canonical field names
- Some normalizers (NeMo AED) have fallback timestamp reconstruction

### 5.8 Progressive Transcription Reporting

All executors implement a 5-stage progress pipeline:
- `start` (0%) -> `preprocess` (20-25%) -> `encode` (40-60%) -> `decode` (40-95%) -> `postprocess` (95%) -> `complete` (100%)
- Each stage reports: elapsedMs, remainingMs (estimated), stage-specific metrics
- Unit-level tracking during decode: completedUnits/totalUnits

### 5.9 Asset Handle Lifecycle

`ResolvedAssetHandle` is stream-first with cleanup:
- `openStream()` for large assets, `readBytes()` for convenience
- `getLocator('url' | 'path')` for ORT sessions that need concrete URLs
- `dispose()` cleans up temporary blob URLs and cache resources
- Asset handles are tracked in executor classes and disposed on executor.dispose()


---

## 6. Relation to S2B2S

S2B2S is a Tauri-based desktop app with STT/TTS/Brain pipelines written in Rust. This library is a TypeScript browser/Node.js library. The relationship is complementary rather than directly competitive.

| Aspect | @asrjs/speech-recognition | S2B2S | Verdict |
|--------|---------------------------|-------|---------|
| **STT engine** | ONNX Runtime (WASM/WebGPU) | transcribe-rs (Parakeet V3 + Whisper + Moonshine) | Different stack; both use ONNX |
| **API design** | Layered: Runtime->Model->Session with factory pattern | Manager pattern: TranscriptionCoordinator->Managers | asrjs has more formal abstraction layers |
| **Transcript contracts** | TranscriptResult (stable POJO, worker-safe) | Custom types via specta bindings | asrjs contracts are more rigorously typed |
| **Model families** | 4 implementation families + 5 presets | 3 STT models (Parakeet/Whisper/Moonshine) | asrjs supports more architectures |
| **Backend abstraction** | Formal ExecutionBackend interface with capability probing | Backend selection embedded in transcribe-rs | asrjs has cleaner backend abstraction |
| **Realtime/streaming** | AudioRingBuffer, StreamingWindowBuilder, UtteranceTranscriptMerger | VAD-based with TripleVAD | asrjs has more sophisticated window/merge logic |
| **Asset loading** | Provider/Cache pattern (HuggingFace, URL, Blob, IndexedDB) | Direct model download via reqwest | asrjs IO layer is more modular |
| **Progress reporting** | 5-stage progressive: preprocess->encode->decode->postprocess->complete | Basic event-based progress | asrjs progress is more granular |
| **Browser vs native** | Browser-first (Web Workers, MediaDevices, IndexedDB) | Native desktop (Tauri, cpal, rodio) | Different target environments |
| **TTS** | None | 8 backends (Piper, Kokoro, Kitten, Pocket, SAPI, OpenAI, ElevenLabs, Cartesia) | S2B2S has TTS; asrjs does not |
| **Brain/LLM** | None | SSE streaming chat + sentence splitter | S2B2S has LLM integration |
| **Long audio** | Windowed transcription with merge strategies | Single-pass or chunked via transcribe-rs | asrjs windowing is more sophisticated |
| **Word timestamps** | Cross-attention DTW (Whisper), frame-based (NeMo TDT), CTC spans (LASR) | Provided by transcribe-rs | Both support word-level timestamps |
| **Language** | TypeScript | Rust (backend) + TypeScript (frontend) | Different ecosystems |

**What S2B2S does better:**
- Full voice-native application (STT -> LLM -> TTS pipeline)
- Native audio I/O (cpal for mic, rodio for playback)
- Desktop integration (system tray, global shortcuts, clipboard, overlay)
- Multiple TTS engine backends
- SQLite history
- Multi-platform desktop (Windows/macOS/Linux)

**What asrjs does better:**
- Formal API abstraction (Runtime -> Model -> Session -> Executor)
- Model-agnostic canonical transcript contract
- Backend capability probing with multi-criteria fallback
- Sophisticated long-audio windowing with model-aware limits
- Cross-attention DTW word timestamps for Whisper
- Split-graph Whisper execution (VRAM optimization)
- Progressive transcription progress with time estimation
- IO layer with provider/cache pattern
- Browser realtime infrastructure (ring buffer, streaming window builder, transcript merger)
- Built-in model registry with quantization detection

---

## 7. Harvest List (Features Worth Copying for S2B2S)

| Feature to harvest | From file | Effort (XS/S/M/L/XL) | Why valuable for S2B2S |
|--------------------|-----------|----------------------|------------------------|
| Canonical transcript contract | `src/types/transcript.ts` (203 lines) | S | A unified `TranscriptResult` POJO across all STT models (Parakeet, Whisper, Moonshine) would simplify the frontend. Currently S2B2S uses model-specific types via specta. |
| Progressive progress pipeline | `src/models/nemo-tdt/executor.ts` (lines 468-855) | M | 5-stage progress (preprocess->encode->decode->postprocess->complete) with time estimation. S2B2S current progress is less granular. |
| Backend capability probing | `src/inference/backends/*/index.ts` + `src/runtime/backend.ts` | M | Formal `ExecutionBackend` interface with `probeCapabilities()` and multi-criteria `selectBackend()`. S2B2S could benefit when adding new STT backends. |
| Model family abstraction | `src/types/runtime.ts` (lines 220-268) | L | `SpeechModelFactory<TLoadOptions, TTranscriptionOptions, TNative>` interface would let S2B2S plug in new STT engines (e.g., sherpa-onnx, faster-whisper) with minimal changes. |
| Per-model inference limits | `src/presets/descriptors.ts` (lines 200-293) | S | `ModelInferenceLimits` (max window, recommended overlap, merge strategy) for each STT model. S2B2S could use similar metadata for auto-configuring chunking. |
| IO provider/cache pattern | `src/io/providers.ts` + `src/io/cache.ts` | M | `AssetProvider` + `AssetCache` pattern for model downloads. S2B2S model downloader in `managers/model.rs` is more monolithic. |
| Long audio windowing with merge | `src/pipeline/long-audio-windowing.ts` + `src/pipeline/whisper-chunking.ts` | M | Overlapping window transcription with automatic merge. S2B2S currently relies on transcribe-rs for this. |
| UtteranceTranscriptMerger | `src/runtime/realtime.ts` (lines 557-731) | M | Sentence-punctuation-aware transcript merging with deduplication tolerance. Would improve S2B2S conversation mode realtime display. |
| AudioRingBuffer + StreamingWindowBuilder | `src/runtime/realtime.ts` (lines 21-423) | M | Sophisticated audio buffering with activity-boundary-aware window building. More advanced than S2B2S current VAD->chunk approach. |
| Worker-safe transcript design | Design pattern throughout | S | Ensuring transcript objects are structured-clone-safe POJOs. S2B2S uses specta which handles serialization, but the discipline is valuable. |

---

## 8. Known Issues, Caveats & Limitations

| Issue | Severity | Impact |
|-------|----------|--------|
| **WASM/WebGPU backends are STUBS** | HIGH | `createWasmBackend()` and `createWebGpuBackend()` throw `NotImplementedSpeechFeatureError` on `createExecutionContext()`. All actual execution happens through individual model-family ORT session creation. The `ExecutionBackend` abstraction is unused for real work. |
| **No built-in resampling** | MEDIUM | Executors warn about sample rate mismatch but do not resample. The `SampleRatePolicy` interface exists but is passthrough-only. |
| **Browser-only VAD** | MEDIUM | Ten VAD and FireRed VAD are browser-only. Node.js VAD is not implemented. |
| **No Node.js streaming** | MEDIUM | The realtime helpers (AudioRingBuffer, StreamingWindowBuilder, RealtimeTranscriptionController) are browser-focused. Node.js streaming support is minimal. |
| **Executor file sizes** | LOW | `OrtNemoTdtExecutor` (866 lines), `WhisperOnnxExecutor` (~2000+ lines), `OrtLasrCtcExecutor` (870 lines) -- these are large single files that break the ESLint 800-line warning. |
| **No TTS, no LLM** | N/A | This is by design -- the library is intentionally speech-first and STT-only. |
| **FireRed LLM executor missing** | LOW | `src/models/firered-llm.ts` exists but appears to be a stub/placeholder. |
| **webgpu-hybrid backend selection** | LOW | The `webgpu-hybrid` backend concept (encoder on GPU, decoder on CPU) is described in the README but the hybrid execution path is limited to resolving component backends in `descriptors.ts`, not a real combined backend. |
| **No Web Speech API fallback** | MEDIUM | The library does not bridge to the browser built-in `webkitSpeechRecognition` / `SpeechRecognition` API. It is ONNX-only. |
| **docs/ file with colon in filename** | LOW | `docs/whisper_onnx_browser_full_export_report.md:Zone.Identifier` -- this is a Windows ADS artifact that prevents `git checkout` on Windows. |

---

## 9. Strengths & Weaknesses

### Strengths

1. **Exceptional API design**: The Runtime->Model->Session->Executor layer with factory pattern is one of the cleanest multi-model abstraction designs seen in open-source STT libraries.

2. **Canonical transcript contract**: `TranscriptResult` as a stable POJO that crosses worker boundaries is a genuine innovation. The `TranscriptNormalizer<TNative>` pattern cleanly separates model-native output from the app-facing canonical format.

3. **Model family extensibility**: Adding a new ASR architecture requires only implementing `SpeechModelFactory` and optionally a `SpeechPresetFactory`. The runtime handles everything else (backend selection, asset loading, lifecycle).

4. **Progressive progress reporting**: The 5-stage pipeline with time estimation (preprocess->encode->decode->postprocess->complete) is feature-complete and implemented consistently across all three major executors.

5. **Sophisticated Whisper support**: Split-graph execution, DTW cross-attention word timestamps, beam search, temperature fallback, quality gates -- the Whisper implementation rivals standalone libraries.

6. **Browser realtime infrastructure**: `AudioRingBuffer`, `StreamingWindowBuilder`, `UtteranceTranscriptMerger`, `RealtimeTranscriptionController` form a complete realtime transcription stack that most libraries lack.

7. **Thorough IO abstraction**: The provider/cache pattern handles HuggingFace, URL, Blob, local files, and IndexedDB caching uniformly through `ResolvedAssetHandle`.

8. **Documentation quality**: 104-line model family index, per-model architecture docs, skill references, handoff notes, export reports -- the docs folder alone is a knowledge base.

9. **Test coverage**: ~100+ test files covering executors, tokenizers, pipelines, alignment, streaming, quality gates, and model manifests.

10. **Explicit design rules**: The architecture.md and README both document intentional design constraints (preset thinness, runtime as orchestration, browser-only code off root path).

### Weaknesses

1. **Stub backends**: The `ExecutionBackend` abstraction is architecturally clean but practically unused -- all ORT sessions are created directly in model-family code. The backend abstraction exists primarily for capability probing and selection.

2. **No resampling**: Audio sample rate mismatches produce warnings but are not auto-corrected. This is a practical gap for real-world usage.

3. **Browser bias**: The realtime infrastructure (ring buffers, VAD adapters, MediaDevices capture, waveform renderers) is browser-focused. Node.js streaming support is thin.

4. **Large single files**: The Whisper executor at ~2000+ lines and NeMo TDT executor at 866 lines are monolithic. The ESLint 800-line warning is consistently exceeded by the most important files.

5. **No Web Speech API fallback**: Being ONNX-only means no graceful degradation on platforms without WASM/WebGPU support.

6. **v0.2.0 maturity**: The library has not reached v1.0. Some paths (FireRed LLM, WebNN backend) are stubs.

---

## 10. Bottom Line / Verdict

`@asrjs/speech-recognition` is the most architecturally sophisticated TypeScript ASR library available. Its layered API (Runtime -> Model -> Session -> Executor), canonical transcript contract, progressive progress reporting, and model-family extensibility pattern represent best-in-class design for multi-model speech inference runtimes. The weakness is that the clean backend abstraction is undermined by stub implementations, and the actual execution happens through model-family-specific ORT session creation.

For S2B2S, the single most valuable idea to harvest is the **canonical transcript contract** (`TranscriptResult` as a structured-clone-safe POJO normalized across model families through `TranscriptNormalizer<TNative>`). This would dramatically simplify S2B2S frontend, which currently deals with model-specific transcript types. The **progressive progress pipeline** (5 stages with time estimation) and **per-model inference limits** (auto-windowing metadata) are the next-most valuable patterns, directly applicable to improving S2B2S STT UX. The library is worth deep study for its API design patterns even though its execution stack (ONNX Runtime Web, browser-first) does not directly apply to S2B2S Rust/Tauri stack.
