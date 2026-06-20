use crate::tts::local_tts_server;
use crate::tts::status::{EngineStatus, WarmEngine};
use crate::tts::{TtsBackend, Voice};
use std::sync::atomic::{AtomicU64, Ordering};

const POCKET_VOICES: &[&str] = &[
    "alba", "marius", "javert", "jean", "fantine", "cosette", "eponine", "azelma",
];

const CLONED_VOICES_DIR: &str = "TTS/pocket-cloned-voices";

pub fn cloned_voices_dir(app: &tauri::AppHandle) -> std::path::PathBuf {
    crate::portable::app_data_dir(app)
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join("models")
        .join(CLONED_VOICES_DIR)
}

#[allow(dead_code)]
pub struct PocketBackend {
    voice: String,
    speed: f32,
    last_used: AtomicU64,
    app: tauri::AppHandle,
}

impl PocketBackend {
    pub fn new(app: tauri::AppHandle, voice: String, speed: f32) -> Self {
        Self {
            voice,
            speed,
            last_used: AtomicU64::new(0),
            app,
        }
    }

    pub fn list_voices(app: &tauri::AppHandle) -> Vec<Voice> {
        let mut voices: Vec<Voice> = POCKET_VOICES
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
                            // Use the stem as the id; synthesize() resolves it back to the
                            // WAV path. (Passing the raw path as the id made the server
                            // reject it and silently fall back to a stock voice.)
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

        let handle = local_tts_server::ensure_running("pocket", "python".to_string(), vec![])?;

        let url = format!("http://127.0.0.1:{}/", handle.port);
        // A cloned voice id is a WAV stem under the cloned-voices dir; when one exists,
        // send its absolute path so the server clones from it instead of falling back to
        // a stock voice. (Pocket can't vary speed, so length_scale stays 1.0.)
        let cloned_wav = cloned_voices_dir(&self.app).join(format!("{voice_to_use}.wav"));
        let body = if cloned_wav.is_file() {
            serde_json::json!({
                "text": text,
                "voice": voice_to_use,
                "voice_wav": cloned_wav.to_string_lossy(),
                "length_scale": 1.0,
            })
        } else {
            serde_json::json!({"text": text, "voice": voice_to_use, "length_scale": 1.0})
        };

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
