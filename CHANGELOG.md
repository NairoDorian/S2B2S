# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

> This file tracks the **`Handy_Multi_STT` fork** on top of upstream
> [cjpais/Handy](https://github.com/cjpais/Handy). The fork carries upstream's
> version number (currently **0.9.6**, merged 2026-08-26) and has not cut a
> release of its own yet, so all fork work is listed under `[Unreleased]`.
> Upstream's own release notes live in `src/content/release-notes/` and on the
> upstream releases page.

## [Unreleased]

### Added

- **Local LLM page: llama.cpp managed in-app (2026-09-11).** The
  `llama-server` that post-processing and the Multi-STT merge use no longer
  has to be launched by hand. Install a build from the llama.cpp releases
  (CUDA / Vulkan / CPU, auto-detected, cudart fetched for CUDA) or point at an
  existing folder (the maintainer's `Llama.cpp/llama` layout is adopted on
  first run), pick the model, MTP draft and mmproj files, tune the command
  line — defaults reproduce `launch_server_E2B_Q4.ps1` exactly — and start,
  stop or restart it with the output in view. The server runs detached, is
  tied to Handy through a Windows job object, starts on demand when a
  request targets it, and stops with Handy. The status bar gained a brain
  indicator (Stopped / Starting / Ready / Error + alias) and CPU, RAM, GPU
  and VRAM meters from a single 1 Hz sampler that sleeps while the window
  is hidden.

- **Help, shortcut cheat sheet and toast history**, ported from the AIVORelay
  fork (2026-09-11): a **Help** page (goal cards, search over the translated
  text, table of contents, one section per feature with an "Open page"
  button, and a copyable prompt for asking an AI assistant); a one-line
  **QuickHelp** banner on every page linking into the matching Help section;
  a right-edge **shortcuts tab** listing the assigned hotkeys by category,
  pinnable and resizable, where each row jumps to and highlights the control
  that changes it — and which, unlike AIVORelay's, stays visible on a fresh
  install to say that no transcribe shortcut is set yet; and **Recent errors
  and warnings** at the top of the Debug page, listing every error/warning
  toast of the session so one that auto-dismissed can still be read.

- **Noise suppression (RNNoise)** toggle in Settings → Advanced, ahead of the
  VAD toggle (2026-09-11). Uses the pure-Rust `nnnoiseless` port of Xiph's
  RNNoise on the microphone path before voice detection and the model, so
  fans, keyboards and room hum reach neither. Off by default. Switching it
  applies on the next captured chunk, and the live VAD test has its own
  On/Off pill so the score and level bars can be compared with and without
  it. Saved raw audio and the overlay level meter show the untouched
  microphone. Ported from the AIVORelay fork's noise cancellation, moved
  ahead of the meters so the effect is visible.

- **Live VAD test** in Settings → Advanced, next to the VAD Threshold slider
  (2026-09-11). "Test with microphone" opens the mic and runs only the
  detector — no model is loaded, nothing is transcribed or saved — and shows
  the raw speech score against the threshold marker, a Speech / Silence
  verdict, the input level and whether the frame would have reached a model
  after smoothing. The threshold slider now applies in place on the next
  frame (`VoiceActivityDetector::set_threshold`), so it can be tuned while the
  test runs; the old rebuild-and-reopen path for threshold changes is gone.

- **Multi-STT mode.** A dedicated `multi_stt_transcribe` shortcut records once
  and transcribes with the primary model plus up to three extra models in
  parallel, each on its own engine (`TranscriptionManager::extra_engines`),
  then optionally merges the outputs through the post-processing LLM provider
  using a prompt with `${output}`, `${output2}`, `${output3}` and `${output4}`
  placeholders (`${output1}` is accepted as an alias of `${output}`). Falls
  back to newline-concatenation when no prompt is configured or the LLM call
  fails. Extra models get their own language and English-translation
  settings, are pre-loaded in parallel while you record, can be kept resident
  between uses (`multi_stt_keep_extra_models_loaded`, on by default) and
  unloaded on demand from Settings → Multi-STT. Multi-STT history rows store
  every model's output alongside the merged text.

- **Multi-STT performance mode.** Optionally simulates a "full power" keyboard
  shortcut (default `ctrl+space`) when a Multi-STT recording starts or ends and
  a "normal" shortcut (default `ctrl+alt+space`) once the merge/paste is done,
  so an external power-profile tool can boost the CPU only while decoding. To
  prevent the simulated keys from retriggering Handy itself, transcription
  hotkeys equal to either shortcut are not registered while performance mode
  is on, and the shortcut recorder refuses them.

- **Silero VAD v6.2, run correctly, with hysteresis.** Voice activity detection
  had been a silent no-op: the `vad-rs` crate only speaks Silero **v4**'s tensor
  interface while the shipped model is **v6.2**, so every frame failed
  (`Invalid input name: h`) and was passed through. `audio_toolkit/vad/silero.rs`
  now drives the ONNX graph directly through `ort` with the v5/v6 contract
  (512-sample / 32 ms window, 64-sample context carry-over, `state` carried
  from `stateN`), validates the model's input names at load, and logs the first
  per-recording failure instead of swallowing all of them. Speech is entered at
  the 0.3 threshold and left at `max(threshold − 0.15, 0.01)`, as in the
  reference pipeline, which removes the frame-to-frame flapping a single
  threshold produced. `src-tauri/tests/vad_speech_clock_probe.rs` is an opt-in
  regression check over a real recording (`HANDY_PROBE_WAV`). _Superseded on
  2026-09-10: Silero and `ort` were removed; Earshot keeps the hysteresis and
  the probe — see Changed / Removed._

- **Earshot VAD backend toggle** (`vad_backend`: `silero` | `earshot`). The
  capture pipeline re-frames itself to the active detector's frame length
  (32 ms Silero, 16 ms Earshot); hangover, prefill and onset are derived from
  millisecond constants via `frames_for_duration_ms` so switching backends
  never shortens them. _The toggle was retired on 2026-09-10 when Earshot
  became the only detector._

- **Skip transcription when a recording contains no speech.** `SpeechClock`
  publishes its running total to a lock-free counter; both shortcut actions
  discard recordings under 200 ms of measured speech
  (`MIN_SPEECH_MS_TO_TRANSCRIBE`) without running a model. Beyond the saved
  decode this prevents Whisper-family models from hallucinating confident text
  out of silence and pasting it. The threshold sits below Silero's own 250 ms
  `min_speech_duration_ms` so a clipped short word still transcribes, and with
  VAD disabled every frame counts as speech, so it can never fire in that mode.

- **Speech statistics in the recording overlay.** Both the Minimal and Live
  overlays can show whether you are speaking or paused, a timer that counts
  only while you are actually talking, and a running words-per-minute figure
  (streaming models only). Driven by the raw per-frame VAD verdict rather than
  the smoothed keep/drop decision, whose hangover tail (≈1.66 s in streaming
  mode) would otherwise bill more than a second of phantom speech per pause.
  Gaps shorter than the pause tolerance count as the same utterance. Settings:
  `overlay_speech_stats` (on by default) and `speech_pause_hold_ms`
  (10–2000 ms, default 500).

- **Direct streaming paste** (`paste_method: direct_streaming`, plus
  `direct_streaming_speed`). Live transcript text is typed into the target
  application character by character as the streaming model commits it,
  instead of being pasted once at the end.

- **Raw uncompressed audio recording** (`save_raw_audio`, Settings → Advanced →
  History). Saves the microphone signal at its captured sample rate and format
  (32-bit float, 24-bit or 16-bit PCM, mono) before resampling and VAD
  filtering. WAV serialisation runs on a blocking thread in parallel with the
  decode. `read_wav_samples` decodes all of these and resamples to 16 kHz for
  playback, retries and benchmarks.

- **Windows real-time optimisations.** `HIGH_PRIORITY_CLASS`, Windows 11 EcoQoS
  power-throttling opt-out, 1 ms `timeBeginPeriod`, MMCSS `"Capture"`
  scheduling for the audio worker thread, and minimal hardware buffer sizes.

- **Status-bar model controls.** Three mutually exclusive popovers in the
  footer: model switcher, quantization picker with a per-quant download button
  and an in-place quantization benchmark (`benchmark_model_quantizations`,
  using your latest recording as reference audio), and a native streaming
  latency panel (`native_streaming_latency_presets`: fastest → accurate, per
  model family).

- **History tools.** Delete all recordings (with confirmation), `VACUUM` the
  history database after cleanups, open the models folder (or the Hugging Face
  cache directory when models live there), and retry a history entry through
  the current model.

- **Accent colour palette selector** (`custom_accent_color`) with a gold
  default and eight presets; the overlay follows the chosen colour.

- **Configurable microphone idle timeout** (`mic_idle_timeout_value`,
  `mic_idle_timeout_unit`, `mic_idle_infinite`) for the lazy-close mode that
  previously hard-coded 30 s.

- **Append trailing newline** (`append_trailing_newline`) alongside the
  existing trailing-space option.

- **Headless transcription CLI.** `handy --transcribe-file <wav>` (with
  `--model`, `--device-index`, `--list-devices`, `--list-models`, `--repeat`,
  `--json`) runs the batch path without a microphone. Accepts any mono WAV the
  app itself writes (16/24-bit PCM or 32-bit float, any sample rate).

- **Build tooling.** `scripts/tauri-runner.ts` wraps the Tauri CLI: it runs
  `scripts/check-transcribe-deps.ts` (bumps the `transcribe-cpp` git pin when
  the `NairoDorian/transcribe.cpp` fork's `main` moves; never blocks a build)
  and adds `--fast`/`--local-gpu` (`bun run build:fast`), which sets
  `TRANSCRIBE_CUDA_ARCHITECTURES=auto` so a release build compiles kernels for
  the local GPU only. `bun run build:full` is the multi-arch distribution
  build. `scripts/update-deps.ts` (`bun run update-deps [--prerelease]`) bumps
  npm and Cargo dependencies with validation steps; `scripts/update-rtk.ts`
  updates the RTK CLI used by the maintainer's Claude Code hook.

- **Review checkpoint docs.** `docs/CODE_REVIEW_2026-08-26.md` (full audit
  findings, what was fixed, what remains) and `docs/KNOWN_ISSUES.md`.

### Changed

- **llama.cpp installs no longer bundle the 500 MB CUDA runtime by default
  (2026-09-11).** The cudart package (cublasLt64 438 MB + cublas64 + cudart64)
  is now an opt-in toggle, exactly like the download script's
  `-IncludeCudart`; the page says whether a CUDA toolkit is installed
  (`CUDA_PATH`) and offers "Remove runtime" on installs that already carry
  it, so the earlier 672 MB install shrinks to about 180 MB. Toolkit
  detection checks `bin\x64` (where CUDA 13 keeps cudart), every
  `CUDA_PATH_V*`, PATH and the default install folder, and the page says
  whether the folder is on PATH; the server launcher adds it to the child's
  PATH when it is not.
- **Faster startup (2026-09-11).** The always-on microphone is opened on a
  background thread instead of stalling startup for ~0.9 s, and on
  Windows/Linux the hotkeys and paste input are registered at the end of
  core startup rather than ~8 s later when the settings window finished
  loading (macOS keeps the permission-driven order). The recorder logs its
  device on one line, and the log prints how long core startup took.
- **Status bar popovers work again; one-line bar; interface scale
  (2026-09-11).** An `overflow-x-auto` added for the one-line status bar
  also clipped the upward model / quantization / latency popovers, so
  clicking them appeared to do nothing — removed. The bar's pills now
  truncate instead of wrapping (model 112 px, quantization 80 px, latency
  96 px, brain alias 96 px, compact meters), and the window minimum is
  1060×720 (also the default size) so it always fits on one line next to the
  default sidebar. Debug → **Interface scale** zooms the whole window
  (70–160 %) for screens whose OS scaling makes the UI too small or large.
- **Shortcut cheat sheet moved to a top-right overlay (2026-09-11).** The
  right-edge tab with pin and resize is gone; a keyboard button in the
  window's top-right corner, reachable from every page, toggles a panel of
  the assigned shortcuts that closes on Escape, click-outside or after a
  jump. A warning dot on the button flags that no transcribe shortcut is
  set.
- **Window and sidebar sizing (2026-09-11).** The main window opens at
  1080×780 with a 960×720 minimum, enough for the whole sidebar and a
  one-line status bar (model, brain, CPU/RAM/GPU/VRAM meters, updater). The
  sidebar is wider by default (208 px, so "Transcribe Files" is no longer
  cut), resizable by dragging its right edge (160–360 px), collapsible to an
  icon rail, and scrolls when the window is shorter than its list; width and
  collapsed state persist per machine. The status bar never wraps.

- **Sharp-cornered, cyberpunk-minimal restyle (2026-09-11).** Every
  `rounded-*` utility (including `rounded-full`) and every shadow token now
  resolves to a square corner and a 1px hairline at the Tailwind theme
  level, so the whole settings app and the overlay lost their radii without
  touching components. New default palette: near-black ground, cold
  off-white ink, neon-cyan accent (light mode keeps the same idea on
  paper); three neon presets join the accent picker; key chips and values
  render in a monospace stack; the sidebar marks the active page with a
  neon edge instead of a filled block. Toggle switches keep their pill shape
  (the one deliberate exception, via a `rounded-pill` utility), and the Debug
  page is always listed in the sidebar. `docs/PERFORMANCE.md` records the
  latency budget and rules every change is held to.

- **transcribe.cpp is the only inference runtime and Earshot the only VAD
  (2026-09-10).** `transcribe-rs`, ONNX Runtime (`ort`, `ndarray`) and
  Silero VAD are gone, together with the `onnxruntime.dll` / `libonnxruntime`
  staging in `build.rs`, `build.yml`, `flake.nix` and the 2.2 MB model file.
  The capture pipeline now runs on Earshot's 16 ms frames
  (`VAD_FRAME_SAMPLES` = 256; hangover / prefill / onset are unchanged in
  milliseconds: 104 / 29 / 4 frames). The Advanced page keeps one threshold
  slider (`vad_threshold_earshot`) and the transcribe.cpp accelerator
  dropdown; the VAD-backend selector and the ONNX accelerator dropdown are
  gone. The 11 hard-coded ONNX models (Parakeet v2/v3, Moonshine base and
  the three streaming variants, SenseVoice, GigaAM, Canary 180m / 1B v2,
  Cohere) are no longer listed; every one has a GGUF conversion in the
  catalog, and settings schema **6** remaps `selected_model` and
  `multi_stt_model_2..4` to that successor (`legacy_onnx_model_replacement`,
  unit-tested against the catalog). Old ONNX directories on disk are left in
  place. `ModelInfo` lost `engine_type` / `is_directory`, tarball download
  extraction and the `model-extraction-*` events are gone, and
  `LoadedEngine` is a single-variant enum around the transcribe-cpp
  `Session`. `tests/vad_speech_clock_probe.rs` now drives Earshot;
  `tests/vad_backend_bench.rs` (a two-backend comparison) was deleted.
  `docs/PLAN_TRANSCRIBE_CPP_ONLY.md` records the plan and what was skipped.

- **GPU backend on Windows x86_64 and Linux is CUDA** (transcribe.cpp
  `cuda` feature via the `NairoDorian/transcribe.cpp` fork) instead of
  upstream's Vulkan; `.cargo/config.toml` passes the CUDA 13.3 / MSVC 2026
  flags and disables sccache for the native build. Windows aarch64 stays CPU,
  macOS stays Metal. The upstream CI workflows (`build.yml`, `test.yml`) still
  install the Vulkan SDK — see `docs/KNOWN_ISSUES.md`.
- **Capture pipeline framed to the VAD's native window** (32 ms for Silero,
  was 30 ms) so one resampled frame is exactly one VAD decision. Hangover,
  prefill and onset are now specified in milliseconds
  (`VAD_STREAMING_HANGOVER_MS` = 1650 → 52 frames = 1.664 s, offline hangover
  and prefill 450 ms → 480 ms, onset 60 ms → 64 ms).
- **Fresh installs ship without a transcribe hotkey.** `transcribe` and
  `multi_stt_transcribe` default to an empty `current_binding` (the
  `default_binding` is kept for "reset"), and settings schema migrations 3/4
  cleared existing users' bindings once, so that the performance-mode
  simulated keys could not recurse. Set your shortcuts in Settings → General.
  The macOS Multi-STT default is now `ctrl+option+space` (the previous
  `option+alt+space` collapsed to `option+space`, the primary shortcut).
- **Linting uses oxlint** (`.oxlintrc.json`) instead of ESLint, which cannot
  parse TypeScript 7; `eslint-plugin-i18next` is loaded through oxlint's
  `jsPlugins` so `i18next/no-literal-string` is still enforced.
- **Bun 1.4** (`bunfig.toml`: isolated linker, `noOrphans`, no `.env`
  loading), lockfile v2, and prerelease pins for React 19.3, TypeScript 7,
  Vite 8, Prettier 4 and Playwright (`bun run update-deps --prerelease`).
- **Release profile** uses thin LTO with 16 codegen units for faster
  parallel linking.
- **Finalised streaming text** uses `stream.text().display()` (committed +
  tentative) so the direct stream writer's target matches what was typed.
- **Log redaction** of transcription text in production builds now also
  covers the Multi-STT per-model outputs.
- `multi_stt_keep_extra_models_loaded` only matters when
  `model_unload_timeout` is `Immediately`; with any other timeout the idle
  watcher unloads extra engines together with the primary model.

### Fixed

- **Direct streaming no longer double-pastes.** With post-processing on, the
  raw live text was typed and the polished text pasted after it (auto-submit
  twice); with Multi-STT the merged result was pasted after the primary
  model's live text. Direct streaming now types into the app **only for plain
  transcription**. When the result will be post-processed or merged, the
  stream is a preview only: nothing is typed, the Live overlay is forced (if
  the model can stream) so the transcript is still visible as it forms, and
  the processed result is pasted once through the default clipboard paste
  (Ctrl+V). `TranscriptionManager::start_stream(live_typing)` carries the
  decision to the stream worker.
- **Extra models cannot load twice concurrently.** `load_extra_model`
  coalesces requests for the same id (a second caller waits for the first and
  shares its outcome), so the pre-load in `MultiSttAction::start` and the load
  in `stop()` no longer build — and briefly hold — two engines on short
  recordings.
- **Quantization download pill** clears whenever the download command
  settles, not only on `model-download-complete`, so a cancel that resolves
  `ok` no longer leaves "Downloading 0%".
- **Recording overlay** keeps and runs its event-listener cleanup (duplicate
  handlers under React StrictMode in dev).
- **Performance-mode shortcut recorder** rejects combinations the backend
  cannot simulate (modifier-only, `page up`, `numpad 0`, …) with a
  translated error instead of saving them and failing silently at run time.
- **Download progress for files over 4 GiB.** `DownloadProgress`,
  `ModelInfo.partial_size` and the HF progress state had been narrowed to
  `u32` to keep the TypeScript bindings free of `BigInt`; 22 catalog files
  exceed that, so `total` wrapped, `downloaded` saturated at 4 GiB, the
  percentage overshot and a progress event was emitted for every chunk. The
  Rust side is `u64` again and crosses to TypeScript as `number` via
  `#[specta(type = f64)]`.
- **Accent colour now persists.** `AccentColorSelector` wrote
  `custom_accent_color` through `updateSetting`, but no updater or Tauri
  command existed, so the colour only lived in `localStorage` and was reset to
  Gold by the next settings refresh or launch. Added
  `change_custom_accent_color_setting` and the store updater.
- **Multi-STT start no longer chimes on a fixed 100 ms delay.** It waits for
  the first real microphone callback and emits `recording-ready`, exactly like
  the standard action, so the overlay leaves its "arming" state and slow
  Bluetooth/USB devices don't chime before they capture.
- **Multi-STT honours Cancel during the decode and merge.** Escape pressed
  while the four models or the LLM merge were running used to re-show the
  overlay, finish the LLM request and still save a history row; the action now
  checks the cancel generation after the parallel decode, polls it during the
  merge, and bails before saving.
- **Multi-STT history rows only after the WAV is verified.** The recording is
  awaited and checked with `verify_wav_file` before `save_entry`, matching the
  standard action, so a failed save no longer leaves a row pointing at a
  missing file.
- **Multi-STT merge output is sanitised** like post-processing output
  (`<think>` blocks and invisible characters stripped), and the llama.cpp
  `/chat/erase_all` cleanup is only sent to the `custom` provider instead of
  to every hosted API after every merge.
- **Shortcut registration is consistent across all code paths.** Startup
  (both keyboard backends), resume after the shortcut recorder,
  implementation switch and the feature toggles now share
  `should_register_binding`. Enabling Multi-STT or post-processing registers
  its hotkey immediately (previously required a restart), disabling
  unregisters it, and changing the performance-mode shortcuts re-derives the
  set. The performance-mode conflict rule only applies while performance mode
  is enabled; with it off, `ctrl+space` is a perfectly good transcribe hotkey
  again. The shortcut recorder rejects a conflicting combination with an error
  instead of accepting it and silently dropping it at the next launch.
- **Extra-engine lifecycle.** Deleting a model that is loaded as an extra
  engine unloads it first (releases memory and, on Windows, the mmap lock on
  the file); turning Multi-STT off unloads all extra engines; changing a slot
  always queues the old engine's unload even while it is leased to an
  in-flight transcription; `load_extra_model` propagates load errors instead
  of reporting success for a model that isn't downloaded; `unload_extra_model`
  runs on a blocking thread instead of freezing the UI while a multi-GB engine
  drops.
- **Extra models get language coercion.** `transcribe_with_engine` now runs
  `effective_language_for_model` for SenseVoice, Canary and Cohere too, so an
  extra model no longer receives the primary model's language verbatim and
  returns nothing when it isn't supported.
- **Raw audio tap only runs when `save_raw_audio` is on.** The native-rate
  buffer (~11 MB/min at 48 kHz float) was accumulated for every recording
  regardless of the setting.
- **"Never close" microphone idle timeout no longer parks a thread per
  recording** (each one slept for `u64::MAX` seconds).
- **`--transcribe-file` accepts the app's own raw recordings** (it rejected
  anything but 16 kHz 16-bit PCM although the reader handles more).
- **`audio_toolkit/bin/cli.rs` compiles again** (it passed three arguments to
  the four-argument `with_vad`; the file is not a build target, so nothing
  noticed).
- **Seven upstream microphone-error unit tests** dropped by the v0.9.6 merge
  are restored.
- **i18n plumbing.** The Multi-STT shortcut's name/description were keyed at
  root `bindings.*` instead of `settings.general.shortcut.bindings.*` (where
  the UI looks), so every language showed the hardcoded English fallback.
  Every non-English locale still carried the pre-merge
  `settings.advanced.vadBackend.{silero,earshot}` keys and
  `modelSelector.downloadSpeed`; they are moved to the `options.*` /
  `common.*` paths upstream v0.9.6 uses. Nine unused fork-added keys removed.
- **`ModelSelector`** clears the quantization list when the variants query
  fails instead of showing the previous model family's quants.
- **`MicIdleTimeout`** number input can be cleared and edited normally: the
  value is drafted locally and committed (clamped to 1…86 400 s / 1…1 440 min)
  on blur or Enter instead of being written to the settings store per
  keystroke.
- Windows EcoQoS opt-out uses `PROCESS_POWER_THROTTLING_CURRENT_VERSION`
  instead of a literal `1`.
- **Earshot VAD now uses the same threshold hysteresis as Silero.** The
  adapter compared each 16 ms score against a single threshold, so the
  flapping that hysteresis was added to fix for Silero (stuttering speech
  indicator, stalling speech clock) could come back by switching backend.
  `Hysteresis` moved to `vad/mod.rs` and gates both detectors. Measured on the
  maintainer's recordings (`tests/vad_backend_bench.rs`): Earshot's kept audio
  went from 12.9 s to 13.3 s, identical to Silero, at 97.7 % frame agreement.
- **Multi-STT performance mode restores "normal power" on two more exit
  paths** — when the recording produced no samples after a trigger-on-start
  full-power request, and when the paste could not be dispatched to the main
  thread.
- New opt-in `tests/vad_backend_bench.rs` (`HANDY_BENCH_WAV_DIR=<dir>`) runs
  both VAD backends over real recordings and reports cost per frame, voiced
  fraction, kept seconds and frame agreement; `earshot` is pinned to
  `opt-level = 3` in the dev profile so the numbers are representative.
  _Deleted on 2026-09-10 with the second backend; the numbers it produced are
  kept in AGENTS.md and `docs/PLAN_TRANSCRIBE_CPP_ONLY.md`._

### Removed

- **ONNX stack (2026-09-10):** `transcribe-rs` (and its `[patch.crates-io]`
  entry), `ort`, `ndarray`, `tar`, `flate2`; `audio_toolkit/vad/silero.rs`,
  `resources/models/silero_vad_v6.2.onnx`, `resources/models/gigaam_vocab.txt`
  and the GigaAM directory migration; the `vad_backend`, `ort_accelerator`
  and `vad_threshold_silero` settings with `change_vad_backend_setting` /
  `change_ort_accelerator_setting` (`change_vad_threshold_setting` lost its
  backend argument); `AvailableAccelerators.ort`; `VadBackendSelector.tsx`;
  the `settings.advanced.vadBackend.*`, `settings.advanced.acceleration.ort.*`
  and `modelSelector.extracting*` locale keys; `tests/vad_backend_bench.rs`.
- `multi_stt_selected_merge_prompt_id` setting (never read or written; the
  merge prompt is a single `multi_stt_merge_prompt`), and the unused
  `NativeStreamingLatencyPreset::all_presets`.
- `vad-rs` dependency (see Silero entry), the redundant direct `audioadapter`
  / `audioadapter-buffers` dependencies (re-exported by `rubato`), the unused
  `Win32_Media_Audio` / `Win32_Media_Multimedia` feature flags, and the unused
  `zod` npm dependency.
- `eslint.config.js` (replaced by `.oxlintrc.json`).

### Known gaps

- All 23 non-English locales are missing the same 87 fork-added strings; they
  fall back to English at runtime and `bun run check:translations` fails
  until they are translated. See `docs/KNOWN_ISSUES.md` for this and the other
  open items from the 2026-08-26 review.
