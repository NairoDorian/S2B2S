//! Native wgpu overlay module (Track B).
//!
//! Feature-gated behind `overlay-native`. When fully implemented, provides a
//! full-screen transparent wgpu surface that renders the cursor trail + click ripple.
//!
//! **Status:** The trail physics engine (`overlay_fx::trail::TrailSystem`) is complete.
//! The wgpu surface integration requires vendoring from `Cross_Platform_Rust_WebGPU_CursorFX`
//! and adapting for the wgpu 29 API surface. The render loop skeleton is in `render_loop_stub.rs`.
//!
//! **GPU backend constraint (Windows):** Must use **Vulkan** — DX12 OOMs on
//! transparent overlay surfaces (confirmed on RTX 4070). Apply the NVAPI
//! "Prefer Native" present fix after first install.

/// Placeholder struct for the native trail overlay.
/// Real implementation will vendored from CursorFX when overlay-native feature is complete.
pub struct NativeTrailOverlay {
    _private: (),
}

impl NativeTrailOverlay {
    pub fn start(
        _app: &tauri::AppHandle,
        _config: crate::settings::WgpuTrailConfig,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        log::info!("NativeTrailOverlay::start called — overlay-native feature is enabled but the wgpu surface integration is pending (see overlay_fx/native/mod.rs)");
        Ok(Self { _private: () })
    }

    pub fn stop(&self) {}
}
