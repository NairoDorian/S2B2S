//! Region capture overlay → multimodal Brain (M3.8).
//!
//! Opens a transparent full-screen overlay window the user drags a rectangle
//! on; on confirm the overlay hides and, after a 50 ms settle delay, the
//! selected region is captured, cropped and PNG-encoded for the Brain.
//!
//! The picker UI lives in `src/region-capture/` and talks to this module
//! through the commands in `commands/region_capture.rs`.
//!
//! Windows is the primary target (DXGI capture via the `screenshots` crate).
//! macOS/Linux keep the state types and commands registered but return a
//! clear "not supported" error from `open_region_picker` — the rest of the
//! app never crashes, it just degrades (cross-platform mandate).

use log::debug;
use specta::Type;
use tauri::{AppHandle, Manager};
use tokio::sync::oneshot;

/// Information about the virtual screen (all monitors combined).
#[derive(Debug, Clone, serde::Serialize, Type)]
pub struct VirtualScreenInfo {
    /// Minimum X coordinate (can be negative if monitors are left of primary)
    pub offset_x: i32,
    /// Minimum Y coordinate
    pub offset_y: i32,
    /// Total width spanning all monitors
    pub total_width: u32,
    /// Total height spanning all monitors
    pub total_height: u32,
    /// Scale factor of the primary monitor (logical → physical conversion)
    pub scale_factor: f64,
}

/// Region selected by the user, in physical (virtual-screen) pixels.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Type)]
pub struct SelectedRegion {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Result of a region capture operation.
#[derive(Debug)]
pub enum RegionCaptureResult {
    /// User confirmed a region; carries the cropped PNG bytes.
    Selected {
        region: SelectedRegion,
        image_data: Vec<u8>,
    },
    /// User cancelled (Escape or window closed).
    Cancelled,
    /// An error occurred.
    Error(String),
}

/// State for tracking the single in-flight region capture operation.
#[derive(Default)]
pub struct RegionCaptureState {
    pub result_sender: Option<oneshot::Sender<RegionCaptureResult>>,
    pub virtual_info: Option<VirtualScreenInfo>,
}

pub type ManagedRegionCaptureState = std::sync::Mutex<RegionCaptureState>;

/// Atomically claims and clears the current picker state.
fn take_pending_region_capture(
    app: &AppHandle,
) -> (
    Option<oneshot::Sender<RegionCaptureResult>>,
    Option<VirtualScreenInfo>,
) {
    let state = app.state::<ManagedRegionCaptureState>();
    let mut guard = state.lock().unwrap();
    (guard.result_sender.take(), guard.virtual_info.take())
}

/// Completes the current picker as cancelled, if it is still pending.
fn cancel_pending_region_capture(app: &AppHandle) {
    let (sender, _) = take_pending_region_capture(app);
    if let Some(sender) = sender {
        let _ = sender.send(RegionCaptureResult::Cancelled);
    }
}

/// Gets the virtual screen info (all monitors combined).
#[cfg(target_os = "windows")]
pub fn get_virtual_screen_info() -> Result<VirtualScreenInfo, String> {
    use screenshots::Screen;

    let screens = Screen::all().map_err(|e| format!("Failed to enumerate screens: {e}"))?;
    if screens.is_empty() {
        return Err("No screens found".to_string());
    }

    let min_x = screens.iter().map(|s| s.display_info.x).min().unwrap_or(0);
    let min_y = screens.iter().map(|s| s.display_info.y).min().unwrap_or(0);
    let max_x = screens
        .iter()
        .map(|s| s.display_info.x + s.display_info.width as i32)
        .max()
        .unwrap_or(0);
    let max_y = screens
        .iter()
        .map(|s| s.display_info.y + s.display_info.height as i32)
        .max()
        .unwrap_or(0);

    let total_width = (max_x - min_x) as u32;
    let total_height = (max_y - min_y) as u32;

    // Scale factor of the primary screen drives the logical→physical math at
    // the confirm boundary (frontend sends logical pixels; we crop physical).
    let scale_factor = screens
        .iter()
        .find(|s| s.display_info.is_primary)
        .or_else(|| screens.first())
        .map(|s| s.display_info.scale_factor as f64)
        .unwrap_or(1.0);

    Ok(VirtualScreenInfo {
        offset_x: min_x,
        offset_y: min_y,
        total_width,
        total_height,
        scale_factor,
    })
}

#[cfg(not(target_os = "windows"))]
pub fn get_virtual_screen_info() -> Result<VirtualScreenInfo, String> {
    Err("Native region capture is only supported on Windows".to_string())
}

/// Captures every screen into one RGBA canvas covering the virtual screen.
#[cfg(target_os = "windows")]
fn capture_virtual_screen_rgba(
    virtual_info: &VirtualScreenInfo,
) -> Result<screenshots::image::RgbaImage, String> {
    use screenshots::image;
    use screenshots::Screen;

    let screens = Screen::all().map_err(|e| format!("Failed to enumerate screens: {e}"))?;
    if screens.is_empty() {
        return Err("No screens found".to_string());
    }

    let mut canvas = image::RgbaImage::new(virtual_info.total_width, virtual_info.total_height);
    let canvas_width = canvas.width() as usize;
    let canvas_height = canvas.height() as usize;
    let canvas_row_bytes = canvas_width * 4;
    let canvas_buf = canvas.as_flat_samples_mut().samples;

    for screen in screens {
        let img = screen
            .capture()
            .map_err(|e| format!("Failed to capture screen: {e}"))?;

        let offset_x = screen.display_info.x - virtual_info.offset_x;
        let offset_y = screen.display_info.y - virtual_info.offset_y;

        if offset_x < 0 || offset_y < 0 {
            continue;
        }
        let offset_x = offset_x as usize;
        let offset_y = offset_y as usize;
        if offset_x >= canvas_width || offset_y >= canvas_height {
            continue;
        }

        let img_width = img.width() as usize;
        let img_height = img.height() as usize;
        let img_row_bytes = img_width * 4;

        let copy_width = img_width.min(canvas_width.saturating_sub(offset_x));
        let copy_height = img_height.min(canvas_height.saturating_sub(offset_y));
        let copy_row_bytes = copy_width * 4;

        let img_buf = img.as_flat_samples().samples;

        for row in 0..copy_height {
            let src_start = row * img_row_bytes;
            let dst_start = (offset_y + row) * canvas_row_bytes + offset_x * 4;
            canvas_buf[dst_start..dst_start + copy_row_bytes]
                .copy_from_slice(&img_buf[src_start..src_start + copy_row_bytes]);
        }
    }

    Ok(canvas)
}

/// Crops a region out of an RGBA canvas and encodes it as PNG.
#[cfg(target_os = "windows")]
fn crop_region_to_png(
    canvas: &screenshots::image::RgbaImage,
    region: &SelectedRegion,
) -> Result<Vec<u8>, String> {
    use screenshots::image::{self, ImageEncoder};

    if region.x < 0 || region.y < 0 {
        return Err("Invalid region: negative coordinates".to_string());
    }
    let x = region.x as u32;
    let y = region.y as u32;
    if x + region.width > canvas.width() || y + region.height > canvas.height() {
        return Err(format!(
            "Region out of bounds: ({}, {}) + {}x{} exceeds {}x{}",
            x,
            y,
            region.width,
            region.height,
            canvas.width(),
            canvas.height()
        ));
    }

    let cropped = image::imageops::crop_imm(canvas, x, y, region.width, region.height).to_image();

    let mut png_bytes: Vec<u8> = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut png_bytes);
    encoder
        .write_image(
            cropped.as_raw(),
            region.width,
            region.height,
            image::ColorType::Rgba8,
        )
        .map_err(|e| format!("Failed to encode cropped PNG: {e}"))?;

    Ok(png_bytes)
}

/// Opens the region capture overlay and resolves when the user confirms or
/// cancels. Never returns until one of those happens.
pub async fn open_region_picker(app: &AppHandle) -> RegionCaptureResult {
    #[cfg(not(target_os = "windows"))]
    {
        let _ = app;
        return RegionCaptureResult::Error(
            "Native region capture is only supported on Windows".to_string(),
        );
    }

    #[cfg(target_os = "windows")]
    {
        open_region_picker_windows(app).await
    }
}

#[cfg(target_os = "windows")]
async fn open_region_picker_windows(app: &AppHandle) -> RegionCaptureResult {
    use tauri::WebviewWindowBuilder;

    // Close any existing picker window first (cancels its pending operation).
    if let Some(existing_window) = app.get_webview_window(REGION_CAPTURE_LABEL) {
        let _ = existing_window.destroy();
        for _ in 0..50 {
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            if app.get_webview_window(REGION_CAPTURE_LABEL).is_none() {
                break;
            }
        }
    }

    let virtual_info = match get_virtual_screen_info() {
        Ok(info) => info,
        Err(e) => return RegionCaptureResult::Error(e),
    };

    let (tx, rx) = oneshot::channel::<RegionCaptureResult>();
    {
        let state = app.state::<ManagedRegionCaptureState>();
        let mut guard = state.lock().unwrap();
        guard.result_sender = Some(tx);
        guard.virtual_info = Some(virtual_info.clone());
    }

    // The overlay window is sized in logical pixels: physical / scale.
    let scale = virtual_info.scale_factor.max(1e-3);
    let x = f64::from(virtual_info.offset_x) / scale;
    let y = f64::from(virtual_info.offset_y) / scale;
    let width = f64::from(virtual_info.total_width) / scale;
    let height = f64::from(virtual_info.total_height) / scale;

    let mut builder = WebviewWindowBuilder::new(
        app,
        REGION_CAPTURE_LABEL,
        tauri::WebviewUrl::App("src/region-capture/index.html".into()),
    )
    .title("Region Capture")
    .position(x, y)
    .inner_size(width, height)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(false)
    .focused(true)
    .visible(false);

    #[cfg(target_os = "windows")]
    if let Ok(runtime) = crate::webview_runtime::config(app) {
        builder = builder.data_directory(runtime.data_directory);
        if let Some(browser_args) = runtime.additional_browser_args {
            builder = builder.additional_browser_args(&browser_args);
        }
    }

    let window = match builder.build() {
        Ok(window) => window,
        Err(e) => {
            log::error!("Failed to create region capture window: {e}");
            let _ = take_pending_region_capture(app);
            return RegionCaptureResult::Error(format!("Failed to create overlay: {e}"));
        }
    };
    crate::webview_hardening::disable_browser_accelerator_keys(&window);

    debug!(
        "Region capture overlay window created ({}x{} @ scale {})",
        virtual_info.total_width, virtual_info.total_height, scale
    );

    let _ = window.show();
    let _ = window.set_focus();

    match rx.await {
        Ok(result) => result,
        Err(_) => {
            RegionCaptureResult::Error("Region capture channel closed unexpectedly".to_string())
        }
    }
}

/// Window label for the region capture overlay.
pub const REGION_CAPTURE_LABEL: &str = "region_capture";

/// Called from the overlay when the user confirms a region (physical pixels).
pub fn on_region_selected(app: &AppHandle, region: SelectedRegion) {
    // Claim the pending result BEFORE destroying the window so the Destroyed
    // event's cancellation path can't race this confirmation.
    let (sender, virtual_info) = take_pending_region_capture(app);

    // Hide/destroy the overlay immediately so it never appears in the capture.
    if let Some(window) = app.get_webview_window(REGION_CAPTURE_LABEL) {
        let _ = window.hide();
        let _ = window.destroy();
    }

    let Some(sender) = sender else {
        log::warn!("Region confirmed with no pending capture operation");
        return;
    };
    let Some(virtual_info) = virtual_info else {
        let _ = sender.send(RegionCaptureResult::Error(
            "Virtual screen info missing".to_string(),
        ));
        return;
    };

    std::thread::spawn(move || {
        // Give the window manager a moment to apply the hide before capturing
        // so the overlay itself never ends up in the screenshot.
        std::thread::sleep(std::time::Duration::from_millis(50));

        let result = (|| {
            #[cfg(target_os = "windows")]
            {
                let canvas = capture_virtual_screen_rgba(&virtual_info)?;
                crop_region_to_png(&canvas, &region)
            }
            #[cfg(not(target_os = "windows"))]
            {
                let _ = (&virtual_info, &region);
                Err("Native region capture is only supported on Windows".to_string())
            }
        })();

        match result {
            Ok(image_data) => {
                let _ = sender.send(RegionCaptureResult::Selected { region, image_data });
            }
            Err(e) => {
                let _ = sender.send(RegionCaptureResult::Error(e));
            }
        }
    });
}

/// Called from the overlay when the user cancels (Escape).
pub fn on_region_cancelled(app: &AppHandle) {
    cancel_pending_region_capture(app);
    if let Some(window) = app.get_webview_window(REGION_CAPTURE_LABEL) {
        let _ = window.destroy();
    }
}

/// Called when the picker window is closed or destroyed externally.
pub fn on_region_window_closed(app: &AppHandle) {
    cancel_pending_region_capture(app);
}
