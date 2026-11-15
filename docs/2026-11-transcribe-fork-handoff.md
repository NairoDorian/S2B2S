# transcribe.cpp fork → ZER0 handoff (2026-11-14)

> **2026-11-15: the user has asked for this work.** The execution order, the
> upstream Handy intake and the decisions are in
> [`2026-11-zer0-update-master-plan.md`](2026-11-zer0-update-master-plan.md);
> this note stays the engine-side record and the source of the §2.3 and §3.1
> recipes that plan cites.

Written from the engine side for the next agent that works on ZER0. **Nothing in
this repository was changed by that work except this note and two pointers to
it** (`AGENTS.md`, `docs/TRANSCRIBE_CPP_INTEGRATION.md`). Updated later on
2026-11-14 with the engine's replacements for ZER0's VAD and denoise layers
(§3.1). Every app-side item
below is _to do when the user asks for it_ — do not start ZER0 work from this
note on your own.

- Engine checkout: `C:/Users/Z/Downloads/PROJECTS/transcribe-fork`, branch `main`,
  pushed to `github.com/NairoDorian/transcribe.cpp` at **`63fd4906`** (allowlist
  work, §1–2); the VAD/denoise work of §3.1 followed at **`cfb07a12`**.
- ZER0 consumes it as a git dependency on `branch = "main"`; the next build
  refreshes the pin (`scripts/check-transcribe-deps.ts`). At the time of writing
  the committed lock pins `ba949120` and the dirty working lock `468e8776`, so a
  rebuild picks up **everything** below at once.
- Engine-side plan with the full record: `docs/plans/2026-11-fork-master-plan.md`
  (§0 sync, §A language allowlist, §B VAD, §F ZER0 integration). Paired contract:
  `docs/zer0-engine-integration.md` in the engine, `docs/TRANSCRIBE_CPP_INTEGRATION.md` here.

---

## 1. What changed in the engine since ZER0's pin

| Area               | Change                                                                                                                                                                                                                                                                                | What ZER0 will notice                                                                                                                                                                                                                                             |
| ------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Upstream sync      | handy-computer/transcribe.cpp 0.4.1 → **0.4.2** merged by hand (Silero VAD role API, memory-mapped weights, conformer causal pre-encode without copies, Phonon-2, NeMo padding-frame drop #252, Parakeet long-audio attention #199, Qwen3/Moonshine/GigaAM/Granite-NAR memory bounds) | #252 changes a few Parakeet/Canary punctuation marks on long clips; full-attention Parakeet checkpoints (v3) use local attention above 20.5 s of audio (dictation below that is unchanged); CPU weights are memory-mapped (RSS accounting changes, not behaviour) |
| ggml               | v0.26 → **v1.0.0**, fork patches refreshed (`0101` CUDA pool trim, `0102` TQ1_G128 ternary, `0103` KV trim)                                                                                                                                                                           | **Granite 4.1 2B encode ~2× faster on CUDA** (graph splits 38 → 6); Qwen3-ASR may move one comma on a few clips                                                                                                                                                   |
| Whisper            | Fix: the CPU static decode read a clobbered cross-attention mask — **every CPU Whisper transcript on the old engine was noise** (CUDA was fine)                                                                                                                                       | Whisper on CPU works again                                                                                                                                                                                                                                        |
| Language allowlist | New API, §2 — the main new capability                                                                                                                                                                                                                                                 | Nothing until the app passes a list                                                                                                                                                                                                                               |
| Rust API           | `RunOptions::allowed_languages`, `RunOptions::allowlist_letters` (`AllowlistLetters`), `Feature::LanguageAllowlist`; `transcribe_cpp::earshot` (engine Earshot) and `transcribe_cpp::vad` (Silero `VadSession`) modules                                                               | Source-compatible: every app `RunOptions` literal uses `..Default::default()`                                                                                                                                                                                     |
| ABI                | public ABI hash `0fa0d2838f92cca0`, architecture-plugin ABI 5                                                                                                                                                                                                                         | ZER0 compiles architectures in; nothing to do                                                                                                                                                                                                                     |
| Deps               | Rust/npm/Python/CI refresh in the engine                                                                                                                                                                                                                                              | none                                                                                                                                                                                                                                                              |

After the pin moves: run `bun run dev:fast` (the full check, see `AGENTS.md`);
close the app before `cargo build`/`cargo test` (it locks `transcribe.dll`).

---

## 2. Language allowlist — "the languages I speak"

### 2.1 What it is

`RunOptions::allowed_languages` names the languages **the speaker uses and may
switch between, mid-sentence included**. It is not "detect one of these and lock
onto it": every constraint is a union over the list. Two layers:

1. **The model's language decision** is restricted to the list — Nemotron's locale
   tags, the Qwen3-ASR / R2T2 `language X` prefix (plus `None`, the model's own
   answer for mixed speech), Whisper's window token, SenseVoice's LID tag — but
   only when the model can name **every** listed language; otherwise it stays free
   (a WARN), because forcing another label onto a language the model cannot name
   is worse than auto.
2. **The letters it may write**: no token carrying a letter none of the listed
   languages uses (another script, a foreign accent such as `ñ`/`ß`, a Cyrillic
   letter of a neighbouring language — Ukrainian vs Russian are separated). This
   is the only lever on models without a language decision (Parakeet TDT
   v3/ultra/redux, Granite). `allowlist_letters = AllowlistLetters::Any` turns
   this layer off (language decision only), so a foreign name keeps its spelling.

An explicit `language` always wins (the list is ignored). An empty list changes
nothing — transcripts are byte-identical to before (verified 35/35 + 21/21).

| Family (ZER0 models)                           | Language decision                                                                          | Letters                 |
| ---------------------------------------------- | ------------------------------------------------------------------------------------------ | ----------------------- |
| Nemotron 3.5 (offline + streaming)             | locale tags                                                                                | yes                     |
| Parakeet TDT v3 / ultra / redux                | —                                                                                          | yes                     |
| Qwen3-ASR 1.7B, R2T2 (offline + streaming)     | `language X` prefix (+ `None`)                                                             | yes                     |
| Granite 4.1 2B                                 | —                                                                                          | yes (off for translate) |
| Whisper, SenseVoice                            | window token / LID tag                                                                     | yes                     |
| **Canary 1B v2**, Cohere, Voxtral, Granite NAR | **not supported** — Canary/Cohere have no language detection (they need a source language) | —                       |

Error contract the app must respect (check before passing a list):

- a model **without** `Feature::LanguageAllowlist` returns `UnsupportedLanguage`
  for a non-empty list — Canary must keep its explicit language;
- a list naming **no** language the model knows also returns
  `UnsupportedLanguage` (e.g. Granite with `ru` only, Parakeet TDT with `ja`).

### 2.2 What it does, measured (engine CLI, CPU and CUDA)

- **Inside the list (the intended use)** it never made a correct result worse,
  including spliced fr→en, en→fr, de→en, fr→es→en, zh→en clips (every switch
  auto mode makes is kept, offline and 320 ms streaming). It fixes the wrong
  guesses that happen on **short dictation snippets (1–2 s)**: R2T2 wrote a 1.2 s
  Indonesian snippet in Hangul (`팀 팀`) → `team team` with `id,en`; Nemotron
  detected 2 s of Indonesian as Ukrainian and wrote Cyrillic → Latin; Qwen3
  labelled French as Dutch → French; Russian/Ukrainian letter leaks removed.
  Long clean clips were already right in auto mode.
- **Outside the list** (speech in a language not listed) the result is forced
  into the list: Qwen3 and Whisper _translate_ into a listed language; the other
  models write what they heard but respell letters the list does not allow
  (`großer` → `grosser`); another script is lost. The user's rule: they only list
  languages they actually speak, to stop wrong guesses — so the default
  (letters restricted) is right for them; `Any` is for occasional foreign names.

### 2.3 How to wire it into ZER0 (recipe — implement only when asked)

A prototype of exactly this was written and then reverted on 2026-11-14 because
the user had not asked for ZER0 work yet. The design that compiled:

**Settings** (`src-tauri/src/settings.rs`, `AppSettings` next to
`selected_language`, plus the `Default` impl):

```rust
#[serde(default)]
pub spoken_languages: Vec<String>,          // empty = off; recognition codes ("zh", not "zh-Hans")
#[serde(default)]
pub spoken_languages_any_letters: bool,     // AllowlistLetters::Any
```

**Commands** (`src-tauri/src/shortcut/mod.rs`, registered in `src-tauri/src/lib.rs`
next to `change_selected_language_setting`): `change_spoken_languages_setting
(languages: Vec<String>)` (trim, drop `auto`, dedupe keeping order) and
`change_spoken_languages_any_letters_setting(enabled: bool)`.

**Run plan** (`src-tauri/src/managers/transcription.rs`): extend
`TranscribeCppRunPlan` with `allowed_languages: Vec<String>` and
`allowlist_letters: AllowlistLetters`, and give `transcribe_cpp_run_plan` a
`SpokenLanguages { languages, any_letters, model_supports_allowlist }` argument.
Decision (pure, unit-testable):

```rust
fn spoken_language_allowlist(language: Option<&str>, spoken: SpokenLanguages<'_>,
                             model_languages: &[String]) -> Vec<String> {
    // pinned language wins; feature off; nothing listed -> no list
    if language.is_some() || spoken.languages.is_empty() || !spoken.model_supports_allowlist {
        return Vec::new();
    }
    // normalize_cjk_language each code, drop "auto"/empty, dedupe in order;
    // pass the WHOLE list if the model knows at least one of them
    // (canonical_language_code match against caps.languages), else none —
    // the engine would answer UnsupportedLanguage.
}
```

`model_supports_allowlist = model.supports(Feature::LanguageAllowlist)`, computed
where each site already reads `caps`. The five `RunOptions` builders that must set
`allowed_languages` / `allowlist_letters`: `run_stream_worker` (live preview; the
`(supports_streaming, supports_translate, languages)` tuple needs a fourth
member, including its error arm), `benchmark_stream`, `transcribe_internal`
(the `(model_supports_translate, model_languages)` tuple), `replay_stream_once`,
`transcribe_with_engine` (Multi-STT extras). Multi-STT slot languages keep
working through `apply_extra_model_settings`: a slot pinned to a language
overrides the list for that model.

Unit tests to add next to the existing `transcribe_cpp_run_plan_*` tests: list
reaches an Auto run; yields to a pinned language; skipped for a model without
the feature; skipped when the model knows none of the languages (and passed
whole when it knows one); `zh-Hans` → `zh` and the letters toggle carried.

**UI** (SolidJS): a "Languages I speak" multi-select (chips + the same searchable
dropdown style as `LanguageSelector.tsx`, options from
`MODEL_CAPABILITY_LANGUAGES` in `src/lib/constants/languages.ts`) and an "Allow
foreign words" toggle (`ToggleSwitch`, like `TranslateToEnglish.tsx`), shown in
`ModelSettingsCard.tsx` while the language is Auto. Map both keys in
`src/stores/settingsStore.ts`. `src/bindings.ts` is regenerated by a debug
`tauri dev` run (`dev:fast`). New strings go into **all 26 locales**
(`check:translations` requires identical keys; the locale files are CRLF — see
the prettier note in `AGENTS.md`).

Then: `bun run precommit` steps by hand (the format gate is broken tree-wide),
`bun run dev:fast`, and a check with real dictation in two languages.

---

## 3. VAD — what actually runs today

| Where                           | Detector                                                                                                                                                                                              | Status                                                                                                                                       |
| ------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| ZER0 capture                    | optional RNNoise (`nnnoiseless`, 48 kHz) → Rust `earshot` 1.2.2 + SmoothedVad (hysteresis, −45 dBFS pre-gate, 60 ms onset, 450 ms pre-roll, 450/1650 ms offline/streaming tails)                      | production; the app's VAD toggle and slider control this one                                                                                 |
| Engine, every native ASR stream | the fork's **native C++ Earshot** (`src/transcribe-earshot.*`; at ZER0's pin scalar and bit-exact to Earshot 1.2.2, SIMD after §3.1) as a _silence fast path_, plus a fixed **−42 dBFS** energy floor | **on by default** (`transcribe_stream_params::enable_vad`); ZER0 inherits it through `StreamOptions { ..Default::default() }`; R2T2 opts out |
| Engine, standalone              | Silero v6.2 VAD role (`transcribe_cpp::vad::VadSession`, upstream #201)                                                                                                                               | available, **not used** by ZER0                                                                                                              |
| Engine/app                      | FireRed Stream-VAD, Smart Turn v3.2, shared native preprocessing frontend                                                                                                                             | **not implemented** anywhere — plans only                                                                                                    |

So ZER0 streams run **two Earshots in series**: the app's Rust one decides what is
sent, then each engine stream runs its own C++ one, with different thresholds,
tails and no shared state, and the app toggle does not reach the second.

**Measured consequence (2026-11-14):** a quiet clip (FLEURS French, −62 dBFS RMS,
peak −47.5 dBFS) streams to an **empty transcript** through Nemotron with the
engine gate on, and is transcribed correctly with
`TRANSCRIBE_DISABLE_ACTIVITY_GATE=1`. The fast path skips a chunk only when the
engine Earshot is not speaking _and_ energy is below −42 dBFS; on this clip
Earshot never fires, so nothing ever reaches the encoder. Quiet microphones or
far-field dictation can hit the same trap in ZER0.

**Recommendation for the app (when asked):** give VAD one owner. Since the app
already gates capture, set `StreamOptions { enable_vad: false, .. }` explicitly
at the stream builders (`run_stream_worker`, `benchmark_stream`,
`replay_stream_once` in `transcription.rs`) — "CaptureOwned" in the engine plan —
and measure CPU cost per family, because that also removes the engine's
silence fast path. The alternative ("EngineOwned": untrimmed capture, one
engine-side policy) is the long-term direction in the engine plan §F.3; the
engine work it needed is in §3.1.

### 3.1 Engine replacements for ZER0's VAD and denoise layers (ready, 2026-11-14)

The plan is for ZER0 to stop owning VAD and noise suppression in Rust and to
call the engine instead (the user: "get rid of zer0 vad layers there to port
them into transcribe cpp"). The engine side is done, tested, merged and
**pushed to the engine's `main` at `cfb07a12`** (2026-11-15, branch
`feat/vad-frontend`; public ABI hash `0c46a78d4d6646a7`), so ZER0's next
build pulls it in. All new Rust items are re-exported from `transcribe_cpp`:

| ZER0 today (Rust)                                                                      | Engine replacement                                                                                                                                      | Notes                                                                                                                                                                                                                                                                            |
| -------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `earshot` 1.2.2 crate inside `vad/earshot.rs` (`EarshotVad`)                           | engine Earshot, SIMD (baseline / AVX2 / AVX-512 picked by cpuid)                                                                                        | 1.32 µs per 16 ms frame vs 5.33 for the crate (AVX2 machine); max score diff vs upstream 2.1e-5                                                                                                                                                                                  |
| `EarshotVad` + `SmoothedVad` (`vad/smoothed.rs`) + `SpeechClock` (`audio/recorder.rs`) | **`VadGate`** / `VadGateParams` / `VadGateUpdate` / `VadGateTail` (`transcribe_vad_gate_*`)                                                             | port state machine for state machine: hysteresis (exit = t − 0.15, ≥ 0.01), 60 ms onset, 450 ms pre-roll that never releases twice, hangover (450 / 1650 via `set_hangover_ms`), speech clock with 500 ms hold, tail report; `set_threshold` live                                |
| `nnnoiseless` 0.5.2 + `Gate` + `denoise_frame` (`audio/denoise.rs`)                    | **`Denoiser`** / `DenoiseParams` (`transcribe_denoise_*`), `DENOISE_FRAME_SAMPLES` = 480, `DENOISE_SAMPLE_RATE` = 48 000, `DENOISE_MAX_GRACE_MS` = 5000 | same model and algorithm (port of the crate), same wrapper math (int16 scaling, wet/dry with the dry side delayed a frame, slew 0.5 / snap 1e-3 gate, grace in 10 ms frames, clamp); output within 1 int16 step, 1.41× faster; `DenoiseParams::normalized()` is ZER0's own clamp |
| per-frame `EarshotVad` loop in `managers/history.rs` (legacy backfill)                 | **`score_frames(pcm, n_threads)`** (`transcribe_vad_score_frames`)                                                                                      | batch scores, bit-identical to frame-by-frame; apply the 0.5 / 0.35 hysteresis to count voiced frames (a few lines), or feed a `VadGate` if the speech clock is the wanted measure                                                                                               |

**The one deliberate behaviour change:** ZER0's `EarshotVad` skipped the
network (score 0) below −45 dBFS while not in speech; the engine scores every
frame. On 10 min of audio that changes 0.5 % of frames, and it is what lets a
quiet microphone reach speech at all (the B.7 trap above). Everything else
matched a Rust re-creation of the ZER0 chain exactly (37,500 frames, same
released samples, same speech clock).

**Migration recipe (do it when the user asks):**

1. Pin moves to engine `main` at or after `cfb07a12` (automatic on the next build).
2. `audio/denoise.rs`: keep `DenoiseParams`/`DenoiseControls` (or switch to
   `transcribe_cpp::DenoiseParams`, which has the same fields, defaults and
   `normalized()`), keep both `FrameResampler`s; replace `denoiser`, `gate`,
   `scaled_*`, `dry_delay` and `denoise_frame` with one `transcribe_cpp::Denoiser`:
   per 48 kHz frame `set_params` if the controls changed, then
   `process_frame(frame, &mut frame_out)` → probability; `finish` calls
   `flush(&mut frame_out)` instead of pushing `SILENT_FRAME`; `reset` calls
   `Denoiser::reset`. The "after noise suppression" Live FFT tap still sees
   `frame_out`.
3. `vad/`: replace `EarshotVad` + `SmoothedVad` with one `transcribe_cpp::VadGate`
   per capture source. `feed(&pcm_16k)` returns the released audio (pre-roll +
   speech + hangover, in order) and `speech` / `voiced` / `speech_started` /
   `speech_ended` / `last_score` / `level_dbfs` / `speech_ms` /
   `clock_speaking`; `finish()` at end of recording, `tail()` for the
   end-of-recording diagnostics, `reset()` per recording. The VAD threshold
   slider maps to `set_threshold` (0.05 ..= 0.95), the offline/streaming tail
   switch to `set_hangover_ms(450 | 1650)`. The live VAD test reads
   `last_score` and `level_dbfs`. `SpeechClock` goes away: WPM uses
   `speech_ms`.
4. `history.rs` backfill: `score_frames` as in the table.
5. One VAD owner: pass `StreamOptions { enable_vad: false, .. }` at
   `run_stream_worker`, `benchmark_stream`, `replay_stream_once`
   (`managers/transcription.rs`).
6. Drop `earshot` and `nnnoiseless` from `src-tauri/Cargo.toml`; their tests in
   `denoise.rs` / `vad/*.rs` move to (or are covered by) the engine's
   `tests/vad_gate_unit.cpp`, `tests/denoise_unit.cpp`, `tests/rnnoise_unit.cpp`
   and the Rust `earshot` / `denoise` tests.
7. Check: `bun run dev:fast`, then real dictation — quiet and normal mic,
   denoise on and off, strength and gate sliders, VAD slider, streaming and
   offline, the Live FFT taps, WPM.

### 3.2 Engine VAD work still planned, not done

From the engine master plan §B (sources: audio.cpp #1095, #1096, #846):

- **B.1 Earshot segmenter fidelity** (offline segments: keep a segment open at
  end of audio, count every frame, consistent stream start rule, Silero-style
  option names). Does not affect the gate above.
- **B.2 Earshot as a VAD-role model**: one `VadSession` API for Silero and Earshot.
- **B.4** any VAD for long-form chunking; **B.5** FireRed only if it beats
  Earshot on ZER0 dictation data (audio.cpp's frame F1 on the TEN VAD set:
  Silero 0.935, Earshot 0.926, FireRed 0.917, and Earshot ~14× faster than
  FireRed); **B.6** benchmark harness.
- audio.cpp's RNNoise v0.2 (ggml, 11 MB weights) is not ported; the classic
  model above is the one ZER0 ships.

The October "three-tier super VAD" idea (Earshot → FireRed verifier → Smart Turn
endpointing, all in the engine) was revised on 2026-10-16 (revision 9) into: one
acoustic detector as the production baseline, FireRed as a challenger, Smart
Turn as optional endpoint advice, and the cascade only as a measured experiment.
The November measurements kept Earshot as tier 1.

---

## 4. Rules for whoever continues

- ZER0 changes only when the user asks; this note is a map, not a work order.
- The user's tree usually holds uncommitted work (typography, live canvas at the
  time of writing): never revert with checkout/restore — undo only your own hunks.
- Engine changes land in the engine checkout and are pushed to its `main`; ZER0
  follows automatically on the next build.
- Update this note, `docs/TRANSCRIBE_CPP_INTEGRATION.md` and the engine's
  `docs/zer0-engine-integration.md` together when the boundary changes.
