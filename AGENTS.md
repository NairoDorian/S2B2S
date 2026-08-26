# AGENTS.md

This file provides guidance to AI coding assistants working with code in this repository.

> **NOTE**: This is the `Handy_Multi_STT` fork. In addition to upstream Handy
> features, this branch adds **Multi-STT** mode — running up to four speech-to-text
> models in parallel and optionally merging their outputs via an LLM. See the
> Architecture Overview and Settings System sections below for fork-specific
> additions.

## Development Commands

**Prerequisites:**

- [Rust](https://rustup.rs/) (latest stable)
- [Bun](https://bun.sh/) package manager

**Core Development:**

```bash
# Install dependencies
bun install

# Run in development mode
bun run tauri dev
# If cmake error on macOS:
CMAKE_POLICY_VERSION_MINIMUM=3.5 bun run tauri dev

# Build for production
bun run build:fast  # Fast local build (auto-detects local GPU arch)
bun run build:full  # Full multi-arch distribution build

# Frontend only development
bun run dev        # Start Vite dev server
bun run build      # Build frontend (TypeScript + Vite)
bun run preview    # Preview built frontend
```

**Linting and Formatting (run before committing):**

```bash
bun run typecheck         # tsc -b
bun run lint              # oxlint (loads eslint-plugin-i18next through jsPlugins)
bun run lint:fix          # oxlint with auto-fix
bun run format            # Prettier + cargo fmt
bun run format:check      # Check formatting without changes
bun run format:frontend   # Prettier only
bun run format:backend    # cargo fmt only
bun run check:translations # every locale must have exactly en's keys (CI gate)
cd src-tauri && cargo clippy --all-targets && cargo test --all-targets
```

**Maintenance scripts (`scripts/`):**

| Script                     | Invoked by                                    | Purpose                                                                                                                                                     |
| -------------------------- | --------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `tauri-runner.ts`          | `bun run tauri`, `build:fast`, `build:full`   | Wraps the Tauri CLI; runs the transcribe.cpp pin check first; `--fast`/`--local-gpu` sets `TRANSCRIBE_CUDA_ARCHITECTURES=auto`                              |
| `check-transcribe-deps.ts` | `tauri-runner.ts` (imported), or run directly | Bumps the `transcribe-cpp` / `transcribe-cpp-sys` git pin in `Cargo.lock` when `NairoDorian/transcribe.cpp` `main` moves; never fails, never blocks a build |
| `check-translations.ts`    | `bun run check:translations`, CI              | Compares every locale's key set with `en`                                                                                                                   |
| `check-nix-deps.ts`        | `postinstall`                                 | Regenerates `.nix/bun.nix` via bun2nix when available (no-op on Windows). Re-run on a Nix machine after changing `package.json`                             |
| `update-deps.ts`           | `bun run update-deps [--prerelease]`          | Bumps npm and Cargo dependencies with validation steps. Run it from the repository root (paths resolve against the CWD)                                     |
| `update-rtk.ts`            | `bun run update:rtk`                          | Updates the RTK CLI used by the maintainer's Claude Code hook — tooling, not part of the app                                                                |
| `gen_catalog.py`           | manual                                        | Regenerates `src-tauri/src/catalog/catalog.json` (upstream tooling)                                                                                         |

**Model Setup:** the Silero VAD model
(`src-tauri/resources/models/silero_vad_v6.2.onnx`, ~2.2 MB) is **committed
to the repository** — nothing needs downloading. Only re-fetch it from
`https://huggingface.co/BricksDisplay/silero-vad-6.2/resolve/main/onnx/model.onnx`
if you deliberately upgrade the VAD, and re-run the probe test afterwards.

**Native build flags:** `.cargo/config.toml` sets `TRANSCRIBE_CMAKE_ARGS` for
every native build — it disables sccache for ggml (corrupts objects on
MSVC) and passes the CUDA 13.3 / MSVC 2026 flags. Windows x86_64 and Linux
build the transcribe.cpp **`cuda`** feature (not upstream's `vulkan`), macOS
`metal`, Windows aarch64 CPU only.

For detailed platform-specific build setup, see [BUILD.md](BUILD.md).

## Architecture Overview

Handy is a cross-platform desktop speech-to-text application built with Tauri 2.x (Rust backend + React/TypeScript frontend).

### Backend Structure (src-tauri/src/)

- `lib.rs` - Main entry point, Tauri setup, manager initialization, the
  `collect_commands!` list (every `#[tauri::command]` must be listed there to
  reach `src/bindings.ts`), and the headless `--transcribe-file` path
- `main.rs` - CLI parsing, calls `lib::run`
- `managers/` - Core business logic:
  - `audio.rs` - Audio recording and device management (`AudioRecordingManager`:
    recording state machine, readiness generation, lazy mic close / idle
    timeout, VAD backend switching, `last_speech_ms()`)
  - `model.rs` - Model catalog, downloading (HF hub or mirror), quantization
    variants; `model/download.rs` is the HTTP downloader
  - `model_capabilities.rs`, `gguf_meta.rs` - Per-model capability detection
    and GGUF metadata parsing
  - `native_streaming_latency.rs` - Maps the latency preset onto the
    transcribe.cpp `StreamExtension` a model family supports
  - `transcription.rs` - Speech-to-text pipeline: primary engine, stream
    worker, `extra_engines` (Multi-STT), idle unload watcher
  - `history.rs` - Transcription history storage (SQLite), WAV files, vacuum
- `audio_toolkit/` - Low-level audio processing:
  - `audio/` - Device enumeration, recording, resampling, WAV read/write
    (`utils.rs`: `save_wav_file`, `save_raw_wav_file`, `read_wav_samples`,
    `verify_wav_file`), level-meter `visualizer.rs`
    - `recorder.rs` also hosts `SpeechClock`, which measures how long the user
      has actually been speaking (see Speech Stats below)
  - `vad/` - Voice Activity Detection — see Voice Activity Detection below;
    `silero.rs` drives the ONNX session directly rather than through `vad-rs`,
    `earshot.rs` is the alternative backend, `smoothed.rs` adds prefill /
    hangover / onset smoothing, `mod.rs` holds the millisecond timing constants
  - `lang_id.rs`, `text.rs` - Language-detection heuristics and text post-filters
  - `bin/cli.rs` - Standalone recorder demo. **Not a build target** (the
    `[[bin]]` in `Cargo.toml` is commented out); keep it compiling by hand
- `commands/` - Tauri command handlers for frontend communication
  (`audio.rs`, `history.rs`, `models.rs`, `transcription.rs`, `mod.rs`)
- `cli.rs` - CLI argument definitions (clap derive)
- `shortcut/` - Global keyboard shortcut handling. `mod.rs` holds the
  settings-change commands and `should_register_binding`, the single rule for
  which bindings are live (feature gates + performance-mode conflicts);
  `tauri_impl.rs` and `handy_keys.rs` are the two backends; `handler.rs`
  dispatches to actions
- `transcription_coordinator.rs` - Pure state machine that turns key presses
  / external inputs into start/stop/cancel decisions (toggle vs push-to-talk)
- `settings.rs` - Application settings management, defaults, schema
  migrations (includes Multi-STT settings)
- `overlay.rs` - Recording overlay window (platform-specific), plus
  `SpeechActivityEvent` and the cached emit gates for `mic-level` /
  speech-activity events
- `signal_handle.rs` - `send_transcription_input()` reusable function
- `actions.rs` - Shortcut action implementations: `TranscribeAction`, `MultiSttAction`, `CancelAction`
  - `MultiSttAction` runs primary + three extra models in parallel, merges outputs via LLM
  - `spawn_recording_ready_cue` is the shared "wait for real mic samples, then
    chime / mute / emit `recording-ready`" step
- `direct_stream_writer.rs` - Types the live transcript into the target app
  for `PasteMethod::DirectStreaming` — plain transcription only; see Direct
  Streaming below
- `llm_client.rs` - OpenAI-compatible API client for post-processing and Multi-STT merge
  - Includes `erase_llama_server_conversations()` for llama.cpp conversation cleanup (only called for the `custom` provider)
- `clipboard.rs`, `paste_tx/` - Paste strategies (clipboard, direct typing,
  receipt-sequenced "reliable" paste on Windows/macOS), key-combo simulation
- `input.rs` - Keyboard simulation (`simulate_shortcut`) used by performance mode
- `secure_input.rs` - macOS Secure Input detection and fallback bindings
- `tray.rs`, `tray_i18n.rs` - System tray and its generated translations
- `autostart.rs`, `portable.rs`, `apple_intelligence.rs`, `audio_feedback.rs`,
  `memory.rs`, `helpers/clamshell.rs` - Platform helpers
- `catalog/` - Bundled model catalog (`catalog.json`)
- `utils.rs` - Platform detection helpers, `cancel_current_operation`,
  Windows real-time process setup (`init_windows_process_performance`)
- `tests/vad_speech_clock_probe.rs` - Opt-in VAD regression probe (`HANDY_PROBE_WAV`)

### Frontend Structure (src/)

- `App.tsx` - Main component with onboarding flow
- `components/` - React UI components:
  - `settings/` - Settings UI, grouped by page (`general/`, `advanced/`,
    `models/`, `history/`, `post-processing/`, `about/`, `debug/`) plus the
    individual setting components. Fork-added ones: `AccentColorSelector`,
    `AppendTrailingNewline`, `KeyComboInput` (performance-mode shortcut
    recorder), `MicIdleTimeout`, `SaveRawAudio`, `SpeechStats`,
    `VadBackendSelector` (upstream), `PasteMethod` (direct streaming +
    speed), `ShowOverlay` (direct mode + speed)
    - `multi-stt/MultiSttSettings.tsx` - Multi-STT configuration (fork feature)
  - `model-selector/` - Status-bar model controls (footer). Three mutually
    exclusive popovers: model switcher, quantization picker
    (`QuantizationPanel.tsx`, which also hosts the quantization benchmark via
    `useQuantBenchmark.ts`), and native streaming latency (`LatencyPanel.tsx`).
    There is deliberately no benchmark settings page — the benchmark lives
    beside the quant list it compares.
  - `onboarding/` - First-run experience
  - `whats-new/` - Release-notes modal (`releaseNotes.ts` globs
    `src/content/release-notes/*.md`)
  - `update-checker/` - App update notifications (`portableInstaller.ts`)
  - `shared/`, `ui/`, `icons/`, `footer/` - Shared components
- `hooks/useSettings.ts` - Settings state management hook; `hooks/useOsType.ts`
- `stores/settingsStore.ts` - Zustand store for settings. **Every settings key
  needs an entry in `settingUpdaters`** — a key without one logs
  `No handler for setting` and is never persisted
- `stores/modelStore.ts` - Model store with load/unload operations
- `bindings.ts` - Auto-generated Tauri type bindings (via tauri-specta; written
  by `bun run tauri dev` in debug builds). When a command or settings field is
  added or removed, the file must be regenerated or hand-edited to match
- `overlay/` - Recording overlay window entry point (`RecordingOverlay.tsx`,
  `main.tsx`, `index.html`); there is no `components/overlay/`
- `lib/types/events.ts` - Shared TypeScript event payload types;
  `lib/utils/{color,theme,keyboard,format,rtl,modelTranslation}.ts`,
  `lib/constants/languages.ts`, `lib/compat.ts`

### Key Architecture Patterns

**Manager Pattern:** Core functionality organized into managers (Audio, Model, Transcription) initialized at startup and managed via Tauri state.

**Command-Event Architecture:** Frontend → Backend via Tauri commands; Backend → Frontend via events.

**Pipeline Processing:** Audio → VAD → Whisper/Parakeet → Text output → Clipboard/Paste

**State Flow:** Zustand → Tauri Command → Rust State → Persistence (tauri-plugin-store)

### Technology Stack

**Core Libraries:**

- `transcribe-cpp` - Local Whisper-family inference (GGML/GGUF) with GPU acceleration
- `transcribe-rs` - ONNX speech recognition (Parakeet, Moonshine, SenseVoice, etc.)
- `cpal` - Cross-platform audio I/O
- `ort` - ONNX Runtime bindings; runs the Silero VAD graph directly (see
  Voice Activity Detection)
- `earshot` - Alternative pure-Rust VAD backend (`vad_backend = earshot`)
- `enigo` - Keystroke simulation for the performance-mode shortcuts
- `rdev` - Global keyboard shortcuts
- `rubato` - Audio resampling
- `rodio` - Audio playback for feedback sounds

### Application Flow

1. **Initialization:** App starts minimized to tray, loads settings, initializes managers
2. **Model Setup:** First-run downloads the chosen model from the bundled catalog (`catalog/catalog.json`: Whisper-family GGUF and ONNX models such as Parakeet)
3. **Recording:** Global shortcut triggers audio recording with VAD filtering
4. **Processing:** Audio sent to Whisper model for transcription
5. **Output:** Text pasted to active application via system clipboard

**Multi-STT mode** (fork feature): When `multi_stt_enabled` is on, the dedicated `multi_stt_transcribe` shortcut triggers a parallel transcription pipeline:

1. Pre-loads extra models (if not already loaded) in parallel on the blocking thread pool
2. Records audio with VAD filtering (same as standard mode)
3. Transcribes with primary + three extra models concurrently on separate engines
4. Optionally merges outputs via an LLM (using `${output}`, `${output2}`, `${output3}`, and `${output4}` placeholders)
5. Saves history entry and pastes merged result concurrently

### Settings System

Settings are stored using Tauri's store plugin with reactive updates:

- Keyboard shortcuts (configurable, supports push-to-talk)
- Audio devices (microphone/output selection)
- Model preferences (Small/Medium/Turbo/Large Whisper variants)
- Audio feedback and translation options

### Voice Activity Detection

Handy ships **Silero VAD v6.2** (`silero_vad_v6.2.onnx`, ~2.2 MB), run on the
audio consumer thread once per 32 ms frame. It is what makes `vad_enabled`
mean anything: frames it scores below threshold never reach the decoder, so a
recording carries speech instead of speech-plus-silence. That matters more than
it sounds — Whisper-family models are prone to hallucinating text on silent
audio, and every second of dropped silence is a second the model doesn't spend
decoding.

`audio_toolkit/vad/silero.rs` runs the Silero ONNX graph directly via `ort`.
It deliberately does **not** use `vad-rs`: that crate only speaks Silero **v4**'s
tensor interface (`input`/`sr`/`h`/`c` in, `hn`/`cn` out), while the model
shipped here is **v6** (`input`/`state`/`sr` in, `output`/`stateN` out). Against
a v6 model every `vad-rs` call fails with `Invalid input name: h`.

Three parts of the v5/v6 contract must all hold, and each fails quietly on its
own:

1. **Window size is fixed** at `constants::VAD_FRAME_SAMPLES` (512 samples =
   32 ms at 16 kHz). Other sizes still run, but return ~0.0005 for speech and
   silence alike. The capture pipeline frames itself to the active detector's
   `frame_samples()` (512 / 32 ms for Silero, 256 / 16 ms for Earshot, 480 /
   30 ms with VAD off) so one resampled frame is exactly one VAD decision;
   hangover, prefill and onset are millisecond constants in `vad/mod.rs`
   converted with `frames_for_duration_ms` (rounding up), so switching
   backends never shortens them.
2. **A 64-sample context** (`constants::VAD_CONTEXT_SAMPLES`) from the previous
   window must be prepended to each chunk, so the tensor fed is 576 long. Omit
   it and the model cannot distinguish speech from silence at all.
3. **`state` must be carried forward** from each run's `stateN` output, and
   cleared on `reset()` so a new recording starts fresh.

**Thresholding uses hysteresis**, as the reference pipeline does: speech is
entered at `VAD_THRESHOLD` (0.3) but only left once the probability falls below
`max(threshold - 0.15, 0.01)` — 0.15 here. A single value for both edges makes a
signal hovering near it flap frame to frame, which shows up directly as a
stuttering speech/silence indicator and a speech clock that stalls mid-word. The
floor keeps the exit threshold above the ~0.0005 the model emits for true
silence. `Hysteresis` in `silero.rs` is a pure struct, unit-tested without the
model.

**Recordings with no speech never reach a decoder.** `SpeechClock` publishes its
running total to an `Arc<AtomicU64>`, which `AudioRecorder::speech_ms()` and
`AudioRecordingManager::last_speech_ms()` expose; both shortcut actions check it
alongside `samples.is_empty()` and skip transcription below
`MIN_SPEECH_MS_TO_TRANSCRIBE` (200 ms, `actions.rs`). This is not just a saved
GPU decode: Whisper-family models hallucinate confidently on silence, and that
invented text would be pasted into whatever the user was typing in. The
threshold sits deliberately below Silero's own 250 ms `min_speech_duration_ms`
so a clipped "yes" still transcribes, and with VAD disabled every frame counts
as speech, so it can never suppress a recording made with filtering off.

`SileroVad::new` validates the model's input names up front, so a mismatched
model fails loudly at load instead of once per frame. `handle_frame` still
treats a per-frame VAD error as "keep this audio" (losing speech is worse than
keeping silence), but now logs the first failure of each recording — a VAD that
fails on every frame otherwise looks exactly like a VAD that is switched off.

**Why this is written out at length:** the v4-wrapper-against-a-v6-model
combination above was live for an unknown period and produced _no_ symptom a
user could see. `compute()` failed on every single frame, `handle_frame` mapped
the error to "keep this audio", and VAD degraded into a pass-through — the
setting appeared to work, recordings just silently contained everything. It only
surfaced when Speech Stats became the first consumer of the raw per-frame
verdict, which a pass-through cannot fake. Measured afterwards on a real 33.8 s
recording: 24.6 s of raw voiced audio, 32.9 s kept once the streaming hangover
tail is included.

`src-tauri/tests/vad_speech_clock_probe.rs` is an opt-in regression check: point
`HANDY_PROBE_WAV` at a 16 kHz mono speech recording and it asserts the real
chain reports a sane fraction of voiced frames. It skips when the variable is
unset.

**Speech Stats** (fork addition): the recording overlay can show a
speech/silence indicator, a timer that runs only while you are actually talking,
and the running average words per minute.

- Driven by `SpeechClock` in `audio_toolkit/audio/recorder.rs`, fed by
  `VoiceActivityDetector::last_frame_voiced` — the **raw** per-frame Silero
  verdict, deliberately _not_ what `push_frame` returns. `SmoothedVad` keeps
  reporting speech through a hangover tail of ≈1.66 s in streaming mode
  (`VAD_STREAMING_HANGOVER_MS` = 1650 rounded up to 52 Silero frames), and
  counting that would add more than a second of phantom speech per pause.
- Gaps shorter than `speech_pause_hold_ms` are billed as part of the same
  utterance once speech resumes; longer gaps are dropped. The clock therefore
  only moves forward — it catches up rather than rewinding.
- Reaches the overlay as `SpeechActivityEvent`, emitted on speaking/silent flips
  and on a ~150 ms heartbeat while speech continues (never during silence), so it
  adds roughly a fifth of the `mic-level` event volume. Gated on both
  `OVERLAY_ENABLED` and `SPEECH_STATS_ENABLED` to keep the issue #1279 emit path
  quiet when the overlay is off.
- Words per minute is computed in `RecordingOverlay.tsx` from the live
  `StreamTextEvent` word count, so it needs a streaming-capable model. It works
  in the Minimal overlay too, because `start_stream()` follows model capability
  rather than overlay style. Non-streaming models show the timers and the
  indicator with `—` in place of the rate.
- With VAD disabled every frame counts as speech, so the speech clock degrades
  into a wall clock instead of needing a separate UI case.

Settings:

- `overlay_speech_stats` - Show the stats in the Minimal and Live overlays
- `speech_pause_hold_ms` - Pause tolerance (10-2000 ms, default 500)
- `save_raw_audio` - Save raw uncompressed microphone audio before resampling and VAD filtering

**Raw Uncompressed Audio Recording** (fork addition):
When `save_raw_audio` is enabled in Settings $\rightarrow$ Advanced $\rightarrow$ History, Handy saves audio in its native captured format:

- **Pre-resampling / pre-VAD capture**: Raw samples are tapped in the capture callback after channel selection/averaging (so the file is always mono) but before resampling to 16 kHz and before Silero VAD filtering. The tap only accumulates when the setting is on — at 48 kHz float it is ~11 MB per minute.
- **Format follows the opened stream**: Saves 32-bit float (`F32`), 24-bit PCM (`I32`), or 16-bit PCM (`I16`) WAV files at the sample rate and format `get_preferred_config` opened the device with (F32 preferred, then I16, then I32; WASAPI shared mode is typically F32 at the mix rate, e.g. 48 kHz).
- **Zero latency impact**: WAV serialization runs asynchronously on background blocking threads via `tauri::async_runtime::spawn_blocking`, allowing model transcription inference to execute immediately in parallel with no latency overhead. Both actions still await and `verify_wav_file` the result before recording a history row.
- **Universal reader**: `read_wav_samples` decodes 16-bit, 24-bit, and 32-bit float WAVs, automatically downsampling to 16 kHz for playback, acoustic model inference, and benchmarks.

**Multi-STT settings** (fork addition):

- `multi_stt_enabled` - Global toggle for multi-model transcription mode
- `multi_stt_model_2` / `multi_stt_model_3` / `multi_stt_model_4` - Select extra STT models
- `multi_stt_language_model_2` / `multi_stt_language_model_3` / `multi_stt_language_model_4` - Per-model language override
- `multi_stt_translate_model_2` / `multi_stt_translate_model_3` / `multi_stt_translate_model_4` - Per-model English translation
- `multi_stt_keep_extra_models_loaded` - Keep extra models resident between uses (default on). Only consulted when `model_unload_timeout` is `Immediately`; with any other timeout the idle watcher unloads extra engines together with the primary model
- `multi_stt_merge_prompt` - LLM prompt for merging outputs (`${output}`, `${output2}`, `${output3}`, `${output4}`; `${output1}` is an alias of `${output}`)
- `multi_stt_performance_mode_enabled` / `multi_stt_performance_mode_trigger_on_start` - Simulate a "full power" shortcut when a Multi-STT recording ends (or starts, with trigger-on-start) and a "normal" shortcut after the merge/paste, for external power-profile tools
- `multi_stt_performance_mode_full_power_shortcut` (default `ctrl+space`) / `multi_stt_performance_mode_normal_shortcut` (default `ctrl+alt+space`) - The simulated key combinations. While performance mode is enabled, a transcription hotkey equal to either is not registered (`shortcut::should_register_binding`) and the shortcut recorder rejects it, so the simulated keys can never retrigger Handy

Extra models are managed by `TranscriptionManager` (`extra_engines` HashMap) with explicit
load/unload lifecycle, separate from the primary model. They are unloaded when Multi-STT is
turned off, when their slot changes, when the model is deleted, and by the idle watcher.

**Shortcut defaults** (fork): `transcribe` and `multi_stt_transcribe` ship with an empty
`current_binding` (`default_binding` is kept for "reset"), and settings schema migrations
3/4 cleared existing users' bindings once. Fresh installs therefore have no transcribe
hotkey until the user sets one — a deliberate consequence of the performance-mode design.

**Other fork settings:**

- `vad_backend` - `silero` (default) | `earshot`
- `paste_method` gained `direct_streaming`; `direct_streaming_speed` (10–60) controls the typing rate (see Direct Streaming below for when it applies)
- `overlay_direct_mode` / `overlay_direct_speed` - Live overlay character-by-character mode
- `save_raw_audio`, `overlay_speech_stats`, `speech_pause_hold_ms` - see Voice Activity Detection below
- `mic_idle_timeout_value` / `mic_idle_timeout_unit` / `mic_idle_infinite` - Lazy microphone close timeout (was a fixed 30 s upstream)
- `append_trailing_newline` - Like `append_trailing_space`, with a newline
- `custom_accent_color` - `#rrggbb` or `null` for the gold default; persisted through `change_custom_accent_color_setting`
- `native_streaming_latency_presets` - Per-model-family latency preset (`fastest` → `accurate`)

### Direct Streaming (fork addition)

`paste_method = direct_streaming` types the live transcript into the foreground
app while the user speaks (`DirectStreamWriter`, driven by the stream worker in
`managers/transcription.rs`). It applies to **plain transcription only**:

| Mode                              | Live stream                                                           | Final text                           |
| --------------------------------- | --------------------------------------------------------------------- | ------------------------------------ |
| `transcribe` (no post-processing) | typed into the app as it forms; trailer + auto-submit at finalize     | nothing more is pasted               |
| `transcribe_with_post_process`    | **preview only**: not typed; Live overlay forced if the model streams | polished text pasted once via Ctrl+V |
| `multi_stt_transcribe`            | **preview only** (primary model)                                      | merged text pasted once via Ctrl+V   |

Implementation: `actions::live_stream_is_preview_only(settings, processed)`
decides, `effective_overlay_style` forces `OverlayStyle::Live` for a
preview-only stream when the model supports streaming (otherwise the user's
overlay choice stands), `TranscriptionManager::start_stream(live_typing)` tells
the worker whether to create the writer, and `final_paste_method` returns the
`PasteMethod::CtrlV` override handed to `clipboard::paste_with_method`. Typing
the raw stream and then pasting the processed result would leave two versions
in the app — that is the case this rule exists to prevent.

### Single Instance Architecture

The app enforces single instance behavior — launching when already running brings the settings window to front rather than creating a new process. Remote control flags (`--toggle-transcription`, etc.) work by launching a second instance that sends args to the running instance via `tauri_plugin_single_instance`, then exits.

## Internationalization (i18n)

All user-facing strings must use i18next translations. oxlint enforces this through `eslint-plugin-i18next` (no hardcoded strings in JSX).

> **Linter note:** this fork uses **oxlint** (`.oxlintrc.json`), not ESLint.
> typescript-eslint hard-throws on TypeScript >= 7, which this fork pins, so
> ESLint could not parse the codebase at all. oxlint ships its own parser and is
> immune to that. `eslint-plugin-i18next` is still a dependency — oxlint loads
> it through `jsPlugins` and enforces `i18next/no-literal-string` exactly as
> before, plus a set of built-in correctness rules ESLint was never configured
> for.
>
> Two caveats when adding suppressions:
>
> - `jsPlugins` is an alpha oxlint API. If it ever stops loading the plugin,
>   oxlint fails the config outright (exit 1) rather than silently skipping the
>   rule, so a regression surfaces in CI instead of leaking hardcoded strings.
> - `src/` carries **no lint suppressions at all**, and it should stay that way.
>   `// eslint-disable-next-line i18next/no-literal-string` is unreliable here
>   anyway — oxlint honours it for JSX text on a single line but not when the
>   flagged element spans multiple lines. For literal data (file paths, version
>   strings) use a JSX expression container instead: `{"%APPDATA%/handy"}` or
>   ``{`v${version}`}``. It needs no suppression, works in every linter, and
>   reads as "data, not prose". See `settings/debug/DebugPaths.tsx`.

**Adding new text:**

1. Add key to `src/i18n/locales/en/translation.json`
2. Use in component: `const { t } = useTranslation(); t('key.path')`

**File structure:**

```
src/i18n/
├── index.ts           # i18n setup
├── languages.ts       # Language metadata
└── locales/
    ├── en/translation.json  # English (source)
    ├── de/, es/, fr/, ja/, ru/, zh/, ...
    └── ...
```

For translation contribution guidelines, see [CONTRIBUTING_TRANSLATIONS.md](CONTRIBUTING_TRANSLATIONS.md).

## Code Style

**Rust:**

- Run `cargo fmt` and `cargo clippy` before committing
- Handle errors explicitly (avoid unwrap in production)
- Use descriptive names, add doc comments for public APIs

**TypeScript/React:**

- Strict TypeScript, avoid `any` types
- Functional components with hooks
- Tailwind CSS for styling
- Path aliases: `@/` → `./src/`

## CLI Parameters

Handy supports command-line parameters on all platforms for integration with scripts, window managers, and autostart configurations.

**Implementation:** `cli.rs` (definitions), `main.rs` (parsing), `lib.rs` (applying), `signal_handle.rs` (shared logic)

| Flag                              | Description                                                                                            |
| --------------------------------- | ------------------------------------------------------------------------------------------------------ |
| `--toggle-transcription`          | Toggle recording on/off on a running instance                                                          |
| `--toggle-post-process`           | Toggle recording with post-processing on/off                                                           |
| `--cancel`                        | Cancel the current operation on a running instance                                                     |
| `--start-hidden`                  | Launch without showing the main window (tray icon visible)                                             |
| `--no-tray`                       | Launch without system tray (closing window quits the app)                                              |
| `--debug`                         | Enable debug mode with verbose (Trace) logging                                                         |
| `-f`, `--transcribe-file <WAV>`   | Headless: transcribe a mono WAV (16/24-bit PCM or 32-bit float, any rate) with the batch path and exit |
| `--model <ID>`                    | Headless: model to use instead of the selected one                                                     |
| `--device-index <N>`              | Headless: GPU device index for GGUF models                                                             |
| `--list-devices`, `--list-models` | Headless: print GPU devices / installed models and exit                                                |
| `--repeat <N>`, `--json`          | Headless: timing runs / machine-readable output                                                        |

**Key design decisions:**

- CLI flags are runtime-only overrides — they do NOT modify persisted settings
- Remote control flags work via `tauri_plugin_single_instance`: second instance sends args, then exits
- `send_transcription_input()` in `signal_handle.rs` is shared between signal handlers and CLI

## Debug Mode

Access debug features: `Cmd+Shift+D` (macOS) or `Ctrl+Shift+D` (Windows/Linux)

## Platform Notes

- **macOS**: Metal acceleration, accessibility permissions required for keyboard shortcuts
- **Windows**: CUDA acceleration on x86_64 (transcribe.cpp `cuda` feature; upstream uses Vulkan), CPU only on aarch64, no code signing in this fork (`signCommand` removed from `tauri.conf.json`), real-time audio optimizations (`HIGH_PRIORITY_CLASS`, Windows 11 EcoQoS power throttling disable, 1ms `timeBeginPeriod`, MMCSS `"Capture"` worker thread scheduling, and hardware buffer size minimization)
- **Linux**: CUDA acceleration (upstream: OpenBLAS + Vulkan), limited Wayland support, overlay uses GTK layer shell (disable with `HANDY_NO_GTK_LAYER_SHELL=1`)

## Troubleshooting

See the [Troubleshooting](README.md#troubleshooting) section in README.md.

## GitHub workflow for AI coding assistants

**MANDATORY. Before opening any PR, issue, or discussion in this repo: you MUST read the relevant template file and follow it strictly.** That includes sections that look "ceremonial" — checklists, AI Assistance disclosures, "Human Written Description". A generic Summary/Test-plan layout is not acceptable.

- **Opening a PR:** Read [`.github/PULL_REQUEST_TEMPLATE.md`](.github/PULL_REQUEST_TEMPLATE.md). Every section listed there is mandatory. If a section requires a human-written paragraph (e.g. "Human Written Description"), leave a clear TODO placeholder and ask the human contributor to fill it in — do not invent their voice.
- **Opening an issue:** Read [`.github/ISSUE_TEMPLATE/`](.github/ISSUE_TEMPLATE/). Blank issues are disabled; pick the right template (`bug_report.md` for bugs). Feature requests do not belong in issues — they go to [Discussions](https://github.com/cjpais/Handy/discussions) (see `.github/ISSUE_TEMPLATE/config.yml`).
- **Proposing a feature:** Handy is under a feature freeze. New features require community support gathered in [Discussions](https://github.com/cjpais/Handy/discussions) before any PR is opened — see the PR template's "Community Feedback" section.
- **Translations:** Follow [CONTRIBUTING_TRANSLATIONS.md](CONTRIBUTING_TRANSLATIONS.md).
- **Full contributor workflow:** [CONTRIBUTING.md](CONTRIBUTING.md).

**Commits:** Use conventional commit prefixes (`feat:`, `fix:`, `docs:`, `refactor:`, `chore:`). Focus the message on _why_, not _what_.
