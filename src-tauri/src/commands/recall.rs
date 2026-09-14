//! Tauri commands of the "Recall" page (fork feature).

use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;

use crate::recall::{self, RecallNoteContent, RecallNoteMeta, RecallVaultInfo, crypto, dictate};
use crate::settings::{RecallSettings, get_settings, write_settings};

fn vault_root_or_err(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let settings = get_settings(app).recall.normalized();
    recall::vault_root(app, &settings)
}

#[tauri::command]
#[specta::specta]
pub fn change_recall_settings(app: AppHandle, settings: RecallSettings) -> Result<(), String> {
    let mut current = get_settings(&app);
    current.recall = settings.normalized();
    write_settings(&app, current);
    // The session key belongs to whichever vault it was derived for; after
    // the vault folder changes, the user unlocks the new one explicitly.
    crypto::session_lock();
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn recall_vault_info(app: AppHandle) -> Result<RecallVaultInfo, String> {
    let root = vault_root_or_err(&app)?;
    recall::vault_info(&root)
}

/// The folder used when none is configured, for display.
#[tauri::command]
#[specta::specta]
pub fn recall_default_vault_dir(app: AppHandle) -> Result<String, String> {
    recall::default_vault_root(&app).map(|p| p.to_string_lossy().to_string())
}

#[tauri::command]
#[specta::specta]
pub fn recall_list_notes(app: AppHandle) -> Result<Vec<RecallNoteMeta>, String> {
    let root = vault_root_or_err(&app)?;
    Ok(recall::list_notes_any(&root)?.0)
}

#[tauri::command]
#[specta::specta]
pub fn recall_read_note(app: AppHandle, id: String) -> Result<RecallNoteContent, String> {
    let root = vault_root_or_err(&app)?;
    recall::read_note_any(&root, &id)
}

#[tauri::command]
#[specta::specta]
pub fn recall_create_note(
    app: AppHandle,
    title: String,
    tags: Vec<String>,
) -> Result<RecallNoteMeta, String> {
    let root = vault_root_or_err(&app)?;
    recall::create_note_any(&root, &title, &tags)
}

#[tauri::command]
#[specta::specta]
pub fn recall_write_note(
    app: AppHandle,
    id: String,
    title: String,
    tags: Vec<String>,
    body: String,
) -> Result<RecallNoteMeta, String> {
    let root = vault_root_or_err(&app)?;
    recall::write_note_any(&root, &id, &title, &tags, &body)
}

#[tauri::command]
#[specta::specta]
pub fn recall_delete_note(app: AppHandle, id: String) -> Result<(), String> {
    let root = vault_root_or_err(&app)?;
    recall::delete_note_any(&root, &id)
}

/// File a transcription (or post-processed text) as a new note — the
/// "Save to Recall" action on a history entry. The recording is copied into
/// the vault's `audio/` folder (encrypted when the vault is), so the vault
/// is self-contained.
#[tauri::command]
#[specta::specta]
pub fn recall_save_transcription(
    app: AppHandle,
    text: String,
    title: Option<String>,
    source: Option<String>,
    audio_file: Option<String>,
    tags: Vec<String>,
) -> Result<RecallNoteMeta, String> {
    if text.trim().is_empty() {
        return Err("Nothing to save: the text is empty".to_string());
    }
    let root = vault_root_or_err(&app)?;
    let audio_source = audio_file.as_ref().and_then(|file_name| {
        let path = app
            .state::<std::sync::Arc<crate::managers::history::HistoryManager>>()
            .get_audio_file_path(file_name);
        path.exists().then_some(path)
    });
    recall::save_transcription_any(
        &root,
        &text,
        title,
        source,
        audio_file,
        &tags,
        audio_source.as_deref(),
    )
}

#[tauri::command]
#[specta::specta]
pub fn recall_open_vault_folder(app: AppHandle) -> Result<(), String> {
    let root = vault_root_or_err(&app)?;
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    app.opener()
        .open_path(root.to_string_lossy().to_string(), None::<&str>)
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// vault encryption
// ---------------------------------------------------------------------------

#[tauri::command]
#[specta::specta]
pub fn recall_encryption_status(app: AppHandle) -> Result<crypto::RecallEncryptionStatus, String> {
    let root = vault_root_or_err(&app)?;
    Ok(crypto::RecallEncryptionStatus {
        enabled: crypto::is_encrypted(&root),
        unlocked: crypto::is_unlocked(),
    })
}

/// Turn encryption on: passphrase → key file + every note becomes `.rcl`.
/// A plaintext backup is written and reported in the reply.
#[tauri::command]
#[specta::specta]
pub fn recall_enable_encryption(
    app: AppHandle,
    passphrase: String,
) -> Result<crypto::EnableReport, String> {
    let root = vault_root_or_err(&app)?;
    crypto::enable(&root, &passphrase)
}

/// Verify the passphrase and hold the key in memory until locked or exit.
#[tauri::command]
#[specta::specta]
pub fn recall_unlock_vault(app: AppHandle, passphrase: String) -> Result<(), String> {
    let root = vault_root_or_err(&app)?;
    crypto::unlock(&root, &passphrase)
}

/// Drop the in-memory key. Notes stay encrypted on disk.
#[tauri::command]
#[specta::specta]
pub fn recall_lock_vault() -> Result<(), String> {
    crypto::session_lock();
    Ok(())
}

/// Turn encryption off: verify the passphrase, decrypt every note back to
/// plain Markdown, remove the key file, lock the session.
#[tauri::command]
#[specta::specta]
pub fn recall_disable_encryption(app: AppHandle, passphrase: String) -> Result<u32, String> {
    let root = vault_root_or_err(&app)?;
    crypto::disable(&root, &passphrase)
}

/// Start a dictate recording: audio accumulates, nothing is typed or pasted.
#[tauri::command]
#[specta::specta]
pub async fn recall_dictate_start(app: AppHandle) -> Result<(), String> {
    // Stream open can block (device I/O), so it never runs on the webview loop.
    tauri::async_runtime::spawn_blocking(move || dictate::start(&app))
        .await
        .map_err(|e| format!("Start task panicked: {e}"))?
}

/// Stop the dictate recording and transcribe it. The text comes back to the
/// caller — the page inserts it at the editor's caret.
#[tauri::command]
#[specta::specta]
pub async fn recall_dictate_stop(app: AppHandle) -> Result<String, String> {
    dictate::stop_and_transcribe(&app).await
}

/// Cancel the dictate recording and discard the take.
#[tauri::command]
#[specta::specta]
pub fn recall_dictate_cancel(app: AppHandle) -> Result<(), String> {
    dictate::cancel(&app)
}

/// Arm/disarm Recall insertion mode: while armed, the transcription and
/// Multi-STT hotkeys deliver their text to the Recall editor's caret
/// instead of pasting and writing a History row. The page arms it when the
/// editor has focus and disarms on blur.
#[tauri::command]
#[specta::specta]
pub fn recall_set_insertion_mode(enabled: bool) -> Result<(), String> {
    crate::recall::insertion::set_armed(enabled);
    Ok(())
}
