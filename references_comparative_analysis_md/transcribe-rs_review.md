# transcribe-rs — Library

> Repo: `cjpais/transcribe-rs` · Version: `0.3.11` · License: MIT · Author: Ilya Stupakov / cjpais
> Nature: library (independent — not a fork)
> Role for S2B2S: Core STT engine dependency. S2B2S delegates all local ASR inference to transcribe-rs.

---

## 1. What transcribe-rs Is

transcribe-rs is a multi-engine speech-to-text Rust library providing a unified `SpeechModel` trait over seven ONNX-based models (Parakeet, Canary, Cohere, Moonshine, SenseVoice, GigaAM, and Moonshine Streaming), plus Whisper via whisper.cpp, Whisperfile via an HTTP server wrapper, and a remote OpenAI API client (async). Every local engine accepts 16 kHz mono f32 PCM audio and returns a `TranscriptionResult` with optional word/segment-level timestamps.

The library is designed as a building block for desktop voice applications. It handles model loading, ONNX session management, feature extraction (mel spectrograms, FBANK, LFR, CMVN), CTC and autoregressive decoding, VAD-based chunked transcription, and hardware acceleration selection (CUDA, DirectML, Metal, Vulkan, CoreML, ROCm, XNNPACK, TensorRT, WebGPU). Engines are feature-gated so consumers only compile what they need.

S2B2S pins this at version `0.3.11` with features `onnx`, `whisper-cpp`, plus platform-specific accelerator flags (`ort-directml` on Windows, `whisper-metal` on macOS, `whisper-vulkan` on Linux).

---

## 2. Tech Stack

### 2.1 Core / Backend

| Layer | Choice | Purpose |
|---|---|---|
| Language | Rust 2021 edition | Full library |
| ONNX runtime | `ort` 2.0.0-rc.12 | Inference for all 7 ONNX engines |
| Linear algebra | `ndarray` 0.17 | Tensor manipulation |
| FFT | `rustfft` 6 | Mel spectrogram / FBANK |
| Audio I/O | `hound` 3.5.1 | WAV reading/writing |
| Serialization | `serde` / `serde_json` | Config, tokenizers, metadata |
| Error handling | `thiserror` 2 | `TranscribeError` enum |
| Logging | `log` + `env_logger` | Tracing |
| Regex | `regex` 1.11 | Token post-processing |
| HTTP (sync) | `ureq` 3 | Whisperfile server |
| HTTP (async) | `async-openai` / `tokio` | OpenAI API |
| Whisper bindings | `whisper-rs` 0.16 | whisper.cpp FFI |

### 2.2 Key Non-Obvious Dependencies

- **`once_cell`** — Lazy static regex for Parakeet token joining
- **`base64`** — FunASR Nano symbol table decoding
- **`ndarray::ShapeError` → `TranscribeError`** — Frequent failure mode in encoder/decoder tensor reshaping
- **`whisper_rs_sys` (raw FFI)** — Direct `ggml_backend_dev_*` calls for GPU enumeration

---

## 3. Architecture & Source Map

```
transcribe-rs/
├── Cargo.toml               (171 lines) — Feature flags, dependencies, example/test registration
├── build.rs                 (8 lines)   — Windows: links advapi32
├── README.md                (462 lines) — Comprehensive docs, quick starts, model download table
├── ADDING_ENGINES.md        (366 lines) — Contributor guide: models + engine families
├── LICENSE                  (MIT)
├── .cargo/config.toml       (7 lines)   — Dev aliases
├── .github/workflows/
│   ├── test.yml             (193 lines) — Multi-OS CI, downloads models, runs tests
│   └── fmt.yml              (14 lines)  — rustfmt check
│
├── src/
│   ├── lib.rs               (287 lines) — SpeechModel trait, TranscriptionResult, TranscribeOptions
│   ├── error.rs             (51 lines)  — TranscribeError enum + From impls
│   ├── audio.rs             (109 lines) — read_wav_samples(), prepend_silence()
│   ├── accel.rs             (505 lines) — Global atomic accelerator preferences (ORT + Whisper)
│   ├── whisperfile.rs       (503 lines) — Whisperfile HTTP server wrapper
│   │
│   ├── features/            # Shared audio feature extraction (audio-features)
│   │   ├── mel.rs           (259 lines) — FBANK + mel spectrogram computation
│   │   ├── cmvn.rs          (18 lines)  — Cepstral Mean-Variance Normalization
│   │   └── lfr.rs           (35 lines)  — Low Frame Rate stacking
│   │
│   ├── decode/              # Shared token decoding (audio-features)
│   │   ├── ctc.rs           (54 lines)  — CTC greedy search + timestamps
│   │   ├── greedy.rs        (127 lines) — GreedyDecoder: EOS + repetition guard
│   │   ├── sentencepiece.rs (94 lines)  — SentencePiece concat, byte-level BPE
│   │   └── tokens.rs        (92 lines)  — load_vocab(), SymbolTable
│   │
│   ├── onnx/                # ONNX engine family (feature: onnx)
│   │   ├── mod.rs           (28 lines)  — Quantization enum, engine declarations
│   │   ├── session.rs       (289 lines) — EP selection, create_session, resolve_model_path
│   │   ├── PORTING.md       (249 lines) — Detailed porting guide
│   │   ├── parakeet/mod.rs  (682 lines) — Parakeet V3: 3-session RNN-T + hierarchical timestamps
│   │   ├── canary/          (604 lines) — Canary Flash/V2: encoder-decoder + prompt tokens
│   │   │   ├── mod.rs       (288 lines)
│   │   │   ├── decoder.rs   (134 lines)
│   │   │   └── vocab.rs     (182 lines)
│   │   ├── cohere/mod.rs    (409 lines) — Cohere: 5-part KV cache, byte-level BPE
│   │   ├── moonshine/       (1307 lines) — Moonshine: non-streaming + streaming
│   │   │   ├── mod.rs       (71 lines)
│   │   │   ├── model.rs     (433 lines)
│   │   │   └── streaming.rs (803 lines)
│   │   ├── sense_voice/mod.rs (434 lines) — SenseVoice: FBANK+LFR+CMVN, metadata-driven
│   │   └── gigaam/mod.rs    (176 lines) — GigaAM: simplest single-session CTC
│   │
│   ├── whisper_cpp/         # whisper.cpp engine (feature: whisper-cpp)
│   │   ├── mod.rs           (301 lines) — WhisperEngine: GGML load + beam search
│   │   └── gpu.rs           (137 lines) — Raw GGML FFI GPU enumeration
│   │
│   ├── remote/              # Remote engines (feature: openai)
│   │   ├── mod.rs           (22 lines)  — RemoteTranscriptionEngine async trait
│   │   └── openai.rs        (257 lines) — OpenAI Whisper-1 + GPT-4o-transcribe
│   │
│   ├── transcriber/         # Chunked transcription strategies
│   │   ├── mod.rs           (163 lines) — Transcriber trait, transcribe_padded()
│   │   ├── vad_chunked.rs   (754 lines) — VAD-based: onset/hangover/prefill, smart split
│   │   ├── energy_adaptive_chunked.rs (442 lines) — Energy-based: low-energy split points
│   │   ├── merge.rs         (150 lines) — Merge chunk results with separator
│   │   └── test_helpers.rs  (89 lines)  — MockModel, FailOnNthModel
│   │
│   └── vad/                 # Voice Activity Detection
│       ├── mod.rs           (467 lines) — Vad trait, EnergyVad, SmoothedVad
│       └── silero.rs        (171 lines) — SileroVad: ONNX LSTM, stateful h/c
│
├── examples/                (9 files)  — One per engine + moonshine_streaming
├── tests/                   (11 files) — Per-engine + transcriber + vad_silero
│   └── common/mod.rs        (14 lines)  — require_paths() skip helper
└── samples/                 (6 WAV files) — jfk, dots, german, itn, pnc, product_names, russian
```

Total Rust source: ~7,500 lines across 50+ files.

---

## 4. Feature Inventory

### 4.1 The SpeechModel Trait (Unified API)

**File:** `src/lib.rs` (287 lines). All local engines implement:

```rust
pub trait SpeechModel: Send {
    fn capabilities(&self) -> ModelCapabilities;
    fn transcribe_raw(&mut self, samples: &[f32], options: &TranscribeOptions) -> Result<TranscriptionResult, TranscribeError>;
    fn transcribe(&mut self, samples: &[f32], options: &TranscribeOptions) -> Result<TranscriptionResult, TranscribeError> { /* adds silence padding */ }
    fn transcribe_file(&mut self, wav_path: &Path, ...) -> Result<TranscriptionResult, TranscribeError> { /* reads WAV */ }
    fn default_leading_silence_ms(&self) -> u32 { 0 }
    fn default_trailing_silence_ms(&self) -> u32 { 0 }
}
```

Key decisions: `Send` bound enables `Box<dyn SpeechModel + Send>`. `transcribe_raw()` is the abstract method; `transcribe()` wraps it with silence padding + timestamp adjustment. Every engine also exposes `transcribe_with(&mut self, samples, &{Model}Params)` for engine-specific parameters. `ModelCapabilities` reports name, engine_id, sample_rate, languages, supports_timestamps, supports_translation, supports_streaming.

### 4.2 RemoteTranscriptionEngine (Async API)

**File:** `src/remote/mod.rs` (22 lines). Separate async trait: `transcribe_file(&self, wav_path, params) -> Result<TranscriptionResult, TranscribeError>`.

### 4.3 STT Engines — Detailed

#### 4.3.1 Parakeet V3 (`src/onnx/parakeet/mod.rs`, 682 lines)

NVIDIA NeMo Parakeet-TDT 0.6B. Three ONNX sessions: preprocessor (nemo128.onnx), encoder, decoder_joint. RNN-T transducer loss decode.

- **Decode:** Frame-aligned — per encoder timestep, `decode_step()` feeds previous tokens + encoder output through decoder, predicts one token. Tracks state (input_states_1/2).
- **Timestamp hierarchy:** Token → Word → Segment. Words grouped from tokens; segments split on `.`, `?`, `!`.
- **Constants:** `SUBSAMPLING_FACTOR = 8`, `WINDOW_SIZE = 0.01`, `MAX_TOKENS_PER_STEP = 10`
- **Default leading silence:** 250 ms (mel windowing attenuates onset)
- **Languages:** English only
- **Benchmark:** ~30x real-time MBP M4 Max, ~5x Skylake

#### 4.3.2 Canary (`src/onnx/canary/`, 604 lines total)

NVIDIA NeMo Canary 180M Flash / 1B V2. Preprocessor → encoder → autoregressive decoder with cross-attention KV cache.

- **Variant detection:** Vocabulary size <10k = Flash (4 lang), >=10k = V2 (25 lang)
- **Prompt:** 9-token sequence: startofcontext, startoftranscript, emotion, src/tgt lang, PNC/NoPNC, ITN/NoITN, notimestamp, nodiarize
- **PNC:** Punctuation/capitalization toggle
- **ITN:** Inverse text normalization; Flash silently ignores
- **Translation:** Set `target_language`
- **Decoder** (`decoder.rs`, 134 lines): KV cache management via `decoder_mems`, shared `GreedyDecoder`

#### 4.3.3 Cohere (`src/onnx/cohere/mod.rs`, 409 lines)

14 languages including CJK. Encoder/decoder with 5-part KV cache (self-k/v, cross-k/v, offset) pre-allocated at max sequence length (1024). Fallback input name resolution to accommodate different export conventions. Byte-level BPE via `parse_byte_token()` for CJK reassembly. Searches both `model_dir/` and `model_dir/onnx/` for files.

#### 4.3.4 Moonshine Non-Streaming (`src/onnx/moonshine/model.rs`, 433 lines)

UsefulSensors Moonshine. Split encoder/decoder. Model files: `encoder_model.onnx`, `decoder_model_merged.onnx`, `tokenizer.json` (HuggingFace).

- **Variants:** Tiny (en/ar/zh/ja/ko/uk/vi, 6 layers, 36 head_dim) vs Base (en/es, 8 layers, 52 head_dim). token_rate: 6 (en), 8 (uk), 13 (CJK)
- **Custom KV cache:** `HashMap<String, ArrayD<f32>>` per-layer per-attention-type per-kv-type
- **Tokenizer:** Parses HuggingFace tokenizer.json; decodes via byte-level BPE
- **Audio limits:** 0.1s – 64s

#### 4.3.5 Moonshine Streaming (`src/onnx/moonshine/streaming.rs`, 803 lines)

5-session pipeline: frontend → encoder → adapter → cross_kv → decoder_kv. 18-field `StreamingState` tracks buffers, accumulated features, encoder frames, adapter position, memory, self/cross KV caches. Binary `.bin` tokenizer format. Config from `streaming_config.json` (16 fields). Only engine with `supports_streaming: true`.

#### 4.3.6 SenseVoice (`src/onnx/sense_voice/mod.rs`, 434 lines)

Alibaba FunASR SenseVoice. Single-session CTC. Pipeline: FBANK (80 mel, Hamming, pre-emphasis 0.97) → LFR → CMVN. Reads 10+ metadata keys from ONNX (vocab_size, blank_id, lfr params, lang-to-id, neg_mean/inv_stddev, ITN IDs). FunASR Nano support via base64 symbol table decode. Languages: zh, en, ja, ko, yue. Timestamps from CTC frame indices.

#### 4.3.7 GigaAM (`src/onnx/gigaam/mod.rs`, 176 lines)

SberDevices GigaAM V3. Simplest model (reference for porting). Single-session CTC. Hann window, 64 mel, n_fft=320. Russian only. Filters `<unk>` tokens from output.

#### 4.3.8 Whisper (`src/whisper_cpp/mod.rs`, 301 lines)

OpenAI Whisper via whisper.cpp. Single GGML file. Dynamic capabilities based on `is_multilingual`. Beam search (size=3). GPU auto-selection via `gpu.rs`.

#### 4.3.9 Whisperfile (`src/whisperfile.rs`, 503 lines)

Mozilla Whisperfile binary wrapper. Spawns child HTTP server, poll `GET /` until ready, `POST /inference` multipart form, kill on Drop. Custom multipart builder. GPU mode: Auto/Apple/AMD/NVIDIA/Disabled.

#### 4.3.10 OpenAI Remote (`src/remote/openai.rs`, 257 lines)

Async API client. Models: whisper-1 (verbose_json + timestamps), gpt-4o-mini-transcribe, gpt-4o-transcribe (json only). Generic over `async_openai::config::Config`.

### 4.4 Hardware Acceleration

**Files:** `src/accel.rs` (505 lines), `src/onnx/session.rs` (289 lines), `src/whisper_cpp/gpu.rs` (137 lines)

Global atomic preferences (lock-free, set-once-early):

**ORT Accelerators** (9 variants): Auto, CpuOnly, Cuda (~800MB binary), TensorRt (falls back to CUDA), DirectMl (Windows, sequential exec), Rocm (AMD), CoreMl (macOS Neural Engine), WebGpu (Dawn, sequential exec), Xnnpack (own threadpool).

**Whisper Accelerators:** Auto, CpuOnly, Gpu. Backend selected at compile time via feature.

**GPU auto-selection** (`gpu.rs`): `list_gpu_devices()` via raw GGML FFI. Prefers dedicated GPUs, then most VRAM. `GpuDeviceInfo`: id, name, kind, total_vram, free_vram.

**Session builder** (`session.rs`): Reads global preference, builds EP priority list, handles DirectML/WebGPU sequential requirements, XNNPACK threadpool isolation, auto-fallback from quantized to FP32.

### 4.5 Feature Extraction

**Files:** `src/features/` (312 lines). `mel.rs` (259 lines): Two paths — `compute_fbank()` (SenseVoice: pre-emphasis → window → FFT → mel → log, supports unnormalized scaling) and `compute_mel_spectrogram()` (GigaAM/Moonshine: window → FFT → mel filterbank dot product → log). Both use `rustfft::FftPlanner`. `MelConfig` with 10 settings. `lfr.rs` (35 lines): Frame stacking with stride. `cmvn.rs` (18 lines): In-place normalization.

### 4.6 Decoding

**Files:** `src/decode/` (367 lines). `ctc.rs` (54 lines): Argmax per timestep, skip blank and repeats, returns tokens + frame timestamps. `greedy.rs` (127 lines): Autoregressive `GreedyDecoder` with EOS stop and repetition guard (max 8 consecutive). `sentencepiece.rs` (94 lines): `sentencepiece_to_text()`, `parse_byte_token()` for byte-level BPE. `tokens.rs` (92 lines): `load_vocab()` (token id format), `SymbolTable` (symbol id format, base64-decodable).

### 4.7 Voice Activity Detection

**Files:** `src/vad/` (638 lines). `Vad` trait: `frame_size()`, `is_speech()`, `drain_prefill()`, `reset()`. `EnergyVad`: RMS threshold, zero deps. `SileroVad` (`silero.rs`, 171 lines): ONNX LSTM model, (2,1,64) h/c states, `speech_probability()` for raw confidence. `SmoothedVad` (`mod.rs`, 467 lines): Onset detection (N consecutive speech), hangover (N silence before exit), prefill ring buffer (N+1 frames). State machine with `at_onset` flag for `drain_prefill()`.

### 4.8 Chunked Transcription

**Files:** `src/transcriber/` (1598 lines). `Transcriber` trait: `feed()` for incremental, `finish()` to flush, object-safe (`Box<dyn Transcriber>`). `VadChunked` (`vad_chunked.rs`, 754 lines): VAD boundaries, min/max chunk duration, smart split (low-energy frame search on force-split), carry-forward short speech, prefill recovery, pending frame alignment. `EnergyAdaptiveChunked` (`energy_adaptive_chunked.rs`, 442 lines): Fixed-duration with energy-based split point search (RMS minimum in search window). `merge.rs` (150 lines): Merge with separator (space/empty).

### 4.9 Build Configuration

**File:** `Cargo.toml` (171 lines). Default: `[]`. Feature tree: `audio-features` → `onnx` → `ort-*` accelerators; `whisper-cpp` → `whisper-metal/vulkan/cuda`; `vad-silero` standalone; `all` = onnx + whisper-cpp + whisperfile + openai.

### 4.10 CI Pipeline

**File:** `.github/workflows/test.yml` (193 lines). Matrix: ubuntu, macos, windows. Downloads + caches models (Moonshine, Parakeet, SenseVoice, Silero VAD, Whisper tiny, Whisperfile). Installs Vulkan SDK. Runs tests in stages per feature set.

---

## 5. Key Code Patterns & Techniques

### 5.1 Engine Abstraction Pattern

Every ONNX engine follows an identical documented template (see `ADDING_ENGINES.md`, `PORTING.md`):
1. `const CAPABILITIES: ModelCapabilities`
2. `Model::load(model_dir, &Quantization) -> Result<Self, TranscribeError>`
3. `#[derive(Debug, Clone, Default)] {Model}Params`
4. `transcribe_with(&mut self, samples, &{Model}Params)` + `SpeechModel::transcribe_raw()`
5. `session::create_session()` with Level3 optimization
6. All errors through `TranscribeError`

### 5.2 ONNX Session Patterns

- Input construction: `TensorRef::from_array_view(arr.view())` in `inputs![]` → `session.run(inputs)` → `outputs["name"].try_extract_array::<f32>()`
- Scoped borrow for output extraction before `.remove()` (Canary, Cohere)
- Output pass-through: session outputs fed as next session inputs without data copy
- Metadata-driven config: SenseVoice reads 10+ ONNX metadata keys

### 5.3 Accelerator Selection

Global atomics (`AtomicU8`, `AtomicI32`): `set_ort_accelerator()`, `get_ort_accelerator()`. `session.rs` reads on every `create_session()` call. Lock-free, set-once-early model.

### 5.4 Token Decoding Patterns

Five tokenizer strategies coexist: simple vocab (Vec indexed by ID, Parakeet/GigaAM), SymbolTable (HashMap, SenseVoice), custom bidirectional vocab (Canary), HuggingFace tokenizer.json (Moonshine), binary tokenizer (Moonshine Streaming). Byte-level BPE via `parse_byte_token()` shared by Cohere and Moonshine.

### 5.5 Chunked Transcription Design

Dual-mode: file mode (feed all samples, get N results) and live mode (feed 30ms frames, occasional results). Reusable after `finish()`. Error-resilient (tested with `FailOnNthModel`). Object-safe trait.

### 5.6 Performance Optimizations

- Padding fast path (skip allocation when none needed)
- KV cache ownership avoids copies (DynValue, HashMap)
- Pre-allocated caches at max sequence length
- Lazy regex compilation with `once_cell::sync::Lazy`

### 5.7 Platform-Specific

- Windows: DirectML sequential exec, advapi32.lib for Vulkan registry
- macOS: CoreML auto-enabled, Metal for whisper
- Linux: Vulkan + Swiftshader CI fallback

---

## 6. Relation to S2B2S

### 6.1 How S2B2S Uses transcribe-rs

**Dependencies** (`src-tauri/Cargo.toml`):
```toml
# Base
transcribe-rs = { version = "0.3.11", features = ["whisper-cpp", "onnx"] }
# Windows: +ort-directml
# macOS: +whisper-metal
# Linux: +whisper-vulkan
```

**Usage points:**

1. **`src/stt/multi_stt.rs`** (276 lines) — Multi-STT engine dispatch. Branches for Whisper, Parakeet, Moonshine, MoonshineStreaming, SenseVoice, GigaAM, Canary, Cohere. Each calls `transcribe_transcribe_rs()` loading model, transcribing, and dropping it per call.

2. **`src/managers/transcription.rs`** — `apply_accelerator_settings()` maps S2B2S settings to transcribe-rs globals: `accel::set_whisper_accelerator()`, `accel::set_ort_accelerator()`, `accel::set_whisper_gpu_device()`. `list_gpu_devices()` for GPU picker UI (with FMA3 guard).

3. **`src/managers/model.rs`** — References transcribe-rs model directory layouts for downloads.

4. **`src/lib.rs` line 657** — First `list_gpu_devices()` call noted.

5. **`src/wake_word.rs`** — Comment about `/MD` dynamic CRT on Windows.

### 6.2 Comparison Table

| Aspect | transcribe-rs | S2B2S | Verdict |
|--------|---------------|-------|---------|
| Engine count | 10 (7 ONNX + 3 other) | 8 via transcribe-rs + 1 Python | S2B2S uses engine abstraction directly |
| Engine abstraction | `SpeechModel` trait | `EngineType` enum + closures | S2B2S wraps rather than builds own |
| Model lifecycle | Persistent (load once) | Load-and-drop per call | ⚠️ S2B2S inefficient; 1-2s load overhead |
| VAD | EnergyVad, SmoothedVad, SileroVad | vad-rs + RNNoise + TripleVAD | S2B2S has more sophisticated pipeline |
| Chunked transcription | `VadChunked` + `EnergyAdaptiveChunked` | Not using | S2B2S could adopt directly |
| Silence padding | Per-engine (Parakeet: 250ms) | Not using | May be losing audio onset |
| Accelerator management | Global atomics | Wraps in settings UI | Correct delegation |
| GPU enumeration | Raw GGML FFI | Consumed for GPU picker | Correct pass-through |
| Tokenizer support | 5 formats | text-processing-rs pipeline | Complementary |

---

## 7. Harvest List

| Feature to harvest | From file | Effort | Why valuable |
|---------------------|-----------|--------|-------------|
| `VadChunked` streaming transcriber | `vad_chunked.rs` | S | VAD-aware chunking for continuous voice mode |
| `EnergyAdaptiveChunked` | `energy_adaptive_chunked.rs` | S | Energy-based splitting without VAD model |
| Silence padding per engine | `lib.rs` SpeechModel::transcribe | XS | Fix Parakeet onset drop |
| `GreedyDecoder` repetition guard | `decode/greedy.rs` | XS | Prevent runaway token repetition |
| Smart split for long utterances | `VadChunked::smart_split_buffer()` | S | Natural pause point finding |
| Persistent model instances | All `Model::load()` | M | Eliminate ~1-2s per-utterance load time |
| `SmoothedVad` prefill buffer | `vad/mod.rs` | XS | Recover pre-onset audio |
| Auto-fallback quantization | `session::resolve_model_path()` | XS | Graceful quantized→FP32 fallback |
| SymbolTable base64 decode | `decode/tokens.rs` | XS | FunASR Nano model support |

---

## 8. Known Issues, Caveats & Limitations

| Issue | Severity | Impact |
|-------|----------|--------|
| Most engines not streaming — only Moonshine Streaming supports true streaming | Medium | Dictation latency |
| ORT CUDA adds ~800 MB binary size | Medium | Distribution size |
| DirectML incompatible with Auto mode | Low | Must be explicitly selected |
| Cohere local translation unsupported | Low | No local translate |
| Canary ITN silently disabled on Flash | Low | Silent behavior difference |
| Moonshine duration limits (0.1s–64s) | Low | Edge case errors |
| Whisperfile poll-based startup | Low | Startup race potential |
| No beam search for ONNX engines | Low | Accuracy tradeoff |
| `nemo-text-processing` commented out | Low | External ITN needed |
| **S2B2S loads models per call** | **High** | **1-2 second overhead per utterance** |

---

## 9. Strengths & Weaknesses

### Strengths

1. **Most comprehensive Rust STT library** — 10 engines unified under one trait
2. **Clean, documented engine abstraction** — 366-line contributor guide, 249-line porting guide
3. **Hardware acceleration done right** — atomic preferences, compile-time gates, runtime EP selection, graceful fallback
4. **Complete audio pipeline** — mel, FBANK, LFR, CMVN, CTC, greedy, SentencePiece, byte BPE
5. **Production-grade VAD** — EnergyVad (zero deps), SmoothedVad (onset+hover), SileroVad (ONNX LSTM)
6. **Chunked transcription** — VAD-based and energy-based, smart split, carry-forward, streaming
7. **Metadata-driven config** — SenseVoice reads model config from ONNX metadata
8. **Multi-OS CI** — Full integration tests with actual models on all platforms

### Weaknesses

1. **`ort` RC version pin** — Locks to pre-release
2. **Synchronous API** — No async for local inference (mitigated by thread pool)
3. **Hardcoded filenames** — No model manifest/registry
4. **Fragile Whisperfile wrapper** — Child process management
5. **No AEC or noise suppression**
6. **S2B2S does not use persistent instances** — Library supports it; S2B2S does not

---

## 10. Bottom Line / Verdict

transcribe-rs is the most mature and comprehensive Rust STT library, directly powering S2B2S's entire local ASR pipeline. Its 10-engine coverage, unified `SpeechModel` trait, built-in feature extraction and decoding, production-grade VAD, and sophisticated hardware acceleration management make it the ideal dependency for any Rust voice application. The library is well-documented for contributors and correctly handles platform-specific GPU quirks.

The single most valuable idea: **`VadChunked` transcriber with prefill-aware onset capture** for continuous voice mode. Second: **silence padding per engine** to fix Parakeet onset drop. Quick win: **`GreedyDecoder` repetition guard** (127-line module). Critical optimization: **persistent model instances** — S2B2S currently pays ~1-2 seconds load time per utterance.

**Worth studying?** Yes — this is the reference implementation for multi-engine Rust ASR.

---

*(Analysis completed. All 50+ files read across 8 subsystems. Total: ~7,500 lines of Rust source.)*
