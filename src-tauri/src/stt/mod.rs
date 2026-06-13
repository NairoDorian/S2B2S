pub mod multi_stt;
pub mod unified_parakeet;

// ============================================================================
// Future: Multi-STT Pipeline Architecture
// ============================================================================
//
// The streaming model (EOU 120M) provides real-time word-by-word feedback via
// `transcription-partial` events during recording. In future iterations,
// additional STT models will run in parallel to improve accuracy:
//
//   ┌─────────────────────────────────────────────────────────┐
//   │                    Multi-STT Pipeline                    │
//   ├─────────────────────────────────────────────────────────┤
//   │  Recording starts                                        │
//   │    │                                                     │
//   │    ├─► Streaming Model (EOU 120M)                        │
//   │    │     • stream_start / stream_feed / stream_end       │
//   │    │     • Emits "transcription-partial" events           │
//   │    │     • Real-time text overlay                        │
//   │    │     • <EOU> token for end-of-utterance detection     │
//   │    │                                                     │
//   │  Recording stops                                         │
//   │    │                                                     │
//   │    ├─► Backup Model 1 (e.g. Parakeet V3)                 │
//   │    │     • Full audio → transcribe()                     │
//   │    │     • High accuracy, multi-language                  │
//   │    │                                                     │
//   │    ├─► Backup Model 2 (e.g. Whisper Large)               │
//   │    │     • Full audio → transcribe()                     │
//   │    │     • Alternative architecture for diversity         │
//   │    │                                                     │
//   │    └─► Post-Processing (LLM)                              │
//   │          • Prompt: "Given 3 transcriptions of the same    │
//   │            noisy audio, produce a clean, accurate final   │
//   │            transcript. Fix errors by cross-referencing.   │
//   │            Return only the corrected text."               │
//   │          • Merges: [streaming_result, backup1, backup2]   │
//   │          • Returns final corrected transcription          │
//   └─────────────────────────────────────────────────────────┘
//
// Implementation Plan:
// 1. MultiSttConfig in settings: { enabled, streaming_model, backup_models[],
//    post_process_prompt, post_process_provider }
// 2. Recording loop spawns streaming thread that periodically polls audio buffer
// 3. On recording stop, spawn N+1 async tasks (streaming finalize + N backups)
// 4. Collect all results via tokio::join!
// 5. Route through post-processing LLM
// 6. Return final corrected text
//
// RAM considerations: Models can be loaded on multiple backends:
//   - Streaming model: CPU (Python ONNX) — always available
//   - Backup models: GPU via CUDA/DirectML/WebGPU when available
//   - UnifiedParakeet models share the same Python server (one at a time)
//   - Non-Python models (Whisper, Parakeet V3, etc.) loaded via transcribe-rs

// ============================================================================
// Future: Higgs Audio v3 TTS (4B params, 100+ languages)
// ============================================================================
//
// Higgs Audio v3 TTS by Boson AI — expressive conversational speech with
// zero-shot voice cloning, inline emotion/style/prosody/sfx control tokens.
// Architecture: 4B autoregressive decoder, 8-codebook audio tokens, 24kHz output.
//
// Blockers:
//   1. License: Boson Higgs Audio v3 Research and Non-Commercial License.
//      Commercial use requires separate license from Boson AI.
//   2. Serving stack: Requires SGLang-Omni server (Python + GPU, ~8GB VRAM),
//      NOT a simple ONNX runtime session like Piper/Kokoro.
//   3. Integration complexity: New TTS backend type (SGLangServer) with
//      model download, server lifecycle, health check, and streaming API.
//
// Integration plan (post-license):
//   1. Download ONNX weights from onnx-community/higgs-audio-v3-tts-4b
//   2. Run SGLang-Omni server as a TTS backend (similar pattern to PiperServer)
//   3. Map control tokens to TTS pipeline: allow Brain to inject emotion/style
//   4. Support streaming SSE response for sub-second time-to-first-audio
