# AIVORelay -- Fork of Handy (Category A)

> Repo: `MaxITService/AIVORelay` · HEAD: `v1.0.19` · License: MIT · Author: Maxim Fomin (MaxITService) · Platform: **Windows-only** (new features)
> Nature: **fork-of-Handy** -- merge-base near `d3a02810` (*"fix: prevent crash on macOS 26.x beta"*), ~**841 commits** of divergence. The most divergent Handy fork.
> Role for S2B2S: **the idea quarry** -- streaming STT patterns, WASAPI loopback, ECDH-encrypted local bridge, transcription profiles, AI Replace Selection, voice commands. Too divergent to merge wholesale; cherry-pick concepts and self-contained modules.

**Total codebase:** ~48,715 lines Rust (78 .rs files) + ~51,088 lines TypeScript/React/CSS = **~100K lines**. 4 active branches (`main`, `release/microsoft-store`, `integration/cuda`, `integration/combined`).

---

## 1. What AIVORelay Is

AIVORelay is Handy's core loop (hotkey -> speak -> text appears at cursor) extended into a Windows voice workstation. It adds 8 major feature clusters absent from Handy:

1. **Streaming STT** -- Soniox, Deepgram, and OpenAI Realtime Whisper all connected via WebSockets with word-level interim tokens, live preview window, and chunk-based insertion into target applications.
2. **System-audio capture** -- WASAPI loopback recording of speaker output, microphone, or both mixed together. Built on an independent audio pipeline that does NOT share recorders with regular STT.
3. **Transcription profiles** -- per-mode language/prompt/model presets with dedicated hotkeys, profile cycling, and auto-switching from the Windows input language.
4. **AI Replace Selection** -- select text anywhere, speak an instruction, LLM transforms and replaces the selection. Works in any Windows app.
5. **Connector** -- encrypted local HTTP bridge (ECDH-P256 + HKDF + AES-256-GCM) to a Chrome extension for voice-to-ChatGPT/Claude with optional screenshots.
6. **Live Preview window** -- always-on-top streaming window with interim/final text, configurable opacity/colors/themes/positioning, drag grip, edge resize.
7. **Voice commands** -- voice-triggered PowerShell with confirmation-gate, fuzzy matching (Levenshtein + Soundex + phonetic boost), and LLM fallback for unmatched commands.
8. **File transcription + subtitles** -- drag-and-drop audio -> text, SRT, or VTT with Deepgram/Soniox speaker diarization.

All original Handy features remain (local Whisper, VAD, PTT, LLM post-processing, history, two shortcut engines). Other platforms "should just use Handy" per the README.

---

## 2. Tech Stack
### 2.1 Frontend
| Layer | Choice | Purpose |
|-------|--------|---------|
| Framework | React 18 + TypeScript 5.6 | Main UI, settings, live preview, region capture |
| Build | Vite 6 | Dev server + bundling |
| State | Zustand 5 | settingsStore, navigationStore, transcribeFileStore |
| Styling | Tailwind CSS 4 | Utility-first CSS |
| i18n | i18next 25 | Multi-language UI |
| UI | Lucide React, react-select, sonner (toasts) | Icons, dropdowns, notifications |

### 2.2 Backend / Core (Rust)
| Layer | Choice | Purpose |
|-------|--------|---------|
| Framework | Tauri 2.10.2 | Desktop app shell, WebView2 |
| STT | transcribe-rs 0.3.8 (`whisper-vulkan`, `ort-directml`, `onnx`, `vad-silero`) | Local Whisper / Parakeet / SenseVoice / Canary / Cohere |
| Audio I/O | cpal 0.16, rodio (cjpais fork) | Mic/speaker capture + playback |
| VAD | vad-rs (Silero ONNX), nnnoiseless 0.5.2 (RNNoise) | Voice activity + denoise |
| HTTP server | axum 0.8, tower-http 0.6 (CORS) | Connector local bridge on `127.0.0.1:38243` |
| WebSockets | tokio-tungstenite 0.24 (rustls-tls), futures-util | Soniox/Deepgram/OpenAI realtime streaming |
| Crypto | aes-gcm 0.10, hkdf 0.12, hmac 0.12, p256 0.13 (ECDH), rsa 0.9, sha2 0.10 | Connector encrypted session protocol v3 |
| API keys | keyring 2.3 (Windows Credential Manager) | Secure at-rest storage |
| Screen capture | screenshots 0.8, windows 0.61 (Win32 APIs) | Region capture for screenshots |
| Keyboard | rdev (rustdesk-org fork), enigo 0.6, handy-keys 0.2 | Two shortcut engines + input simulation |

### 2.3 Key Dependencies (non-obvious)
- **windows 0.61.3** crate with 16 Win32 features for WASAPI audio endpoints, keyboard layout detection, WebView2 hardening
- **webview2-com 0.38.2** for disabling browser accelerator keys (Ctrl+F, F12, etc.)
- **screenshots 0.8** for multi-monitor screen capture
- **winreg 0.55** for Windows registry reads
- **natural 0.5.0** (Soundex/Metaphone) for phonetic voice-command matching
- **ferrous-opencc 0.2.3** for Chinese Traditional<->Simplified conversion on paste

---

## 3. Architecture & Source Map

### 3.1 Rust Backend (`src-tauri/src/`) -- 78 source files, 48,715 lines

```
src-tauri/src/
├── lib.rs                         (1,214 l)  Tauri setup, 17 managers initialized,
│                                             Playwright CDP, window geometry, 250+ commands
├── settings.rs                    (4,097 l)  AppSettings: profiles, voice commands, AI replace,
│                                             connector, 8 STT provider configs, Soniox context...
├── actions.rs                     (7,492 l)  Monolith: all shortcut actions, LLM template
│                                             context, live streaming orchestration, auto-flush
├── shortcut.rs                    (5,269 l)  Dual-engine shortcut manager (Tauri + rdev),
│                                             250+ change_* commands, PTT/toggle logic
│
├── managers/
│   ├── soniox_realtime.rs         (820 l)    **Soniox WebSocket streaming STT**
│   ├── deepgram_realtime.rs       (825 l)    **Deepgram WebSocket streaming STT**
│   ├── openai_realtime_whisper.rs (997 l)    **OpenAI Realtime WS streaming STT**
│   ├── soniox_stt.rs              (982 l)    Soniox async batch STT
│   ├── deepgram_stt.rs            (795 l)    Deepgram async batch STT
│   ├── remote_stt.rs              (1,356 l)  OpenAI-compatible remote STT (Groq, custom)
│   ├── connector.rs               (2,230 l)  **Encrypted local bridge server** (axum, ECDH+AES)
│   ├── live_sound_audio.rs        (528 l)    **WASAPI loopback + Both-mode mixing**
│   ├── live_sound_transcription.rs (364 l)   Live sound state, segments, auto-stop
│   ├── audio.rs                   (980 l)    Audio Recording Manager, mute/pause, VAD routing
│   ├── transcription.rs           (2,193 l)  Main transcription manager (local models)
│   ├── model.rs                   (1,331 l)  Model download/management, SHA256, VRAM
│   ├── history.rs                 (941 l)    SQLite history, audio file retention
│   ├── preview_output_mode.rs     (173 l)    Preview-vs-paste routing state machine
│   ├── llm_operation.rs           (51 l)     LLM request cancellation tracker
│   ├── key_listener.rs            (494 l)    rdev passive key listener (decapitalize)
│   └── microphone_auto_switch.rs  (167 l)    Windows default-device change detection
│
├── commands/
│   ├── voice_command.rs           (307 l)    PowerShell execution with confirm-gate
│   ├── file_transcription.rs      (956 l)    Audio file -> text/SRT/VTT pipeline
│   ├── connector.rs               (511 l)    Connector status/start/stop/queue/export
│   ├── audio.rs                   (381 l)    Audio device + mic permissions commands
│   ├── models.rs                  (364 l)    Model download/delete/switch commands
│   ├── live_sound_transcription.rs (110 l)   Live sound start/stop/clear/process
│   └── region_capture.rs          (85 l)     Region capture commands
│
├── audio_toolkit/
│   ├── audio/
│   │   ├── recorder.rs            (747 l)    **AudioRecorder**: cpal streams, frame callbacks
│   │   ├── noise_suppression.rs   (87 l)     nnnoiseless RNNoise wrapper
│   │   └── resampler.rs           (86 l)     rubato resampling
│   ├── text.rs                    (454 l)    **Filler/stutter filter + custom words fuzzy n-gram**
│   └── vad/
│       ├── silero.rs              (47 l)     Silero VAD wrapper
│       └── smoothed.rs            (96 l)     Smoothed VAD output
│
├── soniox_stream_processor.rs     (417 l)    **Streaming chunk assembler**: stable prefix,
│                                             custom words, text replacements, decapitalize
├── file_transcription_diarization.rs (337 l) Speaker block normalization, artifact persistence
├── subtitle.rs                    (178 l)    SRT/VTT formatting with timing
├── transcript_context.rs          (94 l)     Per-app short transcript memory (${short_prev_transcript})
├── language_resolver.rs           (269 l)    Soniox language code validation + hint list cleanup
├── region_capture.rs              (523 l)    **Native region capture overlay** (Windows only)
├── secure_keys.rs                 (249 l)    Windows Credential Manager API key storage
├── text_replacement_decapitalize.rs (406 l) **Decapitalize-after-edit state machine**
├── recording_auto_stop.rs         (112 l)    Silence watchdog with Arc-token race prevention
├── session_manager.rs             (255 l)    RAII RecordingSession guard + SessionState enum
├── recording_session.rs           (220 l)    Legacy recording session (transitional)
├── plus_overlay_state.rs          (884 l)    Extended overlay error states/categories
├── overlay.rs                     (2,240 l)  All overlay windows, appearance, positioning
├── input_source.rs                (225 l)    **OS keyboard-layout -> language code** (Win/Mac/Linux)
├── active_app.rs                  (35 l)     Windows foreground app name detection
├── input.rs                       (146 l)    Enigo keyboard/mouse simulation
├── clipboard.rs                   (1,150 l)  Clipboard operations, streaming paste sessions
├── llm_client.rs                  (358 l)    Multi-provider LLM client (6 providers)
├── url_security.rs                (366 l)    URL/HTTPS validation, provider preset mapping
├── webview_hardening.rs           (32 l)     Disable WebView2 browser accelerator keys
├── hotkey_guide.rs                (122 l)    Tray hotkey reference builder
├── shortcut_handy_keys.rs         (424 l)    HandyKeys backend shortcut engine
├── tray.rs                        (918 l)    System tray icon, menus, model/mic selection
├── audio_feedback.rs              (125 l)    Sound effects (start/stop/error/cancel)
└── portable.rs                    (51 l)     Portable mode detection
```

### 3.2 Frontend (`src/`) -- React + TypeScript, ~51K lines

```
src/
├── components/
│   ├── settings/
│   │   ├── speech/                Transcription profiles, STT provider, language
│   │   ├── advanced/              AI Replace, voice commands, remote STT, accelerator
│   │   ├── browser-connector/     Connector status, pairing, export
│   │   ├── live-sound-transcription/ Live sound page with diarized transcript
│   │   ├── text-replacement/      Find & replace rules (plain + regex)
│   │   ├── audio-processing/      Mic boost, noise cancellation
│   │   ├── voice-commands/        Command editor with fuzzy match test
│   │   └── debug/                 Shortcut engine, recording buffer, dictation stats
│   └── shared/                    Shared components
├── soniox-live-preview/           **Separate Tauri window** -- streaming text display,
│                                  drag grip, edge resize, final/interim text, action buttons
├── region-capture/                **Separate Tauri window** -- cross-monitor region selection
├── overlay/                       Recording overlay (bars/centerpiece/border/animated-border)
├── voice-activation-button/       Floating on-screen record button
├── command-confirm/               Voice command confirmation dialog
├── stores/
│   ├── settingsStore.ts           Main Zustand settings store
│   └── navigationStore.ts         Sidebar navigation state
└── hooks/
    ├── useSettings.ts             Settings hook
    └── useModels.ts               Model management hook
```

---

## 4. Feature Inventory

### 4.1 Streaming STT Pipeline (3 providers)

**Soniox Realtime** (`managers/soniox_realtime.rs`, 820 lines):
- WS to `wss://stt-rt.soniox.com/transcribe-websocket`
- `ActiveSession { audio_tx (mpsc 256), control_tx, final_text (Arc<Mutex>), join_handle }`
- Pending audio buffer for frames arriving before WS connects
- keepalive timer (5-20s), `tokio::select!` loop
- Token-level: `SonioxToken { text, is_final, speaker }` -> final vs interim split
- Speaker diarization: parses speaker key as string/u64/i64/f64
- `finish_session(timeout_ms)` with partial output recovery
- `restart_session()` for settings changes

**Deepgram Realtime** (`managers/deepgram_realtime.rs`, 825 lines):
- WS URL built with query params: model, encoding=linear16, sample_rate=16000, smart_format, interim_results, diarize, endpointing, language
- Auth: `Authorization: Token <key>` header
- Protocol: `{"type":"Finalize"}` -> `{"type":"CloseStream"}`
- `speech_final` flag parsed (reserved for future)
- Nova-3 model, "nova-3-medical" -> forced "en"

**OpenAI Realtime Whisper** (`managers/openai_realtime_whisper.rs`, 997 lines):
- WS to `wss://api.openai.com/v1/realtime?intent=transcription` (NOT the REST `transcription_sessions` endpoint)
- `gpt-realtime-whisper` model only
- 24kHz PCM16 (resampled from 16kHz via linear interpolation)
- Chunk size: 48,000 bytes, queue capacity 256
- Protocol: `session.update` -> base64 `input_audio_buffer.append` -> periodic `input_audio_buffer.commit`
- Per-commit tracking with `completed_item_ids` HashSet dedup
- `transcribe_flattened()` alternative for batch
- 4 unit tests

### 4.2 System-Audio Capture (WASAPI Loopback)

**`managers/live_sound_audio.rs`** (528 lines):
- Independent audio pipeline from `AudioRecordingManager` - own AudioRecorder, own realtime managers
- Global `LazyLock<Mutex<Option<LiveSoundAudioSession>>>` singleton
- Three modes: `SystemOutput` (loopback), `Microphone`, `Both`
- **Both-mode mixing**: mic drives clock via frame callback; loopback fills shared `Arc<Mutex<Vec<f32>>>` buffer (capped at 16K samples). Mic callback: `(mic[i] + loopback[i]) * 0.5` -> push to STT
- `open_mic_recorder_for_both()` re-wires loopback callback to just fill buffer
- `stop()` spawns async finalization of realtime session with session_id matching

**`managers/live_sound_transcription.rs`** (364 lines):
- State: active, recording, processing_llm, final/interim text, final/interim raw_blocks
- Auto-stop timer with per-session `session_id` race prevention
- Emits kebab_case + snake_case events for frontend compatibility

### 4.3 Transcription Profiles

`settings.rs` `TranscriptionProfile` (67 lines of struct fields):
- `id`, `name`, `language`, `translate_to_english`, `description`, `system_prompt`
- `stt_prompt_override_enabled`, `include_in_cycle`, `push_to_talk`, `preview_output_only_enabled`
- Per-profile LLM: `llm_post_process_enabled`, `llm_prompt_override`, `llm_model_override`
- Per-profile Soniox: `soniox_language_hints_strict`, `soniox_context_general_json/text/terms`
- **Cycle Profile** action: iterates profiles with `include_in_cycle=true`
- **Per-profile dedicated hotkeys**: `transcribe_{profile.id}` binding
- Auto-switch with Windows keyboard input language (`input_source.rs`)

### 4.4 AI Replace Selection

`actions.rs` `AiReplaceSelectionAction`:
- Select text -> hold AI-Replace hotkey -> speak instruction -> LLM transforms -> types/pastes
- **No-selection mode**: empty selection -> pure LLM generation
- **Quick-tap mode**: brief keypress (< threshold ms) triggers without voice
- **Restore-on-error**: saves original selection via clipboard backup
- LLM template: `${output}`, `${instruction}`, `${selection}`, `${current_app}`, `${short_prev_transcript}`, `${language}`, `${profile_name}`, `${time_local}`, `${date_iso}`, `${translate_to_english}`
- `LlmOperationTracker` (51 lines): monotonically increasing IDs, `cancel()`, `is_cancelled(id)`

### 4.5 Connector (Encrypted Local Bridge)

**`managers/connector.rs`** (2,230 lines):
- axum HTTP server on `127.0.0.1:{port}` (default 38243)
- **Protocol v3**: ECDH-P256 handshake -> HKDF-SHA256 -> AES-256-GCM encryption + HMAC-SHA256 signing
- Routes: `POST /session` (handshake), `GET /messages` (long-poll, wait=N up to 30s), `POST /messages` (ack), `GET /blob/{att_id}`
- Message queue: bounded 100, lossy (older dropped on overflow), keepalives in same queue
- Blob storage: 5-min TTL for image attachments, auto-cleanup
- Auth: `X-AivoRelay-Protocol-Version`, `X-AivoRelay-Session-Id`, `X-AivoRelay-Sequence`, `X-AivoRelay-Timestamp`, `X-AivoRelay-Request-Mac`
- Auth backoff: escalating delay (150ms->2000ms max), 5s toast cooldown
- CORS: configurable as Any or Exact origin with axum CORS layer
- Password: auto-generated, encrypted at rest (aes-gcm), rotation, pending password TTL 120s
- Extension export: unpacks bundled Chrome extension zip, generates password
- Background restart with `background_restart_in_progress` flag, >4s delay warning
- Companion repo: [AivoRelay-relay](https://github.com/MaxITService/AivoRelay-relay) Chrome extension

### 4.6 Live Preview Window

**Frontend**: `src/soniox-live-preview/SonioxLivePreview.tsx` (separate Tauri window, label `soniox_live_preview`)
- Displays final text + interim text (different colors/opacity)
- Configurable opacity, font/interim/accent colors, themes
- Positioning: Follow Cursor (with offset), Fixed Corner, Custom XY
- Drag grip (moveable), edge resize handles (8 directions, persisted geometry)
- Action buttons (optional): Clear, Flush, Process, Insert, Delete Last Word, Delete Until Dot/Comma

**Rust state**: `managers/preview_output_mode.rs` (173 lines):
- `PreviewOutputModeStatePayload { active, recording, processing_llm, is_realtime, binding_id, profile_id, error_message }`

**Streaming insert**: `soniox_stream_processor.rs` (417 lines):
- `SonioxStreamProcessor { pending_raw, stable_tail_words, fuzzy_enabled, custom_words, replacements, leading_mode }`
- `push_chunk(raw)` -> detects stable prefix (tail_words safety buffer) -> processes pipeline -> returns delta
- Stable prefix: word-boundary index N words from end (default 3)
- Pipeline: custom words fuzzy -> text replacements -> leading whitespace policy -> decapitalize

### 4.7 File Transcription + Subtitles

- `commands/file_transcription.rs` (956 lines): orchestrates audio file -> text pipeline
- Supports WAV, MP3, OGG, M4A, FLAC via `hound` + system codecs
- Output: Text, SRT, VTT (`subtitle.rs`, 178 lines)
- Chunking for long files (configurable max minutes)
- Diarization: `file_transcription_diarization.rs` (337 lines) -- speaker block normalization, artifact persistence (24h TTL), speaker name profiles

### 4.8 Voice Command Center

`commands/voice_command.rs` (307 lines):
- Pre-written PowerShell commands with fuzzy matching: exact -> Levenshtein -> Soundex -> word-similarity boost
- LLM fallback for unmatched commands
- **Confirmation-gate**: always shows dialog before execution
- Auto-run: optional countdown timer
- Execution: silent (hidden window) or windowed (visible console with -NoExit)
- PowerShell 7 (pwsh) or Windows PowerShell 5.1 selectable
- Execution policy: per-command + global defaults (Bypass/Unrestricted/RemoteSigned/Default)
- Mock test command: tests matching without executing

### 4.9 Text Output Pipeline Extras

| Feature | File(s) | Lines | Description |
|---------|---------|-------|-------------|
| **Text Replacements** | `soniox_stream_processor.rs` | -- | Find/Replace with `\n`, `\t`, `\u{xxxx}` escapes; regex with `$1` capture groups; case-insensitive toggle; applied inline during streaming + post-LLM |
| **Decapitalize After Edit** | `text_replacement_decapitalize.rs` | 406 | State machine: Idle -> Armed (timeout). Monitors Backspace/Del via passive key listener. One-shot lowercase-first-char. Post-recording monitor window for standard STT. |
| **Custom Words Fuzzy** | `audio_toolkit/text.rs` | 454 | N-gram fuzzy correction via `strsim::normalized_levenshtein`. Filler word/stutter filtering. |
| **Prompt Variables** | `actions.rs` | -- | 10 variables: `${output}`, `${instruction}`, `${selection}`, `${current_app}`, `${short_prev_transcript}`, `${language}`, `${profile_name}`, `${time_local}`, `${date_iso}`, `${translate_to_english}` |
| **Filler Word Filter** | `audio_toolkit/text.rs` | -- | Regex removal of "um", "uh", "er", word repetitions |

### 4.10 Additional Features Summary

| Feature | File | Notes |
|---------|------|-------|
| RNNoise denoise | `noise_suppression.rs` (87 l) | nnnoiseless, togglable per mic |
| Mic auto-switch | `microphone_auto_switch.rs` (167 l) | Follows Windows default device changes |
| Recording auto-stop | `recording_auto_stop.rs` (112 l) | Arc<AutoStopToken> race prevention |
| RAII session guard | `session_manager.rs` (255 l) | RecordingSession Drop cleanup |
| API key storage | `secure_keys.rs` (249 l) | Windows Credential Manager via keyring |
| URL validation | `url_security.rs` (366 l) | HTTPS enforcement, preset mapping |
| WebView2 hardening | `webview_hardening.rs` (32 l) | Disable browser accelerator keys |
| Hotkey guide | `hotkey_guide.rs` (122 l) | Categorized reference for tray |
| OS input detection | `input_source.rs` (225 l) | Keyboard layout -> language (Win/Mac/Linux) |
| Region capture | `region_capture.rs` (523 l) | Cross-monitor screenshot selection |
| Playwright CDP | `lib.rs` | PLAYWRIGHT_TAURI_REMOTE_DEBUGGING_PORT |
| Portable mode | `portable.rs` (51 l) | .portable marker file detection |
| Dictation stats | settings.rs + actions.rs | Word/character counters |
| Local preview auto-flush | actions.rs | Timer-based auto-flush, sliding LM window |

---


## 5. Key Code Patterns & Techniques

### 5.1 WebSocket Streaming Pattern (all three managers)
Shared architecture across Soniox/Deepgram/OpenAI:
 + "" + @"`
ActiveSession { binding_id, audio_tx (mpsc 256), control_tx, final_text (Arc<Mutex>), join_handle }
 + "" + @"`
- parking_lot::Mutex<Option<ActiveSession>> for single-session enforcement
- pending_audio: Mutex<Vec<Vec<u8>>> buffers frames arriving before WS connects
- 	okio::select! juggles: control commands, audio frames, WS reads, keepalive timer
- Lifecycle: start_session() / inish_session(timeout_ms) / cancel() / estart_session()
- Error recovery: partial text returned on timeout, per-binding error routing

### 5.2 RAII Session Guard (session_manager.rs)
 + "" + @"`rust
RecordingSession { app, cancel_shortcut_registered: AtomicBool, mute_applied: AtomicBool, cleaned_up: AtomicBool }
// Drop: unregisters cancel shortcut, unmutes, hides overlay, resets tray icon
// finish(): marks cleaned_up so Drop becomes no-op
 + "" + @"`
- SessionState enum: Idle | Recording { session, binding_id, captured_settings } | Processing { binding_id }
- Captures full AppSettings snapshot at recording start to prevent mid-recording drift

### 5.3 Stable Prefix / Safety Buffer (soniox_stream_processor.rs)
 + "" + @"`
push_chunk(raw) -> appends pending_raw -> stable_prefix_end(text, 3) -> split stable/unsafe -> process stable -> return delta
 + "" + @"`
Word-boundary detection keeps last N words as safety buffer before committing text.

### 5.4 Connector Protocol v3 Crypto Flow
 + "" + @"`
POST /session:
  Client -> server: P-256 public key, client_nonce, timestamp, HMAC proof
  Server -> client: server P-256 pubkey, server_nonce, server_proof
  ECDH shared secret -> HKDF-SHA256 -> enc_key (AES-256-GCM) + mac_key (HMAC-SHA256)
Subsequent requests: session ID, strictly increasing sequence, timestamp, HMAC
Response: AES-256-GCM encrypted body (random 12-byte nonce per message)
 + "" + @"`

### 5.5 Both-Mode Audio Mixing (live_sound_audio.rs)
 + "" + @"`
Mic callback (clock driver): frame -> lock loopback_buf -> drain N samples -> (mic[i] + loopback[i]) * 0.5 -> push STT
Loopback callback: just append to loopback_buf (capped 16K samples / 1 second)
 + "" + @"`
Problem solved: WASAPI loopback produces no callbacks when speakers are silent, so mic drives the clock.

### 5.6 Decapitalize State Machine (	ext_replacement_decapitalize.rs)
 + "" + @"`
Idle ----(Backspace/Del pressed)----> Armed (timeout window, one-shot)
Armed ----(next text chunk)---------> lowercases first char -> consume -> Idle
Armed ----(expired)-----------------> Idle
Post-recording monitor: standard STT path arms standard_output_armed for final chunk
 + "" + @"`

### 5.7 Auto-Stop Token Race Prevention (ecording_auto_stop.rs)
 + "" + @"`rust
Arc<AutoStopToken { notify }> in Mutex<Option<Arc<AutoStopToken>>>
On fire: take() token, Arc::ptr_eq() verify still active -> execute or silently abort
 + "" + @"`

### 5.8 LLM Template Variables (ctions.rs)
10 variables: ${output}, ${instruction}, ${selection}, ${current_app} (Windows-only), ${short_prev_transcript} (per-app cache, configurable word count + expiry), ${language}, ${profile_name}, ${time_local}, ${date_iso}, ${translate_to_english}

### 5.9 OS Input Source -> Language (input_source.rs)
- Windows: GetKeyboardLayout(thread_id) -> KLID hex -> lookup (30+ layouts)
- macOS: defaults read com.apple.HIToolbox -> KeyboardLayout Name -> lookup
- Linux: setxkbmap -query -> layout -> LANG fallback

---

## 6. Diff Analysis vs Parent (Handy)

### 6.1 Inherited from Handy
Tauri 2.10.2 framework, local Whisper (transcribe-rs), Audio Recording Manager, VAD (Silero), shortcut engines (Tauri+rdev), system tray, overlay system, LLM client, SQLite history, settings store, clipboard, i18n -- all preserved and heavily extended.

### 6.2 AIVORelay-Added Modules (divergent code)

| New module | Lines | Category |
|-----------|-------|----------|
| managers/soniox_realtime.rs | 820 | Streaming STT (WS) |
| managers/deepgram_realtime.rs | 825 | Streaming STT (WS) |
| managers/openai_realtime_whisper.rs | 997 | Streaming STT (WS) |
| managers/soniox_stt.rs | 982 | Batch cloud STT |
| managers/deepgram_stt.rs | 795 | Batch cloud STT |
| managers/remote_stt.rs | 1,356 | OpenAI-compatible STT |
| managers/connector.rs | 2,230 | Encrypted bridge (ECDH+AES) |
| managers/live_sound_audio.rs | 528 | WASAPI loopback + Both-mode |
| managers/live_sound_transcription.rs | 364 | Live sound state |
| managers/preview_output_mode.rs | 173 | Preview routing state |
| soniox_stream_processor.rs | 417 | Streaming chunk assembler |
| ile_transcription_diarization.rs | 337 | Speaker diarization |
| subtitle.rs | 178 | SRT/VTT formatting |
| language_resolver.rs | 269 | Soniox language validation |
| 	ext_replacement_decapitalize.rs | 406 | Decapitalize trigger |
| 	ranscript_context.rs | 94 | Per-app transcript cache |
| input_source.rs | 225 | OS keyboard layout -> lang |
| egion_capture.rs | 523 | Region screenshot capture |
| secure_keys.rs | 249 | API key vault (Win Cred Mgr) |
| url_security.rs | 366 | URL/HTTPS validation |
| ecording_auto_stop.rs | 112 | Silence timeout watchdog |
| session_manager.rs | 255 | RAII session guard |
| commands/voice_command.rs | 307 | PowerShell executor |
| commands/file_transcription.rs | 956 | File -> text/SRT/VTT |

### 6.3 Massive expansions to inherited files

| Inherited file | Handy approx | AIVORelay | Delta |
|---------------|-------------|-----------|-------|
| ctions.rs | ~2,000 | 7,492 | +5,492 |
| settings.rs | ~500 | 4,097 | +3,597 |
| shortcut.rs | ~1,000 | 5,269 | +4,269 |
| overlay.rs | ~500 | 2,240 | +1,740 |
| clipboard.rs | ~300 | 1,150 | +850 |
| udio.rs (managers) | ~400 | 980 | +580 |
| lib.rs | ~400 | 1,214 | +814 |

---

## 7. Relation to S2B2S

| Aspect | AIVORelay | S2B2S | Verdict |
|--------|-----------|-------|---------|
| Streaming STT | 3 WS providers, interim tokens | None | **AIVORelay wins** |
| TTS pipeline | None | 8 backends, gapless | **S2B2S wins** |
| Brain / conversation | Simple LLM post-processing | SSE streaming, sentence splitter, barge-in | **S2B2S wins** |
| System audio capture | WASAPI loopback + Both-mode | None | **AIVORelay wins** |
| Transcription profiles | Per-mode language/prompt/model | None | **AIVORelay wins** |
| AI Replace Selection | Full flow + quick-tap | None | **AIVORelay wins** |
| Voice commands | PowerShell + fuzzy match + LLM | None | **AIVORelay wins** |
| Secure connector | ECDH+HKDF+AES-GCM protocol v3 | Simple axum HTTP | **AIVORelay wins** |
| Live Preview HUD | Always-on-top streaming | None | **AIVORelay wins** |
| Decapitalize-after-edit | State machine + passive listener | None | **AIVORelay wins** |
| Prompt variables | 10 template vars | None | **AIVORelay wins** |
| Text replacements | Regex + plain, streaming | None | **AIVORelay wins** |
| Cross-platform | **Windows only** | Win/Mac/Linux | **S2B2S wins** |
| Model management | transcribe-rs + DirectML/Vulkan | Same engine | Tie |
| File transcription | SRT/VTT + diarization | None | **AIVORelay wins** |

**Overall**: Complementary strengths. AIVORelay is a dictation/input powerhouse with cloud streaming and voice actions. S2B2S is a conversation platform with full duplex STT->Brain->TTS. The ideal synthesis would cherry-pick AIVORelay's input-side innovations into S2B2S's conversation framework.

---

## 8. Harvest List (Features Worth Copying)

| Feature to harvest | From file | Effort | Why valuable for S2B2S |
|---|---|---|---|
| **Streaming STT WS managers** (architecture) | soniox_realtime.rs, deepgram_realtime.rs, openai_realtime_whisper.rs | L | Cloud streaming STT leg; bounded queues, interim/final split, reconnect, partial recovery |
| **Transcription profiles** (per-mode presets) | settings.rs + ctions.rs | L | S2B2S "persona/mode" system: casual chat vs translator vs coding copilot |
| **Live Preview window** | preview_output_mode.rs + src/soniox-live-preview/ | M | Seed of S2B2S conversation transcript HUD; already has interim/final display, themes |
| **AI Replace Selection** flow | ctions.rs AiReplaceSelectionAction | M | Voice-instructed text editing = killer Brain feature |
| **Connector crypto protocol v3** | connector.rs (ECDH+HKDF+AES-GCM) | M | Production-grade local bridge auth for S2B2S control server |
| **WASAPI loopback + Both-mode** | live_sound_audio.rs | M | Let S2B2S hear system audio for podcast/meeting conversation |
| **SonioxStreamProcessor** (stable prefix + pipeline) | soniox_stream_processor.rs | S | Reusable streaming chunk assembly pattern |
| **Decapitalize-after-edit** | 	ext_replacement_decapitalize.rs | S | Dictation polish: fixes mid-sentence capitalization |
| **Prompt template variables** | ctions.rs LlmTemplateContext + 	ranscript_context.rs | S | Context-aware Brain prompts with ,  |
| **Regex text replacement** (streaming + post-LLM) | soniox_stream_processor.rs StreamChunkReplacementEngine | S | Deterministic final-output control |
| **Recording auto-stop** (Arc-token race prevention) | ecording_auto_stop.rs | S | Clean silence-based auto-stop pattern |
| **RAII RecordingSession guard** | session_manager.rs | S | Guarantees cleanup on any exit path |
| **OS input source detection** | input_source.rs | S | Cross-platform keyboard layout -> language for auto-switching |
| **File -> SRT/VTT + diarization** | subtitle.rs + ile_transcription_diarization.rs | M | Batch utility mode |
| **Voice command confirmation gate** | oice_command.rs + command-confirm/ | M | Action layer pattern: voice -> match -> confirm -> execute |
| **Recording session settings snapshot** | session_manager.rs captured_settings | S | Prevents mid-recording settings drift |
| **WebView2 hardening** | webview_hardening.rs | XS | Disable browser accelerator keys in production |
## 9. Known Issues, Caveats & Limitations

| Issue | Severity | Impact |
|-------|----------|--------|
| **Windows-only** for all new features | High | WASAPI, PowerShell, region capture, WebView2, keyring -- all gated. Non-Windows builds return errors. |
| **841 commits of divergence** from old fork point | High | File layout, crate name (`aivorelay_app_lib`), schema, naming have drifted. Code cannot be merged verbatim. |
| **Cloud-dependent features** | Medium | Streaming STT, diarization, LLM post-processing require paid API keys. |
| **actions.rs is 7,492 lines** | High | God object: all shortcut actions, transcription orchestration, live streaming, post-processing in one file. Do NOT replicate. |
| **shortcut.rs is 5,269 lines** | High | 250+ `change_*_setting` commands. S2B2S's batch-settings pattern is cleaner. |
| **No TTS subsystem** | Medium | Purely STT->text. No voice output for conversation loop. |
| **Three realtime managers near-identical** | Medium | 80%+ structural similarity. Generic `RealtimeSttManager<S>` trait would eliminate ~1,500 lines. |
| **Voice commands are PowerShell-only** | Medium | Confirmation-gate pattern is good, but execution is hardcoded. Should generalize to arbitrary actions. |
| **connector.rs is 2,230 lines** | Medium | Could be split: `server.rs`, `crypto.rs`, `handlers.rs`. |
| **Both-mode mixing fixed *0.5** | Low | Equal power assumes both sources equally loud. Adequate for STT which normalizes internally. |

---

## 10. Strengths & Weaknesses

### Strengths (8 key ones)

1. **Production-grade streaming STT** -- Three WS-based providers with bounded queues, pending buffering, keepalive, partial recovery. Shipped on Microsoft Store.
2. **Connector security** -- Protocol v3 with ECDH-P256 + HKDF + AES-256-GCM + HMAC is the gold standard among all analyzed projects.
3. **Feature density** -- 8 major feature clusters beyond Handy, all integrated. Shipped product, not prototype.
4. **RAII + session guards** -- RecordingSession with Drop cleanup, SessionState preventing concurrent recordings, settings snapshot capture.
5. **SonioxStreamProcessor** -- Elegant stable-prefix approach to streaming chunk assembly.
6. **Multi-branch CI/CD** -- 4 active branches, MS Store packaging, certificate signing, Playwright test infra.
7. **Comprehensive settings** -- 250+ changeable settings covering every feature.
8. **Honest documentation** -- Voice commands labeled "Dangerous!", platform limits stated explicitly.

### Weaknesses (7 key ones)

1. **Windows-only** -- Every new feature is `#[cfg(windows)]` gated. S2B2S cross-platform mandate incompatible.
2. **actions.rs monolith** -- 7,492 lines single file. Hard to navigate, test, extend.
3. **No TTS / conversation loop** -- Pure STT->text. S2B2S core value proposition absent.
4. **Redundant realtime managers** -- ~1,500 lines of duplication across 3 WS managers.
5. **Shortcut command explosion** -- 250+ individual change commands vs batch pattern.
6. **Cloud dependency** -- Key features (streaming STT, diarization) require paid APIs.
7. **Connector complexity** -- Heavy for what it does (message relay). Simpler alternatives exist.

---

## 11. Bottom Line / Verdict

**AIVORelay is the most divergent and feature-rich Handy fork, representing ~100K lines and 841 commits of engineering.** Its streaming STT implementations are production-grade reference patterns. The connector's ECDH-encrypted local bridge is the best among all analyzed projects. Transcription profiles, AI Replace Selection, and the Live Preview window are features S2B2S needs but lacks.

**However**, the Windows-only nature, actions.rs monolith, lack of TTS/Brain, and extreme divergence from Handy make direct code reuse impractical. The correct approach is **selective concept and pattern harvesting**: copy the streaming STT architecture (wrap in generic trait), adopt the profile/mode system (design for cross-platform), port the connector crypto (trim to needs), and study the decapitalize/auto-stop/session-guard patterns. The single most valuable idea: the **SonioxStreamProcessor stable-prefix streaming pipeline** (417 lines, self-contained) solves the hardest problem in live speech-to-text -- when to commit to text that won't be contradicted by later tokens.
