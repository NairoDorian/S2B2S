# S2B2S Agent Takeover & Living Memory Protocol

> **Purpose:** This file is the official living handoff document between sequential AI coding agents.  
> **Rule of Maintenance:** The section headers in this file **NEVER** change. Every agent taking over or finishing a turn must update the _content_ beneath the headers following the lifecycle rules defined below.

---

## 1. Living Protocol (Rules for Future Agents)

1. **Section 2 (`Active Session Pass-Over`) is TRANSIENT:**
   - On every session end or handoff, **completely replace or prune** the text under Section 2.
   - Remove obsolete notes, failed temporary attempts, or resolved bug notes.
   - Keep only the exact immediate build status, active work in progress, and next immediate action.

2. **Section 3 (`Project Memory & Durable Knowledge`) is ACCUMULATING:**
   - Add new verified facts, model quirks, hardware fixes, and architectural decisions.
   - Refine, synthesize, or modify existing notes as understanding deepens (prevent unbounded bloat).

3. **Section 4 (`Sequential Roadmap & Task Tracker`) is STATEFUL:**
   - Mark completed tasks with `[x]`, in-progress tasks with `[/]`, and pending tasks with `[ ]`.
   - Never skip ahead across milestones unless unblocked.

4. **Section 5 (`Subsystem Reference & Pre-Commit Routine`) is FIXED SCHEMA:**
   - Update file paths only when files are created, renamed, or refactored.

---

## 2. Active Session Pass-Over (Transient State)

### Immediate Status & Build Health

- **App Version:** `0.1.4` (cargo, package.json, tauri.conf.json in sync).
- **Upstream Sync:** 0 commits behind `cjpais/Handy:main` (portable HF_HOME fix merged as `6505e0fc`).
- **Milestone M0 (Honesty & Hygiene Sweep) — COMPLETE.** Backend: `cargo check` clean (0 warnings), `cargo test` 315/315 pass, specta bindings regenerated. Frontend: `tsc --noEmit` clean, 24 locales synced + verified, all 5 Playwright specs pass (dictation spec rewritten for the current overlay API; mock now honors `onboarding_completed` + `__settingsOverrides`).
- **Milestone M1 (AI Replace Selection) — COMPLETE.** `selection.rs` (Windows UIA non-destructive read + clipboard fallback), prompt-variable replacer extended, `AiReplaceAction` wired into ACTION_MAP with events + abort, `AiReplaceSettings` card + i18n. Verified: `cargo test` 315/315, clippy 0 warnings, `tsc` clean, translations synced, Playwright 5/5.
- **Known environment issue (pre-existing, NOT caused by M0/M1):** `bun run lint` crashes — `typescript-eslint` rejects the pinned `typescript@7.1.0-dev.20260813.1` ("does not support TS 7.0"). Needs a dep fix (pin TS 6.x or bump typescript-eslint); do not block on it.

### Current Task in Progress

- None. M0, M1, M2 complete (M2.6 stretch remains); M3: M3.1, M3.2 done; M3.3 removed per user decision. Next: M3.4 text replacement rules, M3.5 cloning recorder, M3.8 region capture.

### Next Immediate Action for Takeover

1. M3.4: text replacement rules table (case-sensitive/regex, escape sequences) + realtime decapitalize wiring.
2. M3.5: in-app voice-cloning reference recorder (record → Pocket/Qwen3 import).

### Active Traps & Blockers

- `bun run lint` is broken repo-wide (see above). Use `tsc`, `prettier --check`, `cargo clippy` as substitutes until deps are fixed.
- `conversation_mode` (push_to_talk|toggle|hands_free) is stored but still NOT honored by the backend — deliberately not exposed in the M0.3 UI. Either wire it (M2) or remove the field.
- `session_manager.rs` half-built async-ownership API still present behind `#[allow(dead_code)]` — removed as part of M5.3, not now.
- Keep LLM calls strictly on local `llama-server.exe` (port 8001/8080) with Gemma 4 or Qwen models.

---

## 3. Project Memory & Durable Knowledge (Accumulating)

### STT & transcribe.cpp Invariants

- **Local Engine**: `transcribe.cpp` uses GGUF models. Primary models: Nemotron 3.5 Streaming (Vulkan/CUDA cache-aware), Parakeet TDT 0.6B v3, Whisper Turbo, SenseVoice.
- **Multi-STT Slot Resolution**: Model paths must be resolved via `model_manager.get_model_path(&model_id)` to handle both `models/` directory and HuggingFace cache gracefully without panics.
- **Multimodal Ground-Truth ASR**: Gemma 4 2B running on `llama-server.exe` with `mmproj-F16.gguf` accepts raw 16kHz WAV audio in `input_audio` payload to provide a 2nd independent acoustic hypothesis in ~580ms.
- **Consensus Fusion**: When Multi-STT is enabled, `DEFAULT_MULTI_STT_MERGE_PROMPT` ("Merge and Clean") automatically fuses hypotheses `${output}`, `${output2}`, `${output3}` into a final clean transcript.
- **Portable HF_HOME**: Since upstream `9e534a3d`, portable mode sets `HF_HOME=<Data>/huggingface` so hf-hub snapshots stay inside the portable Data dir. Do not bypass this when adding model-download code paths.

### Brain, Llama.cpp & Local Model Rules

- **Local Server Lifecycle**: Pre-compiled `llama-server.exe` managed by [`src-tauri/src/llama_server/manager.rs`](file:///src-tauri/src/llama_server/manager.rs). Auto-detects GPU backend (Vulkan0, CUDA 12/13, Apple Metal, CPU AVX2). Fixed 16384 context, `--parallel 1`, MTP draft decoding.
- **Zero-Cloud Default**: All features default to offline local operation. No external API keys required for core functionality.
- **Context fragility**: Brain history is an unbounded `Vec<ChatMessage>` truncated to the last `context_turns * 2` messages (`src-tauri/src/brain/manager.rs:150-156`). No token counting, no compaction — long turns silently overflow the 16k context. Failed user turns are dropped, not remembered (`brain/manager.rs:312-322`).

### TTS Engines & Runtime Bindings

- **Offline Engines (6 local)**: Qwen3-TTS (GGML or PyTorch CUDA Graphs via `faster-qwen3-tts`), Kokoro-82M (ONNX HTTP server), Pocket TTS (zero-shot cloning), Piper (ONNX), Kitten TTS, SAPI (Windows COM).
- **Cloud Engines (3)**: OpenAI, ElevenLabs, Cartesia. **Their API-key configs have no frontend UI** (`SpeechSettings.tsx` only shows the engine dropdown) — effectively unusable from the GUI until M0.4 lands.
- **Audio Output**: `rodio` streaming gapless player (`src-tauri/src/tts/player.rs`) with sub-20ms instant flush on barge-in. Only single-entry WAV→MP3/OGG/FLAC export exists (`commands/tts.rs:70-110`); no batch conversion or listen-later queue.

### Verified Review Findings (Aug 2026 — backend)

- **Milestone-1 half-truth**: `ai_replace_selection` command EXISTS (`commands/brain.rs:12`, registered `lib.rs:853`) and a default `ai_replace` binding EXISTS (`settings.rs:1955-1966`), but: no ACTION_MAP/shortcut-handler wiring, no OS selection capture, no frontend caller, no prompt-variable replacer. Treat as scaffold, not feature.
- **Stored-but-dead settings (post-M0 status)**: `endpoint_preset`/`headphone_mode` NOW WIRED (M0.1); wake-word `energy_threshold` NOW WIRED (M0.2). Still dead: `conversation_mode`, `auto_listen` (wired — used in `continuous_voice.rs:272`), `reply_language` (wired via `resolve_reply_language`), `speakable_output_prompt` (wired in `brain/manager.rs`). Remaining unwired: `WakeWordConfig.keyword` (reserved for future KWS), `show_indicator`, `custom_filler_words` list, `tts_save_format`/`tts_workers`/`pagination` UI.
- **Dead code removed (M0.5)**: `tts/fragment_queue.rs`, `managers/transcription_mock.rs`, `recording_session.rs` (all refs → `session_manager`), frontend `ModelFilterBar`/`ModelMetadataPanel`/`useModelFilters` (recoverable from git), `PostProcessActions` (wired, not deleted), KeyboardShortcutsModal/DeveloperHub/DevConsoleLogLevelSelector/DebugPaths/TextDisplay/TranscriptionIcon/`lib/shortcuts.ts`.
- **Triplicated recording state machines**: `TranscriptionCoordinator` stage vs `session_manager::SessionState` vs `AudioRecordingManager::RecordingState`; `recording_auto_stop.rs:89-98` bypasses the coordinator. Consolidate into one owner (coordinator as single entry point).
- **Duplicated infra**: two LLM HTTP clients (`brain/client.rs` SSE vs `llm_client.rs` post-processing) with duplicate structs; two local-TTS server lifecycles (`piper_server.rs` vs `local_tts_server.rs`) with split status commands; parakeet-EOU detection duplicated (`transcription.rs:1631`, `stt/multi_stt.rs:479`).
- **Panic audit debt**: 401 `.unwrap()` in backend — hot spots `managers/audio.rs` (43), `managers/model.rs` (68), `managers/transcription.rs` (28); live-path `expect()` at `continuous_voice.rs:145`.
- **God files (Phase 3 refactor targets)**: `managers/model.rs` (3369), `managers/transcription.rs` (2814), `settings.rs` (2612), `actions.rs` (1750), `shortcut/mod.rs` (1668), `lib.rs` (1351), `clipboard.rs` (1236).
- **Good foundations to build on**: GGUF header parser (`managers/gguf_meta.rs`, wanted-keys + truncation-as-error), generation counters for stale-worker rejection (`session_manager.rs`), PTT debounce/regression tests, sentence splitter (`brain/client.rs:311-446`), settings snapshot capture, trailing audio buffer (`extra_recording_buffer_ms`, `managers/audio.rs:935-955`), recording auto-stop.

### Verified Review Findings (Aug 2026 — frontend, post-M0)

- **Remaining cleanup**: `TtsPlayer::is_paused`, realtime decapitalize path (`text_replacement_decapitalize.rs:26,135-144`), `session_manager.rs` ownership API — deferred to M5.
- **Bugs fixed in M0**: `AccessibilityPermissions` double render, `latencyHud` unrendered, `BrainOverlayApp dir="en"`, ~12 hardcoded i18n strings, cloud TTS keys unreachable, stale Playwright dictation spec + mock onboarding gate.
- **Conversation persistence**: `ConversationView.tsx:30` keeps messages in `useState` only — nothing survives navigation/restart; SQLite history covers dictation/TTS but not conversations (M4.3).
- **i18n**: 24 locales; new M0 keys synced to all; `bun run check:translations` green.
- **Testing**: `bun run test:playwright` 5/5 green. `bun run lint` broken by TS 7.1-dev vs typescript-eslint (pre-existing).

### Patterns Worth Adopting from Reference Projects

- **AIVORelay**: operation-id stale-result rejection (`is_operation_current` before every UI side effect); `Arc::ptr_eq` token claim for auto-stop timers; captured-settings snapshot at recording start; subtitle.rs pure SRT/VTT formatter (6s/0.8s/84-char cue limits); diarization temp-artifact + validated speaker reapply; region-capture logical→physical DPI math with hide-window→50ms→capture ordering; browser connector ECDH P-256 + HKDF + AES-256-GCM handshake; resumable TTS workspace (alternating JSON checkpoint slots + sha256 segments + config-signature invalidation + fs2 lease); UIA-based selection reading that never touches the clipboard (Windows) with clipboard fallback.
- **copyspeak**: audio effects as pure `AudioBuffer→AudioBuffer` Web Audio transforms behind a registry (frontend-only DSP); TTS voice profiles with tagged `{engine, ...knobs}` options and migration ladder (secrets global, knobs profile-owned); engine health check = real "Hello." synthesis through the same runtime, with stable `error_type` strings; HUD hidden = parked off-screen (avoids WebView2 transparent-window repaint bug); installer marker protocol `[STEP]/[DONE]/[ERROR] name` + `manifest.json` as install-state source of truth; history file-tracking map (path↔entry) for O(1) cleanup; import/export via full-config JSON + backend `validate_config` + diff-driven side effects + env-stripping of secrets.
- **speech-to-speech**: generation counter `CancelScope` (snapshot at task start, `is_stale` poll at each await); speculative turns — tag everything `(turn_id, revision)`, generate immediately on endpoint but delay _commit_ (TTS start, chat write-back) by a reopen grace window, bump revision on reopen and drop stale work at every stage gate; two-tier endpointing (cheap Silero boundary + Smart-Turn end-of-turn classifier choosing 800ms vs 2000ms grace); sentence batching (`stream_batch_sentences`) + TTS input coalescing (merge consecutive same-response `TTSInput`s); context compaction via dense-JSON summary prompt in a single-flight background worker with a generation counter; `<code>…</code>` tool-call blocks parseable from any local LLM stream with schema validation before execution; response-key visibility barrier (nothing client-visible before its `response.created`); "always emit terminal sentinel even on error" so state machines never deadlock.

### Cross-Platform & UI Design Rules

- **Cross-Platform Mandate**: Windows 11 (Top priority), macOS (First-class), Linux (First-class). Every `#[cfg(target_os = "...")]` must provide fallbacks.
- **Color Theme**: Golden Accent Palette (`--color-logo-primary: #f59e0b` / `#d97706`, `--dark-color-logo-stroke: #fef3c7`, UI background accent: `#d97706`). **No pink or purple accents.**
- **Internationalization**: 24 languages in `src/i18n/locales/`. Every new user-facing string must be in `en/translation.json` and synced via `bun run sync:translations`.

---

## 4. Sequential Roadmap & Task Tracker (Stateful Milestones)

> Consolidated from: AIVORelay implementation plans (`AIVO_RELAY_IMPLEMENTATION_PLAN.md`, `AIVO_update_integration_plan.md` — historical, superseded by this section), copyspeak, and speech-to-speech deep dives. Sequencing rule: M0 fixes dishonesty before building; M2 conversation quality outranks ecosystem breadth (it's the product's core); M5 refactors run last so they don't destabilize new features.

### Milestone M0: Honesty & Hygiene Sweep (cheapest wins, highest trust impact)

- [x] **M0.1** Wire `endpoint_preset` (snappy/balanced/patient → real ms) + `headphone_mode` into continuous-voice EOU (`recorder.rs:996,1028`). Added `endpoint_silence_frames` atomic (live-updatable, no stream restart) + `endpoint_frames_for_preset()`; `change_brain_config` applies it live; `headphone_mode` now gates the barge-in abort listener (`continuous_voice.rs`).
- [x] **M0.2** Wake word honesty: stale comments fixed (`wake_word.rs:52,99`); `WakeWordConfig.threshold` (unused 0.6) replaced with `energy_threshold` (RMS, default 0.03, live-applied, clamped); UI card added in SpeechSettings with honest "energy activation" labeling.
- [x] **M0.3** Expose dead Brain settings in UI: `endpoint_preset`, `headphone_mode`, `auto_listen`, `reply_language`, `speakable_output_prompt` added to BrainSettings behavior group. `conversation_mode` intentionally NOT exposed (backend ignores it — see traps).
- [x] **M0.4** Cloud TTS credential UI: conditional OpenAI/ElevenLabs/Cartesia API-key/model/voice fields in `SpeechSettings.tsx` (previously unreachable).
- [x] **M0.5** Dead-code sweep: deleted backend `tts/fragment_queue.rs`, `managers/transcription_mock.rs`, `recording_session.rs` shim (all refs migrated to `session_manager`); frontend: wired `PostProcessActions` into PostProcessingSettings (recovers the whole post-process-actions feature), deleted `ModelFilterBar`/`ModelMetadataPanel`/`useModelFilters` (unwired filter UI — recoverable from git, re-wire in M3 if desired), `KeyboardShortcutsModal`, `DeveloperHub`, `DevConsoleLogLevelSelector`, `DebugPaths`, `TextDisplay`, `TranscriptionIcon`, `lib/shortcuts.ts`, dead `prepare/consumeModelDownloadAutoActivation` exports.
- [x] **M0.6** Frontend bug fixes: single `AccessibilityPermissions` render (was doubled), `latencyHud` now rendered, `BrainOverlayApp` RTL `dir` fixed, ~12 hardcoded i18n strings i18n-ized (Brain/Speech/LlamaCpp/PostProcessing), brain history appends user turn on error (`brain/manager.rs`), `continuous_voice.rs` `expect()` removed; Playwright dictation spec rewritten for the current overlay DOM + mock `onboarding_completed`/`__settingsOverrides` support (5/5 specs green).

### Milestone M1: Complete AI Replace Selection (finish the scaffold)

- [x] **M1.1** OS selection capture: new `selection.rs` — Windows UIA text-pattern reader (non-destructive, never synthesizes Copy; focused-element fast path + Chromium Document-control fallback), clipboard sentinel capture as fallback; macOS/Linux use the clipboard capture. `windows` crate features extended with `Win32_UI_Accessibility`.
- [x] **M1.2** Prompt-variable replacer: `substitute_context_variables` now supports `${active_app}` (alias), `${selected_text}`, `${clipboard}`, `${time_local}` alongside `${current_app}`; applied in post-processing and AI Replace paths.
- [x] **M1.3** Wired `ai_replace` into ACTION_MAP (`AiReplaceAction`): capture → instruction (settings) → non-streaming rewrite via shared `actions::rewrite_selected_text` (Brain provider; no history/TTS pollution) → `clipboard::paste` replaces the selection. Events `ai-replace:started/done/error`; single-flight abort via `AI_REPLACE_ABORT` + `ai_replace_abort` command; shortcut gated on `brain.enabled` and re-registered on `change_brain_config`.
- [x] **M1.4** Frontend: `AiReplaceSettings` card in post-processing (instruction textarea with variable hints, shortcut capture, sonner toasts for the three events); `ai_replace_instruction` setting + `change_ai_replace_instruction_setting` command; i18n synced (24 locales).

### Milestone M2: Smart Conversation Core (from speech-to-speech)

- [x] **M2.1** Turn system: `SpeculativeTurnTracker` (`src-tauri/src/speculative_turns.rs`) — `new_turn`/`reopen`/`is_latest`/`commit_if_latest`/`cancel`/`prune` over `(turn_id, revision)`, managed as Tauri state; recorder tags every utterance; pipeline stages gate side effects on `is_latest`. 5 unit tests.
- [x] **M2.2** Two-tier endpointing: silence endpoint now arms a reopen-grace window (400ms snappy / 800ms balanced / 2000ms patient from `endpoint_preset`); speech during the grace reopens the same turn (revision+1, samples continue accumulating); grace expiry finalizes. The pipeline re-checks staleness after STT and before Brain ask, and `commit_if_latest` gates the TTS wait — a barge-in listener now spans the whole pipeline (STT + thinking + TTS) and cancels the turn. (No SmartTurn classifier yet — the preset doubles as the confidence tier; real SmartTurn ONNX is a future enhancement.)
- [x] **M2.3** Brain context compaction: token-aware context building (`select_context_messages` — never exceeds llama.cpp 16k / cloud 8k budget minus headroom), optional LLM summarization (`compaction_enabled`, default on) with dense-JSON prompt, single-flight background worker, generation counter rejecting stale splices, prose fallback parse; hard truncation fallback when disabled or on failure; `brain:history-compacted` event; `compaction_enabled` exposed in BrainSettings; 5 unit tests.
- [x] **M2.4** TTS sentence coalescing: the sentence consumer drains same-generation backlog up to `MAX_COALESCE_CHARS` (800) into single synthesis calls — fewer model invocations, better cross-sentence prosody — while the first sentence still synthesizes alone for fast first audio. (Sentence batching skipped: per-sentence TTS emission already minimizes time-to-first-audio; coalescing covers the backlog case.)
- [x] **M2.5** Tool calling for local models: new `brain/tool_calls.rs` — `<code>…</code>` block prompt + streaming interceptor (handles tags split across arbitrary token boundaries via partial-prefix buffers, swallows blocks from UI/TTS, executes locally, emits `brain:tool-call`), schema-validated built-ins (get_current_time, read_clipboard, copy_to_clipboard), results fed back in a single bounded follow-up turn so the assistant speaks the answer; `tools_enabled` toggle (default off) in BrainSettings; 9 unit tests.
- [ ] **M2.6** (Stretch) Local OpenAI Realtime WebSocket gateway (`ws://127.0.0.1:8765/v1/realtime`) — axum + tokio-tungstenite; protocol layer (response keys, item ordering, lazy `response.created`) separate from pipeline layer; barge-in order: cancel → flush queues → re-enable mic.

### Milestone M3: Dictation Ecosystem (from AIVORelay)

- [x] **M3.1** Subtitle export: new `subtitle.rs` — pure SRT/VTT formatters + cue grouping (ported from AIVORelay) + `history_entries_to_subtitle_segments` (timestamps + recorded duration or chars-per-sec estimate); `export_history_subtitle` command writes timed `.srt`/`.vtt` of all dictation history; export buttons in HistorySettings; 5 unit tests.
- [x] **M3.2** Batch file transcription: new `file_transcription.rs` — rodio decode (wav/mp3/m4a/mp4/ogg/flac via symphonia-aac/isomp4 features), mono downmix + 16kHz rubato resample, extension validation, recording-session guard, `transcribe_audio_file_command` exporting `.txt`/`.srt`/`.vtt` to `<app data>/transcripts/`; `text_to_subtitle_segments` (sentence cues, proportional timing, 120-char cap); UI buttons in HistorySettings; 3 new subtitle tests. (Diarization deferred — requires provider word timings.)
- ~~**M3.3** Multi-profile transcription~~ — **REMOVED per user decision** (profiles + auto-switch dropped entirely; no residue in settings/commands/UI).
- [ ] **M3.4** Text replacement rules table (case-sensitive/regex, escape sequences) + wire the realtime decapitalize path.
- [ ] **M3.5** Voice-cloning reference recorder: record N seconds in-app → `pocket_import_cloned_voice` / `qwen3_import_cloned_voice` (close the loop the UI implies).
- [ ] **M3.6** Mic auto-switch (wildcard mask + manual fallback if device still present), input-channel selection, pause-media-while-recording.
- [ ] **M3.7** Resumable document TTS + listen-later queue: checkpoint workspace (alternating JSON slots, sha256, config signature, fs2 lease) + batch conversion panel.
- [ ] **M3.8** (Windows-gated) Region capture overlay → multimodal Brain (`ask_multimodal`); logical→physical DPI at confirm boundary; hide-window→50ms→capture.
- [ ] **M3.9** (Later) Browser connector (axum loopback + ECDH/HKDF/AES-GCM + extension export) and remote STT gateways (Soniox/Deepgram/OpenAI realtime) with automatic local fallback.

### Milestone M4: TTS UX & History (from copyspeak)

- [ ] **M4.1** Audio effects: registry pattern + walkie-talkie/game-boy as Web Audio transforms (frontend-only DSP), per-voice or per-profile toggle.
- [ ] **M4.2** HUD overlay hardening: adopt off-screen parking instead of hide/show; `hud:*` event protocol; amplitude streaming.
- [ ] **M4.3** History: search/filter/bulk actions; HTML export; file-tracking map for cleanup; conversation persistence (new SQLite table or history reuse) surfaced in `HistorySettings.tsx`.
- [ ] **M4.4** TTS voice profiles: tagged `{engine, knobs}` options, export/import JSON with id-collision remap, migration ladder (`schema_version`).
- [ ] **M4.5** Engine health checks: "Hello." synthesis through the real runtime + stable `error_type`s; per-engine install manifests + marker protocol.
- [ ] **M4.6** Import/export hardening: backend `validate_config`, diff-driven side effects, env-stripping of secrets.

### Milestone M5: Infrastructure Consolidation (STATUS.md Phase 3)

- [ ] **M5.1** Split god files: `managers/model.rs`, `managers/transcription.rs`, `settings.rs`, `actions.rs`, `shortcut/mod.rs`, `lib.rs`, `clipboard.rs`.
- [ ] **M5.2** Settings schema versioning + grouped sub-structs with explicit migrations.
- [ ] **M5.3** Unify: LLM clients (`brain/client.rs` + `llm_client.rs` shared structs), TTS server lifecycles (`piper_server.rs` + `local_tts_server.rs`), recording state machines (coordinator as single owner).
- [ ] **M5.4** Hot-path unwrap audit: `managers/audio.rs` (43), `managers/model.rs` (68), `managers/transcription.rs` (28), `continuous_voice.rs:145` — convert to logged fallbacks.
- [ ] **M5.5** Extract model catalog to JSON/TOML manifest (addresses `managers/model.rs` TODO) using the AIVORelay catalog schema (revision + sha256 pinning for mirrors).

---

## 5. Subsystem Reference & Pre-Commit Routine (Fixed Schema)

### Subsystem File Map

| Area           | Path                                                                                         | Responsibility                                        |
| -------------- | -------------------------------------------------------------------------------------------- | ----------------------------------------------------- |
| **STT Engine** | [`src-tauri/src/managers/transcription.rs`](file:///src-tauri/src/managers/transcription.rs) | transcribe.cpp streaming, VAD feeding, model switches |
| **Multi-STT**  | [`src-tauri/src/stt/multi_stt.rs`](file:///src-tauri/src/stt/multi_stt.rs)                   | Gemma 4 ASR (`mmproj`), parallel models, LLM merge    |
| **Local LLM**  | [`src-tauri/src/llama_server/manager.rs`](file:///src-tauri/src/llama_server/manager.rs)     | llama-server.exe process lifecycle, GPU offload       |
| **Brain**      | [`src-tauri/src/brain/manager.rs`](file:///src-tauri/src/brain/manager.rs)                   | Turn history, sentence splitting, TTS queueing        |
| **TTS**        | [`src-tauri/src/tts/manager.rs`](file:///src-tauri/src/tts/manager.rs)                       | Local TTS servers, gapless audio playback             |
| **Audio/VAD**  | [`src-tauri/src/audio_toolkit/`](file:///src-tauri/src/audio_toolkit/)                       | cpal recording, Silero VAD ONNX, RNNoise              |
| **Shortcuts**  | [`src-tauri/src/shortcut/handler.rs`](file:///src-tauri/src/shortcut/handler.rs)             | Global hotkey dispatcher and event handling           |
| **UI Theme**   | [`src/styles/theme.css`](file:///src/styles/theme.css)                                       | Golden theme tokens and color mappings                |

### Mandatory Pre-Commit Commands

```bash
bun run sync:translations    # 1. Sync all 24 translation languages
bun run check:translations   # 2. Verify all translation keys exist
bunx tsc --noEmit            # 3. TypeScript type checking
bun run format               # 4. Prettier + cargo fmt formatting
bun run lint:fix             # 5. ESLint auto-fix
cargo test                   # 6. Rust backend tests (319 tests)
bun run validate             # 7. Automated pre-commit verification gate
```
