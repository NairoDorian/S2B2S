# Changelog

All notable changes to S2B2S are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/), and this
project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Fixed

- **Piper CUDA DLL path discovery** (`piper_server.rs`): `get_nvidia_dll_paths` used `nvidia.__file__` which is `None` for namespace packages. Switched to `nvidia.__path__[0]` with dynamic `bin`/`bin/x86_64` discovery, fixing `CUDAExecutionProvider` init.  
- **Build broken by windows-core version mismatch** (`webview_hardening.rs`): Added `windows-core-061` alias to fix `ICoreWebView2Settings.cast<>()` across two versions of `windows-core` in the dependency tree.  
- **llama.cpp server Remove button** (`manager.rs`): `list_downloaded_servers` used `splitn(2, '-')` on folder names like `cuda-13.3-b9741`, splitting on the first hyphen. Changed to `rsplit_once('-')` so backends with hyphens (e.g. `cuda-13.3`) parse correctly.

### Changed

- **Venv setup scripts** (`setup_venv_uv.ps1`, `setup_tts_venv.ps1`): Switch onnxruntime-gpu to CUDA 13 nightly feed with canonical nvidia package names; add coloredlogs/flatbuffers/packaging/protobuf/sympy build deps.

## [0.1.4] — 2026-06-20: CI Consolidation, Standalone Speech Runtime & Playwright E2E Tests

> **CI consolidation, standalone speech runtime, panic auditing, and Playwright E2E tests.** Consolidated documentation sprawl, established a single status dashboard (`STATUS.md`), optimized CI configurations, implemented onboarding-time portable Python/uv speech runtime provisioning, audited and converted Rust panics to safe errors, and created comprehensive mock Tauri IPC E2E tests.

### Added

- **Onboarding Python Runtime Installer**: Standalone setup scripts (`scripts/install-speech-runtime.ps1` for Windows, `scripts/install-speech-runtime.sh` for macOS/Linux) to provision a portable speech environment (portable `uv` + relocatable Python 3.12.13 + local `venv/` dependency setup) during onboarding.
- **Playwright E2E Testing**: Custom mock Tauri IPC layer (`tests/helpers/tauri-mock.ts`) to enable headless test execution. Created spec suites for the core pipelines: Onboarding (`tests/specs/onboarding.spec.ts`), Dictation HUD (`tests/specs/dictation.spec.ts`), and Conversation Tab (`tests/specs/conversation.spec.ts`).
- **`STATUS.md`**: Single source of truth status, scorecard, and roadmap for S2B2S.
- **`docs/`**: Consolidated conceptual specifications (`docs/vision.md`), Android companion plans (`docs/android.md`), and dev references (`docs/references.md`).
- **Unified GitHub Actions**: Unified lint, typecheck, translation, and unit test CI workflow (`.github/workflows/ci.yml`) and package builds (`build-main.yml`, `build-test.yml`).

### Changed

- **Onboarding UI**: Modified `Onboarding.tsx` to execute and display installation progress of the portable speech runtime.
- **Rust Backend Panic Audit**: Converted potential panics (`unwrap()` and `expect()` calls) into handled `Result` boundaries in `audio_toolkit/audio/recorder.rs`, `tts/player.rs`, `clipboard.rs`, and command handlers.
- **WebGL Easing & Bypass**: Added automated skips in `HerLoading.tsx` for headless runs to eliminate test flakiness.
- **`README.md` & docs**: Fixed obsolete links, corrected translation completeness metrics, and pointed roadmap references to `STATUS.md`.
- **Developer docs & guidelines**: Merged guidelines, dev commands, and setup instructions into `AGENTS.md`, `BUILD.md`, and `CONTRIBUTING.md`.

### Fixed

- **Mock Model Onboarding Hook**: Ensured mock handlers in `tauri-mock.ts` emit `model-download-complete` and updated `model-state-changed` events, and handle `set_active_model` commands.
- **Playwright strict locator violations**: Refined main layout selectors to prevent strict mode errors on sidebar lookups.
- **App spec console errors**: Wrapped generic dev server spec runs with mock Tauri IPC structures to prevent OS-plugin platform errors.

### Removed

- **Obsolete files & directories**: Cleaned up `CRUSH.md`, `LLAMA_CPP.md`, `CONTRIBUTING_TRANSLATIONS.md`, `android-port-plan.md`, `S2B2S_REVIEW.md`, `reference_github_links.md`, `analysys/`, `futuristic_analysis/`, and duplicate workflow definitions.

---

## [0.1.3] — 2026-06-20: Piper CUDA, Warmup Prompt, Loading Animation & Process Cleanup

> **Piper CUDA GPU inference, configurable Brain warmup prompt, interactive loading animation, and Windows Job Object process cleanup.** Fixed multiple startup/reliability issues: app invisibility on start-hidden, premature brain:llama-ready event during warmup, and test panel metrics race condition. Replaced TTS venv setup with uv-based approach following correct piper→onnxruntime-gpu install order. Added Windows Job Object to auto-kill child processes on crash.

### Added

- **Windows Job Object** (`job_object.rs`): Creates a job with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` and assigns all spawned child processes (Piper, Kokoro, Kitten, Pocket, llama-server, STT server) to it — OS auto-terminates them if the app crashes or exits without cleanup (`lib.rs:877-883`, `piper_server.rs:377`, `local_tts_server.rs:354`, `llama_manager.rs:238`, `unified_parakeet.rs:62`). gated with `#[cfg(windows)]`, no-op on other platforms.
- **Configurable warmup prompt** (`settings.rs`): `BrainConfig.warmup_prompt` field with `#[serde(default = "default_warmup_prompt")]` defaulting to `"Count from 1 to 10"`.
- **`scripts/setup_venv_uv.ps1`**: Fast uv-based venv setup script. Installs piper-tts last, then replaces CPU onnxruntime with GPU onnxruntime + NVIDIA CUDA PyPI DLL packages. Auto-detects uv or falls back to standard pip.

### Changed

- **Piper CUDA default enabled** (`settings.rs`): `PiperConfig::cuda` default changed from `false` to `true`.
- **`setup_tts_venv.ps1`**: Install order corrected to match CopySpeak approach — piper-tts first (pulls CPU onnxruntime), then uninstall CPU onnxruntime, install GPU onnxruntime, install `nvidia-*` CUDA DLL packages, with final force-reinstall override. Added uv auto-detection + `ensurepip` fallback for uv-created venvs.
- **Brain warmup lifecycle** (`brain/manager.rs`, `llama_manager.rs`): Removed premature `brain:llama-ready` from early-return paths in `ensure_server_running()`. Added second `brain:llama-loading` emission after server start so frontend stays on "loading" through warmup inference.
- **App window visibility** (`lib.rs`): Removed `app.get_webview_window("main").is_none()` guard. `win_builder.build()` failure is now non-fatal (log + continue). Window starts `.visible(true)` then conditionally `.hide()` if `start_hidden` + tray available.
- **Loading animation** (`HerLoading.tsx`): Interactive press-and-hold with correct 240-frame easing curve. `step += 1` on hold, `step -= 4` on release. Centered canvas. Click or any keypress triggers full animation.
- **Test panel metrics** (`BrainSettings.tsx`): Added `done` flag + 100ms wait after `brainAsk()` before `unlisten()` to capture `brain:done` event from Tauri 2's post-command queue.
- **Llama MTP log** (`llama_manager.rs`): Removed misleading `(n=13)` from startup log — actual value is `--spec-draft-n-max 2`.
- **Vision Component UI labels** (`BrainSettings.tsx`, `PostProcessingSettings.tsx`): Changed from `"Enabled (mmproj-F16)"` to `"Disabled by default"`.
- **Regenerated TypeScript bindings** (`src/bindings.ts`): After `warmup_prompt` field addition.

### Fixed

- **App invisibility on start-hidden**: Window starts visible then hides — avoids unreliable `.show()` on never-visible Windows windows.
- **Premature `brain:llama-ready`**: Only fires after warmup inference completes, not when server process starts responding.
- **Test panel metrics race**: `unlisten()` called too early dropped the listener before `brain:done` event processed, causing client-side fallback estimation.

## [0.1.2] — 2026-06-20: Storage Encryption & Security Improvements

> **Storage Encryption & Security Improvements.** Added transparent at-rest encryption for settings credentials and the transcription history database using the OS-native keyring with robust fallback to a local master key file.

### Added

- **OS Keyring Integration** (`crypto.rs`): Added `keyring`, `aes-gcm`, and `aead` crates to secure the master key via Windows Credential Manager / macOS Keychain / Linux Secret Service.
- **Robust Key Fallback** (`crypto.rs`): Automatically falls back to a user-restricted local file (`.master_key`) in the app data directory if OS keyring access fails or is blocked, ensuring no-crash startup.
- **Transparent Settings Encryption** (`settings.rs`): Added transparent at-rest AES-256-GCM encryption for critical API keys (OpenAI, ElevenLabs, Cartesia, and post-processing/brain providers) when persisting to `settings_store.json`. Exposes decrypted plaintext in-memory for frontend IPC communication.
- **Database transcription text encryption** (`history.rs`): Transparently encrypts `transcription_text` and `post_processed_text` columns using AES-256-GCM before writing to `history.db`, and decrypts them on SQL queries.

## [0.1.1] — 2026-06-19: Parler Integration & CUDA Fix

> **Parler fork feature port.** Integrated post-process actions system (LLMModel + PostProcessAction CRUD), action-based shortcut bindings (`ppa_<id>`), trigger key selection (1-9), history action re-processing, and action icon system (28 Lucide icons). Backend: settings migration, shortcut scoping, WAL journal mode, batch DB deletes, startup timeout. Frontend: action management dialog, reusable Dialog component, macOS permissions helper.

### Breaking / Notable

- **Rust edition bumped to 2021** (was implicit). MSRV remains 1.87.
- Database migration to version 9 (WAL journal mode + indexes). Rollback-safe.

### Added

- **Post-process actions system** (`settings.rs`): `LLMModel`, `PostProcessAction` structs with `ensure_post_process_actions()` migration from legacy prompts. Gemini provider added to defaults.
- **Action CRUD commands** (`shortcut/mod.rs`): `add/update/delete_llm_model`, `add/update/delete_post_process_action` with `validate_trigger_key()`.
- **Per-action shortcut bindings** (`transcription_coordinator.rs`): `ppa_<actionId>` prefix, `action_map_key()`, action pre-selection from per-action shortcuts, `emit_action_selected/deselected()` events.
- **`run_post_process_action()`** (`actions.rs`): Resolves saved LLM model for an action, falls back to legacy config.
- **`apply_action_to_history_entry()`** (`commands/history.rs`): Re-run any action on any history entry, persists processed text.
- **Action icon system** (`src/lib/constants/actionIcons.tsx`): 28 Lucide icons with `getActionIcon()` picker.
- **Action management UI** (`PostProcessActions.tsx`): Dialog-based create/edit, icon picker grid, model selector, trigger key dropdown, per-action shortcut binding.
- **`Dialog.tsx`** (`src/components/ui/`): Reusable modal component with Escape-to-close.
- **macOS permissions helper** (`src/lib/permissions.ts`): Polling-safe `checkMacOSAccessibilityReady()` avoids spamming TCC prompts.
- **Settings backfill on every `get_settings()`** — new default bindings appear automatically.
- **`#[allow(dead_code)]` annotations** on intentionally unused functions.

### Changed

- **llama-server config** (`llama_manager.rs`): `-c 16384` (was 4096), `--spec-draft-n-max 2`, `-ctk f16 -ctv f16`, `LLAMA_ATTN_ROT_DISABLE=1`, `--mmproj` conditional on multimodal, `-ngl -1` for CUDA builds. 90s startup timeout.
- **`ActiveActionState`** from `Option<u8>` to `Option<String>` — carries action ID instead of key number.
- **`Stage::Recording`** now holds `selected_action: Option<String>` for action-aware pipeline.
- **Shortcut bindings**: `pause`, `show_history`, `copy_latest_history` added to defaults.
- **Settings merge**: `load_or_create_app_settings()` and `get_settings()` both call `ensure_post_process_actions()` + `ensure_action_bindings()`.
- **`settingsStore.ts` initialize guard**: Flag set eagerly before first await.

### Fixed

- **CUDA not activating**: `has_gpu_support()` now verifies server binary path contains `cuda` or `vulkan` before enabling GPU offload.
- **Duplicate main window crash** (`lib.rs`): `get_webview_window("main").is_none()` guard prevents double-build during `applicationDidFinishLaunching`.
- **History DB**: WAL journal mode, `synchronous = NORMAL`, batch DELETE in single transaction (one fsync instead of one per row), `cleanup_old_entries()` moved after event emit, indexes on `(saved, timestamp)` and `(timestamp)`.
- **Libclang download** (`scripts/download-libclang.ps1`): Guided helper for bindgen dependency.

### Infrastructure

- `scripts/download-libclang.ps1` — downloads/extracts `libclang.dll` for `whisper-rs-sys` bindgen.
- `AGENTS.md` / `BUILD.md` — Windows fresh-machine setup with Python 3.12 + libclang instructions.

## [Unreleased] — S2B2S v0.1.0 (Conversation Evolution)

> **Status (June 14, 2026):** 8 TTS backends (5 local, 3 cloud) with RAM-persistent warm model lifecycle (WarmEngine trait is implemented by local backends, direct-managed in orchestrator), voice barge-in for natural conversation interruption, Pocket TTS voice cloning, sentence streaming with word-count fallback, project-local Python venv, Android companion roadmap, system RAM/VRAM footer indicators, pre-compiled llama.cpp CUDA/Vulkan/CPU server with GPU offloading and MTP speculative decoding (n=13, ~216 tok/s), multimodal brain pipeline (native audio + image input via Gemma 4), 10 LLM providers, 9 STT engine types, brain overlay with 3D avatar (8-phase state machine), GPU overlay cursor trail physics, and 20-turn conversation memory.

### Android Port Plan & Streaming Improvement Analysis (June 19, 2026)

Two major planning documents laying the groundwork for the next evolution of S2B2S: a full on-device Android port and a deep-dive analysis of the streaming STT/TTS subsystems.

**`android-port-plan.md`** — Comprehensive architecture & implementation plan for porting S2B2S to Android as a fully on-device STT → Brain → TTS app (not a thin client). Covers: why sherpa-onnx is the single backbone for speech (Kokoro, Piper, streaming ASR, VAD, KWS), llama.cpp for the Brain with model-format continuity from desktop (Gemma 4 E2B QAT GGUF), native Kotlin + Jetpack Compose as the recommended shell, the eSpeak/GPLv3 licensing trap and NekoSpeak's Misaki G2P workaround, a 5-phase roadmap (foundation spike → dictation → read-aloud → conversation loop → polish), a hardware-tuned model strategy, and a full survey of 20+ reference projects (NekoSpeak, SherpaTTS, VoxSherpa, soniqo/speech-core, Kokoro-82M-Android, MediaPipe, llama.cpp Android).

**`improvement-plan.md` revised** — Full codebase re-audit against current `main` (2026-06-19). Confirmed all 7 original findings (F1–F7) remain accurate and none of the P0/P1/P2 recommendations have been implemented. One partial exception: the manual ONNX streaming decoder fix (chunk-boundary token corruption) has been applied. Added 2 new findings from the re-audit: **F8** — `fragment_queue.rs` is 306 lines of fully dead code (`#![allow(dead_code)]`, never imported), and 25 `#[allow(dead_code)]` annotations across 10 TTS files mask maintenance burden; **F9** — `control_server.rs` is a hand-rolled TCP/HTTP loop (5 endpoints, no WebSocket, no auth, fire-and-forget Brain) that needs hardening for any remote/hybrid mode. Added 4 new P3 recommendations: dead code cleanup (P3-10/11), control_server hardening (P3-12), and document consolidation with `futuristic_analysis/` (P3-13). Expanded risk section with dead code rot, control_server fragility, and document fragmentation. Updated evidence map with 9 new source references. Added status update section tracking what's been done and what hasn't.

**`reference_links.md`** — Curated, descriptive reference of 70+ open-source projects across 16 categories relevant to S2B2S: S2B2S core & NairoDorian's own projects, STT/ASR desktop apps (Handy lineage), STT engines & libraries, TTS engines/models/apps, combined voice assistants (STT+Brain+TTS), full-duplex speech-to-speech models, local LLM runtimes, on-device Android LLM inference engines (Llamatik, llmedge, SmolChat, ToolNeuron, PolyEngineInfer, MNN, MLC LLM, ExecuTorch), Google AI Edge stack (LiteRT-LM, LiteRT, Gallery, react-native-litert-lm), NPU/low-bit engines (mllm, PowerInfer, T-MAC, ORT GenAI, ORT QNN), Android STT keyboards, Android TTS engines (SherpaTTS, NekoSpeak, maise, speech-android, VoxSherpa, Kokoro-82M-Android, pocket-tts-unity), Android voice assistants (Box, Open-LLM-VTuber), cross-platform voice I/O studios, upstream models & shared dependencies (piper, chatterbox, vosk, NeMo, speech-core, ggml, unsloth), and curated "awesome" lists.

**`reference_github_links.md`** — Expanded with ANDROID SECTION (Android TTS Engines, Voice Assistants, Sherpa-onnx, Desktop Integration) and RECENTLY ADDED projects (Llamatik, llmedge, maise, LiteRT-LM, Box, react-native-litert-lm, mllm, ExecuTorch, onnxruntime-qnn).

**Documentation consistency audit (June 19, 2026)** — Cross-referenced all 55 markdown files across the project against actual source code (105 Rust files, 113 TSX files, 30 TS files, verified config values from `package.json`/`Cargo.toml`/`tauri.conf.json`). Fixed 15+ inconsistencies across 7 core docs:

- **AGENTS.md** — Fixed STT row in Technology Stack (was missing sherpa-onnx/Nemotron/Canary/Cohere/SenseVoice/GigaAM, now lists 9 engine types, 11 variants); fixed "Rust nightly" → "Rust stable, MSRV 1.87"; clarified `local_tts_server.rs` phantom reference in Application Flow; removed stale `lib/types.ts` entry from frontend tree; added `android-port-plan.md`, `improvement-plan.md`, `reference_links.md`, `reference_github_links.md` to Key Files Reference; marked `S2B2S_ANDROID_COMPANION.md` as superseded
- **README.md** — Fixed Parakeet V3 size (~0.6 GB → ~478 MB matching actual model registry); fixed component count (90+ → 110+ matching 113 actual TSX files); fixed TTS Backend trait count (7+ → 8 engines)
- **S2B2S_REVIEW.md** — Fixed Silero VAD size (~5 MB → ~1.7 MB); fixed Parakeet V3 size (~456 MB → 478 MB matching models table); fixed Whisper Small size (~465 MB → 487 MB matching models table); fixed `handy_keys.rs` → `tauri_impl.rs` in architecture diagram; fixed self-contradictory TTS telemetry status (now "⚠️ Partial" reflecting actual state)
- **BUILD.md** — Fixed duplicate heading number (5 → 6 for Build for Production section)
- **CRUSH.md** — Added `android-port-plan.md`, `improvement-plan.md`, `reference_links.md`, `reference_github_links.md` to file structure reference
- **CLAUDE.md** — Added new planning docs to evolution/planning references
- **S2B2S_ANDROID_COMPANION.md** — Added superseded notice pointing to `android-port-plan.md` for the full on-device approach

**`android-port-plan.md` revised** — Cross-referenced against `reference_links.md` (70+ projects across 16 categories). Key corrections: (1) Replaced deprecated **MediaPipe LLM Inference** with **LiteRT-LM** (Google's recommended successor) throughout; (2) Replaced ambiguous "llama.rn"/"llama.android" references with concrete packages (**Llamatik** KMP library, **llmedge**); (3) Added **transcribe-rs** Android STT continuity path via `notune/android_transcribe_app`; (4) Fixed React Native #2 option to include Brain layer references (`react-native-litert-lm`, `react-native-executorch`); (5) Added **OpenPhonemizer** as G2P option (D) in licensing section; (6) Added **Qwen3-ASR** to STT model strategy table; (7) Expanded reference survey with 15+ missing projects (Maise, soniqo/speech-android, ToolNeuron, PolyEngineInfer, MNN/TaoAvatar, google-ai-edge/gallery, FUTO Keyboard, Transcribro, local-whisper, Open-LLM-VTuber, Box, CrispASR, kokoro-onnx); (8) Corrected NPU landscape from "still experimental" to nuanced (production NPU via LiteRT-LM / ONNX Runtime QNN EP / mllm vs experimental llama.cpp NPU); (9) Added Google 2026/2027 developer-identity deadline risk and distribution channel diversity (F-Droid, Obtainium, Accrescent) to open decisions.

### STT Streaming & Parakeet Accuracy (June 15, 2026)

Cross-checked S2B2S's hand-rolled Parakeet STT server against the proven `transcribe-rs` (Rust) and `sherpa-onnx` (C++) implementations and NeMo's own preprocessor, fixed the divergences, and finished the deferred streaming-correctness cluster.

**Parakeet feature extraction (accuracy).** The manual Python path (eschmidbauer "parakeet-unified" + ysdede "EOU" models) computed mel features that didn't match what those models were trained on:

- **Power spectrum** — used `|FFT|` (magnitude); NeMo's `FilterbankFeatures` uses `|FFT|²` (`mag_power=2.0`), which distorted every mel value.
- **Analysis window** — applied a 512-sample Hann over the whole FFT frame; NeMo uses a **400-sample** (25 ms) Hann centered in the 512-point FFT (the unused `WIN_LENGTH=400` constant showed it was intended but never wired).
- **STFT padding** — `reflect` → `constant` (zero), matching NeMo's `center=True, pad_mode="constant"`.
- **Validated**: the corrected features now correlate **0.99972** with NeMo's preprocessor on real speech (the JFK sample), up from **0.925** — a measurable accuracy gain for those models. (The sherpa-onnx path — Nemotron and the self-exported streaming model — uses sherpa's own feature extraction and was already correct.)

**Parakeet greedy decoder.** Confirmed algorithmically correct vs both references (the "unified" model is plain RNN-T, not TDT). Hardened it anyway: argmax now runs over the in-vocab logits only (`[:vocab_size]`) so a future TDT export can't mis-read duration logits as phantom tokens. Corrected a misleading `targets` dtype comment (every shipped model uses int32, not float32).

**Streaming correctness (the deferred cluster).**

- **Continuous-voice barge-in race** — `is_playing()` was checked right after `ask()` returned, but TTS synthesis is async so it read false and the wait/barge-in block was skipped, making the assistant listen over its own speech. It now waits for the turn's terminal TTS event, which is race-free.
- **Streaming RMS gate** — it skipped any chunk shorter than `CHUNK/4`, so the final (short) chunk of every utterance was dropped, truncating transcripts. Now only near-silent _middle_ chunks are skipped; the final chunk is always fed.
- **EOU streaming decoder** — the server re-encoded the full buffer each chunk but continued the decoder state from the previous pass, corrupting tokens at chunk boundaries. It now decodes from frame 0 with a fresh predictor state, and the final result prefers whichever of the last partial / `stream_end` flush is longer.

**UI.** Hid the **WgpuTrail** settings panel from the normal sidebar (gated behind debug mode) — it persisted config the backend can't yet render.

### Correctness & Concurrency Hardening (June 15, 2026)

A second, agent-assisted bug-hunt across the voice pipeline. Verified with `cargo check` (clean), 153 backend unit tests, and a clean frontend `tsc`.

**Crash / hang / leak fixes**

- **Fixed a UTF-8 panic in custom-word post-processing** — `extract_punctuation` sliced strings by a character count used as a byte index, panicking on multi-byte edge punctuation (`¿`, `—`, CJK brackets). Now byte-offset based.
- **Silero VAD now resets between recordings** — `SileroVad::reset()` was the trait's no-op, leaving stale LSTM state that biased the first ~100–300 ms of each new utterance. It now clears the recurrent state.
- **Local TTS servers no longer hang the synth worker on a failed start** — a launch failure left the engine slot in `Starting` forever and `ensure_running` busy-waited indefinitely. Added a `Failed` state + drop-guard so the caller returns an error (and can retry).
- **Guarded llama-server startup against duplicate spawns / child leaks** — concurrent callers (warmup, `brain_ask`, model fetch, the converse shortcut) check-then-spawned with no lock. Added an await-safe `start_lock` mutex with a double-checked port probe.
- **Added an HTTP timeout to the post-processing client** (`llm_client`) so a hung request can't block a command forever.

**Correctness fixes**

- **`is_model_loading` was inverted** — it returned `current_model.is_none()` (reporting "loading" while idle, "ready" mid-load). Now returns the real in-progress flag.
- **TTS engine/voice/speed changes between Brain turns are honored** — `begin_session` now drops the lazy sentence consumer (it previously reused a stale backend/voice/speed).
- **`concatenate_wavs` parses the RIFF chunk list** instead of assuming a fixed 44-byte header, so non-canonical WAVs (e.g. a float Kokoro WAV with `fact`/`PEAK` chunks) are no longer spliced with their metadata as audio in history saves.
- Removed a dead duplicate `Telemetry` registration; fixed the unmatchable `vs.` abbreviation check in pagination; gave ffmpeg conversion unique temp filenames (concurrent-conversion clobber).

**Frontend**

- **Stopped leaking event listeners** — `settingsStore.initialize()` had no guard, so every mounting component registered another `model-state-changed` listener. Added an `initialized` flag.
- **Fixed two binary sliders** — Volume and Word-Correction-Threshold (0–1 range with no `step`) could only snap to 0 or 1; both now use a fractional step.
- `GlobalShortcutInput` sorts a copy instead of mutating React state in place; the Conversation read-aloud toggle re-syncs once settings finish loading (it was stuck on the default).

### STT / TTS / Brain Correctness Pass (June 15, 2026)

A focused bug-fix sweep across the voice pipeline. Verified with `cargo check` (clean, no new warnings) and the 57-test sanitization suite (all passing).

**STT — model registry & loading**

- **Fixed Whisper "small" downloading the wrong model** — its registry entry carried Parakeet `hf_repo`/`hf_files` (a copy-paste), and since the HuggingFace path takes precedence it fetched Parakeet ONNX files into a `ggml-small.bin` directory, producing an unloadable model. Cleared the stray fields so it downloads the GGML file via its `url`.
- **Fixed Nemotron 3.5 ASR being un-downloadable** — `hf_files` requested bare `encoder.onnx`/`decoder.onnx`/`joiner.onnx`, but the sherpa repo ships only `*.int8.onnx`, so every file 404'd. Corrected to the int8 filenames (the Python sherpa loader already prefers them).
- **Bundled the Python STT/TTS servers for packaged builds** — `unified_parakeet_server.py`, `kokoro_server.py`, `kitten_server.py`, and `pocket_server.py` are now listed in `tauri.conf.json` resources. `resolve_server_script` in `unified_parakeet.rs` had duplicated dead code (it re-checked `dev_path` instead of `bundled_path`) which was removed, and both server resolvers gained resource-dir / macOS-`Resources` fallbacks.
- **Multi-STT no longer discards transcripts** — when post-processing has no LLM merge, `transcribe_parallel` now returns the longest transcript (the best proxy for "most complete") instead of silently dropping all but the first model's output.
- **Verified the self-exported `parakeet-unified-en-0.6b-sherpa-streaming` model** — confirmed correct end-to-end (the JFK sample transcribes accurately through sherpa-onnx 1.13.2's native `nemo_parakeet_unified_streaming` buffered-streaming support). Documented that sherpa reads `feat_dim=128` from the ONNX metadata, so the shared loader's literal `feature_dim=80` is safely overridden — and must **not** be pinned to 128, which would break the 80-dim Nemotron model. It is now registered only when its files are present, since it has no download source.

**TTS — backends**

- **Fixed Cartesia producing garbled/no audio** — it sent `pcm_f32le` inside a `wav` container that the app's WAV parser rejects; it now sends `pcm_s16le` (format-1 WAV). Added `connect_timeout`/`timeout` so a hung request can't wedge the synthesis worker.
- **Added request timeouts to OpenAI & ElevenLabs** — both already pooled their HTTP clients but had no deadline; a stalled connection now fails within 120s instead of blocking indefinitely.
- **Speed control now works on ElevenLabs and Kokoro** — ElevenLabs sends `voice_settings.speed` (clamped to its 0.7–1.2 range, omitted when default); Kokoro sends `length_scale = 1/speed` (its server already honored it). With Piper/SAPI/OpenAI, 5 of 8 engines now apply speed.
- **Fixed Pocket voice cloning (cloned voices never played)** — the client sent the cloned voice's file _path_ as the voice id, which the server rejected and replaced with a stock voice. The client now uses the WAV stem as the id and resolves it to an absolute path sent as `voice_wav`; the server clones via `get_state_for_audio_prompt(path)` and caches voice states so repeats stay fast.
- **Hid the speed slider for engines that ignore it** — Kitten and Pocket have no speed control in their models, and Cartesia isn't plumbed, so the speed slider (and greeting-speed slider) are hidden for those engines to avoid a dead control.

**TTS — text sanitization**

- **Markdown→speech no longer corrupts code** — emphasis stripping is now word-boundary aware, so `snake_case`, `__dunder__`, and `a * b` survive (previously a blunt `replace(['*','_'], "")` mangled them). Added a table-flattening pass that turns `| a | b |` rows into "a, b." and drops `|---|` separators.
- **Normalizer no longer mangles paths/acronyms** — `wordA/wordB → "wordA or wordB"` now skips all-caps acronyms (TCP/IP, I/O) and path chains (`src/main/mod`); unit ratios (`km/h`) are expanded before generic slash-options so they read "kilometers per hour" instead of "km or h".

**Brain**

- **Verified the multimodal pipeline is correct** — the OpenAI-compatible `MessageContent`/`ContentPart` serialization (text / image_url / input_audio), SSE chunk-boundary buffering, and streaming sentence splitter were audited and confirmed correct; no changes required.

### TTS Pipeline Fixes & Telemetry Integration

- **Fixed Text Sanitization/Normalization Pipeline** — Swapped pipeline execution order so that regex-based sanitization and web artifact cleanup (`sanitize_tts`) run _before_ NeMo-based normalization (`tn_normalize_text`). This prevents NeMo from getting confused by URLs/emails and spelling out entire sentences letter-by-letter, and preserves title capitalization ("Doctor"/"Professor"). Made `test_dates` case-insensitive.
- **Wired TTS Performance Telemetry** — Instantiated the `Telemetry` subsystem as a managed Tauri state and wired it into both the `speak` and `speak_sentence` synthesis loops to track real-time character-per-millisecond metrics.
- **Enabled Adaptive Fragment Sizing** — Configured `TtsManager::speak` to query the telemetry store and dynamically adjust text fragment pagination sizes based on the performance speed of the active engine/voice.

### Documentation & Code Review Synchronization

- **Synchronized all project documentation** — Audited and updated `S2B2S_REVIEW.md`, `README.md`, `repomix-file-descriptions.md`, `AGENTS.md`, `BUILD.md`, `CONTRIBUTING.md`, `CHANGELOG.md`, `LLAMA_CPP.md`, and all 8 reference project reviews in `references_comparative_analysis_md/`.
- **Corrected TTS Backend Count** — Standardized backend count to 8 (5 local, 3 cloud: Piper, Kokoro, Kitten, Pocket, SAPI, OpenAI, ElevenLabs, Cartesia) across all documents, correcting historical references that erroneously counted `piper_server` as a 9th backend.

### Windows SAPI Fallback TTS Backend

- **Implemented Local Windows SAPI Synthesis** — Completed the local Windows SAPI (Speech API) fallback TTS backend in `sapi.rs`, replacing the previous stub.
- **COM Interface Integration** — Utilized raw `windows-rs` COM interop to create memory streams (`CreateStreamOnHGlobal`), bind them to SAPI stream objects (`ISpStream`) with the correct format GUID (`C31ADBAE-527F-4FF5-A230-F62BB61FF70C`), select voice tokens, and speak text synchronously.
- **WAV Encapsulation** — Implemented an in-memory `pcm_to_wav` helper to wrap the raw synthesized PCM bytes from SAPI into standard 44-byte WAV container bytes.
- **Unit Testing** — Created and verified backend test cases ensuring SAPI voice lists and text synthesis correctly return valid WAV file data starting with the `RIFF` header.
- **Clarified `WarmEngine` Trait Status** — Corrected outdated claims that `WarmEngine` was dead code. Documented that it is fully implemented in `PiperBackend`, `KokoroBackend`, `KittenBackend`, and `PocketBackend`, but noted that the orchestrator layer currently calls lower-level lifecycle managers directly.
- **Expanded STT Subsystem Documentation** — Added Canary, Cohere, SenseVoice, GigaAM, and Nemotron/UnifiedParakeet to the STT comparison tables and implementation descriptions (now documenting 9 STT engine types and 11 model variants).
- **Added missing code mappings** — Documented `commands/system.rs` (cross-platform RAM retrieval command) and `overlay_fx/native/shader.wgsl` (WGPU trail shader).

### Startup Optimization — Parallel Model Loading, No Timeouts

- **All AI models load in parallel at startup** — Brain (LLM), STT, and TTS engines now fire simultaneously in 3 independent threads with no ordering dependencies. Brain warmup starts immediately as the highest-priority thread.
- **Removed all model loading timeouts** — All poll loops for server readiness (Piper, Kokoro/Kitten/Pocket, llama.cpp, unified parakeet) now run indefinitely until the child process exits or health check succeeds, instead of aborting after arbitrary deadlines (15s–65s). Periodic log messages report elapsed time every 10s.
- **Removed hardcoded 500ms sleep** — The artificial delay between TTS/STT load completion and brain warmup is gone. Brain starts warming up concurrently with other models.
- **Removed blocking `join()` calls** — TTS and STT startup threads no longer block the brain warmup thread. All three run independently.
- **Specific timeouts removed**:
  - `piper_server.rs`: 60s/15s (CUDA/CPU) health poll timeout + 65s caller-side timeout
  - `local_tts_server.rs`: 30s health poll timeout + 65s caller-side timeout
  - `llama_manager.rs`: 60s startup readiness timeout → replaced with child-exit detection
  - `unified_parakeet.rs`: 60s health check timeout → replaced with child-exit detection + periodic logging

### Reference GitHub Links

- **`reference_github_links.md`** — Curated list of 23 STT, TTS, and voice-related open-source projects referenced by the S2B2S ecosystem. Includes Handy, Parler, AIVORelay, Parakeet, TranscriptionSuite, Whispering, speech-recognition (asrjs), transcribe-rs (cjpais), RealtimeSTT, onnx-asr, sherpa-onnx, speechbrain, copyspeak-tts, parrot, vox, pocket-tts-server, voirs, vibevoice-rs, TTS-Audio-Suite, voicebox, Cross_Platform_Rust_WebGPU_CursorFX, LocalAI, and TD_Web_Trail.

### Gemma 4 Reference Docs & Multimodal Brain Pipeline

- **`gemma_4_qat_mtp_e2b/`** — Reference documentation for the Gemma 4 E2B brain model: MTP speculative decoding benchmarks (n=1..32, **n=13 peak at 216 tok/s**), attention rotation/KV cache optimization, multimodal API formats (image/audio input), and optimal llama.cpp server launch commands.
- **`references_comparative_analysis_md/`** — Comparative analysis and individual reviews for all 22 projects in the S2B2S ecosystem, plus architecture pattern catalog, fork lineage, and license compatibility matrix.

#### Llama.cpp Brain Optimizations

- **MTP speculative decoding tuned to `n=13`** — From `--spec-draft-n-max 2` → `13` based on triple-validated benchmarks (3 sweeps × 21 runs each). Steady-state throughput at ~216 tok/s on RTX 4070 Laptop 8GB, up from ~170 tok/s.
- **Switched `--chat-template-kwargs` → `--reasoning off`** — Modern llama.cpp flag replacing deprecated template kwargs.
- **Conditional `--mmproj` loading** — Multimodal projector (CLIP, ~940 MB) only loaded when audio or image multimodal toggles are enabled. Saves ~1150 MiB VRAM for text-only brain use.

#### Multimodal Brain Pipeline (Audio + Image)

- **`BrainConfig.multimodal_audio_enabled`** — When active, raw WAV recording is base64-encoded and sent as `input_audio` alongside the text transcription. Gemma 4 performs its own native STT as an additional transcription pass.
- **`BrainConfig.multimodal_image_enabled`** — Prepares the pipeline for screenshot/image input; images sent as `image_url` before text content (Gemma 4 best practice).
- **`MessageContent` enum** — Brain client now supports OpenAI-compatible multimodal content arrays (`text`, `image_url`, `input_audio` parts) alongside plain string content.
- **`BrainManager.ask_multimodal()`** — Extended `ask()` with optional `audio_wav_base64` and `image_png_base64` parameters. Content parts ordered image → text → audio per Gemma 4 best practices.
- **`encode_wav_bytes()`** — New in-memory WAV encoder in `audio_toolkit` for zero-disk multimodal audio path.
- **Frontend toggles** — "Multimodal Input (Gemma 4)" settings group with Audio/Image toggle switches in BrainSettings, visible only for llama_cpp provider.

### Repomix Codebase Packaging

- **`repomix.config.json`** — Repomix bundler configuration for producing single-file codebase snapshots with file descriptions.
- **`scripts/repomix-with-descriptions.ts`** — TypeScript script generating annotated Repomix outputs with per-file commentary.
- **`S2B2S_repomix.txt`** / **`S2B2S_repomix_annotated.txt`** — Full and annotated Repomix codebase snapshots for AI context ingestion.
- **`repomix-file-descriptions.md`** — Per-file descriptions of the entire S2B2S codebase (348+ documented files).

### WGPU Native Overlay Shader

- **`overlay_fx/native/mod.rs`** — New platform-native overlay module with WGPU render pipeline initialization, surface setup, and platform-agnostic shader compilation.
- **`overlay_fx/native/shader.wgsl`** — WGSL compute/render shader for GPU-accelerated cursor trail effects (spring-friction physics, bloom glow, click ripple).

### Brain Overlay 3D Avatar

- **`brain-overlay/avatar/Avatar3D.tsx`** — 3D avatar component using Three.js with phase-reactive animations (Idle/Listening/Thinking/Speaking), orbital particle effects, and GPU-accelerated rendering.

### GPU/VRAM Monitor

- **`src/components/settings/models/GpuVramMonitor.tsx`** — Footer component displaying real-time GPU VRAM usage from llama.cpp server, with progress bar and memory pressure color coding.

### Docs & Housekeeping

- **AGENTS.md, README.md, BUILD.md, CLAUDE.md, CONTRIBUTING.md, CRUSH.md, S2B2S_REVIEW.md** — Updated cross-references, build instructions, and contributor guidance.
- **sponsor-images/** — Removed deprecated sponsor image assets.
- **models/TTS/** — Added Kitten TTS nano 0.8 pre-downloaded model files for offline first-run.

### Overlay Architecture — Tauri/OS-Native Toggle + WGPU Trail Foundation

- **`OverlayMode` toggle** — Settings → Overlay Window now lets users switch between `Tauri` mode (CopySpeak HUD style — `always_on_top` + `transparent` only) and `OsNative` mode (Handy style — NSPanel on macOS, Win32 `HWND_TOPMOST` on Windows, GTK layer-shell on Linux). Both modes share the same window label and event bus.
- **`OverlayWindowConfig`** — New settings struct controlling mode, position, width/height, opacity, corner radius, reply bubble toggle, and fade-out duration. Persisted via `tauri-plugin-store` with serde defaults.
- **`WgpuTrailConfig`** — New settings struct for the native GPU cursor trail: segments, spring stiffness (0.39), friction (0.5), width taper exponent (1.5), glow opacity, lazy-brush radius/friction, click ripple toggle. All with TD_Web_Trail–derived defaults.
- **`overlay.rs` refactored** — `create_recording_overlay` (both macOS and non-macOS variants) now respects `OverlayMode`. In Tauri mode: macOS skips NSPanel and uses plain `WebviewWindowBuilder`; Linux skips GTK layer-shell init; Windows skips `force_overlay_topmost()`. `calculate_overlay_position` uses configurable dimensions from `OverlayWindowConfig`. `hide_recording_overlay` uses configurable `fade_ms`.
- **`overlay_fx/` Rust module** — New crate-internal module at `src-tauri/src/overlay_fx/` containing:
  - `trail.rs` — Spring-friction chain physics engine + Catmull-Rom spline interpolation, ported from `TD_Web_Trail`. Lazy-brush dead-zone filter with non-linear damping. Trail system with idle-sleep after 2s of no movement. Includes unit tests.
  - `window.rs` — `brain_overlay` window creation (transparent, click-through `set_ignore_cursor_events`, always-on-top). Show/hide helpers. Windows re-asserts topmost via `SetWindowPos`.
  - `cursor_follow.rs` — ~30 Hz cursor polling loop using `enigo` (already a dependency). Quadrant-aware bubble positioning.
  - `placement.rs` — `compute_bubble_anchor()` with DPI scaling and monitor-aware quadrant flipping.
  - `events.rs` — `OverlayPhase` 8-state machine (Idle/Listening/Thinking/Seeing/Speaking/Done/Error/Hidden) + cursor/bubble payloads. All typed via specta.
  - `capabilities.rs` — Per-OS GPU/cursor/layer-shell capability probe.
  - `commands.rs` — 3 Tauri IPC commands: `overlay_fx_probe_capabilities`, `overlay_fx_show_conversation`, `overlay_fx_dismiss`.
- **`brain_overlay` frontend** — New multi-page Vite entry at `src/brain-overlay/`:
  - `index.html` — Transparent standalone HTML page.
  - `main.tsx` — React 19 root with i18n support.
  - `BrainOverlayApp.tsx` — State-driven UI: avatar placeholder (72px circle with phase-dependent emoji) + streaming reply bubble (glassmorphism with live cursor blink). Listens to `overlay:state`, `overlay:append`, `overlay:clear` events.
- **`vite.config.ts`** — Added `brain_overlay` entry to multi-page Rollup input.
- **Settings frontend tabs** — Two new sidebar tabs registered in `SECTIONS_CONFIG`:
  - **Overlay Window** (`Monitor` icon) — OverlayMode selector (Tauri/OS-Native dropdown) + Reply Bubble toggle.
  - **WGPU Trail** (`Zap` icon) — Enable toggle + Click Ripple toggle.
- **i18n keys** — 20+ new keys in `en/translation.json` under `settings.advanced.overlayWindow.*`, `settings.advanced.wgpuTrail.*`, and `sidebar.overlayWindow`/`sidebar.wgpuTrail`.
- **Typed bindings regenerated** — All new structs and commands exported to `src/bindings.ts` via `cargo test export_bindings`.

### Futuristic Analysis — Transparent Overlay Vision Documents

- **`futuristic_analysis/`** — 9 Markdown documents (00–08) analyzing the path from today's S2B2S to a full GPU transparent overlay with 3D avatar:
  - `00_README_START_HERE.md` — Master index, architecture overview, core principles.
  - `01_S2B2S_REVIEW.md` — Honest audit of current code: what works, what's missing.
  - `02_REFERENCE_PROJECTS.md` — Deep read of `TD_Web_Trail` and `Cross_Platform_Rust_WebGPU_CursorFX` (both cloned at `../` for live reference). Exact techniques to lift, including the DX12 OOM / Vulkan+N.VAPI fix.
  - `03_GPU_OVERLAY_ARCHITECTURE.md` — Two-track rendering (webview + native wgpu), per-OS technique matrix, DPI/click-through/perf budgets, failure ladder.
  - `04_CONVERSATION_MODE_2.md` — UX spec: state machine, event contract, reply bubble, quick actions, coexistence with recording pill.
  - `05_VISION_AND_SCREEN_UNDERSTANDING.md` — Screen capture (full + region), multimodal `ChatMessage` upgrade, cross-platform capture matrix, privacy invariants.
  - `06_AVATAR_SPEC.md` — 3D cybernetic avatar spec: 4 senses → pipeline signals map, Catmull-Rom visual language, skins system, reduced-motion accessibility.
  - `07_IMPLEMENTATION_ROADMAP.md` — 5-phase plan with file-level tasks, risk register, test matrix, performance targets.
  - `08_TRANSPARENT_OVERLAY_IMPL_PLAN.md` — Concrete code-level plan bridging analysis to actual patterns from the cloned reference repos.
- **Reference repos cloned** — `Cross_Platform_Rust_WebGPU_CursorFX` (Tauri V2 + wgpu transparent overlay, Vulkan + NVAPI fix) and `TD_Web_Trail` (spring-friction chain physics, 4-pass neon glow, Catmull-Rom splines, idle-sleep optimization) now live at the root `AZ/` directory alongside S2B2S.

### Model Directory Restructure (STT / Brain / TTS)

- **Master `models/` organization** — All model files now live in three category subdirectories: `models/STT/` (speech-to-text: Parakeet, Silero VAD, Whisper), `models/Brain/` (LLM: llama.cpp GGUF), `models/TTS/` (text-to-speech: Kokoro, Piper, Pocket, Kitten). Each engine has its own folder inside its category.
- **Rust path resolution updated** — `portable.rs` now exports `stt_models_dir()`, `brain_models_dir()`, `tts_models_dir()` helpers. `model.rs` uses STT subdir. `llama_manager.rs` uses Brain subdir. `kokoro.rs`, `piper_server.rs`, `pocket.rs`, `local_tts_server.rs` all use TTS subdir with legacy compat fallbacks.
- **Python server scripts updated** — `kokoro_server.py`, `kitten_server.py`, `pocket_server.py` now search `models/TTS/` first, with legacy `models/` flat directory as fallback for backward compatibility.
- **`models/download_models.*` rewritten** — Now accepts `--path` (custom target directory), `--model` flags (pocket, piper, kokoro, kitten, stt, brain, tts, all). Creates proper STT/Brain/TTS folder structure at target path. Supports `--setup-venv` and `--clean-venv` for one-shot setup. Dry-run mode available.
- **Pocket TTS model storage** — The `models/hub/` and `models/xet/` folders are HuggingFace auto-cache directories created by `pocket_tts`/`kittentts` Python packages when `HF_HOME` is set to `models/TTS/`. Pocket and Kitten models auto-download on first synthesis — no manual download needed.
- **`.gitignore` rewritten** — Per-category patterns for binary files. Only scripts in `models/` root are tracked in git.

### Dependency Checker & Script Cleanup

- **`scripts/check-deps.ts`** — New cross-dependency version checker covering Bun, Node, Rust, Cargo, Tauri CLI, JS packages (React, Vite, TypeScript, Tailwind, Zustand, etc.), and Python TTS packages (piper-tts, kokoro-tts, pocket-tts, kittentts, torch). Detects outdated deps and outputs a summary.
- **`scripts/check-deps.ts` overhauled** — Rust check now uses built-in `cargo update --dry-run --verbose` instead of external `cargo-outdated`, covering ALL Rust dependencies at ALL depths (including transitive). Distinguishes auto-updatable (`[Rust]`) from semver-constrained (`[Rust*]`) crates. JS check now parses `bun outdated` table output (the `--format json` flag is broken in Bun 1.3). Fixed Python pip version parser that captured trailing commas.
- **Dependency bumps** — JS: `@tailwindcss/vite` 4.3.0→4.3.1, `tailwindcss` 4.3.0→4.3.1, `lucide-react` 1.17.0→1.18.0, `eslint` 10.4.1→10.5.0. Rust: 15 crates updated in `Cargo.lock` (cc, js-sys, memchr, openssl, openssl-sys, rust_decimal, smallvec, wasip2, wasm-bindgen ×5, web-sys, zeroize). 5 transitive crates remain semver-constrained (cpal pinned for rodio compat, generic-array/toml family via GTK system-deps).
- **Removed unused scripts** — `check-latest.ts`, `find-bigints.ts`, `find-command-bigints.ts` removed (development-only tools, not referenced in CI or build). Kept `check-translations.ts`, `check-nix-deps.ts`, `setup_tts_venv.*`.

### About Tab — Expanded Acknowledgments

- **`AboutSettings.tsx` rewritten** — The Acknowledgments section now renders 6 comprehensive categories mapped from i18n keys, replacing the single Whisper.cpp entry. Data-driven component using `ACKNOWLEDGMENT_SECTIONS` array for easy future additions.
- **6 new acknowledgment sections** covering every project, model, library, and service used by S2B2S:
  - **Project Lineage & Forks** — Handy, AIVORelay, Parler, Parrot, CopySpeak, Vox, Parakeet-Realtime-Transcriber, TranscriptionSuite, Whispering. All 9 projects in the root AZ directory.
  - **STT Models** — Whisper.cpp, NVIDIA NeMo Parakeet, Silero VAD, Moonshine, Whisper, Breeze ASR, Canary, Sense Voice, Giga AM, Cohere.
  - **TTS Engines** — Piper, Kokoro, Kitten, Pocket + cloud (OpenAI, ElevenLabs, Cartesia).
  - **Brain & LLM** — llama.cpp, Gemma-4 + 8 cloud providers (OpenAI, Anthropic, Gemini, Groq, Cerebras, OpenRouter, Z.ai, AWS Bedrock).
  - **Key Rust Crates** — Tauri, rdev, vad-rs, transcribe-rs, nnnoiseless, cpal, rodio, rubato, text-processing-rs, enigo, rusqlite, specta.
  - **Python ML Ecosystem** — piper-tts, kokoro-tts, pocket-tts, kittentts, PyTorch, NumPy, SoundFile, ONNX Runtime.
- **All 20 language files synced** — New acknowledgment keys propagated to all locales; old `whisper`-only keys removed from non-English files.

### Model Path Consolidation (Local-First Storage)

- **`kokoro_server.py`** — Added `resolve_local_path()` fallback resolution. When `--model`/`--voices` args are missing, searches `models/kokoro/` relative to script dir and CWD before falling back to HuggingFace cache. Ensures the local `models/kokoro/kokoro-v1.0.onnx` + `voices-v1.0.bin` are always found.
- **`kitten_server.py`** — Added `--models-dir` argument and `resolve_local_models_dir()` helper. Points HuggingFace Hub downloads at the project-local `models/` directory instead of global HF cache.
- **`pocket_server.py`** — Same `--models-dir` + `resolve_local_models_dir()` pattern for local model storage. Sets `HF_HOME` for any HuggingFace-dependent packages.
- **Kokoro backend** (`kokoro.rs`) — Simplified `kokoro_model_args()` search order: canonical `models/kokoro/` path first, then CWD-based, then legacy. Removed hardcoded `%APPDATA%` path.
- **Model manager** (`model.rs`) — Added `project_local_models_dir` field that discovers `S2B2S/models/` in dev mode. `update_download_status()` and `get_model_path()` now resolve models from the local folder when they exist there.
- **Local TTS server** (`local_tts_server.rs`) — Added `resolve_local_models_dir()`, `local_models_dir_args()`, and sets `HF_HOME` env var on all spawned Python subprocesses. Kitten and Pocket engines now automatically receive `--models-dir` pointing to the local `models/` folder.

### Pocket TTS + Full Kokoro/Kitten Synthesis with RAM Persistency

- **Pocket TTS backend** — New `TtsEngine::Pocket` variant, `PocketBackend` (8 character voices: alba, marius, javert, etc.), dedicated `pocket_server.py` HTTP server for RAM-persistent runtime.
- **Kokoro synthesis** — Completed from skeleton. Now uses `kokoro_server.py` persistent HTTP server with `kokoro_tts` Python API (mode A) or CLI fallback (mode B). Model auto-discovery searches `models/kokoro/`, `CARGO_MANIFEST_DIR/kokoro/`, and common install paths.
- **Kitten synthesis** — Completed from skeleton. Now uses `kitten_server.py` persistent HTTP server with `kittentts` Python API. 8 voices: Bella, Jasper, Luna, Bruno, Rosie, Hugo, Kiki, Leo.
- **`local_tts_server.rs`** — Unified persistent HTTP server lifecycle for Kokoro, Kitten, Pocket (ported from CopySpeak `tts-perf-v2`). Per-engine state machine (Stopped→Starting→Ready), generation counter for safe abort, health polling with exponential backoff, warmup synthesis, idle watcher. Emits `local-tts-status-changed` Tauri events.
- **`WarmEngine` trait** — Implemented on `KokoroBackend`, `KittenBackend`, `PocketBackend` for app-startup pre-warming and engine-switch unload/reload.
- **Python encoding fix** — All local TTS server subprocesses now spawn with `PYTHONIOENCODING=utf-8` to prevent UnicodeEncodeError crashes.

### Conversation Memory & Brain Context

- **`context_turns` default changed to 20** — Brain now remembers 20 conversation turns by default (was 0 = stateless). Model receives full history of last N turns in context window.
- **"Clear" → "New conversation"** button with Eraser icon in ConversationView. Clears both Rust in-memory history and frontend messages.

### System RAM Footer Indicator

- **`get_system_ram` command** — New Rust backend command (`commands/system.rs`) using cross-platform system tools (PowerShell `Get-CimInstance` on Windows, `/proc/meminfo` on Linux, `sysctl`/`vm_stat` on macOS). Returns `total_mb`, `used_mb`, `free_mb`.
- **`RamFooterIndicator` component** — Shows real-time RAM usage percentage with green/yellow/red status dot and detailed hover tooltip. Updates every 5 seconds.

### Sentence Streaming (Fast TTFA)

- **3-fragment streaming pattern** — When `tts_shorten_first_chunk` is enabled: splits text at first period for fast first audio, splits second sentence for parallel synthesis, groups remaining text into one chunk. Replaced the old clause-boundary search with direct period-scanning.
- **Fast first sentence toggle** — New UI toggle in Speech Settings to enable/disable the streaming pattern.

### Engine Descriptions, Badges, Links & Test Button

- **Engine descriptions** — Each TTS engine now shows i18n description, badges (Offline/Free/Cloud/Paid/Freemium), and GitHub/API docs link in Speech Settings.
- **Test Engine button** — "Test Now" button in Speech Settings synthesizes a test phrase with the current engine.
- **Command Preview** — Local engines show their Python server command preview in a collapsible terminal block.
- **Footer engine list** — All 8 engines now visible in TtsSelector dropdown with per-engine status indicators.

### Shutdown & Process Cleanup

- **`Drop` impl on `LlamaManager`** — Ensures llama-server.exe is killed even on abnormal exit (taskbar close, panic, process kill).
- **`RunEvent::Exit` cleanup** — Now unloads Piper server, all local TTS servers (Kokoro/Kitten/Pocket), and llama-server on shutdown.
- **Model download resilience** — HTTP 416 Range Not Satisfiable on resume now auto-deletes stale partial file and restarts fresh.

### UI Fixes

- **Removed latency HUD** from ConversationView (endpoint/STT/token/audio labels above input box).
- **Fixed React hooks order crash** in `GpuVramMonitor` — `useTranslation()` moved before conditional return.
- **TTS synthesis ms in history** — `speak()` now passes `synth_total_ms` to `duration_ms` in `save_entry()`, visible as `{ms}ms` in History Settings.
- **Conversation TTS timing** — `speak_sentence()` now emits `tts:synth-done` with `ms` per sentence, showing `🔊 {ms}ms` on assistant messages.

### Vox-Inspired Improvements (Voice Barge-in, Word-Count Fallback, Voice Cloning)

- **Voice barge-in** — In continuous voice mode, VAD stays active during TTS playback. If user speaks, Brain is aborted, TTS is stopped, and the new utterance is captured immediately. Works like a real conversation — interrupt the assistant mid-sentence.
- **Word-count fallback** — Sentence streaming now splits at 12-word boundaries when no punctuation is found, preventing long run-on text from being synthesized as one chunk.
- **Pocket TTS voice cloning** — Import a 5-20 second WAV file and Pocket TTS clones that voice. `PocketBackend::import_cloned_voice()` copies WAV to persistent storage. Cloned voices appear in the voice list with 🎙️ prefix. New "Clone Voice" section in Speech Settings with WAV file picker.
- **Voice counts in footer** — Each engine in the TTS dropdown now shows voice count (e.g., "Kokoro — 54 voices").
- **Android Companion roadmap** — `S2B2S_ANDROID_COMPANION.md` with PWA architecture, 3-phase feature plan, WebSocket protocol design, references to 6 GitHub projects (NekoSpeak, speech-android, pocket-tts-unity, Kokoro-82M-Android, SherpaTTS, VoxSherpa-TTS), and brainstorm features.
- **Vox vs S2B2S comparison** — `S2B2S_VOX_COMPANION.md` with full architecture comparison, feature gap analysis, and improvement plan.

### Python Virtual Environment

- **`scripts/setup_tts_venv.ps1` / `.sh`** — Creates a project-local Python venv at `venv/` and installs all TTS dependencies: `piper-tts[http]`, `kokoro-tts`, `pocket-tts`, `kittentts`, `torch`, `numpy`, `soundfile`. No more system-wide `pip install`.
- **`resolve_venv_python()`** — New shared helper in `local_tts_server.rs`. Resolves Python from: project venv → app data venv → system fallback. Both Piper and local TTS servers use venv Python.
- **All model/voice paths now project-local** — Piper voices `~/piper-voices` fallback removed. Kokoro model search no longer scans `C:\Python3xx\Scripts`, `/usr/local/bin`, `/opt/kokoro-tts`. Everything resolves to `models/` subfolder or app data directory.
- **Removed dead code** — `resolve_python_command()` (now unused) and Python discovery helpers cleared from both server modules.

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

### Fixes

- **Removed `--flash-attn on`** from the llama-server launch command to resolve compatibility with the CUDA pre-built binary.
- **Llama.cpp tab deduplication** — Filtered out `cudart-*` asset variants so only CUDA 12.4, CUDA 13.3, Vulkan, and CPU (x64) options appear. CUDA version is embedded in the backend string (`cuda-12.4`, `cuda-13.3`) so both are distinct entries.
- **Download/Remove/Use buttons** — Each option now always shows a Use button (downloads then activates if not yet downloaded), with Download/Remove toggling based on existence. Zip files are deleted after extraction.
- **Brain disable kills server** — Toggling Brain off when `llama_cpp` provider is active now terminates the llama-server process immediately.

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

### Documentation Sync (June 2026)

- **LLAMA_CPP.md** — complete rewrite: removed all references to the old CMake-based `build_llama_cpp()` pipeline (removed in v0.1.0). Now documents the pre-compiled `LlamaServerManager` architecture with auto-download from GitHub releases, CUDA/Vulkan/CPU backend auto-detection, GPU VRAM offloading with `-ngl all`, and the Llama.cpp settings management tab.
- **AGENTS.md** — backend tree updated with missing modules: `llama_server/`, `brain/llama_manager.rs`, `commands/llama_server.rs`, `managers/continuous_voice.rs`, `managers/transcription_mock.rs`, `tts/status.rs`, `tts/telemetry.rs`, `tts/audio_format.rs`, `tts/backends/piper_server.rs`, `audio_toolkit/bin/`, `audio_toolkit/constants.rs`, `audio_toolkit/text.rs`. Frontend tree updated: `hooks/useLlamaState.ts`, `hooks/useProviderState.ts`, `lib/constants/`, `utils/`. Text normalization heading fixed: "4-pass" → "5-Stage".
- **README.md** — architecture diagram updated with `continuous_voice.rs`, `status.rs/telemetry.rs`, `llama_manager.rs`. "Why S2B2S?" and Default Stack table now list pre-compiled llama.cpp as the primary Brain option alongside Ollama/LM Studio.
- **S2B2S_REVIEW.md** — roadmap (section 19): added 7 completed features (pre-compiled llama.cpp server, settings tab, performance metrics, GPU VRAM indicator, log viewer console, footer status indicators, hands-free auto-listen). File tree (section 18): added `llama_server/`, `brain/llama_manager.rs`, `commands/llama_server.rs`.
- **BUILD.md** — project structure overview refreshed with `llama_server/`, `brain/` directories, restored `resources/`/`Cargo.toml`/`tauri.conf.json` entries.
- **CRUSH.md** — file structure reference updated with `llama.cpp/` and `llama_server/` references.
- **CONTRIBUTING.md** — managers listing updated to include `continuous_voice`.

### Second-Pass Verification & Corrections

- **README.md** — component count corrected: "100+ components" → "90+ components"; settings file count "~50 files" → "60+ files". Removed `tts-rs` from Core Libraries table (not a compiled dependency — Kokoro synthesis pending integration).
- **AGENTS.md** — Key Files Reference now includes `LLAMA_CPP.md`. Technology stack table: Kokoro entry updated from "tts-rs in-process" to "skeleton". Backend tree: kokoro.rs comment updated.
- **S2B2S_REVIEW.md** — hooks section now includes `useLlamaState.ts`. Kokoro Backend Details updated to note synthesis pending `tts-rs` integration (voice listing works). Platform matrix: Kokoro row updated. Key Files Quick Reference now includes `llama_server/manager.rs`.
- **Full file inventory verification** — confirmed 88 `.rs` files, 91 `.tsx` files, 28 `.ts` files, 9 GitHub Actions workflows. All documented modules verified on disk.

### Third-Pass Consistency Audit

- **README.md** — **critical fix**: text normalization pipeline order corrected from `ITN → Custom Words → TN → Markdown Strip` to `ITN → Custom Words → Markdown Strip → TN → Regex Cleanup` (TN must run after markdown stripping, not before). WarmEngine lifecycle expanded to include "Stopped" state (5 states, was showing 4). Roadmap: added missing "Hands-free auto-listen / continuous voice" (✅) and "Engine-switch cleanup" (now 🚧 In Progress, matching S2B2S_REVIEW.md).
- **CLAUDE.md** — backend summary now includes `llama_server/` directory.
- **BUILD.md** — hooks list updated to include `useLlamaState`.
- **CONTRIBUTING.md** — backend listing now includes `llama_server/` subsystem.
- Verified all 20 i18n locale files match CONTRIBUTING_TRANSLATIONS.md language table. Confirmed version consistency: package.json/Cargo.toml/tauri.conf.json all `0.1.0`. Tag `v0.1.0` created as initial versioning baseline.

### Code Cleanup

- **Cargo.toml cleanup** — removed commented-out `[[bin]]` section for CLI; removed trailing blank line before target-specific dependencies.
- **Module documentation** — added module-level doc comments (`//!`) to `actions.rs`, `input.rs`, `audio_feedback.rs`, `commands/mod.rs`, `helpers/clamshell.rs` for clearer codebase navigation.
- **Documentation sync** — updated roadmap in README.md to reflect all completed features (AI Replace, Latency HUD, wake word, save-to-file, waveform HUD, auto-discovery), removed stale entries.
- **README.md polish** — streamlined features list and added clarity to pipeline descriptions.

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

**STT — Python ONNX Runtime 1.26 Server:**

- **Parakeet Unified EN 0.6B model** — two new STT models: `parakeet-unified-en-0.6b-fp32` (2.4 GB, accuracy 0.88) and `parakeet-unified-en-0.6b-int8` (633 MB, accuracy 0.85). Both English-only RNN-T architecture from NVIDIA NeMo, registered in `ModelManager`.
- **Python onnxruntime 1.26 server** (`unified_parakeet_server.py`) — dedicated HTTP server for Parakeet Unified inference using the latest ONNX Runtime 1.26 (benefits from the Nemotron Conformer MHA fusion optimizer in v1.25). Keeps encoder + decoder ONNX sessions loaded in RAM with SentencePiece tokenizer. Full RNN-T greedy decoder with 128-bin Slaney mel spectrogram, pre-emphasis, and per-feature normalization — matching parakeet-rs exactly.
- **Rust STT backend module** (`src/stt/unified_parakeet.rs`) — manages the Python server lifecycle (spawn, health check with exponential backoff, transcription via HTTP POST of raw float32le audio bytes, graceful kill on drop). Python venv resolution follows the existing TTS priority chain (project venv → app-data venv → system Python).
- **Model download paths** — fp32 and int8 variants served as `.tar.gz` archives from CDN, consistent with existing model distribution pattern.

**STT — Streaming Transcription (EOU 120M):**

- **Streaming RNNT endpoints** (`/stream_start`, `/stream_feed`, `/stream_end`, `/stream_status`) — the Python server now supports incremental audio chunk processing with stateful decoder (LSTM states persist between chunks). Feeds audio in 250ms chunks, returns progressive partial results with EOU detection.
- **EOU model auto-detection** — the transcription pipeline detects the EOU 120M model by its HuggingFace repo URL and routes through the streaming API. The Unified model continues using the offline `/transcribe` endpoint.
- **`transcription-partial` event** — emitted by the Rust backend during streaming transcription whenever text changes. Enables real-time word-by-word overlay display in future frontend work.

**STT — Multi-STT Pipeline (Parallel Transcription + LLM Merge):**

- **Multi-STT orchestrator** (`src/stt/multi_stt.rs`) — runs 2–3 STT models in parallel via `std::thread::spawn`. Each thread loads its own engine independently (transcribe-rs or Python server), transcribes the audio, and returns text. All 9 engine types supported.
- **LLM merge** — results formatted as a transcriptions block and fed through the existing post-processing pipeline with a configurable prompt template (`{transcriptions}` placeholder). Falls back to the primary model's result when post-processing is disabled.
- **Settings** — `multi_stt_enabled` (bool), `multi_stt_models` (Vec), `multi_stt_prompt` (String with default merge prompt). Integration point in `actions.rs` runs multi-STT before the existing post-process step.
- **Architecture** documented in `src/stt/mod.rs` — streaming model for real-time feedback + 1–2 backup models for accuracy + LLM merge.

**STT — EOU Streaming Toggle + Silence Gate:**

- **`eou_streaming_enabled`** setting (default: `true`) — toggles between streaming API (`/stream_start` → `/stream_feed` → `/stream_end`) and offline `/transcribe` for the EOU 120M model. Disabling streaming uses a single HTTP call with no partial events.
- **Silence gate on chunk feeding** — each 250ms audio chunk is checked for RMS energy before being sent to the streaming model. Chunks below the `0.002` RMS threshold (matching TripleVAD's energy gate) are skipped. Prevents background noise and silence gaps from triggering the model or causing premature `<EOU>` emission. Applied in both main transcription path and multi-STT parallel path.

**STT — Sherpa-ONNX Integration (Nemotron 3.5 ASR + Unified Streaming):**

- **Nemotron 3.5 ASR model** (`nemotron-3.5-asr-0.6b-int8`) — 40-language streaming ASR via sherpa-onnx. 80ms chunks, punctuation + capitalization, per-stream language codes. Downloads 4 files from `csukuangfj2/sherpa-onnx-nemotron-3.5-asr-streaming-0.6b-80ms-int8-2026-06-11`.
- **Sherpa-onnx auto-detection** — the unified Python server detects sherpa-onnx format when `tokens.txt` is present and routes through `sherpa_onnx.OnlineRecognizer.from_transducer()`. Full pipeline handled by sherpa-onnx: mel features, encoder cache, buffered RNNT decoder, beam search, tokenizer, endpoint detection.
- **Parakeet Unified sherpa-onnx streaming export** (`temp_export_onnx/`) — complete NeMo-to-ONNX pipeline for exporting the Unified 0.6B model to buffered streaming format (560ms, INT8). Venv with NeMo + PyTorch, download script (`download_nemo.py` via huggingface_hub), export script (`export_onnx_streaming.py` from sherpa-onnx PR #3575). Produces `encoder.int8.onnx` (624 MB), `decoder.int8.onnx` (7 MB), `joiner.int8.onnx` (2 MB), `tokens.txt`. Exported model at `models/STT/parakeet-unified-en-0.6b-sherpa-streaming/`.
- **Sherpa-onnx buffered streaming metadata** — encoder ONNX tagged with `streaming_model_type=nemo_parakeet_unified_streaming`, `buffered_streaming=1`, left/chunk/right frame counts. Metadata-driven config — no more guessing preprocessing params or decoder dtype.
- **Single server, two paths** — `unified_parakeet_server.py` auto-detects model format: tokens.txt → sherpa-onnx, tokenizer.model/vocab.txt → manual ONNX. Same HTTP API for both. Deleted `sherpa_onnx_server.py` (functionality merged).

**STT — HuggingFace Direct Downloads:**

- **Multi-file HF downloads** — `ModelInfo` gains `hf_repo` + `hf_files` fields (hidden from frontend). `download_huggingface_model()` downloads individual ONNX/tokenizer/config files from HuggingFace repos with retry (3 attempts) and progress reporting.
- **All 5 Parakeet ONNX models** now download directly from HuggingFace (eschmidbauer/unified + ysdede/eou-120m) — no CDN dependency.

### Changed

- **Default VAD mode** changed from `"silero"` to `"triple"` for all modes (dictation, conversation, push-to-talk).
- **Text sanitizer pipeline reordered** — markdown stripping runs first, then TN (text-processing-rs), then legacy regex-based TTS normalization, then artifact cleanup.
- **Always-On Microphone toggle moved** from Debug settings to General → Sound section for easy discovery.
- **All dependencies updated to latest** — Tauri 2.11, rodio 0.22, rubato 3.0, reqwest 0.13, rusqlite 0.40, `windows` 0.62, specta rc.25, transcribe-rs 0.3.11. React 19, Vite 8, TypeScript 6, zod 4, ESLint 10, i18next 26. `cpal` pinned to 0.17.
- **Removed `parakeet-rs` crate** — replaced with Python onnxruntime 1.26 server for Parakeet Unified model inference. The Rust `ort` crate (locked to 2.0.0-rc.12, ONNX Runtime ~1.20) cannot be upgraded to 1.26 yet; Python path bypasses this bottleneck.
- **EOU 120M model uses streaming pipeline** — detected by HuggingFace repo URL, routes through `/stream_start` → chunked `/stream_feed` → `/stream_end` with `transcription-partial` events. Unified model stays on offline `/transcribe` endpoint. Toggleable via `eou_streaming_enabled` setting.
- **Multi-STT and EOU streaming settings** added to `AppSettings`: `multi_stt_enabled`, `multi_stt_models`, `multi_stt_prompt`, `eou_streaming_enabled`.
- **Renamed `eou_streaming_enabled` → `parakeet_streaming_enabled`** — streaming mode now applies to all UnifiedParakeet models (Unified 0.6B + EOU 120M), not just EOU. EOU model additionally emits `<EOU>` tokens for end-of-utterance detection.
- **Frontend streaming toggle** — `ParakeetStreamingToggle` component in `ModelSettingsCard`, appears for all UnifiedParakeet models. Reads/writes `parakeet_streaming_enabled` setting via Rust command + store updater.
- **ONNX Runtime dtype inspection** — decoder input signatures read from actual ONNX metadata at load time instead of hardcoded guesses. `targets` dtype and `target_length` presence determined per-model. No more int32/float32 back-and-forth.
- **Encoder output count handling** — `_encoder_forward()` supports both 1-output (EOU FP16) and 2-output (Unified INT8, EOU FP32) encoder ONNX models.
- **`reqwest::blocking` replaced with `ureq`** in `unified_parakeet.rs` — prevents "Cannot drop a runtime" tokio panic when the Python server is launched from within an async context.
- **HuggingFace multi-file downloads** — all 6 Parakeet ONNX models download directly from HuggingFace repos. `download_huggingface_model()` with retry (3 attempts) and progress events.
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
- **TTS sentence ordering** — FIFO sentence queue via `mpsc::channel` consumer thread; sentences synthesized and appended in correct order, fixing out-of-order playback when short sentences synthesized faster than longer earlier ones.
- **Tokio runtime panic** — channel-based approach isolates Piper backend synthesis in a dedicated `std::thread`, avoiding the "Cannot drop a runtime in a context where blocking is not allowed" panic from synchronous calls.

### Removed

- **IMPROVEMENT_PLAN.md** — deleted the improvement plan file.

---

## Documentation Audit & Sync (June 14, 2026)

Full codebase audit and documentation pass — read every source file, verified every claim.

### Corrections Applied

- **S2B2S_REVIEW.md** — Updated TTS backend count (8→9, added Pocket TTS row). Noted `WarmEngine` trait in `tts/status.rs` is dead code (no backend implements it). Noted TTS telemetry `record()` never called. Updated Brain subsystem: 10 LLM providers (not ~7). Added `brain-overlay/`, `overlay_fx/`, and `stt/` modules to file structure. Updated Known Issues with 5 new items (WarmEngine dead code, telemetry not wired, model definitions hardcoded, wgpu placeholder, fragment_queue.rs dead code). Added Evolution Documents section noting `futuristic_analysis/` supersedes `analysys/`.
- **README.md** — Updated TTS backend count (8→9), Brain provider count (3→10), STT engine types (3→10). Fixed "WarmEngine trait lifecycle" roadmap item to note it's defined but unimplemented. Added missing "Voice barge-in" roadmap item. Updated `reference_github_links.md` project count (21→23).
- **AGENTS.md** — Fixed frontend tree: removed non-existent `src/lib/types.ts`, clarified `src/lib/types/events.ts`. Added `brain-overlay/` frontend entry. Added `stt/` and `overlay_fx/` backend entries. Updated TTS engine count, WarmEngine status note. Added `futuristic_analysis/` and `S2B2S_ANDROID_COMPANION.md` to Key Files Reference.
- **CRUSH.md** — Removed orphaned `S2B2S_VOX_COMPARISON.md` reference (file never existed). Added full `futuristic_analysis/` directory with all 9 files. Noted `analysys/` superseded status. Added `stt/`, `overlay_fx/`, `brain-overlay/` to file structure.
- **CHANGELOG.md** — Updated `reference_github_links.md` project count (21→23). This entry.
- **CLAUDE.md** — Updated evolution plans reference to `futuristic_analysis/`, noted `analysys/` superseded status.
- **repomix-file-descriptions.md** — Fixed `settings.rs` line count (1,600→1,800). Fixed `managers/transcription.rs` line count (886→996). Fixed `managers/model.rs` line count and noted 20+ hardcoded entries. Added `stt/` module entries. Added `overlay_fx/` module entries with placeholder note. Added `brain-overlay/` entries. Updated `tts/status.rs` and `tts/telemetry.rs` dead-code notes.
- **LICENSE** — Added NairoDorian copyright line for S2B2S-specific additions.
- **repomix-file-descriptions.md** — Added `stt/` and `overlay_fx/` module descriptions, `brain-overlay/` entries.

### Verified Accurate (No Changes Needed)

- **LLAMA_CPP.md** — Accurate. Pre-compiled server integration fully reflected.
- **CONTRIBUTING.md** — Accurate. Manager listings, build steps, and philosophy all correct.
- **CONTRIBUTING_TRANSLATIONS.md** — Accurate. All 20 languages confirmed.
- **BUILD.md** — Accurate. Platform instructions verified.

### Key Codebase Findings

- **105 Rust source files** in `src-tauri/src/` (plus 8 in `overlay_fx/`, 3 in `stt/`)
- **113+ TypeScript/React files** in `src/`
- **9 CI workflows** in `.github/workflows/`
- `settingsStore.ts` is 741 lines (not 739 as documented)
- Non-English i18n locales are uniform 767 lines (likely auto-generated from template)
- `analysys/` exists on disk but is gitignored and Repomix-excluded — superseded by `futuristic_analysis/`
