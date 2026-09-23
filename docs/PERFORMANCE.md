# Performance and latency rules

ZER0 is a real-time tool: a hotkey press must feel instant, the transcript must
land before the user looks up, and nothing the settings window does may steal
time from the audio thread. Every change in this project is judged against this
file first. **Read it before adding a feature, a setting, a poll, an event, a
dependency or a thread.**

> Before committing, `bun run precommit` (see [AGENTS.md](../AGENTS.md#the-pre-commit-routine)).
> Shell commands go through `rtk` — except `bun`, which is never proxied — and
> dependencies are updated with `bun run update-deps -- --prerelease`: this
> project tracks the newest published version of every dependency on purpose.

## The budget

| Path                            | Target                                           | Where it is measured                                                     |
| ------------------------------- | ------------------------------------------------ | ------------------------------------------------------------------------ |
| Hotkey → first captured sample  | < 30 ms warm mic, < 150 ms cold open             | `first captured samples … after Cmd::Start` debug log                    |
| Audio callback                  | allocation-free, lock-free, log-free             | `write_input_to_ring` — never touch it casually                          |
| Consumer thread per 16 ms frame | ≪ 16 ms (VAD ≈ 30 µs, RNNoise ≈ 100 µs)          | `tests/vad_speech_clock_probe.rs`, debug timings                         |
| Stop → text pasted (batch)      | model bound; everything else < 20 ms             | Statistics page latency distributions                                    |
| Stream → overlay text           | one frame; events ≤ ~30 Hz                       | `StreamTextEvent`, `VadTestEvent` throttles                              |
| LLM post-processing / merge     | provider bound; local llama.cpp, warm            | `post_processing_latency_ms` in history                                  |
| Settings UI interaction         | no synchronous Tauri call on the main thread     | commands are `async` + `spawn_blocking`                                  |
| Live FFT frame                  | ≤ 0.5 ms DSP at N = 32768 on the worker; 5–60 Hz | `dsp_us` in the `live_fft_frame` header, `dropped_samples` in the status |

## Rules

1. **Audio thread first.** `write_input_to_ring` is the only code on the
   platform audio thread. It must not allocate, lock, log, read a clock or
   make a syscall. Everything else happens on the consumer thread that drains
   the ring.
2. **Per-frame work is atomic reads, not locks.** Hot toggles (VAD threshold,
   noise suppression, the live VAD test flag) are `Arc<AtomicBool>` /
   in-place swaps read once per chunk or frame. Never rebuild a recorder or
   reopen the microphone to change a parameter that a detector can take in
   place.
3. **Never block the webview thread.** Every `#[tauri::command]` that touches
   a mutex the audio manager can hold across a device open/close, or that
   does I/O, runs through `tauri::async_runtime::spawn_blocking`. `is_recording`
   reads an atomic mirror of the state, not the state mutex.
4. **Throttle events to the UI.** The overlay polls its scope frame at the
   analyser's update rate and gets speech activity only on flips plus a 150 ms
   heartbeat; the live VAD test reports every second frame (~31 Hz). A new
   event stream needs a stated rate and a gate that keeps it silent when its
   window is not showing (see the issue #1279 notes in `overlay.rs`).
5. **Lazy, warm, resident.** Models stay loaded per `model_unload_timeout`;
   the microphone stays open per the idle timeout; the denoiser chain is
   built on first use; Multi-STT pre-loads its extra models while the user is
   still recording; the local LLM server is kept running between requests.
   Prefer a warm idle resource over a cold start on the critical path.
6. **Parallel where independent.** Multi-STT runs its models concurrently on
   the blocking pool; history WAV writes and paste run concurrently with
   inference; the noise chain and the direct chain are separate objects so a
   toggle never waits on a rebuild. Serialize only what shares a resource.
7. **Long-lived child processes run detached on their own thread.** The
   llama.cpp server is spawned by a supervisor thread that owns the child,
   captures its stdout/stderr line by line into a 400-line ring buffer shown
   on the Local LLM page (not the app log), polls readiness on
   `/health` with backoff, and kills it on app exit. The request path only
   ever talks HTTP to a server that is already warm.
8. **System meters must be cheap.** CPU / RAM / GPU readings come from one
   sampler thread on a 1 s tick (NVML for the GPU when present), published
   through an event; the UI never polls a command per render.
9. **No work for hidden UI.** Pages unmount when not shown; stores keep only
   what must survive navigation. A component that subscribes to a stream
   unsubscribes on unmount and stops any backend activity it started (the
   live VAD test stops itself).
10. **Measure before and after.** Use the Statistics page, the debug timing
    logs and the opt-in probes (`ZER0_PROBE_WAV` — the prefix is
    `app_identity::ENV_PREFIX`, so read it rather than copy it if it ever
    changes). A change that adds a dependency, a thread or a poll states its
    cost in the commit message.

## Startup order (measured from the dev log, 2026-09-11)

`initialize_core_logic` used to run: model registry → always-on microphone
open (**891 ms, synchronous**) → history DB → transcribe.cpp backend init →
meters, and the hotkeys only registered when the webview mounted and
called `initialize_shortcuts` (**~8 s after launch** in dev). Now the
microphone opens on a `mic-open` thread, the hotkeys and Enigo are
registered at the end of core startup on Windows/Linux (macOS keeps the
permission-driven order), and the log prints `Core startup done in …`.
Anything added to startup goes after that line or on its own thread.

## Known costs to keep in mind

- The Live FFT tap costs the audio consumer thread one atomic load per
  chunk while the page is closed and a `try_lock` + memcpy into a 768 KB ring
  while it runs (never a wait). The `live-fft` worker exists only during a
  session; each frame is encoded once into a binary slot (32-byte header +
  `output_bins` raw f32, ≈ 4 KB at 1024 bins) that the page polls with
  `live_fft_frame(known_seq)`: a poll that finds nothing new costs a 32-byte
  reply, and the page stops polling while frozen, stopped or hidden. The axis
  is fetched separately, once per axis change. Output bins go up to 65536
  (256 KB per changed frame) — the cost of the page's own choice, not of the
  overlay, which is capped at 8192. Inline mode (`async_analysis` off) moves
  the transform onto the consumer thread on purpose and is bounded by the
  same 16 ms frame budget; every lock it takes there is a `try_lock`.
- The recording overlay's scope (`live_fft::scope`) is the same tap and
  pipeline on an `overlay-scope` thread that exists only while the overlay
  shows a recording; it always runs off the audio thread whatever
  `async_analysis` says. The overlay polls `overlay_scope_frame` at
  `update_rate_hz`: one memcpy per changed poll of the bins (at most 8192,
  `OVERLAY_MAX_BINS`) plus the waveform window as raw f32 (no JSON, no event;
  an unchanged seq returns the 32-byte header only): ~20 KB with 1024 bins and the default 4096
  samples, ≈ 600 KB/s at 30 Hz; `overlay_scope.wave_samples` can raise the
  window to 16384 samples (64 KB per poll). The 16-bucket level
  meter it replaced has been removed from the recorder.
- FFT resampling adds ~20 ms of buffering per stage; the denoise chain is
  two stages when the microphone is not 48 kHz. Prefer a 48 kHz device.
- The first RNNoise frames after a reset are transient; the chain is reset
  only on recording start and on toggle, never per chunk.
- transcribe.cpp model load is the dominant cold cost; `Immediately` unload
  trades memory for a multi-second load on every dictation.
- The experimental Multi-STT streaming mode's per-break cost is the one place
  where a session-length dependency would be fatal to the GPU, so it is bounded
  by construction: the extras decode `multi_stt_streaming_context_chunks + 1`
  chunks (at most four) plus one short LLM call **per pause**, and the audio
  handed over is a sliding window of the last few chunks rather than everything
  spoken so far. A thirty-minute dictation therefore costs the same at each
  pause as the first one, and the VRAM the extra engines hold for a decode is a
  function of the window, not of the session. Memory follows the same window:
  `context_chunks + 1` chunks of audio plus one pending retry, ≈3.8 MB per 60 s
  chunk at the valve, freed as soon as a chunk can be neither context nor
  retried. The primary model's stream is what is on screen and is never
  re-transcribed; only the window is.
- Local llama.cpp with MTP speculative decoding (`--spec-type draft-mtp`) is
  the fastest configuration measured for the merge/clean prompts; keep the
  draft model on the same device as the main model.

## Live FFT: measured before/after the Plugin_FFT port (2026-09-23)

i9-13900H, CPU lane (`dev:cpu` posture), dev profile with opt-level 3 for the
app crate and the FFT crates. The pre-port pipeline and the current one ran in
one binary, interleaved ABBA, 8 rounds of 300 frames (after 60 warm-up
frames) per configuration, median per round. Three separate processes, each
pinned to one core at high priority; every round's ratio stayed within ±4 %
of its run's median. Numbers are the median of the three runs, µs per
`process()` at 48 kHz.

| Configuration                                               | Before | After | Speed-up |
| ----------------------------------------------------------- | ------ | ----- | -------- |
| N 32768 (the previous default), same work (aggregation off) | 82.7   | 44.3  | ×1.84    |
| N 32768 (the previous default), peak aggregation            | 82.7   | 47.1  | ×1.75    |
| N 8192, cubic, 2048 bins                                    | 32.2   | 11.3  | ×2.85    |
| Full chain: EQ + A-weighting + dB AGC + ballistics          | 80.8   | 42.7  | ×1.87    |
| Mel, Hann, N 4096, 256 bins                                 | 11.7   | 4.7   | ×2.49    |
| Digital silence, defaults (dB + ballistics)                 | 80.8   | 0.3   | ×~270    |
| Frame encode, 1024 bins (JSON event → binary slot)          | 150.5  | < 0.1 | —        |
| Frame encode, 8192 bins                                     | 1216.4 | 0.5   | ×~2400   |

The encode rows time only the serialisation; the old path also paid Tauri's
event IPC and a `JSON.parse` plus a `Float32Array` copy in the webview, which
the binary poll does not. Timings are quantised to the 0.1 µs timer, so the
silence and small-encode ratios are approximate. Rows that name no N ran at
N 32768, the default at the time (the default is now Plugin_FFT's 16384; not
re-measured), and that configuration is dominated by its 32K transform. The
per-kernel A/B results of the v2.12 port (kept and rejected) are recorded
with the change.

## Scheduling and repeatable STT measurements

Use normal OS scheduling. Do not identify or rank P/E cores, exclude core 0,
set CPU affinity, raise process/thread priority, register an elevated MMCSS
task, or override power throttling. Thread counts respect the CPUs available
to the process without assigning work to particular cores.

The STT benchmark policy is exactly three runs on one loaded model: discard
run 1, then average runs 2 and 3. Both CPU and CUDA are measured. See
[STT_BENCHMARKS.md](STT_BENCHMARKS.md) for per-model and installed-subset commands,
regression budgets, raw evidence, and the distinction between unpaced replay
and live microphone latency. No baseline should be collected concurrently
with a build or another inference process.

Capture metrics add two clock reads per drained chunk and two per output
frame on the consumer thread, plus one summary at stop. Live stream queue
metrics add one enqueue timestamp per message. The platform capture callback
has no new work; there are no new threads or dependencies. Debug log polling
exists only while its page is mounted and has one bounded read in flight.
