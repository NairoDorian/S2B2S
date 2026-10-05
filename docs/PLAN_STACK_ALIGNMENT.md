# Plan: Stack alignment — improvements grounded in the mirrored upstream docs

_Status: proposed 2026-09-14. **Still a live plan**, but several Phase 0 and 1
items have since landed by other routes; the "landed" markers below are the only
things that changed in this file, and the bodies are otherwise as written on
2026-09-14. Source of every claim: the `docs/vendor/` mirror
(`bun run docs:fetch`, refreshed 2026-09-14) plus per-tech audits that verified
each finding against this working tree. Items carry the doc page and the app
file that ground them. When a phase lands, delete its items; when the plan is
done, delete this file (the outcomes live in CHANGELOG.md)._

_The stack itself audited the app through six parallel agents (Solid 2, Tauri
2, Bun, Tailwind 4 + Oxlint, transcribe.cpp, tauri-specta). 55 findings, 55
of them verified in code. This plan orders them by user-visible value and
risk, under this project's own rules: real-time budget (docs/PERFORMANCE.md),
gates green before anything ships, no version pinning of dependencies._

_Read the "Tauri 2" items against that word, not the present tense._ The app is
on Tauri **3** alpha (`tauri 3.0.0-alpha.2`) and Solid **2**; the Tauri findings
below were audited against the mirrored **Tauri 2** docs, so re-check any of
them you intend to act on against `docs/vendor/tauri/`'s current major before
trusting the page reference. `docs/vendor/` is refreshed by `bun run docs:fetch`,
so re-run it rather than trusting the 2026-09-14 snapshot.

---

## Phase 0 — correctness and security fixes (high severity, low risk)

**Solid 2 — three user-visible reactivity bugs** (docs/vendor/solidjs/
concepts/reactivity.md, components-and-jsx.md):

1. ~~`AccessibilityPermissions.tsx` reads signals in the once-run body~~ **Landed.**
   `<Show when={isMacOS && !hasAccessibility()}>` and an accessor-based
   `buttonConfig()` are what the file does now.
2. ~~`useQuantBenchmark.ts` — `isBusy` is a mount-time boolean~~ **Landed.**
   `isBusy` is declared `Accessor<boolean>` and read as `isBusy()` in the guards
   and the JSX.
3. ~~`QuantizationPanel.tsx` destructures live props while mounted~~ **Landed.**
   The panel reads `props.*` at every use site, with `downloadedIds` and the
   family results as memos.

**Tauri 2 — security hardening** (docs/vendor/tauri/security/csp.md,
asset-protocol.md, capabilities.md; plugin/store.md):

7. ~~Updater contradiction~~ **Resolved 2026-09-14, sign path:** the project's
   own key pair was generated and installed (private `~/.tauri/zer0.key`,
   public `plugins.updater.pubkey`), `createUpdaterArtifacts: true`, CI
   secrets wired (see Open decisions below).

**Tailwind 4** (docs/vendor/tailwind/dark-mode.md, upgrade-guide.md):

9. `dark:` variants in `LiveLogViewer`/`QuantizationPanel` follow
   `prefers-color-scheme`, not the app's `data-theme` — forced themes break
   them. Fix: `@custom-variant dark (&:where([data-theme="dark"], …))` +
   three-way system resolution in `applyTheme`.
   **Still open** — no `@custom-variant dark` exists, and the mechanism the app
   actually uses for its own tokens is explicit `:root[data-theme="…"]`
   overrides in `styles/theme.css`, which is a different answer than the one
   proposed here.
10. ~~`flex-grow` / `flex-shrink-0`~~ **Landed.** `Slider.tsx` uses `grow`,
    `VadMeter.tsx` uses `grow` / `shrink-0` throughout. (`TextDisplay.tsx`,
    the third site, was deleted as dead code.)

**Oxlint** (docs/vendor/oxlint/guide/usage/linter/config.md):

11. Lint runs on defaults: correctness is warn-severity and nothing fails the
    gate. Add `categories` (`correctness: error`, `suspicious: warn`,
    `perf: warn`) + `options.maxWarnings: 0`, then clear the three existing
    unused-import warnings. Add the native `jsx-a11y` plugin (re-listing the
    default set; no `react` plugin — this is Solid).
    **Partly landed**: `.oxlintrc.json` carries those three `categories` and the
    `jsx-a11y` plugin. `options.maxWarnings: 0` is still absent, and
    `oxlint`'s own `maxWarnings` is not a config key the way the ESLint
    equivalent was — the gate is `bun run lint` in `precommit`, which treats a
    non-zero exit as failure.

**Bun** (docs/vendor/bun/test/\*, pm/cli/install.md):

13. Add `bun install --frozen-lockfile --dry-run` as a gate step ("lockfile in
    sync") so package.json↔bun.lock drift fails locally instead of on the
    7-platform CI matrix. **Still open as a gate** — the three JS workflows do
    install with `--frozen-lockfile`, so drift fails on CI, but nothing runs it
    locally before a commit.

## Phase 1 — architecture and the native pipeline (high value, medium effort)

15. **Overlay scope: poll → push channel** (calling-rust.md streaming
    example). `Channel<&[u8]>` from a subscribe command deletes the 16 ms
    poll loop, the in-flight guard, and the bespoke `generate_handler!`
    bypass.
16. **Partial transcripts are dropped on truncation/abort** (transcribe-cpp/
    input-limits.md: "the partial output is never discarded"; streaming
    callers should check `was_truncated()`). `transcription.rs` stringifies
    crate errors and discards `Error::OutputTruncated.partial`; the stream
    path never checks truncation. Recover partials (flagged), fall back to
    windowed chunking on `INPUT_TOO_LONG`, warn on stream truncation. This
    is the transcribe.cpp audit's top finding.
17. **Keep one stream alive across Live Mode chunk rotations** (input-limits
    bucket 1: whisper/parakeet/voxtral_realtime are unbounded streaming
    encoders; nemotron carries decoder caches across calls). Today every
    rotation finalizes and re-begins, so each chunk starts cold. Rotate the
    recorder/WAV only; finalize once at session stop — gated per model caps.
18. **Use the family's own `buffered_ms` in the Multi-STT close gate**
    (StreamUpdate::buffered_ms vs the derived drain hint). The worker already
    receives it and logs it; thread it into the sink and let `break_outcome`
    use it, keeping the 500 ms tolerance as fallback.
19. **settingsStore: `reconcile` instead of wholesale replacement**
    (solidjs/concepts/stores.md, server-response pattern). Every
    `settings-changed` event replaces the whole `settings` node, re-running
    ~100 components' bindings.
20. **tauri-specta: generate the hand-typed events** (README: event type
    generation). Three payloads (`ModelStateEvent`, `BenchmarkProgressEvent`,
    `RecordingErrorEvent`) are hand-maintained twice; derive
    `specta::Type` + `tauri_specta::Event`, register, rename structs so the
    generated kebab-case matches the current wire names, delete the
    events.ts copies. Phase 2: the wider ~15-event raw `listen` surface.
    Then introduce `thiserror` + `specta::Type` error enums for the
    models/llama/audio command groups (92 `Result<_, String>` signatures).

## Phase 2 — modernization sweeps (mechanical, low risk)

**Solid 2** (migration/from-solid-1.md): 21. Codemod ~51 `createEffect(() => undefined, …)` mount-effects →
`onSettled`; 10 effect-local `onCleanup`s → returned cleanup. 22. Props-destructure sweep (`WhatsNewModal`, `StreamingLatencyControl`,
`QuantizationPanel`, `ModelDropdown`, `HotkeyGroup`, `ModelSelector.onError`,
`WhatsNewPreview`, static-config selectors), with `untrack` annotations
for intentional snapshots (`OverlayScope.tsx:580`). 23. Wrap `renderSettingsContent` per-section (and footer) in the existing
`ErrorBoundary` with a visible fallback — today any settings-page throw
blanks the window. 24. Pilot conversions: one effect-fetch → async memo under `Loading`;
`updateBinding` → `action` + optimistic primitives (pattern-setters; no
wholesale store rewrite).

> **Correction to item 22's file list.** The audit named `LatencyPanel`, which
> does not exist in this tree. The R2T2 / preset latency control is
> `src/components/model-selector/StreamingLatencyControl.tsx`, and
> `QuantizationPanel.tsx` was already converted under Phase 0 item 3 — the two
> are named above because both are still live sweep targets.

**Tailwind 4 / Oxlint**: 25. Migrate semantic status colors to the registered `--color-warning` /
`--color-error` tokens (+ add `--color-success`); ~60 occurrences, per
theme.css's own TODO. 26. Register `--color-log-surface` (and `--color-hairline`) in `@theme
    inline`; drop the one arbitrary-value utility. 27. Container queries (`@container` + `@sm:`) for the settings panes — every
`sm:` variant in them is permanently active (window min-width 1060 px),
so the responsiveness they express is fiction. 28. Delete dead CSS (`.container`, `.text-stroke`, `--color-text-stroke`);
keyframes → `--animate-*` token; define the stroke via `@utility` if
ever needed again.

**Bun / tooling**: 29. Run Vite under Bun: `"dev": "bunx --bun vite"`, `"vite:build": "bunx --bun
    vite build"` (guides/ecosystem/vite.md) — removes the Node-version
variable; gate verifies. 30. `AbortSignal.timeout` in `fetch-stack-docs.ts` `fetchText` (a hung
connection currently stalls the mirror run forever) and in
update-deps.ts queries; replace the batch deadline race. 31. CI: cache `~/.bun/install/cache` in the three JS workflows (bunfig
advertises the isolated-linker win; nothing warms it). 32. Standardize the node:child_process scripts on `Bun.spawnSync` / `$`;
drop the win32 `shell:` toggles. Adopt `Bun.file().json()`, `Bun.write`,
`Bun.Glob` opportunistically. Route update-deps' stable-mode NPM path
through `bun outdated`. 35. Single-instance plugin first in the builder chain (plugin docs: "must be
the first one to be registered"). 36. Tooling: `fetch-stack-docs.ts` INDEX titles — extract each page's first
heading instead of falling back to the repo name.

**transcribe.cpp integration**: 37. Latency presets for Voxtral Realtime (`num_delay_tokens`) and Moonshine
Streaming (`min_decode_interval_ms`) — both documented and typed in the
Rust API; the app's preset UI then covers every streaming family. **Partly
done**: `nemotron-speech-streaming-en-0.6b` was since given its own
`NemotronSpeechCacheAware` kind, so the catalog now has two Nemotron menus
rather than one; Voxtral and the three Moonshine Streaming entries still resolve
to no kind. 38. Re-anchor ParakeetBuffered presets to the fork's validated menu
(chunk ≥ 500 ms; Fastest=500/500, Fast=500/1040, Balanced=1040/1040),
measuring WER first via the fork's `scripts/wer/run.py` recipe before
shipping any sub-500 ms tier. **The numbers here are no longer the ones in the
tree** — `parakeet_buffered_chunk_right_ms` ships
Fastest=160/160, Fast=160/320, Balanced=560/560, Accurate=1040/1040, i.e. the
menu went the _other_ way, to sub-500 ms chunks. Whatever the WER run said is
not recorded in this file; treat the re-anchoring as done-by-another-route and
re-read `managers/native_streaming_latency.rs` before acting on it. 39. Session
thread budgeting for Multi-STT on CPU:
`SessionOptions { n_threads: max(2, cpus / active_engine_count) }` per
the integration guide — four concurrent engines currently each spawn
`hardware_concurrency()` threads. 40. `kv_type: F16` on GPU-constrained devices; optional bounded `n_ctx` for
hard-cap families in file transcription (pairs with 16). 41. Debug hooks behind debug*mode: `TRANSCRIBE_PERF_DEBUG=1` in
`init_transcribe_backend`; guarded dump-dir surface; startup warn on
`TRANSCRIBE_TEST*\*` fault injectors. 42. Per-quant accuracy metadata (fork model cards publish per-quant WER; the
app shows one family-level score — Q4_K_M Nemotron + streaming is the
documented outlier) carried through the catalog into the quant chips.

## Open decisions for the maintainer

- **Updater posture (7): resolved 2026-09-14 — sign artifacts.** The signing
  infrastructure already existed in CI (`sign-binaries` input wiring
  `TAURI_SIGNING_PRIVATE_KEY` into tauri-action; both release workflows pass
  it). A key pair was generated (`~/.tauri/zer0.key`), the public key
  installed in `tauri.conf.json`, `createUpdaterArtifacts` turned on, and the
  in-app `downloadAndInstall()` flow kept. Local builds sign via the runner
  reading `~/.tauri/zer0.key`. The GitHub secret was still unset then; it was
  created on 2026-09-22 as part of a key rotation (the private half had to be
  regenerated — see §5a of
  `2026-09-build-lanes-per-model-backends-and-multi-streaming.md`), so the
  caveat is closed.
- **transcribe.cpp floating `main` (part of 16's context):** the standing
  policy is no pins — keep `branch = "main"` and the guard script. Instead of
  pinning, add a post-bump smoke check that the Fastest preset
  (`att_right=0`) still streams on CUDA before committing the bump (the
  guard script's report is the natural hook).
- **Live Mode single-stream (17):** changes archival semantics (text split
  after the fact). Prototype behind the existing model-capability gates and
  compare boundary quality before switching the default.

## What the docs validated as already right (do not "fix")

Binary `tauri::ipc::Response` for the overlay scope (the documented
ArrayBuffer pattern); `emit_to` targeting; module-scope `createRoot` stores;
60 Hz frames bypassing the reactive graph (`liveFftStore`); `Dynamic`/
`Errored`/`Translation` in v2 forms; no `solid-js/web` 1.x imports; zero
`solid-js/web`/`unwrap`/`Index` legacy APIs; device-selection and language-
filter contracts in the transcribe.cpp integration; frozen-lockfile CI
installs including `--cpu=arm64`.

## Execution order and verification

Phase 0 first (each item independently gated by `bun run precommit`).
Phase 1 items 16-18 verified with real recordings via the CDP harnesses
(`bun tests/tauri-window.ts`, `bun tests/overlay-window.ts`) and the VAD
probe; 38's WER measured before and after. Phase 2 lands as sweeps with a
typecheck checkpoint per batch, warnings-counted before/after where relevant
(STRICT_READ_UNTRACKED residual, oxlint warnings to zero). Every landed
phase updates CHANGELOG.md and prunes its items here; the plan file is
deleted when the last item lands.
