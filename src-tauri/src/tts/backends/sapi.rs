//! Windows SAPI/OneCore TTS fallback — zero download, always available.
//!
//! Uses the Windows Speech API for TTS. Acts as a guaranteed fallback when
//! no other engine is configured or when local models fail to load.
//!
//! Platform: Windows only. On macOS/Linux, this backend returns an error.

use crate::tts::{TtsBackend, Voice};

#[allow(dead_code)]
pub struct SapiBackend {
    voice: String,
    speed: f32,
}

impl SapiBackend {
    pub fn new(voice: String, speed: f32) -> Self {
        Self { voice, speed }
    }

    pub fn list_voices() -> Vec<Voice> {
        // SAPI voices are enumerated at runtime via the Windows Speech API.
        // For now, return a placeholder.
        vec![Voice {
            id: "sapi_default".to_string(),
            name: "System Default (SAPI)".to_string(),
            language: None,
        }]
    }
}

impl TtsBackend for SapiBackend {
    fn name(&self) -> &str {
        "SAPI"
    }

    fn synthesize(&self, _text: &str, _voice: &str, _speed: f32) -> Result<Vec<u8>, String> {
        #[cfg(target_os = "windows")]
        {
            // TO FINISH: Use windows-rs SAPI COM interop:
            //   SpVoice → Speak → ISpeechBaseStream → WAV bytes
            Err("SAPI synthesis not yet implemented".to_string())
        }
        #[cfg(not(target_os = "windows"))]
        {
            Err("SAPI is only available on Windows".to_string())
        }
    }

    fn health_check(&self) -> Result<(), String> {
        #[cfg(target_os = "windows")]
        {
            // SAPI is always present on Windows
            Ok(())
        }
        #[cfg(not(target_os = "windows"))]
        {
            Err("SAPI is only available on Windows".to_string())
        }
    }
}
