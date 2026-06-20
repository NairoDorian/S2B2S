use crate::llama_server::manager::{LlamaServerConfig, LlamaServerManager};
use std::sync::Arc;
use tauri::{AppHandle, State};

#[tauri::command]
#[specta::specta]
pub async fn fetch_llama_releases(
    manager: State<'_, Arc<LlamaServerManager>>,
) -> Result<Vec<crate::llama_server::manager::LlamaRelease>, String> {
    manager.fetch_releases().await
}

#[tauri::command]
#[specta::specta]
pub async fn download_llama_server(
    manager: State<'_, Arc<LlamaServerManager>>,
    backend: String,
    release_tag: String,
    download_url: String,
) -> Result<(), String> {
    manager
        .download_server(&backend, &release_tag, &download_url)
        .await
}

#[tauri::command]
#[specta::specta]
pub fn get_downloaded_llama_servers(
    manager: State<'_, Arc<LlamaServerManager>>,
) -> Result<Vec<crate::llama_server::manager::DownloadedServer>, String> {
    manager.list_downloaded_servers()
}

#[tauri::command]
#[specta::specta]
pub fn remove_llama_server(
    manager: State<'_, Arc<LlamaServerManager>>,
    backend: String,
    release_tag: String,
) -> Result<(), String> {
    manager.remove_server(&backend, &release_tag)
}

#[tauri::command]
#[specta::specta]
pub fn set_llama_server_active(
    app: AppHandle,
    backend: String,
    release_tag: String,
) -> Result<(), String> {
    let mut settings = crate::settings::get_settings(&app);
    settings.llama_server.backend = backend;
    settings.llama_server.release_tag = release_tag;
    crate::settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn get_llama_server_config(app: AppHandle) -> Result<LlamaServerConfig, String> {
    let settings = crate::settings::get_settings(&app);
    Ok(settings.llama_server.clone())
}

#[tauri::command]
#[specta::specta]
pub fn detect_gpu_type(app: AppHandle) -> Result<String, String> {
    Ok(crate::llama_server::manager::LlamaServerManager::new(app).detect_gpu())
}
