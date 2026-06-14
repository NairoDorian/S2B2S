# TranscriptionSuite — Independent STT/TTS Desktop App (GPL-3.0)

> Repo: homelab-00/TranscriptionSuite · License: GPL-3.0-or-later · Author: homelab-00
> Platforms: Linux (primary), Windows 11, macOS (arm64/x64)
> Nature: independent · Role for S2B2S: **Pattern donor** — cannot copy code (GPL), can learn architecture patterns

---

## 1. What TranscriptionSuite Is

TranscriptionSuite is a self-hosted, GPU-accelerated speech-to-text desktop platform: a Python 3.13 FastAPI server running inside Docker (7 compose variants for different GPU/driver/network configurations) and an Electron 40 + React 19 desktop dashboard. It provides multi-backend STT (10 engines), speaker diarization with a review system, live mode with VAD streaming, an audio notebook with FTS5 search and calendar, an OpenAI-compatible API, an in-app update system, profile-based post-transcription auto-actions, outgoing webhooks, and watch-folder auto-processing.

Built by a solo developer ("homelab-00", a mechanical engineer who describes the project as "vibecoded"), the project nevertheless exhibits genuinely sophisticated engineering: a 3-wave durability system researched against AssemblyAI/Deepgram/OpenAI patterns, factory-routed backend architecture, 17 in-app update safety specs, a diarization review state machine with confidence scoring and keyboard-navigable UI, and comprehensive cross-platform CI/CD with GPG-signed releases. At ~262 source files (87 Python + 175 TS/TSX), 868+ backend tests, 185 bmad-output specs, 28 docs, and 7 Docker compose variants, this is roughly 50,000+ lines of code.



# TranscriptionSuite — Independent STT/TTS Desktop App (GPL-3.0)

> Repo: \homelab-00/TranscriptionSuite\ · License: GPL-3.0-or-later · Author: homelab-00
> Platforms: Linux (primary), Windows 11, macOS (arm64+x64)
> Nature: independent · Role for S2B2S: **Pattern donor** — cannot copy code (GPL), can learn patterns

---

## 1. What TranscriptionSuite Is

TranscriptionSuite is a self-hosted, GPU-accelerated speech-to-text desktop platform: a Python 3.13 FastAPI server in Docker (7 compose variants) and an Electron 40 + React 19 desktop dashboard. It features: 10 STT backends (Whisper, NeMo Parakeet/Canary, Microsoft VibeVoice-ASR, whisper.cpp Vulkan, 4 MLX variants), speaker diarization with PyAnnote/Sortformer + review UI, live mode, audio notebook with FTS5 search/calendar, OpenAI-compatible API, in-app updates, profile-based auto-actions, outgoing webhooks, and watch-folder auto-processing.

Built by a solo developer ("homelab-00", a mechanical engineer who describes the project as "vibecoded"), it nonetheless shows sophisticated engineering: a 3-wave durability system, factory-routed backend architecture, 17 in-app update safety specs, a diarization review state machine with keyboard UI, and comprehensive cross-platform CI/CD with GPG signing. At ~262 source files (87 Python + 175 TS/TSX), 868+ backend tests, 185 bmad-output specs, 28 docs, and 7 Docker compose variants — roughly 50,000+ lines of code.

---

## 2. Tech Stack

### 2.1 Frontend (Dashboard)

| Layer | Choice | Purpose |
|-------|--------|---------|
| Desktop shell | Electron 40.8.5 | BrowserWindow, system tray, global shortcuts, IPC |
| UI framework | React 19.2.4 | Component rendering with hooks |
| Language | TypeScript 5.9.3 | ES2022, bundler moduleResolution, noEmit |
| Bundler | Vite 7.3.1 | Dev server port 3000, base './' for Electron file:// |
| Styling | Tailwind CSS 4.2.1 | @tailwindcss/vite plugin, oklab-strip PostCSS |
| Server state | @tanstack/react-query 5.90.21 | Caching, polling, invalidation |
| Client state | Zustand 5.0.12 | Ephemeral: import queue, activity, profiles |
| UI primitives | @headlessui/react 2.2.9 | Accessible components |
| Icons | lucide-react 0.564.0 | Open-source icon library |
| Packaging | electron-builder 26.8.1 | AppImage/NSIS/DMG |
| Auto-update | electron-updater 6.3.9 | GitHub release channel |
| File watching | chokidar 5.0.0 | Watch-folder auto-processing |
| Hashing | xxhash-wasm 1.1.0 | Fast file fingerprinting |
| Config | electron-store 11.0.2 | Persistent settings |

### 2.2 Backend / Core (Server)

| Layer | Choice | Purpose |
|-------|--------|---------|
| Language | Python 3.13.x (strict) | NeMo/lhotse compat |
| Framework | FastAPI 0.135.1 | Async REST + WebSocket API |
| Server | uvicorn 0.41.0 | ASGI server |
| Validation | Pydantic 2.12.5 | Request/response schemas |
| ML framework | PyTorch 2.8.0 (CUDA 12.9) | GPU inference |
| STT primary | faster-whisper 1.2.1 + ctranslate2 4.7.1 + WhisperX 3.8.1 | Whisper models (CUDA) |
| STT NeMo | nemo_toolkit[asr] 2.7.0 | Parakeet/Canary |
| STT VibeVoice | VibeVoice-ASR (git pin) | Microsoft ASR |
| STT Vulkan | whisper.cpp sidecar (HTTP) | AMD/Intel GPU |
| STT Apple | mlx-audio, parakeet-mlx, canary-mlx | MLX on Metal |
| Diarization | pyannote.audio 4.0.4 | Speaker identification |
| Diarization Metal | Sortformer (mlx-audio) | Apple Silicon native |
| Audio | soundfile, scipy, ffmpeg-python | Audio I/O, resampling |
| VAD | webrtcvad 2.0.10 + silero-vad 6.2.1 | Voice activity detection |
| Database | aiosqlite 0.22.1 + SQLAlchemy 2.0.48 | Async SQLite + FTS5 |
| Migrations | Alembic 1.18.4 | Schema versioning (13+) |
| Logging | structlog 25.5.0 | Structured logging |
| Package mgr | uv 0.10.8 | Never pip |

### 2.3 Key Non-Obvious Dependencies

- **setuptools < 81** — webrtcvad imports removed pkg_resources
- **PyTorch cu129 index** — explicit [tool.uv.sources] override
- **MLX vs CUDA conflicts** — [tool.uv] conflicts block co-installation
- **Python 3.13 lhotse patch** — _patch_sampler_for_python313() in ParakeetBackend
- **CUDA graph workaround** — _disable_cuda_graphs() for CUDA >= 12.8
- **Docker bootstrap** — Deps installed at first start into /runtime/.venv, not baked into image
- **FUSE 2** — Required on Linux for AppImage
- **nvidia-container-toolkit** — GPU mode with CDI for modern drivers
- **VibeVoice OOM guard** — DEFAULT_MAX_CHUNK_DURATION_S = 60 (was 600s)


---

## 3. Architecture & Source Map

\\\	ext
TranscriptionSuite/
├── server/                              # Backend (Python/FastAPI, Dockerized)
│   ├── config.yaml                      # ★ Central config (765 lines)
│   ├── backend/
│   │   ├── api/main.py                  # ★ App entry (874L): lifespan, middleware, orphan recovery
│   │   ├── api/routes/
│   │   │   ├── websocket.py             # ★ Longform WS (704L): job lifecycle, persist-before-deliver
│   │   │   ├── transcription.py         # File upload/cancel/retry (1557L)
│   │   │   ├── live.py                  # Live mode WS (586L): real-time streaming
│   │   │   ├── notebook.py              # Audio notebook CRUD (1703L — largest route)
│   │   │   ├── llm.py                   # LM Studio integration (1494L)
│   │   │   ├── openai_audio.py          # OpenAI-compatible /v1/audio (476L)
│   │   │   ├── profiles.py              # Profile CRUD (243L)
│   │   │   ├── admin.py                 # Config/model/logs (388L)
│   │   │   ├── search.py                # FTS5 search (136L)
│   │   │   ├── auth.py                  # Token auth (132L)
│   │   │   └── health.py                # Health/ready/status (99L)
│   │   ├── core/                        # Business logic (32 modules)
│   │   │   ├── model_manager.py         # ★ Model lifecycle hub (858L)
│   │   │   ├── live_engine.py           # Live mode orchestration (278L)
│   │   │   ├── audio_utils.py           # CUDA health check, audio loading (723L)
│   │   │   ├── diarization_engine.py    # PyAnnote diarization (375L)
│   │   │   ├── sortformer_engine.py     # Sortformer Metal diarization (171L)
│   │   │   ├── speaker_merge.py         # Merge diarization+transcription (325L)
│   │   │   ├── parallel_diarize.py      # Parallel chunk-based diarization (216L)
│   │   │   ├── auto_action_coordinator.py # Post-transcription auto-actions (492L)
│   │   │   ├── filename_template.py     # Placeholder grammar {date} {title} (174L)
│   │   │   ├── subtitle_export.py       # SRT/VTT export (315L)
│   │   │   ├── webhook.py + webhook_worker.py # Outgoing webhooks (159L+385L)
│   │   │   ├── token_store.py           # Auth token persistence (281L)
│   │   │   ├── startup_events.py        # Event emitter for Electron fs.watch (56L)
│   │   │   ├── download_progress.py     # HF download tracking (225L)
│   │   │   ├── json_utils.py            # NaN/Inf sanitization (37L)
│   │   │   ├── multitrack.py            # Multi-track splitting (365L)
│   │   │   ├── diarization_confidence.py # Confidence scoring (93L)
│   │   │   ├── diarization_review_lifecycle.py # Review state machine (101L)
│   │   │   ├── alias_substitution.py    # Speaker alias substitution (98L)
│   │   │   └── stt/                     # ★ STT subsystem
│   │   │       ├── engine.py            # AudioToTextRecorder (1000L)
│   │   │       ├── capabilities.py      # Model capability detection (91L)
│   │   │       ├── vad.py               # Silero VAD (247L)
│   │   │       └── backends/            # ☆ 11 STT backend implementations (3450+ total lines)
│   │   │           ├── base.py          #   Abstract STTBackend (134L)
│   │   │           ├── factory.py       #   ★ Backend factory routing (124L)
│   │   │           ├── whisperx_backend.py        # WhisperX (536L)
│   │   │           ├── parakeet_backend.py        # NeMo Parakeet (636L)
│   │   │           ├── canary_backend.py          # NeMo Canary (183L)
│   │   │           ├── vibevoice_asr_backend.py   # VibeVoice (1030L  largest backend)
│   │   │           ├── whispercpp_backend.py      # whisper.cpp sidecar (567L)
│   │   │           ├── faster_whisper_backend.py  # Lightweight for Metal (133L)
│   │   │           ├── mlx_whisper_backend.py     # MLX Whisper (175L)
│   │   │           ├── mlx_parakeet_backend.py    # MLX Parakeet (315L)
│   │   │           ├── mlx_canary_backend.py      # MLX Canary (392L)
│   │   │           └── mlx_vibevoice_backend.py   # MLX VibeVoice (189L)
│   │   ├── database/                    # Data persistence (13 modules)
│   │   │   ├── database.py              # SQLite+FTS5 init (1668L)
│   │   │   ├── job_repository.py        # ★ Durability layer (342L)
│   │   │   ├── audio_cleanup.py         # Periodic cleanup (87L)
│   │   │   ├── backup.py                # DB backup/restore (260L)
│   │   │   ├── profile_repository.py    # Profile CRUD (189L)
│   │   │   ├── alias_repository.py      # Speaker alias CRUD (92L)
│   │   │   ├── dedup_query.py           # Audio dedup (103L)
│   │   │   ├── auto_action_repository.py # Auto-action status (250L)
│   │   │   ├── diarization_review_repository.py # Review state (81L)
│   │   │   ├── webhook_deliveries_repository.py # Webhook tracking (284L)
│   │   │   └── migrations/              # Alembic (13+ versions)
│   │   ├── services/webhook_worker.py   # Async webhook delivery (385L)
│   │   ├── utils/keychain.py            # OS keychain integration (135L)
│   │   ├── logging/setup.py             # Structlog (132L)
│   │   ├── config.py                    # ServerConfig (394L)
│   │   └── config_tree.py               # Config tree for PATCH (326L)
│   └── docker/                          # Docker deployment (25 files)
│       ├── Dockerfile                   # Ubuntu 24.04 + Python 3.13 bootstrap
│       ├── entrypoint.py + bootstrap_runtime.py
│       ├── docker-compose.yml           # ★ Base compose
│       ├── docker-compose.linux-host.yml # Overlay: host networking
│       ├── docker-compose.desktop-vm.yml # Overlay: bridge + ports
│       ├── docker-compose.gpu.yml        # Overlay: NVIDIA runtime
│       ├── docker-compose.gpu-cdi.yml    # Overlay: CDI passthrough
│       ├── docker-compose.vulkan.yml     # ☆ Overlay: whisper.cpp sidecar
│       ├── docker-compose.vulkan-wsl2.yml # Overlay: Vulkan WSL2
│       └── podman-compose.gpu.yml        # Overlay: Podman GPU
│
├── dashboard/                          # Desktop app (Electron/React/TS, 175 files)
│   ├── App.tsx                         # ★ Root component (1006L)
│   ├── electron/                       # Main process (26 modules)
│   │   ├── main.ts                     # ★ BrowserWindow, IPC, lifecycle (2264L)
│   │   ├── preload.ts                  # Context bridge (812L)
│   │   ├── dockerManager.ts            # ★ Docker Compose lifecycle (3659L  LARGEST FILE)
│   │   ├── containerRuntime.ts         # Docker/Podman detection (340L)
│   │   ├── mlxServerManager.ts         # macOS Metal server (529L)
│   │   ├── trayManager.ts              # 11 state-aware tray icons (449L)
│   │   ├── shortcutManager.ts          # Global shortcuts (188L)
│   │   ├── waylandShortcuts.ts         # Wayland portal (532L)
│   │   ├── pasteAtCursor.ts            # Cross-platform paste (319L)
│   │   ├── startupEventWatcher.ts      # fs.watch bootstrap events (124L)
│   │   ├── updateManager.ts            # Opt-in update checker (473L)
│   │   ├── updateInstaller.ts          # Checksum verification (530L)
│   │   ├── installerCache.ts           # Hostile-dir hardening (403L)
│   │   ├── compatGuard.ts              # OS/arch/glibc compat (489L)
│   │   ├── platformGate.ts             # Platform-specific routing (68L)
│   │   ├── checksumVerifier.ts         # SHA-256 verification (39L)
│   │   ├── watcherManager.ts           # chokidar folder watch (313L)
│   │   ├── wslDetect.ts               # WSL2 GPU detection (219L)
│   │   └── launchWatchdog.ts          # Crash watchdog (88L)
│   ├── components/                     # React components
│   │   ├── Sidebar.tsx                 # Navigation (442L)
│   │   ├── AudioVisualizer.tsx         # Canvas waveform (193L)
│   │   ├── ui/                         # Shared primitives (10+)
│   │   │   ├── UpdateBanner.tsx        # Update notification (679L)
│   │   │   ├── UpdateModal.tsx         # Pre-install modal (454L)
│   │   │   ├── GlassCard.tsx, Button.tsx, AppleSwitch.tsx, StatusLight.tsx
│   │   │   ├── CustomSelect.tsx (108L), ShortcutCapture.tsx (173L)
│   │   │   ├── LogTerminal.tsx, ErrorFallback.tsx
│   │   │   ├── ActivityNotifications.tsx (144L)
│   │   │   └── QueuePausedBanner.tsx (25L)
│   │   ├── views/                      # Full-page views
│   │   │   ├── SessionView.tsx         # ★ Main transcription (2258L)
│   │   │   ├── NotebookView.tsx        # Audio notebook (1980L)
│   │   │   ├── ServerView.tsx          # Docker/server mgmt (2878L)
│   │   │   ├── SettingsModal.tsx       # App settings (2255L)
│   │   │   ├── AudioNoteModal.tsx      # Audio note editor (2714L)
│   │   │   └── ModelManagerTab.tsx     # Model download (859L)
│   │   ├── recording/                  # Recording sub-components
│   │   │   ├── DiarizationReviewView.tsx # ☆ Diarization review UI (378L)
│   │   │   ├── SpeakerRenameInput.tsx    # Alias input (110L)
│   │   │   ├── AutoActionStatusBadge.tsx # Status badge (212L)
│   │   │   ├── DownloadButtons.tsx       # Export buttons (100L)
│   │   │   └── ConfidenceChip.tsx        # Confidence chip (45L)
│   │   ├── profiles/                   # Profile management
│   │   │   ├── EmptyProfileForm.tsx      # Defaults form (224L)
│   │   │   ├── ProfileSelector.tsx       # Profile picker (73L)
│   │   │   ├── ModelProfilesPanel.tsx    # Model profiles (232L)
│   │   │   └── TemplatePreviewField.tsx  # Live preview (100L)
│   │   ├── import/DedupPromptModal.tsx   # Dedup prompt (90L)
│   │   └── editor/FindReplaceToolbar.tsx # Find/replace (174L)
│   ├── src/                            # Logic layer
│   │   ├── api/client.ts               # ★ REST+WS client (1182L)
│   │   ├── hooks/                      # 30+ React hooks
│   │   │   ├── useTranscription.ts     # ★ WS lifecycle+HTTP fallback (410L)
│   │   │   ├── useLiveMode.ts          # Live mode streaming (295L)
│   │   │   ├── useDocker.ts            # Docker management (550L)
│   │   │   ├── useServerStatus.ts      # Health polling+GPU error (116L)
│   │   │   ├── useImportQueue.ts       # Batch queue (225L)
│   │   │   ├── useSessionImportQueue.ts # Session queue (277L)
│   │   │   ├── useAuthTokenSync.ts     # Docker log token detect (163L)
│   │   │   ├── useTraySync.ts          # Tray state sync (255L)
│   │   │   ├── useWordHighlighter.ts   # Word highlight (176L)
│   │   │   ├── useDiarizationReview.ts # Review state (86L)
│   │   │   ├── useDiarizationConfidence.ts # Confidence (57L)
│   │   │   ├── useRecordingAliases.ts  # Speaker aliases (97L)
│   │   │   ├── useServerEventReactor.ts # Transition matrix (26L)
│   │   │   └── useAriaAnnouncer.ts     # ARIA announcements (24L)
│   │   ├── services/                   # Business logic (non-React)
│   │   │   ├── websocket.ts            # ★ WS client (503L)
│   │   │   ├── audioCapture.ts         # AudioWorklet mic (207L)
│   │   │   ├── modelRegistry.ts        # ☆ Model catalog (545L)
│   │   │   ├── modelCapabilities.ts    # Feature detection (241L)
│   │   │   ├── modelSelection.ts       # Selection logic (228L)
│   │   │   ├── transcriptionFormatters.ts (153L)
│   │   │   └── clientDebugLog.ts       # Debug logging (135L)
│   │   ├── stores/                     # Zustand stores
│   │   │   ├── importQueueStore.ts     # ★ Unified queue (629L)
│   │   │   ├── activityStore.ts        # Activity feed (114L)
│   │   │   ├── activeProfileStore.ts   # Active profile (41L)
│   │   │   ├── dedupChoiceStore.ts     # Dedup choices (56L)
│   │   │   └── ariaAnnouncerStore.ts   # ARIA queue (46L)
│   │   ├── config/store.ts             # electron-store (244L)
│   │   ├── utils/                      # Utilities (configTree 346L, etc.)
│   │   └── types/electron.d.ts         # IPC types (316L)
│   └── public/audio-worklet-processor.js
│
├── build/                             # Build tooling (scripts, pyproject.toml, icons)
├── scripts/benchmark_stt.py           # STT benchmark tool
├── docs/                              # 28 documentation files (13 generated + 15 existing)
├── _bmad-output/                      # ★ 185 AI-assisted specification artifacts
│   ├── brainstorming/                 # 33 sessions (RCA, research, design discussions)
│   ├── planning-artifacts/            # 8 docs (epics, sprints, PRDs, validation)
│   └── implementation-artifacts/      # 141 specs (durability, vulkan, updates, etc.)
├── .github/                           # 4 CI/CD workflows
├── CLAUDE.md                          # AI assistant instructions (132L)
└── LICENSE                            # GPL-3.0-or-later
\\\

**Key file statistics:**
- Server: 87 Python files (~15,000 lines), largest: notebook.py (1703L), database.py (1668L), transcription.py (1557L)
- Dashboard: 175 TS/TSX files (~35,000 lines), largest: dockerManager.ts (3659L), ServerView.tsx (2878L), AudioNoteModal.tsx (2714L)
- Documentation: 28 MD files in docs/ + 185 MD files in _bmad-output/
- Specs: 141 implementation specs, 33 brainstorming sessions, 8 planning artifacts


---

## 4. Feature Inventory

### 4.1 STT Pipeline — 11 Backends

| # | Backend | GPU Target | Translation | Diarization | Live Mode | File (lines) |
|---|---------|------------|-------------|-------------|-----------|--------------|
| 1 | WhisperX | CUDA | Yes | PyAnnote | No | whisperx_backend.py (536L) |
| 2 | Faster-Whisper (Metal) | Metal | Yes | No | Yes | faster_whisper_backend.py (133L) |
| 3 | Parakeet (NeMo) | CUDA | No | PyAnnote | No | parakeet_backend.py (636L) |
| 4 | Canary (NeMo) | CUDA | Yes (24 EU) | PyAnnote | No | canary_backend.py (183L) |
| 5 | VibeVoice-ASR | CUDA | No | Native | No | vibevoice_asr_backend.py (1030L) |
| 6 | whisper.cpp | Vulkan (AMD/Intel) | Yes | No | Yes | whispercpp_backend.py (567L) |
| 7 | Whisper (legacy) | CUDA | Yes | No | No | whisper_backend.py (149L) |
| 8 | MLX Whisper | Apple Silicon | Yes | Sortformer | No | mlx_whisper_backend.py (175L) |
| 9 | MLX Parakeet | Apple Silicon | No | Sortformer | No | mlx_parakeet_backend.py (315L) |
| 10 | MLX Canary | Apple Silicon | Yes | Sortformer | No | mlx_canary_backend.py (392L) |
| 11 | MLX VibeVoice | Apple Silicon | No | Native | No | mlx_vibevoice_backend.py (189L) |

**Factory routing** (factory.py, 124L): Pattern matching on model names in priority order. GGML (.bin/.gguf) → WhisperCppBackend. MLX (mlx-community/*) → MLX backends. NeMo (nvidia/parakeet*, nvidia/canary*) → NeMo backends. Everything else → WhisperX (default).

**VAD** (vad.py, 247L; engine.py, 1000L): Dual Silero + WebRTC VAD. Configurable sensitivity. Chunk-based silence removal for static files, streaming VAD for live mode.

### 4.2 Speaker Diarization & Review System

- **PyAnnote** (diarization_engine.py, 375L): pyannote.audio 4.x pipeline. Requires HF token. Configurable speakers. Parallel vs sequential mode. Chunk-based parallel diarization (parallel_diarize.py, 216L).
- **Sortformer** (sortformer_engine.py, 171L): Apple Silicon native, no HF token, up to 4 speakers.
- **Speaker merge** (speaker_merge.py, 325L): Merges diarization + transcription segments.
- **Review system**: State machine (pending→in_review→completed→released) in diarization_review_lifecycle.py (101L). Keyboard-navigable UI (DiarizationReviewView.tsx, 378L): Tab/Shift+Tab for turns, arrows for navigation, Enter=accept, Esc=skip, Space=bulk-accept. Confidence chips (ConfidenceChip.tsx, 45L): high/green, medium/neutral, low/amber. Speaker aliasing (SpeakerRenameInput.tsx, 110L): SPEAKER_00 → "Alice". Failure tolerance: returns plain transcript on diarization failure, never 5xx.

### 4.3 Live Mode

**Live engine** (live_engine.py, 278L; live.py, 586L): WebSocket-based real-time streaming. VAD speech detection → partial results → final sentences. Model swap sequence: main unload → live load → on stop: unload live → reload main. Supported for Whisper and whisper.cpp. Config from server, not client.

### 4.4 Audio Notebook

**notebook.py** (1703L — largest route): Calendar-based recording management. FTS5 full-text search. Recording CRUD with 18 REST endpoints. Export: SRT/VTT/ASS/plain text. Speaker labels normalized to "Speaker 1"/"Speaker 2" in subtitles. AI summarization via LLM integration. Backup/restore. Calendar timeslot conflict detection.

### 4.5 Model Registry

**modelRegistry.ts** (545L): Canonical model catalog with metadata (family, size, VRAM, languages, translation, live mode, download URL). Bidirectional GGML maps. Capability mirroring: Python (capabilities.py, 91L) + TypeScript (modelCapabilities.ts, 241L). Feature gating per-model without backend round-trips. Model Manager (model_manager.py, 858L): Lazy imports, background NeMo pre-import, CUDA health probe, single-model-at-a-time constraint.

### 4.6 Watch-Folder Auto-Processing

**watcherManager.ts** (313L): chokidar FS monitoring in Electron main process. Three-point file readiness check (size stability, write completion, extension whitelist). 3-second batch window with summary notification. xxhash-wasm fingerprints for processed-file ledger. Separate session/notebook folders. **importQueueStore.ts** (629L): Unified Zustand queue replacing dual hooks. 4 job types (session-normal, session-auto, notebook-normal, notebook-auto). Global pause/resume with server cancel integration. Persistent paused banner.

### 4.7 OpenAI-Compatible API

**openai_audio.py** (476L): Drop-in replacement at /v1/audio/transcriptions + /v1/audio/translations. Multipart form upload following OpenAI spec. Supports all standard formats + diarized_json extension. Word-level timestamps via timestamp_granularities[]=word. Diarization via diarization=true + expected_speakers (1-10). Error envelope matches OpenAI shape. Diarization failure tolerance (plain transcript on failure, never 5xx).

### 4.8 Persist-Before-Deliver Durability System (3 Waves)

This is the project's **most critical architectural invariant**, documented in CLAUDE.md and project-context.md.

**Wave 1 — Job persistence** (spec-wave-1-transcription-durability.md, 183L):
- create_job() at recording start → save_result() after STT → send_message() delivery → mark_delivered() on success
- transcription_jobs SQLite table (migration 006)
- State machine: pending → processing → completed → delivered (or failed)
- GET /api/transcribe/result/{job_id} for HTTP recovery after WS disconnect
- Client polling: 3s intervals, 10 retries (useTranscription.ts, 410L)
- DB write BEFORE WebSocket delivery; if DB write fails, log CRITICAL but still deliver
- JSON sanitization (json_utils.py, 37L): NaN/Inf → null, numpy types → Python natives
- Adapted from Scriberr job model (attributed in code)

**Wave 2 — Audio preservation** (spec-wave-2-audio-preservation.md, 153L):
- Raw audio saved to /data/recordings/{job_id}.wav BEFORE transcription
- audio_path column populated immediately (even if transcription subsequently fails)
- /tmp usage eliminated for persistence path; finally block only deletes /tmp files
- POST /retry/{job_id}: re-transcribe from saved audio (202 async)
- Periodic cleanup: deletes old completed+delivered recordings (default 7 days)

**Wave 3 — Orphan recovery** (main.py, 874L):
- recover_orphaned_jobs() at startup marks stale processing jobs as failed
- periodic_orphan_sweep() every 30 min (guarded by job_tracker.is_busy())
- Crash-safe sentinel (Linux): setsid polling Electron PID, stops Docker container on crash
- Graceful shutdown: 120s drain, Docker stop_grace_period: 130s

### 4.9 In-App Update System — 17 Tech Specs

This is the project's largest single engineering investment:

| # | Spec | Purpose |
|---|------|---------|
| M1 | electron-updater integration | GitHub release checking |
| M2 | Banner UI | Changelog display, install button, dismiss |
| M3 | Safety gate | Prevent updates on incompatible platforms |
| M4 | Compatibility guard | OS/arch/glibc checks (compatGuard.ts, 489L) |
| M5 | Pre-install modal | Confirmation dialog with safety warnings |
| M6a | Safety error classification | Error taxonomy for failed updates |
| M6b | Safety hardening | Additional safety barriers |
| M7 | Platform-specific routing | AppImage/NSIS/DMG strategy selection |
| — | Installer cache hardening | Hostile-directory protection (403L) |
| — | Installer cache error classification | Cache error taxonomy |
| — | Remote host validation (server+renderer) | Request origin validation |
| — | Network paths install gate | Block installs from network paths |
| — | Banner resilience + dedup | Update notification hardening |
| — | Cache write hardening | Atomic file writes |
| — | Deferred bugs | Bug tracking |
| — | Test coverage closeout | Test completion |

**Key components:** updateManager.ts (473L), updateInstaller.ts (530L), installerCache.ts (403L), compatGuard.ts (489L), platformGate.ts (68L), checksumVerifier.ts (39L), sha256Lookup.ts (48L), UpdateBanner.tsx (679L), UpdateModal.tsx (454L), launchWatchdog.ts (88L).

### 4.10 Docker & Server Management

**dockerManager.ts** (3659L — LARGEST file in entire project): Docker/Podman auto-detection (containerRuntime.ts, 340L). 7 compose variants selected by platform+GPU. Compose V2 validation. Bootstrap progress via fs.watch on JSONL (startupEventWatcher.ts, 124L). GPU health detection (Metal→NVIDIA→Vulkan→CPU). Legacy-GPU image variant for Pascal/Maxwell. Server log tailing with ANSI color. Auth token auto-detection from Docker logs. Docker image fetch tracking.

**7 Docker compose variants:**

| Variant | Platform | GPU | Networking |
|---------|----------|-----|------------|
| Base | All | None | Bridge |
| linux-host | Linux | None | Host |
| desktop-vm | macOS/Windows | None | Bridge + ports |
| gpu | Linux | NVIDIA runtime | Host |
| gpu-cdi | Linux | NVIDIA CDI | Host |
| vulkan | Linux | AMD/Intel (sidecar) | Host |
| podman-gpu | Linux | NVIDIA (Podman) | Host |
| vulkan-wsl2 | Windows | AMD/Intel (exp.) | Bridge |

### 4.11 macOS Metal/MLX Server

**mlxServerManager.ts** (529L): macOS bare-metal server (no Docker). Manages Python 3.13 + MLX venv lifecycle. Two DMG artifacts: thin (~200MB) and bundled Metal (~3-5GB, Python+MLX inside .app). 4 MLX backends. Sortformer diarization.

### 4.12 whisper.cpp Vulkan Sidecar

Whisper.cpp runs as separate Docker container with HTTP API. Endpoint: POST /inference (multipart WAV), POST /load (model). Docker DNS: http://whisper-server:8080. /dev/dri/renderD128 passthrough. WhisperCppBackend (567L): WAV bytes in-memory (no temp files), 1s silence warmup, timeouts 300s/60s. No diarization. Windows: native whisper-server.exe auto-downloaded to %APPDATA%, host.docker.internal:8080. ~11 GGML models (large-v3, turbo, medium, small, q5_0/q8_0).

### 4.13 Profile System (Audio Notebook QoL Pack)

Documented in epics.md (918+L, 8 epics, 57 stories, 54 FRs, 55 NFRs):
- **epic-foundations** (11 stories): Profile CRUD + OS keychain + accessibility scaffold + profile snapshot durability
- **epic-import** (5 stories): SHA-256 audio dedup + dedup prompt modal (format-agnostic: raw bytes hash + normalized PCM hash)
- **epic-export** (7 stories): Filename template engine with placeholders ({date}, {title}, {recording_id}, {model}), live preview, plain-text export, download buttons, deletion semantics
- **epic-aliases-mvp/growth** (14 stories): Speaker renaming, alias substitution in exports/summaries/chat, diarization review keyboard contract
- **epic-auto-actions** (11 stories): Auto-summary + auto-export on transcription completion, retry with idempotency, status badges
- **epic-webhook** (7 stories): SSRF-safe outgoing webhook with durable delivery, 30-day retention
- **epic-model-profiles** (4 stories): Named STT model profiles, one-click switch

### 4.14 Configuration & Settings

**server/config.yaml** (765L): Central YAML with 10+ sections. Environment variable overrides for key settings. **dashboard/src/config/store.ts** (244L): electron-store with dot notation. Server host/port/TLS, runtime profile, model selections, appearance, shortcuts, folder watch paths, update preferences.

### 4.15 Audio Processing

- AudioWorklet mic capture (audioCapture.ts, 207L): PCM Int16, configurable sample rate
- FFmpeg utilities (ffmpeg_utils.py, 249L): SoX resampling, normalization, format conversion
- CUDA health check (audio_utils.py, 723L): GPU probe at startup, Error 999=unrecoverable
- Audio dedup: dual SHA-256 hashes (raw bytes + normalized 16kHz PCM) for format-agnostic matching

### 4.16 Testing Infrastructure

868+ backend tests (49 files in server/backend/tests/). Frontend Vitest + @testing-library/react. CI quality gates: TypeScript, UI contract, ESLint, Prettier. Pre-commit hooks: ruff, codespell, prettier, UI contract check. CodeQL security scanning. Day-1 test fixtures enforced by linter rules.

### 4.17 CI/CD

4 GitHub Actions workflows: CodeQL analysis, dashboard quality, scripts lint, release. Release: 4 parallel builds (Linux AppImage, Windows NSIS, macOS thin DMG, macOS Metal DMG) + GPG signing → draft GitHub Release. GPG opt-in via repository variable, dedicated signing subkey.


---

## 5. Key Code Patterns & Techniques

### 5.1 Persist-Before-Deliver (Concept Donor to S2B2S)

The single most valuable pattern for S2B2S. **Never deliver a transcription result before persisting it.** Three-layered implementation: (1) Job table — SQLite row at start, result written before WS delivery; (2) Audio preservation — raw .wav saved before transcription begins; (3) Recovery — client HTTP polling on disconnect, orphan sweep on startup.

**Key files:** job_repository.py (342L), websocket.py (704L), transcription.py (1557L), useTranscription.ts (410L), main.py (874L), audio_cleanup.py (87L), json_utils.py (37L)

**State machine:** create_job() → transcribe → save_result() BEFORE send_message() → mark_delivered() on success. If DB write fails: log CRITICAL, still attempt delivery (delivery is the priority). Client-side: store jobId from session_started, poll GET /result/{job_id} on WS close (3s intervals, 10 retries).

**S2B2S relevance:** S2B2S has SQLite history (history.rs). Adding this job-durability layer would prevent dictation/conversation data loss on frontend disconnect. The create_job→save→deliver→mark_delivered state machine is directly applicable. Also consider the dual-ref pattern from useTranscription.ts: jobIdRef (stable for closures) + jobId (reactive for render).

### 5.2 LAN GPU Pattern (Concept Donor)

Separate process/container for GPU work, main app handles UI and coordination. Docker container with GPU passthrough, sidecar for multi-vendor GPU support (whisper.cpp container with /dev/dri, WSL2 native .exe).

**Key files:** dockerManager.ts (3659L), containerRuntime.ts (340L), whispercpp_backend.py (567L), 7 docker-compose*.yml files

**S2B2S relevance:** S2B2S manages models in-process (Rust). Could consider sidecar pattern for whisper.cpp/llama.cpp models to isolate GPU crashes and support multi-vendor GPUs. The compose overlay pattern avoids combinatorial explosion.

### 5.3 Model Registry Pattern

Canonical model metadata in TypeScript (modelRegistry.ts, 545L), capability detection mirrored Python (capabilities.py, 91L) ↔ TypeScript (modelCapabilities.ts, 241L). Bidirectional maps for UI display ↔ backend ID conversion. Factory routing by name pattern (factory.py, 124L).

**S2B2S relevance:** Structured catalog for STT engines, TTS voices, brain models with capability metadata queryable before instantiation.

### 5.4 Differential Docker Updates

Bootstrap pattern: small base image + deps installed at first run into persistent /runtime/.venv volume. Two-fingerprint scheme (app code hash + lock hash). Three paths (cold, warm, delta). uv wheel cache survives rebuilds. Files: Dockerfile, bootstrap_runtime.py, entrypoint.py.

### 5.5 GPU Crash Resilience

CUDA health probe at startup (audio_utils.py, 723L). Error 999 = unrecoverable, sets _cuda_probe_failed flag, enters degraded mode. Transient errors get one retry. Downstream consumers check flag. Crash-safe sentinel (setsid) on Linux. GPU errors surfaced to frontend via /api/status with gpu_error + gpu_error_action fields.

### 5.6 Three-Tier State Management

1. React Query — Server data with staleTime per query (health: 5s, recordings: 30s, models: 60s)
2. Zustand — Ephemeral client state with selector pattern + useShallow for arrays
3. Component state — View-local UI

Transition matrix (useServerEventReactor.ts, 26L): Docker starting → poll health; server ready → invalidate model queries; model loaded → update capability flags; GPU error → degraded mode banner.

### 5.7 Startup Event Stream

Server emits JSON events to bind-mounted JSONL file (/startup-events/startup-events.jsonl). Dashboard reads via fs.watch (startupEventWatcher.ts, 124L). Event types: lifespan-start, lifespan-gpu, info-gpu, warn-gpu, warn-gpu-fatal, server-ready. No polling, no WebSocket — simple file-based IPC.

### 5.8 Diarization Review State Machine

Formal state machine: pending → in_review → completed → released. Keyboard-navigable UI with confidence scoring. State persists across restarts via recording_diarization_review table. Keyboard contract: Tab/Shift+Tab for turns, arrows for navigation, Enter accept, Esc skip, Space bulk-accept. Files: diarization_review_lifecycle.py (101L), DiarizationReviewView.tsx (378L).

---

## 6. Relation to S2B2S

| Aspect | TranscriptionSuite | S2B2S | Verdict |
|--------|-------------------|-------|---------|
| **Framework** | Electron + FastAPI (Python) | Tauri 2.x (Rust) + React/TS | S2B2S wins — lighter, faster, more secure |
| **STT engines** | 11 backends (all GPU vendors) | 3 engines via transcribe-rs | TS has more backends; S2B2S tighter Rust integration |
| **TTS engines** | None (transcription only) | 9 backends (Piper to Cartesia) | S2B2S uniquely voice-output capable |
| **Diarization** | PyAnnote + Sortformer, full review UI | Not implemented | TS far ahead |
| **Live mode** | WS-based VAD streaming, model swap | Push-to-talk dictation only | TS has live mode |
| **Durability** | 3-wave system (job+audio+recovery) | SQLite history (basic) | TS pattern to copy for S2B2S |
| **Brain/LLM** | LM Studio summarization, chat | Full streaming LLM with barge-in | S2B2S wins — richer brain |
| **Deployment** | Docker (7 compose variants) | Native Tauri binary | S2B2S simpler |
| **GPU acceleration** | CUDA, Vulkan sidecar, Metal/MLX | Vulkan (llama.cpp), Metal | Similar breadth; CUDA path is TS advantage |
| **Cross-platform** | Linux (primary), Windows, macOS | Windows (primary), macOS, Linux | Both cross-platform |
| **Update system** | 17 in-app update specs, GPG signing | Not implemented | TS massive investment |
| **Model registry** | TS catalog with Python mirror | Hardcoded model names | TS more structured |
| **Audio notebook** | Calendar, FTS5, export, AI summaries | Not implemented | TS has notebook; S2B2S is real-time focused |
| **Watch-folder** | chokidar + xxhash dedup | Not implemented | TS has batch processing |
| **License** | GPL-3.0 (copyleft) | MIT (permissive) | S2B2S can't copy TS code |
| **Architecture** | Client-server (Electron↔Docker) | Monolithic (Tauri Rust+React) | S2B2S simpler, less ops |

---

## 7. Harvest List (Features Worth Copying/Studying)

| Feature to harvest | Key file(s) | Effort | Value for S2B2S |
|-------------------|------------|--------|-----------------|
| Persist-before-deliver job system | job_repository.py (342L), websocket.py (704L) | M | Prevent data loss on dictation/conversation disconnect. S2B2S has SQLite — add job lifecycle |
| Model registry with capability metadata | modelRegistry.ts (545L), modelCapabilities.ts (241L) | M | Structured catalog for STT/TTS/Brain with feature flags |
| Factory pattern for engine backends | factory.py (124L), base.py (134L) | S | S2B2S has TtsBackend trait — extend to STT |
| GPU health probe + degraded mode | audio_utils.py (723L) | M | Check GPU/model availability, degrade gracefully |
| Startup event stream via file watch | startup_events.py (56L), startupEventWatcher.ts (124L) | S | Simple IPC for server→dashboard bootstrap progress |
| Watch-folder auto-processing | watcherManager.ts (313L), importQueueStore.ts (629L) | M | Batch audio processing from filesystem |
| Update safety gate pattern | compatGuard.ts (489L), platformGate.ts (68L) | M | Platform checks prevent bricked installs |
| Config YAML with env overrides | config.yaml (765L), config.py (394L) | S | Clean config layering: defaults → user → env |
| Diarization review keyboard contract | DiarizationReviewView.tsx (378L) | L | If S2B2S adds diarization, strong UX reference |
| Audio dedup (dual hash) | dedup_query.py (103L), sha256File.ts (26L) | S | Content-aware duplicate detection |

---

## 8. Known Issues, Caveats & Limitations

| Issue | Severity | Impact |
|-------|----------|--------|
| **GPL-3.0 license** | Critical for S2B2S | Cannot copy any code. Patterns only. If S2B2S incorporates GPL code, it MUST be GPL |
| **Docker dependency** | Medium | Requires Docker/Podman. Heavy base image (Ubuntu 24.04 + PyTorch). 7 compose variants add support burden |
| **Single-model-at-a-time** | Medium | Only one STT model in GPU. Live mode requires unload→load swap cycle |
| **No TTS** | N/A | Transcription-only — S2B2S uniquely has this |
| **No i18n** | Low | English-only UI despite transcribing 90+ languages |
| **macOS unsigned** | Low | Ad-hoc signed DMGs require xattr quarantine bypass. No Squirrel.Mac update channel |
| **VibeVoice OOM** | Medium | Default max chunk 60s (was 600s — CUDA OOM on 12GB). Never increase without testing |
| **NeMo /tmp req** | Low | NeMo backends require temp WAV files (no direct array transcription) |
| **Vulkan limitations** | Medium | No diarization, no alignment, no batched inference. Model switch = container restart |
| **Windows Dev Mode** | Low | Required for Windows builds (symlink creation in electron-builder) |
| **Intel Mac CPU-only** | Medium | MLX is Apple Silicon only. Docker on macOS runs CPU-only |
| **Single developer** | Medium | Bus factor of 1. "Vibecoded" origin — patterns may not follow best practices |
| **Electron bloat** | Medium | dockerManager.ts at 3659L is a god object. Electron heavier than Tauri |
| **No streaming TTS** | N/A | Transcription-only — S2B2S wins on voice output |

---

## 9. Strengths & Weaknesses

### Strengths

1. **Exceptional durability engineering.** The 3-wave persist-before-deliver system is the best-designed feature. Researched against AssemblyAI, Deepgram, OpenAI patterns. Clear invariants: DB write before WS delivery, audio saved before transcription, orphan recovery at startup. Client-side HTTP fallback (3s poll, 10 retries). Adapted from Scriberr with attribution.

2. **Comprehensive backend coverage.** 11 STT backends covering all GPU vendors (NVIDIA CUDA, AMD/Intel Vulkan, Apple Metal/MLX). Factory pattern with priority routing. VibeVoice backend at 1030 lines is the largest — all others follow the same STTBackend interface.

3. **Massive planning investment.** 185 bmad-output spec documents. 8-epic, 57-story breakdown with 54 functional requirements and 55 non-functional requirements. Key decisions documented as ADRs. Cross-feature constraints validated.

4. **In-app update system depth.** 17 separate tech specs covering safety gates, platform gating, checksum verification, error classification, hostile-directory hardening, and test coverage. Enterprise-grade update infrastructure built by a solo developer.

5. **Cross-platform CI/CD.** 4 parallel builds producing 4 artifacts (AppImage, NSIS, 2 DMG types) with GPG signing, CodeQL, and quality gates. The Metal DMG build injecting Python+MLX into .app bundle is creative engineering.

6. **Docker compose overlay pattern.** Elegant solution to combinatorial platform×GPU×network configurations. Base + overlays instead of monolithic files avoids 12+ variants.

7. **Diarization review system.** Keyboard-navigable review UI with confidence scoring, formal state machine, persistent state, speaker aliasing. Rare in open-source transcription tools.

8. **Model registry + capability mirroring.** Server-side Python + client-side TypeScript stay in sync. Per-model feature gating without backend round-trips.

9. **GPU crash resilience.** Health probe, degraded mode, retry-on-transient, crash-safe sentinel. GPU errors surfaced to frontend via /api/status.

10. **Startup event stream.** Creative use of bind-mounted JSONL file + fs.watch instead of polling/WebSocket for bootstrap progress.

### Weaknesses

1. **Electron bloat.** Heavier than Tauri, higher memory, larger downloads. dockerManager.ts at 3659 lines is an architectural smell (god object doing too much).

2. **Docker complexity.** 7 compose variants require user understanding of CDI, legacy-GPU toggles, WSL2 backends. Support burden.

3. **No TTS or conversation.** Transcription-only. S2B2S's dictation→brain→TTS pipeline is more complete as a voice assistant.

4. **Single-developer limitations.** Bus factor of 1. "Vibecoded" origin may mean some code quality is inconsistent despite planning rigor.

5. **English-only UI.** No i18n despite supporting 90+ languages for transcription. Deferred by design.

6. **Large codebase for function.** 50K+ lines for transcription+notebook. Some views are very large (AudioNoteModal.tsx 2714L, ServerView.tsx 2878L).

7. **No streaming TTS.** Not applicable to S2B2S's TTS pipeline — different domain focus entirely.

8. **macOS distribution friction.** Ad-hoc signed DMGs require terminal commands. No code signing certificate. No auto-update path for macOS.

---

## 10. Bottom Line / Verdict

TranscriptionSuite is a remarkably well-engineered solo project that punches far above its "vibecoded" weight class. Its **persist-before-deliver durability system** is the single most valuable concept for S2B2S — a 3-wave architecture (job persistence → audio preservation → client recovery) guaranteeing no transcription result is ever silently lost, researched against industry standards and implemented with clear invariants.

The project demonstrates patterns worth studying: Docker compose overlays avoiding combinatorial explosion, a model registry with cross-language capability mirroring, GPU crash resilience with graceful degradation, a startup event stream via filesystem watch, watch-folder auto-processing, and an in-app update system with 17 safety specs.

S2B2S cannot copy code (GPL-3.0), but can freely learn from the architecture: the create_job→save_result→deliver→mark_delivered state machine, factory routing for engine backends, the file-based startup event IPC, the watch-folder import queue, and the diarization review keyboard contract. TranscriptionSuite leads in durability, diarization, model variety, and update infrastructure. S2B2S leads in Tauri performance, TTS engines, conversation/brain pipeline, and deployment simplicity (no Docker required).

**Worth studying? Yes, extensively.** Focus on the persist-before-deliver docs (spec-wave-1 and spec-wave-2), the whisper.cpp Vulkan sidecar tech spec, the in-app update safety gates (M3-M7), and the model registry pattern. The Electron/Docker architecture is less relevant to S2B2S's Tauri-native approach, but the patterns within are gold.

---

*Analysis completed 2026-06-14. Sources read: 87 Python files, 175 TypeScript/TSX files, 28 documentation files, 185 bmad-output specification files, 7 Docker compose files, 4 CI/CD workflows. Total approximately 50,000+ lines of code across both parts. Key files read: CLAUDE.md (132L), project-context.md (324L/101 rules), README_DEV.md (1200+L), architecture-server.md (210L), architecture-dashboard.md (223L), integration-architecture.md (190L), source-tree-analysis.md (286L), data-models-server.md (156L), api-contracts-server.md (189L), epics.md (918+L), spec-wave-1-transcription-durability.md (183L), spec-wave-2-audio-preservation.md (153L), research-data-loss-prevention-2026-03-29.md (352L), tech-spec-whispercpp-vulkan-sidecar-archived-2026-03-26.md (351L), tech-spec-watch-folder-auto-processing.md (608L), config.yaml (765L), dockerManager.ts (3659L), main.py (874L).*
