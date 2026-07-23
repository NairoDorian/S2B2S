use crate::tts::local_tts_server;
use crate::tts::status::{EngineStatus, WarmEngine};
use crate::tts::{TtsBackend, Voice};
use std::sync::atomic::{AtomicU64, Ordering};

const QWEN3_VOICES: &[&str] = &[
    "aiden",
    "ashley",
    "ben",
    "cora",
    "daniel",
    "elsa",
    "felix",
    "grace",
    "hale",
    "iris",
    "jack",
    "katherine",
];

const CLONED_VOICES_DIR: &str = "TTS/qwen3-cloned-voices";

pub fn cloned_voices_dir(app: &tauri::AppHandle) -> std::path::PathBuf {
    crate::portable::app_data_dir(app)
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join("models")
        .join(CLONED_VOICES_DIR)
}

#[allow(dead_code)]
pub struct Qwen3Backend {
    voice: String,
    speed: f32,
    last_used: AtomicU64,
    app: tauri::AppHandle,
}

impl Qwen3Backend {
    pub fn new(app: tauri::AppHandle, voice: String, speed: f32) -> Self {
        Self {
            voice,
            speed,
            last_used: AtomicU64::new(0),
            app,
        }
    }

    pub fn list_voices(app: &tauri::AppHandle) -> Vec<Voice> {
        let mut voices: Vec<Voice> = QWEN3_VOICES
            .iter()
            .map(|id| Voice {
                id: id.to_string(),
                name: id.to_string(),
                language: Some("en".to_string()),
            })
            .collect();

        // Scan for cloned voice WAV files
        let dir = cloned_voices_dir(app);
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("wav") {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        voices.push(Voice {
                            id: stem.to_string(),
                            name: format!("🎙️ {}", stem),
                            language: Some("cloned".to_string()),
                        });
                    }
                }
            }
        }

        voices
    }

    /// Import a WAV file as a cloned voice. Copies to persistent storage.
    pub fn import_cloned_voice(
        app: &tauri::AppHandle,
        source_wav: &std::path::Path,
    ) -> Result<Voice, String> {
        let dir = cloned_voices_dir(app);
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create cloned voices dir: {e}"))?;

        let stem = source_wav
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("cloned");
        let dest = dir.join(format!("{}.wav", stem));

        std::fs::copy(source_wav, &dest).map_err(|e| format!("Failed to copy voice WAV: {e}"))?;

        Ok(Voice {
            id: stem.to_string(),
            name: format!("🎙️ {}", stem),
            language: Some("cloned".to_string()),
        })
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

impl WarmEngine for Qwen3Backend {
    fn warm(&self) -> Result<(), String> {
        let handle = local_tts_server::ensure_running(
            "qwen3",
            "python".to_string(),
            vec!["--backend".to_string(), "torch".to_string()],
        )?;
        log::info!("[Qwen3] WarmEngine: server ready on port {}", handle.port);
        Ok(())
    }

    fn unload(&self) -> Result<(), String> {
        if local_tts_server::unload("qwen3") {
            log::info!("[Qwen3] WarmEngine: model unloaded");
        }
        Ok(())
    }

    fn status(&self) -> EngineStatus {
        match local_tts_server::get_engine_status("qwen3").as_deref() {
            Some("ready") => EngineStatus::Ready,
            Some("loading") => EngineStatus::Loading,
            Some("error") => EngineStatus::Error,
            _ => EngineStatus::Stopped,
        }
    }
}

impl TtsBackend for Qwen3Backend {
    fn name(&self) -> &str {
        "Qwen3"
    }

    fn synthesize(&self, text: &str, voice: &str, _speed: f32) -> Result<Vec<u8>, String> {
        self.touch();
        let voice_to_use = if voice.trim().is_empty() {
            "aiden"
        } else {
            voice
        };

        let handle = local_tts_server::ensure_running(
            "qwen3",
            "python".to_string(),
            vec!["--backend".to_string(), "torch".to_string()],
        )?;

        let url = format!("http://127.0.0.1:{}/", handle.port);
        let cloned_wav = cloned_voices_dir(&self.app).join(format!("{voice_to_use}.wav"));

        let body = if cloned_wav.is_file() {
            serde_json::json!({
                "text": text,
                "voice": voice_to_use,
                "voice_wav": cloned_wav.to_string_lossy(),
                "length_scale": 1.0,
            })
        } else {
            serde_json::json!({
                "text": text,
                "voice": voice_to_use,
                "length_scale": 1.0,
            })
        };

        let text_chars = text.chars().count() as u64;
        let deadline_ms = (8000u64 + text_chars * 50).clamp(15_000, 300_000); // Qwen3 is slightly heavier than pocket
        let deadline = std::time::Duration::from_millis(deadline_ms);

        let response = handle
            .client
            .post(&url)
            .timeout(deadline)
            .json(&body)
            .send()
            .map_err(|e| {
                let _ = local_tts_server::unload("qwen3");
                format!("Qwen3 HTTP request failed: {e}")
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let err_text = response.text().unwrap_or_default();
            return Err(format!("Qwen3 HTTP error {status}: {err_text}"));
        }

        let bytes = response.bytes().map_err(|e| {
            let _ = local_tts_server::unload("qwen3");
            format!("Failed to read Qwen3 response bytes: {e}")
        })?;

        Ok(bytes.to_vec())
    }

    fn health_check(&self) -> Result<(), String> {
        match local_tts_server::get_engine_status("qwen3").as_deref() {
            Some("ready") => Ok(()),
            Some("loading") => Err("Qwen3 engine is still loading".to_string()),
            _ => Err("Qwen3 engine is not running".to_string()),
        }
    }
}
