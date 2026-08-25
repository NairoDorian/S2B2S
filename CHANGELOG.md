# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- **Silero VAD was silently doing nothing; it now actually filters.** Voice
  activity detection has been a no-op: the `vad-rs` crate only speaks Silero
  **v4**'s tensor interface (`input`/`sr`/`h`/`c` in, `hn`/`cn` out), while the
  model Handy ships is **v6.2** (`input`/`state`/`sr` in, `output`/`stateN`
  out). Every inference call failed with `Invalid input name: h`, and because
  the recorder treats a VAD error as "keep this audio" — losing speech being
  worse than keeping silence — the failure produced no visible symptom. The
  `vad_enabled` setting appeared to work while every frame, speech or silence,
  was passed straight through to the decoder.

  `audio_toolkit/vad/silero.rs` now drives the ONNX graph directly through
  `ort` (the runtime already loaded in-process for Parakeet/Moonshine) and
  `vad-rs` has been dropped. Three parts of the v5/v6 contract are required,
  and each one fails quietly on its own — a model that runs without error but
  reports ~0.0005 for speech and silence alike:
  - the analysis window must be exactly 512 samples (32 ms at 16 kHz);
  - the previous window's last 64 samples must be prepended to each chunk, so
    the tensor actually fed is 576 long;
  - the packed `state` tensor must be carried forward from each run's
    `stateN` output and cleared between recordings.

  The capture pipeline is now framed to the VAD's native 32 ms rather than the
  previous 30 ms, so one resampled frame is exactly one VAD decision;
  `FRAME_MS` derives from `constants::VAD_FRAME_MS` instead of being chosen
  independently. Hangover, prefill and onset are all frame counts, so their
  wall-clock spans shift accordingly (the streaming hangover tail goes from
  1.65 s to 1.76 s).

  Verified against a real 33.8 s recording: 1057 frames, zero errors, 24.6 s of
  raw voiced audio and 32.9 s kept once the hangover tail is included — where
  before, all 33.8 s was kept because nothing was ever filtered. Behaviour
  change to expect: recordings are now shorter and carry less silence, which
  reduces decode time and the silence-driven hallucinations Whisper-family
  models are prone to, but it is a genuine change to the audio the models
  receive.

  Two guards so this cannot regress invisibly: `SileroVad::new` validates the
  model's input names at load and fails loudly on a mismatch, and the recorder
  logs the first VAD failure of each recording instead of swallowing all of
  them. `src-tauri/tests/vad_speech_clock_probe.rs` is an opt-in regression
  check over a real recording (set `HANDY_PROBE_WAV`; skipped otherwise).

  Note: transcribe.cpp is not involved in VAD and has no VAD API of its own —
  its only Silero awareness is rejecting Silero `.bin` files if one is loaded
  as a Whisper model.

### Added

- **Speech statistics in the recording overlay.** Both the Minimal and Live
  overlays can now show whether you are speaking or paused, a timer that counts
  only while you are actually talking, and a running average words-per-minute.
  Driven by the raw per-frame Silero verdict rather than by the smoothed
  keep/drop decision, which stays true through a hangover tail up to 1.76 s and
  would otherwise bill more than a second of phantom speech per pause. Gaps
  shorter than the configured pause tolerance are counted as part of the same
  utterance once speech resumes, so the timer only ever moves forward. Words
  per minute is computed from the live streaming transcript, so it needs a
  streaming-capable model; other models show the timers and the indicator with
  a placeholder in place of the rate. Two new settings under
  Advanced → Transcription: `overlay_speech_stats` (on by default) and
  `speech_pause_hold_ms` (100–2000 ms, default 500).

- **Auto-detect and pull transcribe.cpp upstream changes.** A new
  `scripts/check-transcribe-deps.ts` guard runs before every
  `bun run tauri` invocation (wired into the `tauri` npm script in
  `package.json`). It compares the commit pinned in `Cargo.lock` for the
  `transcribe-cpp` / `transcribe-cpp-sys` git dependencies (forked from
  `NairoDorian/transcribe.cpp`, branch `main`, applied via
  `[patch.crates-io]`) against the remote `main` branch tip via
  `git ls-remote`. When the remote is ahead, it runs
  `cargo update -p transcribe-cpp -p transcribe-cpp-sys` to fetch the latest
  commit, so the next `tauri dev` / `tauri build` automatically recompiles
  the native C++/CMake crate from the new source rather than silently
  reusing a stale cached build. The check is safe by design: it always
  exits `0`, so it never blocks a build, even when offline.
