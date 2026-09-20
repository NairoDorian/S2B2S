# Upstream Handy measurement branch

`Handy_benchmarks` starts at upstream main
`8f9cf53cd1410cda26beea39ff802ac306e39585`, preserving upstream history.
This branch adds headless benchmark measurements only. It retains upstream
inference, capture, settings and logging behavior outside benchmark mode.
It is a worktree of the existing NairoDorian fork.

Build the sibling `transcribe_benchmarks` with its benchmark build script first.
Then build this app against that upstream native install:

```powershell
$env:TRANSCRIBE_DIR = (Resolve-Path ../transcribe_benchmarks/build/bench-native/install).Path
cargo build --manifest-path src-tauri/Cargo.toml --bin handy
bun scripts/bench-stt.ts --exe src-tauri/target/debug/handy.exe `
  --wav ../transcribe_benchmarks/samples/jfk.wav --output build/bench/upstream
```

No weights are downloaded. The script discovers installed canonical GGUF
models and CPU/CUDA devices. Use repeated `--model <id>` flags for a subset.
The default subset is Granite 4.1 2B Q4, Qwen3 1.7B/0.6B, Nemotron 3.5 Q6/Q8,
and Parakeet TDT v3 Q4/Q8. Nemotron is measured in batch and streaming modes.
Each model/mode/backend loads once and runs exactly three times; run 1 is
excluded and runs 2 and 3 are averaged. Immediate unloading is suppressed only
for the headless benchmark process. Cold load remains a separate metric.

Streaming replay feeds 16 ms chunks without microphone pacing, with right
context 6. It uses the app's loaded session and language plan, bypassing VAD,
denoise, UI and clipboard. First-text compute time is not live microphone
latency. Native stage timer definitions are those of the selected library.
Batch replay uses the normal upstream app transcription/postprocessing path.

Run comparisons sequentially on an idle machine. Match WAV, installed weights,
language, translation/custom-word settings, backend, build profile and power
state. JSON records the resolved language, WAV hash and native build identity.
`--baseline <summary.json>` rejects mismatched configurations, >15% slower warm
averages, and changed transcripts. Do not compare a different language plan or
quiet/busy machine state as an engine regression.

Reinstalling the native library can leave Cargo's staged DLLs stale when the
link manifest is unchanged. Before rebuilding a changed prebuilt install, run
`cargo clean --manifest-path src-tauri/Cargo.toml -p transcribe-cpp-sys` so
the upstream sys crate restages it. Use a separate target directory from ZER0.
