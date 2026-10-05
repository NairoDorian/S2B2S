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

- **Old ONNX model directories are never cleaned up.** The transcribe.cpp-only
  change (2026-09-10) stopped listing the 11 legacy ONNX models but
  deliberately leaves their extracted directories (`parakeet-tdt-0.6b-v2-int8/`
  etc., up to ~1 GB each) under the models folder. The "remove unused model
  files" action that was planned for that change was never built; users delete
  them by hand.
- **Earshot was never benchmarked in noisy rooms** before becoming the only
  VAD (quiet-room agreement with Silero was 97.7 %). If speech gets clipped in
  noise, `vad_threshold_earshot` is the knob (speech below Earshot's −45 dBFS
  energy pre-gate needs microphone gain instead); adding a second detector back is
  a larger change.
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
- **The Tauri core cannot be updated past `3.0.0-alpha.2` (2026-10-05).**
  `bun run update-deps --prerelease` proposes `tauri 3.0.0-alpha.4`, and taking
  it produces a lockfile that resolves and cannot compile. **Two** independent
  breaks stand in the way, both verified against the published
  `tauri-3.0.0-alpha.4` (whose commit _is_ the `tauri-apps/tauri` `v3` branch
  head):
  1. Thirteen `tauri-plugin-*` crates are pinned to `plugins-workspace`
     `branch = "v3"`, and that branch's tip (`d9be6d0`) is also the
     `v3.0.0-alpha.2` release of every plugin — the plugins are not behind the
     core, the core has moved past them. The tip calls
     `handle.run_on_main_thread(..)` without `use tauri::Manager`, which alpha.3
     turned into a trait method, so `tauri-plugin-dialog` and the rest fail
     E0599.
  2. `b9a77ebb` _`refactor(core)!: remove the macos-private-api feature flag`_
     landed on `v3` on 2026-09-30. `macos-private-api` is **no longer a
     feature**, and `src-tauri/Cargo.toml` passes it to both `tauri` and
     `tauri-runtime-wry`. `c9a3cb89` is the other half — macOS private APIs are
     now enabled unconditionally — so this side is a code change, not just an
     upstream fork.

  A _partial_ bump is worse than none: `cargo update` unifies the **transitive**
  family (`tauri-utils`, `tauri-macros`, `tauri-plugin`, `tauri-codegen`) forward
  on its own, and `tauri-build` then rejects `macOSPrivateApi` in
  `tauri.conf.json` as an unknown field — a diagnostic that names neither the
  crate nor the cause. Contained by pinning the whole family with `=` in
  `src-tauri/Cargo.toml` (including four members nothing calls directly,
  declared so the `=` has somewhere to live) and holding it in
  `update-deps.ts`'s `CARGO_VERSION_LOCKED`. **Reaching alpha.4 needs both** a
  plugins-workspace fork (the missing `use tauri::Manager;` imports) and a local
  edit dropping the dead feature flags. See
  [FORKS.md](FORKS.md#constraints-that-bite-when-bumping).

- clippy reports a stable set of upstream style warnings (see the advisory
  clippy step in `test.yml`); fork-added code is clean. Clearing the upstream
  ones would create merge conflicts for little gain.
