# Logging

Runtime records are captured at **Trace** in development and release builds.
The Debug console's severity chips only hide rows; they never change capture.
The old `log_level` setting is retained for settings compatibility but has no
effect. `RUST_LOG` does not filter runtime capture: inherited shell settings must not
silence the durable console. Hide records with the viewer chips.

## Destinations

- Interactive runtime: stdout (visible under `bun run tauri dev`).
- Headless commands: stderr, keeping JSON stdout parseable.
- File: the same records, with a 10 MB rotation cap and one current file.
  `portable::app_log_dir` resolves its location; the Debug page can open it.
- In-app console: polls the last 2,000 file lines every 500 ms while mounted.
  No polling or log events when the page is closed. Only one read is in flight.

The viewer uses a single durable source. The previous live-event/file merge
could delete a live record and then mistakenly consider its file copy already
present. Removing that merge also removes its quadratic reconciliation work.
Repeated identical records survive. Reads run on the blocking pool, are byte
bounded, and tolerate a tail boundary inside a UTF-8 character.

Pause freezes the view; resume fetches the current tail. All severity chips
start enabled and can be toggled independently. Clear explicitly truncates the
current file. Rendering retains unchanged line objects to avoid rebuilding
2,000 rows on every poll. The file retains older records until rotation; the
viewer is a bounded tail, not an unlimited history.

Build output from Bun/Cargo/Vite belongs to those parent processes and does not
pass through the app logger. Runtime Rust records and the transcribe.cpp logging
callback do. The file and panel show runtime diagnostics, including native
model messages, without claiming to capture the parent build tools.

## Pipeline records

Search for the `pipeline` target:

- Native version/commit and executable path at initialization.
- Model/backend, effective run/stream options and stream begin time.
- Stream queue maximum wait, maximum feed/finalize call, audio duration,
  compute time, finalize time and native mel/encoder/decoder counters.
- Batch native mel/encoder/decoder counters and audio duration.
- Capture consumer totals, VAD/routing callbacks, frontend/tap work, maximum
  chunk time, tail-flush time, dropped samples and VAD errors.

Native Parakeet chunk debug records break down graph building, scheduler
allocation, graph compute, readback, cache rotation, decoder, and remaining
host work. `graph_compute` can be CPU or CUDA; the record names the backend.
Enable `TRANSCRIBE_PERF_DEBUG=nodes` only for separate diagnostic runs: its
synchronizing per-node profiler changes timing and must not produce baselines.

## Cost

The logger already writes synchronously to terminal/file; full capture can
increase I/O. There is no per-record webview event. Capture instrumentation
adds clock reads on the consumer thread (two per drained chunk and two per
output frame), counters, and one summary at stop; the platform audio callback
stays unchanged. Queue timing adds one timestamp to each routed audio message.
No new threads or dependencies were added.
