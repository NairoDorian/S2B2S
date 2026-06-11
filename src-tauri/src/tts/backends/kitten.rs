//! Kitten TTS backend — ultra-light ONNX engine, 8 built-in English voices.
//!
//! Phase 1: Python CLI subprocess (same pattern as Piper server).
//! Phase 2: Direct ONNX Runtime inference via `ort` crate.
//!
//! Model sizes: small (~25 MB), medium (~100 MB), large (~200 MB).
//! License: Apache 2.0.

use crate::tts::{TtsBackend, Voice};

const KITTEN_VOICES: &[&str] = &["alba", "elias", "hasper", "jill", "kasper", "melina", "nimitz", "sarah"];

pub struct KittenBackend {
    voice: String,
    speed: f32,
}

impl KittenBackend {
    pub fn new(voice: String, speed: f32) -> Self {
        Self { voice, speed }
    }

    pub fn list_voices() -> Vec<Voice> {
        KITTEN_VOICES
            .iter()
            .map(|id| Voice {
                id: id.to_string(),
                name: id.to_string(),
                language: Some("en".to_string()),
            })
            .collect()
    }
}

impl TtsBackend for KittenBackend {
    fn name(&self) -> &str {
        "Kitten"
    }

    fn synthesize(&self, _text: &str, _voice: &str, _speed: f32) -> Result<Vec<u8>, String> {
        // TODO: Spawn Python `kittentts-cli.py --voice {voice} --text "{text}"`
        // once the CLI adapter is created. For now, returns a clear error.
        Err("Kitten TTS requires the Python CLI adapter.\n\
             Run: install-kittentts.ps1 to set up the engine.".to_string())
    }

    fn health_check(&self) -> Result<(), String> {
        // Probe: does python + kittentts module exist?
        Err("Kitten TTS not yet configured".to_string())
    }
}
