# References Comparative Analysis — Folder Index

> Generated: June 14, 2026 | 22 projects analyzed | All files read by dedicated agents

---

## Quick Navigation

| File | What it contains |
|------|-----------------|
| **`00_COMPARATIVE_ANALYSIS.md`** | **START HERE** — cross-project comparison, feature matrices, harvest priorities, final synthesis |
| `ANALYSIS_TEMPLATE.md` | The standardized template all 22 analyses follow |
| `README.md` | This file — folder index and reading guide |

---

## Individual Project Reviews

### STT (Speech-to-Text)

| Project | File | Type | License | Fork of | Role for S2B2S |
|---------|------|------|---------|---------|---------------|
| Handy | [`Handy_review.md`](Handy_review.md) | App | MIT | — | **The skeleton** — Tauri shell, shortcuts, VAD, STT, i18n |
| Parler | [`Parler_review.md`](Parler_review.md) | Fork | MIT | Handy | Pause/resume, long-audio, settings backfill, crash log |
| AIVORelay | [`AIVORelay_review.md`](AIVORelay_review.md) | Fork | MIT | Handy | Streaming STT, profiles, system audio, AI Replace |
| Parakeet-RT | [`Parakeet-Realtime-Transcriber_review.md`](Parakeet-Realtime-Transcriber_review.md) | Reference | None | — | Three-trigger VAD endpointing, threading model |
| TranscriptionSuite | [`TranscriptionSuite_review.md`](TranscriptionSuite_review.md) | App | **GPL** | — | Persist-before-deliver, Vulkan sidecar (concept only) |
| whispering | [`whispering_review.md`](whispering_review.md) | App | **AGPL** | — | Provider matrix pattern (concept only) |

### LLM / Brain / Local AI Infrastructure

| Project | File | Type | License | Fork of | Role for S2B2S |
|---------|------|------|---------|---------|---------------|
| LocalAI | [`LocalAI_review.md`](LocalAI_review.md) | Server | MIT | — | OpenAI-compatible local AI server, potential Brain backend alternative |

### TTS (Text-to-Speech)

| Project | File | Type | License | Fork of | Role for S2B2S |
|---------|------|------|---------|---------|---------------|
| Parrot | [`Parrot_review.md`](Parrot_review.md) | Fork | MIT | Handy | **The TTS proof** — Kokoro in-process, shorten-first-chunk |
| CopySpeak TTS | [`copyspeak-tts_review.md`](copyspeak-tts_review.md) | App | MIT | — | TtsBackend trait, Piper server, telemetry |
| voicebox | [`voicebox_review.md`](voicebox_review.md) | App | MIT | — | MCP server, voice cloning (RVC), multi-engine Python |
| vox | [`vox_review.md`](vox_review.md) | App | MIT | — | Qwen3-TTS vendor, Raspberry Pi optimizations |
| vibevoice-rs | [`vibevoice-rs_review.md`](vibevoice-rs_review.md) | Monorepo | MIT | — | Pure-Rust TTS, 5-crate workspace, Candle |
| TTS-Audio-Suite | [`TTS-Audio-Suite_review.md`](TTS-Audio-Suite_review.md) | Framework | MIT | — | 13+ engines, streaming coordinator, RVC |
| voirs | [`voirs_review.md`](voirs_review.md) | Framework | MIT | — | 16-crate Rust workspace, full pipeline from G2P to ASR |
| pocket-tts-server | [`pocket-tts-server_review.md`](pocket-tts-server_review.md) | Server | MIT | — | Simplest TTS server, voice cloning from WAV |

### Libraries & Frameworks

| Project | File | Type | Language | Role for S2B2S |
|---------|------|------|----------|---------------|
| transcribe-rs | [`transcribe-rs_review.md`](transcribe-rs_review.md) | Library | Rust | **Core STT dependency** — 7 engine families |
| sherpa-onnx | [`sherpa-onnx_review.md`](sherpa-onnx_review.md) | Framework | C++ | 26+ models, 7 OSes, design reference |
| onnx-asr | [`onnx-asr_review.md`](onnx-asr_review.md) | Library | Python | ONNX ASR reference (15+ models) |
| speech-recognition | [`speech-recognition_review.md`](speech-recognition_review.md) | Library | TypeScript | Browser-based ONNX ASR runtime |
| speechbrain | [`speechbrain_review.md`](speechbrain_review.md) | Framework | Python | Definitive PyTorch speech toolkit (70K+ LOC) |

### Visual / Overlay / Physics

| Project | File | Type | Role for S2B2S |
|---------|------|------|---------------|
| CursorFX | [`Cross_Platform_Rust_WebGPU_CursorFX_review.md`](Cross_Platform_Rust_WebGPU_CursorFX_review.md) | App | **GPU overlay blueprint** — Tauri + wgpu transparent window |
| TD_Web_Trail | [`TD_Web_Trail_review.md`](TD_Web_Trail_review.md) | Reference | **Tether physics** — spring-friction, 4-pass glow, Catmull-Rom |

---

## Recommended Reading Order

### If you're a developer working on S2B2S:
1. `00_COMPARATIVE_ANALYSIS.md` — the big picture and priority list
2. `Parrot_review.md` — TTS latency techniques to adopt
3. `copyspeak-tts_review.md` — engine architecture tradeoffs
4. `Cross_Platform_Rust_WebGPU_CursorFX_review.md` — overlay implementation reference
5. `AIVORelay_review.md` — feature ideas for upcoming phases

### If you're evaluating which features to add next:
1. `00_COMPARATIVE_ANALYSIS.md` → Section 10 (Harvest Priority Matrix)
2. The individual reviews for the feature sources listed in each tier

### If you want to understand the Handy family lineage:
1. `Handy_review.md` — the original
2. `Parler_review.md` — the gentle fork
3. `AIVORelay_review.md` — the wild fork
4. `Parrot_review.md` — the inverted fork
5. Then see how S2B2S merged them all

### If you're building TTS:
1. `00_COMPARATIVE_ANALYSIS.md` → Section 2 (TTS Ecosystem)
2. `copyspeak-tts_review.md` — engine abstraction design
3. `Parrot_review.md` — latency engineering
4. `voicebox_review.md` — voice cloning
5. `vox_review.md` — Qwen3 vendor integration

---

## Statistics

| Metric | Value |
|--------|-------|
| Number of project analyses | 23 |
| Total analysis content | ~910 KB |
| Estimated total lines | ~10,300 |
| Deepest analysis | TD_Web_Trail_review.md (661 lines) |
| Projects with code reusable by S2B2S (MIT) | 12 |
| Projects that are concept donors only (GPL/AGPL/no-license) | 4 |
| Projects already integrated into S2B2S | 2 (transcribe-rs, pocket-tts-server) |
| Projects planned for integration | 2 (CursorFX, TD_Web_Trail) |

---

*All analyses generated June 2026 by dedicated OpenCode explore agents reading every source file of every project.*
