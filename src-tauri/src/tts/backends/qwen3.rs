use crate::tts::local_tts_server;
use crate::tts::status::{EngineStatus, WarmEngine};
use crate::tts::{TtsBackend, Voice};
use std::sync::atomic::{AtomicU64, Ordering};

const QWEN3_VOICES: &[&str] = &[
    "Aiden",
    "Ashley",
    "Ben",
    "Cora",
    "Daniel",
    "Elsa",
    "Felix",
    "Grace",
    "Hale",
    "Iris",
    "Jack",
    "Katherine",
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

pub fn server_args(app: &tauri::AppHandle) -> Vec<String> {
    let settings = crate::settings::get_settings(app);
    let model = settings.tts.qwen3.model.clone();
    vec![
        "--backend".to_string(),
        "torch".to_string(),
        "--model".to_string(),
        model,
        // ~333ms of audio per streamed chunk — fast time-to-first-audio.
        "--chunk-size".to_string(),
        "4".to_string(),
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

    fn supports_streaming(&self) -> bool {
        true
    }

    fn synthesize_streaming(
        &self,
        text: &str,
        voice: &str,
        _speed: f32,
        on_pcm: &mut dyn FnMut(u32, Vec<i16>),
    ) -> Result<(), String> {
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

        let mut body = serde_json::json!({
            "text": text,
            "voice": voice_to_use,
            "length_scale": 1.0,
            "stream": true,
        });
        if cloned_wav.is_file() {
            body["voice_wav"] =
                serde_json::Value::String(cloned_wav.to_string_lossy().into_owned());
            // A recorded/imported reference carries a transcribed sidecar so
            // in-context cloning gets the true ref_text instead of the generic
            // default (much better clone fidelity).
            let sidecar = cloned_wav.with_extension("txt");
            if let Ok(ref_text) = std::fs::read_to_string(&sidecar) {
                if !ref_text.trim().is_empty() {
                    body["voice_text"] = serde_json::Value::String(ref_text);
                }
            }
        }

        let text_chars = text.chars().count() as u64;
        let deadline_ms = (8000u64 + text_chars * 50).clamp(15_000, 300_000);
        let deadline = std::time::Duration::from_millis(deadline_ms);

        let response = handle
            .client
            .post(&url)
            .timeout(deadline)
            .json(&body)
            .send()
            .map_err(|e| {
                let _ = local_tts_server::unload("qwen3");
                format!("Qwen3 streaming HTTP request failed: {e}")
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let err_text = response.text().unwrap_or_default();
            return Err(format!("Qwen3 streaming HTTP error {status}: {err_text}"));
        }

        // Protocol: one JSON header line, then raw little-endian i16 mono PCM.
        // The header carries the sample rate; frames stream until the server
        // closes the response. The client is reqwest::blocking, so read the
        // body incrementally with std::io::Read — chunks are pushed to the
        // player as soon as they arrive.
        use std::io::Read;
        let mut response = response;
        let mut pending: Vec<u8> = Vec::new();
        let mut sample_rate: Option<u32> = None;
        let mut first = true;
        let mut buf = [0u8; 8192];

        loop {
            match response.read(&mut buf) {
                Ok(0) => break, // connection closed — stream complete
                Ok(n) => {
                    pending.extend_from_slice(&buf[..n]);

                    // Parse the header line on the first data.
                    if first {
                        if let Some(nl) = pending.iter().position(|&b| b == b'\n') {
                            let header_bytes: Vec<u8> = pending.drain(..=nl).collect();
                            let header_str = String::from_utf8_lossy(&header_bytes);
                            let header: serde_json::Value = serde_json::from_str(header_str.trim())
                                .map_err(|e| format!("Qwen3 bad stream header: {e}"))?;
                            sample_rate = header
                                .get("sample_rate")
                                .and_then(|v| v.as_u64())
                                .map(|v| v as u32);
                            first = false;
                        }
                        if first {
                            // Header not complete yet — wait for more bytes.
                            continue;
                        }
                    }

                    let sr = sample_rate.ok_or("Qwen3 stream header missing sample_rate")?;
                    let usable = pending.len() - (pending.len() % 2);
                    if usable > 0 {
                        let frames: Vec<i16> = pending[..usable]
                            .chunks_exact(2)
                            .map(|c| i16::from_le_bytes([c[0], c[1]]))
                            .collect();
                        pending.drain(..usable);
                        if !frames.is_empty() {
                            on_pcm(sr, frames);
                        }
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => {
                    let _ = local_tts_server::unload("qwen3");
                    return Err(format!("Qwen3 stream read error: {e}"));
                }
            }
        }

        if first || sample_rate.is_none() {
            let _ = local_tts_server::unload("qwen3");
            return Err("Qwen3 stream closed before sending audio".to_string());
        }

        Ok(())
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

        let body = if cloned_wav.is_file() {
            let mut b = serde_json::json!({
                "text": text,
                "voice": voice_to_use,
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
