use crate::managers::history::HistoryManager;
use crate::managers::model::{ModelInfo, ModelManager};
use crate::managers::transcription::{BenchmarkResult, ModelStateEvent, TranscriptionManager};
use crate::settings::{
    ModelUnloadTimeout, NativeStreamingLatencyPreset, get_settings, write_settings,
};
use log::error;
use log::info;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

/// A single quantization variant of a catalog model — returned to the frontend
/// so the UI can render a quant picker (chips / dropdown) and let the user
/// select or download a non-default quant.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct QuantVariant {
    pub quant: String,
    pub filename: String,
    pub model_id: String,
    #[specta(type = u32)]
    pub size_mb: u32,
    pub is_default: bool,
}

#[tauri::command]
#[specta::specta]
pub fn get_model_quant_variants(
    _model_manager: State<'_, Arc<ModelManager>>,
    model_id: String,
) -> Result<Vec<QuantVariant>, String> {
    let (repo_id, filename) = model_id
        .rsplit_once('/')
        .ok_or_else(|| format!("Invalid model id: {}", model_id))?;
    let (descriptor, _) = crate::catalog::file_in_catalog(filename, Some(repo_id))
        .ok_or_else(|| format!("Model '{}' is not a catalog model", model_id))?;
    Ok(descriptor
        .files
        .iter()
        .map(|f| QuantVariant {
            quant: f.quant.clone(),
            filename: f.filename.clone(),
            model_id: format!("{}/{}", repo_id, f.filename),
            size_mb: f.size_bytes.div_ceil(1024 * 1024) as u32,
            is_default: f.quant == descriptor.default_quant.clone().unwrap_or_default(),
        })
        .collect())
}

#[tauri::command]
#[specta::specta]
pub async fn download_model_quant(
    model_manager: State<'_, Arc<ModelManager>>,
    model_id: String,
) -> Result<(), String> {
    model_manager
        .download_catalog_quant(&model_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn change_native_streaming_latency_preset_setting(
    app: AppHandle,
    model_id: String,
    preset: NativeStreamingLatencyPreset,
) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings
        .native_streaming_latency_presets
        .insert(model_id, preset);
    write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn get_available_models(
    model_manager: State<'_, Arc<ModelManager>>,
) -> Result<Vec<ModelInfo>, String> {
    Ok(model_manager.get_available_models())
}

#[tauri::command]
#[specta::specta]
pub async fn get_model_info(
    model_manager: State<'_, Arc<ModelManager>>,
    model_id: String,
) -> Result<Option<ModelInfo>, String> {
    Ok(model_manager.get_model_info(&model_id))
}

/// Re-scan local sources (custom models dir + shared HF cache) for models added
/// since launch
#[tauri::command]
#[specta::specta]
pub async fn rescan_local_models(
    model_manager: State<'_, Arc<ModelManager>>,
) -> Result<(), String> {
    let mm = model_manager.inner().clone();
    tokio::task::spawn_blocking(move || mm.rescan_local_models())
        .await
        .map_err(|e| format!("rescan task panicked: {e}"))?
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn download_model(
    app_handle: AppHandle,
    model_manager: State<'_, Arc<ModelManager>>,
    model_id: String,
) -> Result<(), String> {
    let result = model_manager
        .download_model(&model_id)
        .await
        .map_err(|e| e.to_string());

    if let Err(ref error) = result {
        // Log as well as emit: the toast is transient, and failed downloads have
        // historically been undiagnosable because logs showed nothing (#1579).
        error!("Model download failed for {}: {}", model_id, error);
        let _ = app_handle.emit(
            "model-download-failed",
            serde_json::json!({ "model_id": &model_id, "error": error }),
        );
    }

    result
}

#[tauri::command]
#[specta::specta]
pub async fn delete_model(
    app_handle: AppHandle,
    model_manager: State<'_, Arc<ModelManager>>,
    transcription_manager: State<'_, Arc<TranscriptionManager>>,
    model_id: String,
) -> Result<(), String> {
    // If deleting the active model, unload it and clear the setting
    let settings = get_settings(&app_handle);
    if settings.selected_model == model_id {
        transcription_manager
            .unload_model()
            .map_err(|e| format!("Failed to unload model: {}", e))?;

        let mut settings = get_settings(&app_handle);
        settings.selected_model = String::new();
        write_settings(&app_handle, settings);
    }

    model_manager
        .delete_model(&model_id)
        .map_err(|e| e.to_string())
}

/// Shared logic for switching the active model, used by both the Tauri command
/// and the tray menu handler.
///
/// Validates the model, updates the persisted setting, and loads the model
/// unless the unload timeout is set to "Immediately" (in which case the model
/// will be loaded on-demand during the next transcription).
pub fn switch_active_model(app: &AppHandle, model_id: &str) -> Result<(), String> {
    let model_manager = app.state::<Arc<ModelManager>>();
    let transcription_manager = app.state::<Arc<TranscriptionManager>>();

    // Atomically claim the loading slot — prevents concurrent model loads
    // from tray double-clicks or overlapping commands. The guard resets the
    // flag on drop (including early returns, errors, and panics).
    let _loading_guard = transcription_manager
        .try_start_loading()
        .ok_or_else(|| "Model load already in progress".to_string())?;

    // Check if model exists and is available
    let model_info = model_manager
        .get_model_info(model_id)
        .ok_or_else(|| format!("Model not found: {}", model_id))?;

    if !model_info.is_downloaded {
        return Err(format!("Model not downloaded: {}", model_id));
    }

    let settings = get_settings(app);
    let unload_timeout = settings.model_unload_timeout;
    let old_model = settings.selected_model.clone();
    let old_onboarding_completed = settings.onboarding_completed;

    // Persist the new selection early so the frontend sees the correct model
    // when it reacts to events emitted by load_model.
    let mut settings = settings;
    settings.selected_model = model_id.to_string();
    settings.onboarding_completed = true;

    write_settings(app, settings);

    // Skip eager loading if unload is set to "Immediately" — the model
    // will be loaded on-demand during the next transcription.
    if unload_timeout == ModelUnloadTimeout::Immediately {
        // Notify frontend — load_model won't be called so no events
        // would otherwise be emitted.
        let _ = app.emit(
            "model-state-changed",
            ModelStateEvent {
                event_type: "selection_changed".to_string(),
                model_id: Some(model_id.to_string()),
                model_name: Some(model_info.name.clone()),
                error: None,
            },
        );
        log::info!(
            "Model selection changed to {} (not loading — unload set to Immediately).",
            model_id
        );
        return Ok(());
    }

    // Load the model. On failure, revert the persisted selection.
    if let Err(e) = transcription_manager.load_model(model_id) {
        let mut settings = get_settings(app);
        settings.selected_model = old_model;
        settings.onboarding_completed = old_onboarding_completed;
        write_settings(app, settings);
        return Err(e.to_string());
    }

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn set_active_model(
    app_handle: AppHandle,
    _model_manager: State<'_, Arc<ModelManager>>,
    _transcription_manager: State<'_, Arc<TranscriptionManager>>,
    model_id: String,
) -> Result<(), String> {
    switch_active_model(&app_handle, &model_id)
}

#[tauri::command]
#[specta::specta]
pub async fn get_current_model(app_handle: AppHandle) -> Result<String, String> {
    let settings = get_settings(&app_handle);
    Ok(settings.selected_model)
}

#[tauri::command]
#[specta::specta]
pub async fn get_transcription_model_status(
    transcription_manager: State<'_, Arc<TranscriptionManager>>,
) -> Result<Option<String>, String> {
    Ok(transcription_manager.get_current_model())
}

#[tauri::command]
#[specta::specta]
pub async fn is_model_loading(
    transcription_manager: State<'_, Arc<TranscriptionManager>>,
) -> Result<bool, String> {
    // Check if transcription manager has a loaded model
    let current_model = transcription_manager.get_current_model();
    Ok(current_model.is_none())
}

#[tauri::command]
#[specta::specta]
pub async fn cancel_download(
    model_manager: State<'_, Arc<ModelManager>>,
    model_id: String,
) -> Result<(), String> {
    model_manager
        .cancel_download(&model_id)
        .map_err(|e| e.to_string())
}

/// Benchmark all downloaded quantization variants of the model identified by
/// `model_id`.  Uses the latest completed recording as the reference audio.
///
/// For each downloaded quant (e.g. Q4_K_M, Q5_K_M, Q8_0) the model is loaded
/// on a temporary engine, a warmup transcription is discarded, three timed
/// transcriptions are averaged, and the engine is dropped before the next
/// variant.  Progress events are emitted on the `benchmark-progress` channel
/// and the full result vector is returned when all variants are done.
#[tauri::command]
#[specta::specta]
pub async fn benchmark_model_quantizations(
    _model_manager: State<'_, Arc<ModelManager>>,
    transcription_manager: State<'_, Arc<TranscriptionManager>>,
    history_manager: State<'_, Arc<HistoryManager>>,
    model_id: String,
) -> Result<Vec<BenchmarkResult>, String> {
    // Find the latest completed recording to use as reference audio.
    let entry = history_manager
        .get_latest_completed_entry()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| {
            "No completed recordings found. Record audio first to use the benchmark.".to_string()
        })?;

    let audio_path = history_manager.get_audio_file_path(&entry.file_name);
    let samples = crate::audio_toolkit::read_wav_samples(&audio_path)
        .map_err(|e| format!("Failed to load audio: {}", e))?;

    if samples.is_empty() {
        return Err("Recording has no audio samples".to_string());
    }

    info!(
        "Starting quantization benchmark for model '{}' using recording '{}' ({} samples)",
        model_id,
        entry.file_name,
        samples.len()
    );

    let tm = Arc::clone(&transcription_manager);
    let model_id_clone = model_id.clone();
    let results = tauri::async_runtime::spawn_blocking(move || {
        tm.benchmark_quantizations(&model_id_clone, &samples)
    })
    .await
    .map_err(|e| format!("Benchmark task panicked: {}", e))?
    .map_err(|e| e.to_string())?;

    Ok(results)
}

/// Benchmark a single quantization variant of the current model.
/// Uses the latest completed recording as the reference audio.
///
/// The engine is loaded from a clean state (primary model unloaded first),
/// a warmup transcription is discarded, three timed runs are averaged,
/// and the engine is dropped after the run.
#[tauri::command]
#[specta::specta]
pub async fn benchmark_single_quantization(
    transcription_manager: State<'_, Arc<TranscriptionManager>>,
    history_manager: State<'_, Arc<HistoryManager>>,
    model_id: String,
) -> Result<BenchmarkResult, String> {
    let entry = history_manager
        .get_latest_completed_entry()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| {
            "No completed recordings found. Record audio first to use the benchmark.".to_string()
        })?;

    let audio_path = history_manager.get_audio_file_path(&entry.file_name);
    let samples = crate::audio_toolkit::read_wav_samples(&audio_path)
        .map_err(|e| format!("Failed to load audio: {}", e))?;

    if samples.is_empty() {
        return Err("Recording has no audio samples".to_string());
    }

    info!(
        "Starting single-quant benchmark for model '{}' using recording '{}' ({} samples)",
        model_id,
        entry.file_name,
        samples.len()
    );

    let tm = Arc::clone(&transcription_manager);
    let model_id_clone = model_id.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        tm.benchmark_single_quantization(&model_id_clone, &samples)
    })
    .await
    .map_err(|e| format!("Benchmark task panicked: {}", e))?
    .map_err(|e| e.to_string())?;

    Ok(result)
}
