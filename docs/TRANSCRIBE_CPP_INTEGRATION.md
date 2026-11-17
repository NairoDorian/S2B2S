# ZER0 and transcribe.cpp: paired development contract

Last audited: 2026-10-16. ZER0 and the local transcribe.cpp fork are developed
together. Inspect the engine's implementation when changing capture-to-inference
behavior, and inspect ZER0's real use when changing the engine. Do not infer the
consumer contract from an upstream README or an old plan alone.

## Local peer discovery and branch safety

- App: `C:/Users/Z/Downloads/PROJECTS/Handy_V2`.
- Engine: normally `../transcribe-fork`, or
  `C:/Users/Z/Downloads/PROJECTS/transcribe-fork`.
- A managed feature worktree may live elsewhere. Use `git worktree list` in the
  engine to find `codex/vad-integration` before reading VAD development; the
  canonical folder may still be on main with unrelated uncommitted work.
- Read both `AGENTS.md` files, engine `CONTRIBUTING.md`, app
  [performance rules](PERFORMANCE.md), and the engine's
  `docs/zer0-engine-integration.md`. That note is the reciprocal source map.
- Read the engine's
  `docs/plans/2026-10-three-tier-super-vad-and-firered-architecture-plan.md` for
  current VAD/RNNoise work, instead of treating September prose as current code.
- Inspect both branch/dirty states before editing. Preserve unrelated work and
  do not switch, reset, merge or update the peer merely to inspect it. If it is
  absent, report the boundary assumptions that remain unverified.

## Files to read together

| Change                                        | App sources                                                                                  | Engine sources                                                                                                  |
| --------------------------------------------- | -------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| Stream defaults / API / lifecycle             | `src-tauri/src/managers/transcription.rs`, all StreamOptions builders                        | `include/transcribe.h`, `src/transcribe-asr.cpp`, Rust `streaming.rs` / `session.rs`                            |
| VAD policy and timing                         | `managers/audio.rs`, `audio_toolkit/vad/{mod,gate}.rs`, SpeechClock in `audio/recorder.rs`   | `src/transcribe-vad-gate.*`, `src/transcribe-earshot*`, Rust `earshot.rs` (`VadGate`, `score_frames`)           |
| Noise suppression / sample rate / final tails | `audio_toolkit/audio/{denoise,resampler,recorder}.rs`                                        | `src/transcribe-denoise.cpp`, `src/transcribe-rnnoise.*`, Rust `denoise.rs` (`Denoiser`); ASR takes 16 kHz mono |
| Pause/chunk/text ownership                    | `multi_stt_stream.rs`, `multi_streaming.rs`, `live_mode.rs`, `file_transcription.rs`         | Stream cursors, finalize behavior and family-specific silence handling                                          |
| Diagnostics / history                         | `live_fft/`, `managers/history.rs`, `settings.rs`, `shortcut/mod.rs`                         | Raw scores, voiced/retained states and their distinct telemetry contracts                                       |
| Built dependency                              | `src-tauri/Cargo.{toml,lock}`, `scripts/check-transcribe-deps.ts`, `native_build_identity()` | Public ABI hash, generated bindings, native install and model composite options                                 |

Before the VAD merge, the engine's embedded Earshot occupied
`src/transcribe-vad.*`. The VAD integration separates it into
`src/transcribe-earshot.*` so the upstream Silero role can coexist. Locate symbols
with `rg` if the engine checkout has another layout.

## Verified current behavior and outstanding work

At audit time app HEAD was `51a69d55` on `Handy_Multi_STT`. Its committed lock
referenced engine `ba949120`; the dirty working lock referenced `468e8776`.
Neither implies that a running executable was rebuilt against that revision.
`TRANSCRIBE_DIR` / `TRANSCRIBE_PREBUILT_DIR` can select a prebuilt native install,
and the Tauri runner can refresh git pins. Record actual build provenance for
comparisons; do not change pins as a side effect of a documentation task.

Capture currently owns mono conversion, optional 48 kHz RNNoise, 16 kHz framing,
Rust Earshot and SmoothedVad, the speech clock and UI taps. Native ASR streams
use `..Default::default()`, whose engine VAD default is true: capture filtering
can be followed by another Earshot evaluation inside each engine stream. The
app VAD toggle and sensitivity slider currently control the Rust detector;
engine telemetry is not the source of the app's speaking indicator or WPM.
The ownership fix remains planned, not implemented by these notes.

RNNoise's probability gate, Earshot's keep/drop policy and semantic endpointing
are three different operations. Suppression defaults off; when enabled it uses
480-sample / 10 ms frames at 48 kHz. Strength zero still runs the network and
any enabled probability gate. Preserve dry/wet alignment, tails, resets, raw
audio, FFT tap points and live settings when changing this chain.

SmoothedVad's circular buffer is already preallocated. Remaining candidates are
allocating Rubato output plus another copy in `FrameResampler::process_chunk`,
and per-model `frame.to_vec()` calls in `StreamRouter::feed`. Measure these
before attributing speedups to a language change or stacking more VAD models.

The app's 450 ms pre-roll, 60 ms onset and 450/1650 ms offline/streaming tails
round up to whole Earshot frames. Its speech clock defers short pauses and only
counts them when speech resumes. Embedded engine telemetry has different
hangover accounting. History re-analysis at threshold 0.5 is another policy;
imported files do not currently use the microphone denoise/VAD chain.

Upstream Silero VAD was merged in `c63b18e2` on 2026-10-10. Its standalone role
supports streaming probabilities with partial-frame buffering and offline
segments; the latest public API has no live START/END iterator. The local
`codex/vad-integration` engine worktree prepares that code on top of fork main
`692350de`, without committing. FireRed, Smart Turn, shared engine pre-roll and
the RNNoise frontend remain proposals. Read current Git status before assuming
any of these changes have landed or that the app consumes them.

## Working rules across the pair

1. Trace capture -> preprocessing -> queue -> ASR feed/finalize -> displayed text
   and telemetry, in both projects, before editing defaults or moving work.
2. Give each transform one owner per source; share prepared audio across models.
   Keep original-time and retained-audio coordinates explicit. A late detector
   cannot recover speech discarded earlier.
3. Preserve UI controls, source taps, cancellation, error handling, final tails,
   WPM monotonicity, Live Mode and Multi-STT chunk ownership. Preserve R2T2's
   documented silence-skip opt-out.
4. Keep C ABI additions compatible and regenerate/review all bindings. Test the
   app against the actual new native build, not merely an unchanged Cargo pin.
5. Update this note, the engine's reciprocal note, and the active plan together
   when responsibility or behavior changes. Label proposals and historical
   measurements; do not promise zero clipping/hallucinations or mobile battery
   savings without corresponding evidence.
6. Follow each repo's verification policy for the changed layer. A headless ASR
   replay may bypass microphone/VAD/RNNoise/UI; use capture/queue metrics and
   paired raw/processed quality checks for preprocessing claims.

The [September plan](2026-09-native-earshot-vad-and-zer0-integration-plan.md) is
historical context. This note corrects its former claim that engine VAD was
unused and its unverified UI/WPM parity promises. New work follows the revised
plan in the local engine checkout.

## Research follow-up: 2026-10-16

The engine VAD worktree now carries revision 8 of the roadmap, plus
`docs/research/2026-10-vad-preprocessing-evidence.md` and
`docs/plans/2026-10-voxlock-voice-isolation-plan.md`. Read them before changing
preprocessing. The source-level review found different Rust/C++ hysteresis and
hangover rounding, toggle resets that can discard pending audio, and unbounded
per-model feed channels. A deterministic source probe separates buffer delay
from compute time; it is not a ZER0 ASR performance claim.

VoxLock is an optional enrolled-speaker waveform-isolation candidate. Its live
auto-enrollment initially passes through audio, so short dictation needs an
explicit pre-enrolled profile if isolation is promised from the start. Keep
profile/UI/storage in the app and share one native audio context across all
ASR models. File/meeting workflows must opt in rather than lose other speakers
implicitly. No runtime, dependency, profile feature or audio default was changed
by this research update. The plan records feasibility and native port gates.

The revision 9 follow-up adds the engine's
`docs/plans/2026-10-audio-frontend-contract.md`: read it for physical/prepared/
retained/text timelines, capture gap positions, the recorder Stop handshake,
model locks and skip-aware acceptance. Preserve Multi-STT's text/drain gate
when adding semantic endpointing. The VoxLock plan now includes an executed
identity-stub wrapper probe; this is not a real-model quality result.

## Update: 2026-11-14 (engine side)

Read [the November handoff](2026-11-transcribe-fork-handoff.md) first. Since
the audit above: `codex/vad-integration` was committed, merged into engine `main`
and deleted, so Silero (`transcribe_cpp::vad`) and the engine Earshot
(`transcribe_cpp::earshot`) both live on `main`; the engine took upstream 0.4.2
and ggml 1.0, gained a language allowlist (`RunOptions::allowed_languages`,
`allowlist_letters`) and a CPU Whisper fix, and is pushed at `63fd4906`. No app
code changed. One measured VAD fact for the ownership decision above: the
engine stream gate (enabled by default for ZER0's streams) drops a whole quiet
utterance (−62 dBFS RMS) that streams correctly with the gate disabled —
details and the recommended single owner are in the handoff, §3.

## Update: 2026-11-15 (app side, plan only)

The user asked for the app work. The ordered plan is
[`2026-11-zer0-update-master-plan.md`](2026-11-zer0-update-master-plan.md):
engine bump to `cfb07a12` (Phase 2), `VadGate` + `Denoiser` + `score_frames`
replacing the Rust `earshot` / `nnnoiseless` layers with `enable_vad = false`
on every native stream (Phase 5), and "languages I speak" through
`RunOptions::allowed_languages` plus upstream Handy's spoken-language detector
(Phase 6). No app code changed with this note; update this section as each
phase lands.

## Update: 2026-11-17 (app side, Phase 5 landed)

The engine owns VAD and noise suppression; the paragraphs of the 2026-10-16
audit about Rust Earshot, `SmoothedVad` and the double gate are history.

- Capture: one `transcribe_cpp::VadGate` per recorder (`SpeechGate`, fed one
  256-sample frame per call), its threshold moved in place by the slider and
  its hangover set per recording (450 / 1650 ms). Exit threshold, onset and
  pre-roll are passed explicitly. ZER0's `SpeechClock` still counts the gate's
  raw `voiced` verdict; the gate's own clock is not read.
- Noise suppression: `transcribe_cpp::Denoiser` inside `DenoiseChain`; params
  cross the FFI only when a slider moved; `flush` replaces the silent frame.
- Streams: `StreamOptions { enable_vad: false, .. }` at the live builder,
  `benchmark_stream` and `replay_stream_once`. The engine's per-stream silence
  gate no longer runs on audio the app already filtered.
- History re-analysis: `transcribe_cpp::score_frames` + the same
  two-threshold rule (`vad::voiced_ms`).
- Removed crates: `earshot`, `nnnoiseless`.
- Behaviour change: no −45 dBFS pre-gate; the quiet FLEURS French clip
  (−62 dBFS RMS) reads 76 % voiced through `vad_speech_clock_probe`, where the
  old pre-gate scored every frame 0.
