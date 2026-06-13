//! Cursor-follow service for the brain overlay window.
//!
//! Polls the global cursor position at ~30 Hz and repositions the overlay window. Handles
//! quadrant flipping (so the bubble doesn't go off-screen), DPI scaling, and the freeze-on-speak
//! rule (the bubble stays put while the avatar is speaking so the user can read).

use crate::overlay_fx::placement;
use tauri::{AppHandle, LogicalPosition, Manager};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Spawn a background thread that repositions the brain overlay to follow the cursor.
/// Returns a shared flag — set to `true` to stop the follow loop.
pub fn start_cursor_follow(app: AppHandle) -> Arc<AtomicBool> {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_clone = stop.clone();

    std::thread::spawn(move || {
        let label = super::window::BRAIN_OVERLAY_LABEL;
        loop {
            if stop_clone.load(Ordering::Relaxed) {
                break;
            }

            if let Some(window) = app.get_webview_window(label) {
                if let Ok(true) = window.is_visible() {
                    if let Some(anchor) = placement::compute_bubble_anchor(&app, &window) {
                        let _ = window.set_position(tauri::Position::Logical(
                            LogicalPosition::new(anchor.x, anchor.y),
                        ));
                    }
                }
            }

            std::thread::sleep(std::time::Duration::from_millis(33)); // ~30 Hz
        }
    });

    stop
}
