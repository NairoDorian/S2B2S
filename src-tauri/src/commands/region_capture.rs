//! Tauri commands for the region capture overlay → multimodal Brain bridge.

use crate::region_capture::{ManagedRegionCaptureState, SelectedRegion, VirtualScreenInfo};
use tauri::{AppHandle, Manager};

/// Response for the picker's initial data request.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct RegionCaptureData {
    pub virtual_screen: VirtualScreenInfo,
}

/// Called from the overlay when it is ready to render: returns the virtual
/// screen geometry so the frontend can convert logical → physical pixels.
#[tauri::command]
#[specta::specta]
pub fn region_capture_get_data(app: AppHandle) -> Result<RegionCaptureData, String> {
    let state = app.state::<ManagedRegionCaptureState>();
    let guard = state.lock().unwrap();
    let virtual_info = guard
        .virtual_info
        .as_ref()
        .ok_or("No virtual screen info available")?
        .clone();
    Ok(RegionCaptureData {
        virtual_screen: virtual_info,
    })
}

/// Called from the overlay when the user confirms a region (physical pixels).
#[tauri::command]
#[specta::specta]
pub fn region_capture_confirm(app: AppHandle, region: SelectedRegion) {
    crate::region_capture::on_region_selected(&app, region);
}

/// Called from the overlay when the user cancels (Escape).
#[tauri::command]
#[specta::specta]
pub fn region_capture_cancel(app: AppHandle) {
    crate::region_capture::on_region_cancelled(&app);
}
