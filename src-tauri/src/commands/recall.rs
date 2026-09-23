//! Tauri commands of the "Recall" page (fork feature).
//!
//! Every command that touches the vault runs on the blocking pool (`vault`):
//! unlocking runs Argon2id over 64 MiB, and toggling encryption rewrites every
//! note and recording, none of which may stall the webview. Those commands are
//! also serialised by one process-wide lock (`recall::lock_vault`, which
//! insertion mode's recording hand-off takes too). They used to run one at a
//! time on the main thread, and enabling or disabling encryption is a multi-step
//! rewrite that a concurrent save, unlock or folder change must not interleave
//! with (a note written mid-disable would stay encrypted after the key file is
//! gone). Dictation only records and transcribes, so it takes a plain
//! `spawn_blocking` and never waits behind a long toggle.

use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;

use crate::recall::{self, RecallNoteContent, RecallNoteMeta, RecallVaultInfo, crypto, dictate};
use crate::settings::{RecallSettings, get_settings, write_settings};

fn vault_root_or_err(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let settings = get_settings(app).recall.normalized();
    recall::vault_root(app, &settings)
}

/// Run `task` on the blocking pool and flatten its result.
async fn blocking<T: Send + 'static>(
    task: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(task)
        .await
        .map_err(|e| format!("Recall task panicked: {e}"))?
}

/// Run a vault operation on the blocking pool, one at a time.
async fn vault<T: Send + 'static>(
    task: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    blocking(move || {
        let _guard = recall::lock_vault();
        task()
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn change_recall_settings(
    app: AppHandle,
    settings: RecallSettings,
) -> Result<(), String> {
    vault(move || {
        let mut current = get_settings(&app);
        let new = settings.normalized();
        let folder_changed = current.recall.normalized().output_dir != new.output_dir;
        current.recall = new;
        write_settings(&app, current);
        // The session key belongs to whichever vault it was derived for; after
        // the vault folder changes, the user unlocks the new one explicitly.
        if folder_changed {
            crypto::session_lock();
        }
        Ok(())
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn recall_vault_info(app: AppHandle) -> Result<RecallVaultInfo, String> {
    vault(move || recall::vault_info(&vault_root_or_err(&app)?)).await
}

/// The folder used when none is configured, for display.
#[tauri::command]
#[specta::specta]
pub fn recall_default_vault_dir(app: AppHandle) -> Result<String, String> {
    recall::default_vault_root(&app).map(|p| p.to_string_lossy().to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn recall_list_notes(app: AppHandle) -> Result<Vec<RecallNoteMeta>, String> {
    vault(move || Ok(recall::list_notes_any(&vault_root_or_err(&app)?)?.0)).await
}

#[tauri::command]
#[specta::specta]
pub async fn recall_read_note(app: AppHandle, id: String) -> Result<RecallNoteContent, String> {
    vault(move || recall::read_note_any(&vault_root_or_err(&app)?, &id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn recall_create_note(
    app: AppHandle,
    title: String,
    tags: Vec<String>,
) -> Result<RecallNoteMeta, String> {
    vault(move || recall::create_note_any(&vault_root_or_err(&app)?, &title, &tags)).await
}

#[tauri::command]
#[specta::specta]
pub async fn recall_write_note(
    app: AppHandle,
    id: String,
    title: String,
    tags: Vec<String>,
    body: String,
) -> Result<RecallNoteMeta, String> {
    vault(move || recall::write_note_any(&vault_root_or_err(&app)?, &id, &title, &tags, &body))
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn recall_delete_note(app: AppHandle, id: String) -> Result<(), String> {
    vault(move || recall::delete_note_any(&vault_root_or_err(&app)?, &id)).await
}

/// File a transcription (or post-processed text) as a new note — the
/// "Save to Recall" action on a history entry. The recording is copied into
/// the vault's `audio/` folder (encrypted when the vault is), so the vault
/// is self-contained.
#[tauri::command]
#[specta::specta]
pub async fn recall_save_transcription(
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
    vault(move || {
        let root = vault_root_or_err(&app)?;
        // A bare file name only: the name comes from the webview, and a path
        // (`..\x`, an absolute one) would copy a file from outside the
        // recordings folder into the vault.
        let audio_source = audio_file.as_ref().and_then(|file_name| {
            let is_bare_name = std::path::Path::new(file_name)
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n == file_name);
            if !is_bare_name {
                return None;
            }
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
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn recall_open_vault_folder(app: AppHandle) -> Result<(), String> {
    vault(move || {
        let root = vault_root_or_err(&app)?;
        std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        app.opener()
            .open_path(root.to_string_lossy().to_string(), None::<&str>)
            .map_err(|e| e.to_string())
    })
    .await
}

// ---------------------------------------------------------------------------
// vault encryption
// ---------------------------------------------------------------------------

#[tauri::command]
#[specta::specta]
pub async fn recall_encryption_status(
    app: AppHandle,
) -> Result<crypto::RecallEncryptionStatus, String> {
    vault(move || {
        let root = vault_root_or_err(&app)?;
        Ok(crypto::RecallEncryptionStatus {
            enabled: crypto::is_encrypted(&root),
            unlocked: crypto::is_unlocked(),
        })
    })
    .await
}

/// Turn encryption on: passphrase → key file + every note becomes `.rcl`.
/// A plaintext backup is written and reported in the reply.
#[tauri::command]
#[specta::specta]
pub async fn recall_enable_encryption(
    app: AppHandle,
    passphrase: String,
) -> Result<crypto::EnableReport, String> {
    vault(move || crypto::enable(&vault_root_or_err(&app)?, &passphrase)).await
}

/// Verify the passphrase and hold the key in memory until locked or exit.
#[tauri::command]
#[specta::specta]
pub async fn recall_unlock_vault(app: AppHandle, passphrase: String) -> Result<(), String> {
    vault(move || crypto::unlock(&vault_root_or_err(&app)?, &passphrase)).await
}

/// Drop the in-memory key. Notes stay encrypted on disk.
#[tauri::command]
#[specta::specta]
pub async fn recall_lock_vault() -> Result<(), String> {
    vault(|| {
        crypto::session_lock();
        Ok(())
    })
    .await
}

/// Turn encryption off: verify the passphrase, decrypt every note back to
/// plain Markdown, remove the key file, lock the session.
#[tauri::command]
#[specta::specta]
pub async fn recall_disable_encryption(app: AppHandle, passphrase: String) -> Result<u32, String> {
    vault(move || crypto::disable(&vault_root_or_err(&app)?, &passphrase)).await
}

/// Start a dictate recording: audio accumulates, nothing is typed or pasted.
#[tauri::command]
#[specta::specta]
pub async fn recall_dictate_start(app: AppHandle) -> Result<(), String> {
    // Stream open can block (device I/O), so it never runs on the webview loop.
    blocking(move || dictate::start(&app)).await
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
pub async fn recall_dictate_cancel(app: AppHandle) -> Result<(), String> {
    // Stopping the recorder takes the audio manager's lock: off the webview loop.
    blocking(move || dictate::cancel(&app)).await
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
