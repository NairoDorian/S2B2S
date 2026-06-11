use crate::settings::{get_settings, write_settings};
use crate::settings::WakeWordConfig;
use crate::wake_word::{start_wake_word_detection, stop_wake_word_detection};
use std::sync::Arc;
use tauri::{AppHandle, Manager};

#[tauri::command]
#[specta::specta]
pub fn wake_word_start(app: AppHandle) -> Result<(), String> {
    let settings = get_settings(&app);
    if !settings.tts.wake_word.enabled {
        return Err("Wake word is disabled in settings".to_string());
    }
    start_wake_word_detection(app);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn wake_word_stop(app: AppHandle) -> Result<(), String> {
    stop_wake_word_detection(app);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn wake_word_set_config(app: AppHandle, config: WakeWordConfig) -> Result<(), String> {
    // If currently running, stop first
    if let Some(detector) = app.try_state::<Arc<crate::wake_word::WakeWordDetector>>() {
        if detector.active.load(std::sync::atomic::Ordering::SeqCst) {
            stop_wake_word_detection(app.clone());
        }
    }
    let mut settings = get_settings(&app);
    settings.tts.wake_word = config;
    write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn wake_word_status(app: AppHandle) -> Result<bool, String> {
    let running = app.try_state::<Arc<crate::wake_word::WakeWordDetector>>()
        .map(|d| d.active.load(std::sync::atomic::Ordering::SeqCst))
        .unwrap_or(false);
    Ok(running)
}
