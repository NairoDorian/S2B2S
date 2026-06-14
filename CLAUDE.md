# CLAUDE.md — AI Assistant Entry Point

This project has comprehensive documentation for AI coding assistants. Please read the following files in order:

## Key Reference Files

1. **AGENTS.md** — Full project architecture, development commands, cross-platform mandate, code style, and GitHub workflow. **READ FIRST.**

2. **S2B2S_REVIEW.md** — Comprehensive project analysis covering every subsystem, all 7+ TTS backends, TripleVAD pipeline, text normalization, the Brain, model comparisons, dependency analysis, file structure, and diagrams. Essential for deep understanding.

3. **README.md** — Project overview, quick start, architecture, roadmap, troubleshooting.

4. **BUILD.md** — Platform-specific build instructions.

5. **CRUSH.md** — Quick-reference development commands and code style.

6. **CONTRIBUTING.md** — Contribution guidelines and workflow.

7. **CHANGELOG.md** — Version history with all feature additions and fixes.

## Quick Start

```bash
bun install
# Set up Python venv for TTS engines:
#   Windows: .\scripts\setup_tts_venv.ps1
#   macOS/Linux: bash scripts/setup_tts_venv.sh
# Download models:
#   Windows: .\models\download_models.ps1
#   macOS/Linux: bash models/download_models.sh
bun run tauri dev
```

## Cross-Platform Mandate

Every change must work on Windows 11 (primary), macOS, and Linux. Never introduce single-OS code paths without fallbacks. See AGENTS.md for full details.

## Architecture Summary

S2B2S = Tauri 2 (Rust + React/TS)

- Backend: `src-tauri/src/` — managers/, tts/, brain/, llama_server/, audio_toolkit/, commands/
- Frontend: `src/` — components/, hooks/, stores/, i18n/ (20 languages)
- Evolution plans: `futuristic_analysis/` (active, supersedes `analysys/`) — GPU overlay, Conversation Mode 2.0, Screen Vision, 3D Avatar "Four Senses"
- `analysys/` is **superseded** — the original evolution plan with corrected CursorFX assumptions (DX12 → Vulkan). Excluded from git. See `futuristic_analysis/00_README_START_HERE.md`.
- IPC: tauri-specta typed bindings (`src/bindings.ts`)
- State: Zustand → Tauri Command → Rust → SQLite/Store

## Code Cleanup Notes (June 2026)

This project has been reviewed and cleaned up across multiple passes. Current state (June 2026): 8 TTS backends (Piper, Kokoro, Kitten, Pocket, SAPI, OpenAI, ElevenLabs, Cartesia), llama.cpp pre-compiled server integration, Pocket voice cloning, word-count fallback sentence streaming. Known gaps: wake word detector runs idle (audio pipeline not connected). See CHANGELOG.md for full history.

### Pass 1 — Initial Cleanup
- All module doc comments added to top-level Rust modules
- `Cargo.toml` cleaned of commented-out sections
- Documentation (README, CHANGELOG, AGENTS, CLAUDE, BUILD, CRUSH) synced and updated
- Removed unused crate: `pulldown-cmark` (markdown stripping uses regex)
- Removed dead file: `clipboard_ax.rs` (orphan, not declared in lib.rs)
- Fixed unsafe `static mut` usage in `clipboard_watch.rs` (replaced with `Mutex`/`AtomicU64`)
- Fixed stray doc-comment block in `actions.rs` (mid-function)
- Cleaned up misleading `pulldown-cmark` references in comments and docs

### Pass 2 — Code Quality & i18n
- Resolved 44 clippy warnings across backend
- Resolved 35 ESLint i18n errors across frontend (added 16 new i18n keys)
- Settings enums now use `#[derive(Default)]` with `#[default]`
- All dependencies updated to latest compatible versions

### Pass 3 — Refactoring & Type Safety
- Extracted shared `useProviderState` hook (eliminated ~200 lines of duplicate code between `useBrainProviderState` and `usePostProcessProviderState`)
- Deduplicated brain/post-process provider logic in `settingsStore.ts` via internal `_setProvider`, `_updateProviderSetting`, `_updateProviderBaseUrl`, `_fetchProviderModels` helpers
- Extracted `TooltipIcon` sub-component in `SettingContainer.tsx` (eliminated 3x duplicated SVG markup)
- Extracted `parseTimestamp` / `safeFormat` helpers in `dateFormat.ts` (eliminated duplicate parsing logic)
- Fixed `RecordingOverlay.tsx` event listener cleanup bug (listeners now properly unregistered on unmount)
- Fixed type safety: `Sidebar.tsx` index signature `[key: string]: unknown`, `settingsStore.ts` LogLevel cast, `AccessibilityPermissions.tsx` null guard
- Replaced hardcoded strings with i18n keys in `SpeechSettings.tsx` (9 new keys) and `ConversationView.tsx` (8 new keys)
- Removed unused `normalizeKey` export and dead `_osType` parameter in `keyboard.ts`
- Completed barrel exports in `ui/index.ts` and `icons/index.ts`
- See CHANGELOG.md for the full history
