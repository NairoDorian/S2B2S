//! Tauri commands of the "Transcribe Files" page.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::file_transcription::{FileTranscriptionManager, FileTranscriptionStatus};
use crate::settings::{FileTranscriptionSettings, get_settings, write_settings};

/// Largest text file `read_text_file` will return (transcripts are small; this
/// only guards against pointing the viewer at something huge).
const MAX_TEXT_FILE_BYTES: u64 = 8 * 1024 * 1024;

#[tauri::command]
#[specta::specta]
pub fn change_file_transcription_settings(
    app: AppHandle,
    settings: FileTranscriptionSettings,
) -> Result<(), String> {
    let mut current = get_settings(&app);
    current.file_transcription = settings.normalized();
    write_settings(&app, current);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn list_audio_files_in_folder(
    folder: String,
    include_subfolders: bool,
) -> Result<Vec<String>, String> {
    // A recursive walk of a large folder is disk I/O: keep it off the webview thread.
    tauri::async_runtime::spawn_blocking(move || {
        crate::file_transcription::list_audio_files(Path::new(&folder), include_subfolders)
            .map(|paths| {
                paths
                    .into_iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect()
            })
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("Folder scan task failed: {e}"))?
}

#[tauri::command]
#[specta::specta]
pub fn start_file_transcription(
    app: AppHandle,
    manager: State<'_, Arc<FileTranscriptionManager>>,
    paths: Vec<String>,
) -> Result<u32, String> {
    manager.inner().start(&app, paths)
}

#[tauri::command]
#[specta::specta]
pub fn cancel_file_transcription(manager: State<'_, Arc<FileTranscriptionManager>>) {
    manager.cancel();
}

#[tauri::command]
#[specta::specta]
pub fn get_file_transcription_status(
    manager: State<'_, Arc<FileTranscriptionManager>>,
) -> FileTranscriptionStatus {
    manager.status()
}

/// Show `path` in the OS file manager: folders are opened, files are revealed
/// (selected) inside their folder.
#[tauri::command]
#[specta::specta]
pub fn reveal_path_in_file_manager(app: AppHandle, path: String) -> Result<(), String> {
    let target = PathBuf::from(&path);
    if !target.exists() {
        return Err(format!("Path does not exist: {path}"));
    }
    if target.is_dir() {
        app.opener()
            .open_path(target.to_string_lossy().to_string(), None::<&str>)
            .map_err(|e| format!("Failed to open folder: {e}"))
    } else {
        app.opener()
            .reveal_item_in_dir(&target)
            .map_err(|e| format!("Failed to reveal file: {e}"))
    }
}

/// Read a UTF-8 text file (transcript viewer). Invalid UTF-8 is replaced
/// rather than rejected so a partially written live transcript still shows.
#[tauri::command]
#[specta::specta]
pub async fn read_text_file(path: String) -> Result<String, String> {
    // Up to `MAX_TEXT_FILE_BYTES` of disk I/O: keep it off the webview thread.
    tauri::async_runtime::spawn_blocking(move || {
        let target = PathBuf::from(&path);
        let meta = std::fs::metadata(&target).map_err(|e| format!("Cannot read {path}: {e}"))?;
        if !meta.is_file() {
            return Err(format!("Not a file: {path}"));
        }
        if meta.len() > MAX_TEXT_FILE_BYTES {
            return Err(format!(
                "File is too large to display ({} MB)",
                meta.len() / (1024 * 1024)
            ));
        }
        let bytes = std::fs::read(&target).map_err(|e| format!("Cannot read {path}: {e}"))?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    })
    .await
    .map_err(|e| format!("Read task failed: {e}"))?
}
