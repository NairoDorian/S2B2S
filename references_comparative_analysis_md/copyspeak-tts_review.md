# CopySpeak TTS -- Comprehensive Analysis

> **Repo:** `NairoDorian/copyspeak-tts` (fork of `ilyaizen/copyspeak-tts`) · Branch: `tts-perf-v2` · HEAD: `78531eb` · Version: **0.1.5** · License: **MIT** · Author: **ilyaizen** (upstream) / **NairoDorian** (fork) · Platforms: **Windows-only**
> **Nature:** fork of `ilyaizen/CopySpeak` -- independent TTS app, 21 commits ahead of upstream (+4,092/-3,280 lines over 67 files). Fork theme: **TTS performance** -- persistent server architecture, RAM/VRAM model residency, telemetry-driven adaptive pagination, zero-gap streaming playback.
> **Role for S2B2S:** **Feature donor.** CopySpeak's `TtsBackend` trait, persistent Piper server pattern, telemetry system, and clipboard-trigger UX are the direct ancestors of S2B2S's TTS subsystem. S2B2S copied and extended these patterns. The self-review ledger in this repo (R1-R7, H1-H7) is a **port-time checklist** for S2B2S.

---

## 1. What CopySpeak TTS Is

CopySpeak TTS is a Windows desktop application that converts text to speech using multiple TTS engines. Its signature interaction is **double-copy**: copy the same text twice within 1.5 seconds, and it's read aloud. No manual pasting, no hotkey presses required (though a global hotkey is also available).

The app supports **7 TTS engines** spanning local and cloud: **Kitten** (lightweight ONNX CPU, 8 built-in voices, default), **Piper** (20+ EN-US voices with CUDA acceleration), **Kokoro** (11 ONNX voices), **Pocket** (8+ voices with cloning), plus **OpenAI**, **ElevenLabs**, and **Cartesia** (cloud). All four local engines use **persistent HTTP servers** with models kept resident in RAM/VRAM between utterances, achieving sub-second warm synthesis after initial pre-warm.

The fork (`tts-perf-v2`) dramatically upgraded Piper from CLI-per-invocation (~1.6 s load each time) to a persistent `piper.http_server` child process with a state machine, exponential-backoff health polling, CUDA/CPU pre-warming with hidden warm-up synthesis, and a generation-counter guard against stale starts. It also added telemetry-driven progress estimates, adaptive pagination, zero-gap fragment streaming, an HTTP control API, and extensive robustness improvements (mutex-poisoning recovery, WAV bounds hardening, Unicode-safe pagination).
## 2. Tech Stack

### 2.1 Frontend
| Layer | Choice | Purpose |
|-------|--------|---------|
| Framework | **Svelte 5 (runes)** + **SvelteKit 2** | Reactive UI with `$state`, `$derived`, `$effect` |
| UI Kit | **shadcn-svelte** + **bits-ui** | Component library |
| Styling | **Tailwind CSS 4.3** with brutalist design | Dark/light themes |
| Build | **Vite 8**, **Bun 1.3** | Dev server and bundling |
| Testing | **Vitest 4**, Playwright, jsdom | Frontend tests |
| State | Svelte-runes stores: `playback-store`, `synthesis-store`, `hud-store`, `history-store`, `listening-store`, `piper-store` | Reactive state management |
| i18n | `svelte-i18n` | EN/ES full translations |
| Audio | Web Audio API (`AudioContext`, `decodeAudioData`) | Playback, pitch, effects, pre-decode |
| Auth | `@tauri-apps/api` | IPC invoke/listen/emit |

### 2.2 Backend / Core (Rust, Tauri 2)
| Concern | Crate/Tech | Purpose |
|---------|-----------|---------|
| Shell | **Tauri 2** + plugins: dialog, opener, process, single-instance, updater, global-shortcut | Desktop shell, IPC, tray |
| Audio | **rodio 0.19** (`Sink` + `OutputStream`) | Playback only -- no cpal, no recording |
| Async | **tokio** (process, fs, sync, time, macros) | Async runtime |
| HTTP | **reqwest 0.13** (blocking + json) | Cloud TTS, Piper/Kokoro/Kitten/Pocket HTTP |
| OS | **windows 0.62** (Win32 clipboard, GDI, input), **winreg** | Windows-native features |
| Logging | **flexi_logger** (rotating, compressed) | Structured logging |
| CSPRNG | **getrandom 0.2** | Control server token generation |
| Text | **regex**, base64, chrono | Sanitization, encoding, timestamps |
| Error | **thiserror**, **dirs 6**, tempfile, lazy_static, serde/serde_json | Error types, paths, temp files, JSON |

### 2.3 Key Dependencies (Non-Obvious)
| Dependency | Why Notable |
|------------|-------------|
| `windows 0.62` | Raw Win32: `AddClipboardFormatListener`, `GetClipboardData`, `GlobalLock`, `CreateWindowExW` with message-only windows, `SetClipboardData`, `CREATE_NO_WINDOW` process flags |
| `rodio 0.19` (not 0.22) | Older rodio; CopySpeak wraps it in a dedicated audio thread with channel-based commands (Stop, TogglePause, SetMode, SetVolume, SeekRelative) |
| `reqwest 0.13` blocking | Most backends use blocking reqwest from `spawn_blocking` -- the Piper server client is a single pooled instance reused across all synthesis calls |
| `piper.http_server` (Python Flask) | External Python HTTP server -- not bundled, installed by user via `pip install piper-tts[http]` |
| `kittentts-cli.py`, `kokoro_server.py`, `kitten_server.py`, `pocket_server.py` | Python server scripts shipped in repo (`src-tauri/` bundle resources) |

---

## 3. Architecture & Source Map

```
copyspeak-tts/
├── src/                                # Svelte 5 frontend (SvelteKit)
│   ├── lib/
│   │   ├── components/
│   │   │   ├── engine/                 # Per-engine UI panels
│   │   │   │   ├── engine-page.svelte     # Engine selector + settings
│   │   │   │   ├── local-engine.svelte    # Local engines (Piper/Kokoro/Kitten/Pocket)
│   │   │   │   ├── openai-engine.svelte   # OpenAI TTS settings
│   │   │   │   ├── elevenlabs-engine.svelte  # ElevenLabs settings
│   │   │   │   ├── cartesia-engine.svelte # Cartesia settings
│   │   │   │   ├── engine-page.test.ts    # 4/4 preserved
│   │   │   │   ├── local-engine.test.ts   # 18/18 preserved
│   │   │   │   ├── openai-engine.test.ts  # !! Gutted: 15->1 cases
│   │   │   │   └── elevenlabs-engine.test.ts  # !! Gutted: 15->1 cases
│   │   │   ├── layout/
│   │   │   │   ├── app-header.svelte
│   │   │   │   └── app-footer.svelte   # Piper status indicator (spinner/VRAM/green dot)
│   │   │   ├── settings/               # 12 settings sub-panels
│   │   │   │   ├── general-settings, playback-settings, pagination-settings
│   │   │   │   ├── hotkey-settings, history-settings, effects-settings
│   │   │   │   ├── sanitization-settings, post-process-settings
│   │   │   │   ├── batch-settings, appearance-settings, about-settings
│   │   │   │   └── import-export-settings
│   │   │   ├── history/                # History with virtual-list, bulk actions, export
│   │   │   ├── hud/                    # HUD overlay components
│   │   │   │   ├── hud-status.svelte, hud-playback-content.svelte
│   │   │   │   ├── hud-synthesis-progress.svelte, clipboard-notification.svelte
│   │   │   ├── landing/                # Onboarding/landing page
│   │   │   ├── ui/                     # shadcn-svelte UI primitives
│   │   │   ├── global-player.svelte    # Persistent <audio> element across all routes
│   │   │   ├── hud-overlay.svelte      # Transparent always-on-top HUD window
│   │   │   ├── playback-controls.svelte
│   │   │   ├── waveform.svelte         # Live amplitude waveform from envelope
│   │   │   ├── virtual-list.svelte     # Virtual scrolling for history
│   │   │   ├── hotkey-capture.svelte
│   │   │   └── quick-settings.svelte
│   │   ├── stores/                     # Svelte-runes reactive stores
│   │   │   ├── playback-store.svelte.ts  # ~471 lines -- Audio playback state machine
│   │   │   ├── synthesis-store.svelte.ts # Synthesis state flag
│   │   │   ├── hud-store.svelte.ts     # HUD state
│   │   │   ├── history-store.svelte.ts # History entries + operations
│   │   │   └── piper-store.svelte.ts   # Piper server status
│   │   ├── services/tauri.ts           # TauriService singleton wrapping invoke/listen
│   │   ├── models/                     # History, HTML export templates
│   │   ├── i18n/                       # EN/ES, store, types
│   │   ├── mocks/                      # Test mocks
│   │   ├── utils/                      # Timer, text, HTML export, history events
│   │   ├── types.ts                    # Shared TypeScript types
│   │   └── utils.ts                    # cn() utility
│   ├── routes/
│   │   ├── +layout.svelte              # Root layout (theme, font)
│   │   ├── +page.svelte                # Landing/play page
│   │   ├── settings/, engine/, history/, hud/, effects/, onboarding/
│   └── test-setup.ts
│
├── src-tauri/src/                      # Rust backend
│   ├── main.rs (861 lines)             # Entry point, tray, hotkey, clipboard watcher spawn, pre-warm, abort logic, lock_or_recover! macro
│   │
│   ├── tts/                            # TTS backend abstraction (THE engine layer)
│   │   ├── mod.rs (72 lines)           # trait TtsBackend { synthesize, health_check, file_extension, voice_display_name } + TtsError + Voice
│   │   ├── cli.rs (1171 lines)         # CliTtsBackend -- CLI invocation (Kitten/Kokoro/Piper), server routing, voice discovery, path expansion, pre-warm/restart delegates
│   │   ├── piper_server.rs (498 lines) # Persistent Piper HTTP server lifecycle: state machine, generation counter, ensure_running, pre-warm, CUDA DLL discovery, health polling
│   │   ├── local_tts_server.rs (578 lines)  # Generic persistent server for Kokoro/Kitten/Pocket: same arch as piper_server but multi-engine via EngineSlot statics
│   │   ├── openai.rs (155 lines)       # OpenAI TTS cloud backend
│   │   ├── elevenlabs.rs (684 lines)   # ElevenLabs: voice library listing, MP3 native
│   │   └── cartesia.rs (149 lines)     # Cartesia Sonic cloud backend
│   │
│   ├── clipboard.rs (532 lines)        # Win32 AddClipboardFormatListener: message-only window, WM_CLIPBOARDUPDATE pump, double-copy state machine (50ms debounce + 1.5s window), text truncation, native dispatch
│   │
│   ├── audio/                          # Audio subsystem (playback only -- no recording)
│   │   ├── mod.rs (25 lines)           # Re-exports + AmplitudeEnvelope struct
│   │   ├── player.rs (274 lines)       # rodio Sink on dedicated thread: interrupt/queue modes, pause, seek, volume, 200ms Windows preroll
│   │   ├── wav.rs                      # WAV parsing: duration, envelope extraction, concat (with data-size clamping)
│   │   └── format.rs                   # WAV->MP3/OGG/FLAC conversion
│   │
│   ├── commands/                       # Tauri IPC command handlers
│   │   ├── mod.rs                      # Command module + event types (AudioFragmentEvent, PaginationEvent)
│   │   ├── config.rs                   # get/set/reset/validate config
│   │   ├── tts/
│   │   │   ├── synthesis.rs (1371 lines) # speak_now, speak_queued -- main synthesis orchestration: guard lock, sanitize, paginate, synthesize, emit, history, telemetry
│   │   │   ├── voices.rs               # Voice discovery + dynamic Piper .onnx scanning
│   │   │   ├── health.rs               # Engine health checks
│   │   │   ├── credentials.rs          # API key checks
│   │   │   ├── helpers.rs              # create_backend(), engine_identifier(), SynthesisGuard
│   │   │   └── selection.rs            # Voice selection helpers
│   │   ├── history.rs                  # ~30 commands: CRUD, search, export, cleanup, batch ops, file tracking
│   │   ├── playback.rs                 # Playback control commands
│   │   ├── queue.rs                    # Fragment queue commands
│   │   ├── post_process.rs             # Groq LLM pre-rewrite (not for S2B2S)
│   │   ├── install.rs                  # Kittentts installer
│   │   ├── update.rs                   # Update check trigger
│   │   └── logging.rs                  # Debug mode, log access
│   │
│   ├── config/                         # Typed settings (persisted as JSON)
│   │   ├── mod.rs (319 lines)          # AppConfig root + validation + save/load + minute counter
│   │   ├── general.rs, trigger.rs, tts.rs, playback.rs, output.rs
│   │   ├── sanitization.rs, pagination.rs, hud.rs, effects.rs, hotkey.rs, post_process.rs
│   │   └── tests.rs                    # Config validation unit tests
│   │
│   ├── sanitize/                       # Text cleaning pipeline
│   │   ├── mod.rs (39 lines)           # Orchestrator: markdown strip -> TTS normalize -> cleanup
│   │   ├── markdown.rs                 # Regex-based markdown stripping
│   │   ├── cleanup.rs                  # Spacing/punctuation artifact cleanup
│   │   └── tts_normalize.rs            # Legacy TTS normalization rules
│   │
│   ├── pagination.rs (1060 lines)      # Sentence-boundary splitting, CJK/Unicode-safe, abbreviation detection, force_split, adaptive sizing, 51+ tests
│   ├── fragment_queue.rs (226 lines)   # Ordered fragment queue: Idle/Playing/Paused/Stopped, atomic stop flag
│   ├── telemetry.rs (370 lines)        # EMA-based per-engine/voice/bucket timing stats, chars_per_ms, deferred saves (every 10 samples)
│   ├── control_server.rs (286 lines)   # Localhost HTTP :43117: GET /health(unauthenticated), GET /piper-status, POST /speak; bearer token auth, 200KB cap
│   ├── hud.rs (450 lines)              # HUD window: show/hide, positioning (6 presets), synthesis progress, clipboard notification
│   ├── history.rs (897 lines)          # Circular buffer (MAX 1000), serialization, cleanup service, file tracking
│   ├── post_process/mod.rs (161 lines) # Groq chat-completions pre-TTS rewrite (pooled client, best-effort fallback)
│   ├── logging.rs                      # Flexi_logger init, debug mode
│   └── autostart.rs                    # Winreg autostart registration
│
├── agent-harness/                      # pip-installable Python CLI + REPL
│   ├── setup.py                        # Package: cli-anything-copyspeak v0.1.0
│   ├── COPYSPEAK.md                    # SOP for agents
│   └── cli_anything/copyspeak/
│       ├── copyspeak_cli.py            # CLI: project new/info, queue add/list, export, backend check/launch
│       ├── core/                       # project.py, queue.py, export.py, session.py
│       ├── utils/                      # copyspeak_backend.py (subprocess synthesis), repl_skin.py
│       ├── tests/                      # test_full_e2e.py, test_core.py
│       └── skills/SKILL.md             # Agent skill definition (for AI coding agents)
│
├── scripts/                            # setup-piper-cpu.ps1, setup-piper-cuda.ps1, test-piper-perf.ps1
├── docs_internal/                      # Architecture, development guide, TTS backends, brutalist design, roadmap
├── docs/                               # CONTRIBUTING.md
├── skills/                             # Copied agent skill
└── copyspeak-tts-fork-v3-deep-review-and-plan.md  # Self-review ledger (R1-R7, H1-H7, M1-M12)
```

### Total source lines (approximate)
| Component | Files | Lines |
|-----------|-------|-------|
| `src-tauri/src/tts/` | 6 | ~3,130 |
| `src-tauri/src/commands/tts/synthesis.rs` | 1 | 1,371 |
| `src-tauri/src/main.rs` | 1 | 861 |
| `src-tauri/src/history.rs` | 1 | 897 |
| `src-tauri/src/pagination.rs` | 1 | 1,060 |
| `src-tauri/src/clipboard.rs` | 1 | 532 |
| `src-tauri/src/tts/cli.rs` | 1 | 1,171 |
| `src-tauri/src/tts/piper_server.rs` | 1 | 498 |
| `src-tauri/src/tts/local_tts_server.rs` | 1 | 578 |
| `src-tauri/src/hud.rs` | 1 | 450 |
| All Rust | 43+ | ~12,500 |
| Frontend TS/Svelte | 80+ | ~8,000+ |

---

## 4. Feature Inventory

### 4.1 TTS Engines / Backends (7 backends behind `TtsBackend` trait)

| Engine | Type | Implementation | Voices | Key Detail |
|--------|------|----------------|--------|------------|
| **Kitten** (default) | Local ONNX | `kittentts-cli.py` via `local_tts_server.rs` persistent HTTP server | 8 built-in | Ultra-lightweight CPU-optimized, model stays in RAM |
| **Piper** | Local ONNX | `piper.http_server` via `piper_server.rs` persistent HTTP server | 20+ EN-US | CUDA support, pre-warm at startup, dynamic voice discovery scanning `piper-voices/` for `.onnx` files |
| **Kokoro** | Local ONNX | `kokoro_server.py` via `local_tts_server.rs` persistent HTTP server | 11 voices | Shared `EngineSlot` architecture with Kitten/Pocket |
| **Pocket** | Local | `pocket_server.py` via `local_tts_server.rs` persistent HTTP server | 8+ voices | Voice cloning support |
| **OpenAI** | Cloud | `openai.rs` -- POST `/v1/audio/speech` | 9 voices | Precomputed Bearer header, pooled reqwest client |
| **ElevenLabs** | Cloud | `elevenlabs.rs` -- voice library API + TTS | Dynamic | MP3 native (`file_extension()` override), cached voice name |
| **Cartesia** | Cloud | `cartesia.rs` -- Cartesia Sonic API | Multiple | Low-latency streaming, added by fork v0.1.1 |

### 4.2 Persistent Server Architecture (fork centerpiece)

The fork replaced Piper's CLI-per-invocation model (~1.6s load each call) with a persistent child process:

- **State machine:** `ServerState::Stopped -> Starting{generation, config, stderr_tail} -> Ready(Arc<ActiveServer>)` (`piper_server.rs` 498 lines)
- **Generation counter:** `AtomicU64 CURRENT_GENERATION` guards against stale starters clobbering newer ones
- **Pre-warm at app startup:** Background thread starts server + runs hidden warm-up synthesis ("Hello" on CPU, longer sentence on CUDA) to force ONNX Runtime JIT/GPU-kernel init
- **Health polling:** Exponential backoff 100->1600 ms, `GET /voices` for 2xx, 60s CUDA / 15s CPU budget, premature-exit detection with stderr tail surfacing
- **CUDA:** `get_nvidia_dll_paths()` queries Python once (`OnceLock` cached) for NVIDIA runtime paths (cublas/cudnn/etc.) and injects them into the child's `PATH`
- **Config-change restart:** Keyed on command+voice+cuda+preset+backend; `PIPER_WARMING` atomic + `ClearWarming` RAII guard
- **Lifecycle events:** `piper-status-changed` events (loading -> warming_up -> ready -> error -> stopped) -> footer indicator
- **Stdout/stderr drainage:** Dedicated threads prevent pipe-buffer freezes; stderr kept in a 30-line ring buffer surfaced in error messages
- **Generalized:** Same pattern adapted into `local_tts_server.rs` for Kokoro, Kitten, and Pocket using per-engine `EngineSlot` statics

### 4.3 Pagination (Long-Text Splitting) -- `pagination.rs` 1060 lines

- **Sentence-boundary detection:** Splits at `. ! ? . ! ?` (ASCII + CJK fullwidth)
- **Abbreviation awareness:** Detects `Mr.`, `Mrs.`, `Dr.`, `e.g.`, `i.e.`, `etc.`, `vs.`, and 40+ more -- does not split mid-abbreviation
- **Unicode-safe:** Full rewrite using `char::len_utf8` and `char_indices` -- no raw byte slicing panics on multi-byte characters (CJK, curly quotes, Spanish accented, emoji, Devanagari, 4-byte scalars)
- **Force-split fallback:** When no sentence boundaries exist, splits at exact character positions (char-safe)
- **Adaptive fragment sizing:** `adaptive_fragment_size()` -- with >=3 telemetry samples, scales fragments: fast engines (>1.0 chars/ms) 3x capped at 2000, moderate (0.3-1.0) 2x capped at 1500, slow/unknown default
- **Empty fragment guard (H1 fix):** `fragments.retain(|f| !f.text.trim().is_empty())`
- **51 embedded tests** + 11 adversarial regression tests at sizes 1-500 with ZWJ emoji, combining marks, fullwidth delimiters

### 4.4 Triggers
| Trigger | Mechanism | File |
|---------|-----------|------|
| **Double-copy** | Win32 `AddClipboardFormatListener` on message-only window; 50ms debounce + configurable `double_copy_window_ms` (default 1.5s); same text twice -> native `speak_queued` dispatch (skips webview round-trip) | `clipboard.rs` |
| **Global hotkey** | `tauri-plugin-global-shortcut`; configurable combo via `hotkey-capture.svelte`; routes long texts to `speak_queued` for streaming playback (H7 fixed) | `main.rs` `spawn_speak` |
| **Manual** | Paste/play in Play page; re-speak uses current config voice | frontend invoke |
| **Tray "Speak Clipboard Now"** | Same path as hotkey -> `spawn_speak` | `main.rs` |
| **Control server** | External `POST /speak {text, engine?, effect?}` | `control_server.rs` |
| **History replay** | Click entry -> `replay_cached` or `speak_history_entry` | `synthesis.rs` |

### 4.5 Playback -- `audio/player.rs` 274 lines
- **rodio `Sink` + `OutputStream`** on dedicated audio thread with channel-based commands
- **200ms near-silent preroll on Windows** -- prevents output-device wake-up clipping first phonemes
- **Interrupt vs Queue modes:** `RetriggerMode::Interrupt` stops current; `Queue` enqueues
- **Speed 0.25x-4x, pitch 0.5x-2x, volume 0-100%**, pause/resume with position tracking, relative seek
- **Streaming fragment playback:** Frontend pre-decodes next fragment's base64->AudioBuffer while current plays; `AudioContext` pre-warmed at startup (50-200ms savings)

### 4.6 HUD (Heads-Up Display) -- `hud.rs` 450 lines
- **Floating always-on-top transparent window** (Tauri "hud" label, 300x140, decorations:false)
- **6 preset positions:** TopLeft/Center/Right, BottomLeft/Center/Right
- **States:** Clipboard copied (countdown timer), Synthesizing (progress bar with telemetry ETA + confidence), Playing (live waveform from AmplitudeEnvelope, provider/voice display)
- **Click-through** via `set_ignore_cursor_events(true)`, multi-monitor aware

### 4.7 History -- `history.rs` 897 lines
- **Circular buffer** (MAX 1000) in JSON with rich per-entry metadata
- **Audio caching:** Scans history newest-first for matching `(text, voice, engine)` with existing audio file -> replay without re-synthesis
- **Bulk operations:** Multi-select checkboxes, Select All, Export Selected, Delete Selected, Clear All
- **Virtual-list rendering**, file tracking (tracked/orphaned/missing), cleanup service, HTML export
- **~30 Tauri commands** for history operations

### 4.8 Control Server -- `control_server.rs` 286 lines
- **Localhost HTTP API** on `:43117` (overridable via `COPYSPEAK_CONTROL_ADDR` env)
- **Routes:** `GET /health` (unauthenticated), `GET /piper-status`, `POST /speak {text, engine?, effect?}`
- **Bearer token auth:** CSPRNG-generated 32-hex-char token, constant-time compare, stored in config
- **200KB body cap**, 5s read/write timeout, thread-per-connection plain TCP parser
- **Engine/effect override** in real-time via `/speak` parameters

### 4.9 Effects
- **EffectId::None, WalkieTalkie, GameBoy** -- selectable globally and per `/speak` request, applied via Web Audio API effect chain

### 4.10 Additional Features
- **Text sanitization pipeline** (`sanitize/`): Markdown strip (regex) -> TTS normalize -> artifact cleanup
- **LLM pre-processing (Groq):** Optional pre-TTS rewrite via Groq chat-completions; best-effort -- failure falls back to original text. (Inverse of S2B2S's "Brain before mouth" pattern.)
- **Audio save mode:** Write output to MP3/OGG/FLAC files via `audio/format.rs`
- **System tray:** Listening toggle, speak clipboard, unload model, settings, quit; icon changes when busy
- **Auto-updater:** GitHub Releases via `tauri-plugin-updater`
- **Single instance, auto-start (winreg), close-to-tray, EN+ES i18n, dark/light theme, debug mode**
- **Python path auto-discovery** (`cli.rs`): Scans Python310-Python314, uv tools, system paths

### 4.11 Agent Harness -- `agent-harness/`
- **`cli-anything-copyspeak`**: pip-installable Python CLI + REPL
- **Commands:** `project new/info/set-config`, `queue add/list/remove/clear`, `export text/queue` (verifies audio magic bytes), `backend check/launch`
- **Skills file** (`SKILL.md`) documented for AI agents (Claude Code etc.)
- **All commands support `--json`** flag for machine consumption

---

## 5. Key Code Patterns & Techniques

### 5.1 The `TtsBackend` Trait (`src-tauri/src/tts/mod.rs`, 72 lines)

```rust
pub trait TtsBackend: Send + Sync {
    fn name(&self) -> &str;
    fn synthesize(&self, text: &str, voice: &str, speed: f32) -> Result<Vec<u8>, TtsError>;
    fn health_check(&self) -> Result<(), TtsError>;
    fn file_extension(&self) -> &str { "wav" }  // Override for MP3 backends (ElevenLabs)
    fn voice_display_name(&self, voice_id: &str) -> String { ... }
}
```

This is the engine abstraction **S2B2S copied**. S2B2S's version adds `WarmEngine` and engine status reporting, but the core design -- a trait returning `Vec<u8>` audio bytes, with `Send + Sync` and `health_check()` -- originates here. The trait is intentionally small (5 methods, 2 with defaults) and has proven extensible to 7 backends.

### 5.2 Piper Server State Machine (`piper_server.rs`, 498 lines)

```rust
enum ServerState {
    Stopped,
    Starting { _generation: u64, config: StartingConfig, stderr_tail: Arc<Mutex<Vec<String>>> },
    Ready(Arc<ActiveServer>),
}
static CURRENT_GENERATION: AtomicU64 = AtomicU64::new(0);
```

- **Generation counter** guards against stale starters. Every `spawn_start_thread` receives a generation number. At poll points (health check, readiness gate), the start thread checks if it's still current -- if superseded, it kills the child. This pattern prevents a slow CUDA warmup from clobbering a newer CPU start.
- **`ensure_running()`** locks state, checks Ready (verifies alive + config match), waits on Starting, or kicks off a new start from Stopped. Re-verifies under lock before killing dead servers.
- **Kill-through-the-Mutex** (fixed at HEAD): Uses `active.child.lock().kill() + wait()` through the Arc -- no `try_unwrap`/dummy-cmd fallback.
- **Pre-warm with hidden synthesis:** After readiness, sends a warm-up POST (longer text on CUDA to compile JIT kernels) so the first user utterance avoids the JIT cost.
- **Exponential-backoff health polling:** 100->1600ms, checks `GET /voices` for 2xx, budgets 60s CUDA / 15s CPU.
- **Generalized to `local_tts_server.rs`** for Kokoro/Kitten/Pocket using per-engine `EngineSlot` statics with the same state machine pattern.

### 5.3 Double-Copy State Machine (`clipboard.rs`, 532 lines)

```
ClipboardState { last_text: Option<String>, last_copy_time: Option<Instant> }
```

- **50ms debounce** (Windows fires multiple WM_CLIPBOARDUPDATE per Ctrl+C)
- **Same text twice within `trigger_window_ms`** (default 1500ms) -> trigger
- **State consumed on trigger** (set to None) so a 3rd copy doesn't immediately re-trigger
- **Native dispatch:** `spawn_speak_queued()` calls `speak_queued` directly as an async Tauri state operation, avoiding the webview round-trip that previously added 2x full-text IPC serializations
- **Inline sanitization + truncation** in the clipboard thread before dispatching

### 5.4 `lock_or_recover!` Macro (`main.rs` line 7)

```rust
macro_rules! lock_or_recover {
    ($mutex:expr) => { $mutex.lock().unwrap_or_else(|e| e.into_inner()) };
}
```

Used ubiquitously (synthesis, queue, telemetry, piper_server, config, hud) to recover from mutex poisoning instead of panicking. Fork addition -- upstream panicked.

### 5.5 Telemetry: EMA-Based Performance Tracking (`telemetry.rs`, 370 lines)

- **6 character-count buckets:** [0, 500, 2000, 5000, 10000, u32::MAX)
- **Exponential Moving Average** (alpha=0.3) for duration and `chars_per_ms`
- **Per-backend + per-voice + per-bucket** keying via `TimingKey { backend, voice, bucket }`
- **Deferred saves:** Every 10 samples via `AtomicU32 SAMPLE_COUNTER`; flush on `RunEvent::Exit`
- **Confidence score:** 0.0-1.0 based on sample count (10 samples = 100% confidence)
- **Paginated estimates:** Per-fragment estimates rolled up into total + average confidence

### 5.6 Zero-Gap Streaming Playback

- **Backend:** `speak_queued` synthesizes fragments sequentially (Piper) or 3-concurrently (cloud), emits each as base64 `audio-fragment-ready` event in index order via `spawn_fragment_emit` + `JoinHandle` await
- **Frontend:** `FragmentQueue` in `playback-store.svelte.ts` pre-decodes next fragment's base64 -> `AudioBuffer` while current plays; `AudioContext` pre-warmed at startup
- **200ms Windows preroll:** `prependLowLevelPreroll()` adds near-silent samples to prevent output-device clipping

### 5.7 Synthesis Guard & Abort Escalation

- **`SynthesisGuard`** RAII struct acquires a global `tokio::sync::Mutex<()>` that serializes all synthesis calls
- **`ABORT_REQUESTED` atomic** polled between fragments
- **Abort escalation sequence:** Sets `ABORT_REQUESTED` -> kills active CLI PID -> unloads Piper server (breaks wedged `send()`) -> unloads local servers -> re-prewarms active engine for next utterance

### 5.8 CPU-Intensive Work Offloading

- **WAV envelope extraction:** `tokio::task::spawn_blocking` -- avoids stalling async runtime for large WAV files
- **Base64 encoding:** All audio encoding runs on `spawn_blocking` -- base64 of multi-MB WAVs is CPU-expensive
- **Blocking HTTP synthesis:** Called from `spawn_blocking` within `synthesize_async()` (all backends use blocking reqwest clients)

### 5.9 Pooled HTTP Clients

- **Piper synthesis:** Single `static CLIENT: OnceLock<reqwest::blocking::Client>` with `tcp_nodelay(true)`, `connect_timeout(2s)`, `pool_max_idle_per_host(2)` -- reused across all synthesis calls
- **Piper health check:** Separate client with 1s timeout
- **Cloud backends:** Each holds one pooled `reqwest::Client` with keepalive 60s, <=2 idle per host
- **Groq post-process:** Pooled client with 10s connect + 30s total timeout on the synthesis hot path

---

## 6. Relation to S2B2S

CopySpeak TTS is **the direct ancestor** of S2B2S's TTS subsystem. S2B2S copied and extended the following:

| Aspect | CopySpeak TTS | S2B2S | Verdict |
|--------|---------------|-------|---------|
| **TTS trait** | `TtsBackend { synthesize(), health_check(), file_extension() }` -- 5 methods, `Send+Sync` | Extended with `WarmEngine` trait, engine status, `list_voices()`, async `synthesize_streaming()` -- 8 backends | S2B2S extended it |
| **Piper server** | Persistent `piper.http_server` with state machine (`Stopped/Starting/Ready`), generation counter, CUDA, pre-warm | Same pattern adapted for S2B2S's Piper server with added `WarmEngine` lifecycle (`Loading/WarmingUp/Ready/Error`) | Copied pattern, S2B2S added lifecycle |
| **Pagination** | Sentence-boundary, CJK-safe, abbreviation-aware, adaptive sizing from telemetry | Same module with Unicode-safe splitting; S2B2S also has CJK-safe pagination | Largely replicated |
| **Telemetry** | EMA-based per-engine/voice/bucket, defer saves (every 10), confidence score | Similar pattern in S2B2S `telemetry.rs` | Copied pattern |
| **Fragment queue** | `FragmentQueue` with atomic stop flag, Idle/Playing/Paused/Stopped | Similar but more integrated with the player | Copied pattern |
| **Clipboard trigger** | Win32 `AddClipboardFormatListener`, double-copy 1.5s, 50ms debounce, native dispatch | S2B2S uses `arboard` + polling, also has double-copy trigger | Different OS approach (cross-platform) |
| **Control server** | Plain TCP HTTP parser, bearer token, `/speak` with engine/effect override | S2B2S uses axum, more routes, ECDH/AES-GCM for auth | S2B2S improved it |
| **Sanitization** | Regex-based markdown strip, TTS normalize, artifact cleanup | S2B2S adds ITN (inverse text normalization via `text-processing-rs`) and 5-stage pipeline (ITN->TN->Markdown->TTsNorm->Cleanup) | S2B2S has stronger multilang pipeline |
| **HUD** | Transparent overlay, waveform via envelope, synthesis progress, clipboard notification | S2B2S has recording/speaking overlay window (not HUD-focused) | Different purposes |
| **Audio cache** | History lookup for matching (text, voice, engine) -> replay from cached file | Similar cache concept in S2B2S | Copied pattern |
| **LLM integration** | Groq pre-TTS rewrite (best-effort, simple prompt) | S2B2S has full streaming LLM "Brain" with sentence-splitter TTS bridge + barge-in | S2B2S vastly more capable |

### What S2B2S Does Better
- **Cross-platform:** Windows + macOS + Linux vs CopySpeak's Windows-only
- **STT pipeline:** Full recording + TripleVAD + ParakeetV3 STT (CopySpeak is TTS-only)
- **Brain (LLM):** Streaming LLM chat with barge-in, sentence-splitting TTS bridge
- **More backends:** 9 vs 6 (S2B2S added SAPI stub, but CopySpeak has Cartesia which S2B2S also has)
- **i18n:** 20 languages vs EN+ES
- **Auth:** ECDH+AES-GCM vs bearer token
- **Audio feedback:** Sound effects for recording start/stop

### What CopySpeak Does Better
- **Piper server maturity:** More battle-tested with generation counters, `ensure_running` retries, CUDA auto-discovery, stderr ring buffers
- **Telemetry UX:** EMA-based ETA with confidence score is user-facing (progress bar + % in HUD)
- **Adaptive pagination:** Fragment size scaling based on measured engine speed -- S2B2S uses fixed size
- **Zero-gap streaming:** Pre-decode overlap with `spawn_fragment_emit` ordering
- **Agent harness:** Ready-made Python CLI+REPL+skill for AI agents to drive TTS
- **Clipboard UX polish:** Native dispatch (no webview round-trip), config consumption on trigger, debounce + truncation events

---

## 7. Harvest List (Features Worth Copying)

| Feature to Harvest | From File | Effort | Why Valuable for S2B2S |
|--------------------|-----------|--------|------------------------|
| **Adaptive fragment sizing** | `pagination.rs::adaptive_fragment_size()` | XS | Fast engines get larger fragments (fewer API calls); S2B2S currently uses fixed fragment size |
| **Pre-warm with hidden synthesis** | `piper_server.rs` lines 309-327 | S | Sends warm-up text after server readiness to compile JIT/GPU kernels before first user utterance |
| **Generation counter pattern** | `piper_server.rs` `CURRENT_GENERATION` | S | Guards against stale starters in persistent server lifecycle -- S2B2S's `WarmEngine` could benefit |
| **Stderr ring buffer** | `piper_server.rs` lines 215-232 | XS | 30-line tail surfaced in error messages -- debugging server failures becomes much easier |
| **Deferred telemetry saves** | `telemetry.rs` `SAMPLE_COUNTER` + `SAVE_EVERY_N_SAMPLES` | XS | Avoids disk I/O on every synthesis; S2B2S currently saves every sample |
| **Audio cache pre-check** | `synthesis.rs` history lookup before synthesis | S | Skipping re-synthesis for previously cached text saves API calls and latency |
| **Synthesis progress HUD** | `hud.rs` `SynthesisProgressPayload` + frontend `hud-synthesis-progress.svelte` | M | Real progress bar with telemetry ETA during synthesis (S2B2S lacks this UX) |
| **Zero-gap pre-decode** | `playback-store.svelte.ts` `FragmentQueue` with `AudioContext` pre-decode | M | Pre-decoding next fragment while playing current eliminates gaps |
| **200ms Windows preroll** | `playback-store.svelte.ts` `WINDOWS_AUDIO_PREROLL_MS` | XS | Prevents audio output device from clipping first phonemes on Windows |
| **Agent harness pattern** | `agent-harness/` | L | pip-installable CLI+REPL+skill for AI agents to drive TTS -- for S2B2S's future agent integrations |
| **`lock_or_recover!` macro** | `main.rs` line 7 | XS | Prevents panics from poisoned mutexes -- S2B2S would benefit in its managers |
| **Abort escalation (kill server)** | `main.rs` `do_abort_synthesis()` | S | When Abort is pressed, kill the Piper server to break a wedged send() call -- S2B2S lacks this |
| **Voice change without restart** | Config logic in `main.rs` `set_config` | XS | Piper HTTP server lazy-loads voices per request -- no need to restart on voice change |
| **CUDA DLL path auto-discovery** | `piper_server.rs` `get_nvidia_dll_paths()` | XS | Queries Python once for NVIDIA runtime paths, injects into child PATH -- CUDA works out-of-box |

---

## 8. Known Issues, Caveats & Limitations

The repo's `copyspeak-tts-fork-v3-deep-review-and-plan.md` (2026-06-10, reviewing HEAD `894931f`) contains an exhaustive audit with executed repro cases. Several issues were fixed in subsequent commits (HEAD `78531eb`), but many remain open.

### Critical (R-series)

| ID | Issue | Status | Impact |
|----|-------|--------|--------|
| **R1** | Speed bug: `length_scale` mapping inverted but defused (hard-coded 1.0 in `synthesize_async`); dead `playback_speed` plumbing through ~8 call sites; unit test asserts wrong semantics | **Defused, not fixed** | If someone removes the hard-code, inverted speed + double-application reactivates simultaneously. At slider max 4x, 4x the synthesis work for ~1x perceived speed. |
| **R2** | Dead-server branch spawned `Command::new("cmd")` -- leaked process on Windows, panic everywhere else | **Fixed at HEAD** | Kill-through-the-Mutex replacement in place; no more dummy process |
| **R3** | No total timeout on Piper HTTP synthesis; Abort can't cancel in-flight server request; wedged server freezes `speak_now` with global queue lock held | **Open** | User presses Abort and nothing happens; all future hotkey presses queue behind dead one until app restart |
| **R4** | Control-server token: was non-cryptographic `DefaultHasher`, non-constant-time compare; repo's own clients (`.pi` extension, Claude hook, `test-piper-perf.ps1`) didn't send the token | **Partially fixed** | Token now CSPRNG (32 hex chars) + constant-time compare; `/health` exempt from auth; clients need updating |
| **R5** | Server leaks on engine switch (RAM/VRAM held until exit); voice change triggers unnecessary restart (Piper lazy-loads per request); upstream caches every voice forever | **Open** | Memory/VRAM waste when toggling engines or sampling voices |
| **R6** | Missing voice model -> Piper silently speaks wrong voice with HTTP 200 | **Open** | No error feedback when `.onnx` missing; the helpful CLI-era "download voice" error is unreachable on the server path |
| **R7** | Parallel queued synthesis reports success after a fragment failure | **Open** | `pagination:complete` fires even when fragments failed; UI treats half-finished batches as successful |

### High (H-series)

| ID | Issue | Status | Impact |
|----|-------|--------|--------|
| **H1** | `paginate_text` emitted empty fragments at small sizes (e.g. `"Hi! Ok."` size=1 -> `["H","i","!","","O","k","."]`) | **Fixed at HEAD** | `retain(!trim().is_empty())` + regression tests added |
| **H2** | Undeclared MSRV: requires Rust >= 1.87 (`u32::is_multiple_of`) | **Fixed** | `rust-version = "1.87"` added to `Cargo.toml` |
| **H3** | OpenAI & ElevenLabs test suites gutted (15 -> 1 each) | **Open** | CI green on these files is meaningless; 28 cases still deleted at HEAD |
| **H4** | `unload_piper_model` shelled `taskkill`/`kill -9`, didn't reap, couldn't cancel Starting | **Fixed at HEAD** | Now uses `Child::kill()` + `wait()`; Starting bumped via generation counter |
| **H5** | Piper health check spawned full CLI synthesis (seconds of model load defeating the server's purpose) | **Open** | Health check cost is user-facing; `test_tts_engine` triggers it on the engine page |
| **H6** | `is_piper()` substring heuristic over command+args -> false-positive rerouting | **Open** | Custom engines with "piper" in path get routed through Piper server |
| **H7** | Hotkey path waited for entire long text before any audio | **Fixed at HEAD** | `spawn_speak` now routes long texts to `speak_queued` for streaming (but file-output mode still uses concat) |

### Medium (M-series -- selected highlights)

| ID | Issue | Impact |
|----|-------|--------|
| **M1** | `concat_wav_files` silently corrupts on mismatched formats (22kHz + 44kHz) | Plays at wrong speed |
| **M3** | `adaptive_fragment_size` thresholds mostly unreachable (only warm CUDA crosses 0.5 chars/ms) | Feature is effectively dead |
| **M6** | Pre-decoded fragments re-encode PCM->WAV even at neutral pitch/no effect | Unnecessary CPU overhead |
| **M8** | `windows` crate is unconditional dependency | Bloats non-Windows builds |
| **M11** | Exit cleanup only covers graceful exits; task-manager kill orphans Python server | Memory leak on crash |

### Other Known Limitations

- **Windows-only:** `windows` crate unconditional; Win32 clipboard, `CREATE_NO_WINDOW`, `winreg` autostart
- **SAPI backend is a stub:** `sapi.rs` marked "STUB -- COM interop pending" (S2B2S has same limitation)
- **No audio recording:** No cpal dependency -- pure TTS app, no STT pipeline
- **Config validation gap:** `PaginationConfig` absent from `validate()` chain -- `fragment_size=1` in config bypasses all guards
- **Piper server single-threaded:** Sequential synthesis for local engine (Python Flask); parallel only for cloud backends

---

## 9. Strengths & Weaknesses

### Strengths

1. **Clean engine abstraction:** The `TtsBackend` trait is small (5 methods), well-documented, and has proven extensible to 7 backends including persistent servers and cloud APIs. S2B2S's 9-backend system is a direct evolution.

2. **Persistent server architecture is the right design:** Keeping models in RAM/VRAM between utterances is essential for low-latency local TTS. The state machine with generation counters and health polling is carefully designed for concurrency safety. The fork eliminated the ~1.6s CLI-per-invocation penalty.

3. **Telemetry-driven UX:** EMA-based ETA with confidence score, adaptive pagination that scales with measured engine speed -- genuinely useful features that most TTS apps lack. The user sees real progress bars with confidence percentages.

4. **Zero-gap streaming playback:** Fragment pre-decode overlap and `spawn_fragment_emit` ordering (background base64 encode, frontend `AudioContext` pre-decode) dramatically reduces time-to-first-audio for long texts.

5. **Clipboard UX is polished:** 50ms debounce, configurable window, state consumed on trigger (no accidental re-trigger), native dispatch skipping webview round-trip, inline sanitization, truncation events -- the double-copy interaction is thoughtfully implemented end-to-end.

6. **Honest self-review:** The in-repo self-review ledger with executed repro cases (R1-R7, H1-H7) is rare and valuable. It provides an actionable checklist for anyone porting the code.

7. **Robustness work (fork):** `lock_or_recover!` macro, WAV data-size clamping, Unicode-safe pagination with 51 tests, stdout/stderr drainage threads, exponential-backoff health polling, premature-exit detection with stderr tail -- the fork systematically eliminated crash vectors.

8. **Agent harness:** Forward-thinking `cli-anything-copyspeak` package with skill file for AI agents to drive the app. S2B2S could replicate this pattern.

9. **Config system is well-structured:** Directory-based config modules with enum validation, serialization, and a single `set_config` command that validates and persists. The `ValidationError` enum provides clear, user-facing messages.

10. **Dual synthesis path:** `speak_now` for short texts and file output, `speak_queued` for streaming long texts -- each optimized for its use case.

### Weaknesses

1. **Windows-only:** The `windows` crate is unconditional; Win32 clipboard API, `CREATE_NO_WINDOW`, and `winreg` autostart lock it to Windows. This is a fundamental limitation.

2. **Piper server has no per-request timeout (R3):** A wedged ONNX session freezes `speak_now` with the global queue lock held, blocking all future speech until app restart. The Abort button can't reach an in-flight server request.

3. **Half-finished fixes create landmines (R1, R7):** Speed inversion defused but code still has the inverted mapping and dead plumbing. Parallel synthesis reports success after failures. Both will re-break on the first refactor.

4. **Resource leaks on engine switch (R5):** Switching away from Piper leaves Python server + model in RAM/VRAM until app exit. Voice changes trigger unnecessary full restarts.

5. **No STT pipeline:** Pure TTS -- no recording, VAD, or transcription. By design, but means the app is half of what S2B2S does.

6. **Test coverage degraded (H3):** OpenAI and ElevenLabs test suites gutted from 15 to 1 case each; no integration tests for the Piper server state machine; no CI enforcement of clippy warnings.

7. **Frontend is Svelte 5 (not React):** Can't directly transplant frontend components into S2B2S's React codebase -- behaviors must be re-implemented.

8. **`is_piper()` heuristic is fragile (H6):** Substring matching on command+args for Piper detection will break on paths containing "piper". The preset enum already provides ground truth.

9. **No GPU memory management (R5c):** The Piper server caches every loaded voice in RAM forever (upstream design). No mechanism to bound the cache or evict on low memory.

10. **Silent wrong-voice fallback (R6):** Missing `.onnx` -> Piper silently uses default voice with HTTP 200. The helpful CLI-era error message is lost.

11. **Config validation gaps:** `PaginationConfig` not validated, `fragment_size=1` bypasses all guards and can produce empty fragments (fixed via retain filter at HEAD but validation would prevent it).

---

## 10. Bottom Line / Verdict

CopySpeak TTS (`tts-perf-v2` fork) is the **architectural blueprint** for S2B2S's TTS subsystem. Its `TtsBackend` trait, persistent server pattern with generation counters, telemetry-driven adaptive pagination, and zero-gap streaming playback are all proven designs that S2B2S has already adopted and extended. The self-review ledger (R1-R7, H1-H7, M1-M12) is an invaluable port-time checklist -- it documents exactly which bugs are present and provides concrete fixes.

The single most valuable idea from CopySpeak is the **persistent-server + pre-warm pattern**. Keeping a TTS model server resident and sending a hidden warm-up synthesis after readiness eliminates the ~1.6-second cold-start penalty that makes CLI-per-invocation TTS feel sluggish. S2B2S already implements this, but CopySpeak's version has more mature teardown, stderr diagnostics, and CUDA path discovery that S2B2S could adopt.

**For S2B2S, the harvest priority is:** (1) adaptive fragment sizing from telemetry, (2) pre-warm with hidden synthesis, (3) abort escalation (kill server to break wedged requests), (4) audio cache pre-check before re-synthesis, (5) synthesis progress HUD with telemetry ETA. The agent harness pattern is also worth replicating as S2B2S grows its API surface.

**Verdict: Essential reference.** Even though the frontend is Svelte (not React) and the platform is Windows-only, the Rust backend patterns are directly transferable. Every S2B2S TTS developer should read `piper_server.rs`, `synthesis.rs`, `telemetry.rs`, and the self-review ledger. The known bugs (R1-R7) are particularly important -- they represent exactly the pitfalls S2B2S must avoid when extending its own TTS subsystem.
