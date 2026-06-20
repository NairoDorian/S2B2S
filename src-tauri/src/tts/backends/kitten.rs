use crate::tts::local_tts_server;
use crate::tts::status::{EngineStatus, WarmEngine};
use crate::tts::{TtsBackend, Voice};
use std::sync::atomic::{AtomicU64, Ordering};

const KITTEN_VOICES: &[&str] = &[
    "Bella", "Jasper", "Luna", "Bruno", "Rosie", "Hugo", "Kiki", "Leo",
];

#[allow(dead_code)]
pub struct KittenBackend {
    voice: String,
    speed: f32,
    last_used: AtomicU64,
}

impl KittenBackend {
    pub fn new(voice: String, speed: f32) -> Self {
        Self {
            voice,
            speed,
            last_used: AtomicU64::new(0),
        }
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

    fn touch(&self) {
        self.last_used.store(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
            Ordering::Release,
        );
    }
}

impl WarmEngine for KittenBackend {
    fn warm(&self) -> Result<(), String> {
        let handle = local_tts_server::ensure_running("kitten", "python".to_string(), vec![])?;
        log::info!("[Kitten] WarmEngine: server ready on port {}", handle.port);
        Ok(())
    }

    fn unload(&self) -> Result<(), String> {
        if local_tts_server::unload("kitten") {
            log::info!("[Kitten] WarmEngine: model unloaded");
        }
        Ok(())
    }

    fn status(&self) -> EngineStatus {
        match local_tts_server::get_engine_status("kitten").as_deref() {
            Some("ready") => EngineStatus::Ready,
            Some("loading") => EngineStatus::Loading,
            Some("error") => EngineStatus::Error,
            _ => EngineStatus::Stopped,
        }
    }
}

impl TtsBackend for KittenBackend {
    fn name(&self) -> &str {
        "Kitten"
    }

    fn synthesize(&self, text: &str, voice: &str, _speed: f32) -> Result<Vec<u8>, String> {
        self.touch();
        let voice_to_use = if voice.trim().is_empty() {
            "Rosie"
        } else {
            voice
        };

        let handle = local_tts_server::ensure_running("kitten", "python".to_string(), vec![])?;

        let url = format!("http://127.0.0.1:{}/", handle.port);
        let body = serde_json::json!({"text": text, "voice": voice_to_use, "length_scale": 1.0});

        let text_chars = text.chars().count() as u64;
        let deadline_ms = (5000u64 + text_chars * 30).clamp(10_000, 180_000);
        let deadline = std::time::Duration::from_millis(deadline_ms);

        let response = handle
            .client
            .post(&url)
            .timeout(deadline)
            .json(&body)
            .send()
            .map_err(|e| {
                let _ = local_tts_server::unload("kitten");
                format!("Kitten HTTP request failed: {e}")
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let err_text = response.text().unwrap_or_default();
            return Err(format!("Kitten HTTP error {status}: {err_text}"));
        }

        let bytes = response.bytes().map_err(|e| {
            let _ = local_tts_server::unload("kitten");
            format!("Failed to read Kitten response bytes: {e}")
        })?;

        Ok(bytes.to_vec())
    }

    fn health_check(&self) -> Result<(), String> {
        match local_tts_server::get_engine_status("kitten").as_deref() {
            Some("ready") => Ok(()),
            Some("loading") => Err("Kitten engine is still loading".to_string()),
            _ => Err("Kitten engine is not running".to_string()),
        }
    }
}
