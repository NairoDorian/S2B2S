# Parler — Fork of Handy

> Repo: `Melvynx/Parler` · HEAD: `53b4ad5` · License: MIT · Author: Melvyn Malherbe · Platforms: macOS, Windows, Linux
> Nature: **fork-of-Handy** (true git fork, shared history, semi-actively tracks upstream)
> Role for S2B2S: surgical fork model — proves rebrand path, contributes pause/resume, long-audio model switching, settings export/import, crash logging, multi-monitor overlay fixes, settings bindings backfill, Gemini provider adapter pattern.

---

## 1. What Parler Is

Parler ("to speak" in French) is a rebranded fork of Handy (the speech-to-text desktop app by cjpais). It was created by Melvyn Malherbe to deliver macOS-quality releases with a handful of power-user features layered on top. Parler maintains the same core architecture as Handy — Tauri 2.x with Rust backend and React/TypeScript frontend — but adds cloud transcription via Gemini, pause/resume recording, settings export/import, crash logging, and various reliability improvements.

The project shares git history with Handy (fork point at commit `b76febd`), with **66 commits unique to Parler** and **109 files changed** (+3,442 / -1,479 lines net, excl. lockfiles). The Rust-side delta alone is ~30 files, +1,469 / -99 lines. Parler performed at least one mid-life upstream merge and is approximately 33 commits behind Handy main.

---

## 2. Tech Stack

### 2.1 Frontend

| Layer | Choice | Purpose |
|-------|--------|---------|
| Framework | React 18, TypeScript 5.6 | Settings UI, onboarding, overlay |
| Styling | Tailwind CSS 4 | Utility-first styling |
| State | Zustand 5 (+ immer) | Settings and model state |
| Build | Vite 6 | Frontend bundling |
| i18n | i18next + react-i18next | 20 languages |
| Icons | Lucide React 0.542 | UI icons |
| Fonts | Geist Pixel webfont | Branding (Parler-specific addition) |
| Forms | react-select 5 | Dropdowns |

### 2.2 Backend / Core

| Layer | Choice | Purpose |
|-------|--------|---------|
| Framework | Tauri 2.10.2 (patched) | Desktop app shell |
| Audio I/O | cpal 0.16 | Cross-platform audio capture/playback |
| Audio Playback | rodio (cjpais fork) | Sound feedback |
| STT Engine | transcribe-rs 0.3.3 (whisper-cpp + onnx) | Local STT: Whisper, Parakeet, Moonshine, SenseVoice, GigaAM, Canary |
| VAD | vad-rs (cjpais fork) | Silero VAD with SmoothedVad wrapper |
| Shortcuts | rdev (rustdesk fork) + handy-keys 0.2.4 | Dual implementation (Tauri + HandyKeys) |
| Clipboard/Input | enigo 0.6.1 | Text input simulation |
| Resampling | rubato 0.16.2 | Audio resampling to 16kHz |
| WAV I/O | hound 3.5.1 | WAV file writing |
| HTTP | reqwest 0.12 | API calls (post-processing, Gemini) |
| Storage | rusqlite 0.37 (bundled) + rusqlite_migration 2.3 | History DB with migrations |
| Store | tauri-plugin-store 2.4.1 | Settings persistence |
| IPC | tauri-specta 2.0.0-rc.22 | Typed command/event bindings |
| CLI | clap 4 | CLI argument parsing |
| Text Processing | strsim 0.11, natural 0.5, ferrous-opencc 0.2.3 | Word correction, Chinese conversion |
| Spectrum | rustfft 6.4.0 | Audio visualization |

### 2.3 Key Dependencies (Parler-Specific)

| Dependency | Purpose | Where |
|-----------|---------|-------|
| `base64` 0.22 | Gemini audio upload (WAV-to-base64) | `gemini_client.rs` (243 l) |
| Geist Pixel webfont | Rebranding visual identity | `package.json` / frontend |
| `tauri.dev.conf.json` | Side-by-side dev flavor ("ParlerDev") | `src-tauri/tauri.dev.conf.json` (20 l) |
| macOS signing + notarization | CI release pipeline | `.github/workflows/release.yml` |
| thin LTO on Windows | Full LTO caused transcription crash | `Cargo.toml` profile.release |

---

## 3. Architecture & Source Map

```
Parler/
├── src-tauri/                              Rust backend
│   ├── Cargo.toml                   (125 l)  Parler crate, deps, thin LTO, custom Tauri patch
│   ├── tauri.conf.json              (70 l)   App config, identifier "com.melvynx.parler"
│   ├── tauri.dev.conf.json          (20 l)   Dev flavor config "ParlerDev" (NEW)
│   ├── build.rs                             Tray translations auto-gen
│   ├── resources/default_settings.json       Default settings seed
│   └── src/
│       ├── main.rs                  (18 l)   CLI parse + launch
│       ├── lib.rs                   (606 l)  App setup, window mgmt, tray, manager init, 80+ commands
│       ├── cli.rs                   (29 l)   Clap CLI args
│       ├── settings.rs              (1037 l) Settings schema, defaults, backfill, redacted secrets
│       ├── actions.rs               (862 l)  Shortcut actions: TranscribeAction, CancelAction, post-processing
│       ├── signal_handle.rs         (38 l)   SIGUSR1/SIGUSR2 for Linux control
│       ├── gemini_client.rs         (243 l)  Gemini generateContent client (NEW — deprecated)
│       ├── crash_logging.rs         (80 l)   Panic capture with backtraces to file (NEW)
│       ├── llm_client.rs            (296 l)  Multi-provider LLM client with structured output (UPGRADED)
│       ├── overlay.rs               (495 l)  Recording overlay: create, position, show/hide, multi-monitor (FIXED)
│       ├── utils.rs                 (65 l)   Cancel operation, Wayland/KDE detection
│       ├── clipboard.rs             (687 l)  Paste via clipboard/direct/external-script, Linux tool chain
│       ├── input.rs                 (122 l)  Enigo wrapper, cursor position, paste key combos
│       ├── audio_feedback.rs        (184 l)  Sound effects (rodio): start/stop, custom themes
│       ├── tray.rs                  (294 l)  System tray: icon state, dynamic menu, model submenu
│       ├── tray_i18n.rs             (34 l)   Build-time tray menu localization
│       ├── portable.rs              (166 l)  Portable mode: marker file detection, Data/ dir redirect
│       ├── apple_intelligence.rs    (84 l)   macOS Apple Intelligence FFI (NEW)
│       ├── transcription_coordinator.rs (244 l)  Single-threaded lifecycle coordinator
│       ├── managers/
│       │   ├── audio.rs             (610 l)  AudioRecordingManager: open/close, mute, pause, clamshell
│       │   ├── model.rs             (1276+ l) ModelManager: 14 models, download, SHA256, extraction
│       │   ├── transcription.rs     (866 l)  TranscriptionManager: 7 engines, idle watcher, long-audio switch
│       │   ├── transcription_mock.rs (93 l)   CI mock for testing without ML deps
│       │   └── history.rs           (759 l)  HistoryManager: SQLite, migrations, cleanup, dual-text
│       ├── commands/
│       │   ├── mod.rs               (210 l)  Core commands: settings export/import, Enigo init, shortcuts init
│       │   ├── audio.rs             (322 l)  Audio device commands, microphone permissions, pause toggle
│       │   ├── gemini.rs            (2 l)    Deprecated stub
│       │   ├── history.rs           (201 l)  History CRUD, retry transcription, reprocess
│       │   ├── models.rs            (220 l)  Model listing, download, delete, switch
│       │   └── transcription.rs     (40 l)   Model load status, unload
│       ├── shortcut/
│       │   ├── mod.rs               (1150 l) Shortcut manager: dual impl, binding CRUD, 30+ setting commands
│       │   ├── handler.rs           (145 l)  Shortcut event dispatch: coordinator, pause, cancel (double-press)
│       │   ├── handy_keys.rs        (600 l)  HandyKeys impl: manager thread, recording mode, key capture
│       │   └── tauri_impl.rs        (240 l)  Tauri global-shortcut impl with validation
│       ├── audio_toolkit/
│       │   ├── constants.rs         (1 l)    WHISPER_SAMPLE_RATE = 16000
│       │   └── audio/
│       │       └── recorder.rs      (596 l)  AudioRecorder: cpal stream, resampler, VAD, visualizer
│       └── helpers/clamshell.rs     (86 l)   macOS clamshell/laptop detection (ioreg + pmset)
├── src/                                      React/TypeScript frontend
│   ├── App.tsx                      (293 l)  Main app: onboarding flow, permissions, debug toggle
│   ├── bindings.ts                          Auto-generated Tauri bindings (tauri-specta)
│   ├── components/settings/
│   │   ├── about/AboutSettings.tsx  (95 l)   Version, donate, source code, export/import
│   │   ├── post-processing/PostProcessingSettings.tsx (403 l)  Provider system UI (REWRITTEN)
│   │   ├── general/LongAudioModelSettings.tsx (75 l)  Long-audio model + threshold dropdowns (NEW)
│   │   ├── ExportImportSettings.tsx (77 l)   Import/export UI (NEW)
│   │   └── ... (35+ settings components total)
│   ├── components/onboarding/               Model selection + permissions onboarding
│   ├── components/model-selector/            Model list/dropdown/download progress
│   ├── components/ui/                        Reusable UI primitives (Button, Dropdown, ToggleSwitch, etc.)
│   ├── stores/
│   │   ├── settingsStore.ts         (596 l)  Zustand store: settings, bindings, model options
│   │   └── modelStore.ts                    Model state store
│   ├── lib/
│   │   └── permissions.ts           (26 l)   macOS accessibility polling-safe check (NEW)
│   ├── i18n/locales/                         (20 language files)
│   └── overlay/                              Recording overlay window entry
├── .github/workflows/              (10 files) CI/CD pipelines including build-windows.yml (NEW)
└── package.json                     (68 l)   "parler-app" v0.8.6
```

---

## 4. Feature Inventory

### 4.1 STT Pipeline (Inherited + Extended)

| Feature | Description | Files | Status |
|---------|-------------|-------|--------|
| Whisper models (Small/Medium/Turbo/Large) | Local GGML Whisper inference | `transcription.rs` (866 l), `model.rs` (1276+ l) | Inherited |
| Parakeet V2/V3 | ONNX-based STT, CPU-optimized | `transcription.rs`, `model.rs` | Inherited |
| Moonshine Base/Streaming | Ultra-fast English STT | `transcription.rs`, `model.rs` | Inherited |
| SenseVoice | Multi-language (zh/en/ja/ko/yue) | `transcription.rs`, `model.rs` | Inherited |
| GigaAM v3 | Russian STT | `transcription.rs`, `model.rs` | Inherited |
| Canary 180M Flash / 1B v2 | Multi-language STT with translation | `transcription.rs`, `model.rs` | Inherited |
| Gemini cloud transcription | `generateContent` with audio inline_data (WAV→base64) | `gemini_client.rs` (243 l) | **ADDED → now deprecated** |
| Long-audio model switching | Duration-threshold-based engine swap | `transcription.rs` lines 460-500, `LongAudioModelSettings.tsx` (75 l) | **ADDED** |
| Engine panic recovery | `catch_unwind` → drop engine, clear model ID | `transcription.rs` lines 549-704 | Inherited |
| Model idle watcher | 10s poll, auto-unload after configurable timeout | `transcription.rs` lines 96-160 | Inherited |
| Download resume + SHA256 verify | Range-request resume, blocking SHA256 on spawn_blocking | `model.rs` lines 900-1260+ | Inherited |
| Custom Whisper model discovery | Auto-detect `.bin` files in models dir | `model.rs` lines 779-898 | Inherited |
| Accelerator settings | Whisper (Auto/CPU/GPU) + ORT (Auto/CPU/CUDA/DirectML/ROCm) | `transcription.rs` `apply_accelerator_settings()` | Inherited |

### 4.2 Voice Activity Detection

| Feature | Description | Files |
|---------|-------------|-------|
| Silero VAD | ONNX-based voice probability at 0.3 threshold, 30ms frames | `vad/silero.rs` (52 l) |
| SmoothedVad | Prefill (15 frames) + hangover (15 frames) + onset (2 frames) smoothing | `vad/smoothed.rs` (105 l) |
| Audio visualization | rustfft spectrum (16 buckets, 400-4000Hz vocal range), sent to frontend via `mic-level` event | `audio/recorder.rs` `run_consumer()` |

### 4.3 Audio Recording

| Feature | Description | Files |
|---------|-------------|-------|
| Always-On / On-Demand microphone modes | Always-On keeps stream open continuously; On-Demand opens/closes per recording with optional lazy close | `managers/audio.rs` (610 l) |
| Lazy stream close | 30s idle timeout before closing mic stream after recording | `managers/audio.rs` `schedule_lazy_close()` |
| Mute while recording | System-level mute: Win32 COM (IAudioEndpointVolume), macOS AppleScript, Linux wpctl/pactl/amixer fallback chain | `managers/audio.rs` `set_mute()` |
| Clamshell mode | macOS lid state via ioreg, automatic mic switch when switching displays | `helpers/clamshell.rs` (86 l) |
| Extra recording buffer | Configurable trailing silence capture (ms) to avoid cutting off | `managers/audio.rs` lines 518-525 |
| Short-audio padding | Pads recordings shorter than 1s to minimum 1.25s to avoid STT errors | `managers/audio.rs` lines 552-558 |
| Recording error classification | `is_microphone_access_denied()` / `is_no_input_device_error()` with test coverage | `audio/recorder.rs` lines 344-399 |

### 4.4 Pause/Resume Recording (PARLER-SPECIFIC)

| Feature | Description | Files |
|---------|-------------|-------|
| Pause binding | Default F6, toggle pause/resume during recording | `settings.rs` `get_default_settings()` lines 764-773 |
| AtomicBool pause flag | `AudioRecorder::with_pause_flag(Arc<AtomicBool>)` — build-time config | `audio/recorder.rs` line 67-69 |
| Pause drops frames | When paused, level callback emits zeros (visual feedback), recorded frames silently dropped; VAD not fed | `managers/audio.rs` `create_audio_recorder()` level callback |
| Stream stays open | Recording continues, resume is instant with zero audio gap | `recorder.rs` `run_consumer()` |
| Pause toggle command | `toggle_pause` Tauri command, emits `recording-paused` event for overlay state | `commands/audio.rs` lines 314-322 |
| Cancel resets pause | `is_paused.store(false)` on cancel to avoid stale state | `managers/audio.rs` `cancel_recording()` |

### 4.5 Post-Processing & LLM Integration (MATURED IN PARLER)

| Feature | Description | Files |
|---------|-------------|-------|
| Unified provider system | Multi-provider LLM with per-provider saved models, API keys, structured output flags | `settings.rs` `PostProcessProvider` struct (8 providers) |
| Providers | OpenAI, Z.AI, OpenRouter, Anthropic, Groq, Cerebras, Apple Intelligence, Custom | `settings.rs` `default_post_process_providers()` lines 529-607 |
| Structured output | JSON Schema mode (`response_format` with `json_schema`) for providers supporting it | `llm_client.rs` `send_chat_completion_with_schema()` (296 l) |
| System prompt enforcement | Forces models to output only the final text, no chatty preamble | `actions.rs` `build_system_prompt()` / `process_action()` |
| Gemini provider adapter | `fetch_gemini_models()` strips "models/" prefix from Gemini API response | `llm_client.rs` lines 254-296 |
| Provider defaults backfill | `ensure_post_process_defaults()` syncs `supports_structured_output`, adds missing providers/models/keys | `settings.rs` lines 655-709 |
| Apple Intelligence | Native Swift FFI bridge for macOS aarch64 (separate system prompt + user content) | `apple_intelligence.rs` (84 l) |
| Invisible char stripping | Removes \u200B, \u200C, \u200D, \uFEFF from LLM output | `actions.rs` `strip_invisible_chars()` |
| Prompt CRUD | Add/update/delete prompts with `${output}` variable substitution | `shortcut/mod.rs` `add_post_process_prompt` etc. |

### 4.6 Output / Pasting

| Feature | Description | Files |
|---------|-------------|-------|
| Clipboard paste (6 modes) | CtrlV, CtrlShiftV, ShiftInsert, Direct, None, ExternalScript — with save/restore clipboard cycle | `clipboard.rs` (687 l) |
| Linux tool chain | Auto-detects wtype, kwtype, dotool, ydotool, xdotool, wl-copy; KDE Wayland special-cased | `clipboard.rs` lines 83-500 |
| Auto-submit | Enter/Ctrl+Enter/Cmd+Enter after paste with configurable delay | `clipboard.rs` `send_return_key()` |
| Trailing space | Optional space appended after text | `clipboard.rs` lines 597-601 |
| Copy-to-clipboard option | Post-paste copy, configurable separately | `clipboard.rs` lines 655-660 |

### 4.7 UI & Overlay Features

| Feature | Description | Files |
|---------|-------------|-------|
| Recording overlay | 200x36px semi-transparent popup, 3 states (recording/transcribing/processing), fade animations | `overlay.rs` (495 l), `RecordingOverlay.tsx` |
| Multi-monitor overlay | Scale-factor-correct physical-to-logical conversion; on macOS follows focused window, elsewhere follows cursor | `overlay.rs` `get_monitor_containing_logical_point()` lines 161-187 |
| macOS panel overlay | NSPanel with `can_join_all_spaces` + `full_screen_auxiliary` collection behavior | `overlay.rs` lines 386-419 |
| Windows topmost Z-order | Win32 `SetWindowPos(HWND_TOPMOST)` to prevent burying | `overlay.rs` `force_overlay_topmost()` lines 133-159 |
| Linux GTK layer shell | Optional layer-shell overlay with KDE Wayland fallback to regular window | `overlay.rs` `init_gtk_layer_shell()` lines 96-129 |
| System tray | Dynamic menu: model submenu (radio buttons), cancel (recording state), copy-last-transcript, unload model | `tray.rs` (294 l) |
| Toast notifications | sonner toasts for recording errors (mic permission denied / no input device / unknown) and model load failures | `App.tsx` lines 114-159 |
| Onboarding flow | 3-step: accessibility permissions → model selection → main app; detects returning users | `App.tsx` `checkOnboardingStatus()` |
| Debug mode | Ctrl+Shift+D toggle + `--debug` CLI flag, advanced settings section | `App.tsx` lines 77-99 |

### 4.8 Configuration & Settings

| Feature | Description | Files |
|---------|-------------|-------|
| 85+ settings | Keyboard shortcuts, audio, models, text processing, post-processing, debug | `settings.rs` (1037 l) |
| **Settings bindings backfill** | On every read, merges missing default bindings and persists — fixes "undefined binding" UI bugs on upgrade | `settings.rs` `get_settings()` lines 943-953 |
| Post-process defaults backfill | `ensure_post_process_defaults()` syncs provider fields, adds missing entries | `settings.rs` lines 655-709 |
| **Settings export/import** | JSON dump/restore of entire settings store via file dialog | `commands/mod.rs` lines 118-136, `ExportImportSettings.tsx` (77 l) |
| Platform-specific defaults | Shortcuts, overlay position, paste method vary by OS at compile time | `settings.rs` `get_default_settings()` |
| Runtime keyboard implementation switching | Tauri ↔ HandyKeys with validation, rollback on failure | `shortcut/mod.rs` `change_keyboard_implementation_setting()` |
| Portable mode | Marker file detection (`portable` file with magic string), all data redirected to `Data/` directory | `portable.rs` (166 l) |

### 4.9 Crash Logging & Reliability

| Feature | Description | Files |
|---------|-------------|-------|
| Panic capture | `std::panic::set_hook` with `Backtrace::force_capture()`, writes to `parler-crash.log` | `crash_logging.rs` (80 l) |
| Engine panic isolation | `catch_unwind` during transcription; on panic, drops engine (don't re-use), clears model ID | `transcription.rs` lines 549-704 |
| Mutex poison recovery | `unwrap_or_else(|poisoned| poisoned.into_inner())` in `lock_engine()` and actions | `transcription.rs` lines 166-171 |
| Coordinator panic catch | `catch_unwind` around coordinator event loop | `transcription_coordinator.rs` line 63 |
| Cancel crash fix | `FinishGuard` RAII notifies coordinator when pipeline finishes (including panics) | `transcription_coordinator.rs` lines 33-41 |
| Accessibility stale-permission | Polling-safe check reads TCC db without triggering Enigo prompt spam | `lib/permissions.ts` (26 l) |

### 4.10 Shortcut System

| Feature | Description | Files |
|---------|-------------|-------|
| Dual implementation | Tauri global-shortcut + HandyKeys, selectable at runtime with fallback on HandyKeys failure | `shortcut/mod.rs` (1150 l), `handy_keys.rs` (600 l), `tauri_impl.rs` (240 l) |
| 6 shortcut bindings | transcribe, transcribe_with_post_process, cancel, pause, show_history, copy_latest_history | `settings.rs` |
| Cancel double-press | 1500ms window, emits `cancel-pending` event on first press, requires confirmation | `handler.rs` lines 112-137 |
| Binding suspend/resume | Unregister during UI editing, re-register after to avoid accidental triggers | `shortcut/mod.rs` `suspend_binding`/`resume_binding` |
| HandyKeys recording mode | Dedicated `KeyboardListener` for UI key capture with modifier-aware event emission | `handy_keys.rs` `start_recording()`/`recording_loop()` |
| Linux cancel disabled | Dynamic shortcut registration unstable on Linux — cancel shortcut unavailable during recording | `handy_keys.rs` lines 465-471, `tauri_impl.rs` lines 163-168 |

### 4.11 History & Database

| Feature | Description | Files |
|---------|-------------|-------|
| SQLite storage | `transcription_history` table with 4 migrations via `rusqlite_migration` | `managers/history.rs` (759 l) |
| Dual-text + prompt storage | Raw transcription + post-processed text + prompt template all stored per entry | `managers/history.rs` `save_entry()` |
| Paginated queries | Cursor-based pagination with `has_more` flag (fetch limit+1, pop if exceeded) | `managers/history.rs` `get_history_entries()` |
| Retry transcription | Re-load WAV, re-transcribe with current model | `commands/history.rs` `retry_history_entry_transcription()` |
| Record retention | Configurable: Never, PreserveLimit (count-based), 3 days, 2 weeks, 3 months | `managers/history.rs` `cleanup_by_count()`/`cleanup_by_time()` |
| Typed real-time events | `HistoryUpdatePayload` enum: Added, Updated, Deleted, Toggled via tauri-specta | `managers/history.rs` |

### 4.12 CI & Release Engineering

| Feature | Description | Files |
|---------|-------------|-------|
| Windows x64 pipeline | NSIS + MSI bundling with unsigned-build fallback | `.github/workflows/build-windows.yml` (NEW) |
| macOS signing + notarization | Hardened runtime, entitlements, notarization in CI | `.github/workflows/release.yml` |
| 10 CI workflows | build, build-test, build-windows, code-quality, main-build, nix-check, playwright, pr-test-build, release, test | `.github/workflows/` |
| thin LTO | Windows release uses `lto = "thin"` — full LTO caused transcription crash | `Cargo.toml` |
| CI mock | `transcription_mock.rs` (93 l) available but not currently wired into CI | `managers/transcription_mock.rs` |

---

## 5. Key Code Patterns & Techniques

### 5.1 Settings Bindings Backfill (Critical Pattern for Forks)
**File:** `src-tauri/src/settings.rs`, lines 943-953 (in `get_settings()`) and lines 886-894 (in `load_or_create_app_settings()`)

On every read, checks if user's stored settings contain all default bindings. Missing ones (added in newer versions) are merged in and persisted to the store. This fixes "undefined binding" UI bugs and accessibility-prompt spam when upgrading. Same pattern applied to post-process providers via `ensure_post_process_defaults()` (lines 655-709).

### 5.2 Long-Audio Model Switching
**File:** `src-tauri/src/managers/transcription.rs`, lines 460-500

After recording stops, computes `duration_seconds = audio.len() / 16000`. If > `long_audio_threshold_seconds` (default 10.0) and `long_audio_model` is configured (different from current), calls `load_model(long_model_id)`. Below threshold, restores the default model. UI in `LongAudioModelSettings.tsx` shows only downloaded models and configurable threshold dropdown (5/10/15/20/30/60s).

### 5.3 Pause Flag Architecture
**Files:** `src-tauri/src/audio_toolkit/audio/recorder.rs` lines 67-69, `src-tauri/src/managers/audio.rs` `create_audio_recorder()`

An `Arc<AtomicBool>` is shared between `AudioRecordingManager` and `AudioRecorder` via `with_pause_flag()`. When paused, the level callback sends zeroes (visual feedback of pause). The `run_consumer()` main loop continues processing audio — frames are still resampled and visualized, but VAD and sample collection are skipped. This means: (a) resume is instant (no stream restart), (b) the audio pipeline keeps running so no data is lost, (c) overhead is one atomic load per audio frame.

### 5.4 TranscriptionCoordinator — Single-Threaded Lifecycle
**File:** `src-tauri/src/transcription_coordinator.rs` (244 l)

All transcription lifecycle events are serialized through a single `mpsc::channel` thread. A `Stage` enum (Idle → Recording → Processing) is owned exclusively by this thread. Commands: `Input` (key press/release with 30ms debounce), `Cancel`, `ProcessingFinished` (RAII `FinishGuard`), `SelectAction`. This eliminates race conditions between keyboard shortcuts, Unix signals, CLI flags, and the async transcribe-paste pipeline.

### 5.5 Engine Panic Isolation
**File:** `src-tauri/src/managers/transcription.rs`, lines 549-704

Takes the engine OUT of the mutex before transcribing (`engine_guard.take()`), drops the lock, then uses `catch_unwind(AssertUnwindSafe(...))`. On success: puts engine back in mutex. On panic: engine is dropped (not re-used), model ID is cleared, an `unloaded` event with error is emitted. This prevents poisoned mutexes from making the app permanently unusable after one bad transcription.

### 5.6 Poison-Aware Mutex Recovery
**File:** `src-tauri/src/managers/transcription.rs`, lines 166-171

`lock_engine()` uses `unwrap_or_else(|poisoned| poisoned.into_inner())` — if a previous transcription panicked while holding the lock, the poison is swallowed and the engine (if still valid) is recovered. Same pattern in `actions.rs` line 625-630 for `ActiveActionState`.

### 5.7 Structured Output LLM Adapter
**File:** `src-tauri/src/llm_client.rs`, `send_chat_completion_with_schema()` (296 l)

Unified API for OpenAI-compatible providers with optional JSON Schema structured output. Providers declare `supports_structured_output: bool` in their `PostProcessProvider` definition. Apple Intelligence gets a completely separate FFI path (Swift function via C ABI). Falls back to legacy mode on structured output failure.

### 5.8 Gemini Provider Adapter Pattern
**File:** `src-tauri/src/llm_client.rs`, `fetch_gemini_models()` lines 254-296

Gemini returns model IDs as `models/gemini-2.5-flash`. This function strips the `models/` prefix and filters to only `gemini`-containing entries. The pattern: (1) detect non-OpenAI-shaped provider, (2) make a custom API call with provider-specific auth (`x-goog-api-key`), (3) normalize the response to the common format. This is the exact template S2B2S needs for non-OpenAI-shaped TTS voice listings.

### 5.9 Multi-Monitor Overlay Math
**File:** `src-tauri/src/overlay.rs`, `get_monitor_containing_logical_point()` lines 161-187

Tauri monitors report physical pixels. Dividing position/size by `scale_factor` normalizes to logical coordinates matching macOS CoreGraphics output. Uses `LogicalPosition` (not `PhysicalPosition`) for cross-monitor positioning because Tauri/tao converts PhysicalPosition using the scale factor of the monitor the window is CURRENTLY on, not the target monitor. macOS: overlay follows focused window center. Windows/Linux: overlay follows cursor.

### 5.10 Download Cleanup RAII Guard
**File:** `src-tauri/src/managers/model.rs`, `DownloadCleanup` struct lines 65-85

On every error path, `is_downloading` flag and cancel flags must be cleaned up. A RAII guard automates this: constructor sets up cleanup, `disarmed` flag prevents cleanup on success. Eliminates manual cleanup at every `?` operator and `return Err(...)`.

### 5.11 LoadingGuard Pattern
**File:** `src-tauri/src/managers/transcription.rs`, `LoadingGuard` lines 51-62

RAII guard that clears `is_loading` on Drop and notifies `loading_condvar`. Used by `try_start_loading()` (returns `None` if load in progress) and `switch_active_model()` (atomically claims loading slot). Prevents concurrent model loads from tray double-clicks or overlapping commands.

### 5.12 Log Level Atomic Runtime Switch
**File:** `src-tauri/src/lib.rs`, `FILE_LOG_LEVEL: AtomicU8` + filter closure

Stored as u8 (matching `log::LevelFilter` enum), read by custom `filter` closure on every log message. Allows runtime log level changes from the Debug settings without restarting the logging subsystem. Console log level is separately controlled via `RUST_LOG` env var.

---

## 6. Diff Analysis vs Handy (Parent)

### 6.1 Fork Genealogy

| Metric | Value |
|--------|-------|
| Fork point (merge-base) | `b76febd` — "docs: add release signature verification steps (#1178)" |
| Commits only in Parler | **66** |
| Commits in Handy missing from Parler | **33** |
| Net diff vs fork point (excl. lockfiles) | **109 files, +3,442 / -1,479 lines** |
| Rust-side diff | ~30 files, **+1,469 / -99 lines** |
| Mid-life upstream merge | `022ae...` "Merge upstream cjpais/Handy main into Parler" |

### 6.2 New Files Introduced by Parler

| File | Lines | Purpose |
|------|-------|---------|
| `src-tauri/src/gemini_client.rs` | 243 | Gemini generateContent client: WAV→base64→inline_data |
| `src-tauri/src/crash_logging.rs` | 80 | Panic capture with backtraces to `parler-crash.log` |
| `src-tauri/tauri.dev.conf.json` | 20 | "ParlerDev" side-by-side dev flavor config |
| `src/components/settings/ExportImportSettings.tsx` | 77 | Settings export/import UI with file dialog |
| `src/components/settings/general/LongAudioModelSettings.tsx` | 75 | Long-audio model selection + threshold dropdown |
| `src/components/icons/ParlerTextLogo.tsx` | — | Parler branding text logo |
| `src/lib/permissions.ts` | 26 | macOS accessibility polling-safe check (reads TCC, no prompt) |
| `.github/workflows/build-windows.yml` | — | Windows x64 NSIS+MSI CI pipeline |

### 6.3 Key Features Added (Delta Summary)

| Feature | Files Modified | Approx. Lines Added |
|---------|---------------|---------------------|
| Gemini cloud transcription (now deprecated) | `gemini_client.rs` (new), `commands/gemini.rs` (stub) | +243 |
| Long-audio model switching | `transcription.rs`, `settings.rs`, `LongAudioModelSettings.tsx` | ~+60 |
| Pause/resume recording | `recorder.rs`, `actions.rs`, `handler.rs`, `managers/audio.rs`, `settings.rs` | ~+200 |
| Settings export/import | `commands/mod.rs`, `ExportImportSettings.tsx` | ~+60 |
| Crash logging | `crash_logging.rs` (new) | +80 |
| Multi-monitor overlay fixes | `overlay.rs` (+169 lines) | ~+169 |
| Settings bindings backfill | `settings.rs` | ~+40 |
| History dual-text + prompt columns | `history.rs` (+3 migrations) | ~+40 |
| Post-processing maturation | `llm_client.rs`, `settings.rs`, `actions.rs`, `PostProcessingSettings.tsx` (729 l churn) | ~+300 |
| Show history / copy latest shortcuts | `handler.rs`, `settings.rs`, `tray.rs` | ~+50 |
| Dev flavor config | `tauri.dev.conf.json` (new) | +20 |
| Apple Intelligence FFI | `apple_intelligence.rs` (new) | +84 |
| Accessibility stale-permission fix | `lib/permissions.ts` (new) | +26 |
| macOS signing + notarization | CI workflows | ~+100 |
| Cancel crash fix (coordinator) | `transcription_coordinator.rs`, `FinishGuard` | ~+50 |
| Windows thin LTO workaround | `Cargo.toml` | +3 |

### 6.4 What Parler Did NOT Change (Inherited Wholesale)
- Audio capture path (cpal stream, VAD, resampler)
- Model manager core (download, SHA256, tar.gz extraction, custom model discovery)
- Engine set (all 7 STT engines: Whisper, Parakeet, Moonshine, SenseVoice, GigaAM, Canary)
- Paste machinery (`clipboard.rs`, `input.rs`)
- History SQLite schema basics (extended with 3 new columns)
- i18n infrastructure (20 languages, unchanged)
- Tray icon/menu core (extended with model submenu items)
- Audio feedback sound themes
- Onboarding flow skeleton (extended with permissions recovery)
- CLI parameters
- Single-instance architecture

---

## 7. Relation to S2B2S

Parler is the most surgically clean fork in the Handy family. It adds ~1.5K lines of Rust without removing or restructuring anything — the model S2B2S followed for its Phase 0.

| Aspect | Parler | S2B2S | Verdict |
|--------|--------|-------|---------|
| **Rebrand approach** | Renamed identifier + productName; README still Handy | Full rebrand, custom README, custom AGENTS.md | S2B2S more thorough |
| **TTS subsystem** | None | 8 backends (Piper, Kokoro, Kitten, Pocket, SAPI, OpenAI, ElevenLabs, Cartesia) with warm-persistent lifecycle | S2B2S has massive TTS advantage |
| **Brain/LLM streaming** | None (post-processing only) | Full streaming conversation with SSE, sentence splitter, barge-in | S2B2S has full Brain |
| **Pause/resume** | Yes (AtomicBool flag pattern) | Yes — inherited from Parler | Equivalent |
| **Long-audio model switching** | Yes (duration threshold) | Yes — generalized into routing policies with additional conditions | S2B2S extended |
| **Settings backfill** | Yes (bindings + providers) | Yes — extended to all new S2B2S settings (TTS, Brain, etc.) | S2B2S uses same pattern |
| **Export/import settings** | Yes | Yes — identical mechanism | Equivalent |
| **Crash logging** | Yes (`parler-crash.log`) | Yes — inherited, rebranded to `s2b2s-crash.log` | Equivalent |
| **Multi-monitor overlay** | Yes (scale-factor-correct) | Yes — extended for conversation HUD overlay | S2B2S extended |
| **Gemini integration** | Added → deprecated | Not present — S2B2S uses own provider system for TTS only | Different approaches |
| **Apple Intelligence** | Yes (Swift FFI bridge) | Not present — could harvest for zero-cost local Brain on macOS | Parler advantage |
| **CI/CD** | Windows + macOS signing | Windows + macOS + Linux (all 3 OSes) | S2B2S broader |
| **TripleVAD** | No (single Silero + SmoothedVad) | Yes (RMS → RNNoise prob → Silero cascade) | S2B2S more sophisticated |
| **Wake word / KWS** | No | Yes (`wake_word.rs`, KWS-ready, audio feed-in pending) | S2B2S advantage |
| **Model count** | 14 STT models | Same STT models + 9 TTS engines + llama.cpp server | S2B2S has vastly more |
| **i18n** | 20 languages | Same 20 languages | Equivalent |
| **Portable mode** | Yes (magic string marker) | Yes — identical mechanism | Equivalent |

### What S2B2S Inherited from Parler
1. **Pause/resume recording**: The `with_pause_flag(Arc<AtomicBool>)` builder pattern and `toggle_pause` command
2. **Settings bindings backfill**: The `get_settings()` merge-on-read pattern (now extended to all S2B2S additions)
3. **Settings export/import**: The `export_settings`/`import_settings` commands + `ExportImportSettings.tsx` UI
4. **Crash logging**: The panic hook pattern in `crash_logging.rs` (rebranded)
5. **Multi-monitor overlay fixes**: The `get_monitor_containing_logical_point()` scale-factor-correct math
6. **Long-audio model switching**: Generalized into S2B2S's routing policy engine
7. **Dev flavor config**: The `tauri.dev.conf.json` side-by-side installation pattern
8. **TranscriptionCoordinator**: The single-threaded lifecycle coordinator (evolved into S2B2S's own version)
9. **Thin LTO on Windows**: Release profile optimization that avoids transcription crashes

### What S2B2S Ditched
- **Gemini STT**: S2B2S uses transcribe-rs exclusively for STT; cloud features only on TTS side
- **Post-processing system**: Replaced entirely by S2B2S's full Brain (streaming LLM conversation) subsystem
- **Parakeet V2 (deprecated)**: S2B2S dropped V2, keeping only V3+
- **Moonshine non-streaming**: S2B2S uses only streaming variants

---

## 8. Harvest List (Features Worth Copying)

| Feature to harvest | From file | Effort | Why valuable for S2B2S |
|-------------------|-----------|--------|------------------------|
| Settings bindings backfill on read | `settings.rs` lines 943-953 | XS | Already in S2B2S — verify all new settings have defaults |
| Post-process provider defaults backfill | `settings.rs` `ensure_post_process_defaults()` (655-709) | XS | Apply same pattern to TTS provider defaults |
| Pause/resume recording pattern | `recorder.rs` `with_pause_flag()`, `managers/audio.rs` `toggle_pause()` | S | Already in S2B2S — verify correctness under TripleVAD |
| Long-audio model switching | `transcription.rs` lines 460-500 | S | Already generalized into S2B2S routing policies |
| Crash logging panic hook | `crash_logging.rs` (80 l) | XS | Already in S2B2S |
| Settings export/import | `commands/mod.rs` lines 118-136, `ExportImportSettings.tsx` (77 l) | XS | Already in S2B2S |
| Multi-monitor overlay math | `overlay.rs` `get_monitor_containing_logical_point()` | S | Already in S2B2S |
| Gemini provider adapter pattern | `llm_client.rs` `fetch_gemini_models()` (254-296) | M | Template for ElevenLabs/Cartesia voice listing APIs |
| Apple Intelligence FFI bridge | `apple_intelligence.rs` (84 l) | L | Zero-cost local Brain on macOS aarch64 — unique feature |
| Dev flavor config | `tauri.dev.conf.json` (20 l) | XS | Already in S2B2S |
| Copy-latest + show-history shortcuts | `handler.rs` lines 87-101, `tray.rs` `copy_last_transcript()` | XS | Quick QoL — Brain audit trail |
| History dual-text + prompt columns | `history.rs` `save_entry()` + migrations 2-3 | S | Enables Brain re-play / audit trail |
| Cancel double-press confirmation | `handler.rs` lines 112-137 | XS | Prevents accidental cancel during long dictation |
| Windows thin LTO release profile | `Cargo.toml` line 122 | XS | Full LTO causes transcription crash — already applied |
| RecordingErrorEvent classification | `recorder.rs` + `App.tsx` lines 114-139 | XS | Granular error toasts (mic denied vs no device vs unknown) |

---

## 9. Known Issues, Caveats & Limitations

| Issue | Severity | Impact |
|-------|----------|--------|
| **33 commits behind Handy main** | Medium | Missing upstream bug fixes and features; port Parler features as patches, not wholesale merge |
| **Gemini STT deprecated but compiles** | Low | `commands/gemini.rs` is a 2-line stub; `gemini_client.rs` still compiles with dead code; no API key for Gemini in settings defaults |
| **Overlay disabled by default on Linux** | Medium | Same as Handy — compositor steals focus, prevents paste into target app |
| **KDE Wayland layer-shell broken** | Medium | Detected and gracefully skipped in `init_gtk_layer_shell()`, but no layer-shell means overlay can steal focus |
| **HandyKeys cancel disabled on Linux** | Medium | Dynamic shortcut registration unstable on Linux; cancel double-press unavailable during recording |
| **Version label still says "Handy"** | Low | `tray.rs` line 98: `"Handy v0.8.6"` hardcoded, not "Parler" |
| **README not rebranded** | Low | Still describes cjpais/Handy, links to handy.computer |
| **Source code link stale** | Low | `AboutSettings.tsx` line 69 points to `Melvynx/Handy` (old org) |
| **Clamshell detection macOS-only** | Medium | Windows/Linux stubs return `false` — no laptop-specific mic switching on those platforms |
| **System mute check Linux stub** | Low | `is_system_already_muted()` returns `false` on Linux — mute-while-recording may redundantly (un)mute |
| **`reprocess_history_entry` dead code** | Low | `#[allow(dead_code)]` — implemented but not wired to any UI |
| **`transcription_mock.rs` unused in CI** | Low | Mock exists for headless testing but no workflow copies it |
| **No TTS at all** | High (for comparison) | Parler is STT-only — zero TTS, zero Brain. S2B2S's entire value-add is absent here |
| **No streaming transcription** | Medium | Batch-mode only; no incremental/streaming results during recording |
| **Single VAD layer** | Medium | SmoothedVad only — no cascaded VAD like S2B2S's TripleVAD |

---

## 10. Strengths & Weaknesses

### Strengths

1. **Surgical fork discipline** — ~1.5K lines of Rust added, nothing structural removed. The cleanest fork in the Handy family and the model S2B2S followed.
2. **Settings backfill pattern** — The bindings merge-on-read mechanism in `get_settings()` is robust against upgrade breaks. Every Handy fork needs this pattern, and Parler implemented it first.
3. **Pause/resume implementation** — Clean `AtomicBool` flag pattern with visual feedback. The `with_pause_flag()` builder keeps the stream alive so resume is instant.
4. **Multi-monitor overlay math** — Scale-factor-correct physical-to-logical conversion fixed a real pain point for laptop users with external displays.
5. **Unified post-processing provider system** — Clean abstraction with 8 providers, structured output support, and a Gemini adapter. This is the template S2B2S used for its multi-provider TTS backends.
6. **Crash logging with full backtraces** — Straightforward `std::panic::set_hook` + `Backtrace::force_capture()`. Every desktop app needs this and Parler pioneered it.
7. **Engine panic isolation** — Taking the engine out of the mutex before transcribing + `catch_unwind` prevents hard hangs from ML crashes. This is production-grade resilience.
8. **Coordinator single-threaded lifecycle** — Eliminates entire class of race conditions between shortcuts, signals, CLI, and async pipeline.
9. **macOS-first release quality** — Signing, notarization, hardened runtime, accessibility permission recovery, NSPanel overlay. Production-grade macOS experience.
10. **Download cleanup RAII guards** — `DownloadCleanup` and `LoadingGuard` patterns ensure consistent state cleanup on every error path without manual bookkeeping.
11. **Comprehensive Linux paste tool chain** — Auto-detects wtype, kwtype, dotool, ydotool, xdotool, wl-copy with KDE Wayland special-casing. The most complete Linux paste implementation of any Handy fork.
12. **10 CI workflows** — Build, test, code quality, playwright, PR builds, release pipelines for all platforms.

### Weaknesses

1. **Incomplete rebranding** — README, tray version display, source code link, and AGENTS.md still say "Handy". Inconsistent with the Parler brand.
2. **33 commits behind upstream Handy** — Missing upstream bug fixes and features. Port features as patches rather than merging Parler wholesale.
3. **Gemini STT dead code** — `commands/gemini.rs` is a 2-line stub; `gemini_client.rs` still compiles but is unused. Adds maintenance burden.
4. **Linux second-class treatment** — Cancel shortcut disabled, overlay issues on Wayland compositors, clamshell detection macOS-only, system mute check stub. This is a macOS-first app.
5. **No TTS subsystem** — Purely STT, limited to the original Handy scope. Compared to S2B2S's 9 TTS backends + streaming Brain, this is the fundamental gap.
6. **No streaming/incremental transcription** — Batch-mode only. No real-time feedback during recording.
7. **Single VAD layer** — SmoothedVad is better than bare Silero but lacks the sophistication of S2B2S's TripleVAD (RMS → RNNoise probability → Silero cascade with tunable thresholds).
8. **`reprocess_history_entry` dead code** — Full implementation with model switching exists but tagged `#[allow(dead_code)]` — not wired to UI.
9. **No wake word / KWS** — No hands-free activation mode. Must use keyboard shortcuts.
10. **thin LTO workaround, not fix** — The full LTO transcription crash on Windows was worked around (`lto = "thin"`) rather than root-caused and fixed.

---

## 11. Bottom Line / Verdict

Parler is the gold-standard fork in the Handy ecosystem: surgical, focused, and production-quality. It proves that a single developer can add meaningful power-user features — pause/resume, long-audio model switching, settings export/import, crash logging, multi-monitor overlay fixes — to Handy without restructuring the codebase. At +1,469 / -99 lines of Rust, it is the leanest fork and the model S2B2S deliberately followed for its Phase 0.

S2B2S inherited the most valuable patterns from Parler: the settings bindings backfill mechanism (now extended to all TTS/Brain settings), the pause/resume architecture, the TranscriptionCoordinator single-threaded lifecycle, and the crash logging infrastructure. S2B2S then built its entire Brain (streaming LLM conversation with barge-in) and TTS (8 backends with warm-persistent lifecycle) superstructure on top — capabilities that exist nowhere in the Handy fork ecosystem.

If you have time to harvest one more thing from Parler, take the **Apple Intelligence FFI bridge** (`src-tauri/src/apple_intelligence.rs`, 84 lines). It would give S2B2S a zero-cost, privacy-preserving local Brain option on Apple Silicon Macs — a unique feature no other Handy fork offers. The Gemini provider adapter pattern in `llm_client.rs` (`fetch_gemini_models()`, lines 254-296) is also worth studying — it's the exact template for integrating non-OpenAI-shaped TTS voice listing APIs (ElevenLabs, Cartesia) into S2B2S's existing provider abstraction.

**Primary value to S2B2S:** Reference implementation proving the fork-rebrand-add pattern works cleanly; source of battle-tested patterns for platform overlay quirks, crash resilience, and settings migration hygiene.

---

*Analysis based on reading every file in the Parler project tree. Total codebase: ~39,188 lines across Rust, TypeScript, JSON, TOML, Markdown, and YAML files. Key files read with line counts: `lib.rs` (606), `settings.rs` (1037), `actions.rs` (862), `transcription.rs` (866), `history.rs` (759), `model.rs` (1276+), `audio.rs` (610), `recorder.rs` (596), `overlay.rs` (495), `clipboard.rs` (687), `llm_client.rs` (296), `gemini_client.rs` (243), `crash_logging.rs` (80), `shortcut/mod.rs` (1150), `handy_keys.rs` (600), `tauri_impl.rs` (240), `handler.rs` (145), `transcription_coordinator.rs` (244), `tray.rs` (294), `portable.rs` (166), `apple_intelligence.rs` (84), `clamshell.rs` (86), `settingsStore.ts` (596), `PostProcessingSettings.tsx` (403), `App.tsx` (293), `ExportImportSettings.tsx` (77), `LongAudioModelSettings.tsx` (75), `AboutSettings.tsx` (95), `permissions.ts` (26).*
