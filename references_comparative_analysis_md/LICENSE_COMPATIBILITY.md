# License Compatibility Matrix

> For S2B2S development — what code can be directly reused vs concept-only.

---

## License Color Key

| Color | Meaning |
|-------|---------|
| 🟢 **MIT / Apache-2.0 / Unlicense** | Code CAN be copied with proper attribution |
| 🟡 **GPL-3.0** | Code CANNOT be copied into MIT projects (S2B2S is MIT). Concepts CAN be learned. |
| 🔴 **AGPL-3.0** | Even stricter than GPL. Zero code can cross. Concepts only. |
| ⚪ **No license** | All rights reserved. Study only. Do not copy any code. |

---

## Project License Matrix

### 🟢 MIT Licensed — Code Reusable

| Project | License | What S2B2S already copied | What S2B2S can still copy |
|---------|---------|--------------------------|--------------------------|
| **Handy** | MIT | Entire Tauri skeleton, shortcuts, VAD, STT, managers, settings, i18n, tray, overlay | Pause/resume not yet ported; on-device Apple Intelligence bridge |
| **Parler** | MIT | Long-audio model switching, Gemini provider pattern | Pause/resume, crash logging, settings export/import, bindings backfill |
| **AIVORelay** | MIT | Nothing yet (too divergent for wholesale merge) | Streaming WS managers, profiles, system audio, AI Replace, prompt variables, RNNoise, connector crypto |
| **Parrot** | MIT | TTS managers pattern, espeak-ng resource packaging | shorten_first_chunk, crossfade, AX-API selection capture, pulldown-cmark normalizer, engine pool |
| **CopySpeak** | MIT | TtsBackend trait, Piper persistent server, pagination, pre-warm, lock_or_recover | Telemetry-driven progress, audio caching, effects system, control server |
| **voicebox** | MIT | Nothing yet | MCP server pattern, ModelConfig registry, voice cloning (RVC) |
| **vox** | MIT | Nothing yet | Qwen3-TTS vendor integration, Raspberry Pi optimizations |
| **vibevoice-rs** | MIT | Nothing yet | Monorepo structure, SSE streaming protocol, PyTorch RNG parity |
| **TTS-Audio-Suite** | MIT | Nothing yet | Engine registry, streaming coordinator, viseme analysis |
| **voirs** | MIT | Nothing yet | Pure-Rust ONNX integration, crate organization, SciRS2 abstractions |
| **pocket-tts-server** | MIT | Integrated via pocket_server.py | (already integrated) |
| **TD_Web_Trail** | MIT | Nothing yet (planned for overlay phase) | Spring-friction physics, 4-pass glow, Catmull-Rom |
| **CursorFX** | MIT | Nothing yet (planned for overlay phase) | wgpu transparent overlay, Vulkan+NVAPI, per-frame click-through |
| **LocalAI** | MIT | Nothing yet | YAML model configs, provider option, GPU auto-detection, 36+ backend patterns |

### 🟡 GPL-3.0 — Concept Donor Only

| Project | License | What CANNOT be copied | What CAN be learned |
|---------|---------|----------------------|---------------------|
| **TranscriptionSuite** | GPL-3.0 | Any code | Persist-before-deliver, whisper.cpp Vulkan sidecar, watch-folder, diarization review, in-app updater patterns |

### 🔴 AGPL-3.0 — Concept Donor Only

| Project | License | What CANNOT be copied | What CAN be learned |
|---------|---------|----------------------|---------------------|
| **whispering** | AGPL-3.0 | Any code | Provider matrix pattern (`as const satisfies` registry), transformation pipeline, accessibility-first design |

### ⚪ No License — Study Only

| Project | License | What CANNOT be copied | What CAN be learned |
|---------|---------|----------------------|---------------------|
| **Parakeet-Realtime-Transcriber** | None | Any code | Three-trigger VAD endpointing, producer/consumer threading, duplicate filter |

### 🟢 Other Permissive Licenses

| Project | License | Code Reusable? |
|---------|---------|---------------|
| **transcribe-rs** | MIT | ✅ Already a dependency |
| **sherpa-onnx** | Apache-2.0 | ✅ Reusable with attribution |
| **onnx-asr** | MIT | ✅ Reusable with attribution |
| **speech-recognition** | MIT | ✅ Reusable with attribution |
| **speechbrain** | Apache-2.0 | ✅ Reusable with attribution |

---

## Key License Takeaways

1. **The Handy family is MIT.** All 4 forks (Handy, Parler, AIVORelay, Parrot) are safe to copy code from.
2. **CopySpeak and most independent projects are MIT.** The biggest concept donors are also code donors.
3. **Only 3 projects have restrictive licenses:** TranscriptionSuite (GPL), whispering (AGPL), Parakeet-RT (no license).
4. **S2B2S is MIT.** Inherited from Handy. Can't incorporate GPL/AGPL code.
5. **The harvest list in the comparative analysis accounts for licenses** — all TIER 1-2 features come from MIT projects.

---

## If You Want to Use GPL/AGPL Concepts Legally

1. **Read the code, understand the concept.**
2. **Close the code. Do not reference it while writing.**
3. **Write a clean-room implementation** of the concept in S2B2S.
4. **Never copy-paste any GPL/AGPL code** into the S2B2S repository.
5. **Document the inspiration** (e.g., "inspired by the persist-before-deliver pattern in TranscriptionSuite") but never the code.

This is legally sound and common practice in open-source development.

---

*Generated June 2026. Not legal advice — consult a lawyer for definitive guidance.*
