//! Bubble / avatar placement math.
//!
//! Compute the screen position for the brain overlay bubble relative to the cursor,
//! with quadrant-aware flipping so the bubble never goes off-screen.

use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewWindow};

use crate::input;

/// Position anchor for the bubble (top-left corner of the overlay).
pub struct BubbleAnchor {
    pub x: f64,
    pub y: f64,
}

/// Compute where the brain overlay should be placed relative to the cursor.
/// Places the bubble offset from the cursor, flipping quadrants as needed.
pub fn compute_bubble_anchor(
    app: &AppHandle,
    _overlay_window: &WebviewWindow,
) -> Option<BubbleAnchor> {
    let (cursor_x, cursor_y) = input::get_cursor_position(app)?;

    // Get the monitor under the cursor
    let monitor = get_cursor_monitor(app, cursor_x, cursor_y)?;
    let scale = monitor.scale_factor();
    let mon_x = monitor.position().x as f64 / scale;
    let mon_y = monitor.position().y as f64 / scale;
    let mon_w = monitor.size().width as f64 / scale;
    let mon_h = monitor.size().height as f64 / scale;

    let cursor_logical_x = cursor_x as f64 / (if scale > 0.0 { scale } else { 1.0 });
    let cursor_logical_y = cursor_y as f64 / (if scale > 0.0 { scale } else { 1.0 });

    // Bubble dimensions (default: ~360×280 logical px, avatar ~72 px beside it)
    let bubble_w = 360.0;
    let bubble_h = 280.0;
    let margin = 12.0;
    let cursor_offset = 20.0;

    // Quadrant logic:
    // - Default: bubble to the right and below the cursor (for readability)
    // - Flip left if too close to right edge
    // - Flip up if too close to bottom edge

    let right_overflow = cursor_logical_x + bubble_w + margin > mon_x + mon_w;
    let bottom_overflow = cursor_logical_y + bubble_h + margin > mon_y + mon_h;

    let x = if right_overflow {
        (cursor_logical_x - bubble_w - cursor_offset).max(mon_x + margin)
    } else {
        cursor_logical_x + cursor_offset
    };

    let y = if bottom_overflow {
        (cursor_logical_y - bubble_h - cursor_offset).max(mon_y + margin)
    } else {
        cursor_logical_y + cursor_offset
    };

    Some(BubbleAnchor { x, y })
}

/// Find the monitor whose rectangle contains the given cursor position.
fn get_cursor_monitor(
    app: &AppHandle,
    cursor_x: i32,
    cursor_y: i32,
) -> Option<tauri::Monitor> {
    if let Ok(monitors) = app.available_monitors() {
        for monitor in monitors {
            let scale = monitor.scale_factor();
            let pos = PhysicalPosition::new(
                (monitor.position().x as f64 / scale) as i32,
                (monitor.position().y as f64 / scale) as i32,
            );
            let size = PhysicalSize::new(
                (monitor.size().width as f64 / scale) as u32,
                (monitor.size().height as f64 / scale) as u32,
            );

            if cursor_x >= pos.x
                && cursor_x < (pos.x + size.width as i32)
                && cursor_y >= pos.y
                && cursor_y < (pos.y + size.height as i32)
            {
                return Some(monitor);
            }
        }
    }
    app.primary_monitor().ok().flatten()
}
