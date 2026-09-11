# Performance and latency rules

Handy is a real-time tool: a hotkey press must feel instant, the transcript
must land before the user looks up, and nothing the settings window does may
steal time from the audio thread. Every change on this fork is judged against
this file first. **Read it before adding a feature, a setting, a poll, an
event, a dependency or a thread.**

## The budget

| Path                            | Target                                       | Where it is measured                                  |
| ------------------------------- | -------------------------------------------- | ----------------------------------------------------- |
| Hotkey → first captured sample  | < 30 ms warm mic, < 150 ms cold open         | `first captured samples … after Cmd::Start` debug log |
| Audio callback                  | allocation-free, lock-free, log-free         | `write_input_to_ring` — never touch it casually       |
| Consumer thread per 16 ms frame | ≪ 16 ms (VAD ≈ 30 µs, RNNoise ≈ 100 µs)      | `tests/vad_speech_clock_probe.rs`, debug timings      |
| Stop → text pasted (batch)      | model bound; everything else < 20 ms         | Statistics page latency distributions                 |
| Stream → overlay text           | one frame; events ≤ ~30 Hz                   | `StreamTextEvent`, `VadTestEvent` throttles           |
| LLM post-processing / merge     | provider bound; local llama.cpp, warm        | `post_processing_latency_ms` in history               |
| Settings UI interaction         | no synchronous Tauri call on the main thread | commands are `async` + `spawn_blocking`               |

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
4. **Throttle events to the UI.** The overlay gets `mic-level` at the
   visualizer's rate and speech activity only on flips plus a 150 ms
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

- FFT resampling adds ~20 ms of buffering per stage; the denoise chain is
  two stages when the microphone is not 48 kHz. Prefer a 48 kHz device.
- The first RNNoise frames after a reset are transient; the chain is reset
  only on recording start and on toggle, never per chunk.
- transcribe.cpp model load is the dominant cold cost; `Immediately` unload
  trades memory for a multi-second load on every dictation.
- Local llama.cpp with MTP speculative decoding (`--spec-type draft-mtp`) is
  the fastest configuration measured for the merge/clean prompts; keep the
  draft model on the same device as the main model.
