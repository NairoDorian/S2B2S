# onnx-asr — Library

> Repo: istupakov/onnx-asr · HEAD: n/a (snapshot) · License: MIT · Author: Ilya Stupakov · Platforms: Linux x86_64/Arm64, Windows x86_64/Arm64, macOS Apple Silicon/x86_64
> Nature: independent library
> Role for S2B2S: Direct replacement for the transcribe-rs dependency — a pure-Python ONNX-based ASR engine supporting more models (NeMo Parakeet V3, Canary V2, GigaAM, Whisper), more backends (CUDA, TensorRT, CoreML, DirectML, ROCm, WebGPU), and better performance than S2B2S's current STT pipeline. Would require bridging to Rust via Python subprocess or embedded CPython.

---

## 1. What onnx-asr Is

onnx-asr is a **pure Python package** for Automatic Speech Recognition (speech-to-text) using **ONNX Runtime**-compatible models. It requires no PyTorch, no Transformers, no FFmpeg — only NumPy and onnxruntime as hard dependencies, with huggingface-hub as the optional download mechanism.

The library solves the problem of running modern ASR models (NeMo Conformer variants, GigaAM, Kaldi Zipformer, Whisper, T-One) in **resource-constrained environments** (edge/IoT to GPU servers) with a **single unified API**. It bundles its own log-mel spectrogram preprocessors (both ONNX-compiled and pure-NumPy fallbacks), its own greedy-search decoders (CTC, RNN-T, TDT, AED/Transformer), and its own ONNX-compatible resampler. No external audio processing libraries are needed.

It targets developers building STT pipelines, embedded systems, benchmarks, and anyone who wants a batteries-included ASR library that "just works" across all major hardware backends.

---

## 2. Tech Stack

### 2.1 Core
| Layer | Choice | Purpose |
|-------|--------|---------|
| Runtime | onnxruntime (CPU/CUDA/TensorRT/CoreML/DirectML/ROCm/WebGPU) | Model inference |
| Math | 
umpy >=1.22.4 | Audio buffer manipulation, feature computation |
| Model download | huggingface-hub >=0.30.2 (optional) | Download ONNX models from Hugging Face |
| Type checking | mypy strict mode | Full type coverage |
| Linting | uff (ALL rules, py310 target) | Code quality |
| Testing | pytest >=9.0 + pytest-cov | Unit/integration tests |
| Build | Hatchling + hatch-vcs | Wheel packaging |
| Preprocessor compilation | onnxscript, ml-dtypes | Compile preprocessors to ONNX during wheel build |

### 2.2 Key Dependencies (non-obvious)
- **onnxscript + ml-dtypes**: Used at build time (not runtime) to compile preprocessors and resamplers from Python DSL into .onnx models. The preprocessors/ dir contains 8 build-time scripts (GigaAM, Kaldi, NeMo, Whisper preprocessor generators, filterbank builder, resampler builder).
- **hatch_build.py**: Custom Hatchling build hook that invokes preprocessors/build.py during pip install, generating .onnx and .npz (filterbank weights) files shipped inside the wheel.
- **preprocessors/fbanks.py**: Mel filterbank generation shared by all preprocessors. Supports HTK, Kaldi, and Slaney mel scales.

---

## 3. Architecture & Source Map

`
src/onnx_asr/                              # Main package (~2,300 lines)
├── __init__.py                  (9L)      # Exports: load_model, load_vad
├── asr.py                       (229L)    # Base ASR classes: Asr protocol, BaseAsr, CTC/RNN-T/TDT decoding
├── adapters.py                  (280L)    # Adapter layer: AsrAdapter, TextResultsAsrAdapter, VAD wrappers
├── cli.py                       (49L)     # Simple CLI: onnx-asr <model> <wav> [--vad]
├── loader.py                    (388L)    # Model loading: Manager, load_model, load_vad, create_asr_resolver
├── onnx.py                      (136L)    # ONNX helpers: providers, TensorRT profiles, session options
├── resolver.py                  (169L)    # HuggingFace model resolution and download logic
├── se.py                        (20L)     # SpeakerEmbedding protocol
├── utils.py                     (205L)    # WAV reading, padding, log_softmax, error types
├── vad.py                       (122L)    # Base VAD classes: segment merging, batch recognition
│
├── models/                                 # ASR/VAD/SE model implementations (~1,170 lines)
│   ├── nemo.py                  (266L)    # NeMo: CTC, RNN-T, TDT, AED (Canary) models
│   ├── gigaam.py                (160L)    # GigaAM v2/v3: CTC, RNN-T, E2E variants
│   ├── kaldi.py                 (91L)     # Kaldi Zipformer: stateless RNN-T transducer
│   ├── whisper.py               (227L)    # Whisper: ort-exported (beam search) and HF-optimum (encoder+decoder)
│   ├── tone.py                  (86L)     # T-Tech T-One: streaming CTC with ONNX state
│   ├── silero.py                (111L)    # Silero VAD: sliding window + stateful RNN
│   ├── pyannote.py              (235L)    # PyAnnote VAD: 10s sliding windows, speaker permutation
│   └── wespeaker.py             (53L)     # WeSpeaker: speaker embedding extraction
│
├── preprocessors/                          # Runtime preprocessors (~367 lines)
│   ├── preprocessor.py          (88L)     # OnnxPreprocessor, IdentityPreprocessor, ConcurrentPreprocessor
│   ├── numpy_preprocessor.py    (218L)    # Pure NumPy fallbacks: GigaAM, Kaldi, NeMo, Whisper
│   └── resampler.py             (61L)     # ONNX-based sample rate resampler (8k to 48k)
│
preprocessors/                             # Build-time preprocessor generators (~590 lines)
├── build.py                     (72L)     # Orchestrator: compiles all preprocessors+resamplers, saves fbanks.npz
├── fbanks.py                    (57L)     # Mel filterbank math (HTK/Kaldi/Slaney scales)
├── gigaam.py                    (67L)     # GigaAM preprocessor ONNX DSL (v2, v3)
├── kaldi.py                     (170L)    # Kaldi/Wespeaker preprocessor ONNX DSL
├── nemo.py                      (89L)     # NeMo preprocessor ONNX DSL (80/128 mel bins)
├── whisper.py                   (79L)     # Whisper preprocessor ONNX DSL (80/128 mel bins)
└── resample.py                  (55L)     # Sinc-based resampler ONNX DSL (all sample rate pairs)
│
wrappers/                                  # Reference wrappers for testing/comparison (~415 lines)
├── gigaam.py                    (40L)     # Original GigaAM via transformers
├── nemo.py                      (242L)    # Original NeMo Toolkit + ONNX export helpers
├── sherpa.py                    (71L)     # sherpa-onnx wrapper (Vosk)
├── tone.py                      (30L)     # Original T-One streaming wrapper
└── whisper.py                   (32L)     # Original openai-whisper wrapper
│
tests/                                     # Test suite (~680 lines)
├── conftest.py                  (10L)     # pytest header with dep versions
├── onnx_asr/
│   ├── test_cli.py              (56L)     # CLI arg parsing + run tests
│   ├── test_embedding.py        (40L)     # Speaker embedding tests
│   ├── test_manager.py          (34L)     # Manager config tests (CPU/CUDA/TensorRT)
│   ├── test_read_wav.py         (121L)    # WAV reading tests (PCM, multi-channel, error cases)
│   ├── test_recognize.py        (117L)    # Recognize tests (all models, batch, timestamps)
│   ├── test_resolver.py         (237L)    # Resolver tests (all model names, paths, errors, offline)
│   └── test_vad.py              (38L)     # VAD tests (Silero, PyAnnote)
└── preprocessors/
    ├── test_gigaam.py           (176L)
    ├── test_kaldi.py            (148L)
    ├── test_nemo.py             (97L)
    ├── test_whisper.py          (82L)
    ├── test_build.py            (57L)
    └── test_resample.py         (49L)
`

**Total: ~5,100 source lines** across 55 Python files, plus 3 Jupyter benchmark notebooks.

---

## 4. Feature Inventory

### 4.1 API Surface
| Feature | Implementation | File(s) |
|---------|---------------|---------|
| onnx_asr.load_model() | Top-level load with auto-download from Hugging Face | __init__.py, loader.py:288-347 |
| onnx_asr.load_vad() | VAD model loader (Silero, PyAnnote) | loader.py:350-388 |
| model.recognize() | Single/batch recognition, WAV files or NumPy arrays | dapters.py:111-146 |
| model.with_timestamps() | Returns TimestampedResult (text+tokens+timestamps+logprobs) | dapters.py:162-167 |
| model.with_vad(vad) | Combines ASR+VAD for long audio | dapters.py:69-80 |
| Manager class | Low-level model creation with custom providers/options | loader.py:155-285 |
| CLI | onnx-asr <model> <file.wav> [--vad] [--lang] | cli.py:47-49 |
| Quantization | quantization="int8" / "fp16" / "uint8" | loader.py:300, esolver.py:101 |
| Offline mode | offline=True skips downloads, uses local cache only | loader.py:296 |
| Language selection | language="ru" for multilingual models (Whisper, Canary) | dapters.py:49-55 |
| PNC control | pnc="pnc" for Canary punctuation/capitalization | dapters.py:54-55 |
| Concurrent preprocessing | max_concurrent_workers=N for batch parallelism | preprocessor.py:63-88 |
| Speaker embedding | model.embedding(waveform) returns 256-dim float32 vector | dapters.py:237-280 |

### 4.2 Supported Models (18+ named, unlimited custom)
| Model Name | Architecture | Decoder | Languages |
|-----------|-------------|---------|-----------|
| 
emo-parakeet-tdt-0.6b-v3 | NeMo Parakeet 0.6B | TDT | Multilingual (53) |
| 
emo-parakeet-tdt-0.6b-v2 | NeMo Parakeet 0.6B | TDT | English |
| 
emo-parakeet-ctc-0.6b | NeMo Parakeet 0.6B | CTC | English |
| 
emo-parakeet-rnnt-0.6b | NeMo Parakeet 0.6B | RNN-T | English |
| 
emo-canary-1b-v2 | NeMo Canary 1B | AED/Transformer | Multilingual |
| 
emo-fastconformer-ru-ctc | NeMo FastConformer Hybrid | CTC | Russian |
| 
emo-fastconformer-ru-rnnt | NeMo FastConformer Hybrid | RNN-T | Russian |
| gigaam-v2-ctc / gigaam-v2-rnnt | GigaAM v2 | CTC / RNN-T | Russian |
| gigaam-v3-ctc / gigaam-v3-rnnt | GigaAM v3 | CTC / RNN-T | Russian |
| gigaam-v3-e2e-ctc / gigaam-v3-e2e-rnnt | GigaAM v3 E2E | CTC / RNN-T | Russian |
| lphacep/vosk-model-ru | Kaldi Icefall Zipformer | RNN-T | Russian |
| lphacep/vosk-model-small-ru | Kaldi Icefall Zipformer | RNN-T | Russian |
| 	-tech/t-one | T-Tech T-One | CTC (streaming) | Russian |
| whisper-base | OpenAI Whisper (ort export) | Beam search | Multilingual |
| onnx-community/whisper-* | OpenAI Whisper (optimum) | Greedy | Multilingual |
| Custom (any HF repo) | Config-driven via config.json | Auto-detected | Any |

### 4.3 Decoding Strategies (all greedy except Whisper-ort)
| Decoder | Base class | Files (lines) | Used by |
|---------|-----------|---------------|---------|
| CTC greedy | _AsrWithCtcDecoding | sr.py:160-176 | GigaAM CTC, NeMo CTC, T-One |
| RNN-T greedy | _AsrWithTransducerDecoding | sr.py:179-229 | GigaAM RNN-T, NeMo RNN-T, Kaldi |
| TDT greedy | NemoConformerTdt._decode | 
emo.py:131-138 | NeMo Parakeet TDT |
| AED greedy | NemoConformerAED._decoding | 
emo.py:222-266 | NeMo Canary |
| Beam search | WhisperOrt._decoding | whisper.py:126-143 | Whisper (ort export) |

### 4.4 Preprocessors (Log-Mel Spectrogram)
| Preprocessor | Features size | Backend | Build file | Runtime file (lines) |
|-------------|--------------|---------|-----------|---------------------|
| NeMo | 80 or 128 mel bins | ONNX or NumPy | preprocessors/nemo.py:89 | 
umpy_preprocessor.py:139-182 |
| GigaAM | 64 mel bins | ONNX or NumPy | preprocessors/gigaam.py:67 | 
umpy_preprocessor.py:30-64 |
| Kaldi | 80 mel bins | ONNX or NumPy | preprocessors/kaldi.py:170 | 
umpy_preprocessor.py:67-136 |
| Whisper | 80 or 128 mel bins | ONNX or NumPy | preprocessors/whisper.py:79 | 
umpy_preprocessor.py:185-218 |
| Identity | passthrough | N/A | N/A | preprocessor.py:17-24 (for T-One) |

### 4.5 VAD (Voice Activity Detection)
| Feature | Implementation | File (lines) |
|---------|---------------|-------------|
| Silero VAD | Sliding window (512-hop at 16kHz), stateful RNN | silero.py:39-65 |
| PyAnnote VAD | 10s windows, 5s overlap, speaker permutation resolution | pyannote.py:51-235 |
| Segment merging | Configurable min/max speech duration, silence tolerance, padding | ad.py:55-83 |
| Batch VAD | Processes entire waveform batches with per-waveform segmentation | ad.py:95-121 |

### 4.6 ONNX Runtime Integration
| Feature | Implementation | File (lines) |
|---------|---------------|-------------|
| Provider selection | Auto-detect available providers; manual override | loader.py:168, onnx.py:8-16 |
| TensorRT profiles | Shape profiles for encoder models (batch, waveform_len_ms) | onnx.py:31-73 |
| Provider exclusion | Per-model excluded providers (e.g., Kaldi decoder excludes TensorRT) | kaldi.py:36-37 |
| IO binding | WhisperHf uses io_binding for GPU zero-copy (encoder->decoder) | whisper.py:172-213 |
| Device detection | get_onnx_device() returns (type, id) from session | onnx.py:125-136 |
| Preprocessor offload | Auto-uses NumPy preprocessors when only CPU or CUDA available | loader.py:190-192 |
| Resampler | 8 ONNX models for all sample rate conversions (8k..48k to 8k/16k) | esampler.py:18-61 |

---

## 5. Key Code Patterns & Techniques

### 5.1 Protocol-Based Polymorphism (no ABC inheritance)
The library uses Python Protocol classes extensively instead of ABC inheritance for the core types:

- Asr protocol: sr.py:54-66 — requires ecognize_batch() and _get_sample_rate()
- Vad protocol: ad.py:33-47 — requires ecognize_batch()
- Preprocessor protocol: sr.py:44-51 — requires __call__(waveforms, waveforms_lens)
- SpeakerEmbedding protocol: se.py:9-20 — requires embedding()

This allows structural subtyping — any object with the right methods works regardless of class hierarchy.

### 5.2 Adapter Pattern
Adapters wrap raw Asr/Vad/Se objects with resampling, WAV reading, and convenience methods:

- TextResultsAsrAdapter (dapters.py:162-177): wraps Asr -> returns plain str
- TimestampedResultsAsrAdapter (dapters.py:149-159): wraps Asr -> returns TimestampedResult
- SegmentResultsAsrAdapter (dapters.py:207-234): wraps Asr+Vad -> returns per-segment text
- SeAdapter (dapters.py:237-280): wraps SpeakerEmbedding -> returns embedding vectors
- Each adapter owns a Resampler that auto-converts any supported sample rate to the model's native rate

### 5.3 CTC Greedy Decoding with Deduplication
_AsrWithCtcDecoding._decoding() (sr.py:160-176):
- Argmax over vocab at each time step
- Masked by encoder_out_lens
- Blank token removal + consecutive duplicate deduplication via 
p.diff()
- Logprobs aggregated with 
p.add.reduceat() for efficiency — a NumPy trick worth studying

### 5.4 RNN-T Greedy Decoding Loop
_AsrWithTransducerDecoding._decoding() (sr.py:192-228):
- Step-by-step stateful decoding: encoder frame -> decoder step -> joint -> argmax
- TDT variant (
emo.py:131-138): model output split into vocab logits + step prediction
- TensorRT low-precision workaround: clamp encoder_out_lens to actual tensor shape
- max_tokens_per_step capping prevents runaway decoding loops

### 5.5 ONNX-in-Pure-Python Preprocessors
The preprocessors/ directory contains build-time scripts that generate ONNX models using onnxscript DSL. These are compiled at wheel build time (via hatch_build.py) and shipped as .onnx files inside the package. At runtime, OnnxPreprocessor (preprocessor.py:27-60) loads them from package resources. The NumPy fallbacks in 
umpy_preprocessor.py provide identical logic for CPU-only setups. This is the **single most innovative pattern** in the project — self-contained preprocessing without external libraries.

### 5.6 HuggingFace Model Resolution
Resolver class (esolver.py:43-169):
- Three-tier resolution: known model name -> HF repo ID mapping, custom HF repo (owner/repo), local directory
- Quantization suffix appending (e.g., model.onnx -> model?int8.onnx)
- Falls back from local_files_only=True to full download if files missing
- Config-driven: reads config.json to auto-detect model_type for unknown repos

### 5.7 Kaldi Stateless Transducer Cache
KaldiTransducer._decode() (kaldi.py:78-91):
- Maintains a dict[(context_tokens), decoder_out] cache keyed by context history
- Cache key: last CONTEXT_SIZE=2 tokens
- Avoids redundant decoder forward passes for repeated contexts — elegant memoization pattern

### 5.8 PyAnnote Speaker Permutation Resolution
PyAnnoteVad._decode() (pyannote.py:98-145):
- Model outputs 7 speaker probabilities (no-speech + individual + pairs)
- Overlapping windows cause speaker label permutation across windows
- Resolves permutations by trying all 3! speaker orderings, selecting the one minimizing diff with previous overlap chunk
- Fuses overlapping regions by averaging probabilities — sophisticated multi-speaker VAD without diarization dependency

---

## 6. Relation to S2B2S

### 6.1 Comparison Table

| Aspect | onnx-asr | S2B2S | Verdict |
|--------|----------|-------|---------|
| **Language** | Python | Rust + TypeScript | S2B2S (native desktop) |
| **STT runtime** | ONNX Runtime (Python) | transcribe-rs (Rust ONNX bindings) | Tie (both use ONNX RT) |
| **STT models** | 18+ (Parakeet V2/V3, Canary V2, GigaAM, Whisper, Kaldi, T-One) | 3 (Parakeet V3, Whisper, Moonshine) | onnx-asr (far more) |
| **VAD** | Silero + PyAnnote (ONNX) | TripleVAD (RMS + RNNoise + Silero) in Rust | S2B2S (more stages) |
| **Resampling** | Custom ONNX sinc resampler | rubato (Rust) | Tie |
| **GPU backends** | CUDA, TensorRT, CoreML, DirectML, ROCm, WebGPU | CUDA, DirectML (via transcribe-rs) | onnx-asr (more) |
| **Timestamps** | Token-level timestamps + logprobs | Not exposed | onnx-asr |
| **Batch** | Full batch (padded waveforms) | Single audio | onnx-asr |
| **Speaker embedding** | WeSpeaker ResNet34 | None | onnx-asr |
| **Model download** | Auto from HuggingFace | Custom shell scripts | onnx-asr |
| **Pipeline** | STT only | STT + ITN + Brain (LLM) + TN + TTS | S2B2S (full pipeline) |
| **Streaming** | None (T-One model supports but not wrapped) | Audio streaming + SSE LLM streaming | S2B2S |

### 6.2 What S2B2S Could Learn
- **Parakeet V3 is already shared**: Both use it. S2B2S's transcribe-rs and onnx-asr both support the same ONNX model.
- **Canary V2 would bring multilingual dictation**: S2B2S currently lacks a dedicated multilingual STT model. onnx-asr's Canary V2 AED decoder (266 lines in 
emo.py) shows exactly how to run it.
- **TensorRT could 10x GPU speed**: onnx-asr's TensorRtOptions pattern (onnx.py:31-73) shows how to configure shape profiles for ONNX encoders — S2B2S could pass similar options through transcribe-rs.
- **VAD integration**: S2B2S's VAD is a separate udio_toolkit subsystem; onnx-asr's with_vad() adapter pattern shows a cleaner integration where VAD segmentation and ASR recognition are composed in one call.

---

## 7. Harvest List (Features Worth Copying for S2B2S)

| Feature to harvest | From file | Effort | Why valuable for S2B2S |
|-------------------|-----------|--------|----------------------|
| TDT decoding step prediction | 
emo.py:131-138 | S | S2B2S uses Parakeet V3 TDT but decoding is opaque inside transcribe-rs; understanding this enables custom decoding or optimization |
| Token-level timestamps | sr.py:143-151 | S | Expose timestamps for dictation overlay display in S2B2S frontend |
| Kaldi stateless transducer cache | kaldi.py:78-91 | S | Memoizing decoder outputs by context tokens is a universal RNN-T optimization |
| ONNX preprocessor compilation pipeline | preprocessors/build.py, hatch_build.py | M | S2B2S could compile its own ONNX preprocessors at build time for better portability and no external deps |
| Auto-download from HuggingFace | esolver.py:43-169 | M | Replace shell-script model download in S2B2S's models/download_models.ps1 with in-app auto-download |
| PyAnnote VAD (speaker-aware) | pyannote.py:51-235 | L | Multi-speaker VAD would enable conversation mode with speaker diarization |
| WeSpeaker integration | wespeaker.py, se.py | M | Speaker embedding could enable "who is speaking" awareness for multi-user setups |
| TensorRT provider profiles | onnx.py:31-73 | S | Pass TensorRT shape profiles through transcribe-rs for GPU acceleration |
| Concurrent preprocessor | preprocessor.py:63-88 | S | ThreadPoolExecutor for batch audio preprocessing |
| Adapter pattern (resampling+WAV) | dapters.py:111-146 | M | Clean separation: raw ASR engine vs user-facing API with auto-resampling |

---

## 8. Known Issues, Caveats & Limitations

| Issue | Severity | Impact |
|-------|----------|--------|
| Max audio length 20-30 seconds | Medium | Must use VAD for longer audio; no streaming support (except T-One model, not exposed) |
| Greedy decoding only (most models) | Low | Slightly lower accuracy than beam search, but comparison data shows negligible WER difference |
| HuggingFace dependency for auto-download | Low | Offline mode supported; huggingface-hub is optional |
| onnxruntime 1.24.1 incompatibility | Medium | Known issue with symlinks to data files; upgrade to 1.24.2+ or downgrade |
| Some onnx-community Whisper models have broken fp16 | Low | Documented limitation; use fp32 or int8 versions |
| T-One: no streaming support exposed in API | Low | Model supports streaming but onnx-asr wrapper does not expose it |
| Python requirement for S2B2S integration | High | S2B2S is Rust/Tauri; would need subprocess, PyO3/CPython embedding, or Rust reimplementation |
| cpu_preprocessing argument deprecated | Low | Only a deprecation warning at loader.py:340-344 |
| Canary 180M Flash xfail on onnxruntime 1.18.1 | Low | Missing Trilu ONNX operator; fixed in newer versions |
| VAD requires manual threshold tuning | Medium | Default thresholds may not work for all audio sources |

---

## 9. Strengths & Weaknesses

### Strengths
1. **Exceptional performance**: TensorRT RTFx of 1,500+ (FastConformer CTC on RTX 5070 Ti), 320x for Parakeet V3. Even CPU-only achieves 36x RTFx on a desktop CPU. Benchmark tables cover 4 hardware configurations across 15+ models.
2. **Minimal dependencies**: Only NumPy + onnxruntime as hard requirements. No PyTorch, no transformers, no FFmpeg. The entire wheel is ~2 MB.
3. **Broad model coverage**: 5 architectures (NeMo, GigaAM, Kaldi, Whisper, T-One), 18+ named models, plus custom models via config.json. The only Python ASR library supporting Parakeet TDT, Canary AED, Kaldi stateless transducer, and T-One streaming CTC in one package.
4. **Exceptional engineering quality**: mypy strict, ruff ALL rules, 55 source files with zero TODO/FIXME/dead-code stubs, 237 resolver tests, CI on multiple OS/Python version combinations.
5. **Dual preprocessor backends**: ONNX for GPU offload, NumPy for CPU-only — auto-selected based on available providers. Preprocessors are compiled at build time and shipped inside the wheel.
6. **Built-in VAD**: Two VAD models (Silero + PyAnnote) with configurable segment merging, no external VAD library needed.
7. **Auto-download with offline fallback**: HuggingFace download with local cache; works without internet after first run.
8. **Comprehensive benchmarks**: Published RTFx tables for 4 hardware configs (x64 CPU, Arm CPU, T4 CUDA, RTX 5070 Ti TensorRT) across 15+ models. Also accuracy comparison (CER/WER) against original implementations.

### Weaknesses
1. **Python-only**: Cannot be directly integrated into S2B2S's Rust backend without an IPC bridge or CPython embedding. This is the single biggest barrier.
2. **Greedy decoding only**: No beam search for most models (only Whisper-ort has beam search). The comparison docs show this doesn't hurt WER significantly for most models.
3. **No streaming ASR API**: Only batch/offline recognition. T-One model supports streaming but the wrapper doesn't expose it.
4. **Max 30s audio**: Hard limit for most models without VAD segmentation.
5. **VAD requires manual tuning**: Default thresholds may not work well without adjustment.
6. **Monolingual bias**: Most pre-configured models are Russian or English; multilingual support only through Whisper and Parakeet V3.
7. **Limited speaker diarization**: Only WeSpeaker embedding + PyAnnote speaker count; no full diarization pipeline.
8. **No punctuation restoration**: Except Canary's PNC flag; most models return raw (unpunctuated) text.

---

## 10. Bottom Line / Verdict

onnx-asr is an **exceptionally well-engineered library** that represents the state of the art for ONNX-based ASR in Python. Its single most valuable idea for S2B2S is the **preprocessor compilation pipeline**: building ONNX feature extractors at build time and shipping them in the wheel, then auto-selecting between GPU (ONNX) and CPU (NumPy) backends at runtime. S2B2S could adopt this pattern to eliminate its dependency on external preprocessing and make its STT pipeline more self-contained.

The library's second-most-valuable insight is its **protocol-based architecture** (Asr/Vad/Preprocessor as structural types) which makes it trivially extensible — any model with an ONNX encoder and decoder following the protocol can be plugged in. S2B2S currently hardcodes 3 models via transcribe-rs; adopting this pattern would allow community model contributions.

For direct S2B2S integration, the path is clear but non-trivial: run onnx-asr as a **Python subprocess** (similar to how S2B2S already runs Piper/Kokoro as HTTP servers) or embed CPython with PyO3. The payoff would be access to Canary V2 multilingual ASR, GigaAM for Russian, TensorRT-accelerated Parakeet V3 (up to 10x faster than CUDA-only), and PyAnnote's speaker-aware VAD — all of which surpass S2B2S's current transcribe-rs pipeline in either model quality or throughput.
