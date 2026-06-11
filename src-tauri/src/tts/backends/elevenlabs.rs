use super::super::TtsBackend;
use crate::settings::{ElevenLabsConfig, ElevenLabsOutputFormat};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;

/// Voice information from ElevenLabs API
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct ElevenLabsVoice {
    pub voice_id: String,
    pub name: Option<String>,
    pub category: Option<String>,
    pub labels: Option<serde_json::Value>,
    pub description: Option<String>,
    pub preview_url: Option<String>,
}

/// Voice settings for ElevenLabs TTS
#[derive(Debug, Clone, Serialize)]
pub struct VoiceSettings {
    pub stability: f32,
    pub similarity_boost: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_speaker_boost: Option<bool>,
}

impl Default for VoiceSettings {
    fn default() -> Self {
        Self {
            stability: 0.5,
            similarity_boost: 0.75,
            style: None,
            use_speaker_boost: None,
        }
    }
}

fn get_elevenlabs_client() -> &'static Client {
    static CLIENT: std::sync::OnceLock<Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        Client::builder()
            .pool_max_idle_per_host(2)
            .pool_idle_timeout(std::time::Duration::from_secs(90))
            .tcp_nodelay(true)
            .tcp_keepalive(std::time::Duration::from_secs(60))
            .build()
            .expect("Failed to create ElevenLabs HTTP client")
    })
}

pub struct ElevenLabsTtsBackend {
    config: ElevenLabsConfig,
    client: Client,
}

impl ElevenLabsTtsBackend {
    pub fn new(config: ElevenLabsConfig) -> Self {
        Self {
            config,
            client: get_elevenlabs_client().clone(),
        }
    }

    /// Execute an async block using the current Tokio runtime if available,
    /// or create a new one if called from outside a runtime context.
    fn block_on_async<F, T>(f: F) -> T
    where
        F: std::future::Future<Output = T>,
    {
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => handle.block_on(f),
            Err(_) => {
                let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
                rt.block_on(f)
            }
        }
    }

    /// Fetch available voices from ElevenLabs API
    pub fn list_voices(&self) -> Result<Vec<ElevenLabsVoice>, String> {
        log::debug!("ElevenLabs - fetching available voices");

        if self.config.api_key.trim().is_empty() {
            log::error!("ElevenLabs - API key is missing");
            return Err("ElevenLabs API key is missing".to_string());
        }

        match self.list_voices_internal() {
            Ok(voices) => Ok(voices),
            Err(e) => {
                log::warn!(
                    "ElevenLabs - failed to fetch voices from API, using defaults: {}",
                    e
                );
                Ok(Self::default_voices())
            }
        }
    }

    fn list_voices_internal(&self) -> Result<Vec<ElevenLabsVoice>, String> {
        let url = "https://api.elevenlabs.io/v1/voices";
        let api_key = self.config.api_key.clone();

        let start_time = std::time::Instant::now();

        let fetch_result: Result<(reqwest::StatusCode, Vec<u8>), String> = {
            let client = self.client.clone();
            Self::block_on_async(async move {
                let response = client
                    .get(url)
                    .header("xi-api-key", api_key)
                    .header("Accept", "application/json")
                    .send()
                    .await
                    .map_err(|e| format!("Failed to fetch voices: {}", e))?;

                let status = response.status();
                let body = response
                    .bytes()
                    .await
                    .map_err(|e| format!("Failed to read voices response: {}", e))?;

                Ok((status, body.to_vec()))
            })
        };

        let (status, response_bytes) = fetch_result.map_err(|e| {
            let elapsed = start_time.elapsed();
            log::error!(
                "ElevenLabs - failed to fetch voices after {:?}: {}",
                elapsed,
                e
            );
            e
        })?;

        let elapsed = start_time.elapsed();
        log::info!(
            "ElevenLabs - voices list response: {} {} (took {:?})",
            status.as_u16(),
            status.canonical_reason().unwrap_or("Unknown"),
            elapsed
        );

        let response_text = String::from_utf8_lossy(&response_bytes).to_string();

        if !status.is_success() {
            log::error!("ElevenLabs API error {}: {}", status, response_text);
            return Err(format!(
                "ElevenLabs API error {}: {}",
                status, response_text
            ));
        }

        #[derive(Deserialize)]
        struct VoicesResponse {
            voices: Vec<ElevenLabsVoice>,
        }

        let voices_response: VoicesResponse =
            serde_json::from_str(&response_text).map_err(|e| {
                log::error!("ElevenLabs - failed to parse voices response: {}", e);
                format!("Failed to parse voices response: {}", e)
            })?;

        Ok(voices_response.voices)
    }

    /// Default ElevenLabs voices (fallback when API fails)
    fn default_voices() -> Vec<ElevenLabsVoice> {
        vec![
            ElevenLabsVoice {
                voice_id: "21m00Tcm4TlvDq8ikWAM".to_string(),
                name: Some("Rachel".to_string()),
                category: Some("premade".to_string()),
                labels: Some(serde_json::json!({"language": "en", "gender": "female"})),
                description: Some("Young female voice, confident and clear".to_string()),
                preview_url: Some("https://storage.googleapis.com/eleven-public-prod/premade/voices/21m00Tcm4TlvDq8ikWAM".to_string()),
            },
            ElevenLabsVoice {
                voice_id: "AZnzlk1XvdvUeBnXmlld".to_string(),
                name: Some("Domi".to_string()),
                category: Some("premade".to_string()),
                labels: Some(serde_json::json!({"language": "en", "gender": "female"})),
                description: Some("Female voice, clear and professional".to_string()),
                preview_url: Some("https://storage.googleapis.com/eleven-public-prod/premade/voices/AZnzlk1XvdvUeBnXmlld".to_string()),
            },
            ElevenLabsVoice {
                voice_id: "EXAVITQu4vr4xnSDxMaL".to_string(),
                name: Some("Sarah".to_string()),
                category: Some("premade".to_string()),
                labels: Some(serde_json::json!({"language": "en", "gender": "female"})),
                description: Some("Female voice, energetic and friendly".to_string()),
                preview_url: Some("https://storage.googleapis.com/eleven-public-prod/premade/voices/EXAVITQu4vr4xnSDxMaL".to_string()),
            },
            ElevenLabsVoice {
                voice_id: "JBFqnCBsd6RMkjVDRZzb".to_string(),
                name: Some("George".to_string()),
                category: Some("premade".to_string()),
                labels: Some(serde_json::json!({"language": "en", "gender": "male"})),
                description: Some("Male voice, deep and professional".to_string()),
                preview_url: Some("https://storage.googleapis.com/eleven-public-prod/premade/voices/JBFqnCBsd6RMkjVDRZzb".to_string()),
            },
            ElevenLabsVoice {
                voice_id: "N2lVS1w4xneBdscFXVwa".to_string(),
                name: Some("Arnold".to_string()),
                category: Some("premade".to_string()),
                labels: Some(serde_json::json!({"language": "en", "gender": "male"})),
                description: Some("Male voice, strong and confident".to_string()),
                preview_url: Some("https://storage.googleapis.com/eleven-public-prod/premade/voices/N2lVS1w4xneBdscFXVwa".to_string()),
            },
            ElevenLabsVoice {
                voice_id: "pNInz6obpgDQGcFmaJgB".to_string(),
                name: Some("Adam".to_string()),
                category: Some("premade".to_string()),
                labels: Some(serde_json::json!({"language": "en", "gender": "male"})),
                description: Some("Male voice, expressive and natural".to_string()),
                preview_url: Some("https://storage.googleapis.com/eleven-public-prod/premade/voices/pNInz6obpgDQGcFmaJgB".to_string()),
            },
            ElevenLabsVoice {
                voice_id: "ThT5KcBeYPX3keUQqHPh".to_string(),
                name: Some("Dorothy".to_string()),
                category: Some("premade".to_string()),
                labels: Some(serde_json::json!({"language": "en", "gender": "female"})),
                description: Some("Female voice, soft and calming".to_string()),
                preview_url: Some("https://storage.googleapis.com/eleven-public-prod/premade/voices/ThT5KcBeYPX3keUQqHPh".to_string()),
            },
            ElevenLabsVoice {
                voice_id: "CwhRBWXzGAHq8TQ4Fs17".to_string(),
                name: Some("Roger".to_string()),
                category: Some("premade".to_string()),
                labels: Some(serde_json::json!({"language": "en", "gender": "male"})),
                description: Some("Male voice, casual and laid-back".to_string()),
                preview_url: Some("https://storage.googleapis.com/eleven-public-prod/premade/voices/CwhRBWXzGAHq8TQ4Fs17".to_string()),
            },
        ]
    }
}

impl TtsBackend for ElevenLabsTtsBackend {
    fn name(&self) -> &str {
        "ElevenLabs"
    }

    fn file_extension(&self) -> &str {
        match self.config.output_format {
            ElevenLabsOutputFormat::Mp344100_128
            | ElevenLabsOutputFormat::Mp344100_192
            | ElevenLabsOutputFormat::Mp344100_32
            | ElevenLabsOutputFormat::Mp322050_32 => "mp3",
            ElevenLabsOutputFormat::OggVorbis44100 | ElevenLabsOutputFormat::OggVorbis22050 => {
                "ogg"
            }
            ElevenLabsOutputFormat::Flac44100 => "flac",
            _ => "wav",
        }
    }

    fn synthesize(&self, text: &str, voice: &str, _speed: f32) -> Result<Vec<u8>, String> {
        let voice_id = if voice.trim().is_empty() {
            &self.config.voice_id
        } else {
            voice
        };

        log::info!(
            "ElevenLabs TTS request - voice: {}, model: {}, format: {}, text length: {} chars",
            voice_id,
            self.config.model_id,
            self.config.output_format.as_str(),
            text.len()
        );

        let url = format!(
            "https://api.elevenlabs.io/v1/text-to-speech/{}?output_format={}",
            voice_id,
            self.config.output_format.as_str()
        );

        let body = json!({
            "text": text,
            "model_id": self.config.model_id,
            "voice_settings": VoiceSettings {
                stability: self.config.voice_stability,
                similarity_boost: self.config.voice_similarity_boost,
                style: self.config.voice_style,
                use_speaker_boost: self.config.use_speaker_boost,
            },
        });

        let api_key = self.config.api_key.clone();
        let mime_type = self.config.output_format.mime_type();

        let start_time = std::time::Instant::now();

        let response = {
            let client = self.client.clone();
            Self::block_on_async(async move {
                client
                    .post(&url)
                    .header("xi-api-key", api_key)
                    .header("Content-Type", "application/json")
                    .header("Accept", mime_type)
                    .json(&body)
                    .send()
                    .await
            })
        }
        .map_err(|e| {
            let elapsed = start_time.elapsed();
            log::error!("ElevenLabs TTS request failed after {:?}: {}", elapsed, e);
            format!("Request failed: {}", e)
        })?;

        let elapsed = start_time.elapsed();
        let status = response.status();

        log::info!(
            "ElevenLabs TTS response: {} {} (took {:?})",
            status.as_u16(),
            status.canonical_reason().unwrap_or("Unknown"),
            elapsed
        );

        if !response.status().is_success() {
            let error_text =
                Self::block_on_async(async { response.text().await.unwrap_or_default() });
            log::error!("ElevenLabs API error {}: {}", status, error_text);
            if status == reqwest::StatusCode::PAYMENT_REQUIRED
                || error_text.contains("paid_plan_required")
                || error_text.contains("payment_required")
            {
                return Err("This voice is from the ElevenLabs voice library and requires a paid plan. \
                     Please use one of your own cloned voices, or upgrade your ElevenLabs subscription."
                    .to_string());
            }
            return Err(format!("ElevenLabs API error {}: {}", status, error_text));
        }

        let bytes = Self::block_on_async(async { response.bytes().await }).map_err(|e| {
            log::error!("Failed to read response bytes: {}", e);
            format!("Failed to read bytes: {}", e)
        })?;

        log::info!(
            "ElevenLabs TTS synthesis complete: received {} bytes",
            bytes.len()
        );

        Ok(bytes.to_vec())
    }

    fn health_check(&self) -> Result<(), String> {
        log::debug!("ElevenLabs TTS health check - validating API key");

        if self.config.api_key.trim().is_empty() {
            log::error!("ElevenLabs TTS health check failed - API key is missing");
            return Err("ElevenLabs API key is missing".to_string());
        }

        log::debug!("ElevenLabs TTS health check passed");
        Ok(())
    }
}
