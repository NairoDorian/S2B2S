//! Engine lifecycle: warm (pre-load model in RAM), status monitoring, and
//! graceful unload. The warm-model pattern (from CopySpeak tts-perf-v2) is the
//! single biggest latency win for local TTS engines.
//!
//! ─── Lifecycle States ───
//!   Stopped → Loading → WarmingUp → Ready
//!     ↑          ↑         ↓
//!     └──────────┴───── Error

use std::fmt;

/// Engine lifecycle status, surfaced to the UI footer and control API.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum EngineStatus {
    /// Model not loaded; idle.
    Stopped,
    /// Model is being loaded from disk (IO-bound, ~seconds).
    Loading,
    /// Model loaded; running warm-up inference to JIT-compile kernels (GPU-bound, ~seconds).
    WarmingUp,
    /// Model resident in RAM/VRAM and ready for synthesis (sub-100ms TTFA).
    Ready,
    /// Initialization or warm-up failed; requires user action.
    Error,
    /// Model was explicitly unloaded by the user or on engine switch.
    Unloaded,
}

#[allow(dead_code)]
impl EngineStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            EngineStatus::Stopped => "stopped",
            EngineStatus::Loading => "loading",
            EngineStatus::WarmingUp => "warming_up",
            EngineStatus::Ready => "ready",
            EngineStatus::Error => "error",
            EngineStatus::Unloaded => "unloaded",
        }
    }
}

impl fmt::Display for EngineStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Trait for engines that support warm-up (loading the model and running a
/// no-op inference to force JIT/GPU kernel compilation).
///
/// Engines implementing this trait are loaded at app startup so the first user
/// request pays zero cold-start tax. Engines that don't implement it (cloud
/// backends, SAPI) are always "ready" by definition.
#[allow(dead_code)]
pub trait WarmEngine {
    /// Load the model and run a warm-up inference sentence.
    /// `warm()` must be safe to call multiple times (idempotent).
    fn warm(&self) -> Result<(), String>;

    /// Free the model from RAM/VRAM. Called on engine switch or manual unload.
    fn unload(&self) -> Result<(), String>;

    /// Current lifecycle status.
    fn status(&self) -> EngineStatus;

    /// Whether the engine is ready for synthesis.
    fn is_ready(&self) -> bool {
        matches!(self.status(), EngineStatus::Ready)
    }
}
