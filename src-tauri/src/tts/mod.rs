//! Text-to-Speech subsystem — the "Read Anywhere" / CopySpeak pillar of S2B2S.
//!
//! Ported and adapted from the AgentZero prototype (MIT). The app does not care
//! how speech is synthesized — only that it gets audio bytes back from a
//! [`TtsBackend`]. Engines are warm and resident where possible (CopySpeak's key
//! perf win); long-lived child processes must have their stdio drained (C4).

pub mod audio_format;
pub mod backends;
pub mod clipboard_watch;
pub mod fragment_queue;
pub mod manager;
pub mod pagination;
pub mod player;
pub mod sanitize;
pub mod status;
pub mod telemetry;

/// Metadata for a voice option exposed in the settings UI.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct Voice {
    pub id: String,
    pub name: String,
    pub language: Option<String>,
}

/// The core abstraction. Every TTS engine implements this.
///
/// Kept intentionally small — synthesize text, get audio bytes. `synthesize`
/// blocks until synthesis completes and is therefore always called from a
/// blocking context (e.g. `tauri::async_runtime::spawn_blocking`).
pub trait TtsBackend: Send + Sync {
    /// Human-readable name for settings UI / logs.
    fn name(&self) -> &str;

    /// Synthesize `text` with `voice` at `speed` into audio bytes.
    ///
    /// `speed` is the single owner of playback rate (CopySpeak C1): it is passed
    /// to the engine here and must never be re-applied at playback time.
    fn synthesize(&self, text: &str, voice: &str, speed: f32) -> Result<Vec<u8>, String>;

    /// Check that the engine/server is reachable.
    fn health_check(&self) -> Result<(), String>;

    /// File extension for the bytes returned by [`Self::synthesize`].
    fn file_extension(&self) -> &str {
        "wav"
    }
}
