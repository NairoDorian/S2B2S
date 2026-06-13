# Changelog

All notable changes to S2B2S are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/), and this
project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased] — S2B2S v0.10 (Conversation Evolution)

> **Status (June 2026):** 8 local TTS backends with RAM-persistent warm model lifecycle, voice barge-in for natural conversation interruption, Pocket TTS voice cloning, sentence streaming with word-count fallback, project-local Python venv, Android companion roadmap, system RAM/VRAM footer indicators, pre-compiled llama.cpp CUDA/Vulkan/CPU server with GPU offloading, and 20-turn conversation memory.

### Reference GitHub Links

- **`reference_github_links.md`** — Curated list of 21 STT, TTS, and voice-related open-source projects referenced by the S2B2S ecosystem. Includes Handy, Parler, AIVORelay, Parakeet, TranscriptionSuite, Whispering, speech-recognition (asrjs), transcribe-rs (cjpais), RealtimeSTT, onnx-asr, sherpa-onnx, copyspeak-tts, parrot, vox, pocket-tts-server, voirs, vibevoice-rs, TTS-Audio-Suite, voicebox, Cross_Platform_Rust_WebGPU_CursorFX, and TD_Web_Trail.

### Repomix Codebase Packaging

- **`repomix.config.json`** — Repomix bundler configuration for producing single-file codebase snapshots with file descriptions.
- **`scripts/repomix-with-descriptions.ts`** — TypeScript script generating annotated Repomix outputs with per-file commentary.
- **`S2B2S_repomix.txt`** / **`S2B2S_repomix_annotated.txt`** — Full and annotated Repomix codebase snapshots for AI context ingestion.
- **`repomix-file-descriptions.md`** — Per-file descriptions of the entire S2B2S codebase (348+ documented files).

### WGPU Native Overlay Shader

- **`overlay_fx/native/mod.rs`** — New platform-native overlay module with WGPU render pipeline initialization, surface setup, and platform-agnostic shader compilation.
- **`overlay_fx/native/shader.wgsl`** — WGSL compute/render shader for GPU-accelerated cursor trail effects (spring-friction physics, bloom glow, click ripple).

### Brain Overlay 3D Avatar

- **`brain-overlay/avatar/Avatar3D.tsx`** — 3D avatar component using Three.js with phase-reactive animations (Idle/Listening/Thinking/Speaking), orbital particle effects, and GPU-accelerated rendering.

### GPU/VRAM Monitor

- **`src/components/settings/models/GpuVramMonitor.tsx`** — Footer component displaying real-time GPU VRAM usage from llama.cpp server, with progress bar and memory pressure color coding.

### Docs & Housekeeping

- **AGENTS.md, README.md, BUILD.md, CLAUDE.md, CONTRIBUTING.md, CRUSH.md, S2B2S_REVIEW.md** — Updated cross-references, build instructions, and contributor guidance.
- **sponsor-images/** — Removed deprecated sponsor image assets.
- **models/TTS/** — Added Kitten TTS nano 0.8 pre-downloaded model files for offline first-run.

### Overlay Architecture — Tauri/OS-Native Toggle + WGPU Trail Foundation

- **`OverlayMode` toggle** — Settings → Overlay Window now lets users switch between `Tauri` mode (CopySpeak HUD style — `always_on_top` + `transparent` only) and `OsNative` mode (Handy style — NSPanel on macOS, Win32 `HWND_TOPMOST` on Windows, GTK layer-shell on Linux). Both modes share the same window label and event bus.
- **`OverlayWindowConfig`** — New settings struct controlling mode, position, width/height, opacity, corner radius, reply bubble toggle, and fade-out duration. Persisted via `tauri-plugin-store` with serde defaults.
- **`WgpuTrailConfig`** — New settings struct for the native GPU cursor trail: segments, spring stiffness (0.39), friction (0.5), width taper exponent (1.5), glow opacity, lazy-brush radius/friction, click ripple toggle. All with TD_Web_Trail–derived defaults.
- **`overlay.rs` refactored** — `create_recording_overlay` (both macOS and non-macOS variants) now respects `OverlayMode`. In Tauri mode: macOS skips NSPanel and uses plain `WebviewWindowBuilder`; Linux skips GTK layer-shell init; Windows skips `force_overlay_topmost()`. `calculate_overlay_position` uses configurable dimensions from `OverlayWindowConfig`. `hide_recording_overlay` uses configurable `fade_ms`.
- **`overlay_fx/` Rust module** — New crate-internal module at `src-tauri/src/overlay_fx/` containing:
  - `trail.rs` — Spring-friction chain physics engine + Catmull-Rom spline interpolation, ported from `TD_Web_Trail`. Lazy-brush dead-zone filter with non-linear damping. Trail system with idle-sleep after 2s of no movement. Includes unit tests.
  - `window.rs` — `brain_overlay` window creation (transparent, click-through `set_ignore_cursor_events`, always-on-top). Show/hide helpers. Windows re-asserts topmost via `SetWindowPos`.
  - `cursor_follow.rs` — ~30 Hz cursor polling loop using `enigo` (already a dependency). Quadrant-aware bubble positioning.
  - `placement.rs` — `compute_bubble_anchor()` with DPI scaling and monitor-aware quadrant flipping.
  - `events.rs` — `OverlayPhase` 8-state machine (Idle/Listening/Thinking/Seeing/Speaking/Done/Error/Hidden) + cursor/bubble payloads. All typed via specta.
  - `capabilities.rs` — Per-OS GPU/cursor/layer-shell capability probe.
  - `commands.rs` — 3 Tauri IPC commands: `overlay_fx_probe_capabilities`, `overlay_fx_show_conversation`, `overlay_fx_dismiss`.
- **`brain_overlay` frontend** — New multi-page Vite entry at `src/brain-overlay/`:
  - `index.html` — Transparent standalone HTML page.
  - `main.tsx` — React 19 root with i18n support.
  - `BrainOverlayApp.tsx` — State-driven UI: avatar placeholder (72px circle with phase-dependent emoji) + streaming reply bubble (glassmorphism with live cursor blink). Listens to `overlay:state`, `overlay:append`, `overlay:clear` events.
- **`vite.config.ts`** — Added `brain_overlay` entry to multi-page Rollup input.
- **Settings frontend tabs** — Two new sidebar tabs registered in `SECTIONS_CONFIG`:
  - **Overlay Window** (`Monitor` icon) — OverlayMode selector (Tauri/OS-Native dropdown) + Reply Bubble toggle.
  - **WGPU Trail** (`Zap` icon) — Enable toggle + Click Ripple toggle.
- **i18n keys** — 20+ new keys in `en/translation.json` under `settings.advanced.overlayWindow.*`, `settings.advanced.wgpuTrail.*`, and `sidebar.overlayWindow`/`sidebar.wgpuTrail`.
- **Typed bindings regenerated** — All new structs and commands exported to `src/bindings.ts` via `cargo test export_bindings`.

### Futuristic Analysis — Transparent Overlay Vision Documents

- **`futuristic_analysis/`** — 9 Markdown documents (00–08) analyzing the path from today's S2B2S to a full GPU transparent overlay with 3D avatar:
  - `00_README_START_HERE.md` — Master index, architecture overview, core principles.
  - `01_S2B2S_REVIEW.md` — Honest audit of current code: what works, what's missing.
  - `02_REFERENCE_PROJECTS.md` — Deep read of `TD_Web_Trail` and `Cross_Platform_Rust_WebGPU_CursorFX` (both cloned at `../` for live reference). Exact techniques to lift, including the DX12 OOM / Vulkan+N.VAPI fix.
  - `03_GPU_OVERLAY_ARCHITECTURE.md` — Two-track rendering (webview + native wgpu), per-OS technique matrix, DPI/click-through/perf budgets, failure ladder.
  - `04_CONVERSATION_MODE_2.md` — UX spec: state machine, event contract, reply bubble, quick actions, coexistence with recording pill.
  - `05_VISION_AND_SCREEN_UNDERSTANDING.md` — Screen capture (full + region), multimodal `ChatMessage` upgrade, cross-platform capture matrix, privacy invariants.
  - `06_AVATAR_SPEC.md` — 3D cybernetic avatar spec: 4 senses → pipeline signals map, Catmull-Rom visual language, skins system, reduced-motion accessibility.
  - `07_IMPLEMENTATION_ROADMAP.md` — 5-phase plan with file-level tasks, risk register, test matrix, performance targets.
  - `08_TRANSPARENT_OVERLAY_IMPL_PLAN.md` — Concrete code-level plan bridging analysis to actual patterns from the cloned reference repos.
- **Reference repos cloned** — `Cross_Platform_Rust_WebGPU_CursorFX` (Tauri V2 + wgpu transparent overlay, Vulkan + NVAPI fix) and `TD_Web_Trail` (spring-friction chain physics, 4-pass neon glow, Catmull-Rom splines, idle-sleep optimization) now live at the root `AZ/` directory alongside S2B2S.

### Model Directory Restructure (STT / Brain / TTS)

- **Master `models/` organization** — All model files now live in three category subdirectories: `models/STT/` (speech-to-text: Parakeet, Silero VAD, Whisper), `models/Brain/` (LLM: llama.cpp GGUF), `models/TTS/` (text-to-speech: Kokoro, Piper, Pocket, Kitten). Each engine has its own folder inside its category.
- **Rust path resolution updated** — `portable.rs` now exports `stt_models_dir()`, `brain_models_dir()`, `tts_models_dir()` helpers. `model.rs` uses STT subdir. `llama_manager.rs` uses Brain subdir. `kokoro.rs`, `piper_server.rs`, `pocket.rs`, `local_tts_server.rs` all use TTS subdir with legacy compat fallbacks.
- **Python server scripts updated** — `kokoro_server.py`, `kitten_server.py`, `pocket_server.py` now search `models/TTS/` first, with legacy `models/` flat directory as fallback for backward compatibility.
- **`models/download_models.*` rewritten** — Now accepts `--path` (custom target directory), `--model` flags (pocket, piper, kokoro, kitten, stt, brain, tts, all). Creates proper STT/Brain/TTS folder structure at target path. Supports `--setup-venv` and `--clean-venv` for one-shot setup. Dry-run mode available.
- **Pocket TTS model storage** — The `models/hub/` and `models/xet/` folders are HuggingFace auto-cache directories created by `pocket_tts`/`kittentts` Python packages when `HF_HOME` is set to `models/TTS/`. Pocket and Kitten models auto-download on first synthesis — no manual download needed.
- **`.gitignore` rewritten** — Per-category patterns for binary files. Only scripts in `models/` root are tracked in git.

### Dependency Checker & Script Cleanup

- **`scripts/check-deps.ts`** — New cross-dependency version checker covering Bun, Node, Rust, Cargo, Tauri CLI, JS packages (React, Vite, TypeScript, Tailwind, Zustand, etc.), and Python TTS packages (piper-tts, kokoro-tts, pocket-tts, kittentts, torch). Detects outdated deps and outputs a summary.
- **`scripts/check-deps.ts` overhauled** — Rust check now uses built-in `cargo update --dry-run --verbose` instead of external `cargo-outdated`, covering ALL Rust dependencies at ALL depths (including transitive). Distinguishes auto-updatable (`[Rust]`) from semver-constrained (`[Rust*]`) crates. JS check now parses `bun outdated` table output (the `--format json` flag is broken in Bun 1.3). Fixed Python pip version parser that captured trailing commas.
- **Dependency bumps** — JS: `@tailwindcss/vite` 4.3.0→4.3.1, `tailwindcss` 4.3.0→4.3.1, `lucide-react` 1.17.0→1.18.0, `eslint` 10.4.1→10.5.0. Rust: 15 crates updated in `Cargo.lock` (cc, js-sys, memchr, openssl, openssl-sys, rust_decimal, smallvec, wasip2, wasm-bindgen ×5, web-sys, zeroize). 5 transitive crates remain semver-constrained (cpal pinned for rodio compat, generic-array/toml family via GTK system-deps).
- **Removed unused scripts** — `check-latest.ts`, `find-bigints.ts`, `find-command-bigints.ts` removed (development-only tools, not referenced in CI or build). Kept `check-translations.ts`, `check-nix-deps.ts`, `setup_tts_venv.*`.

### About Tab — Expanded Acknowledgments

- **`AboutSettings.tsx` rewritten** — The Acknowledgments section now renders 6 comprehensive categories mapped from i18n keys, replacing the single Whisper.cpp entry. Data-driven component using `ACKNOWLEDGMENT_SECTIONS` array for easy future additions.
- **6 new acknowledgment sections** covering every project, model, library, and service used by S2B2S:
  - **Project Lineage & Forks** — Handy, AIVORelay, Parler, Parrot, CopySpeak, Vox, Parakeet-Realtime-Transcriber, TranscriptionSuite, Whispering. All 9 projects in the root AZ directory.
  - **STT Models** — Whisper.cpp, NVIDIA NeMo Parakeet, Silero VAD, Moonshine, Whisper, Breeze ASR, Canary, Sense Voice, Giga AM, Cohere.
  - **TTS Engines** — Piper, Kokoro, Kitten, Pocket + cloud (OpenAI, ElevenLabs, Cartesia).
  - **Brain & LLM** — llama.cpp, Gemma-4 + 8 cloud providers (OpenAI, Anthropic, Gemini, Groq, Cerebras, OpenRouter, Z.ai, AWS Bedrock).
  - **Key Rust Crates** — Tauri, rdev, vad-rs, transcribe-rs, nnnoiseless, cpal, rodio, rubato, text-processing-rs, enigo, rusqlite, specta.
  - **Python ML Ecosystem** — piper-tts, kokoro-tts, pocket-tts, kittentts, PyTorch, NumPy, SoundFile, ONNX Runtime.
- **All 20 language files synced** — New acknowledgment keys propagated to all locales; old `whisper`-only keys removed from non-English files.

### Model Path Consolidation (Local-First Storage)

- **`kokoro_server.py`** — Added `resolve_local_path()` fallback resolution. When `--model`/`--voices` args are missing, searches `models/kokoro/` relative to script dir and CWD before falling back to HuggingFace cache. Ensures the local `models/kokoro/kokoro-v1.0.onnx` + `voices-v1.0.bin` are always found.
- **`kitten_server.py`** — Added `--models-dir` argument and `resolve_local_models_dir()` helper. Points HuggingFace Hub downloads at the project-local `models/` directory instead of global HF cache.
- **`pocket_server.py`** — Same `--models-dir` + `resolve_local_models_dir()` pattern for local model storage. Sets `HF_HOME` for any HuggingFace-dependent packages.
- **Kokoro backend** (`kokoro.rs`) — Simplified `kokoro_model_args()` search order: canonical `models/kokoro/` path first, then CWD-based, then legacy. Removed hardcoded `%APPDATA%` path.
- **Model manager** (`model.rs`) — Added `project_local_models_dir` field that discovers `S2B2S/models/` in dev mode. `update_download_status()` and `get_model_path()` now resolve models from the local folder when they exist there.
- **Local TTS server** (`local_tts_server.rs`) — Added `resolve_local_models_dir()`, `local_models_dir_args()`, and sets `HF_HOME` env var on all spawned Python subprocesses. Kitten and Pocket engines now automatically receive `--models-dir` pointing to the local `models/` folder.

### Pocket TTS + Full Kokoro/Kitten Synthesis with RAM Persistency

- **Pocket TTS backend** — New `TtsEngine::Pocket` variant, `PocketBackend` (8 character voices: alba, marius, javert, etc.), dedicated `pocket_server.py` HTTP server for RAM-persistent runtime.
- **Kokoro synthesis** — Completed from skeleton. Now uses `kokoro_server.py` persistent HTTP server with `kokoro_tts` Python API (mode A) or CLI fallback (mode B). Model auto-discovery searches `models/kokoro/`, `CARGO_MANIFEST_DIR/kokoro/`, and common install paths.
- **Kitten synthesis** — Completed from skeleton. Now uses `kitten_server.py` persistent HTTP server with `kittentts` Python API. 8 voices: Bella, Jasper, Luna, Bruno, Rosie, Hugo, Kiki, Leo.
- **`local_tts_server.rs`** — Unified persistent HTTP server lifecycle for Kokoro, Kitten, Pocket (ported from CopySpeak `tts-perf-v2`). Per-engine state machine (Stopped→Starting→Ready), generation counter for safe abort, health polling with exponential backoff, warmup synthesis, idle watcher. Emits `local-tts-status-changed` Tauri events.
- **`WarmEngine` trait** — Implemented on `KokoroBackend`, `KittenBackend`, `PocketBackend` for app-startup pre-warming and engine-switch unload/reload.
- **Python encoding fix** — All local TTS server subprocesses now spawn with `PYTHONIOENCODING=utf-8` to prevent UnicodeEncodeError crashes.

### Conversation Memory & Brain Context

- **`context_turns` default changed to 20** — Brain now remembers 20 conversation turns by default (was 0 = stateless). Model receives full history of last N turns in context window.
- **"Clear" → "New conversation"** button with Eraser icon in ConversationView. Clears both Rust in-memory history and frontend messages.

### System RAM Footer Indicator

- **`get_system_ram` command** — New Rust backend command (`commands/system.rs`) using cross-platform system tools (PowerShell `Get-CimInstance` on Windows, `/proc/meminfo` on Linux, `sysctl`/`vm_stat` on macOS). Returns `total_mb`, `used_mb`, `free_mb`.
- **`RamFooterIndicator` component** — Shows real-time RAM usage percentage with green/yellow/red status dot and detailed hover tooltip. Updates every 5 seconds.

### Sentence Streaming (Fast TTFA)

- **3-fragment streaming pattern** — When `tts_shorten_first_chunk` is enabled: splits text at first period for fast first audio, splits second sentence for parallel synthesis, groups remaining text into one chunk. Replaced the old clause-boundary search with direct period-scanning.
- **Fast first sentence toggle** — New UI toggle in Speech Settings to enable/disable the streaming pattern.

### Engine Descriptions, Badges, Links & Test Button

- **Engine descriptions** — Each TTS engine now shows i18n description, badges (Offline/Free/Cloud/Paid/Freemium), and GitHub/API docs link in Speech Settings.
- **Test Engine button** — "Test Now" button in Speech Settings synthesizes a test phrase with the current engine.
- **Command Preview** — Local engines show their Python server command preview in a collapsible terminal block.
- **Footer engine list** — All 8 engines now visible in TtsSelector dropdown with per-engine status indicators.

### Shutdown & Process Cleanup

- **`Drop` impl on `LlamaManager`** — Ensures llama-server.exe is killed even on abnormal exit (taskbar close, panic, process kill).
- **`RunEvent::Exit` cleanup** — Now unloads Piper server, all local TTS servers (Kokoro/Kitten/Pocket), and llama-server on shutdown.
- **Model download resilience** — HTTP 416 Range Not Satisfiable on resume now auto-deletes stale partial file and restarts fresh.

### UI Fixes

- **Removed latency HUD** from ConversationView (endpoint/STT/token/audio labels above input box).
- **Fixed React hooks order crash** in `GpuVramMonitor` — `useTranslation()` moved before conditional return.
- **TTS synthesis ms in history** — `speak()` now passes `synth_total_ms` to `duration_ms` in `save_entry()`, visible as `{ms}ms` in History Settings.
- **Conversation TTS timing** — `speak_sentence()` now emits `tts:synth-done` with `ms` per sentence, showing `🔊 {ms}ms` on assistant messages.

### Vox-Inspired Improvements (Voice Barge-in, Word-Count Fallback, Voice Cloning)

- **Voice barge-in** — In continuous voice mode, VAD stays active during TTS playback. If user speaks, Brain is aborted, TTS is stopped, and the new utterance is captured immediately. Works like a real conversation — interrupt the assistant mid-sentence.
- **Word-count fallback** — Sentence streaming now splits at 12-word boundaries when no punctuation is found, preventing long run-on text from being synthesized as one chunk.
- **Pocket TTS voice cloning** — Import a 5-20 second WAV file and Pocket TTS clones that voice. `PocketBackend::import_cloned_voice()` copies WAV to persistent storage. Cloned voices appear in the voice list with 🎙️ prefix. New "Clone Voice" section in Speech Settings with WAV file picker.
- **Voice counts in footer** — Each engine in the TTS dropdown now shows voice count (e.g., "Kokoro — 54 voices").
- **Android Companion roadmap** — `S2B2S_ANDROID_COMPANION.md` with PWA architecture, 3-phase feature plan, WebSocket protocol design, references to 6 GitHub projects (NekoSpeak, speech-android, pocket-tts-unity, Kokoro-82M-Android, SherpaTTS, VoxSherpa-TTS), and brainstorm features.
- **Vox vs S2B2S comparison** — `S2B2S_VOX_COMPANION.md` with full architecture comparison, feature gap analysis, and improvement plan.

### Python Virtual Environment

- **`scripts/setup_tts_venv.ps1` / `.sh`** — Creates a project-local Python venv at `venv/` and installs all TTS dependencies: `piper-tts[http]`, `kokoro-tts`, `pocket-tts`, `kittentts`, `torch`, `numpy`, `soundfile`. No more system-wide `pip install`.
- **`resolve_venv_python()`** — New shared helper in `local_tts_server.rs`. Resolves Python from: project venv → app data venv → system fallback. Both Piper and local TTS servers use venv Python.
- **All model/voice paths now project-local** — Piper voices `~/piper-voices` fallback removed. Kokoro model search no longer scans `C:\Python3xx\Scripts`, `/usr/local/bin`, `/opt/kokoro-tts`. Everything resolves to `models/` subfolder or app data directory.
- **Removed dead code** — `resolve_python_command()` (now unused) and Python discovery helpers cleared from both server modules.

### Performance Metrics (Token/s, Latency, STT/TTS Timing)

- **Brain response metrics** — `brain:done` event now carries `tokens_per_sec` and `total_ms` from the llama.cpp server timing response. Displayed in the Conversation view as `t/s` and `ms` next to each assistant message, and in the Brain Settings test panel.
- **STT timing** — `brain:asked` event now includes `stt_ms` (audio-to-text latency). Rendered next to user messages in the Conversation view with a 🎤 icon.
- **TTS synthesis timing** — `tts:synth-done` now emits total synthesis `ms`. `tts:first-audio` event tracks time-to-first-audio (TTFA) for streaming TTS.
- **Brain client timing capture** — `BrainClient::stream_chat` now returns `BrainResult` with optional `BrainTiming` (tokens/sec, total ms) parsed from the SSE stream's `usage` or `delta.timings` fields.

### Pre-compiled llama.cpp Server Integration (Drop-in GPU Acceleration)

- **No more source compilation** — Removed the entire CMake-based `build_llama_cpp()` pipeline from `build.rs`. The app now downloads pre-compiled `llama-server` binaries directly from the [llama.cpp GitHub releases](https://github.com/ggml-org/llama.cpp/releases), supporting CUDA, Vulkan, and CPU backends across Windows, macOS, and Linux.
- **LlamaServerManager** — New Rust module (`src-tauri/src/llama_server/`) with full lifecycle management: fetches GitHub releases, downloads/installs/extracts archives, lists installed servers, and auto-selects the best available backend (CUDA > Vulkan > CPU). Stores binaries in `llama_cpp_servers/` within the app data directory for persistence.
- **GPU VRAM offloading** — The server launch command now passes `-ngl all` when a GPU-capable binary (CUDA or Vulkan) is detected, loading all model layers directly into GPU VRAM. CUDA runtime detection (`nvidia-smi` / `CUDA_PATH`) is automatic.
- **Auto-start on Brain activation** — `warmup()` in `BrainManager` now calls `ensure_server_running()` for llama_cpp before sending the warmup prompt. The llama.cpp server auto-starts at app launch when Brain is enabled (default: `true`) with the `llama_cpp` provider selected.
- **Brain status indicator** — The footer Brain dot now shows three states: orange pulsing (model loading into VRAM), green (ready), gray (disabled). Driven by `brain:llama-loading` / `brain:llama-ready` / `brain:llama-error` Tauri events.

### Llama.cpp Settings Tab

- **New "Llama.cpp" sidebar tab** — Full settings UI in `LlamaCppSettings.tsx` for managing pre-compiled server binaries. Shows detected GPU type, fetches available releases from GitHub, displays per-backend download buttons with progress, lists installed servers with version tags (e.g., `b9601`), and allows switching active backends.
- **Default model display** — Footer now shows "Gemma-4 2B (Local)" for the llama_cpp provider instead of the raw server alias.

### Developer Experience

- **VRAM log cleanup** — All `[VRAM]` info-level logs in `commands/models.rs` demoted to `debug!` to stop the per-second VRAM polling spam in `npm run tauri dev` console output.

### Fixes

- **Removed `--flash-attn on`** from the llama-server launch command to resolve compatibility with the CUDA pre-built binary.
- **Llama.cpp tab deduplication** — Filtered out `cudart-*` asset variants so only CUDA 12.4, CUDA 13.3, Vulkan, and CPU (x64) options appear. CUDA version is embedded in the backend string (`cuda-12.4`, `cuda-13.3`) so both are distinct entries.
- **Download/Remove/Use buttons** — Each option now always shows a Use button (downloads then activates if not yet downloaded), with Download/Remove toggling based on existence. Zip files are deleted after extraction.
- **Brain disable kills server** — Toggling Brain off when `llama_cpp` provider is active now terminates the llama-server process immediately.


### Refactoring & Code Quality

- **Shared `useProviderState` hook** — extracted common provider state management from `useBrainProviderState.ts` and `usePostProcessProviderState.ts` into `src/hooks/useProviderState.ts`. Eliminated ~200 lines of nearly identical code. Both hooks now delegate to the shared generic hook with a configuration object.
- **Deduplicated settings store** — extracted brain/post-process provider logic in `settingsStore.ts` into internal helpers (`_setProvider`, `_updateProviderSetting`, `_updateProviderBaseUrl`, `_fetchProviderModels`). Eliminated ~200 lines of duplicate provider management code.
- **`TooltipIcon` sub-component** — extracted from `SettingContainer.tsx` to eliminate 3x duplicated SVG tooltip icon markup across stacked/horizontal layouts.
- **`parseTimestamp` / `safeFormat` helpers** — extracted from `dateFormat.ts` to eliminate duplicate timestamp parsing and error handling in `formatDateTime` and `formatDate`.

### Bug Fixes

- **RecordingOverlay cleanup** — event listeners registered via `listen()` are now properly unregistered on component unmount. Previously the `setupEventListeners()` returned a cleanup function but the effect never called it, causing potential memory leaks.

### Type Safety

- **`Sidebar.tsx`** — changed `IconProps` index signature from `[key: string]: any` to `[key: string]: unknown` for better type safety with Lucide icons.
- **`settingsStore.ts`** — replaced `value as any` for LogLevel with `value as LogLevel` using explicit `LogLevel` type import.
- **`AccessibilityPermissions.tsx`** — replaced unsafe `as ButtonConfig` cast with proper null guard + early return.

### i18n Completion

- **`SpeechSettings.tsx`** — replaced 9 hardcoded strings with i18n keys (Greeting settings: group title, toggles, labels, descriptions, placeholders, noise scales).
- **`ConversationView.tsx`** — replaced 8 hardcoded strings with i18n keys (voice mode status labels, toggle titles, button text).
- Added all new i18n keys to `en/translation.json`.

### Dead Code Removal

- **`keyboard.ts`** — removed unused `normalizeKey` export. The `_osType` parameter in `formatKeyCombination` is retained (function signature matches callers).

### Barrel Exports

- **`ui/index.ts`** — completed barrel exports with all 17 UI components (added Alert, AudioPlayer, Badge, Button, Input, PathDisplay, ResetButton, Select).
- **`icons/index.ts`** — completed barrel exports with all 6 icon components (added ResetIcon, S2B2SIcon, S2B2STextLogo).

### Documentation Fixes

- **S2B2S_REVIEW.md** — replaced remaining `pulldown-cmark` references with regex-based stripping across 6 locations; corrected pipeline heading from "4-Pass" to "5-Stage"; synced roadmap (section 19) with current state — moved 6 completed features from Planned/Later to Completed; updated architecture limitations.
- **README.md** — fixed stale version number in verification example (`0.9.0`→`0.1.0`); removed extra blank lines.
- **CHANGELOG.md** — fixed missing `### Added` header; merged orphaned entries; noted `clipboard_ax.rs` removal lifecycle.
- **AGENTS.md** — updated backend and frontend architecture trees with missing files (`active_app.rs`, `apple_intelligence.rs`, `wake_word.rs`, commands detail, `helpers/`, `shortcut/` backends, `modelStore.ts`).
- **Module doc comments** — added `//!` docs to `active_app.rs`.

### Documentation Sync (June 2026)

- **LLAMA_CPP.md** — complete rewrite: removed all references to the old CMake-based `build_llama_cpp()` pipeline (removed in #v0.10). Now documents the pre-compiled `LlamaServerManager` architecture with auto-download from GitHub releases, CUDA/Vulkan/CPU backend auto-detection, GPU VRAM offloading with `-ngl all`, and the Llama.cpp settings management tab.
- **AGENTS.md** — backend tree updated with missing modules: `llama_server/`, `brain/llama_manager.rs`, `commands/llama_server.rs`, `managers/continuous_voice.rs`, `managers/transcription_mock.rs`, `tts/status.rs`, `tts/telemetry.rs`, `tts/audio_format.rs`, `tts/backends/piper_server.rs`, `audio_toolkit/bin/`, `audio_toolkit/constants.rs`, `audio_toolkit/text.rs`. Frontend tree updated: `hooks/useLlamaState.ts`, `hooks/useProviderState.ts`, `lib/constants/`, `utils/`. Text normalization heading fixed: "4-pass" → "5-Stage".
- **README.md** — architecture diagram updated with `continuous_voice.rs`, `status.rs/telemetry.rs`, `llama_manager.rs`. "Why S2B2S?" and Default Stack table now list pre-compiled llama.cpp as the primary Brain option alongside Ollama/LM Studio.
- **S2B2S_REVIEW.md** — roadmap (section 19): added 7 completed features (pre-compiled llama.cpp server, settings tab, performance metrics, GPU VRAM indicator, log viewer console, footer status indicators, hands-free auto-listen). File tree (section 18): added `llama_server/`, `brain/llama_manager.rs`, `commands/llama_server.rs`.
- **BUILD.md** — project structure overview refreshed with `llama_server/`, `brain/` directories, restored `resources/`/`Cargo.toml`/`tauri.conf.json` entries.
- **CRUSH.md** — file structure reference updated with `llama.cpp/` and `llama_server/` references.
- **CONTRIBUTING.md** — managers listing updated to include `continuous_voice`.

### Second-Pass Verification & Corrections

- **README.md** — component count corrected: "100+ components" → "90+ components"; settings file count "~50 files" → "60+ files". Removed `tts-rs` from Core Libraries table (not a compiled dependency — Kokoro synthesis pending integration).
- **AGENTS.md** — Key Files Reference now includes `LLAMA_CPP.md`. Technology stack table: Kokoro entry updated from "tts-rs in-process" to "skeleton". Backend tree: kokoro.rs comment updated.
- **S2B2S_REVIEW.md** — hooks section now includes `useLlamaState.ts`. Kokoro Backend Details updated to note synthesis pending `tts-rs` integration (voice listing works). Platform matrix: Kokoro row updated. Key Files Quick Reference now includes `llama_server/manager.rs`.
- **Full file inventory verification** — confirmed 88 `.rs` files, 91 `.tsx` files, 28 `.ts` files, 9 GitHub Actions workflows. All documented modules verified on disk.

### Third-Pass Consistency Audit

- **README.md** — **critical fix**: text normalization pipeline order corrected from `ITN → Custom Words → TN → Markdown Strip` to `ITN → Custom Words → Markdown Strip → TN → Regex Cleanup` (TN must run after markdown stripping, not before). WarmEngine lifecycle expanded to include "Stopped" state (5 states, was showing 4). Roadmap: added missing "Hands-free auto-listen / continuous voice" (✅) and "Engine-switch cleanup" (now 🚧 In Progress, matching S2B2S_REVIEW.md).
- **CLAUDE.md** — backend summary now includes `llama_server/` directory.
- **BUILD.md** — hooks list updated to include `useLlamaState`.
- **CONTRIBUTING.md** — backend listing now includes `llama_server/` subsystem.
- Verified all 20 i18n locale files match CONTRIBUTING_TRANSLATIONS.md language table. Confirmed version consistency: package.json/Cargo.toml both `0.1.0`, CHANGELOG working title `v0.10`. Confirmed Kokoro and Kitten backends are both skeletons (synthesis pending).

### Code Cleanup

- **Cargo.toml cleanup** — removed commented-out `[[bin]]` section for CLI; removed trailing blank line before target-specific dependencies.
- **Module documentation** — added module-level doc comments (`//!`) to `actions.rs`, `input.rs`, `audio_feedback.rs`, `commands/mod.rs`, `helpers/clamshell.rs` for clearer codebase navigation.
- **Documentation sync** — updated roadmap in README.md to reflect all completed features (AI Replace, Latency HUD, wake word, save-to-file, waveform HUD, auto-discovery), removed stale entries.
- **README.md polish** — streamlined features list and added clarity to pipeline descriptions.

### Code Quality & Cleanup

- **Rust clippy cleanup** — resolved all 44 clippy warnings across the backend: deprecated `rodio::DeviceTrait::name` → `description()`, `map_or(false, ...)` → `is_some_and(...)`, `map_or(true, ...)` → `is_none_or(...)`, `contains_key` + `insert` → `Entry::Vacant` API, `return Ok(())` → `Ok(())`, redundant closures → direct function references, manual saturating arithmetic → `saturating_sub`, collapsible `if` statements collapsed, manual `Default` impls replaced with `#[derive(Default)]`, empty doc comments fixed, `write!` → `writeln!`, test modules repositioned after function definitions, `io_other_error` patterns modernized, and `needless_borrow` references simplified. Architecture-level `#[allow(clippy::too_many_arguments)]` added to 4 long-parameter-sig functions.
- **Rust formatting** — `cargo fmt` applied project-wide; trailing whitespace in `piper_server.rs` removed.
- **ESLint i18n cleanup** — resolved all 35 `i18next/no-literal-string` errors across the frontend: added 16 new i18n keys (`conversation.latency.*`, `footer.brain*`, `footer.brainTitle`, `footer.tts*`, `gpuVram.*`, `debug.logViewer.*`, `ui.slider.resetToDefault`, `settings.speech.playGreeting`) and applied `eslint-disable-next-line` for icon/technical-unit literals.
- **Settings enums** — 5 enums (`ModelUnloadTimeout`, `ClipboardHandling`, `AutoSubmitKey`, `TypingTool`, `WhisperAcceleratorSetting`, `OrtAcceleratorSetting`) now use `#[derive(Default)]` with `#[default]` annotations instead of manual `impl Default`.

### Added

- **GPU VRAM usage indicator** — green (<75%), yellow (75-90%), red (>90%) with hover tooltip showing used/total MB. Polls every 5s via `get_active_gpu_vram_status` command.
- **Log viewer console** — developer log viewer in Debug settings with level filter, search, auto-refresh (2s), manual refresh, copy to clipboard, and clear logs. Backed by `get_recent_logs` / `clear_logs` commands.
- **Footer status indicators** — STT, Brain, and TTS indicators collapsed to emoticon + title + status dot (🎙️ STT 🟢, 🧠 Brain 🟢, 🗣️ TTS 🟢). Full model/voice details visible on hover tooltip and in their respective dropdown popovers.
- **Documentation cleanup** — removed all remaining `IMPROVEMENT_PLAN.md` references from CONTRIBUTING.md, AGENTS.md, CRUSH.md, S2B2S_REVIEW.md, and PULL_REQUEST_TEMPLATE.md. Removed Sponsors section from README. Marked RAM-persistent warm model lifecycle as ✅ Complete in roadmap.

**Conversation & Brain:**

- **Speakable-output system prompt** — separate `speakable_output_prompt` appended when `read_aloud` is ON, instructs LLM to answer conversationally for listening. Editable in settings.
- **TTS toggle in conversation UI** — 🔊/🔇 button in ConversationView header toggles `read_aloud` per-chat in real time. Keyboard shortcut `Ctrl+Shift+T`.
- **AI Replace Selection** — select text anywhere, press `Ctrl+Alt+Space`, speak an instruction — the Brain rewrites the selection in place. Uses dedicated system prompt: "Output ONLY the rewritten text — no preamble, no explanation."
- **Latency HUD** — per-stage timestamps (EP: endpoint, STT, TTFT: time-to-first-token, TTFA: time-to-first-audio) emitted as `brain:latency` events. Color-coded display in conversation view (green < target, yellow < 2x, red > 2x).
- **Sentence splitter optimization** — `split_at_clause_boundary()` at 60 chars for fast TTFA. Prefers strong boundaries (`.`, `)`, `]`) over weak (`,`) with 10-char bonus. Wire `tts_shorten_first_chunk` setting through to `TtsManager::speak()`.
- **Brain config extensions** — new settings: `conversation_mode` (push_to_talk/toggle/hands_free), `endpoint_preset` (snappy/balanced/patient), `headphone_mode`, `auto_listen` (auto-rearm after reply).
- **Ollama/LM Studio/llama.cpp model discovery** — `discover_local_brains()` command probes `:11434/api/tags` (Ollama), `:1234/v1/models` (LM Studio), `:8080/v1/models` (llama.cpp). Returns discovered servers with model lists, zero-config detection.

**TTS Ecosystem:**

- **Save-to-file MP3/OGG/FLAC** — `tts/audio_format.rs` converts WAV via ffmpeg shell-out. `tts_save_format` setting. `tts_save_to_file` command saves most recent TTS audio to user-chosen path.
- **Warm model unload timeout** — `WarmEngine` trait implemented on `PiperBackend` (`warm()`, `unload()`, `status()`). `start_idle_watcher()` in `piper_server.rs` checks `ModelUnloadTimeout` every 15s, auto-unloads on idle expiry. Tray "Unload Model" action wired.
- **Piper server health monitor** — already robust with generation-based cancellation, stdout/stderr drain threads, CUDA warm-up synthesis, health polling with exponential backoff 100→1600ms.
- **Waveform HUD** — `AmplitudeEnvelope` struct + `extract_envelope()` in `audio_toolkit/utils.rs`. 32-bar RMS envelope extracted per TTS fragment and emitted via `tts:waveform` event.
- **Cross-platform selection capture** — sentinel-based clipboard capture writes unique sentinel before Ctrl+C, reliably distinguishes "no selection" from "clipboard unchanged". Fallback for all platforms.
- **Cross-platform double-copy trigger** — Windows: `GetClipboardSequenceNumber`. macOS: `NSPasteboard.changeCount` via AppKit FFI. Linux: content-based polling with xclip/wl-paste. Graceful degradation on unsupported platforms.

**Wake Word Detection:**

- **VAD-based activity detection** — `WakeWordDetector` uses RMS energy threshold (0.03) with 3-frame debounce (~150ms). Zero model files needed. ~2s ring buffer auto-cleared.
- **sherpa-onnx KWS prepared** — integration code written (init_kws/feed_kws in git history). Blocked on Windows CRT linking: `sherpa-onnx-sys` uses `/MT` static CRT while `transcribe-rs`/`whisper` uses `/MD` dynamic CRT. To enable: add `sherpa-onnx = "1.13.2"` to `Cargo.toml` and download KWS model files to `models/wake_word/`.
- **Privacy-first design** — feature defaults OFF, requires explicit consent. Audio processed entirely on-device, never saved. 👁 tray indicator when active.
- **Wake word commands** — `wake_word_start`, `wake_word_stop`, `wake_word_set_config`, `wake_word_status` Tauri commands. `WakeWordConfig` in settings (enabled, keyword, threshold, show_indicator).

**Recording & Audio:**

- **Recording auto-stop** — silence watchdog with configurable duration. `set_recording_auto_stop` command, `auto_stop_enabled` + `auto_stop_duration_secs` in `AudioRecordingManager`.
- **Hands-free auto-listen** — auto-rearms mic after Brain+TTS finishes with 250ms grace period to avoid capturing room reverb. Controlled by `brain.auto_listen` setting.
- **Always-on mic for wake word** — `enable_wake_word()` in `AudioRecordingManager` activates always-on microphone stream when wake word detection is running.

**Developer & Diagnostics:**

- **Better sentinel clipboard** — `capture_selection_text()` now writes unique sentinel before Ctrl+C, allowing reliable detection of "no selection" vs "clipboard unchanged".

### Changed

- **Dependencies Upgrade** — Safely updated backend and frontend dependencies to their latest compatible versions, including Tauri v2.11.2, once_cell v1.21.4, rusqlite v0.40.1, rusqlite_migration v2.6.0, chrono v0.4.45, regex v1.12.4, flate2 v1.1.9, sha2 v0.11.0, clap v4.6.1, tauri-plugin-dialog v2.7.1, and @types/node v25.9.3.
- **Specta v2 Type Mapping** — Converted `duration_ms`, `id`, and `timestamp` type overrides from `f64`/`Option<f64>` to `u32`/`Option<u32>` in `HistoryEntry` and `HistoryUpdatePayload` to resolve TypeScript compilation issues with nullable fields.
- **Auto-stop watch parameters** — Changed parameter type for `set_recording_auto_stop` from `u64` to `u32` to comply with Specta's BigInt restrictions.
- **Kokoro backend** — replaced `parking_lot::Mutex` with `std::sync::Mutex`, removed external dependency.
- **PiperBackend** — implements `WarmEngine` trait with `warm()`/`unload()`/`status()` methods. Tracks `last_used` timestamp for idle timeout.
- **TTS manager** — `speak()` now respects `tts_shorten_first_chunk` setting, splits first clause near 60 chars via `split_at_clause_boundary`.
- **Brain manager** — `ask()` concatenates `speakable_output_prompt` when `read_aloud` is ON. Emits `brain:latency` events with per-stage timestamps.
- **ConversationView** — latency HUD bar shows color-coded EP/STT/TTFT/TTFA. TTS toggle button in header. `ai_replace_selection` import.
- **Continuous voice** — 250ms grace re-arm, respects `auto_listen` setting.

### Fixed

- **Frontend Type Safety** — Resolved TypeScript compiler errors in `ConversationView.tsx` (added null-checks to `settings.brain`) and `SpeechSettings.tsx` (provided `?? null` fallback for `greeting.engine`).
- **sherpa-onnx CRT conflict** — removed `sherpa-onnx` dependency due to `/MT` static CRT vs. `/MD` dynamic CRT conflict with `whisper-rs-sys` on Windows. VAD-based wake word retained; KWS integration code preserved in git history. To re-enable: add `sherpa-onnx = "1.13.2"` to `Cargo.toml` and download KWS model files to `models/wake_word/`.
- **Specta TS bindings export** — softened to warning (no longer crashes debug builds) while root cause is investigated.

### Added Files

- `src-tauri/src/commands/discovery.rs` — Ollama/LM Studio/llama.cpp auto-discovery
- `src-tauri/src/commands/wake_word.rs` — wake word commands
- `src-tauri/src/tts/audio_format.rs` — MP3/OGG/FLAC conversion
- `src-tauri/src/wake_word.rs` — VAD-based wake word detector (KWS-ready architecture)
- `src-tauri/src/clipboard_ax.rs` — cross-platform selection capture (subsequently removed; code folded into `clipboard.rs`)

**Documentation Overhaul:**

- **S2B2S_REVIEW.md** — new 91KB comprehensive project analysis covering 21 sections: architecture deep dive, all 3 pipelines, STT/TTS/Brain subsystems, TripleVAD, text normalization (4 passes), audio toolkit, model management, settings, frontend architecture, i18n, CI/CD, project lineage/donor map, dependency analysis, complete file tree, roadmap, known issues, platform matrix, and 6 ASCII diagrams. Serves as reference for non-tech users, developers, and AI agents.
- **README.md** — complete rewrite with table of contents, default stack table, all pipeline diagrams, text normalization pass tables, full architecture section, CLI/env vars reference, sponsor section.
- **AGENTS.md** — full architecture tree visualization, frontend+backend structure maps, technology stack table, i18n details, code style, platform notes, key files reference.
- **BUILD.md** — macOS Intel ONNX Runtime setup, env vars table, CI/CD workflow table, project structure overview.
- **CLAUDE.md** — expanded from single line to full entry point doc referencing all key project files.
- **CONTRIBUTING.md, CONTRIBUTING_TRANSLATIONS.md, CRUSH.md** — all updated with current state, commands, and architecture info.
- **PR template** — softened feature-freeze language to focus on priorities rather than rejection.
- **Bug report template** — added crash log path and debug mode instructions.

**Core STT / VAD:**

- **Triple VAD as default** — 3-stage voice activity detector (RMS energy gate → RNNoise voice probability → Silero VAD) is now the default for all modes. Provides better noise rejection at ~2ms additional latency per frame.
- **RNNoise voice probability threshold** — new `rnnoise_voice_threshold` setting (0.05–0.9, default 0.2) with slider in Advanced → Audio Enhancements. Controls how aggressively RNNoise filters non-speech audio.

**Text Normalization Pipeline (ITN + TN + Markdown):**

- **ITN (Inverse Text Normalization)** via `text-processing-rs` (Apache 2.0) — spoken-form ASR output normalized to written form: "two hundred thirty two" → "232", "january fifth" → "January 5, 2025". Applied post-STT in both dictation and conversation pipelines.
- **TN (Text Normalization)** via `text-processing-rs` — written-form text normalized to spoken form before TTS: "$5.50" → "five dollars and fifty cents", "123" → "one hundred twenty three".
- **Markdown stripping** (regex-based, replaced `pulldown-cmark`) — headings, bold, lists, links, code blocks, HTML entities all converted to natural spoken form before TTS.

**TTS Backends (7+ engines):**

- **Kokoro-82M TTS backend** — in-process ONNX engine via `tts-rs` with 54 voices across 9 languages (US/UK English, Spanish, French, Hindi, Italian, Japanese, Portuguese, Mandarin). Voice-per-language auto-selection, `tts_workers` setting for worker pool support.
- **Kitten TTS backend** — ultra-light ONNX engine (8 English voices, 3 model sizes). Skeleton ready for Python CLI adapter.
- **Windows SAPI backend** — zero-download fallback engine always available on Windows.
- **Cloud TTS backends** — OpenAI, ElevenLabs, and Cartesia integration via pooled `reqwest::Client`.

**TTS Engine Lifecycle & Performance:**

- **WarmEngine trait** — lifecycle states (`Stopped → Loading → WarmingUp → Ready`) for engines that support pre-warming. Engine status surfaced to UI.
- **TTS performance telemetry** — per-engine `chars_per_ms` tracking drives adaptive fragment sizing. Fast engines get larger fragments; slow engines get smaller ones.
- **Kokoro/Kitten worker settings** — `tts_workers` (auto-tuned from CPU count, 1–4 range) and `tts_shorten_first_chunk` (default ON, clause-split for fast time-to-first-audio).
- **TTS entries saved to history** — all spoken text (double-copy trigger, speak-selection shortcut, test button) persisted to History as `tts`-type entries with engine name.

**Speech Output (TTS) Subsystem:**

- **Read Aloud** — select text anywhere, press `Alt+Shift+R` / `Option+Shift+R` to hear it spoken. Press again to stop. Clipboard contents preserved.
- **Double-copy trigger** — copy the same text twice within 1.5s to hear it spoken (Windows detection; other platforms degrade gracefully).
- **Speaking HUD overlay** with stop control and "Speech" settings section (engine, voice, speed, volume, Piper setup, toggles, test button).
- **Streaming gapless playback** — fragment _i+1_ synthesized while _i_ plays. UTF-8-safe sentence pagination.
- **Piper HTTP server** — warm, persistent local TTS (model stays in RAM; child stdio drained for long-session reliability).
- **Noise Scale / Noise W Scale sliders** — Piper HTTP `noise_scale` and `noise_w_scale` parameters (0–1.5 range) in greeting settings with reset-to-default buttons.
- **French Piper TTS voices** — all 7 fr_FR voices (gilles, mls, mls_1840, siwis, tom, upmc).

**The Brain (Streaming LLM):**

- **Streaming LLM subsystem** — OpenAI-compatible SSE streaming client (Ollama default, LM Studio/cloud via base URL + key). Multi-turn memory with configurable context window.
- **Conversation mode** — sentence-by-sentence read-aloud while the reply streams. Barge-in: new question (or Stop) aborts previous turn and speech.
- **Talk to the Brain** shortcut (`Alt+Shift+B` / `Option+Shift+B`) — record → transcribe → Brain → spoken streamed reply.
- **Conversation view** — live transcript of spoken/typed turns with streaming tokens, plus text input fallback. "Brain" settings section (endpoint, model picker, system prompt, memory, read-aloud toggle).

**UI & UX:**

- **Her-style 3D loading animation** — Three.js animated tube geometry (lissajous curve) with ring-reveal transition. Minimum 3-second display; startup greeting plays at 0.9x speed.
- **Complete retheme** — pure black (#000000) background with purple (#7c3aed) + gold (#f59e0b) accents across all UI (icons, sliders, overlays, recording bars). Dark mode media query removed.
- **New app icon and logo** — icon for taskbar/titlebar/tray; logo for README and sidebar menu.
- **All platform icons regenerated** — taskbar, tray, and window icons from new source. Tray state icons updated to 64x64.
- **History enhancements** — "Delete All" button; STT/TTS type badges per entry; model name and transcription duration (ms) displayed. Database schema extended with `entry_type`, `model_name`, `model_info`, `duration_ms` columns.

**Developer & Diagnostics:**

- **Crash logging** — panics captured to `s2b2s-crash.log` in the app log directory with full backtraces and thread names.
- **Debug mode toggle in Advanced settings** — previously only via `Ctrl+Shift+D` shortcut; now has UI toggle alongside crash log path display.
- **MSRV declared** — minimum Rust version 1.87 in `Cargo.toml`.
- **Typed bindings regeneration** — `cargo test export_bindings` works headlessly (no GUI launch needed).
- **i18n** — UI keys for all new features across all 20 locales (ar, bg, cs, de, en, es, fr, he, it, ja, ko, pl, pt, ru, sv, tr, uk, vi, zh, zh-TW).

**STT — Python ONNX Runtime 1.26 Server:**

- **Parakeet Unified EN 0.6B model** — two new STT models: `parakeet-unified-en-0.6b-fp32` (2.4 GB, accuracy 0.88) and `parakeet-unified-en-0.6b-int8` (633 MB, accuracy 0.85). Both English-only RNN-T architecture from NVIDIA NeMo, registered in `ModelManager`.
- **Python onnxruntime 1.26 server** (`unified_parakeet_server.py`) — dedicated HTTP server for Parakeet Unified inference using the latest ONNX Runtime 1.26 (benefits from the Nemotron Conformer MHA fusion optimizer in v1.25). Keeps encoder + decoder ONNX sessions loaded in RAM with SentencePiece tokenizer. Full RNN-T greedy decoder with 128-bin Slaney mel spectrogram, pre-emphasis, and per-feature normalization — matching parakeet-rs exactly.
- **Rust STT backend module** (`src/stt/unified_parakeet.rs`) — manages the Python server lifecycle (spawn, health check with exponential backoff, transcription via HTTP POST of raw float32le audio bytes, graceful kill on drop). Python venv resolution follows the existing TTS priority chain (project venv → app-data venv → system Python).
- **Model download paths** — fp32 and int8 variants served as `.tar.gz` archives from CDN, consistent with existing model distribution pattern.

**STT — Streaming Transcription (EOU 120M):**

- **Streaming RNNT endpoints** (`/stream_start`, `/stream_feed`, `/stream_end`, `/stream_status`) — the Python server now supports incremental audio chunk processing with stateful decoder (LSTM states persist between chunks). Feeds audio in 250ms chunks, returns progressive partial results with EOU detection.
- **EOU model auto-detection** — the transcription pipeline detects the EOU 120M model by its HuggingFace repo URL and routes through the streaming API. The Unified model continues using the offline `/transcribe` endpoint.
- **`transcription-partial` event** — emitted by the Rust backend during streaming transcription whenever text changes. Enables real-time word-by-word overlay display in future frontend work.

**STT — Multi-STT Pipeline (Parallel Transcription + LLM Merge):**

- **Multi-STT orchestrator** (`src/stt/multi_stt.rs`) — runs 2–3 STT models in parallel via `std::thread::spawn`. Each thread loads its own engine independently (transcribe-rs or Python server), transcribes the audio, and returns text. All 9 engine types supported.
- **LLM merge** — results formatted as a transcriptions block and fed through the existing post-processing pipeline with a configurable prompt template (`{transcriptions}` placeholder). Falls back to the primary model's result when post-processing is disabled.
- **Settings** — `multi_stt_enabled` (bool), `multi_stt_models` (Vec), `multi_stt_prompt` (String with default merge prompt). Integration point in `actions.rs` runs multi-STT before the existing post-process step.
- **Architecture** documented in `src/stt/mod.rs` — streaming model for real-time feedback + 1–2 backup models for accuracy + LLM merge.

**STT — EOU Streaming Toggle + Silence Gate:**

- **`eou_streaming_enabled`** setting (default: `true`) — toggles between streaming API (`/stream_start` → `/stream_feed` → `/stream_end`) and offline `/transcribe` for the EOU 120M model. Disabling streaming uses a single HTTP call with no partial events.
- **Silence gate on chunk feeding** — each 250ms audio chunk is checked for RMS energy before being sent to the streaming model. Chunks below the `0.002` RMS threshold (matching TripleVAD's energy gate) are skipped. Prevents background noise and silence gaps from triggering the model or causing premature `<EOU>` emission. Applied in both main transcription path and multi-STT parallel path.

**STT — HuggingFace Direct Downloads:**

- **Multi-file HF downloads** — `ModelInfo` gains `hf_repo` + `hf_files` fields (hidden from frontend). `download_huggingface_model()` downloads individual ONNX/tokenizer/config files from HuggingFace repos with retry (3 attempts) and progress reporting.
- **All 5 Parakeet ONNX models** now download directly from HuggingFace (eschmidbauer/unified + ysdede/eou-120m) — no CDN dependency.

### Changed

- **Default VAD mode** changed from `"silero"` to `"triple"` for all modes (dictation, conversation, push-to-talk).
- **Text sanitizer pipeline reordered** — markdown stripping runs first, then TN (text-processing-rs), then legacy regex-based TTS normalization, then artifact cleanup.
- **Always-On Microphone toggle moved** from Debug settings to General → Sound section for easy discovery.
- **All dependencies updated to latest** — Tauri 2.11, rodio 0.22, rubato 3.0, reqwest 0.13, rusqlite 0.40, `windows` 0.62, specta rc.25, transcribe-rs 0.3.11. React 19, Vite 8, TypeScript 6, zod 4, ESLint 10, i18next 26. `cpal` pinned to 0.17.
- **Removed `parakeet-rs` crate** — replaced with Python onnxruntime 1.26 server for Parakeet Unified model inference. The Rust `ort` crate (locked to 2.0.0-rc.12, ONNX Runtime ~1.20) cannot be upgraded to 1.26 yet; Python path bypasses this bottleneck.
- **EOU 120M model uses streaming pipeline** — detected by HuggingFace repo URL, routes through `/stream_start` → chunked `/stream_feed` → `/stream_end` with `transcription-partial` events. Unified model stays on offline `/transcribe` endpoint. Toggleable via `eou_streaming_enabled` setting.
- **Multi-STT and EOU streaming settings** added to `AppSettings`: `multi_stt_enabled`, `multi_stt_models`, `multi_stt_prompt`, `eou_streaming_enabled`.
- **Silence-gated streaming** — audio chunks below 0.002 RMS energy are skipped before feeding to the streaming decoder, preventing noise-induced EOU hallucinations.
- **Python venv now includes onnxruntime >= 1.26.0 and sentencepiece** for the Unified Parakeet STT server.
- **Overlay threading simplified** — removed `run_on_main_thread` wrapping; overlay executes directly on calling thread.
- **Removed COM initialization** from TTS audio player background thread.
- **Removed dynamic Piper server reload** — `change_tts_config` no longer restarts the persistent server on voice/CUDA changes.
- **Renamed** `warmup_speak_out_loud` → `play_startup_greeting`, `speak_warmup_bytes` → `play_raw`.

### Fixed

- **TripleVAD voice threshold** was hardcoded at `0.2` in `managers/audio.rs`; now reads from user-configurable `rnnoise_voice_threshold` setting.
- **Greeting text now editable** — fixed `onChange` handler using raw event object instead of `e.target.value`.
- **Removed pitch from greeting settings** — Piper HTTP API doesn't support pitch; replaced with proper Piper noise params.
- **Removed redundant test speak sample section** — "Play Greeting" button already serves this purpose.
- **TTS entries not appearing in history** — double-copy, speak-selection, and test button spoken text now persisted after successful synthesis.
- **Windows test executables** — `build.rs` now embeds Common-Controls v6 manifest into test binaries (fixes `STATUS_ENTRYPOINT_NOT_FOUND` after dependency upgrade).
- **TTS sentence ordering** — FIFO sentence queue via `mpsc::channel` consumer thread; sentences synthesized and appended in correct order, fixing out-of-order playback when short sentences synthesized faster than longer earlier ones.
- **Tokio runtime panic** — channel-based approach isolates Piper backend synthesis in a dedicated `std::thread`, avoiding the "Cannot drop a runtime in a context where blocking is not allowed" panic from synchronous calls.

### Removed

- **IMPROVEMENT_PLAN.md** — deleted the improvement plan file.
