# STT performance investigation — 2026-09-20

The engine and app fixes are complete. The main regression was competing CPU
worker pools, compounded by affinity manipulation; CUDA inference still used
the affected CPU mel/decoder path. Both were fixed in the sibling engine.

This app removes process elevation, EcoQoS/timer overrides and MMCSS capture
registration. Runtime logging always captures every level to terminal/file.
The Debug console uses a bounded, UTF-8-safe file tail; severity chips only hide
rows. The capture-level selector was removed. Native build provenance and
capture/queue/feed/finalize/stage metrics were added. DLL staging now refreshes
development binaries when the prebuilt native install changes.

Use `bun run bench:stt -- --exe src-tauri/target/debug/zer0.exe --wav
../transcribe-fork/samples/jfk.wav --output build/bench/current` on one line.
Every model/backend/mode runs exactly three times with one loaded model; run 1
is excluded and runs 2 and 3 are averaged. CPU and CUDA are both required.
No weights are downloaded. See [benchmark instructions](STT_BENCHMARKS.md).

The rebuilt app passed 407 Rust tests (one ignored); frontend typecheck/lint
and script bundling passed. Both apps completed 14-case replay suites. The
final Qwen default was then changed from four to five workers based on the
explicit thread sweep, and the app was rebuilt. No further tests or benchmarks
were started after the user's instruction to conclude.

CUDA graphs help Granite but did not consistently help Nemotron/Parakeet.
The compatible audio.cpp optimizer had no consistent gain. Both stay disabled
by default; the experiments remain reproducible in the engine fork. No CPU
pinning, P/E-core selection or elevated scheduling is restored.

Detailed timings, upstream comparisons, causes, candidate optimizations and
limitations are in the engine's [investigation](https://github.com/NairoDorian/transcribe.cpp/blob/main/docs/nemotron-performance-investigation.md)
and [CUDA/optimizer experiments](https://github.com/NairoDorian/transcribe.cpp/blob/main/docs/cuda-graph-experiments.md).
The benchmark-only Handy branch is `Handy_benchmarks` (commit `6c7311f0`).
Previously provided reference directories remain unchanged.

The app lockfile now selects engine commit 28a3f9851d413c4bb9ce379934e92b5868831890. Local development can use the rebuilt prebuilt install by setting `TRANSCRIBE_DIR` in the shell (see [STT_BENCHMARKS.md](STT_BENCHMARKS.md)): `bunfig.toml` sets `env = false`, so an `.env.local` is not loaded. No machine path is committed.

An existing conflict-resolved upstream README/sponsor merge was completed in a separate commit before the STT commit; its content was preserved.
