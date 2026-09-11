//! Tauri commands for the in-app llama.cpp server and its releases.

use std::sync::Arc;

use tauri::{AppHandle, Manager};

use crate::llama_releases::{self, InstalledLlamaServer, LlamaRelease};
use crate::llama_server::{
    self, GgufFile, LlamaDetectedInstall, LlamaServerManager, LlamaServerStateEvent,
};
use crate::settings::{LlamaSettings, get_settings, write_settings};

fn manager(app: &AppHandle) -> Arc<LlamaServerManager> {
    app.state::<Arc<LlamaServerManager>>().inner().clone()
}

#[tauri::command]
#[specta::specta]
pub fn get_llama_server_state(app: AppHandle) -> LlamaServerStateEvent {
    manager(&app).snapshot()
}

#[tauri::command]
#[specta::specta]
pub fn get_llama_server_logs(app: AppHandle) -> Vec<String> {
    manager(&app).logs()
}

#[tauri::command]
#[specta::specta]
pub async fn start_llama_server(app: AppHandle) -> Result<(), String> {
    let m = manager(&app);
    tauri::async_runtime::spawn_blocking(move || m.start())
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
#[specta::specta]
pub async fn stop_llama_server(app: AppHandle) -> Result<(), String> {
    let m = manager(&app);
    tauri::async_runtime::spawn_blocking(move || m.stop())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn restart_llama_server(app: AppHandle) -> Result<(), String> {
    let m = manager(&app);
    tauri::async_runtime::spawn_blocking(move || m.restart())
        .await
        .map_err(|e| e.to_string())?
}

/// Persist the llama settings. A running server keeps its current command
/// line until it is restarted; the UI offers that explicitly.
#[tauri::command]
#[specta::specta]
pub fn change_llama_settings(app: AppHandle, settings: LlamaSettings) -> Result<(), String> {
    let mut current = get_settings(&app);
    current.llama = settings.normalized();
    write_settings(&app, current);
    Ok(())
}

/// The command line the current settings would launch, for the page preview.
#[tauri::command]
#[specta::specta]
pub fn get_llama_command_preview(app: AppHandle) -> Result<String, String> {
    let settings = get_settings(&app).llama.normalized();
    let exe = llama_server::resolve_server_exe(&settings)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "llama-server".into());
    let args = llama_server::build_args(&settings)?;
    Ok(format!(
        "{exe} {}",
        args.iter()
            .map(|a| if a.contains(' ') {
                format!("\"{a}\"")
            } else {
                a.clone()
            })
            .collect::<Vec<_>>()
            .join(" ")
    ))
}

#[tauri::command]
#[specta::specta]
pub fn list_gguf_files(dir: String) -> Vec<GgufFile> {
    llama_server::list_gguf_files(&dir)
}

#[tauri::command]
#[specta::specta]
pub fn detect_llama_install() -> Option<LlamaDetectedInstall> {
    llama_server::detect_existing_install()
}

#[tauri::command]
#[specta::specta]
pub fn apply_llama_to_post_processing(app: AppHandle) -> Result<(), String> {
    manager(&app).apply_to_post_processing()
}

#[tauri::command]
#[specta::specta]
pub async fn fetch_llama_releases(
    channel: String,
    force: bool,
) -> Result<Vec<LlamaRelease>, String> {
    llama_releases::fetch_releases(&channel, force).await
}

#[tauri::command]
#[specta::specta]
pub fn detect_llama_backend() -> String {
    llama_releases::detect_backend()
}

#[tauri::command]
#[specta::specta]
pub fn list_installed_llama_servers(app: AppHandle) -> Vec<InstalledLlamaServer> {
    llama_releases::list_installed(&app)
}

/// Download + unpack a release; progress arrives as `LlamaDownloadEvent`.
/// Runs to completion in the background so the page can navigate away.
#[tauri::command]
#[specta::specta]
pub async fn install_llama_release(
    app: AppHandle,
    tag: String,
    backend: String,
    include_cudart: bool,
) -> Result<(), String> {
    tauri::async_runtime::spawn(async move {
        if let Err(e) = llama_releases::install(&app, &tag, &backend, include_cudart).await {
            log::error!("llama.cpp install failed: {e}");
        }
    });
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn remove_installed_llama_server(app: AppHandle, dir: String) -> Result<(), String> {
    llama_releases::remove_installed(&app, &dir)
}

/// The installed CUDA toolkit whose runtime DLLs a CUDA build can use.
#[tauri::command]
#[specta::specta]
pub fn detect_cuda_toolkit() -> Option<llama_releases::CudaToolkitInfo> {
    llama_releases::detect_cuda_toolkit()
}

/// Delete the bundled cudart/cuBLAS DLLs from an install; returns MB freed.
#[tauri::command]
#[specta::specta]
pub fn remove_bundled_cuda_runtime(app: AppHandle, dir: String) -> Result<u32, String> {
    llama_releases::remove_bundled_cuda_runtime(&app, &dir)
}
