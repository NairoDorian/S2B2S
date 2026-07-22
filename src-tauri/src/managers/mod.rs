//! Core subsystem managers: audio recording, STT model lifecycle,
//! transcription orchestration, and history persistence.

pub mod audio;
pub mod continuous_voice;
pub mod gguf_meta;
pub mod history;
pub mod model;
pub mod model_capabilities;
pub mod moonshine_streaming_shim;
pub mod native_streaming_latency;
pub mod transcription;
