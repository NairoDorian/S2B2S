//! Brain overlay window — the transparent, click-through, always-on-top webview
//! that hosts the 3D avatar + reply bubble (Track A).

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// Window label for the brain/conversation overlay.
pub const BRAIN_OVERLAY_LABEL: &str = "brain_overlay";

/// Creates the brain overlay window (hidden) at app startup.
/// The window is reused across the app's lifetime — shown/hidden on converse trigger.
pub fn create_brain_overlay(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = WebviewWindowBuilder::new(
        app,
        BRAIN_OVERLAY_LABEL,
        WebviewUrl::App("src/brain-overlay/index.html".into()),
    )
    .title("S2B2S Brain Overlay")
    .resizable(false)
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .transparent(true)
    .focused(false)
    .visible(false);

    #[cfg(target_os = "macos")]
    {
        builder = builder.shadow(false);
    }

    #[cfg(target_os = "windows")]
    if let Ok(runtime) = crate::webview_runtime::config(app) {
        builder = builder.data_directory(runtime.data_directory);
        if let Some(browser_args) = runtime.additional_browser_args {
            builder = builder.additional_browser_args(&browser_args);
        }
    }

    #[cfg(not(target_os = "windows"))]
    if let Some(data_dir) = crate::portable::data_dir() {
        builder = builder.data_directory(data_dir.join("webview"));
    }

    let window = builder.build()?;
    crate::webview_hardening::disable_browser_accelerator_keys(&window);

    log::debug!("Brain overlay window created (hidden)");
    Ok(())
}

/// Show the brain overlay and begin the conversation flow.
pub fn show_brain_overlay(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(BRAIN_OVERLAY_LABEL) {
        let _ = window.show();
        #[cfg(target_os = "windows")]
        {
            use windows::Win32::Foundation::HWND;
            use windows::Win32::UI::WindowsAndMessaging::{
                SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
            };
            if let Ok(raw_hwnd) = window.hwnd() {
                let hwnd = HWND(raw_hwnd.0);
                unsafe {
                    let _ = SetWindowPos(
                        hwnd,
                        Some(HWND_TOPMOST),
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                    );
                }
            }
        }
    }
}

/// Hide the brain overlay.
pub fn hide_brain_overlay(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(BRAIN_OVERLAY_LABEL) {
        let _ = window.hide();
    }
}

/// Check whether the brain overlay window exists.
pub fn has_brain_overlay(app: &AppHandle) -> bool {
    app.get_webview_window(BRAIN_OVERLAY_LABEL).is_some()
}
