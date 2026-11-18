# STT regression benchmarks

Run locally against installed models only. No script downloads weights.
Every compute device `--list-devices` reports is measured — CPU, CUDA and
Vulkan, since the Windows x86_64 and Linux targets build both GPU backends.
A case whose loaded backend does not match the requested one is a recorded
failure (`Requested vulkan, loaded cuda`), never a silent fallback or a
substituted CPU result.

Every case loads a model once and runs exactly **three** inferences on that
resident model. Run 1 is warm-up, retained only as raw evidence. Scores are the
arithmetic mean of runs 2 and 3. Cold load is separate. The headless path skips
the `Immediately` unload for its own process — `maybe_unload_immediately` returns
early when `--transcribe-file` is set, because three runs must share one warmed
model — and errors out rather than score a run whose model was unloaded. Settings
are not changed.

```powershell
bun scripts/bench-stt.ts --exe src-tauri/target/debug/zer0.exe `
  --wav ../transcribe-fork/samples/jfk.wav --output reports/bench/current
```

The default subset is every _installed_ model matching Granite Speech 4.1 2B Q4,
Qwen3-ASR 1.7B/0.6B, Nemotron 3.5 Q6/Q8, Parakeet TDT 0.6B v3 Q4/Q8 or R2T2
Q4/Q8. Pass `--model <id>` one or more times for an exact subset, and
`--backend <kind>` (repeatable, matching `--list-devices`' `kind=` or a
`vulkan_nvidia` / `vulkan_intel` tag) to narrow the device set; `--timeout`
defaults to 600 s per case. Nemotron and R2T2 are tested offline _and_
streaming; other profiles use their offline API. Nemotron streams in 16 ms
feeds with right context 6; R2T2 streams in 320 ms feeds. Effective language
and stream options are included in the results, and a baseline whose language
or `stream_chunk_ms` differs is a configuration mismatch rather than a
comparison. Device indices are discovered on each run, never hardcoded.

For one model use the executable directly:

```powershell
src-tauri/target/debug/zer0.exe --transcribe-file ../transcribe-fork/samples/jfk.wav `
  --model <installed-model-id> --device-index <index-from-list-devices> --repeat 3 --json
# For Nemotron, append --stream-chunk-ms 16 --stream-att-right 6
# For R2T2, append --stream-chunk-ms 320
```

Each case has a JSON result and stderr log. `summary.json` contains results
and failures, including timeouts. `--baseline <summary.json>` fails on >15%
slower warm mean (`--max-regression` changes this budget), changed transcripts,
or a mismatched backend, WAV, audio duration, language or stream chunk. Review
noisy measurements with repeated full three-run cases; never replace a score
with its fastest run. Keep the same WAV, language/settings, quant, device, power
state and native build when evaluating an app-only change. Do not run builds or
GPU work during benchmarks.

App replay measures WAV decode/resampling, cold load, native stages, stream
begin/feed/finalize, and app batch overhead. It does not simulate microphone
arrival pacing, VAD/denoise, UI, clipboard, or Multi-STT merge. Use live
`pipeline` records for those stages and the existing
`src-tauri/tests/vad_speech_clock_probe.rs` for capture/VAD testing.
`first_text_compute_ms` is unpaced compute latency, not microphone latency.
Live queue wait is measured separately on the real stream worker.

## Local native changes

The app's Cargo dependency follows a **git revision**, not the sibling working
tree. Editing `transcribe-fork` alone cannot change that compiled runtime.
Use the library's supported prebuilt install path for local validation:

```powershell
# In transcribe-fork, after building the desired Release backend:
cmake --install build/diagnostics/native --config Release --prefix build/diagnostics/install
# In this app's shell; keep this set while running dev:
$env:TRANSCRIBE_DIR = (Resolve-Path ../transcribe-fork/build/diagnostics/install).Path
bun run tauri dev
```

`TRANSCRIBE_DIR` (or `TRANSCRIBE_PREBUILT_DIR`) does two things: the runner
loads that install instead of the pinned revision, and it skips
`scripts/check-transcribe-deps.ts`, so the pin refresh that normally runs
before every build will not replace the library you are measuring. That is the
whole point of setting it — clear it before benchmarking anything you want
compared against a baseline.

The diagnostic install for this investigation includes the existing CUDA
module built from the same fork GGML source. A fresh general-purpose install
should build CUDA itself. Check `--list-devices` and startup native provenance.
Do not infer the loaded DLL from Cargo.lock alone. Set `TRANSCRIBE_DIR` only in
the development shell; do not commit machine paths. The native suite at
`transcribe-fork/scripts/bench/suite.py` accepts an exact `--library` and keeps
an independent engine baseline without the app.

## Engine bump A/B: `cfb07a12` → `8b27bc6b` (2026-11-18)

Same app code built twice on the CPU lane (`bun scripts/cargo-cpu.ts build`),
once per engine pin; both exes and their `transcribe.dll` (different hashes)
copied aside and run alternately through `bench-stt.ts` on `fleurs-en.wav`
(10.5 s): 4 rounds, the arm order flipped each round, 3 runs per case with
the warm-up dropped, so 8 timed runs per arm. Devices: the RTX 4070 Laptop
through Vulkan and the i9-13900H CPU. CUDA was not measured (the CPU lane
does not build it).

Every case produced the same transcript on both pins.

| Case                                            | Old ms | New ms | New vs old                                              |
| ----------------------------------------------- | -----: | -----: | ------------------------------------------------------- |
| Nemotron 3.5 Q8_0, Vulkan, batch                |    193 |    129 | 1.50x faster                                            |
| Nemotron 3.5 Q8_0, Vulkan, stream (16 ms feeds) |    479 |    367 | 1.31x faster                                            |
| Parakeet TDT 0.6B v3 Q4_K_M, Vulkan             |    118 |     91 | 1.30x faster                                            |
| Parakeet Ultra Q8_0, Vulkan                     |    135 |    104 | 1.30x faster                                            |
| Parakeet Redux TQ1, Vulkan                      |    124 |     99 | 1.26x faster                                            |
| R2T2 Q4_K_M, Vulkan, batch                      |    391 |    354 | 1.10x faster (a later 4-round rerun: 370 vs 400, noise) |
| R2T2 Q4_K_M, Vulkan, stream (320 ms)            |   2181 |   2253 | **3.3 % slower** (rerun: 2209 vs 2289, 3.6 %)           |
| Granite 4.1 2B Q4_K_M, Vulkan                   |    297 |    293 | level                                                   |
| Qwen3-ASR 1.7B Q5_K_M, Vulkan                   |    389 |    392 | level                                                   |
| CPU, every model above, batch and stream        |        |        | level (within ±6 %, ranges overlap)                     |

- The Vulkan gains are the Parakeet / Nemotron decoder running on the GPU
  (`decode_ms` 43 → 21 for Parakeet v3, 147 → 57 for the Nemotron stream).
- R2T2 streaming on Vulkan is the one consistent slowdown: the new arm was
  never below 2204 ms, the old one rarely above 2200. A rerun with
  `TRANSCRIBE_STEP_PIPELINE=0` (2302 ms) shows the new step pipeline is not
  the cause, as expected: R2T2's streaming ticks verify a draft, a path the
  pipeline does not take. Not bisected; about 2 ms per 320 ms tick.
- Granite's `decode_ms` is not comparable across the bump: the engine now
  counts the token loop in it (Vulkan 26 → 177 ms) where it used to report
  the prefill only. Its total is unchanged.
- One CPU outlier (the first new-arm round of the Nemotron stream, up to
  2105 ms) did not repeat in the three later rounds (1334-1480 vs
  1325-1482).

### CUDA and the Intel iGPU (same day)

CUDA needs the fast lane, so both pins were also built with the `dev:fast`
environment (`TRANSCRIBE_CUDA_ARCHITECTURES=auto`, the `FAST_BUILD_OPTIONS`
CMake args, the `local-<hash>` cache root) as debug `cargo build`s, and run
the same way: 4 alternating rounds on CUDA (2 on the slow Intel iGPU), one
benchmark at a time, nothing building alongside. Transcripts identical in
every case.

| CUDA                      | fleurs-en 10.5 s, old → new    | dots 35 s, old → new           |
| ------------------------- | ------------------------------ | ------------------------------ |
| Nemotron 3.5 Q8_0, batch  | 196 → 95 ms, 2.06x             | 615 → 251 ms, 2.45x            |
| Nemotron 3.5 Q8_0, stream | 389 → 227 ms, 1.71x            | 1442 → 886 ms, 1.63x           |
| Parakeet Ultra Q8_0       | 112 → 79 ms, 1.42x             | 356 → 198 ms, 1.80x            |
| Parakeet TDT v3 Q4_K_M    | 100 → 75 ms, 1.34x             | 322 → 193 ms, 1.67x            |
| Parakeet Redux TQ1        | 108 → 80 ms, 1.35x             | 365 → 236 ms, 1.55x            |
| Granite 4.1 2B Q4_K_M     | 260 → 258 ms, level            | 1097 → 1065 ms, 1.03x          |
| Qwen3-ASR 1.7B Q5_K_M     | 296 → 298 ms, level            | 1157 → 1146 ms, level          |
| R2T2 Q4_K_M, batch        | 264 → 262 ms, level            | 995 → 985 ms, level            |
| R2T2 Q4_K_M, stream       | 1540 → 1572 ms, +2 % (overlap) | 6446 → 6520 ms, +1 % (overlap) |

| Vulkan, Intel Iris Xe (iGPU)   | old → new                                        | new, `TRANSCRIBE_PARAKEET_DEVICE_DECODER=0` |
| ------------------------------ | ------------------------------------------------ | ------------------------------------------- |
| Nemotron 3.5, batch            | 490 → 540 ms, **10 % slower** (decode 107 → 158) | 500 ms (decode 112)                         |
| Nemotron 3.5, stream           | 1958 → 1995 ms, 2 % slower (decode 169 → 219)    | 1951 ms                                     |
| Parakeet Redux                 | 263 → 286 ms, **9 % slower**                     | 264 ms                                      |
| Parakeet TDT v3                | 317 → 331 ms, 4 % slower                         | 319 ms                                      |
| Parakeet Ultra                 | 429 → 442 ms, 3 % slower                         | 430 ms                                      |
| Granite 4.1 2B                 | 1853 → 1675 ms, 1.11x faster                     |                                             |
| Qwen3-ASR, R2T2 batch / stream | level                                            |                                             |

- **The one regression, with its cause:** the engine's new default runs the
  Parakeet / Nemotron TDT and RNN-T decoder on the GPU for every Vulkan
  device, the integrated Intel GPU included, where it is slower than the CPU
  decoder it replaced. Turning only that off restores the old numbers. Fix
  belongs in the engine: `session_device_decoder` in
  `src/arch/parakeet/model.cpp` should leave a
  `GGML_BACKEND_DEVICE_TYPE_IGPU` device on the host decoder by default
  (ggml's Vulkan backend reports it from `is_integrated_gpu`), keeping
  `TRANSCRIBE_PARAKEET_DEVICE_DECODER=1` to force it. The app does not set
  the variable itself: it is process-wide, and a Multi-STT setup can run a
  Parakeet model on CUDA and another on the iGPU at once.
- **R2T2 streaming on Vulkan (RTX), measured 1-3 % slower above, has no
  code-level cause.** With `TRANSCRIBE_PERF_DEBUG=1` on the 35 s clip both
  pins do identical work (112 ticks, 674 steps, the same prefilled tokens);
  every GPU compute stage is 1-3 % up together, the step pipeline on or off
  makes no difference (tick time 9966 / 10034 vs 9725 ms), and the same old
  exe drifted 8554 → 10980 ms within the session as the laptop GPU heated up.
  An even spread across stages is the GPU's clock, not a changed path. CUDA
  is level. Side finding for the engine: on Vulkan the step pipeline gains
  nothing here (its event waits move into `tensor_get`, 106 → 302 ms, with
  the step loop unchanged).
