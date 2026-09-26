# R2T2 streaming latency sweep (2026-09-26)

What happens to Confucius4-R2T2's streaming speed as its latency (the decode
chunk size) goes from 80 to 2000 ms, why the speed looked irregular, and which
settings are genuinely fast.

**Short answer.** The irregular speeds, including the 50–153× "fast" settings,
were not fast decoding: at those chunk sizes transcribe.cpp stopped handing
R2T2 audio, so the model decoded less and lost words, sometimes the whole
transcript. Two fixes in transcribe.cpp (`f2de3264`, `ba949120`) remove this.
Now every chunk size from 80 to 2000 ms gives the complete transcript, and
speed rises smoothly with the chunk size, from 1.9× at 80 ms to 25–27× at
1630–1980 ms. On this recording **no setting reaches 90× with a complete
transcript**; every result above ~27× was undecoded audio.

## Method

- **Build:** ZER0 debug binary on the local CUDA lane (`dev:fast` environment),
  RTX 4070 Laptop (CUDA0). The native library is optimised C++ in every Rust
  profile. transcribe.cpp before: `145c96a6`; after: `ba949120`.
- **Model:** `davidxifeng/Confucius4-R2T2-gguf/r2t2-q4_k_m.gguf`, language auto.
- **Audio:** the latest history recording, `zer0-multi-1790296592.wav`
  (48 kHz mono float, 10.93 s; "So I have twelve dollars, no, ten."). Speech is
  at 1.75–4.25 s ("So I have 12 dollars, no") and 9.5–9.9 s ("ten", very quiet:
  −64 to −78 dBFS); the rest is silence.
- **Path:** the app's own headless streaming replay:

  ```
  zer0 --transcribe-file <wav> --model <id> --device-index 0 \
       --stream-chunk-ms 16 --stream-r2t2-chunk-ms <N> --repeat 3 --json
  ```

  `--stream-chunk-ms 16` feeds 256-sample frames, which is what the live app
  feeds after its VAD. `--stream-r2t2-chunk-ms` is a runtime-only override added
  for this sweep; it never touches the saved setting.

- **Protocol:** 3 runs per latency in one process. Run 1 is a discarded warmup;
  runs 2 and 3 are averaged. Speed = audio seconds ÷ mean wall time of runs 2–3.
- **Grid:** before the fixes, 80–2000 ms in 10 ms steps (193 points). After,
  50 ms steps plus every value that had misbehaved (51 points).
- **"Complete"** means the transcript contains the final word (ten / 10, or a
  mishearing such as "then"). Missing it means that audio was never decoded.

Each point is one process, so neighbouring values carry run-to-run noise of a
few ×. Treat single points as indicative and ranges as the result.

## How R2T2 streams, and why speed depends on the chunk size

R2T2 keeps no incremental encoder state. At every tick it re-encodes **all audio
received so far** and re-decodes the transcript, committing the stable prefix
and holding back the last 5 tokens. The chunk size sets how often a tick runs,
so ticks ≈ audio ÷ chunk (136 at 80 ms, 5 at 2000 ms), and each tick costs more
than the last because the buffer keeps growing. That gives the smooth curve
below. The encoder works in 1 s blocks with 8 s attention windows, so a tick's
cost also rises in 1 s steps as audio accumulates.

## What caused the irregularities

### 1. The silence fast path dropped speech (fixed in `f2de3264`)

Before handing a 16 ms slice to a model, transcribe.cpp's dispatcher skips it
(never decodes it) when the model has nothing buffered, there is no tentative
text, and the slice looks silent. "Nothing buffered" is read from two session
cursors that each streaming family must keep up to date. **R2T2 never updated
them**, so that condition was always true, and the gate fired whenever a pause
happened to line up with empty tentative text:

| Before the fixes          |         Speed | Ticks | Transcript                           |
| ------------------------- | ------------: | ----: | ------------------------------------ |
| 540 / 560 ms              | 13.3× / 13.4× |    15 | "So I have twelve dollars, no, ten." |
| 550 ms                    |         62.3× |     2 | "So I have twelve dollars, no"       |
| 710 ms                    |         82.8× |     1 | "So I have 12 dollars, no"           |
| 740, 780, 810–830, 870 ms |        54–60× |     2 | ends at "no" / "not"                 |
| 1300 ms                   |         49.3× |     2 | _(empty)_                            |
| 1400–2000 ms              |        64–75× |     0 | "So I have 12 dollars, no"           |

The fix makes R2T2 keep the cursors, and makes the dispatcher skip a slice only
when it is silence by both measures (VAD not speaking **and** energy below
−42 dBFS). Before, either one sufficed, which also threw away the audible start
of a word before the VAD flipped to "speaking".

### 2. Skipping silence changed what R2T2 hears (fixed in `ba949120`)

After the first fix, 1880 and 2000 ms still returned an empty transcript. The
token trace (`TRANSCRIBE_R2T2_TRACE=1`) showed ticks where the model generated
**zero tokens**: its first prediction was end-of-sequence. Turning off
speculative drafting, KV reuse, the encoder cache and the mel cache one at a
time changed nothing; turning off the silence gate
(`TRANSCRIBE_DISABLE_ACTIVITY_GATE=1`) made every tick produce text.

Because R2T2 re-encodes the whole buffer as one utterance, skipping silence (the
leading silence, and any pause that arrives while the buffer happens to be
empty) hands it a _different utterance_: speech starting abruptly with pauses
spliced out. On that input the checkpoint can decide there is nothing to
transcribe. Skipping silence is safe for families whose state advances
incrementally; it is not for R2T2.

- **R2T2 now opts out of the silence fast path** (a per-stream flag its begin
  hook sets). It receives every sample the caller feeds; in ZER0 that is already
  filtered by the app's own VAD. Every other family keeps the fast path.
- **Finalize always runs one final pass.** It used to be skipped when the chunk
  buffer was empty at the end, i.e. whenever the audio was a whole number of
  chunks. The last tick was then a non-final one, which may not have produced
  text yet. Verified on the real model (CPU, 10.000 s clip, 2000 ms chunks): the
  old code ends at tick 5 with no final pass, the new one runs it.

### 3. Word-choice changes between ticks (inherent, not a bug)

Most ticks cost 30–90 ms on this GPU, but a few spike to 150–260 ms. The spikes
coincide with the model switching between "twelve dollars, no, ten" and "$12, no
10". Each tick uses the previous tick's held-back tokens as a speculative draft;
when a new cut point changes the wording, the draft is rejected and the tick
steps token by token. The cost depends on where the chunk boundary falls in the
words, not on the chunk size itself. This is the small wobble that remains
between neighbouring values.

## Results after both fixes (transcribe.cpp `ba949120`)

| Latency |     Speed | Ticks | Complete |
| ------: | --------: | ----: | :------: |
|      80 |      1.9× |   136 |   yes    |
|     130 |      2.9× |    84 |   yes    |
|     180 |      3.8× |    60 |   yes    |
|     230 |      5.0× |    47 |   yes    |
|     280 |      6.0× |    39 |   yes    |
|     330 |      6.6× |    33 |   yes    |
|     380 |      7.8× |    28 |   yes    |
|     430 |      8.1× |    25 |   yes    |
|     480 |      9.1× |    22 |   yes    |
|     530 |     10.4× |    20 |   yes    |
|     550 |     10.1× |    19 |   yes    |
|     580 |     11.4× |    18 |   yes    |
|     630 |     11.2× |    17 |   yes    |
|     680 |     12.7× |    16 |   yes    |
|     710 |     12.8× |    15 |   yes    |
|     730 |     13.6× |    14 |   yes    |
|     740 |     14.1× |    14 |   yes    |
|     780 |     14.0× |    14 |   yes    |
|     810 |     14.6× |    13 |   yes    |
|     820 |     15.9× |    13 |   yes    |
|     830 |     15.5× |    13 |   yes    |
|     870 |     15.4× |    12 |   yes    |
|     880 |     14.8× |    12 |   yes    |
|     930 |     16.5× |    11 |   yes    |
|     980 |     16.7× |    11 |   yes    |
|    1010 |     16.3× |    10 |   yes    |
|    1030 |     17.2× |    10 |   yes    |
|    1080 |     17.3× |    10 |   yes    |
|    1130 |     19.0× |     9 |   yes    |
|    1180 |     19.1× |     9 |   yes    |
|    1230 |     20.5× |     8 |   yes    |
|    1250 |     20.0× |     8 |   yes    |
|    1280 |     19.3× |     8 |   yes    |
|    1300 |     19.5× |     8 |   yes    |
|    1330 |     18.3× |     8 |   yes    |
|    1380 |     21.7× |     7 |   yes    |
|    1400 |     24.1× |     7 |   yes    |
|    1430 |     22.2× |     7 |   yes    |
|    1480 |     22.6× |     7 |   yes    |
|    1500 |     22.1× |     7 |   yes    |
|    1530 |     21.8× |     7 |   yes    |
|    1580 |     23.7× |     6 |   yes    |
|    1630 | **25.4×** |     6 |   yes    |
|    1680 |     24.0× |     6 |   yes    |
|    1730 |     23.0× |     6 |   yes    |
|    1780 | **25.8×** |     6 |   yes    |
|    1830 | **25.5×** |     5 |   yes    |
|    1880 |     25.0× |     5 |   yes    |
|    1930 | **25.6×** |     5 |   yes    |
|    1980 | **27.1×** |     5 |   yes    |
|    2000 |     16.0× |     5 |   yes    |

2000 ms is slower than its neighbours because it divides the 16 ms feed exactly,
so the stream pays for the extra final pass at the end.

### Ranges (complete transcript)

| Speed  | Latency                                 |
| ------ | --------------------------------------- |
| ≥ 90×  | none on this recording                  |
| ≥ 25×  | 1630, 1780, 1830, 1880, 1930, 1980 ms   |
| 20–25× | 1230–1250 and 1380–1980 ms (contiguous) |
| 15–20× | 820–1330 ms                             |
| 10–15× | 530–810 ms                              |
| < 10×  | 80–480 ms                               |

Fastest: 1980 ms (27.1×), 1780 ms (25.8×), 1930 ms (25.6×), 1830 ms (25.5×),
1630 ms (25.4×).

For live use, compute speed is not the only thing that matters. The live app
merges any backlog into one tick, so it never falls behind by more than one
tick; what the chunk size really buys is **how soon text appears**. On this GPU
even 80 ms runs faster than real time (1.9×) on an 11 s clip, but per-tick cost
grows with the utterance, so small chunks on long dictations will fall back to
coalesced ticks.

## Reproducing

The sweep driver and report generator used here are small Bun scripts around the
command in "Method"; one point takes 2–8 s. The flag `--stream-r2t2-chunk-ms`
is headless-only and lives in `src-tauri/src/cli.rs`. R2T2's own diagnostics:
`TRANSCRIBE_R2T2_TRACE=1` (generated tokens per tick) and
`TRANSCRIBE_DISABLE_ACTIVITY_GATE=1` (bypass the silence gate for every family).
