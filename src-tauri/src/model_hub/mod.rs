//! Model Hub: one normalized download/management spine behind every engine
//! collection (STT, Brain, TTS/audio.cpp, Runtimes), feeding the unified
//! Models page in the frontend.

pub mod commands;
pub mod transport;
pub mod types;

use std::collections::HashMap;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};
use tauri_specta::Event as _;

pub use types::{
    HubDownloadRequest, HubDownloadStatus, HubNotificationKind, ModelCollection,
    ModelHubDownloadProgress, ModelHubNotification,
};

/// Registry of in-flight (and last-known) downloads, so the UI can query
/// state on mount instead of waiting for the next event tick.
#[derive(Default)]
pub struct HubRegistry {
    entries: Mutex<HashMap<(ModelCollection, String), ModelHubDownloadProgress>>,
}

impl HubRegistry {
    pub fn update(&self, progress: ModelHubDownloadProgress) {
        self.entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert((progress.collection, progress.id.clone()), progress);
    }

    pub fn remove(&self, collection: ModelCollection, id: &str) {
        self.entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&(collection, id.to_string()));
    }

    pub fn snapshot(&self) -> Vec<ModelHubDownloadProgress> {
        self.entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .cloned()
            .collect()
    }
}

/// Update the registry and emit a typed progress event. Called by every
/// collection's download loop, throttled on the transport side already.
pub fn emit_progress(app: &AppHandle, progress: ModelHubDownloadProgress) {
    if let Some(registry) = app.try_state::<HubRegistry>() {
        registry.update(progress.clone());
    }
    let _ = progress.emit(app);
}

/// Record a terminal state, emit the notification event, and drop the entry
/// from the active-downloads registry.
pub fn notify(
    app: &AppHandle,
    collection: ModelCollection,
    id: &str,
    name: &str,
    kind: HubNotificationKind,
    error: Option<String>,
) {
    if let Some(registry) = app.try_state::<HubRegistry>() {
        if matches!(
            kind,
            HubNotificationKind::Completed
                | HubNotificationKind::Cancelled
                | HubNotificationKind::Deleted
        ) {
            registry.remove(collection, id);
        }
    }
    let _ = ModelHubNotification {
        collection,
        id: id.to_string(),
        name: name.to_string(),
        kind,
        error,
    }
    .emit(app);
}
