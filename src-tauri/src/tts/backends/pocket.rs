use crate::tts::local_tts_server;
use crate::tts::status::{EngineStatus, WarmEngine};
use crate::tts::{TtsBackend, Voice};
use std::sync::atomic::{AtomicU64, Ordering};

const POCKET_VOICES: &[&str] = &[
    "alba", "marius", "javert", "jean",
    "fantine", "cosette", "eponine", "azelma",
];

#[allow(dead_code)]
pub struct PocketBackend {
    voice: String,
    speed: f32,
    last_used: AtomicU64,
}

impl PocketBackend {
    pub fn new(voice: String, speed: f32) -> Self {
        Self {
            voice,
            speed,
            last_used: AtomicU64::new(0),
        }
    }

    pub fn list_voices() -> Vec<Voice> {
        POCKET_VOICES
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

impl WarmEngine for PocketBackend {
    fn warm(&self) -> Result<(), String> {
        let handle = local_tts_server::ensure_running("pocket", "python".to_string(), vec![])?;
        log::info!("[Pocket] WarmEngine: server ready on port {}", handle.port);
        Ok(())
    }

    fn unload(&self) -> Result<(), String> {
        if local_tts_server::unload("pocket") {
            log::info!("[Pocket] WarmEngine: model unloaded");
        }
        Ok(())
    }

    fn status(&self) -> EngineStatus {
        match local_tts_server::get_engine_status("pocket").as_deref() {
            Some("ready") => EngineStatus::Ready,
            Some("loading") => EngineStatus::Loading,
            Some("error") => EngineStatus::Error,
            _ => EngineStatus::Stopped,
        }
    }
}

impl TtsBackend for PocketBackend {
    fn name(&self) -> &str {
        "Pocket"
    }

    fn synthesize(&self, text: &str, voice: &str, _speed: f32) -> Result<Vec<u8>, String> {
        self.touch();
        let voice_to_use = if voice.trim().is_empty() {
            "alba"
        } else {
            voice
        };

        let handle = local_tts_server::ensure_running(
            "pocket",
            "python".to_string(),
            vec![],
        )?;

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
                let _ = local_tts_server::unload("pocket");
                format!("Pocket HTTP request failed: {e}")
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let err_text = response.text().unwrap_or_default();
            return Err(format!("Pocket HTTP error {status}: {err_text}"));
        }

        let bytes = response.bytes().map_err(|e| {
            let _ = local_tts_server::unload("pocket");
            format!("Failed to read Pocket response bytes: {e}")
        })?;

        Ok(bytes.to_vec())
    }

    fn health_check(&self) -> Result<(), String> {
        match local_tts_server::get_engine_status("pocket").as_deref() {
            Some("ready") => Ok(()),
            Some("loading") => Err("Pocket engine is still loading".to_string()),
            _ => Err("Pocket engine is not running".to_string()),
        }
    }
}
