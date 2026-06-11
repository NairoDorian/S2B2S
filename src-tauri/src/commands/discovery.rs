use crate::settings::get_settings;
use serde::Serialize;
use specta::Type;
use std::sync::OnceLock;
use tauri::AppHandle;

#[derive(Debug, Clone, Serialize, Type)]
pub struct DiscoveredServer {
    pub name: String,
    pub base_url: String,
    pub models: Vec<String>,
    pub provider_id: String,
}

static DISCOVERED_SERVERS: OnceLock<std::sync::Mutex<Vec<DiscoveredServer>>> = OnceLock::new();

fn get_cache() -> &'static std::sync::Mutex<Vec<DiscoveredServer>> {
    DISCOVERED_SERVERS.get_or_init(|| std::sync::Mutex::new(Vec::new()))
}

#[tauri::command]
#[specta::specta]
pub async fn discover_local_brains(app: AppHandle) -> Result<Vec<DiscoveredServer>, String> {
    let settings = get_settings(&app);
    let mut servers = Vec::new();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))?;

    let probes = vec![
        ("Ollama", "http://127.0.0.1:11434", "custom", "/api/tags"),
        ("LM Studio", "http://127.0.0.1:1234", "custom", "/v1/models"),
        ("llama.cpp", "http://127.0.0.1:8080", "custom", "/v1/models"),
    ];

    for (name, base_url, provider_id, models_path) in &probes {
        let full_url = format!("{}{}", base_url, models_path);
        match client.get(&full_url).send().await {
            Ok(resp) if resp.status().is_success() => {
                let models = if *models_path == "/api/tags" {
                    parse_ollama_models(resp.json().await.unwrap_or_default()).await
                } else {
                    parse_openai_models(resp.json().await.unwrap_or_default()).await
                };
                if !models.is_empty() {
                    servers.push(DiscoveredServer {
                        name: name.to_string(),
                        base_url: format!("{}/v1", base_url),
                        models,
                        provider_id: provider_id.to_string(),
                    });
                }
            }
            _ => continue,
        }
    }

    if let Ok(cache) = get_cache().lock() {
        log::info!("[Discovery] Found {} local Brain server(s)", servers.len());
    }
    Ok(servers)
}

async fn parse_ollama_models(value: serde_json::Value) -> Vec<String> {
    value
        .get("models")
        .and_then(|m| m.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m.get("name").and_then(|n| n.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

async fn parse_openai_models(value: serde_json::Value) -> Vec<String> {
    value
        .get("data")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m.get("id").and_then(|n| n.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

#[tauri::command]
#[specta::specta]
pub fn is_ollama_running() -> bool {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .ok();
    let running = client
        .and_then(|c| c.get("http://127.0.0.1:11434/api/tags").send().ok())
        .map(|r| r.status().is_success())
        .unwrap_or(false);
    if running {
        log::info!("[Discovery] Ollama detected at localhost:11434");
    }
    running
}
