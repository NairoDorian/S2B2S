# Known Issues and Open Decisions

Open items carried over from the [2026-08-26 review](CODE_REVIEW_2026-08-26.md).
Numbers refer to that document's findings table. Remove entries as they are
closed; add new ones with a date.

## Needs a maintainer decision

- **(#7) Fresh installs ship without transcription hotkeys.** `transcribe` and
  `multi_stt_transcribe` default to an empty `current_binding`, and schema
  migrations 3/4 cleared existing users' bindings once, so the performance-mode
  simulated keys (`ctrl+space` / `ctrl+alt+space`) could never retrigger Handy.
  Since the conflict rule now only applies while performance mode is enabled,
  the defaults could be restored (`ctrl+space` etc.) without reintroducing the
  loop — but that is a product decision. Until then the README tells users to
  set the hotkeys in Settings → General.
- **(#47) CI workflows still target Vulkan.** `build.yml` and `test.yml` are
  upstream's: they install the Vulkan SDK, never install a CUDA toolkit, and
  the AppImage audit requires `libggml-vulkan.so`, while `Cargo.toml` now asks
  for the `cuda` feature on Windows x86_64 and Linux. A release build on this
  branch will fail in CI. Options: add CUDA runners/steps and relax the audit,
  or keep `vulkan` for CI only. Also: the workflows trigger on `main` only (the
  fork works on `Handy_Multi_STT`), and the Azure signing secrets are unused
  now that `signCommand` is gone.
- **(#26) `HIGH_PRIORITY_CLASS` for the whole process.** Raises every thread,
  including the tokio pool and the 4-way Multi-STT inference threads, which
  can starve foreground apps during a merge; the capture thread already has
  MMCSS. `ABOVE_NORMAL_PRIORITY_CLASS` (or per-thread priority) would be the
  conservative choice.
- **(#18) `multi_stt_keep_extra_models_loaded` semantics.** Only matters when
  `model_unload_timeout` is `Immediately`; the idle watcher unloads extras
  regardless. Either make the flag independent of the timeout or reword the UI
  text (currently documented as-is in AGENTS.md).

## Security / hygiene

- **(#48) `.cclaude/settings.local.json` contains an API token** and
  model-routing overrides for a coding assistant. It is untracked (now
  explicitly ignored), but it sits in the project tree — rotate the key and
  keep the file outside the repository.
- `nsis - Shortcut.lnk` at the repository root and `.commandcode/taste/…`
  (a path long enough to trip `git status` with "Filename too long") are
  local clutter; both are ignored, delete when convenient.

## Bugs left open

- **(#17) Settings schema migrations 3/4/5 don't persist the version bump**
  unless a binding was actually cleared, so they re-run on every
  `get_settings`. A unit test asserts this behaviour, so confirm intent before
  changing.

Closed on 2026-08-26 (same day, second pass): #9 concurrent extra-model loads
(coalesced in `load_extra_model`), #10 direct-streaming double paste (direct
streaming is now plain-transcription only; post-processing / Multi-STT stream
to the Live overlay as a preview and paste the result via Ctrl+V), #35
`KeyComboInput` validation, #36 stuck download pill, #37 overlay listener
cleanup.

## Translations

- All 23 non-English locales are missing the same **87** fork-added keys
  (they fall back to English). `bun run check:translations` fails until they
  are translated; the list is identical for every locale — run the script to
  get it. Structural problems (misplaced shortcut key, stale `vadBackend`
  keys, moved `downloadSpeed`) were fixed on 2026-08-26.

## Tooling

- **(#49) `scripts/update-deps.ts`** resolves `package.json` / `Cargo.toml`
  against the current working directory (run it from the repo root), keys
  `Cargo.lock` entries by name only (multi-version crates collapse), and
  rewrites exact Cargo specs to caret ranges. `scripts/` is outside
  `tsconfig.include`, so scripts are never type-checked.
- **(#50) `.nix/bun.nix`** is only regenerated where bun2nix exists (not on
  Windows); it is stale after the `zod` removal until `bun install` runs on a
  Nix machine. `scripts/ci/stage-transcribe-libs.sh` is referenced by nothing.
- **Old ONNX model directories are never cleaned up (2026-09-10).** The
  transcribe.cpp-only change stopped listing the 11 legacy ONNX models but
  deliberately leaves their extracted directories (`parakeet-tdt-0.6b-v2-int8/`
  etc., up to ~1 GB each) under the models folder. A "remove unused model
  files" action in Settings → Models was planned
  (`docs/PLAN_TRANSCRIBE_CPP_ONLY.md` §4 Phase 2) and not built; users delete
  them by hand.
- **Earshot was never benchmarked in noisy rooms** before becoming the only
  VAD (the plan's Phase 0). Quiet-room agreement with Silero was 97.7 %. If
  speech gets clipped in noise, `vad_threshold_earshot` is the knob; adding a
  second detector back is a larger change.
- `audio_toolkit/bin/cli.rs` is not a build target (`[[bin]]` commented out,
  as upstream) and therefore not compiled by CI. It compiles again as of
  2026-08-26; either register the bin or accept that it can rot.
- Dependencies are pinned to prereleases (React 19.3 canary, TypeScript 7.1
  dev, Vite 8, Prettier 4 alpha, Playwright alpha, Bun 1.4). This is the
  fork's explicit policy (`bun run update-deps --prerelease`); expect
  occasional breakage from upstream tooling.
- clippy reports 69 warnings, all upstream style lints (`collapsible_if`
  from Rust 1.97's let-chains suggestion, `redundant reference in format!`,
  `items_after_test_module`). Fork-added code is clean; clearing the
  upstream ones would create merge conflicts for little gain.
