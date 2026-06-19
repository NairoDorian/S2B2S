# S2B2S — File Descriptions Reference

> This file provides a 1-2 sentence description of every source file in the S2B2S project.
> It supplements the repomix output (which shows the tree/structure but not contents)
> with concise descriptions of what each file does.
>
> Run `bun run repomix:with-descriptions` to merge this into the repomix output.

## Rust Backend (`src-tauri/src/`)

| File | Description |
|------|-------------|
| `main.rs` | Binary entry point — parses CLI args (clap) and calls `lib::run()` |
| `lib.rs` | App setup — Tauri builder, manager init, specta binding export, signal handlers |
| `cli.rs` | 6 CLI flags: `--start-hidden`, `--no-tray`, `--toggle-transcription`, `--toggle-post-process`, `--cancel`, `--debug` |
| `settings.rs` | All config types, defaults, migration, and store logic (~1,800 lines monolith) |
| `actions.rs` | Shortcut action implementations (transcribe, converse, speak-selection, cancel) — 973 lines |
| `active_app.rs` | Windows-only foreground application detection via Win32 toolhelp snapshot |
| `apple_intelligence.rs` | macOS aarch64 Apple Intelligence LLM integration via Swift interop |
| `audio_feedback.rs` | Start/stop recording sound effects |
| `clipboard.rs` | Multi-method clipboard paste (Ctrl+V, Direct, Shift+Insert) with Linux Wayland support |
| `control_server.rs` | Local HTTP API on `127.0.0.1:43117` for remote control via axum |
| `crash_logging.rs` | Panic capture with full backtraces saved to `s2b2s-crash.log` |
| `input.rs` | Keyboard input simulation via enigo (macOS/Windows/Linux) |
| `llm_client.rs` | OpenAI-compatible non-streaming chat completions + model fetching |
| `overlay.rs` | Per-platform recording overlay (macOS NSPanel, Windows HWND_TOPMOST, Linux GTK Layer Shell) |
| `portable.rs` | Detects `.s2b2s-portable` marker file for self-contained config directories |
| `signal_handle.rs` | Unix signal handlers (SIGUSR1/SIGUSR2 → transcription toggle) |
| `transcription_coordinator.rs` | Single-threaded state machine (Idle→Recording→Processing) for recording pipeline |
| `tray.rs` | System tray icon, menu, and state updates |
| `tray_i18n.rs` | Tray menu i18n label generation |
| `utils.rs` | Platform detection, tray updates, paste, overlay management helpers |
| `wake_word.rs` | VAD-based wake word detection (fully connected via recorder.rs callback for local VAD energy check) |

### `managers/`

| File | Description |
|------|-------------|
| `managers/mod.rs` | Module declarations for all managers |
| `managers/audio.rs` | Recording lifecycle, mute/unmute, microphone mode, VAD integration (643 lines) |
| `managers/model.rs` | Model download and management from HuggingFace — 2,224 lines with 20+ hardcoded model entries (not JSON-driven) |
| `managers/transcription.rs` | STT engine management, idle watcher, GPU accelerator enumeration — 996 lines |
| `managers/transcription_mock.rs` | CI mock for testing without hardware |
| `managers/history.rs` | SQLite persistence of transcriptions and TTS with retention policies |
| `managers/continuous_voice.rs` | Hands-free conversation pipeline (VAD→STT→Brain→TTS→resume listening, 210 lines) |

### `brain/`

| File | Description |
|------|-------------|
| `brain/mod.rs` | Brain module declarations |
| `brain/client.rs` | SSE streaming chat client with `SentenceSplitter` (454 lines) |
| `brain/manager.rs` | Conversation state, turn history, abort/barge-in, sentence→TTS bridge |
| `brain/llama_manager.rs` | Llama.cpp server lifecycle, Gemma-4 model download, MTP speculative decoding (338 lines) |

### `tts/`

| File | Description |
|------|-------------|
| `tts/mod.rs` | `TtsBackend` trait + `Voice` struct definitions |
| `tts/manager.rs` | TTS orchestration: sanitize → paginate → synthesize → play (566 lines) |
| `tts/player.rs` | Streaming gapless audio playback via rodio `Sink` + `OutputStream` (205 lines) |
| `tts/pagination.rs` | UTF-8-safe text chunking at sentence boundaries with abbreviation awareness (509 lines) |
| `tts/fragment_queue.rs` | ⚠️ Dead code (306 lines) — pre-synthesis queue. Unused, preserved for future reference. Not connected to any pipeline. |
| `tts/clipboard_watch.rs` | Double-copy clipboard trigger for speak-selection (1.5s detection window) |
| `tts/audio_format.rs` | WAV to MP3/OGG/FLAC conversion |
| `tts/status.rs` | `WarmEngine` trait (warm/unload/status lifecycle methods) — Implemented by all local TTS backends (Piper, Kokoro, Kitten, Pocket) but the orchestration layer calls server utilities directly. |
| `tts/telemetry.rs` | Per-engine performance tracking (154 lines, partially implemented — `chars_per_ms` adaptive sizing infrastructure exists but `record()` is never called; telemetry state is registered in lib.rs but not wired) |
| `tts/local_tts_server.rs` | Generic lifecycle for Python TTS servers (Kokoro, Kitten, Pocket) — 648 lines |

### `tts/backends/`

| File | Description |
|------|-------------|
| `tts/backends/mod.rs` | Backend module declarations |
| `tts/backends/piper.rs` | Piper HTTP client — communicates with persistent Piper server |
| `tts/backends/piper_server.rs` | Piper persistent server lifecycle — CUDA auto-discovery, JIT warmup, idle unloader (788 lines) |
| `tts/backends/kokoro.rs` | Kokoro-82M ONNX TTS via Python HTTP server — 54 voices across 9 languages |
| `tts/backends/kitten.rs` | KittenTTS via Python HTTP server — ultra-lightweight (25-80MB models, 8 voices) |
| `tts/backends/pocket.rs` | Pocket TTS via Python HTTP server — voice cloning from WAV files |
| `tts/backends/sapi.rs` | Windows SAPI backend — local Windows speech fallback using windows-rs COM interop; returns voice lists and synthesizes text to WAV bytes |
| `tts/backends/openai.rs` | OpenAI cloud TTS integration |
| `tts/backends/elevenlabs.rs` | ElevenLabs cloud TTS with voice library |
| `tts/backends/cartesia.rs` | Cartesia Sonic cloud TTS (low-latency streaming) |

### `tts/sanitize/`

| File | Description |
|------|-------------|
| `tts/sanitize/mod.rs` | 5-stage text normalization pipeline orchestrator |
| `tts/sanitize/itn.rs` | Inverse Text Normalization — spoken numbers/dates → written form |
| `tts/sanitize/tn.rs` | Text Normalization — written form → spoken form for TTS |
| `tts/sanitize/markdown.rs` | Regex-based markdown stripping (bold, italic, links, code blocks) |
| `tts/sanitize/tts_normalize.rs` | Legacy TTS normalization rules |
| `tts/sanitize/cleanup.rs` | Regex-based final text scrub |

### `audio_toolkit/`

| File | Description |
|------|-------------|
| `audio_toolkit/mod.rs` | Audio toolkit module declarations |
| `audio_toolkit/constants.rs` | Sample rates and frame size constants |
| `audio_toolkit/text.rs` | Text processing utilities |

### `audio_toolkit/audio/`

| File | Description |
|------|-------------|
| `audio_toolkit/audio/mod.rs` | Audio module declarations |
| `audio_toolkit/audio/device.rs` | Audio device enumeration |
| `audio_toolkit/audio/recorder.rs` | cpal-based audio capture with resampling |
| `audio_toolkit/audio/resampler.rs` | rubato sample rate conversion (16kHz target) |
| `audio_toolkit/audio/visualizer.rs` | rustfft-based audio visualizer |
| `audio_toolkit/audio/noise_suppression.rs` | RNNoise integration via nnnoiseless crate |
| `audio_toolkit/audio/utils.rs` | WAV I/O, envelope extraction |

### `audio_toolkit/vad/`

| File | Description |
|------|-------------|
| `audio_toolkit/vad/mod.rs` | VAD module declarations |
| `audio_toolkit/vad/silero.rs` | Silero VAD via vad-rs (ONNX) |
| `audio_toolkit/vad/smoothed.rs` | Smoothed VAD output with configurable thresholds |
| `audio_toolkit/vad/triple_vad.rs` | 3-stage VAD cascade: RMS energy → RNNoise probability → Silero neural (92 lines) |

### `commands/`

| File | Description |
|------|-------------|
| `commands/mod.rs` | Cross-cutting Tauri commands (cancel, settings, log, export/import) |
| `commands/audio.rs` | Microphone, device, and VAD mode commands |
| `commands/brain.rs` | Brain ask, AI replace, and model fetching commands |
| `commands/discovery.rs` | Ollama and LM Studio auto-discovery on localhost |
| `commands/history.rs` | History CRUD, export, and retry commands |
| `commands/llama_server.rs` | Pre-compiled llama.cpp server management commands |
| `commands/models.rs` | STT model download, delete, and switch commands |
| `commands/system.rs` | System RAM usage retrieval command. Implemented for Windows, Linux, and macOS. |
| `commands/transcription.rs` | Unload timeout and load status commands |
| `commands/tts.rs` | TTS speak, stop, pause, resume, voices, and save commands |
| `commands/wake_word.rs` | Wake word start, stop, and status commands |

### `shortcut/`

| File | Description |
|------|-------------|
| `shortcut/mod.rs` | Dual-implementation shortcut manager (Tauri global-shortcut vs rdev KeyListener) |
| `shortcut/handler.rs` | Shortcut event dispatch (key press → action lookup) |
| `shortcut/key_listener.rs` | rdev-based global key listener implementation |
| `shortcut/tauri_impl.rs` | Tauri global-shortcut plugin implementation |

### `stt/`

| File | Description |
|------|-------------|
| `stt/mod.rs` | Multi-STT architecture documentation and module declarations (93 lines) |
| `stt/unified_parakeet.rs` | Parakeet Unified/EOU Python ONNX Runtime server lifecycle — spawn, health check, streaming STT via HTTP (292 lines) |
| `stt/multi_stt.rs` | Parallel multi-model transcription — runs 2-3 STT engines simultaneously, LLM merge (276 lines) |

### `overlay_fx/`

| File | Description |
|------|-------------|
| `overlay_fx/mod.rs` | Module declarations + `OverlayCapabilities` probe |
| `overlay_fx/trail.rs` | Spring-friction chain physics engine + Catmull-Rom spline interpolation (248 lines) |
| `overlay_fx/window.rs` | Transparent brain overlay webview — always-on-top, click-through (77 lines) |
| `overlay_fx/cursor_follow.rs` | ~30 Hz cursor position polling loop (40 lines) |
| `overlay_fx/placement.rs` | Bubble anchor math — quadrant-aware with DPI scaling (92 lines) |
| `overlay_fx/events.rs` | `OverlayPhase` 8-state machine + cursor/bubble typed events (61 lines) |
| `overlay_fx/capabilities.rs` | Per-OS GPU/cursor/layer-shell capability probe (10 lines) |
| `overlay_fx/commands.rs` | 3 Tauri IPC commands — probe, show, dismiss (29 lines) |
| `overlay_fx/native/mod.rs` | ⚠️ Placeholder — wgpu surface integration from CursorFX is pending (30 lines) |
| `overlay_fx/native/shader.wgsl` | WGSL shader for cursor trail ribbon + click ripple SDF. Ported from WebGPU CursorFX and TD Web Trail glow recipe. |

### `llama_server/`

| File | Description |
|------|-------------|
| `llama_server/mod.rs` | Llama server module declarations |
| `llama_server/manager.rs` | Pre-compiled llama.cpp binary management — download, GPU detection, server spawn/kill (430 lines) |

### `helpers/`

| File | Description |
|------|-------------|
| `helpers/mod.rs` | Helper module declaration |
| `helpers/clamshell.rs` | Laptop clamshell/lid-close mode detection |

### Python Servers (`src-tauri/`)

| File | Description |
|------|-------------|
| `kokoro_server.py` | HTTP server wrapping kokoro-tts — Direct API mode or CLI fallback (271 lines) |
| `kitten_server.py` | HTTP server wrapping KittenTTS — 8 voices, 25-80MB models (209 lines) |
| `pocket_server.py` | HTTP server wrapping pocket_tts — 8 voices + custom cloned voices (189 lines) |
| `unified_parakeet_server.py` | STT HTTP server — auto-detects sherpa-onnx vs manual ONNX, streaming + offline decode (703 lines) |

### Export Scripts (`temp_export_onnx/`)

| File | Description |
|------|-------------|
| `export_onnx_streaming.py` | Sherpa-onnx Parakeet Unified streaming exporter — identical to upstream PR #3575 |
| `download_nemo.py` | Downloads nvidia/parakeet-unified-en-0.6b checkpoint from HuggingFace |
| `export_unified.ps1` | PowerShell wrapper — runs export with 560ms latency preset |
| `export_unified.sh` | Bash wrapper — same as .ps1 for Linux/macOS |
| `README.md` | Export pipeline documentation |
| `notes.md` | Developer notes on the export process |

## React Frontend (`src/`)

| File | Description |
|------|-------------|
| `main.tsx` | Main window entry point — renders App component |
| `App.tsx` | Root component — onboarding flow (2 steps), main layout with sidebar + footer |
| `bindings.ts` | Auto-generated tauri-specta TypeScript bindings (~711 lines) |
| `App.css` | Global styles |

### `components/`

| File | Description |
|------|-------------|
| `Sidebar.tsx` | Settings navigation sidebar with section listing |
| `HerLoading.tsx` | 3D loading animation using Three.js (Her movie style) |
| `AccessibilityPermissions.tsx` | macOS accessibility permissions banner with polling check |

### `components/conversation/`

| File | Description |
|------|-------------|
| `ConversationView.tsx` | Chat UI with streaming tokens, 6-state voice mode, latency HUD (439 lines) |

### `components/footer/`

| File | Description |
|------|-------------|
| `Footer.tsx` | Status bar container — GPU VRAM, RAM, engine selectors, update checker |
| `GpuVramFooterIndicator.tsx` | Real-time GPU VRAM usage indicator with 1-second polling |
| `RamFooterIndicator.tsx` | System RAM usage indicator with 5-second polling |
| `BrainSelector.tsx` | Brain/LLM provider selection dropdown in footer |
| `TtsSelector.tsx` | TTS engine selection dropdown in footer |

### `components/model-selector/`

| File | Description |
|------|-------------|
| `ModelSelector.tsx` | STT model status indicator with dropdown |
| `ModelStatusButton.tsx` | Model status action button (download/switch/delete) |
| `ModelDropdown.tsx` | Model selection dropdown with language filter |
| `DownloadProgressDisplay.tsx` | Download progress bar with speed and ETA |

### `components/onboarding/`

| File | Description |
|------|-------------|
| `Onboarding.tsx` | First-run wizard — accessibility step + model selection step |
| `ModelCard.tsx` | Individual STT model card with download/delete actions |
| `AccessibilityOnboarding.tsx` | macOS-specific accessibility setup step |

### `components/shared/`

| File | Description |
|------|-------------|
| `ProgressBar.tsx` | Reusable progress bar component |

### `components/ui/` (16 reusable primitives)

| File | Description |
|------|-------------|
| `ui/Button.tsx` | 6 variants, 3 sizes, consistent CSS variable styling |
| `ui/Select.tsx` | react-select wrapper with creatable mode and `React.memo` |
| `ui/Dropdown.tsx` | Custom dropdown with click-outside detection and keyboard accessibility |
| `ui/ToggleSwitch.tsx` | Peer-based toggle with spinner-on-update and RTL awareness |
| `ui/Slider.tsx` | Range slider with dynamic gradient background and value display |
| `ui/SettingContainer.tsx` | Layout abstraction for settings controls (horizontal/stacked/tooltip) |
| `ui/SettingsGroup.tsx` | Section wrapper with title, description, bordered container |
| `ui/Tooltip.tsx` | Portal-based tooltip with viewport-edge position-aware flipping |
| `ui/Alert.tsx` | Status alert with 4 variants (error/warning/info/success) |
| `ui/AudioPlayer.tsx` | Full audio playback with RAF progress, drag-to-seek, blob URL cleanup |
| `ui/TextDisplay.tsx` | Monospace copyable text with click-to-copy feedback |
| `ui/PathDisplay.tsx` | Path display with open-in-file-manager button |
| `ui/Input.tsx` / `ui/Textarea.tsx` | Form inputs with compact/default variants |
| `ui/ResetButton.tsx` | Icon button for refresh/reset actions |
| `ui/Badge.tsx` | Label badge with 3 variants (primary/success/secondary) |

### `components/settings/`

| File | Description |
|------|-------------|
| `settings/general/GeneralSettings.tsx` | Shortcuts, microphone, audio device settings |
| `settings/general/ModelSettingsCard.tsx` | STT model settings card with language filter |
| `settings/advanced/AdvancedSettings.tsx` | 20+ advanced settings (startup, output, history, experimental) |
| `settings/advanced/AudioEnhancements.tsx` | RNNoise noise suppression settings |
| `settings/advanced/LongAudioRouting.tsx` | Long-audio model switching configuration |
| `settings/speech/SpeechSettings.tsx` | TTS engine, voice, speed, volume, sanitization, greeting settings |
| `settings/speech/TtsEngineSelector.tsx` | TTS engine dropdown with descriptions and badges |
| `settings/brain/BrainSettings.tsx` | LLM provider, system prompt, context turns, read-aloud toggle |
| `settings/brain/useBrainProviderState.ts` | Brain provider state management hook |
| `settings/brain/BrainProviderSelector.tsx` | Brain provider dropdown with base URL and API key |
| `settings/models/ModelsSettings.tsx` | Full STT model management with download progress |
| `settings/models/GpuVramMonitor.tsx` | GPU VRAM usage display with % bar and color coding |
| `settings/models/ModelSettingsCard.tsx` | Individual model card with download/delete/streaming toggle |
| `settings/llama-cpp/LlamaCppSettings.tsx` | llama.cpp server GPU detection, release list, download/activate |
| `settings/post-processing/PostProcessingSettings.tsx` | LLM post-processing config with API settings and prompt CRUD |
| `settings/post-processing/PostProcessingApiSettings.tsx` | Post-processing API key and endpoint settings |
| `settings/history/HistorySettings.tsx` | Transcription and TTS history with search, delete, type badges |
| `settings/about/AboutSettings.tsx` | Language selector, version, export/import, source, log dir |
| `settings/debug/DebugSettings.tsx` | Debug mode toggle, crash log path, log viewer |
| `settings/debug/LogViewer.tsx` | Real-time log viewer with search, level filter, copy, clear |
| `settings/PushToTalk.tsx` | Push-to-talk mode toggle |
| `settings/ShortcutInput.tsx` | Keyboard shortcut capture input |
| `settings/PasteMethod.tsx` | Paste method selector (Ctrl+V, Direct, etc.) |
| `settings/CustomWords.tsx` | Custom word correction editor |
| `settings/ExportImportSettings.tsx` | Settings export/import JSON |
| `settings/ShowOverlay.tsx` | Recording overlay visibility toggle |
| `settings/AlwaysOnMicrophone.tsx` | Always-on microphone mode toggle |

### `stores/`

| File | Description |
|------|-------------|
| `stores/settingsStore.ts` | Zustand store for all app settings with optimistic updates and rollback (739 lines) |
| `stores/modelStore.ts` | Zustand store for STT model lifecycle with download progress tracking (435 lines) |

### `hooks/`

| File | Description |
|------|-------------|
| `hooks/useSettings.ts` | Settings access hook wrapping settingsStore |
| `hooks/useProviderState.ts` | Shared provider state management (Brain + Post-Processing) |
| `hooks/useLlamaState.ts` | llama.cpp server state and VRAM monitoring |
| `hooks/useOsType.ts` | OS type detection hook (Windows/macOS/Linux) |

### `i18n/`

| File | Description |
|------|-------------|
| `i18n/index.ts` | i18next initialization with auto-discovery via `import.meta.glob` |
| `i18n/languages.ts` | 20-language metadata (names, flags, RTL flags) |
| `i18n/locales/en/translation.json` | English source translations (all other locales mirror this structure) |
| `i18n/locales/{ar,bg,cs,de,es,fr,he,it,ja,ko,pl,pt,ru,sv,tr,uk,vi,zh,zh-TW}/translation.json` | Translated UI strings for 20 languages |

### `lib/`

| File | Description |
|------|-------------|
| `lib/types/events.ts` | Tauri event type definitions |
| `lib/constants/languages.ts` | Language code constants and metadata |
| `lib/constants/models.ts` | Model-related constants |
| `lib/utils/rtl.ts` | Right-to-left layout utilities for Arabic/Hebrew |
| `lib/utils/keyboard.ts` | Keyboard event and shortcut utilities |
| `lib/utils/format.ts` | Number and duration formatting utilities |
| `lib/utils/modelTranslation.ts` | Model name translation and display helpers |

### `utils/`

| File | Description |
|------|-------------|
| `utils/dateFormat.ts` | Date formatting utilities |

### `overlay/`

| File | Description |
|------|-------------|
| `overlay/main.tsx` | Recording overlay window entry point (separate Tauri window) |
| `overlay/RecordingOverlay.tsx` | Frameless recording indicator with mic level bars and cancel button |

### `brain-overlay/` — Brain conversation overlay (separate Vite/webview entry)

| File | Description |
|------|-------------|
| `brain-overlay/main.tsx` | Brain overlay window entry point |
| `brain-overlay/BrainOverlayApp.tsx` | Main component — 8-phase state machine, reply bubble, metric chip, Esc dismiss (223 lines) |
| `brain-overlay/avatar/Avatar3D.tsx` | Three.js 3D avatar (197 lines) — 4-sided pyramid, glow orb, phase-reactive animations |

## Config & Build Files

| File | Description |
|------|-------------|
| `package.json` | Node dependencies, scripts (dev, build, lint, format, tauri, playwright) |
| `tsconfig.json` | TypeScript configuration with path aliases (`@/` → `./src/`) |
| `tsconfig.node.json` | TypeScript config for Node.js scripts and Vite config |
| `vite.config.ts` | Vite build config — multi-input (main + overlay), Tauri integration |
| `tailwind.config.js` | Tailwind CSS configuration with custom colors and themes |
| `eslint.config.js` | ESLint configuration enforcing i18next/no-literal-string rule |
| `playwright.config.ts` | Playwright E2E test configuration |
| `src-tauri/Cargo.toml` | Rust dependencies — 80+ crates (transcribe-rs, cpal, rodio, reqwest, etc.) |
| `src-tauri/tauri.conf.json` | Tauri app configuration — windows, security, permissions |
| `.prettierrc` | Prettier formatting configuration |
| `.prettierignore` | Prettier ignore patterns |
| `.gitignore` | Git ignore patterns |
| `flake.nix` / `flake.lock` | Nix flake for reproducible builds on Linux |
| `BUILD.md` | Platform-specific build instructions |
| `CONTRIBUTING.md` | Contributor guidelines |
| `CONTRIBUTING_TRANSLATIONS.md` | Translation contribution guide |
| `CHANGELOG.md` | Version history |
| `CRUSH.md` | Dev commands quick reference |
| `LLAMA_CPP.md` | llama.cpp server integration reference |
| `S2B2S_ANDROID_COMPANION.md` | Android thin-client companion PWA brainstorm (⚠️ superseded by android-port-plan.md) |
| `android-port-plan.md` | Full on-device Android STT→Brain→TTS architecture & implementation plan |
| `improvement-plan.md` | Streaming STT/TTS deep-dive analysis with P0–P3 prioritized roadmap |
| `reference_links.md` | Curated reference of 70+ open-source projects across 16 categories |
| `reference_github_links.md` | Curated list of 30+ STT/TTS/voice-related GitHub projects with Android section |
| `AGENTS.md` | AI coding assistant guide — architecture, conventions, commands, file tree |
| `CLAUDE.md` | AI assistant entry point — key docs, cleanup notes, file structure |
| `README.md` | Project overview, quick start, architecture diagrams |
| `S2B2S_REVIEW.md` | Comprehensive project analysis (1,857 lines) — architecture deep-dive, pipeline specs, roadmap |
| `LICENSE` | MIT License |
| `repomix.config.json` | Repomix bundler configuration for codebase snapshots |
| `repomix-file-descriptions.md` | This file — per-file descriptions for the repomix annotated output |

### Reference Documentation

| File | Description |
|------|-------------|
| `gemma_4_qat_mtp_e2b/MULTIMODAL.md` | Gemma 4 E2B multimodal input docs — image + audio, MTP optimization |
| `gemma_4_qat_mtp_e2b/REFERENCE.md` | Gemma 4 llama.cpp setup reference — commands, CUDA config, model files |
| `futuristic_analysis/00_README_START_HERE.md` | Master index — architecture overview, core principles |
| `futuristic_analysis/01_S2B2S_REVIEW.md` | Honest audit of current code — what works, what's missing |
| `futuristic_analysis/02_REFERENCE_PROJECTS.md` | Deep read of TD_Web_Trail and CursorFX reference repos |
| `futuristic_analysis/03_GPU_OVERLAY_ARCHITECTURE.md` | Two-track rendering (webview + native wgpu), per-OS matrix |
| `futuristic_analysis/04_CONVERSATION_MODE_2.md` | UX spec — state machine, event contract, reply bubble |
| `futuristic_analysis/05_VISION_AND_SCREEN_UNDERSTANDING.md` | Screen capture, multimodal ChatMessage, privacy invariants |
| `futuristic_analysis/06_AVATAR_SPEC.md` | 3D cybernetic avatar spec — 4 senses, Catmull-Rom visual language |
| `futuristic_analysis/07_IMPLEMENTATION_ROADMAP.md` | 5-phase plan with file-level tasks, risk register |
| `futuristic_analysis/08_TRANSPARENT_OVERLAY_IMPL_PLAN.md` | Concrete code-level plan bridging analysis to reference repos |
| `references_comparative_analysis_md/00_COMPARATIVE_ANALYSIS.md` | 23-project comparative analysis master document |
| `references_comparative_analysis_md/ARCHITECTURE_PATTERNS_CATALOG.md` | Reusable architecture patterns extracted from reference projects |
| `references_comparative_analysis_md/FORK_LINEAGE.md` | Fork lineage and project genealogy |
| `references_comparative_analysis_md/LICENSE_COMPATIBILITY.md` | License compatibility matrix across all reference projects |
| `references_comparative_analysis_md/ANALYSIS_TEMPLATE.md` | Template used for individual project reviews |
