use crate::tts::local_tts_server;
use crate::tts::status::{EngineStatus, WarmEngine};
use crate::tts::{TtsBackend, Voice};
use std::sync::atomic::{AtomicU64, Ordering};

/// Built-in speakers of the 12Hz CustomVoice checkpoints (0.6B and 1.7B share
/// this set). Used as the offline fallback; when the server is running, the
/// authoritative list is fetched from its `/voices` endpoint instead.
const QWEN3_VOICES: &[&str] = &[
    "Vivian", "Serena", "Uncle_Fu", "Dylan", "Eric", "Ryan", "Aiden", "Ono_Anna", "Sohee",
];

/// Language ids supported by every 12Hz CustomVoice checkpoint
/// (`talker_config.codec_language_id` minus the auto-applied dialect
/// entries). Offline fallback for `/languages`; "auto" = per-utterance
/// auto-detect. Dialects (beijing/sichuan) apply automatically for
/// Eric/Dylan when the language is chinese/auto.
const QWEN3_LANGUAGES: &[&str] = &[
    "auto",
    "chinese",
    "english",
    "german",
    "italian",
    "portuguese",
    "spanish",
    "japanese",
    "korean",
    "french",
    "russian",
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

        // The running server knows the loaded model's real speaker list — use
        // it when available so the picker never offers speakers the model
        // doesn't have (e.g. after switching model size or versions).
        if let Some(port) = local_tts_server::get_ready_port("qwen3") {
            let client = reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(2))
                .build()
                .unwrap_or_default();
            let url = format!("http://127.0.0.1:{port}/voices");
            if let Ok(resp) = client.get(&url).send() {
                if resp.status().is_success() {
                    match resp.json::<serde_json::Value>() {
                        Ok(json) => {
                            if let Some(list) = json.get("voices").and_then(|v| v.as_array()) {
                                let live: Vec<Voice> = list
                                    .iter()
                                    .filter_map(|v| {
                                        v.as_str().map(|s| Voice {
                                            id: s.to_string(),
                                            name: s.to_string(),
                                            language: Some("en".to_string()),
                                        })
                                    })
                                    .collect();
                                if !live.is_empty() {
                                    voices = live;
                                }
                            }
                        }
                        Err(e) => {
                            log::debug!("[Qwen3] Failed to parse /voices response: {e}");
                        }
                    }
                }
            }
        }

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

    /// List the languages the loaded model can synthesize, "auto" first.
    /// Mirrors `list_voices`: live `/languages` response wins, static
    /// checkpoint table is the offline fallback.
    pub fn list_languages() -> Vec<String> {
        let mut languages: Vec<String> = QWEN3_LANGUAGES.iter().map(|id| id.to_string()).collect();

        if let Some(port) = local_tts_server::get_ready_port("qwen3") {
            let client = reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(2))
                .build()
                .unwrap_or_default();
            let url = format!("http://127.0.0.1:{port}/languages");
            if let Ok(resp) = client.get(&url).send() {
                if resp.status().is_success() {
                    match resp.json::<serde_json::Value>() {
                        Ok(json) => {
                            if let Some(list) = json.get("languages").and_then(|v| v.as_array()) {
                                let live: Vec<String> = list
                                    .iter()
                                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                    .collect();
                                if !live.is_empty() {
                                    languages = live;
                                }
                            }
                        }
                        Err(e) => {
                            log::debug!("[Qwen3] Failed to parse /languages response: {e}");
                        }
                    }
                }
            }
        }

        languages
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

pub fn server_args(app: &tauri::AppHandle) -> Vec<String> {
    let settings = crate::settings::get_settings(app);
    let model = settings.tts.qwen3.model.clone();
    vec![
        "--backend".to_string(),
        "torch".to_string(),
        "--model".to_string(),
        model,
    ]
}

impl WarmEngine for Qwen3Backend {
    fn warm(&self) -> Result<(), String> {
        let handle = local_tts_server::ensure_running(
            "qwen3",
            "python".to_string(),
            server_args(&self.app),
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
            "Aiden"
        } else {
            voice
        };

        let handle = local_tts_server::ensure_running(
            "qwen3",
            "python".to_string(),
            server_args(&self.app),
        )?;

        let url = format!("http://127.0.0.1:{}/", handle.port);
        let cloned_wav = cloned_voices_dir(&self.app).join(format!("{voice_to_use}.wav"));
        let language = crate::settings::get_settings(&self.app)
            .tts
            .qwen3
            .language
            .clone();

        let body = if cloned_wav.is_file() {
            let mut b = serde_json::json!({
                "text": text,
                "voice": voice_to_use,
                "language": language,
                "voice_wav": cloned_wav.to_string_lossy(),
                "length_scale": 1.0,
            });
            // Sidecar ref_text (transcribed at import/record time) beats the
            // server's generic default for in-context cloning fidelity.
            let sidecar = cloned_wav.with_extension("txt");
            if let Ok(ref_text) = std::fs::read_to_string(&sidecar) {
                if !ref_text.trim().is_empty() {
                    b["voice_text"] = serde_json::Value::String(ref_text);
                }
            }
            b
        } else {
            serde_json::json!({
                "text": text,
                "voice": voice_to_use,
                "language": language,
                "length_scale": 1.0,
            })
        };

        let text_chars = text.chars().count() as u64;
        // Buffered synthesis holds the connection open for the entire
        // generation; budget ~6x the estimated speech duration.
        let est_speech_ms = (text_chars * 1000) / 12;
        let deadline_ms = (est_speech_ms * 6 + 10_000).clamp(60_000, 1_800_000);
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
