use crate::audiocpp_server::{
    ensure_running, get_engine_status, list_voices as server_list_voices, unload as server_unload,
};
use crate::settings::get_settings;
use crate::tts::status::{EngineStatus, WarmEngine};
use crate::tts::{TtsBackend, Voice};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::AppHandle;

pub struct AudioCppBackend {
    app: AppHandle,
    voice: String,
    speed: f32,
    last_used: AtomicU64,
}

impl AudioCppBackend {
    pub fn new(app: AppHandle, voice: String, speed: f32) -> Self {
        Self {
            app,
            voice,
            speed,
            last_used: AtomicU64::new(0),
        }
    }

    pub fn list_voices(app: &AppHandle) -> Vec<Voice> {
        let settings = get_settings(app);
        let model = if settings.tts.audiocpp.model.is_empty() {
            "supertonic"
        } else {
            &settings.tts.audiocpp.model
        };

        let mut voices = match model {
            "supertonic" => vec![
                Voice {
                    id: "M1".to_string(),
                    name: "M1 (Male 1)".to_string(),
                    language: Some("en".to_string()),
                },
                Voice {
                    id: "M2".to_string(),
                    name: "M2 (Male 2)".to_string(),
                    language: Some("en".to_string()),
                },
                Voice {
                    id: "M3".to_string(),
                    name: "M3 (Male 3)".to_string(),
                    language: Some("en".to_string()),
                },
                Voice {
                    id: "M4".to_string(),
                    name: "M4 (Male 4)".to_string(),
                    language: Some("en".to_string()),
                },
                Voice {
                    id: "M5".to_string(),
                    name: "M5 (Male 5)".to_string(),
                    language: Some("en".to_string()),
                },
                Voice {
                    id: "F1".to_string(),
                    name: "F1 (Female 1)".to_string(),
                    language: Some("en".to_string()),
                },
                Voice {
                    id: "F2".to_string(),
                    name: "F2 (Female 2)".to_string(),
                    language: Some("en".to_string()),
                },
                Voice {
                    id: "F3".to_string(),
                    name: "F3 (Female 3)".to_string(),
                    language: Some("en".to_string()),
                },
                Voice {
                    id: "F4".to_string(),
                    name: "F4 (Female 4)".to_string(),
                    language: Some("en".to_string()),
                },
                Voice {
                    id: "F5".to_string(),
                    name: "F5 (Female 5)".to_string(),
                    language: Some("en".to_string()),
                },
            ],
            "qwen3_tts" => vec![
                Voice {
                    id: "Vivian".to_string(),
                    name: "Vivian".to_string(),
                    language: Some("en".to_string()),
                },
                Voice {
                    id: "Serena".to_string(),
                    name: "Serena".to_string(),
                    language: Some("en".to_string()),
                },
                Voice {
                    id: "Uncle_Fu".to_string(),
                    name: "Uncle Fu".to_string(),
                    language: Some("en".to_string()),
                },
                Voice {
                    id: "Dylan".to_string(),
                    name: "Dylan".to_string(),
                    language: Some("en".to_string()),
                },
                Voice {
                    id: "Eric".to_string(),
                    name: "Eric".to_string(),
                    language: Some("en".to_string()),
                },
                Voice {
                    id: "Ryan".to_string(),
                    name: "Ryan".to_string(),
                    language: Some("en".to_string()),
                },
                Voice {
                    id: "Aiden".to_string(),
                    name: "Aiden".to_string(),
                    language: Some("en".to_string()),
                },
                Voice {
                    id: "Ono_Anna".to_string(),
                    name: "Ono Anna".to_string(),
                    language: Some("en".to_string()),
                },
                Voice {
                    id: "Sohee".to_string(),
                    name: "Sohee".to_string(),
                    language: Some("en".to_string()),
                },
            ],
            "pocket_tts" => vec![
                Voice {
                    id: "alba".to_string(),
                    name: "Alba".to_string(),
                    language: Some("en".to_string()),
                },
                Voice {
                    id: "cosette".to_string(),
                    name: "Cosette".to_string(),
                    language: Some("en".to_string()),
                },
                Voice {
                    id: "marius".to_string(),
                    name: "Marius".to_string(),
                    language: Some("en".to_string()),
                },
                Voice {
                    id: "fantine".to_string(),
                    name: "Fantine".to_string(),
                    language: Some("en".to_string()),
                },
                Voice {
                    id: "eponine".to_string(),
                    name: "Eponine".to_string(),
                    language: Some("en".to_string()),
                },
                Voice {
                    id: "valjean".to_string(),
                    name: "Valjean".to_string(),
                    language: Some("en".to_string()),
                },
                Voice {
                    id: "javert".to_string(),
                    name: "Javert".to_string(),
                    language: Some("en".to_string()),
                },
                Voice {
                    id: "enjolras".to_string(),
                    name: "Enjolras".to_string(),
                    language: Some("en".to_string()),
                },
            ],
            _ => vec![Voice {
                id: "default".to_string(),
                name: "Default".to_string(),
                language: Some("en".to_string()),
            }],
        };

        let live_voices = server_list_voices(app, model);
        if live_voices.len() > 1 {
            voices = live_voices;
        }

        voices
    }

    fn touch_last_used(&self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.last_used.store(now, Ordering::Relaxed);
    }
}

impl TtsBackend for AudioCppBackend {
    fn name(&self) -> &'static str {
        "audiocpp"
    }

    fn health_check(&self) -> Result<(), String> {
        let settings = get_settings(&self.app);
        let backend_pref = if settings.tts.audiocpp.backend.is_empty() {
            "cuda"
        } else {
            &settings.tts.audiocpp.backend
        };
        let _ = ensure_running(&self.app, backend_pref)?;
        Ok(())
    }

    fn synthesize(&self, text: &str, voice: &str, speed: f32) -> Result<Vec<u8>, String> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Err("Cannot synthesize empty text".to_string());
        }

        let settings = get_settings(&self.app);
        let model_id = if settings.tts.audiocpp.model.is_empty() {
            "supertonic".to_string()
        } else {
            settings.tts.audiocpp.model.clone()
        };
        let backend_pref = if settings.tts.audiocpp.backend.is_empty() {
            "cuda".to_string()
        } else {
            settings.tts.audiocpp.backend.clone()
        };

        let handle = ensure_running(&self.app, &backend_pref)?;
        let url = format!("http://127.0.0.1:{}/v1/audio/speech", handle.port);

        let selected_voice = match model_id.as_str() {
            "supertonic" => {
                if voice.starts_with('M') || voice.starts_with('F') {
                    voice
                } else if self.voice.starts_with('M') || self.voice.starts_with('F') {
                    &self.voice
                } else {
                    "M1"
                }
            }
            "qwen3_tts" => {
                if [
                    "Vivian", "Serena", "Uncle_Fu", "Dylan", "Eric", "Ryan", "Aiden", "Ono_Anna",
                    "Sohee",
                ]
                .contains(&voice)
                {
                    voice
                } else {
                    "Vivian"
                }
            }
            "pocket_tts" => {
                if [
                    "alba", "cosette", "marius", "fantine", "eponine", "valjean", "javert",
                    "enjolras",
                ]
                .contains(&voice)
                {
                    voice
                } else {
                    "alba"
                }
            }
            "chatterbox" => {
                if [
                    "demo_1_man",
                    "demo_2_man",
                    "demo_3_woman",
                    "demo_4_woman",
                    "default",
                ]
                .contains(&voice)
                {
                    voice
                } else if !self.voice.is_empty() {
                    &self.voice
                } else {
                    "demo_1_man"
                }
            }
            _ => {
                if !voice.is_empty() {
                    voice
                } else if !self.voice.is_empty() {
                    &self.voice
                } else {
                    "default"
                }
            }
        };

        let effective_speed = if speed > 0.0 { speed } else { self.speed };

        let payload = serde_json::json!({
            "model": model_id,
            "input": trimmed,
            "voice": selected_voice,
            "speed": effective_speed,
            "response_format": "wav"
        });

        log::info!(
            "[AudioCpp] Synthesizing {} chars with model '{}', voice '{}' on port {}",
            trimmed.len(),
            model_id,
            selected_voice,
            handle.port
        );

        let resp = handle
            .client
            .post(&url)
            .json(&payload)
            .timeout(Duration::from_secs(60))
            .send()
            .map_err(|e| format!("HTTP request to audiocpp_server failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            if body.contains("unknown model id") {
                return Err(format!(
                    "Model '{}' is not installed yet on disk. Please select an installed model (Supertonic 3, Qwen3-TTS, PocketTTS) or wait for its download to finish.",
                    model_id
                ));
            }
            return Err(format!("audiocpp_server returned HTTP {status}: {body}"));
        }

        let bytes = resp
            .bytes()
            .map_err(|e| format!("Failed to read audio bytes from audiocpp_server: {e}"))?
            .to_vec();

        self.touch_last_used();
        Ok(bytes)
    }
}

impl WarmEngine for AudioCppBackend {
    fn warm(&self) -> Result<(), String> {
        let settings = get_settings(&self.app);
        let backend_pref = if settings.tts.audiocpp.backend.is_empty() {
            "cuda"
        } else {
            &settings.tts.audiocpp.backend
        };
        let _ = ensure_running(&self.app, backend_pref)?;
        Ok(())
    }

    fn unload(&self) -> Result<(), String> {
        server_unload();
        Ok(())
    }

    fn status(&self) -> EngineStatus {
        match get_engine_status().as_deref() {
            Some("ready") => EngineStatus::Ready,
            Some("loading") => EngineStatus::Loading,
            Some("error") => EngineStatus::Error,
            _ => EngineStatus::Stopped,
        }
    }
}
