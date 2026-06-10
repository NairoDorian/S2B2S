use super::super::TtsBackend;
use crate::settings::CartesiaConfig;
use reqwest::Client;
use serde_json::json;

const CARTESIA_TTS_URL: &str = "https://api.cartesia.ai/tts/bytes";
const CARTESIA_VERSION: &str = "2024-06-10";

fn get_cartesia_client() -> &'static Client {
    static CLIENT: std::sync::OnceLock<Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        Client::builder()
            .pool_max_idle_per_host(2)
            .pool_idle_timeout(std::time::Duration::from_secs(90))
            .tcp_nodelay(true)
            .tcp_keepalive(std::time::Duration::from_secs(60))
            .build()
            .expect("Failed to create Cartesia HTTP client")
    })
}

pub struct CartesiaTtsBackend {
    config: CartesiaConfig,
    client: Client,
}

impl CartesiaTtsBackend {
    pub fn new(config: CartesiaConfig) -> Self {
        Self {
            config,
            client: get_cartesia_client().clone(),
        }
    }

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

impl TtsBackend for CartesiaTtsBackend {
    fn name(&self) -> &str {
        "Cartesia"
    }

    fn synthesize(&self, text: &str, voice: &str, _speed: f32) -> Result<Vec<u8>, String> {
        let voice_id = if voice.trim().is_empty() {
            &self.config.voice_id
        } else {
            voice
        };

        let body = json!({
            "model_id": self.config.model_id,
            "transcript": text,
            "voice": {
                "mode": "id",
                "id": voice_id,
            },
            "output_format": {
                "container": self.config.output_format,
                "encoding": "pcm_f32le",
                "sample_rate": 44100,
            },
        });

        log::info!(
            "Cartesia TTS request - model: {}, voice: {}, text length: {} chars",
            self.config.model_id,
            voice_id,
            text.len()
        );

        let start_time = std::time::Instant::now();
        let api_key = self.config.api_key.clone();

        let response = {
            let client = self.client.clone();
            Self::block_on_async(async move {
                client
                    .post(CARTESIA_TTS_URL)
                    .header("X-API-Key", api_key)
                    .header("Cartesia-Version", CARTESIA_VERSION)
                    .header("Content-Type", "application/json")
                    .json(&body)
                    .send()
                    .await
            })
        }
        .map_err(|e| {
            log::error!(
                "Cartesia TTS request failed after {:?}: {}",
                start_time.elapsed(),
                e
            );
            format!("Request failed: {}", e)
        })?;

        let status = response.status();
        log::info!(
            "Cartesia TTS response: {} {} (took {:?})",
            status.as_u16(),
            status.canonical_reason().unwrap_or("Unknown"),
            start_time.elapsed()
        );

        if !status.is_success() {
            let error_text =
                Self::block_on_async(async { response.text().await.unwrap_or_default() });
            log::error!("Cartesia API error {}: {}", status, error_text);
            return Err(format!("Cartesia API error {}: {}", status, error_text));
        }

        let bytes = Self::block_on_async(async { response.bytes().await }).map_err(|e| {
            log::error!("Failed to read Cartesia response bytes: {}", e);
            format!("Failed to read bytes: {}", e)
        })?;

        log::info!(
            "Cartesia TTS synthesis complete: received {} bytes",
            bytes.len()
        );
        Ok(bytes.to_vec())
    }

    fn health_check(&self) -> Result<(), String> {
        if self.config.api_key.trim().is_empty() {
            return Err("Cartesia API key is missing".to_string());
        }
        Ok(())
    }

    fn file_extension(&self) -> &str {
        &self.config.output_format
    }
}
