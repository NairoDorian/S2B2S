//! Event types shared between the Rust backend and the overlay webview frontend.
//!
//! These are the "dumb renderer contract" — the avatar renders whatever state
//! it receives, holding zero logic.

#[derive(Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct OverlayState {
    pub phase: OverlayPhase,
    pub text: Option<String>,
    pub tokens_per_sec: Option<f32>,
    pub latency_ms: Option<u32>,
    pub duration_s: Option<f32>,
}

#[derive(Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum OverlayPhase {
    Idle,
    Listening,
    Thinking,
    Seeing,
    Speaking,
    Done,
    Error,
    Hidden,
}

impl OverlayState {
    pub fn new(phase: OverlayPhase) -> Self {
        Self {
            phase,
            text: None,
            tokens_per_sec: None,
            latency_ms: None,
            duration_s: None,
        }
    }

    pub fn idle() -> Self {
        Self::new(OverlayPhase::Idle)
    }

    pub fn hidden() -> Self {
        Self::new(OverlayPhase::Hidden)
    }
}

/// Payload for the cursor-follow position update (sent at ~30 Hz).
#[derive(Clone, serde::Serialize, specta::Type)]
pub struct CursorPosition {
    pub x: f64,
    pub y: f64,
    pub monitor_id: Option<String>,
}

/// Payload for the bubble append (streaming reply text).
#[derive(Clone, serde::Serialize, specta::Type)]
pub struct BubbleAppend {
    pub text: String,
    pub is_final: bool,
}
