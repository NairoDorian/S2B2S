//! Tauri commands for the in-app llama.cpp server and its releases.

use std::sync::Arc;

use tauri::{AppHandle, Manager};

use crate::llama_releases::{self, InstalledLlamaServer, LlamaRelease};
use crate::llama_server::{
    self, GgufFile, LlamaDetectedInstall, LlamaServerManager, LlamaServerStateEvent, LlamaStatus,
};
use crate::settings::{LlamaSettings, get_settings, write_settings};

fn manager(app: &AppHandle) -> Arc<LlamaServerManager> {
    app.state::<Arc<LlamaServerManager>>().inner().clone()
}

/// Run a filesystem- or process-bound command body on the blocking pool, so
/// it never runs on the main thread (docs/PERFORMANCE.md rule 3). For the
/// commands whose return type is not a `Result` (the generated bindings must
/// keep their shape): a panic in `f` is logged and yields `fallback`.
async fn off_main<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static, fallback: T) -> T {
    match tauri::async_runtime::spawn_blocking(f).await {
        Ok(value) => value,
        Err(e) => {
            log::error!("llama command task failed: {e}");
            fallback
        }
    }
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
///
/// Deliberately synchronous: the server fields save on every keystroke, and
/// running on the main thread keeps those writes in the order they were typed.
/// Only the provider relink, which can probe a socket for ~1 s, goes to the
/// blocking pool; it re-reads the settings it writes (see
/// `LlamaServerManager::relink_custom_provider`), so it cannot undo an edit.
#[tauri::command]
#[specta::specta]
pub fn change_llama_settings(app: AppHandle, settings: LlamaSettings) -> Result<(), String> {
    let mut current = get_settings(&app);
    let previous = current.llama.clone();
    current.llama = settings.normalized();
    // The `custom` provider's URL embeds the port and its model the alias, so
    // those two moving means the link needs redoing. Guarded rather than
    // unconditional because this command runs on every keystroke of the
    // server fields, and the sync probes a socket when the URL disagrees.
    let relink = current.llama.port != previous.port || current.llama.alias != previous.alias;
    write_settings(&app, current);
    if relink {
        tauri::async_runtime::spawn_blocking(move || {
            let m = manager(&app);
            let running = m.snapshot().status == LlamaStatus::Ready;
            m.relink_custom_provider(running);
        });
    }
    Ok(())
}

/// The command line the current settings would launch, for the page preview.
/// Asked for on every keystroke of the server fields, and resolving it stats
/// the model files and lists the server folder, so it runs on the blocking
/// pool rather than the main thread.
#[tauri::command]
#[specta::specta]
pub async fn get_llama_command_preview(app: AppHandle) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
        let settings = get_settings(&app).llama.normalized();
        let exe = llama_server::resolve_server_exe(&settings)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "llama-server".into());
        let args = llama_server::build_args(&settings)?;
        Ok(llama_server::format_command_line(&exe, &args))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
#[specta::specta]
pub async fn list_gguf_files(dir: String) -> Vec<GgufFile> {
    off_main(move || llama_server::list_gguf_files(&dir), Vec::new()).await
}

#[tauri::command]
#[specta::specta]
pub async fn detect_llama_install() -> Option<LlamaDetectedInstall> {
    off_main(llama_server::detect_existing_install, None).await
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
pub async fn detect_llama_backend() -> String {
    // The first call runs `nvidia-smi` and waits for it; "cpu" is also what a
    // failed detection reports.
    off_main(llama_releases::detect_backend, "cpu".to_string()).await
}

#[tauri::command]
#[specta::specta]
pub async fn list_installed_llama_servers(app: AppHandle) -> Vec<InstalledLlamaServer> {
    off_main(move || llama_releases::list_installed(&app), Vec::new()).await
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
pub async fn remove_installed_llama_server(app: AppHandle, dir: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || llama_releases::remove_installed(&app, &dir))
        .await
        .map_err(|e| e.to_string())?
}

/// The installed CUDA toolkit whose runtime DLLs a CUDA build can use.
#[tauri::command]
#[specta::specta]
pub async fn detect_cuda_toolkit() -> Option<llama_releases::CudaToolkitInfo> {
    off_main(llama_releases::detect_cuda_toolkit, None).await
}

/// Delete the bundled cudart/cuBLAS DLLs from an install; returns MB freed.
#[tauri::command]
#[specta::specta]
pub async fn remove_bundled_cuda_runtime(app: AppHandle, dir: String) -> Result<u32, String> {
    tauri::async_runtime::spawn_blocking(move || {
        llama_releases::remove_bundled_cuda_runtime(&app, &dir)
    })
    .await
    .map_err(|e| e.to_string())?
}
