# TTS-Audio-Suite -- Research/Reference Analysis (Category D)

> Repo: `diodiogod/TTS-Audio-Suite` · Version: 4.27.3 · License: MIT (code) / varies (model weights) · Author: Diogod (forked from ShmuelRonen's ChatterBox Voice) · Platforms: Python/ComfyUI (Win/Mac/Linux)
> Nature: structural-fork -> independent · 13-engine universal TTS/VC/ASR extension for ComfyUI
> Role for S2B2S: Engine abstraction patterns, streaming architecture, text-pipeline design, TTS backend multiplicity strategy

---

## 1. What TTS-Audio-Suite Is

TTS-Audio-Suite is a comprehensive ComfyUI custom node extension that provides a **universal multi-engine Text-to-Speech, Voice Conversion, and Speech Recognition workstation**. It evolved from the original ChatterBox Voice ComfyUI node into a full audio production suite supporting 13 distinct TTS engines under a single unified architecture.

The project solves the problem of TTS engine fragmentation: instead of installing 13 different ComfyUI extensions for different TTS models, users get one extension with a **single unified node interface** (`🎤 TTS Text`, `📺 TTS SRT`, `🔄 Voice Changer`, `✏️ ASR Transcribe`) that delegates to any engine. Every engine gets character switching, language switching, pause tags, per-segment parameter overrides, SRT subtitle timing, audio caching, and model lifecycle management for free because these are implemented at the orchestration layer, not per-engine.

The target audience is ComfyUI users doing audio/voice production -- audiobook narration, multi-character dialogue, video dubbing, voice cloning, and real-time voice conversion. The extension is version 4.27.3 as of May 2026, maintained by a single developer (Diogod) with 174 dedicated documentation/guide files and a structured onboarding process for LLM-guided new engine additions.

---

## 2. Tech Stack

### 2.1 Frontend (ComfyUI JavaScript Widgets)
| Layer | Choice | Purpose |
|-------|--------|---------|
| UI Framework | ComfyUI widget system (vanilla JS) | Custom node widgets within ComfyUI |
| Audio Analyzer | `web/audio_analyzer_*.js` (15 files) | Canvas-based interactive waveform visualization with region selection, drag, zoom |
| Tag Editor | `web/string_multiline_tag_editor.js` + 8 widget-*.js | Rich text editor with syntax highlighting, preset system, undo/redo |
| Emotion Radar | `web/emotion_radar_*.js` (8 files) | Canvas widget for IndexTTS-2 8-emotion vector visualization |
| Voice Capture | `web/chatterbox_voice_capture.js` | Microphone recording UI with silence detection |
| RVC Training | `web/model_training_dashboard.js`, `web/rvc_*.js` | Live training dashboard with epoch progress |

### 2.2 Backend / Core
| Layer | Choice | Purpose |
|-------|--------|---------|
| Framework | ComfyUI custom node (Python) | Node-based workflow execution |
| ML Runtime | PyTorch 2.x, CUDA | All models run on GPU with CPU fallback |
| Audio I/O | torchaudio + soundfile + librosa | Load/save/resample audio in multiple formats |
| Text Processing | regex, espeak/phonemizer, cmudict | Character/language parsing, phonemization |

### 2.3 Key Dependencies (non-obvious ones)
- **`accelerate`** -- Required by VibeVoice/Higgs/MOSS for multi-GPU and device_map
- **`transformers`** (must be >=4.51.3, <5.0.0 for Qwen3-TTS) -- HuggingFace model loading
- **`HyperPyYAML`** -- Step Audio EditX CosyVoice config parsing with custom !new:/!ref tags
- **`numba`** -- RVC pitch extraction (RMVPE, harvest, dio) and librosa fallback
- **`mediapipe`** -- Mouth movement analysis (WARNING: incompatible with Python 3.13, falls back to OpenSeeFace)
- **`cv2` (OpenCV)** -- Video frame processing for Silent Speech Analyzer
- **`sounddevice` + PortAudio** -- Voice recording with smart silence detection
- **`onnxruntime`** -- RVC HuBERT models and CosyVoice speaker verification (campplus)

---

## 3. Architecture & Source Map

```
TTS-Audio-Suite/
├── __init__.py (564 lines)          # Entry: patches, dependency checks, API routes, node loading
├── nodes.py (848 lines)             # Central node discovery + registration with graceful degradation
├── install.py                       # Smart dependency installer with Python 3.13 fallback
│
├── engines/                         # TTS/VC/ASR ENGINE IMPLEMENTATIONS (the "what")
│   ├── adapters/                    # BRIDGE LAYER: Processor -> Engine
│   │   ├── chatterbox_adapter.py (403 ln)
│   │   ├── chatterbox_streaming_adapter.py (369 ln)   # ABC StreamingEngineAdapter impl
│   │   ├── f5tts_adapter.py (192 ln), f5tts_streaming_adapter.py
│   │   ├── rvc_adapter.py (369 ln), higgs_audio_adapter.py
│   │   ├── vibevoice_adapter.py, index_tts_adapter.py
│   │   ├── step_audio_editx_adapter.py, cosyvoice_adapter.py
│   │   ├── qwen3_tts_adapter.py, echo_tts_adapter.py, moss_tts_adapter.py
│   │   └── asr_qwen3_adapter.py, asr_granite_adapter.py
│   ├── chatterbox/                  # Original ChatterBox TTS + VC (tts.py, vc.py)
│   │   ├── stateless_wrapper.py     # Thread-safe wrapper for streaming
│   │   ├── streaming_model_manager.py (128 ln), overlapping_processor.py
│   ├── chatterbox_official_23lang/  # ResembleAI 23-language official model
│   ├── f5_tts/                      # Forked F5-TTS: train/, infer/, model/, eval/
│   ├── f5tts/                       # Suite-specific F5-TTS wrappers
│   │   ├── f5tts.py, f5tts_edit_engine.py, audio_compositing.py
│   ├── rvc/                         # Real-time Voice Conversion
│   │   ├── rvc_engine.py (566 ln)   # Core: model loading, conversion, caching
│   │   ├── impl/                    # Bundled RVC v2 with UVR5 vocal removal
│   │   └── training/                # RVC training backend
│   ├── index_tts/                   # IndexTTS-2 with emotion control (infer_v2.py)
│   ├── vibevoice_engine/            # Microsoft VibeVoice + KugelAudio
│   ├── step_audio_editx/            # 3B LLM audio editing (tts.py, tokenizer.py)
│   ├── cosyvoice/                   # Alibaba CosyVoice3 (ultra-fast 0.05 RTF)
│   ├── qwen3_tts/ + qwen3_asr/     # Alibaba Qwen3-TTS/ASR
│   ├── higgs_audio/                 # Boson Higgs Audio 2 (CUDA graphs)
│   ├── moss_tts/                    # OpenMOSS (Local/Delay/TTSD + LoRA training)
│   ├── granite_asr/                 # IBM Granite ASR
│   ├── echo_tts/                    # Echo-TTS (DiT-based, CC-BY-NC-SA)
│   └── video/                       # LIP-SYNC / VIDEO ANALYSIS
│       ├── providers/
│       │   ├── abstract_provider.py (334 ln)  # ABC with MAR, segment detection
│       │   ├── mediapipe_provider.py (1291 ln) # Google MediaPipe + viseme analysis
│       │   └── openseeface_provider.py        # OpenSeeFace fallback (Python 3.13)
│       ├── openseeface/             # Bundled tracker, retinaface, model
│       └── utils/preview_creator.py
│
├── nodes/                           # COMFYUI NODE DEFINITIONS (the "how")
│   ├── base/base_node.py            # BaseChatterBoxNode, BaseTTSNode
│   ├── unified/                     # UNIFIED ENTRY POINTS (engine-agnostic)
│   │   ├── tts_text_node.py (1597 ln)     # 🎤 TTS Text
│   │   ├── tts_srt_node.py               # 📺 TTS SRT
│   │   ├── voice_changer_node.py          # 🔄 Voice Changer
│   │   ├── asr_transcribe_node.py         # ✏️ ASR Transcribe
│   │   └── model_training_node.py         # 🎓 Model Training
│   ├── engines/                     # ENGINE CONFIG NODES (one per engine, 14 files)
│   ├── audio/                       # AUDIO TOOLS
│   │   ├── analyzer_node.py (835 ln)      # 🌊 Audio Wave Analyzer
│   │   ├── recorder_node.py               # 🎙️ Voice Capture
│   │   ├── vocal_removal_node.py          # 🤐 Noise/Vocal Removal (UVR5)
│   │   ├── voice_fixer_node.py            # 🤐 Voice Fixer
│   │   ├── merge_audio_node.py            # 🥪 Merge Audio
│   │   └── rvc_pitch_options_node.py
│   ├── video/                       # VIDEO ANALYSIS
│   │   ├── mouth_movement_analyzer_node.py (923 ln)  # 🗣️ Silent Speech Analyzer
│   │   └── viseme_options_node.py
│   ├── subtitles/                   # SUBTITLE PIPELINE
│   │   ├── text_to_srt_builder_node.py     # 📺 Text to SRT Builder
│   │   └── srt_advanced_options_node.py    # 🔧 SRT Advanced Options
│   ├── text/                        # TEXT PROCESSING
│   │   ├── tts_tag_editor_node.py          # 🏷️ Multiline TTS Tag Editor
│   │   ├── phoneme_text_normalizer_node.py # 📝 Phoneme Text Normalizer
│   │   └── asr_punctuation_truecase_node.py
│   ├── training/                    # RVC + MOSS training nodes (7 files)
│   └── shared/                      # Character Voices, Refresh Voice Cache
│
├── utils/                           # UTILITY LAYER (shared infrastructure)
│   ├── models/                      # MODEL MANAGEMENT (the abstraction)
│   │   ├── engine_registry.py (196 ln)         # EngineCapabilities + registry
│   │   ├── unified_model_interface.py (1151+ ln) # Factory-based loading
│   │   ├── factory_config.py, manager.py, language_mapper.py
│   │   ├── fallback_handler.py, exceptions.py, extra_paths.py
│   │   └── comfyui_model_wrapper/              # ComfyUI model lifecycle bridge
│   ├── streaming/                   # UNIVERSAL STREAMING SYSTEM
│   │   ├── streaming_interface.py (250 ln)     # StreamingEngineAdapter ABC
│   │   ├── streaming_types.py (186 ln)         # Segment, Result, Config dataclasses
│   │   ├── streaming_coordinator.py (435 ln)   # Mode decision + data conversion
│   │   └── work_queue_processor.py             # Parallel worker pool
│   ├── audio/                       # Audio utilities: analysis.py (711 ln), processing.py,
│   │   ├── cache.py, chunk_combiner.py, chunk_timing.py, audio_hash.py
│   ├── text/                        # Text parsing: character_parser.py, chunking.py,
│   │   ├── pause_processor.py, segment_parameters.py, phonemizer_utils.py
│   ├── voice/                       # Voice discovery, multilingual engine
│   ├── downloads/                   # unified_downloader.py (HF download)
│   ├── compatibility/               # pytorch_patches.py, transformers_patches.py
│   ├── asr/                         # ASR types, pipeline, srt_builder
│   └── device/                      # torch_device_resolver.py
│
├── web/                             # JAVASCRIPT FRONTEND WIDGETS (48 files)
│   ├── audio_analyzer_*.js (15 files)         # 🌊 Audio Wave Analyzer widget
│   ├── string_multiline_tag_editor*.js (9 files) # 🏷️ Tag editor widget
│   ├── emotion_radar_*.js (8 files)           # 🌈 Emotion radar widget
│   └── rvc_*.js, index_tts_*.js, qwen3_tts_*.js, etc.
│
├── docs/ (70+ files)                # DOCUMENTATION
│   ├── Dev reports/ (40+ files: impl plans, reviews, refactoring summaries)
│   ├── RVC/ (5 analysis docs)
│   ├── New Engines Guides/ (8-step LLM-guided onboarding)
│   └── Upstream_Engine_Tracking/
├── example_workflows/ (17 JSON workflow files)
├── tests/                           # pytest + workflow fixtures
├── scripts/                         # version bump, doc generation, import fixers
├── data/dictionaries/               # moby_words.txt, common_words.txt, cmudict.txt
└── voices_examples/                 # Reference voice audio samples
```

### Architecture Layer Diagram

```
┌─────────────────────────────────────────────────────────┐
│                  UNIFIED NODE LAYER                       │
│  🎤 TTS Text  │  📺 TTS SRT  │  🔄 Voice Changer  │  ✏️ ASR  │
└───────────────────────┬─────────────────────────────────┘
                        │ delegates to
┌───────────────────────▼─────────────────────────────────┐
│               PROCESSOR LAYER (per engine)                │
│  Orchestration: chunking, character/pause tag parsing,   │
│  parameter switching, caching, SRT timing, interrupt    │
└───────────────────────┬─────────────────────────────────┘
                        │ bridges via
┌───────────────────────▼─────────────────────────────────┐
│               ADAPTER LAYER (per engine)                  │
│  Standardized: get_model_for_language(),                 │
│  generate_segment_audio(), convert_voice()              │
└───────────────────────┬─────────────────────────────────┘
                        │ loads via
┌───────────────────────▼─────────────────────────────────┐
│         UNIFIED MODEL INTERFACE (factory pattern)         │
│  EngineRegistry -> ModelLoadConfig -> Factory ->         │
│  ComfyUI ModelManager (lazy loading, device tracking,    │
│  cache, variant exclusion)                              │
└───────────────────────┬─────────────────────────────────┘
                        │ runs on
┌───────────────────────▼─────────────────────────────────┐
│         ENGINE IMPLEMENTATIONS (13 engines)              │
│ ChatterBox │ F5-TTS │ RVC │ VibeVoice │ IndexTTS-2 │ ...│
└─────────────────────────────────────────────────────────┘
```

---

## 4. Feature Inventory

### 4.1 TTS Engine Abstraction (THE CORE ARCHITECTURE)

**Unified Model Interface** (`utils/models/unified_model_interface.py`, 1151+ lines): Central abstraction all 13 engines load through. Factory-registration pattern:
1. `EngineCapabilities` dataclass (`engine_registry.py`, 196 lines) declares per-engine metadata: supports_voice_conversion, multilingual_model_switching, can_corrupt_on_reload, supports_training.
2. `UnifiedModelInterface` class registers factory_func callbacks per engine/model_type via register_model_factory().
3. load_model(config) generates cache key from engine_name, model_type, model_name, device, language, path, additional_params. Handles model variant mutual exclusion (Qwen3, MOSS).
4. Factory functions handle local-first then HuggingFace download fallback. Models wrap with .to(device) and .is_dead().

**Engine Registry** (`utils/models/engine_registry.py`):
```python
ENGINE_REGISTRY = {
    "chatterbox": EngineCapabilities(supports_voice_conversion=True, multilingual_model_switching=True),
    "chatterbox_official_23lang": EngineCapabilities(supports_voice_conversion=True),
    "f5tts": EngineCapabilities(multilingual_model_switching=True),
    "higgs_audio": EngineCapabilities(can_corrupt_on_reload=True, requires_special_init=True),
    "rvc": EngineCapabilities(supports_voice_conversion=True, supports_training=True, training_modes=["voice_model"]),
    "moss_tts": EngineCapabilities(supports_training=True, training_modes=["lora_adapter"]),
    ...
}
```

**Adapter Layer** (`engines/adapters/`): 17 adapter files, each implementing consistent interface: get_model_for_language(), load_base_model(), generate_segment_audio(), convert_voice(). Bridges processor (orchestration) to engine (inference).

**Streaming Adapter** (`utils/streaming/streaming_interface.py`, 250 lines): ABC StreamingEngineAdapter with process_segment(), load_model_for_language(), group_segments_by_language(), group_segments_by_character(). Two implementations: ChatterBoxStreamingAdapter, F5TTSStreamingAdapter.

### 4.2 TTS Engines (13 engines, quick reference)

| Engine | Key File(s) | Model Size | Unique Strength |
|--------|-------------|------------|-----------------|
| ChatterBox | engines/chatterbox/tts.py, vc.py | ~4.3GB | Per-language model switching, 11 community languages |
| ChatterBox 23L | engines/chatterbox_official_23lang/ | ~4.3GB | Single model, 23 languages, zero-shot cloning |
| F5-TTS | engines/f5tts/f5tts.py | ~1.2GB/model | Flow-matching, word/speech editing, socket server |
| Higgs Audio 2 | engines/higgs_audio/ | ~9GB | CUDA graphs (55+ tokens/sec), stateless wrapper |
| VibeVoice | engines/vibevoice_engine/ | 5.4-18GB | 90-min long-form, native 4-speaker, auto-detect language |
| IndexTTS-2 | engines/index_tts/ | ~4.7GB | 8-dimension emotion vectors, QwenEmotion text analysis |
| Step Audio EditX | engines/step_audio_editx/ | ~7GB | 14 emotions, 32 styles, paralinguistic effects, LLM-based |
| CosyVoice3 | engines/cosyvoice/ | ~5.4GB | Ultra-fast 0.05 RTF, instruct mode, native paralinguistic tags |
| Qwen3-TTS | engines/qwen3_tts/ | ~3-6GB | 4 model types, text-to-voice design, torch.compile 1.7x |
| MOSS-TTS | engines/moss_tts/ | ~14GB | 1.7B/8B/Dialogue variants, native multi-speaker, LoRA training |
| Echo-TTS | engines/echo_tts/ | ~7.1GB | DiT-based diffusion, Force Speaker KV (CC-BY-NC-SA) |
| RVC | engines/rvc/rvc_engine.py (566 ln) | 100-300MB/model | Real-time VC, 8 pitch methods, integrated training |
| Granite ASR | engines/granite_asr/ | ~4.6GB | IBM Granite ASR, Qwen forced aligner for timestamps |

### 4.3 Voice Cloning & Conversion (RVC)

**RVC Engine** (engines/rvc/rvc_engine.py, 566 lines): Bundles full RVC v2 inference pipeline:
- Model loading via Unified Model Interface with direct get_vc() fallback
- HuBERT feature extraction (content-vec-best.safetensors)
- convert_voice(): int16->float32 normalization, pitch param merging, calls vc_single()
- 8 pitch extraction: rmvpe, rmvpe+, mangio-crepe, crepe, pm, harvest, dio, fcpe
- MD5-based caching with .npy files
- Device-aware reloading after ComfyUI "Clear VRAM"
- RVCModelWrapper: wraps model dict with .to(device) for ComfyUI lifecycle

**RVC Training Pipeline** (engines/rvc/training/, nodes/training/):
1. RVC Dataset Prep -> path/zip upload, audio slicing, HuBERT features, F0 extraction
2. RVC Training Config -> practical controls with tooltips
3. Model Training -> unified entry, routes by engine type
4. Live dashboard: epoch progress, ETA, speed, loss trend
5. Resume + continue_from warm-start, safe interrupt handling

### 4.4 Video Lip-Sync (Silent Speech Analyzer)

**Mouth Movement Analyzer** (nodes/video/mouth_movement_analyzer_node.py, 923 lines):
- Multi-provider: AbstractProvider ABC (engines/video/providers/abstract_provider.py, 334 lines) with analyze_video(), detect_movement(), calculate_mar(), filter_segments(), frames_to_segments()
- MediaPipe provider (1291 lines): 468-landmark face mesh, A/E/I/O/U viseme classification, consonant detection, CMU dictionary word prediction from viseme sequences
- OpenSeeFace provider: C++ tracker, Python 3.13 compatible fallback
- 4 output formats: SRT, JSON, CSV, AUDIO_REGIONS
- 5 SRT placeholder formats: Words, Syllables, Characters, Underscores, Duration+Length
- Annotated preview with green/red overlays
- Smart caching with parameter-aware re-filtering

### 4.5 Audio Analysis (Wave Analyzer)

**Audio Wave Analyzer** (nodes/audio/analyzer_node.py, 835 lines + utils/audio/analysis.py, 711 lines):
- AudioAnalyzer class: torchaudio load + librosa fallback, resample to target rate, mono conversion
- 3 analysis methods: silence (RMS threshold), energy (volume derivative), peaks (RMS envelope)
- TimingRegion dataclass: start_time, end_time, label, confidence, metadata
- Region grouping by gap threshold, overlapping/adjacent handling
- Export: JSON, CSV, F5-TTS format
- LRU cache (max 50 items) with MD5 keys
- Interactive JS widget (15 files): Canvas waveform with zoom, pan, region selection, multi-select, loop markers, keyboard shortcuts, bidirectional sync

### 4.6 Streaming Architecture

**Streaming System** (utils/streaming/):
- StreamingSegment: universal data (index, text, character, language, voice_path, metadata)
- StreamingResult: audio tensor + duration + processing_time + worker_id + success/error
- StreamingConfig: batch_size, model_preloading, fallback, timeout (300s), max_workers (12)
- StreamingCoordinator (435 lines): should_use_streaming() when batch_size > 1 AND segments >= threshold. Converts node data -> universal format -> processes (parallel or sequential) -> reassembles results
- HONEST NOTE: Parallel streaming provides only ~10-15% improvement due to GPU serialization. Sequential (batch_size=0) remains optimal. Architecture for future CPU-bound engines.

### 4.7 Text Processing Pipeline

- Character Parser (utils/text/character_parser.py): [CharacterName], [language:CharacterName]
- Language Switching (utils/voice/multilingual_engine.py): Routes [de:Alice], [fr:Bob], aliases
- Pause Tags (utils/text/pause_processor.py): [pause:1.5s], [wait:500ms] -> silent audio insertion
- Per-Segment Parameters (utils/text/segment_parameters.py): [Alice|seed:42|temp:0.5]
- Inline Edit Tags (utils/text/step_audio_editx_special_tags.py): <Laughter:2|emotion:happy>
- Smart Chunking (utils/text/chunking.py): sentence-boundary-aware, comma-fallback, char limits
- Phoneme Normalizer: IPA via espeak, Unicode decomposition, ASCII fallback

### 4.8 Model Training

Unified Model Training (nodes/unified/model_training_node.py): Single node, routes by TTS_ENGINE:
- RVC: dataset prep -> config -> train -> load, HuBERT+F0, resumable checkpoints
- MOSS LoRA: 8B Delay model, 4-bit option, manifest building, clip staging
- Progress tracking via /api/tts-audio-suite/training-progress

### 4.9 Subtitle System

Modular ASR+SRT Pipeline:
- ASR Transcribe -> ASR Punctuation/Truecase -> Text to SRT Builder
- SRT Builder works with ASR timing data OR estimates timings from plain text
- Preserves project control tags through subtitle heuristics
- SRT Advanced Options for readability/segmentation policy

---

## 5. Key Code Patterns & Techniques

### 5.1 Engine Abstraction Pattern (THE KEY PATTERN)

**6-layer architecture** that S2B2S could adopt:
```
Engine Node -> Unified Node -> Processor -> Adapter -> UnifiedModelInterface -> Engine Impl
```
Each layer owns exactly one concern. The orchestration layer (character switching, pause tags, caching, SRT timing) is engine-agnostic -- new engines get it for free.

### 5.2 Factory-Registration Pattern (unified_model_interface.py, 1151+ ln)
```python
# Registration (once at init)
unified_model_interface.register_model_factory("chatterbox", "tts", chatterbox_tts_factory)
# Usage (anywhere)
config = ModelLoadConfig(engine_name="chatterbox", model_type="tts", ...)
model = unified_model_interface.load_model(config)
```
The factory receives full ModelLoadConfig. Cache key = engine_name + model_type + model_name + device + language + additional_params. Variant exclusion for mutually exclusive models.

### 5.3 Declarative Engine Capabilities (engine_registry.py, 196 ln)
Rather than hardcoding `if engine_name == "higgs_audio": handle_corruption()`, capabilities are declared once and queried: `caps = get_engine_capabilities(name); if caps.can_corrupt_on_reload: caps.recovery_handler()`.

### 5.4 Graceful Degradation Node Loading (nodes.py, 848 ln)
Every node loaded in try/except with *_AVAILABLE flag. Missing dependencies don't crash the extension. Startup prints only successfully loaded nodes.

### 5.5 Streaming Coordinator Pattern (streaming_coordinator.py, 435 ln)
Pure static utility: decides streaming vs traditional, converts any node data to universal StreamingSegment format, processes via parallel or sequential, converts back to node-specific format. Knows nothing about specific engines.

### 5.6 Multimodal Mouth Movement Analysis
AbstractProvider ABC with two concrete implementations (MediaPipe, OpenSeeFace). Shared: filter_segments(), frames_to_segments(), annotate_frame(). Per-provider: calculate_mar(), detect_movement().

### 5.7 Caching Patterns
- UnifiedModelInterface: ComfyUI-native model caching with device tracking and dead-model detection
- Audio cache: content-hash-based segment-level cache (text+character+voice+params)
- RVC cache: MD5(audio + model_id + pitch_params) -> .npy
- Analysis cache: MD5(video_path + provider + sensitivity) with separate filtered-result cache
- Mouth movement cache: combined analysis + filtered-result keys

### 5.8 Custom ComfyUI JavaScript Widgets (48 files)
- Audio Analyzer (15 files): Canvas waveform, zoom/pan, region management, keyboard shortcuts
- Tag Editor (9 files): Syntax highlighting, dropdowns, presets, undo/redo, SRT-aware editing
- Emotion Radar (8 files): Canvas radar chart for 8-D emotion vectors

### 5.9 Compatibility Patching
- pytorch_patches.py: Monkey-patches torchaudio.load() for PyTorch 2.9 on Windows
- transformers_patches.py: Patches Step Audio EditX tokenizer for transformers 4.54+
- numba_compat.py: Detects broken JIT, sets NUMBA_DISABLE_JIT=1
- All applied lazily to avoid startup cost (~1.3s for transformers import)

---

## 6. Relation to S2B2S

| Aspect | TTS-Audio-Suite | S2B2S | Verdict |
|--------|-----------------|-------|---------|
| Engine count | 13 TTS + 2 ASR + 1 VC | 9 TTS backends | TAS more diverse (VC, ASR, lip-sync) |
| Engine abstraction | 6-layer with declarative caps | TtsBackend trait + match arms | TAS pattern scales better for >5 engines |
| Model lifecycle | Factory-registered, ComfyUI-integrated | WarmEngine trait (Loading->WarmingUp->Ready) | TAS handles variant exclusion |
| Text pipeline | 5-stage: char parse->lang route->pause inject->param override->chunk | 4-stage: markdown strip->TN->paginate->synth | TAS richer controls |
| Streaming | ABC StreamingEngineAdapter + Coordinator | Streaming gapless playback (rodio) | Different focus: TAS=gen parallelism; S2B2S=playback |
| Voice cloning | RVC + ChatterBox iterative + per-engine zero-shot | Pocket voice cloning server | TAS offers 3 VC technologies |
| Lip-sync | Silent Speech Analyzer (video->SRT) | None | TAS unique capability |
| Audio analysis | Interactive waveform + silence/energy/peak | Basic processing only | TAS production-grade tooling |
| Training | RVC + MOSS LoRA in-node | None | TAS allows custom voice creation |
| Subtitle/SRT | Full ASR->SRT builder->TTS pipeline | None | TAS complete subtitle workflow |
| Plugin model | ComfyUI custom node (tightly coupled) | Tauri desktop app (standalone) | Different deployment models |
| Frontend | 48 JS widget files for ComfyUI | React 19 + Vite 8 + Tailwind 4 | S2B2S has modern frontend |

---

## 7. Harvest List (Features Worth Copying)

| Feature | From File | Effort | Value for S2B2S |
|---------|-----------|--------|-----------------|
| Declarative engine capabilities | engine_registry.py (196 ln) | XS | Replace per-backend match arms with registry queries |
| Factory-registration model loading | unified_model_interface.py | S | Centralize model loading across Piper/Kokoro/Kitten/Pocket |
| 6-layer engine abstraction | PROJECT_INDEX.md architecture | L | Separate config/orchestration/translation/loading/inference concerns |
| Silent Speech Analyzer (video->SRT) | engines/video/ + nodes/video/ | XL | Feature: mouth movement timing for video dubbing |
| Audio Wave Analyzer widget | web/audio_analyzer_*.js + utils/audio/analysis.py | L | Interactive waveform review for spoken audio |
| Per-segment parameter switching | utils/text/segment_parameters.py | XS | [Voice|seed:42|temp:0.5] inline syntax |
| Content-hash audio caching | utils/audio/audio_hash.py + cache.py | S | Hash-based TTS output cache for repeated reads |
| Graceful node degradation | nodes.py try/except pattern | XS | Wrap each backend init; app starts even with missing deps |
| New backend onboarding guide | docs/New Engines Guides/ | XS | Create S2B2S "How to Add TTS Backend" checklist |

---

## 8. Known Issues, Caveats & Limitations

| Issue | Severity | Impact |
|-------|----------|--------|
| Python 3.13 incompatibility with MediaPipe | High | Falls back to lower-quality OpenSeeFace for lip-sync |
| Streaming parallelism ineffective (~10-15%) | Low | Sequential mode remains optimal; architecture for future |
| ChatterBox v2 emotion tokens experimental | Low | whisper/laughter tokens produce minimal effects |
| Step Audio EditX language limited (6 langs) | Medium | Other languages distort audio |
| Higgs Audio CUDA graph corruption | Medium | can_corrupt_on_reload=True, needs recovery handler |
| Echo-TTS CC-BY-NC-SA license | Medium | No commercial use |
| F5-TTS CC-BY-NC-4.0 license | Medium | No commercial use |
| Massive total model size (~100GB) | Medium | Per-user download only what they use |
| ComfyUI coupling | High | Architecture not extractable as standalone library |
| Single-maintainer bus factor | Medium | 13 engine integrations, 48 JS files, 70+ docs |
| No real-time streaming TTS | Low | Batch generation only, no incremental output |
| Startup import cost | Low | Deferred imports help; first engine use still ~1.3s |

---

## 9. Strengths & Weaknesses

### Strengths
1. **Best-in-class engine abstraction**: 6-layer architecture gives new engines character switching, language switching, pause tags, per-segment params, SRT timing, and caching for free.
2. **13 engines behind 2 unified nodes**: All TTS, VC, and ASR capabilities accessible through 🎤 TTS Text and 📺 TTS SRT.
3. **Declarative capabilities**: EngineCapabilities + ENGINE_REGISTRY replaces hardcoded engine-specific logic.
4. **Production-quality tooling**: Audio Wave Analyzer, Silent Speech Analyzer, Multiline Tag Editor, Emotion Radar -- genuine production tools, not demo features.
5. **LLM-guided extensibility**: 8-step "New Engines Guides" is the most thorough engine-onboarding docs in any TTS project.
6. **Graceful degradation**: Every node loads in try/except; missing deps don't crash the extension.
7. **Rich text pipeline**: Four distinct composable tag systems (character, language, pause, parameter).
8. **Integrated training**: RVC voice model + MOSS LoRA training in-workflow.
9. **Comprehensive multi-layer caching**: Content-hash audio, ComfyUI model, analysis re-filter, RVC conversion.

### Weaknesses
1. **Tight coupling to ComfyUI**: Cannot be extracted for standalone use; complete reimplementation needed for S2B2S.
2. **Python-only**: No Rust/WASM/cross-platform binary. Dependent on Python + PyTorch + CUDA.
3. **GPU-heavy**: Most engines realistically require CUDA; CPU "very slow for real workloads."
4. **Streaming is aspirational**: Negligible speedup; architecture for future CPU-bound engines only.
5. **Massive codebase**: ~200+ Python files, ~48 JS files, 70+ docs, single maintainer.
6. **Vendored engine forks**: Each engine directory is a fork requiring manual upstream merge.
7. **Experimental features abound**: Multiple features flagged as experimental with degraded quality.
8. **No real-time streaming TTS**: Batch-only; no incremental output for low-latency use cases.
9. **Complex model licensing**: Mix of MIT, CC-BY-NC, CC-BY-NC-SA, Apache-2.0, custom -- commercial audit needed.

---

## 10. Bottom Line / Verdict

TTS-Audio-Suite is the most architecturally sophisticated open-source TTS engine aggregator available. Its 6-layer abstraction with declarative engine capabilities, factory-registered model loading, and universal text pipeline is a **masterclass in how to manage 13 disparate TTS/VC/ASR engines behind two unified UI nodes**. The ComfyUI coupling makes direct code reuse impossible for S2B2S, but the architectural patterns are directly transferable.

The single most valuable idea for S2B2S is the **declarative engine registry + factory-registration pattern** (`engine_registry.py` + `unified_model_interface.py`). S2B2S currently uses a `TtsBackend` trait with per-backend match arms in its manager. Adopting TAS's approach would make adding new backends a matter of declaring capabilities and registering a factory -- no manager code changes needed. The 6-layer separation (config -> orchestration -> translation -> loading -> inference) would also cleanly separate concerns currently mixed in S2B2S's TTS manager and individual backends.

For S2B2S's future roadmap, the Silent Speech Analyzer (video -> mouth movement -> SRT timing) and Audio Wave Analyzer (interactive waveform with region selection) represent compelling features that align with S2B2S's "spoken audio workstation" vision, though they would require significant porting effort from ComfyUI to Tauri+React. The New Engines Guides documentation set is also worth studying as a template for S2B2S's own backend onboarding process.
