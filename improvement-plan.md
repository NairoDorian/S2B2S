# S2B2S — Streaming STT/TTS & Converted-Model Improvement Plan

**Scope:** Verify that the locally-converted Parakeet streaming model is fully wired into the latest `main`, analyze the streaming STT and streaming TTS subsystems end-to-end, compare against `sherpa-onnx` and other relevant open-source projects, and lay out a prioritized improvement roadmap.

**Repos reviewed:** `NairoDorian/S2B2S` @ `aa1c230` (2026-06-15, branch `main`) and `k2-fsa/sherpa-onnx` @ `6206c9c` (2026-06-15).
**Commits analyzed:** last ~55 of `S2B2S` (the STT/TTS/streaming cluster spans 2026-06-12 → 2026-06-15).
**This is analysis + plan only. No code changes are proposed as diffs here.**

---

## 0. Executive Summary

The good news up front: **the converted model is correct and is wired into `main`.** The S2B2S export script is *byte-for-byte identical* to sherpa-onnx's official export script from PR #3575, the emitted ONNX metadata uses the exact `streaming_model_type` string sherpa-onnx expects, sherpa-onnx ≥ 1.13.2 ships native support for this model family, the Python server auto-detects and routes it through `OnlineRecognizer`, and the model registry exposes it. The changelog records a successful end-to-end verification (the JFK sample transcribes accurately).

The bad news, and the central finding of this review: **S2B2S does not actually stream.** Both streaming code paths take the *complete, already-recorded* audio buffer and *replay* it through the streaming API in 250 ms slices **after recording has stopped**. The buffered-streaming model's entire reason for existing — sub-second partial results *while the user is still speaking* — is therefore unrealized. Every `transcription-partial` event fires in a burst after the utterance ends. This is faithfully reflected in the README, where Streaming STT is marked **"✅ Partial."**

The second finding: **the flagship converted model has no download source.** It is registered only when its files already exist on disk, so in practice it is unavailable to anyone who has not personally run a ~15-minute NeMo export against a 2.6 GB checkpoint.

The third finding: **"streaming TTS" is pipelined fragment synthesis, not true streaming.** It lowers time-to-first-audio (a real win) but does not stream audio samples out of a single synthesis call.

The single highest-leverage change is **#P0-2: feed live microphone audio to the streaming recognizer during capture** (true streaming). The second is **#P0-1: publish the converted model so users can actually get it.** Everything else compounds on top of those two.

| # | Finding | Severity | Status today |
|---|---------|----------|--------------|
| F1 | Converted Parakeet-Unified streaming model is correctly built, routed, and verified | — (good) | ✅ Working |
| F2 | "Streaming" replays a finished recording; no real-time partials, no latency win | **Critical** | ❌ Pseudo-streaming |
| F3 | Converted model has **no download source** (self-export only) | **Critical** | ⚠️ Effectively unavailable |
| F4 | Two parallel STT paths (hand-rolled ONNX vs sherpa) — fragile DSP, maintenance load | High | ⚠️ Both maintained |
| F5 | Nemotron 3.5 (80 ms, 40 langs, downloadable) is the stronger streaming model and is under-leveraged | High | ⚠️ Not default |
| F6 | TTS "streaming" is fragment pipelining, not sample-level streaming | Medium | ⚠️ Partial |
| F7 | sherpa-onnx WebSocket/streaming-server mode unused; HTTP-POST-per-chunk overhead | Medium | ⚠️ Custom HTTP |

---

## 1. Is the Converted Model Implemented in the Latest Version? (Verification)

**Question asked:** "make sure the model that we converted and generated is implemented in the last version of S2B2S, especially with streaming."

**Answer: Yes — the build, format, routing, and registration are all correct and verified.** Details, with evidence:

### 1.1 The export pipeline is the canonical one

- `temp_export_onnx/export_onnx_streaming.py` is **identical** (zero diff) to `sherpa-onnx/scripts/nemo/parakeet-unified-en-0.6b/export_onnx_streaming.py` (PR #3575, "Milan Leonard"). S2B2S is using upstream's own exporter, not a hand-rolled fork — the safest possible position.
- It exports `nvidia/parakeet-unified-en-0.6b` → `tokens.txt` + `encoder/decoder/joiner.int8.onnx`, with dynamic quantization (QUInt8 encoder, QInt8 decoder/joiner) and external-data weights for the encoder.
- It bakes the streaming attention-context size into the encoder via `set_default_att_context_size([left, chunk, right])` and writes the latency contract into ONNX metadata.
- Latency presets: `1120ms` (70/7/7), `560ms` (70/2/5), `240ms` (70/1/2). **`export_unified.ps1` uses `--latency 560ms`**, producing `sherpa-onnx-nemo-parakeet-unified-en-0.6b-int8-streaming-560ms/`.

### 1.2 The format matches what sherpa-onnx expects

- The exporter writes `streaming_model_type = "nemo_parakeet_unified_streaming"`, `buffered_streaming = 1`, `feat_dim = 128`, plus the chunk/left/right frame counts.
- sherpa-onnx `online-recognizer-impl.cc` matches the literal string `"nemo_parakeet_unified_streaming"` and dispatches to `online-recognizer-transducer-nemo-parakeet-unified-impl.h` + `online-transducer-nemo-parakeet-unified-model.{h,cc}` + the dedicated greedy decoder. ✅ Strings line up exactly.

### 1.3 sherpa-onnx version supports it

- sherpa-onnx CHANGELOG: **1.13.2 → "Add buffered RNNT streaming path for Parakeet Unified (#3575)"**; 1.13.0 added the export. Current `main` is well past this (1.13.3+).
- The S2B2S server comment records verification on **2026-06-15 against sherpa-onnx 1.13.2**.

### 1.4 The server auto-detects and routes it correctly

- `src-tauri/unified_parakeet_server.py` → `load_model()` checks for `tokens.txt`; if present it calls `_load_sherpa_model()` → `sherpa_onnx.OnlineRecognizer.from_transducer(...)` with endpoint detection enabled (`rule1/2/3` trailing-silence + min-utterance rules).
- It passes `feature_dim=80` as a *default only*; sherpa reads the real `feat_dim` (128 for this model, 80 for Nemotron) from each model's ONNX metadata. The code comment explicitly warns **not** to pin it to 128 (would break Nemotron). Good defensive note — keep it.
- Streaming feed (`_stream_feed_sherpa`) uses the correct streaming API: `accept_waveform` → `while is_ready: decode_streams` → `get_result` → `is_endpoint`. This is exactly right.

### 1.5 It is registered and exposed in the app

- `src-tauri/src/managers/model.rs` registers `parakeet-unified-en-0.6b-sherpa-streaming` ("INT8, 560 ms chunks, buffered RNNT with ONNX Runtime 1.26") **but only when the directory exists locally** — `hf_repo: None`, no download URL.
- Rust client `src-tauri/src/stt/unified_parakeet.rs` provides `stream_start` / `stream_feed` / `stream_end` over HTTP via `ureq`, and the transcription manager + multi-STT both call them.

**Conclusion:** the converted model is genuinely integrated. The problem is not *whether* it's wired in — it's that (a) almost nobody can obtain it (§2) and (b) the way it's *driven* throws away its streaming advantage (§3).

---

## 2. Critical Gap #1 — The Converted Model Has No Download Source

**Symptom.** `model.rs` registers the streaming model with `hf_repo: None` and gates it behind a local-directory existence check. The README's export table even lists the EOU export as "TBD." So the only way to get the flagship streaming model is to:

1. set up the `temp_export_onnx` venv (NeMo from GitHub main, kaldi-native-fbank, onnx, onnxruntime, librosa, numpy<2) — "~15 min,"
2. download/restore the 2.6 GB `nvidia/parakeet-unified-en-0.6b` checkpoint,
3. run `export_unified.ps1` (PyTorch export + 3× dynamic quantization),
4. copy the output folder into `models/STT/`.

This is fine for the maintainer; it is a wall for essentially every end user. The artifact itself is **completely distributable** — a ~624 MB int8 encoder + small decoder/joiner + `tokens.txt` — and is plain MIT-tooling output over a CC-BY-4.0 model.

**Plan.**

- **P0-1a.** Publish the 560 ms export to a HuggingFace repo (mirror the Nemotron pattern already in the registry: `csukuangfj2/sherpa-onnx-nemotron-3.5-asr-streaming-...`). Add `hf_repo` + `hf_files` (the `*.int8.onnx` set + `tokens.txt` + `encoder.weights`) to the `parakeet-unified-en-0.6b-sherpa-streaming` registry entry, and remove the local-only gate (or keep it as a fallback).
- **P0-1b.** Publish **all three latency variants** (1120/560/240 ms) as separate repos or subfolders, so users can trade latency for accuracy without re-exporting. The 240 ms build is the one that makes real-time conversation feel instant.
- **P0-1c.** Keep `temp_export_onnx/` as the reproducibility path, but in the README point users to the *download* first and the *export* second. Pin `sherpa-onnx>=1.13.2`, `onnxruntime>=1.26`, `numpy<2` in `setup_venv.*` and in the server's runtime requirements so the model can't silently fail to load on an old wheel.
- **License hygiene.** The model card must carry NVIDIA's CC-BY-4.0 attribution for parakeet-unified-en-0.6b and credit the sherpa-onnx exporter (PR #3575).

---

## 3. Critical Gap #2 — "Streaming" Is Replay, Not Real-Time

This is the heart of the request and the most important section.

### 3.1 What actually happens today

Both streaming paths operate on a **complete buffer that only exists after recording has ended:**

- `TranscriptionManager::transcribe(&self, audio: Vec<f32>)` receives the full utterance once VAD has decided the user stopped talking.
- Inside it, the `UnifiedParakeet` branch (when `parakeet_streaming_enabled == true`, **which is the default**) does:
  - `stream_start()`,
  - split the **already-finished** `audio` into 250 ms (`CHUNK_SAMPLES = 4000`) slices,
  - skip near-silent *middle* slices (RMS gate `0.002`), always keep the final slice,
  - `stream_feed(chunk)` each slice, emitting a `transcription-partial` event whenever the text grows,
  - `stream_end(&[])`, then keep whichever of the last partial / final flush is longer.
- `stt/multi_stt.rs::transcribe_python` does the same replay pattern for the EOU model.

### 3.2 Why this defeats the purpose

- **No latency benefit.** The user finishes speaking, *then* the chunks are fed back-to-back as fast as the server answers. A 560 ms (or 80 ms) chunk model gives you nothing here that the offline `/transcribe` endpoint wouldn't — the model never sees audio early.
- **No live feedback.** All `transcription-partial` events arrive in a post-utterance burst. The "real-time text overlay" described in the `stt/mod.rs` comment block does not happen in real time.
- **Endpoint detection is moot.** sherpa's `is_endpoint` / the EOU `<EOU>` token can't end a turn early because the turn is already over by the time we feed audio. Turn-end is still driven entirely by the upstream VAD stop.
- **The good part:** the *server* side is correct. `_stream_feed_sherpa` is a textbook streaming loop. The bottleneck is purely the **Rust caller**, which hands over a finished recording instead of a live tap.

### 3.3 The fix — feed the mic, not the recording

Move the streaming feed **into the live capture loop** so audio reaches the recognizer as it is produced:

- **P0-2a — Live tap.** While recording (dictation and conversation), push each captured frame (post-resample to 16 kHz mono, optionally post-VAD-gate) straight into `stream_feed` on a dedicated thread, instead of accumulating the whole buffer first. Emit `transcription-partial` as results grow — *during* speech.
- **P0-2b — Endpoint-driven turns (conversation mode).** Let sherpa's `is_endpoint` (Parakeet Unified) or the `<EOU>` token (EOU 120 m) signal turn-end and trigger the Brain call, rather than waiting on the VAD's trailing-silence timer. This is the latency unlock for conversation: the Brain can start generating the instant the user's intent is complete. Keep the VAD as a backstop and for barge-in.
- **P0-2c — Stabilization.** Real streaming partials rewrite themselves. Adopt RealtimeSTT's pattern: show "unstable" tail text greyed/italic, commit a prefix once it stops changing for N frames. Prevents the cursor/overlay from flickering.
- **P0-2d — Keep an offline confirm pass (optional).** For dictation accuracy, optionally run the existing offline `/transcribe` (or a higher-accuracy backup model) once on `stream_end`, and reconcile against the streamed text — this is the natural bridge to the existing multi-STT merge.
- **P0-2e — Persistent connection.** Per-chunk HTTP POST via `ureq` adds a round-trip per frame. With true streaming the chunk rate goes up (e.g. 80 ms chunks → ~12.5 req/s), so move to a persistent transport: a local WebSocket / Unix-domain-socket / stdin-pipe to the Python server, or sherpa-onnx's native streaming server (see §7).

### 3.4 What "done" looks like

- Speaking shows text appearing within a few hundred ms, updating live.
- In conversation mode, the Brain begins responding within ~1 chunk of the user actually finishing — not after a fixed silence timeout.
- The 240 ms Parakeet export and the 80 ms Nemotron export now produce visibly different, *felt* latencies (today they would feel identical because neither streams).

---

## 4. Gap #3 — Converge the STT Paths onto sherpa-onnx

The server maintains **three** decode paths:

1. **Unified (manual ONNX)** — `tokenizer.model` + `decoder_joint.onnx`, hand-rolled mel + greedy RNN-T.
2. **EOU (manual ONNX)** — `vocab.txt`, hand-rolled mel + decoder, `<EOU>` emission.
3. **sherpa-onnx** — `tokens.txt`, full pipeline inside `OnlineRecognizer`.

The manual paths have been a recurring source of subtle, high-effort bugs (visible across the last 20 commits):

- Mel feature extraction was wrong in three ways at once (used `|FFT|` instead of `|FFT|²`; a 512-wide Hann instead of a 400-sample Hann centered in a 512 FFT; reflect padding instead of zero) — fixed only on 2026-06-15, lifting correlation with NeMo's preprocessor from **0.925 → 0.99972**.
- The streaming greedy decoder re-encoded the whole buffer each chunk while continuing decoder state, corrupting tokens at chunk boundaries — fixed.
- Multiple `targets` dtype confusions (int32 vs int64 vs float32) across half a dozen commits.

The sherpa path, by contrast, "was already correct" because it uses `kaldi-native-fbank` and the upstream decoder. **Recommendation:**

- **P1-4a.** Export the **EOU 120 m** model to sherpa format too (README lists this as "TBD"). Then all Parakeet-family streaming runs through one well-tested engine.
- **P1-4b.** Demote the hand-rolled Unified/EOU ONNX paths to a clearly-labeled legacy/fallback (or remove once sherpa parity is confirmed for each model). This deletes hundreds of lines of fragile DSP/decoder code and an entire class of accuracy regressions.
- **P1-4c.** Add a small regression test that pins feature-parity (the JFK-sample correlation check) and the `feature_dim` guard (must stay metadata-driven, never pinned to 128).

---

## 5. Gap #4 — Model Strategy: Which Streaming Model Should Lead?

Three streaming-capable Parakeet-family models are in play. They are **not** equivalent for a real-time assistant:

| Model | Chunk / latency | Languages | Download source | Path | Best role |
|---|---|---|---|---|---|
| **Nemotron 3.5 ASR** | **80 ms** | **40** | ✅ HF (`csukuangfj2/...80ms-int8`) | sherpa | **Default streaming** (low latency, multilingual, punct+caps) |
| **Parakeet Unified 0.6B (exported)** | 560 ms (240/1120 avail.) | English | ❌ none yet (§2) | sherpa | English quality alternative |
| **Parakeet EOU 120 m** | chunked + `<EOU>` | English | ✅ HF (manual ONNX) | manual (→ port to sherpa, §4) | Cheap turn-end signal / overlay |

**Recommendation:**

- **P1-5a.** Once true streaming (§3) lands, make **Nemotron 3.5 (80 ms)** the default *streaming* STT for conversation mode: lowest latency, 40 languages, and it already has a download URL. Keep **Parakeet V3** as the default *dictation* model (offline, 25-lang, high accuracy) and **Parakeet Unified streaming** as an English-first streaming option.
- **P1-5b.** Expose a clear UI distinction between "streaming model (live partials, turn-end)" and "accuracy model (final pass)" so the multi-STT merge has a coherent mental model.
- **P1-5c.** Surface the latency/accuracy trade per model in the model picker (e.g. "80 ms · 40 langs" vs "560 ms · EN") — today the chunk size is invisible to the user.

---

## 6. Gap #5 — Streaming TTS Is Fragment Pipelining, Not Sample Streaming

### 6.1 What exists

`tts/manager.rs` paginates the reply and runs a **3-fragment pattern** for fast time-to-first-audio (TTFA): split sentence 1 at the first `.`/`!`/`?` and play it immediately; synthesize sentence 2 while sentence 1 plays; synthesize the rest while sentence 2 plays (with a word-count fallback when no boundary is found). Fragments are produced on a worker thread and appended to the `rodio` player as they become ready (`fragment_queue.rs`, `pagination.rs`).

This is a genuinely good **pipelining** design and it does reduce perceived latency. But it is **chunked synthesis**, not streaming synthesis: each fragment is a complete, separate synth call; audio is not emitted *during* the generation of a single fragment.

### 6.2 Why it matters less for STT-grade models, more for expressive ones

- For **Piper / Kokoro / Kitten** (fast, non-autoregressive), a whole short fragment synthesizes in tens of ms, so sample-level streaming buys little — fragment pipelining is the right tool.
- For **long fragments** and for **autoregressive/expressive** models (the planned **Higgs Audio v3**, or any future LLM-style TTS), the first audio sample can lag noticeably; here true streaming (emit audio as tokens/frames are generated) is the difference between "snappy" and "sluggish."

### 6.3 Plan

- **P1-6a (cheap win).** Cut the first fragment smaller still — split at the first *clause* boundary (comma / conjunction / em-dash) or a low word-count cap, not the first sentence. Pre-warm the synth backend at session start so the very first fragment doesn't pay model spin-up. Track and display TTFA in the existing telemetry (`chars_per_ms` adaptive sizing is already there).
- **P1-6b (medium).** Use **sherpa-onnx TTS** for Kokoro/Matcha/VITS with its **generated-audio callback** to stream audio chunks out *within* a fragment (sub-200 ms TTFA), instead of waiting for the whole fragment WAV. This also unifies the TTS runtime with the STT runtime (one sherpa dependency).
- **P1-6c (long).** Land **Higgs Audio v3** behind its license, wrapped as an HTTP/WebSocket server like the other backends, with **SSE/WebSocket audio streaming** (already called out in the `stt/mod.rs` planning comment) for true token-streamed expressive speech.
- **P1-6d.** For conversation mode specifically, combine §3 (Brain starts on endpoint) + §6 (TTS streams first clause) so the loop "user stops → ~first audio of reply" is dominated by Brain TTFT, not pipeline stalls.

---

## 7. Gap #6 — sherpa-onnx Server / WebSocket Mode Is Unused

S2B2S runs a **custom Python HTTP server** (`unified_parakeet_server.py`) and talks to it with one `ureq` POST per chunk. sherpa-onnx itself ships:

- a **streaming ASR WebSocket server** (`websocketpp` + `asio`) designed for exactly this live-tap use case,
- a **TTS server**,
- and 12 language bindings including **Rust** (prebuilt libs auto-fetched in `build.rs`).

**Options (evaluate in this order):**

- **P2-7a.** Keep the Python server but switch the transport to **persistent** (WebSocket or local socket / stdin pipe). Smallest change; removes per-chunk HTTP overhead once §3 raises the chunk rate.
- **P2-7b.** Replace the Python server with sherpa-onnx's **native streaming server** for the sherpa-format models. Removes the custom server for those models entirely.
- **P2-7c (most invasive, best long-term).** Use the **sherpa-onnx Rust bindings** in-process — no separate server, no IPC, no Python venv for STT at all. This is the cleanest architecture but is a real migration (and `transcribe-rs` still owns Parakeet V3 / Whisper, so the two would coexist during transition). The existing `references_comparative_analysis_md/sherpa-onnx_review.md` already frames this as "a major migration" — agree; stage it.

---

## 8. Relevant External Projects (Comparison & What to Borrow)

Beyond the projects S2B2S already credits, these are directly useful for the streaming work:

| Project | License | Why it matters here |
|---|---|---|
| **k2-fsa/sherpa-onnx** | Apache-2.0 | Already the de-facto streaming engine; source of the exporter and the `nemo_parakeet_unified_streaming` runtime. Owns the WebSocket server, TTS streaming, KWS, diarization, Rust bindings. **Primary dependency.** |
| **KoljaB/RealtimeSTT** | — | The reference real-time streaming STT loop: live mic feeding, partial **stabilization**, wake word, low-latency callbacks. Direct model for §3 (especially 3.2c). Listed in S2B2S STT references. |
| **TheSethRose/Parakeet-Realtime-Transcriber** | model CC-BY-4.0 | Segment-based **endpointing** (three-trigger policy) — good reference for turn detection, but explicitly "no streaming partials," i.e. the trap S2B2S is currently in. Contrast, not template. |
| **MaxITService/AIVORelay** | MIT | A **fork of this lineage that already added streaming STT + profiles + browser relay** (per S2B2S README "Related Projects"). Study their live-streaming approach before reimplementing. |
| **istupakov/onnx-asr** | — | Lightweight pure-ONNX ASR; alternative reference for ONNX I/O wiring and a sanity check on the manual paths being retired in §4. |
| **csukuangfj2/sherpa-onnx-nemotron-3.5-asr-streaming-0.6b-80ms-int8** | (model) | The downloadable 80 ms multilingual streaming model that should anchor the default streaming experience (§5). |
| **Higgs Audio v3** (Boson AI; GGUF + PyTorch CLI variants) | Research/Non-Commercial | Already in the S2B2S TTS roadmap; the target for true **streaming, expressive** TTS (§6c). License is the gating item. |
| **Kyutai / Moshi-style full-duplex** | (varies) | Long-horizon reference for the README's "Full-duplex conversation with AEC" roadmap item — simultaneous listen+speak. Not near-term, but the architecture in §3 (live tap) + §6 (streaming TTS) is the on-ramp. |

Existing internal docs to align with (not duplicate): `references_comparative_analysis_md/sherpa-onnx_review.md`, `..._RealtimeSTT`/`Parakeet-Realtime-Transcriber_review.md`, `transcribe-rs_review.md`, and `futuristic_analysis/02_REFERENCE_PROJECTS.md`.

---

## 9. Prioritized Roadmap

Priorities are by **leverage**, not effort. P0 items are the ones that make the converted streaming models *actually deliver streaming*.

### P0 — Make streaming real and the model obtainable
- **P0-1** Publish the converted Parakeet-Unified streaming model (all 3 latency variants) to HuggingFace; add `hf_repo`/`hf_files`; drop the local-only gate; pin `sherpa-onnx>=1.13.2`, `onnxruntime>=1.26`, `numpy<2`. *(§2)*
- **P0-2** True streaming STT: feed live mic audio to `stream_feed` during capture; emit partials in real time; drive conversation turn-end from `is_endpoint`/`<EOU>`; add partial stabilization; move to a persistent transport. *(§3)*

### P1 — Consolidate, choose the right models, tighten TTS
- **P1-4** Export EOU → sherpa format; demote/remove hand-rolled ONNX paths; add feature-parity + `feat_dim` guard tests. *(§4)*
- **P1-5** Default conversation streaming = Nemotron 3.5 (80 ms); keep Parakeet V3 for dictation, Unified-streaming for EN; expose latency/lang per model in the picker. *(§5)*
- **P1-6** TTS: smaller first fragment (clause-level) + pre-warm + TTFA telemetry; then sherpa-onnx TTS streaming callback for Kokoro/Piper. *(§6)*

### P2 — Architecture and long-horizon
- **P2-7** Persistent/WebSocket transport → evaluate sherpa-onnx native server → (long-term) in-process Rust bindings. *(§7)*
- **P2-8** Higgs Audio v3 with SSE/WebSocket streaming once licensing clears. *(§6c)*
- **P2-9** Full-duplex conversation w/ AEC (listen while speaking), building on the P0-2 live tap + P1-6 streaming TTS. *(roadmap)*

### Suggested sequencing
```
P0-1 ──► (model is downloadable)
P0-2 ──► (streaming is real) ──► P1-5 (now latency differences are felt) ──► P1-6d (snappy conversation)
P1-4 ──► (one STT engine) ──────► P2-7 (clean transport/bindings)
P1-6a/b ─► (fast TTS) ───────────► P2-8 (expressive streaming TTS) ──► P2-9 (full-duplex)
```

---

## 10. Risks, Pins, and Validation

- **Version coupling.** The whole streaming story depends on `sherpa-onnx >= 1.13.2` (the `nemo_parakeet_unified_streaming` runtime) and `onnxruntime >= 1.26` (Nemotron Conformer MHA fusion). These must be pinned in the STT venv and in any packaged Python runtime, or models will silently fail to load on older wheels. The current "verified 2026-06-15 / 1.13.2" note should become an enforced constraint.
- **Do not pin `feature_dim`.** Keep it metadata-driven (128 for Parakeet-Unified, 80 for Nemotron). Add a test so a future "optimization" can't reintroduce the bug the code comment warns about.
- **Streaming ≠ accuracy.** Lower-latency chunking (240 ms, 80 ms) trades some accuracy. The P0-2d offline-confirm pass and the existing multi-STT merge are the mitigations; surface the trade-off in the UI.
- **Partial flicker.** Without P0-2c stabilization, live partials rewriting themselves will visibly churn the cursor/overlay. Ship stabilization with the live tap, not after.
- **Transport overhead.** True streaming multiplies the request rate; landing P0-2 without P0-2e (persistent transport) will trade latency for syscall/HTTP overhead. Treat them as one unit.
- **EOU sherpa parity.** Before removing the manual EOU path (P1-4b), confirm the sherpa-exported EOU reproduces the `<EOU>` turn-end behavior the conversation loop relies on.
- **Licensing.** Parakeet/Nemotron model cards: CC-BY-4.0 attribution to NVIDIA; exporter credit to sherpa-onnx (#3575). Higgs Audio v3 stays blocked until its Research/Non-Commercial license is cleared for S2B2S's use.

---

## 11. Appendix — Evidence Map (where each claim comes from)

| Claim | Source in repo |
|---|---|
| Exporter identical to upstream | `diff temp_export_onnx/export_onnx_streaming.py` vs `sherpa-onnx/scripts/nemo/parakeet-unified-en-0.6b/export_onnx_streaming.py` → no diff |
| sherpa supports the model type | `sherpa-onnx/sherpa-onnx/csrc/online-recognizer-impl.cc` (`"nemo_parakeet_unified_streaming"`); `online-transducer-nemo-parakeet-unified-model.{h,cc}` |
| sherpa version added it | `sherpa-onnx/CHANGELOG.md` 1.13.2 "buffered RNNT streaming path for Parakeet Unified (#3575)"; 1.13.0 export |
| Server auto-detects & routes | `src-tauri/unified_parakeet_server.py` → `load_model()`, `_load_sherpa_model()`, `_stream_feed_sherpa()` |
| 560 ms default export | `temp_export_onnx/export_unified.ps1` (`--latency 560ms`) |
| Replay-not-stream (dictation/convo) | `src-tauri/src/managers/transcription.rs` ~L566–L760 (chunks `audio` after recording) |
| Replay-not-stream (multi-STT) | `src-tauri/src/stt/multi_stt.rs` ~L252–L278 |
| Receives full buffer | `TranscriptionManager::transcribe(&self, audio: Vec<f32>)` ~L461 |
| Streaming default ON | `src-tauri/src/settings.rs` `default_parakeet_streaming_enabled() → true` |
| No download source | `src-tauri/src/managers/model.rs` ~L341–L358 (`hf_repo: None`, local gate) |
| Nemotron 80 ms downloadable | `src-tauri/src/managers/model.rs` ~L293–L331 |
| Feature-extraction fixes (0.925→0.99972) | `CHANGELOG.md` "STT Streaming & Parakeet Accuracy (June 15, 2026)" |
| TTS fragment pipelining | `src-tauri/src/tts/manager.rs` ~L182–L224 (3-fragment pattern) |
| Streaming STT = "Partial" | `README.md` Roadmap table |
| sherpa WebSocket/migration framing | `references_comparative_analysis_md/sherpa-onnx_review.md` |

---

*Prepared from a direct read of `S2B2S@aa1c230` and `sherpa-onnx@6206c9c`. The converted model is correctly built and integrated; the work that remains is to (1) ship it and (2) drive it — and the streaming-capable models — as the real-time engines they were designed to be.*
