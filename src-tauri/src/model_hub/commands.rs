//! Tauri command surface for the unified Model Hub: download, cancel, delete
//! and list with one signature across all four collections, dispatched to the
//! owning manager.

use super::{HubDownloadRequest, HubRegistry, ModelCollection, ModelHubDownloadProgress};
use std::sync::Arc;
use tauri::{AppHandle, Manager};

/// Cancel an in-flight download, whatever collection owns it.
#[tauri::command]
#[specta::specta]
pub fn hub_cancel_download(
    app: AppHandle,
    collection: ModelCollection,
    id: String,
) -> Result<bool, String> {
    match collection {
        ModelCollection::Stt => {
            let manager = app
                .try_state::<std::sync::Arc<crate::managers::model::ModelManager>>()
                .ok_or("Model manager not initialized")?;
            manager.cancel_download(&id).map_err(|e| e.to_string())?;
            Ok(true)
        }
        ModelCollection::Brain => {
            let manager = app
                .try_state::<std::sync::Arc<crate::brain::llama_manager::LlamaManager>>()
                .ok_or("Llama manager not initialized")?;
            Ok(manager.cancel_download())
        }
        ModelCollection::Tts => Ok(crate::audiocpp_server::cancel_package_download(&id)),
        ModelCollection::Runtime => {
            let manager = app
                .try_state::<std::sync::Arc<crate::llama_server::manager::LlamaServerManager>>()
                .ok_or("Llama server manager not initialized")?;
            Ok(manager.cancel_download(&id))
        }
    }
}

/// Delete an installed model/package/runtime, whatever collection owns it.
#[tauri::command]
#[specta::specta]
pub fn hub_delete_model(
    app: AppHandle,
    collection: ModelCollection,
    id: String,
) -> Result<(), String> {
    let result = match collection {
        ModelCollection::Stt => {
            let manager = app
                .try_state::<std::sync::Arc<crate::managers::model::ModelManager>>()
                .ok_or("Model manager not initialized")?;
            manager.delete_model(&id).map_err(|e| e.to_string())
        }
        ModelCollection::Brain => {
            let manager = app
                .try_state::<std::sync::Arc<crate::brain::llama_manager::LlamaManager>>()
                .ok_or("Llama manager not initialized")?;
            manager.delete_model_file(&id)
        }
        ModelCollection::Tts => crate::audiocpp_server::delete_package(&app, &id),
        ModelCollection::Runtime => {
            let manager = app
                .try_state::<std::sync::Arc<crate::llama_server::manager::LlamaServerManager>>()
                .ok_or("Llama server manager not initialized")?;
            // id format is "{backend}-{tag}"; backend may itself contain a
            // hyphen (cuda-13.3) so split on the LAST hyphen.
            let (backend, tag) = id
                .rsplit_once('-')
                .ok_or_else(|| format!("Invalid runtime id '{id}'"))?;
            manager.remove_server(backend, tag)
        }
    };
    result.map(|_| {
        super::notify(
            &app,
            collection,
            &id,
            &id,
            super::HubNotificationKind::Deleted,
            None,
        );
    })
}

/// All downloads known to the hub (in-flight and last-seen), for restoring
/// UI state on page mount / app reload.
#[tauri::command]
#[specta::specta]
pub fn hub_get_active_downloads(app: AppHandle) -> Vec<ModelHubDownloadProgress> {
    app.try_state::<HubRegistry>()
        .map(|r| r.snapshot())
        .unwrap_or_default()
}

/// Start a download for any collection through one unified command.
///
/// The `id` is collection-scoped:
/// - **STT**: a model id (e.g. `"whisper-tiny"`) or a quant variant id
///   (`"repo/filename"` — routed through `download_catalog_quant`).
/// - **Brain**: any non-empty string (the LlamaManager reads its target from
///   settings, so the id is informational only).
/// - **TTS**: a package id from the audio.cpp catalog.
/// - **Runtime**: `"{backend}-{tag}"` (e.g. `"cuda-b9741"`); the command looks
///   up the GitHub release asset URL automatically.
#[tauri::command]
#[specta::specta]
pub async fn hub_download_model(app: AppHandle, request: HubDownloadRequest) -> Result<(), String> {
    match request.collection {
        ModelCollection::Stt => {
            let manager = app
                .try_state::<Arc<crate::managers::model::ModelManager>>()
                .ok_or("Model manager not initialized")?;
            // A quant-variant id contains '/' (e.g. "repo/filename.Q4_K_M.gguf").
            let result = if request.id.contains('/') {
                manager.download_catalog_quant(&request.id).await
            } else {
                manager.download_model(&request.id).await
            };
            result.map_err(|e| e.to_string())
        }
        ModelCollection::Brain => {
            let manager = app
                .try_state::<Arc<crate::brain::llama_manager::LlamaManager>>()
                .ok_or("Llama manager not initialized")?;
            manager.inner().clone().start_download_in_background();
            Ok(())
        }
        ModelCollection::Tts => crate::audiocpp_server::start_package_download(app, request.id),
        ModelCollection::Runtime => {
            let manager = app
                .try_state::<Arc<crate::llama_server::manager::LlamaServerManager>>()
                .ok_or("Llama server manager not initialized")?;
            let (backend, tag) = request
                .id
                .rsplit_once('-')
                .ok_or_else(|| format!("Invalid runtime id '{}'", request.id))?;
            let url = manager.find_release_download_url(backend, tag).await?;
            manager.download_server(backend, tag, &url).await
        }
    }
}
