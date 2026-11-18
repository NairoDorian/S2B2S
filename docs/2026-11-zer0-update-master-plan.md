# ZER0 November 2026 update — master plan

Written 2026-11-15. This is the **execution plan** for the large ZER0 update the
user asked for on that date. It replaces "do it when asked" in
[`2026-11-transcribe-fork-handoff.md`](2026-11-transcribe-fork-handoff.md): the
user has now asked. The handoff stays the engine-side record and holds the
detailed recipes this plan cites (§2.3 allowlist, §3.1 VAD/denoise).

The user's request, in short:

1. Pull the newest transcribe.cpp fork and use everything it now offers.
2. Drop ZER0's Rust VAD (`earshot`) and denoise (`nnnoiseless`) layers in favour
   of the engine's SIMD C++ ports.
3. Ship a "languages I speak" list that constrains the STT models, using the
   engine's allowlist **and** upstream Handy's spoken-language preference.
4. Merge the 102 upstream Handy commits by hand, one at a time, adapted smartly.
5. Finish the uncommitted work in the tree, including the new window menu
   (Live Canvas, see D1).
6. Fix the llama.cpp CUDA 13.5 detection and download (added mid-request).

Every step below is one focused commit (or a short series) that leaves the
gate green. Tick a step by writing its ZER0 commit hash next to it.

---

## Sources read for this plan

| Source                                                                                                       | What it contributed                                                                                                  |
| ------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------- |
| `docs/2026-11-transcribe-fork-handoff.md`                                                                    | Engine changes since ZER0's pin, the allowlist recipe (§2.3) and the VAD/denoise migration recipe (§3.1)             |
| `docs/TRANSCRIBE_CPP_INTEGRATION.md`, engine `docs/zer0-engine-integration.md`                               | Paired-development rules and the boundary traps                                                                      |
| Engine `docs/plans/2026-11-fork-master-plan.md` §A, §B.7, §B.9, §F                                           | Allowlist semantics, the quiet-clip stream gate defect, the engine replacements, the app's one-VAD-owner decision    |
| `docs/2026-09-native-earshot-vad-and-zer0-integration-plan.md`                                               | Historical spec; its §1.2 parity table becomes acceptance criteria for Phase 5                                       |
| Engine Rust bindings at `cfb07a12` (`earshot.rs`, `denoise.rs`, `langid.rs`, `session.rs`)                   | Exact APIs: `VadGate`, `Denoiser`, `score_frames`, `LangIdSession`, `RunOptions::{allowed_languages, vocabulary}`    |
| Upstream Handy `8f9cf53c..aca89851` (local checkout `../STT_BRAIN_TTS/Handy`)                                | 102 commits, every message and file list read; the language-detection policy, tray submenu and settings read in code |
| AIVORelay `.AGENTS/upstream-sync-log.md` and its commits since 2026-09-15                                    | A sibling fork's disposition of the same 102 commits: a second opinion, not a template                               |
| `../Llama.cpp/download-llama_v2.ps1`, `src-tauri/src/llama_releases.rs`, `nvidia-smi`                        | The CUDA 13.5 root cause (Phase 1)                                                                                   |
| The uncommitted tree (`git status`, `git diff`), `bun run typecheck / test:unit / check:translations / lint` | What is in flight and that it currently passes                                                                       |

State on 2026-11-15:

- **ZER0**: HEAD `51a69d55` on `Handy_Multi_STT`, 63 modified + 16 untracked
  files (Phase 0). Committed lock pins engine `ba949120`; the dirty lock pins
  `468e8776` (crate 0.3.1).
- **Engine**: `transcribe-fork` clean on `main` at **`cfb07a12`** (VadGate,
  Denoiser, SIMD Earshot, allowlist + `allowlist_letters`, upstream 0.4.2, ggml
  1.0, CPU Whisper fix). 66 commits past the dirty lock.
- **Upstream Handy**: `v0.10.1` at `aca89851`; ZER0's recorded cursor is
  `8f9cf53c` (merge `596eb942`). Upstream is still React 18 — every frontend
  commit is a **re-implementation** in Solid 2, never a cherry-pick.

---

## Decisions (recommendations in bold)

**All eight confirmed by the user on 2026-11-15**, with the standing rule: when
an upstream change would break a ZER0 feature, do not port it as-is — adapt it
or skip it, and record why in the ledger.

| #   | Question                                                                                                                     | Recommendation                                                                                                                                                                                                                                                       |
| --- | ---------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| D1  | Is "the new window menu" the uncommitted **Live Canvas** (new sidebar entry + frameless, transparent full-window canvas)?    | **Assumed yes** — it is the only window/menu work in flight. Phase 0.2 lists what is missing. Say so if you meant something else (e.g. a custom title bar for the main window).                                                                                      |
| D2  | Port upstream's out-of-process engine supervisor (`c8262ab`, #2208)?                                                         | **No.** Its rule "at most one worker holds a model" contradicts Multi-STT (primary + up to 8 extras, dual-GPU per-model backends, shared stream router, Live FFT). Take its fault handling instead: Phase 7.9 (`2eab8fc`). Record the decision in `KNOWN_ISSUES.md`. |
| D3  | Move to the `handy-recorder` crate (`a42131d`, #2188)?                                                                       | **No.** ZER0's recorder carries the raw tap, `AnalysisSink` FFT taps, denoise chain, chunk tap, Live Mode rotation and deferred device switch; Phase 5 rewrites the same code anyway. AirPods mode (`8c293dd`) depends on it: deferred.                              |
| D4  | Transcription hook (`033c627`, #930): upstream's implicit `hooks/transcription` file, or an explicit setting?                | **Upstream's protocol** (opt-in by the file existing, stdin → stdout, after post-processing, 30 s, cancellable), path from `portable::app_data_dir`. Not applied to text already typed by `DirectStreaming` / streaming-first `owns_typing`.                         |
| D5  | Post-processing deadline (`0911f2b`): upstream bounds the whole step to 60 s; AIVORelay kept 600 s.                          | **One deadline around the whole step, default 60 s for remote providers**; a local `llama-server` start-on-demand cold start is waited for separately (its own health timeout), so the 60 s is inference only.                                                       |
| D6  | Chinese script as its own setting (`0bb0428` + `c67ec6a` + `b5920f2`) instead of `zh-Hans`/`zh-Hant` in `selected_language`? | **Yes**, with a schema migration (`zh-Hant` → `selected_language: zh` + `chinese_script: traditional`). Text-based conversion fixes mixed Mandarin/English that Whisper tags as `en`.                                                                                |
| D7  | Switch to upstream's catalog package (`93c9ee0`, `@handy-computer/models`)?                                                  | **No** — ZER0's catalog carries fork-only entries (Parakeet ultra/redux, R2T2 Q4_K_M repo override). Port only the four deletion fixes and add the language detector entry by hand.                                                                                  |
| D8  | Download the language detector (VoxLingua107 ECAPA, Q8_0 26.7 MB, CPU) on demand?                                            | **Yes, only when needed**: two or more spoken languages **and** a primary or extra model that cannot detect the language itself (Canary, Cohere, Voxtral, Granite NAR).                                                                                              |

---

## Phase order and why

| Phase | Content                                                                              | Why here                                                                                                    |
| ----- | ------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------- |
| 0     | Land the uncommitted work (typography, Live Canvas, engine 0.3 API adaptation, docs) | Every later phase edits the same files (`transcription.rs`, `settings.rs`, all locales, `App.tsx`)          |
| 1     | llama.cpp CUDA 13.5 fix                                                              | Broken today, self-contained, no dependency on the rest                                                     |
| 2     | Engine bump to `cfb07a12`+                                                           | Phases 5 and 6 need its APIs; isolating the bump makes engine regressions bisectable                        |
| 3     | Upstream batch U1 — small backend fixes + dependency items                           | Low risk, chronological, shrinks the ledger before the big work                                             |
| 4     | Upstream batch U2 — frontend a11y/UX fixes                                           | The language picker in Phase 6 is built on `Dropdown`/`Select`, which these fix                             |
| 5     | One VAD + denoise owner: the engine                                                  | Biggest app-side engine change; lands before languages so the language detector reads the new gate's speech |
| 6     | Languages I speak (allowlist + spoken languages + detector + tray + Chinese script)  | Absorbs the upstream language commits                                                                       |
| 7     | Upstream batch U3 — transcription / LLM pipeline                                     | Touches the same run-option builders as Phase 6                                                             |
| 8     | Upstream batch U4 — notices framework and its dependants                             | Needs Phase 7's failure paths                                                                               |
| 9     | Upstream batch U5 — files (History, Opus)                                            | Independent; after notices so file failures report through them                                             |
| 10    | Platform and docs batches (Linux, macOS, docs)                                       | Compile-only on this machine; least verifiable, so last                                                     |
| 11    | Close-out: ancestry merge, docs, CHANGELOG, release                                  |                                                                                                             |
| 12    | Deferred work, recorded so it is not lost                                            |                                                                                                             |

### Rules for every phase

- `bun run precommit` before each commit; `bun run precommit:full` at the end of
  each phase. Rust verification on the CPU lane (`bun scripts/cargo-cpu.ts …`,
  `bun run dev:cpu`), never `--release` or a CUDA lane for tests. Close the
  running ZER0 before any cargo command (a live process locks the transcribe
  DLLs, os error 32).
- `src/bindings.ts` is regenerated by a debug `tauri dev` run (or hand-edited to
  match) whenever a command or settings field changes.
- New strings go into all 26 locales (`check:translations`). Locale files are
  CRLF-sensitive under prettier; check `format:check` after any stash/checkout.
- Each upstream port cites the upstream sha and PR in its commit message
  (`fix: … (upstream 8ef8dd4, #2156)`), states the cost of any new thread, poll
  or dependency (`docs/PERFORMANCE.md` rule 10), and ends with the Co-Authored-By
  trailer. Never edit the `LICENSE` attribution.
- Never spell the product name by hand (`scripts/app-meta.ts`). Upstream text
  that says "Handy" is rewritten through `app_identity` / `appIdentity`;
  `HANDY_*` environment variables become `ENV_PREFIX` + suffix with the legacy
  fallback (`app_env_var`).
- When a phase changes the engine boundary, update this file,
  `TRANSCRIBE_CPP_INTEGRATION.md`, the handoff and the engine's
  `docs/zer0-engine-integration.md` together.

---

## Phase 0 — Land the work already in the tree

**Done 2026-11-15** — `eeb2e9a5` (engine bump + pin checker, which also
completes Phase 2.1–2.2), `ad03ca52` (Live Canvas + fonts), and the docs
commit that follows them. Verified: CPU-lane clippy and `cargo test
--all-targets` (482 passed, 2 ignored), Playwright live-canvas + fonts specs
6/6, `bun run precommit`. Open from 0.2: step 4's WebView2 cost and step 8's
real-window check over CDP need the running app (`bun run dev:cpu`); 0.3
keeps the English fallback for the other 24 locales, as `TYPOGRAPHY.md`
says. `docs/SEARCH_ARCHITECTURE.md` stays untracked (a proposal).
The hotkey guide lists only global bindings, so the canvas's own keys (Esc,
Ctrl+L) live in its button tooltips and its Help section, not there.

### 0.1 Inventory (what the uncommitted changes are)

| Strand                    | Files                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| ------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Engine 0.3 API adaptation | `Cargo.lock` (pin `468e8776`, crate 0.3.1), `managers/transcription.rs` (`Model::capabilities()` now returns `Result`, five call sites; the stream worker reports a failure as `NeverStarted`)                                                                                                                                                                                                                                                                                                                                                                     |
| Typography                | `settings.rs` `TypographySettings` + tests, `shortcut::change_typography_setting`, `FontSettings.tsx`, `lib/typography.ts(+test)`, `styles/fonts.css`, `public/fonts/libron/`, `App.css` font tokens, `main.tsx`, `overlay/main.tsx`, `docs/TYPOGRAPHY.md`, `DESIGN.md`, `tests/fonts.spec.ts`                                                                                                                                                                                                                                                                     |
| Live Canvas               | `live-canvas/LiveCanvas.tsx` (rewrite: particle-carried characters, in-place corrections, telemetry), `settings/live-canvas/LiveCanvasSettings.tsx` (new sidebar page), `lib/liveCanvasMotion.ts`, `lib/liveCanvasTranscript.ts` (+tests), `stores/liveCanvasMotionStore.ts`, `commands::live_fft::set_live_canvas_active`, `LiveFftManager::set_canvas_active` + `scope.rs` (canvas keeps the shared analyser across Live Mode chunk gaps), `lib.rs` main window `.transparent(true)`, capability `core:window:allow-start-dragging`, `tests/live-canvas.spec.ts` |
| Small UI touches          | `Dropdown.tsx`, `Select.tsx`, `StatusBarPopover.tsx`, history/recall/live-mode/file pages, spectrum canvases (font token use)                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| Docs                      | `AGENTS.md` paired-development section, `PERFORMANCE.md` typography cost, the September VAD plan's status correction, new `TRANSCRIBE_CPP_INTEGRATION.md`, handoff, `SEARCH_ARCHITECTURE.md` (a proposal only)                                                                                                                                                                                                                                                                                                                                                     |

### 0.2 Finish Live Canvas (D1)

1. **i18n**: the canvas still hardcodes `Telemetry`, `Words`, `WPM`, `Speech`,
   `Elapsed` under the file-level `i18next/no-literal-string` suppression, and
   `Sidebar.tsx` has `aria-label="Open Live Canvas"`. Move them to keys
   (`settings.liveCanvas.telemetry.*`, `sidebar.openLiveCanvas`); keep the
   suppression only for what AGENTS.md allows.
2. **Typed IPC**: replace the raw `invoke("set_live_canvas_active", …)` with
   `commands.setLiveCanvasActive` (already in `bindings.ts`).
3. **Frameless window controls**: with decorations off the canvas has a drag
   strip and an exit button only. Add minimize / maximize-restore controls in
   the exit zone (or confirm exit-only is intended), double-click-to-maximize on
   the drag strip, and the matching capability permissions
   (`core:window:allow-minimize`, `allow-toggle-maximize`). Check that the
   window stays resizable from its edges while undecorated on Windows 11.
4. **Transparent main window**: `.transparent(true)` now applies to the
   settings window at all times. Confirm `body` keeps an opaque background
   outside the canvas in both themes, and measure WebView2 composition cost
   with the canvas closed (record it in `PERFORMANCE.md`).
5. **Words per minute**: currently settled words ÷ wall-clock `elapsed_ms`.
   After Phase 5 switch it to the gate's `speech_ms`, like the overlay.
6. **Discoverability**: QuickHelp line and Help anchor for the Live Canvas
   page; `Esc` / `Ctrl+L` in `lib/hotkeyGuide.ts`.
7. **Lint**: the two `consistent-function-scoping` warnings in `LiveCanvas.tsx`.
8. **Tests**: run `tests/live-canvas.spec.ts` and `tests/fonts.spec.ts` through
   the Playwright harness; then the real window over CDP (`dev:cpu`): open,
   record with Live Mode, chunk rotation, exit restores decorations and shadow.
9. **Docs**: AGENTS.md Live Canvas and Fonts paragraphs (frontend structure +
   settings list), CHANGELOG, `PERFORMANCE.md` (one worker shared with the
   overlay scope, no new thread).

### 0.3 Typography leftovers

Font copy exists in English and French only; the other 24 locales carry English
text under complete keys. Translate them, or record the English fallback as
intended in `TYPOGRAPHY.md` (it already says so).

### 0.4 Commit split

1. `fix(engine): adapt to transcribe.cpp 0.3 capabilities() Result` — lock +
   `transcription.rs`. Verify with `bun scripts/cargo-cpu.ts check` first; the
   runner may move the pin to `cfb07a12` at the same time, which is Phase 2's
   job — if it does, fold Phase 2.1 into this commit.
2. `feat(ui): font preferences with bundled Libron`.
3. `feat(live-canvas): …` (after 0.2).
4. `docs: paired development with transcribe.cpp, November handoff`.
5. `SEARCH_ARCHITECTURE.md` stays out of this update (a proposal with no code);
   commit it separately as a proposal or leave it untracked.

**Done when** the tree is clean and `bun run precommit:full` passes.

---

## Phase 1 — llama.cpp CUDA 13.5 detection and download

**Done 2026-11-15** (steps 1, 2, 4, 5, 7; step 3 keeps the synthesized cudart name, now with a toolkit-mismatch warning instead of an automatic 500 MB download). Step 8 (a real install) is still to run in the app.

### Root cause (verified on this machine)

- Driver 621.14's `nvidia-smi` header reads `KMD Version: 621.14  CUDA UMD
Version: 13.5`. `detect_backend_uncached` (`llama_releases.rs`) searches for
  the literal `"CUDA Version:"`, finds nothing, and because `nvidia-smi`
  succeeded returns the hardcoded **`cuda-13.4`**.
- llama.cpp releases now ship `cuda-13.5` and `cuda-12.4` builds (no 13.4).
  `pick_asset("cuda-13.4")` misses the exact name and falls through to "first
  asset starting with `cuda-`", i.e. whatever GitHub lists first — the **12.4**
  build. That build needs `cudart64_12.dll`; the installed toolkits are
  v13.3/v13.4/v13.5 (`cudart64_13.dll`), and `include_cudart` is off by default,
  so the server fails to load CUDA.
- `LlamaSettings::normalized` (`settings.rs`) and the UI list `BACKENDS`
  (`LlamaSettings.tsx`) only accept `auto | cuda-13.4 | cuda-12.4 | vulkan |
cpu`, so a user cannot even pick `cuda-13.5`.

The fix the script already made (`download-llama_v2.ps1` §4): choose from the
release's own file list, never from a fixed list of versions — the newest CUDA
build the driver supports; an exact `cuda-X.Y` only when asked for; stop with a
clear message instead of silently installing another backend.

### Steps

1. `detect_driver_cuda() -> Option<(u32, u32)>`: parse `CUDA (UMD )?Version:\s*X.Y`
   (both header forms). `detect_backend()` returns `"cuda"` when an NVIDIA GPU
   is present and `"cpu"` otherwise — no version baked in. Keep the cache.
2. `pick_asset(release, backend, driver)`:
   - `auto` / `cuda`: every asset whose backend parses as `cuda-X.Y`, sorted by
     version descending; the first `<=` the driver's CUDA (or the newest when
     the driver version is unknown). No CUDA build at all → Vulkan → CPU, and
     the result says why (surfaced in the install event / UI).
   - `cuda-X.Y`: exact match or an error naming the CUDA builds the release has.
   - `vulkan`, `cpu`, others: unchanged.
3. cudart package: prefer the asset the release actually lists
   (`cudart-*-win-cuda-X.Y-x64.zip` among all assets — keep cudart names in the
   parsed release instead of filtering them out), synthesize the name only as
   the script's fallback. Offer it automatically (as the script does) when
   `cudart64_<major>.dll` is found neither on PATH nor in `detect_cuda_toolkit`.
4. Settings: `backend` accepts `auto | cuda | cuda-\d+\.\d+ | vulkan | cpu`;
   a stored `cuda-13.4` / `cuda-12.4` migrates to `cuda` (newest supported) —
   leaving the exact value would make the next install fail on a release that
   no longer has it.
5. UI: build the backend list from the chosen release's assets (`auto`,
   `cuda (newest the driver supports)`, each `cuda-X.Y` present, `vulkan`,
   `cpu`), and show the detected driver CUDA version next to it.
6. Already correct, keep: `detect_cuda_toolkit` orders `CUDA_PATH_V*` by parsed
   version and handles `bin\x64` (CUDA 13); `remove_bundled_cuda_runtime`.
7. Tests (`llama_releases.rs`): both header forms; `parse_backend` for
   `cuda-13.5`; picking 13.5 with driver 13.5, 12.4 with driver 13.4 when only
   13.5/12.4 exist, newest with unknown driver; exact-version miss errors;
   settings migration of the two old values.
8. Check end to end: Local LLM page → install latest → `cuda-13.5` build
   chosen, server starts on the RTX GPU without the cudart package (toolkit
   13.5 on PATH via `CUDA_PATH`).

Cost: none at runtime (one `nvidia-smi` run per process, as today).

---

## Phase 1b — Every dependency to its newest version (added 2026-11-15)

Asked for after Phase 1: "update all dependencies to latest, including Tauri 3
and Solid 2", forks included.

- **Tauri family** — moved by hand as one set (the update script deliberately
  never moves it): `tauri` / `tauri-runtime-wry` 3.0.0-alpha.6,
  `tauri-runtime` alpha.5, `tauri-build` / `tauri-codegen` / `tauri-utils` /
  `tauri-macros` / `tauri-plugin` alpha.4, plugins on `v3` `14ed4c4f`,
  `@tauri-apps/cli` alpha.6. Code impact: `macos-private-api` and
  `app.macOSPrivateApi` removed; nothing else (ZER0 never called the inherent
  `run_on_main_thread`). CPU-lane clippy clean.
- **tauri-specta fork** (`NairoDorian/tauri-specta-v3`, `8109588`) — upstream
  rc.29 merged, Tauri 3 adaptation on top (see `docs/FORKS.md`). ZER0 pins
  `ErrorHandlingMode::Result`, because rc.27 changed the default to `Throw`.
- **tauri-plugin-macos-permissions fork** — no change needed: its upstream has
  nothing new and its `3.0.0-alpha.0` requirement resolves to alpha.6.
- **Patches dropped** — Specta (rc.28 / 0.0.15 published) and tao (0.37.2
  published contains #1218 / #1274).
- **Everything else** — `bun run update` (RTK, `update-deps --prerelease`,
  repomix). `scripts/update-rtk.ts` needed the package name (`rtk`): the RTK
  repository now has two binaries. Result: Solid 2.0.0-rc.21, Vite 8.3.8,
  TypeScript 7.1 dev, i18next 27, sysinfo 0.40 and every other direct
  dependency at its newest; `rusqlite` held on 0.40 (`rusqlite_migration` 2.7
  is on `rusqlite` 0.41 — two `libsqlite3-sys` cannot link). Fixes needed:
  sysinfo 0.40 API, i18next 27 `TFunction`, readonly tuples under TS 7.1.
- **Verified 2026-11-15/16** — CPU-lane clippy (3 pre-existing warnings), 488
  Rust tests, 69 unit tests, Playwright 16/16 (Chromium 1255 installed by hand
  over IPv4), and the real app launched with `bun run dev:cpu` (bindings
  regenerated by tauri-specta rc.29: same result shape, plus an optional
  `InvokeOptions` argument; settings schema 10 migrated `cuda-12.4` → `cuda`).

## Phase 2 — Engine bump to transcribe-fork `main` (`cfb07a12` or newer)

**Steps 1–2 done 2026-11-15, folded into Phase 0** (the bump was needed to
compile Phase 0's API adaptation once rather than twice).

1. ~~Move the pin.~~ Done: `Cargo.lock` pins `cfb07a12` (crate 0.4.2).
   **Trap found and fixed:** a plain `cargo update -p transcribe-cpp
-p transcribe-cpp-sys` (what `check-transcribe-deps.ts` ran) kept `log`
   0.4.34 / `serde_json` 1.0.151 locked; the fork needs 0.4.35 / 1.0.154, so
   cargo parked the fork under `[[patch.unused]]` and locked **crates.io
   `transcribe-cpp` 0.4.1** without an error. The right command was
   `cargo update -p transcribe-cpp@0.4.1 -p transcribe-cpp-sys@0.4.1 -p log
-p serde_json`. The script now verifies the `[[package]]` source, retries
   with `--recursive`, and restores the lock when the fork is not what got
   locked. After any future pin move, check: `grep patch.unused
src-tauri/Cargo.lock` must print nothing.
   Side finding: tauri `plugins-workspace` `v3` has moved to `14ed4c4f`, which
   cargo cannot resolve against the pinned Tauri core alpha; the lock keeps
   `d9be6d04`. That is `update-deps`' job (a Tauri core bump), not this plan's.
2. ~~Compile fixes.~~ Done: `capabilities()` → `Result` (5 sites);
   `ModelOptions` gained `no_mmap` (5 literals now end in
   `..Default::default()`, i.e. memory-mapped CPU weights, as upstream chose).
   No `ExtSlot` / `AbiStruct` matches needed changes. CPU-lane
   `clippy --all-targets` is clean apart from 3 pre-existing warnings
   (`actions.rs:623`, `actions.rs:1860`, `recall/mod.rs:561`).
   **Mmap check to add to step 4:** on Windows a mapped file cannot be deleted
   or overwritten while the model is loaded — delete a loaded primary and a
   loaded Multi-STT extra from the Models page, and re-download over an
   existing file, and confirm each unloads first or reports the error.
3. Behaviour changes to verify, not code: non-finite samples are rejected by
   `run` / `stream_feed` (`INVALID_ARG`; the resampler must never emit NaN);
   a stream that fails mid-feed releases its model lease; Parakeet/Canary #252
   punctuation shifts on long clips; full-attention Parakeet uses local
   attention above 20.5 s; CPU Whisper is correct again; Granite 4.1 ~2× faster
   on CUDA; CPU weights are memory-mapped (RSS accounting).
4. Verify: `bun scripts/cargo-cpu.ts clippy --all-targets`, `… test`, then
   `bun run dev:cpu` smoke (dictation, streaming, Multi-STT, Live Mode), then
   `bun run bench:stt` against the last baseline (`docs/STT_BENCHMARKS.md`) on
   CPU. The CUDA lane (`dev:fast`, nvcc 13.5 is now `CUDA_PATH`) is checked once
   by the user, not by the agent.
5. Update `native_build_identity()` expectations if they print the ABI hash
   (`0c46a78d4d6646a7`).
6. **Second bump, 2026-11-18: `cfb07a12` → `8b27bc6b`** (the user pushed six
   engine commits: Parakeet decoder on the GPU, conformer attention kept on
   CUDA, the Granite / Qwen3 / R2T2 step pipeline, the whole-fork audit). ABI
   hash and generated bindings unchanged; `grep patch.unused` empty. App
   adaptations in the same commit: `SpeechGate` no longer passes an explicit
   exit threshold (an explicit exit is kept across `set_threshold`, so the
   slider moved one edge only — a Phase 5 bug the engine's `vad-gate` change
   made visible); `arch_plugins.rs` stops registering the models folder;
   `build.rs` bundles the engine's license texts. Measured A/B through the
   app (CPU lane, Vulkan + CPU, 8 runs per arm, alternating): transcripts
   identical in all 18 cases, Vulkan 1.24-1.51x faster for Nemotron and the
   Parakeets, CPU level, R2T2 streaming on Vulkan 3.6 % slower (open,
   [STT_BENCHMARKS.md](STT_BENCHMARKS.md)).

---

## Phase 3 — Upstream batch U1: small backend fixes

**Done 2026-11-16/17** — every row below landed or was verified; the ledger
at the end carries the commit of each.

Chronological. Each is one commit (see the ledger at the end for every row).

| Upstream                                    | Change in ZER0                                                                                                                                                                                                                     |
| ------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `8ef8dd4` #2156                             | `audio_toolkit/text.rs`: drop `"ha"` from the English fillers (still present at line ~318)                                                                                                                                         |
| `eb49dc0` #2157                             | `text.rs`: re-capitalize the sentence start after removing a capitalized leading filler (+ upstream's tests)                                                                                                                       |
| `eea1f5f` #2161                             | `actions.rs`: refuse to start (no mic, no overlay) when no model is loaded and the selected model's files are missing; same check in Recall dictate and Live Mode start. Multi-STT checks the primary only                         |
| `f5c27e6` #2158                             | `shortcut/tauri_impl.rs`: validate with the accelerator parser so `option_left+space` (native-keys spelling) is reset when switching backend                                                                                       |
| `29bd2c0` #2160                             | `init_transcribe_backend`: register backends on the main thread, list devices on the prewarm thread (`report_compute_devices`); keep the headless path inline. Check `available_model_backends` does not force the listing earlier |
| `b38987a` #2190                             | `shortcut/mod.rs`: cancel-shortcut register/unregister record the requested state and spawn one reconcile pass, for both backends                                                                                                  |
| `8605699` #2117                             | `tauri.conf.json`: NSIS `installerIcon` (confirm `app-meta.ts` does not rewrite the key)                                                                                                                                           |
| `a94b403` #2116                             | `managers/model.rs` + `portable.rs`: keep reading a portable install's legacy model cache (mind the identity-rename store chain)                                                                                                   |
| `8020c87` #2327                             | **Covered** (checked 2026-11-15): `native_keys.rs` already creates `HotkeyManager` behind a readiness channel that returns the error, and `shortcut/mod.rs` falls back to the Tauri backend and persists it                        |
| `ca4a611` #2434                             | `transcription.rs`: a result with no letter or digit is empty — batch, stream finalize, Multi-STT slots, Live Mode chunks                                                                                                          |
| `56c7e21` #2097                             | `transcription_coordinator.rs`: stamp key events when observed and measure holds edge to edge (matters for the default hold-or-toggle: the first activation spends ~400 ms opening the mic)                                        |
| `73cc857` #2432, `4c811d7`, `f01b807` #2505 | `bun run update` covers them (TLS crates, Playwright, handy-keys); raise the `handy-keys` floor to `^0.3.5` and run `cargo audit` once                                                                                             |
| `f248301` #2318                             | Verify only: ZER0 uses `huggingface/hf-hub` main (1.1.0), not upstream's `cjpais/hf-hub` fork with the Range fix. Read its chunk path; port nothing unless it accepts a 200 for a ranged request                                   |
| `88185c8` #2478                             | Verify only: ZER0 has no per-record webview log target (`docs/LOGGING.md`), so the deadlock cannot occur. Record as not applicable                                                                                                 |

---

## Phase 4 — Upstream batch U2: frontend fixes (Solid re-implementations)

**Done 2026-11-17** — three commits (`b0b9c2fb`, `2e9e849c`, `b2f51e6f`).
The Escape handlers listen on the document in the capture phase while open,
so a menu or tooltip consumes Escape before an enclosing dialog; `0685192`
was already covered (the optimistic delete reloads and rethrows to the
row's handler). The accent colour's reset keeps its own existing label.

| Upstream                                          | Change in ZER0                                                                                                                                                                   |
| ------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `345caa8` #1941                                   | `Dropdown` gains `onOpen`; `SoundPicker` reads custom sounds when opened                                                                                                         |
| `20a6685` #2227, `417dc6a`                        | Overlay follows `model-state-changed`: "Loading model…" instead of a stuck "Transcribing…"; after 2 s of loading, open the Live preview with the notice (new overlay string ×26) |
| `5732406` #2256                                   | History: no duplicate rows during live updates                                                                                                                                   |
| `53db4cb` #2238                                   | `Slider` / `ToggleSwitch` expose the setting name to screen readers                                                                                                              |
| `a537ec4` #2253                                   | Reset buttons name the setting they reset (new key ×26)                                                                                                                          |
| `8fd85aa` #2246                                   | Escape dismisses `Dropdown` / `Select` and returns focus to the trigger                                                                                                          |
| `090b370` #2248                                   | Ignore the IME confirmation Enter (`isComposing`) in Custom Words, language search, model table search (`lib/utils/keyboard.ts`)                                                 |
| `0685192` #2255                                   | Report a failed History deletion                                                                                                                                                 |
| `77ad955` #2254, `26d2003` #2247, `a1cb29d` #2309 | `AudioPlayer`: named position control (key ×26), seek keeps position, release late sources after leaving History                                                                 |
| `3b60fc7` #2251                                   | Escape dismisses a focused setting tooltip                                                                                                                                       |
| `40b8e72` #2403                                   | `overlay/main.tsx`: `preventDefault` on `contextmenu` in production builds (complements `webview_hardening`)                                                                     |

---

## Phase 5 — One VAD and denoise owner: the engine

**Done 2026-11-17** — `a12ca902` (code), `560a5581` (measurement and a
correction), engine `7b4df8da` (its integration note; committed locally, not
pushed). Deviations from the recipe below: ZER0's `SpeechClock` stays (it
counts the gate's raw `voiced`, keeps the per-recording pause hold and the
`Disabled` path) instead of reading the gate's clock; the detector trait
stays for the recorder tests' scripted detectors. The quiet-clip claim in
the handoff is stale at `cfb07a12` (the stream gate no longer empties it);
the old capture pre-gate was what lost it. Real-microphone checks of 5.6
(slider mid-recording, denoise sliders, Live FFT taps, Live Mode and
Multi-STT breaks) still need a person at the microphone.

Recipe source: handoff §3.1. Upstream's own move (`3985508`, Silero through
transcribe.cpp, and `7b2a28a`, a pre-roll double-release fix) is **superseded**
by this phase: the engine's `VadGate` is the Earshot path ZER0 already tuned, and
its pre-roll ring never releases a frame twice.

Acceptance criteria = the September plan's §1.2 table (slider, hotkeys,
overlay indicator, WPM clock, live VAD test, History re-analysis), now required
**and verified**.

### 5.1 Denoise → `transcribe_cpp::Denoiser`

`audio_toolkit/audio/denoise.rs`: keep `DenoiseControls` (atomics) and both
`FrameResampler`s; replace `nnnoiseless` + `Gate` + `denoise_frame` +
`scaled_*` + `dry_delay` with one `Denoiser`. Per 48 kHz frame: `set_params`
only when the controls changed (`DenoiseParams::normalized()` is ZER0's clamp),
then `process_frame(frame, &mut out)` → RNNoise probability (feeds
`VadFrameReport::denoise_prob`). `finish` → `flush(&mut out)` instead of pushing
`SILENT_FRAME`; `reset` → `Denoiser::reset`. The Live FFT `Denoised` tap still
sees `out`. Expected: output within one int16 step of today, 1.41× faster.

### 5.2 Earshot + SmoothedVad + SpeechClock → `transcribe_cpp::VadGate`

One `VadGate` per capture source (`CaptureProcessor`):

| Today                                                         | With `VadGate`                                                                                                                                    |
| ------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| `EarshotVad` + `Hysteresis` (enter t, exit max(t−0.15, 0.01)) | `VadGateParams { threshold, exit_threshold: None, … }`                                                                                            |
| `VAD_ONSET_MS` 60, `VAD_PREFILL_MS` 450                       | `onset_ms: 60`, `preroll_ms: 450`                                                                                                                 |
| `VadPolicy::Offline` 450 / `Streaming` 1650 hangover          | `hangover_ms`, switched per recording with `set_hangover_ms(450 \| 1650)`                                                                         |
| `SpeechClock` + `speech_pause_hold_ms`                        | `speech_hold_ms` (construction-time: a changed setting rebuilds the gate at the next recording start), `speech_ms`, `clock_speaking`              |
| `last_frame_voiced` / `last_frame_score` / level              | `VadGateUpdate::{voiced, last_score, level_dbfs}` (the live VAD test and `VadMeter`)                                                              |
| `set_vad_threshold` mid-recording                             | `VadGate::set_threshold` (0.05..=0.95)                                                                                                            |
| released frames → batch buffer, stream feed, chunk tap        | `VadGateUpdate::audio` (borrowed slice; keep the 256-sample framing downstream so `StreamRouter`, the chunk tap and `AnalysisSink` are unchanged) |
| end-of-recording diagnostics                                  | `finish()` then `tail()` (`withheld_frames`, …)                                                                                                   |
| `VadPolicy::Disabled`                                         | bypass the gate; the speech clock counts every frame, as today                                                                                    |

- `SpeechActivityEvent`, `MIN_SPEECH_MS_TO_TRANSCRIBE`, `last_speech_ms()`
  (Live Mode boundary, Multi-STT streaming break detection) read `speech_ms` —
  check the break detector's "standing still" semantics are unchanged.
- **Deliberate behaviour change**: the −45 dBFS pre-gate disappears (the engine
  scores every frame; 0.5 % of frames differ on 10 min of audio). This is what
  lets a quiet microphone reach speech. Rewrite AGENTS.md's VAD section
  accordingly (the pre-gate paragraph and "a quiet microphone needs gain").
- Audio thread stays allocation-free: `feed` returns a borrowed slice; confirm
  with the existing allocation discipline (no `to_vec` per frame added).

### 5.3 History re-analysis

`managers/history.rs` backfill: `transcribe_cpp::score_frames(pcm, threads)`
then the 0.5 / 0.35 hysteresis count (or a `VadGate` if the speech clock is the
wanted measure). Bit-identical to frame-by-frame scoring.

### 5.4 One owner for the streams

Pass `StreamOptions { enable_vad: false, .. }` in `run_stream_worker` (covers
the primary and every Multi Streaming STT extra slot), `benchmark_stream` and
`replay_stream_once`. R2T2 already opts out. This removes the engine's silence
fast path, so measure CPU per streaming family with VAD on and off
(`bench:stt` stream mode) and record it in `PERFORMANCE.md`. With the app VAD
off, the user asked for every frame: the engine gate stays off too.

### 5.5 Remove the Rust copies

Drop `earshot` and `nnnoiseless` from `Cargo.toml` (and earshot's dev-profile
`opt-level` override); delete `vad/earshot.rs`, `vad/smoothed.rs` and the RNNoise
internals of `denoise.rs`. Port or retire their tests (the engine's
`vad_gate_unit.cpp`, `denoise_unit.cpp`, `rnnoise_unit.cpp` and Rust tests cover
the algorithms; keep ZER0 tests for framing, policy switches and taps). Port
`tests/vad_speech_clock_probe.rs` to `VadGate`; keep `bin/cli.rs` compiling.

### 5.6 Verify

`bun run dev:cpu`, then real dictation: quiet and normal mic; denoise on/off,
strength and gate sliders; VAD slider mid-recording; VAD off; streaming and
batch; Live FFT taps (raw / denoised / processed); overlay speaking indicator
and WPM; Live Mode chunk boundaries; Multi-STT streaming-first breaks; History
re-analysis. Replay the quiet FLEURS French clip (−62 dBFS RMS) through the app:
it must transcribe (it streamed to nothing with the engine gate on).

Docs: AGENTS.md VAD + noise-suppression sections, `TRANSCRIBE_CPP_INTEGRATION.md`,
the handoff §3, the engine's `zer0-engine-integration.md` ("baseline traps"),
mark the September VAD plan superseded, CHANGELOG.

Cost: removes two crates; no new thread or poll.

---

## Phase 6 — Languages I speak

**In progress (2026-11-17).** Landed: 6.1 settings, 6.2 the policy port,
6.3 the allowlist in all five builders (`375de7b9`); 6.4 per-slot plans
with one shared detector run and 6.5 the detector (pinned Q8_0, download
only when needed, `get_language_status` line with a download button)
(`6efcf51b`); 6.7 the Chinese script setting with schema 11 (`15a39e2e`). The detector was checked on real speech: all seven FLEURS
clips identified, ~68 ms per 5 s. On 2026-11-18: 6.6's tray Language
submenu and 6.5's History language/source columns. Left: the onboarding
suggestion (6.6), the Multi-STT page showing each slot's plan (6.6), and a
real-microphone check of the allowlist (6.8).

Three mechanisms, one setting:

- **Engine allowlist** (`RunOptions::allowed_languages`, `allowlist_letters`) for
  every model with `Feature::LanguageAllowlist`: the model's own language
  decision is restricted to the list and letters outside it are refused.
  A union, not a lock — mid-sentence switches between listed languages survive.
- **Upstream's spoken-language policy + detector** (`126bed7`, `693aa63`) for
  models that cannot detect a language (Canary 1B v2, Cohere, Voxtral, Granite
  NAR): one compatible spoken language → that language; two or more → the
  VoxLingua107 ECAPA detector (`LangIdSession`, CPU, in-process) picks among
  them from the first seconds of speech.
- **Explicit language wins** everywhere (the global selector or a Multi-STT
  slot's own language).

### 6.1 Settings (`settings.rs`)

- `spoken_languages: Vec<String>` — **upstream's name and semantics** (codes,
  preference order, empty = off), so a store shared with upstream reads the
  same. Recognition codes (`zh`, not `zh-Hans`).
- `spoken_languages_any_letters: bool` — fork-only, maps to
  `AllowlistLetters::Any` ("allow foreign words").
- `language_detection: LanguageDetectionMode` and
  `language_detection_window_secs` — upstream's (`WhenNeeded`, the default: only
  for models that cannot detect the language; `Always`: every non-streaming
  model, restricted to the spoken languages). The window is seconds of
  **speech** the detector hears (default 5, VoxLingua107 accepts 0.5–30 s), a
  debug setting, clamped.
- Commands: `change_spoken_languages_setting` (trim, drop `auto`, dedupe in
  order), `change_spoken_languages_any_letters_setting`,
  `change_language_detection_setting`; `settingUpdaters` entries for each.
- Schema: no migration for the new fields (serde defaults); D6's Chinese-script
  migration is §6.7.

### 6.2 The decision — one pure, unit-tested function

Port upstream `language_detection/policy.rs` (`plan`, `status`,
`auto_language`, `compatible_preferences`, `detector_label`) and extend its
`Plan::Native` with the allowlist:

```text
plan(intent, spoken, model{languages, detects, streaming, supports_allowlist}, mode, detector_labels)
  intent is a supported code             -> Fixed(code)                       (explicit wins)
  model language-agnostic / no list       -> Native{allowlist: []}
  model detects natively                  -> Native{allowlist: compatible(spoken) if supports_allowlist}
                                             (Always mode + batch: Detect, fallback Native)
  model has letters-only allowlist        -> Native{allowlist: compatible(spoken)}   (Parakeet TDT v3/ultra/redux, Granite)
  no native detection, 1 compatible       -> Fixed(that language)
  no native detection, >=2 compatible     -> Detect{languages, allowed labels, fallback FirstLanguage}
  none compatible                         -> Native (and the UI says why)
```

The allowlist is passed whole when the model knows at least one listed language
(engine contract: a list the model knows nothing of returns `UnsupportedLanguage`;
a model without the feature returns it for any non-empty list). Normalize with
`normalize_cjk_language`, dedupe, drop `auto`.

### 6.3 Thread it through the run plan

Extend `TranscribeCppRunPlan` with `allowed_languages` and `allowlist_letters`
(handoff §2.3) and set them in all five `RunOptions` builders:
`run_stream_worker` (its capability tuple gains `supports_allowlist`, including
the error arm), `benchmark_stream`, `transcribe_internal`, `replay_stream_once`,
`transcribe_with_engine` (Multi-STT extras). Unit tests next to the existing
`transcribe_cpp_run_plan_*`: list reaches an Auto run; yields to a pinned
language; skipped for a model without the feature; skipped when the model knows
none of the languages, whole when it knows one; `zh-Hans` → `zh`; letters toggle
carried.

### 6.4 Multi-STT

Each slot gets its own plan against its own model's capabilities: a slot pinned
to a language keeps it (`apply_extra_model_settings`); otherwise Nemotron /
Qwen3 / R2T2 / Parakeet extras get the allowlist and a Canary extra gets
`Fixed`/`Detect`. **One detector run per recording**, shared by every slot whose
plan is `Detect` (the merge prompt then compares like with like). Streaming
paths cannot wait for detection: a streaming model gets its allowlist, a
non-detecting streaming model its first compatible language (upstream's and
AIVORelay's behaviour).

### 6.5 The detector

- Catalog: add `lang-id-voxlingua107-ecapa` (role `langid`, `minEngine` 0.4.0,
  Q8_0 26.7 MB; F16/F32 listed) and a top-level `languageDetector` pointer, as
  upstream's catalog has them. `catalog/mod.rs` must not list it as a
  transcription model.
- Download on demand (D8) through the existing downloader; dependent download
  when a newly downloaded model needs it.
- Port `language_detection/{detector,speech,mod}.rs`: CPU-only `LangIdSession`
  loaded on first use, unloaded with the primary model, run on the blocking
  pool over the gate's retained speech (min 0.5 s; `InputTooShort` → fallback).
- ~~History: record each transcription's language and its source
  (selected / model / detected / fallback) — additive columns, readable by
  older builds (upstream `126bed7` + `73c0958`'s compatibility rule).~~
  Done 2026-11-18: `language_source` added outside the numbered migrations
  (`ADDED_COLUMNS`, `user_version` unchanged); `language` now holds the
  language the text came out in. Derived from ZER0's existing per-run
  output-language evidence plus the plan's source (`language_used`), for
  batch runs, live streams, Multi-STT (the primary's) and History re-runs.
  A language chip on the History row, amber for a fallback.

Cost: one ~27 MB file on disk when needed; one CPU inference (tens of ms) per
recording only when a `Detect` plan exists; no thread, no poll.

### 6.6 UI (Solid)

- **Languages I speak**: searchable multi-select with chips (port of upstream
  `PreferredLanguages.tsx` after `693aa63` + `shared/LanguagePicker.tsx`; options
  from `MODEL_CAPABILITY_LANGUAGES`), shown in the General model card under the
  language selector; "Allow foreign words" toggle beside it.
- **Language selector** shows what Auto will do for the selected model
  (upstream `status_for`: "Auto (French, English)", undetectable languages,
  detector not downloaded → download button).
- **Multi-STT page**: each slot's language control shows the slot's effective
  plan.
- **Tray Language submenu** (`3d94031`, #1319): spoken languages first, the
  model's others under "Other languages", Auto greyed when it cannot work;
  skip the detector lookup while dictating (upstream's perf fix).
- **Onboarding** (part of `0e5e9ca`): a languages step suggested from the
  system languages (port `system_languages.rs`), writing `spoken_languages`.
- Strings in all 26 locales.

### 6.7 Chinese script (D6)

Port `chinese_script.rs` (`0bb0428`) with the text-based rule (`c67ec6a`) and
the ja/ko protection (`b5920f2`); the live preview uses the same rule as the
paste. Migration: `selected_language` `zh-Hant` / `zh-Hans` → `zh` +
`chinese_script`; default from the OS locale for fresh installs. ZER0's current
`ferrous-opencc` conversion in `actions.rs` moves into the module.

### 6.8 Verify

Unit tests (policy port + run-plan tests above). Real dictation: two-language
list with short 1–2 s snippets (the case the allowlist fixes), a mixed sentence,
a Canary extra with fr+en (detector path), streaming Nemotron with the list,
"allow foreign words" with a foreign name, tray switching.

---

## Phase 7 — Upstream batch U3: transcription and LLM pipeline

| Upstream                          | Change in ZER0                                                                                                                                                                                                                                                                                                                             |
| --------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `4150a4a` #2284 + `1e10e71` #2506 | Custom words through `RunOptions::vocabulary` when the model has `Feature::Vocabulary` (Qwen3, FunASR-Nano, Granite, MOSS; Whisper keeps working), replacing the Whisper-only initial prompt; skip fuzzy post-correction exactly when vocabulary was sent; treat a result that is just the word list read back as empty. All five builders |
| `14ae9c6` #2257                   | `llm_client.rs`: `api_format` (OpenAI / Anthropic Messages) on the Custom provider, built-in Anthropic routed through Messages, `finish_reason: length` / `max_tokens` → error so the fallbacks run. Applies to post-processing **and** the Multi-STT merge                                                                                |
| `0ed36e1` #2344 + `27322df`       | One shared `reqwest::Client` (`OnceLock`, headers per request, idle/connect timeouts), model list 15 s — ZER0 still builds a client per call (`create_client`)                                                                                                                                                                             |
| `889fca0` #2290                   | Blank post-processing output keeps the dictation; a blank Multi-STT merge keeps the primary's text                                                                                                                                                                                                                                         |
| `0911f2b`                         | One deadline around the whole post-processing step (D5)                                                                                                                                                                                                                                                                                    |
| `5a9451e` #2302                   | Paste off the main thread; a finishing paste must not reset a newer session's overlay/tray (check `paste_tx` and the streaming-first `owns_typing` path)                                                                                                                                                                                   |
| `033c627` #930                    | Transcription hook (D4), new module `transcription_hook.rs`; README section rewritten for ZER0                                                                                                                                                                                                                                             |
| `93c9ee0` #2299 (part)            | Model deletion: keep other quants of the same repo, match cached repos by exact name, harden deletion on Windows, report failures (D7: no catalog package)                                                                                                                                                                                 |
| `2eab8fc` #2485                   | In-process recovery: a `Backend` error from run / feed / finalize drops that engine and reloads it once on its backend, then on CPU if it fails again — per model (primary and each extra), honouring `per_model_backends`; reported through a notice after Phase 8                                                                        |

---

## Phase 8 — Upstream batch U4: notices

`b99b9af` (#2372) adapted, not copied: ZER0 keeps `sessionToast` for transient
messages (it never had sonner) and gains **backend-owned notices** for failures
that must stay visible until resolved or dismissed.

1. `src-tauri/src/notices.rs`: notice codes, sources, `raise` / `clear_matching`,
   one event; tray badge and tooltip; overlay notice line; footer chip + panel
   (`NoticePanel.tsx`, `noticeStore.ts`); dismissal matches code **and** source.
2. Notices to raise: model load failed, post-processing failed/skipped,
   Multi-STT merge failed, file transcription failed, **No model selected**
   (`d654302` #2418), **microphone sends only silence** (`70f490e` #2486 — needs
   a recorder flag "every sample was zero", raised when such a take produces
   nothing; the onboarding check of `0e5e9ca` reuses it), **paste blocked by an
   elevated window** (`e529447` #2502 — Windows integrity-level check before
   paste and before `DirectStreaming` typing; paste runs as configured, only the
   report changes), llama server failed to start.
3. Keep the Debug page's `SessionToastHistory`; notices are listed there too.

Cost: no thread, no poll; one event per notice change.

---

## Phase 9 — Upstream batch U5: files

| Upstream                               | Change in ZER0                                                                                                                                                                                                                                                                                                                                                                                                                  |
| -------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `73c0958` #2315 + `815e73f`            | Transcribe Files page: optional "also save to History" per file (audio written before its History row; title = file name; no retention cleanup of imports); dropping audio on the main window routes to the Transcribe Files page with a visible "decoding" state; job panic recovery                                                                                                                                           |
| `1fe3ae2` #2482 + `2696c49`, `1472087` | Opus (`.opus`, `.ogg` Opus, `.webm`) via libopus: ZER0 is on symphonia **0.6**, upstream's `symphonia-adapter-libopus` 0.2 targets 0.5 — pick the adapter release for 0.6 or decode Opus directly with `opusic-sys`; Windows ARM64 `no-runtime-feature-detection`; add the extensions to `SUPPORTED_EXTENSIONS` and the frontend list; upstream's two fixtures as tests. Cost: libopus built with cmake (new native dependency) |
| `9e57a0d` #2282                        | Deferred (Phase 12): long-form VAD chunking for batch dictation                                                                                                                                                                                                                                                                                                                                                                 |

---

## Phase 10 — Platform and docs batches

Compile-checked here; runtime unverified (no Linux/macOS machine). Say so in
each commit.

**Linux**: `f3d8b87` #2360 (supersedes `48a3d8e` #2249) WebKitGTK renderer
policy (`webkit_renderer.rs`, env with `ENV_PREFIX`); `bd87c72` #2453
`ZER0_GDK_BACKEND` for AppImage (legacy `HANDY_` fallback); `310749b` #2454
`DISABLE_GAMESCOPE_WSI`; `e286103` #2448 updater off where the bundle cannot
self-update; `c530a49` #2447 resizable X11 overlay, `d2ee871` #2488 no focus on
map; `ef3086e` #1300 `KeyboardImplementation::None` (Linux-only option,
`should_register_binding` honours it); `4ec61f0` #2199 dotool multi-line;
`3c7a538` #2427 ydotool socket lookup (owner-checked); `bd4b027` #2495 next
paste tool on failure; `4a096e3` #2504 Shift+Insert + PRIMARY selection
(`primary_selection.rs`, new-install default and longer restore delay,
platform-specific What's New); `2b3f7c8` #2336 Debian OpenSSL dependency;
`298653c` #2367 Linux `default_binding` avoids the ibus hotkey (ZER0's
`current_binding` is empty, so only "reset" changes).

**macOS**: `7bfb2e2` #2340 Metal off on Intel Macs (target table);
`5197faf` (part) link `clang_rt.osx` now that ORT no longer supplies it (check
ZER0's `build.rs`); `dce2976` #2057 `tccutil` reset before asking again.

**Docs**: `73ab851`, `586a838`, `afe5a63`, `1607381` Ubuntu 26.04 GNOME/Wayland
troubleshooting (rewritten for ZER0's backends and names); `f6b3f82`
CONTRIBUTING_TRANSLATIONS alignment; `7102b63` OpenBLAS in BUILD.md only if
ZER0's Linux build links BLAS; `340defb`, `5ec58f6` PR template wording if it
applies to ZER0's template.

---

## Phase 11 — Close-out

1. `rtk git fetch upstream` in this repo (the local `upstream/main` still points
   at `8f9cf53c`; the newer commits are only in `../STT_BRAIN_TTS/Handy`).
2. Record the intake: `git merge -s ours aca89851` with the ledger summary in
   the message ("content integrated in …; skipped …"), as `596eb942` did. The
   merge records ancestry only — every row of the ledger must have a
   disposition first.
3. Update `AGENTS.md` (VAD, noise suppression, languages, notices, Live Canvas,
   typography, Technology Stack without `earshot` / `nnnoiseless`), `README.md`
   (languages, hook, Opus), `docs/KNOWN_ISSUES.md` (D2/D3 decisions, platform
   items unverified), `CHANGELOG.md`, release notes
   (`src/content/release-notes/`), the paired notes, and delete or archive the
   September VAD plan.
4. `bun run precommit:routine` (dependencies to newest prerelease + full gate),
   `meta:bump minor`.

---

## Phase 12 — Deferred, recorded so it is not lost

- `9e57a0d` long-form chunking with VAD for long batch dictations, on the
  engine's `score_frames` + ZER0's quiet-point cutter (`file_transcription::
segment_boundaries`), one policy for live and batch.
- Engine supervisor / process isolation (D2) as an ADR, if crash isolation
  becomes a goal without giving up Multi-STT.
- `handy-recorder` (D3) and AirPods mode (`8c293dd`, `d628da4`).
- Upstream's full onboarding (`0e5e9ca`) and model table / grades UI.
- Engine §A.3 (LangID attached to an ASR session in the engine) would replace
  §6.5's app-side detector; §B.1/B.2 (Silero as an alternative VAD role,
  FireRed) once measured on ZER0 dictation data.
- `docs/SEARCH_ARCHITECTURE.md` (proposal).

---

## Ledger — every upstream commit `8f9cf53c..aca89851`

Disposition: **Port** (near as-is), **Adapt** (re-implemented for ZER0 / Solid),
**Phase N** (absorbed by that phase), **Covered** (ZER0 already has it or the
update routine brings it), **Verify** (probably n/a, check), **Skip** (with
reason), **Defer** (Phase 12). Fill the last column with the ZER0 commit.

| #   | Upstream  | PR    | Title                                                    | Disposition                                                                      | ZER0                                                    |
| --- | --------- | ----- | -------------------------------------------------------- | -------------------------------------------------------------------------------- | ------------------------------------------------------- |
| 1   | `8ef8dd4` | #2156 | stop removing "Ha" as an English filler                  | Port, P3                                                                         | `23561e8e`                                              |
| 2   | `2d526b1` | #2147 | transcribe-cpp 0.2.4                                     | Covered, P2                                                                      | `eeb2e9a5`                                              |
| 3   | `eb49dc0` | #2157 | keep the sentence capital after a leading filler         | Port, P3                                                                         | `23561e8e`                                              |
| 4   | `eea1f5f` | #2161 | don't record when no model can transcribe                | Adapt, P3                                                                        | `47fe8c57`                                              |
| 5   | `f5c27e6` | #2158 | reject shortcuts the Tauri backend cannot parse          | Port, P3                                                                         | `5a9659b9`                                              |
| 6   | `29bd2c0` | #2160 | list compute devices off the startup path                | Adapt, P3                                                                        | `175806e9`                                              |
| 7   | `340defb` |       | PR template                                              | Adapt, P10                                                                       |                                                         |
| 8   | `5ec58f6` |       | PR template                                              | Adapt, P10                                                                       |                                                         |
| 9   | `0bb0428` | #2186 | Chinese script selector                                  | Phase 6.7                                                                        | `15a39e2e`                                              |
| 10  | `b38987a` | #2190 | cancel shortcut stays registered after a short recording | Adapt, P3                                                                        | `5a9659b9`                                              |
| 11  | `14f6f0d` |       | release v0.9.8                                           | Skip (ZER0 versions via app-meta)                                                |                                                         |
| 12  | `8605699` | #2117 | NSIS installer icon                                      | Port, P3                                                                         | `175806e9` (staged by the gate with it)                 |
| 13  | `345caa8` | #1941 | refresh custom sounds when opening the picker            | Adapt, P4                                                                        | `b0b9c2fb`                                              |
| 14  | `ffbc950` | #2203 | transcribe 0.3.0                                         | Covered, P2                                                                      | `eeb2e9a5`                                              |
| 15  | `73ab851` | #2206 | Ubuntu 26.04 GNOME Wayland guide                         | Adapt, P10                                                                       |                                                         |
| 16  | `586a838` | #2212 | Ubuntu guide: overlay, wl-clipboard, non-QWERTY          | Adapt, P10                                                                       |                                                         |
| 17  | `a94b403` | #2116 | portable: legacy model cache                             | Adapt, P3                                                                        | `3d125dfd`                                              |
| 18  | `c8262ab` | #2208 | transcribe.cpp in its own worker process                 | Skip (D2); ideas in P7                                                           |                                                         |
| 19  | `20a6685` | #2227 | show "loading model" instead of "Transcribing…"          | Adapt, P4                                                                        | `b2f51e6f`                                              |
| 20  | `417dc6a` |       | loading notice in the Live preview                       | Adapt, P4                                                                        | `b2f51e6f`                                              |
| 21  | `afe5a63` |       | format Ubuntu notes                                      | Adapt, P10                                                                       |                                                         |
| 22  | `f6b3f82` | #2235 | translation guide vs supported locales                   | Adapt, P10                                                                       |                                                         |
| 23  | `48a3d8e` | #2249 | Linux renderer opt-out                                   | Adapt, P10 (final form = row 68)                                                 |                                                         |
| 24  | `1b011a1` | #2265 | transcribe 0.4.0                                         | Covered, P2                                                                      | `eeb2e9a5`                                              |
| 25  | `a42131d` | #2188 | move to handy-recorder                                   | Skip (D3)                                                                        |                                                         |
| 26  | `3985508` | #2267 | Silero VAD through transcribe.cpp                        | Phase 5 (engine Earshot instead)                                                 | `a12ca902`                                              |
| 27  | `7b2a28a` |       | don't re-emit hangover frames in pre-roll                | Phase 5 (VadGate never releases twice)                                           | `a12ca902`                                              |
| 28  | `14ae9c6` | #2257 | Anthropic Messages API format                            | Adapt, P7                                                                        |                                                         |
| 29  | `5732406` | #2256 | no duplicate History rows during live updates            | Adapt, P4                                                                        | `2e9e849c`                                              |
| 30  | `53db4cb` | #2238 | setting names for screen readers                         | Adapt, P4                                                                        | `b0b9c2fb`                                              |
| 31  | `cfcb650` | #2277 | ONNX → GGUF migration UI                                 | Skip (schema 6 remap since 2026-09-10)                                           |                                                         |
| 32  | `5197faf` | #2279 | remove ONNX                                              | Covered; `clang_rt.osx` link P10                                                 |                                                         |
| 33  | `4150a4a` | #2284 | custom words via the vocabulary API                      | Adapt, P7                                                                        |                                                         |
| 34  | `9e57a0d` | #2282 | long-form chunking with VAD                              | Defer                                                                            |                                                         |
| 35  | `93c9ee0` | #2299 | catalog from @handy-computer/models                      | Skip package (D7); deletion fixes P7                                             |                                                         |
| 36  | `5a9451e` | #2302 | paste off the main thread                                | Adapt, P7                                                                        |                                                         |
| 37  | `0e5e9ca` | #2303 | new onboarding                                           | Partial: P6.6 languages, P8 silent mic; rest Defer                               |                                                         |
| 38  | `f81e7e7` | #2326 | centre grade chip labels                                 | Skip (no GradeChip)                                                              |                                                         |
| 39  | `73c0958` | #2315 | transcribe files into History                            | Adapt, P9                                                                        |                                                         |
| 40  | `126bed7` | #2331 | language detection                                       | Phase 6                                                                          | `375de7b9`, `6efcf51b`, `6fced8c0`                      |
| 41  | `2b3f7c8` | #2336 | Debian OpenSSL 3 dependency                              | Port, P10                                                                        |                                                         |
| 42  | `7bfb2e2` | #2340 | Metal off on Intel macOS                                 | Adapt, P10                                                                       |                                                         |
| 43  | `0ed36e1` | #2344 | one HTTP client for post-processing                      | Port, P7                                                                         |                                                         |
| 44  | `1607381` | #2348 | stale user manager and the input group                   | Adapt, P10                                                                       |                                                         |
| 45  | `889fca0` | #2290 | keep dictation when post-processing is blank             | Port, P7                                                                         |                                                         |
| 46  | `a537ec4` | #2253 | reset buttons name their setting                         | Adapt, P4                                                                        | `2e9e849c`                                              |
| 47  | `298653c` | #2367 | Linux defaults avoid the ibus hotkey                     | Adapt, P10                                                                       |                                                         |
| 48  | `8fd85aa` | #2246 | Escape dismisses dropdowns                               | Adapt, P4                                                                        | `b0b9c2fb`                                              |
| 49  | `e4869fc` |       | cargo fmt                                                | Skip                                                                             |                                                         |
| 50  | `4c811d7` |       | Playwright 1.64.1                                        | Covered (update routine)                                                         | `0759c8a4`                                              |
| 51  | `b99b9af` | #2372 | error notices framework                                  | Adapt, P8                                                                        |                                                         |
| 52  | `090b370` | #2248 | ignore IME confirmation Enter                            | Adapt, P4                                                                        | `b0b9c2fb`                                              |
| 53  | `0685192` | #2255 | report failed History deletions                          | Adapt, P4                                                                        | already reported (verified, `2e9e849c` msg)             |
| 54  | `77ad955` | #2254 | name the playback position control                       | Adapt, P4                                                                        | `2e9e849c`                                              |
| 55  | `26d2003` | #2247 | keep playback position when a seek starts                | Adapt, P4                                                                        | `2e9e849c`                                              |
| 56  | `3b60fc7` | #2251 | Escape dismisses focused tooltips                        | Adapt, P4                                                                        | `b0b9c2fb`                                              |
| 57  | `a1cb29d` | #2309 | release late audio sources after leaving History         | Adapt, P4                                                                        | `2e9e849c`                                              |
| 58  | `8020c87` | #2327 | shortcut fallback when the manager fails to start        | Covered (readiness channel + fallback)                                           | —                                                       |
| 59  | `8c293dd` | #1991 | macOS AirPods mode                                       | Defer (needs handy-recorder)                                                     |                                                         |
| 60  | `033c627` | #930  | transcription hook                                       | Adapt, P7 (D4)                                                                   |                                                         |
| 61  | `ef3086e` | #1300 | Linux "None" keyboard implementation                     | Adapt, P10                                                                       |                                                         |
| 62  | `4ec61f0` | #2199 | dotool multi-line typing                                 | Port, P10                                                                        |                                                         |
| 63  | `693aa63` |       | better preferred languages                               | Phase 6.6                                                                        | `375de7b9` (picker; system-language suggestion pending) |
| 64  | `815e73f` |       | state update while decoding a file                       | Adapt, P9                                                                        |                                                         |
| 65  | `1b13488` |       | merge                                                    | n/a                                                                              |                                                         |
| 66  | `3d94031` | #1319 | tray language submenu                                    | Phase 6.6                                                                        | `59e6cf4c`                                              |
| 67  | `4fd7ac4` |       | 0.10.0 release notes                                     | Skip (ZER0's own notes)                                                          |                                                         |
| 68  | `f3d8b87` | #2360 | WebKitGTK 2.54 transparent overlay                       | Adapt, P10                                                                       |                                                         |
| 69  | `69420ed` |       | release v0.10.0                                          | Skip                                                                             |                                                         |
| 70  | `82dfab9` |       | merge                                                    | n/a                                                                              |                                                         |
| 71  | `73cc857` | #2432 | TLS security advisories                                  | Covered (update + `cargo audit`), P3                                             | `0759c8a4`                                              |
| 72  | `40b8e72` | #2403 | no WebView2 context menu on the overlay                  | Port, P4                                                                         | `b0b9c2fb`                                              |
| 73  | `88185c8` | #2478 | async runtime before webview log streaming               | n/a: no webview log target (stdout/stderr/file only)                             | —                                                       |
| 74  | `0911f2b` |       | 60 s post-processing deadline                            | Adapt, P7 (D5)                                                                   |                                                         |
| 75  | `bd87c72` | #2453 | AppImage native Wayland opt-in                           | Adapt, P10                                                                       |                                                         |
| 76  | `e286103` | #2448 | updater off where the build cannot update                | Adapt, P10                                                                       |                                                         |
| 77  | `c530a49` | #2447 | X11 overlay stays resizable                              | Adapt, P10                                                                       |                                                         |
| 78  | `27322df` |       | model list request timeout                               | Port, P7                                                                         |                                                         |
| 79  | `dce2976` | #2057 | macOS: clear stale permission grants                     | Port, P10                                                                        |                                                         |
| 80  | `1fe3ae2` | #2482 | decode Opus files                                        | Adapt, P9                                                                        |                                                         |
| 81  | `ca4a611` | #2434 | punctuation-only transcript is empty                     | Port, P3                                                                         | `b1a98752`                                              |
| 82  | `d654302` | #2418 | No Model Selected notice                                 | Adapt, P8                                                                        |                                                         |
| 83  | `7102b63` | #2484 | OpenBLAS in Linux build docs                             | Verify, P10                                                                      |                                                         |
| 84  | `d628da4` |       | bump handy-recorder                                      | Skip (D3)                                                                        |                                                         |
| 85  | `c67ec6a` |       | Chinese script from the text                             | Phase 6.7                                                                        | `15a39e2e`                                              |
| 86  | `b5920f2` |       | leave Japanese and Korean alone                          | Phase 6.7                                                                        | `15a39e2e`                                              |
| 87  | `56c7e21` | #2097 | tap vs hold by key event time                            | Adapt, P3                                                                        | `06e647f7`                                              |
| 88  | `2eab8fc` | #2485 | replace the worker after a backend error                 | Adapt in-process, P7                                                             |                                                         |
| 89  | `70f490e` | #2486 | say so when the mic sends only silence                   | Adapt, P8                                                                        |                                                         |
| 90  | `f248301` | #2318 | hf-hub survives Range-stripping proxies                  | n/a: hf-hub 1.x never ranges a file, ZER0's downloader already restarts on a 200 | —                                                       |
| 91  | `0ff1ce1` | #2489 | bump transcribe.cpp                                      | Covered, P2                                                                      | `eeb2e9a5`                                              |
| 92  | `2696c49` |       | libopus on Windows ARM                                   | Adapt, P9                                                                        |                                                         |
| 93  | `1472087` |       | libopus without SIMD on Windows ARM                      | Adapt, P9                                                                        |                                                         |
| 94  | `310749b` | #2454 | skip the Gamescope WSI Vulkan layer                      | Adapt, P10                                                                       |                                                         |
| 95  | `3c7a538` | #2427 | find the ydotoold socket                                 | Port, P10                                                                        |                                                         |
| 96  | `bd4b027` | #2495 | next paste tool when one fails                           | Adapt, P10                                                                       |                                                         |
| 97  | `e529447` | #2502 | warn about elevated windows                              | Adapt, P8                                                                        |                                                         |
| 98  | `d2ee871` | #2488 | X11 overlay does not steal focus                         | Port, P10                                                                        |                                                         |
| 99  | `f01b807` | #2505 | handy-keys 0.3.5                                         | Covered, P3                                                                      | `0759c8a4`                                              |
| 100 | `1e10e71` | #2506 | drop custom words read back                              | Port, P7                                                                         |                                                         |
| 101 | `4a096e3` | #2504 | Linux Shift+Insert paste                                 | Adapt, P10                                                                       |                                                         |
| 102 | `aca8985` |       | release v0.10.1                                          | Skip                                                                             |                                                         |
