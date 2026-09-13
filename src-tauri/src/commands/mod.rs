pub mod audio;
pub mod file_transcription;
pub mod history;
pub mod live_fft;
pub mod live_mode;
pub mod llama;
pub mod models;
pub mod statistics;
pub mod system;
pub mod transcription;

use crate::settings::{
    AppSettings, LogLevel, get_settings, update_checks_forced_disabled, write_settings,
};
use crate::utils::cancel_current_operation;
use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;

#[tauri::command]
#[specta::specta]
pub fn cancel_operation(app: AppHandle) {
    cancel_current_operation(&app);
}

#[tauri::command]
#[specta::specta]
pub fn is_portable() -> bool {
    crate::portable::is_portable()
}

#[tauri::command]
#[specta::specta]
pub fn is_update_checks_locked() -> bool {
    update_checks_forced_disabled()
}

#[tauri::command]
#[specta::specta]
pub fn get_app_dir_path(app: AppHandle) -> Result<String, String> {
    let app_data_dir = crate::portable::app_data_dir(&app)
        .map_err(|e| format!("Failed to get app data directory: {}", e))?;

    Ok(app_data_dir.to_string_lossy().to_string())
}

#[tauri::command]
#[specta::specta]
pub fn get_app_settings(app: AppHandle) -> Result<AppSettings, String> {
    Ok(get_settings(&app))
}

#[tauri::command]
#[specta::specta]
pub fn get_default_settings() -> Result<AppSettings, String> {
    Ok(crate::settings::get_default_settings())
}

#[tauri::command]
#[specta::specta]
pub fn get_log_dir_path(app: AppHandle) -> Result<String, String> {
    let log_dir = crate::portable::app_log_dir(&app)
        .map_err(|e| format!("Failed to get log directory: {}", e))?;

    Ok(log_dir.to_string_lossy().to_string())
}

/// The log file the plugin's file target writes to: `<log dir>/<basename>.log`
/// (`RotationStrategy::KeepOne`, so there is exactly one current file and the
/// name never changes).
fn current_log_file(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let log_dir = crate::portable::app_log_dir(app)
        .map_err(|e| format!("Failed to get log directory: {}", e))?;
    Ok(log_dir.join(format!("{}.log", crate::app_identity::RECORDING_BASENAME)))
}

/// The last `limit` lines of the log file, oldest first.
///
/// The debug panel's live log viewer polls this as its ground truth: the
/// `log://log` webview stream only carries records emitted while a listener is
/// attached, so without the file the panel's history would depend entirely on
/// when the page happened to be open. The tail read is capped at 512 KiB —
/// plenty for any realistic line count at ~100 bytes a line, and it keeps a
/// 500 MB rotated log from being read whole.
#[tauri::command]
#[specta::specta]
pub fn get_recent_logs(app: AppHandle, limit: u32) -> Result<String, String> {
    use std::io::{Read, Seek, SeekFrom};

    let path = current_log_file(&app)?;
    let mut file =
        std::fs::File::open(&path).map_err(|e| format!("Failed to open log file: {}", e))?;

    const TAIL_CAP: u64 = 512 * 1024;
    let len = file
        .metadata()
        .map_err(|e| format!("Failed to stat log file: {}", e))?
        .len();
    let start = len.saturating_sub(TAIL_CAP);
    file.seek(SeekFrom::Start(start))
        .map_err(|e| format!("Failed to seek log file: {}", e))?;

    let mut buf = String::new();
    file.read_to_string(&mut buf)
        .map_err(|e| format!("Failed to read log file: {}", e))?;

    let mut lines: Vec<&str> = buf.lines().collect();
    // When the cap cut into the middle of the file, the first line read is a
    // partial record — drop it rather than show a truncated line.
    if start > 0 && !lines.is_empty() {
        lines.remove(0);
    }
    let skip = lines.len().saturating_sub(limit as usize);
    Ok(lines
        .into_iter()
        .skip(skip)
        .collect::<Vec<&str>>()
        .join("\n"))
}

/// Truncate the log file. Explicit user action only — the panel itself never
/// clears what it shows on its own.
#[tauri::command]
#[specta::specta]
pub fn clear_logs(app: AppHandle) -> Result<(), String> {
    let path = current_log_file(&app)?;
    std::fs::File::create(&path).map_err(|e| format!("Failed to truncate log file: {}", e))?;
    Ok(())
}

#[specta::specta]
#[tauri::command]
pub fn set_log_level(app: AppHandle, level: LogLevel) -> Result<(), String> {
    let tauri_log_level: tauri_plugin_log::LogLevel = level.into();
    let log_level: log::Level = tauri_log_level.into();

    let mut settings = get_settings(&app);
    let previous = settings.log_level;

    // Say so *before* the atomic moves, at `info`. This is the one command that
    // can silence the log, and once the level is `warn` or `error` an info
    // record is gone — so a transition reported after the store would be
    // invisible in exactly the case worth recording. A downgrade with no record
    // anywhere is how a store reaches `info` with nothing in the log to say who
    // set it, which is what made the "logs got quieter" report take a file-log
    // archaeology session to explain. The asymmetry is deliberate: a downgrade
    // is the dangerous direction and this catches it, while an upgrade is
    // self-announcing (the log fills up) and may legitimately be filtered by the
    // near-silent level it is leaving.
    if previous != level {
        log::info!(
            "Log level {:?} -> {:?}, set from the settings UI; file log now {}",
            previous,
            level,
            log_level.to_level_filter(),
        );
    }

    // Update the file log level atomic so the filter picks up the new level
    crate::FILE_LOG_LEVEL.store(
        log_level.to_level_filter() as u8,
        std::sync::atomic::Ordering::Relaxed,
    );

    settings.log_level = level;
    write_settings(&app, settings);

    Ok(())
}

#[specta::specta]
#[tauri::command]
pub fn open_recordings_folder(app: AppHandle) -> Result<(), String> {
    let app_data_dir = crate::portable::app_data_dir(&app)
        .map_err(|e| format!("Failed to get app data directory: {}", e))?;

    let recordings_dir = app_data_dir.join("recordings");

    let path = recordings_dir.to_string_lossy().as_ref().to_string();
    app.opener()
        .open_path(path, None::<String>)
        .map_err(|e| format!("Failed to open recordings folder: {}", e))?;

    Ok(())
}

#[specta::specta]
#[tauri::command]
pub fn open_models_folder(app: AppHandle) -> Result<(), String> {
    let app_data_dir = crate::portable::app_data_dir(&app)
        .map_err(|e| format!("Failed to get app data directory: {}", e))?;

    let models_dir = app_data_dir.join("models");
    let hf_cache = crate::managers::model::hf_cache_dir();

    let has_hf_models = hf_cache.exists()
        && std::fs::read_dir(&hf_cache)
            .map(|entries| {
                entries.filter_map(|e| e.ok()).any(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    name.starts_with("models--")
                })
            })
            .unwrap_or(false);

    let has_custom_models = models_dir.exists()
        && std::fs::read_dir(&models_dir)
            .map(|mut entries| entries.next().is_some())
            .unwrap_or(false);

    let target_dir = if has_hf_models && !has_custom_models {
        hf_cache
    } else if models_dir.exists() {
        models_dir
    } else if hf_cache.exists() {
        hf_cache
    } else {
        std::fs::create_dir_all(&models_dir)
            .map_err(|e| format!("Failed to create models folder: {}", e))?;
        models_dir
    };

    let path = target_dir.to_string_lossy().as_ref().to_string();
    app.opener()
        .open_path(path, None::<String>)
        .map_err(|e| format!("Failed to open models folder: {}", e))?;

    Ok(())
}

#[specta::specta]
#[tauri::command]
pub fn open_plugins_folder(app: AppHandle) -> Result<(), String> {
    let app_data_dir = crate::portable::app_data_dir(&app)
        .map_err(|e| format!("Failed to get app data directory: {}", e))?;

    let plugins_dir = app_data_dir.join("plugins");
    let _ = std::fs::create_dir_all(&plugins_dir);

    let path = plugins_dir.to_string_lossy().as_ref().to_string();
    app.opener()
        .open_path(path, None::<String>)
        .map_err(|e| format!("Failed to open plugins directory: {}", e))?;

    Ok(())
}

#[specta::specta]
#[tauri::command]
pub fn open_log_dir(app: AppHandle) -> Result<(), String> {
    let log_dir = crate::portable::app_log_dir(&app)
        .map_err(|e| format!("Failed to get log directory: {}", e))?;

    let path = log_dir.to_string_lossy().as_ref().to_string();
    app.opener()
        .open_path(path, None::<String>)
        .map_err(|e| format!("Failed to open log directory: {}", e))?;

    Ok(())
}

#[specta::specta]
#[tauri::command]
pub fn open_app_data_dir(app: AppHandle) -> Result<(), String> {
    let app_data_dir = crate::portable::app_data_dir(&app)
        .map_err(|e| format!("Failed to get app data directory: {}", e))?;

    let path = app_data_dir.to_string_lossy().as_ref().to_string();
    app.opener()
        .open_path(path, None::<String>)
        .map_err(|e| format!("Failed to open app data directory: {}", e))?;

    Ok(())
}

/// Check if Apple Intelligence is available on this device.
/// Called by the frontend when the user selects Apple Intelligence provider.
#[specta::specta]
#[tauri::command]
pub fn check_apple_intelligence_available() -> bool {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        crate::apple_intelligence::check_apple_intelligence_availability()
    }
    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    {
        false
    }
}

/// Try to initialize Enigo (keyboard/mouse simulation).
/// On macOS, this will return an error if accessibility permissions are not granted.
#[specta::specta]
#[tauri::command]
pub fn initialize_enigo(app: AppHandle) -> Result<(), String> {
    use crate::input::EnigoState;

    // Check if already initialized
    if app.try_state::<EnigoState>().is_some() {
        log::debug!("Enigo already initialized");
        return Ok(());
    }

    // Try to initialize
    match EnigoState::new() {
        Ok(enigo_state) => {
            app.manage(enigo_state);
            log::info!("Enigo initialized successfully after permission grant");
            Ok(())
        }
        Err(e) => {
            if cfg!(target_os = "macos") {
                log::warn!(
                    "Failed to initialize Enigo: {} (accessibility permissions may not be granted)",
                    e
                );
            } else {
                log::warn!("Failed to initialize Enigo: {}", e);
            }
            Err(format!("Failed to initialize input system: {}", e))
        }
    }
}

/// Marker state to track if shortcuts have been initialized.
pub struct ShortcutsInitialized;

/// Initialize keyboard shortcuts.
/// On macOS, this should be called after accessibility permissions are granted.
/// This is idempotent - calling it multiple times is safe.
#[specta::specta]
#[tauri::command]
pub fn initialize_shortcuts(app: AppHandle) -> Result<(), String> {
    // Check if already initialized
    if app.try_state::<ShortcutsInitialized>().is_some() {
        log::debug!("Shortcuts already initialized");
        return Ok(());
    }

    // Initialize shortcuts
    crate::shortcut::init_shortcuts(&app);

    // Mark as initialized before reconciling the macOS Secure Input fallback.
    app.manage(ShortcutsInitialized);
    crate::secure_input::reconcile_fallback(&app);

    log::info!("Shortcuts initialized successfully");
    Ok(())
}
