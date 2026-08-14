use crate::managers::transcription::TranscriptionManager;
use crate::settings::{get_settings, write_settings, ModelUnloadTimeout};
use serde::Serialize;
use specta::Type;
use std::sync::Arc;
use tauri::{AppHandle, Manager, State};

#[derive(Serialize, Type)]
pub struct ModelLoadStatus {
    is_loaded: bool,
    current_model: Option<String>,
}

#[tauri::command]
#[specta::specta]
pub fn set_model_unload_timeout(app: AppHandle, timeout: ModelUnloadTimeout) {
    let mut settings = get_settings(&app);
    settings.model_unload_timeout = timeout;
    write_settings(&app, settings);
}

#[tauri::command]
#[specta::specta]
pub fn get_model_load_status(
    transcription_manager: State<TranscriptionManager>,
) -> Result<ModelLoadStatus, String> {
    Ok(ModelLoadStatus {
        is_loaded: transcription_manager.is_model_loaded(),
        current_model: transcription_manager.get_current_model(),
    })
}

#[tauri::command]
#[specta::specta]
pub fn unload_model_manually(
    transcription_manager: State<TranscriptionManager>,
) -> Result<(), String> {
    transcription_manager
        .unload_model()
        .map_err(|e| format!("Failed to unload model: {}", e))
}

#[tauri::command]
#[specta::specta]
pub fn set_long_audio_model(app: AppHandle, model: Option<String>) {
    let mut settings = get_settings(&app);
    settings.long_audio_model = model;
    write_settings(&app, settings);
}

#[tauri::command]
#[specta::specta]
pub fn set_long_audio_threshold(app: AppHandle, threshold: f64) {
    let mut settings = get_settings(&app);
    settings.long_audio_threshold_seconds = threshold;
    write_settings(&app, settings);
}

#[tauri::command]
#[specta::specta]
pub fn unload_extra_model(
    transcription_manager: State<TranscriptionManager>,
    model_id: String,
) -> Result<(), String> {
    transcription_manager
        .unload_extra_model(&model_id)
        .map_err(|e| format!("Failed to unload extra model: {}", e))
}

/// Load the configured multi-STT extra models (slots 2/3) into RAM/VRAM so the
/// first multi-STT turn doesn't pay the model-load cost. Returns the display
/// names of the models that were (or already are) loaded. Missing/un-downloaded
/// models are skipped with a warning.
#[tauri::command]
#[specta::specta]
pub async fn preload_multi_stt_models(app: AppHandle) -> Result<Vec<String>, String> {
    let settings = get_settings(&app);
    let ids: Vec<String> = [
        settings.multi_stt_model_2.clone(),
        settings.multi_stt_model_3.clone(),
    ]
    .into_iter()
    .flatten()
    .collect();

    let tm = app.state::<Arc<TranscriptionManager>>().inner().clone();

    tauri::async_runtime::spawn_blocking(move || {
        let mut loaded = Vec::new();
        for id in ids {
            if tm.is_extra_model_loaded(&id) {
                continue;
            }
            match tm.load_extra_model(&id) {
                Ok(name) => {
                    log::info!("[Multi-STT] Preloaded extra model '{}' into RAM/VRAM", id);
                    loaded.push(name);
                }
                Err(e) => {
                    log::warn!("[Multi-STT] Preload failed for '{}': {}", id, e);
                }
            }
        }
        loaded
    })
    .await
    .map_err(|e| format!("Multi-STT preload task panicked: {e}"))
}

/// Unload every extra (multi-STT) engine to free RAM/VRAM.
#[tauri::command]
#[specta::specta]
pub fn unload_all_extra_models(
    transcription_manager: State<TranscriptionManager>,
) -> Result<(), String> {
    transcription_manager.unload_all_extra_models();
    Ok(())
}
