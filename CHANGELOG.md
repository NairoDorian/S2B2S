# Changelog

All notable changes to S2B2S are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/), and this
project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased] — S2B2S v0.9 (Speech → Brain → Speech)

> **Status (June 2026):** Core STT → Brain → TTS pipeline is feature-complete.
> Working through Kokoro worker pool, crossfade, and RAM persistence improvements.
> See [IMPROVEMENT_PLAN.md](IMPROVEMENT_PLAN.md) for detailed roadmap.

### Added

**Documentation Overhaul:**
- **S2B2S_REVIEW.md** — new 91KB comprehensive project analysis covering 21 sections: architecture deep dive, all 3 pipelines, STT/TTS/Brain subsystems, TripleVAD, text normalization (4 passes), audio toolkit, model management, settings, frontend architecture, i18n, CI/CD, project lineage/donor map, dependency analysis, complete file tree, roadmap, known issues, platform matrix, and 6 ASCII diagrams. Serves as reference for non-tech users, developers, and AI agents.
- **README.md** — complete rewrite with table of contents, default stack table, all pipeline diagrams, text normalization pass tables, full architecture section, CLI/env vars reference, sponsor section.
- **AGENTS.md** — full architecture tree visualization, frontend+backend structure maps, technology stack table, i18n details, code style, platform notes, key files reference.
- **BUILD.md** — macOS Intel ONNX Runtime setup, env vars table, CI/CD workflow table, project structure overview.
- **CLAUDE.md** — expanded from single line to full entry point doc referencing all key project files.
- **CONTRIBUTING.md, CONTRIBUTING_TRANSLATIONS.md, CRUSH.md, IMPROVEMENT_PLAN.md** — all updated with current state, commands, and architecture info.
- **PR template** — softened feature-freeze language to focus on priorities rather than rejection.
- **Bug report template** — added crash log path and debug mode instructions.

**Core STT / VAD:**
- **Triple VAD as default** — 3-stage voice activity detector (RMS energy gate → RNNoise voice probability → Silero VAD) is now the default for all modes. Provides better noise rejection at ~2ms additional latency per frame.
- **RNNoise voice probability threshold** — new `rnnoise_voice_threshold` setting (0.05–0.9, default 0.2) with slider in Advanced → Audio Enhancements. Controls how aggressively RNNoise filters non-speech audio.

**Text Normalization Pipeline (ITN + TN + Markdown):**
- **ITN (Inverse Text Normalization)** via `text-processing-rs` (Apache 2.0) — spoken-form ASR output normalized to written form: "two hundred thirty two" → "232", "january fifth" → "January 5, 2025". Applied post-STT in both dictation and conversation pipelines.
- **TN (Text Normalization)** via `text-processing-rs` — written-form text normalized to spoken form before TTS: "$5.50" → "five dollars and fifty cents", "123" → "one hundred twenty three".
- **Markdown stripping** via `pulldown-cmark` — headings, bold, lists, links, code blocks, HTML entities all converted to natural spoken form before TTS.

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
- **Streaming gapless playback** — fragment *i+1* synthesized while *i* plays. UTF-8-safe sentence pagination.
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

### Changed

- **Default VAD mode** changed from `"silero"` to `"triple"` for all modes (dictation, conversation, push-to-talk).
- **Text sanitizer pipeline reordered** — markdown stripping runs first, then TN (text-processing-rs), then legacy regex-based TTS normalization, then artifact cleanup.
- **Always-On Microphone toggle moved** from Debug settings to General → Sound section for easy discovery.
- **All dependencies updated to latest** — Tauri 2.11, rodio 0.22, rubato 3.0, reqwest 0.13, rusqlite 0.40, `windows` 0.62, specta rc.25, transcribe-rs 0.3.11. React 19, Vite 8, TypeScript 6, zod 4, ESLint 10, i18next 26. `cpal` pinned to 0.17.
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
