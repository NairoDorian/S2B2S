use crate::TranscriptionCoordinator;
use crate::app_identity;
use crate::managers::audio::AudioRecordingManager;
use crate::managers::transcription::TranscriptionManager;
use crate::shortcut;
use log::{debug, info, warn};
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

/// Emit a multi-line payload — an LLM prompt, a model's answer, one STT model's
/// transcript — as one log line per source line, under `label`.
///
/// The LLM and merge paths are the ones where "what did it actually send, and
/// what came back" is the entire question, and a payload logged as a length
/// answers none of it. A single `debug!` holding a 3 KB prompt is equally
/// useless: it is one wrapped paragraph in a terminal. One line in, one line
/// out, so a chunk's slots and the merge's answer can be read against each
/// other.
///
/// `redact_text` keeps the body out of a release log while leaving a debug
/// build — the only one whose console carries these lines — intact, and the
/// level check keeps the payload from being formatted at all when nobody is
/// listening.
pub fn log_multiline(label: &str, text: &str) {
    if !log::log_enabled!(log::Level::Debug) {
        return;
    }
    let text = redact_text(text);
    debug!(
        "{} — {} chars, {} line(s)",
        label,
        text.chars().count(),
        text.lines().count()
    );
    for (index, line) in text.lines().enumerate() {
        debug!("  {:>3}| {}", index + 1, line);
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
        // so threads are not relegated to E-cores or downclocked while the app sits in the system tray.
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
            log::info!(
                "Disabled Windows EcoQoS power throttling for {}",
                app_identity::NAME
            );
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

    // Resolve IsWow64Process2 dynamically so merely starting the app never raises
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

    // The overlay preview is a recording like any other to the cancel hotkey;
    // its own stop cleans its flag, statistics run and cancel shortcut before
    // the generic teardown below re-does the shared parts harmlessly.
    let _ = crate::overlay_preview::stop(app);

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

/// Truthiness of a flag *value*: "1", "true", "yes" and "on" are true;
/// "0", "false", "no", "off" and the empty string are false, case-insensitively.
///
/// Shared by [`env_flag_enabled`] and [`app_env_var`] so the two entry points
/// cannot drift apart on what "set to true" means.
fn env_flag_truthy(value: &str) -> bool {
    !matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "" | "0" | "false" | "no" | "off"
    )
}

/// Returns true when the environment variable is set to a truthy value
/// (e.g. "1", "true", "yes", "on").
/// "0", "false", "no", "off" and empty string are treated as falsy (case-insensitive).
/// Returns false when the variable is not set.
pub fn env_flag_enabled(name: &str) -> bool {
    std::env::var(name)
        .map(|value| env_flag_truthy(&value))
        .unwrap_or(false)
}

/// Read an application variable by its suffix alone — the prefix comes from
/// [`app_identity::ENV_PREFIX`], so no call site spells it.
///
/// The legacy `HANDY_`-prefixed spelling is tried second. These variables are
/// set by build scripts, CI, the Nix package and users' own launch
/// configurations, so dropping the old spelling outright would silently change
/// behaviour for anyone who set one — a Nix user's `HANDY_DISABLE_UPDATER`
/// would stop disabling the updater, with nothing in the log to say why. The new
/// name wins when both are set; the old one still works and says so, which is a
/// handful of one-shot reads on the startup and overlay-init paths.
///
/// This is the value-typed form, for variables that carry a number or a path
/// ([`app_env_flag`] is the boolean form built on it).
///
/// # Panics
///
/// Debug builds panic on a suffix that is not `[A-Z0-9_]+`, which catches a
/// caller passing a pre-prefixed or lower-case name at the first test rather
/// than at the one launch where the variable mattered.
pub fn app_env_var(suffix: &str) -> Option<String> {
    debug_assert!(
        !suffix.is_empty()
            && suffix
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_'),
        "app_env_var takes the bare suffix, e.g. \"DISABLE_UPDATER\", got {suffix:?}",
    );

    let name = format!("{}{suffix}", app_identity::ENV_PREFIX);
    if let Ok(value) = std::env::var(&name) {
        return Some(value);
    }

    let legacy = format!("{}{suffix}", app_identity::LEGACY_ENV_PREFIX);
    if legacy != name
        && let Ok(value) = std::env::var(&legacy)
    {
        warn!("{legacy} is set — that is the pre-0.9.7 name, please rename it to {name}");
        return Some(value);
    }
    None
}

/// Read an application flag by its suffix alone, as a boolean — the prefix comes
/// from [`app_identity::ENV_PREFIX`], so no call site spells it.
///
/// See [`app_env_var`] for the legacy-spelling rule and the suffix contract.
pub fn app_env_flag(suffix: &str) -> bool {
    app_env_var(suffix)
        .map(|value| env_flag_truthy(&value))
        .unwrap_or(false)
}

/// A fresh, empty directory under the system temp directory for a test that
/// needs to touch the filesystem.
///
/// The name is `<slug>-<purpose>-<pid>-<n>`, with `n` counted per process, so
/// two tests never share a directory and a leftover is obviously this app's.
/// That is what a test-written directory should look like; the alternative
/// (`<purpose>-<pid>`, which several tests used to build inline) breaks the
/// moment two tests pick the same purpose, or when a directory survives a
/// run that panicked — the next run then starts from dirty state and fails
/// somewhere unrelated.
///
/// The caller owns cleanup: `std::fs::remove_dir_all` at the end, and a test
/// that panics deliberately leaves the directory behind to be inspected.
#[cfg(test)]
pub fn temp_test_dir(purpose: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static NEXT: AtomicU32 = AtomicU32::new(0);

    let path = std::env::temp_dir().join(format!(
        "{}-{purpose}-{}-{}",
        app_identity::SLUG,
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed),
    ));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("create the test's temp directory");
    path
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

    // `env_flag_enabled` takes a *raw* name rather than a suffix, so these two
    // tests deliberately use a name outside the application's namespace: the
    // point is the truthiness rule itself, and a name derived from the prefix
    // would make the rule and the prefix lookup fail together.
    #[test]
    fn env_flag_enabled_true_for_truthy_values() {
        for value in ["1", "true", "TRUE", "yes", "on", " 1 "] {
            unsafe { std::env::set_var("TEST_FLAG_TRUTHY", value) };
            assert!(env_flag_enabled("TEST_FLAG_TRUTHY"), "{value:?}");
        }
        unsafe { std::env::remove_var("TEST_FLAG_TRUTHY") };
    }

    #[test]
    fn env_flag_enabled_false_for_falsy_or_unset() {
        assert!(!env_flag_enabled("TEST_FLAG_UNSET"));

        for value in ["0", "false", "FALSE", "no", "off", ""] {
            unsafe { std::env::set_var("TEST_FLAG_FALSY", value) };
            assert!(!env_flag_enabled("TEST_FLAG_FALSY"), "{value:?}");
        }
        unsafe { std::env::remove_var("TEST_FLAG_FALSY") };
    }

    // The tests below share the process-wide environment, so each uses its own
    // variable name. Two tests on the same name would race under `cargo test`'s
    // thread pool, and the failure would only appear on a loaded machine.
    //
    // The prefixed spellings are built from the constants rather than written
    // out: these tests exist to prove that `app_env_var` *derives* the name from
    // the prefix, and a test that hardcodes the prefixed form passes just as
    // happily against an implementation that ignores the constant.

    /// `<PREFIX><suffix>` for whichever prefix this build ships with.
    fn current(suffix: &str) -> String {
        format!("{}{suffix}", app_identity::ENV_PREFIX)
    }

    /// The same for the pre-rename prefix.
    fn legacy(suffix: &str) -> String {
        format!("{}{suffix}", app_identity::LEGACY_ENV_PREFIX)
    }

    #[test]
    fn app_env_var_reads_the_current_prefix() {
        unsafe { std::env::set_var(current("TEST_VALUE_CURRENT"), "42") };
        assert_eq!(app_env_var("TEST_VALUE_CURRENT").as_deref(), Some("42"));
        unsafe { std::env::remove_var(current("TEST_VALUE_CURRENT")) };
    }

    #[test]
    fn app_env_var_falls_back_to_the_legacy_prefix() {
        unsafe { std::env::set_var(legacy("TEST_VALUE_LEGACY"), "42") };
        assert_eq!(app_env_var("TEST_VALUE_LEGACY").as_deref(), Some("42"));
        unsafe { std::env::remove_var(legacy("TEST_VALUE_LEGACY")) };
    }

    #[test]
    fn the_current_prefix_wins_over_the_legacy_one() {
        unsafe { std::env::set_var(current("TEST_VALUE_BOTH"), "current") };
        unsafe { std::env::set_var(legacy("TEST_VALUE_BOTH"), "legacy") };
        assert_eq!(app_env_var("TEST_VALUE_BOTH").as_deref(), Some("current"));
        unsafe { std::env::remove_var(current("TEST_VALUE_BOTH")) };
        unsafe { std::env::remove_var(legacy("TEST_VALUE_BOTH")) };
    }

    #[test]
    fn app_env_var_is_none_when_neither_spelling_is_set() {
        assert_eq!(app_env_var("TEST_VALUE_UNSET"), None);
    }

    /// A flag is the truthiness of the same lookup, so a value that is not a
    /// number still reads as "set": this is what lets a value-typed flag such as
    /// `DEBUG_MIC_READY_DELAY_MS` and a boolean one such as `NO_PRUNE` share one
    /// naming rule.
    #[test]
    fn app_env_flag_is_the_truthiness_of_app_env_var() {
        unsafe { std::env::set_var(current("TEST_FLAG_BOTH_FORMS"), "0") };
        assert_eq!(app_env_var("TEST_FLAG_BOTH_FORMS").as_deref(), Some("0"));
        assert!(!app_env_flag("TEST_FLAG_BOTH_FORMS"));

        unsafe { std::env::set_var(current("TEST_FLAG_BOTH_FORMS"), "1") };
        assert!(app_env_flag("TEST_FLAG_BOTH_FORMS"));
        unsafe { std::env::remove_var(current("TEST_FLAG_BOTH_FORMS")) };
    }
}
