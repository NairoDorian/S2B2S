//! Tauri commands of the "Live FFT" page.

use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager, State};

use crate::live_fft::{LiveFftManager, LiveFftStatus};
use crate::settings::{LiveFftSettings, get_settings, write_settings};

/// Persist the page's parameters and hand them to the running analysis,
/// which applies them on its next frame. A threading-mode change restarts
/// the session, which can reopen the microphone, so the hand-off runs on a
/// blocking thread.
#[tauri::command]
#[specta::specta]
pub async fn change_live_fft_settings(
    app: AppHandle,
    settings: LiveFftSettings,
) -> Result<(), String> {
    let settings = settings.normalized();
    let mut current = get_settings(&app);
    current.live_fft = settings.clone();
    write_settings(&app, current);
    let manager = app.state::<Arc<LiveFftManager>>().inner().clone();
    let settings_for_manager = settings.clone();
    tauri::async_runtime::spawn_blocking(move || manager.update_settings(settings_for_manager))
        .await
        .map_err(|e| format!("live fft task join failed: {e}"))?;
    // The overlay's miniature analyser polls the same FFT manager — nudge a
    // running overlay preview so it picks up the new analysis parameters
    // (scale, warp, EQ, weighting, dB, ballistics) on its next frame.
    let _ = app.emit("live-fft-settings", settings);
    Ok(())
}

/// Start the analyser: opens the microphone (a device open can block, so
/// this stays off the webview thread) and streams `LiveFftFrameEvent`s.
#[tauri::command]
#[specta::specta]
pub async fn live_fft_start(app: AppHandle) -> Result<(), String> {
    let manager = app.state::<Arc<LiveFftManager>>().inner().clone();
    tauri::async_runtime::spawn_blocking(move || manager.start())
        .await
        .map_err(|e| format!("live fft task join failed: {e}"))?
}

#[tauri::command]
#[specta::specta]
pub fn live_fft_stop(manager: State<'_, Arc<LiveFftManager>>) -> Result<(), String> {
    manager.stop()
}

#[tauri::command]
#[specta::specta]
pub fn live_fft_status(manager: State<'_, Arc<LiveFftManager>>) -> LiveFftStatus {
    manager.status()
}

/// The page's Reset button: clears ballistics, AGC and EQ state.
#[tauri::command]
#[specta::specta]
pub fn live_fft_reset(manager: State<'_, Arc<LiveFftManager>>) {
    manager.reset();
}

/// The "Raw" preset: linear magnitude, frame-peak reference, no ballistics.
#[tauri::command]
#[specta::specta]
pub fn live_fft_raw_defaults() -> LiveFftSettings {
    LiveFftSettings::raw_defaults()
}

/// The latest frame of the recording overlay's miniature analyser as raw
/// little-endian bytes (`live_fft::scope::encode_scope_frame`). The overlay
/// polls it at the analyser's update rate; each call copies the ~20 KB
/// frame and never waits. Returns `tauri::ipc::Response`, which tauri-specta
/// cannot type, so it is registered beside the typed commands in lib.rs and
/// called with `invoke<ArrayBuffer>` rather than through `bindings.ts`.
#[tauri::command]
pub async fn overlay_scope_frame(app: AppHandle) -> tauri::ipc::Response {
    let manager = app.state::<Arc<LiveFftManager>>();
    tauri::ipc::Response::new(manager.scope_frame())
}
