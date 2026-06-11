# Changelog

All notable changes to S2B2S are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/), and this
project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- **Her-style 3D loading animation** — Three.js animated tube geometry (lissajous
  curve) with ring-reveal transition on load completion; replaces blank screen
  during app startup.
- **French Piper TTS voices** — downloaded all 7 fr_FR voices (gilles, mls,
  mls_1840, siwis, tom, upmc) at available quality levels.
- **New app icon and logo** — icon for taskbar/titlebar/tray, logo for README
  and sidebar menu.

- **Speech output (TTS) subsystem** — the "Read Anywhere" / CopySpeak pillar:
  - `TtsBackend` engine abstraction with a warm, persistent **Piper** local
    HTTP-server backend (model stays resident in RAM; child stdio drained so
    long sessions can never freeze the server).
  - Streaming gapless playback: fragment *i+1* is synthesized while *i* plays.
  - UTF-8-safe sentence pagination and a 3-pass text sanitizer (markdown
    stripping, speech normalization — `$50` → "50 dollars" — and artifact
    cleanup), all covered by unit tests.
  - **Speak Selection** global shortcut (default `Alt+Shift+R` /
    `Option+Shift+R`): reads the selected text (clipboard fallback) aloud;
    press again to stop. Clipboard contents are preserved.
  - **Double-copy trigger**: copy the same text twice within 1.5 s to hear it
    (Windows detection for now; other platforms degrade gracefully).
  - Speaking HUD overlay with a stop control; "Speech" settings section
    (engine, voice, speed, volume, Piper setup, cleanup toggles, test button).
- **The Brain** — a streaming LLM subsystem completing the Speech → Brain →
  Speech loop (separate from transcription post-processing):
  - OpenAI-compatible SSE streaming client (Ollama default, LM Studio/cloud
    via base URL + key) with multi-turn memory and a configurable context
    window.
  - Sentence-by-sentence **read-aloud while the reply streams**, with
    barge-in: a new question (or Stop) aborts the previous turn and speech.
  - **Talk to the Brain** global shortcut (default `Alt+Shift+B` /
    `Option+Shift+B`): record → transcribe → Brain → spoken streamed reply.
  - **Conversation** view: live transcript of spoken/typed turns with
    streaming tokens, plus a text input fallback; "Brain" settings section
    (endpoint, model picker, system prompt, memory, read-aloud toggle).
- Typed bindings can now be regenerated headlessly with
  `cargo test export_bindings` (no GUI launch needed).
- i18n keys for all new UI across all 20 locales (English placeholders pending
  translation).

### Changed

- **All dependencies updated to latest** (Rust and frontend):
  - Tauri 2.11 (official crates.io — the patched `cjpais/tauri` fork and the
    `cjpais/rodio` fork were dropped), rodio 0.22, rubato 3.0 (resampler
    rewritten on the new adapter API), reqwest 0.13, rusqlite 0.40, sha2 0.11,
    `windows` 0.62, specta rc.25, transcribe-rs 0.3.11.
  - React 19, Vite 8, TypeScript 6, zod 4, ESLint 10, i18next 26.
  - `cpal` is pinned to 0.17 (rodio 0.22's supported range; one cpal version
    is required because recording devices are shared with playback).
- **Overlay threading simplified** — removed `run_on_main_thread` wrapping
  from overlay show/hide/reposition operations; overlay now executes directly
  on the calling thread without a main-thread hop.
- **Removed COM initialization from TTS audio player** — the `CoInitializeEx`
  call on the background playback thread was dropped.
- **Removed dynamic Piper server reload** — `change_tts_config` no longer
  restarts the persistent Piper HTTP server in the background when voice or
  CUDA settings change.
- Renamed `warmup_speak_out_loud` setting to `play_startup_greeting` and
  `speak_warmup_bytes` method to `play_raw`.
- **Complete retheme** — switched from light/pink to pure black (#000000)
  background with purple (#7c3aed) + gold (#f59e0b) accents across all UI
  (icons, sliders, overlays, recording bars), removed dark mode media query.
- **Loading animation timing** — minimum 3-second display before transition,
  startup greeting plays at 0.9x speed.
- **All platform icons regenerated** — taskbar, tray, and window icons from
  new icon source; tray state icons updated to 64x64.

### Fixed

- Windows test executables failed to load (`STATUS_ENTRYPOINT_NOT_FOUND`)
  because they lacked a Common-Controls v6 manifest after the dependency
  upgrade; `build.rs` now embeds one into test binaries.
