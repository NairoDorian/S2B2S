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
bun run lint              # ESLint for frontend
bun run lint:fix          # ESLint with auto-fix
bun run format            # Prettier + cargo fmt
bun run format:check      # Check formatting without changes
bun run format:frontend   # Prettier only
bun run format:backend    # cargo fmt only
```

**Model Setup (Required for Development):**

```bash
mkdir -p src-tauri/resources/models
curl -o src-tauri/resources/models/silero_vad_v6.2.onnx https://huggingface.co/BricksDisplay/silero-vad-6.2/resolve/main/onnx/model.onnx
```

For detailed platform-specific build setup, see [BUILD.md](BUILD.md).

## Architecture Overview

Handy is a cross-platform desktop speech-to-text application built with Tauri 2.x (Rust backend + React/TypeScript frontend).

### Backend Structure (src-tauri/src/)

- `lib.rs` - Main entry point, Tauri setup, manager initialization
- `managers/` - Core business logic:
  - `audio.rs` - Audio recording and device management
  - `model.rs` - Model downloading and management
  - `transcription.rs` - Speech-to-text processing pipeline
  - `history.rs` - Transcription history storage
- `audio_toolkit/` - Low-level audio processing:
  - `audio/` - Device enumeration, recording, resampling
    - `recorder.rs` also hosts `SpeechClock`, which measures how long the user
      has actually been speaking (see Speech Stats below)
  - `vad/` - Voice Activity Detection (Silero VAD) — see Voice Activity
    Detection below; `silero.rs` drives the ONNX session directly rather than
    through `vad-rs`
- `commands/` - Tauri command handlers for frontend communication
- `cli.rs` - CLI argument definitions (clap derive)
- `shortcut/mod.rs` - Global keyboard shortcut handling (includes multi-STT shortcut registration)
- `settings.rs` - Application settings management (includes Multi-STT settings)
- `overlay.rs` - Recording overlay window (platform-specific), plus
  `SpeechActivityEvent` and the cached emit gates for `mic-level` /
  speech-activity events
- `signal_handle.rs` - `send_transcription_input()` reusable function
- `actions.rs` - Shortcut action implementations: `TranscribeAction`, `MultiSttAction`, `CancelAction`
  - `MultiSttAction` runs primary + three extra models in parallel, merges outputs via LLM
- `llm_client.rs` - OpenAI-compatible API client for post-processing and Multi-STT merge
  - Includes `erase_llama_server_conversations()` for llama.cpp conversation cleanup
- `utils.rs` - Platform detection helpers

### Frontend Structure (src/)

- `App.tsx` - Main component with onboarding flow
- `components/` - React UI components:
  - `settings/` - Settings UI
    - `multi-stt/MultiSttSettings.tsx` - Multi-STT configuration (fork feature)
  - `model-selector/` - Status-bar model controls (footer). Three mutually
    exclusive popovers: model switcher, quantization picker
    (`QuantizationPanel.tsx`, which also hosts the quantization benchmark via
    `useQuantBenchmark.ts`), and native streaming latency (`LatencyPanel.tsx`).
    There is deliberately no benchmark settings page — the benchmark lives
    beside the quant list it compares.
  - `onboarding/` - First-run experience
  - `overlay/` - Recording overlay UI
  - `update-checker/` - App update notifications
  - `shared/`, `ui/`, `icons/`, `footer/` - Shared components
- `hooks/useSettings.ts` - Settings state management hook
- `stores/settingsStore.ts` - Zustand store for settings
- `stores/modelStore.ts` - Model store with load/unload operations
- `bindings.ts` - Auto-generated Tauri type bindings (via tauri-specta)
- `overlay/` - Recording overlay window entry point
- `lib/types.ts` - Shared TypeScript type definitions

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
- `rdev` - Global keyboard shortcuts
- `rubato` - Audio resampling
- `rodio` - Audio playback for feedback sounds

### Application Flow

1. **Initialization:** App starts minimized to tray, loads settings, initializes managers
2. **Model Setup:** First-run downloads preferred Whisper model (Small/Medium/Turbo/Large)
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
   silence alike. The whole capture pipeline is framed to 32 ms
   (`constants::VAD_FRAME_MS`) so one resampled frame is exactly one VAD
   decision — `FRAME_MS` in `recorder.rs` is derived from it, not chosen.
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
  reporting speech through a hangover tail up to 1.76 s long in streaming mode,
  and counting that would add more than a second of phantom speech per pause.
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
- `speech_pause_hold_ms` - Pause tolerance (100-2000 ms, default 500)

**Multi-STT settings** (fork addition):

- `multi_stt_enabled` - Global toggle for multi-model transcription mode
- `multi_stt_model_2` / `multi_stt_model_3` / `multi_stt_model_4` - Select extra STT models
- `multi_stt_language_model_2` / `multi_stt_language_model_3` / `multi_stt_language_model_4` - Per-model language override
- `multi_stt_translate_model_2` / `multi_stt_translate_model_3` / `multi_stt_translate_model_4` - Per-model English translation
- `multi_stt_keep_extra_models_loaded` - Keep extra models resident between uses (memory/speed tradeoff)
- `multi_stt_merge_prompt` - LLM prompt for merging outputs (`${output}`, `${output2}`, `${output3}`, `${output4}`)
- `multi_stt_selected_merge_prompt_id` - Active merge prompt selector

Extra models are managed by `TranscriptionManager` (`extra_engines` HashMap) with explicit
load/unload lifecycle, separate from the primary model.

### Single Instance Architecture

The app enforces single instance behavior — launching when already running brings the settings window to front rather than creating a new process. Remote control flags (`--toggle-transcription`, etc.) work by launching a second instance that sends args to the running instance via `tauri_plugin_single_instance`, then exits.

## Internationalization (i18n)

All user-facing strings must use i18next translations. ESLint enforces this (no hardcoded strings in JSX).

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

| Flag                     | Description                                                |
| ------------------------ | ---------------------------------------------------------- |
| `--toggle-transcription` | Toggle recording on/off on a running instance              |
| `--toggle-post-process`  | Toggle recording with post-processing on/off               |
| `--cancel`               | Cancel the current operation on a running instance         |
| `--start-hidden`         | Launch without showing the main window (tray icon visible) |
| `--no-tray`              | Launch without system tray (closing window quits the app)  |
| `--debug`                | Enable debug mode with verbose (Trace) logging             |

**Key design decisions:**

- CLI flags are runtime-only overrides — they do NOT modify persisted settings
- Remote control flags work via `tauri_plugin_single_instance`: second instance sends args, then exits
- `send_transcription_input()` in `signal_handle.rs` is shared between signal handlers and CLI

## Debug Mode

Access debug features: `Cmd+Shift+D` (macOS) or `Ctrl+Shift+D` (Windows/Linux)

## Platform Notes

- **macOS**: Metal acceleration, accessibility permissions required for keyboard shortcuts
- **Windows**: Vulkan acceleration, code signing
- **Linux**: OpenBLAS + Vulkan, limited Wayland support, overlay uses GTK layer shell (disable with `HANDY_NO_GTK_LAYER_SHELL=1`)

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
