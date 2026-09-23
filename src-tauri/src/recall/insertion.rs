//! Recall insertion mode (fork feature).
//!
//! When the Recall page's editor is focused, the transcription hotkeys stop
//! being "paste into whatever has focus + write a history row" and become
//! "insert into the note at the caret". The text travels to the webview as
//! [`RecallInsertTextEvent`]; nothing is pasted, nothing is typed, no
//! history row is written and the recording moves into the vault's
//! `audio/` ([`absorb_recording`]) — the note the caret sits in *is* the
//! destination, and its content lives in the vault only.
//!
//! The flag is armed by the frontend on editor focus and disarmed on blur,
//! so an ordinary dictation anywhere else behaves exactly as always.

use serde::{Deserialize, Serialize};
use specta::Type;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::AppHandle;
use tauri_specta::Event;

use log::warn;

/// Final text for the Recall editor's caret. Replaces the paste step.
#[derive(Clone, Debug, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct RecallInsertTextEvent {
    pub text: String,
}

static ARMED: AtomicBool = AtomicBool::new(false);

pub fn set_armed(armed: bool) {
    ARMED.store(armed, Ordering::Release);
}

pub fn is_armed() -> bool {
    ARMED.load(Ordering::Acquire)
}

pub fn emit_insert(app: &AppHandle, text: String) -> Result<(), String> {
    RecallInsertTextEvent { text }
        .emit(app)
        .map_err(|e| e.to_string())
}

/// A recording made while insertion mode was armed belongs to the vault,
/// not to History: move it out of the recordings folder into the vault's
/// `audio/`, encrypted when the vault is and the session holds the key.
/// A take that can neither be moved nor encrypted is discarded — the vault
/// is the only destination this recording has.
pub fn absorb_recording(app: &AppHandle, wav_path: &Path, saved: bool) {
    if !saved || !wav_path.exists() {
        return;
    }
    let discard = |why: &str| {
        warn!("Recall insert: {why}; discarding {}", wav_path.display());
        let _ = fs::remove_file(wav_path);
    };
    let settings = crate::settings::get_settings(app);
    let root = match super::vault_root(app, &settings.recall.normalized()) {
        Ok(root) => root,
        Err(e) => return discard(&format!("Cannot resolve vault: {e}")),
    };
    let Some(name) = wav_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
    else {
        return discard("Unusable recording file name");
    };
    let audio_dir = root.join(super::AUDIO_SUBDIR);
    // Both branches need it: enabling encryption never creates `audio/`.
    if let Err(e) = fs::create_dir_all(&audio_dir) {
        return discard(&format!("Failed to create audio folder: {e}"));
    }
    if super::crypto::is_encrypted(&root) {
        match super::crypto::session_key() {
            Ok(key) => {
                let bytes = match fs::read(wav_path) {
                    Ok(bytes) => bytes,
                    Err(e) => return discard(&format!("Failed to read recording: {e}")),
                };
                if let Err(e) = super::crypto::write_encrypted_file(
                    &audio_dir.join(format!("{name}.rcl")),
                    &key,
                    &bytes,
                ) {
                    return discard(&format!("Failed to encrypt recording: {e}"));
                }
            }
            Err(_) => return discard("Vault is locked"),
        }
    } else {
        let dest = audio_dir.join(&name);
        // A rename cannot cross volumes (a vault on another drive): fall back
        // to a copy, and the source is removed below like the encrypted one.
        if fs::rename(wav_path, &dest).is_err()
            && let Err(e) = fs::copy(wav_path, &dest)
        {
            return discard(&format!("Failed to move recording: {e}"));
        }
    }
    // Gone already after a successful rename; otherwise this is the copy the
    // vault now holds a version of.
    if wav_path.exists()
        && let Err(e) = fs::remove_file(wav_path)
    {
        warn!(
            "Recall insert: failed to remove recording copy {}: {e}",
            wav_path.display()
        );
    }
}
