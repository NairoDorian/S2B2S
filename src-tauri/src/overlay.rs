use crate::audio_toolkit::SpeechActivity;
use crate::input;
use crate::settings;
use crate::settings::{OverlayPosition, OverlayScopeSettings, OverlayStyle};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize};
use tauri_specta::Event;

#[cfg(not(target_os = "macos"))]
use log::debug;

#[cfg(not(target_os = "macos"))]
use tauri::WebviewWindowBuilder;

#[cfg(target_os = "macos")]
use tauri::WebviewUrl;

#[cfg(target_os = "macos")]
use tauri_nspanel::{CollectionBehavior, PanelBuilder, PanelLevel, StyleMask, tauri_panel};

#[cfg(target_os = "linux")]
use crate::utils;

#[cfg(target_os = "linux")]
use gtk_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

#[cfg(target_os = "macos")]
tauri_panel! {
    panel!(RecordingOverlayPanel {
        config: {
            can_become_key_window: false,
            is_floating_panel: true
        }
    })
}

// Native overlay window sizes (logical points). One window is reused for every
// state and resized in `show_overlay_state`; each size need only be at least as
// large as the card it hosts (the `--ov-*` vars in RecordingOverlay.css). The
// card is CSS-anchored flush to the screen edge, so window height doesn't move
// where the card sits — only OVERLAY_TOP_OFFSET / OVERLAY_BOTTOM_OFFSET do. Keep
// these in sync with the CSS card geometry.
//
// On Windows these sizes are additionally multiplied by the accessibility text
// scale (see windows_text_scale_factor), which WebView2 applies as a zoom.
//
// Compact overlay (Minimal / transcribing / processing): the 40h pill animates
// width between the resting width (the scope views, see `overlay_scope`) and
// 216 (--ov-work-w) and expands from center, so the window must fit the
// widest state plus a little slack. These are the sizes for the default
// scope geometry; `overlay_dimensions` derives the live ones from the cached
// setting with the same arithmetic as `src/lib/overlayScope.ts`.
const OVERLAY_WIDTH: f64 = 280.0;
const OVERLAY_HEIGHT: f64 = 50.0;
/// The resting pill without its scope block (172 px around the old level
/// bars minus those 46 px), and the same for the pill carrying speech stats.
const OVERLAY_REST_BASE_W: f64 = 126.0;
const OVERLAY_STATS_BASE_W: f64 = 198.0;
/// Window width beyond the pill, and height beyond the control row.
const OVERLAY_WINDOW_SLACK_W: f64 = 44.0;
const OVERLAY_WINDOW_SLACK_H: f64 = 10.0;
/// The control row is 40 px unless the scope views need more (view + 18).
const OVERLAY_ROW_H: f64 = 40.0;
const OVERLAY_ROW_PADDING_H: f64 = 18.0;

// Speech stats (timer + words-per-minute) ride in the pill's right-hand cluster,
// which grows the resting pill to 308 (--ov-stats-w) — past --ov-work-w, so the
// window has to fit that instead. Only the compact overlay needs a bigger
// window: the Live panel already has room for the stats inside its 392px card.
// Documented default; the live width is derived in `compact_dimensions`.
#[cfg_attr(not(test), allow(dead_code))]
const OVERLAY_STATS_WIDTH: f64 = 352.0;

// Actual is 394x118, just a little extra
const OVERLAY_STREAM_WIDTH: f64 = 400.0;
const OVERLAY_STREAM_HEIGHT: f64 = 120.0;

/// Compact pill window size for a scope block `block` px wide and views
/// `view_h` px tall, with or without the speech-stats cluster.
fn compact_dimensions(block: u32, view_h: u32, stats: bool) -> (f64, f64) {
    let base = if stats {
        OVERLAY_STATS_BASE_W
    } else {
        OVERLAY_REST_BASE_W
    };
    let row_h = (f64::from(view_h) + OVERLAY_ROW_PADDING_H).max(OVERLAY_ROW_H);
    (
        base + f64::from(block) + OVERLAY_WINDOW_SLACK_W,
        row_h + OVERLAY_WINDOW_SLACK_H,
    )
}

/// Overlay window size (logical) for a given UI state.
fn overlay_dimensions(state: &str) -> (f64, f64) {
    // Read the cached values rather than the store: this runs on the main
    // thread inside the show path, where a settings read is pure added latency.
    let view_h = OVERLAY_SCOPE_VIEW_H.load(Ordering::Relaxed);
    if state == "streaming" {
        let (width, base_height) = streaming_dimensions_baseline();
        return (width, base_height + streaming_text_height());
    }
    compact_dimensions(
        OVERLAY_SCOPE_BLOCK_PX.load(Ordering::Relaxed),
        view_h,
        SPEECH_STATS_ENABLED.load(Ordering::Relaxed),
    )
}

static OVERLAY_SHOW_GENERATION: AtomicU64 = AtomicU64::new(0);

#[cfg(target_os = "macos")]
const OVERLAY_TOP_OFFSET: f64 = 46.0;
#[cfg(any(target_os = "windows", target_os = "linux"))]
const OVERLAY_TOP_OFFSET: f64 = 4.0;

#[cfg(target_os = "macos")]
const OVERLAY_BOTTOM_OFFSET: f64 = 15.0;

#[cfg(any(target_os = "windows", target_os = "linux"))]
const OVERLAY_BOTTOM_OFFSET: f64 = 40.0;

/// Configures the edge and offset of a GTK layer surface. gtk-layer-shell
/// commits anchor and margin changes itself, including while the surface is
/// mapped, so changing position does not require a manual hide/show cycle.
#[cfg(target_os = "linux")]
fn configure_layer_shell_position(gtk_window: &gtk::ApplicationWindow, position: OverlayPosition) {
    let (edge, opposite_edge, margin) = match position {
        OverlayPosition::Top => (Edge::Top, Edge::Bottom, OVERLAY_TOP_OFFSET),
        OverlayPosition::Bottom => (Edge::Bottom, Edge::Top, OVERLAY_BOTTOM_OFFSET),
    };

    gtk_window.set_anchor(edge, true);
    gtk_window.set_anchor(opposite_edge, false);
    gtk_window.set_layer_shell_margin(edge, margin.round() as i32);
    gtk_window.set_layer_shell_margin(opposite_edge, 0);
}

/// Configures a GTK layer surface before it is shown.
///
/// Tauri's normal `set_size` path calls `gtk_window_resize`, but layer surfaces
/// derive their dimensions from GTK's size request. gtk-layer-shell documents
/// the `set_size_request` + `resize(1, 1)` sequence for forcing a new size.
#[cfg(target_os = "linux")]
fn configure_layer_shell_surface(
    gtk_window: &gtk::ApplicationWindow,
    position: OverlayPosition,
    width: f64,
    height: f64,
) {
    use gtk::prelude::{GtkWindowExt, WidgetExt};

    configure_layer_shell_position(gtk_window, position);

    gtk_window.set_size_request(
        width.round().max(1.0) as i32,
        height.round().max(1.0) as i32,
    );
    gtk_window.resize(1, 1);
}

/// Initializes GTK layer shell for Linux overlay window
/// Returns true if layer shell was successfully initialized, false otherwise
#[cfg(target_os = "linux")]
fn init_gtk_layer_shell(overlay_window: &tauri::webview::WebviewWindow) -> bool {
    if utils::app_env_flag("NO_GTK_LAYER_SHELL") {
        debug!(
            "Skipping GTK layer shell init ({}NO_GTK_LAYER_SHELL is enabled)",
            app_identity::ENV_PREFIX
        );
        return false;
    }

    if !gtk_layer_shell::is_supported() {
        return false;
    }

    // Try to get the GTK window from the Tauri webview
    if let Ok(gtk_window) = overlay_window.gtk_window() {
        gtk_window.init_layer_shell();
        gtk_window.set_layer(Layer::Overlay);
        gtk_window.set_keyboard_mode(KeyboardMode::None);
        gtk_window.set_exclusive_zone(0);

        let overlay_position = settings::get_settings(overlay_window.app_handle()).overlay_position;
        configure_layer_shell_surface(&gtk_window, overlay_position, OVERLAY_WIDTH, OVERLAY_HEIGHT);

        let initialized = gtk_window.is_layer_window();
        LAYER_SHELL_ACTIVE.store(initialized, Ordering::SeqCst);
        return initialized;
    }
    false
}

/// Forces a window to be topmost using Win32 API (Windows only)
/// This is more reliable than Tauri's set_always_on_top which can be overridden
#[cfg(target_os = "windows")]
fn force_overlay_topmost(overlay_window: &tauri::webview::WebviewWindow) {
    use windows::Win32::UI::WindowsAndMessaging::{
        HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SetWindowPos,
    };

    // Clone because run_on_main_thread takes 'static
    let overlay_clone = overlay_window.clone();

    // Make sure the Win32 call happens on the UI thread
    let _ = overlay_clone.clone().run_on_main_thread(move || {
        if let Ok(hwnd) = overlay_clone.hwnd() {
            unsafe {
                // Force Z-order: make this window topmost without changing size/pos or stealing focus
                // hwnd comes from tao (windows 0.61.3), cast to our windows 0.62.2 HWND
                let hwnd: windows::Win32::Foundation::HWND = std::mem::transmute_copy(&hwnd);
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
    });
}

fn get_monitor_with_cursor(app_handle: &AppHandle) -> Option<tauri::Monitor> {
    if let Some(mouse_location) = input::get_cursor_position(app_handle) {
        if let Ok(monitors) = app_handle.available_monitors() {
            for monitor in monitors {
                // On Windows both the cursor (enigo -> GetCursorPos) and the
                // monitor bounds are physical pixels, so compare them directly.
                #[cfg(target_os = "windows")]
                if is_mouse_within_monitor(mouse_location, monitor.position(), monitor.size()) {
                    return Some(monitor);
                }

                // macOS/Linux: enigo returns logical coords, so scale the bounds down.
                #[cfg(not(target_os = "windows"))]
                {
                    let scale = monitor.scale_factor();
                    let pos = PhysicalPosition::new(
                        (monitor.position().x as f64 / scale) as i32,
                        (monitor.position().y as f64 / scale) as i32,
                    );
                    let size = PhysicalSize::new(
                        (monitor.size().width as f64 / scale) as u32,
                        (monitor.size().height as f64 / scale) as u32,
                    );
                    if is_mouse_within_monitor(mouse_location, &pos, &size) {
                        return Some(monitor);
                    }
                }
            }
        }
    }

    app_handle.primary_monitor().ok().flatten()
}

fn is_mouse_within_monitor(
    mouse_pos: (i32, i32),
    monitor_pos: &PhysicalPosition<i32>,
    monitor_size: &PhysicalSize<u32>,
) -> bool {
    let (mouse_x, mouse_y) = mouse_pos;
    let PhysicalPosition {
        x: monitor_x,
        y: monitor_y,
    } = *monitor_pos;
    let PhysicalSize {
        width: monitor_width,
        height: monitor_height,
    } = *monitor_size;

    mouse_x >= monitor_x
        && mouse_x < (monitor_x + monitor_width as i32)
        && mouse_y >= monitor_y
        && mouse_y < (monitor_y + monitor_height as i32)
}

/// Returns overlay position in logical coordinates (points on macOS).
///
/// The Bottom anchor uses the macOS work area (visibleFrame) so the overlay
/// tracks the Dock — above it when shown, at the screen edge when hidden.
/// This relies on tauri 2.11's work_area.position.y fix (#14655), the same
/// bug that led PR #969 to abandon work_area for full monitor bounds. Top and
/// the other platforms keep full monitor bounds plus the fixed offsets
/// (work_area is unreliable on Wayland; Windows' offset clears the taskbar).
///
/// We must use LogicalPosition (not PhysicalPosition) because Tauri/tao
/// converts PhysicalPosition using the scale factor of the monitor the window
/// is *currently* on, which is wrong when moving cross-monitor. Windows uses
/// `place_windows_overlay` instead (no single logical space across mixed DPI).
fn calculate_overlay_position(
    app_handle: &AppHandle,
    width: f64,
    height: f64,
) -> Option<(f64, f64)> {
    let monitor = get_monitor_with_cursor(app_handle)?;
    let scale = monitor.scale_factor();
    let monitor_x = monitor.position().x as f64 / scale;
    let monitor_y = monitor.position().y as f64 / scale;
    let monitor_width = monitor.size().width as f64 / scale;

    let settings = settings::get_settings(app_handle);

    let x = monitor_x + (monitor_width - width) / 2.0;
    let y = match settings.overlay_position {
        OverlayPosition::Top => monitor_y + OVERLAY_TOP_OFFSET,
        OverlayPosition::Bottom => {
            // work_area.position shares monitor.position's global coordinate
            // space, so no monitor offset is added.
            #[cfg(target_os = "macos")]
            let bottom = {
                let wa = monitor.work_area();
                (wa.position.y as f64 + wa.size.height as f64) / scale
            };
            #[cfg(not(target_os = "macos"))]
            let bottom = monitor_y + monitor.size().height as f64 / scale;

            bottom - height - OVERLAY_BOTTOM_OFFSET
        }
    };

    Some((x, y))
}

/// Current overlay window size in logical units (points), for repositioning
/// without assuming a fixed size (compact vs. streaming).
#[cfg(not(target_os = "windows"))]
fn current_overlay_logical_size(window: &tauri::webview::WebviewWindow) -> Option<(f64, f64)> {
    let size = window.inner_size().ok()?;
    let scale = window.scale_factor().ok()?;
    Some((size.width as f64 / scale, size.height as f64 / scale))
}

#[cfg(target_os = "windows")]
static WINDOWS_OVERLAY_IS_STREAMING: AtomicBool = AtomicBool::new(false);

/// Windows accessibility text size (Settings > Accessibility > Text size), a
/// separate axis from display scaling that WebView2 applies as a document zoom.
#[cfg(target_os = "windows")]
fn windows_text_scale_factor() -> f64 {
    // Absent until the user moves the slider off 100%; stored as a percentage.
    winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
        .open_subkey(r"Software\Microsoft\Accessibility")
        .and_then(|key| key.get_value::<u32, _>("TextScaleFactor"))
        .map(|percent| (percent as f64 / 100.0).clamp(1.0, 2.25))
        .unwrap_or(1.0)
}

/// Overlay rectangle in the destination monitor's physical pixels, so nothing
/// is converted through the window's previous-monitor DPI.
#[cfg(target_os = "windows")]
fn windows_overlay_bounds(
    monitor_position: PhysicalPosition<i32>,
    monitor_size: PhysicalSize<u32>,
    scale: f64,
    text_scale: f64,
    logical_width: f64,
    logical_height: f64,
    overlay_position: OverlayPosition,
) -> (i32, i32, i32, i32) {
    // Grow the window with the text scale; offsets stay DPI-only since the
    // card sits flush against the window's screen-edge side.
    let content_scale = scale * text_scale;
    let width = (logical_width * content_scale).round().max(1.0) as i32;
    let height = (logical_height * content_scale).round().max(1.0) as i32;
    let x = (monitor_position.x as f64 + (monitor_size.width as f64 - width as f64) / 2.0).round()
        as i32;
    let y = match overlay_position {
        OverlayPosition::Top => {
            (monitor_position.y as f64 + OVERLAY_TOP_OFFSET * scale).round() as i32
        }
        OverlayPosition::Bottom => (monitor_position.y as f64 + monitor_size.height as f64
            - height as f64
            - OVERLAY_BOTTOM_OFFSET * scale)
            .round() as i32,
    };

    (x, y, width, height)
}

/// Moves and sizes the overlay in one native SetWindowPos, bypassing tao's
/// current-DPI logical conversion that mislands cross-monitor moves.
#[cfg(target_os = "windows")]
fn place_windows_overlay(
    app_handle: &AppHandle,
    overlay_window: &tauri::webview::WebviewWindow,
    logical_width: f64,
    logical_height: f64,
) -> Result<(), String> {
    use windows::Win32::UI::WindowsAndMessaging::{SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos};

    let monitor = get_monitor_with_cursor(app_handle)
        .ok_or_else(|| "failed to determine the monitor containing the cursor".to_string())?;
    let text_scale = windows_text_scale_factor();
    let (x, y, width, height) = windows_overlay_bounds(
        *monitor.position(),
        *monitor.size(),
        monitor.scale_factor(),
        text_scale,
        logical_width,
        logical_height,
        settings::get_settings(app_handle).overlay_position,
    );
    let hwnd = overlay_window
        .hwnd()
        .map_err(|error| format!("failed to get overlay window handle: {error}"))?;

    unsafe {
        // hwnd comes from tao (windows 0.61.3), cast to our windows 0.62.2 HWND
        let hwnd: windows::Win32::Foundation::HWND = std::mem::transmute_copy(&hwnd);
        SetWindowPos(
            hwnd,
            None,
            x,
            y,
            width,
            height,
            SWP_NOACTIVATE | SWP_NOZORDER,
        )
        .map_err(|error| format!("failed to set overlay bounds: {error}"))?;
    }

    log::debug!(
        "windows overlay bounds: x={} y={} width={} height={} scale={} text_scale={}",
        x,
        y,
        width,
        height,
        monitor.scale_factor(),
        text_scale
    );
    Ok(())
}

/// Creates the recording overlay window and keeps it hidden by default
#[cfg(not(target_os = "macos"))]
pub fn create_recording_overlay(app_handle: &AppHandle) {
    // On Linux (Wayland), monitor detection often fails, but we don't need exact coordinates
    // for Layer Shell as we use anchors. On other platforms, we require a monitor.
    #[cfg(not(target_os = "linux"))]
    {
        let position = calculate_overlay_position(app_handle, OVERLAY_WIDTH, OVERLAY_HEIGHT);
        if position.is_none() {
            debug!("Failed to determine overlay position, not creating overlay window");
            return;
        }
    }

    // Position starts unset — update_overlay_position() sets the correct
    // LogicalPosition before the overlay is shown.
    let mut builder = WebviewWindowBuilder::new(
        app_handle,
        "recording_overlay",
        tauri::WebviewUrl::App("src/overlay/index.html".into()),
    )
    .title("Recording")
    .resizable(false)
    .inner_size(OVERLAY_WIDTH, OVERLAY_HEIGHT)
    .shadow(false)
    .maximizable(false)
    .minimizable(false)
    .closable(false)
    .accept_first_mouse(true)
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .transparent(true)
    .focusable(false)
    .focused(false)
    .visible(false);

    if let Some(data_dir) = crate::portable::data_dir() {
        builder = builder.data_directory(data_dir.join("webview"));
    }

    #[allow(unused_variables)]
    match builder.build() {
        Ok(window) => {
            #[cfg(target_os = "linux")]
            {
                // Try to initialize GTK layer shell, ignore errors if compositor doesn't support it
                if init_gtk_layer_shell(&window) {
                    debug!("GTK layer shell initialized for overlay window");
                } else {
                    debug!("GTK layer shell not available, falling back to regular window");
                }
            }

            debug!("Recording overlay window created successfully (hidden)");
        }
        Err(e) => {
            debug!("Failed to create recording overlay window: {}", e);
        }
    }
}

/// Creates the recording overlay panel and keeps it hidden by default (macOS)
#[cfg(target_os = "macos")]
pub fn create_recording_overlay(app_handle: &AppHandle) {
    if let Some((x, y)) = calculate_overlay_position(app_handle, OVERLAY_WIDTH, OVERLAY_HEIGHT) {
        // PanelBuilder creates a Tauri window then converts it to NSPanel.
        // The window remains registered, so get_webview_window() still works.
        match PanelBuilder::<_, RecordingOverlayPanel>::new(app_handle, "recording_overlay")
            .url(WebviewUrl::App("src/overlay/index.html".into()))
            .title("Recording")
            .position(tauri::Position::Logical(tauri::LogicalPosition { x, y }))
            .level(PanelLevel::Status)
            .size(tauri::Size::Logical(tauri::LogicalSize {
                width: OVERLAY_WIDTH,
                height: OVERLAY_HEIGHT,
            }))
            .has_shadow(false)
            .transparent(true)
            .no_activate(true)
            .corner_radius(0.0)
            .style_mask(StyleMask::empty().borderless().nonactivating_panel())
            .with_window(|w| w.decorations(false).transparent(true).focusable(false))
            .collection_behavior(
                CollectionBehavior::new()
                    .can_join_all_spaces()
                    .full_screen_auxiliary(),
            )
            .build()
        {
            Ok(panel) => {
                panel.hide();
            }
            Err(e) => {
                log::error!("Failed to create recording overlay panel: {}", e);
            }
        }
    }
}

fn show_overlay_state(app_handle: &AppHandle, state: &str) {
    OVERLAY_SHOW_GENERATION.fetch_add(1, Ordering::SeqCst);

    // Whether the overlay shows at all is governed by overlay_style; position
    // only chooses Top vs Bottom placement. Checked here (off the main thread)
    // so the common overlay-disabled case never pays for a main-thread hop.
    let settings = settings::get_settings(app_handle);
    if settings.overlay_style == OverlayStyle::None {
        return;
    }

    // The overlay's miniature analyser (live_fft::scope) follows the overlay:
    // it starts with a recording state and stops with the working states.
    if let Some(fft) = app_handle.try_state::<Arc<crate::live_fft::LiveFftManager>>() {
        if state == "recording" || state == "streaming" {
            fft.start_overlay_scope();
        } else {
            fft.stop_overlay_scope();
        }
    }

    // The rest queries monitors and the cursor and mutates window geometry. On
    // Linux the monitor/cursor lookups hit GDK/Xlib on the process's shared X11
    // connection, which is only safe from the GTK main thread — running them on
    // a background thread corrupts the connection and hard-crashes the app
    // (issue #227). Hop to the main thread on every platform to keep the
    // geometry path uniform (a no-op cost on Windows, and it also keeps macOS's
    // NSScreen access main-thread-correct). run_on_main_thread runs the closure
    // inline when already on the main thread, so this never deadlocks.
    let handle = app_handle.clone();
    let state = state.to_string();
    let _ = app_handle.run_on_main_thread(move || show_overlay_state_on_main(&handle, &state));
}

fn show_overlay_state_on_main(app_handle: &AppHandle, state: &str) {
    // Size the overlay for this state (compact vs. streaming), then position it.
    let (width, height) = overlay_dimensions(state);
    if let Some(overlay_window) = app_handle.get_webview_window("recording_overlay") {
        // Invalidate any delayed hide still in flight from a previous session
        // (see `hide_recording_overlay`).
        OVERLAY_SHOW_GENERATION.fetch_add(1, Ordering::SeqCst);
        // A transcript measured by a previous session's card must not size this
        // one: the new card reports its own height as it fills.
        if state != "streaming" {
            OVERLAY_STREAM_TEXT_H.store(0, Ordering::Relaxed);
        }
        OVERLAY_STREAMING.store(state == "streaming", Ordering::Relaxed);

        #[cfg(target_os = "linux")]
        let shown_with_layer_shell = if LAYER_SHELL_ACTIVE.load(Ordering::SeqCst) {
            let position = settings::get_settings(app_handle).overlay_position;
            match overlay_window.gtk_window() {
                Ok(gtk_window) => {
                    configure_layer_shell_surface(&gtk_window, position, width, height)
                }
                Err(error) => log::error!("Failed to access GTK overlay window: {error}"),
            }
            let _ = overlay_window.show();
            true
        } else {
            false
        };
        #[cfg(not(target_os = "linux"))]
        let shown_with_layer_shell = false;

        if !shown_with_layer_shell {
            let size_started = std::time::Instant::now();
            #[cfg(not(target_os = "windows"))]
            let _ =
                overlay_window.set_size(tauri::Size::Logical(tauri::LogicalSize { width, height }));
            #[cfg(target_os = "windows")]
            WINDOWS_OVERLAY_IS_STREAMING.store(state == "streaming", Ordering::Relaxed);
            let size_elapsed = size_started.elapsed();

            let pos_started = std::time::Instant::now();
            #[cfg(not(target_os = "windows"))]
            let set_pos_elapsed =
                if let Some((x, y)) = calculate_overlay_position(app_handle, width, height) {
                    let set_pos_started = std::time::Instant::now();
                    let _ = overlay_window
                        .set_position(tauri::Position::Logical(tauri::LogicalPosition { x, y }));
                    set_pos_started.elapsed()
                } else {
                    std::time::Duration::ZERO
                };
            #[cfg(target_os = "windows")]
            let set_pos_elapsed = {
                let set_pos_started = std::time::Instant::now();
                if let Err(error) =
                    place_windows_overlay(app_handle, &overlay_window, width, height)
                {
                    log::error!("Failed to place recording overlay: {error}");
                }
                set_pos_started.elapsed()
            };
            let pos_calc_elapsed = pos_started.elapsed() - set_pos_elapsed;

            let show_started = std::time::Instant::now();
            let _ = overlay_window.show();
            let show_elapsed = show_started.elapsed();

            // On Windows, aggressively re-assert "topmost" in the native Z-order after showing
            #[cfg(target_os = "windows")]
            force_overlay_topmost(&overlay_window);

            // Re-assert bounds after show(): the pre-show move crosses the DPI
            // boundary, and tao's WM_DPICHANGED reflow clobbers the first placement.
            #[cfg(target_os = "windows")]
            if let Err(error) = place_windows_overlay(app_handle, &overlay_window, width, height) {
                log::error!("Failed to re-assert recording overlay position: {error}");
            }

            log::debug!(
                "overlay '{}': set_size={:?} pos_calc={:?} set_pos={:?} show={:?}",
                state,
                size_elapsed,
                pos_calc_elapsed,
                set_pos_elapsed,
                show_elapsed
            );
        }

        let _ = overlay_window.emit("show-overlay", state);
    }
}

/// Notify the visible recording overlay that the input stream has delivered its
/// first sample chunk. Audio feedback uses the same backend readiness signal,
/// but this targeted event is skipped when overlays are disabled.
pub fn emit_recording_ready(app_handle: &AppHandle) {
    if !OVERLAY_ENABLED.load(Ordering::Relaxed) {
        return;
    }

    // Showing the overlay is also queued onto the main thread. Queue readiness
    // there as well so a very fast always-on stream cannot overtake show-overlay
    // and then get reset back to the arming state by the frontend.
    let handle = app_handle.clone();
    let _ = app_handle.run_on_main_thread(move || {
        let _ = handle.emit_to("recording_overlay", "recording-ready", ());
    });
}

/// Shows the recording overlay window with fade-in animation
pub fn show_recording_overlay(app_handle: &AppHandle) {
    show_overlay_state(app_handle, "recording");
}

/// Shows the larger streaming overlay that displays live transcription text
pub fn show_streaming_overlay(app_handle: &AppHandle) {
    show_overlay_state(app_handle, "streaming");
}

/// Shows the transcribing overlay window
pub fn show_transcribing_overlay(app_handle: &AppHandle) {
    show_overlay_state(app_handle, "transcribing");
}

/// Shows the processing overlay window
pub fn show_processing_overlay(app_handle: &AppHandle) {
    show_overlay_state(app_handle, "processing");
}

/// Updates the overlay window position based on current settings
pub fn update_overlay_position(app_handle: &AppHandle) {
    // Positioning queries monitors/cursor (GDK/Xlib on Linux) and moves the
    // window, so it must run on the main thread — see show_overlay_state.
    let handle = app_handle.clone();
    let _ = app_handle.run_on_main_thread(move || update_overlay_position_on_main(&handle));
}

fn update_overlay_position_on_main(app_handle: &AppHandle) {
    if let Some(overlay_window) = app_handle.get_webview_window("recording_overlay") {
        #[cfg(target_os = "linux")]
        if LAYER_SHELL_ACTIVE.load(Ordering::SeqCst) {
            let position = settings::get_settings(app_handle).overlay_position;
            match overlay_window.gtk_window() {
                Ok(gtk_window) => configure_layer_shell_position(&gtk_window, position),
                Err(error) => log::error!("Failed to access GTK overlay window: {error}"),
            }
            return;
        }

        #[cfg(target_os = "windows")]
        {
            let state = if WINDOWS_OVERLAY_IS_STREAMING.load(Ordering::Relaxed) {
                "streaming"
            } else {
                "recording"
            };
            let (width, height) = overlay_dimensions(state);
            if let Err(error) = place_windows_overlay(app_handle, &overlay_window, width, height) {
                log::error!("Failed to update recording overlay position: {error}");
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            // Use the window's current size so centering stays correct whether the
            // overlay is in compact or streaming layout.
            let (width, height) = current_overlay_logical_size(&overlay_window)
                .unwrap_or((OVERLAY_WIDTH, OVERLAY_HEIGHT));
            if let Some((x, y)) = calculate_overlay_position(app_handle, width, height) {
                let _ = overlay_window
                    .set_position(tauri::Position::Logical(tauri::LogicalPosition { x, y }));
            }
        }
    }
}

/// Hides the recording overlay window with fade-out animation
pub fn hide_recording_overlay(app_handle: &AppHandle) {
    if let Some(fft) = app_handle.try_state::<Arc<crate::live_fft::LiveFftManager>>() {
        fft.stop_overlay_scope();
    }
    // The next session's card starts compact and reports its own height as it
    // fills, so a long session's transcript never sizes the next one.
    OVERLAY_STREAMING.store(false, Ordering::Relaxed);
    OVERLAY_STREAM_TEXT_H.store(0, Ordering::Relaxed);
    // Always hide the overlay regardless of settings - if setting was changed while recording,
    // we still want to hide it properly
    if let Some(overlay_window) = app_handle.get_webview_window("recording_overlay") {
        // Snapshot before doing anything observable, so any show that lands
        // after this point invalidates the delayed hide below.
        let scheduled_at = OVERLAY_SHOW_GENERATION.load(Ordering::SeqCst);
        // Emit event to trigger fade-out animation
        let _ = overlay_window.emit("hide-overlay", ());
        // Hide the window after a short delay to allow animation to complete,
        // unless a newer session has shown the overlay again by then.
        let window_clone = overlay_window.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(300));
            if OVERLAY_SHOW_GENERATION.load(Ordering::SeqCst) != scheduled_at {
                log::debug!("Skipping stale overlay hide: a newer session is showing the overlay");
                return;
            }
            let _ = window_clone.hide();
        });
    }
}

// Cached "overlay is enabled" flag, kept in sync with overlay_style. Avoids
// reading the Tauri store on every audio callback (~24 Hz during recording).
// Defaults to false so the audio path doesn't emit until lib.rs::setup
// populates the cache from initial settings.
static OVERLAY_ENABLED: AtomicBool = AtomicBool::new(false);

/// Tracks whether gtk-layer-shell was successfully initialized (Linux only).
/// Used to skip layer-shell calls when the window is a regular fallback.
#[cfg(target_os = "linux")]
static LAYER_SHELL_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Cached geometry of the overlay's scope block (`overlay_scope`), so the
/// show path sizes the window without a store read. Kept in sync by
/// `update_overlay_scope_cache` at startup and on every change; the defaults
/// match `OverlayScopeSettings::default()`.
static OVERLAY_SCOPE_BLOCK_PX: AtomicU32 = AtomicU32::new(110);
static OVERLAY_SCOPE_VIEW_H: AtomicU32 = AtomicU32::new(22);

/// Update the cached scope geometry. Called from `lib.rs` at startup and from
/// `change_overlay_scope_settings`.
pub fn update_overlay_scope_cache(scope: &OverlayScopeSettings) {
    OVERLAY_SCOPE_BLOCK_PX.store(scope.block_width_px(), Ordering::Relaxed);
    // The derived height, not the raw `view_height`: the per-view scale
    // parameters and the circular view's own size can grow the row past it.
    OVERLAY_SCOPE_VIEW_H.store(scope.view_height_px(), Ordering::Relaxed);
}

/// Extra height (logical px) the streaming overlay's transcript needs beyond its
/// base card, as last measured by the frontend, and the cap it may grow to.
///
/// The Multi-STT streaming-first mode shows the whole session's text, so the
/// card has to grow with it. The frontend measures its own transcript and reports
/// the height in 24 px steps (`overlay_stream_text_height`) — one call per line
/// of text, not per character — and reads the cap from the same reply so its
/// `--ov-cap-max-h` and this window size can never disagree. The cap is ~70 % of
/// the monitor height (minus the card's own chrome), past which the card scrolls
/// back instead of growing.
static OVERLAY_STREAM_TEXT_H: AtomicU32 = AtomicU32::new(0);
static OVERLAY_STREAM_TEXT_CAP: AtomicU32 = AtomicU32::new(0);

/// The transcript height to add to the streaming card, never past the cap.
///
/// A cap of 0 means no monitor has been measured yet (no session has shown the
/// overlay), in which case nothing is added: the first measurement arrives with
/// the cap, so the card can never be sized off an unclamped report.
fn streaming_text_height() -> f64 {
    let cap = OVERLAY_STREAM_TEXT_CAP.load(Ordering::Relaxed);
    f64::from(OVERLAY_STREAM_TEXT_H.load(Ordering::Relaxed).min(cap))
}

/// Whether the overlay is currently laid out as the streaming card. The measured
/// transcript height belongs to that layout only — a later state change resets it.
static OVERLAY_STREAMING: AtomicBool = AtomicBool::new(false);

/// Report the streaming card's transcript height (logical px) and grow the native
/// window to fit it. Returns the height the frontend may render before scrolling.
///
/// Called by `RecordingOverlay.tsx` in 24 px steps while the Multi-STT
/// streaming-first session fills the card, so this runs about once per line of
/// text rather than per character. The reply is the same capped value the window
/// was just sized with, so the cap the card applies and the window it lives in
/// are one number.
#[tauri::command]
#[specta::specta]
pub fn overlay_stream_text_height(app: AppHandle, height_px: u32) -> u32 {
    // Only the streaming card reports, and only while it is on screen: a card
    // that is fading out or hidden must not resize the window under the next
    // state's layout.
    if !OVERLAY_STREAMING.load(Ordering::Relaxed) {
        return OVERLAY_STREAM_TEXT_CAP.load(Ordering::Relaxed);
    }
    let cap = streaming_text_cap(&app);
    OVERLAY_STREAM_TEXT_CAP.store(cap, Ordering::Relaxed);
    OVERLAY_STREAM_TEXT_H.store(height_px.min(cap), Ordering::Relaxed);

    let (width, height) = overlay_dimensions("streaming");
    let handle = app.clone();
    let _ =
        app.run_on_main_thread(move || resize_streaming_overlay_on_main(&handle, width, height));
    cap
}

/// The tallest the streaming card's transcript may be: the whole card capped at
/// ~70 % of the monitor it is on, minus the card's own chrome.
///
/// The frontend's reported height is in CSS px and the window is sized in logical
/// px, which are the same length on every platform (WebView2's accessibility text
/// scale zooms both together, see `windows_text_scale_factor`), so the conversion
/// is the monitor's device pixel ratio and nothing else.
fn streaming_text_cap(app: &AppHandle) -> u32 {
    let (_, base_height) = streaming_dimensions_baseline();
    let logical_monitor_height = app
        .get_webview_window("recording_overlay")
        .and_then(|window| {
            window
                .current_monitor()
                .ok()
                .flatten()
                .or_else(|| window.primary_monitor().ok().flatten())
        })
        .map(|monitor| {
            let scale = monitor.scale_factor();
            f64::from(monitor.size().height) / if scale > 0.0 { scale } else { 1.0 }
        })
        // No monitor to measure (the window is gone): leave the card at its base
        // size rather than inventing a screen big enough for anything.
        .unwrap_or(base_height);
    (logical_monitor_height * 0.7 - base_height)
        .max(0.0)
        .round() as u32
}

/// The streaming card's size with no transcript measured yet — what the card
/// grows from, and the chrome the transcript cap is measured against.
fn streaming_dimensions_baseline() -> (f64, f64) {
    let view_h = OVERLAY_SCOPE_VIEW_H.load(Ordering::Relaxed);
    let row_h = (f64::from(view_h) + OVERLAY_ROW_PADDING_H).max(OVERLAY_ROW_H);
    (
        OVERLAY_STREAM_WIDTH,
        OVERLAY_STREAM_HEIGHT + (row_h - OVERLAY_ROW_H),
    )
}

/// Resize the visible streaming overlay to `width` x `height`, keeping it
/// anchored where it is — the mirror of `show_overlay_state_on_main`'s sizing
/// path without the show.
fn resize_streaming_overlay_on_main(app_handle: &AppHandle, width: f64, height: f64) {
    let Some(overlay_window) = app_handle.get_webview_window("recording_overlay") else {
        return;
    };
    // A hide is in flight: it has already faded this card out, so resizing would
    // only leave a flash of the wrong layout behind.
    if !overlay_window.is_visible().unwrap_or(false) {
        return;
    }

    #[cfg(target_os = "linux")]
    if LAYER_SHELL_ACTIVE.load(Ordering::SeqCst) {
        let position = settings::get_settings(app_handle).overlay_position;
        match overlay_window.gtk_window() {
            Ok(gtk_window) => {
                configure_layer_shell_surface(&gtk_window, position, width, height);
            }
            Err(error) => log::error!("Failed to access GTK overlay window: {error}"),
        }
        return;
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = overlay_window.set_size(tauri::Size::Logical(tauri::LogicalSize { width, height }));
        if let Some((x, y)) = calculate_overlay_position(app_handle, width, height) {
            let _ = overlay_window
                .set_position(tauri::Position::Logical(tauri::LogicalPosition { x, y }));
        }
    }

    // Windows sizes and places in one native call — the layout's own set_size is
    // tao's current-DPI conversion, which mislands a cross-monitor move.
    #[cfg(target_os = "windows")]
    if let Err(error) = place_windows_overlay(app_handle, &overlay_window, width, height) {
        log::error!("Failed to resize recording overlay: {error}");
    }
}

/// Cached "speech stats are enabled" flag, kept in sync with
/// `overlay_speech_stats`. Read on the audio path (every frame produces a
/// candidate update) and in the overlay show path, so neither has to touch the
/// Tauri store.
static SPEECH_STATS_ENABLED: AtomicBool = AtomicBool::new(false);

/// Update the cached overlay-enabled flag. Called from `lib.rs` at
/// startup after settings load, and from `change_overlay_style_setting`
/// whenever the user changes whether the overlay is shown.
pub fn update_overlay_enabled_cache(enabled: bool) {
    OVERLAY_ENABLED.store(enabled, Ordering::Relaxed);
}

/// Update the cached speech-stats flag. Called from `lib.rs` at startup and
/// from `change_overlay_speech_stats_setting`.
pub fn update_speech_stats_enabled_cache(enabled: bool) {
    SPEECH_STATS_ENABLED.store(enabled, Ordering::Relaxed);
}

/// Live speech statistics for the recording overlay.
///
/// Not a fixed-rate stream: the recorder sends it
/// when the speaking/silent state flips, and roughly every 150 ms while speech
/// continues. A silent stretch produces no events at all.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct SpeechActivityEvent {
    /// Whether the user is speaking right now, debounced by the configured
    /// pause tolerance so it does not flicker between words.
    pub speaking: bool,
    /// Milliseconds of speech so far in this recording, excluding pauses long
    /// enough to count as silence. The overlay divides its word count by this
    /// to get words per minute.
    pub speech_ms: u32,
}

/// Forward a speech-clock update to the overlay.
pub fn emit_speech_activity(app_handle: &AppHandle, activity: SpeechActivity) {
    // The overlay window exists even when it is never shown, and every event
    // delivered to it costs WebKit allocations that accumulate (issue #1279).
    // No overlay, or stats turned off, means no event.
    if !OVERLAY_ENABLED.load(Ordering::Relaxed) || !SPEECH_STATS_ENABLED.load(Ordering::Relaxed) {
        return;
    }

    let _ = SpeechActivityEvent {
        speaking: activity.speaking,
        speech_ms: activity.speech_ms.min(u64::from(u32::MAX)) as u32,
    }
    .emit_to(app_handle, "recording_overlay");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_dimensions_match_the_default_constants_and_grow_with_the_views() {
        let default = OverlayScopeSettings::default();
        assert_eq!(
            compact_dimensions(default.block_width_px(), default.view_height, false),
            (OVERLAY_WIDTH, OVERLAY_HEIGHT)
        );
        assert_eq!(
            compact_dimensions(default.block_width_px(), default.view_height, true),
            (OVERLAY_STATS_WIDTH, OVERLAY_HEIGHT)
        );
        // No views: the pill shrinks to its base; taller views raise the row.
        assert_eq!(
            compact_dimensions(0, 22, false).0,
            OVERLAY_REST_BASE_W + OVERLAY_WINDOW_SLACK_W
        );
        assert_eq!(
            compact_dimensions(110, 40, false).1,
            40.0 + 18.0 + OVERLAY_WINDOW_SLACK_H
        );
    }

    #[test]
    fn monitor_hit_test_uses_half_open_physical_bounds() {
        let position = PhysicalPosition::new(-2560, -200);
        let size = PhysicalSize::new(2560, 1440);

        assert!(is_mouse_within_monitor((-2560, -200), &position, &size));
        assert!(is_mouse_within_monitor((-1, 1239), &position, &size));
        assert!(!is_mouse_within_monitor((0, 0), &position, &size));
        assert!(!is_mouse_within_monitor((-1, 1240), &position, &size));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_cursor_hit_test_does_not_scale_physical_monitor_bounds() {
        let position = PhysicalPosition::new(1920, 0);
        let size = PhysicalSize::new(3840, 2160);
        let cursor = (5000, 1000);

        assert!(is_mouse_within_monitor(cursor, &position, &size));

        // This is the old mixed-coordinate comparison. It excludes a cursor
        // that is visibly inside a secondary display running at 150%.
        let scale = 1.5;
        let logical_position = PhysicalPosition::new(
            (position.x as f64 / scale) as i32,
            (position.y as f64 / scale) as i32,
        );
        let logical_size = PhysicalSize::new(
            (size.width as f64 / scale) as u32,
            (size.height as f64 / scale) as u32,
        );
        assert!(!is_mouse_within_monitor(
            cursor,
            &logical_position,
            &logical_size
        ));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_overlay_bounds_use_destination_monitor_scale() {
        let monitor_position = PhysicalPosition::new(1920, 0);
        let monitor_size = PhysicalSize::new(3840, 2160);

        assert_eq!(
            windows_overlay_bounds(
                monitor_position,
                monitor_size,
                1.5,
                1.0,
                OVERLAY_WIDTH,
                OVERLAY_HEIGHT,
                OverlayPosition::Bottom,
            ),
            // OVERLAY_WIDTH (280) at 150 %: 420 px, centred on the 3840 px monitor.
            (3630, 2025, 420, 75)
        );
        assert_eq!(
            windows_overlay_bounds(
                monitor_position,
                monitor_size,
                1.5,
                1.0,
                OVERLAY_WIDTH,
                OVERLAY_HEIGHT,
                OverlayPosition::Top,
            ),
            (3630, 6, 420, 75)
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_overlay_bounds_support_negative_monitor_origins() {
        assert_eq!(
            windows_overlay_bounds(
                PhysicalPosition::new(-2560, -200),
                PhysicalSize::new(2560, 1440),
                1.25,
                1.0,
                OVERLAY_STREAM_WIDTH,
                OVERLAY_STREAM_HEIGHT,
                OverlayPosition::Bottom,
            ),
            (-1530, 1040, 500, 150)
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_overlay_bounds_grow_with_text_scale_without_moving_the_anchored_edge() {
        let monitor_position = PhysicalPosition::new(-2560, -200);
        let monitor_size = PhysicalSize::new(2560, 1440);

        let (x, y, width, height) = windows_overlay_bounds(
            monitor_position,
            monitor_size,
            1.25,
            1.1,
            OVERLAY_STREAM_WIDTH,
            OVERLAY_STREAM_HEIGHT,
            OverlayPosition::Bottom,
        );
        // 400x120 logical at 1.25 DPI x 1.1 text, still centered horizontally.
        assert_eq!((x, y, width, height), (-1555, 1025, 550, 165));
        // Bottom edge unchanged from the 1.0 case above (1040 + 150).
        assert_eq!(y + height, 1190);

        let (_, top_y, _, _) = windows_overlay_bounds(
            monitor_position,
            monitor_size,
            1.25,
            1.1,
            OVERLAY_STREAM_WIDTH,
            OVERLAY_STREAM_HEIGHT,
            OverlayPosition::Top,
        );
        // Top offset rides the DPI scale alone, so the top edge doesn't move.
        assert_eq!(top_y, -195);
    }
}
