# Plan: transcribe.cpp-only ZER0 — drop ONNX Runtime and Silero, keep Earshot

_Drafted 2026-08-26 for the `Handy_Multi_STT` fork. Status: **implemented
2026-09-10** in a single change set (see `CHANGELOG.md` → Changed / Removed
and the AGENTS.md "Voice Activity Detection" section). Deviations from the
plan below: Phase 0 (noisy-room bench, golden 16 ms SpeechClock test, settings
fixture) was skipped — the existing SpeechClock unit tests were re-based on
`VAD_FRAME_MS` and the 0.9.0 settings fixture test still loads; Phases 1–5
landed together; the migration remaps every retired ONNX id to the GGUF
conversion of the same model (the catalog turned out to have all eleven, so
the §4 "nearest family" table was not needed) and does it silently with a log
line, without the toast or the "Remove unused ONNX models" action; the
two-backend bench was deleted rather than kept as a single-backend bench.
Everything else below is kept as the design record._
Every file named below was verified to exist / contain the reference at the time
of writing. Numbers come from `src-tauri/tests/vad_backend_bench.rs` run on the
maintainer's own recordings (18 s, quiet room, one mic).\_

## 1. Goal

Ship a Handy whose only inference runtime is **transcribe.cpp** (GGUF/ggml, CUDA
on Windows x64 / Linux, Metal on macOS, CPU elsewhere) and whose only voice
activity detector is **Earshot** (pure Rust). Remove ONNX Runtime, the `ort`
crate, `transcribe-rs`, `ndarray`, the Silero model file and every setting, UI
element, build step and CI step that exists only for them.

Non-goals: changing the recording pipeline, the smoothing (`SmoothedVad`),
the speech clock, the skip-silent-recordings guard, or anything about
Multi-STT / post-processing. Those keep working on top of the new VAD unchanged.

## 2. What depends on ONNX today (inventory)

| Consumer                               | Where                                                                                                                                                                                                                                                                                                                                                                         | Notes                                                                                                          |
| -------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| **Silero VAD v6.2**                    | `audio_toolkit/vad/silero.rs` (278 lines), `constants.rs`, `resources/models/silero_vad_v6.2.onnx` (2.2 MB), `tests/vad_speech_clock_probe.rs`                                                                                                                                                                                                                                | Only direct `ort` user; only `ndarray` user                                                                    |
| **transcribe-rs** (ONNX speech models) | `managers/transcription.rs`: `use transcribe_rs::{…}` (l.28), `transcribe_rs::onnx::{…}` in `create_engine` (l.2772), `transcribe_rs::accel` in `apply_accelerator_settings` (l.3203/3306); `LoadedEngine` variants `Parakeet, Moonshine, MoonshineStreaming, SenseVoice, GigaAM, Canary, Cohere` (l.223–234); engine-specific branches in `transcribe_with_engine` (l.2861+) | 14 `EngineType::…` match sites in `transcription.rs`, 11 in `model.rs`                                         |
| **Legacy hard-coded ONNX models**      | `managers/model.rs` l.754–1160: `parakeet-tdt-0.6b-v2`, `parakeet-tdt-0.6b-v3`, `moonshine-base`, `moonshine-{tiny,small,medium}-streaming-en`, `sense-voice-int8`, `gigaam-v3-e2e-ctc`, `canary-180m-flash`, `canary-1b-v2`, `cohere-int8` — downloaded as tarballs from `blob.handy.computer` (`is_directory: true`), `gigaam_vocab.txt` staged by `build.rs`               | **The catalog (`catalog.json`, 69 models) is already 100 % GGUF** — zero `.onnx` files. ONNX is a legacy tail. |
| **`ort_accelerator` setting**          | `settings.rs` `OrtAcceleratorSetting { Auto, Cpu, Cuda, DirectMl, Rocm }` (l.310), field l.503, `shortcut/mod.rs` `change_ort_accelerator_setting`, `apply_accelerator_settings`; `AccelerationSelector.tsx`, `settingsStore.ts`, `bindings.ts`, i18n `settings.advanced.acceleration.*`                                                                                      | Separate from `transcribe_accelerator` (transcribe.cpp), which stays                                           |
| **Build / bundle**                     | `build.rs` `stage_onnxruntime_dll()` (l.105–140, `ORT_LIB_LOCATION` / `ORT_PREFER_DYNAMIC_LINK`); `.github/workflows/build.yml` l.360–390 downloads ORT 1.24.2 for macOS x64/arm64; `flake.nix` `onnxruntime` + `ORT_LIB_LOCATION` (l.41, 63); `BUILD.md`                                                                                                                     | Upstream also ships ORT; the fork's CI is upstream's                                                           |
| **Docs**                               | `AGENTS.md` "Voice Activity Detection" (Silero contract, ~80 lines), `README.md`, `CHANGELOG.md`, `CRUSH.md`, `BUILD.md`, `CONTRIBUTING.md`                                                                                                                                                                                                                                   |                                                                                                                |

What does **not** depend on ONNX and stays: `transcribe-cpp` / `transcribe-cpp-sys`
(the `NairoDorian/transcribe.cpp` fork), `hf-hub` (GGUF downloads from the
catalog go through it — keep), `SmoothedVad`, `SpeechClock`, `earshot`.

## 3. Decisions to settle before starting

1. **Model coverage.** Users lose Moonshine (tiny streaming English), SenseVoice
   (CJK, emotion/ITN), GigaAM (Russian), Canary (translate), Cohere. Whisper-family
   and Parakeet are covered by GGUF (`parakeet-unified-en-0.6b-gguf` is in the
   catalog). Options: (a) accept the loss and migrate users to the nearest GGUF
   model — **recommended**, it is what upstream's catalog direction already
   implies; (b) add those architectures to the transcribe.cpp fork first (weeks
   of C++ work per model family; out of scope for this plan).
2. **Earshot threshold and hysteresis values.** Earshot returns a 0–1 score per
   16 ms frame. Since 2026-08-26 the adapter applies the shared `Hysteresis`
   gate (enter 0.5 / exit 0.35, Silero's `max(t − 0.15, 0.01)` rule); the
   Phase 0 bench should confirm those values hold in noisy rooms before Silero
   is removed.
3. **Migration policy for on-disk ONNX models.** Never auto-delete. Keep the
   directories, stop listing them, and add a one-time "Remove unused ONNX
   models (N MB)" action in Settings → Models.
4. **Upstream relationship.** After this change `transcription.rs`, `model.rs`,
   `build.rs` and `build.yml` will conflict on every upstream merge. Accept that
   (the fork already diverges on CUDA vs Vulkan) or keep a thin compatibility
   layer — this plan assumes **accept**.

## 4. Phases

Each phase is a separate PR/commit series on a `drop-onnx` branch cut from a
tagged checkpoint (`git tag pre-drop-onnx`). Every phase ends with the full gate:
`cargo check/clippy/test`, `bun run typecheck/lint/format:check`,
`bun run check:translations`, plus the manual checks listed.

### Phase 0 — Measure and guard (no product change)

- Extend `tests/vad_backend_bench.rs` to accept a list of directories and to
  print per-file agreement; record recordings in **noisy conditions** (laptop
  fan, café, open window, headset vs. desk mic, whisper-quiet speech) and keep
  them out of git (`HANDY_BENCH_WAV_DIR`). Target: Earshot within ±3 % voiced
  fraction of Silero and ≥ 95 % frame agreement on every file; investigate any
  file below that before proceeding.
- Add a golden test for `SmoothedVad` + `SpeechClock` driven by a synthetic
  voiced/unvoiced pattern at 16 ms frames (they are currently only tested at
  32 ms via `FRAME_MS`), so Phase 1 cannot silently change kept-audio or
  speech-time accounting.
- Snapshot the current `AppSettings` JSON schema (`get_default_settings()`
  serialised) into a test fixture; Phases 1 and 3 must load it without error.

Exit: bench numbers committed to this document under §7.

### Phase 1 — Earshot only (drop Silero)

Backend:

- `audio_toolkit/vad/earshot.rs`: add hysteresis. Move `Hysteresis` from
  `silero.rs` to `vad/mod.rs` (it is a pure struct with unit tests), construct
  it in `EarshotVad::new(threshold)` with `exit = max(threshold − 0.15, 0.05)`,
  and drive `last_voiced` from it. Keep the clamp/non-finite guards.
- `audio_toolkit/constants.rs`: `VAD_FRAME_SAMPLES = 256`, `VAD_FRAME_MS = 16`,
  delete `VAD_CONTEXT_SAMPLES`; rewrite the doc comments (they describe Silero's
  512-sample contract).
- `audio_toolkit/vad/mod.rs`: delete `mod silero`, its re-export, and any
  Silero-specific wording; `frames_for_duration_ms` and the `VAD_*_MS`
  constants are unchanged (they were designed for this).
- `managers/audio.rs`: delete `SILERO_VAD_THRESHOLD`, the `VadBackend` match in
  the detector constructor (l.287–311), `preload_vad`'s model-path logic
  (Earshot constructs in microseconds — the preload thread in both actions can
  stay as a no-op or be removed), the backend-switch command path
  (`set_vad_backend` / recorder swap, l.890–935) — keep the recorder rebuild
  helper if anything else uses it.
- `settings.rs`: remove `VadBackend` and `vad_backend`; add schema migration 6
  that drops the key (serde ignores unknown keys, so this is only for
  cleanliness) — **do not** fail on old stores.
- `shortcut/mod.rs`: remove `change_vad_backend_setting` (+ `lib.rs`
  `collect_commands!`).
- `audio_toolkit/bin/cli.rs`: build the Earshot chain instead of Silero.
- `lib.rs`: remove Silero model-path resolution (`resources/models/…`) and the
  `VAD_CONTEXT_SAMPLES` mention if any.
- `resources/models/silero_vad_v6.2.onnx`: delete; `tauri.conf.json` bundle
  resources if listed; `build.rs` if it stages it.
- Tests: `tests/vad_speech_clock_probe.rs` → Earshot chain; `tests/vad_backend_bench.rs`
  → keep as a single-backend regression bench (or delete once Silero is gone);
  `recorder.rs` unit tests use `FRAME_MS` = 16 now — re-check the heartbeat
  test (150 ms → 10 frames) and the hangover test (1650 ms → 104 frames).
- Cargo: remove `ort`, `ndarray` (verify `cargo tree -i ndarray` is empty
  afterwards — `transcribe-rs` may still pull it until Phase 3).

Frontend:

- Delete `VadBackendSelector.tsx`; remove it from `AdvancedSettings.tsx`;
  remove the `vad_backend` updater in `settingsStore.ts`; remove
  `settings.advanced.vadBackend.*` from **all 24** locale files (script it like
  `scratchpad/fix_i18n.ts` did); regenerate `bindings.ts` (`bun run tauri dev`).

Docs: rewrite `AGENTS.md` "Voice Activity Detection" around the Earshot
contract (16 ms frames, [-1, 1] input, hysteresis, no model file), fix
`README.md` core libraries, `CHANGELOG.md` (Removed + Changed), `CRUSH.md`
model-setup note.

Manual checks: speech/silence indicator does not flap at a steady hum; speech
timer stalls in pauses; a recording of pure room noise is skipped by
`MIN_SPEECH_MS_TO_TRANSCRIBE`; Multi-STT and direct streaming unaffected.

### Phase 2 — Retire the legacy ONNX model definitions (user-facing migration)

- `managers/model.rs`: delete the 11 ONNX entries (l.754–1160) from the
  built-in list; keep `small/medium/turbo/large/breeze-asr` (transcribe-cpp).
- Add a **migration table** applied in `settings.rs` schema 7 to
  `selected_model` and `multi_stt_model_2/3/4`:

  | Old id                    | New default                                                                               | Why                                |
  | ------------------------- | ----------------------------------------------------------------------------------------- | ---------------------------------- |
  | `parakeet-tdt-0.6b-v2/v3` | `handy-computer/parakeet-unified-en-0.6b-gguf`                                            | same family, streaming, in catalog |
  | `moonshine-*`             | `handy-computer/parakeet-unified-en-0.6b-gguf`                                            | English streaming replacement      |
  | `sense-voice-int8`        | best multilingual GGUF with `lang_detect` (Whisper turbo / Qwen3-ASR — pick from catalog) | CJK                                |
  | `gigaam-v3-e2e-ctc`       | Whisper large/turbo GGUF                                                                  | Russian                            |
  | `canary-*`                | Whisper GGUF with `translate: true`                                                       | translate to English               |
  | `cohere-int8`             | Whisper turbo GGUF                                                                        | multilingual                       |

  The migration writes a log line and emits a one-time toast ("Model X is no
  longer supported; switched to Y — download it from the model picker").
  The new model may not be downloaded yet: the existing "model not downloaded"
  flow already handles that.

- Models settings: add "Remove unused model files" listing directories under
  the models folder that no catalog entry references (the old ONNX tarball
  extracts), with sizes, behind a confirmation. Never automatic.
- `commands/models.rs` `rescan_local_models`: stop recognising `.onnx` /
  directory models as custom models (only `.gguf` / `.bin`).
- `scripts/gen_catalog.py` / `mirror_models.py`: drop any ONNX handling.
- Headless CLI (`--transcribe-file`, `--list-models`): verify they only see
  GGUF models.

Manual checks: upgrade a settings store that selects `sense-voice-int8` with
Multi-STT slots on `moonshine-base` and `canary-1b-v2` → app starts, toast
shown, slots remapped, nothing crashes when the replacement isn't downloaded.

### Phase 3 — Remove the ONNX engine code

- `managers/transcription.rs`: `LoadedEngine` becomes a single-variant enum or
  a plain `Session` wrapper (keep the enum if Multi-STT's `extra_engines` API
  reads better with it); delete the seven `create_engine` branches and the
  seven `transcribe_with_engine` branches; delete `normalize_cjk_language` if
  its only callers were SenseVoice/Cohere (grep — Whisper language codes are
  handled by transcribe.cpp); delete the ORT half of `apply_accelerator_settings`
  and `init_transcribe_backend` stays; drop `use transcribe_rs::*`.
- `managers/model.rs`: `EngineType` → `TranscribeCpp` only (or remove the enum
  and the `engine_type` field from `ModelInfo`/bindings — prefer removing, one
  less thing in `bindings.ts`); remove `is_directory` plumbing and tarball
  download/extract paths in `model/download.rs` if GGUF never uses them
  (verify: `download.rs` has resumable single-file HTTP; the `.tar.gz` branch
  exists for ONNX bundles).
- `managers/model_capabilities.rs`, `gguf_meta.rs`: untouched (GGUF only).
- `audio_toolkit/lang_id.rs`, `text.rs`: untouched (text-based, no ONNX).
- `settings.rs`: remove `OrtAcceleratorSetting` / `ort_accelerator` (+ command,
  `lib.rs`, `settingsStore.ts`, `AccelerationSelector.tsx` ORT dropdown,
  i18n `settings.advanced.acceleration.ort*` keys in 24 locales, `bindings.ts`).
- `build.rs`: delete `stage_onnxruntime_dll()` and the `gigaam_vocab.txt`
  staging; delete `resources/models/gigaam_vocab.txt`.
- Cargo: remove `transcribe-rs`, then `ort`/`ndarray` if Phase 1 left them via
  transitive deps; run `cargo tree | grep -iE "ort|onnx|ndarray"` → empty.
- Frontend: `ModelCard.tsx` / onboarding / `ModelsSettings.tsx` — remove any
  ONNX-specific badges or branches (`engine_type`, `is_directory`); `bindings.ts`
  regen.

Manual checks: cold start, download a GGUF model, transcribe, Multi-STT with
three GGUF extras, quant benchmark, latency panel, headless CLI.

### Phase 4 — Build, packaging, CI, Nix

- `.github/workflows/build.yml`: delete the ORT download/bundle steps
  (l.360–390) and `ORT_*` env; `test.yml`: nothing ORT-specific but re-check;
  this is also the moment to fix the **Vulkan-vs-CUDA mismatch** listed in
  `KNOWN_ISSUES.md` (#47), since the workflow is being touched anyway.
- `flake.nix`: drop `onnxruntime` and `ORT_LIB_LOCATION`; re-run
  `bun install` on a Nix machine so `.nix/bun.nix` is current.
- `tauri.conf.json` / `tauri.windows.conf.json`: remove ORT dylib/DLL from
  bundle resources if present; confirm installer size drop.
- `BUILD.md`: remove ORT prerequisites; `.cargo/config.toml` unchanged.

### Phase 5 — Documentation and release

- `AGENTS.md`: Technology Stack (remove `transcribe-rs`, `ort`, `ndarray`;
  describe `earshot`), Backend Structure (`vad/` entries), Voice Activity
  Detection section rewritten, Settings System (remove `vad_backend`,
  `ort_accelerator`), Platform Notes.
- `README.md`: "How it works", core libraries, model list wording.
- `CHANGELOG.md`: `### Removed` (ONNX Runtime, transcribe-rs, Silero, 11
  legacy models, two settings) and `### Changed` (Earshot default with
  hysteresis, migration table); `src/content/release-notes/<version>.md`
  with the user-facing migration note; bump the fork version so the
  What's-New modal shows it.
- `docs/KNOWN_ISSUES.md`: close what this resolves, add "Earshot-only: no
  fallback VAD" as a known limitation with the bench numbers.

### Phase 6 — Verification and rollback

Gate before merging `drop-onnx` into `Handy_Multi_STT`:

- All automated checks green; `cargo tree` free of `ort`, `onnx`, `ndarray`,
  `transcribe-rs`; installer size compared to the tagged build.
- The Phase 0 bench re-run on the same recordings with the final Earshot
  thresholds; numbers appended to §7.
- Settings fixtures from Phase 0 load; a store with every legacy model id
  selected migrates.
- Rollback = `git revert` of the phase merges or `git checkout pre-drop-onnx`;
  user settings are forward-compatible (unknown keys ignored) so downgrading
  keeps working except the migrated model ids, which the old build simply
  shows as "not downloaded".

## 5. Risks

| Risk                                                                                                                      | Mitigation                                                                                                                                                  |
| ------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Earshot underperforms Silero in noisy rooms (its accuracy claims are the vendor's; the fork's 18 s bench is a quiet room) | Phase 0 bench in noise **before** any deletion; keep `SmoothedVad` prefill/hangover which absorbs short misses; threshold/hysteresis tunable constants      |
| Speech stats / silence-skip regress invisibly (the v4-wrapper bug went unnoticed for this reason)                         | Golden `SpeechClock` test at 16 ms; probe test kept and pointed at Earshot; the bench prints voiced fraction so a "everything voiced" regression is visible |
| Users on Moonshine/SenseVoice/GigaAM/Canary/Cohere lose a model they picked deliberately                                  | Migration table + toast; on-disk files untouched; release note lists exact replacements                                                                     |
| Multi-STT slots referencing ONNX models                                                                                   | same migration applied to `multi_stt_model_2/3/4`                                                                                                           |
| Upstream merges conflict in `transcription.rs`/`model.rs`/`build.yml`                                                     | Accept (fork already diverges); keep deletions in dedicated commits so `git rerere` / cherry-picks stay tractable                                           |
| `transcribe-rs` removal drags `hf-hub` or `ndarray` usage elsewhere                                                       | `cargo tree -i` checks per phase; `hf-hub` is kept on purpose                                                                                               |
| Direct-streaming / Live overlay depend on `supports_streaming` capability; Moonshine streaming was ONNX                   | transcribe.cpp streaming (`StreamExtension`) already drives the overlay for GGUF models; no change                                                          |

## 6. Effort (single developer, rough)

| Phase | Size                    | Notes                                                 |
| ----- | ----------------------- | ----------------------------------------------------- |
| 0     | ½ day + recording time  | mostly collecting noisy recordings                    |
| 1     | 1 day                   | mechanical; hysteresis + constants are the only logic |
| 2     | 1 day                   | migration table needs catalog id verification         |
| 3     | 1–2 days                | largest diff; `transcription.rs` is 3.6 k lines       |
| 4     | ½ day (+ CI iterations) | CI cannot be tested locally                           |
| 5     | ½ day                   |                                                       |
| 6     | ½ day                   |                                                       |

Order matters: **1 before 3** (VAD is independent and low-risk), **2 before 3**
(users are migrated while the old engines can still load their models one last
time, which lets the toast name the model).

## 7. Measurements (append as phases complete)

Baseline, 2026-08-26, 5 recordings / 18 s / quiet room / one desk mic
(`tests/vad_backend_bench.rs`):

|                      | Silero v6.2 (ort)                              | Earshot 1.2.2 |
| -------------------- | ---------------------------------------------- | ------------- |
| Construction         | 109 ms                                         | 9 µs          |
| Compute per frame    | 120 µs / 32 ms                                 | 27 µs / 16 ms |
| RTF                  | 0.0039                                         | 0.0017        |
| Raw voiced           | 43 %                                           | 41 %          |
| Kept after smoothing | 13.3 s                                         | 12.9 s        |
| Frame agreement      | 97.5 % (Silero-only 2.0 %, Earshot-only 0.5 %) |               |

With Earshot hysteresis (same recordings, 2026-08-26): raw voiced 7.9 s vs
Silero 7.7 s, kept **13.3 s vs 13.3 s**, frame agreement 97.7 % (Silero-only
0.4 %, Earshot-only 2.0 %) — hysteresis closed the "Earshot is slightly more
conservative" gap.

Missing: noisy-room numbers (Phase 0).
