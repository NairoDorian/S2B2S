//! Persistent warm Piper TTS backend.
//!
//! Spawns Python's `piper.http_server` once, keeps the model resident in RAM,
//! and synthesizes via HTTP POST — the CopySpeak performance win. Faithful to
//! the AgentZero prototype's proven lifecycle.
//!
//! ─── Piper HTTP API Reference ────────────────────────────────────────────
//!
//! Install:
//!   python3 -m pip install piper-tts[http]
//!   python3 -m piper.download_voices en_US-lessac-medium
//!
//! Run server:
//!   python3 -m piper.http_server -m <VOICE> [--host HOST] [--port PORT] [--data-dir DIR]
//!
//! Synthesize (POST /synthesize):
//!   Fields:
//!     text         (required) - text to synthesize
//!     voice        (optional) - name of voice to use; defaults to -m <VOICE>
//!     speaker      (optional) - name of speaker for multi-speaker voices
//!     speaker_id   (optional) - id of speaker; overrides speaker
//!     length_scale (optional) - speaking speed; defaults to 1
//!     noise_scale  (optional) - speaking variability
//!     noise_w_scale(optional) - phoneme width variability
//!
//!   Example:
//!     curl -X POST -H 'Content-Type: application/json' \
//!       -d '{ "text": "Hello." }' -o test.wav localhost:5000/synthesize
//!
//! Voices (GET /voices):
//!   curl localhost:5000/voices
//!
//! Info (GET /info):
//!   curl localhost:5000/info
//!
//! Web interface:
//!   Open http://localhost:5000 in a browser.
//!
//! ──────────────────────────────────────────────────────────────────────────

use crate::tts::{TtsBackend, Voice};
use tauri::AppHandle;
use super::piper_server;

/// List available Piper voices by scanning the resolved voices directory for `*.onnx`.
pub fn list_voices(app: &AppHandle) -> Vec<Voice> {
    let dir = piper_server::resolve_piper_voices_dir(Some(app));
    let mut voices = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("onnx") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    voices.push(Voice {
                        id: stem.to_string(),
                        name: stem.to_string(),
                        language: None,
                    });
                }
            }
        }
    }
    voices.sort_by(|a, b| a.id.cmp(&b.id));
    voices
}

pub struct PiperBackend {
    app: AppHandle,
    cuda: bool,
    noise_scale: f32,
    noise_w_scale: f32,
}

impl PiperBackend {
    pub fn new(app: AppHandle, cuda: bool) -> Self {
        Self {
            app,
            cuda,
            noise_scale: 0.667,
            noise_w_scale: 0.8,
        }
    }

    pub fn with_noise(mut self, noise_scale: f32, noise_w_scale: f32) -> Self {
        self.noise_scale = noise_scale;
        self.noise_w_scale = noise_w_scale;
        self
    }
}

impl TtsBackend for PiperBackend {
    fn name(&self) -> &str {
        "piper"
    }

    fn synthesize(&self, text: &str, voice: &str, speed: f32) -> Result<Vec<u8>, String> {
        let voice_to_use = if voice.trim().is_empty() {
            "en_US-joe-medium"
        } else {
            voice
        };

        // R6: Pre-flight check — verify the voice model exists
        let voice_stem = if voice_to_use.ends_with(".onnx") {
            voice_to_use.trim_end_matches(".onnx").to_string()
        } else {
            voice_to_use.to_string()
        };
        let voices_dir = piper_server::resolve_piper_voices_dir(Some(&self.app));
        let model_path = voices_dir.join(format!("{}.onnx", voice_stem));
        if !model_path.exists() && !std::path::Path::new(voice_to_use).exists() {
            return Err(format!(
                "Piper voice model not found: {voice_to_use}\n\n\
                 Please place the .onnx and .onnx.json files in:\n  {}",
                voices_dir.display()
            ));
        }

        // 1. Ensure server is running and get handle
        let handle = piper_server::ensure_running(
            voice_to_use.to_string(),
            self.cuda,
        )?;

        let url = format!("http://127.0.0.1:{}/", handle.port);

        // R1: Map speed correctly: length_scale = 1.0 / speed
        let mut body = serde_json::json!({
            "text": text,
            "noise_scale": self.noise_scale,
            "noise_w_scale": self.noise_w_scale,
        });
        if speed > 0.0 && (speed - 1.0).abs() > f32::EPSILON {
            body["length_scale"] = serde_json::json!(1.0 / speed);
        }

        // R3: Adaptive request deadline — scales with text length
        let text_chars = text.chars().count() as u64;
        let per_char_ms = if self.cuda { 5 } else { 30 };
        let deadline_ms = (5000u64 + text_chars * per_char_ms).clamp(10_000, 180_000);
        let deadline = std::time::Duration::from_millis(deadline_ms);

        let response = handle.client
            .post(&url)
            .timeout(deadline)
            .json(&body)
            .send()
            .map_err(|e| {
                // If synthesis fails on HTTP error, unload model so next attempt is fresh
                let _ = piper_server::unload_piper_model();
                format!("Piper HTTP request failed: {e}")
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let err_text = response.text().unwrap_or_default();
            return Err(format!("Piper HTTP error {status}: {err_text}"));
        }

        let bytes = response
            .bytes()
            .map_err(|e| {
                let _ = piper_server::unload_piper_model();
                format!("Failed to read Piper response bytes: {e}")
            })?;

        Ok(bytes.to_vec())
    }

    fn health_check(&self) -> Result<(), String> {
        piper_server::resolve_python_command().map(|_| ())
    }
}
