# AGENTS.md

This file provides guidance to AI coding assistants working with code in this repository.

**The product is ZER0.** It began as a fork of the MIT-licensed
[Handy](https://github.com/cjpais/Handy) project by CJ Pais — credited in
`LICENSE`, `README.md` and the About page — and is now its own project with its
own name, identity and release line. The branch is still called
`Handy_Multi_STT`; nothing else is.

Two consequences for every change you make here:

- **Never spell the product name by hand.** Values come from `scripts/app-meta.ts`
  and its generated mirrors (`src/lib/appIdentity.ts`, `src-tauri/src/app_identity.rs`).
  `bun run check:identity` fails the build when a stale name survives a rename, so
  read the constant instead of writing the string. `bun run meta:sync` after
  editing `app-meta.ts`; `bun run meta:check` verifies the mirrors in CI.
- **Attribution stays.** The upstream notice in `LICENSE` is a legal requirement
  and is never edited; the fork's origin is named in prose.

> **NOTE**: this branch additionally carries **Multi-STT** mode — running up to
> four speech-to-text models in parallel and optionally merging their outputs via
> an LLM. See the Architecture Overview and Settings System sections below.

## Performance first

This is a real-time tool. Read [docs/PERFORMANCE.md](docs/PERFORMANCE.md)
before adding a feature, setting, poll, event, dependency or thread: it holds
the latency budget per path and the rules (audio thread is allocation-free,
hot toggles are atomics, commands never block the webview, events are
throttled and gated, resources stay warm, child processes run detached on a
supervisor thread, meters sample on one thread). State the cost of any new
thread, poll or dependency in the commit message.

## Logging

The runtime logging policy is documented in [docs/LOGGING.md](docs/LOGGING.md).
Terminal and file capture all severities (Trace) in every build profile;
`RUST_LOG` does not filter runtime capture. The old persisted `log_level`
field is compatibility data only. The in-app console reads the durable log
file regardless of debug mode, and its chips only hide records in the view.
Do not add capture-level UI or a per-record webview event stream.

## The pre-commit routine

**Run `bun run precommit` before every commit.** It is the gate, and it is one
command:

```bash
bun run hooks:install      # once per clone: points git at .githooks/
bun run precommit          # identity → translations → lint → typecheck → unit checks → format → repomix
bun run precommit:full     # the same, plus clippy and the Rust test suite
bun run precommit:routine  # dependencies to newest, then precommit:full — run by hand
```

`bun run precommit` runs, in order: `meta:sync` (regenerate the identity
mirrors), `meta:check` (they are in sync), `check:identity` (no stale product
name anywhere), `check:translations`, `lint`, `typecheck`, `test:unit`
(`bun test --parallel` over `*.test.ts` under `src/`, rooted by `bunfig.toml`'s
`[test] root`), `format:check`, and the repomix pack. `precommit:full` adds
`lint:backend` (clippy) and `test:backend` (cargo test). The checks read the
working tree, not the index; the only files the gate stages itself are the
identity mirrors `meta:sync` rewrites (`identityPaths()` in `app-meta.ts`), so
a selective `git add -p` is never widened behind your back. The hook deliberately does **not** run clippy and
the Rust suite by default — a ten-minute hook is a hook everyone bypasses with
`--no-verify`; run `precommit:full` before a release or a PR.

### The routine, in order

The gate is not the routine. The routine is what you run before a commit, and
the gate is what git then enforces:

```bash
bun run update             # 1. rtk → latest, EVERY dep to its newest PRERELEASE, then repomix
bun run precommit:full     # 2. the gate, plus clippy and the Rust suite
git commit                 # 3. the hook runs `bun run precommit` (on the working tree)
```

**Step 1 always goes to prerelease, for every dependency, Solid included.**
`--prerelease` is baked into `bun run update` and into `precommit:routine`, so
there is no way to run the routine and get a stable-only update — a "newest
published" that skips prereleases would not be tracking the newest of anything.
This is not a detail: for part of the stack the newest is _only ever_ a
prerelease, so a stable-only update would silently hold those packages back.
`solid-js`'s `latest` dist-tag is still the 1.x major; the entire Solid 2
toolchain (`solid-js` and `@solidjs/web` 2.0 release candidates,
`@solidjs/vite-plugin` 3.0 `next` builds) exists only on prerelease lines, as
does `typescript`'s dev build. See `update-deps.ts`'s `NPM_LINE_PINNED` for the
three packages that have to be held to one prerelease line rather than
"newest tag wins".

Step 1 is **manual and deliberately not in the hook.** It reaches two
registries, rewrites four lockfiles, and runs its own validation as part of the
run: `tsc -b`, then a Vite production build, then `cargo check`. That validation
is the point. The tempting shortcut is to run the update inside the hook without
it and let the gate's `typecheck` stand in, but that is a smaller proof (no
production build, clippy instead of cargo check) applied to the change that most
needs the larger one. `update-deps` therefore has no early-exit flag: a lockfile
that resolves is not a lockfile that builds. `bun run precommit:routine` is steps
1 and 2 as one command, and is the intended way to run them.

**Keeping everything current is part of the routine:**

```bash
bun run update          # rtk → latest, deps with --prerelease, then repomix
bun run update:rtk      # just the RTK CLI (tooling for the agent hook)
bun run update-deps -- --prerelease
bun run docs:fetch      # mirror the stack's own docs into docs/vendor/ and
                        # report what changed upstream — see docs/STACK_WATCH.md
bun run repomix         # regenerate repomix-output.xml (also in the gate)
bun run repomix:check   # fail if the pack is older than the newest tracked file
```

`update-deps` **always** takes `--prerelease` in this project: the point is to
track the newest published version of every dependency — prereleases included,
Solid included — so the next release is tested against what is actually newest.
A few packages cannot be resolved by "newest published" alone and are held
deliberately, with the reason printed in the report: **`NPM_LINE_PINNED`** holds
a package to one prerelease _line_ resolved from one dist-tag (`solid-js`,
`@solidjs/web` and `@solidjs/vite-plugin` follow `next` within their major and
refuse a downgrade, because `latest` is the previous major for `solid-js`, a
downgrade for `@solidjs/web`, and the vite plugin's `next` tag lags its
`latest`), and the ceilings — `NPM_MAJOR_LOCKED_PREFIXES` for `@tauri-apps/*`,
which must stay on the backend's major (the 3.x alpha today) so a JS API never
crosses the IPC to a different Tauri, and `CARGO_MAJOR_LOCKED` for
crates that cannot move without breaking a sibling pin — cap a bump at the
version already installed.

**`rtk` is used for every shell command**, with one exception: `bun` commands
are never proxied (`rtk bun …` is not supported). So `rtk git …`, `rtk cargo …`,
`rtk gh …`, `rtk node …` — and plain `bun run …`.

## Development Commands

**Prerequisites:**

- [Rust](https://rustup.rs/) (latest stable)
- [Bun](https://bun.sh/) package manager

**Core Development:**

```bash
# Install dependencies
bun install

# Run in development mode
bun run dev:cpu     # CPU-only, every model — the usual working loop
bun run dev:fast    # Local CUDA architecture
bun run dev:full    # Full CUDA architecture matrix
bun run tauri dev   # Same as dev:fast
# If cmake error on macOS:
CMAKE_POLICY_VERSION_MINIMUM=3.5 bun run tauri dev

# Build for production
bun run build:cpu   # CPU-only (a smoke build)
bun run build:fast  # Fast local build (auto-detects local GPU arch)
bun run build:full  # Full multi-arch distribution build

# Frontend only development
bun run dev        # Start Vite dev server
bun run build      # Build frontend (TypeScript + Vite)
bun run preview    # Preview built frontend
```

**Linting and Formatting (run before committing):**

```bash
bun run typecheck         # tsc -b
bun run test:unit         # bun test over *.test.ts under src/ (bunfig [test] root)
bun run lint              # oxlint (loads eslint-plugin-i18next through jsPlugins)
bun run lint:fix          # oxlint with auto-fix
bun run format            # Prettier + cargo fmt
bun run format:check      # Check formatting without changes
bun run format:frontend   # Prettier only
bun run format:backend    # cargo fmt only
bun run check:translations # every locale must have exactly en's keys (CI gate)
cd src-tauri && cargo clippy --all-targets && cargo test --all-targets
```

**Maintenance scripts (`scripts/`):**

| Script                             | Invoked by                                                                                     | Purpose                                                                                                                                                                                                                                                                                                                                                            |
| ---------------------------------- | ---------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `tauri-runner.ts`                  | `bun run tauri`, `dev:cpu`, `dev:fast`, `dev:full`, `build:*`                                  | Wraps the Tauri CLI; runs the transcribe.cpp pin check first; sets the build posture — `--fast` → `TRANSCRIBE_CUDA_ARCHITECTURES=auto`, `--full` → `=default`, `--cpu` → `TRANSCRIBE_CMAKE_ARGS=-DTRANSCRIBE_CUDA=OFF` plus a private cache root. Every lane packs the full family set                                                                             |
| `check-transcribe-deps.ts`         | `tauri-runner.ts` (imported), or run directly                                                  | Bumps the `transcribe-cpp` / `transcribe-cpp-sys` (`main`) and `tauri-plugin-*` (`tauri-apps/plugins-workspace`, `v3`) git pins in `Cargo.lock` when remote branches move; never fails, never blocks a build                                                                                                                                                       |
| `prune-target.ts`                  | `tauri-runner.ts` (imported), `bun run prune:target [--dry-run] [--verbose]`                   | Removes stale artifacts from `src-tauri/target` before every run: `deps` units and build-script outputs of versions / git revisions no longer in `Cargo.lock`, superseded incremental caches, older builds of the workspace crates (newest two kept), installers of another version. Nothing current is touched. `ZER0_NO_PRUNE=1` skips it                        |
| `check-translations.ts`            | `bun run check:translations`, CI                                                               | Compares every locale's key set with `en`                                                                                                                                                                                                                                                                                                                          |
| `check-nix-deps.ts`                | `postinstall`                                                                                  | Regenerates `.nix/bun.nix` via bun2nix when available (no-op on Windows). Re-run on a Nix machine after changing `package.json`                                                                                                                                                                                                                                    |
| `update-deps.ts`                   | `bun run update-deps [--prerelease]`                                                           | Bumps npm and Cargo dependencies with validation steps. A `cargo update` conflict is parsed, the direct crate at fault is held back with the reason printed once, and the final report lists held crates and transitive crates pinned behind latest. `libc` stays on 0.2 even with `--prerelease`. Run it from the repository root (paths resolve against the CWD) |
| `update-rtk.ts`                    | `bun run update:rtk`                                                                           | Updates the RTK CLI used by the maintainer's Claude Code hook — tooling, not part of the app                                                                                                                                                                                                                                                                       |
| `gen_catalog.py`                   | manual                                                                                         | Regenerates `src-tauri/src/catalog/catalog.json` (upstream tooling)                                                                                                                                                                                                                                                                                                |
| `app-meta.ts`                      | `meta:sync` / `meta:check` (gate, CI) / `meta:set <x.y.z>` / `meta:bump <major\|minor\|patch>` | The single source of identity and version. Rewrites the mirrors in `package.json`, `Cargo.toml` `[package]`, `Cargo.lock`'s root block, `tauri.conf.json`, `index.html`, `flake.nix` and `installer.nsi`, and generates `app_identity.rs`, `appIdentity.ts`, `nix/module.nix` and `nix/hm-module.nix`; `--check` fails on drift                                    |
| `check-identity.ts`                | `check:identity` (gate)                                                                        | Scans tracked files for the legacy name / identifier and, in `src/` + `src-tauri/src/` code, a hand-written current name; per-file exemptions with reasons                                                                                                                                                                                                         |
| `check-model-language-coverage.ts` | `check:model-languages` (CI code-quality; not in the gate)                                     | Every catalog model language code must map to exactly one frontend language intent                                                                                                                                                                                                                                                                                 |
| `pre-commit.ts`                    | `precommit` / `precommit:full` / `precommit:routine`, `.githooks/pre-commit`                   | The gate described above; `--full` adds clippy + cargo test, `--routine` runs `update` first. Stages only the files it rewrote                                                                                                                                                                                                                                     |
| `update-all.ts`                    | `bun run update [--dry-run]`                                                                   | `update:rtk` → `update-deps -- --prerelease` → `repomix`                                                                                                                                                                                                                                                                                                           |
| `repomix.ts`                       | `repomix` (gate, last) / `repomix:check`                                                       | Packs the tree with a pinned `repomix` into the gitignored `repomix-output.xml`; `--check` compares mtimes                                                                                                                                                                                                                                                         |
| `fetch-stack-docs.ts`              | `docs:fetch [--source <id>] [--limit N]`                                                       | Mirrors the stack's docs into the gitignored `docs/vendor/` and reports new / changed / removed pages ([docs/STACK_WATCH.md](docs/STACK_WATCH.md))                                                                                                                                                                                                                 |
| `bench-stt.ts`                     | `bench:stt -- --exe … --wav … --output … [--model …] [--baseline …]`                           | Replays a WAV through the headless `--transcribe-file --repeat 3 --json` path per installed model × {cpu, cuda}; flags regressions and transcript changes against a baseline                                                                                                                                                                                       |
| `gen-icons.ts`                     | `icons:generate [app\|tray]`                                                                   | App icons via `tauri icon` from an SVG built from `src/lib/brandMark.ts`; tray PNGs via the SDF rasteriser (`lib/sdf.ts`, `lib/png.ts`)                                                                                                                                                                                                                            |
| `png-inspect.ts`                   | manual                                                                                         | Prints a PNG as ASCII coverage / luminance maps and a colour histogram (decoded by Playwright's Chromium)                                                                                                                                                                                                                                                          |
| `lib/env-flag.ts`                  | imported                                                                                       | The `<PREFIX>` env-flag reader, mirroring `utils.rs` (legacy-prefix fallback with a warning)                                                                                                                                                                                                                                                                       |
| `mirror_models.py`                 | manual (`uv run`)                                                                              | Mirrors the catalog's GGUF files to R2 blob storage (upstream tooling)                                                                                                                                                                                                                                                                                             |
| `ci/stage-transcribe-libs.sh`      | nothing (see [docs/KNOWN_ISSUES.md](docs/KNOWN_ISSUES.md))                                     | Linux runtime-library staging helper, currently unreferenced                                                                                                                                                                                                                                                                                                       |

**Forked dependencies:** three crates in this graph are forks under
`NairoDorian` rather than their upstream projects (`tauri-specta`,
`tauri-plugin-macos-permissions`, `transcribe-cpp`) — upstream stopped
publishing the version this app runs on. Their source is **read and edited in
their own working copies** under `C:\Users\Z\Downloads\PROJECTS\` (never here,
and never in `src-tauri/target` or `~/.cargo/git/checkouts`): edit, build and
push to `NairoDorian` from that folder, then move the pin in ZER0. The folders,
the end-to-end procedure and the constraints that bite when bumping one are in
[docs/FORKS.md](docs/FORKS.md).

**Model Setup:** nothing to download for development. Voice activity
detection is pure Rust (Earshot, no model file), and speech models are fetched
from the in-app catalog on first run.

**Native build flags:** the two `.cargo/config.toml` files (root and
`src-tauri/`, so a cargo invocation from either directory picks one up) set
`rustflags = ["-C", "target-cpu=native"]` and nothing else — local builds use
the host's AVX2/FMA and BMI rather than baseline x86-64. Windows x86_64 and
Linux build the transcribe.cpp **`cuda`** feature (not upstream's `vulkan`),
macOS `metal`, Windows aarch64 CPU only; `dev:cpu` turns CUDA off at configure
time on top of that. Every lane packs the full family set:
`TRANSCRIBE_MODEL_SET=full` is set by the runner, not by a config file.

For detailed platform-specific build setup, see [BUILD.md](BUILD.md).

## Architecture Overview

ZER0 is a cross-platform desktop speech-to-text application built with Tauri 3 alpha (Rust backend + Solid 2 / TypeScript frontend — migrated from React on 2026-09-13; see the Solid 2 conventions under Code Style). The `tauri` crate is on the 3.0 alpha line, and the `@tauri-apps/*` JS packages are capped at its 3.x major.

### Backend Structure (src-tauri/src/)

- `lib.rs` - Main entry point, Tauri setup, manager initialization, the
  `collect_commands!` list (every `#[tauri::command]` must be listed there to
  reach `src/bindings.ts`), and the headless `--transcribe-file` path
- `main.rs` - CLI parsing, calls `lib::run`
- `managers/` - Core business logic:
  - `audio.rs` - Audio recording and device management (`AudioRecordingManager`:
    recording state machine, readiness generation, lazy mic close / idle
    timeout, in-place VAD threshold and denoise updates, `last_speech_ms()`)
  - `model.rs` - Model catalog, downloading (HF hub or mirror), quantization
    variants; `model/download.rs` is the HTTP downloader
  - `model_capabilities.rs`, `gguf_meta.rs` - Per-model capability detection
    and GGUF metadata parsing
  - `native_streaming_latency.rs` - Maps the latency preset onto the
    transcribe.cpp `StreamExtension` a model family supports
  - `transcription.rs` - Speech-to-text pipeline: primary engine, stream
    worker, `extra_engines` (Multi-STT), idle unload watcher
  - `history.rs` - Transcription history storage (SQLite), WAV files, vacuum
  - `statistics.rs` - Metrics tracking (independent SQLite table, streak calculation, stop-relative transcription and LLM post-processing latency distributions)
  - `arch_plugins.rs` - transcribe.cpp per-family architecture-plugin
    directories and listing (`list_arch_plugins`, `load_arch_plugin`,
    `register_arch_dir`, `init_arch_plugin_dirs`,
    `ensure_arch_plugin_for_model`); the Tauri commands `get_arch_plugins`,
    `load_arch_plugin` and `register_arch_dir` live in `commands/models.rs`,
    `open_plugins_folder` in `commands/mod.rs`. The `arch-dl` feature is off
    in every shipped posture, so it normally reports nothing
- `audio_toolkit/` - Low-level audio processing:
  - `audio/` - Device enumeration, recording, resampling, WAV read/write
    (`utils.rs`: `save_wav_file`, `save_raw_wav_file`, `read_wav_samples`,
    `verify_wav_file`). There is no level-meter path: the overlay draws the
    Live FFT scope
    - `device.rs` resolves the system default to the concrete endpoint
      (`default_input_endpoint` / `default_output_endpoint`) for the recorder,
      the channel query and the feedback sounds. cpal 0.18's virtual default
      handle activates through `ActivateAudioInterfaceAsync` and installs a
      device-change listener whose callbacks initialise COM on Windows' own
      notification thread; after the first callback every activation in the
      process failed with RPC_E_CHANGED_MODE ("Cannot change thread mode
      after it is set") until restart. A changed system default is therefore
      followed at the next open (idle close, stream rebuild), not live. A
      microphone picked in settings while a recording runs is persisted at
      once and switched to when that recording ends
      (`AudioRecordingManager::apply_pending_device_change`, one short-lived
      thread per deferred change), so capture never restarts mid-recording
    - `recorder.rs` also hosts `SpeechClock`, which measures how long the user
      has actually been speaking (see Speech Stats below)
    - `recorder.rs` also defines `AnalysisSink`, the trait the Live FFT tap
      implements: `CaptureProcessor` offers it the native-rate chunk (after
      the raw tap), the 48 kHz frames leaving RNNoise and the 16 kHz frames
      (before the VAD), each behind a `wants_*` atomic check. `Cmd::Start.discard_audio`
      keeps a session from accumulating audio it will never return (VAD test,
      Live FFT)
  - `vad/` - Voice Activity Detection — see Voice Activity Detection below;
    `earshot.rs` wraps the pure-Rust Earshot detector, `smoothed.rs` adds
    prefill / hangover / onset smoothing, `mod.rs` holds the `Hysteresis` gate
    and the millisecond timing constants
  - `audio/chunk_tap.rs` - The drainable mid-recording copy of the VAD-filtered
    16 kHz frames the experimental Multi-STT streaming mode decodes its chunks
    from, plus the session token that keeps a stale holder off the next
    recording's audio. Process-wide, like the Live FFT tap; inert while the
    mode is off
  - `lang_id.rs`, `text.rs` - Language-detection heuristics and text post-filters
  - `bin/cli.rs` - Standalone recorder demo. **Not a build target** (there is
    no `[[bin]]` entry in `Cargo.toml`, and it sits outside `src/bin/`, so
    Cargo does not discover it); keep it compiling by hand
- `commands/` - Tauri command handlers for frontend communication
  (`audio.rs`, `history.rs`, `models.rs`, `statistics.rs`, `transcription.rs`,
  `file_transcription.rs`, `live_mode.rs`, `live_fft.rs`, `llama.rs`,
  `recall.rs`, `system.rs` — `get_system_stats`, the latest
  `SystemStatsEvent` sample for a freshly mounted footer — and `mod.rs`)
- `cli.rs` - CLI argument definitions (clap derive)
- `app_identity.rs` - Generated identity constants (name, slug, identifier,
  env prefix, legacy names, recording file names) from `scripts/app-meta.ts`;
  never edit by hand
- `session_log.rs` - Freezes one durable log-file stem per app session
  (`<slug>-YYYYMMDD-HHMMSS-mmm`) before the log plugin opens its file; shared
  with `commands::current_log_file` (see [docs/LOGGING.md](docs/LOGGING.md))
- `webview_hardening.rs` - Disables WebView2's browser accelerator keys (F5,
  Ctrl+F, F12, …) on Windows: on the main window in every build profile
  (`disable_accelerators_now`), on the overlay windows in release builds only
  (`disable_browser_accelerator_keys`); adapted from AivoRelay (MIT)
- `shortcut/` - Global keyboard shortcut handling. `mod.rs` holds the
  settings-change commands and `should_register_binding`, the single rule for
  which bindings are live (feature gates + performance-mode conflicts);
  `tauri_impl.rs` (`tauri-plugin-global-shortcut`) and `native_keys.rs` (the
  `handy-keys` crate: a manager thread owns `HotkeyManager`, and a recording
  listener emits `native-keys-event` for the shortcut recorder; the persisted
  setting value stays `"handy_keys"`) are the two backends; `handler.rs`
  dispatches to actions
- `transcription_coordinator.rs` - Pure state machine that turns key presses
  / external inputs into start/stop/cancel decisions (toggle vs push-to-talk)
- `settings.rs` - Application settings management, defaults, schema
  migrations (includes Multi-STT settings)
- `overlay.rs` - Recording overlay window (platform-specific), plus
  `SpeechActivityEvent` and the cached emit gates for the readiness /
  speech-activity events
- `signal_handle.rs` - `send_transcription_input()` reusable function
- `actions.rs` - Shortcut action implementations: `TranscribeAction`, `MultiSttAction`, `CancelAction`
  - `MultiSttAction` runs primary + three extra models in parallel, merges outputs via LLM
  - `spawn_recording_ready_cue` is the shared "wait for real mic samples, then
    chime / mute / emit `recording-ready`" step
- `file_transcription.rs` - "Transcribe Files" page backend (fork):
  `decode_audio_file` (symphonia → mono 16 kHz), `segment_boundaries`
  (quiet-point cuts), `FileTranscriptionManager` job runner and its
  `FileTranscriptionEvent` progress events; see Transcribe Files below
- `live_mode.rs` - "Live Mode" page backend (fork): `LiveModeManager` chunk
  loop, `TranscriptWriter` (committed prefix + rewritten live tail),
  `LiveModeStateEvent` / `LiveModeTranscriptEvent`; see Live Mode below
- `live_fft/` - "Live FFT" page backend (fork): `mod.rs` holds the
  `AnalysisTap` (the wait-free ring the recorder pushes into), the
  `LiveFftManager` worker, the `LiveFftStateEvent` and the binary frame slot
  the page polls through `live_fft_frame`;
  `scope.rs` is the recording overlay's miniature analyser (same tap and
  pipeline, binary frames polled through `overlay_scope_frame`);
  `dsp.rs` is the pure DSP (FIFO, RBJ EQ, windows, warp + band
  aggregation, weighting, dB, ballistics, spectral features on a `realfft`
  transform); see Live FFT below
- `direct_stream_writer.rs` - Types the live transcript into the target app
  for `PasteMethod::DirectStreaming`; also the reconciler the experimental
  Multi-STT streaming mode retypes through; see Direct Streaming below
- `multi_stt_stream.rs` - The experimental Multi-STT streaming-first
  coordinator (fork): the break-delimited chunk model, the bounded merge window
  and the merge jobs that replace the live text in place; see Multi-STT
  Streaming First below
- `multi_streaming.rs` - The experimental **Multi Streaming STT** mode (fork),
  nested under streaming-first; see Multi Streaming STT below
- `overlay_preview.rs` - The Overlay page's "Preview" button: a real recording
  whose only output is the overlay (audio discarded, nothing pasted, saved or
  typed), started and stopped by `start_overlay_preview` /
  `stop_overlay_preview`
- `recall/` - "Recall" page backend (fork): the Markdown note vault. `mod.rs`
  is the vault and its notes, `crypto.rs` the optional encryption,
  `dictate.rs` records and returns text to the editor, `insertion.rs` routes
  hotkey transcriptions into the focused note; commands in
  `commands/recall.rs`; see Recall below
- `llama_server.rs` - In-app llama.cpp supervisor (fork): builds the
  `llama-server` command line from `settings.llama` (defaults = the
  maintainer's `launch_server_E2B_Q4.ps1`: Gemma 4 E2B Q4 + MTP draft, 8k
  ctx, temp 0.05 / top-p 0.35, reasoning off), spawns it detached with piped
  output into a 400-line ring buffer, polls `/health`, and emits
  `LlamaServerStateEvent`. `ensure_ready_for_provider` (called from
  `llm_client::send_chat_completion_with_schema`) starts it on demand when
  a request targets the local port. `adopt_detected_install_if_unconfigured`
  picks up an existing `<home>/Downloads/PROJECTS/Llama.cpp` layout on
  first run. When `stop_on_exit` is on, stopped on `RunEvent::Exit` and, on
  Windows, by a job object if the app dies; with it off the server is not
  tied to the job object and survives the app's exit
- `job_object.rs` - The Windows job object (kill-on-close) that ties
  `llama-server` to the app's lifetime when `llama.stop_on_exit` is on; a
  no-op elsewhere
- `llama_releases.rs` - GitHub release discovery (10-minute cache), backend
  detection via nvidia-smi, streamed download with `LlamaDownloadEvent`
  progress, pure-Rust zip extraction into `<app data>/llama_cpp/<backend>-<tag>`
  A CUDA build is ~180 MB; the ~500 MB `cudart` runtime package (cublasLt /
  cublas / cudart DLLs) is only fetched when `llama.include_cudart` is on —
  the same opt-in as the download script's `-IncludeCudart` — because a
  machine with the CUDA toolkit already has those DLLs. `detect_cuda_toolkit`
  looks in `CUDA_PATH` / `CUDA_PATH_V*` (`bin\x64` on CUDA 13, `bin` before),
  every PATH entry and the default install folder, and reports whether the
  folder is on PATH; `LlamaServerManager::start` prepends it to the child's
  PATH when it is not. `remove_bundled_cuda_runtime` trims the DLLs from an
  existing install
- `system_monitor.rs` - One sampler thread, 1 Hz, CPU + RAM (sysinfo) and
  GPU / VRAM / temperature (NVML when present) as `SystemStatsEvent`; while
  the main window is hidden it only wakes every 3 s to keep the CPU baseline
  fresh — no NVML query and no event
- `llm_client.rs` - OpenAI-compatible API client for post-processing and Multi-STT merge
  - Includes `erase_llama_server_conversations()` for llama.cpp conversation cleanup (only called for the `custom` provider)
- `clipboard.rs`, `paste_tx/` - Paste strategies (clipboard, direct typing,
  receipt-sequenced "reliable" paste on Windows/macOS), key-combo simulation
- `input.rs` - Keyboard simulation (`simulate_shortcut`) used by performance mode
- `secure_input.rs` - macOS Secure Input detection and fallback bindings
- `tray.rs`, `tray_i18n.rs` - System tray and its generated translations
- `autostart.rs`, `portable.rs`, `apple_intelligence.rs`, `audio_feedback.rs`,
  `memory.rs`, `helpers/clamshell.rs` - Platform helpers
- `catalog/` - Bundled model catalog (`catalog.json`)
- `utils.rs` - Platform detection helpers, `cancel_current_operation`, the
  `app_env_var` / `app_env_flag` readers
- `tests/vad_speech_clock_probe.rs` - Opt-in VAD regression probe (`ZER0_PROBE_WAV`)

### Frontend Structure (src/)

- `App.tsx` - Main component with onboarding flow; renders the per-page
  `QuickHelp` banner and the right-edge `HotkeySidebar`
- `stores/navigationStore.ts` - Active settings page + the Help anchor
  hand-off (`openHelp(anchor)`); `lib/anchorNavigation.ts` switches page,
  waits for the target element, scrolls, focuses and pulses
  `.settings-anchor-highlight` (App.css)
- `components/hotkey-sidebar/` - Shortcut cheat sheet as a top-right overlay:
  a keyboard button in the window corner toggles a panel of the assigned
  shortcuts (`lib/hotkeyGuide.ts` is the table of bindings → category /
  page / feature gate; `ShortcutInput` wraps each control in a
  `shortcut-<id>` anchor). Closes on Escape / click-outside / jump. Shows a
  "set your shortcut" call-out (and a warning dot on the button) instead of
  hiding when nothing is bound, since fresh installs ship without a
  transcribe hotkey
- `components/settings/llama/LlamaSettings.tsx` + `stores/llamaStore.ts` -
  Local LLM page (server control + logs, release install, model pickers,
  command-line editor with live preview); `footer/BrainIndicator.tsx` and
  `footer/SystemMeters.tsx` are the status-bar brain state and CPU/RAM/GPU/
  VRAM meters
- `components/settings/help/` - Help page (`helpContent.ts` is the list of
  sections/anchors, copy under `help.*`); `settings/QuickHelp.tsx` maps
  each page to a one-line summary and a Help anchor
- `lib/sessionToast.ts` + `stores/sessionToastStore.ts` - `sessionToast` is
  the app's toast API (rendered by `components/ui/Toaster.tsx` from
  `stores/toastStore.ts`; there is no sonner) and records error/warning
  toasts for the Debug page's `SessionToastHistory`; import it (as `toast`)
  in app code, so no error disappears unread
- `components/` - Solid UI components:
  - `settings/` - Settings UI, grouped by page (`general/`, `advanced/`,
    `models/`, `history/`, `statistics/`, `post-processing/`, `about/`, `debug/`) plus the
    individual setting components. Fork-added ones: `AccentColorSelector`,
    `AppendTrailingNewline`, `KeyComboInput` (performance-mode shortcut
    recorder), `MicIdleTimeout`, `SaveRawAudio`, `SpeechStats`,
    `VadSensitivity` (threshold slider), `PasteMethod` (direct streaming +
    speed), `ShowOverlay` (direct mode + speed)
    - `statistics/StatisticsSettings.tsx` - Analytics & streak dashboard (transcriptions, words, audio duration, WPM, streak, latency distributions)
    - `multi-stt/MultiSttSettings.tsx` - Multi-STT configuration (fork feature)
    - `file-transcription/FileTranscriptionSettings.tsx` - Transcribe Files page
      (fork feature): file/folder queue, mode & output options, run controls
    - `live-mode/LiveModeSettings.tsx` - Live Mode page (fork feature): session
      controls, live transcript preview, chunk list, past sessions
    - `overlay/OverlaySettings.tsx` - Overlay page (fork feature): appearance
      (`ShowOverlay`), speech stats (`SpeechStats`) and the analyser picture
      (`OverlayScopeGroup.tsx`, the nested `overlay_scope` setting; helpers in
      `lib/overlayScope.ts` shared with the overlay window)
    - `live-fft/LiveFftSettings.tsx` - Live FFT page (fork feature): analyser
      canvas + waterfall (`SpectrumCanvas.tsx`, `SpectrogramCanvas.tsx`),
      presets and every analyser parameter group (`ParamSlider.tsx`,
      `liveFftMath.ts`, `liveFftPresets.ts`)
    - `VadSensitivity.tsx` - Threshold slider for the Earshot VAD (fork)
  - `model-selector/` - Status-bar model controls (footer). Three mutually
    exclusive popovers: model switcher, quantization picker
    (`QuantizationPanel.tsx`, which also hosts the quantization benchmark via
    `useQuantBenchmark.ts`), and native streaming latency (`LatencyPanel.tsx`).
    There is deliberately no benchmark settings page — the benchmark lives
    beside the quant list it compares.
  - `onboarding/` - First-run experience
  - `whats-new/` - Release-notes modal (`releaseNotes.ts` globs
    `src/content/release-notes/*.md`)
  - `update-checker/` - App update notifications (`portableInstaller.ts`)
  - `shared/`, `ui/`, `icons/`, `footer/` - Shared components
- `hooks/useSettings.ts` - Settings state management hook; `hooks/useOsType.ts`
- `stores/settingsStore.ts` - Solid store (`createSolidStore`,
  `lib/solidStore.ts`) for settings. **Every settings key needs an entry in
  `settingUpdaters`** — a key without one logs `No handler for setting` and is
  never persisted
- `stores/modelStore.ts` - Model store: lists, selects, downloads, cancels
  downloads and deletes models (loading is the backend's job)
- `stores/fileTranscriptionStore.ts`, `stores/liveModeStore.ts` - Queue /
  session state of the two fork pages. They live outside the page components
  because both jobs keep running in the backend while the user browses other
  pages; each installs its typed event listeners once (`events.*.listen`)
- `stores/liveFftStore.ts` - Live FFT session state. Spectrum frames stay
  outside the reactive store (a module-level holder fed by the
  `live_fft_frame` poller; the canvases subscribe and paint only on a new
  frame), so a 60 Hz stream never re-renders the page
- `bindings.ts` - Auto-generated Tauri type bindings (via tauri-specta; written
  by `bun run tauri dev` in debug builds). When a command or settings field is
  added or removed, the file must be regenerated or hand-edited to match
- `overlay/` - Recording overlay window entry point (`RecordingOverlay.tsx`,
  `main.tsx`, `index.html`, `OverlayScope.tsx` for the miniature analyser);
  there is no `components/overlay/`
- `lib/types/events.ts` - Shared TypeScript event payload types;
  `lib/utils/{color,theme,keyboard,format,rtl,modelTranslation}.ts`,
  `lib/constants/languages.ts`

### Key Architecture Patterns

**Manager Pattern:** Core functionality organized into managers (Audio, Model, Transcription) initialized at startup and managed via Tauri state.

**Command-Event Architecture:** Frontend → Backend via Tauri commands; Backend → Frontend via events.

**Pipeline Processing:** Audio → (RNNoise, optional) → VAD → transcribe.cpp model → Text output → Clipboard/Paste

**State Flow:** Solid store → Tauri Command → Rust State → Persistence (tauri-plugin-store)

### Technology Stack

**Core Libraries:**

- `transcribe-cpp` - The only inference runtime: every model (Whisper family,
  Parakeet, Moonshine, Canary, Voxtral, Qwen3-ASR, … as GGUF/ggml) with GPU
  acceleration. There is no ONNX Runtime and no `transcribe-rs` in this fork
- `cpal` - Cross-platform audio I/O
- `earshot` - Voice activity detection (pure Rust, no model file; see Voice
  Activity Detection)
- `nnnoiseless` - RNNoise noise suppression (pure Rust, weights compiled in;
  optional, off by default)
- `enigo` - Keystroke simulation for the performance-mode shortcuts
- `handy-keys` (native backend, the default on Windows and macOS) /
  `tauri-plugin-global-shortcut` (the default on Linux) - Global keyboard
  shortcuts
- `rubato` - Audio resampling
- `rodio` - Audio playback for feedback sounds

### Application Flow

1. **Initialization:** App starts minimized to tray, loads settings, initializes managers
2. **Model Setup:** First-run downloads the chosen model from the bundled catalog (`catalog/catalog.json`: GGUF models only — Whisper family, Parakeet, Moonshine, Canary, …)
3. **Recording:** Global shortcut triggers audio recording with VAD filtering
4. **Processing:** Audio sent to Whisper model for transcription
5. **Output:** Text pasted to active application via system clipboard

**Multi-STT mode** (fork feature): When `multi_stt_enabled` is on, the dedicated `multi_stt_transcribe` shortcut triggers a parallel transcription pipeline:

1. Pre-loads extra models (if not already loaded) in parallel on the blocking thread pool
2. Records audio with VAD filtering (same as standard mode)
3. Transcribes with primary + three extra models concurrently on separate engines
4. Optionally merges outputs via an LLM (using `${output}`, `${output2}`, `${output3}`, and `${output4}` placeholders)
5. Saves history entry and pastes merged result concurrently

### Settings System

Settings are stored using Tauri's store plugin with reactive updates:

- Keyboard shortcuts (configurable, supports push-to-talk)
- Audio devices (microphone/output selection)
- Model preferences (Small/Medium/Turbo/Large Whisper variants)
- Audio feedback and translation options

### Voice Activity Detection

ZER0's only voice activity detector is **Earshot** (`earshot` crate, pure
Rust, ~8 KiB of state, constructed in microseconds — there is no model file,
no ONNX Runtime and nothing to download). It runs on the audio consumer thread
once per 16 ms frame and is what makes `vad_enabled` mean anything: frames it
scores below threshold never reach the decoder, so a recording carries speech
instead of speech-plus-silence. That matters more than it sounds —
Whisper-family models are prone to hallucinating text on silent audio, and
every second of dropped silence is a second the model doesn't spend decoding.

- **Frame size** is `constants::VAD_FRAME_SAMPLES` (256 samples = 16 ms at
  16 kHz, `VAD_FRAME_MS`). The capture pipeline frames itself to the
  detector's `frame_samples()` so one resampled frame is exactly one VAD
  decision (with `vad_enabled` off the recorder still carries the detector
  and frames stay 256 samples; the 480 / 30 ms fallback exists only for a
  recorder built without one, i.e. tests). Hangover, prefill and onset are
  millisecond constants in `vad/mod.rs` converted with
  `frames_for_duration_ms` (rounding up), so a change of frame size never
  shortens them: `VAD_STREAMING_HANGOVER_MS` = 1650 → 104 frames,
  `VAD_PREFILL_MS` / `VAD_OFFLINE_HANGOVER_MS` = 450 → 29, `VAD_ONSET_MS`
  = 60 → 4.
- **Input contract**: exactly 256 mono 16 kHz samples in `[-1, 1]`.
  `EarshotVad` clamps resampler overshoot for the prediction only (the audio
  passed downstream is untouched) and rejects non-finite samples and wrong
  frame sizes with an error rather than a silent wrong answer.
- **Energy pre-gate**: while not already in speech, a frame whose RMS is below
  −45 dBFS is scored 0 without running the model (a cost saving on deep
  silence). The live VAD test's "raw score" is that synthetic 0 for such
  frames, and a very quiet microphone needs gain, not a lower threshold.
- **Thresholding uses hysteresis** (`Hysteresis` in `vad/mod.rs`, a pure
  struct with its own unit tests): speech is entered at the threshold
  (`vad_threshold_earshot`, default 0.5) but only left once the score falls
  below `max(threshold - 0.15, 0.01)`. A single value for both edges makes a
  signal hovering near it flap frame to frame, which shows up directly as a
  stuttering speech/silence indicator and a speech clock that stalls mid-word.
- `SmoothedVad` wraps the detector with prefill / hangover / onset smoothing
  (`VadPolicy::Streaming` keeps the long hangover so a live decoder keeps
  receiving audio across a pause; `Offline` uses the short one).
- `handle_frame` treats a per-frame VAD error as "keep this audio" (losing
  speech is worse than keeping silence) but logs the first failure of each
  recording — a VAD that fails on every frame otherwise looks exactly like a
  VAD that is switched off. That failure mode is real: before 2026-08-26 the
  fork ran a Silero v6 model through a v4 wrapper, every frame errored, and
  VAD silently degraded into a pass-through for an unknown period. It only
  surfaced when Speech Stats became the first consumer of the raw per-frame
  verdict, which a pass-through cannot fake.
- **History**: until 2026-09-10 the app shipped Silero VAD v6.2 through `ort`
  with Earshot as an optional backend. Measured side by side on the
  maintainer's recordings (18 s, quiet room) the two agreed on 97.7 % of
  frames and kept identical audio (13.3 s), with Earshot ≈ 2–3× cheaper per
  frame and 109 ms → 9 µs to construct, so Silero, `ort`, `ndarray` and the
  2.2 MB model file were removed (see CHANGELOG, the transcribe.cpp-only
  change of 2026-09-10). Noisy rooms were never benchmarked; if Earshot clips
  speech there, lower `vad_threshold_earshot` first — unless the speech is
  below the −45 dBFS pre-gate, which no threshold reaches. `earshot` is pinned to `opt-level = 3` in
  the dev profile so debug builds pay the real (small) cost.

**Recordings with no speech never reach a decoder.** `SpeechClock` publishes its
running total to an `Arc<AtomicU64>`, which `AudioRecorder::speech_ms()` and
`AudioRecordingManager::last_speech_ms()` expose; both shortcut actions check it
alongside `samples.is_empty()` and skip transcription below
`MIN_SPEECH_MS_TO_TRANSCRIBE` (200 ms, `actions.rs`). This is not just a saved
GPU decode: Whisper-family models hallucinate confidently on silence, and that
invented text would be pasted into whatever the user was typing in. The
threshold sits deliberately below silero-vad's reference 250 ms
`min_speech_duration_ms` so a clipped "yes" still transcribes, and with VAD
disabled every frame counts as speech, so it can never suppress a recording
made with filtering off.

`src-tauri/tests/vad_speech_clock_probe.rs` is an opt-in regression check: point
`ZER0_PROBE_WAV` at a 16 kHz mono speech recording and it asserts the real
chain reports a sane fraction of voiced frames. It skips when the variable is
unset.

**Speech Stats** (fork addition): the recording overlay can show a
speech/silence indicator, a timer that runs only while you are actually talking,
and the running average words per minute.

- Driven by `SpeechClock` in `audio_toolkit/audio/recorder.rs`, fed by
  `VoiceActivityDetector::last_frame_voiced` — the **raw** per-frame Earshot
  verdict, deliberately _not_ what `push_frame` returns. `SmoothedVad` keeps
  reporting speech through a hangover tail of ≈1.66 s in streaming mode
  (`VAD_STREAMING_HANGOVER_MS` = 1650 rounded up to 104 Earshot frames), and
  counting that would add more than a second of phantom speech per pause.
- Gaps shorter than `speech_pause_hold_ms` are billed as part of the same
  utterance once speech resumes; longer gaps are dropped. The clock therefore
  only moves forward — it catches up rather than rewinding.
- Reaches the overlay as `SpeechActivityEvent`, emitted on speaking/silent flips
  and on a ~150 ms heartbeat while speech continues (never during silence), so it
  adds a few events per second at most. Gated on both
  `OVERLAY_ENABLED` and `SPEECH_STATS_ENABLED` to keep the issue #1279 emit path
  quiet when the overlay is off.
- Words per minute is computed in `RecordingOverlay.tsx` from the live
  `StreamTextEvent` word count, so it needs a streaming-capable model. It works
  in the Minimal overlay too, because `start_stream()` follows model capability
  rather than overlay style. Non-streaming models show the timers and the
  indicator with `—` in place of the rate.
- With VAD disabled every frame counts as speech, so the speech clock degrades
  into a wall clock instead of needing a separate UI case.

Settings:

- `overlay_speech_stats` - Show the stats in the Minimal and Live overlays
- `speech_pause_hold_ms` - Pause tolerance (10-2000 ms, default 500)
- `save_raw_audio` - Save raw uncompressed microphone audio before resampling and VAD filtering

**Raw Uncompressed Audio Recording** (fork addition):
When `save_raw_audio` is enabled in Settings $\rightarrow$ Advanced $\rightarrow$ History, the app saves audio in its native captured format:

- **Pre-resampling / pre-VAD capture**: Raw samples are tapped on the audio consumer thread (`CaptureProcessor::process_raw_chunk`) after the callback's channel selection/averaging (so the file is always mono) but before noise suppression, resampling to 16 kHz and VAD filtering. The tap only accumulates when the setting is on — at 48 kHz float it is ~11 MB per minute.
- **Format follows the opened stream**: Saves 32-bit float (`F32`), 24-bit PCM (`I32`), or 16-bit PCM (`I16`) WAV files at the sample rate and format `get_preferred_config` opened the device with (F32 preferred, then I16, then I32; WASAPI shared mode is typically F32 at the mix rate, e.g. 48 kHz).
- **Zero latency impact**: WAV serialization runs asynchronously on background blocking threads via `tauri::async_runtime::spawn_blocking`, allowing model transcription inference to execute immediately in parallel with no latency overhead. Both actions still await and `verify_wav_file` the result before recording a history row.
- **Universal reader**: `read_wav_samples` decodes 16-bit, 24-bit, and 32-bit float WAVs, automatically downsampling to 16 kHz for playback, acoustic model inference, and benchmarks.

**Multi-STT settings** (fork addition):

- `multi_stt_enabled` - Global toggle for multi-model transcription mode
- `multi_stt_model_2` / `multi_stt_model_3` / `multi_stt_model_4` - Select extra STT models
- `multi_stt_language_model_2` / `multi_stt_language_model_3` / `multi_stt_language_model_4` - Per-model language override
- `multi_stt_translate_model_2` / `multi_stt_translate_model_3` / `multi_stt_translate_model_4` - Per-model English translation
- `multi_stt_keep_extra_models_loaded` - Keep extra models resident between uses (default on). Only consulted when `model_unload_timeout` is `Immediately`; with any other timeout the idle watcher unloads extra engines together with the primary model
- `multi_stt_merge_prompt` - LLM prompt for merging outputs (`${output}`, `${output2}`, `${output3}`, `${output4}`; `${output1}` is an alias of `${output}`)
- `multi_stt_performance_mode_enabled` / `multi_stt_performance_mode_trigger_on_start` - Simulate a "full power" shortcut when a Multi-STT recording ends (or starts, with trigger-on-start) and a "normal" shortcut after the merge/paste, for external power-profile tools
- `multi_stt_performance_mode_full_power_shortcut` (default `ctrl+space`) / `multi_stt_performance_mode_normal_shortcut` (default `ctrl+alt+space`) - The simulated key combinations. While performance mode is enabled, a transcription hotkey equal to either is not registered (`shortcut::should_register_binding`) and the shortcut recorder rejects it, so the simulated keys can never retrigger the app
- `multi_stt_streaming_first_enabled` (default off) / `multi_stt_streaming_pause_ms`
  (default 1000, settable 100–10000) / `multi_stt_streaming_context_chunks`
  (default 1, settable 0–3) - The experimental streaming-first mode: the pause
  that ends a chunk, and how many already-closed chunks are re-run with it for
  accuracy (see below). All three take effect mid-session, refreshed every 2 s:
  turning the enable flag off while recording releases the session on the next
  refresh — it retires the same way the text-catchup grace does, handing the
  overlay back the primary's own live text — so a mode the user just switched
  off cannot keep closing chunks at every pause
- `multi_stt_streaming_multi_enabled` (default off) - The nested Multi
  Streaming STT mode under streaming-first (see below)
- `multi_stt_streaming_multi_debug_view` (default off) - That mode's debug
  view: one overlay block per live model plus the merged block, instead of the
  single corrected text

Extra models are managed by `TranscriptionManager` (`extra_engines` HashMap) with explicit
load/unload lifecycle, separate from the primary model. They are unloaded when Multi-STT is
turned off, when their slot changes, when the model is deleted, and by the idle watcher.

**Shortcut defaults** (fork): `transcribe` and `multi_stt_transcribe` ship with an empty
`current_binding` (`default_binding` is kept for "reset"), and settings schema migrations
3/4 cleared existing users' bindings once. Fresh installs therefore have no transcribe
hotkey until the user sets one — a deliberate consequence of the performance-mode design.

**Other fork settings:**

- `paste_method` gained `direct_streaming`; `direct_streaming_speed` (10–60) controls the typing rate (see Direct Streaming below for when it applies)
- `overlay_direct_mode` / `overlay_direct_speed` - Live overlay
  character-by-character mode. The experimental Multi-STT streaming mode's
  preview is exempt: its updates are whole blocks, not a reveal (see below)
- `overlay_back_correction` (default off) - Whether that typewriter may rewrite
  text it has already revealed. Off, a revision that is not an extension of what
  is on screen lands as one block; on, the reveal rewinds to the longest common
  prefix and retypes. Deliberately independent of
  `multi_stt_streaming_first_enabled` — that flag decides whether a batch pass
  re-transcribes each pause, this one only whether the display may move
  backwards. A model that re-attends over its whole audio context (R2T2)
  replaces its volatile `tentative_text` tail on every decode, so leaving this
  off is what keeps such a preview from stuttering backwards
- `save_raw_audio`, `overlay_speech_stats`, `speech_pause_hold_ms` - see Voice Activity Detection below
- `mic_idle_timeout_value` / `mic_idle_timeout_unit` / `mic_idle_infinite` - Lazy microphone close timeout (was a fixed 30 s upstream)
- `append_trailing_newline` - Like `append_trailing_space`, with a newline
- `custom_accent_color` - `#rrggbb` or `null` for the neon-cyan default; persisted through `change_custom_accent_color_setting`
- `native_streaming_latency_presets` - Per-model-family latency preset (`fastest` → `accurate`)
- `native_streaming_chunk_ms` - Per-model R2T2 stream chunk in milliseconds
  (80–2000, default 320 when absent); R2T2's latency control is continuous,
  so it is a separate map from the presets
- `per_model_backends` - Per-model backend override keyed by model id; an
  absent or `Auto` entry follows `transcribe_accelerator`. Applied on the
  model's next load
- `shortcut_activation` (`toggle` / `push_to_talk` / `hold_or_toggle`, default
  hold-or-toggle) / `hold_threshold_ms` (default 300) - How a hotkey press is
  read; in hold-or-toggle a press held at least the threshold is push-to-talk,
  a shorter tap locks recording on. Replaces the old `push_to_talk` bool
- `ui_scale` (0.7–1.6, default 1.0) - CSS zoom of the settings window
- `overlay_window_fade_ms` (0–2000, default 300) /
  `overlay_window_corner_radius` (0–25, default 0) - The overlay window's
  fade and corner radius
- `reliable_paste` (debug-gated, macOS and Windows) - Receipt-sequenced paste
  (`paste_tx`): restore the clipboard only after the target app has read the
  transcript, instead of after a fixed delay
- `recall` - One nested `RecallSettings` struct (`output_dir`, the vault root;
  default `<app data>/recall`); see Recall
- `vad_threshold_earshot` (default 0.5) - Speech-probability threshold the
  detector is built with (0.05–0.95; lower = more sensitive).
  `create_audio_recorder` reads it, so `change_vad_threshold_setting` writes
  the setting first and then calls
  `AudioRecordingManager::set_vad_threshold`, which swaps the detector's
  hysteresis gate in place (`VoiceActivityDetector::set_threshold`) — no
  rebuild, no microphone reopen, works mid-recording. The Advanced page shows
  it as the `VadSensitivity` slider
- **Noise suppression** (`denoise_enabled`, default off): RNNoise via the
  pure-Rust `nnnoiseless` crate (`audio_toolkit/audio/denoise.rs`,
  `DenoiseChain`: native rate → 48 kHz 10 ms frames → RNNoise → 16 kHz VAD
  frames). It sits in `CaptureProcessor::process_raw_chunk` after the raw
  tap and before `handle_frame`, so the VAD, the
  speech clock, streaming and the model all get the denoised signal while
  saved raw audio and the native analysis tap stay untouched. The flag is an
  `Arc<AtomicBool>` read per chunk (`AudioRecorder::set_denoise_enabled`,
  `AudioRecordingManager::set_denoise_enabled`,
  `change_denoise_enabled_setting`): a toggle mid-recording swaps between
  the direct and the denoised chain and resets the one it leaves, which is
  what lets the live VAD test react to the switch. The chain is built lazily
  on first use. A 48 kHz microphone (the WASAPI default) makes the first
  stage pure framing, so only one real resample happens either way. Three
  tunables shape RNNoise's output per 10 ms frame, read from atomics
  (`DenoiseControls`, pushed by `change_denoise_{strength,vad_threshold,
vad_grace}_setting`): `denoise_strength` (wet/dry mix), `denoise_vad_threshold`
  (RNNoise's own speech probability below which the frame is muted; 0 = off)
  and `denoise_vad_grace_ms` (hold after the last frame above it); the gate
  slews its gain over ~3 frames. The probability rides along in
  `VadFrameReport::denoise_prob` / `VadTestEvent::denoise_prob` for the meters.
  The Live FFT source `FftSource::Denoised` taps the 48 kHz frames as they
  leave RNNoise (`AnalysisSink::wants_denoised`), or the untouched chunk while
  suppression is off, so toggling it is a direct A/B in the spectrum
- **Live VAD test** (`VadLiveTest.tsx`, next to the slider): `start_vad_test`
  records under the `vad_test` binding with `VadPolicy::Streaming` and no
  model; the recorder's `with_vad_frame_callback` reports every frame's raw
  score (`last_frame_score`), hysteresis verdict, smoothed kept/dropped state
  and peak level, and the manager emits every second one as `VadTestEvent`
  while a `VAD_REPORT_FLAGS` bit is set (the Advanced test or the Live FFT
  page's voice-detection view; never during dictation). `stop_vad_test`
  cancels the recording and discards the audio; a 5-minute safety thread in
  `commands/audio.rs` stops a test the page forgot. Hotkeys get "Already
  recording" while it runs, and the cancel hotkey ends it (the page notices
  the missing frames and shows "No signal")
- **Retired settings** (`vad_backend`, `ort_accelerator`,
  `vad_threshold_silero`) are ignored when found in an old store. Settings
  schema **6** remaps a `selected_model` / `multi_stt_model_2..4` that names
  one of the 11 retired hard-coded ONNX models (`parakeet-tdt-0.6b-v2/v3`,
  `moonshine-*`, `sense-voice-int8`, `gigaam-v3-e2e-ctc`, `canary-*`,
  `cohere-int8`) to its GGUF successor in the catalog
  (`settings::legacy_onnx_model_replacement` →
  `catalog::default_id_for_repo`). Model files on disk are never deleted;
  the old ONNX directories under the models folder are simply no longer
  listed
- `llama` - One nested `LlamaSettings` struct (server folder, model / draft /
  mmproj paths, port, context, sampling, alias, extra or custom args,
  autostart / start-on-demand / stop-on-exit, backend, channel), persisted
  through `change_llama_settings`; see `llama_server::build_args`
- `file_transcription` - One nested `FileTranscriptionSettings` struct (mode,
  output_dir, output_format, overwrite_existing, include_subfolders,
  max_segment_minutes) persisted through a single command; see Transcribe Files
- `live_mode` - One nested `LiveModeSettings` struct (output_dir, chunk_minutes,
  transcript_format, granularity, save_audio, prefer_silence_boundary); see
  Live Mode
- `overlay_scope` - One nested `OverlayScopeSettings` struct: which of the
  overlay's two views are drawn, the spectrum style, the waveform window and
  its edge fade in samples, the auto-gain floor and the view size; persisted
  through `change_overlay_scope_settings`, which also refreshes the cached
  window geometry (`overlay::update_overlay_scope_cache`) and hands the
  waveform window to the running scope
- `live_fft` - One nested `LiveFftSettings` struct: the analyser
  parameters (source, raw bins, scale, warp + aggregation, output bins and
  their Auto/Fixed mode, window length, zero-padding, EQ, window & Kaiser β
  mode, weighting, loudness & ballistics, spectral features, async analysis,
  update rate) persisted
  through `change_live_fft_settings`; see Live FFT

### Transcribe Files (fork addition)

The **Transcribe Files** page turns audio files into `.txt` / `.md` transcripts.
Backend: `file_transcription.rs` + `commands/file_transcription.rs`; frontend:
`settings/file-transcription/` + `stores/fileTranscriptionStore.ts`.

- **Input**: any number of files, or whole folders (optionally recursive),
  added through the dialog plugin or OS drag & drop onto the window
  (`getCurrentWebviewWindow().onDragDropEvent`). Supported extensions are the
  `SUPPORTED_EXTENSIONS` list (WAV, MP3, M4A/MP4/AAC, FLAC, OGG-Vorbis),
  mirrored in `SUPPORTED_AUDIO_EXTENSIONS` on the frontend. Decoding is done
  with `symphonia` (the same crate + features `rodio` already pulls in, so no
  new transitive dependency), downmixed to mono and resampled to 16 kHz with
  `FrameResampler`; `.wav` falls back to `hound` if the probe fails.
- **Segmentation**: `segment_boundaries` cuts audio longer than
  `max_segment_minutes` at the quietest 100 ms window in the 20 s before each
  hard boundary, so a one-hour file becomes ~6 decode calls instead of one and
  a cut never has to land mid-word. Segment texts are joined with single spaces.
- **Modes** (`FileTranscriptionMode`): `simple` (primary model only),
  `post_process` (then `actions::process_transcription_output(.., true)`),
  `multi_stt` (primary + the Multi-STT extra models per segment, merged with
  `actions::multi_stt_merge_transcriptions`; newline-concatenated when no merge
  prompt / provider is configured) and `multi_stt_post_process` (merge, then
  the post-processing prompt). The page warns when the chosen mode needs a
  feature that is switched off, but still runs with what is available.
- **Output**: `<stem>.txt|md` next to the source or in `output_dir`; without
  `overwrite_existing`, `write_without_overwrite` appends `-2`, `-3`, … using
  `create_new` so the existence check and the create are one step. Markdown
  gets a one-line header (file, mode, model, date).
- **Job lifecycle**: `FileTranscriptionManager::start` refuses to run while a
  recording or Live Mode is active, then spawns the job on the async runtime
  with every blocking step (decode, load, transcribe) on the blocking pool.
  Progress is streamed as `FileTranscriptionEvent` (one per status change per
  file, matched on `path`; the last event carries `batch_finished`). Cancel is
  a flag checked between files and between segments — an in-flight decode call
  finishes first. The primary engine is reloaded before each segment when the
  unload timeout is `Immediately`, exactly like the headless path does.
- File transcriptions do not create history rows or statistics runs.
- **Known limitation**: the job refuses to start during a recording, but not
  the reverse — a hotkey dictation started while a file job runs competes for
  the primary engine, and one of the two can fail with "Model is not available
  for transcription (unloaded or in use)".

### Live Mode (fork addition)

The **Live Mode** page keeps the microphone open indefinitely, saves the raw
signal in chunked WAV files and mirrors the live transcription into a text file
while you speak. Backend: `live_mode.rs` + `commands/live_mode.rs`; frontend:
`settings/live-mode/` + `stores/liveModeStore.ts`.

- **Requires a streaming-capable model** (`ModelInfo::supports_streaming`);
  `live_mode_start` refuses otherwise and the page shows why. It also refuses
  while a hotkey recording or a file-transcription job is running, and vice
  versa.
- **One session = one folder** `<output_dir>/live_<YYYY-MM-DD_HH-MM-SS>/`
  (default `<app data>/live_mode`) holding `transcript.txt|md` and
  `chunk_0001.wav`, `chunk_0002.wav`, …
- **Chunk loop** (`LiveModeManager::run_session`, its own thread): each chunk
  is a normal recording — `try_start_recording_with_raw(.., Some(save_audio))`
  sets the native-rate raw tap to the session's `save_audio` option (on when
  it is on, off otherwise), independent of `save_raw_audio`, and
  `start_stream(false, ..)` runs the model's native live stream. A chunk ends
  at `chunk_minutes`, or (with `prefer_silence_boundary`) on the first ≥1.2 s
  pause after 80 % of it, as measured by `last_speech_ms()` standing still.
  Rotation = `finalize_stream` → `stop_recording` → WAV written on a blocking
  thread (raw samples at native rate/format via `save_raw_wav_file`, or the 16
  kHz STT samples if the raw tap is empty) → text committed → next chunk
  starts. Audio spoken during the finalize and restart (plus a batch decode,
  below) is not captured; the WAV write adds nothing. If the stream produced
  no text (`NeverStarted`, failed or timed out), the chunk's STT samples are
  batch-transcribed instead: a chunk the batch fallback rescues counts as
  `Success`, and one whose fallback decode fails too is `Failed`. VAD policy follows `vad_enabled`
  (`Streaming` or `Disabled`).
- **Transcript file** (`TranscriptWriter`): an append-only committed prefix
  plus a live tail rewritten in place (`seek` + `write` + `set_len`) on every
  stream update, throttled to one write per 60 ms. `live_tail_text` decides
  what the tail is: `character` = `committed + tentative` (identical to
  transcribe-cpp's `StreamText::display()`), `word` = the committed text cut at
  its last whitespace while anything is still tentative. Each finalized chunk
  is committed followed by `\n` (`.txt`) or `\n\n` (`.md`).
- **Live text plumbing**: `TranscriptionManager::set_stream_text_sink` installs
  an in-process observer that `emit_stream_text` calls alongside the overlay
  `StreamTextEvent`; Live Mode owns it for the session and clears it on stop.
  The UI receives `LiveModeTranscriptEvent { reset, stable_appended, live }` and
  `LiveModeStateEvent` (status snapshot on every transition + a 1 s heartbeat).
- **Interactions**: while a session runs, the transcription hotkeys get
  "Already recording" from the audio manager. The cancel hotkey / tray cancel
  stops the recorder under Live Mode; the loop notices (`!rm.is_recording()`),
  loses that chunk's audio, logs a warning and starts the next chunk. Each chunk
  is a `begin_normal_run("live_mode")` statistics run that records the chunk's
  speech duration before finalizing, so Live Mode sessions do show up in
  Statistics.

### Live FFT (fork addition)

The **Live FFT** page is a real-time spectrum analyser for the microphone.
Backend: `live_fft/mod.rs` (plumbing) + `live_fft/dsp.rs` (DSP, unit-tested) +
`commands/live_fft.rs`; frontend: `settings/live-fft/` + `stores/liveFftStore.ts`.

- **DSP** (`dsp.rs`, `SpectrumPipeline`): stage order — FIFO of the
  last `window_samples` → RBJ high/low shelf EQ applied at **ingest** to new
  samples only (stateful, time order) → window (Kaiser / Hann / Hamming /
  Blackman / Blackman-Harris / Rectangular, coherent-gain or full-scale
  normalisation; Kaiser β manual or derived from `db_range`) centred
  8-float-aligned in a zero-padded frame (`zero_padding` off: the transform
  is the window, rounded up to even) → `realfft` R2C → magnitude
  (`sqrt(re²+im²)`) of only the bins the warp reads → psychoacoustic warp (Log /
  Mel / ERB / Bark / Chroma / Linear / Melog, `warp_blend`, linear or
  Catmull-Rom, memcpy when the grid is exactly 1:1) with **band aggregation**
  (`warp_aggregation` Peak / RMS over the FFT bins an output bin owns, so a
  coarse bin never drops a narrow partial) → A / C / ITU-R 468 weighting → dB
  against frame peak / 0 dBFS / slow AGC (`FastLog2Seg`, an 8-segment
  quadratic of the mantissa, < 0.0002 dB; the portable fallback uses a
  2048-entry mantissa table) → attack/release ballistics (per-frame
  coefficients or milliseconds, in place). Output bins: Fixed
  (`output_bins`, 8–65536, interpolated above N/2+1), Auto (N/2+1 of the
  transform run) or `raw_bins` (the rfft magnitude itself, identity copy).
  `spectral_features` adds centroid, 85 % rolloff, flatness, flux, RMS and
  bass/mid/high band levels from the linear magnitude. A digitally silent
  window skips the FFT in every mode and yields exactly what the full chain
  would (`silence_shortcut_equals_the_full_chain_bit_for_bit`). Steady-state
  `ingest` + `process` allocate nothing, pinned like Plugin_FFT's allocation
  gate by a test-only counting allocator
  (`steady_state_ingest_and_process_allocate_nothing`). The page's Reset
  clears the EQ state before the next chunk is filtered. This is a
  port of the owner's Plugin_FFT (TouchDesigner CHOP) v2.11, minus what does
  not apply here (channel modes, FFTW planner / wisdom / backend, worker
  priority), including v2.12's measured performance pass. Three kernels are
  explicit AVX2 + FMA (`std::arch`, runtime-detected by `avx2_fma`, each
  tested against its portable twin): the warp's load + permute, the dB
  conversion and the feature band sums. Everything else is portable code that
  auto-vectorises; a kernel was only made explicit when an interleaved A/B
  measured it faster (the magnitude stayed on `sqrt`: every AVX2 variant,
  rsqrt + Newton included, was slower). `realfft` was
  already a transitive dependency (rubato), so no new crate is compiled.
- **Threading** ("Async analysis" on the page): the audio consumer thread
  pushes each chunk into a wait-free `rtrb` ring through `AnalysisTap` (an
  `AnalysisSink`; one atomic load per chunk while the page is closed, a
  `try_lock` on an uncontended mutex plus a memcpy while it is open — the
  consumer thread never waits). The `live-fft` worker drains the ring, runs
  the pipeline at `update_rate_hz` (5–60) and encodes each frame into a
  binary slot (8-word header: seq, flags, bin count, axis version, peak Hz,
  peak value, DSP µs, feature count; then f32 bins and features) that the page
  polls with `live_fft_frame(known_seq)`, a raw `tauri::ipc::Response` command
  like `overlay_scope_frame`: an unchanged seq answers with the 32-byte header
  only. The frequency axis is fetched with `live_fft_axis` only when a frame's
  axis version changes. With `async_analysis` off the pipeline runs inline on
  the consumer thread (every lock there is a `try_lock`, nothing is emitted)
  and the worker only supervises.
  The tap is a process-wide `LazyLock` so the always-on microphone, opened on
  its own thread during startup, is wired to it before the manager exists.
- **Gates**: frames are neither computed nor sent while the main window is
  hidden; a session hidden for 2 minutes is stopped (`LiveFftStopReason::
WindowHidden`); the page stops the session on unmount; the worker notices
  when the recording ends elsewhere (cancel hotkey → `Cancelled`).
- **Session**: a normal recording under the `live_fft` binding with
  `VadPolicy::Disabled` and `RecordingStartOptions { discard_audio: true }`,
  so the hotkeys get "Already recording" (like the VAD test) and nothing is
  accumulated, transcribed or saved. `cancel_recording_if_binding` ends only
  a recording that is ours. `FftSource::Processed` taps the 16 kHz frames a
  model hears instead of the native-rate microphone. With `show_vad` the
  session records under `VadPolicy::Streaming` instead and sets the
  `VAD_REPORT_LIVE_FFT` bit of `VAD_REPORT_FLAGS`, so the recorder streams
  the same `VadTestEvent`s as the Advanced page's live test; the page's
  "Voice detection & noise suppression" group (`VoiceDetectionGroup.tsx`)
  shows the shared `VadMeter` beside the spectrum together with the VAD
  toggle, the threshold slider and the RNNoise toggle. Toggling `show_vad`
  restarts a running session, like a threading change.
- **Presets**: the page's "Raw" preset (`live_fft_raw_defaults`) restores
  linear magnitude, frame-peak reference and no ballistics; the app's own
  defaults differ only in the display choices (dB / 0 dBFS / 90 dB,
  ballistics 15 ms / 250 ms). The transform defaults and limits are
  Plugin_FFT's: zero-pad length 16384 (a store that already holds a length
  keeps it), window 1…65536 samples or 0.1…5000 ms. There is no FFT planner
  policy (rustfft plans instantly), no poll interval (settings are pushed)
  and no channel menu
  (the app's capture ring is already mono); the analysis frame rate is
  `update_rate_hz`.
- **Recording overlay scope** (`scope.rs`, `overlay/OverlayScope.tsx`): the
  overlay's old 16-bucket level bars are replaced by a miniature area
  spectrum and a line of the last 4096 samples, computed with the page's
  `settings.live_fft` (scale, warp, window, EQ, weighting, loudness, dB
  range, ballistics, update rate; `async_analysis` is ignored, the scope
  always has its own `overlay-scope` thread). `show_overlay_state` starts it
  for the recording / streaming states and stops it for the working states
  and on hide; it also stops itself when the recording ends, and never
  starts while the page session owns the tap. The 4096-sample window is
  read from a `WaveRing` with 512-sample raised-cosine ramps at both ends
  (`taper_table`) so the trace starts and ends at zero. Frames are encoded
  by `encode_scope_frame` (8-word header + f32 bins + f32 wave, little
  endian) and returned as `tauri::ipc::Response` by `overlay_scope_frame`,
  a command registered beside the typed ones in `lib.rs` because
  tauri-specta cannot type raw bytes; the overlay polls it at
  `update_rate_hz` with its last seq (header-only reply when unchanged; the
  block and circular-background instances share one poll) and maps
  `Float32Array` views onto the buffer. The scope computes at most 8192 bins
  (`OVERLAY_MAX_BINS`; a capped Raw grid becomes a peak-aggregated linear
  axis) whatever the page asks for. The
  spectrum is the page's spectrum: the scope engine runs the same
  `SpectrumPipeline` on the same `Shared` settings snapshot (EQ shelves,
  window, weighting, scale / warp, dB reference and range, ballistics,
  source, update rate), and `scope_bins_equal_the_page_pipeline_for_the_same_settings`
  asserts the bins are bit-identical. Only the picture follows
  `overlay_scope` (Overlay page): which views, area / line / bars, the
  mirrored (centred) spectrum, peak hold, the waveform window and fade (the
  scope rebuilds its `WaveRing` when the setting changes), the auto-gain
  floor and the view size. The overlay
  publishes the geometry as CSS variables before it shows and `overlay.rs`
  sizes the native window from the same numbers through a cached atomic, so
  the two never disagree.
- **Frontend**: `SpectrumCanvas` (bars / line / area, peak hold, grid, hover
  readout with note name, peak marker) and `SpectrogramCanvas` (waterfall,
  inferno / accent / ice colormaps, a ring of rows rather than a scrolling
  blit) paint on demand — a new frame, hover, resize or a prop change — from
  one shared units pass over the frame holder in `liveFftStore.ts`; peak-hold
  and ceiling decays are per millisecond. The status event (≤ 1 Hz) carries
  telemetry only (Hz per bin, magnitude bins, aggregated bins, effective β,
  latency). Plugin_FFT's quality presets Visual 60 / Visual 120 / Analysis
  are one-shot buttons, not a persisted mode, and change only the transform
  size, interpolation, β mode and aggregation — never the output size
  (`liveFftPresets.test.ts`).
  Display preferences live in localStorage (`live_fft.view` under the app's
  storage prefix, through `readPref`, which falls back to the pre-rename
  prefix once); everything else is `settings.live_fft`.

### Direct Streaming (fork addition)

`paste_method = direct_streaming` types the live transcript into the foreground
app while the user speaks (`DirectStreamWriter`, driven by the stream worker in
`managers/transcription.rs`). It applies to **plain transcription only**:

| Mode                              | Live stream                                                           | Final text                           |
| --------------------------------- | --------------------------------------------------------------------- | ------------------------------------ |
| `transcribe` (no post-processing) | typed into the app as it forms; trailer + auto-submit at finalize     | nothing more is pasted               |
| `transcribe_with_post_process`    | **preview only**: not typed; Live overlay forced if the model streams | polished text pasted once via Ctrl+V |
| `multi_stt_transcribe`            | **preview only** (primary model)                                      | merged text pasted once via Ctrl+V   |

Implementation: `actions::live_stream_is_preview_only(settings, processed)`
decides, `effective_overlay_style` forces `OverlayStyle::Live` for a
preview-only stream when the model supports streaming (otherwise the user's
overlay choice stands), `TranscriptionManager::start_stream(live_typing)` tells
the worker whether to create the writer, and `final_paste_method` returns the
`PasteMethod::CtrlV` override handed to `clipboard::paste_with_method`. Typing
the raw stream and then pasting the processed result would leave two versions
in the app — that is the case this rule exists to prevent.

The two rows below the first are the **Ctrl+V shape**: `clipboard` / `ctrl_v` is
the default paste method and the one the experimental streaming mode is built
around. The live text is typed nowhere, nothing is typed over, and the finished
text arrives in the foreground app in a single paste when the session ends — so
the **overlay is the only place the text is visible while it is being spoken**,
which is why that mode forces `OverlayStyle::Live` on every path rather than
only when `DirectStreaming` is configured (see the note under the mode below).

### Multi-STT Streaming First (experimental, fork addition)

`multi_stt_streaming_first_enabled` turns the Multi-STT primary model into the
live 1st model: the stream's own text _is_ output #1, and the extra models plus
the merge prompt rewrite it in place while the recording is still running.
Backend: `multi_stt_stream.rs`; audio: `audio_toolkit/audio/chunk_tap.rs`;
overlay: the Live overlay, which grows with the text.

What the mode is for, in one sentence: **live streaming transcription that gets
more accurate as you speak, without the GPU work growing with the session.**
The user watches the text form and then watches it being corrected, at every
pause, for as long as they keep talking — and the corrections are already in the
text by the time they stop. The reasoning behind this section, including the
close-gate defect described below, is in
[docs/MULTI_STT_STREAMING.md](docs/MULTI_STT_STREAMING.md).

- **Two things compose, and only two: the live stream and the sliding window.**
  The primary model streams as it always did — that is what is on screen, and it
  is never re-transcribed. What the break adds is a _re-decode of a bounded
  window_ by the extra models, merged with the prompt, replacing the rough text
  of the chunk that just closed. The mode is therefore not "batch Multi-STT run
  repeatedly": the audio handed to the extras is a window the settings size, and
  the rest of the session is already settled text that no model will see again.
- **The standard path — audio since the beginning — is untouched and still the
  fallback.** `MultiSttAction`'s batch pipeline (record everything, decode
  everything, merge everything at stop) was the mode's first implementation and
  works; it still runs whenever the streaming mode is off, when the primary
  model cannot stream, when no merge prompt is configured, or when the stream
  never starts (`StreamFinalization::NeverStarted`). Nothing in this section
  changes it. The difference is _when_ the work happens and what it is done on:
  the batch path decodes the whole session once, at stop; the streaming mode
  decodes a bounded window at each pause, while the user is still speaking, and
  the text is already merged by the time the recording ends.
- **The unit of work is a chunk, and a chunk is the audio between two breaks.**
  A break is `multi_stt_streaming_pause_ms` of `last_speech_ms()` standing still
  (the test Live Mode uses for its silence boundary) — **every** break closes the
  chunk that was open, with no second condition and nothing detected in the text.
  That is the whole delimiter, and it is decided on the audio alone: a run of
  speech in any language, punctuated or not, closes at every pause the speaker
  makes, so the session is a list of slices with an obvious start and end.
  `MAX_CHUNK_SECONDS` (60, a constant) is the only other trigger, and it is a
  valve rather than a policy: someone who talks for a minute without pausing
  still gets merged, and neither a chunk nor a merge window can grow without
  bound. Nothing else closes a chunk — never a length, never a character.
  The one thing a break waits for is the chunk's **own text**: a merge is a
  _replacement_, so a chunk merged before its last words exist is merged against
  an empty slot 1 and hands those words to its successor — the same speech
  twice. Two conditions must hold, and they are independent (see
  `break_outcome`). The family must have **decoded** the chunk's audio:
  `input_received_ms − audio_committed_ms` within `STREAM_DRAIN_TOLERANCE_MS`
  (500 ms, capped at the configured pause, so a 100–499 ms pause keeps the
  invariant). And it must have **published** the text for it: `Chunk::live`
  non-empty. The first is a drain hint rather than a text cursor —
  `transcribe.h` says as much and Parakeet derives it from `mel_frames_consumed`
  — so an exact zero is unreachable while a stream runs; the tolerance is safe
  because a break is silence at least that long, and a sequential decoder's un-decoded audio is a _suffix_
  of what it was fed, short enough to lie inside that silence. The second is not
  implied by the first: a family that decodes eagerly and commits late drains
  completely with its text still owed. A break waits up to `TEXT_CATCHUP_GRACE`
  (2.5 s, a constant) for both, which is cheap because a pause is silence by
  definition. A break still owed either when the grace runs out does not close a
  chunk: the session retires itself (see Fallbacks), because waiting longer
  cannot produce text the model has not written.
- **The audio sent to the extras is a window, never the session.** Feeding the
  extras everything since the recording began is what makes the mode useless on
  a long session: each break would re-decode the whole dictation, so the cost
  would grow with the session rather than with the chunk. Instead,
  `multi_stt_streaming_context_chunks` (0–3, default 1) already-closed chunks
  are sent in front of the one that just closed, so the window is at most four
  chunks and is flat in the length of the session. **That flatness is the point
  of the whole design** — it is what keeps VRAM and decode time independent of
  how long the user has been dictating, which is the one thing a session-long
  re-decode cannot do. The context is an **input only**: the merge's text
  replaces the closed chunk's text and nothing else.
- **Why there is a context at all, and why it is the user's to size.** A spoken
  sentence does not end at a pause. People hum, breathe, think, restart — a
  single sentence can run across many breaks, and each of those breaks closes a
  chunk in the middle of it. Without context, the extras would be handed the
  second half of a sentence with nothing in front of it and the merge would
  decide what it is from half the evidence, so a chunk that is a fragment would
  be corrected as if it were whole. The context chunks are exactly that missing
  evidence: the extras hear the joined audio, so a word cut at the pause, a
  pronoun whose referent is in the previous chunk, or a clause whose verb is in
  the next one all reach the merge with their surroundings. And because a merge
  is a _rewrite of a chunk that is already on screen_, that evidence lands as a
  correction the user watches happen: the preview shows the sentence complete
  itself across successive pauses, and the final transcription is the same text
  after the last one. The count is a slider (0–3) rather than a constant because
  how far back the useful context reaches is a property of how the user speaks —
  0 for clean dictation in short sentences, more for someone reasoning out loud
  in long ones — and each step back costs one more chunk of audio per decode.
- **Every slot of a merge covers the same span, and that is the invariant.** The
  extras decode `[context audio + chunk audio]`, so their text begins with the
  context's words, while slot 1 (`${output}`) is the primary's own live text for
  the chunk. Each extra's decode is therefore **cropped** back to the chunk by
  `strip_context_prefix`, which finds a run of the context's words (12 down to 3,
  longest first, tolerating `CONTEXT_SEAM_DRIFT` words of disagreement at the
  seam) in the decode and cuts after it. The ruler is the context chunks'
  `display_text()` — the text that is already on screen. A decode the crop cannot
  align is dropped for that chunk, which costs one model's opinion and never text
  that is already on screen; slot 1 is always intact.
- **Why the window cannot replace what it covers.** A merge whose output replaced
  the whole window would overlap its neighbour's window by `context` chunks, and
  folding two overlapping windows either drops the chunk that left the window or
  shows the context's words twice. Per-chunk ownership is what makes the text
  composable: the context is only ever an input, earlier chunks keep their own
  text, and nothing is dropped, duplicated or shown twice.
- **The tap** is a process-wide `ChunkTap` the recorder pushes the VAD-filtered
  16 kHz frames into, in the same place `handle_frame` hands them to the batch
  buffer and the stream feed — so a chunk's audio is the recording's own speech
  without the silence the VAD dropped. A session holds it by _token_ (`begin` /
  `end` / `is_current`), so a coordinator that outlives its recording cannot
  swallow the next one's audio. Off: one relaxed atomic load per frame.
  `audio_committed_ms` is a **drain hint**, not a cursor into the committed
  text: it reports how much audio the family has decoded, and the close gate
  reads `input_received_ms − audio_committed_ms` against
  `STREAM_DRAIN_TOLERANCE_MS` to decide whether a break may close (see above).
  It never cuts audio — a chunk's boundaries are its own `begin`/`end` timestamps,
  so a mis-timed hint cannot shift a seam.
- **Retained audio, and the memory bound.** A closed chunk keeps its audio while
  it can still be a merge's context — the next `context_chunks` closes — and
  while a retry of its own merge is pending, and is freed after that
  (`retain_context_audio`). Worst case is `context_chunks + 1` chunks plus one
  pending retry, and a chunk at the valve is ~3.8 MB. The retry state is one
  chunk id, not a copy of the audio: the audio is read back off the retained
  chunk.
- **Text model**: `display = closed chunks' current text + the open chunk's`.
  A merge replaces only its own chunk, so the rough streaming text degrades
  into the polished one in place and earlier chunks are untouched. On merge
  failure the chunk keeps the extras' outputs joined by newlines, is counted in
  `failed_chunks`, and is retried after the next break, once that break's own
  merge has landed (one merge in flight at a time; a retry still pending at
  stop is not run, and that chunk keeps its fallback text); the failure is never put
  in the text (with `DirectStreaming` that text is typed into the user's
  document) — the overlay badge and `MultiSttStreamChunkFailedEvent` carry it.
- **Output, and the two ways the mode reaches the app.** The mode is built
  around **Ctrl+V**: with `clipboard` / `ctrl_v` — the default — nothing is
  typed anywhere while the session runs, the overlay is the only live view of
  the text being spoken, and the finished text is pasted **once**, at stop, by
  the ordinary Multi-STT paste. That is why the mode forces `OverlayStyle::Live`
  whatever the user's overlay setting is: the Minimal overlay does not render the
  transcript, so on this path a minimal overlay would mean _no_ live view at all.
  With `paste_method = direct_streaming` the coordinator instead owns the
  `DirectStreamWriter` (`owns_typing`) and pushes the composed text as its
  target, so the corrections land in the user's document as they happen; the
  writer's prefix diff backspaces and retypes only the divergent suffix — the
  chunk a merge just replaced, and everything after it — gated on
  `is_caught_up()` so a revision never outruns the typewriter. `flush` at stop
  types the remainder and applies the trailing space / newline / auto-submit
  behaviour. `owns_typing` is carried on the session outcome so `MultiSttAction`
  knows the text is already delivered and pastes nothing. The history row's
  "Model 1" column is the primary's own session text either way.
- **Overlay**: the mode's events carry `whole_session` (grow the card with the
  transcript) and `failed_chunks` (the badge). The card reports its height in
  24 px steps through `overlay_stream_text_height`, which `overlay.rs` adds to
  the streaming window's height, clamped to 70 % of the monitor — past that the
  card scrolls back, so the whole session stays readable, never hidden.
  `whole_session` is set by exactly one emitter (`emit_composed_stream_text`,
  the coordinator's own path: the production view with an exclusive sink, or
  the Multi Streaming STT debug view's merged block on `MERGE_BLOCK_SLOT`), so
  it _is_ "this payload is the mode's preview": the
  overlay applies such an update as one block, stopping the typewriter first.
  A merge replaces text that is already on screen, and a character-by-character
  reveal could only retype its way back to every correction — one
  render per 1–3 characters instead of one per update. Normal dictation keeps
  `overlay_direct_mode`'s typewriter; its rewind-to-common-prefix revision path
  is its own setting, `overlay_back_correction`, off by default (see the
  settings list above). The `DirectStreamWriter` path is deliberately untouched:
  reaching a revision in someone else's document can only be done by backspacing
  the divergence and retyping it.
- **The merge bookkeeping is a queue, not a slot** (`MergeQueue`). The watchdog
  abandons a job past `MERGE_TIMEOUT` but cannot cancel it, and the session
  dispatches a new one meanwhile; with a single slot the abandoned job's later
  write destroyed whichever result lost the race, and a close merge's audio has
  already been taken off its chunk, so the loss was permanent. Each job carries
  the generation it was dispatched under: an abandoned result is still _applied_
  — its chunk has been waiting for it — but only a result matching the awaited
  generation retires the session, so a second merge can never start alongside
  the one in flight. `wait_for_merge` drains in a loop and once more before
  composing, so a result that lands late still reaches the final text.
- **The session always releases what it holds.** `run` wraps the tick loop in
  `catch_unwind` and calls `shutdown` either way: a panic in `step` used to
  unwind past `tap.end` and `set_stream_text_sink(None)`, leaving the audio tap
  claimed and the exclusive sink installed, which shows up as the _next_
  recording drawing no overlay text. `shutdown` also clears a shared
  `Arc<AtomicBool>` that `is_active` reads — the user's cancel hotkey stops the
  recorder directly and never calls `cancel()`, so the coordinator has to retire
  itself rather than rely on `start`'s cancel clearing the stale entry.
- **Fallbacks**: no streaming-capable primary model, no merge prompt, or a
  stream that never starts (`StreamFinalization::NeverStarted`) → the
  coordinator is dropped and the recording takes the normal Multi-STT batch
  path. So does a session that **retires itself mid-recording**: when a break is
  still owed the chunk's own text after `TEXT_CATCHUP_GRACE` — either because the
  stream has not drained the chunk's audio or because it has not published the
  text for it — the mode's premise is dead for that session, and a chunk closed
  then would show the same speech twice. The
  coordinator stops (the overlay falls back to the primary's own live text, via
  one last publish that drops the composed preview), the recording keeps running
  and the batch path transcribes and merges the whole session at stop, exactly
  as it does when the mode is off — with one exception: with
  `paste_method = direct_streaming`, a session that retires mid-recording (or
  hits the finish timeout) has already typed part of the text into the
  document, and the batch path then pastes the full result, so the text can
  appear twice in that configuration. Both cases are logged, with the characters
  still owed, the audio left un-drained, and the grace that produced them.
  Cancel cancels the coordinator: nothing more is typed, nothing is pasted, no
  history row.

Cost: one 50 ms thread per recording in this mode; on the audio path one mutex
lock and one memcpy per 16 ms frame (≈62/s) while it is armed, nothing while it
is off; one merge per break — three decodes of the **window** (`context_chunks +
1` chunks, at most four) plus one short LLM call. The cost per break is therefore
bounded by the settings, not by how long the session has been running: a
thirty-minute dictation costs the same per pause as the first one, where feeding
the extras the whole session would make each break more expensive than the last.
Memory is bounded the same way: `context_chunks + 1` chunks of audio plus one
retry, ≈3.8 MB per chunk at the 60 s valve, and a chunk's audio is freed as soon
as it can neither be context nor be retried. The preview costs less than what it
replaced: one render per update instead of the typewriter's one per 1–3
characters, and the card's measuring layout effect runs once per update instead
of once per tick. Each publish carries the whole session's text (the same shape
the plain streaming path already emits, bounded by `TICK` at ≤ 20/s, deduped
against the last publish), and composes it into a fresh `String` per changed tick
— O(session) memcpy at a few KB for a realistic session, which is not the cost
here. The nested Multi Streaming STT mode below adds up to three short-lived
waiter threads and up to three extra stream workers per recording.

### Multi Streaming STT (experimental, fork addition)

`multi_stt_streaming_multi_enabled`, nested under the streaming-first mode.
Backend: `multi_streaming.rs`. It is not a second coordinator: it arms
`multi_stt_stream`'s own coordinator with `TextSource::Live` instead of
`ReDecode`, so every streaming-capable Multi-STT slot (models 2–4, in list
order, via `streaming_slots`) runs its own live stream on stream slots 1–3
beside the primary's slot 0. Each pause merges the models' _live_ texts for the
chunk with one LLM call and no re-decode, and nothing is batch-decoded at stop
either. A slot that cannot stream is skipped and not even preloaded; with no
streaming-capable slot the mode refuses and the parent mode arms with batch
extras.

- **The module's own job is the extras' lifecycle.** One waiter thread per slot
  (`wait_for_engine_and_start`, up to `LOAD_WAIT` = 120 s) leases the extra
  engine once its preload finishes and opens the stream. Frames queue in
  `StreamRouter` until then, so a late stream still hears the whole recording.
- **Stop and cancel.** `finish_extras` finalizes those streams at stop,
  strictly after the primary's finalize and before `multi_stt_stream::finish`;
  `cancel` releases them.
- **Display.** `multi_stt_streaming_multi_debug_view` makes the sink
  non-exclusive and shows one overlay column per live model plus the merged
  block on `MERGE_BLOCK_SLOT`; off, the overlay shows the parent mode's single
  corrected text. The merge, the pauses, the result and the paste are
  identical either way.

### Recall (fork addition)

The **Recall** page is a note vault that is a plain folder, with no database.
Backend: `recall/` + `commands/recall.rs`. The root is
`settings.recall.output_dir`, or `<app data>/recall` by default.

- **Layout**: each note is `notes/<YYYY-MM-DD-slug>.md` with a small
  `key: value` frontmatter (title, created, updated, tags, source, audio) above
  a Markdown body. `index.json` is a disposable tag-count cache, rebuilt after
  every plain-vault mutation; `audio/` holds copies of the recordings behind
  notes.
- **Save to Recall** on a History entry (`recall_save_transcription`) files the
  text as a new note and copies its WAV into `audio/`.
- **Dictate** (`recall/dictate.rs`, `recall_dictate_start/stop/cancel`) records
  under the `recall_dictate` binding, batch-transcribes with the primary model
  (post-processing when enabled; under 200 ms of speech is refused) and returns
  the text to the editor's caret. Nothing is pasted and no History row or
  statistics run is written; the cancel hotkey ends it through
  `cancel_current_operation`.
- **Insertion mode** (`recall/insertion.rs`) is an atomic flag the page arms
  while the editor has focus. While armed, the Transcribe and Multi-STT hotkeys
  emit `RecallInsertTextEvent` instead of pasting, write no History row, and
  move their WAV into the vault's `audio/`.
- **Encryption** (`recall/crypto.rs`, optional and reversible): the passphrase
  goes through Argon2id (64 MiB, t=3, p=4; parameters, salt and a sealed
  verifier in `vault.json`) to a master key held only in process memory while
  unlocked. Every note and recording becomes `<name>.rcl` = magic ‖ per-file
  XChaCha20-Poly1305 key wrapped by the master key ‖ nonce ‖ ciphertext.
  Enabling writes a one-time plaintext backup to `backup/<timestamp>/` and
  deletes `index.json`; disabling needs the passphrase, restores byte-identical
  files and rebuilds the index. Changing the vault folder locks the session;
  while locked, a vault lists only note ids and every write is refused.

### Single Instance Architecture

The app enforces single instance behavior — launching when already running brings the settings window to front rather than creating a new process. Remote control flags (`--toggle-transcription`, etc.) work by launching a second instance that sends args to the running instance via `tauri_plugin_single_instance`, then exits.

## Internationalization (i18n)

All user-facing strings must use i18next translations. oxlint enforces this through `eslint-plugin-i18next` (no hardcoded strings in JSX).

Locale files load on demand: `src/i18n/index.ts` registers a small i18next
backend over a non-eager `import.meta.glob`, so each `translation.json` is its
own chunk imported the first time that language is used (eager bundling put
every locale — 26 today, about 2.8 MB of JSON — into the startup chunk of
both windows). `i18nReady`
resolves once the fallback bundle is loaded and the language is synced from
settings; `main.tsx` and `overlay/main.tsx` wait for it before the first
render so no raw keys are painted.

> **Linter note:** this fork uses **oxlint** (`.oxlintrc.json`), not ESLint.
> typescript-eslint hard-throws on TypeScript >= 7, which this fork pins, so
> ESLint could not parse the codebase at all. oxlint ships its own parser and is
> immune to that. `eslint-plugin-i18next` is still a dependency — oxlint loads
> it through `jsPlugins` and enforces `i18next/no-literal-string` exactly as
> before, plus a set of built-in correctness rules ESLint was never configured
> for.
>
> Two caveats when adding suppressions:
>
> - `jsPlugins` is an alpha oxlint API. If it ever stops loading the plugin,
>   oxlint fails the config outright (exit 1) rather than silently skipping the
>   rule, so a regression surfaces in CI instead of leaking hardcoded strings.
> - `src/` carries **no `i18next/no-literal-string` suppressions at all**, and
>   it should stay that way. (The only suppressions are file-level `jsx-a11y`
>   ones on custom widgets — `ui/Dialog.tsx`, `ui/Select.tsx`,
>   `ui/Toaster.tsx`, `Sidebar.tsx`, `HotkeySidebar.tsx`,
>   `onboarding/ModelCard.tsx` and the status-bar popovers — and one
>   `oxc/approx-constant` on `liveFftPresets.ts`.)
>   `// eslint-disable-next-line i18next/no-literal-string` is unreliable here
>   anyway — oxlint honours it for JSX text on a single line but not when the
>   flagged element spans multiple lines. For literal data (symbols, indices,
>   units, version strings) use a JSX expression container instead: `{"—"}` or
>   ``{`#${chunk.index}`}``. It needs no suppression, works in every linter, and
>   reads as "data, not prose". See `overlay/RecordingOverlay.tsx` and
>   `settings/live-mode/LiveModeSettings.tsx`.

**Adding new text:**

1. Add key to `src/i18n/locales/en/translation.json`
2. Use in component: `const { t } = useTranslation(); t('key.path')`

**File structure:**

```
src/i18n/
├── index.ts           # i18n setup
├── languages.ts       # Language metadata
└── locales/
    ├── en/translation.json  # English (source)
    ├── de/, es/, fr/, ja/, ru/, zh/, ...
    └── ...
```

For translation contribution guidelines, see [CONTRIBUTING_TRANSLATIONS.md](CONTRIBUTING_TRANSLATIONS.md).

## Code Style

**Rust:**

- Run `cargo fmt` and `cargo clippy` before committing
- Handle errors explicitly (avoid unwrap in production)
- Use descriptive names, add doc comments for public APIs

**TypeScript/Solid:**

- Strict TypeScript, avoid `any` types
- Solid 2 conventions: never destructure
  props in a component body (it runs once — a destructured prop is a
  mount-time snapshot); read reactive values inside JSX bindings, not in
  component bodies; use the `createEffect(compute, apply)` returned-cleanup
  form; `Dynamic` for reactively-switched components
- Tailwind CSS for styling. `App.css` imports it with `source(".")` so only
  `src/` is scanned: Tailwind's automatic source detection walks the whole
  repository (gitignored `src-tauri/target` included) to build its watch
  globs, which cost seconds per build
- Path aliases: `@/` → `./src/`

## CLI Parameters

ZER0 supports command-line parameters on all platforms for integration with scripts, window managers, and autostart configurations.

**Implementation:** `cli.rs` (definitions), `main.rs` (parsing), `lib.rs` (applying), `signal_handle.rs` (shared logic)

| Flag                                              | Description                                                                                                                                                                                |
| ------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `--toggle-transcription`                          | Toggle recording on/off on a running instance                                                                                                                                              |
| `--toggle-post-process`                           | Toggle recording with post-processing on/off                                                                                                                                               |
| `--cancel`                                        | Cancel the current operation on a running instance                                                                                                                                         |
| `--start-hidden`                                  | Launch without showing the main window (tray icon visible)                                                                                                                                 |
| `--no-tray`                                       | Launch without system tray (closing window quits the app)                                                                                                                                  |
| `--debug`                                         | Accepted for compatibility: logging is always captured at Trace (see [docs/LOGGING.md](docs/LOGGING.md)); it only shows in the startup log line and does not turn on the in-app debug mode |
| `-f`, `--transcribe-file <WAV>`                   | Headless: transcribe a mono WAV (16/24-bit PCM or 32-bit float, any rate) with the batch path and exit                                                                                     |
| `--model <ID>`                                    | Headless: model to use instead of the selected one                                                                                                                                         |
| `--device-index <N>`                              | Headless: GPU device index for GGUF models                                                                                                                                                 |
| `--list-devices`, `--list-models`                 | Headless: print GPU devices / installed models and exit                                                                                                                                    |
| `--stream-chunk-ms <N>`, `--stream-att-right <N>` | Headless: replay the WAV through native streaming in N-ms feeds (1–10000), with an optional Nemotron right context                                                                         |
| `--repeat <N>`, `--json`                          | Headless: benchmark runs (must be 3: warm-up discarded, runs 2–3 averaged) / machine-readable output                                                                                       |

**Key design decisions:**

- CLI flags are runtime-only overrides — they do NOT modify persisted settings
- Remote control flags work via `tauri_plugin_single_instance`: second instance sends args, then exits
- `send_transcription_input()` in `signal_handle.rs` is shared between signal handlers and CLI

## Debug Mode

Access debug features: `Cmd+Shift+D` (macOS) or `Ctrl+Shift+D` (Windows/Linux)

## Platform Notes

- **macOS**: Metal acceleration, accessibility permissions required for keyboard shortcuts
- **Windows**: CUDA acceleration on x86_64 (transcribe.cpp `cuda` feature; upstream uses Vulkan), CPU only on aarch64, no Authenticode code signing (`signCommand` removed from `tauri.conf.json`) but **signed updater artifacts** (`createUpdaterArtifacts` on, the project's own minisign key: private at `~/.tauri/zer0.key`, public in `plugins.updater.pubkey`; CI injects it via the `TAURI_SIGNING_PRIVATE_KEY` secrets, the tauri runner reads the local file), normal OS CPU scheduling (no affinity, core ranking, process/thread priority elevation, MMCSS registration or power-throttling override). The NSIS installer (`src-tauri/nsis/installer.nsi`, upstream's template) creates a desktop shortcut only on request: the finish-page box starts unchecked and silent / passive installs need `/DESKTOP`. Implicit Vulkan layers (overlays, capture hooks) are disabled for the ZER0 process via `VK_LOADER_LAYERS_DISABLE=~implicit~` set in `main.rs`, as upstream does (upstream issue #2049); the CUDA build never loads the Vulkan loader, so this only keeps the process environment identical to upstream. Opt out with `ZER0_KEEP_VULKAN_IMPLICIT_LAYERS=1` or by setting `VK_LOADER_LAYERS_DISABLE` yourself
- **Linux**: CUDA acceleration (upstream: OpenBLAS + Vulkan), limited Wayland support, overlay uses GTK layer shell (disable with `ZER0_NO_GTK_LAYER_SHELL=1`)
- **Nix/NixOS**: the Nix package sets `ZER0_DISABLE_UPDATER=1` to force-disable the self-updater at runtime without touching the persisted setting (self-update can't work against an immutable `/nix/store`)

Every one of those flags is `ENV_PREFIX` + the suffix (`src-tauri/src/app_identity.rs`),
and each falls back to the `HANDY_`-prefixed spelling with a warning, so a shell
script or a desktop file written before the rename keeps working. Read the prefix
from `app_identity` rather than copying it into new code.

## Troubleshooting

See the [Troubleshooting](README.md#troubleshooting) section in README.md.

## GitHub workflow for AI coding assistants

**MANDATORY. Before opening any PR, issue, or discussion in this repo: you MUST read the relevant template file and follow it strictly.** That includes sections that look "ceremonial" — checklists, AI Assistance disclosures, "Human Written Description". A generic Summary/Test-plan layout is not acceptable.

- **Opening a PR:** Read [`.github/PULL_REQUEST_TEMPLATE.md`](.github/PULL_REQUEST_TEMPLATE.md). Every section listed there is mandatory. If a section requires a human-written paragraph (e.g. "Human Written Description"), leave a clear TODO placeholder and ask the human contributor to fill it in — do not invent their voice.
- **Opening an issue:** Read [`.github/ISSUE_TEMPLATE/`](.github/ISSUE_TEMPLATE/). Blank issues are disabled; pick the right template (`bug_report.md` for bugs). Feature requests do not belong in issues — they go to [Discussions](https://github.com/NairoDorian/S2B2S/discussions) (see `.github/ISSUE_TEMPLATE/config.yml`).
- **Proposing a feature:** new features require community support gathered in [Discussions](https://github.com/NairoDorian/S2B2S/discussions) before any PR is opened — see the PR template's "Community Feedback" section.
- **Translations:** Follow [CONTRIBUTING_TRANSLATIONS.md](CONTRIBUTING_TRANSLATIONS.md).
- **Full contributor workflow:** [CONTRIBUTING.md](CONTRIBUTING.md).

**Commits:** Use conventional commit prefixes (`feat:`, `fix:`, `docs:`, `refactor:`, `chore:`). Focus the message on _why_, not _what_. End the message with
`Co-Authored-By: Claude Code <noreply@anthropic.com>` when an assistant wrote it, and
state the cost of any new thread, poll or dependency (see `docs/PERFORMANCE.md`
rule 10).
