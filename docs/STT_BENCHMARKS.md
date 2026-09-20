# STT regression benchmarks

Run locally against installed models only. No script downloads weights.
Both CPU and CUDA are required by the suites; an unavailable backend is a
recorded failure rather than a silent fallback or substituted CPU result.

Every case loads a model once and runs exactly **three** inferences on that
resident model. Run 1 is warm-up, retained only as raw evidence. Scores are the
arithmetic mean of runs 2 and 3. Cold load is separate. The headless app
overrides immediate unload for this process only; settings are not changed.

```powershell
bun scripts/bench-stt.ts --exe src-tauri/target/debug/zer0.exe `
  --wav ../transcribe-fork/samples/jfk.wav --output reports/bench/current
```

The default subset is installed Granite Speech 4.1 2B Q4, Qwen3-ASR 1.7B/0.6B,
Nemotron 3.5 Q6/Q8, and Parakeet TDT 0.6B v3 Q4/Q8. Pass `--model <id>` one or
more times for an exact subset. Nemotron is tested offline and streaming;
other profiles use their offline API. Streaming uses 16 ms feeds and right
context 6. Effective language and stream options are included in the results.
Device indices are discovered on each run, never hardcoded.

For one model use the executable directly:

```powershell
src-tauri/target/debug/zer0.exe --transcribe-file ../transcribe-fork/samples/jfk.wav `
  --model <installed-model-id> --device-index <index-from-list-devices> --repeat 3 --json
# For Nemotron, append --stream-chunk-ms 16 --stream-att-right 6
```

Each case has a JSON result and stderr log. `summary.json` contains results
and failures, including timeouts. `--baseline <summary.json>` fails on >15%
slower warm mean (`--max-regression` changes this budget), changed transcripts,
or mismatched backend/audio duration. Review noisy measurements with repeated
full three-run cases; never replace a score with its fastest run. Keep the same
WAV, language/settings, quant, device, power state and native build when
evaluating an app-only change. Do not run builds or GPU work during benchmarks.

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

The diagnostic install for this investigation includes the existing CUDA
module built from the same fork GGML source. A fresh general-purpose install
should build CUDA itself. Check `--list-devices` and startup native provenance.
Do not infer the loaded DLL from Cargo.lock alone. Set `TRANSCRIBE_DIR` only in
the development shell; do not commit machine paths. The native suite at
`transcribe-fork/scripts/bench/suite.py` accepts an exact `--library` and keeps
an independent engine baseline without the app.
