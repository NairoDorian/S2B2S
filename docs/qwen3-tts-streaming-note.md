# Qwen3-TTS Streaming Capabilities (as shipped by faster-qwen3-tts)

Source of truth: `C:\Users\Z\Downloads\PROJECTS\STT_BRAIN_TTS\faster-qwen3-tts`
(mirror of `andimarafioti/faster-qwen3-tts`, MIT). This note explains what
"streaming" means for Qwen3-TTS and why S2B2S does NOT use the chunk-streaming
path.

## Two different "streaming" concepts

### 1. Audio output streaming (chunk-level)

- The **official `QwenLM/Qwen3-TTS` repo does not support streaming at all** —
  it returns the full utterance only after generation finishes.
- `faster-qwen3-tts` adds it, adapting the idea from the community fork
  [`dffdeeq/Qwen3-TTS-streaming`](https://github.com/dffdeeq/Qwen3-TTS-streaming).
- Mechanism (`faster_qwen3_tts/streaming.py::fast_generate_streaming`): the
  CUDA-graphed decode loop stays identical to non-streaming (same per-step
  performance), but instead of collecting all codec tokens it **yields every
  `chunk_size` codec steps**. The 12 Hz codec means `chunk_size` steps ≈
  `chunk_size / 12` seconds of audio:
  - chunk_size 1 ≈ 83 ms, 2 ≈ 167 ms, 4 ≈ 333 ms, 8 ≈ 667 ms, 12 ≈ 1 s.
- The wrapper (`model.py::generate_*_streaming`) decodes each chunk to audio
  with a **hybrid decode strategy**: accumulated decode for early chunks (to
  calibrate `samples_per_frame`), then a **sliding window with 25-frame left
  context** (constant cost) to avoid boundary pops.
- Public API: `generate_voice_clone_streaming`, `generate_custom_voice_streaming`,
  `generate_voice_design_streaming` — all yield `(audio_chunk, sample_rate, timing)`
  tuples. `timing` carries `prefill_ms` (first chunk only) and `decode_ms` per chunk.
- CLI flag `--streaming`; server mode `serve --streaming` streams PCM chunks;
  `examples/openai_server.py` streams WAV/PCM per OpenAI contract (MP3 needs pydub).
- The demo (`demo/server.py`) has a streaming/non-streaming toggle, adjustable
  chunk size and live TTFA/RTF metrics; it defaults to the GGML backend.

### 2. Text feeding streaming (`non_streaming_mode`)

- This is an **orthogonal, upstream concept** about _how the model consumes the
  text_, not about audio output: feed the full utterance in one prefill
  (`non_streaming_mode=True`) vs. feed text progressively during decode
  (`non_streaming_mode=False`).
- Naming is inherited from the original Qwen3TTS implementation; it has nothing
  to do with audio chunking.
- `None` sentinel → upstream defaults: voice-clone → `False` (step-by-step text),
  CustomVoice/VoiceDesign → `True` (full prefill).
- Measured on RTX 4090 (1.7B, ICL, chunk 8): TTFA unchanged (~159 ms), RTF
  effectively identical (4.85 vs 4.87). Quality samples under
  `samples/non_streaming_mode/`.
- GGML backend: no ABI switch for step-by-step feeding; `non_streaming_mode=False`
  warns and uses qwentts.cpp's native prompt layout.

## The chunk_size tradeoff (upstream measurements)

From the README "Chunk size vs performance" table (Jetson AGX Orin, 0.6B):

| chunk_size | TTFA  | RTF   | Audio/chunk |
| ---------- | ----- | ----- | ----------- |
| 1          | 240ms | 0.750 | 83ms        |
| 2          | 266ms | 1.042 | 167ms       |
| 4          | 362ms | 1.251 | 333ms       |
| 8          | 556ms | 1.384 | 667ms       |
| 12         | 753ms | 1.449 | 1000ms      |
| non-stream | —     | 1.57  | all at once |

Smaller chunks = lower first-audio latency but **more per-chunk decode overhead**
(synchronize + yield + sliding-window decode each chunk).

## What our hardware actually does (RTX 4070 Laptop, shared with STT + llama-server)

S2B2S benchmark (`bench_chunk.py`, torch backend, bf16, 3-iter medians with
discarded warmup):

| chunk_size | 0.6B RTF | 0.6B first audio | 1.7B RTF             | 1.7B first audio |
| ---------- | -------- | ---------------- | -------------------- | ---------------- |
| 2          | 0.72x    | ~212ms           | —                    | —                |
| 4          | 0.50x    | ~284ms           | —                    | —                |
| 6          | 0.46x    | ~272ms           | ~6.2x (pathological) | ~2187ms          |
| 12         | 0.43x    | ~459ms           | 0.58x                | ~627ms           |
| 24         | 0.41x    | ~825ms           | 0.53x                | ~1048ms          |
| buffered   | 0.43x    | —                | 0.51x                | —                |

- On this GPU the fixed per-chunk overhead dominates: chunk 2 costs ~85 ms
  extra per chunk and only makes sense on machines where generation is 4–5x
  faster than real-time (RTX 4090 class).
- **Conclusion: chunk-level streaming buys nothing here** — RTF never exceeds
  1x at any chunk size, so the first chunk can never arrive before the whole
  utterance would finish. Buffered generation at `chunk_size=12` is equal or
  faster than every streaming configuration.

## S2B2S decision

- The chunk-level streaming mode (JSON-header + raw PCM over HTTP,
  `player.append_pcm`, `TtsPlayer::stream_begin/stream_end`) was **removed**
  in the Qwen3 integration rework.
- TTS synthesizes the whole utterance in one buffered request using `FasterQwen3TTS`'s
  native whole-utterance generation methods (`generate_custom_voice`,
  `generate_voice_clone`, `generate_voice_design`). This runs fast CUDA graph
  generation and decodes all codec tokens in a single `speech_tokenizer.decode`
  pass without chunking or sliding-window boundary artifacts.
- The existing sentence/fragment pipeline (pagination, `tts_shorten_first_chunk`)
  provides per-sentence first-audio behavior for long texts.
- Revisit chunk streaming only if the model runs on a much faster GPU or with
  the GGML backend on this machine.
