# Whispering - Independent STT Desktop App (AGPL-3.0)

> Repo: `EpicenterHQ/epicenter` (monorepo, `apps/whispering`) · Version: `7.11.0` · License: AGPL-3.0-or-later · Author: Braden Wong · Platforms: macOS / Windows / Linux / Web
> Nature: independent · Role for S2B2S: concept donor - provider matrix, transformation pipeline, platform DI, Svelte architecture

---

## 1. What Whispering Is

Whispering is an open-source desktop speech-to-text (STT) application with a companion web version. The user presses a global keyboard shortcut, speaks, and transcribed text is delivered to the cursor via clipboard-sandwich paste or simulated typing. It supports **9 transcription providers** (3 local engines + 5 cloud APIs + 1 self-hosted server) and an optional **transformation pipeline** that sends transcribed text through LLM-powered formatting/fixup/translation steps.

The app is built as a Tauri + Svelte 5 desktop app (~22 MB binary) with a 97% code-shared web version hosted at `whispering.epicenter.so`. It is the flagship product of the Epicenter ecosystem and the most polished non-Fork STT app in the analyzed set. The author uses it daily and explicitly designed it to compete with paid tools (Superwhisper, Wispr Flow).

Core workflow: **Shortcut trigger → audio capture** (manual push-to-talk or VAD) → **transcription** (local or cloud) → **optional AI transformation chain** → **deliver to cursor/clipboard**.

---

## 2. Tech Stack

### 2.1 Frontend

| Layer | Choice | Purpose |
|-------|--------|---------|
| UI framework | Svelte 5 (runes mode) | Reactive components, `$state`/`$derived`/`$effect` |
| Routing | SvelteKit (static adapter) | SPA routing, page layouts, SSG for both Tauri and Cloudflare |
| Styling | Tailwind CSS 4 + shadcn-svelte | Utility-first CSS, accessible component primitives |
| State (domain) | Yjs CRDT + SvelteMap | Reactive CRDT-backed maps for recordings, transformations, runs |
| State (settings) | workspace KV (Yjs) + SvelteMap | Synced settings with per-key last-write-wins |
| State (queries) | TanStack Query v5 | Server-state caching for audio URLs, device enumeration |
| Forms/validation | TypeBox + @epicenter/field | Runtime type checking, workspace schema validation |
| Toast/OS notify | svelte-sonner + OS notify (via `$lib/report`) | Unified report spine fanning to toast, console, OS notification |
| Icons | Lucide Svelte + raw SVG imports | Component library + provider brand icons |
| Tables | TanStack Table v9 alpha | Recordings list with sorting, filtering |
| Build tool | Vite (with SvelteKit + Tailwind plugin) | Dev server, HMR, production bundling |
| Package manager | Bun | Monorepo workspaces, scripts |
| Hosting | Cloudflare Workers + Assets | Web app deployment via Wrangler |
| MCP server | `@sveltejs/mcp` | Editor integration for Svelte development |

### 2.2 Backend / Core (Rust via Tauri)

| Layer | Choice | Purpose |
|-------|--------|---------|
| Framework | Tauri 2.x | Desktop app shell, IPC, plugin system |
| Audio capture | cpal 0.16 | Cross-platform audio input (WASAPI / CoreAudio / ALSA) |
| Audio decode | Symphonia 0.5 + audiopus 0.3 | Container demux + codec decode (WAV, MP3, M4A/AAC, FLAC, OGG, WebM/Opus) |
| Audio encode | audiopus + ogg 0.9 | Opus encode to OGG container for cloud upload (24 kbps VBR voice) |
| Resampling | rubato 0.15 | Sinc fixed-input resampler (BlackmanHarris2 window, 64-tap) |
| Local STT | transcribe-rs 0.3.8 | Whisper.cpp (Vulkan/Metal), Parakeet (ONNX + CoreML/DirectML), Moonshine (ONNX) |
| Clipboard | tauri-plugin-clipboard-manager | Read/write clipboard |
| Keyboard paste | enigo 0.5 | Simulate Ctrl+V/Cmd+V keystrokes (virtual key codes, layout-independent) |
| Shortcuts | tauri-plugin-global-shortcut | OS-level global hotkeys |
| HTTP | tauri-plugin-http | reqwest-backed fetch for Tauri context |
| File system | tauri-plugin-fs | Artifact read/write, model downloads |
| IPC types | tauri-specta (v2 RC) | Typed Rust to TS bindings, Result-based error handling |
| Logging | tauri-plugin-log + log | Structured logging to stdout and log directory |
| Analytics | tauri-plugin-aptabase | Anonymized event tracking (opt-out) |
| Crash handling | Custom panic hook | Captures backtraces to temp directory |
| Single instance | tauri-plugin-single-instance | Prevents duplicate launches |
| Auto-start | tauri-plugin-autostart | Login item management (macOS LaunchAgent) |
| Updater | tauri-plugin-updater | In-app updates from GitHub releases |


### 2.3 Key Dependencies (non-obvious)

| Dependency | Purpose |
|------------|---------|
| `wellcrafted` | Custom Rust-inspired Result/Error library by the author. `defineErrors` creates tagged error unions; `tryAsync`/`trySync` wrap async/sync code in Result; `defineKeys` creates typed TanStack cache keys |
| `@epicenter/workspace` | Custom CRDT workspace framework by the author. Defines tables + KV schemas backed by Yjs, with schema validation, observers, and Svelte reactivity adapters |
| `@epicenter/svelte` | `fromTable()` adapter: Yjs table to reactive SvelteMap, auto-subscribing observer pattern |
| `@epicenter/field` | Schema field constructors (string, number, boolean, select, json) used in workspace definitions |
| `@ricky0123/vad-web` | Browser-side Silero VAD v5 (`MicVAD` class), encodes WAV on speech end |
| `dexie` + `idb` | IndexedDB wrapper for browser blob persistence |
| `nanoid` | Non-secure ID generation (recording IDs, run IDs) |
| `marked` | Markdown-to-HTML rendering for transformation output display |
| `dompurify` | HTML sanitization for rendered markdown |
| `arktype` | Runtime type validation library |
| `groq-sdk`, `openai`, `elevenlabs`, `@anthropic-ai/sdk`, `@google/generative-ai`, `@mistralai/mistralai` | Provider SDKs for cloud transcription and LLM completions |

---

## 3. Architecture & Source Map

```
whispering/
├── README.md                          (1119 lines) -- User-facing docs, setup guide, contributing guide
├── AGENTS.md                          (53 lines)  -- AI agent instructions
├── ARCHITECTURE.md                    (193 lines) -- Architecture deep dive
├── LICENSE                            (666 lines) -- AGPL-3.0-or-later
├── package.json                       (148 lines) -- Dependencies, scripts, #platform/* imports map
├── svelte.config.js                   (36 lines)  -- SvelteKit config, static adapter, aliases
├── vite.config.ts                     (45 lines)  -- Build-time platform DI (tauri condition), plugins
├── tsconfig.json                      (15 lines)  -- Bundler resolution for #platform/*
├── wrangler.jsonc                     (21 lines)  -- Cloudflare deployment config
│
├── docs/
│   ├── articles/array-access-at-vs-brackets.md  (89 lines) -- Code style: .at() vs [] for arrays
│   └── audio-test-fixtures.md                    (30 lines) -- How to regenerate test fixtures
│
├── specs/                             (~25 spec files) -- Design decisions and implementation plans
│
├── src-tauri/                         -- Rust backend --
│   ├── Cargo.toml                     (109 lines) -- Rust deps, per-platform features, release profile
│   ├── tauri.conf.json                (84 lines)  -- App config, bundle targets, CSP, updater keys
│   ├── build.rs                       (3 lines)   -- Standard tauri-build
│   ├── src/
│   │   ├── main.rs                    (6 lines)   -- Binary entry point
│   │   ├── lib.rs                     (327 lines) -- Tauri setup: plugins, specta builder, crash hook,
│   │   │                                           -- write_text (clipboard-sandwich paste), simulate_enter
│   │   ├── command.rs                 (33 lines)  -- open_accessibility_settings (macOS only)
│   │   ├── markdown.rs                (88 lines)  -- Atomic markdown file export with path traversal guard
│   │   ├── audio/
│   │   │   ├── mod.rs                 (24 lines)  -- Module declarations
│   │   │   ├── command.rs             (48 lines)  -- encode_recording_for_upload (raw IPC byte body)
│   │   │   ├── decode.rs              (394 lines) -- Symphonia + libopus decode pipeline to 16kHz mono f32
│   │   │   ├── encode.rs              (320 lines) -- libopus encode to OGG container (24kbps VBR voice)
│   │   │   ├── error.rs               (48 lines)  -- AudioError enum (Decode/Unsupported/Resample/Encode)
│   │   │   └── resample.rs            (73 lines)  -- rubato SincFixedIn resampler (64-tap BlackmanHarris2)
│   │   ├── recorder/
│   │   │   ├── mod.rs                 (11 lines)  -- Module declarations
│   │   │   ├── recorder.rs            (598 lines) -- CPAL two-thread pipeline: callback to mpsc to consumer
│   │   │   │                                           -- worker, downmix, resample, device finding, error categorization
│   │   │   ├── artifact.rs            (352 lines) -- Durable WAV artifact: write, read, find, delete, clear
│   │   │   │                                           -- with path traversal guard and tests
│   │   │   └── commands.rs            (192 lines) -- Tauri commands: init/start/stop/cancel/close session
│   │   └── transcription/
│   │       ├── mod.rs                 (73 lines)  -- set_transcription_config, get_transcription_state, transcribe_recording
│   │       ├── config.rs              (61 lines)  -- TranscriptionConfig, Engine enum, UnloadPolicy
│   │       ├── error.rs               (29 lines)  -- TranscriptionError tagged enum
│   │       ├── events.rs              (163 lines) -- ModelStateEvent, ModelStatus, UnloadReason lifecycle
│   │       └── model_manager.rs       (934 lines) -- Model cache (Arc<Mutex<...>>), generation-gated preload/inference,
│   │                                                   -- idle eviction, per-engine dispatch (Whisper/Parakeet/Moonshine)
│   └── tests/
│       ├── decode_fixtures.rs         (66 lines)  -- Integration tests: MP3, M4A/AAC, WebM/Opus, OGG/Opus
│       └── fixtures/                   -- 4 audio test fixtures (sine_440_2s.{mp3,m4a,opus,webm})
│
├── src/                               -- TypeScript/Svelte frontend --
│   ├── app.html / app.d.ts            -- SvelteKit app shell and type declarations
│   ├── routes/                        -- SvelteKit file-based routing
│   │   ├── +layout.svelte / +layout.ts
│   │   ├── +error.svelte
│   │   ├── (app)/
│   │   │   ├── +layout.svelte         -- Main app layout, command registration, onboarding, icon sync
│   │   │   ├── +page.svelte           -- Home page: manual/VAD recording toggle
│   │   │   ├── _components/           -- AppLayout, BottomNav, VerticalNav, ManualRecordingButton
│   │   │   ├── _layout-utils/         -- alwaysOnTop, checkForUpdates, registerAccessibilityPermission, etc.
│   │   │   └── (config)/              -- Settings and data pages
│   │   │       ├── settings/          -- Transcription, API keys, recording, shortcuts, sound, analytics
│   │   │       ├── recordings/        -- Recording list, playback, row actions, transformation picker
│   │   │       ├── transformations/   -- CRUD for transformation pipelines, editor, test runner
│   │   │       ├── debug/             -- Debug page
│   │   │       └── global-shortcut/   -- Global shortcut configuration
│   │   └── transform-clipboard/       -- Standalone clipboard transformation window
│   │
│   └── lib/
│       ├── commands.ts                -- Re-exported public API
│       ├── tauri/
│       │   ├── commands.ts            (115 lines) -- Boundary adapter: specta Result to wellcrafted Result
│       │   ├── bindings.gen.ts        -- Auto-generated tauri-specta TypeScript bindings
│       │   ├── tauri.tauri.ts         -- Tauri-only namespace (fs, permissions, window, tray, shortcuts)
│       │   ├── tauri.browser.ts       -- Web fallback: tauri = null
│       │   └── autostart-keys.ts      -- Cross-platform autostart key definitions
│       │
│       ├── workspace/
│       │   ├── definition.ts          (412 lines) -- Complete workspace schema: 5 tables + ~40 KV settings
│       │   └── index.ts               (12 lines)  -- Re-exports
│       │
│       ├── services/                  -- Service Layer (pure, platform-agnostic, Result-typed) --
│       │   ├── index.ts               (21 lines)  -- Service barrel
│       │   ├── README.md              (500 lines) -- Comprehensive service layer documentation
│       │   ├── transcription/
│       │   │   ├── providers.ts       (261 lines) -- PROVIDERS registry: 9 providers single source of truth
│       │   │   ├── provider-ui.ts     (44 lines)  -- UI-facing join: provider + icon + dark mode invert
│       │   │   ├── local-preflight.ts (92 lines)  -- FE-side model path validation before Rust call
│       │   │   ├── model-file.ts      (11 lines)  -- isModelFileSizeValid (>=90% expected size)
│       │   │   ├── utils.ts           (29 lines)  -- getAudioExtension with webm/ogg normalization
│       │   │   ├── cloud/openai.ts    (176 lines) -- OpenAI Whisper transcription (status-code dispatch)
│       │   │   ├── cloud/groq.ts      (162 lines) -- Groq Whisper transcription
│       │   │   ├── cloud/deepgram.ts  -- Deepgram Nova transcription
│       │   │   ├── cloud/elevenlabs.ts-- ElevenLabs Scribe transcription
│       │   │   ├── cloud/mistral.ts   -- Mistral Voxtral transcription
│       │   │   └── self-hosted/speaches.ts -- Speaches (local Whisper server) transcription
│       │   ├── completion/
│       │   │   ├── index.ts           (16 lines)  -- Completion barrel (6 providers)
│       │   │   ├── types.ts           (42 lines)  -- CompletionService interface, CompletionError
│       │   │   ├── openai.ts          (7 lines)   -- OpenAI (via OpenAI-compatible factory)
│       │   │   ├── anthropic.ts       (48 lines)  -- Anthropic Claude completions
│       │   │   ├── google.ts          -- Google Gemini completions
│       │   │   ├── groq.ts            -- Groq Llama completions
│       │   │   ├── custom.ts          -- Custom OpenAI-compatible endpoint
│       │   │   └── openrouter.ts      -- OpenRouter completions
│       │   ├── recorder/
│       │   │   ├── types.ts           (275 lines) -- RecorderService interface, RecordingSession, DeviceIdentifier brand
│       │   │   ├── index.tauri.ts     -- Tauri CPAL recorder implementation
│       │   │   ├── index.browser.ts   -- Navigator MediaRecorder implementation
│       │   │   └── categorize-error.ts-- isMicDenied/isNoDevice for navigator errors
│       │   ├── device-stream.ts       (152 lines) -- getRecordingStream, enumerateDevices, cleanup
│       │   ├── blob-store/            -- Audio persistence (IndexedDB on web, filesystem on Tauri)
│       │   ├── text/                  -- Clipboard operations (copy, write to cursor, simulate enter)
│       │   ├── sound/                 -- Playback of notification sounds (with assets/)
│       │   ├── analytics/             -- Aptabase event logging
│       │   ├── download/              -- Model download service
│       │   ├── http/                  -- Custom fetch with platform-specific transport
│       │   └── local-shortcut-manager.ts -- In-window keyboard shortcut manager
│       │
│       ├── state/                     -- Reactive State (Svelte 5 runes + Yjs CRDTs) --
│       │   ├── settings.svelte.ts     (51 lines)  -- Reactive settings via SvelteMap + Yjs KV observer
│       │   ├── device-config.svelte.ts-- localStorage-backed device config (API keys, paths, IDs)
│       │   ├── manual-recorder.svelte.ts (159 lines) -- Reactive manual recorder: state, bootstrap, start/stop/cancel
│       │   ├── vad-recorder.svelte.ts (208 lines) -- Reactive VAD: MicVAD lifecycle, stream management
│       │   ├── recordings.svelte.ts   (133 lines) -- Reactive recordings via fromTable() + SvelteMap
│       │   ├── transformations.svelte.ts -- Reactive transformations table
│       │   ├── transformation-steps.svelte.ts -- Reactive transformation steps table
│       │   ├── transformation-runs.svelte.ts -- Reactive transformation run records
│       │   ├── transformation-step-runs.svelte.ts -- Reactive step run records
│       │   └── local-model.svelte.ts  -- Local model download state
│       │
│       ├── operations/                -- Orchestrations (workflow coordination) --
│       │   ├── recording.ts           (243 lines) -- start/stop/cancel manual and VAD recording
│       │   ├── pipeline.ts            (148 lines) -- processRecordingPipeline: persist to transcribe to transform
│       │   ├── transcribe.ts          (263 lines) -- transcribeAudio: dispatch by provider id
│       │   ├── transform.ts           (255 lines) -- runTransformation: step pipeline with per-step run records
│       │   ├── delivery.ts            (144 lines) -- deliverTranscriptionResult, deliverTransformationResult
│       │   ├── analytics.ts           -- Log event wrapper
│       │   ├── shortcuts.ts           -- Shortcut action dispatchers
│       │   ├── sound.ts               -- Sound playback orchestrator
│       │   └── transformation-clipboard.ts -- Clipboard-only transformation workflow
│       │
│       ├── rpc/                       -- RPC Layer (TanStack adapters) --
│       │   ├── index.ts               (15 lines)  -- RPC barrel
│       │   ├── README.md              (180 lines) -- Comprehensive RPC layer documentation
│       │   ├── client.ts              -- QueryClient, defineQuery/defineMutation factories
│       │   ├── audio.ts               -- Audio playback URL queries
│       │   ├── download.ts            -- Download mutations
│       │   ├── transcription.ts       (52 lines)  -- Transcribe mutation with recording status updates
│       │   └── transformer.ts         (68 lines)  -- Transform mutation (input + recording variants)
│       │
│       ├── report/                    -- Unified error reporting spine --
│       │   ├── index.ts               (126 lines) -- report.error/success/info/loading + log.info
│       │   ├── humanize.ts            (42 lines)  -- CamelCase to Title Case (handles acronyms)
│       │   ├── os-notify.tauri.ts     -- Tauri OS notification
│       │   └── os-notify.browser.ts   -- Browser notification
│       │
│       ├── constants/                 -- Type-safe configuration constants --
│       │   ├── local-models.ts        (284 lines) -- WHISPER_MODELS, PARAKEET_MODELS, MOONSHINE_MODELS download catalogs
│       │   ├── inference.ts           (104 lines) -- INFERENCE providers: OpenAI, Groq, Anthropic, Google, OpenRouter, Custom
│       │   ├── transformations.ts     (22 lines)  -- TRANSFORMATION_STEP_TYPES, type options
│       │   ├── audio/                 -- Recording modes, states, sample rates, bitrates, constraints
│       │   ├── keyboard/              -- Accelerator keys, browser keys, modifiers, dead keys
│       │   ├── icons/                 -- Provider brand SVGs (deepgram, elevenlabs, ggml, groq, etc.)
│       │   ├── languages.ts           -- Supported STT languages
│       │   └── urls.ts                -- App route constants
│       │
│       ├── utils/                     -- Shared utilities --
│       │   ├── accelerator.ts         -- Keyboard accelerator parsing/formatting
│       │   ├── template.ts            -- {{input}} template interpolation
│       │   └── createPressedKeys.svelte.ts -- Pressed key tracker (Svelte rune)
│       │
│       ├── migration/                 -- Data migration --
│       │   ├── migrate-database.ts    -- IndexedDB to Yjs workspace migration
│       │   ├── migrate-settings.ts    -- localStorage to Yjs KV migration
│       │   ├── MigrationDialog.svelte -- Migration progress UI
│       │   └── migration-dialog.svelte.ts -- Dialog state
│       │
│       └── components/                -- Reusable UI components --
│           ├── settings/              -- API key inputs (8 providers), transcription selectors, device selectors
│           ├── transformations-editor/-- Configuration, Editor, Runs, Test components
│           ├── MoreDetailsDialog.svelte -- Error detail dialog
│           └── UpdateDialog.svelte    -- App update notification
│
└── static/                            -- Static assets (favicons, webmanifest)
```

---

## 4. Feature Inventory

### 4.1 STT Pipeline

| Feature | Implementation | Files |
|---------|---------------|-------|
| **9-provider transcription matrix** | `PROVIDERS` registry + per-provider service files + Rust dispatch | `providers.ts` (261L), `transcribe.ts` (263L), `cloud/*.ts`, `model_manager.rs` (934L) |
| **Local Whisper.cpp** | ggml .bin models, Vulkan/Metal/CPU backends via transcribe-rs | `model_manager.rs`, `config.rs`, `local-models.ts` (284L) |
| **Local Parakeet (NVIDIA NeMo)** | ONNX INT8 quantized, CoreML/DirectML acceleration | `model_manager.rs`, `local-models.ts` |
| **Local Moonshine (UsefulSensors)** | ONNX encoder-decoder KV cache, tiny/base variants | `model_manager.rs`, `local-models.ts` |
| **Cloud Groq** | Whisper v3/v3-turbo (fastest, $0.04/hr) | `cloud/groq.ts` (162L) |
| **Cloud OpenAI** | whisper-1, gpt-4o-transcribe, gpt-4o-mini-transcribe | `cloud/openai.ts` (176L) |
| **Cloud ElevenLabs** | Scribe v1/v2 ($0.40/hr, 99 languages, speaker diarization) | `cloud/elevenlabs.ts` |
| **Cloud Deepgram** | Nova-2/Nova-3, Enhanced, Base | `cloud/deepgram.ts` |
| **Cloud Mistral** | Voxtral mini/small | `cloud/mistral.ts` |
| **Self-hosted Speaches** | Local Whisper server, your own base URL | `self-hosted/speaches.ts` |
| **Model lifecycle** | Generation-gated preload on config change, idle watcher with configurable timeout, immediate eviction option | `model_manager.rs` (934L) |
| **Model download catalog** | 3 Whisper + 1 Parakeet + 2 Moonshine models with exact URLs, sizes, file lists | `local-models.ts` (284L) |
| **Truncated download detection** | File size >=90% expected check | `model-file.ts` (11L), `transcribe.ts` (lines 169-190) |
| **FE preflight validation** | Model path existence + type (file/dir) checked before Rust IPC | `local-preflight.ts` (92L) |

### 4.2 Audio Pipeline (Rust)

| Feature | Implementation | Files |
|---------|---------------|-------|
| **CPAL two-thread recorder** | Callback thread: downmix to mono f32, mpsc send. Consumer: accumulate, resample 16kHz, pad short clips, emit artifact | `recorder.rs` (598L) |
| **Universal audio decode** | Symphonia demux/decoder (WAV, MP3, AAC/M4A, FLAC, OGG, WebM) + libopus for Opus | `decode.rs` (394L) |
| **Cloud upload encode** | libopus Voip mode, 24kbps VBR, 20ms frames, OGG container | `encode.rs` (320L) |
| **WAV artifact persistence** | Hand-written IEEE float WAV (format tag 3), fsynced, path traversal guard | `artifact.rs` (352L) |
| **Short recording padding** | <=1s recordings padded to 1.25s (20,000 samples) to prevent Whisper hallucination | `recorder.rs` (line 44) |
| **NaN/Inf sanitization** | Replaces non-finite f32 before whisper.cpp FFI (prevents GGML_ASSERT crash) | `model_manager.rs` (lines 790-809) |
| **GPU acceleration** | DirectML (Windows), CoreML (macOS), Vulkan (Linux) at startup | `lib.rs` (lines 133-142) |
| **Integration tests** | MP3, M4A/AAC, WebM/Opus, OGG/Opus fixture decode tests | `decode_fixtures.rs` (66L) |
| **Round-trip test** | 5s sine: decode -> encode -> decode -> verify duration (<=50ms drift) + frequency (<=10Hz drift) | `encode.rs` (lines 256-304) |

### 4.3 Transformation Pipeline

| Feature | Implementation | Files |
|---------|---------------|-------|
| **Step types** | `prompt_transform` (LLM call) and `find_replace` (regex or literal) | `transformations.ts` (22L) |
| **6 LLM providers** | OpenAI, Anthropic, Google Gemini, Groq, OpenRouter, Custom | `completion/` dir, `inference.ts` (104L) |
| **Per-step provider memory** | Flat row schema: all provider model fields present on every step | `definition.ts` (lines 72-98) |
| **Template interpolation** | `{{input}}` replaced in system and user prompt templates | `template.ts` |
| **Per-step run records** | Each step input, output, status, error persisted to workspace | `transform.ts` (lines 192-244) |
| **Run tracking** | TransformationRuns + TransformationStepRuns tables with timeline | `definition.ts` (lines 145-171) |
| **Active transformation** | Per-recording or global active transformation, auto-runs after transcription | `pipeline.ts` (lines 107-147) |

### 4.4 Voice Activity Detection (VAD)

| Feature | Implementation | Files |
|---------|---------------|-------|
| **Browser-side Silero VAD** | `@ricky0123/vad-web` v5, MicVAD class, onSpeechStart/onSpeechEnd | `vad-recorder.svelte.ts` (208L) |
| **Automatic stream management** | getRecordingStream with device fallback, cleanup on errors | `device-stream.ts` (152L) |
| **WAV encoding** | `utils.encodeWAV(audio)` converts Float32Array to WAV blob | `vad-recorder.svelte.ts` (line 147) |

### 4.5 Delivery / Output

| Feature | Implementation | Files |
|---------|---------------|-------|
| **Clipboard-sandwich paste** | Save original clipboard -> write text -> simulate Ctrl+V/Cmd+V -> restore original | `lib.rs` (lines 259-310) |
| **Layout-independent paste** | Virtual key codes (Key::Other(0x56) on Windows, Key::Other(9) on macOS) | `lib.rs` (lines 275-281) |
| **Simulated Enter** | enigo Key::Return Click, for auto-submit after paste | `lib.rs` (lines 318-327) |
| **Per-stage output config** | Independent toggles for transcription vs transformation: clipboard, cursor, enter | `definition.ts` (lines 214-221) |
| **Delivery fallbacks** | Cursor write failure -> offer clipboard copy; copy failure -> standalone error toast | `delivery.ts` (144L) |
| **Atomic markdown export** | Temp file + persist, path traversal guard, duplicate filename detection | `markdown.rs` (88L) |

### 4.6 Recording History

| Feature | Implementation | Files |
|---------|---------------|-------|
| **CRDT-backed recording store** | Yjs table -> SvelteMap via `fromTable()`, instant reactivity across windows | `recordings.svelte.ts` (133L) |
| **Sorted recordings** | `$derived` memoized sorted array (newest first), TanStack Table compatible | `recordings.svelte.ts` (lines 36-42) |
| **Bulk delete** | Single-scan O(n) delete vs n x O(n) individual deletes | `recordings.svelte.ts` (lines 118-120) |
| **Transcription status tracking** | UNPROCESSED -> TRANSCRIBING -> DONE / FAILED | `definition.ts` (lines 35-40) |
| **Retention policy** | keep-forever or limit-count (default 100) | `definition.ts` (lines 236-242) |

### 4.7 Settings & Configuration

| Feature | Implementation | Files |
|---------|---------------|-------|
| **Synced workspace settings** | ~40 KV entries with per-key LWW resolution via Yjs, dot-notation namespace | `definition.ts` (lines 188-340) |
| **Reactive settings access** | SvelteMap + Yjs observeAll -> components re-render per-key on change | `settings.svelte.ts` (51L) |
| **Device-local config** | API keys, filesystem paths, device IDs stay in localStorage, not synced | `device-config.svelte.ts` |
| **Bulk reset** | Single Yjs transaction resets all settings to defaults | `definition.ts` (lines 400-409) |
| **Sound effect toggles** | Per-event sound enable/disable: manual, VAD, transcribe/transform complete | `definition.ts` (lines 188-197) |

### 4.8 Platform Features

| Feature | Implementation | Files |
|---------|---------------|-------|
| **97% code sharing desktop/web** | Build-time platform DI via `#platform/*` subpath imports | `package.json` imports, `vite.config.ts` |
| **Tauri-only namespace** | `tauri.tauri.ts` + `tauri.browser.ts` (null fallback), `if (tauri)` guard pattern | services README |
| **Crash logging** | Custom panic hook writes backtrace to temp directory | `lib.rs` (lines 77-131) |
| **Single instance** | tauri-plugin-single-instance, second launch focuses existing window | `lib.rs` (lines 198-203) |
| **App updates** | tauri-plugin-updater with GitHub releases endpoint | `tauri.conf.json` (lines 54-59) |

### 4.9 UI/UX

| Feature | Implementation | Files |
|---------|---------------|-------|
| **Unified report spine** | `report.error/success/info/loading` -> toast + console + OS notify (when unfocused) | `report/index.ts` (126L) |
| **Loading lifecycle** | `report.loading()` -> `.resolve(notice)` / `.reject(problem)` pattern | `report/index.ts` (lines 46-55) |
| **Humanized error names** | `humanize.ts` converts CamelCase to Title Case, handles acronyms | `humanize.ts` (42L) |
| **Error detail dialog** | "More details" action opens raw error for debugging | `MoreDetailsDialog.svelte` |
| **Dark mode** | mode-watcher, per-provider SVG invertInDarkMode flag | `provider-ui.ts` |
| **Recording state tray icon** | System tray icon changes per recording state | `syncIconWithRecorderState.svelte.ts` |
| **Migration dialog** | IndexedDB/localStorage -> Yjs workspace migration with progress UI | `migration/` directory |

---

## 5. Key Code Patterns & Techniques

### 5.1 The Provider Matrix Concept

This is Whispering single most valuable architectural contribution. The provider matrix separates **provider identity** from **provider behavior**, enabling a multi-provider UI that is type-safe and compile-checked.

**Files:** `providers.ts` (261L), `provider-ui.ts` (44L), `transcribe.ts` (263L)

**Pattern:**

1. **`PROVIDERS`** is a single `as const` object with one entry per provider. Each entry declares location (`cloud` / `local` / `self-hosted`), capabilities (`supportsPrompt`, `supportsLanguage`), models, default model, and settings key names (never values - the dispatcher reads values later).

```typescript
const PROVIDERS = {
  OpenAI: {
    location: 'cloud',
    label: 'OpenAI',
    apiKeyKey: 'apiKeys.openai',
    modelKey: 'transcription.openai.model',
    defaultModel: 'whisper-1',
    models: [{ name: 'whisper-1', cost: '$0.36/hour' }],
    capabilities: { supportsPrompt: true, supportsLanguage: true },
  },
  whispercpp: {
    location: 'local',
    label: 'Whisper C++',
    modelPathKey: 'transcription.whispercpp.modelPath',
    preflightKind: 'file',
    capabilities: { supportsPrompt: true, supportsLanguage: true },
  },
  // ... 7 more
} as const satisfies Record<string, TranscriptionProvider>;
```

2. **`CLOUD_TRANSCRIBERS`** is a `satisfies Record<CloudProviderId, CloudTranscribe>` dispatch table. Adding a cloud provider to `PROVIDERS` without a transcriber is a compile error.

3. **`PROVIDER_ICONS`** is a separate `satisfies Record<TranscriptionServiceId, {icon, invertInDarkMode}>` structure. Kept in a different file so the workspace schema can import `TRANSCRIPTION_SERVICE_IDS` without bundling SVG blobs.

4. **The `transcribeAudio()` dispatcher** reads `PROVIDERS[selectedService].location` to choose local (Rust IPC) vs cloud/self-hosted (service call), then uses the provider config keys to pull API keys, model selection, and endpoint overrides.

**What makes this special:** The `as const satisfies` pattern creates a closed union type `TranscriptionServiceId = keyof typeof PROVIDERS`. Every switch is exhaustiveness-checked. Adding a provider means adding one entry to `PROVIDERS`, one icon to `PROVIDER_ICONS`, one transcriber to `CLOUD_TRANSCRIBERS`, and one KV setting entry - the compiler catches missing pieces.

**For S2B2S:** This is the concept S2B2S should adapt for its engine abstraction. S2B2S TTS backend trait already does this in Rust; Whispering shows how to do it purely in TypeScript with type-level enforcement.

### 5.2 The Transformation Pipeline Concept

The transformation pipeline is a mini workflow engine that chains steps on transcribed text.

**Files:** `transform.ts` (255L), `definition.ts` (lines 72-171), `constants/transformations.ts` (22L)

**Key patterns:**

1. **Flat row schema for steps** - instead of discriminated unions per step type, every step row has ALL fields (openaiModel, groqModel, systemPromptTemplate, findText, replaceText, etc.). When `table.set()` replaces the row, a discriminated union would lose the inactive variant data. Flat rows preserve everything.

2. **Per-provider model memory** - each inference provider model selection is stored independently on the step row. Switching providers and back preserves choices. Same pattern used for KV transcription settings.

3. **Run tracking** - each run and step run gets a record with `{ status: 'running' | 'completed' | 'failed' }` results using TypeBox union types via `field.json()`.

**For S2B2S:** Concept donor for S2B2S text sanitize pipeline. The flat-row trick applies to any entity with per-variant field memory.

### 5.3 Build-Time Platform Dependency Injection

**Files:** `package.json` (imports field, lines 10-58), `vite.config.ts` (lines 12-23)

**Pattern:** Each platform-bound service has `.browser.ts` and `.tauri.ts` siblings sharing a `types.ts` contract. The `package.json` `imports` field maps `#platform/<service>` to the correct file based on build condition:

```jsonc
"#platform/recorder": {
  "tauri": "./src/lib/services/recorder/index.tauri.ts",
  "default": "./src/lib/services/recorder/index.browser.ts"
}
```

The Tauri build activates the `tauri` condition; the web build falls through to `default` (browser). Consumers import `from '#platform/recorder'` with no platform branch. The off-target file is never resolved, physically absent from the bundle.

**What makes this special:** This is NOT tree-shaking - it is a build-time resolution guarantee. A Tauri-only import in shared code fails the web build entirely. Uses Node-standard subpath imports with no custom Vite plugin.

### 5.4 Yjs CRDT + SvelteMap Reactive State

**Files:** `definition.ts` (412L), `recordings.svelte.ts` (133L), `settings.svelte.ts` (51L)

**Pattern:** Domain data lives in Yjs CRDT documents. The `fromTable()` adapter wraps a Yjs table into a reactive `SvelteMap` that auto-subscribes to Yjs observers. Settings use Yjs KV entries for per-key last-write-wins resolution. A `$derived` memoized sorted array provides stable references.

**What makes this special:** Eliminates cache invalidation and optimistic updates entirely. The CRDT IS the cache, always consistent. Multi-window apps get instant sync for free.

### 5.5 Tagged Error Flow (WellCrafted)

**Files:** Services README (500L), `report/index.ts` (126L), `humanize.ts` (42L), every service file

**Pattern:** Every service returns `Result<T, E>` where `E` is a tagged error created via `defineErrors`. Errors flow unchanged from service -> operation -> UI. The report spine auto-derives toast title from error variant name via `humanize()` (e.g., `MissingApiKey` -> "Missing api key").

**For S2B2S:** S2B2S uses `anyhow::Error` in Rust. This TS pattern shows how to preserve error structure across layers without a translation step.

### 5.6 Rust Model Manager with Generation-Gated Operations

**Files:** `model_manager.rs` (934L)

**Pattern:** Model cache is `Arc<Mutex<Option<(PathBuf, Engine)>>>` holding ONE resident model. A `model_generation` atomic counter increments on config changes. Operations carry their generation; before publishing state, they check if still current. Stale preloads silently drop; stale transcriptions keep engine but stop publishing events.

Key details:
- `ensure_loaded()` checks generation twice (before and after acquiring cache lock)
- `publish_if_current()` atomically checks generation under read lock, closing check-then-publish gap
- Idle watcher uses `try_lock()` so long transcription does not block eviction
- Cache mutex poisoning recovery: clears cached engine, lets next caller reload from scratch
- NaN/Inf sanitization and MAX_SAMPLES cap prevent whisper.cpp GGML_ASSERT aborts

**For S2B2S:** S2B2S model lifecycle could benefit from generation-gating to prevent stale loads publishing state.

### 5.7 Recording Session Abstraction

**Files:** `recorder/types.ts` (275L), `manual-recorder.svelte.ts` (159L), `recorder.rs` (598L)

**Pattern:** The `RecorderService` is a factory returning `RecordingSession` objects. Each session owns stop/cancel/subscribe lifecycle. The manual recorder state wraps the service with Svelte runes, bootstrap logic (rehydrate CPAL sessions surviving JS reload), and in-flight start guards.

Key details:
- `RecorderStopResult`: tagged union `kind: 'artifact'` (CPAL WAV handle) or `kind: 'blob'` (navigator blob)
- `DeviceAcquisitionOutcome`: `outcome: 'success' | 'fallback'` with actual device ID
- Bootstrap: `resumeActiveSession()` rehydrates CPAL sessions outliving webview reload
- `_starting` guard prevents double-starts during async bootstrap window

**For S2B2S:** Session-as-value-object pattern is cleaner than S2B2S state-machine-in-manager.

### 5.8 Audio Decode Pipeline

**Files:** `decode.rs` (394L), `encode.rs` (320L)

**Pattern:** Single canonical decode path: bytes -> Symphonia probe -> per-codec dispatch. Non-Opus codecs use Symphonia built-in decoder. Opus uses Symphonia for container demux + libopus (audiopus) for decode. Target rate always 16 kHz mono f32.

Key details:
- Downmix: channel average (not L-only, preserves all channel content)
- Resampler: rubato SincFixedIn with 64-tap BlackmanHarris2, 128x oversampling
- Opus decode: extracts channel count from OpusHead extra data (RFC 7845)
- Encode: libopus Voip mode, 24 kbps VBR, 20ms frames, OGG container
- Roundtrip test: 5s sine verify duration (<=50ms drift) + frequency (<=10Hz drift) using Goertzel DFT

### 5.9 Workspace Schema with TypeBox

**Files:** `definition.ts` (412L)

**Pattern:** Entire application data model in one file using `@epicenter/workspace` `defineTable` and `defineKv` functions, backed by TypeBox for runtime validation and Yjs for CRDT persistence. Table schemas validate rows on read. KV schemas define defaults and value types. The `createWhispering()` factory produces the complete workspace bundle.

---

## 6. Relation to S2B2S

| Aspect | Whispering | S2B2S | Verdict |
|--------|-----------|-------|---------|
| **Scope** | STT only (speech->text->cursor) | Full voice pipeline (STT + Brain + TTS) | S2B2S broader |
| **License** | AGPL-3.0 (copyleft, strong) | MIT (permissive) | S2B2S more permissive |
| **UI framework** | Svelte 5 (runes) | React 19 + TypeScript | Different ecosystem |
| **Desktop framework** | Tauri 2 | Tauri 2 | Same |
| **Audio capture** | CPAL two-thread pipeline | CPAL + VAD (audio_toolkit) | Whispering simpler |
| **STT backends** | 9 providers (local + cloud matrix) | transcribe-rs (Parakeet V3, Whisper, Moonshine) | Whispering has provider matrix; S2B2S has deeper native integration |
| **TTS** | None | 8 backends (Piper, Kokoro, Kitten, Pocket, SAPI, OpenAI, ElevenLabs, Cartesia) | S2B2S only |
| **Brain/LLM** | Transformations only (LLM post-processing) | Full streaming LLM conversation with sentence splitter, barge-in | S2B2S much deeper |
| **Text normalization** | None (relies on LLM transforms) | 5-stage pipeline: ITN, TN, markdown strip, custom words, cleanup | S2B2S builds what Whispering delegates to LLMs |
| **Audio decode** | Symphonia + libopus (universal) | hound + resample (WAV only) | Whispering more universal |
| **State management** | Yjs CRDT + SvelteMap + TanStack Query | Zustand + tauri-plugin-store | Different philosophies |
| **Platform targets** | Desktop + Web (97% code share) | Desktop only (Win/Mac/Linux) | Whispering dual-target |
| **Error handling** | Tagged errors, humanize, report spine | anyhow::Error (Rust), try-catch (TS) | Whispering more structured |
| **Model lifecycle** | Generation-gated cache, preload, idle eviction | WarmEngine trait (Loading->WarmingUp->Ready->Error) | Both sophisticated |
| **Provider abstraction** | TypeScript `as const satisfies` + switch dispatch | Rust `TtsBackend` trait + macro dispatch | Both valid, different languages |
| **VAD** | Silero VAD v5 (browser-side) | TripleVAD: RMS -> RNNoise prob -> Silero (Rust-side) | S2B2S more sophisticated |
| **Binary size** | ~22 MB (Svelte) | Larger (React + more Rust deps) | Whispering lighter |

---

## 7. Harvest List (Concepts Worth Copying - Zero Code Transfer, AGPL Barrier)

| Concept to harvest | From file | Effort | Why valuable for S2B2S |
|-------------------|-----------|--------|------------------------|
| **Provider matrix pattern** (registry + dispatch table + type-safe switch) | `providers.ts`, `transcribe.ts` | M | S2B2S already has TTS backends in Rust. This shows how to build the TypeScript equivalent for any provider-selectable front-end feature (e.g., LLM provider selection for the Brain) |
| **Tagged error -> humanize -> toast spine** | `report/index.ts`, `humanize.ts` | S | S2B2S error handling could benefit from a unified reporting pattern that auto-derives user-facing error messages from error variant names |
| **Flat row schema for multi-variant entities** | `definition.ts` (transformationSteps) | S | When S2B2S needs to persist entities with per-variant field memory (e.g., per-TTS-engine voice/speed settings), the flat-row pattern avoids discriminated-union data loss |
| **Config ambient push pattern** | `transcription/mod.rs`, `config.rs` | S | Push config once, read on every operation. S2B2S could use this for the LLM config (instead of passing it per-call) |
| **Generation-gated async operations** | `model_manager.rs` | M | S2B2S model loading could benefit from generation-gating to prevent stale loads from publishing state after a config change |
| **Session-as-value-object pattern** | `recorder/types.ts`, `manual-recorder.svelte.ts` | S | S2B2S recording sessions could be refactored from state-machine-in-manager to values that own their lifecycle |
| **Dual-purpose `as const satisfies`** for closed union + UI metadata | `providers.ts`, `inference.ts` | S | Type-safe enumerations that serve as both schema validation and UI dropdown options |
| **Per-provider model memory in KV** | `definition.ts` (transcription settings) | S | When users switch STT/TTS providers and back, their model choices persist independently |
| **Short recording padding** | `recorder.rs` (lines 42-44) | XS | Pad sub-1s recordings to 1.25s to prevent Whisper hallucination on short clips |
| **NaN/Inf sanitization before FFI** | `model_manager.rs` (lines 790-809) | XS | Prevent GGML_ASSERT crashes from malformed sample buffers reaching whisper.cpp |
| **Symphonia + libopus decode** | `decode.rs` | L | Universal audio decode (WAV, MP3, M4A, FLAC, OGG, WebM/Opus). S2B2S hound-only path is limited |

---

## 8. Known Issues, Caveats & Limitations

| Issue | Severity | Impact |
|-------|----------|--------|
| **AGPL-3.0 license** | Critical for reuse | ZERO code can cross from Whispering into MIT-licensed S2B2S. Any code copied would require S2B2S to also be AGPL-3.0. This analysis is concept-only, zero code transfer. |
| **Mac-only accessibility feature** | Low | `open_accessibility_settings` is macOS-only; Windows/Linux return an error. |
| **VAD is browser-side only** | Medium | Silero VAD runs in browser (via @ricky0123/vad-web), not in Rust. CPAL recorder has no VAD integration. S2B2S TripleVAD in Rust is cleaner. |
| **No TTS or brain** | N/A | Whispering intentionally stays STT-only. Transformation pipeline fills gap for text post-processing via LLMs. |
| **No text normalization** | Medium | Relies on LLM-based transformations for formatting. No ITN/TN pipeline. S2B2S 5-stage pipeline is more deterministic. |
| **Flat row schema complexity** | Low | Transformation step schema has all fields on every row. Preserves data but makes schema large. Intentional trade-off. |
| **Transcription config push-once** | Low | If FE pushes corrupted config, Rust side has no recovery beyond rejecting it. |
| **Moonshine limited to English** | Low | Only `en` language enabled; 7 other languages commented out in `local-models.ts`. |
| **Release profile disables LTO** | Medium | macOS CoreAudio FFI crashes forced disabling `lto = true` and `opt-level = "s"`. |
| **Specta v2 RC dependency** | Low | Uses tauri-specta RC, not stable. Raw IPC byte responses are manually typed. |
| **No streaming transcription** | Medium | All transcription is batch (record -> stop -> transcribe -> deliver). No real-time STT. By design per README. |

---

## 9. Strengths & Weaknesses

### Strengths

1. **Provider matrix is the gold standard** - the `PROVIDERS` registry + `satisfies Record<CloudProviderId, ...>` dispatch table is exhaustiveness-checked at compile time. Adding a provider is a checklist, not a hunt through scattered switch statements. This is the single most transferable concept for S2B2S.

2. **Tagged error flow is rigorous** - every service returns `Result<T, E>`, errors carry structured context, the report spine auto-derives user-facing copy from error variant names. No `try-catch`, no swallowed errors, no ad-hoc string error messages.

3. **Build-time platform DI is elegant** - Node-standard subpath imports (`#platform/*`) with build condition resolution prevent Tauri-only code from ever reaching the web bundle. No runtime branches, build-time failure for wrong-platform imports.

4. **CRDT-backed reactive state is real-time** - Yjs CRDTs with `SvelteMap` auto-subscription means zero cache invalidation code. Multi-window apps get instant sync for free. The architecture eliminates an entire class of bugs.

5. **Rust audio pipeline is production-grade** - Symphonia + libopus for universal decode, libopus Voip mode for cloud upload at 24kbps VBR, rubato sinc resampling, comprehensive test coverage (4 codec/container integration tests, roundtrip test with Goertzel frequency verification).

6. **Workspace schema is self-documenting** - the entire data model lives in one 412-line file (`definition.ts`). Tables, KV entries, validation rules, defaults, and types are all in one place. The `createWhispering()` factory composes it all.

7. **Excellent documentation** - README (1119L) with setup guide, cost comparison, FAQ, contributing guide, adapter authoring guide. ARCHITECTURE.md (193L) with layer diagrams. Services README (500L) with patterns, anti-patterns, and worked examples. RPC README (180L) with canonical module shape.

8. **Polished UX** - toast + OS notification spine, loading lifecycle (resolve/reject), "More details" error dialog, humanized error names, keyboard shortcut recorder.

### Weaknesses

1. **AGPL-3.0 is a barrier** - the strongest copyleft license means zero code reuse for MIT projects. S2B2S can only harvest concepts, not implementation.

2. **No native VAD in Rust** - VAD runs in the browser via `@ricky0123/vad-web`. The CPAL recording path has no voice activity detection. For truly hands-free dictation on desktop, this is a gap.

3. **Svelte-only UI** - all components are Svelte 5 with runes. S2B2S uses React. The architecture patterns transfer, but no UI code can be reused even if the license allowed it.

4. **Transformation pipeline is LLM-dependent** - text post-processing requires an LLM provider with an API key. No deterministic ITN/TN pipeline. Whispering cannot correct common ASR errors (numbers, dates, abbreviations) without hitting an LLM.

5. **Model download UX is manual** - users must select models and click "Download". No automatic model discovery or hardware-based recommendations.

6. **Monorepo coupling** - depends on `@epicenter/workspace`, `@epicenter/svelte`, `@epicenter/field`, etc. These are not published npm packages. Extracting any piece requires extracting the entire workspace framework.

7. **No streaming transcription** - all transcription is batch (record -> stop -> transcribe -> deliver). No real-time streaming STT. This is by design but limits real-time use cases.

8. **Web version is feature-limited** - no global shortcuts, no local transcription (requires Rust/CPAL), no auto-paste. The 97% code share stat is mostly framework-level; actual feature parity varies significantly.

---

## 10. Bottom Line / Verdict

Whispering is the most architecturally sophisticated independent STT app in the analyzed set. Its three most valuable contributions to S2B2S are: **(1) the provider matrix pattern** - a type-safe, exhaustiveness-checked registry + dispatch table that eliminates provider-switch fragility; **(2) the tagged error -> humanize -> toast spine** - a unified error reporting architecture that auto-derives user-facing copy from structured error types; and **(3) the session-as-value-object pattern** - recording sessions that own their lifecycle rather than relying on mutable manager state.

However, the AGPL-3.0 license imposes a hard boundary: **zero code can cross**. Every concept must be independently reimplemented. The Svelte 5 + Yjs CRDT tech stack is fundamentally incompatible with S2B2S React 19 + Zustand stack. The patterns transfer cleanly; the implementation does not.

The single most valuable idea from Whispering is the `as const satisfies` provider registry with a type-keyed dispatch table. S2B2S should study this pattern for any future frontend feature that requires multi-provider selection (e.g., LLM provider picker for the Brain). The Rust equivalent already exists in S2B2S `TtsBackend` trait; the TypeScript equivalent does not, and Whispering shows the cleanest way to build it.

---

*Analysis complete. Every source file in the `whispering/` project was read and analyzed. Total source files: ~90 (16 Rust + ~45 TypeScript/Svelte + ~25 spec files + 2 docs + config files).*
