# Performance and latency rules

Handy is a real-time tool: a hotkey press must feel instant, the transcript
must land before the user looks up, and nothing the settings window does may
steal time from the audio thread. Every change on this fork is judged against
this file first. **Read it before adding a feature, a setting, a poll, an
event, a dependency or a thread.**

## The budget

| Path                            | Target                                           | Where it is measured                                             |
| ------------------------------- | ------------------------------------------------ | ---------------------------------------------------------------- |
| Hotkey → first captured sample  | < 30 ms warm mic, < 150 ms cold open             | `first captured samples … after Cmd::Start` debug log            |
| Audio callback                  | allocation-free, lock-free, log-free             | `write_input_to_ring` — never touch it casually                  |
| Consumer thread per 16 ms frame | ≪ 16 ms (VAD ≈ 30 µs, RNNoise ≈ 100 µs)          | `tests/vad_speech_clock_probe.rs`, debug timings                 |
| Stop → text pasted (batch)      | model bound; everything else < 20 ms             | Statistics page latency distributions                            |
| Stream → overlay text           | one frame; events ≤ ~30 Hz                       | `StreamTextEvent`, `VadTestEvent` throttles                      |
| LLM post-processing / merge     | provider bound; local llama.cpp, warm            | `post_processing_latency_ms` in history                          |
| Settings UI interaction         | no synchronous Tauri call on the main thread     | commands are `async` + `spawn_blocking`                          |
| Live FFT frame                  | ≤ 0.5 ms DSP at N = 32768 on the worker; 5–60 Hz | `dsp_us` in `LiveFftFrameEvent`, `dropped_samples` in the status |

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
   pipes its stderr to the log at line granularity, polls readiness on
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
    logs and the opt-in probes (`HANDY_PROBE_WAV`). A change that adds a
    dependency, a thread or a poll states its cost in the commit message.

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
  session; each frame is a JSON event of `output_bins` floats (≈ 10 KB at
  1024 bins, ≈ 300 KB/s at 30 Hz), sent to the main window only and skipped
  while it is hidden. Inline mode (`async_analysis` off) moves the transform
  onto the consumer thread on purpose and is bounded by the same 16 ms frame
  budget.
- The recording overlay's scope (`live_fft::scope`) is the same tap and
  pipeline on an `overlay-scope` thread that exists only while the overlay
  shows a recording; it always runs off the audio thread whatever
  `async_analysis` says. The overlay polls `overlay_scope_frame` at
  `update_rate_hz`: one memcpy per poll of the bins plus the waveform window
  as raw f32 (no JSON, no event): ~20 KB with 1024 bins and the default 4096
  samples, ≈ 600 KB/s at 30 Hz; `overlay_scope.wave_samples` can raise the
  window to 16384 samples (64 KB per poll). The 16-bucket level
  meter it replaced no longer runs (no callback is registered).
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
