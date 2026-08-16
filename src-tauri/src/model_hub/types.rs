//! Unified model-management types shared by every engine collection
//! (STT, Brain, TTS/audio.cpp, Runtimes).
//!
//! All fields use specta-safe types (`f64`/`String`/`bool` — never
//! `usize`/`u64`/`i64`) so `bindings.ts` generation never trips over
//! BigInt-style mappings.

use serde::{Deserialize, Serialize};
use specta::Type;
use std::hash::Hash;

/// Which engine collection a model/download belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type, Hash)]
#[serde(rename_all = "lowercase")]
pub enum ModelCollection {
    /// Speech-to-text models (transcribe.cpp / transcribe-rs family).
    Stt,
    /// Local Brain GGUF models (Gemma 4 via llama.cpp).
    Brain,
    /// Text-to-speech models (audio.cpp packages, Python-engine models).
    Tts,
    /// Runtime binaries (llama.cpp server builds).
    Runtime,
}

impl ModelCollection {
    /// Stable string key used to address entries in hub state.
    pub fn key(self) -> &'static str {
        match self {
            ModelCollection::Stt => "stt",
            ModelCollection::Brain => "brain",
            ModelCollection::Tts => "tts",
            ModelCollection::Runtime => "runtime",
        }
    }
}

/// Lifecycle phase of a hub download.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum HubDownloadStatus {
    Downloading,
    Verifying,
    Extracting,
    Completed,
    Failed,
    Cancelled,
}

/// One progress snapshot for a single downloadable entry. Emitted on the
/// typed `model-hub-download-progress` event, throttled by each emitter.
#[derive(Debug, Clone, Serialize, Deserialize, Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct ModelHubDownloadProgress {
    pub collection: ModelCollection,
    /// Collection-scoped entry id (model id, package id, or `backend-tag`).
    pub id: String,
    /// Human-readable name for the row/card.
    pub name: String,
    /// Optional sub-file being fetched (multi-file packages).
    pub file: Option<String>,
    pub downloaded_mb: f64,
    pub total_mb: f64,
    /// 0–100.
    pub percent: f64,
    pub speed_mbps: f64,
    pub status: HubDownloadStatus,
    pub error: Option<String>,
}

/// Terminal notifications (complete/failed/cancelled/deleted) for a hub entry,
/// emitted once at the end of an operation.
#[derive(Debug, Clone, Serialize, Deserialize, Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct ModelHubNotification {
    pub collection: ModelCollection,
    pub id: String,
    pub name: String,
    pub kind: HubNotificationKind,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum HubNotificationKind {
    Completed,
    Failed,
    Cancelled,
    Deleted,
}

/// Emitted event names (tauri-specta kebab-cases the struct name).
impl ModelHubDownloadProgress {
    pub const EVENT_NAME: &'static str = "model-hub-download-progress";
}

impl ModelHubNotification {
    pub const EVENT_NAME: &'static str = "model-hub-notification";
}

/// Unified download request dispatched through [`super::commands::hub_download_model`].
///
/// The `id` is collection-scoped: a model id for STT, the hub download id for
/// Brain, a package id for TTS, or `"{backend}-{tag}"` for Runtime.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct HubDownloadRequest {
    pub collection: ModelCollection,
    pub id: String,
}
