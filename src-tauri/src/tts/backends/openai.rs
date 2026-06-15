use super::super::TtsBackend;
use crate::settings::OpenAIConfig;
use reqwest::Client;
use serde_json::json;

fn get_openai_client() -> &'static Client {
    static CLIENT: std::sync::OnceLock<Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        Client::builder()
            .pool_max_idle_per_host(2)
            .pool_idle_timeout(std::time::Duration::from_secs(90))
            .tcp_nodelay(true)
            .tcp_keepalive(std::time::Duration::from_secs(60))
            // Bounded deadlines — a hung request must not hold the synthesis
            // worker (and the per-engine lock) forever.
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .expect("Failed to create OpenAI HTTP client")
    })
}

pub struct OpenAiTtsBackend {
    config: OpenAIConfig,
    client: Client,
    auth_header: String,
}

impl OpenAiTtsBackend {
    pub fn new(config: OpenAIConfig) -> Self {
        let auth_header = format!("Bearer {}", config.api_key);
        Self {
            config,
            client: get_openai_client().clone(),
            auth_header,
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
}

impl TtsBackend for OpenAiTtsBackend {
    fn name(&self) -> &str {
        "OpenAI"
    }

    fn synthesize(&self, text: &str, voice: &str, speed: f32) -> Result<Vec<u8>, String> {
        let url = "https://api.openai.com/v1/audio/speech";
        let resolved_voice = if voice.trim().is_empty() {
            &self.config.voice
        } else {
            voice
        };

        let body = json!({
            "model": self.config.model,
            "input": text,
            "voice": resolved_voice,
            "speed": speed,
            "response_format": "wav",
        });

        let auth_header = self.auth_header.clone();

        log::info!(
            "OpenAI TTS request - model: {}, voice: {}, speed: {}, text length: {} chars",
            self.config.model,
            resolved_voice,
            speed,
            text.len()
        );

        let start_time = std::time::Instant::now();

        let response = {
            let client = self.client.clone();
            Self::block_on_async(async move {
                client
                    .post(url)
                    .header("Authorization", auth_header)
                    .header("Content-Type", "application/json")
                    .json(&body)
                    .send()
                    .await
            })
        }
        .map_err(|e| {
            let elapsed = start_time.elapsed();
            log::error!("OpenAI TTS request failed after {:?}: {}", elapsed, e);
            format!("Request failed: {}", e)
        })?;

        let elapsed = start_time.elapsed();
        let status = response.status();

        log::info!(
            "OpenAI TTS response: {} {} (took {:?})",
            status.as_u16(),
            status.canonical_reason().unwrap_or("Unknown"),
            elapsed
        );

        if !response.status().is_success() {
            let error_text =
                Self::block_on_async(async { response.text().await.unwrap_or_default() });
            log::error!("OpenAI API error {}: {}", status, error_text);
            return Err(format!("OpenAI API error {}: {}", status, error_text));
        }

        let bytes = Self::block_on_async(async { response.bytes().await }).map_err(|e| {
            log::error!("Failed to read response bytes: {}", e);
            format!("Failed to read bytes: {}", e)
        })?;

        log::info!(
            "OpenAI TTS synthesis complete: received {} bytes",
            bytes.len()
        );

        Ok(bytes.to_vec())
    }

    fn health_check(&self) -> Result<(), String> {
        log::debug!("OpenAI TTS health check - validating API key");

        if self.config.api_key.trim().is_empty() {
            log::error!("OpenAI TTS health check failed - API key is missing");
            return Err("OpenAI API key is missing".to_string());
        }

        log::debug!("OpenAI TTS health check passed");
        Ok(())
    }
}
