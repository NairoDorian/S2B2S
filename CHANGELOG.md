# Changelog

All notable changes to S2B2S are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/), and this
project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased] — S2B2S v0.10 (Conversation Evolution)

> **Status (June 2026):** Pre-compiled llama.cpp CUDA/Vulkan/CPU server integration, GPU VRAM offloading with `-ngl all`, auto-download from GitHub releases, Llama.cpp settings management tab, and per-message performance metrics.

### Performance Metrics (Token/s, Latency, STT/TTS Timing)

- **Brain response metrics** — `brain:done` event now carries `tokens_per_sec` and `total_ms` from the llama.cpp server timing response. Displayed in the Conversation view as `t/s` and `ms` next to each assistant message, and in the Brain Settings test panel.
- **STT timing** — `brain:asked` event now includes `stt_ms` (audio-to-text latency). Rendered next to user messages in the Conversation view with a 🎤 icon.
- **TTS synthesis timing** — `tts:synth-done` now emits total synthesis `ms`. `tts:first-audio` event tracks time-to-first-audio (TTFA) for streaming TTS.
- **Brain client timing capture** — `BrainClient::stream_chat` now returns `BrainResult` with optional `BrainTiming` (tokens/sec, total ms) parsed from the SSE stream's `usage` or `delta.timings` fields.

### Pre-compiled llama.cpp Server Integration (Drop-in GPU Acceleration)

- **No more source compilation** — Removed the entire CMake-based `build_llama_cpp()` pipeline from `build.rs`. The app now downloads pre-compiled `llama-server` binaries directly from the [llama.cpp GitHub releases](https://github.com/ggml-org/llama.cpp/releases), supporting CUDA, Vulkan, and CPU backends across Windows, macOS, and Linux.
- **LlamaServerManager** — New Rust module (`src-tauri/src/llama_server/`) with full lifecycle management: fetches GitHub releases, downloads/installs/extracts archives, lists installed servers, and auto-selects the best available backend (CUDA > Vulkan > CPU). Stores binaries in `llama_cpp_servers/` within the app data directory for persistence.
- **GPU VRAM offloading** — The server launch command now passes `-ngl all` when a GPU-capable binary (CUDA or Vulkan) is detected, loading all model layers directly into GPU VRAM. CUDA runtime detection (`nvidia-smi` / `CUDA_PATH`) is automatic.
- **Auto-start on Brain activation** — `warmup()` in `BrainManager` now calls `ensure_server_running()` for llama_cpp before sending the warmup prompt. The llama.cpp server auto-starts at app launch when Brain is enabled (default: `true`) with the `llama_cpp` provider selected.
- **Brain status indicator** — The footer Brain dot now shows three states: orange pulsing (model loading into VRAM), green (ready), gray (disabled). Driven by `brain:llama-loading` / `brain:llama-ready` / `brain:llama-error` Tauri events.

### Llama.cpp Settings Tab

- **New "Llama.cpp" sidebar tab** — Full settings UI in `LlamaCppSettings.tsx` for managing pre-compiled server binaries. Shows detected GPU type, fetches available releases from GitHub, displays per-backend download buttons with progress, lists installed servers with version tags (e.g., `b9601`), and allows switching active backends.
- **Default model display** — Footer now shows "Gemma-4 2B (Local)" for the llama_cpp provider instead of the raw server alias.

### Developer Experience

- **VRAM log cleanup** — All `[VRAM]` info-level logs in `commands/models.rs` demoted to `debug!` to stop the per-second VRAM polling spam in `npm run tauri dev` console output.


### Refactoring & Code Quality

- **Shared `useProviderState` hook** — extracted common provider state management from `useBrainProviderState.ts` and `usePostProcessProviderState.ts` into `src/hooks/useProviderState.ts`. Eliminated ~200 lines of nearly identical code. Both hooks now delegate to the shared generic hook with a configuration object.
- **Deduplicated settings store** — extracted brain/post-process provider logic in `settingsStore.ts` into internal helpers (`_setProvider`, `_updateProviderSetting`, `_updateProviderBaseUrl`, `_fetchProviderModels`). Eliminated ~200 lines of duplicate provider management code.
- **`TooltipIcon` sub-component** — extracted from `SettingContainer.tsx` to eliminate 3x duplicated SVG tooltip icon markup across stacked/horizontal layouts.
- **`parseTimestamp` / `safeFormat` helpers** — extracted from `dateFormat.ts` to eliminate duplicate timestamp parsing and error handling in `formatDateTime` and `formatDate`.

### Bug Fixes

- **RecordingOverlay cleanup** — event listeners registered via `listen()` are now properly unregistered on component unmount. Previously the `setupEventListeners()` returned a cleanup function but the effect never called it, causing potential memory leaks.

### Type Safety

- **`Sidebar.tsx`** — changed `IconProps` index signature from `[key: string]: any` to `[key: string]: unknown` for better type safety with Lucide icons.
- **`settingsStore.ts`** — replaced `value as any` for LogLevel with `value as LogLevel` using explicit `LogLevel` type import.
- **`AccessibilityPermissions.tsx`** — replaced unsafe `as ButtonConfig` cast with proper null guard + early return.

### i18n Completion

- **`SpeechSettings.tsx`** — replaced 9 hardcoded strings with i18n keys (Greeting settings: group title, toggles, labels, descriptions, placeholders, noise scales).
- **`ConversationView.tsx`** — replaced 8 hardcoded strings with i18n keys (voice mode status labels, toggle titles, button text).
- Added all new i18n keys to `en/translation.json`.

### Dead Code Removal

- **`keyboard.ts`** — removed unused `normalizeKey` export. The `_osType` parameter in `formatKeyCombination` is retained (function signature matches callers).

### Barrel Exports

- **`ui/index.ts`** — completed barrel exports with all 17 UI components (added Alert, AudioPlayer, Badge, Button, Input, PathDisplay, ResetButton, Select).
- **`icons/index.ts`** — completed barrel exports with all 6 icon components (added ResetIcon, S2B2SIcon, S2B2STextLogo).

### Documentation Fixes

- **S2B2S_REVIEW.md** — replaced remaining `pulldown-cmark` references with regex-based stripping across 6 locations; corrected pipeline heading from "4-Pass" to "5-Stage"; synced roadmap (section 19) with current state — moved 6 completed features from Planned/Later to Completed; updated architecture limitations.
- **README.md** — fixed stale version number in verification example (`0.9.0`→`0.1.0`); removed extra blank lines.
- **CHANGELOG.md** — fixed missing `### Added` header; merged orphaned entries; noted `clipboard_ax.rs` removal lifecycle.
- **AGENTS.md** — updated backend and frontend architecture trees with missing files (`active_app.rs`, `apple_intelligence.rs`, `wake_word.rs`, commands detail, `helpers/`, `shortcut/` backends, `modelStore.ts`).
- **Module doc comments** — added `//!` docs to `active_app.rs`.

### Code Cleanup

- **Cargo.toml cleanup** — removed commented-out `[[bin]]` section for CLI; removed trailing blank line before target-specific dependencies.
- **Module documentation** — added module-level doc comments (`//!`) to `actions.rs`, `input.rs`, `audio_feedback.rs`, `commands/mod.rs`, `helpers/clamshell.rs` for clearer codebase navigation.
- **Documentation sync** — updated roadmap in README.md to reflect all completed features (AI Replace, Latency HUD, wake word, save-to-file, waveform HUD, auto-discovery), removed stale entries.
- **README.md polish** — streamlined features list and added clarity to pipeline descriptions.

> **Status (June 2026):** All 19 focused improvement items complete.
> Hybrid KWS+VAD wake word, AI Replace, latency HUD, Ollama discovery, save-to-file,
> warm model unload timeout, cross-platform selection capture + double-copy, waveform HUD.

### Code Quality & Cleanup

- **Rust clippy cleanup** — resolved all 44 clippy warnings across the backend: deprecated `rodio::DeviceTrait::name` → `description()`, `map_or(false, ...)` → `is_some_and(...)`, `map_or(true, ...)` → `is_none_or(...)`, `contains_key` + `insert` → `Entry::Vacant` API, `return Ok(())` → `Ok(())`, redundant closures → direct function references, manual saturating arithmetic → `saturating_sub`, collapsible `if` statements collapsed, manual `Default` impls replaced with `#[derive(Default)]`, empty doc comments fixed, `write!` → `writeln!`, test modules repositioned after function definitions, `io_other_error` patterns modernized, and `needless_borrow` references simplified. Architecture-level `#[allow(clippy::too_many_arguments)]` added to 4 long-parameter-sig functions.
- **Rust formatting** — `cargo fmt` applied project-wide; trailing whitespace in `piper_server.rs` removed.
- **ESLint i18n cleanup** — resolved all 35 `i18next/no-literal-string` errors across the frontend: added 16 new i18n keys (`conversation.latency.*`, `footer.brain*`, `footer.brainTitle`, `footer.tts*`, `gpuVram.*`, `debug.logViewer.*`, `ui.slider.resetToDefault`, `settings.speech.playGreeting`) and applied `eslint-disable-next-line` for icon/technical-unit literals.
- **Settings enums** — 5 enums (`ModelUnloadTimeout`, `ClipboardHandling`, `AutoSubmitKey`, `TypingTool`, `WhisperAcceleratorSetting`, `OrtAcceleratorSetting`) now use `#[derive(Default)]` with `#[default]` annotations instead of manual `impl Default`.

### Added

- **GPU VRAM usage indicator** — green (<75%), yellow (75-90%), red (>90%) with hover tooltip showing used/total MB. Polls every 5s via `get_active_gpu_vram_status` command.
- **Log viewer console** — developer log viewer in Debug settings with level filter, search, auto-refresh (2s), manual refresh, copy to clipboard, and clear logs. Backed by `get_recent_logs` / `clear_logs` commands.
- **Footer status indicators** — STT, Brain, and TTS indicators collapsed to emoticon + title + status dot (🎙️ STT 🟢, 🧠 Brain 🟢, 🗣️ TTS 🟢). Full model/voice details visible on hover tooltip and in their respective dropdown popovers.
- **Documentation cleanup** — removed all remaining `IMPROVEMENT_PLAN.md` references from CONTRIBUTING.md, AGENTS.md, CRUSH.md, S2B2S_REVIEW.md, and PULL_REQUEST_TEMPLATE.md. Removed Sponsors section from README. Marked RAM-persistent warm model lifecycle as ✅ Complete in roadmap.

### Added

**Conversation & Brain:**

- **Speakable-output system prompt** — separate `speakable_output_prompt` appended when `read_aloud` is ON, instructs LLM to answer conversationally for listening. Editable in settings.
- **TTS toggle in conversation UI** — 🔊/🔇 button in ConversationView header toggles `read_aloud` per-chat in real time. Keyboard shortcut `Ctrl+Shift+T`.
- **AI Replace Selection** — select text anywhere, press `Ctrl+Alt+Space`, speak an instruction — the Brain rewrites the selection in place. Uses dedicated system prompt: "Output ONLY the rewritten text — no preamble, no explanation."
- **Latency HUD** — per-stage timestamps (EP: endpoint, STT, TTFT: time-to-first-token, TTFA: time-to-first-audio) emitted as `brain:latency` events. Color-coded display in conversation view (green < target, yellow < 2x, red > 2x).
- **Sentence splitter optimization** — `split_at_clause_boundary()` at 60 chars for fast TTFA. Prefers strong boundaries (`.`, `)`, `]`) over weak (`,`) with 10-char bonus. Wire `tts_shorten_first_chunk` setting through to `TtsManager::speak()`.
- **Brain config extensions** — new settings: `conversation_mode` (push_to_talk/toggle/hands_free), `endpoint_preset` (snappy/balanced/patient), `headphone_mode`, `auto_listen` (auto-rearm after reply).
- **Ollama/LM Studio/llama.cpp model discovery** — `discover_local_brains()` command probes `:11434/api/tags` (Ollama), `:1234/v1/models` (LM Studio), `:8080/v1/models` (llama.cpp). Returns discovered servers with model lists, zero-config detection.

**TTS Ecosystem:**

- **Save-to-file MP3/OGG/FLAC** — `tts/audio_format.rs` converts WAV via ffmpeg shell-out. `tts_save_format` setting. `tts_save_to_file` command saves most recent TTS audio to user-chosen path.
- **Warm model unload timeout** — `WarmEngine` trait implemented on `PiperBackend` (`warm()`, `unload()`, `status()`). `start_idle_watcher()` in `piper_server.rs` checks `ModelUnloadTimeout` every 15s, auto-unloads on idle expiry. Tray "Unload Model" action wired.
- **Piper server health monitor** — already robust with generation-based cancellation, stdout/stderr drain threads, CUDA warm-up synthesis, health polling with exponential backoff 100→1600ms.
- **Waveform HUD** — `AmplitudeEnvelope` struct + `extract_envelope()` in `audio_toolkit/utils.rs`. 32-bar RMS envelope extracted per TTS fragment and emitted via `tts:waveform` event.
- **Cross-platform selection capture** — sentinel-based clipboard capture writes unique sentinel before Ctrl+C, reliably distinguishes "no selection" from "clipboard unchanged". Fallback for all platforms.
- **Cross-platform double-copy trigger** — Windows: `GetClipboardSequenceNumber`. macOS: `NSPasteboard.changeCount` via AppKit FFI. Linux: content-based polling with xclip/wl-paste. Graceful degradation on unsupported platforms.

**Wake Word Detection:**

- **VAD-based activity detection** — `WakeWordDetector` uses RMS energy threshold (0.03) with 3-frame debounce (~150ms). Zero model files needed. ~2s ring buffer auto-cleared.
- **sherpa-onnx KWS prepared** — integration code written (init_kws/feed_kws in git history). Blocked on Windows CRT linking: `sherpa-onnx-sys` uses `/MT` static CRT while `transcribe-rs`/`whisper` uses `/MD` dynamic CRT. To enable: add `sherpa-onnx = "1.13.2"` to `Cargo.toml` and download KWS model files to `models/wake_word/`.
- **Privacy-first design** — feature defaults OFF, requires explicit consent. Audio processed entirely on-device, never saved. 👁 tray indicator when active.
- **Wake word commands** — `wake_word_start`, `wake_word_stop`, `wake_word_set_config`, `wake_word_status` Tauri commands. `WakeWordConfig` in settings (enabled, keyword, threshold, show_indicator).

**Recording & Audio:**

- **Recording auto-stop** — silence watchdog with configurable duration. `set_recording_auto_stop` command, `auto_stop_enabled` + `auto_stop_duration_secs` in `AudioRecordingManager`.
- **Hands-free auto-listen** — auto-rearms mic after Brain+TTS finishes with 250ms grace period to avoid capturing room reverb. Controlled by `brain.auto_listen` setting.
- **Always-on mic for wake word** — `enable_wake_word()` in `AudioRecordingManager` activates always-on microphone stream when wake word detection is running.

**Developer & Diagnostics:**

- **Better sentinel clipboard** — `capture_selection_text()` now writes unique sentinel before Ctrl+C, allowing reliable detection of "no selection" vs "clipboard unchanged".

### Changed

- **Dependencies Upgrade** — Safely updated backend and frontend dependencies to their latest compatible versions, including Tauri v2.11.2, once_cell v1.21.4, rusqlite v0.40.1, rusqlite_migration v2.6.0, chrono v0.4.45, regex v1.12.4, flate2 v1.1.9, sha2 v0.11.0, clap v4.6.1, tauri-plugin-dialog v2.7.1, and @types/node v25.9.3.
- **Specta v2 Type Mapping** — Converted `duration_ms`, `id`, and `timestamp` type overrides from `f64`/`Option<f64>` to `u32`/`Option<u32>` in `HistoryEntry` and `HistoryUpdatePayload` to resolve TypeScript compilation issues with nullable fields.
- **Auto-stop watch parameters** — Changed parameter type for `set_recording_auto_stop` from `u64` to `u32` to comply with Specta's BigInt restrictions.
- **Kokoro backend** — replaced `parking_lot::Mutex` with `std::sync::Mutex`, removed external dependency.
- **PiperBackend** — implements `WarmEngine` trait with `warm()`/`unload()`/`status()` methods. Tracks `last_used` timestamp for idle timeout.
- **TTS manager** — `speak()` now respects `tts_shorten_first_chunk` setting, splits first clause near 60 chars via `split_at_clause_boundary`.
- **Brain manager** — `ask()` concatenates `speakable_output_prompt` when `read_aloud` is ON. Emits `brain:latency` events with per-stage timestamps.
- **ConversationView** — latency HUD bar shows color-coded EP/STT/TTFT/TTFA. TTS toggle button in header. `ai_replace_selection` import.
- **Continuous voice** — 250ms grace re-arm, respects `auto_listen` setting.

### Fixed

- **Frontend Type Safety** — Resolved TypeScript compiler errors in `ConversationView.tsx` (added null-checks to `settings.brain`) and `SpeechSettings.tsx` (provided `?? null` fallback for `greeting.engine`).
- **sherpa-onnx CRT conflict** — removed `sherpa-onnx` dependency due to `/MT` static CRT vs. `/MD` dynamic CRT conflict with `whisper-rs-sys` on Windows. VAD-based wake word retained; KWS integration code preserved in git history. To re-enable: add `sherpa-onnx = "1.13.2"` to `Cargo.toml` and download KWS model files to `models/wake_word/`.
- **Specta TS bindings export** — softened to warning (no longer crashes debug builds) while root cause is investigated.

### Added Files

- `src-tauri/src/commands/discovery.rs` — Ollama/LM Studio/llama.cpp auto-discovery
- `src-tauri/src/commands/wake_word.rs` — wake word commands
- `src-tauri/src/tts/audio_format.rs` — MP3/OGG/FLAC conversion
- `src-tauri/src/wake_word.rs` — VAD-based wake word detector (KWS-ready architecture)
- `src-tauri/src/clipboard_ax.rs` — cross-platform selection capture (subsequently removed; code folded into `clipboard.rs`)

**Documentation Overhaul:**

- **S2B2S_REVIEW.md** — new 91KB comprehensive project analysis covering 21 sections: architecture deep dive, all 3 pipelines, STT/TTS/Brain subsystems, TripleVAD, text normalization (4 passes), audio toolkit, model management, settings, frontend architecture, i18n, CI/CD, project lineage/donor map, dependency analysis, complete file tree, roadmap, known issues, platform matrix, and 6 ASCII diagrams. Serves as reference for non-tech users, developers, and AI agents.
- **README.md** — complete rewrite with table of contents, default stack table, all pipeline diagrams, text normalization pass tables, full architecture section, CLI/env vars reference, sponsor section.
- **AGENTS.md** — full architecture tree visualization, frontend+backend structure maps, technology stack table, i18n details, code style, platform notes, key files reference.
- **BUILD.md** — macOS Intel ONNX Runtime setup, env vars table, CI/CD workflow table, project structure overview.
- **CLAUDE.md** — expanded from single line to full entry point doc referencing all key project files.
- **CONTRIBUTING.md, CONTRIBUTING_TRANSLATIONS.md, CRUSH.md** — all updated with current state, commands, and architecture info.
- **PR template** — softened feature-freeze language to focus on priorities rather than rejection.
- **Bug report template** — added crash log path and debug mode instructions.

**Core STT / VAD:**

- **Triple VAD as default** — 3-stage voice activity detector (RMS energy gate → RNNoise voice probability → Silero VAD) is now the default for all modes. Provides better noise rejection at ~2ms additional latency per frame.
- **RNNoise voice probability threshold** — new `rnnoise_voice_threshold` setting (0.05–0.9, default 0.2) with slider in Advanced → Audio Enhancements. Controls how aggressively RNNoise filters non-speech audio.

**Text Normalization Pipeline (ITN + TN + Markdown):**

- **ITN (Inverse Text Normalization)** via `text-processing-rs` (Apache 2.0) — spoken-form ASR output normalized to written form: "two hundred thirty two" → "232", "january fifth" → "January 5, 2025". Applied post-STT in both dictation and conversation pipelines.
- **TN (Text Normalization)** via `text-processing-rs` — written-form text normalized to spoken form before TTS: "$5.50" → "five dollars and fifty cents", "123" → "one hundred twenty three".
- **Markdown stripping** (regex-based, replaced `pulldown-cmark`) — headings, bold, lists, links, code blocks, HTML entities all converted to natural spoken form before TTS.

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
- **Streaming gapless playback** — fragment _i+1_ synthesized while _i_ plays. UTF-8-safe sentence pagination.
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

### Removed

- **IMPROVEMENT_PLAN.md** — deleted the improvement plan file.
