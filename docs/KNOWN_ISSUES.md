# Known Issues and Open Decisions

Open items, each verified against the working tree on **2026-09-14**. Remove
entries as they are closed; add new ones with a date. The 2026-08-26 audit
whose findings fed the first version of this file has been deleted — its open
items live here, self-contained.

## Needs a maintainer decision

- **Fresh installs ship without transcription hotkeys.** `transcribe` and
  `multi_stt_transcribe` default to an empty `current_binding`, and schema
  migrations 3/4 cleared existing users' bindings once, so the performance-mode
  simulated keys (`ctrl+space` / `ctrl+alt+space`) could never retrigger the
  app. Since the conflict rule now only applies while performance mode is
  enabled, the defaults could be restored (`ctrl+space` etc.) without
  reintroducing the loop — but that is a product decision. Until then the
  README tells users to set the hotkeys in Settings → General.
- **`HIGH_PRIORITY_CLASS` for the whole process (Windows).** Raises every
  thread, including the tokio pool and the 4-way Multi-STT inference threads,
  which can starve foreground apps during a merge; the capture thread already
  has MMCSS. `ABOVE_NORMAL_PRIORITY_CLASS` (or per-thread priority) would be
  the conservative choice.
- **`multi_stt_keep_extra_models_loaded` semantics.** Only matters when
  `model_unload_timeout` is `Immediately`; the idle watcher unloads extras
  regardless. Either make the flag independent of the timeout or reword the UI
  text (currently documented as-is in AGENTS.md).
- **Settings schema migrations 3/4/5 don't persist the version bump** unless a
  binding was actually cleared, so they re-run on every `get_settings`. A unit
  test asserts this behaviour, so confirm intent before changing.

## Release readiness

- **CI release builds still target Vulkan, not CUDA.** `.github/workflows/
build.yml` installs the Vulkan SDK on every platform, installs no CUDA
  toolkit, and `test.yml` apt-installs Vulkan dev packages for the Linux build,
  while `src-tauri/Cargo.toml` asks for the `cuda` feature on Windows x86_64
  and Linux. A release build on this branch will fail in CI until either CUDA
  runners/steps are added or CI keeps `vulkan` for itself. (The workflow files
  already carry uncommitted fixes for a separate set of problems — frozen
  lockfile installs, the clippy step, `test:unit` in code-quality, Playwright
  `--with-deps` — but the Vulkan/CUDA split is untouched.)
- **All workflows trigger on `main` only.** The fork develops and releases
  from `Handy_Multi_STT`, so push-triggered CI does not run on the branch
  where the work actually happens. Either add the branch to the triggers or
  work through PRs.
- **Azure signing secrets are unused.** `signCommand` (Azure Trusted Signing,
  Authenticode) was removed from `tauri.conf.json` — Windows binaries are not
  code-signed. The updater signing secrets (`TAURI_SIGNING_PRIVATE_KEY`) are
  the opposite: they must be set as repository secrets, or release builds
  cannot sign the updater artifacts.

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
  noise, `vad_threshold_earshot` is the knob; adding a second detector back is
  a larger change.
- `audio_toolkit/bin/cli.rs` is not a build target (`[[bin]]` commented out,
  as upstream) and therefore not compiled by CI. It compiles as of
  2026-08-26; either register the bin or accept that it can rot.

## Tooling

- **`scripts/update-deps.ts` resolves against the current working directory.**
  Run it from the repository root (paths resolve against the CWD). `scripts/`
  is outside `tsconfig.include`, so scripts are never type-checked.
- **`.nix/bun.nix`** is only regenerated where bun2nix exists (not on
  Windows); it goes stale whenever `package.json` changes until `bun install`
  runs on a Nix machine.
- `scripts/ci/stage-transcribe-libs.sh` is referenced by nothing (checked
  2026-09-14). Either wire it into the Linux packaging or delete it.
- Dependencies are tracked at prereleases (`bun run update-deps --prerelease`,
  the fork's explicit policy: Solid 2 RC, TypeScript 7 dev, Vite 8, Prettier 4
  alpha, Playwright alpha, Bun 1.4); expect occasional breakage from upstream
  tooling.
- clippy reports a stable set of upstream style warnings (see the advisory
  clippy step in `test.yml`); fork-added code is clean. Clearing the upstream
  ones would create merge conflicts for little gain.
