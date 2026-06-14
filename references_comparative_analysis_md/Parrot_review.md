# Parrot — Structural Fork of Handy (TTS-Only)

> Repo: `rishiskhare/parrot` · Version: **26.2.4** · HEAD analyzed: `44351ff` · License: **MIT** · Author: **Rishi Khare**
> Platforms: **macOS / Windows / Linux** (cross-platform, Tauri 2.x)
> Nature: **structural fork of Handy** (CJ Pais's Handy, MIT) — squashed history with no `git merge-base`; kinship is in the file tree, not the commit graph.
> Role for S2B2S: **The single most important reference.** Parrot proves Handy's skeleton can carry a TTS subsystem cleanly. Half of S2B2S already built on your exact base.

---

## 1. What Parrot Is

Parrot is a **privacy-first, local-only, cross-platform text-to-speech desktop app.** The user selects text in any application, presses a global shortcut (default: `Option+Space` macOS / `Ctrl+Space` Windows/Linux), and a floating overlay appears while the selected text is spoken aloud via **Kokoro-82M**, a compact neural TTS model (~115 MB ONNX + voices) that runs entirely on CPU with no GPU required.

There is no cloud component, no account, and no API key. The model downloads once (~115 MB), then the app works fully offline. Parrot supports **54 voices across 9 languages** (EN-US, EN-GB, ES, FR, HI, IT, JA, PT-BR, ZH/CMN) with automatic voice-per-language selection plus manual override. Streaming playback starts before the full text is synthesized, and chunks are crossfaded to eliminate seams. Pause/resume is supported mid-utterance.

Parrot is a **fork of Handy** — it keeps Handy's Tauri skeleton, settings system, tray, i18n, model download manager, overlay infrastructure, shortcut handling, and SQLite history, but it **removes everything STT/LLM related** and replaces it with a single, polished TTS subsystem built around tts-rs and Kokoro-82M.

---

## 2. Tech Stack

### 2.1 Frontend

| Layer | Choice | Purpose |
|-------|--------|---------|
| Framework | React 18.3.1 + TypeScript 5.6 | UI components (settings, onboarding, overlay) |
| Build | Vite 6.4 | Dev server + production bundling |
| Styling | Tailwind CSS 4.1 | Utility-first CSS |
| State | Zustand 5.0.8 | Settings store, model store |
| Validation | Zod 3.25 | Type-safe settings schemas |
| i18n | i18next 25.7 + react-i18next 16.4 | 17 languages (en, ar, cs, de, es, fr, it, ja, ko, pl, pt, ru, tr, uk, vi, zh, zh-TW) |
| Icons | Lucide React 0.542 | Icon components |
| Notifications | Sonner 2.0 | Toast notifications |
| Testing | Playwright 1.58 | E2E tests |
| Tauri API | @tauri-apps/api 2.9 | IPC, clipboard, fs, updater, etc. |

### 2.2 Backend / Core

| Layer | Choice | Purpose |
|-------|--------|---------|
| Framework | Tauri 2.9.1 (Rust edition 2021) | Desktop shell, IPC, windowing |
| TTS Engine | tts-rs 2026.2.3 (`kokoro` feature) | In-process Kokoro-82M ONNX synthesis |
| Phoneme frontend | espeak-ng (bundled binary + data) | Kokoro's G2P (grapheme-to-phoneme) |
| Audio I/O | cpal 0.17.1 | Audio device enumeration |
| Audio playback | rodio 0.22.1 (upstream, not cjpais fork) | Sink-based gapless playback via MixerDeviceSink |
| Markdown parsing | pulldown-cmark 0.13 | Convert markdown/HTML → speakable text |
| Shortcuts | handy-keys 0.2.0 + rdev + tauri-plugin-global-shortcut 2.3.1 | Dual-backend shortcut capture |
| Input simulation | enigo 0.6.1 | Keyboard/mouse for clipboard copy |
| Persistence | rusqlite 0.37 (bundled) + tauri-plugin-store 2.4 | History DB + settings JSON |
| Logging | env_filter 0.1 + tauri-plugin-log 2.7 | Console + file logs, rotating |
| IPC binding | tauri-specta 2.0.0-rc.21 | Type-safe command bindings |
| Model download | reqwest 0.12 + tar 0.4 + flate2 1.0 | HTTP download + tar.gz extraction |
| macOS AX API | raw FFI (ApplicationServices + CoreFoundation) | Read selected text without clipboard |
| macOS panels | tauri-nspanel (v2.1 branch) | Floating overlay panel |
| Linux layer | gtk-layer-shell 0.8 + gtk 0.18 | Transparent overlay layer |
| Windows | windows 0.61.3 | HWND topmost forcing for overlay |
| Unix signals | signal-hook 0.3 | SIGUSR1/SIGUSR2 for remote control |
| Updates | tauri-plugin-updater 2.9 | Auto-updater + homebrew cask CI |
| Autostart | tauri-plugin-autostart 2.5 | Login item / LaunchAgent |

### 2.3 Key Dependencies (non-obvious)

| Dependency | Why notable |
|------------|-------------|
| tts-rs 2026.2.3 (`kokoro`) | Author's own crate — wraps Kokoro ONNX inference directly in Rust, no Python subprocess |
| ferrous-opencc 0.2.3 | OpenCC simplified/traditional Chinese conversion for zh language detection |
| once_cell 1.0 | Lazy static `ACTION_MAP` |
| pulldown-cmark 0.13 | Event-based markdown parser with 12 extensions enabled |
| rubato 0.16.2 | Audio resampling (may be vestigial post-STT removal) |
| rusqlite_migration 2.3 | Versioned SQLite schema migrations |

---

## 3. Architecture & Source Map

```
src-tauri/src/                                   (Rust backend — ~6,000+ lines total)
│
├── main.rs                               (18 l)   Binary entry: parse CLI args, Linux DMABUF workaround
├── lib.rs                                (495 l)  Tauri setup, 35+ commands via specta, manager init, espeak-ng resolution
├── cli.rs                                (25 l)   Clap CLI args: --start-hidden, --no-tray, --toggle-transcription, --cancel, --debug
│
├── [CORE TTS SUBSYSTEM]
│   ├── managers/tts.rs                   (2111 l) THE CROWN JEWEL — Kokoro engine pool, lifecycle, streaming playback, chunking,
│   │                                               crossfade, voice selection, overlay text updater, cpu tuning, 21 unit tests
│   ├── text_normalization.rs             (617 l)  pulldown-cmark event walker: markdown→speakable text, URL simplification,
│   │                                               code stripping, HTML entity decoding, 7 unit tests
│   └── managers/model.rs                (969 l)  Model catalog (Kokoro-82M), multi-component download with resume/cancel,
│                                                   progress events, extraction, deletion, auto-selection
│
├── [ACTION PIPELINE]
│   ├── action_coordinator.rs             (115 l)  mpsc thread serializing shortcut events, 30ms debounce, repeat suppression,
│   │                                               panic isolation via catch_unwind
│   ├── actions.rs                        (298 l)  SpeakAction (AX API + sentinel clipboard), PlayPauseAction; ShortcutAction trait
│   ├── signal_handle.rs                  (39 l)   Unix SIGUSR1/SIGUSR2 → ActionCoordinator bridge
│   └── input.rs                          (74 l)   Enigo wrapper, send_copy_ctrl_c (platform-specific: Cmd+C / Ctrl+C)
│
├── [SHORTCUT SYSTEM]
│   ├── shortcut/mod.rs                   (703 l)  Dual-backend router (Tauri/HandyKeys), runtime switching, 14 setting commands
│   ├── shortcut/handler.rs               (63 l)   Shared event handler: routes to coordinator (speak) or ACTION_MAP
│   ├── shortcut/handy_keys.rs            [~200 l] HandyKeys global shortcut implementation (rdev-based)
│   └── shortcut/tauri_impl.rs            [~150 l] Tauri global-shortcut plugin implementation
│
├── [WINDOWING / OVERLAY]
│   ├── overlay.rs                        (440 l)  Speaking overlay: create/show/hide/reposition, NSPanel (macOS),
│   │                                               GTK layer-shell (Linux), Win32 topmost (Windows)
│   ├── tray.rs                           (200 l)  System tray: icon theming, menu (settings/copy/unload/quit), visibility
│   └── tray_i18n.rs                      [~100 l] Tray menu label translations for 17 languages
│
├── [STORAGE & SETTINGS]
│   ├── settings.rs                       (500 l)  AppSettings struct, tauri-plugin-store persistence, migration, serde custom deser
│   └── managers/history.rs               (569 l)  SQLite DB, 1 migration, save/delete/query utterances, WAV per entry,
│                                                   count+time retention, 2 unit tests
│
├── [AUDIO INFRASTRUCTURE]
│   ├── audio_feedback.rs                 (115 l)  Sound cues: start/stop WAV playback via rodio Player, device routing, volume
│   ├── audio_toolkit/mod.rs              (6 l)    Module declarations
│   ├── audio_toolkit/constants.rs        (1 l)    WHISPER_SAMPLE_RATE = 16000 (vestigial from Handy)
│   ├── audio_toolkit/utils.rs            (12 l)   get_cpal_host() — ALSA on Linux, default elsewhere
│   ├── audio_toolkit/audio/device.rs     (36 l)   list_output_devices via cpal
│   ├── audio_toolkit/audio/resampler.rs  [~30 l]  rubato FrameResampler wrapper
│   └── audio_toolkit/audio/utils.rs      (30 l)   save_wav_file: f32 samples → 16-bit WAV via hound
│
├── [COMMANDS (IPC)]
│   ├── commands/mod.rs                   (266 l)  General: cancel, settings, log, history, enigo init, shortcut init,
│   │                                               model status, preload, pause, resize overlay
│   ├── commands/audio.rs                 (95 l)   Audio devices: list, select, test sound, custom sounds
│   ├── commands/models.rs                (172 l)  Models: download, delete, cancel, kokoro voices, active model
│   ├── commands/history.rs               [~100 l] History: entries, toggle saved, audio file path, delete, limit/retention
│   └── commands/transcription.rs         [~30 l]  Vestigial: send_transcription_input wrapper (dead code from Handy)
│
├── [HELPERS]
│   └── helpers/clamshell.rs              (46 l)   is_laptop() — macOS via pmset battery, stub on other platforms
│
└── utils.rs                              (31 l)   cancel_current_operation: stop TTS + unload + hide overlay + update tray

src/                                            (React/TypeScript frontend — ~88 .ts/.tsx files)
│
├── App.tsx                               (217 l)  Main shell: 3-stage onboarding, sidebar nav, TTS error/no-selection toasts
├── main.tsx                              [~15 l]  React root render
├── bindings.ts                           [~500 l] Manually maintained tauri-specta command bindings
│
├── components/
│   ├── settings/                         [30+ files] All settings UI:
│   │   ├── KokoroVoiceSelector.tsx       Voice dropdown from get_kokoro_voices
│   │   ├── TtsSpeed.tsx                  Speed slider 0.5–2.0
│   │   ├── TtsWorkers.tsx                Worker count 0=auto, 1–2
│   │   ├── ShortenFirstChunk.tsx         Toggle for fast-first-chunk optimization
│   │   ├── ShowCloseButton.tsx           Toggle for X button on overlay
│   │   ├── HistoryRetentionPeriod.tsx    Retention policy dropdown
│   │   ├── ModelUnloadTimeout.tsx        Inactivity timeout dropdown
│   │   ├── OutputDeviceSelector.tsx      Audio output device picker
│   │   └── ... (18 more settings components)
│   ├── model-selector/                   [3 files] ModelSelector, ModelDropdown, DownloadProgressDisplay
│   ├── onboarding/                       [3 files] First-run: accessibility step, model card, download progress
│   ├── ui/                               [11 files] Reusable: Button, Select, Input, Slider, ToggleSwitch, Alert, etc.
│   ├── shared/ProgressBar.tsx            Download progress bar
│   ├── footer/Footer.tsx                 Status bar with version
│   └── icons/                            [4 files] ParrotIcon, ParrotTextLogo, etc.
│
├── overlay/
│   ├── main.tsx                          [~20 l]  Overlay window entry point
│   └── SpeakingOverlay.tsx               [~200 l] Three states (processing/speaking/paused), ResizeObserver
│
├── stores/
│   ├── settingsStore.ts                  [~300 l] Zustand: AppSettings, settingUpdaters map, device refresh
│   └── modelStore.ts                     [~200 l] Model download/status state, event listeners
│
├── hooks/
│   ├── useSettings.ts                    [~20 l]  Thin settings store wrapper
│   └── useOsType.ts                      [~15 l]  Platform detection
│
├── i18n/
│   ├── index.ts / languages.ts                        i18next init, 17 language metadata
│   └── locales/{code}/translation.json   [17 files] Translation key files
│
└── lib/
    └── utils/                            [5 files] rtl, keyboard, format, modelTranslation, dateFormat helpers
```

---

## 4. Feature Inventory

### 4.1 TTS Pipeline — The Core

| Feature | Implementation | File(s) | Notes |
|---------|---------------|---------|-------|
| Kokoro engine pool | 2 slots of `Arc<Mutex<Option<KokoroEngine>>>`, auto-tuned by CPU count | `managers/tts.rs:72-86, 1154-1178` | `take_engine_for_active_request` / `return_engine_to_slot` — engine removed from mutex during synthesis |
| Parallel chunk synthesis | Atomic work-stealing index, results via `sync_channel`, re-ordered in `BTreeMap` | `managers/tts.rs:657-756` | Up to 2 workers, sync_channel capacity = workers × 2 |
| Request lifecycle | Monotonically increasing `generation` + `active_request` atomic pair; toggle semantics | `managers/tts.rs:185-225` | `compare_exchange` for race-free cancellation |
| Model lazy loading | `initiate_model_load()` on first use; `wait_for_pending_model_load()` with 30s timeout + condvar | `managers/tts.rs:228-384, 1019-1045` | espeak-ng resolved from bundled resources |
| Model idle auto-unload | Background watcher every 10s checks `last_activity` vs `ModelUnloadTimeout` | `managers/tts.rs:115-164` | `Immediately` option in speak() return path |
| `speak()` orchestrator | Normalize → split chunks → spawn workers → ordered playback → crossfade → history save | `managers/tts.rs:494-991` | 497-line thread::spawn closure; the heart of the app |
| Output device routing | `DeviceSinkBuilder::from_device()` for non-default; falls back to default | `managers/tts.rs:1476-1506` | Matches by cpal device name |

### 4.2 Text Chunking & Latency Engineering

| Feature | Implementation | Constants | Notes |
|---------|---------------|-----------|-------|
| Sentence segmentation | `split_into_sentences()` — scans for `. ! ? …` with context (not between digits) | `managers/tts.rs:1692-1733` | Preserves "2.0" and "1,000" |
| Shorten first chunk | `FIRST_CHUNK_TARGET_CHARS = 60` for first chunk, `CHUNK_TARGET_CHARS = 260` for rest | `managers/tts.rs:1516-1518` | Setting default: `true` |
| Clause boundary split | `split_at_clause_boundary()` finds `,;:` and `)]` break near target with strong-boundary bonus | `managers/tts.rs:1613-1671` | Window: target/2 to target×2 chars, never mid-word |
| Hard limit guard | `split_long_segment()` forced split at best space/clause point | `managers/tts.rs:1736-1801` | CHUNK_HARD_LIMIT_CHARS = 320 |
| Numeric connector protect | `split_breaks_numeric_connector()` prevents "2.0" or "1,000" mid-number split | `managers/tts.rs:1826-1881` | Adjusts fallback split index |

### 4.3 Crossfade Blending & Overlay Sync

| Feature | Implementation | Notes |
|---------|---------------|-------|
| Crossfade | `apply_crossfade()` — linear blend over `CROSSFADE_SAMPLES = 240` (10ms @ 24kHz) | `managers/tts.rs:1135-1145` |
| Tail holdback | Each chunk's last 240 samples held for next chunk; final tail flushed to sink | `managers/tts.rs:844-849, 924-929` |
| Overlay text updater | Dedicated thread sleeps through chunk audio durations, emits `overlay-text` events | `managers/tts.rs:1054-1130` |

### 4.4 Text Normalization (Markdown → Speech)

| Feature | Implementation | Notes |
|---------|---------------|-------|
| Headings stripped | `Tag::Heading` emits block break + terminal punctuation | `text_normalization.rs:89, 153-156` |
| Emphasis/bold/strikethrough | No-ops — plain text passes through | `text_normalization.rs:138-143` |
| Links rendered | If link text differs from URL, use text; else simplify URL ("example dot com") | `text_normalization.rs:173-186, 414-442` |
| Code blocks omitted | "Code example omitted." placeholder | `text_normalization.rs:97-102` |
| Inline code simplified | `simplify_inline_code()`: replaces `_ - / \ . :` between word chars with spaces | `text_normalization.rs:387-408` |
| HTML entities decoded | Named (`&amp;`, `&lt;`, etc.) + numeric (`&#8217;`, `&#x2014;`) | `text_normalization.rs:491-517` |
| HTML tags stripped | `strip_html_tags()`: in-tag chars ignored | `text_normalization.rs:444-489` |
| Lists → flowing text | Ordered lists emit "1. Item."; task lists emit "Completed." / "Not completed." | `text_normalization.rs:75-81, 103-116` |
| Tables → CSV-like | Cells joined with commas, rows separated by block breaks | `text_normalization.rs:126-137` |
| Whitespace normalization | Collapses multiple spaces; normalizes blank lines | `text_normalization.rs:292-350` |
| Terminal punctuation | Adds `.` if block ends without `.!?;:…` | `text_normalization.rs:274-290` |
| Smart spacing | `needs_space_between()` handles apostrophes, quotes, parentheses | `text_normalization.rs:352-385` |
| 7 unit tests | Includes test reading actual `../README.md` and validating >1000 chars | `text_normalization.rs:520-617` |

### 4.5 Selection Capture

| Platform | Method | File(s) |
|----------|--------|---------|
| macOS | Direct AX API: `AXUIElementCreateSystemWide() → AXFocusedUIElement → AXSelectedText` via raw FFI, retried at 0/40/90ms | `actions.rs:67-197` |
| Windows/Linux | Sentinel clipboard probe: write unique `__PARROT_SELECTION_PROBE_{ts}__` → Ctrl+C → poll → restore original clipboard | `actions.rs:199-240` |
| Windows Ctrl+C safety | Releases Shift/Alt/Meta before injecting Ctrl+C (prevents Shift+C → Terminal) | `input.rs:49-74` |

### 4.6 Voice Intelligence

| Feature | Implementation | File(s) |
|---------|---------------|---------|
| Voice enumeration | `collect_available_voices()` from all loaded engines | `managers/tts.rs:1200-1219` |
| Lang → voice mapping | `voice_prefixes_for_language()`: af_=EN-US, bf_=EN-GB, ef_=ES, ff_=FR, hf_=HI, if_=IT, jf_=JA, pf_=PT-BR, zf_=CMN | `managers/tts.rs:1377-1389` |
| Lang code normalization | Handles zh-Hans→cmn, zh-Hant→cmn, yue→cmn, pt-PT→pt-br | `managers/tts.rs:1391-1433` |
| Manual voice override | `selected_kokoro_voice` bypasses language detection; graceful fallback | `managers/tts.rs:1318-1331` |
| Style index | `estimate_kokoro_style_index()`: total non-whitespace chars (min 1) | `managers/tts.rs:1294-1300` |

### 4.7 Overlay / Window Management

| Feature | Implementation | Notes |
|---------|---------------|-------|
| macOS NSPanel | `tauri-nspanel` v2.1, PanelLevel::Status, can_join_all_spaces, transparent | `overlay.rs:314-347` |
| Linux GTK layer-shell | Layer::Overlay, KeyboardMode::None; disabled on KDE Wayland | `overlay.rs:89-125` |
| Windows topmost | `SetWindowPos(HWND_TOPMOST, SWP_NOACTIVATE)` via win32, re-asserted after show | `overlay.rs:129-155, 366` |
| Multi-monitor | Cursor-following overlay on the active monitor | `overlay.rs:157-229` |
| Resize-on-demand | Frontend ResizeObserver → `resize_overlay` command → `resize_and_reposition()` | `overlay.rs:417-424` |
| Fade-out | `hide-overlay` event → CSS animation → 300ms delay → `window.hide()` | `overlay.rs:427-440` |

### 4.8 Settings Categories

| Group | Fields | Default |
|-------|--------|---------|
| General | Shortcuts, TTS language, Kokoro voice, output device, audio feedback, sound theme | speak=Option+Space/Ctrl+Space |
| Models | Download/delete/select Kokoro-82M | 115 MB |
| Advanced → App | Start hidden, autostart, tray icon, overlay position, model unload timeout | overlay=Bottom, unload=Never |
| Advanced → Speech | Worker threads (0=auto, 1–2), speed (0.5–2.0), shorten-first-chunk | workers=0, speed=1.0, shorten=on |
| Advanced → History | Entry limit (1–20), retention (never/limit/3d/2w/3m) | limit=3, period=limit |
| Debug | Log level, keyboard implementation, paths, experimental toggle | log=Debug |

### 4.9 Platform Support

| Platform | Min OS | Key specifics |
|----------|--------|---------------|
| macOS | 10.13+ | HandyKeys default, NSPanel overlay, accessibility perm, Metal, Homebrew cask |
| Windows | 11/10 | Tauri shortcuts default, Vulkan, NSIS installer, Win32 topmost |
| Linux | X11/Wayland | Tauri shortcuts default, overlay disabled by default, GTK layer-shell, libgtk-layer-shell0 |

---

## 5. Key Code Patterns & Techniques

### 5.1 Engine Pool with Mutex-Free Synthesis (`managers/tts.rs`)
The pool is `Arc<Vec<Arc<Mutex<Option<KokoroEngine>>>>>` — 2 slots each behind a Mutex. The trick: `take_engine_for_active_request()` removes the engine from the slot before synthesis (`guard.take()`), so the Mutex is released during the potentially long `synthesize()` call. After synthesis, `return_engine_to_slot()` puts it back. This means cancel signals can acquire the mutex immediately instead of waiting for synthesis to finish. Polling interval: `ENGINE_LOCK_POLL_INTERVAL = 2ms`.

### 5.2 Request ID Toggle Semantics (`managers/tts.rs:185-225`)
Two `AtomicU64` values: `generation` (monotonically increments on each new request + each cancellation) and `active_request`. A request is "active" only when both match. `begin_request_or_toggle_stop()` returns `None` (stop) if `active_request != 0`; otherwise creates a new request ID. `stop_if_request_active()` uses `compare_exchange` on `generation` — only the current holder can cancel the request, preventing races between multiple threads.

### 5.3 Ordered Parallel Synthesis (`managers/tts.rs:657-917`)
Worker threads use an `AtomicUsize` work-stealing index to grab chunks. Results go through a `sync_channel` and are collected in a `BTreeMap<chunk_index, result>`. The main loop drains the BTreeMap in order via a `next_chunk_to_append` counter. This means chunks can synthesize in any order on any worker, but playback is always sequential. The BTreeMap naturally handles out-of-order completion with O(log n) operations.

### 5.4 `shorten_first_chunk` Trick (`managers/tts.rs:1521-1601`)
The first chunk gets a much smaller target (60 chars vs 260). A long first sentence is split at a **clause boundary** (`,;:`) or closing bracket (`)]`) — never mid-word. The search window is `[target/2, target×2]` with a strong-boundary bonus of 10 chars (prefers a `.` or `)` that's slightly farther over a `,` that's slightly closer). This keeps time-to-first-audio low (~1-2s synthesis) without jarring prosody breaks. The key insight: splitting at clause boundaries is perceptually smooth because commas and semicolons are natural pause points in speech.

### 5.5 Crossfade Blending (`managers/tts.rs:1135-1145`)
Each chunk holds back its last 240 samples (10ms @ 24kHz) as a tail. When the next chunk arrives, `apply_crossfade()` linear blurs the boundary: `prev_tail[i] × (1-t) + samples[i] × t` where `t = (i+1)/(overlap+1)`. If the tail is longer than the next chunk (edge case), the non-overlapping prefix is prepended. The final tail is flushed directly to the sink. At 24kHz, 10ms is imperceptibly short yet eliminates all click artifacts.

### 5.6 macOS AX API Selection Capture (`actions.rs:67-142`)
Raw FFI to Apple frameworks — no external crate. Uses `AXUIElementCreateSystemWide()`, `AXUIElementCopyAttributeValue()` with `AXFocusedUIElement` then `AXSelectedText`, converts `CFString` to Rust String. Retried at `[0, 40, 90]ms` because some apps expose selection asynchronously. Falls back to clipboard sentinel probe if AX API fails (e.g., no accessibility permissions).

### 5.7 Sentinel Clipboard Probe (`actions.rs:199-240`)
Writes a unique sentinel value (`__PARROT_SELECTION_PROBE_{timestamp}__`) to clipboard, injects Cmd/Ctrl+C, polls clipboard after 120ms delay, restores original clipboard content. If clipboard still equals sentinel (meaning nothing was copied), returns `None`.

### 5.8 Dual Shortcut Backend (`shortcut/mod.rs`)
`KeyboardImplementation` enum toggles between Tauri's built-in `global-shortcut` plugin and `handy-keys` (rdev-based). HandyKeys auto-falls back to Tauri with persistence on failure. Runtime switching unregisters all shortcuts from old backend, validates for new backend, resets invalid bindings to defaults. `play_pause` is dynamically managed (registered only during playback).

### 5.9 Model Download with Resume (`managers/model.rs:280-528`)
Kokoro-82M downloads as **two separate component files** (ONNX model 88MB + voices binary 27MB) into `models/kokoro/` directory. Each component supports HTTP `Range` header resume from partial files. V1.0+ detection: if the server returns 200 to a Range request, restarts fresh. Progress events throttled to 100ms intervals to avoid UI freeze.

### 5.10 pulldown-cmark Event Renderer (`text_normalization.rs`)
A custom `SpeechTextRenderer` struct walks pulldown-cmark's event iterator, maintaining state stacks for lists (`ListContext` with ordered indices), links (collect text for label vs URL decision), images (collect alt text), and quote depth. `pending_breaks` system defers whitespace until text is actually pushed, ensuring clean spacing. `ensure_terminal_punctuation()` at block boundaries prevents TTS from running sentences together without audible separation.

---

## 6. Diff Analysis vs Handy (What Was Removed, Added, Changed)

### 6.1 Removed (Handy STT/LLM subsystems)

| Removed | Est. Lines | What Parrot does instead |
|---------|-----------|--------------------------|
| `transcribe-rs` (Parakeet V3, Whisper, Moonshine) | All STT models | TTS-only; no transcription |
| `vad-rs` (Silero ONNX), `nnnoiseless` RNNoise | ~400 l | No mic path; VAD and noise suppression unused |
| `rustfft` audio visualizer | ~200 l | Unused |
| LLM client, SSE streaming, llama_server | ~1,400 l | No Brain/LLM |
| `enigo` paste pipeline, clamshell typing | ~200 l | Only kept Ctrl+C injection |
| `strsim`, `natural` fuzzy matching | — | No fuzzy word correction |
| Post-processing settings (translate, custom words, paste mode) | ~15 UI components | 8 new TTS-specific components added |
| `apple_intelligence.rs` | ~100 l | Removed entirely |
| `control_server.rs` (axum HTTP API) | — | Removed entirely |
| `wake_word.rs` KWS detector | — | Removed entirely |
| `transcription_coordinator.rs` | ~200 l | Replaced by `action_coordinator.rs` (115 l) |
| VAD pipeline (triple_vad, smoothed, silero) | ~300 l | Removed entirely |

### 6.2 Added (Parrot's TTS innovation)

| Added | Lines | Purpose |
|-------|-------|---------|
| `managers/tts.rs` | 2,111 l | Complete TTS engine room: Kokoro pool, streaming, crossfade, chunking, voice selection |
| `text_normalization.rs` | 617 l | pulldown-cmark event walker for markdown→speech |
| `tts-rs` (kokoro feature) | external crate | In-process ONNX inference with espeak-ng G2P |
| `espeak-ng-data/` in resources | bundled | 115 MB phoneme data for Kokoro's frontend |
| `pulldown-cmark 0.13` | dependency | Structural markdown parsing |
| `ferrous-opencc 0.2.3` | dependency | Chinese character conversion for zh detection |
| Windows clipboard safety (Shift release) | `input.rs:49-63` | Prevents injected Ctrl+C from opening Terminal |
| `split_at_clause_boundary()` | `tts.rs:1613-1671` | Intelligent first-chunk splitting |
| `apply_crossfade()` | `tts.rs:1135-1145` | 10ms linear crossfade for seamless joins |
| `overlay_text_updater()` | `tts.rs:1054-1130` | Live caption of currently-spoken text |
| macOS AX API selection capture | `actions.rs:67-142` | Direct accessibility read (no clipboard) |
| SpeakingOverlay (3 states + close button) | `overlay/` + `overlay.rs` | Rebuilt from Handy's transcription overlay |
| 8 new settings UI components | `components/settings/` | TTS-specific configuration |

### 6.3 Changed / Repurposed

| Changed | Description |
|---------|-------------|
| `managers/model.rs` | Handy's STT model catalog → Parrot's single Kokoro-82M multi-component model |
| `managers/history.rs` | "Transcription history" → "utterance history" (stores spoken text + WAV) |
| `settings.rs` | Added `tts_workers/tts_speed/tts_shorten_first_chunk/selected_kokoro_voice/show_close_button`; removed STT/Brain fields |
| `action_coordinator.rs` | Simplified from transcription coordinator, 30ms debounce, toggle semantics |
| `audio_feedback.rs` | Added SoundTheme enum (Marimba/Pop/Custom), volume control, device-aware playback |
| `overlay.rs` | Complete rewrite: floating speaking indicator with resize-on-content |
| `shortcut/` | Added HandyKeys backend, runtime switching, play_pause dynamic management |
| Tauri runtime | Patched cjpais fork → stock Tauri 2.9.1 |
| rodio | Patched cjpais fork → upstream rodio 0.22.1 |

### 6.4 Kept Identical / Near-identical from Handy

- Tauri setup pattern (plugin registration, specta command collection)
- Settings deserialization with backward-compatible migration
- tauri-plugin-store pattern (settings_store.json)
- rusqlite database startup and migration flow
- Tray icon theming (dark/light/colored)
- i18n infrastructure and 17 locales
- Onboarding flow (three-step: permissions → model → done)
- Enigo lazy initialization (after accessibility permissions on macOS)
- CLI flags pattern (tauri_plugin_single_instance forwarding)
- Debug mode (Cmd/Ctrl+Shift+D), Unix signal handling
- Autostart / start-hidden / no-tray logic
- Update checker (tauri-plugin-updater)
- Custom sound theme support

---

## 7. Relation to S2B2S

| Aspect | Parrot | S2B2S | Verdict |
|--------|--------|-------|---------|
| **TTS engines** | Kokoro-82M only (single engine, tts-rs in-process) | 8 backends: Piper, Kokoro, Kitten, Pocket, SAPI, OpenAI, ElevenLabs, Cartesia — plus `TtsBackend` trait | S2B2S has broader engine support; Parrot has deeper single-engine polish (pool, crossfade, chunking) |
| **TTS text prep** | `text_normalization.rs` — pulldown-cmark structural markdown parsing | 5-stage pipe: ITN → Custom Words → Markdown strip → TN → Cleanup (regex-based) | Parrot's markdown handling is **strictly superior** (structural vs regex). S2B2S's ITN/TN is absent from Parrot |
| **TTS streaming** | Parallel chunk synthesis + ordered BTreeMap + crossfade + overlay text updater | Fragment queue (unused), player.rs streaming, gapless via pre-decode | Parrot's chunk-based streaming with crossfade is **more sophisticated** |
| **Selection capture** | macOS AX API (direct read, no clipboard) + sentinel clipboard probe fallback | Clipboard-based copy/paste only | Parrot's AX API approach is **cleaner** — no clipboard manipulation on macOS |
| **First-chunk latency** | `shorten_first_chunk` with clause boundary split (60-char target) | No equivalent optimization | Parrot is **ahead** — key latency win |
| **Engine lifecycle** | Pool of 1-2 engines, lazy load, auto-unload on idle timeout | `WarmEngine` trait with Loading→WarmingUp→Ready→Error; no pool parallelism | Parrot's pool enables **true parallel chunk processing** |
| **Architecture** | Tauri 2, 3 managers, mpsc coordinator, specta bindings | Tauri 2, 7+ managers, specta bindings | **Same structural foundation** — merger is architecturally clean |
| **STT** | None (removed) | Parakeet V3 + Whisper + Moonshine + TripleVAD | S2B2S has what Parrot lacks |
| **Brain/LLM** | None (removed) | Streaming SSE client, sentence splitter, barge-in, llama.cpp | S2B2S has what Parrot lacks |
| **Cloud TTS** | None | OpenAI, ElevenLabs, Cartesia | S2B2S has what Parrot lacks |
| **i18n** | 17 languages | 20 languages | Both strong |
| **Platform** | macOS/Win/Linux | macOS/Win/Linux | Both cross-platform |

---

## 8. Harvest List (Features Worth Copying into S2B2S)

| Feature to harvest | From file | Effort | Why valuable for S2B2S |
|--------------------|-----------|--------|------------------------|
| Kokoro engine pool (parallel synthesis) | `managers/tts.rs` lines 72-86, 1154-1178, 1233-1265 | L | Replaces single-engine sequential; required for conversational latency |
| `split_text_for_playback` (chunking + shorten_first_chunk) | `managers/tts.rs` lines 1521-1601 | M | Dramatically reduces time-to-first-audio; #1 latency win |
| `split_at_clause_boundary` | `managers/tts.rs` lines 1613-1671 | S | Intelligent first-chunk split at natural prosody boundaries |
| `apply_crossfade` (linear blend) | `managers/tts.rs` lines 1135-1145 | XS | 15 lines of code to eliminate chunk boundary clicks |
| pulldown-cmark text normalizer (entire file) | `text_normalization.rs` (617 lines) | M | Replace regex markdown stripping with structural parser |
| `overlay_text_updater` (live spoken-text caption) | `managers/tts.rs` lines 1054-1130 | M | Seed for conversation HUD — shows which sentence is spoken |
| macOS AX API selection capture | `actions.rs` lines 67-142 | S | Cleaner than clipboard manipulation; direct accessibility read |
| Request ID toggle semantics | `managers/tts.rs` lines 185-225 | S | Race-free cancellation with no locks |
| Sentinel clipboard probe | `actions.rs` lines 199-240 | S | Reliable fallback when AX API unavailable |
| Ordered parallel synthesis (BTreeMap + sync_channel) | `managers/tts.rs` lines 657-756 | M | Reusable pattern for any ordered-parallel pipeline |
| Voice-per-language auto selection | `managers/tts.rs` lines 1377-1433 | S | Prefix-based matching, simpler than S2B2S's approach |
| CPU auto-tuning | `managers/tts.rs` lines 1154-1178 | XS | Adaptive worker/thread count; 20 lines |
| Dual shortcut backend (HandyKeys + Tauri) | `shortcut/mod.rs` | M | HandyKeys better on macOS; S2B2S uses Tauri only |
| espeak-ng bundling + resolution | `lib.rs` lines 42-92 | S | No PATH dependency; self-contained phoneme frontend |
| Model unload timeout watcher thread | `managers/tts.rs` lines 115-164 | S | Auto-free memory; S2B2S has setting but no watcher |

---

## 9. Known Issues, Caveats & Limitations

| Issue | Severity | Impact |
|-------|----------|--------|
| **Single TTS engine** — Kokoro-82M only; no Piper, no cloud engines, no `TtsBackend` trait | Medium | Must be wrapped behind S2B2S's `TtsBackend` trait for multi-engine support |
| **No STT pipeline** — all transcription removed | N/A (by design) | Must be merged with S2B2S's STT leg |
| **No LLM/Brain** — no conversation mode, no sentence streaming, no barge-in | N/A (by design) | Must be merged with S2B2S's Brain leg |
| **No ITN/TN normalization** — only markdown stripping, no number/date/currency expansion | Medium | S2B2S's `text-processing-rs` ITN/TN pipe is needed |
| **Kokoro engine is in-process** — not shareable across processes | Low | Not relevant for desktop app |
| **Overlay disabled by default on Linux** — focus-stealing issue with some compositors | Low | UX limitation on Wayland; documented WM workaround |
| **macOS HandyKeys not tested on Win/Linux** — forced fallback to Tauri backend | Low | Works fine; fallback is automatic |
| **`WHISPER_SAMPLE_RATE = 16000`** constant is vestigial from Handy | Trivial | Dead code in `audio_toolkit/constants.rs` |
| **rubato in deps** — resampler may be unused post-STT removal | Low | Dependency bloat; could be trimmed |
| **Nix flake present** — may not be actively maintained | Low | Non-critical |
| **No save-to-file or audio format conversion** beyond WAV history | Low | Users can't export MP3/OGG |
| **No voice cloning** — unlike Pocket in S2B2S | Low | Kokoro doesn't support voice cloning |
| **No audio effects** (reverb, EQ, post-processing) | Low | S2B2S doesn't have this either |
| **History limit default of 3** — very conservative | Trivial | Configurable up to 20 |
| **`commands/transcription.rs`** is vestigial/empty from Handy | Trivial | Dead code; should be removed |
| **`ferrous-opencc`** adds binary size for zh detection | Low | Could be simplified to manual code matching |
| **Unit test on README.md** depends on repo root README at compile time | Low | Fragile — README changes could break test |

---

## 10. Strengths & Weaknesses

### Strengths

1. **Depth over breadth.** Parrot does exactly one thing (local TTS from selected text) and executes it flawlessly. The polish is visible in every detail: chunk-at-clause-boundary splitting, 240-sample crossfade blending, per-monitor overlay positioning, macOS AX API with retry logic, Windows clipboard safety guards.

2. **Latency engineering.** The `shorten_first_chunk` trick (60-char target, clause-boundary splitting) combined with parallel synthesis (BTreeMap re-ordering) means audio starts playing ~1-2 seconds after shortcut press for typical text. This is the key metric that makes a TTS app feel "instant."

3. **Clean, idiomatic Rust.** The codebase avoids unwrap(), uses `AtomicU64` for lock-free state, `compare_exchange` for race-free cancellation, `Condvar` for model load synchronization, and `BTreeMap` for ordered parallel results. The `catch_unwind` on the coordinator thread is a thoughtful touch.

4. **Comprehensive text normalization.** The pulldown-cmark event walker handles real-world markdown edge cases that regex-based approaches miss: HTML entity decoding (named + numeric), URL simplification, ordered list numbering, task list state, table rendering, code block omission.

5. **Self-contained distribution.** espeak-ng binary + data bundled in resources, no PATH dependency. Kokoro model is two files downloaded directly from GitHub releases with resume support.

6. **Sturdy fork.** Parrot preserves Handy's Tauri architecture faithfully — settings, tray, i18n, autostart, updates, CLI, single-instance — proving the skeleton is sound for TTS-only apps.

7. **Good test coverage.** 21 unit tests in tts.rs (chunking, voice selection, tuning, cancellation), 7 in text_normalization.rs (including live README parsing), 2 in history.rs, 1 in clamshell.rs. The tests validate the trickiest logic.

### Weaknesses

1. **Single-engine architecture.** No `TtsBackend` trait means adding a second engine requires duplicating the entire `TTSManager` pattern. This is Parrot's biggest architectural debt.

2. **No ITN/TN normalization.** The text normalization is markdown-focused. It does not expand abbreviations ("Dr." → "Doctor"), normalize numbers ("42" → "forty-two"), handle dates ("2024-01-15" → "January 15th 2024"), or resolve currency symbols. S2B2S has this covered with `text-processing-rs`.

3. **No streaming LLM integration.** Parrot is a "read selection" TTS, not a "conversation" TTS. There is no sentence-by-sentence streaming from an LLM, no barge-in support, and no turn-taking logic.

4. **Vestigial Handy code.** `WHISPER_SAMPLE_RATE` constant, `rubato` resampler, `commands/transcription.rs`, `audio_toolkit/constants.rs` — leftover from Handy and could be cleaned up.

5. **Kokoro model is CPU-only.** ONNX Runtime with CPU execution provider means no GPU acceleration. On very long texts (>5000 chars), synthesis latency grows proportionally.

6. **macOS-centric polish.** The AX API selection capture is macOS-only. Windows/Linux users fall back to clipboard manipulation, which has edge cases with password fields, terminals, and apps that don't support Ctrl+C.

7. **No accessibility API on Windows.** Windows UIA (UI Automation) could mirror the macOS AX API approach but is not implemented.

8. **History database uses `transcription_history` table name** despite being a TTS-only app — confusing naming inherited from Handy.

---

## 11. Bottom Line / Verdict

Parrot is **the single most valuable reference project for S2B2S's TTS leg.** It proves beyond doubt that Handy's Tauri 2 skeleton (settings, tray, i18n, shortcut system, model download manager, overlay infrastructure) can cleanly host a local TTS subsystem at near-instant latency. Every component Parrot added — engine pool, chunk-at-clause-boundary splitting, crossfade blending, pulldown-cmark normalization, macOS AX API selection capture, ordered parallel synthesis — is directly transferable to S2B2S.

The merger strategy is clear: **lift Parrot's `managers/tts.rs` engine pool, chunking, and crossfade patterns into S2B2S's existing multi-backend TTS framework** (behind `TtsBackend` trait), adopt `text_normalization.rs` as the markdown sanitizer (replacing regex-based stripping), and backport the AX-API + sentinel selection capture. The estimated combined effort is approximately 3-5 days of focused Rust work.

**The single most valuable idea:** `shorten_first_chunk` with clause-boundary splitting. This one technique, implemented in under 200 lines of Rust, gets time-to-first-audio down from "wait for full synthesis" to "hear it by the time you look at the overlay" — and that perceptual difference makes the product feel like magic. For S2B2S's conversation mode where the LLM streams sentences one at a time, this technique applied to each sentence would make the voice feel immediate and natural.

*— Analysis completed June 2026. Source files read: all Rust files under src-tauri/src/ (19 files, ~6,000+ lines), all TypeScript/TSX files under src/ (88 files), all config files (Cargo.toml, package.json, tauri.conf.json, tsconfig.json, vite.config.ts), README.md, CLAUDE.md, BUILD.md, CHANGELOG.md, LICENSE.*
