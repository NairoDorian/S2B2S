use crate::managers::transcription::TranscriptionManager;
use crate::settings::{ModelUnloadTimeout, get_settings, write_settings};
use serde::Serialize;
use specta::Type;
use std::sync::Arc;
use tauri::{AppHandle, State};

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
    transcription_manager: State<'_, Arc<TranscriptionManager>>,
) -> Result<ModelLoadStatus, String> {
    Ok(ModelLoadStatus {
        is_loaded: transcription_manager.is_model_loaded(),
        current_model: transcription_manager.get_current_model(),
    })
}

#[tauri::command]
#[specta::specta]
pub fn unload_model_manually(
    transcription_manager: State<'_, Arc<TranscriptionManager>>,
) -> Result<(), String> {
    transcription_manager
        .unload_model()
        .map_err(|e| format!("Failed to unload model: {}", e))
}

#[tauri::command]
#[specta::specta]
pub async fn unload_extra_model(
    transcription_manager: State<'_, Arc<TranscriptionManager>>,
    model_id: String,
) -> Result<(), String> {
    // Dropping a multi-GB engine takes hundreds of milliseconds; keep it off
    // the main thread so the settings UI doesn't freeze.
    let tm = Arc::clone(&*transcription_manager);
    tauri::async_runtime::spawn_blocking(move || {
        tm.unload_extra_model(&model_id)
            .map_err(|e| format!("Failed to unload extra model: {}", e))
    })
    .await
    .map_err(|e| format!("Unload task panicked: {}", e))?
}

#[tauri::command]
#[specta::specta]
pub fn get_extra_loaded_models(
    transcription_manager: State<'_, Arc<TranscriptionManager>>,
) -> Result<Vec<String>, String> {
    Ok(transcription_manager.get_extra_loaded_models())
}

#[tauri::command]
#[specta::specta]
pub async fn load_extra_model(
    transcription_manager: State<'_, Arc<TranscriptionManager>>,
    model_id: String,
) -> Result<(), String> {
    let tm = Arc::clone(&*transcription_manager);
    let model_id_clone = model_id.clone();
    // Propagate the load error: the frontend toasts success on `Ok`, so
    // swallowing it here showed "loaded" for a model that wasn't downloaded.
    tauri::async_runtime::spawn_blocking(move || {
        tm.load_extra_model(&model_id_clone)
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("Load task panicked: {}", e))?
}
