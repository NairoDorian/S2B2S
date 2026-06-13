// Kokoro-82M TTS backend via local persistent HTTP server.
//
// Pattern: Piper server lifecycle (Loading → WarmingUp → Ready → Error → Stopped).
// 54 voices across 9 languages, ~115 MB ONNX model, Apache 2.0.

use crate::tts::local_tts_server;
use crate::tts::status::{EngineStatus, WarmEngine};
use crate::tts::{TtsBackend, Voice};
use std::sync::atomic::{AtomicU64, Ordering};

/// Voice prefixes per language (from Kokoro voice naming convention).
const VOICE_LANGUAGE_MAP: &[(&str, &str, &[&str])] = &[
    ("en", "US", &["af_", "am_"]),
    ("en", "GB", &["bf_", "bm_"]),
    ("es", "", &["ef_"]),
    ("fr", "", &["ff_"]),
    ("hi", "", &["hf_"]),
    ("it", "", &["if_"]),
    ("ja", "", &["jf_"]),
    ("pt", "BR", &["pf_"]),
    ("zh", "", &["zf_", "zm_"]),
];

/// Known voice IDs (54 total).
const KNOWN_VOICES: &[&str] = &[
    "af_alloy", "af_aoede", "af_bella", "af_heart", "af_jessica", "af_kore",
    "af_nicole", "af_nova", "af_river", "af_sarah", "af_sky",
    "am_adam", "am_echo", "am_eric", "am_fenrir", "am_liam", "am_michael", "am_onyx", "am_puck", "am_santa",
    "bf_alice", "bf_emma", "bf_isabella", "bf_lily",
    "bm_daniel", "bm_fable", "bm_george", "bm_lewis",
    "ef_dora",
    "ff_siwis",
    "hf_alpha", "hf_beta",
    "if_sara", "if_nicola",
    "jf_alpha", "jf_gongitsune", "jf_nezumi", "jf_tebukuro",
    "pf_dora",
    "zf_xiaobei", "zf_xiaoni", "zf_xiaoxiao", "zf_xiaoyi",
    "zm_yunjian", "zm_yunxia", "zm_yunyang",
];

#[allow(dead_code)]
pub struct KokoroBackend {
    voice: String,
    speed: f32,
    last_used: AtomicU64,
}

impl KokoroBackend {
    pub fn new(voice: String, speed: f32) -> Self {
        Self {
            voice,
            speed,
            last_used: AtomicU64::new(0),
        }
    }

    pub fn list_voices() -> Vec<Voice> {
        KNOWN_VOICES
            .iter()
            .map(|id| {
                let (lang, name) = VOICE_LANGUAGE_MAP
                    .iter()
                    .find_map(|(lang_code, region, prefixes)| {
                        prefixes.iter().find(|&&pfx| id.starts_with(pfx)).map(|_| {
                            let label = if region.is_empty() {
                                lang_code.to_string()
                            } else {
                                format!("{}-{}", lang_code, region)
                            };
                            (Some(label), id.trim_start_matches(&id[..3]))
                        })
                    })
                    .unwrap_or((None, id));
                Voice {
                    id: id.to_string(),
                    name: name.to_string(),
                    language: lang,
                }
            })
            .collect()
    }

    pub fn voice_for_language(lang_code: &str) -> Option<&'static str> {
        for (code, _, prefixes) in VOICE_LANGUAGE_MAP {
            if *code == lang_code {
                return Some(prefixes[0]);
            }
        }
        None
    }

    /// Find Kokoro model paths for passing to the server script.
    /// Priority: project-local models/kokoro/ > app data dir > CWD fallbacks.
    pub fn kokoro_model_args() -> Vec<String> {
        let model_name = "kokoro-v1.0.onnx";
        let voices_name = "voices-v1.0.bin";

        let search_paths: Vec<Option<std::path::PathBuf>> = vec![
            // 1. Project-root: S2B2S/models/kokoro/ (canonical dev location)
            Some(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("models").join("kokoro")),
            // 2. CWD-based: models/kokoro/ (running from project root)
            std::env::current_dir().ok().map(|d| d.join("models").join("kokoro")),
            // 3. CWD-based: kokoro/ (legacy dev mode)
            std::env::current_dir().ok().map(|d| d.join("kokoro")),
            // 4. src-tauri local: CARGO_MANIFEST_DIR/kokoro/ (legacy)
            Some(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("kokoro")),
        ];

        let mut args = Vec::new();
        for base_path in search_paths.iter().flatten() {
            let model_path = base_path.join(model_name);
            let voices_path = base_path.join(voices_name);
            if model_path.exists() && voices_path.exists() {
                args.push("--model".to_string());
                args.push(model_path.to_string_lossy().to_string());
                args.push("--voices".to_string());
                args.push(voices_path.to_string_lossy().to_string());
                break;
            }
        }
        args
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

impl WarmEngine for KokoroBackend {
    fn warm(&self) -> Result<(), String> {
        let script_args = Self::kokoro_model_args();
        let handle = local_tts_server::ensure_running("kokoro", "python".to_string(), script_args)?;
        log::info!("[Kokoro] WarmEngine: server ready on port {}", handle.port);
        Ok(())
    }

    fn unload(&self) -> Result<(), String> {
        if local_tts_server::unload("kokoro") {
            log::info!("[Kokoro] WarmEngine: model unloaded");
        }
        Ok(())
    }

    fn status(&self) -> EngineStatus {
        match local_tts_server::get_engine_status("kokoro").as_deref() {
            Some("ready") => EngineStatus::Ready,
            Some("loading") => EngineStatus::Loading,
            Some("error") => EngineStatus::Error,
            _ => EngineStatus::Stopped,
        }
    }
}

impl TtsBackend for KokoroBackend {
    fn name(&self) -> &str {
        "Kokoro"
    }

    fn synthesize(&self, text: &str, voice: &str, _speed: f32) -> Result<Vec<u8>, String> {
        self.touch();
        let voice_to_use = if voice.trim().is_empty() {
            "af_heart"
        } else {
            voice
        };

        let script_args = Self::kokoro_model_args();
        let handle = local_tts_server::ensure_running(
            "kokoro",
            "python".to_string(),
            script_args,
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
                let _ = local_tts_server::unload("kokoro");
                format!("Kokoro HTTP request failed: {e}")
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let err_text = response.text().unwrap_or_default();
            return Err(format!("Kokoro HTTP error {status}: {err_text}"));
        }

        let bytes = response.bytes().map_err(|e| {
            let _ = local_tts_server::unload("kokoro");
            format!("Failed to read Kokoro response bytes: {e}")
        })?;

        Ok(bytes.to_vec())
    }

    fn health_check(&self) -> Result<(), String> {
        match local_tts_server::get_engine_status("kokoro").as_deref() {
            Some("ready") => Ok(()),
            Some("loading") => Err("Kokoro engine is still loading".to_string()),
            _ => Err("Kokoro engine is not running".to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_voices() {
        let voices = KokoroBackend::list_voices();
        assert_eq!(voices.len(), KNOWN_VOICES.len());
        let af_heart = voices.iter().find(|v| v.id == "af_heart").unwrap();
        assert_eq!(af_heart.language.as_deref(), Some("en-US"));
    }

    #[test]
    fn test_voice_for_language() {
        assert_eq!(KokoroBackend::voice_for_language("fr"), Some("ff_"));
        assert_eq!(KokoroBackend::voice_for_language("ja"), Some("jf_"));
        assert!(KokoroBackend::voice_for_language("de").is_none());
    }
}
