# S2B2S — Comprehensive Improvement Plan v4

> **Last updated:** June 2026
> Status icons: ✅ Done · 🚧 In progress · 📋 Planned

---

## Phase 1 — Quick Wins ✅ COMPLETE

| # | Item | Files Changed | Status |
|---|------|--------------|--------|
| 1 | **Speakable-output system prompt** — separate prompt appended when `read_aloud` is ON, instructs LLM to answer conversationally | `settings.rs` (BrainConfig.speakable_output_prompt), `brain/manager.rs` (concatenates in `ask()`) | ✅ |
| 2 | **TTS toggle in conversation UI** — 🔊/🔇 button in ConversationView header, toggles `read_aloud` per-chat | `ConversationView.tsx` (Volume2/VolumeX icons, live toggle) | ✅ |
| 3 | **Ollama/LM Studio model discovery** — probes `:11434/api/tags`, `:1234/v1/models`, `:8080/v1/models` at startup | `commands/discovery.rs` (new), `commands/mod.rs`, `lib.rs` (invoke handler) | ✅ |
| 4 | **Recording auto-stop command** — silence watchdog with configurable duration + enable/disable | `commands/audio.rs`, `managers/audio.rs` (set_auto_stop) | ✅ |
| 5 | **RNNoise standalone toggle** — already existed, exposed via `set_noise_suppression_enabled` | Already implemented in `commands/audio.rs:332` + settings `noise_suppression_enabled` | ✅ |

## Phase 2 — Conversation & Brain Depth ✅ COMPLETE

| # | Item | Files Changed | Status |
|---|------|--------------|--------|
| 6 | **Sentence splitter optimization** — clause-split for fast TTFA (`split_at_clause_boundary` at 60 chars), abbreviation window, strong boundary preference | `brain/client.rs` (new `split_at_clause_boundary`, improved `force_clause_boundary`) | ✅ |
| 7 | **Shorten-first-chunk in TTS manager** — uses `tts_shorten_first_chunk` setting to split first clause near 60 chars before standard pagination | `tts/manager.rs` (speak() checks setting, calls `split_at_clause_boundary`) | ✅ |
| 8 | **Latency HUD** — per-stage timestamps (endpoint→STT→first_token→first_audio) emitted as `brain:latency` events, color-coded display in conversation UI | `brain/manager.rs` (emit_latency, timing markers), `ConversationView.tsx` (latency bar) | ✅ |
| 9 | **AI Replace Selection** — select text + hotkey + speak instruction → Brain rewrites in place | `commands/brain.rs` (ai_replace_selection), `settings.rs` (shortcut binding), `lib.rs` (register) | ✅ |
| 10 | **Brains auto-discovery** — returns discovered servers with model lists from Ollama/LM Studio/llama.cpp | `commands/discovery.rs` (discover_local_brains, is_ollama_running) | ✅ |

## Phase 3 — TTS Backend Fixes ✅ COMPLETE

| # | Item | Files Changed | Status |
|---|------|--------------|--------|
| 11 | **Kokoro `parking_lot` dependency removed** — replaced with `std::sync::Mutex` | `kokoro.rs` (Mutex, `lock().unwrap()`) | ✅ |
| 12 | **TextFragment fields public** — ensured `text`, `index`, `total` are accessible for shorten-first-chunk | `pagination.rs` (already public, verified) | ✅ |
| 13 | **Piper server health monitoring** — already robust with generation-based cancellation, stdout/stderr drain threads, CUDA warm-up synthesis, health polling | `piper_server.rs` — already mature (706 lines) | ✅ |

## Phase 4 — TTS Ecosystem 📋 PLANNED

| # | Item | Approach | Priority |
|---|------|----------|----------|
| 14 | **Kokoro worker pool + crossfade** — wire `tts-rs` synthesis, implement 2-worker pool (Parrot pattern), integrate `crossfade()` into playback | L (1-2 weeks) — needs `tts-rs` crate verification | High |
| 15 | **Warm model unload timeout** — implement `WarmEngine` trait on PiperBackend, add idle watcher thread checking `ModelUnloadTimeout`, tray "Unload Model" | M (3-5 days) | High |
| 16 | **History bulk ops frontend** — multi-select checkboxes, Select All, Export JSON/MD, Delete with two-stage confirm — backend already has `delete_history_entries` + `export_history_entries` | S (1-2 days) — mostly frontend | Medium |
| 17 | **Save-to-file MP3/OGG/FLAC** — ffmpeg shell-out (CopySpeak pattern) for local engines, pass-through for cloud engines (ElevenLabs server-encoded) | M (3-5 days) — needs encoder decision | Medium |
| 18 | **Selection capture cross-platform** — macOS AX API FFI + sentinel clipboard fallback (Parrot pattern) | M (3-5 days) — needs FFI + platform gating | Medium |
| 19 | **Double-copy macOS/Linux** — NSPasteboard.changeCount polling (macOS), arboard/X11 polling (Linux) — stub exists | M (3-5 days) — platform-specific | Medium |
| 20 | **Waveform HUD** — `AmplitudeEnvelope` extraction + canvas rendering during TTS playback | M (3-5 days) — backend + frontend | Medium |
| 21 | **Hands-free auto-listen** — auto-re-arm mic after Brain+TTS finishes, 250ms grace — `continuous_voice.rs` already exists | S (1-2 days) | Medium |
| 22 | **Recording auto-stop wire into recorder** — continuous voice mode already has 40-frame silence detection; wire `auto_stop_enabled` to recorder loop | S (1 day) | Medium |
| 23 | **Debug mode log viewer** — in-app scrollable log viewer with level filters, search, export | M (3-5 days) | Medium |
| 24 | **Wake word** — "Hey S2B2S" via sherpa-onnx KWS (ONNX runtime already in-use for Kokoro) | L (1-2 weeks) — needs eval | Low |

---

## Architecture Changes Summary

### New Files Created

| File | Purpose |
|------|---------|
| `src-tauri/src/commands/discovery.rs` | Ollama/LM Studio/llama.cpp auto-discovery + model listing |

### Files Modified

| File | Change |
|------|--------|
| `settings.rs` | Added `speakable_output_prompt`, `conversation_mode`, `endpoint_preset`, `headphone_mode`, `auto_listen` fields + AI Replace shortcut binding |
| `brain/manager.rs` | Speakable-prompt concatenation, latency timing markers + `emit_latency()` |
| `brain/client.rs` | `split_at_clause_boundary()` for fast TTFA, improved `force_clause_boundary()` with strong boundary preference |
| `tts/manager.rs` | Shorten-first-chunk in `speak()` using `tts_shorten_first_chunk` setting + `split_at_clause_boundary` |
| `tts/backends/kokoro.rs` | `parking_lot::Mutex` → `std::sync::Mutex` |
| `commands/audio.rs` | `set_recording_auto_stop()` command |
| `commands/brain.rs` | `ai_replace_selection()` command |
| `commands/mod.rs` | Added `pub mod discovery` |
| `lib.rs` | Registered discovery + ai_replace + auto_stop commands |
| `managers/audio.rs` | `auto_stop_enabled` + `auto_stop_duration_secs` fields + `set_auto_stop()` |
| `ConversationView.tsx` | TTS toggle button, latency HUD bar, Volume2/VolumeX icons |

### Key Dependencies (no new crates added)

All new features use existing dependencies (`reqwest`, `serde`, `serde_json`, `std::sync`).

---

## Success Criteria for Remaining Items

- **Kokoro worker pool + crossfade**: 2-parallel synthesis, 10ms crossfade @ 24kHz, < 50ms gap between fragments
- **Warm model unload**: Piper server unloads after idle timeout, re-loads on next synthesis < 2s
- **History bulk ops**: Select All / Export / Delete all work with visual feedback, zero backend changes needed
- **Save-to-file**: ffmpeg-less option via pure Rust encoders preferred; ffmpeg fallback acceptable
- **Selection capture**: AX API on macOS (no clipboard touch), sentinel clipboard on Win/Linux
- **Double-copy**: macOS NSPasteboard + Linux X11/Wayland polling at 200ms
- **Latency HUD**: ≤1.5s end-of-speech→first-audio target, color-coded per-stage display
