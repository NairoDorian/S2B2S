//! Portable mode support.
//!
//! When a file named `portable` exists next to the executable, all user data
//! (settings, models, recordings, database, logs) is stored in a `Data/`
//! directory alongside the executable instead of `%APPDATA%`.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tauri::Manager;

use crate::app_identity::{
    LEGACY_DATA_DIR_NAME, LEGACY_IDENTIFIER, LEGACY_PORTABLE_MARKER, PORTABLE_MARKER,
};

static PORTABLE_DATA_DIR: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Detect portable mode by looking for a `portable` marker file next to the exe.
/// Must be called once at startup before Tauri initializes.
pub fn init() {
    PORTABLE_DATA_DIR.get_or_init(|| {
        let exe_path = std::env::current_exe().ok()?;
        let exe_dir = exe_path.parent()?;

        let marker_path = exe_dir.join("portable");
        let data_dir = exe_dir.join("Data");

        let is_portable = if is_valid_portable_marker(&marker_path) {
            true
        } else if marker_path.exists() && data_dir.exists() {
            // Migration: v0.8.0 created an empty marker file. If we find an
            // empty/invalid marker alongside an existing Data/ dir, this is a
            // real portable install — upgrade the marker in place.
            eprintln!("[portable] upgrading legacy empty marker to magic string");
            let _ = std::fs::write(&marker_path, PORTABLE_MARKER);
            true
        } else {
            false
        };

        if is_portable {
            if !data_dir.exists() {
                std::fs::create_dir_all(&data_dir).ok()?;
            }
            let hf_home = hugging_face_home(&data_dir);
            // SAFETY: called once at startup, before any threads spawn; the
            // process is single-threaded at this point so set_var cannot race.
            unsafe { std::env::set_var("HF_HOME", &hf_home) };
            eprintln!("[portable] data dir: {}", data_dir.display());
            eprintln!("[portable] Hugging Face home: {}", hf_home.display());
            Some(data_dir)
        } else {
            None
        }
    });
}

/// Keep hf-hub downloads inside the portable data directory. hf-hub appends
/// its own `hub` component to `HF_HOME` for model snapshots and blobs.
fn hugging_face_home(data_dir: &Path) -> PathBuf {
    data_dir.join("huggingface")
}

/// Returns `true` if running in portable mode.
pub fn is_portable() -> bool {
    PORTABLE_DATA_DIR.get().and_then(|v| v.as_ref()).is_some()
}

/// Get the portable data dir (if active). Does not require an AppHandle.
/// Returns `None` when not in portable mode.
pub fn data_dir() -> Option<&'static PathBuf> {
    PORTABLE_DATA_DIR.get().and_then(|v| v.as_ref())
}

/// Portable-aware replacement for `app.path().app_data_dir()`.
pub fn app_data_dir(app: &tauri::AppHandle) -> Result<PathBuf, tauri::Error> {
    if let Some(dir) = data_dir() {
        Ok(dir.clone())
    } else {
        app.path().app_data_dir()
    }
}

/// Portable-aware replacement for `app.path().app_log_dir()`.
pub fn app_log_dir(app: &tauri::AppHandle) -> Result<PathBuf, tauri::Error> {
    if let Some(dir) = data_dir() {
        Ok(dir.join("logs"))
    } else {
        app.path().app_log_dir()
    }
}

/// Resolve a relative path against the app data directory (portable-aware).
/// Replaces `app.path().resolve(path, BaseDirectory::AppData)`.
pub fn resolve_app_data(app: &tauri::AppHandle, relative: &str) -> Result<PathBuf, tauri::Error> {
    Ok(app_data_dir(app)?.join(relative))
}

/// Get the path to use with `tauri-plugin-store`.
/// Returns an absolute path in portable mode (so the store plugin writes to
/// the portable Data dir) or the original relative path otherwise.
pub fn store_path(relative: &str) -> PathBuf {
    if let Some(dir) = data_dir() {
        dir.join(relative)
    } else {
        PathBuf::from(relative)
    }
}

/// Move a pre-rename install's data to where 0.9.7 and later look for it.
///
/// The fork renamed its bundle identifier from `com.pais.handy` to
/// `com.nairodorian.zer0`, and Tauri derives the app data, cache and log
/// directories from that identifier — so a straight rename would leave every
/// existing user with a fresh, empty profile: no models (gigabytes to
/// re-download), no transcription history, no settings.
///
/// This runs once at startup, before settings are read, and is deliberately
/// conservative: it moves a directory only when the new one does not exist at
/// all and the old one does. If both exist the user has already run 0.9.7 and
/// made changes, so nothing is touched and the old directory is left as the
/// backup it has become.
///
/// Never runs in portable mode, where all of this lives in `Data/` beside the
/// executable and the identifier is not involved.
pub fn migrate_legacy_app_data(app: &tauri::AppHandle) {
    if is_portable() {
        return;
    }

    let paths = app.path();
    let dirs: [(&str, Result<PathBuf, tauri::Error>); 3] = [
        ("app data", paths.app_data_dir()),
        ("cache", paths.app_cache_dir()),
        ("logs", paths.app_log_dir()),
    ];

    for (label, new_dir) in dirs {
        let Ok(new_dir) = new_dir else { continue };
        let Some(parent) = new_dir.parent() else {
            continue;
        };
        let Some(new_leaf) = new_dir.file_name().and_then(|n| n.to_str()) else {
            continue;
        };

        // The rename changed the leaf on Windows and macOS (the directory is
        // named after the identifier) but not necessarily elsewhere, where
        // Tauri may use the product name. Try both former spellings.
        for legacy_leaf in [LEGACY_IDENTIFIER, LEGACY_DATA_DIR_NAME] {
            if legacy_leaf == new_leaf {
                continue;
            }
            let old_dir = parent.join(legacy_leaf);
            if !old_dir.is_dir() || new_dir.exists() {
                continue;
            }
            match std::fs::rename(&old_dir, &new_dir) {
                Ok(()) => log::info!(
                    "migrated {label} directory: {} -> {}",
                    old_dir.display(),
                    new_dir.display()
                ),
                Err(error) => log::warn!(
                    "could not migrate the {label} directory from {} to {}: {error}. \
                     Models, history and settings stay where they are; move the \
                     directory by hand to keep them.",
                    old_dir.display(),
                    new_dir.display()
                ),
            }
            // One legacy spelling wins even if the other also exists: a second
            // rename would land on the directory just created.
            break;
        }
    }
}

/// Check if a marker file path contains the portable magic string.
/// Extracted for testability.
///
/// Both markers are accepted: the current one, and the one 0.9.6 and earlier
/// wrote. A marker this rejects silently sends a portable install back to
/// `%APPDATA%`, where the user's models and history are not — so accepting the
/// old spelling is what lets an existing portable install survive the rename
/// with its `Data/` directory intact. Only the current marker is ever written.
fn is_valid_portable_marker(path: &std::path::Path) -> bool {
    std::fs::read_to_string(path)
        .map(|s| {
            let s = s.trim();
            s.starts_with(PORTABLE_MARKER) || s.starts_with(LEGACY_PORTABLE_MARKER)
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::temp_test_dir;
    use std::io::Write;

    /// A fresh temp dir per call — see `utils::temp_test_dir`.
    fn scratch() -> PathBuf {
        temp_test_dir("portable-test")
    }

    /// Write `contents` to `<dir>/portable` and report whether the reader
    /// accepts it.
    fn marker_accepts(contents: &str) -> bool {
        let dir = scratch();
        let marker = dir.join("portable");
        let mut file = std::fs::File::create(&marker).unwrap();
        write!(file, "{contents}").unwrap();
        let accepted = is_valid_portable_marker(&marker);
        std::fs::remove_dir_all(dir).unwrap();
        accepted
    }

    #[test]
    fn current_marker_enables_portable() {
        assert!(marker_accepts(PORTABLE_MARKER));
    }

    #[test]
    fn legacy_marker_still_enables_portable() {
        // The rename must not move an existing portable install's Data/ dir.
        assert!(marker_accepts(LEGACY_PORTABLE_MARKER));
    }

    #[test]
    fn marker_with_surrounding_whitespace_enables_portable() {
        assert!(marker_accepts(&format!("  {PORTABLE_MARKER}\n")));
        assert!(marker_accepts(&format!("\n{LEGACY_PORTABLE_MARKER}  ")));
    }

    #[test]
    fn marker_with_a_trailing_line_still_enables_portable() {
        // The check is `starts_with`, so extra lines are tolerated. Pinned
        // because the NSIS template writes the marker through a different code
        // path than this module and the two must stay compatible.
        assert!(marker_accepts(&format!("{PORTABLE_MARKER}\n")));
    }

    #[test]
    fn empty_marker_does_not_enable_portable() {
        assert!(!marker_accepts(""));
    }

    #[test]
    fn unrelated_content_does_not_enable_portable() {
        assert!(!marker_accepts("some other content"));
        // A marker from a different application must not claim this install.
        assert!(!marker_accepts("OtherApp Portable Mode"));
    }

    #[test]
    fn missing_marker_file_does_not_enable_portable() {
        let dir = scratch();
        assert!(!is_valid_portable_marker(&dir.join("portable")));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn hugging_face_home_is_inside_portable_data() {
        let data_dir = Path::new("portable-root").join("Data");

        assert_eq!(hugging_face_home(&data_dir), data_dir.join("huggingface"));
    }
}
