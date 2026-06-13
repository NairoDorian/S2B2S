//! Per-OS capability probing for the overlay FX system.
//!
//! Used by the frontend Settings → Overlay Window tab to grey out
//! options unsupported on the current machine.

use super::OverlayCapabilities;

pub fn get_overlay_capabilities() -> OverlayCapabilities {
    OverlayCapabilities::probe()
}
