# Known Issues and Open Decisions

Open items, each verified against the working tree on **2026-10-04**. Remove
entries as they are closed; add new ones with a date. The 2026-08-26 audit
whose findings fed the first version of this file has been deleted — its open
items live here, self-contained.

## Needs a maintainer decision

- **Fresh installs ship without transcription hotkeys.** `transcribe` and
  `multi_stt_transcribe` default to an empty `current_binding`, and schema
  migrations 3/4 cleared existing users' bindings once, so the performance-mode
  simulated keys (`ctrl+space` / `ctrl+alt+space`) could never retrigger the
  app. Since the conflict rule now only applies while performance mode is
  enabled (`shortcut::conflicts_with_performance_mode` returns false when
  `multi_stt_performance_mode_enabled` is off), the defaults could be restored
  (`ctrl+space` etc.) without reintroducing the loop — but that is a product
  decision. Until then the README tells users to set the hotkeys in
  Settings → General.
- **`multi_stt_keep_extra_models_loaded` semantics.** Only matters when
  `model_unload_timeout` is `Immediately`; the idle watcher unloads extras
  regardless. Either make the flag independent of the timeout or reword the UI
  text (currently documented as-is in AGENTS.md).

## Closed since the last pass

- **Settings schema migrations 3/4/5 re-ran on every `get_settings`.** Closed:
  `apply_settings_migrations` now ends with a trailing
  `.max(CURRENT_SETTINGS_SCHEMA_VERSION)` stamp plus an explicit
  `updated = true` whenever the stored version was behind, so a store that
  needed no other fix still persists the bump. The reasoning is the doc comment
  on `CURRENT_SETTINGS_SCHEMA_VERSION` in `src-tauri/src/settings.rs`: a version
  bump is a change in itself, and without persisting it every migration replays
  on each read — and `get_settings` is on hot paths.

## Release readiness

- **CI has no CUDA toolkit, and the comment that excused it is now wrong.**
  Since the dual-GPU change, `src-tauri/Cargo.toml` asks `transcribe-cpp` for
  `dynamic-backends`, `cuda` **and** `vulkan` on Windows x86_64 and Linux — so
  `.github/workflows/build.yml`'s Vulkan SDK installs (Windows x64, Ubuntu
  22.04/24.04, Ubuntu ARM64) and its vcpkg `spirv-headers` step are all needed
  now, and the AppImage audit requires `libggml-vulkan.so` plus a "loaded Vulkan
  backend" line. What is still missing is the other half: no workflow installs a
  CUDA toolkit or requests a GPU runner, so whether the `cuda` feature's
  configure step survives on a CUDA-less runner is the open question. Relatedly,
  `test.yml`'s apt comment still reads "This fork's Linux target enables
  transcribe-cpp's `cuda` feature instead (src-tauri/Cargo.toml), so nothing
  here builds the Vulkan backend" — true when written, false now. (A separate set
  of workflow problems is fixed: frozen-lockfile installs, the clippy step,
  `test:unit` in code-quality, Playwright `--with-deps`.)
- **Push-triggered CI never runs on the branch the work happens on.**
  `code-quality.yml`, `main-build.yml`, `nix-check.yml`, `playwright.yml` and
  `test.yml` trigger on `push` to `main` only, and the fork develops and
  releases from `Handy_Multi_STT`. Two of them also run on `pull_request` —
  `playwright.yml` (all paths) and `test.yml` (paths `src-tauri/**` only) — which
  covers the Rust tests and the browser suite while a PR is open and nothing
  else: `build.yml` (`workflow_call` plus dispatch), `build-test.yml`,
  `pr-test-build.yml` and `release.yml` never fire on a push at all. Either add
  the branch to the `push` triggers or work through PRs and dispatch.
- **Windows binaries are not code-signed.** `signCommand` (Azure Trusted
  Signing, Authenticode) was removed from `tauri.conf.json`, and the CI steps
  and `AZURE_*` secrets that fed it are gone with it. The updater signing
  secrets (`TAURI_SIGNING_PRIVATE_KEY`) are the opposite: they must be set as
  repository secrets, or release builds cannot sign the updater artifacts.

## Security / hygiene

- **`.cclaude/settings.local.json` contains an API token** and model-routing
  overrides for a coding assistant. It is untracked and gitignored, but it
  sits in the project tree — rotate the key and keep the file outside the
  repository.

## Bugs left open

- **Linux and macOS are not built or run here.** The November cycle's
  platform items that compile on Windows landed (Phase 10 of the update plan);
  the Linux-only modules (WebKitGTK renderer policy, the X11 overlay, the
  dotool / ydotool / paste-tool chain, PRIMARY-selection paste, the updater
  switch for packages that cannot update, macOS permission reset) are deferred
  until a Linux or macOS build can type-check them. CI builds those platforms
  only on pushes to `main` and on pull requests.
- **Decisions that leave upstream features out.** D2: no engine worker
  process, so a native crash (a CUDA abort) ends the app; D3: no
  `handy-recorder`, so no AirPods mode. Both are recorded in the plan's
  Phase 12.

- **A CUDA fault still ends the app.** The backend-error recovery of
  2026-11-18 (upstream `2eab8fc`, adapted in-process) catches what the engine
  returns as `TRANSCRIBE_ERR_BACKEND`: every C++ exception at its API (Vulkan's
  device-lost included) and failed graph computes. ggml's CUDA backend instead
  reports an error through `GGML_ABORT`, which ends the process before any
  error comes back; upstream survives that only because its engine runs in a
  worker process (decision D2 declined that design for Multi-STT). Fixing it
  in-process would need the engine to return CUDA errors rather than abort.
- **Old ONNX model directories are never cleaned up.** The transcribe.cpp-only
  change (2026-09-10) stopped listing the 11 legacy ONNX models but
  deliberately leaves their extracted directories (`parakeet-tdt-0.6b-v2-int8/`
  etc., up to ~1 GB each) under the models folder. The "remove unused model
  files" action that was planned for that change was never built; users delete
  them by hand.
- **Earshot was never benchmarked in noisy rooms** before becoming the only
  VAD (quiet-room agreement with Silero was 97.7 %). If speech gets clipped in
  noise, `vad_threshold_earshot` is the knob (the engine scores every frame
  since 2026-11-17, so a quiet microphone answers to a lower threshold too; the
  old −45 dBFS pre-gate is gone); adding a second detector back is a larger
  change.
- `audio_toolkit/bin/cli.rs` is not a build target (there is no `[[bin]]` entry
  in `Cargo.toml`, and it sits outside `src/bin/`, so Cargo does not discover
  it) and therefore not compiled by CI. It compiles as of
  2026-08-26; either register the bin or accept that it can rot.

## Tooling

- **`scripts/update-deps.ts` resolves against the current working directory.**
  Run it from the repository root (paths resolve against the CWD). `scripts/`
  is outside `tsconfig.include`, so scripts are never type-checked.
- **`.nix/bun.nix`** is only regenerated where bun2nix exists (not on
  Windows); it goes stale whenever `package.json` changes until `bun install`
  runs on a Nix machine.
- `scripts/ci/stage-transcribe-libs.sh` is referenced by nothing (re-checked
  2026-10-04). Either wire it into the Linux packaging or delete it.
- Dependencies are tracked at prereleases (`bun run update-deps --prerelease`,
  the fork's explicit policy: Solid 2 RC, TypeScript 7 dev, Vite 8, Prettier 4
  alpha, Playwright alpha, Bun 1.4); expect occasional breakage from upstream
  tooling.
- clippy reports a stable set of upstream style warnings (see the advisory
  clippy step in `test.yml`); fork-added code is clean. Clearing the upstream
  ones would create merge conflicts for little gain.
