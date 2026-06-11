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
mkdir -p src-tauri/resources/models
curl -o src-tauri/resources/models/silero_vad_v4.onnx https://blob.handy.computer/silero_vad_v4.onnx
bun run tauri dev
```

## Cross-Platform Mandate

Every change must work on Windows 11 (primary), macOS, and Linux. Never introduce single-OS code paths without fallbacks. See AGENTS.md for full details.

## Architecture Summary

S2B2S = Tauri 2 (Rust + React/TS)
- Backend: `src-tauri/src/` — managers/, tts/, brain/, audio_toolkit/, commands/
- Frontend: `src/` — components/, hooks/, stores/, i18n/ (20 languages)
- IPC: tauri-specta typed bindings (`src/bindings.ts`)
- State: Zustand → Tauri Command → Rust → SQLite/Store
