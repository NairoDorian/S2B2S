//! Text-to-Speech subsystem — the "Read Anywhere" / CopySpeak pillar of S2B2S.
//!
//! The app does not care how speech is synthesized — only that it gets audio
//! bytes back from a [`TtsBackend`]. Engines are warm and resident where
//! possible; long-lived child processes must have their stdio drained.

pub mod audio_format;
pub mod backends;
pub mod clipboard_watch;
pub mod local_tts_server;
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
    #[allow(dead_code)]
    fn name(&self) -> &str;

    /// Synthesize `text` with `voice` at `speed` into audio bytes.
    ///
    /// `speed` is the single owner of playback rate (CopySpeak C1): it is passed
    /// to the engine here and must never be re-applied at playback time.
    fn synthesize(&self, text: &str, voice: &str, speed: f32) -> Result<Vec<u8>, String>;

    /// Whether this backend can deliver audio incrementally via
    /// [`Self::synthesize_streaming`] (chunk-level streaming — lower
    /// time-to-first-audio on long replies).
    fn supports_streaming(&self) -> bool {
        false
    }

    /// Stream-synthesize `text`, invoking `on_pcm(sample_rate, i16_frames)`
    /// for every audio chunk as it is generated. Blocks until synthesis
    /// finishes. Default implementation fails — backends that return true
    /// from [`Self::supports_streaming`] must override it.
    fn synthesize_streaming(
        &self,
        _text: &str,
        _voice: &str,
        _speed: f32,
        _on_pcm: &mut dyn FnMut(u32, Vec<i16>),
    ) -> Result<(), String> {
        Err("This TTS backend does not support streaming".to_string())
    }

    /// Check that the engine/server is reachable.
    #[allow(dead_code)]
    fn health_check(&self) -> Result<(), String>;

    /// File extension for the bytes returned by [`Self::synthesize`].
    #[allow(dead_code)]
    fn file_extension(&self) -> &str {
        "wav"
    }
}
