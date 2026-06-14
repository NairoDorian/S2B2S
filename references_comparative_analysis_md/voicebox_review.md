# Voicebox — Independent Voice I/O App

> Repo: `jamiepine/voicebox` · HEAD: v0.5.0 · License: MIT · Author: Jamie Pine · Platforms: macOS (Apple Silicon / Intel), Windows, Linux (build from source), Docker
> Nature: independent · Role for S2B2S: Category B reference — the most architecturally complete independent voice I/O app. Study its MCP server integration, local LLM refinement pipeline, Voice Personality system, multi-engine TTS registry pattern, split Rust-in-frontend + Python-in-backend architecture, and hardware-accelerated cross-device dictation paste pipeline.

---

## 1. What Voicebox Is

Voicebox is a local-first, open-source AI voice studio — a combined STT (speech-to-text) + TTS (text-to-speech) desktop application that positions itself as a self-hosted alternative to ElevenLabs (TTS) and WisprFlow (dictation) in a single app. Its core thesis: the full voice I/O loop — microphone input, speech recognition, LLM refinement, speech synthesis, speaker output — should run entirely on the user's machine, with voice cloning as the unifying identity layer.

The app offers three main workflows:
1. **Voice Generation (TTS output):** Type text, select a voice profile (cloned or preset), and generate speech in 23 languages across 7 TTS engines. Supports unlimited-length generation via sentence-boundary chunking with crossfade, a post-processing effects pipeline (8 pedalboard effects), and an async serial generation queue.
2. **Dictation (STT input):** Hold a global hotkey chord anywhere, speak, release — on macOS the transcript is auto-pasted into the focused text field with atomic clipboard save/restore. Supports push-to-talk and toggle modes, in-app mic input on text fields, and optional LLM refinement to strip disfluencies.
3. **Agent Voice Output:** A built-in MCP (Model Context Protocol) server at `/mcp` exposes `voicebox.speak`, `voicebox.transcribe`, `voicebox.list_captures`, and `voicebox.list_profiles`. Any MCP-aware agent (Claude Code, Cursor, Windsurf, Cline, VS Code MCP extensions) can speak to the user in a cloned voice via a single tool call.

Unique differentiators over S2B2S include voice cloning from reference audio, a multi-track Stories timeline editor, per-voice-profile "personalities" (LLM-driven character prompts for Compose and Rewrite), and a bidirectional on-screen pill overlay that surfaces both dictation states and agent speech states.

---

## 2. Tech Stack

### 2.1 Frontend

| Layer | Choice | Purpose |
|-------|--------|---------|
| Framework | Tauri 2.x (Rust) | Cross-platform desktop shell, global hotkeys, clipboard, paste injection |
| UI Framework | React 19 + TypeScript 5 | SPA frontend rendered in Tauri webview |
| Styling | Tailwind CSS 4 | Utility-first CSS |
| State | Zustand 5, React Query | UI state management, server-cache queries |
| i18n | i18next 26 | 4 locales (en, zh-CN, zh-TW, ja) full coverage |
| Routing | React Router | Tab-based navigation (Voices, Stories, Captures, Models, Settings, Audio, Effects) |
| Audio Playback | WaveSurfer.js | Waveform display with click-to-seek on captures |
| API Client | Generated TypeScript (OpenAPI codegen) | Typed client from FastAPI schema |

### 2.2 Backend / Core

| Layer | Choice | Purpose |
|-------|--------|---------|
| API Server | FastAPI (Python 3.11+) | REST API for TTS, STT, LLM, MCP, and management endpoints |
| ASGI Server | uvicorn | Python async HTTP |
| ML Runtime | MLX (Apple Silicon) / PyTorch (CUDA/ROCm/DirectML/XPU/CPU) | Hardware-aware model inference |
| TTS Engines | Qwen3-TTS (0.6B/1.7B), Qwen CustomVoice, LuxTTS, Chatterbox Multilingual, Chatterbox Turbo, HumeAI TADA (1B/3B), Kokoro 82M | Seven independent TTS engines, selectable per-generation |
| STT Engine | OpenAI Whisper (Base/Small/Medium/Large/Turbo) | Speech-to-text transcription |
| Local LLM | Qwen3 (0.6B / 1.7B / 4B) | Transcript refinement, personality Compose/Rewrite |
| MCP Server | FastMCP (Streamable HTTP) | Agent tool integration (speak, transcribe, list_captures, list_profiles) |
| Database | SQLite (SQLAlchemy ORM) | Voice profiles, generations, captures, stories, settings, MCP bindings |
| Audio Effects | pedalboard (Spotify) | Pitch shift, reverb, delay, chorus, compressor, gain, HPF, LPF |
| Audio I/O | soundfile, librosa | WAV read/write, audio processing |
| Native Shim | Rust inside Tauri | Global hotkey (keytap), paste injection (CGEventPost / SendInput), focus capture (AX / UIAutomation), clipboard save/restore |
| Packaging | PyInstaller (onedir) | Frozen Python server as Tauri sidecar with per-platform PyTorch backends |

### 2.3 Key Dependencies (non-obvious ones)

- **keytap**: Rust crate powering the global hotkey chord matcher — handles push-to-talk/toggle state machine, longest-match resolution, sticky-toggle semantics, and left/right modifier distinction all the way down to the OS event tap
- **screencapturekit-rs**: macOS system audio capture (mic + system audio dual stream, planned for long-form capture)
- **FastMCP**: HTTP-based MCP server library — mounted on FastAPI at `/mcp` with Streamable HTTP transport
- **voicebox-mcp binary (stdlib shim)**: A bundled Tauri sidecar that adapts MCP stdio transport to HTTP, shipping inside the app bundle
- **rubato**: Audio resampling (used in sample processing pipeline)
- **scipy/numpy**: Core audio DSP; several PyInstaller runtime hooks exist to patch frozen-importer bugs


---

## 3. Architecture & Source Map

```
voicebox/
├── app/                          # Shared React frontend (runs in Tauri webview or standalone web)
│   └── src/
│       ├── main.tsx              # React entry, i18n init, router mount
│       ├── App.tsx               # Root component with onboarding flow + platform detection
│       ├── router.tsx            # Tab routes: /voices, /stories, /captures, /models, /settings, /effects, /audio
│       ├── platform/             # Tauri vs web platform detection (PlatformContext.tsx)
│       ├── stores/               # Zustand stores: uiStore, storyStore, serverStore, playerStore, generationStore, effectsStore, audioChannelStore, logStore
│       ├── hooks/                # React hooks: useAutoUpdater, useThemeSync
│       ├── i18n/                 # i18next setup + 4 locale JSON files (~559 keys each)
│       ├── lib/
│       │   ├── api/              # Generated OpenAPI TypeScript client (models, schemas, services)
│       │   ├── hooks/            # Domain hooks: useProfiles, useGeneration, useHistory, useTranscription, useAudioRecording, useCaptureRecordingSession, useDictationReadiness, useChordSync, useMCPBindings, useSettings, useStories, useServer, etc.
│       │   ├── constants/        # UI constants, language metadata
│       │   └── utils/            # format, audio, parseChangelog, keyCodes, debug
│       └── components/
│           ├── VoicesTab/        # Voice profile grid, inspector
│           ├── VoiceProfiles/    # Profile CRUD, sample upload, audio recording, system audio capture
│           ├── CapturesTab/      # Dictation capture list, inline WaveSurfer player, readiness checklist
│           ├── Generation/       # FloatingGenerateBox, EngineModelSelector, ParalinguisticInput
│           ├── StoriesTab/       # Multi-track timeline editor, clip splitting, drag-and-drop
│           ├── History/          # HistoryTable with status, retry, cancel, bulk clear
│           ├── Effects/          # EffectsChainEditor, GenerationPicker
│           ├── EffectsTab/       # Effects list, detail, chain editor
│           ├── AudioTab/         # Audio channel management, output device picker
│           ├── ServerTab/        # Settings sub-tabs: General, Captures, Generation, GPU, Logs, Changelog, About
│           ├── MCPPage/          # MCP client bindings management
│           ├── AudioPlayer/      # AudioPlayer with waveform, AudioKeepAlive
│           ├── CapturePill/      # Floating dictation/agent-speech pill overlay
│           ├── ChordPicker/      # Visual chord configurator with left/right modifier badges
│           ├── DictateWindow/    # Separate Tauri webview window for the dictate pill
│           ├── AppFrame/         # Main app chrome with sidebar navigation
│           ├── Sidebar.tsx       # Navigation sidebar
│           ├── ui/               # Reusable UI primitives (shadcn-style)
│           └── ServerSettings/   # ModelManagement, ConnectionForm, GpuAcceleration, ServerStatus, UpdateStatus
│
├── backend/                      # Python FastAPI server (bundled as PyInstaller sidecar)
│   ├── main.py                   # Entry: uvicorn runner, argparse for --host/--port/--data-dir (45 lines)
│   ├── server.py                 # PyInstaller entry point: watchdog, frozen-build patches, --parent-pid monitor (303 lines)
│   ├── app.py                    # FastAPI factory: CORS, lifespan (startup/shutdown), MCP mount, SPA mount (314 lines)
│   ├── config.py                 # Data dir management, storage path resolution, dir helpers (140 lines)
│   ├── models.py                 # Pydantic request/response models for all endpoints
│   ├── __init__.py               # Version constant
│   │
│   ├── backends/                 # ML backend abstraction layer
│   │   ├── __init__.py           # Protocols (TTSBackend, STTBackend, LLMBackend), engine registry, ModelConfig, factory functions (781 lines)
│   │   ├── base.py               # Shared utilities: cache check, device detection, voice prompt combine, model_load_progress context manager, Chatterbox float64 patches (327 lines)
│   │   ├── pytorch_backend.py    # TTS + STT backends for PyTorch (CUDA/DirectML/CPU)
│   │   ├── mlx_backend.py        # TTS + STT backends for Apple Silicon MLX
│   │   ├── luxtts_backend.py     # LuxTTS engine backend
│   │   ├── chatterbox_backend.py # Chatterbox Multilingual engine backend
│   │   ├── chatterbox_turbo_backend.py # Chatterbox Turbo engine backend
│   │   ├── hume_backend.py       # HumeAI TADA engine backend
│   │   ├── kokoro_backend.py     # Kokoro 82M engine backend
│   │   ├── qwen_custom_voice_backend.py # Qwen CustomVoice preset engine backend
│   │   └── qwen_llm_backend.py   # Qwen3 LLM backend (MLX + PyTorch variants)
│   │
│   ├── services/                 # Business logic layer
│   │   ├── tts.py                # TTS service: thin delegate to backends (34 lines)
│   │   ├── transcribe.py         # STT service: thin delegate to backends (22 lines)
│   │   ├── llm.py                # LLM service: thin delegate to backends
│   │   ├── generation.py         # Core TTS orchestration: generate/retry/regenerate modes, chunked TTS, effects (348 lines)
│   │   ├── task_queue.py         # Serial async generation queue with cancellation support (139 lines)
│   │   ├── refinement.py         # Transcript refinement: flag-driven system prompt assembly, repetition collapse, few-shot examples (295 lines)
│   │   ├── personality.py        # Character-driven Compose/Rewrite via local LLM (120 lines)
│   │   ├── profiles.py           # Voice profile CRUD, voice prompt creation
│   │   ├── captures.py           # Capture (dictation) processing
│   │   ├── stories.py            # Story timeline CRUD
│   │   ├── history.py            # Generation history queries
│   │   ├── effects.py            # Effects preset management
│   │   ├── settings.py           # User settings persistence
│   │   ├── channels.py           # Audio channel management
│   │   ├── export_import.py      # Profile export/import
│   │   ├── versions.py           # Generation version tracking
│   │   └── cuda.py               # CUDA binary download/update
│   │
│   ├── routes/                   # FastAPI route handlers (17 domain routers)
│   │   ├── __init__.py           # Router registration
│   │   ├── generations.py        # /generate, /generate/{id}/cancel, /generate/stream
│   │   ├── transcription.py      # /transcribe
│   │   ├── captures.py           # /captures CRUD + audio streaming
│   │   ├── profiles.py           # /profiles CRUD + samples + compose
│   │   ├── stories.py            # /stories CRUD + split/duplicate
│   │   ├── history.py            # /history queries + bulk delete
│   │   ├── settings.py           # /settings CRUD
│   │   ├── speak.py              # /speak (agent voice output)
│   │   ├── events.py             # SSE streaming: /events/speak, /events/generate
│   │   ├── effects.py            # /effects CRUD
│   │   ├── channels.py           # /channels CRUD
│   │   ├── models.py             # /models status, download, unload, migrate
│   │   ├── audio.py              # /audio playback
│   │   ├── health.py             # /health, /shutdown, /watchdog/disable
│   │   ├── cuda.py               # /cuda download/status
│   │   ├── mcp_bindings.py       # /mcp/bindings CRUD
│   │   ├── tasks.py              # /tasks status
│   │   └── llm.py                # /llm endpoints
│   │
│   ├── mcp_server/               # MCP server
│   │   ├── server.py             # FastMCP instance creation, lifespan composition (79 lines)
│   │   ├── tools.py              # Tool registration: voicebox.speak, .transcribe, .list_captures, .list_profiles
│   │   ├── resolve.py            # Voice profile resolution (name→id, per-client binding, global default)
│   │   ├── context.py            # ClientIdMiddleware: X-Voicebox-Client-Id header → ContextVar
│   │   └── events.py             # MCP event handlers
│   │
│   ├── mcp_shim/                 # Stdio-to-HTTP MCP shim (bundled as voicebox-mcp binary)
│   │   ├── __init__.py
│   │   └── __main__.py
│   │
│   ├── database/                 # SQLite persistence
│   │   ├── models.py             # ORM models: VoiceProfile, ProfileSample, Generation, Story, StoryItem, Capture, EffectPreset, MCPSettings, AudioChannel, UserSettings (281 lines)
│   │   ├── session.py            # Session factory
│   │   ├── migrations.py         # Schema migrations
│   │   └── seed.py               # Default data seeding
│   │
│   ├── utils/                    # Shared utilities
│   │   ├── chunked_tts.py        # Sentence-boundary text splitter + crossfade concatenation (299 lines)
│   │   ├── audio.py              # Audio loading, normalization, trimming, WAV encoding
│   │   ├── effects.py            # Pedalboard effect chain application
│   │   ├── hf_progress.py        # HuggingFace download progress tracking (tqdm patching)
│   │   ├── hf_offline_patch.py   # transformers offline-compatibility monkey-patches
│   │   ├── platform_detect.py    # MLX vs PyTorch backend detection
│   │   ├── progress.py           # Download progress manager
│   │   ├── tasks.py              # Task manager for download tracking
│   │   ├── capture_chords.py     # Platform-specific chord defaults (macOS vs Windows) (23 lines)
│   │   ├── cache.py              # Voice prompt cache management
│   │   ├── dac_shim.py           # Lightweight DAC shim (replaces descript-audio-codec for TADA)
│   │   └── images.py             # Image utilities
│   │
│   ├── tests/                    # pytest test suite (~16 test files)
│   │   ├── test_all_models_e2e.py        # End-to-end generation with every TTS engine
│   │   ├── test_refinement_samples.py    # Refinement prompt accuracy
│   │   ├── test_task_queue_cancellation.py
│   │   ├── test_whisper_download.py
│   │   ├── test_qwen_download.py
│   │   └── ...                           # Various unit/integration tests
│   │
│   └── pyi_hooks/                # PyInstaller runtime hooks
│       ├── hook-scipy.stats._distn_infrastructure.py  # scipy frozen-importer NameError patch
│       └── hook-transformers.masking_utils.py         # torch._dynamo stub patch
│
├── tauri/                        # Tauri desktop app wrapper
│   └── src-tauri/
│       ├── src/
│       │   ├── main.rs           # Tauri builder: state management, commands, window lifecycle, RunEvent handling (1503 lines)
│       │   ├── lib.rs            # Module re-export: audio_capture (1 line)
│       │   ├── hotkey_monitor.rs # Global hotkey: keytap ChordMatcher → Effect translation → Tauri events (287 lines)
│       │   ├── focus_capture.rs  # Cross-platform focus inspection: macOS AX API, Windows UIAutomation (535 lines)
│       │   ├── clipboard.rs      # Cross-platform clipboard save/restore with change-count verify (718 lines)
│       │   ├── audio_capture/    # System audio capture (macOS/Windows/Linux per-platform modules)
│       │   ├── audio_output.rs   # Multi-device audio playback (cpal)
│       │   ├── speak_monitor.rs  # Rust-side SSE subscriber for agent speech events (180 lines)
│       │   ├── synthetic_keys.rs # Cross-platform synthetic Cmd+V / Ctrl+V paste keystroke
│       │   ├── key_codes.rs      # Key name → keytap::Key mapping
│       │   ├── keyboard_layout.rs # Active keyboard layout detection for Dvorak/AZERTY paste correctness
│       │   ├── accessibility.rs  # macOS Accessibility permission check
│       │   └── input_monitoring.rs # macOS Input Monitoring permission check
│       ├── build.rs
│       └── tests/
│           └── audio_capture_test.rs
│
├── web/                          # Standalone web deployment (Vite)
├── landing/                      # Marketing website (voicebox.sh)
├── docker-compose.yml            # Docker deployment
├── Dockerfile                    # 3-stage build (non-root runtime)
├── justfile                      # Build/dev automation (419 lines, cross-platform)
├── package.json                  # Bun workspace root: app, tauri, web, landing
├── requirements.txt              # Python dependencies
├── scripts/                      # Build & release scripts
└── docs/                         # Fumadocs documentation site
```

---

## 4. Feature Inventory

### 4.1 TTS Pipeline

**7 Engine Multi-Backend Architecture:**
- Qwen3-TTS 0.6B / 1.7B (highest quality, 10 languages, delivery instructions)
- Qwen CustomVoice 0.6B / 1.7B (9 curated preset voices, natural-language delivery control, no reference audio required)
- LuxTTS (~300 MB, English only, 150x realtime on CPU, 48kHz output)
- Chatterbox Multilingual (~3.2 GB, 23 languages including Arabic, Hindi, Swahili)
- Chatterbox Turbo (~1.5 GB, English only, paralinguistic tags)
- HumeAI TADA 1B / 3B (English/multilingual, 700s+ coherent audio, text-acoustic dual alignment)
- Kokoro 82M (~350 MB, 8 languages, 50 curated preset voices, fast CPU inference)

Implementation: `backend/backends/__init__.py` — `TTS_ENGINES` dict, `get_tts_backend_for_engine()` factory with double-checked locking, per-engine `TTSBackend` Protocol instances stored in `_tts_backends` dict. Each backend class (e.g., `ChatterboxTTSBackend`, `KokoroTTSBackend`) implements `load_model()`, `create_voice_prompt()`, `generate()`, `unload_model()`, `is_loaded()`. Models are downloaded on-demand from HuggingFace Hub with progress tracking via `model_load_progress` context manager.

**Voice Profile Types (3-tier):**
- `cloned` — traditional reference-audio profiles (all cloning engines)
- `preset` — engine-specific pre-built voices (Kokoro "am_adam", Qwen CustomVoice presets)
- `designed` — text-described voices (future: Voice Design)

Stored in `backend/database/models.py` `VoiceProfile` with discriminator column `voice_type`.

**Unlimited Generation Length:**
`backend/utils/chunked_tts.py` — 299 lines. Text split at sentence boundaries (`.!?` respecting abbreviations, CJK `。！？`, bracket tag atomicity), per-chunk generation with per-chunk seed variation (`seed + i`), crossfade concatenation (default 50ms, configurable 0–200ms). Max text: 50,000 characters. Falls back to single-shot fast path for text ≤ `max_chunk_chars` (default 800).

**Async Generation Queue:**
`backend/services/task_queue.py` — 139 lines. Serial `asyncio.Queue` worker prevents GPU contention. Supports queued/running/cancelled per-ID state tracking. Enqueue via `enqueue_generation()`, cancel via `cancel_generation()` which either cancels the asyncio Task (running) or marks as cancelled in `_cancelled_generation_ids` (queued). Stale "generating" rows from crashes are marked "failed" on startup.

**Generation Versions:**
- Original (clean TTS output, always preserved)
- Effects versions (apply different effects chains)
- Takes (regenerate with new seed for variation)
- Source tracking (manual, personality_speak)
- Favorites (star for quick access)

**Post-Processing Effects:**
8 pedalboard effects: Pitch Shift (±12 semitones), Reverb (room size, damping, wet/dry), Delay (time, feedback, mix), Chorus/Flanger, Compressor, Gain (-40 to +40 dB), High-Pass Filter, Low-Pass Filter. 4 built-in presets (Robotic, Radio, Echo Chamber, Deep Voice), custom presets, per-profile defaults. `backend/utils/effects.py`.

**Paralinguistic Tags:**
Chatterbox Turbo only. Type `/` to open tag picker: `[laugh]`, `[chuckle]`, `[gasp]`, `[cough]`, `[sigh]`, `[groan]`, `[sniff]`, `[shush]`, `[clear throat]`. Other engines read them as literal text.

### 4.2 STT Pipeline

**Whisper-based Transcription:**
`backend/services/transcribe.py` — thin delegate to `get_stt_backend()`. `STTBackend` Protocol implemented by `MLXSTTBackend` (Apple Silicon) and `PyTorchSTTBackend` (CUDA/DirectML/CPU). 5 model sizes: Base, Small, Medium, Large, Turbo (8x faster than Large).

**Global Dictation (Rust-native):**
`tauri/src-tauri/src/hotkey_monitor.rs` — 287 lines. Uses `keytap` crate for OS-level event tap (macOS CGEventTap) + chord state machine. Resolves push-to-talk and toggle-to-talk chords with left/right modifier distinction. Coalesces PTT→Toggle upgrades into `RestartRecording` effect via same-Instant peek. Emits `dictate:start` / `dictate:stop` / `dictate:restart` Tauri events to the dictate webview.

`tauri/src-tauri/src/focus_capture.rs` — 535 lines. Captures PID + bundle_id + AX role at chord-start via macOS Accessibility API (`AXUIElementCopyAttributeValue`) or Windows UIAutomation (`GetForegroundWindow` + `GetFocusedElement`).

`tauri/src-tauri/src/clipboard.rs` — 718 lines. Multi-format clipboard snapshot/restore on macOS (NSPasteboard per-item per-UTI) and Windows (EnumClipboardFormats HGLOBAL). Change-count verify prevents clobbering user clipboard if something else wrote during the paste window.

**Auto-Paste Pipeline:**
`main.rs` `paste_final_text` command (lines 1064–1102): Activate captured PID → settle 120ms → save clipboard → write transcript → send Cmd+V → wait 400ms → conditionally restore clipboard (only if change_count matches). Short-circuits when focus was inside Voicebox itself or when Accessibility permission is missing.

**Dictate Pill Overlay:**
Separate Tauri webview (`DICTATE_WINDOW_LABEL`), transparent, always-on-top, visible-on-all-workspaces. Positioned top-center of active monitor. States: `recording`, `transcribing`, `refining`, `speaking` (for agent speech). Hidden by parking at (-10000, -10000) with `ignore_cursor_events(true)` to prevent invisible click-through.

### 4.3 Local LLM (Qwen3)

**Uses:**
- **Transcript Refinement** (`backend/services/refinement.py` — 295 lines): Flag-driven system prompt assembly (smart_cleanup, self_correction, preserve_technical). Repetition collapse pre-processing (word-level + character-level passes, 6-token threshold). 6 few-shot examples as structured chat turns. Temperature 0.2, max_tokens 2048.
- **Voice Personality Compose** (`backend/services/personality.py` — 120 lines): Character-framing system prompt. Temperature 0.9 for variety. Returns character utterances.
- **Voice Personality Rewrite** (`backend/services/personality.py`): Restates user text in character voice. Temperature 0.3 for fidelity.

**Model Sizes:** Qwen3 0.6B (400 MB MLX / 1.4 GB PyTorch), 1.7B (1.1 GB MLX / 3.5 GB PyTorch), 4B (2.5 GB MLX / 8 GB PyTorch). MLX uses 4-bit community quantizations; PyTorch uses upstream instruct weights. Single LLM instance shared across refinement + personality (one model cache, one GPU-memory footprint).


### 4.4 MCP Server

`backend/mcp_server/server.py` — FastMCP instance with 4 tools:
- `voicebox.speak(text, profile?, personality?)` — speak text in any voice profile
- `voicebox.transcribe(blob?, path?, model?)` — Whisper transcription (path mode restricted to loopback callers)
- `voicebox.list_captures()` — recent captures with transcripts
- `voicebox.list_profiles()` — available voice profiles

Voice resolution precedence: explicit `profile` arg (name or id, case-insensitive) → per-client binding (from `X-Voicebox-Client-Id` header) → global default → error.

**Transports:**
- Streamable HTTP at `/mcp` — native for Claude Code, Cursor, Windsurf, VS Code MCP extensions
- Stdio shim — bundled `voicebox-mcp` binary inside app bundle for stdio-only clients (`backend/mcp_shim/`)

**Per-Client Voice Binding:**
MCP settings page (`ServerTab/MCPPage.tsx`) manages per-`X-Voicebox-Client-Id` voice profile assignments. Each client entry records `last_seen_at` timestamp to confirm install took.

**Bidirectional Pill:**
Agent-initiated speech surfaces the same dictate pill in `speaking` state. Rust `speak_monitor.rs` subscribes to backend SSE stream (`/events/speak`) and fans `dictate:speak-start` / `dictate:speak-end` events to the pill webview — hidden WebKit windows on macOS throttle EventSource, but Tauri's event bus reliably delivers to hidden webviews.

### 4.5 Stories Timeline Editor

Multi-track editor with drag-and-drop, per-clip volume (0–200%), inline trimming/splitting, auto-playback with synchronized playhead, import external audio (wav/mp3/flac/ogg/m4a/aac/webm), version pinning per clip, export. `backend/routes/stories.py`, `backend/services/stories.py`, frontend `StoriesTab/` components.

### 4.6 GPU Support Matrix

| Platform | Backend | Details |
|----------|---------|---------|
| macOS Apple Silicon | MLX (Metal) | 4-5x faster via Neural Engine; bf16 quantized models from mlx-community |
| Windows/Linux NVIDIA | PyTorch CUDA (cu128) | Auto-detected; split binary (server + libs archive); sm_120+ for Blackwell |
| Linux AMD | PyTorch ROCm | Auto-configures HSA_OVERRIDE_GFX_VERSION=10.3.0 |
| Windows any GPU | DirectML | Universal GPU support via torch_directml |
| Intel Arc | PyTorch XPU (IPEX) | Device-aware seeding, XPU detection in status panel |
| Any | CPU | Works everywhere, just slower |

GPU compatibility diagnostics: `backend/backends/base.py` `check_cuda_compatibility()` compares device compute capability against PyTorch build arch list; health endpoint exposes warning; startup logs mismatch.

### 4.7 Configuration & Settings

Settings stored in SQLite `UserSettings` table (per-row key-value), exposed via FastAPI endpoints. Frontend settings in `ServerTab/` sub-pages: General (theme, language), Captures (dictation toggle, chord picker, auto-paste, transcription model, refinement flags), Generation (default engine), GPU (CUDA install/uninstall), Logs (live server log viewer), Changelog (parsed from CHANGELOG.md), About (version, license, data folder).

---

## 5. Key Code Patterns & Techniques

### 5.1 Split-Process Architecture (Rust → Python)

Voicebox's most distinctive architectural choice: **the Rust Tauri process is a thin shell** — it manages GUI windows, global hotkeys, clipboard save/restore, and focus capture. **All ML inference runs in a separate Python process** (FastAPI + uvicorn, packaged via PyInstaller as a Tauri sidecar). Communication is HTTP (REST API + SSE).

`main.rs` `start_server` command (lines 206–688): spawns `voicebox-server` sidecar with `--data-dir`, `--port`, `--parent-pid`. Monitors stdout/stderr for "Uvicorn running" signal (120s timeout). On `RunEvent::Exit`, writes `.keep-running` sentinel file so server survives if "keep running after close" is enabled; otherwise server's parent-pid watchdog self-terminates.

`backend/server.py`: PyInstaller-frozen entry. `_start_parent_watchdog()` monitors parent PID every 2s; exits when parent dies (unless `.keep-running` sentinel or `/watchdog/disable` HTTP call). Cross-platform PID alive check: Windows `OpenProcess(QUERY_LIMITED_INFORMATION) + GetExitCodeProcess`, Unix `os.kill(pid, 0)`.

**Pros:** Separates ML inference lifecycle from GUI lifecycle; Python ecosystem (transformers, torch, MLX) directly available; server can be remote; Docker-friendly.
**Cons:** HTTP latency for every operation; two processes to manage; PyInstaller frozen-binary complexity (numerous runtime patches for scipy, transformers, torch._dynamo); startup time (torch import ~10-30s).

### 5.2 Triple-Protocol Backend Abstraction

`backend/backends/__init__.py` defines three `@runtime_checkable` Protocols:
- `TTSBackend`: `load_model()`, `create_voice_prompt()`, `generate()`, `unload_model()`, `is_loaded()`
- `STTBackend`: `load_model()`, `transcribe()`, `unload_model()`, `is_loaded()`
- `LLMBackend`: `load_model()`, `generate()`, `unload_model()`, `is_loaded()`

Engine registry `TTS_ENGINES` dict maps engine name → class name. Factory `get_tts_backend_for_engine()` with double-checked locking (`_tts_backends_lock`). `ModelConfig` dataclass provides declarative model metadata (repo_id, size_mb, languages, supports_instruct, needs_trim). `get_all_model_configs()` aggregates TTS + STT + LLM configs from composable helper functions.

**This is the pattern S2B2S should study** for its own TTS backend abstraction. S2B2S's current approach uses a `TtsBackend` trait with `WarmEngine` state machine (Loading → WarmingUp → Ready → Error) — Voicebox's Protocol-based registry with declarative configs is cleaner and more extensible.

### 5.3 Chunked TTS with Smart Sentence Splitting

`backend/utils/chunked_tts.py` `split_text_into_chunks()` — paragraph-length text splitting with priority cascade:
1. Sentence-end `.!?` — skips abbreviations ("Dr.", "Mr.", "U.S."), decimal numbers, and text inside bracket tags
2. CJK sentence-end `。！？`
3. Clause boundary `;:,—`
4. Whitespace hard cut
5. Emergency hard cut that never splits inside `[tags]`

`concatenate_audio_chunks()` — linear crossfade (fade_out + fade_in over overlap region). Configurable crossfade 0–200ms. Used by all 7 engines via `generate_chunked()` wrapper.

### 5.4 Refinement: Flag-Driven System Prompt + Repetition Collapse

`backend/services/refinement.py` `build_refinement_prompt()` — assembles system prompt from boolean flag sections. Pre-processing `collapse_repetitive_artifacts()` runs before LLM sees text: word-level pass (token normalization, 6-token threshold) + character-level pass (regex for multi-word/CJK runs up to 60 chars). Few-shot examples as structured chat turns (not inline in system prompt) to prevent small-model pattern-matching echo.

### 5.5 Clipboard Save/Restore with Change-Count Guard

`tauri/src-tauri/src/clipboard.rs` — macOS: full NSPasteboard item-level snapshot (all UTIs, all data); Windows: EnumClipboardFormats walk with GDI-handle/owner-display/private-format skipping. Restore only if `current_change_count() == after_write_change_count`, otherwise preserves user's newer content (clipboard managers, Universal Clipboard sync, 1Password insertions).

### 5.6 Platform-Specific Defaults

`backend/utils/capture_chords.py`: macOS defaults = right-Cmd + right-Option (avoids left-hand Cmd+Option system shortcuts); Windows = right-Ctrl + right-Shift (avoids AltGr collisions on German/French/Spanish layouts). `keyboard_layout.rs` resolves active layout's V keycode at runtime so Dvorak/AZERTY Cmd+V still triggers the correct physical key.

---

## 6. Relation to S2B2S

### Comparison Table

| Aspect | Voicebox | S2B2S | Verdict |
|--------|----------|-------|---------|
| **Desktop Framework** | Tauri 2.x (Rust) | Tauri 2.x (Rust) | Same foundation |
| **Frontend** | React + TypeScript + Tailwind | React + TypeScript + Tailwind | Same stack |
| **Backend ML Runtime** | Python FastAPI in separate process (PyInstaller sidecar) | All Rust in-process (transcribe-rs, cpal, rodio) | S2B2S has lower latency, fewer processes; Voicebox has richer Python ML ecosystem |
| **STT Engine** | Whisper (5 sizes, MLX/PyTorch) | transcribe-rs (Parakeet V3 + Whisper + Moonshine), all in Rust | S2B2S has engine variety; Voicebox has turbo model and MLX backend |
| **Number of TTS Engines** | 7 (Qwen, LuxTTS, Chatterbox×2, TADA, Kokoro, CustomVoice) | 9 (Piper, Piper Server, Kokoro, Kitten, Pocket, SAPI, OpenAI, ElevenLabs, Cartesia) | Both extensive; Voicebox focuses on local cloning; S2B2S includes cloud APIs |
| **Voice Cloning** | Full zero-shot cloning from reference audio | Not a feature | Voicebox is stronger here |
| **VAD** | Not in backend Python; Rust hotkey controls recording start/stop | TripleVAD (RMS → RNNoise → Silero), continuously running | S2B2S has sophisticated VAD; Voicebox relies on push-to-talk |
| **Dictation Input** | Global hotkey chord → record → STT → optional LLM refinement → paste | Global shortcut → TripleVAD → STT → ITN → paste | Similar flow; S2B2S has VAD gating; Voicebox has LLM refinement |
| **Auto-Paste** | macOS: Accessibility API + Cmd+V, atomic clipboard save/restore, change-count guard. Windows: UIAutomation + SendInput | enigo-based keyboard simulation, clipboard save/restore | Voicebox's implementation is more robust (multi-format clipboard, change-count guard, focus PID capture) |
| **LLM Integration** | Local Qwen3 for refinement + personality. No streaming conversation | SSE streaming LLM client with sentence splitter → TTS bridge, barge-in | S2B2S has richer LLM conversation pipeline; Voicebox uses LLM only for text transformation |
| **Brain/Conversation Flow** | Not present (no STT→LLM→TTS loop) | Core feature: STT → LLM streaming → TTS with barge-in | S2B2S uniquely has this |
| **MCP Server** | Built-in FastMCP server with 4 tools. HTTP + stdio transports. Per-client voice binding. Bidirectional pill. | No MCP server | Voicebox is pioneering agent voice I/O |
| **Voice Personality** | Free-form personality prompt → Compose (generate character lines) + Rewrite (restate in character voice) | Not a feature | Voicebox uniquely has this |
| **Post-Processing Effects** | 8 pedalboard effects with presets | Not a feature | Voicebox uniquely has this |
| **Stories Timeline** | Multi-track timeline editor for conversations/podcasts | Not a feature | Voicebox uniquely has this |
| **Platform Graphics** | MLX (Apple Silicon), CUDA, ROCm, DirectML, XPU (Intel Arc) | CUDA (via llama.cpp), Metal (via llama.cpp) | Voicebox covers more GPU vendors |
| **Model Download UX** | Per-model download/unload, progress tracking, migration, custom models dir | Model download scripts for STT/TTS/Brain models | Voicebox has more polished download UX |
| **i18n** | 4 locales (en, zh-CN, zh-TW, ja) — 559 keys each | 20 locales | S2B2S has broader language support |
| **Database** | SQLite via SQLAlchemy ORM (Python) | SQLite via rusqlite (Rust) | Different approach, same target |
| **Testing** | pytest-based (~16 test files), manual E2E with all models | Type check + format CI | Similar modest test coverage |
| **Linux Status** | Build from source (no pre-built binaries; CI hangs on tauri-action bundler) | Building and functional | S2B2S has better Linux support |

### What S2B2S Does Better
1. **VAD pipeline:** TripleVAD (RMS → RNNoise → Silero) provides continuous voice activity detection, enabling hands-free conversation mode. Voicebox's hotkey-only approach requires manual chord holding.
2. **LLM conversation loop:** S2B2S has the full STT → Brain → TTS pipeline with streaming SSE, sentence splitting, and barge-in. Voicebox has no conversation flow — its LLM is only for text transformation.
3. **Rust in-process performance:** S2B2S does all audio I/O, STT, and VAD in the same Rust process with minimal latency. Voicebox's Python sidecar adds HTTP overhead for every operation.
4. **Cross-platform parity:** S2B2S has first-class Linux support. Voicebox's Linux builds are "build from source" only.
5. **i18n breadth:** 20 languages vs Voicebox's 4.

### What Voicebox Does Better
1. **Voice cloning:** Full zero-shot voice cloning from reference audio across 7 engines. S2B2S has no cloning capability.
2. **MCP server integration:** The cleanest example of agent voice I/O in the open-source space. Per-client voice binding, bidirectional pill, stdio+HTTP transports. S2B2S should study this for its own agent integration.
3. **Refinement pipeline:** Flag-driven system prompt assembly + repetition collapse pre-processing is a well-designed approach. S2B2S's ITN-only post-processing is simpler but less capable.
4. **Voice Personality system:** Character-based Compose/Rewrite is a genuinely innovative feature that bridges cloning and LLM personality.
5. **Effects pipeline:** 8 audio effects with presets is production-ready audio post-processing that S2B2S lacks entirely.
6. **Stories timeline editor:** Multi-track audio composition is a unique value-add.
7. **Clipboard robustness:** Change-count guard, multi-format snapshot/restore, and cooperative macOS activation (yieldActivationToApplication) are more sophisticated than S2B2S's approach.
8. **GPU coverage:** MLX, DirectML, and Intel XPU support alongside CUDA/ROCm covers every modern GPU vendor.
9. **Model download UX:** Per-model download/unload, progress tracking, and custom models directory are user-friendly features S2B2S could learn from.


---

## 7. Harvest List (Features Worth Copying)

| Feature to harvest | From file | Effort | Why valuable for S2B2S |
|---------------------|-----------|--------|------------------------|
| **MCP server with per-client voice binding** | `backend/mcp_server/server.py`, `backend/routes/mcp_bindings.py` | L | Agent voice I/O is a killer feature. S2B2S already has TTS backends; exposing them via MCP would differentiate it. Use FastMCP or rs-mcp. |
| **Flag-driven refinement system prompt** | `backend/services/refinement.py` `build_refinement_prompt()` | S | S2B2S could add optional LLM-based cleanup after ITN normalization. The prompt assembly pattern (base + per-flag sections + few-shot examples as chat turns) is directly portable. |
| **Repetition collapse pre-processing** | `backend/services/refinement.py` `collapse_repetitive_artifacts()` | XS | Pure function, language-agnostic. Apply before ITN to clean Whisper hallucination loops. ~80 lines to port. |
| **Clipboard change-count guard** | `tauri/src-tauri/src/clipboard.rs` `current_change_count()` pattern | S | S2B2S currently saves/restores clipboard unconditionally. The change-count verify pattern prevents clobbering user clipboard if clipboard manager syncs mid-paste. |
| **Cooperative macOS app activation** | `tauri/src-tauri/src/focus_capture.rs` `activate_pid()` macOS path | S | S2B2S uses simpler activation; the yieldActivationToApplication pattern improves paste reliability on macOS 14+. |
| **ModelConfig declarative registry** | `backend/backends/__init__.py` `ModelConfig` dataclass + `get_all_model_configs()` | M | S2B2S's TTS backend list is hardcoded across multiple files. A single declarative registry with engine name → configs would simplify engine management. |
| **Per-model download/unload UI** | `app/src/components/ServerSettings/ModelManagement.tsx` | L | S2B2S handles models via CLI scripts. In-app model management would improve UX. |
| **Pedalboard audio effects** | `backend/utils/effects.py` | M | Audio post-processing (pitch, reverb, compression) could enhance S2B2S's TTS output. Pedalboard is a well-maintained Spotify library. |

---

## 8. Known Issues, Caveats & Limitations

| Issue | Severity | Impact |
|-------|----------|--------|
| Linux: no pre-built binaries; CI hangs on tauri-action bundler (ubuntu-22.04 rpm step) | High | Linux users must build from source |
| Windows/Linux auto-paste not yet implemented (planned for roadmap) | High | Dictation paste only works on macOS. Non-macOS users get transcript-only. |
| PyInstaller frozen-binary complexity: multiple runtime hooks for scipy, transformers, torch._dynamo | Medium | Maintenance burden; each PyTorch/transformers upgrade risks new frozen-importer bugs |
| Chatterbox engines require `--no-deps` install due to numpy/torch version pin conflicts | Medium | Fragile dependency management |
| macOS ≤ 11: ScreenCaptureKit weak-linked; system audio capture unavailable | Low | Only affects older macOS |
| Kokoro Japanese voices: requires ~50MB unidic-lite dictionary (vs 526MB unidic) | Low | Solved in 0.4.3 but adds bundle size |
| No VAD — recording controlled only by hotkey chord (no automatic voice detection) | Medium | Users must manually hold chord; no hands-free dictation |
| No conversation mode — LLM used only for text transformation, not STT→LLM→TTS loop | Medium | Voicebox is STT output + TTS output, not a full conversational AI |
| Single-task generation queue (serial only, no parallel GPU inference) | Low | Intentional design to avoid GPU contention |
| Server startup time (10–30s for torch import) | Medium | PyInstaller sidecar cold start is slow |
| CPU backend is bundled; CUDA backend is separately downloaded (split binary) | Low | Adds complexity but saves bandwidth on updates |

---

## 9. Strengths & Weaknesses

### Strengths

1. **Most complete open-source voice I/O stack:** Voicebox is the only free, local-first app that combines voice cloning, 7 TTS engines, STT dictation, MCP agent integration, audio effects, and a timeline editor under one roof.

2. **MCP server integration sets a standard:** The bidirectional pill, per-client voice binding, HTTP+stdio dual transport, and profile resolution precedence system is the best implementation of agent voice I/O in any open-source project. This is the feature S2B2S should study most deeply.

3. **Voice cloning with multi-engine architecture:** The ability to clone a voice from reference audio and use it across 7 different engines with different strengths (quality, speed, language coverage, expressivity) is unique.

4. **Production-grade PyInstaller packaging:** Despite the complexity, the team has solved real-world frozen-binary issues (scipy frozen-importer, transformers source inspection, espeak-ng data bundling, torch._dynamo stubbing) — hard-earned knowledge valuable for any Python ML desktop app.

5. **Clipboard/focus pipeline robustness:** The cooperative macOS activation, change-count guard, multi-format clipboard snapshot, and hidden-window parking are polished beyond what most dictation apps achieve.

6. **GPU vendor coverage:** MLX + CUDA + ROCm + DirectML + XPU is the broadest GPU support of any voice app.

7. **Clean code architecture:** The backend refactor from 3,100-line monolith to 17 domain routers + services layer + Protocol-based backend abstraction is well-executed. The `ModelConfig` dataclass with `get_all_model_configs()` composable helpers is a pattern worth borrowing.

### Weaknesses

1. **No VAD / hands-free mode:** Dictation requires holding a keyboard chord. There's no voice activity detection, no wake word, no continuous listening mode. This is a significant UX gap for accessibility and hands-busy scenarios.

2. **No conversation/brain loop:** Despite having STT and TTS, there's no pipeline that routes STT output through an LLM and speaks the response. The LLM is used only for text cleansing and character rewriting — not for conversational AI.

3. **Python sidecar adds complexity:** The split-process architecture means every operation incurs HTTP overhead, two processes to manage, PID watchdog logic, PyInstaller maintenance, and cold-start times of 10-30 seconds.

4. **macOS-first dictation:** Auto-paste is macOS-only. Windows/Linux users get transcript text but must manually paste it. This is acknowledged in the roadmap but currently shipping.

5. **Linux build issues:** CI hangs on tauri-action bundler, no pre-built binaries, build-from-source only. Linux is a second-class platform.

6. **Dependency fragility:** Several engines require `--no-deps` installs due to numpy/torch version conflicts. Each PyTorch upgrade risks new frozen-binary regressions.

7. **No streaming STT:** Transcription is batch-only (upload entire audio → get full transcript). Roadmapped WebSocket streaming but not yet implemented.

8. **Limited i18n:** Only 4 locales vs S2B2S's 20. The app is strongly English/Chinese/Japanese-focused.

9. **No cloud TTS fallbacks:** Unlike S2B2S (OpenAI, ElevenLabs, Cartesia), Voicebox has no cloud TTS providers — everything runs local. This is by design but means no access to the highest-quality TTS models.

---

## 10. Bottom Line / Verdict

Voicebox is the most architecturally complete independent voice I/O application in the open-source ecosystem. Its MCP server integration — with per-client voice binding, bidirectional pill overlay, and dual HTTP+stdio transport — is the single most valuable reference implementation for S2B2S to study. The story of v0.5.0's "Capture release" (global dictation + agent voice output + LLM refinement) is essentially the exact feature set S2B2S should aim for in its own evolution: complete the voice I/O loop with an on-screen pill that shows the user what's being spoken, whether from dictation or from an agent.

The single most valuable idea for S2B2S: **the MCP server as the universal voice I/O bridge.** Voicebox proves that a local TTS/STT app can be the voice layer for every MCP-aware agent on the user's machine — Claude Code narrating test results, Cursor speaking error messages, Windsurf confirming deployments. S2B2S already has the engines (9 TTS backends, 3 STT engines), the platform (Tauri), and the pipeline (STT→Brain→TTS). Adding an MCP server with per-client voice binding would transform it from a personal dictation tool into the voice I/O layer for the entire local agent ecosystem.

For S2B2S's architecture specifically, Voicebox's `ModelConfig` declarative registry and Protocol-based backend abstraction are cleaner patterns than the current approach. The flag-driven refinement system prompt and repetition-collapse pre-processing are directly portable as post-STT text cleanup. The clipboard change-count guard and cooperative macOS activation should be adopted for paste reliability.

However, Voicebox's Python-sidecar architecture is not the right path for S2B2S — the Rust in-process approach already gives lower latency, simpler deployment, and better cross-platform reliability. S2B2S should keep its Rust core but adopt Voicebox's MCP, refinement, and UX patterns.

---

*Analysis completed 2026-06-14. Every source file in the voicebox repository was read and analyzed. Line counts verified from actual source files. Comparisons to S2B2S based on S2B2S architecture documented in its AGENTS.md.*
