# Build lanes, per-model backends, and multi-streaming STT

Written 2026-09-21, from a study of the four build commands, the multi-STT
panel, the engine's backend API and the streaming code path. It records what
each lane actually does, what is going to change, and the two new features:
per-model backend selection and a second experimental streaming mode that runs
two or more streaming models side by side, merges their live texts at every
pause, and pastes the merged text as the result.

The directives behind part 4 — quoted verbatim, each mapped to the code that
answers it — are in **`2026-09-multi-streaming-stt-directives.md`**. This note
keeps the design; that one keeps the requirements.

## 0. Status

All four parts are implemented. Where the implementation departs from what this
note first proposed, §4.2 says so and gives the reason; it is the one substantial
deviation, and it is flagged in place rather than left for the reader to infer
from the code.

| Part                                      | State                                             | Where                                            |
| ----------------------------------------- | ------------------------------------------------- | ------------------------------------------------ |
| 1. Lanes, always-full model set, `cpu`    | done                                              | §1.1–§1.3, `scripts/tauri-runner.ts`             |
| 2. Per-model backend selection            | done, Rust + UI                                   | §2.3, `managers/*`, `components/model-selector/` |
| 3. Rename + nested toggle                 | done                                              | §3, `components/settings/multi-stt/`             |
| 4. Multi streaming STT                    | done: merge-at-pauses, both views, merged result  | §4.2–§4.4, `multi_streaming.rs`                  |
| Directives for part 4, verbatim           | recorded, each mapped to its code                 | `2026-09-multi-streaming-stt-directives.md`      |
| 4a. First real run through the debug view | two fixes + two findings, one fixed, one reported | §4.4, §4.5, D8                                   |
| 4b. `build:fast` aborting in makensis     | fixed: the custom template caught up to the CLI   | §5, `src-tauri/nsis/installer.nsi`               |
| 4c. `build:fast` exiting 1 once bundled   | fixed: the updater signing key was rotated        | §5a, `tauri.conf.json`, `tauri-runner.ts`        |

Gates run on this state, all green: `cargo check --all-targets` (no warnings),
`cargo test -p zer0 --lib` (**415 passed**), `cargo clippy
--all-targets` (two warnings, both pre-existing and neither in a file this
change touched), `cargo fmt -- --check`, `bun run vite:build`,
`bun run check:translations`, `bun run check:identity`,
`bun run check:model-languages`, `bun run meta:check`, `bun run test:unit`,
`bun run typecheck`, `bun run lint` (warnings only, all pre-existing),
`bunx prettier --check` on every file touched.

The last row is the pass that came out of the mode's first real recording, and it
is the reason §4.4's overlay table now reads `Merge & Cleaned` instead of `M` and
carries the rule about a live model's block: the block's mark was renamed, a
model's column stopped waiting for its first word (the backend announces the slot
when the stream goes live, the overlay renders a known slot with a `no live text
yet` placeholder while it is empty, and `debugStream()` keys off a known slot
rather than off text), per-slot settings were extended from the first extra to
every extra, and a stream that runs a whole session without producing text is now
warned about by name, slot and language hint. Its gates: `cargo test -p zer0
--lib` **416 passed** (the new one pins slot→model and model→settings against
each other), clippy still two pre-existing warnings, `cargo fmt -- --check`
clean, and the frontend gates above re-run green.

Two things about this state are worth stating plainly — one red gate, and the
check the earlier state could not run at all:

- `bun run format:check` fails tree-wide on 41 files — the 26 locale JSONs, which
  are **CRLF on disk against a `.prettierrc` that asks for LF**, plus 15 other
  files unrelated to this work. `bun run precommit` runs `format:check` first, so
  the hook stops there and never reaches its Rust steps; the steps above were run
  by hand instead. Fixing it means a tree-wide `prettier --write`, which is a
  large diff of CRLF→LF churn and is deliberately not folded into this change.
- `bun run dev:fast` — the whole-app build **and launch** — was run on this state
  and came up: native library staged (13 files), binary running, frontend served.
  It holds `transcribe.dll` and `zer0.exe` while the app is up, so any gate that
  links needs the app closed first. Related, from that same run: the native build
  script's cache (`%LOCALAPPDATA%\handy\transcribe_cpp_cache\`) is keyed by a
  source-tree fingerprint rather than by `OUT_DIR`, so a cache entry restored into
  a new `OUT_DIR` carries the previous one's CMake cache and CMake refuses to
  configure over it (`CMAKE_CACHEFILE_DIR` mismatch). It cleared itself on the
  next run with no source change; the hatch if it does not is
  `TRANSCRIBE_FORCE_REBUILD=1`.

`check:identity` needed one new exemption: the CPU lane's cache root mirrors
`<LOCALAPPDATA>/handy/transcribe_cpp_cache/`, the directory transcribe.cpp's own
build script owns and every machine already has. It is a path this app does not
name, cannot rename, and must be a sibling of rather than a tree of its own. The
exemption is one line-anchored pattern in `scripts/check-identity.ts`.

## 1. The four build commands — what they really do

`scripts/tauri-runner.ts` is the only entry point for all of them. It sets two
environment variables and the native build script (`bindings/rust/sys/build.rs`
in the transcribe.cpp fork) reads them at configure time.

**Before the change below**, the lanes were:

| Command              | `TRANSCRIBE_MODEL_SET` | `TRANSCRIBE_CUDA_ARCHITECTURES` | CUDA arch actually compiled             |
| -------------------- | ---------------------- | ------------------------------- | --------------------------------------- |
| `bun run tauri dev`  | `minimal-multilingual` | unset                           | local arch (dev profile probes the GPU) |
| `bun run dev:fast`   | `minimal-multilingual` | `auto`                          | local arch                              |
| `bun run dev:full`   | `full`                 | unset                           | local arch (dev profile)                |
| `bun run build:fast` | `minimal-multilingual` | `auto`                          | local arch, release profile             |
| `bun run build:full` | `full`                 | unset                           | **all** architectures (release profile) |

So the answer to "do fast and full really differ" is **yes, but not the way it
looks**: they differ on _two_ independent axes, and only one of them is the CUDA
architecture the names suggest.

- **The CUDA axis** is real but weak. `--fast` pins `TRANSCRIBE_CUDA_ARCHITECTURES=auto`
  (one architecture, probed from `nvidia-smi`); `--full` leaves it unset, and the
  value then falls back to the profile: a **dev** build probes the local GPU
  anyway, a **release** build takes the full matrix. That is why `dev:fast` is
  effectively identical to plain `tauri dev`, and why `dev:full` gets the _fast_
  CUDA arch. The only way to get the full CUDA matrix without the `--full` model
  set is to export the variable yourself (`TRANSCRIBE_CUDA_ARCHITECTURES=default`
  forces the matrix even in a dev build).
- **The model axis** is the strong one. `--full` sets `TRANSCRIBE_MODEL_SET=full`
  (all 19 architecture families); everything else is `minimal-multilingual`
  (parakeet, granite, qwen3_asr). With `arch-dl` on — Windows x86_64 and Linux —
  that decides which `transcribe-arch-<family>.dll` modules exist, so a
  `minimal-multilingual` build simply **cannot open** a model whose family was
  not built. That is the modularity the next section turns off.

Two smaller findings worth fixing while here: the runner prints "18
architectures" while the fork builds 19, and several comments (in
`tauri-runner.ts`, `src-tauri/Cargo.toml`, `AGENTS.md`) still describe a
`.cargo/config.toml` `[env]` block that commit `0767f673` deleted in favour of a
`target-cpu=native` rustflags line.

### 1.1 Decision: deactivate the module split, keep CUDA arch as the only axis

The modular architecture delivery (`arch-dl`: one `transcribe-arch-*.dll` per
family, resolved at model-open time) is switched off for now, as asked. What
that changes:

- `arch-dl` goes away in the two Cargo target tables that have it (Windows
  x86_64, Linux). Architectures are compiled into `transcribe.dll` again, the
  way macOS and Windows aarch64 already work. Nothing in the app breaks:
  `managers/arch_plugins.rs` declares both C entry points unconditionally
  precisely because the static posture exists today.
- `TRANSCRIBE_MODEL_SET` stops being a lane knob at all: the runner sets it to
  `full` unconditionally, so **every** lane — `cpu`, `fast`, `full`, dev or
  release — packs all 19 families. That is what "deactivate the family-set
  selection" means here: a build never comes up missing a model the user asks
  for, and `fast` vs `full` means exactly one thing, local CUDA architecture
  versus all of them.
- `dynamic-backends` stays. That is a different mechanism (ggml backend modules:
  CPU ISA tiers + `ggml-cuda`), not the model-modularity being removed, and it
  is what lets one binary serve both a CUDA and a CPU device.
- The Models page's "plugins folder" tooltip will report zero external plugins
  in these builds, which is accurate: with no per-family modules there is
  nothing to drop in. The copy stays honest, no code change needed.

Cost, stated plainly: **every** lane now compiles all 19 families, where a GPU
lane used to compile 3. That is the price of "every model works in every build",
and it is accepted here for two reasons: the lane for fast iteration is the new
CPU one below, whose speed comes from dropping CUDA rather than from dropping
models, and the per-model backend work in §2 needs every family present in one
binary anyway — a model cannot be moved to CPU or Vulkan from a build that does
not contain it.

### 1.2 New lane: `dev:cpu` / `build:cpu`

A CPU-only lane for the "does this compile and does the app come up" loop, which
is most of the time. It sets:

- no CUDA: the feature-derived `-DTRANSCRIBE_CUDA=ON` is overridden with
  `TRANSCRIBE_CMAKE_ARGS=-DTRANSCRIBE_CUDA=OFF` (the native build script applies
  `TRANSCRIBE_CMAKE_ARGS` _after_ the feature defines, so the override wins
  without touching the manifest — which keeps CI, which never runs the runner,
  on its current CUDA posture).
- its **own** `TRANSCRIBE_CACHE_DIR`. The native cache key does not include
  `TRANSCRIBE_CMAKE_ARGS`, so a CPU configure sharing the CUDA cache directory
  could hit a CUDA entry and silently link a CUDA-built library. A separate
  cache root makes that impossible in both directions. (`TRANSCRIBE_MODEL_SET`
  _is_ in the key, so switching a lane's model set is a cache miss rather than a
  stale hit — the always-full change below cannot be served from the old
  three-family entry.)
- the **full** model set, like every other lane. It is a CPU lane, not a small
  lane: all four streaming-capable models (§4.5) are present in it, so the new
  multi-streaming mode can be exercised end to end without a CUDA build.

The result drops the ~189 `.cu` translation units and the ~30 MB `ggml-cuda`
module from the build entirely and runs on the CPU backend that is compiled in
regardless.

Verified end to end (2026-09-21): a `cargo build -p transcribe-cpp-sys` under
exactly this environment produced a tree whose `CMakeCache.txt` reads
`TRANSCRIBE_CUDA=OFF`, `TRANSCRIBE_ARCH_DL=OFF`, `TRANSCRIBE_MODEL_SET=full`;
`compile_commands.json` carries **164** translation units and **zero** `.cu`;
the staged `bin/` holds `transcribe.dll`, `ggml.dll`, `ggml-base.dll` and the ten
per-ISA `ggml-cpu-*.dll` tiers — and **no `ggml-cuda.dll`**. The 2.1 MB
`transcribe.dll` contains a marker string for every family that the old
three-family set did not have (`medasr`, `gigaam`, `sensevoice`, `sortformer`,
`canary_qwen`, `granite5_ctc`), so the full set is genuinely compiled in, not
just requested. Two entries now sit under the CPU cache root — the old
three-family one and the new full-set one — which is the cache-key behaviour
described above, observed.

When to use which, and this is the line the docs will carry:

- **`bun run dev:cpu`** — the normal working loop, and the "did I break the app"
  check. Fast to build, no GPU modules staged.
- **`bun run dev:fast`** — when the change touches the GPU path (CUDA kernels,
  device selection, VRAM, a perf measurement). Local CUDA architecture, all
  model families.
- **`bun run build:full`** — release-shaped builds, with the full CUDA matrix.

### 1.3 The lanes as implemented

The runner now carries three CUDA policies (`local`, `matrix`, `off`) instead of
two, and the model set is no longer a lane axis — every lane packs the full set:

| Command              | model set | CUDA                                         |
| -------------------- | --------- | -------------------------------------------- |
| `bun run dev:cpu`    | `full`    | off (`-DTRANSCRIBE_CUDA=OFF`, private cache) |
| `bun run dev:fast`   | `full`    | `TRANSCRIBE_CUDA_ARCHITECTURES=auto`         |
| `bun run dev:full`   | `full`    | `TRANSCRIBE_CUDA_ARCHITECTURES=default`      |
| `bun run build:cpu`  | `full`    | off                                          |
| `bun run build:fast` | `full`    | `auto`                                       |
| `bun run build:full` | `full`    | `default`                                    |

CUDA architecture is the only axis left, and every command in the table states
its value explicitly rather than leaving it to the profile.

`--full` now sets the variable explicitly, so it means "every architecture" in a
dev build too — previously it left it unset and a dev profile probed the local
GPU, which made `dev:full` behave like `dev:fast` on the CUDA axis. `arch-dl` is
gone from both target tables that had it, and the stale "18 architectures" count
and the `.cargo/config.toml` references (that file now holds only
`rustflags = ["-C", "target-cpu=native"]`, root and `src-tauri/`) are corrected
in `AGENTS.md`, `BUILD.md` and the runner itself.

## 2. Per-model backend selection (CUDA / Vulkan / CPU)

### 2.1 What is there today

One global setting decides the backend for **every** model — the primary, all
Multi-STT extras, and benchmark variants alike:

- `settings.rs` `transcribe_accelerator: TranscribeAcceleratorSetting` — an enum
  of `Auto | Cpu | Gpu`, with no vendor axis at all — plus
  `transcribe_gpu_device: Option<String>`, a stable device identity key.
- `managers/transcription.rs` resolves that into a `(Backend, Option<Device>)`
  pair in two places: the primary load, and `create_engine`, the helper every
  extra model goes through. `create_engine` takes no device parameter; it re-reads
  the globals, which is why all four Multi-STT engines land on one backend.

### 2.2 What the engine offers

Per-load, with no global backend state:

```rust
pub enum Backend { Auto, Cpu, CpuAccel, Metal, Vulkan, Cuda, Rocm }
pub struct ModelOptions { pub backend: Backend, pub device: Option<Device> }
Model::load_with(path, &ModelOptions)
```

Devices are enumerated at runtime (`devices()`, `backend_available(Backend)`,
and the CPU is registered as a device like any other), the resolved backend is
stored **per model**, and a CUDA-bound model already coexists with a live CPU
backend in the same process — the CPU backend is pushed onto every GPU model's
scheduler list as its fallback. Two models on two backends is therefore not a
new capability, it is the current arrangement with a different primary.

One trap: an explicit backend request has **no silent fallback**. Asking for
`Cuda` on a machine without CUDA fails the load outright, so a per-model setting
must be validated against `backend_available()` before it is applied.

### 2.3 Decision and shape

Give each model its own backend, defaulting to "follow the global setting" so
nothing changes for anyone who does not touch it:

- a per-model override keyed by model id (`backends: HashMap<String, ...>` in
  settings, or a field beside the Multi-STT slots for those four), holding
  `Auto | Cpu | Cuda | Vulkan | Metal` — `Auto` meaning "use the global
  accelerator setting";
- `create_engine` takes the resolved `(Backend, Option<Device>)` instead of
  reading globals, with the primary load passing its own;
- the UI: a backend dropdown per model row in the Multi-STT panel and on the
  model cards, listing only what `devices()` / `backend_available()` reports as
  present on this machine, so an impossible choice cannot be selected.

Shipping note: today only one accelerator feature is enabled per lane, so a
"Vulkan" choice would be selectable but not actually compiled in on the current
Windows build unless `vulkan` is added beside `cuda`. Since `dynamic-backends`
builds each backend as a loadable module and silently skips the ones that fail
to load, adding it is a one-word feature change plus the module's size — to be
done when the feature is wanted, not before.

## 3. The experimental panel: rename, and a nested second mode

The group currently titled "Experimental: streaming 1st model" becomes
**"Experimental Live Streaming Merge & Clean"**, its first toggle is renamed to
match what it does, and its tooltip and info box are rephrased to explain the
idea rather than the mechanism. Inside that group, gated on its toggle, a new
**"Experimental Multi Streaming STT"** toggle adds the second mode (§4). When
that one is on it takes over: only streaming-capable models are loaded and
used, and the non-streaming slots are ignored.

The panel currently never checks `supports_streaming` — a user can enable the
mode with a non-streaming primary and the session silently falls back to batch
with a log warning. The nested toggle is where that surfaces instead.

Implementation notes that matter: the strings are i18n keys
(`multiStt.streamingFirst.*`) whose **values** are still English in all 26
locales, so this is a values-only edit and `check-translations` stays green
because no key is added or removed. New keys for the nested toggle do need all
26 files. The new setting is a `#[serde(default)] bool`, which needs no schema
bump, plus: default literal, command, `collect_commands!`, regenerated
`bindings.ts` (written at app startup), the store's `settingUpdaters` map, the
UI toggle, 26 locale files, and regenerated test fixtures.

## 4. Multi-streaming STT — two or more live streaming models

### 4.1 What exists, and why two streams are not just "call start_stream twice"

The existing mode lives in one module, `src-tauri/src/multi_stt_stream.rs`: it
arms a session before recording, claims the chunk audio tap, installs an
**exclusive** text sink, and runs its own `std::thread` coordinator on a 50 ms
tick. Each tick drains audio, watches for a pause
(`multi_stt_streaming_pause_ms`), closes a chunk, and re-runs the previous
chunks through the Multi-STT extras plus the LLM merge
(`run_merge_job` → `multi_stt_merge_transcriptions`), publishing the merged
result. The primary model's own live stream is separate — it is the app's
normal `TranscriptionManager::start_stream` worker — and the coordinator only
_absorbs_ its text into the current chunk.

Native streaming itself is model-agnostic. Everything specific to a model is one
resolver, `managers/native_streaming_latency.rs::stream_extension_for`, which
both streaming call sites share, and which returns the `StreamExtension` a model
needs (for R2T2: `R2T2StreamOptions { chunk_size_ms }`, 80–2000, default 320,
copied into the stream at `stream_begin` and **rejected, not clamped**, if out of
range).

Four things in the design **as it stood before this change** are single-slot, and
they are the whole cost of this feature. They are listed here as the diagnosis;
§4.2 records what was done about each:

1. `active_stream_worker` and `active_engine_lease` are process-wide
   single-value atomics; `start_stream` refuses to start if either is held. A
   second stream cannot exist under the current API.
2. There is **one** text sink slot and one exclusive flag. `emit_stream_text`
   returns early when the sink is exclusive, and the exclusive sink writes a
   single `Snapshot`.
3. `StreamTextEvent { committed, tentative, failed_chunks, whole_session }`
   carries **no identity** — the overlay cannot tell two live texts apart.
4. `transcribe_with_extra` leases an extra engine **per decode** (removes it from
   the map, decodes, returns it). A streaming extra must be leased for the whole
   session instead, or an unrelated batch decode yanks its engine out from under
   the live stream.

Also load-bearing: `Stream<'_>` borrows its `Session` mutably and takes that
model's `compute_lock` for the stream's lifetime, so two streams are legal **only
across two separate `Model` loads** — which is exactly what Multi-STT extras
already are (a fresh `Model` per engine, hence a fresh lock).

### 4.2 Decision as implemented: key the existing single-slot state by stream, and run two workers

> **This section was rewritten after implementation, and it records a deviation
> from the design this note first proposed.** The original §4.2 called for _one
> coordinator holding two `Stream` handles_, feeding both from the same 50 ms
> tick. That is not what was built. What was built was the smaller change the
> next paragraph describes, and the reason is the paragraph after it.

The single-slot state listed in §4.1 is not flattened and rebuilt into a
coordinator. It is **arrayed**: every piece of it becomes a two-entry structure
indexed by a **stream slot** (a `u8`), and the existing `start_stream` worker is
left exactly as it is — its feed, backlog coalescing, direct typing, statistics
and finalize all untouched — with a second worker started beside it on the other
slot.

- `PRIMARY_STREAM_SLOT: u8 = 0`, `EXTRA_STREAM_SLOT: u8 = 1`, `STREAM_SLOTS = 4`
  (`managers/transcription.rs`) — one slot per Multi-STT model slot, so a fourth
  streaming model has somewhere to run. Stream slot _n_ carries
  `multi_stt_model_{n+1}` (`multi_streaming::stream_slot_of`).
- `active_stream_worker`, `active_engine_lease`, `stream_active` and
  `stream_attempt` become `Arc<[_; STREAM_SLOTS]>` — one entry per slot, each
  entry still held with exactly the same exclusivity as before.
- `StreamRouter` holds `Mutex<Vec<(u8, Sender<StreamCmd>)>>` with
  `open(slot)` / `take(slot)` / `clear(slot)` / `feed`. `feed` **broadcasts** to
  every open slot, and each slot's channel is an unbounded `mpsc::channel()`, so
  feeding two streams cannot block the audio callback and neither worker waits on
  the other.
- `start_stream` → `start_stream_on(slot, live_typing, statistics, supplied)`,
  where `supplied` is `None` for the primary (the worker leases the app's own
  engine) and `Some((model_id, Option<LoadedEngine>))` for an extra.
  `start_extra_stream` is the extra's entry point and **never types**: with
  `DirectStreaming` a second writer would race the first into the same document.
- `finalize_stream` → `finalize_stream_on(slot)`, `cancel_stream` →
  `cancel_stream_on(slot)`; the unload-immediately policy stays on the primary's
  slot only, because an extra's unload policy belongs to the Multi-STT paths that
  loaded it.
- `StreamTextEvent` gains `slot: Option<u8>` — `None` for the primary, so every
  existing consumer and the serialized bytes of the plain path are unchanged.
  `emit_stream_text` notifies the exclusive sink **only** for the primary slot;
  `emit_composed_stream_text` sets `slot: None`. The overlay discriminates on
  `slot` in one listener (see §4.4).

**Why not the coordinator.** The coordinator design would have moved the
primary's stream out of the worker it has always run in and into a new thread
that also drives the extra — which means re-implementing, inside the new
coordinator, the primary's feed, its backlog coalescing, its direct-typing and
its per-session statistics, and then keeping that copy in step with the original
forever. The slot-keyed design reuses all of it verbatim: the only genuinely new
code is the slot plumbing, the extra's engine lease, and a second overlay column.
It also needs **no audio tap at all** — `StreamRouter::feed` already receives
every frame the recorder produces, so both models hear the same frames in the
same order by construction. And it holds no `Stream` handle at all: each worker
owns its own, lifetime unchanged, so the `Stream<'_>`-borrows-its-`Session`
constraint noted at the end of §4.1 is satisfied the same way it always was.

The mode itself is `src-tauri/src/multi_streaming.rs`, and it is deliberately
thin — no coordinator, no tap, no merge. In the parent mode those exist to do a
second job _between_ breaks (re-decode the chunk with the extras, merge, replace
the preview). This mode has no such job: both texts are the models' own, produced
by the two workers, and no batch decode runs at all. What is left is plumbing:

- **which extras**: every **streaming-capable** slot among models 2, 3 and 4, in
  that order — the user's own list, read the way it is read everywhere else — one
  stream per slot, up to three beside the primary. Non-streaming slots are
  **skipped rather than loaded** — `streaming_slots` logs each one it skips — and
  the preload is narrowed to the same list (`MultiSttAction::start`), so the
  preload and the session cannot disagree about which models run.
- **the engine lease**: a live stream holds its engine for the session, so it
  cannot be leased per decode (§4.1 item 4) and it must not be leased before the
  model is loaded. `start_extra_stream_when_loaded` (on the manager, which owns
  both pieces of state, and which keeps `LoadedEngine` private to itself) polls
  for the load, leases, and starts the stream — and it is the _only_ caller of
  `start_extra_stream`/`lease_extra_engine`, both now private.
- **the wait**: the extra is usually still loading when the recording starts,
  because the preload is backgrounded so the user can speak at once. `start`
  therefore spawns one waiter thread on that poll, bounded by `LOAD_WAIT`
  (120 s) and by a `stop` flag it polls alongside the load, so a recording that
  ends first is not given a stream nobody will collect. Frames pushed before the
  stream opens are queued on its channel and are not lost. A load that fails, or
  a poll that times out, degrades honestly to a **one-column session** with a
  warning — the recording is unaffected.
- **the second column's result**: `finish` returns the whole
  `TrackedTranscription`, not just its text, and `task2` closes its statistics
  attempt exactly the way the primary's is closed, so both columns are measured
  in the same statistics run.

### 4.3 Behaviour of the parent mode (`TextSource::ReDecode`)

With **Experimental Live Streaming Merge & Clean** on, and the nested toggle off:

- The primary must report `supports_streaming` (`ModelInfo.supports_streaming`,
  from the catalog's `capabilities.streaming`) or `start` refuses and the
  ordinary batch path runs — there would be no live text to correct. A merge
  prompt is required for the same reason: the mode exists to replace the
  primary's rough text with a merged one.
- The overlay shows **one** block: the primary model's live text, composed from
  the chunks. The sink is exclusive, the composed text goes out on `slot: None`,
  and no per-model column exists — the extras are batch models here, so there is
  no second live text to draw.
- Each pause closes a chunk; the extras **batch-decode that chunk's window** and
  the brain merges the texts. Nothing re-runs the recording: the window is the
  chunk's audio (plus the context chunks the setting asks for), and each extra's
  decode is cropped back to the chunk by `strip_context_prefix` because it heard
  the context.
- The merge's text replaces that chunk's span in what is displayed — the
  correction the user sees — and each closed chunk's `display_text()` joins into
  the session's `final_text` (`multi_stt_stream::finish`). **That composed text
  is the result that is pasted** (or typed, when the mode owns the writer), with
  the per-slot texts kept for statistics and history.
- The stop path also runs the Multi-STT extras' own batch decode
  (`extra_model_2/3/4` → `task2/3/4`) unless the nested mode is active, which is
  the ordinary batch path's behaviour and unchanged here.

The nested toggle supersedes this: when it arms, the coordinator runs with
`TextSource::Live` and the parent's own branch is not entered.

### 4.4 Behaviour of the nested mode (`TextSource::Live`), in two views

**Experimental Multi Streaming STT** is the parent mode with a different text
source (see D1–D6 of `2026-09-multi-streaming-stt-directives.md` for the
directives and the mapping; this is the design summary):

- **Every streaming-capable Multi-STT slot** among models 2, 3 and 4 streams
  beside the primary, one stream per slot (`multi_streaming::start`), each on its
  own engine lease for the whole session and its own worker. A non-streaming slot
  is not loaded at all, and the preload is narrowed to the same list.
- **A break merges the models' own live texts.** Each model's text for the chunk
  is the part of its live text that belongs to the chunk's span — no decode runs
  at a break, so a break costs **one LLM round trip and no inference**, where the
  parent mode's break costs one decode per extra plus the merge.
- **The extras are finalized before the session closes**, in the order the module
  docs make load-bearing: primary `finalize_stream()`, then
  `multi_streaming::finish_extras` (one `finalize_stream_on(slot)` per started
  slot, each returning the `TrackedTranscription` that `task2/3/4` then close),
  then `multi_stt_stream::finish(FINISH_TIMEOUT)`. Finalizing after the close
  would hand those models' last words to a coordinator with no chunk to put them
  in.
- **Two views, one switch** (`multi_stt_streaming_multi_debug_view`, off by
  default):

  | View          | Overlay                                                                        | Sink      |
  | ------------- | ------------------------------------------------------------------------------ | --------- |
  | debug **on**  | one block per model (`1`, `2`, …) + the merged block (`Merge & Cleaned`) below | shared    |
  | debug **off** | one block: model 1's live text, corrected in place at every pause              | exclusive |

  The switch reaches the coordinator as `debug_view` and decides both the publish
  shape and the sink's exclusivity — the two have to agree, or the columns would
  be suppressed by an exclusive sink. The merged block rides
  `MERGE_BLOCK_SLOT = STREAM_SLOTS as u8`, a slot no model can occupy, and
  `whole_session: true` is what tells the overlay it is the block and not a
  column. The block's mark is the settings' own name for the pause-time work
  (`overlay.mergeAndCleaned`) rather than a letter, so it cannot be read as one
  more model beside the numerals.

- **A model's block is up for as long as its stream is**, whether or not that
  model has said anything: the stream's slot is announced with an empty-text
  event when it goes live (`announce_stream_slot`, on the same terms as every
  other numbered-slot event, so the production view never sees it), and the
  overlay gives a known slot its column with a muted `no live text yet` while it
  is empty. A column that waited for its first word would be indistinguishable
  from a model that was never started — the whole point of the view — and a model
  whose language hint does not match the speech never has a first word (§4.5).
  In the production view a silent extra has no column at all, so the account it
  gets instead is a warning from `finalize_stream_on` naming the slot, the model
  and the hint it streamed under.

- **The merged text is the result in both views**: `finish` composes
  `final_text` from the closed chunks' merged text exactly as the parent mode
  does, and `MultiSttAction` pastes it. The debug view changes what is _drawn_,
  never what is _produced_.
- **No batch STT anywhere in the session**, and none at stop either: while the
  mode is active the extras are taken out of the list entirely
  (`extra_model_2/3/4 = None`), so nothing decodes the recording and nothing
  decodes a chunk.
- **Failure is per column, not per session.** A model that never loads, a wait
  that times out (`LOAD_WAIT`, 120 s) or a stream that fails leaves the merge one
  text short and the recording untouched; the log names the slot and the model.
  If the **primary** stream does not complete, the mode has nothing to report:
  the extras are cancelled (not finalized), their engines released, and the batch
  path takes over.

The direction this is built for is where it now stands: with the streaming models
live and trustworthy, the pause-triggered work stopped being "re-transcribe the
chunk with every model" and became "merge and clean the streaming texts at
pauses", which removed the batch decode from the loop entirely. What is left on
the list is per-model backend selection for those extras (§2) and the merge's own
cost, which is one brain round trip per pause.

### 4.5 Which models this can run with today

`capabilities.streaming: true` in the catalog, four entries:

| Model                             | Latency kind           | Note                                  |
| --------------------------------- | ---------------------- | ------------------------------------- |
| `parakeet-unified-en-0.6b`        | `ParakeetBuffered`     | buffered, English only                |
| `nemotron-3.5-asr-streaming-0.6b` | `Nemotron35CacheAware` | the "Nemotron 3.5" of the target pair |
| `Voxtral-Mini-4B-Realtime-2602`   | _(none)_               | streams, but no latency extension     |
| `Confucius4-R2T2` (`r2t2-q8_0`)   | `R2T2ChunkMs`          | the other half of the target pair     |

Two notes. There is **no "Nemotron 3.5 ultra"** in the catalog — the entry is the
0.6B streaming model; if an ultra build exists in the local HF cache it would
have to be matched by its own slug. And R2T2 shares the `qwen3_asr` architecture
with the _non-streaming_ `Qwen3-ASR-0.6B`, which is why streaming capability and
latency kind are resolved by **id slug**, never by architecture — the guard test
`r2t2_entry_is_wired_for_streaming_from_its_pinned_reference`
(`catalog/mod.rs:312`) exists to keep that true.

In a session, the language each of these streams under is the one the Multi-STT
panel pinned for **that** slot (`multi_stt_language_model_2/3/4`, applied per
slot by `apply_extra_model_settings`), and for a prompt-conditioned model that
pin is not a preference but an instruction: `nemotron-3.5-asr-streaming-0.6b` is
told which language to transcribe and returns **nothing** when the hint does not
match the speech, while an unrelated hint can return the speech unchanged. Its
own doc says a language must be provided. Verified against the fork's CLI on
`jfk.wav`: `en-US` and `en` transcribe it, `fr`, `fr-FR` and `de-DE` come back
empty, and `es-ES` comes back as the full English text. So a second column that
never fills in is first a settings question, not a streaming one — which is what
§4.4's warning exists to say.

## 5. The installer: `build:fast` aborting in makensis

`build:fast` compiled the app (7m50s), bundled the MSI, and then died in the NSIS
step:

```
!insertmacro: macro named "RestartManager_StartSession" not found!
Error in macro CheckIfAppIsRunning on macroline 11
Error in script "...\target\release\nsis\x64\installer.nsi" on line 734 -- aborting creation process
failed to bundle project: Failed to bundle app with makensis
```

**The cause is one file out of step, and it is the one file in the NSIS step the
repo owns.** `bundle.windows.nsis.template` points the bundler at
`src-tauri/nsis/installer.nsi`: a fork of the tauri-v2.9.1 upstream template that
adds portable mode. Everything else in that step is the installed CLI's —
`utils.nsh` and `FileAssociation.nsh` are rendered from `@tauri-apps/cli
3.0.0-alpha.2`, and `English.nsh`, `Win\*.nsh` and the plugins come from the NSIS
tree it installs under `%LOCALAPPDATA%\tauri\NSIS`. An old template against a new
`utils.nsh` is a mismatch NSIS only reports at the point of use: the new
`CheckIfAppIsRunning` inserts three macros from **`RestartManager.nsh`**, which
the current upstream template pulls in with `!include "Win\RestartManager.nsh"`
and the fork never had.

The file's own header has said what to do about this since it was forked —
"when upgrading Tauri, diff this file against the new upstream template and
merge changes while preserving the portable sections". Diffing it against the
template embedded in the installed CLI (`include_str!`'d into
`cli.win32-x64-msvc.node`, extractable as plain text) gives 18 hunks: the
portable-mode ones, which stay, and the alpha's, of which exactly two are
needed to build:

| Edit                                    | Why it is required, not cosmetic                                                                                                                                                                                               |
| --------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `!include "Win\RestartManager.nsh"`     | Defines `RestartManager_StartSession`, `_RegisterFile` and `_EndSession`. Without it the macro expansion fails outright, which is the error above.                                                                             |
| full path at both `CheckIfAppIsRunning` | The macro hands its first argument to `RmRegisterResources`, which resolves a bare `ZER0.exe` against the installer's own directory, fails with `ERROR_FILE_NOT_FOUND`, and makes the caller skip the running-app check whole. |

The alpha's other hunks are deliberately **not** merged: an uninstaller-icon and
uninstaller-header-image block (cosmetic, and nothing configures those images),
`!addplugindir "{{signed_plugins_path}}"` (this build's plugins resolve without
it — the two `nsis_tauri_utils::StrReplace` calls ahead of the failing line
compiled), and upstream's removal of `DesktopShortcutMode`, which is the repo's
own opt-in desktop-shortcut feature and would take the `/DESKTOP` flag with it.

**How it was verified, cheaply then fully.** `makensis` compiles a script in
about a minute, so the two edits were first applied to a copy of the _rendered_
`target/release/nsis/x64/installer.nsi` and compiled there — success, a 30.9 MB
`nsis-output.exe`, and the only warning the long-standing one about
`SkipIfPassiveOrPortable` being unreferenced when `STARTMENUFOLDER` is empty. The
template was then edited and the whole `bun run build:fast` re-run: `Finished 2
bundles at:` with `ZER0_0.9.7_x64_en-US.msi` (38.63 MiB) and
`ZER0_0.9.7_x64-setup.exe` (29.50 MiB), and the setup was run and installed the
app.

**The command still exits 1, one step further on, and that is not this.** After
`Finished 2 bundles at:` it stops on the Tauri updater signing:

```
A public key has been found, but no private key. Make sure to set `TAURI_SIGNING_PRIVATE_KEY` environment variable.
```

`bundle.createUpdaterArtifacts` is on and `plugins.updater.pubkey` is set, so
every bundle build signs unless `TAURI_SIGNING_PRIVATE_KEY` is in the
environment; `scripts/tauri-runner.ts` supplies it from `~/.tauri/zer0.key`, and
that file is not on this machine — `%USERPROFILE%\.tauri\` exists and is empty.
Both installers are already written by then, so a local release is usable, but
the `.sig` artifacts are not produced. BUILD.md already
documents this exact ending and the remedy (regenerate with `bun x tauri signer
generate -w ~/.tauri/zer0.key`, or restore the existing private key, whose public
half is pinned in the config); it is a release secret, so nothing here touches
it.

Two notes for whoever is here next. The MSI is untouched by any of this — WiX
does not read these files, which is why it succeeded while NSIS aborted. And the
build directory under `target/release/nsis/x64/` is regenerated from the template
on every bundle, so a fix applied there and not to the template disappears on the
next build.

## 5a. The updater signing key, rotated (2026-09-22)

`build:fast` now ends at exit 0 — `Finished 2 bundles at:` and `Finished 2 updater
signatures at:`, four artifacts: both installers and both `.sig` files. Two things
stood in the way, and neither was the installer work of §5.

**There was no key to restore.** The repository has no Actions secrets at all
(`gh api repos/NairoDorian/S2B2S/actions/secrets` → `total_count: 0`), so the
`TAURI_SIGNING_PRIVATE_KEY` the release workflow reads has never been set, and no
copy of the key existed on this machine either. That is what left rotation as the
only option.

**The public half has a shape that fails only in the field.**
`plugins.updater.pubkey` is base64 _of the minisign public key text_ — not by
convention but because the pinned plugin runs `base64_to_string(pub_key)` before
`PublicKey::decode` (`plugins/updater/src/updater.rs:1537` of the
`plugins-workspace` checkout at `2fd27c2`). The alpha CLI writes exactly that
value into `~/.tauri/zer0.key.pub`, so the config wants that file's contents,
trimmed; encoding them a second time yields something that base64-decodes without
complaint — to base64 rather than to `untrusted comment: minisign public key: …`.
The first splice here did that, and only reading the consumer caught it.

Three checks now separate that from a release:

- **The bundler checks the pair itself.** With a mismatched key it warns that the
  secret key does not match the public key in `plugins > updater > pubkey` and
  that the configuration "won't be accepted at runtime when performing update".
  That warning is absent from the build above, and the probe that produced it —
  `bun x tauri bundle --bundles nsis` against the already-built binary, 25 seconds
  rather than eight minutes — is the cheap way to see it.
- **An independent verifier.** `sigcheck`, a scratch crate, pins
  `minisign-verify` to the version `Cargo.lock` resolves the plugin to, and
  reproduces `verify_signature` step for step: both `.sig` files verify against
  the config's value. Its negative control is the point of it — handed a
  signature made with the other key, it fails, naming the key as the reason.
- **The signatures carry `version: 0.9.7`** in the trusted comment, matching the
  announced version, so the check would hold with `require_signed_version` turned
  on as well.

**One more obstruction, in the runner.** `tauri signer generate` always writes an
_encrypted_ secret key, so with no password in the environment the bundler stops
at `Decrypting updater signing key, expect a prompt for password` — after the
bundles are built, reading from a terminal that a non-interactive run does not
have. It does not fail; it waits. `scripts/tauri-runner.ts` now sets
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD` to empty when it is the one supplying the
local key, which skips the prompt (`minisign` reads an empty password exactly as
it reads no password) and leaves any password already in the environment alone.

`latest.json` is not the bundler's to write: it emits `.sig` files, and the
release workflow's `tauri-apps/tauri-action@v0` step is what turns them into the
manifest the updater endpoint serves.

**What the rotation costs, and what it does not.** Installs built before it pin
the old public key and reject new-key artifacts (`UnexpectedKeyId`) until they are
updated by hand once — but the app has never been released and the only install in
existence is the developer's own, so that is one reinstall, not a migration. CI
had the matching gap: the secret list was empty, so its bundle builds signed with
an empty key and failed exactly where this did. That is closed —
`gh secret set TAURI_SIGNING_PRIVATE_KEY < ~/.tauri/zer0.key` was run on
2026-09-22 and `gh secret list` now shows the one secret, with no password secret
needed for this pair. BUILD.md's "Updater artifact signing"
section carries the procedure, the shape of the config value, and the warning to
back the private key up somewhere outside the machine.
