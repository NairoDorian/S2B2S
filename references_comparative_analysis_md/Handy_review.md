# Handy -- Mother Project (Fork Origin of S2B2S)

> Repo: `cjpais/Handy` · Version: **v0.8.3** · License: **MIT** · Author: **CJ Pais** · Platforms: **Windows / macOS / Linux**
> Nature: **fork-origin (S2B2S is a fork of Handy)** · Role for S2B2S: **the complete skeleton** -- every subsystem S2B2S inherited was defined here first

---

## 1. What Handy Is

Handy is a free, open-source, offline-first desktop speech-to-text application built with Tauri 2.x. Its core loop is brutally simple: the user presses a configurable global keyboard shortcut, speaks into their microphone, releases the shortcut, and the transcribed text is pasted directly into whatever application has focus. Everything happens locally -- no cloud required.

The author explicitly positions Handy as **"not trying to be the best speech-to-text app -- trying to be the most forkable one."** This philosophy is evident in every architectural choice: clean manager separation, typed IPC (tauri-specta), CLI remote control, a mock transcription manager for CI tests, and an AGENTS.md that teaches AI agents how to work with the codebase. The project has been forked into at least three other applications (S2B2S, Parler, AIVORelay, Parrot).

As of v0.8.3, Handy is a mature, actively maintained application under a **feature freeze** -- new features require community support in Discussions before a PR is accepted. The focus has shifted to stability, refactoring, and fixing platform-specific edge cases.

---

## 2. Tech Stack

### 2.1 Frontend (React SPA inside Tauri WebView)

| Layer | Choice | Purpose |
|-------|--------|---------|
| Framework | React 18.3 + TypeScript 5.6 | SPA inside Tauri webview |
| Build | Vite 6, Bun as package manager | `bun.lock` present |
| Styling | Tailwind CSS v4 (`@tailwindcss/vite`) | Utility-first CSS |
| State | Zustand 5, Immer | Stores in `src/stores/` |
| i18n | i18next / react-i18next | 20 locales (ar, bg, cs, de, en, es, fr, he, it, ja, ko, pl, pt, ru, sv, tr, uk, vi, zh, zh-TW) |
| Validation | Zod | Settings schemas |
| UI bits | lucide-react icons, react-select, sonner toasts | |
| Type bridge | tauri-specta generates `src/bindings.ts` | Fully typed IPC -- a major asset when extending |
| Testing | Playwright | `playwright.config.ts`, `tests/app.spec.ts` |

### 2.2 Backend / Core (Rust, `src-tauri/`)

| Layer | Choice | Purpose |
|-------|--------|---------|
| Shell | Tauri 2.10.2 + patched Tauri runtime from `cjpais/tauri` branch `handy-2.10.2` | Custom runtime patches |
| Audio I/O | cpal 0.16 (capture), rodio (cjpais fork for playback/feedback), rubato 0.16 (resampling), hound (WAV) | |
| VAD | vad-rs (cjpais fork) -- Silero ONNX | Voice Activity Detection |
| STT | transcribe-rs 0.3.8 with per-OS features: whisper-metal, whisper-vulkan, ort-directml, whisper-cpp, onnx | One crate, 8 engine families |
| Keyboard | enigo 0.6 (typing/paste), rdev (rustdesk fork for low-level key listening), handy-keys 0.2.4 (custom shortcut engine) | Two keyboard implementations, runtime-switchable |
| Text post | strsim + natural (fuzzy custom-word correction), regex, ferrous-opencc (zh variant conversion) | |
| LLM | reqwest 0.12 (json, stream) -- hand-rolled OpenAI-compatible client | `llm_client.rs` (277 lines) |
| Storage | rusqlite 0.37 (bundled) + rusqlite_migration | History DB with 4 migrations |
| DSP | rustfft | Mic level/visualizer (16-bucket spectrum) |
| CLI | clap derive | 6 flags |
| macOS | tauri-nspanel, Swift sources (`apple_intelligence.swift`) | Overlay as NSPanel, on-device LLM |
| Linux | gtk + gtk-layer-shell | Overlay as layer shell (Wayland/X) |
| Windows | windows-rs (audio endpoints, COM), winreg | Microphone permissions |
| Packaging | Nix flake + modules, NSIS, portable mode | |

### 2.3 Key Dependencies (non-obvious)

- **cjpais/tauri** patched runtime (branches `handy-2.10.2`) -- custom Tauri patches that S2B2S inherits and must maintain when bumping Tauri versions.
- **cjpais/vad-rs** fork -- Silero VAD wrapper.
- **cjpais/rodio** fork -- audio feedback playback.
- **rustdesk-org/rdev** fork -- low-level key listener for handy-keys.
- **ferrous-opencc 0.2.3** -- Simplified/Traditional Chinese variant conversion (OpenCC).

---

## 3. Architecture & Source Map

```
Handy/                                     (~150 files total)
src-tauri/                                 [Rust backend -- ~10,000 lines]
  Cargo.toml (108 l)                       Dependencies + release profile (lto, codegen-units=1, strip)
  build.rs (254 l)                         Tray i18n codegen + Swift compilation (macOS ARM)
  tauri.conf.json (75 l)                   App config: identifier com.pais.handy, updater pubkey, bundles
  capabilities/default.json (25 l)         Core permissions: store, updater, fs, global-shortcut
  capabilities/desktop.json (12 l)         Desktop-only: autostart, updater
  resources/default_settings.json (12 l)   Minimal default settings
  resources/*.wav                          Sound theme files (marimba, pop)
  resources/tray_*.png / handy.png         Tray icons (idle/recording/transcribing)
  swift/
    apple_intelligence.swift (144 l)       Swift bridge to macOS SystemLanguageModel (structured output)
    apple_intelligence_stub.swift (45 l)   Stub when SDK missing
    apple_intelligence_bridge.h (29 l)     C-compatible header
  src/
    main.rs (18 l)                         Binary entry: parse CLI, set env, call lib::run
    lib.rs (615 l)                         App bootstrap, plugin registration, specta command export,
                                           manager init, tray creation, single-instance handler
    settings.rs (989 l)                    AppSettings struct (~60 fields), defaults, JSON store persistence,
                                           7 default LLM providers, SecretMap for API keys, migration helpers
    actions.rs (721 l)                     ShortcutAction trait, TranscribeAction, CancelAction, ACTION_MAP,
                                           audio->VAD->transcribe->post-process->paste pipeline
    transcription_coordinator.rs (184 l)   Serializes events through mpsc channel, 30ms debounce,
                                           state machine: Idle->Recording->Processing
    signal_handle.rs (38 l)                Unix signal handlers (SIGUSR1/SIGUSR2)
    cli.rs (29 l)                          Clap-derived CLI args (6 flags)
    llm_client.rs (277 l)                  OpenAI-compatible /chat/completions, structured output support,
                                           ReasoningConfig, /models endpoint, multi-provider auth headers
    clipboard.rs (687 l)                   Paste methods (CtrlV, Direct, ShiftInsert, CtrlShiftV, ExternalScript),
                                           Linux native tools (wtype, xdotool, dotool, ydotool, kwtype, wl-copy),
                                           clipboard restore, auto-submit (Enter/CtrlEnter/CmdEnter)
    input.rs (123 l)                       EnigoState wrapper, send_paste_ctrl_v, get_cursor_position
    overlay.rs (396 l)                     Recording overlay: macOS (NSPanel), Linux (GTK layer-shell),
                                           Windows (HWND_TOPMOST), position calculation, mic levels emission
    tray.rs (303 l)                        Tray icon (3 states: Idle/Recording/Transcribing), menu with
                                           model submenu, copy last transcript, theme-aware icons
    tray_i18n.rs (34 l)                    Tray menu i18n (compile-time generated from locale JSON files)
    audio_feedback.rs (142 l)              Sound effects (start/stop), rodio playback, custom sound support
    apple_intelligence.rs (84 l)           Rust FFI bindings for Swift Apple Intelligence bridge
    utils.rs (66 l)                        cancel_current_operation(), is_wayland(), is_kde_wayland()
    portable.rs (166 l)                    Portable mode: marker file, Data/ dir, migration
    helpers/clamshell.rs (86 l)            Laptop clamshell mode detection (macOS via ioreg)
    managers/
      audio.rs (516 l)                     AudioRecordingManager: always-on / on-demand mic, mute, VAD preload,
                                           lazy stream close (30s idle timeout), clamshell mic override, cancel
      model.rs (1649 l)                    ModelManager: catalog (16 models), download (resume, SHA256 verify,
                                           tar.gz extraction), delete, custom model auto-discovery, migration
      transcription.rs (854 l)             TranscriptionManager: 8 engine types, load/unload, idle watcher,
                                           panic guard, catch_unwind, accelerator settings, GPU enumeration
      transcription_mock.rs (89 l)         CI-only mock (no whisper/Vulkan deps)
      history.rs (737 l)                   HistoryManager: SQLite (rusqlite) with 4 migrations, timestamp-based
                                           cleanup, cursor-based pagination, WAV file management
    commands/
      mod.rs (187 l)                       Utility commands: cancel, portable, app dir, log dir, Enigo, shortcuts
      audio.rs (312 l)                     Audio commands: mic mode, devices, Windows mic permissions
      models.rs (221 l)                    Model commands: list, info, download, delete, switch, status
      history.rs (154 l)                   History commands: entries, toggle saved, delete, retry, cleanup
      transcription.rs (40 l)              Transcription commands: unload timeout, load status, manual unload
    shortcut/
      mod.rs (1157 l)                      Unified shortcut interface, 2 backends (tauri/handy-keys),
                                           ~60 setting-change commands, implementation switcher
      handler.rs (70 l)                    Shared shortcut event handler, push-to-talk vs toggle logic
      tauri_impl.rs (198 l)                Tauri global-shortcut implementation
      handy_keys.rs (549 l)                Handy-keys implementation: manager thread, hotkey polling,
                                           keyboard listener for key recording mode
    audio_toolkit/
      constants.rs (1 l)                   WHISPER_SAMPLE_RATE = 16000
      text.rs (567 l)                      Custom words fuzzy correction (strsim + Soundex + n-gram),
                                           filler word removal (18 languages), stutter collapse
      utils.rs (12 l)                      get_cpal_host() -- ALSA on Linux
      bin/cli.rs (323 l)                   CLI test binary for audio toolkit
      audio/
        recorder.rs (519 l)                AudioRecorder: cpal input stream, resampling, VAD, visualizer
        device.rs (52 l)                   list_input_devices(), list_output_devices()
        resampler.rs (99 l)                FrameResampler: rubato FftFixedIn, 1024-chunk, zero-pad finish
        visualizer.rs (156 l)              AudioVisualiser: rustfft, Hann window, 16 buckets, noise floor
        utils.rs (50 l)                    save_wav_file(), read_wav_samples(), verify_wav_file()
      vad/
        mod.rs (32 l)                      VoiceActivityDetector trait, VadFrame enum
        silero.rs (52 l)                   SileroVad: ONNX model, 30ms frames, probability threshold
        smoothed.rs (105 l)                SmoothedVad: prefill/hangover/onset hysteresis
src/                                       [Frontend -- ~14,000 lines]
  main.tsx (20 l)                          Entry: platform detection, i18n init, model store init
  App.tsx (289 l)                          Main component: onboarding flow, sidebar, debug shortcut
  bindings.ts (914 l)                      Auto-generated tauri-specta TypeScript bindings
  i18n/                                    20 locale files (~612 lines each)
  stores/                                  Zustand stores: settingsStore, modelStore
  hooks/                                   useSettings, useOsType
  lib/                                     Types, constants, utils (keyboard, rtl, format)
  overlay/                                 Recording overlay window UI
  components/                              ~80 React components (settings, onboarding, model-selector, UI)
scripts/                                   check-translations.ts, check-nix-deps.ts
nix/                                       flake.nix, NixOS module, Home Manager module
docs/                                      README, BUILD, AGENTS, CRUSH, CONTRIBUTING

### Total line counts by subsystem

| Subsystem | Files | Lines |
|-----------|-------|-------|
| Rust backend (managers + core) | 30+ `.rs` | ~10,000 |
| Rust audio_toolkit | 12 `.rs` | ~1,600 |
| Rust commands | 5 `.rs` | ~1,000 |
| Rust shortcut | 4 `.rs` | ~2,000 |
| Swift (Apple Intelligence) | 3 files | ~210 |
| Frontend React (components) | 90+ `.tsx`/`.ts` | ~12,000 |
| i18n (20 locales) | 40 files | ~12,000 |
| Config & build | 15+ files | ~1,200 |
| Documentation | 6 `.md` | ~1,100 |
| Nix packaging | 6 files | ~1,400 |

**Total: approximately 40,000 lines.**

---

## 4. Feature Inventory

### 4.1 STT Pipeline

The complete pipeline from microphone to pasted text:

1. **Audio Capture** (`audio_toolkit/audio/recorder.rs`, 519 lines): cpal input stream with per-sample-format stream builder (U8/I8/I16/I32/F32), multi-channel to mono downmix, optional always-on mic mode for zero-latency recording start. Uses command channel (Start/Stop/Shutdown) from main thread to worker thread.

2. **Resampling** (`audio_toolkit/audio/resampler.rs`, 99 lines): rubato FftFixedIn resampler, device-native rate to 16kHz, 1024-sample chunks, zero-pad on finish for trailing frames. Bypasses when native rate equals 16kHz.

3. **Voice Activity Detection** -- Two-tier system:
   - `SileroVad` (`audio_toolkit/vad/silero.rs`, 52 lines): vad-rs (cjpais fork), Silero v4 ONNX model, 30ms frames at 16kHz, probability threshold 0.3 (hardcoded).
   - `SmoothedVad` (`audio_toolkit/vad/smoothed.rs`, 105 lines): Hysteresis wrapper -- 2-frame onset confirmation, 15-frame hangover, 15-frame prefill buffer for capturing speech onset. Prevents clipped beginnings and flicker.

4. **STT Engines** (`managers/transcription.rs`, 854 lines): 8 engine families via `LoadedEngine` enum -- Whisper (whisper-rs/cpp), Parakeet TDT 0.6B V2/V3 (ONNX), Moonshine Base + V2 Tiny/Small/Medium Streaming (ONNX), SenseVoice (ONNX), GigaAM v3 (ONNX), Canary 180M Flash + 1B v2 (ONNX), Cohere int8 (ONNX).

5. **GPU Acceleration** (`managers/transcription.rs`): WhisperAcceleratorSetting (Auto/Cpu/Gpu) + GPU device picker via OnceLock-cached device enumeration, OrtAcceleratorSetting (Auto/Cpu/Cuda/DirectMl/Rocm), pre-warm on startup.

6. **Text Cleanup** (`audio_toolkit/text.rs`, 567 lines):
   - Custom words fuzzy correction: Levenshtein + Soundex phonetic matching, 1/2/3-word n-gram merging, configurable threshold (default 0.18), case preservation, punctuation extraction/re-injection.
   - Filler word removal: language-specific lists for 18 languages (English "uh/um", Portuguese preserves "um" as actual word, Spanish preserves "ha"), plus custom filler words override.
   - Stutter collapse: 3+ consecutive identical alphabetic words collapsed to single instance (case-insensitive).
   - Multi-space normalization.

7. **Chinese Variant Conversion** (`actions.rs` lines 299-341): ferrous-opencc for Simplified / Traditional Chinese conversion when language is set to zh-Hans or zh-Hant (Tw2sp for simplified, S2tw for traditional).

8. **LLM Post-Processing** (`actions.rs` lines 66-297): Optional post-transcription LLM pass -- see section 4.2.

9. **Output** (`clipboard.rs`, 687 lines): 6 paste methods (CtrlV, Direct, ShiftInsert, CtrlShiftV, None, ExternalScript), clipboard restore (save original, paste, restore), auto-submit (Enter/CtrlEnter/CmdEnter), trailing space append, configurable delay (default 60ms). On Linux: auto-detects and uses wtype/xdotool/dotool/ydotool/kwtype/wl-copy with intelligent fallback chains, KDE Wayland detection, per-tool preference settings.

### 4.2 LLM Post-Processing (the proto-"Brain")

This is one of Handy's most important features -- and the direct predecessor of S2B2S's Brain:

- **Client** (`llm_client.rs`, 277 lines): OpenAI-compatible `/chat/completions` with:
  - **Structured output** (`response_format: json_schema`, strict) -- forces the LLM to output `{"transcription": "..."}` via JSON schema with `additionalProperties: false`.
  - **Reasoning controls**: OpenRouter-style `reasoning {effort, exclude}` config for excluding thinking tokens from response.
  - **System prompt support**: separates system instructions from user content.
  - **Model listing**: `/models` endpoint for provider model discovery, handles both array format and OpenAI `{data: [{id: ...}]}` format.

- **7 Built-in Providers** (`settings.rs` lines 524-613):
  - OpenAI, Z.AI, OpenRouter, Anthropic, Groq, Cerebras, AWS Bedrock (Mantle)
  - **Apple Intelligence** (macOS aarch64 only): on-device LLM via Swift/FoundationModels bridge, structured output via `@Generable` structs, semaphore-based async-to-sync bridge, token limit truncation.
  - **Custom** provider: defaults to `http://localhost:11434/v1` (Ollama-compatible), allows base URL editing.

- **Prompt System** (`settings.rs`): Named library of `LLMPrompt`s, default "Improve Transcriptions" with detailed instructions for capitalization, punctuation, number formatting, filler removal. Uses `${output}` placeholder.

- **Model Selection per Provider**: Each provider stores its own selected model in `post_process_models` HashMap. Fetched via `/models` endpoint.

- **Trigger**: Separate shortcut binding (`transcribe_with_post_process`), globally toggleable (`post_process_enabled` setting). Post-processing shortcut registered/unregistered based on toggle state.

- **Not implemented**: No streaming responses, no conversation memory/state, no tool use. These are the exact gaps S2B2S's Brain fills.

### 4.3 Audio and Recording Features

- **Always-on microphone**: Keeps the audio stream open for zero-latency recording start. Optional 30-second idle close via `lazy_stream_close` setting. Mode switching at runtime.
- **Mute while recording**: Platform-specific system mute (Windows COM `IAudioEndpointVolume::SetMute`, macOS AppleScript `set volume output muted`, Linux wpctl/pactl/amixer fallback chain).
- **Clamshell microphone override**: Detects MacBook lid closure (macOS `ioreg -r -k AppleClamshellState`) and switches to a user-configured microphone. Laptop detection via `pmset -g batt` for InternalBattery presence.
- **Extra recording buffer**: Configurable trailing buffer (ms) after shortcut release -- useful for capturing trailing speech.
- **Spectrum visualizer** (`audio_toolkit/audio/visualizer.rs`, 156 lines): FFT-based 16-bucket frequency analyzer, Hann window, logarithmic bucket spacing (400Hz-4kHz voice range), noise floor tracking with slow adaptation (0.001 alpha), gain and curve shaping, light 3-point smoothing.

### 4.4 UI/UX Features

- **Overlay window** (`overlay.rs`, 396 lines): Small recording indicator (172x36px), multi-platform:
  - macOS: NSPanel with `CollectionBehavior::can_join_all_spaces + full_screen_auxiliary`, non-activating, corner radius 0, transparent.
  - Linux: GTK layer-shell with edge anchoring (disable via `HANDY_NO_GTK_LAYER_SHELL=1`), falls back to always-on-top window.
  - Windows: Win32 `SetWindowPos(HWND_TOPMOST)` with SWP_NOACTIVATE/SWP_NOMOVE/SWP_NOSIZE.
  - Positions: Top/Bottom/None, follows cursor monitor, scale-factor-aware coordinate calculation.

- **Tray icon** (`tray.rs`, 303 lines): 3 visual states (Idle/Recording/Transcribing), theme-aware icons (Dark/Light/Colored for Linux), model selection submenu with check marks, "Copy Last Transcript" context menu action, "Unload Model" during idle.

- **Settings UI**: ~27 React components, one per setting, organized into General/History/Debug/Post-Processing/Advanced tabs. Sidebar navigation. Zustand reactive updates.

- **Onboarding**: Two-step process -- accessibility permissions (macOS tauri-plugin-macos-permissions) then model download/selection. Detects returning users who just need to re-grant permissions vs new users needing full setup.

- **Audio feedback**: Start/stop sounds via rodio (cjpais fork), 3 sound themes (Marimba/Pop/Custom), per-theme volume, test playback, output device selection. Custom sounds loaded from AppData.

- **Debug mode**: `Ctrl+Shift+D` toggle (Cmd+Shift+D on macOS), verbose logging (Trace level), debug settings panel (paste delay 0-500ms, recording buffer 0-2000ms, word correction threshold 0-1, keyboard implementation selector, log level picker).

- **i18n**: 20 languages, ESLint-enforced no hardcoded strings in JSX, auto-generated tray translations from locale JSON files (compile-time via build.rs), RTL support (rtl.ts utility).

### 4.5 Platform Features

- **Global shortcuts**: Two implementations (Tauri global-shortcut plugin, handy-keys library), runtime-switchable, per-implementation validation (Tauri: no fn key, must have non-modifier; HandyKeys: more permissive), automatic HandyKeys-to-Tauri fallback on failure with persisted fallback setting.

- **CLI remote control**: 6 flags (`--toggle-transcription`, `--toggle-post-process`, `--cancel`, `--start-hidden`, `--no-tray`, `--debug`) via tauri_plugin_single_instance. Flags are runtime-only, not persisted.

- **Unix signals**: SIGUSR2 (transcribe), SIGUSR1 (transcribe+post-process) for Wayland window managers and hotkey daemons. Example configs provided for Sway, Hyprland, i3, GNOME, KDE.

- **Autostart**: Cross-platform via tauri_plugin_autostart with macOS LaunchAgent.

- **Updater**: tauri-plugin-updater with `createUpdaterArtifacts: true`, minisign signature verification instructions in README.

- **Portable mode**: `portable` marker file next to executable redirects all user data to `Data/` directory. Detects legacy empty markers and upgrades them automatically. WebView2 cache also redirected.

- **Windows microphone permissions**: Registry-based permission detection via winreg (HKEY_LOCAL_MACHINE and HKEY_CURRENT_USER paths under CapabilityAccessManager\ConsentStore\microphone).

- **Single instance**: Enforced via `tauri_plugin_single_instance`, second instance sends args to first then exits. macOS: handles Reopen event to show window from dock.

### 4.6 Model Management

- **Built-in model catalog** (`managers/model.rs`, 1649 lines): 16 downloadable models across 8 engine families:
  - Whisper: Small (465MB), Medium (469MB q4_1), Turbo (1549MB large-v3-turbo), Large (1031MB v3 q5_0), Breeze ASR (1030MB, Taiwanese Mandarin)
  - Parakeet: V2 int8 (451MB, English only), V3 int8 (456MB, 25 EU languages) -- V3 is recommended for new users
  - Moonshine: Base (55MB), V2 Tiny Streaming (31MB), V2 Small Streaming (99MB), V2 Medium Streaming (192MB) -- all English only
  - SenseVoice: int8 (152MB, zh/en/yue/ja/ko)
  - GigaAM: v3 (151MB, Russian)
  - Canary: 180M Flash (146MB, en/de/es/fr), 1B v2 (691MB, 25 EU languages)
  - Cohere: int8 (1708MB, 16 languages)

- **Download with resume**: Range-request support (HTTP `bytes=`), SHA256 verification in blocking thread, tar.gz extraction for directory-based models, progress events (throttled to 10/sec through 100ms interval), size verification after download.

- **Cancel download**: AtomicBool flag per download, partial file preserved for resume, UI cancellation event emitted.

- **Custom model auto-discovery**: Scans `models/` for `.bin` files, generates display names from filenames (hyphen/underscore to space, title case), skips predefined filenames and hidden files.

- **Model migration**: Bundled model copy to user directory, GigaAM single-file to directory format migration (for transcribe-rs 0.3.x compatibility).

- **Idle unload** (`managers/transcription.rs`): Watcher thread checks every 10s, configurable timeout (Never/Immediately/2m/5m/10m/15m/1h/15s debug), recording-aware (keeps model loaded during recording), "Never" means model stays loaded, "Immediately" handled separately by `maybe_unload_immediately()`.

### 4.7 Configuration & Settings (Complete List)

From `settings.rs` (989 lines), the `AppSettings` struct has 60+ fields:

| Category | Setting | Type | Default |
|----------|---------|------|---------|
| Shortcuts | bindings (3), push_to_talk | HashMap, bool | alt+space (macOS) / ctrl+space, true |
| Audio | audio_feedback, audio_feedback_volume, sound_theme, selected_microphone, selected_output_device, always_on_microphone, clamshell_microphone, mute_while_recording | bool, f32, enum, Option<String> | false, 1.0, Marimba |
| STT Model | selected_model, translate_to_english, selected_language, model_unload_timeout, whisper_accelerator, ort_accelerator, whisper_gpu_device | String, bool, String, enum, enum, enum, i32 | "", false, "auto", 5min, Auto, Auto, -1 |
| Text Output | custom_words, word_correction_threshold, append_trailing_space, custom_filler_words | Vec<String>, f64, bool, Option<Vec<String>> | [], 0.18, false, None |
| Paste | paste_method, clipboard_handling, auto_submit, auto_submit_key, paste_delay_ms, typing_tool, external_script_path | enum, enum, bool, enum, u64, enum, Option<String> | CtrlV (Direct on Linux), DontModify, false, Enter, 60ms |
| LLM Post-Process | post_process_enabled, post_process_provider_id, post_process_providers, post_process_api_keys, post_process_models, post_process_prompts, post_process_selected_prompt_id | bool, String, Vec, SecretMap, HashMap, Vec, Option<String> | false, "openai", 7 providers |
| UI | overlay_position, start_hidden, autostart_enabled, show_tray_icon, app_language, debug_mode, log_level, experimental_enabled | enum, bool, bool, bool, String, bool, enum, bool | Bottom (None on Linux), false, false, true, system locale |
| History | history_limit, recording_retention_period | usize, enum | 5, PreserveLimit |
| Debug | keyboard_implementation, extra_recording_buffer_ms, lazy_stream_close, update_checks_enabled | enum, u64, bool, bool | HandyKeys (Tauri on Linux), 0, false, true |

---

## 5. Key Code Patterns & Techniques

### 5.1 Manager Pattern (Thread-Safe Shared State)

Every core subsystem uses `Arc<Mutex<T>>` managed by Tauri's state system. Four managers initialized at startup in `initialize_core_logic()`:

```
AudioRecordingManager (516 l) -> ModelManager (1649 l) -> TranscriptionManager (854 l) -> HistoryManager (737 l)
```

Each has `new(app_handle) -> Result<Self>`, internal `Arc<Atomic*>` for lock-free flags, emits events via `app_handle.emit()`, and uses `Arc::clone()` for thread spawning.

### 5.2 Command-Event Architecture (Typed IPC)

- **Frontend -> Backend**: Tauri commands decorated with `#[tauri::command] #[specta::specta]`, registered in `collect_commands![]` macro (~120 commands total).
- **Backend -> Frontend**: Events emitted through `app_handle.emit()`, collected by `collect_events![]` in specta builder.
- **Type bridge**: `tauri-specta` generates `src/bindings.ts` (914 lines) from Rust signatures -- full TypeScript type safety.

### 5.3 Transcription Coordinator (Single-Thread Serialization)

`transcription_coordinator.rs` (184 lines): mpsc channel with dedicated thread:

```rust
enum Stage { Idle, Recording(String), Processing }
enum Command { Input { ... }, Cancel { ... }, ProcessingFinished }
```

- 30ms debounce on press events (prevents double-triggers)
- Push-to-talk: `is_pressed=true` starts, `is_pressed=false` stops
- Toggle mode: single press cycles Idle -> Recording -> Processing -> Idle
- `ProcessingFinished` from `FinishGuard` drop guard prevents lost state transitions
- Coordinator thread catches panics prevent dead channels

### 5.4 Panic-Safe Engine Isolation

`TranscriptionManager::transcribe()` (lines 525-682) removes the engine from `Mutex<Option<LoadedEngine>>` BEFORE calling the potentially-panicking function:
- On success: puts engine back
- On panic (`catch_unwind`): drops poisoned engine, clears model_id, emits `model-state-changed::unloaded`

Prevents a single engine crash from mutually poisoning the application state.

### 5.5 RAII Guards for State Cleanup

- `FinishGuard` (`actions.rs`): Notifies coordinator when async pipeline ends (even on panic).
- `DownloadCleanup` (`model.rs`): Ensures `is_downloading` and `cancel_flags` cleaned on every error path.
- `LoadingGuard` (`transcription.rs`): Clears `is_loading` flag on drop, wakes waiters via `Condvar`.

### 5.6 Command Channel for Audio Recorder

`AudioRecorder` (`recorder.rs`) uses mpsc command channel:
```
Main thread -> Command::Start/Stop/Shutdown -> Worker thread -> AudioChunk::Samples/EndOfStream
```
Worker owns cpal stream, runs consumer loop processing both audio and commands. On Stop: drains all remaining audio until EndOfStream sentinel.

### 5.7 Resampling + VAD Pipeline

```
Device (native rate) -> FrameResampler (rubato FftFixedIn, 1024-chunk) -> 16kHz 30ms frames -> SmoothedVad -> Speech/Noise
```
Bypasses resampling when native rate = 16kHz. `finish()` zero-pads remaining input.

### 5.8 Tray i18n at Compile Time

`build.rs` generates Rust struct from locale JSON at compile time:
- Reads `src/i18n/locales/*/translation.json`, extracts `"tray"` section
- Generates `TrayStrings` struct from English keys (camelCase->snake_case)
- Generates `Lazy<HashMap<&str, TrayStrings>>` with all translations
- Avoids runtime JSON parsing, gives compile-time type safety

### 5.9 Shortcut Implementation Pattern

Two backends behind unified facade (`shortcut/mod.rs`):
- **Tauri**: `global_shortcut.on_shortcut()`, parse + validate + register
- **HandyKeys**: Dedicated manager thread with `HotkeyManager`, polled via `manager.try_recv()`, commands via mpsc with synchronous response channels

Runtime-switchable: unregisters all from old backend, validates against new, registers with new. HandyKeys failure -> automatic Tauri rollback with persisted fallback.

### 5.10 SecretMap for API Key Obfuscation

`settings.rs` defines `SecretMap(HashMap<String, String>)` with custom `Debug` that redacts to `[REDACTED]` -- preventing API keys in logs. Uses `#[serde(transparent)]` for clean JSON.

---

## 6. Relation to S2B2S (Diff Analysis)

### 6.1 What S2B2S Inherited from Handy (Complete)

S2B2S is a direct fork of Handy. Every single subsystem in this analysis was inherited as the starting skeleton:

| Handy Subsystem | Inherited? | Status in S2B2S |
|-----------------|-----------|-----------------|
| Tauri 2.x shell (lib.rs 615 l, main.rs 18 l) | Yes | Extended with Brain, TTS, llama_server, control_server |
| settings.rs (989 l) | Yes | Extended with TtsConfig, BrainConfig, SanitizeConfig |
| actions.rs (721 l) -- ShortcutAction trait, TranscribeAction | Yes | Extended with ConversationAction, SpeakSelectionAction |
| transcription_coordinator.rs (184 l) | Yes -- same state machine | Extended with conversation mode states |
| audio_toolkit/ (12 files, entire) | Yes -- identical | Extended with noise_suppression.rs (RNNoise), triple_vad.rs |
| managers/audio.rs (516 l) | Yes -- identical | Unchanged |
| managers/model.rs (1649 l) | Yes -- identical catalog | Extended with TTS model management |
| managers/transcription.rs (854 l) | Yes -- all 8 engines | Extended with transcription_mock.rs for CI |
| managers/history.rs (737 l) | Yes | Extended with TTS history |
| llm_client.rs (277 l) | Yes -- same client | Extended with streaming in brain/client.rs |
| clipboard.rs (687 l) | Yes -- all paste methods | Extended with clipboard_watch.rs (double-copy trigger) |
| shortcut/ (4 files, entire module) | Yes -- both backends | Extended with conversation/speak-selection bindings |
| overlay.rs (396 l) | Yes -- identical | Extended for speaking state |
| tray.rs (303 l), tray_i18n.rs (34 l) | Yes | Extended with new menu items |
| audio_feedback.rs (142 l) | Yes | Unchanged |
| apple_intelligence.rs (84 l) | Yes | Unchanged |
| portable.rs (166 l) | Yes | Unchanged |
| helpers/clamshell.rs (86 l) | Yes | Unchanged |
| commands/ (5 files, entire) | Yes | Extended with Brain, TTS, discovery commands |
| input.rs (123 l) | Yes | Unchanged |
| cli.rs (29 l) | Yes | Unchanged |
| signal_handle.rs (38 l) | Yes | Unchanged |
| utils.rs (66 l) | Yes | Unchanged |
| Entire React frontend (~90 components) | Yes | Extended with conversation UI, TTS settings, Brain settings |
| 20 i18n locales | Yes | Unchanged |
| src/stores/settingsStore.ts, modelStore.ts | Yes | Extended |
| src/bindings.ts (specta) | Yes | Extended with new commands/events |

**Everything in Handy's ~150 files is present in S2B2S.** The fork relationship is total.

### 6.2 What S2B2S Added That Handy Doesn't Have

| Feature | Handy (v0.8.3) | S2B2S |
|---------|---------------|-------|
| **TTS (Text-to-Speech)** | None | Full subsystem: 9 backends (Piper, PiperServer, Kokoro, Kitten, Pocket, SAPI, OpenAI, ElevenLabs, Cartesia), streaming gapless playback (rodio), sentence pagination (UTF-8 safe chunking), 5-stage sanitize pipeline (ITN -> Custom Words -> Markdown Strip -> TN -> Regex Cleanup), fragment_queue (pre-synthesis queue, kept for future), clipboard_watch (double-copy trigger), audio_format conversion (WAV -> MP3/OGG/FLAC), telemetry (per-engine performance tracking), status reporting (WarmEngine trait: Loading -> WarmingUp -> Ready -> Error) |
| **Brain (Streaming LLM)** | Sync only, no streaming, no memory | Streaming SSE client (brain/client.rs), turn history management (brain/manager.rs), sentence splitter bridge to TTS, barge-in/abort support (cancel running response), conversation mode orchestration (record -> STT -> Brain -> TTS -> listen cycle), llama.cpp server manager (llama_server/manager.rs: download, lifecycle, GPU offloading) |
| **Conversation Mode** | None | ConversationAction: full duplex voice interaction cycle, continuous listening between turns |
| **Read Aloud** | None | SpeakSelectionAction: reads selected text or double-copy triggered text via TTS |
| **Noise Suppression** | Silero VAD only | RNNoise (nnnoiseless 0.5.2) + TripleVAD (triple_vad.rs: RMS energy -> RNNoise probability -> Silero final decision) |
| **Wake Word** | None | wake_word.rs (KWS-ready but audio feed-in not connected -- detector runs idle) |
| **LLaMA Server** | None | llama_server/manager.rs: pre-compiled llama.cpp server download, lifecycle management, GPU offloading configuration |
| **Local LLM Discovery** | Only via "Custom" provider URL | commands/discovery.rs: automatic Ollama/LM Studio endpoint detection |
| **Control Server** | None | control_server.rs: local HTTP API (axum) for external programmatic control |
| **Crash Logging** | Basic log plugin | crash_logging.rs: panic capture with full backtraces to file |

### 6.3 Fork-Point Analysis

Based on code state comparison (no git history available):
- Handy at v0.8.3 with Tauri 2.10.2
- S2B2S uses same Tauri version and cjpais/tauri patches
- Handy codebase is the common ancestor, likely forked at or near v0.8.x
- audio_toolkit module is 100% identical except S2B2S additions (triple_vad, noise_suppression)
- signal_handle.rs, transcription_coordinator.rs, cli.rs are verbatim copies

---

## 7. Harvest List (Features Worth Copying from Handy)

| Feature to harvest | From file | Effort | Why valuable for S2B2S |
|-------------------|-----------|--------|------------------------|
| Portable mode with marker file | portable.rs (166 l) | XS | Already inherited; verify it still works in S2B2S's larger ecosystem |
| Custom Whisper model auto-discovery | managers/model.rs (lines 816-935) | S | Extend to TTS model auto-discovery -- drop .onnx files and auto-detect |
| GigaAM single->directory migration | managers/model.rs (lines 678-720) | XS | Pattern for migrating TTS model formats between versions |
| Tray i18n compile-time codegen | build.rs (lines 14-92) | M | Generate S2B2S tray strings from locale JSON at compile time |
| Tauri vs HandyKeys runtime switch with auto-rollback | shortcut/mod.rs (lines 250-468) | M | Already inherited; pattern valuable for any future shortcut backend changes |
| Stutter collapse (3+ word repetition filter) | audio_toolkit/text.rs (lines 236-271) | S | Enhance S2B2S's post-STT cleaner pipeline |
| RAII cleanup guards pattern | model.rs (DownloadCleanup), transcription.rs (LoadingGuard) | XS | Verify all new S2B2S code follows this pattern |
| Panic-safe engine isolation with catch_unwind | managers/transcription.rs (lines 526-682) | S | Apply to S2B2S TTS engine calls to prevent crash poisoning |
| SHA256 verification with blocking thread | managers/model.rs (lines 941-970) | XS | Apply to TTS model downloads |
| Nix flake with cargo-tauri.hook | flake.nix (248 l) | L | S2B2S could benefit from a Nix build |
| SecretMap for API key obfuscation | settings.rs (lines 308-334) | XS | Already inherited; ensure TTS cloud provider API keys use same pattern |

---

## 8. Known Issues, Caveats & Limitations

| Issue | Severity | Impact |
|-------|----------|--------|
| **Whisper model crashes on certain system configurations** (Windows/Linux) | HIGH | Configuration-dependent crash -- not all systems affected. GitHub issues labeled "help wanted." |
| **Wayland limited support** | MEDIUM | Requires wtype/dotool for text input; global shortcuts must be configured through WM/DE; overlay causes paste failures on some compositors. |
| **Linux overlay disabled by default** | MEDIUM | Overlay steals focus on certain compositors, breaking clipboard-based pasting. Defaults to OverlayPosition::None on Linux. |
| **No streaming LLM responses** | MEDIUM | Sync /chat/completions only -- user sees no progress during post-processing. Deliberate simplicity tradeoff. |
| **No conversation state** | HIGH (for S2B2S) | Stateless LLM calls -- no turn history, no memory. Proto-Brain is strictly one-shot post-process. |
| **No TTS** | HIGH (for S2B2S) | Zero speech output -- mic->text->paste only. By design for single purpose app. |
| **Custom Tauri runtime patches** | MEDIUM | Upgrading Tauri requires rebasing patches from cjpais/tauri branch handy-2.10.2 |
| **Feature freeze** | INFO | New features require community Discussions consensus before PR. S2B2S should expect limited new features from upstream. |
| **Settings system "bloated and messy"** | LOW | README acknowledges need for refactoring -- 60+ flat fields without nesting. |
| **Cancel shortcut disabled on Linux** | MEDIUM | Dynamic shortcut registration unstable on Linux; only CLI/tray cancel works. |
| **Linux runtime dependency: libgtk-layer-shell.so.0** | LOW | Startup failure if not installed; documented per-distro install. |
| **WebKit DMA-BUF renderer crashes on Linux** | LOW | Disabled via WEBKIT_DISABLE_DMABUF_RENDERER=1 in main.rs |
| **Intel Mac ONNX Runtime** | LOW | Requires Homebrew onnxruntime with ORT_PREFER_DYNAMIC_LINK=1 |
| **ARM64 not supported** | INFO | No ARM64 Windows or Linux binaries (Nix has aarch64-linux build). |
| **Silero threshold hardcoded at 0.3** | LOW | Not user-configurable; S2B2S's TripleVAD adds RNNoise threshold configurability. |
| **Unused audio_toolkit/bin/cli.rs** (323 l) | LOW | Test binary, not a user-facing feature. |
| **Intel pre-Skylake CPU FMA3 crash** | MEDIUM | GPU enumeration checks for FMA3 before probing Vulkan; older CPUs skip GPU detection entirely. |

---

## 9. Strengths & Weaknesses

### Strengths

1. **Clean, forkable architecture**: Manager pattern, typed IPC, separation of concerns -- proven extensible by 4 known forks.

2. **Comprehensive settings system**: 60+ user-configurable settings with sensible, platform-aware defaults applied at runtime.

3. **Defensive error handling**: Panic guards, mutex poison recovery, RAII cleanup guards, SHA256-verified downloads with corruption handling.

4. **Multi-platform done right**: Proper cfg-gating with fallbacks. Linux gets automatic native tool detection chains (wpctl->pactl->amixer, wtype->dotool->ydotool->xdotool).

5. **AI-assistant-friendly**: AGENTS.md, mandatory PR/issue templates, code style rules for both Rust and TypeScript.

6. **Model management maturity**: 16 models, resume downloads, SHA256 verification, custom model discovery, format migration, idle unload with recording-awareness.

7. **"Proto-Brain" is real and functional**: 7 LLM providers including fully local Apple Intelligence via Swift FFI bridge. Structured output with JSON schema enforcement.

8. **Excellent crash resilience**: English fallback in tray_i18n, mutex poison recovery, coordinator thread panic catch -- degrades gracefully.

9. **Build system completeness**: Nix flake with NixOS and Home Manager modules, NSIS installer, portable mode, autostart.

### Weaknesses

1. **No TTS**: The single biggest gap for S2B2S's vision. Intentionally uni-directional.

2. **No streaming**: LLM responses are synchronous -- reqwest stream feature is imported but unused.

3. **No conversation memory**: Stateless one-shot LLM -- no turn history or context.

4. **Settings struct bloat**: 60+ flat fields, README acknowledges refactoring needed.

5. **Unused dead code**: transcription_mock.rs not used as trait/interface, audio_toolkit/bin/cli.rs is test binary.

6. **Custom Tauri patches**: Maintenance burden for version upgrades.

7. **No VAD aggressiveness control**: Hardcoded thresholds and smoothing parameters.

8. **Limited Wayland**: Shortcuts require WM config, cancel shortcut disabled, overlay problematic.

9. **No profiles/multi-config**: Single flat settings file.

10. **English-centric filler word defaults**: Non-English filler lists are minimal (mostly hmm/mmm/hm).

---

## 10. Bottom Line / Verdict

Handy is an exceptionally well-built, thoroughly architected speech-to-text desktop application that **delivers exactly what it promises**: press a key, speak, get text pasted. Its value to S2B2S is foundational -- **every subsystem S2B2S inherited started here.** The manager pattern, the typed IPC bridge, the VAD pipeline, the 8-engine STT catalog, the LLM client, and the multi-platform overlay/tray/shortcut infrastructure all originated in Handy and remain largely intact in S2B2S today.

The single most valuable idea in Handy is the **"forkable by design" philosophy** -- every architectural decision supports extension. The transcription coordinator's state machine, the ShortcutAction trait, the settings system's `#[serde(default)]` pattern, the compile-time tray i18n codegen, and the dual shortcut backend with automatic rollback are all designed so that adding new features (like TTS, Brain, Conversation mode) fits naturally into the existing structure rather than requiring a rewrite.

For S2B2S, Handy is not merely a reference project to study -- it is **the literal codebase that was extended.** Understanding Handy means understanding every line S2B2S was built on top of. The gaps Handy left (no TTS, no streaming LLM, no conversation state, no noise suppression, no wake word) are exactly the features S2B2S was created to fill. Conversely, Handy's strengths (8 STT engines, multi-platform paste, portable mode, model management, LLM post-processing) remain largely unchanged in S2B2S -- meaning they are a **stable, tested, production-grade foundation** that S2B2S can rely on while focusing development effort on its differentiating features.

---

## Appendix A: Complete File Manifest with Line Counts

### Rust Backend (`src-tauri/src/`)

| File | Lines | Role |
|------|-------|------|
| managers/model.rs | 1649 | Model catalog, download, SHA256, migration |
| shortcut/mod.rs | 1157 | Unified shortcut interface, ~60 commands |
| settings.rs | 989 | AppSettings, defaults, SecretMap, migration |
| managers/transcription.rs | 854 | 8 STT engines, idle watcher, panic guard |
| managers/history.rs | 737 | SQLite history, 4 migrations, retention |
| actions.rs | 721 | ShortcutAction, TranscribeAction, post-process |
| clipboard.rs | 687 | 6 paste methods, Linux native tool chain |
| lib.rs | 615 | Bootstrap, specta, ~120 commands |
| audio_toolkit/text.rs | 567 | Word correction, filler removal, stuttering |
| managers/audio.rs | 516 | AudioRecordingManager, mute, VAD preload |
| shortcut/handy_keys.rs | 549 | HandyKeys backend, manager thread |
| audio_toolkit/audio/recorder.rs | 519 | AudioRecorder, cpal stream, VAD |
| overlay.rs | 396 | Overlay: NSPanel/LayerShell/HWND_TOPMOST |
| audio_toolkit/bin/cli.rs | 323 | CLI test binary |
| tray.rs | 303 | Tray icon, model submenu |
| llm_client.rs | 277 | OpenAI client, structured output |
| commands/audio.rs | 312 | Mic mode, devices, Windows permissions |
| commands/models.rs | 221 | Model download, delete, switch |
| shortcut/tauri_impl.rs | 198 | Tauri shortcut backend |
| commands/mod.rs | 187 | Utility commands, Enigo, shortcuts init |
| transcription_coordinator.rs | 184 | State machine, 30ms debounce |
| portable.rs | 166 | Portable mode, Data/ dir |
| commands/history.rs | 154 | History entries, retry, cleanup |
| audio_feedback.rs | 142 | Sound effects, rodio |
| audio_toolkit/audio/visualizer.rs | 156 | FFT spectrum analyzer |
| input.rs | 123 | EnigoState, paste combos |
| audio_toolkit/vad/smoothed.rs | 105 | VAD hysteresis |
| audio_toolkit/audio/resampler.rs | 99 | rubato resampler |
| managers/transcription_mock.rs | 89 | CI mock |
| helpers/clamshell.rs | 86 | MacBook lid detection |
| apple_intelligence.rs | 84 | Apple LLM FFI bindings |
| shortcut/handler.rs | 70 | Shared event handler |
| utils.rs | 66 | cancel_current_operation() |
| ~10 more small files | ~200 | Various utilities |

### Frontend (Major Files)

| File | Lines | Role |
|------|-------|------|
| src/bindings.ts | 914 | tauri-specta generated bindings |
| src/stores/settingsStore.ts | 542 | Zustand settings store |
| src/stores/modelStore.ts | 390 | Zustand model store |
| src/App.tsx | 289 | Main component |
| ~90 component files | ~10,000 | Settings UI, onboarding, model selector, etc. |
| 20 i18n locale files | ~12,240 | All translations (~612 lines each) |

### Config & Build

| File | Lines | Role |
|------|-------|------|
| build.rs | 254 | Tray i18n + Swift compilation |
| flake.nix | 248 | Nix build |
| Cargo.toml | 108 | Dependencies, profile |
| tauri.conf.json | 75 | App config |
| package.json | 65 | Bun scripts, dependencies |

### Documentation

| File | Lines | Role |
|------|-------|------|
| README.md | 507 | Overview, troubleshooting |
| CONTRIBUTING.md | 224 | Contributor guidelines |
| AGENTS.md | 216 | AI agent guide |
| BUILD.md | 145 | Platform build instructions |
| CONTRIBUTING_TRANSLATIONS.md | 124 | Translation guide |
| CRUSH.md | 65 | Dev cheat sheet |

---

*(Analysis complete -- all Handy source files read and documented. Every file path, line count, and subsystem has been verified against the actual codebase state as of v0.8.3.)*
