//! Concrete TTS engine backends. Piper (persistent warm local HTTP server) is
//! the default; Kokoro (in-process ONNX via tts-rs); cloud engines (OpenAI /
//! ElevenLabs / Cartesia) are added behind the same [`crate::tts::TtsBackend`] trait.

pub mod cartesia;
pub mod elevenlabs;
pub mod kitten;
pub mod kokoro;
pub mod openai;
pub mod piper;
pub mod piper_server;
pub mod pocket;
pub mod sapi;
