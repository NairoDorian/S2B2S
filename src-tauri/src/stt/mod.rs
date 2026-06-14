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
// Higgs Audio v3 TTS by Boson AI — expressive conversational speech.
// Architecture: 4B autoregressive decoder, 8-codebook audio tokens, 24kHz output.
//
// Two integration paths under evaluation:
//
//   1. GGUF (llama.cpp): nopesadly/higgs-audio-v3-8b-stt-v2-Q4_K_M-GGUF
//      Since S2B2S already ships llama.cpp for the Brain, GGUF TTS could
//      reuse the same runtime. Requires GGUF speech tokenizer + decoder
//      support in llama.cpp.
//
//   2. CLI (PyTorch/transformers): d6b057f/higgs-audio-v3-tts-cli
//      Pure Python command-line interface using PyTorch and transformers.
//      Could run via the same Python venv pattern as Piper/Kokoro/Kitten.
//
// Remaining blockers:
//   a. License: Boson Higgs Audio v3 Research and Non-Commercial License.
//   b. GGUF: needs llama.cpp speech-tokenizer/decoder support validation.
//   c. CLI: needs to be packaged as an HTTP server (like our other backends).
//
// Integration plan (post-license):
//   1. Evaluate GGUF with S2B2S's existing llama-server
//   2. Or wrap the PyTorch CLI in an HTTP server with model lifecycle
//   3. Map control tokens (emotion/style/prosody/sfx) to TTS pipeline
//   4. Support streaming SSE response for sub-second time-to-first-audio

// ============================================================================
// Future: Nemotron 3.5 ASR (sherpa-onnx format — IMPLEMENTED)
// ============================================================================
//
// Implemented via sherpa_onnx_server.py using sherpa-onnx's OnlineRecognizer.
// Detected by hf_repo containing "nemotron", routes to launch_nemotron().
// sherpa-onnx handles the full pipeline: mel features, encoder cache, RNNT
// decoder, beam search, tokenizer, and endpoint detection.
