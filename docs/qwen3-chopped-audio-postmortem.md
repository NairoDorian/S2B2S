# Postmortem: Chopped, stuttering Qwen3-TTS audio on RTX 4070 Laptop

**Hardware:** RTX 4070 Laptop (8 GB, shared with STT engines + a llama-server
Brain doing MTP speculative decoding at ~216 tok/s — the GPU is never idle).
**Symptoms:** Qwen3-TTS output sounded like short isolated audio bursts —
"chopped up", never fluid — while the same text read through the buffered
greeting path sounded fine.

## Timeline of what was tried

### 1. Initial implementation: chunk-level streaming at `chunk_size 4`

(`bac4cad2`, Aug 15)
The Qwen3 backend was built around faster-qwen3-tts's streaming generator.
The Python server streamed raw int16 PCM chunks over HTTP as they were
generated; the Rust player got a dedicated `append_pcm` path that pushed
`SamplesBuffer`s straight into the rodio sink, no decode step. Chunk size 4
≈ 333 ms of audio per chunk.

### 2. "Make it faster": `chunk_size 2` + pre-opened output device

(`72bd9e2d`, Aug 15)
Cited the faster-qwen3-tts README benchmark table (measured on an **RTX
4090 / Jetson AGX Orin**, not a laptop) which claimed chunk 2 was the
smallest real-time chunk, TTFA dropped to ~167 ms. `chunk_size` was set to 2
and the player started pre-opening the WASAPI device before audio existed
to shave device-open latency off the TTFA path.

**Result: audio became _more_ chopped.** Each chunk was only ~167 ms of
speech; every chunk boundary paid fixed per-chunk overhead (Python yields,
torch.cuda.synchronize, sliding-window decode) and the shared GPU (llama
-server + STT + TTS on the same 4070) starved the sink between chunks.

### 3. Logs: `Dropping DeviceSink` between chunks

The player's 300 ms empty-sink debounce fired _between_ chunks on the loaded
GPU — it believed playback had finished and dropped the output device,
killing queued audio, then reopened it for the next chunk. Speech came out as
isolated 167 ms bursts.

### 4. Fix attempt: stream lifetime tracking

(`CHANGELOG` entry "Chopped Qwen3 streamed audio")
Added `stream_begin` / `stream_end` around the streaming synthesis so the
empty-sink debounce was suspended while a stream was active.

**Result: still chopped.** The device-drop was fixed but the underlying
problem remained — generation could not keep the sink fed in real time.

### 5. Benchmarking before further tuning (the decisive step)

`bench_chunk.py` measured actual RTF on this machine (torch backend, bf16,
3 iterations per chunk size with a discarded warmup run):

| chunk_size | 0.6B RTF  | first audio |
| ---------- | --------- | ----------- |
| 2          | **0.72x** | ~212 ms     |
| 4          | 0.50x     | ~284 ms     |
| 6          | 0.46x     | ~272 ms     |
| 12         | 0.43x     | ~459 ms     |
| 24         | 0.41x     | ~825 ms     |
| buffered   | 0.43x     | —           |

1.7B model: chunk 12 → 0.58x, chunk 24 → 0.53x, buffered → 0.51x; chunk 6
was pathological (~6.2x, i.e. generation ran slower than 1x real time).

**Root cause found:** on this GPU, RTF never exceeds 1x at _any_ chunk size.
A streaming architecture only pays off when the model can synthesize at
multiple times real time (the 4090 does 4.2x; the laptop does 0.4x). Here
the first chunk could never beat the whole utterance finishing — the fixed
per-chunk overhead (~85 ms per 2-step chunk) just made things worse.

Also discovered: the first run of a fresh process was always much slower
(warmup: CUDA graph capture, weight loading) — a common measurement trap
that made early tuning decisions look better than reality.

### 6. The timeout bug

(`CHANGELOG` entry "Qwen3 request timeout killed long streams")
The HTTP deadline `(8s + chars*50ms).clamp(15s, 300s)` expired mid-synthesis
when generation ran slower than real time, aborting the stream, unloading
the server (~20 s model reload) and cascading request failures. Made the
deadline speech-duration-aware (~12 chars/s, 6x RTF slack, 60 s floor,
30 min cap).

### 7. Chunk size 2 → 12

Chunk 12 (≈ 1 s of audio) measured RTF 0.43x on 0.6B — same throughput
plateau as chunk 24/36/50 — but only ~460 ms first audio. Applied to both
the Rust `--chunk-size` arg and the server default. Buffered greeting also
became ~2x faster to synthesize.

### 8. Final decision: remove streaming entirely

Even at chunk 12 the output could not stay fluid under GPU contention.
Streaming was stripped end-to-end: the trait's streaming methods, the HTTP
streaming handler, the player's `append_pcm`/`stream_begin`/`stream_end`
paths. TTS now synthesizes the whole utterance buffered, then plays it —
exactly what the greeting already did (which never sounded chopped).

## Conclusions

1. **Streaming is only a win when RTF > 1x with headroom.** The upstream
   benchmark table (RTX 4090: RTF 4.2x) does not transfer to a shared
   laptop GPU. On this machine RTF is 0.4x at best — play whole audio.
2. **The chopped audio was not a bug in the player** — it was the player
   faithfully playing tiny chunks as fast as the GPU could make them, while
   the GPU could not make them fast enough.
3. **Per-chunk overhead is real and fixed**: ~85 ms per 2-step chunk on this
   hardware. Small chunks are the _worst_ configuration for a slow GPU.
4. **Measure before tuning.** The warmup-polluted first run made chunk 2 look
   better than chunk 12 initially; the clean 3-iteration benchmark reversed
   that conclusion and killed the entire feature.

## Current state (post-revert)

- Qwen3 speaks through the buffered path: whole utterance synthesized in one
  HTTP request using `FasterQwen3TTS`'s native full generation methods
  (`generate_custom_voice`, `generate_voice_clone`, `generate_voice_design`),
  then played with the normal gapless player.
- The sentence/fragment pipeline still exists (`tts_shorten_first_chunk`,
  3-fragment pattern) so long text still gets per-sentence first audio.
- RTF headroom on this machine: ~2.4x real-time (0.6B) — comfortable margin
  even when llama-server and STT share the GPU.

See `docs/qwen3-tts-streaming-note.md` for what streaming is/does in
faster-qwen3-tts and the full benchmark tables.
