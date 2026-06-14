# vibevoice-rs — Category D (Research/Reference)

> Repo: `danielclough/vibevoice-rs` · HEAD: not in git · License: MIT · Author: Daniel Clough · Platforms: macOS (Metal), Linux (CUDA), Windows (CUDA)
> Nature: independent · Rust reimplementation of VibeVoice (Microsoft Research) TTS
> Role for S2B2S: Reference architecture for a pure-Rust TTS cloud, split-LLM streaming model, monorepo with Leptos+Tauri frontends, and PyTorch numerical parity techniques

---

## 1. What vibevoice-rs Is

vibevoice-rs is a pure-Rust reimplementation of Microsoft Research''s VibeVoice text-to-speech system, using the Candle ML framework instead of PyTorch. It provides high-quality voice cloning TTS with three model sizes (0.5B realtime, 1.5B batch, 7B batch) and multi-speaker dialogue synthesis.

The project is organized as a Cargo workspace with five crates: a core library (`vibevoice`), a CLI (`vibevoice-cli`), an Axum HTTP server with SSE streaming (`vibevoice-server`), a Leptos CSR web frontend (`vibevoice-web`), and a Tauri v2 desktop app (`vibevoice-tauri`). This makes it a complete end-to-end reference for building a TTS product in Rust.

The problem it solves: bringing a state-of-the-art neural TTS model (based on Qwen2.5 with diffusion head and VAE acoustic tokenizer) to Rust for deployment flexibility — from command line to HTTP service to embedded desktop app — without Python runtime dependencies.

---

## 2. Tech Stack

### 2.1 Frontend (Web + Desktop)

| Layer | Choice | Purpose |
|-------|--------|---------|
| Web framework | Leptos 0.8 (CSR mode) | Reactive Rust WASM frontend |
| Bundler | Trunk | WASM build tooling |
| Desktop framework | Tauri v2 | Native shell for the web frontend |
| State persistence | LocalStorage (gloo-storage) | Browser-side settings and history |
| Audio playback | Web Audio API (AudioContext) | Client-side audio decoding/playback |
| SSE | Fetch + ReadableStream | Streaming audio from server |

### 2.2 Backend / Core

| Layer | Choice | Purpose |
|-------|--------|---------|
| ML framework | Candle (custom fork: `feat/forward_from_embeds_no_norm`) | Neural network inference |
| LLM backbone | Qwen2.5 (1.5B/7B/0.5B params) | Transformer text-to-speech token generation |
| Diffusion | DPM-Solver++ (2nd order) | Audio latent generation |
| VAE | Custom encoder/decoder with depthwise conv | Acoustic token compression/decompression |
| Text processing | tokenizers (HuggingFace) | Tokenization of input scripts |
| Model hub | candle-hf-hub | Downloading models from HuggingFace |
| Audio I/O | hound, rubato | WAV read/write and sample-rate conversion |
| HTTP server | Axum 0.8 + Tokio | REST API + SSE streaming |
| CLI | Clap 4.5 | Argument parsing |
| RNG | rand_mt (Mersenne Twister 32-bit) | PyTorch-compatible random numbers |

### 2.3 Key Dependencies (non-obvious ones)

- **Custom Candle fork** (`danielclough/candle`, branch `feat/forward_from_embeds_no_norm`): Required because the lower language_model uses `nn.Identity()` as final norm — Candle''s default `forward_from_embeds` always applies the final RMS norm, corrupting the hidden states. The fork adds `forward_from_embeds_no_norm()` that skips the final norm.
- **rand_mt** for PyTorch RNG parity: Critical for bit-identical output with Python reference. Uses MT19937 (32-bit) not MT19937-64, plus a full Box-Muller implementation with double-precision uniforms, scalar/vectorized path selection, and a single-value cache — all exact matches to PyTorch''s `torch.randn()`.
- **ndarray + ndarray-npy**: Used for debugging checkpoint compatibility with Python''s `.npz` export format.
- **serde_yaml**: Shared config format between server and Tauri desktop app.

---

## 3. Architecture & Source Map

```
vibevoice-rs/                          (Cargo workspace, 5 crates, ~9,200+ lines Rust)
│
├── vibevoice/                         (Core library ~5,800 lines)
│   └── src/
│       ├── lib.rs                     ([77 L] Public API: VibeVoice, AudioData, Device, ModelVariant)
│       ├── facade.rs                  ([694 L] Builder pattern, synthesize(), synthesize_script(),
│       │                               batch/realtime dispatch, voice file validation)
│       ├── model.rs                   ([1703 L] VibeVoiceModel: Qwen2 LLM + diffusion + VAE,
│       │                               autoregressive generation, DPM solver, token sampling)
│       ├── config.rs                  ([373 L] Nested deserialization: VibeVoiceConfig, LLMConfig,
│       │                               DiffusionHeadConfig, AcousticTokenizerConfig, VAEDecoderConfig)
│       ├── processor.rs              ([618 L] VibeVoiceProcessor: tokenizer setup, voice prompt
│       │                               construction, text-to-token + voice-to-mask integration)
│       ├── audio.rs                   ([207 L] AudioData: WAV I/O, PCM encoding, streaming header,
│       │                               concat, tensor conversion)
│       ├── error.rs                   ([58 L] thiserror enum: Device, Init, Download, Voice,
│       │                               Audio, Processing, Generation, Io, Unsupported, Config)
│       ├── diffusion.rs              ([284 L] DiffusionHead: SwiGLU, AdaLN modulation, final layer,
│       │                               DPM-Solver++ skeleton; sigma schedule in model.rs)
│       ├── speech_connector.rs       ([69 L] SpeechConnector: fc1->norm->fc2 (128->hidden_size))
│       ├── acoustic_connector.rs     ([66 L] AcousticConnector: fc1->norm->fc2 (64->hidden_size))
│       ├── semantic_tokenizer.rs     ([61 L] SemanticTokenizer: VAE encoder, streaming cache)
│       ├── pytorch_rng.rs            ([608 L] MT19937 + Box-Muller, scalar/vectorized paths,
│       │                               global CPU RNG with save/restore, seed management)
│       ├── utils.rs                   ([786 L] Model download, BF16->F32 conversion, tensor name
│       │                               remapping, voice resolution, audio resampling (rubato),
│       │                               dB normalization, tensor_stats, file logging)
│       ├── voice_mapper.rs           ([215 L] VoiceMapper: directory scanning, name extraction,
│       │                               fuzzy matching, speaker-to-voice assignment, script parser)
│       ├── voice_converter.rs        ([238 L] PyTorch .pt -> .safetensors voice cache conversion)
│       ├── streaming_cache.rs        ([75 L] HashMap-based conv state cache for VAE streaming)
│       ├── test_helpers.rs           ([457 L] NPZ checkpoint I/O, ndarray<->tensor conversion,
│       │                               ToTensor/ToNdarray traits, comparison/verification)
│       │
│       ├── vae_encoder.rs            ([313 L] Convolutional encoder: stem + downsample + stages)
│       ├── vae_decoder.rs            ([379 L] Convolutional decoder: stages + upsample + head)
│       ├── vae_layers.rs             ([731 L] SConv1d, SConvTranspose1d, Block1D, ConvRMSNorm,
│       │                               depthwise conv, pad1d with small-input handling)
│       └── vae_utils.rs              (VAE stage builder, depth parsing, channel computation)
│       └── realtime/
│           ├── mod.rs                 ([42 L] Module docs: dual LLM + CFG architecture ASCII art)
│           ├── model.rs              ([707 L] VibeVoiceRealtimeModel: from_pretrained, generate(),
│           │                           sample_diffusion, VAE decode with cache, windowed main loop)
│           ├── config.rs             ([277 L] RealtimeConfig: split LLM (4 lower + 20 upper),
│           │                           TTS_TEXT_WINDOW_SIZE=5, TTS_SPEECH_WINDOW_SIZE=6, 24kHz)
│           ├── generation.rs         ([447 L] WindowedGenerator: process_text_window, get_condition,
│           │                           update_after_speech_token, EOS check, text_windows())
│           ├── split_llm.rs          ([584 L] DualSplitLLM: 4 Qwen2Model instances (pos/neg x
│           │                           lm/tts), weight sharing, KV cache management, splice_hidden_states)
│           ├── binary_classifier.rs  ([235 L] BinaryClassifier: fc1->ReLU->fc2, sigmoid EOS gate)
│           └── voice_cache.rs        ([307 L] SafetensorCache: 4-entry (pos_lm, pos_tts, neg_lm,
│                                       neg_tts), each with hidden_state + KV pairs, validation)
│
├── vibevoice-cli/                     (CLI binary ~225 lines)
│   └── src/
│       └── main.rs                    ([225 L] Clap parser, model/build, convert-voice subcommand,
│                                       synthesize with progress callback, multi-speaker dispatch)
│
├── vibevoice-server/                  (HTTP server ~843 lines)
│   └── src/
│       ├── lib.rs                     ([732 L] Config (YAML), Args (Clap), WorkerRequest/Response
│       │                               enums, spawn_worker_thread with HashMap model cache,
│       │                               handlers: /health, /voices, /synthesize, /synthesize/json,
│       │                               /synthesize/stream (SSE), router(), voice resolution)
│       └── main.rs                    ([111 L] tokio::main, config loading, worker spawn, CORS setup)
│
├── vibevoice-web/                     (Leptos CSR frontend ~1,400 lines)
│   └── src/
│       ├── main.rs                    ([14 L] Leptos mount_to_body)
│       ├── app.rs                     ([578 L] App component: server setup wizard, model/voice
│       │                               selectors, text input, streaming toggle, synthesis dispatch,
│       │                               history, templates, Tauri integration, toasts)
│       ├── api/
│       │   ├── mod.rs                 (Module declarations)
│       │   └── client.rs             (fetch_voices, synthesize_json HTTP helpers)
│       ├── sse/
│       │   ├── mod.rs                 (Module declarations)
│       │   └── stream.rs             ([274 L] SSE streaming: Fetch+ReadableStream, event parsing,
│       │                               StreamingState with WAV header/PCM chunk accumulation)
│       ├── components/                (16 Leptos components)
│       │   ├── audio_player.rs, audio_history.rs, batch_processor.rs
│       │   ├── model_selector.rs, voice_selector.rs, voice_preview.rs
│       │   ├── synth_button.rs, progress.rs, text_input.rs, text_templates.rs
│       │   ├── server_setup.rs, settings.rs, sidebar.rs, modal.rs, toast.rs
│       ├── storage/
│       │   ├── mod.rs, history.rs, templates.rs  (LocalStorage persistence)
│       └── tauri.rs                   ([236 L] Tauri IPC bridge: invoke, config fetching,
│                                       directory picker, embedded server start/stop)
│
├── vibevoice-tauri/                   (Tauri desktop app ~788 lines)
│   └── src/
│       ├── main.rs                    ([296 L] AppState, 7 Tauri commands (get_server_url,
│       │                               start/stop_embedded_server, save_config, pick_directory),
│       │                               setup with tray, hotkeys, graceful shutdown on exit)
│       ├── lib.rs                     ([6 L] Module re-exports)
│       ├── config.rs                 ([137 L] Platform config paths (ProjectDirs), YAML load/save,
│       │                               TOML->YAML migration from old config format)
│       ├── server.rs                 ([155 L] EmbeddedServer: spawns worker+tokio threads,
│       │                               graceful shutdown via oneshot, ready signal, Drop cleanup)
│       ├── tray.rs                    ([72 L] System tray: Show/Hide/Quit menu, left-click toggle)
│       └── hotkeys.rs                ([140 L] Global shortcut parser (string->Shortcut), registration)
│
├── examples/                          (5 example binaries)
│   ├── simple_tts.rs                 ([104 L] Basic synthesis with model selection)
│   ├── voice_cloning.rs              (Voice cloning from WAV samples)
│   ├── streaming.rs                  ([104 L] Realtime model with progress callback + RTF measurement)
│   ├── multi_speaker.rs             ([154 L] Script-based multi-speaker dialogue)
│   └── batch_processing.rs           (Batch synthesis from list)
│
├── config.example.yaml               ([58 L] Unified YAML: server + desktop settings)
├── Cargo.toml                         ([36 L] Workspace root: 5 members, candle fork deps, LTO)
├── scripts/
│   ├── convert_voice_cache.py        (Python: PyTorch .pt -> .safetensors conversion)
│   └── sse_to_wav.py                 (Python: SSE stream -> ffplay pipe)
└── voices/                            (Test voice samples + streaming_model/ subdir)
```

---

## 4. Feature Inventory

### 4.1 TTS Engine (Core)

**Model Variants (3):**
| Model | Params | Hidden | Layers | Use Case | File |
|-------|--------|--------|--------|----------|------|
| Batch 1.5B | 1.5B | 1536 | 28 + 12 heads | Default quality/speed balance | `model.rs` |
| Batch 7B | 7B | 3584 | 28 + 28 heads | Highest quality | `model.rs` |
| Realtime 0.5B | 0.5B | 1024 | 24 (4+20 split) | Streaming, lowest latency | `realtime/model.rs` |

**Batch Model Pipeline** (`model.rs:1110-1703`):
1. Token embedding -> optional voice injection -> Qwen2 LLM forward
2. Dual-pass CFG: positive (conditioned) + negative (unconditional) forward passes
3. Token sampling with constrained logits (only 4 allowed tokens: SPEECH_START, SPEECH_END, SPEECH_DIFFUSION, EOS)
4. On SPEECH_DIFFUSION token: extract conditions -> DPM-Solver++ diffusion -> VAE decoder -> audio
5. Acoustic connector + semantic tokenizer produce combined embed for next iteration
6. SPEECH_END zeros streaming caches; EOS stops generation

**Realtime Model Pipeline** (`realtime/model.rs:278-473`):
1. Text tokenized, split into 5-token windows
2. For each window: DualSplitLLM forward through both paths (pos/neg)
3. Generate 6 speech tokens per window: DPM-Solver++ diffusion -> VAE decode -> acoustic connector -> feed back
4. Binary classifier checks EOS after each speech token
5. KV caches from voice cache initialize speaker characteristics

### 4.2 Voice Cloning

**Batch models**: WAV voice sample -> VAE encoder -> Gaussian sampling -> normalization -> acoustic connector -> LLM embeddings. Voice embeddings injected into token embeddings at masked positions (via `inject_voice_embeddings()` in `model.rs:597-724`).

**Realtime model**: Pre-computed voice cache (`.safetensors` file) containing KV states from processing a reference audio through all 4 model paths (pos_lm, pos_tts, neg_lm, neg_tts). Cached once, reused for all synthesis.

**Voice cache format** (`realtime/voice_cache.rs`): 4 entries, each with `last_hidden_state` + per-layer KV pairs. Total ~24 layers x 2 tensors x 4 paths = significant memory but enables zero-shot voice cloning.

### 4.3 Multi-Speaker Dialogue

Script-based (`facade.rs:200-246`, `processor.rs:156-230`):
- Input: `"Speaker 1: Hello\nSpeaker 2: Hi there"` format
- VoiceMapper scans voice directory, maps speakers to WAV samples
- Each speaker gets unique VAE token injection for voice cloning
- Multi-speaker audio: all voices processed in a single forward pass with per-speaker voice masks

### 4.4 Streaming SSE Audio

Server-side (`server/lib.rs:648-715`):
- SSE endpoint: header event (44-byte WAV header) -> chunk events (raw PCM) -> complete event
- PCM chunks encoded as base64 for JSON-safe transport
- Client reconstructs: WAV header + all PCM chunks -> valid WAV file
- Keep-alive every 15 seconds

Client-side (`web/src/sse/stream.rs:274 L`):
- Fetch API with ReadableStream for SSE parsing
- StreamingState accumulates PCM chunks
- Web Audio API playback via AudioContext

### 4.5 GPU Acceleration

Compile-time feature flags:
- `metal`: Apple Silicon GPU
- `cuda`: NVIDIA CUDA
- `cudnn`: NVIDIA cuDNN
- `flash-attn`: Flash Attention (CUDA only)
- `accelerate`: Apple Accelerate Framework (CPU)
- `mkl`: Intel MKL (CPU)

Platform convenience features: `macos = [metal, accelerate]`, `linux-gpu = [cuda, cudnn, flash-attn]`, `windows-gpu = [cuda, cudnn, flash-attn]`

### 4.6 HTTP API

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/health` | GET | Health check `{"status":"ok"}` |
| `/voices` | GET | List available voice safetensors + WAV samples |
| `/synthesize` | POST | Returns WAV binary, model selection per-request |
| `/synthesize/json` | POST | Returns JSON with base64-encoded WAV |
| `/synthesize/stream` | POST | SSE streaming with WAV header + PCM chunks |

### 4.7 Configuration & Settings

Unified YAML (`config.example.yaml`, `server/lib.rs:28-181`):
- Server binding: host, port
- Directories: safetensors_dir (realtime), wav_dir (batch), output_dir, web_dir
- CORS origins list
- Desktop section: embedded_server toggle, remote_server_url, default_model, hotkey, notifications

Config migration (`tauri/config.rs:61-137`): Automatic TOML->YAML migration on first run with backup.

### 4.8 Desktop Features (Tauri)

- System tray with Show/Hide/Quit menu + left-click toggle
- Global hotkey (`Ctrl+Shift+V` default) to show window and trigger synthesis
- Embedded server mode (runs vibevoice-server in-process)
- Remote server mode (connects to remote HTTP API)
- Native directory picker for voice/sample directories
- Desktop notifications on synthesis complete
- Platform config: macOS `~/Library/Application Support`, Linux `~/.config`, Windows `%APPDATA%`

---

## 5. Key Code Patterns & Techniques

### 5.1 PyTorch RNG Parity (`pytorch_rng.rs`, 608 lines)

This is the most technically impressive module in the codebase. It achieves bit-identical random number generation with PyTorch by:

1. **MT19937 32-bit** (not 64-bit!) — PyTorch truncates seeds to u32
2. **Two algorithm paths** based on tensor size:
   - Scalar (< 16 elements): 53-bit double-precision uniforms with Box-Muller caching
   - Vectorized (>= 16, multiples of 16): 24-bit float uniforms with SIMD-style batch processing
3. **Box-Muller caching**: PyTorch caches the second value of each pair; must match exactly
4. **`log(1 - u2)` not `log(u2)`**: Avoids log(0) when u2=1.0
5. **Global CPU RNG with save/restore**: Enables independent RNG sequences for voice embedding vs diffusion, matching Python''s separate MPS/CPU RNG streams

This module alone is worth studying for any Rust ML project that needs Python parity.

### 5.2 Dual-Pass CFG Architecture (`model.rs:1156-1703`)

For classifier-free guidance, the batch model uses a single Qwen2Model but alternates between positive and negative KV caches:
- **Positive path**: Regular token embeddings forward pass -> extract KV cache
- **Negative path**: Restore negative KV cache -> forward with SPEECH_DIFFUSION as conditioning token -> save negative KV cache
- **Restore positive KV cache** before next sampling step
- CFG applied in diffusion: `output = negative + cfg_scale * (positive - negative)`

This is more memory-efficient than the realtime model''s approach of having 4 separate Qwen2Model instances.

### 5.3 Split LLM Architecture (`realtime/split_llm.rs`, 584 lines)

The realtime model splits a 24-layer Qwen2 into:
- **Lower LM** (4 layers): Text processing, no final RMS norm (`forward_from_embeds_no_norm`)
- **Upper TTS LM** (20 layers): Speech generation with type embeddings (text=1, speech=0)

**Four Qwen2Model instances**: pos_lm, pos_tts, neg_lm, neg_tts. They share weights via VarBuilder''s tensor registry (reference-counted) but maintain independent KV caches. A single `image_pad_token_id` embedding is used as the unconditional baseline for the negative path.

**Hidden state splicing** (`splice_hidden_states`, line 454-509): LM hidden states replace the corresponding positions in TTS LM input embeddings — this connects the text understanding with speech generation.

### 5.4 Windowed Streaming Generation (`realtime/generation.rs`, 447 lines)

- Text processed in 5-token windows
- Each window produces 6 speech tokens
- Generation is stateful: `WindowedGenerator` tracks hidden states, KV cache positions, and streaming VAE cache across windows
- EOS detection via binary classifier (fc1->ReLU->fc2->sigmoid > 0.5)
- Generation continues even after text is exhausted (auto-regressive extension)

### 5.5 BF16->F32 Weight Conversion (`utils.rs`)

All model weights are converted from BF16 to F32 on load. This is critical for CPU inference (BF16 has no native CPU support) and for numerical stability. The conversion happens once at VarBuilder creation time, not per-forward-pass.

### 5.6 Weight Name Remapping (`utils.rs:394-563`)

Two separate remapping strategies:
1. **Batch model**: `model.language_model.X` -> `model.X`, `model.acoustic_tokenizer.X` -> `acoustic_tokenizer.X`, etc.
2. **Realtime model**: `model.language_model.X` -> `model.language_model.model.X` (inserts `.model.` for Qwen2Model compatibility), adds dummy norm.weight
   
This allows loading HuggingFace-format safetensors directly into Candle''s VarBuilder with different internal prefix conventions.

### 5.7 Unified Config (Shared YAML)

The same `Config` struct (`server/lib.rs:28-181`) is used by both the CLI server and Tauri desktop app. The desktop adds an optional `desktop:` section ignored by the CLI server. This avoids config format fragmentation across sub-projects.

### 5.8 Voice Path Resolution (`utils.rs:53-167`)

Multi-stage fallback:
1. Absolute path -> used directly
2. Relative path from CWD -> check existence
3. Relative to script directory -> common for examples
4. Relative to executable directory -> packaged apps
5. Voice name search in `voices/` dirs -> API ergonomics

With model-aware extension checking: `.safetensors` for realtime, `.wav` for batch.

### 5.9 Debug Checkpoint System (`model.rs:22-54`)

When tracing is at DEBUG level, every intermediate tensor (noise, conditions, diffusion output) is saved as `.npz` to `debug/checkpoints/` with auto-incrementing names. This enables bit-exact comparison with Python reference outputs.

---

## 6. Relation to S2B2S

### Comparison Table

| Aspect | vibevoice-rs | S2B2S | Verdict |
|--------|-------------|-------|---------|
| **TTS engine type** | Neural diffusion LLM (Qwen2-based) | Multi-backend orchestrator (Piper, Kokoro, Kitten, Pocket, SAPI, cloud APIs) | S2B2S is more pragmatic; vibevoice is higher quality but GPU-only |
| **Real-time factor** | 0.5B model achieves >1x RTF on GPU | Streaming gapless playback (rodio-based) | vibevoice is faster for neural TTS on GPU |
| **Voice cloning** | Zero-shot via voice cache (KV states) | Pocket TTS voice cloning (separate engine) | vibevoice is more integrated |
| **Model loading** | Lazy + cached in HashMap (up to 3 models simultaneously) | Persistent HTTP servers (WarmEngine trait: Loading->Ready) | vibevoice is simpler; S2B2S has richer lifecycle |
| **Streaming** | SSE with WAV header + raw PCM chunks | rodio Sink with streaming gapless playback | S2B2S has real audio playback pipeline; vibevoice delegates to client |
| **Text processing** | Python-matched tokenizer preprocessing (typographic normalization) | 5-stage pipeline: ITN->custom->markdown strip->TN->cleanup | S2B2S is far more comprehensive |
| **Multi-speaker** | Script-based with voice directory mapping | Not applicable (single-voice TTS per invocation) | vibevoice has unique capability |
| **GPU framework** | Candle (pure Rust) | transcribe-rs (Ort), ONNX runtime | Different ecosystems; S2B2S uses ONNX |
| **Desktop app** | Tauri v2 with embedded server + Leptos frontend | Tauri v2 with React/TypeScript frontend | S2B2S has richer UI (20 languages, onboarding, overlay) |
| **Error handling** | thiserror enum (10 variants) | anyhow + thiserror throughout | Comparable |
| **Testing** | Unit tests in each module + NPZ-based parity testing | Integration-heavy (manager pattern) | Different philosophies |
| **Build system** | Cargo only | Bun + Cargo | S2B2S has more build complexity |
| **i18n** | None (English only) | 20 languages via i18next | S2B2S wins |
| **Config** | YAML (server+desktop unified) | Tauri store plugin | vibevoice YAML is more portable |

### What vibevoice-rs Does Better

1. **Pure-Rust TTS stack**: No Python dependency, single `cargo build`
2. **Neural voice cloning integrated**: Not a separate engine, part of the core model
3. **Multi-speaker dialogue**: Single-inference multi-voice synthesis with automatic voice mapping
4. **Numerical parity with Python**: PyTorch RNG + debug checkpoints ensure identical output
5. **Model variant abstraction**: Clean enum dispatch in `facade.rs` with minimal code duplication

### What S2B2S Does Better

1. **Text normalization pipeline**: 5-stage processing (ITN, custom words, markdown strip, TN, cleanup) vs vibevoice''s basic typographic normalization
2. **Audio playback**: Streaming gapless playback via rodio with pause/resume/pre-decode vs client-side Web Audio API
3. **Engine abstraction**: Trait-based `TtsBackend` with 8 backends vs hardcoded 3 model variants
4. **Internationalization**: 20 languages vs English-only
5. **Full application feature set**: STT, Brain (LLM), VAD, audio toolkit, tray i18n, shortcut system

---

## 7. Harvest List (Features Worth Copying)

| Feature to harvest | From file | Effort (XS/S/M/L/XL) | Why valuable for S2B2S |
|---|---|---|---|
| Unified YAML config (server + desktop) | `server/src/lib.rs:28-181`, `config.example.yaml` | S | S2B2S splits config across Tauri store + Python scripts; a unified file would simplify server/CLI/desktop coordination |
| PyTorch RNG parity module | `vibevoice/src/pytorch_rng.rs` | M | Any time S2B2S ports a Python model to ONNX/Rust, bit-identical RNG helps validate correctness |
| SSE streaming with header+chunk pattern | `server/src/lib.rs:648-715`, `web/src/sse/stream.rs` | S | S2B2S''s TTS streaming could adopt this protocol for web-based playback or remote TTS |
| Multi-stage voice path resolution | `vibevoice/src/utils.rs:53-167` | XS | S2B2S''s voice resolution is simpler; this multi-fallback approach would improve robustness |
| Voice cache format (KV states as safetensors) | `realtime/voice_cache.rs` | L | If S2B2S ever adds a neural voice cloning backend, pre-computed KV caches are the standard approach |
| Debug checkpoint export (.npz) | `model.rs:22-54` | S | For validating any model port, saving intermediates for Python comparison is invaluable |
| Model variant enum with per-request override | `facade.rs:13-23`, `server/lib.rs:208-229` | XS | S2B2S''s TTS backends could benefit from runtime model selection per synthesis request |
| BF16->F32 weight conversion on load | `utils.rs:428-433` | XS | Relevant if S2B2S adopts any Candle-based models with BF16 weights |

---

## 8. Known Issues, Caveats & Limitations

| Issue | Severity | Impact |
|-------|----------|--------|
| **GPU required for usable speed** | High | CPU-only mode exists but is impractically slow; 0.5B model needs ~2-4GB VRAM, 7B needs ~20-24GB VRAM |
| **Custom Candle fork required** | Medium | Cannot use upstream Candle; the `forward_from_embeds_no_norm` patch is essential and may drift |
| **Metal buffer overflow on long inputs** | Medium | Documented in README: "Very long inputs may run over the buffer" on Apple Silicon |
| **Flash Attention CUDA-only** | Medium | Metal acceleration is limited without flash attention; worse on Apple hardware for long sequences |
| **No text normalization pipeline** | Medium | Only basic typographic normalization (smart quotes, em dashes); no TN/ITN, no markdown strip |
| **No audio output device support** | Medium | Must save to file or stream via HTTP; no direct speaker output (unlike S2B2S''s rodio pipeline) |
| **Voice cloning requires .safetensors conversion** | Low | Python script needed to convert original .pt voice caches; not fully self-contained |
| **English-only tokenizer** | Medium | Qwen2.5 tokenizer handles English; no multi-language TTS support |
| **No WebRTC or real-time audio streaming** | Low | SSE is HTTP-polling-based; real-time bidirectional audio would need WebRTC |
| **Tauri app bundles model in-process** | High | Starting the embedded server loads the full model in the desktop process; no sandboxing or crash isolation |

---

## 9. Strengths & Weaknesses

### Strengths

1. **Architecture purity**: Everything is Rust — model inference, HTTP serving, desktop app, web frontend. No Python runtime, no Node.js. Single `cargo build` produces all artifacts.

2. **Monorepo organization**: Clean workspace structure with clear dependency hierarchy: vibevoice (lib) -> vibevoice-server -> vibevoice-tauri; vibevoice-web is standalone WASM. No circular dependencies.

3. **Numerical fidelity**: The PyTorch RNG module is a masterclass in cross-framework parity. The debug checkpoint system shows deep commitment to correctness.

4. **Builder pattern**: `VibeVoiceBuilder` with sensible defaults (`seed=524242`, `cfg_scale=1.3`, `diffusion_steps=5/10`) makes the complex model easy to use.

5. **Runtime model switching**: Server caches up to 3 model variants in memory, switching per-request via an optional `model` field. Models loaded lazily on first use.

6. **Streaming from day one**: The SSE protocol with WAV header + PCM chunks is well-designed for progressive playback; the realtime model was built for streaming.

7. **Unified configuration**: YAML format shared between CLI server and Tauri desktop, with desktop extensions that the CLI ignores. Clean separation of concerns.

8. **Comprehensive test infrastructure**: Unit tests in every module, NPZ checkpoint I/O for Python parity testing, well-commented test helpers.

### Weaknesses

1. **GPU-only practical**: CPU inference exists but is unusably slow. This makes the project inaccessible to many users and increases deployment complexity.

2. **No audio I/O abstraction**: Unlike S2B2S which uses cpal + rodio for device enumeration and playback, vibevoice only produces WAV bytes. Client-side playback is entirely the user''s responsibility.

3. **Minimal text processing**: Only basic typographic normalization. No inverse text normalization (ITN) for post-STT, no text normalization (TN) for pre-TTS numbers/dates, no markdown stripping.

4. **Heavy model requirements**: 7B model needs ~20GB VRAM. Even the 0.5B model needs 2-4GB. Not suitable for edge devices or laptops without discrete GPU.

5. **Limited platform matrix**: Metal support is documented as having issues. Linux GPU requires CUDA (no Rocm/OpenCL). Windows GPU untested in CI (no Windows CI config visible).

6. **Single-language**: English only, tied to Qwen2.5 tokenizer. No multi-language voice support.

7. **No hot-reload or live config**: Server must restart on config change. Tauri app requires user-initiated restart.

8. **Voice cache complexity**: Requires Python script for .pt->.safetensors conversion. No bundled voice cache creation tool in Rust.

---

## 10. Bottom Line / Verdict

vibevoice-rs is an impressive achievement: a complete, pure-Rust TTS stack from neural inference to desktop deployment. Its strongest technical contributions are the PyTorch RNG parity module (worth studying for any ML port), the split-LLM architecture with dual-pass CFG (a novel approach to classifier-free guidance), and the clean monorepo organization that serves as a template for Rust ML application development.

For S2B2S, the most actionable takeaways are the unified YAML config pattern, the SSE streaming protocol design, and the multi-stage voice path resolution. The neural TTS architecture is fascinating but orthogonal to S2B2S''s ONNX-based multi-backend approach. The single most valuable idea is the debug checkpoint system — saving every intermediate tensor to .npz for Python comparison — which could dramatically accelerate S2B2S''s model validation efforts.

**Worth studying?** Yes, as a reference for how to structure a Rust ML monorepo and for its numerical parity techniques. **Worth copying?** The config unification, SSE streaming protocol, and debug checkpoint patterns are directly portable. The neural architecture itself is not applicable to S2B2S''s current ONNX-based approach.

---

*Analysis generated from full source reading of ~9,200 lines of Rust across 58 source files, 5 examples, and all configuration/markdown documentation.*
