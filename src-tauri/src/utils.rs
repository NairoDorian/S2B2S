use crate::TranscriptionCoordinator;
use crate::managers::audio::AudioRecordingManager;
use crate::managers::transcription::TranscriptionManager;
use crate::shortcut;
use log::info;
use std::sync::Arc;
use tauri::{AppHandle, Manager};

// Re-export all utility modules for easy access
// pub use crate::audio_feedback::*;
pub use crate::clipboard::*;
pub use crate::overlay::*;
pub use crate::tray::*;

/// Preserve diagnostic text in development builds, but redact it in releases.
/// Do not use for secrets such as API keys, which must always be redacted.
pub fn redact_text(text: &str) -> &str {
    if cfg!(debug_assertions) {
        text
    } else {
        "[REDACTED]"
    }
}

/// Configures Windows process priority, power-throttling bypass (EcoQoS disable),
/// and high-resolution multimedia system timers for lowest latency.
#[cfg(target_os = "windows")]
pub fn init_windows_process_performance() {
    use windows::Win32::Media::timeBeginPeriod;
    use windows::Win32::System::Threading::{
        GetCurrentProcess, HIGH_PRIORITY_CLASS, PROCESS_POWER_THROTTLING_CURRENT_VERSION,
        PROCESS_POWER_THROTTLING_EXECUTION_SPEED, PROCESS_POWER_THROTTLING_STATE,
        ProcessPowerThrottling, SetPriorityClass, SetProcessInformation,
    };

    unsafe {
        // 1. Elevate Process Priority Class to HIGH_PRIORITY_CLASS
        if let Err(e) = SetPriorityClass(GetCurrentProcess(), HIGH_PRIORITY_CLASS) {
            log::warn!("Failed to set HIGH_PRIORITY_CLASS: {e}");
        } else {
            log::info!("Elevated process priority to HIGH_PRIORITY_CLASS");
        }

        // 2. Disable Windows 11 background power throttling (EcoQoS / Efficiency Mode)
        // so threads are not relegated to E-cores or downclocked when Handy is in the system tray.
        let mut throttling_state = PROCESS_POWER_THROTTLING_STATE {
            Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
            ControlMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
            StateMask: 0, // Disable throttling
        };
        let res = SetProcessInformation(
            GetCurrentProcess(),
            ProcessPowerThrottling,
            &raw mut throttling_state as *mut _,
            std::mem::size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
        );
        if let Err(e) = res {
            log::debug!("SetProcessInformation (EcoQoS disable) skipped or unsupported: {e}");
        } else {
            log::info!("Disabled Windows EcoQoS power throttling for Handy");
        }

        // 3. Request 1ms global system timer resolution. Deliberately never
        //    paired with timeEndPeriod: the request is meant to last for the
        //    process lifetime and Windows releases it at exit.
        let timer_res = timeBeginPeriod(1);
        if timer_res == 0 {
            log::info!("Set Windows system timer resolution to 1ms (timeBeginPeriod)");
        } else {
            log::warn!("timeBeginPeriod returned {timer_res}");
        }
    }
}
#[cfg(any(test, all(target_os = "windows", target_arch = "x86_64")))]
const IMAGE_FILE_MACHINE_ARM64: u16 = 0xaa64;

#[cfg(any(test, all(target_os = "windows", target_arch = "x86_64")))]
fn native_machine_is_arm64(native_machine: Option<u16>) -> bool {
    native_machine == Some(IMAGE_FILE_MACHINE_ARM64)
}

/// Whether this is the x64 Windows build running under emulation on Windows ARM64.
///
/// Only that exact process/host pairing disables the transcribe.cpp GPU path.
/// Detection is deliberately fail-open: a native x64 host, an older Windows
/// version without `IsWow64Process2`, or any API error leaves existing behavior
/// unchanged.
pub fn is_windows_x64_emulated_on_arm64() -> bool {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        use std::sync::OnceLock;

        static DETECTED: OnceLock<bool> = OnceLock::new();
        *DETECTED.get_or_init(|| native_machine_is_arm64(native_windows_machine()))
    }

    #[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
    {
        false
    }
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn native_windows_machine() -> Option<u16> {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
    use windows::Win32::System::Threading::GetCurrentProcess;
    use windows::core::{BOOL, s, w};

    type IsWow64Process2 = unsafe extern "system" fn(HANDLE, *mut u16, *mut u16) -> BOOL;

    // Resolve IsWow64Process2 dynamically so merely starting Handy never raises
    // the minimum Windows version. Windows-on-ARM versions provide this API,
    // while a missing symbol or failed query safely preserves the x64 behavior.
    unsafe {
        let kernel32 = GetModuleHandleW(w!("kernel32.dll")).ok()?;
        let address = GetProcAddress(kernel32, s!("IsWow64Process2"))?;
        // SAFETY: GetProcAddress returned the documented IsWow64Process2 symbol;
        // function pointers have the same representation on supported Windows.
        let is_wow64_process2: IsWow64Process2 = std::mem::transmute(address);
        let mut process_machine = 0u16;
        let mut native_machine = 0u16;
        is_wow64_process2(
            GetCurrentProcess(),
            &mut process_machine,
            &mut native_machine,
        )
        .as_bool()
        .then_some(native_machine)
    }
}

/// Centralized cancellation function that can be called from anywhere in the app.
/// Handles cancelling both recording and transcription operations and updates UI state.
pub fn cancel_current_operation(app: &AppHandle) {
    info!("Initiating operation cancellation...");

    // Unregister the cancel shortcut asynchronously
    shortcut::unregister_cancel_shortcut(app);

    // Cancel any ongoing recording
    let audio_manager = app.state::<Arc<AudioRecordingManager>>();
    let recording_was_active = audio_manager.is_recording();
    audio_manager.cancel_recording();

    // Abandon any live streaming transcription
    let tm = app.state::<Arc<TranscriptionManager>>();
    tm.cancel_stream();

    if let Some(sm) = app.try_state::<Arc<crate::managers::statistics::StatisticsManager>>() {
        sm.cancel_active_normal_runs();
    }

    // Update tray icon and hide overlay
    set_tray_state(app, crate::tray::TrayIconState::Idle);
    hide_recording_overlay(app);

    // Unload model if immediate unload is enabled
    tm.maybe_unload_immediately("cancellation");

    // Restore normal power mode if performance mode is enabled
    let settings = crate::settings::get_settings(app);
    if settings.multi_stt_performance_mode_enabled {
        let normal_shortcut = settings.multi_stt_performance_mode_normal_shortcut.clone();
        let app_handle = app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            crate::clipboard::simulate_key_combination(&app_handle, &normal_shortcut);
        });
    }

    // Notify coordinator so it can keep lifecycle state coherent.
    if let Some(coordinator) = app.try_state::<TranscriptionCoordinator>() {
        coordinator.notify_cancel(recording_was_active);
    }

    info!("Operation cancellation completed - returned to idle state");
}

/// Check if using the Wayland display server protocol
#[cfg(target_os = "linux")]
pub fn is_wayland() -> bool {
    std::env::var("WAYLAND_DISPLAY").is_ok()
        || std::env::var("XDG_SESSION_TYPE")
            .map(|v| v.to_lowercase() == "wayland")
            .unwrap_or(false)
}

/// Check if running on KDE Plasma desktop environment
#[cfg(target_os = "linux")]
pub fn is_kde_plasma() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP")
        .map(|v| v.to_uppercase().contains("KDE"))
        .unwrap_or(false)
        || std::env::var("KDE_SESSION_VERSION").is_ok()
}

/// Check if running on KDE Plasma with Wayland
#[cfg(target_os = "linux")]
pub fn is_kde_wayland() -> bool {
    is_wayland() && is_kde_plasma()
}

/// Check if running on GNOME desktop environment
#[cfg(target_os = "linux")]
pub fn is_gnome() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP")
        .map(|v| v.to_uppercase().contains("GNOME"))
        .unwrap_or(false)
}

/// Check if running on GNOME with Wayland
#[cfg(target_os = "linux")]
pub fn is_gnome_wayland() -> bool {
    is_wayland() && is_gnome()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arm64_native_machine_is_the_only_match() {
        assert!(native_machine_is_arm64(Some(IMAGE_FILE_MACHINE_ARM64)));
        assert!(!native_machine_is_arm64(Some(0x8664))); // AMD64
        assert!(!native_machine_is_arm64(Some(0x014c))); // I386
        assert!(!native_machine_is_arm64(None)); // API unavailable or failed
    }
}
