//! Tauri commands of the "Live Mode" page.

use std::sync::Arc;

use tauri::{AppHandle, State};

use crate::live_mode::{LiveModeManager, LiveModeStatus, LiveSessionInfo};
use crate::settings::{LiveModeSettings, get_settings, write_settings};

#[tauri::command]
#[specta::specta]
pub fn change_live_mode_settings(app: AppHandle, settings: LiveModeSettings) -> Result<(), String> {
    let mut current = get_settings(&app);
    current.live_mode = settings.normalized();
    write_settings(&app, current);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn live_mode_start(manager: State<'_, Arc<LiveModeManager>>) -> Result<(), String> {
    manager.inner().start()
}

#[tauri::command]
#[specta::specta]
pub fn live_mode_stop(manager: State<'_, Arc<LiveModeManager>>) -> Result<(), String> {
    manager.stop()
}

#[tauri::command]
#[specta::specta]
pub fn live_mode_status(manager: State<'_, Arc<LiveModeManager>>) -> LiveModeStatus {
    manager.status()
}

/// Past sessions under the configured output folder, newest first.
#[tauri::command]
#[specta::specta]
pub fn live_mode_list_sessions(app: AppHandle) -> Result<Vec<LiveSessionInfo>, String> {
    let settings = get_settings(&app).live_mode.normalized();
    let root = LiveModeManager::output_root(&app, &settings).map_err(|e| e.to_string())?;
    Ok(crate::live_mode::list_sessions(&root))
}

/// The folder used when no output folder is configured, for display.
#[tauri::command]
#[specta::specta]
pub fn live_mode_default_output_dir(app: AppHandle) -> Result<String, String> {
    LiveModeManager::output_root(&app, &LiveModeSettings::default())
        .map(|p| p.to_string_lossy().to_string())
        .map_err(|e| e.to_string())
}
