//! Concrete TTS engine backends. Piper (persistent warm local HTTP server) is
//! the default; cloud engines (OpenAI / ElevenLabs / Cartesia) are added behind
//! the same [`crate::tts::TtsBackend`] trait.

pub mod piper;
pub mod piper_server;
pub mod openai;
pub mod elevenlabs;
pub mod cartesia;
