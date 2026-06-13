//! overlay_fx — Transparent overlay engine for the S2B2S avatar + cursor trail.
//!
//! Track A:  webview overlay (Three.js avatar + HTML bubble)  — always available.
//! Track B:  native wgpu layer (cursor trail + click ripple)  — feature-gated,
//!           vendored from Cross_Platform_Rust_WebGPU_CursorFX + TD_Web_Trail.
//!
//! Both tracks are driven by the same event bus: brain:*, mic-level, tts:level.

pub mod capabilities;
pub mod commands;
pub mod cursor_follow;
pub mod events;
pub mod placement;
pub mod trail;
pub mod window;

/// Overlay capability probe result — returned to the frontend, typed via specta.
#[derive(Clone, serde::Serialize, specta::Type)]
pub struct OverlayCapabilities {
    pub os: String,
    pub webgpu: bool,
    pub vulkan: bool,
    pub cursor_position: bool,
    pub layer_shell: bool,
    pub native_transparent: bool,
}

impl OverlayCapabilities {
    pub fn probe() -> Self {
        Self {
            os: std::env::consts::OS.to_string(),
            webgpu: cfg!(any(target_os = "windows", target_os = "macos")),
            vulkan: cfg!(any(
                target_os = "windows",
                target_os = "linux"
            )),
            cursor_position: cfg!(any(
                target_os = "windows",
                target_os = "macos",
                all(target_os = "linux", target_env = "gnu")
            )),
            layer_shell: cfg!(target_os = "linux"),
            native_transparent: cfg!(any(
                target_os = "windows",
                target_os = "linux",
                target_os = "macos"
            )),
        }
    }
}
