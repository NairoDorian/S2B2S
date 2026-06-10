# AGENTS.md

This file provides guidance to AI coding assistants working with code in this repository.

## ⚠️ Cross-Platform Mandate (READ FIRST — applies to EVERY change)

**S2B2S must stay cross-platform.** Priority order:

1. **Windows 11 — top priority.** Primary launch + test platform. Everything must work great here.
2. **macOS — first-class.** Keep it building and functional.
3. **Linux — first-class.** Keep it building and functional.

Rules for all code (Rust **and** TypeScript):

- **Never** introduce a Windows-only (or any single-OS) code path without an equivalent or graceful fallback for macOS and Linux. Platform-specific code MUST be gated with `#[cfg(target_os = "...")]` (Rust) or runtime platform checks (TS), and every gated branch needs a counterpart (or a documented, non-crashing degradation) for the other two OSes.
- Prefer cross-platform crates/APIs (cpal, rodio, tauri, enigo, etc.) over OS-native calls. Reach for `windows`/`objc`/`gtk` only when unavoidable, always behind a `cfg`.
- Examples already in the tree to follow: `overlay.rs` (per-OS overlay impls), `audio_toolkit` (cpal), clipboard/paste, shortcuts. New features (TTS playback/output-device, double-copy clipboard watcher, conversation hotkeys, HUD) must provide Windows + macOS + Linux paths from the start.
- Don't let macOS/Linux silently rot: if a feature can't be fully implemented on one OS yet, `cfg` it off there with a clear `// TODO(cross-platform):` note and a no-op/fallback — never a compile error or panic.
- CI is expected to build on all three OSes (Windows required to pass; macOS/Linux kept compiling).

When in doubt, choose the portable solution. A Windows-only shortcut that breaks the macOS/Linux build is **not acceptable**.

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
bun run tauri build

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
curl -o src-tauri/resources/models/silero_vad_v4.onnx https://blob.handy.computer/silero_vad_v4.onnx
```

For detailed platform-specific build setup, see [BUILD.md](BUILD.md).

## Architecture Overview

S2B2S is a cross-platform desktop speech-to-text application built with Tauri 2.x (Rust backend + React/TypeScript frontend).

### Backend Structure (src-tauri/src/)

- `lib.rs` - Main entry point, Tauri setup, manager initialization, `specta_builder()` (typed IPC; regenerate `src/bindings.ts` with `cargo test export_bindings`)
- `managers/` - Core business logic:
  - `audio.rs` - Audio recording and device management
  - `model.rs` - Model downloading and management
  - `transcription.rs` - Speech-to-text processing pipeline
  - `history.rs` - Transcription history storage
- `tts/` - Text-to-speech subsystem (the "Read Anywhere" / CopySpeak pillar):
  - `mod.rs` - `TtsBackend` trait + `Voice`
  - `backends/piper.rs` - warm persistent Piper HTTP server (drained stdio)
  - `player.rs` - streaming gapless playback (rodio), drives the speaking HUD
  - `manager.rs` - sanitize → paginate → synthesize-ahead orchestration
  - `sanitize/` - markdown stripping + speech normalization + cleanup
  - `pagination.rs` / `fragment_queue.rs` - UTF-8-safe text chunking
  - `clipboard_watch.rs` - double-copy trigger (Windows detection for now)
- `brain/` - Streaming LLM subsystem (the "Brain" of Speech → Brain → Speech):
  - `client.rs` - SSE streaming chat client + sentence splitter
  - `manager.rs` - turn history, abort (barge-in), sentence → TTS bridge
- `audio_toolkit/` - Low-level audio processing:
  - `audio/` - Device enumeration, recording, resampling
  - `vad/` - Voice Activity Detection (Silero VAD)
- `commands/` - Tauri command handlers (incl. `tts.rs`, `brain.rs`)
- `actions.rs` - Shortcut actions: transcribe, converse (→ Brain), speak selection
- `cli.rs` - CLI argument definitions (clap derive)
- `shortcut.rs` - Global keyboard shortcut handling
- `settings.rs` - Application settings management (incl. `TtsConfig`, `BrainConfig`)
- `overlay.rs` - Recording/speaking overlay window (platform-specific)
- `signal_handle.rs` - `send_transcription_input()` reusable function
- `utils.rs` - Platform detection helpers

### Frontend Structure (src/)

- `App.tsx` - Main component with onboarding flow
- `components/` - React UI components:
  - `settings/` - Settings UI
  - `model-selector/` - Model management interface
  - `onboarding/` - First-run experience
  - `overlay/` - Recording overlay UI
  - `update-checker/` - App update notifications
  - `shared/`, `ui/`, `icons/`, `footer/` - Shared components
- `hooks/useSettings.ts` - Settings state management hook
- `stores/settingsStore.ts` - Zustand store for settings
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

- `whisper-rs` - Local Whisper inference with GPU acceleration
- `cpal` - Cross-platform audio I/O
- `vad-rs` - Voice Activity Detection
- `rdev` - Global keyboard shortcuts
- `rubato` - Audio resampling
- `rodio` - Audio playback for feedback sounds

### Application Flow

1. **Initialization:** App starts minimized to tray, loads settings, initializes managers
2. **Model Setup:** First-run downloads preferred Whisper model (Small/Medium/Turbo/Large)
3. **Recording:** Global shortcut triggers audio recording with VAD filtering
4. **Processing:** Audio sent to Whisper model for transcription
5. **Output:** Text pasted to active application via system clipboard

### Settings System

Settings are stored using Tauri's store plugin with reactive updates:

- Keyboard shortcuts (configurable, supports push-to-talk)
- Audio devices (microphone/output selection)
- Model preferences (Small/Medium/Turbo/Large Whisper variants)
- Audio feedback and translation options

### Single Instance Architecture

The app enforces single instance behavior — launching when already running brings the settings window to front rather than creating a new process. Remote control flags (`--toggle-transcription`, etc.) work by launching a second instance that sends args to the running instance via `tauri_plugin_single_instance`, then exits.

## Internationalization (i18n)

All user-facing strings must use i18next translations. ESLint enforces this (no hardcoded strings in JSX).

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

s2b2s supports command-line parameters on all platforms for integration with scripts, window managers, and autostart configurations.

**Implementation:** `cli.rs` (definitions), `main.rs` (parsing), `lib.rs` (applying), `signal_handle.rs` (shared logic)

| Flag                     | Description                                                    |
| ------------------------ | -------------------------------------------------------------- |
| `--toggle-transcription` | Toggle recording on/off on a running instance                  |
| `--toggle-post-process`  | Toggle recording with post-processing on/off                   |
| `--cancel`               | Cancel the current operation on a running instance             |
| `--start-hidden`         | Launch without showing the main window (tray icon visible)     |
| `--no-tray`              | Launch without system tray (closing window quits the app)      |
| `--debug`                | Enable debug mode with verbose (Trace) logging                 |

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
- **Opening an issue:** Read [`.github/ISSUE_TEMPLATE/`](.github/ISSUE_TEMPLATE/). Blank issues are disabled; pick the right template (`bug_report.md` for bugs). Feature requests do not belong in issues — they go to [Discussions](https://github.com/cjpais/s2b2s/discussions) (see `.github/ISSUE_TEMPLATE/config.yml`).
- **Proposing a feature:** s2b2s is under a feature freeze. New features require community support gathered in [Discussions](https://github.com/cjpais/s2b2s/discussions) before any PR is opened — see the PR template's "Community Feedback" section.
- **Translations:** Follow [CONTRIBUTING_TRANSLATIONS.md](CONTRIBUTING_TRANSLATIONS.md).
- **Full contributor workflow:** [CONTRIBUTING.md](CONTRIBUTING.md).

**Commits:** Use conventional commit prefixes (`feat:`, `fix:`, `docs:`, `refactor:`, `chore:`). Focus the message on *why*, not *what*.
