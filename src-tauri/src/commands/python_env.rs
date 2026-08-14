//! Tauri commands for the Python Environment Manager.
//!
//! These commands let the frontend:
//!   - Query the current env status  (uv version, python version, per-backend health)
//!   - Install / detect uv
//!   - Create or recreate the shared venv
//!   - Install individual or all backends (with CPU / GPU switch)
//!   - Open the venv folder in Explorer / Finder / Files

use crate::python_env;
use crate::python_env::{BackendStatus, PythonEnvStatus};
use tauri::{AppHandle, Emitter};
use tauri_plugin_opener::OpenerExt;

// ---------------------------------------------------------------------------
// Status query
// ---------------------------------------------------------------------------

/// Return a full snapshot of the Python environment (synchronous, quick).
#[tauri::command]
#[specta::specta]
pub fn get_python_env_status() -> PythonEnvStatus {
    python_env::get_env_status()
}

/// Check a single backend's install status.
#[tauri::command]
#[specta::specta]
pub fn check_backend_status(backend_id: String) -> BackendStatus {
    let installed = python_env::is_backend_installed(&backend_id);
    let all = python_env::get_env_status();
    all.backends
        .into_iter()
        .find(|b| b.id == backend_id)
        .unwrap_or(BackendStatus {
            id: backend_id.clone(),
            label: backend_id.clone(),
            installed,
            category: python_env::BackendCategory::Tts,
        })
}

// ---------------------------------------------------------------------------
// uv management
// ---------------------------------------------------------------------------

/// Install `uv` if not already present, streaming progress events.
/// Returns the uv version string on success.
#[tauri::command]
#[specta::specta]
pub async fn install_uv(app: AppHandle) -> Result<String, String> {
    if let Some(uv) = python_env::find_uv() {
        let ver = python_env::uv_version(&uv).unwrap_or_else(|| "unknown".to_string());
        python_env::emit_progress(&app, "uv", &format!("uv already installed: {ver}"), "info");
        return Ok(ver);
    }

    let _uv_path = python_env::install_uv(&app)?;
    let ver = python_env::find_uv()
        .and_then(|u| python_env::uv_version(&u))
        .unwrap_or_else(|| "installed".to_string());
    Ok(ver)
}

// ---------------------------------------------------------------------------
// Venv management
// ---------------------------------------------------------------------------

/// Create (or recreate) the shared venv using Python 3.12.
/// Streams installation progress via `python-env-progress` events.
#[tauri::command]
#[specta::specta]
pub async fn create_python_venv(app: AppHandle) -> Result<(), String> {
    let uv = python_env::find_uv()
        .ok_or_else(|| "uv is not installed. Install it first.".to_string())?;
    python_env::create_venv(&app, &uv)
}

// ---------------------------------------------------------------------------
// Backend installation
// ---------------------------------------------------------------------------

/// Install packages for a single backend.
/// `gpu` chooses CUDA 13 onnxruntime / CUDA torch vs CPU-only.
#[tauri::command]
#[specta::specta]
pub async fn setup_backend(app: AppHandle, backend_id: String, gpu: bool) -> Result<(), String> {
    let uv = python_env::find_uv()
        .ok_or_else(|| "uv is not installed. Install it first.".to_string())?;

    if !python_env::venv_dir().exists() {
        python_env::create_venv(&app, &uv)?;
    }

    python_env::install_backend(&app, &backend_id, &uv, gpu)?;

    // Emit updated status after install
    let _ = app.emit("python-env-status", python_env::get_env_status());

    Ok(())
}

/// Install all TTS + STT backends in one shot.
#[tauri::command]
#[specta::specta]
pub async fn setup_all_backends(app: AppHandle, gpu: bool) -> Result<(), String> {
    let uv = python_env::find_uv()
        .ok_or_else(|| "uv is not installed. Install it first.".to_string())?;

    if !python_env::venv_dir().exists() {
        python_env::create_venv(&app, &uv)?;
    }

    python_env::install_all_backends(&app, &uv, gpu)?;

    let _ = app.emit("python-env-status", python_env::get_env_status());

    Ok(())
}

/// Full GPU setup: create fresh venv + install everything with CUDA 13 support.
/// Equivalent to running `scripts/setup_venv_uv.ps1` manually.
#[tauri::command]
#[specta::specta]
pub async fn full_gpu_setup(app: AppHandle) -> Result<(), String> {
    let uv = match python_env::find_uv() {
        Some(u) => u,
        None => {
            python_env::install_uv(&app)?;
            python_env::find_uv().ok_or_else(|| "uv still not found after install".to_string())?
        }
    };

    python_env::create_venv(&app, &uv)?;
    python_env::install_all_backends(&app, &uv, true)?;

    let _ = app.emit("python-env-status", python_env::get_env_status());

    Ok(())
}

// ---------------------------------------------------------------------------
// Filesystem helpers
// ---------------------------------------------------------------------------

/// Open the venv directory in the system file manager.
#[tauri::command]
#[specta::specta]
pub fn open_venv_folder(app: AppHandle) -> Result<(), String> {
    let venv = python_env::venv_dir();
    let path = venv.to_string_lossy().to_string();
    app.opener()
        .open_path(path, None::<String>)
        .map_err(|e| format!("Failed to open venv folder: {e}"))
}
