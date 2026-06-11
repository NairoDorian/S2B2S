# AGENTS.md

This file provides guidance to AI coding assistants working with code in this repository.

## Cross-Platform Mandate (READ FIRST — applies to EVERY change)

**S2B2S must stay cross-platform.** Priority order:

1. **Windows 11 — top priority.** Primary launch + test platform. Everything must work great.
2. **macOS — first-class.** Keep it building and functional.
3. **Linux — first-class.** Keep it building and functional.

Rules for all code (Rust **and** TypeScript):

- **Never** introduce a Windows-only (or any single-OS) code path without an equivalent or graceful fallback for macOS and Linux. Platform-specific code MUST be gated with `#[cfg(target_os = "...")]` (Rust) or runtime platform checks (TS), and every gated branch needs a counterpart (or a documented, non-crashing degradation) for the other two OSes.
- Prefer cross-platform crates/APIs (cpal, rodio, tauri, enigo, etc.) over OS-native calls. Reach for `windows`/`objc`/`gtk` only when unavoidable, always behind a `cfg`.
- Examples already in the tree to follow: `overlay.rs` (per-OS overlay impls), `audio_toolkit` (cpal), clipboard/paste, shortcuts. New features must provide Windows + macOS + Linux paths from the start.
- Don't let macOS/Linux silently rot: if a feature can't be fully implemented on one OS yet, `cfg` it off with a clear `// TODO(cross-platform):` note and a no-op/fallback — never a compile error or panic.
- CI is expected to build on all three OSes (Windows required to pass; macOS/Linux kept compiling).

When in doubt, choose the portable solution.

---

## Development Commands

**Prerequisites:** Rust (latest stable), Bun

```bash
# Install dependencies
bun install

# Run in development mode
bun run tauri dev
# If cmake error on macOS:
CMAKE_POLICY_VERSION_MINIMUM=3.5 bun run tauri dev

# Build for production
bun run tauri build

# Frontend only
bun run dev        # Vite dev server
bun run build      # TypeScript + Vite build
bun run preview    # Preview built frontend
```

**Model Setup (Required for Development):**
```bash
mkdir -p src-tauri/resources/models
curl -o src-tauri/resources/models/silero_vad_v4.onnx https://blob.handy.computer/silero_vad_v4.onnx
```

**Linting and Formatting:**
```bash
bun run lint              # ESLint for frontend
bun run lint:fix          # ESLint with auto-fix
bun run format            # Prettier + cargo fmt
bun run format:check      # Check formatting without changes
bun run format:frontend   # Prettier only
bun run format:backend    # cargo fmt only
```

**Type Check & Bindings:**
```bash
bunx tsc --noEmit          # TypeScript type checking
cargo test export_bindings # Regenerate src/bindings.ts (headless)
```

For detailed platform-specific build setup, see [BUILD.md](BUILD.md).

---

## Architecture Overview

S2B2S is a cross-platform desktop voice-native application built with **Tauri 2.x** (Rust backend + React/TypeScript frontend). It combines STT (speech-to-text), a streaming LLM "Brain", and TTS (text-to-speech) into three main workflows.

### Backend Structure (`src-tauri/src/`)

```
src-tauri/src/
├── lib.rs                  # Main entry, Tauri setup, manager init, specta_builder()
├── main.rs                 # Binary entry point
├── actions.rs              # Shortcut actions: transcribe, converse, speak selection
├── cli.rs                  # CLI argument definitions (clap derive)
├── settings.rs             # Application settings (TtsConfig, BrainConfig, SanitizeConfig)
├── signal_handle.rs        # send_transcription_input() reusable function
├── utils.rs                # Platform detection helpers
├── overlay.rs              # Recording/speaking overlay (platform-specific)
├── clipboard.rs            # Clipboard operations
├── input.rs                # Keyboard input (enigo)
├── audio_feedback.rs       # Sound effects
├── control_server.rs       # Local HTTP API (axum)
├── crash_logging.rs        # Panic capture with full backtraces
├── portable.rs             # Portable mode detection
├── tray.rs                 # System tray
├── tray_i18n.rs            # Tray i18n labels
├── llm_client.rs           # Multi-provider LLM client
├── transcription_coordinator.rs  # Record → VAD → transcribe → paste orchestrator
│
├── managers/
│   ├── audio.rs            # Audio recording and device management
│   ├── model.rs            # Model downloading and management
│   ├── transcription.rs    # STT processing pipeline
│   └── history.rs          # SQLite transcription/TTS history
│
├── tts/                    # Text-to-Speech subsystem
│   ├── mod.rs              # TtsBackend trait + Voice struct
│   ├── manager.rs          # Sanitize → Paginate → Synthesize orchestration
│   ├── player.rs           # Streaming gapless playback (rodio)
│   ├── pagination.rs       # UTF-8-safe text chunking
│   ├── fragment_queue.rs   # Synthesize-ahead fragment scheduling
│   ├── clipboard_watch.rs  # Double-copy trigger
│   ├── backends/
│   │   ├── piper.rs        # Warm persistent Piper HTTP server
│   │   ├── kokoro.rs       # Kokoro-82M in-process ONNX via tts-rs
│   │   ├── kitten.rs       # Kitten TTS (skeleton)
│   │   ├── sapi.rs         # Windows SAPI fallback
│   │   ├── openai.rs       # OpenAI TTS cloud
│   │   ├── elevenlabs.rs   # ElevenLabs TTS cloud
│   │   └── cartesia.rs     # Cartesia Sonic cloud
│   └── sanitize/
│       ├── mod.rs          # Pipeline orchestrator
│       ├── itn.rs          # Inverse Text Normalization (spoken→written)
│       ├── tn.rs           # Text Normalization (written→spoken)
│       ├── markdown.rs     # pulldown-cmark markdown stripping
│       └── cleanup.rs      # Regex-based final scrub
│
├── brain/
│   ├── client.rs           # SSE streaming chat client + sentence splitter
│   └── manager.rs          # Turn history, abort (barge-in), sentence → TTS bridge
│
├── audio_toolkit/
│   ├── audio/
│   │   ├── device.rs       # Device enumeration
│   │   ├── recorder.rs     # Audio recording
│   │   ├── resampler.rs    # rubato resampling
│   │   ├── visualizer.rs   # rustfft visualizer
│   │   ├── noise_suppression.rs  # RNNoise (nnnoiseless)
│   │   └── utils.rs        # Audio utilities
│   └── vad/
│       ├── silero.rs       # Silero VAD (vad-rs)
│       ├── smoothed.rs     # Smoothed VAD output
│       └── triple_vad.rs   # 3-stage: RMS → RNNoise prob → Silero
│
├── commands/
│   ├── mod.rs              # Tauri command registration
│   ├── tts.rs              # TTS-related commands
│   └── brain.rs            # Brain/LLM-related commands
│
└── shortcut/
    ├── mod.rs              # Shortcut manager
    └── handy_keys.rs       # HandyKeys/rdev shortcuts
```

### Frontend Structure (`src/`)

```
src/
├── App.tsx                 # Main component with onboarding flow
├── main.tsx                # Entry point
├── bindings.ts             # Auto-generated Tauri type bindings (tauri-specta)
├── App.css                 # Global styles
│
├── components/
│   ├── conversation/       # Conversation window UI
│   ├── settings/           # Settings panels
│   ├── model-selector/     # Model management
│   ├── onboarding/         # First-run experience
│   ├── overlay/            # Recording overlay UI
│   ├── update-checker/     # App update notifications
│   ├── shared/             # Shared utilities
│   ├── ui/                 # Reusable UI primitives
│   ├── icons/              # Icon components
│   ├── footer/             # Status footer
│   ├── Sidebar.tsx         # Navigation sidebar
│   ├── HerLoading.tsx      # 3D loading animation (Three.js)
│   └── AccessibilityPermissions.tsx
│
├── hooks/
│   └── useSettings.ts      # Settings state hook
│
├── stores/
│   └── settingsStore.ts    # Zustand store
│
├── i18n/
│   ├── index.ts            # i18n setup
│   ├── languages.ts        # Language metadata
│   └── locales/            # 20 language files
│       ├── en/translation.json
│       ├── de/ fr/ es/ ja/ ru/ zh/ ...
│
├── lib/
│   ├── types.ts            # Shared TS types
│   ├── types/              # Type definitions (events, etc.)
│   └── utils/              # Utility functions (RTL, etc.)
│
├── overlay/                # Recording overlay window entry
└── assets/                 # Static assets (logo, icons)
```

### Key Architecture Patterns

**Manager Pattern:** Core functionality organized into managers (Audio, Model, Transcription, History, TTS, Brain) initialized at startup and managed via Tauri state.

**Command-Event Architecture:** Frontend → Backend via Tauri commands; Backend → Frontend via events (tauri-specta typed).

**Pipeline Processing:**
- **Dictation:** Audio → TripleVAD → Parakeet V3 STT → ITN normalization → Clipboard/Paste
- **Conversation:** Audio → TripleVAD → STT → ITN → Brain (LLM streaming) → Markdown strip → TN → TTS → Speaker
- **Read Aloud:** Selected text / double-copy → Markdown strip → TN → TTS → Speaker

**State Flow:** Zustand → Tauri Command → Rust State → Persistence (tauri-plugin-store)

**Text Normalization (4-pass):**
```
Post-STT: ITN (text-processing-rs) → Custom Words (fuzzy correction)
Pre-TTS:  pulldown-cmark → TN (text-processing-rs) → Regex Cleanup
```

### Technology Stack

| Category | Libraries |
|----------|-----------|
| **Framework** | Tauri 2.x, React 19, TypeScript 6, Vite 8 |
| **Styling** | Tailwind CSS 4 |
| **State** | Zustand 5, Zod 4 |
| **i18n** | i18next 26, react-i18next 17 |
| **Animation** | Three.js 0.184, Lucide React |
| **STT** | transcribe-rs (Parakeet V3 + Whisper + Moonshine) |
| **TTS** | Piper (persistent HTTP), Kokoro (tts-rs in-process), Kitten, SAPI, OpenAI, ElevenLabs, Cartesia |
| **Audio I/O** | cpal 0.17, rodio 0.22, rubato 3.0 |
| **VAD** | vad-rs (Silero ONNX), nnnoiseless 0.5.2 (RNNoise) |
| **Text Processing** | text-processing-rs 0.2.2 (ITN/TN), pulldown-cmark 0.13 |
| **HTTP** | reqwest 0.13 |
| **Storage** | rusqlite 0.40, tauri-plugin-store |
| **IPC** | tauri-specta (typed bindings) |
| **Shortcuts** | rdev + Tauri global-shortcut |
| **Build** | Bun, Cargo (Rust nightly) |

### Application Flow

1. **Initialization:** App starts (optionally minimized to tray), loads settings, initializes managers (Audio, Model, TTS, Brain). Shows Her-style 3D loading animation.
2. **Model Setup:** First-run downloads Parakeet V3 (~0.6 GB) and Silero VAD model; Piper/Kokoro TTS models downloaded on first TTS use.
3. **Dictation:** Global shortcut triggers audio recording with TripleVAD filtering → Parakeet V3 transcription → ITN normalization → paste at cursor.
4. **Read Aloud:** Global shortcut reads selected text (or double-copy clipboard trigger) → markdown stripping → TN normalization → TTS playback with streaming gapless playback.
5. **Conversation:** Global shortcut starts recording → transcribe → send to Brain (LLM) → stream reply tokens → sentence splitter → TTS reads each sentence aloud with barge-in support.
6. **TTS Engine Selection:** 7+ backends available (Piper, Kokoro, Kitten, SAPI, OpenAI, ElevenLabs, Cartesia) with warm-persistent model lifecycle (WarmEngine trait: Loading → WarmingUp → Ready → Error).

### Settings System

Settings are stored using Tauri's store plugin with reactive updates:

- **Keyboard shortcuts**: configurable for push-to-talk, speak-selection, conversation, cancel
- **Audio devices**: microphone/output selection
- **STT model**: Parakeet V3, Whisper (Small/Medium/Turbo/Large), Moonshine
- **TTS engine**: Piper, Kokoro, Kitten, SAPI, OpenAI, ElevenLabs, Cartesia — voice, speed, volume per engine
- **Brain**: Ollama/LM Studio endpoint, model, system prompt, memory, read-aloud toggle
- **VAD mode**: TripleVAD/Silero with tunable RNNoise threshold (0.05–0.9)
- **Text pipeline**: ITN/TN/markdown-strip toggles per stage
- **Audio feedback**: toggle, volume, themes
- **Overlay**: position, opacity
- **Debug**: crash log path, debug mode toggle

### Single Instance Architecture

The app enforces single instance behavior via `tauri_plugin_single_instance`. Launching when already running brings the settings window to front. Remote control flags (`--toggle-transcription`, etc.) work by launching a second instance that sends args to the running instance, then exits.

---

## Internationalization (i18n)

All user-facing strings must use i18next translations. ESLint enforces this (no hardcoded strings in JSX). **20 languages supported.**

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
    ├── ar/ bg/ cs/ de/ es/ fr/ he/ it/ ja/ ko/
    └── pl/ pt/ ru/ sv/ tr/ uk/ vi/ zh/ zh-TW/
```

For translation contribution guidelines, see [CONTRIBUTING_TRANSLATIONS.md](CONTRIBUTING_TRANSLATIONS.md).

---

## Code Style

### Rust

- Run `cargo fmt` and `cargo clippy` before committing
- Handle errors explicitly (`anyhow::Error`, avoid `unwrap` in production)
- Use `Arc<Mutex<T>>` for shared state in managers
- Log with appropriate levels: `debug!`, `info!`, `error!`
- `#[cfg(target_os = "...")]` for platform-specific code; always provide macOS + Linux fallbacks
- Snake_case for functions, PascalCase for types
- Use descriptive names, add doc comments for public APIs

### TypeScript/React

- Strict TypeScript, avoid `any` types
- Functional components with hooks
- Tailwind CSS for styling
- Path aliases: `@/` → `./src/`
- Zod schemas for type validation and inference
- `useCallback` hooks for stable function references
- Destructure props with defaults
- PascalCase for components, camelCase for functions/variables
- All user-facing strings must use i18next

### Imports

- Group imports: external libs, internal modules, relative imports
- Use type imports: `import type { Settings }`
- Named imports over default exports

### Commits

Use conventional commit prefixes: `feat:`, `fix:`, `docs:`, `refactor:`, `chore:`, `test:`. Focus on _why_, not _what_.

---

## CLI Parameters

s2b2s supports command-line parameters on all platforms for integration with scripts, window managers, and autostart configurations.

**Implementation:** `cli.rs` (definitions), `main.rs` (parsing), `lib.rs` (applying), `signal_handle.rs` (shared logic)

| Flag | Description |
|------|-------------|
| `--toggle-transcription` | Toggle recording on/off on a running instance |
| `--toggle-post-process` | Toggle recording with post-processing |
| `--cancel` | Cancel current operation |
| `--start-hidden` | Launch without showing main window (tray icon visible) |
| `--no-tray` | Launch without system tray (closing window quits the app) |
| `--debug` | Enable debug mode with verbose (Trace) logging |

**Key design decisions:**
- CLI flags are runtime-only overrides — they do NOT modify persisted settings
- Remote control flags work via `tauri_plugin_single_instance`: second instance sends args, then exits
- `send_transcription_input()` in `signal_handle.rs` is shared between signal handlers and CLI

---

## Debug Mode

Access debug features: `Cmd+Shift+D` (macOS) or `Ctrl+Shift+D` (Windows/Linux). Also toggleable in Advanced settings.

---

## Platform Notes

| Platform | Notes |
|----------|-------|
| **macOS** | Metal acceleration, accessibility permissions required for keyboard shortcuts, Globe key support |
| **Windows** | Vulkan acceleration, code signing, NSIS installer, Common-Controls v6 manifest |
| **Linux** | OpenBLAS + Vulkan, Wayland limited (needs wtype/dotool), overlay GTK layer shell (disable with `S2B2S_NO_GTK_LAYER_SHELL=1`), Nix flake build |

---

## GitHub Workflow for AI Coding Assistants

**Before opening any PR, issue, or discussion:** read the relevant template file and follow it strictly.

- **Opening a PR:** Read [`.github/PULL_REQUEST_TEMPLATE.md`](.github/PULL_REQUEST_TEMPLATE.md). Every section mandatory. Use `feat:`, `fix:`, `docs:`, `refactor:`, `chore:` prefixes.
- **Opening an issue:** Read [`.github/ISSUE_TEMPLATE/bug_report.md`](.github/ISSUE_TEMPLATE/bug_report.md). Feature requests go to [Discussions](https://github.com/NairoDorian/S2B2S/discussions).
- **Feature proposals:** s2b2s prioritizes stability. New features require community support via [Discussions](https://github.com/NairoDorian/S2B2S/discussions) before PR.
- **AI Assistance Disclosure:** AI-assisted PRs are welcome. In the PR description, include whether AI was used, which tools, and how extensively.
- **Translations:** Follow [CONTRIBUTING_TRANSLATIONS.md](CONTRIBUTING_TRANSLATIONS.md).
- **Full contributor workflow:** [CONTRIBUTING.md](CONTRIBUTING.md).

---

## Key Files Reference

| File | Purpose |
|------|---------|
| [README.md](README.md) | Project overview, quick start, architecture |
| [S2B2S_REVIEW.md](S2B2S_REVIEW.md) | Comprehensive project analysis (non-tech users, devs, AI agents) |
| [BUILD.md](BUILD.md) | Platform-specific build instructions |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Contributor guidelines |
| [CONTRIBUTING_TRANSLATIONS.md](CONTRIBUTING_TRANSLATIONS.md) | Translation guide |
| [CHANGELOG.md](CHANGELOG.md) | Version history |
| [CRUSH.md](CRUSH.md) | Dev commands quick reference |
| [LICENSE](LICENSE) | MIT License |

---

## Troubleshooting

See [README.md#troubleshooting](README.md#troubleshooting) for known issues and solutions.
