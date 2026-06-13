//! overlay_fx Tauri IPC commands — exposed to the frontend via specta.

use crate::overlay_fx::capabilities;
use crate::overlay_fx::window;
use crate::overlay_fx::OverlayCapabilities;
use tauri::AppHandle;

/// Probe the current machine's overlay capabilities (WebGPU, Vulkan, cursor position, etc.).
#[tauri::command]
#[specta::specta]
pub fn overlay_fx_probe_capabilities() -> OverlayCapabilities {
    capabilities::get_overlay_capabilities()
}

/// Show the brain overlay at the cursor position and begin conversation.
#[tauri::command]
#[specta::specta]
pub fn overlay_fx_show_conversation(app: AppHandle) -> Result<(), String> {
    window::show_brain_overlay(&app);
    Ok(())
}

/// Hide the brain overlay and end the conversation.
#[tauri::command]
#[specta::specta]
pub fn overlay_fx_dismiss(app: AppHandle) -> Result<(), String> {
    window::hide_brain_overlay(&app);
    Ok(())
}
