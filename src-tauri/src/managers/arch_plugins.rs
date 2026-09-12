//! Architecture plugin management for transcribe.cpp.
//!
//! In a build with the `arch-dl` feature (Windows x86_64 and Linux — see
//! `src-tauri/Cargo.toml`) libtranscribe carries **no** model architecture of
//! its own: each family is a loadable `transcribe-arch-<family>.dll` / `.so` /
//! `.dylib` that the library resolves at model-open time. `TRANSCRIBE_MODEL_SET`
//! picks which families get built — `minimal-multilingual` (parakeet, granite,
//! qwen3_asr) for everyday development, `full` for a distribution build. See
//! `scripts/tauri-runner.ts` and `.cargo/config.toml`.
//!
//! **This module adds no loading of its own to the normal path.** The library
//! already searches, in order, the model file's directory and its `arch/`
//! subdirectory, every directory registered here, `$TRANSCRIBE_ARCH_DIR`, and
//! the directory holding libtranscribe. `cmake --install` puts the plugins
//! beside libtranscribe and Handy's `build.rs` stages them next to `handy.exe`,
//! which is the last of those — so a stock install opens every model with no
//! help from this file.
//!
//! What is left here is for the layouts that search cannot cover: letting a user
//! drop a plugin into a Handy-owned folder ([`init_arch_plugin_dirs`],
//! `register_arch_dir`), and reporting what is installed ([`list_arch_plugins`]).
//!
//! A model whose family is not in the build's set fails to load with
//! "unsupported architecture"; the two `Model::load_with` call sites in
//! `managers/transcription.rs` turn that into a message naming the plugin
//! directory. That is the intended failure for the minimal preset.

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::HashSet;
use std::ffi::CString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::AppHandle;

// The C entry points are declared here rather than used through
// `transcribe_cpp::{register_arch_dir, load_arch_plugin}` because those are
// `#[cfg(feature = "arch-dl")]`-gated: they do not exist in a macOS or Windows
// aarch64 build, which is static and compiles every architecture in. These two
// symbols, by contrast, are defined unconditionally in `transcribe-arch.cpp` in
// every configuration — on a static build `register_arch_dir` simply adds a
// directory the loader will never consult. Declaring them keeps one code path
// on every platform and posture.
unsafe extern "C" {
    pub fn transcribe_register_arch_dir(dir: *const std::os::raw::c_char) -> std::os::raw::c_int;
    pub fn transcribe_load_arch_plugin(path: *const std::os::raw::c_char) -> std::os::raw::c_int;
}

/// Plugin modules this module asked libtranscribe to load explicitly.
///
/// Not the whole truth about what is loaded: the library also auto-discovers
/// plugins from its own search path at model-open time, and the C API exposes no
/// way to enumerate those. Treat a `false` here as "Handy did not have to load
/// it", not as "it is not loaded".
static EXPLICITLY_LOADED: Lazy<Mutex<HashSet<String>>> = Lazy::new(|| Mutex::new(HashSet::new()));
static REGISTERED_DIRS: Lazy<Mutex<Vec<PathBuf>>> = Lazy::new(|| Mutex::new(Vec::new()));

/// Metadata describing an architecture plugin module.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ArchPluginInfo {
    /// Architecture identifier (e.g. "parakeet", "granite", "qwen3_asr", "whisper").
    pub name: String,
    /// Absolute filesystem path to the plugin module.
    pub path: String,
    /// Whether Handy explicitly loaded this module (see `EXPLICITLY_LOADED`).
    pub is_loaded: bool,
    /// File size in bytes.
    pub file_size_bytes: Option<u64>,
}

/// Encode a path for the C API the way the rest of the bindings do: raw bytes on
/// Unix, and a hard error on Windows for a path that is not representable as
/// UTF-8. Deliberately not `to_string_lossy()` — that substitutes U+FFFD for
/// anything it cannot encode, so the C side would be handed a path that is not
/// the one on disk and would report a missing plugin rather than a bad path.
fn path_to_cstring(path: &Path) -> Result<CString, String> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        CString::new(path.as_os_str().as_bytes())
            .map_err(|e| format!("path contains a NUL byte: {e}"))
    }
    #[cfg(not(unix))]
    {
        let s = path
            .to_str()
            .ok_or_else(|| format!("path is not valid UTF-8: {}", path.display()))?;
        CString::new(s).map_err(|e| format!("path contains a NUL byte: {e}"))
    }
}

/// Extract the architecture name from a plugin filename.
/// E.g. "transcribe-arch-parakeet.dll" -> "parakeet",
/// "libtranscribe-arch-whisper.so" -> "whisper".
pub fn extract_arch_from_filename(filename: &str) -> String {
    let mut s = filename;
    if let Some(stripped) = s.strip_prefix("lib") {
        s = stripped;
    }
    if let Some(stripped) = s.strip_prefix("transcribe-arch-") {
        s = stripped;
    }
    if let Some((arch, _)) = s.split_once('.') {
        s = arch;
    }
    s.to_string()
}

/// True for a filename that looks like an architecture plugin module.
fn is_plugin_filename(name: &str) -> bool {
    (name.starts_with("transcribe-arch-") || name.starts_with("libtranscribe-arch-"))
        && (name.ends_with(".dll") || name.ends_with(".so") || name.ends_with(".dylib"))
}

/// Register a directory the architecture loader will search.
///
/// A missing directory is not an error — there is simply nothing to register —
/// so callers can offer a list of candidate locations without checking each one.
pub fn register_arch_dir<P: AsRef<Path>>(dir: P) -> Result<(), String> {
    let dir = dir.as_ref();
    if !dir.exists() {
        return Ok(());
    }
    let c_str = path_to_cstring(dir)?;
    let status = unsafe { transcribe_register_arch_dir(c_str.as_ptr()) };
    if status != 0 {
        return Err(format!(
            "transcribe_register_arch_dir failed with status code {status} for {}",
            dir.display()
        ));
    }
    let mut dirs = REGISTERED_DIRS.lock().unwrap();
    if !dirs.iter().any(|d| d == dir) {
        dirs.push(dir.to_path_buf());
    }
    log::info!(
        "Registered architecture plugin directory: {}",
        dir.display()
    );
    Ok(())
}

/// Ask libtranscribe to load one architecture plugin module from `path`.
pub fn load_arch_plugin<P: AsRef<Path>>(path: P) -> Result<(), String> {
    let path = path.as_ref();
    if !path.is_file() {
        return Err(format!(
            "Architecture plugin file does not exist: {}",
            path.display()
        ));
    }
    let c_str = path_to_cstring(path)?;
    let status = unsafe { transcribe_load_arch_plugin(c_str.as_ptr()) };
    if status != 0 {
        return Err(format!(
            "transcribe_load_arch_plugin failed with status code {status} for {}",
            path.display()
        ));
    }
    let arch_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .map(extract_arch_from_filename)
        .unwrap_or_default();
    EXPLICITLY_LOADED.lock().unwrap().insert(arch_name.clone());
    log::info!(
        "Loaded architecture plugin '{}' from {}",
        arch_name,
        path.display()
    );
    Ok(())
}

/// Register the standard architecture plugin search directories.
///
/// Nothing is loaded here. The library resolves a plugin lazily, when a model
/// that needs it is opened, so pre-loading every module in every directory would
/// pay a `LoadLibrary` plus each plugin's static initializers at startup for
/// models the user may never open — and with the full preset that is 18 modules.
/// Registration is enough: the directories below join the loader's search path.
///
/// Returns the directories that were registered, in registration order.
pub fn init_arch_plugin_dirs(app_handle: &AppHandle) -> Vec<PathBuf> {
    let mut candidate_dirs = Vec::new();

    if let Ok(app_data) = crate::portable::app_data_dir(app_handle) {
        let plugins_dir = app_data.join("plugins");
        let models_dir = app_data.join("models");

        // Created on demand: this is the folder the Models page opens, so it
        // has to exist for the user to have somewhere to drop a plugin.
        let _ = fs::create_dir_all(&plugins_dir);

        candidate_dirs.push(plugins_dir);
        candidate_dirs.push(app_data.join("arch"));
        candidate_dirs.push(models_dir.clone());
        candidate_dirs.push(models_dir.join("arch"));
    }

    if let Ok(exe_path) = std::env::current_exe()
        && let Some(exe_dir) = exe_path.parent()
    {
        // Where build.rs stages the shipped plugins, and where `cmake --install`
        // puts them in a packaged build.
        candidate_dirs.push(exe_dir.to_path_buf());
        candidate_dirs.push(exe_dir.join("plugins"));
        candidate_dirs.push(exe_dir.join("arch"));
    }

    let mut registered = Vec::new();
    for dir in candidate_dirs {
        if dir.exists() && register_arch_dir(&dir).is_ok() {
            registered.push(dir);
        }
    }
    registered
}

/// The architecture plugin modules currently installed, sorted by name.
///
/// Scans the registered directories for plugin modules. This is what the Models
/// page reports, so in a `minimal-multilingual` build it lists three and in a
/// `full` build eighteen — the count is read off disk, not from a hardcoded list
/// of what a build is assumed to contain.
pub fn list_arch_plugins(app_handle: &AppHandle) -> Vec<ArchPluginInfo> {
    // The UI can ask before anything has loaded a model, so make sure the
    // standard directories have been registered.
    let dirs = {
        let dirs = REGISTERED_DIRS.lock().unwrap();
        if dirs.is_empty() {
            drop(dirs);
            init_arch_plugin_dirs(app_handle)
        } else {
            dirs.clone()
        }
    };

    let loaded = EXPLICITLY_LOADED.lock().unwrap().clone();
    let mut plugins = Vec::new();
    // A plugin can be reachable through more than one registered directory (the
    // exe dir and its `arch/` subdirectory, say). Identity is the canonical
    // path, matching how the loader dedups its own load list.
    let mut seen = HashSet::new();

    for dir in dirs {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !is_plugin_filename(name) {
                continue;
            }

            let canonical = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
            if !seen.insert(canonical) {
                continue;
            }

            let arch_name = extract_arch_from_filename(name);
            plugins.push(ArchPluginInfo {
                is_loaded: loaded.contains(&arch_name),
                name: arch_name,
                path: path.to_string_lossy().to_string(),
                file_size_bytes: entry.metadata().ok().map(|m| m.len()),
            });
        }
    }

    plugins.sort_by(|a, b| a.name.cmp(&b.name));
    plugins
}

/// Make sure a plugin for `arch` is loaded before a model of that architecture
/// is opened, when one is installed.
///
/// A convenience fast path, not a requirement: the library searches the
/// registered directories itself, so a plugin this does not find is still found
/// at model-open time. Finding nothing is therefore not an error — the model may
/// be served by a compiled-in architecture (a static build, or the `full` set)
/// — and the load that follows produces the accurate error if it is not.
pub fn ensure_arch_plugin_for_model(arch: &str, app_handle: &AppHandle) -> Result<(), String> {
    let arch_lower = arch.to_ascii_lowercase();
    if EXPLICITLY_LOADED.lock().unwrap().contains(&arch_lower) {
        return Ok(());
    }

    let dirs = {
        let registered = REGISTERED_DIRS.lock().unwrap();
        if registered.is_empty() {
            drop(registered);
            // Registration is what puts the exe directory in the loader's search
            // path, and that is where the shipped plugins live.
            init_arch_plugin_dirs(app_handle)
        } else {
            registered.clone()
        }
    };

    for dir in dirs {
        // Extension-agnostic: the same directory can hold `.dll`, `.so` or
        // `.dylib` depending on how it was populated.
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !is_plugin_filename(name) || extract_arch_from_filename(name) != arch_lower {
                continue;
            }
            log::info!(
                "Loading architecture plugin for '{}' from {}",
                arch_lower,
                path.display()
            );
            return load_arch_plugin(&path);
        }
    }

    Ok(())
}
