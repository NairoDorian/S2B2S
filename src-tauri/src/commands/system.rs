//! System meter commands (the live stream is `SystemStatsEvent`).

use crate::system_monitor::{self, SystemStatsEvent};

/// The most recent sample, so a footer that mounts between ticks does not
/// show empty meters for up to a second.
#[tauri::command]
#[specta::specta]
pub fn get_system_stats() -> Option<SystemStatsEvent> {
    system_monitor::latest()
}
