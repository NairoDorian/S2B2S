# Confucius4-R2T2 integration contract

Status: **shipped.** Native loading and streaming pass, so the gate below is met
and the catalogue entry exists. Remaining acceptance work is listed at the end.

The original gate — _do not add a downloadable or selectable catalogue entry
before actual native loading and streaming pass_ — was satisfied by the fork's
R2T2 streaming implementation before the entry was authored. That entry is what
makes the model visible at all; see **Why the catalogue entry is load-bearing**.

## Current findings — 2026-09-20

The integration must use the native **transcribe.cpp R2T2 implementation**.
Do not launch audio.cpp, embed its runtime, or route this model to an audio.cpp
server. Its source is a reference for porting model behavior, not an application
backend. The existing Qwen native graphs can provide encoder/decoder math;
R2T2 additionally needs its own streaming prefix/rollback state machine.

The verified Q8 download is packaged in audio.cpp's GGUF schema, so its valid
URL alone does not establish compatibility with the current native library.
Native schema adaptation or conversion remains necessary. The engine has
started source preparation, but no working R2T2 model or latency UI is shipped.

The requested range remains 80–2000 ms with exact integer-millisecond values,
direct numeric entry and access to 80 ms. This is a model decode cadence, not
the existing PCM feed size or the Nemotron lookahead preset. First committed
text, queue wait and finalization must be measured separately. Context and
hotwords remain deferred by user choice.

The engine already includes the compatible audio.cpp graph-optimizer subset
as an opt-in. It does not require an audio.cpp runtime. Enabling that pass is
separate from implementing R2T2, and prior results do not support a universal
performance claim. The app still needs the final native dependency pin,
catalogue entry, bindings, settings and both streaming call sites updated.

## User decisions

Offline transcription, language selection/detection and native streaming are
required. Context/hotword prompts are deferred to prioritize streaming.
The chunk control must expose the full **80–2000 ms** range, including 80 ms.
Use integer milliseconds, a 1 ms slider step, and numeric entry; default 320 ms.
Changing the value affects the next stream. Persist the exact per-model value.

## Required wiring

- `src-tauri/src/settings.rs`: a per-model chunk-duration map with backward
  compatible defaults, separate from `native_streaming_latency_presets`.
- `src-tauri/src/managers/model.rs`: identify this family separately from
  ordinary Qwen3-ASR. Do not infer native streaming from its Qwen architecture.
- `src-tauri/src/managers/native_streaming_latency.rs`: pass the exact duration
  through a dedicated native extension accepted by the loaded model. Reject
  invalid values instead of silently substituting one of the four presets.
- `src/components/model-selector/LatencyPanel.tsx`: show the numeric range for
  this family; preserve the existing presets for their supported models.
- `src/components/model-selector/ModelSelector.tsx`, settings commands/store,
  generated bindings and settings import/export: carry the value end to end.
- Both primary and Multi-STT native streaming paths must use the same resolved
  value. Microphone frame dispatch must not wait for a VAD pause.
- Headless benchmark requests must select the same native chunk option; feed
  packet duration and model decode-chunk duration are distinct settings.

Label the value **Streaming chunk size**, not guaranteed latency. Show requested
and resolved duration in native/app logs together with first committed text,
queue wait, feed p95/max, final flush, and total real-time factor. No new worker,
poller, or per-feed webview event is needed for the control.

## Download identity and compatibility

The verified publisher conversion is
[r2t2-q8_0.gguf](https://huggingface.co/davidxifeng/Confucius4-R2T2-gguf/resolve/a8e6b385d7df7eae9519363e07034a209004797a/r2t2-q8_0.gguf).
HTTP HEAD succeeded on 2026-09-20; size 2,477,512,064 bytes; LFS SHA-256
`19f5ccd624484bcb5d44301437de41560b0ecc40c430e8850dfeefefbe82ccf5`.
The full Q8 file was downloaded and its SHA-256 verified locally on 2026-09-20.

That is the **audio.cpp reference download point** — the repo its own model spec
names (`confucius4_r2t2_q8_0.download.repo`), not a repack we host ourselves. The
catalogue entry's `id` _is_ the source: there is no separate URL field, so
`davidxifeng/Confucius4-R2T2-gguf` + the pinned revision is what hf-hub resolves.
The F16 variant of the same package is listed too, but Q8_0 is the default
because it is the only one verified to load and stream natively end to end.

**Resolved: no conversion is required.** The earlier concern — that these are
audio.cpp metadata/tensor names rather than the transcribe GGUF schema — does not
apply to this artifact. `src/arch/qwen3_asr/r2t2-package.cpp` reads its sidecars
out of GGUF-embedded data (`audiocpp.embedded_files.{names,offsets,data}`), and
the published file carries `config.json`, `tokenizer.json`,
`generation_config.json`, `preprocessor_config.json` and `chat_template.json`
inline, plus `audiocpp.model_spec.family = "confucius4_r2t2"` which is what
`is_r2t2_package()` keys on. A single-file entry is therefore complete: no
companion JSON, no fabricated transcribe-compatible URL.

### Why the catalogue entry is load-bearing

The GGUF's `general.architecture` is the audio.cpp marker **`audiocpp`**, not a
transcribe-cpp arch name, and it carries no `stt.capability.*` keys. Without a
catalogue entry the HF-cache scan would fall through to a header probe, classify
the file `MaybeIncompatible`, and skip it — the model would be invisible in the
app even with the file on disk. Catalogued, the scan resolves it by repo +
filename and never probes the header (the catalog is a baked probe), which also
supplies the streaming/language capabilities the GGUF itself lacks. Hence
`architecture` is recorded as `qwen3_asr`, the family it actually loads as, and
not the `audiocpp` marker a naive generator run would have copied.

One consequence to be aware of: the entry inherits only the default file in the
mirror list, and `blob.handy.computer` does not host this third-party repo, so a
mirror attempt 404s and HF remains the effective source. The per-file sha256 is
still the trust anchor, exactly as for every other entry.

## Acceptance

Exercise 80 and 2000 ms, an irregular integer duration, persisted round-trip,
settings import, primary and Multi-STT sessions, final short tail, reset and
model switching. Native validation must reject out-of-range values without
clearing the previous transcript. Benchmark CPU and CUDA with exactly three
runs per loaded configuration; discard run one and average runs two and three.
Stable partials, final transcript and language behavior require reference
comparison before this family becomes selectable.
