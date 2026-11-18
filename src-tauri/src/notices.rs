//! Notices: how the app reports a failure that must stay visible until it is
//! resolved or dismissed (upstream `b99b9af`, adapted).
//!
//! Upstream replaced every toast with them; here toasts stay what they were
//! (transient messages, `sessionToast`), and a notice is for what the user
//! cannot be trusted to have seen — a dictation fails while the settings window
//! is hidden, a file job fails while they work elsewhere, a model falls back to
//! the CPU. A notice badges the tray and names itself in its menu and tooltip,
//! is counted in the footer (the panel lists them), and pops up once as a toast
//! when the window is open.
//!
//! [`raise`] is the one way to report one; [`clear`] is called when the source
//! next succeeds. Each source holds one notice, the newest, so a source cannot
//! pile up a history. Adding a code: the variant here (with its level and its
//! tray title), `errors.<code>Title` (and an optional explanation) in
//! `en/translation.json` — `build.rs` copies every `errors.*Title` into the
//! tray strings — and its entry in `src/lib/notices.ts`.

use crate::tray;
use crate::tray_i18n::TrayStrings;
use log::warn;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::sync::{Mutex, MutexGuard, PoisonError};
use tauri::{AppHandle, Manager, State};
use tauri_specta::Event;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum NoticeLevel {
    /// Nothing came out, or the app cannot do its job.
    Error,
    /// The user got a result, but not the one they set up.
    Warning,
}

/// Each source has one slot in the store.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum NoticeSource {
    /// A dictation (the transcribe or Multi-STT hotkeys).
    Dictation,
    /// A Transcribe Files job.
    File,
    /// Loading a speech model, the primary or a Multi-STT extra.
    Model,
    /// A model running on CPU after its GPU backend failed.
    Gpu,
    /// The in-app llama.cpp server.
    #[serde(rename = "local_llm")]
    LocalLlm,
}

/// The frontend maps each code to its text in `src/lib/notices.ts`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
pub enum NoticeCode {
    TranscriptionFailed,
    PasteFailed,
    PasteBlockedElevated,
    NoMicrophone,
    MicPermissionDenied,
    RecordingFailed,
    MicrophoneSilent,
    NoModelSelected,
    ModelLoadFailed,
    PostProcessFailed,
    MergeFailed,
    FileTranscriptionFailed,
    GpuFallback,
    LlamaServerFailed,
}

impl NoticeCode {
    fn level(self) -> NoticeLevel {
        match self {
            Self::TranscriptionFailed
            | Self::PasteFailed
            | Self::PasteBlockedElevated
            | Self::NoMicrophone
            | Self::MicPermissionDenied
            | Self::RecordingFailed
            | Self::MicrophoneSilent
            | Self::NoModelSelected
            | Self::ModelLoadFailed
            | Self::FileTranscriptionFailed
            | Self::LlamaServerFailed => NoticeLevel::Error,
            Self::PostProcessFailed | Self::MergeFailed | Self::GpuFallback => NoticeLevel::Warning,
        }
    }

    /// The same `errors.*Title` string the frontend shows (see `build.rs`).
    pub fn tray_title(self, strings: &TrayStrings) -> &str {
        match self {
            Self::TranscriptionFailed => &strings.transcription_failed_title,
            Self::PasteFailed => &strings.paste_failed_title,
            Self::PasteBlockedElevated => &strings.paste_blocked_elevated_title,
            Self::NoMicrophone => &strings.no_input_device_title,
            Self::MicPermissionDenied => &strings.mic_permission_denied_title,
            Self::RecordingFailed => &strings.recording_failed_title,
            Self::NoModelSelected => &strings.no_model_selected_title,
            Self::MicrophoneSilent => &strings.microphone_silent_title,
            Self::ModelLoadFailed => &strings.model_load_failed_title,
            Self::PostProcessFailed => &strings.post_process_failed_title,
            Self::MergeFailed => &strings.merge_failed_title,
            Self::FileTranscriptionFailed => &strings.file_transcription_failed_title,
            Self::GpuFallback => &strings.gpu_fallback_title,
            Self::LlamaServerFailed => &strings.llama_server_failed_title,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Notice {
    pub code: NoticeCode,
    pub level: NoticeLevel,
    pub source: NoticeSource,
    /// What only Rust knows, such as a model name or a raw error.
    pub detail: Option<String>,
}

impl Notice {
    pub fn new(code: NoticeCode, source: NoticeSource) -> Self {
        Self {
            code,
            level: code.level(),
            source,
            detail: None,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<Option<String>>) -> Self {
        self.detail = detail.into();
        self
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct NoticesChanged {
    /// Every active notice, newest first.
    pub notices: Vec<Notice>,
    /// The notice just raised; `None` when one was cleared.
    pub raised: Option<Notice>,
}

/// The active notices, newest first.
#[derive(Default)]
pub struct NoticeStore(Mutex<Vec<Notice>>);

impl NoticeStore {
    fn lock(&self) -> MutexGuard<'_, Vec<Notice>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn insert(active: &mut Vec<Notice>, notice: Notice) {
    remove(active, notice.source, None);
    active.insert(0, notice);
}

/// Removes the source's notice, if it has `code` when one is given. Returns
/// whether one was removed.
fn remove(active: &mut Vec<Notice>, source: NoticeSource, code: Option<NoticeCode>) -> bool {
    let before = active.len();
    active.retain(|notice| notice.source != source || code.is_some_and(|code| notice.code != code));
    active.len() != before
}

/// The notice the tray names: the newest error, else the newest warning.
fn headline(active: &[Notice]) -> Option<&Notice> {
    active
        .iter()
        .find(|notice| notice.level == NoticeLevel::Error)
        .or(active.first())
}

/// Sent under the store's lock, so the last event always matches the store.
fn publish(app: &AppHandle, active: &[Notice], raised: Option<Notice>) {
    let event = NoticesChanged {
        notices: active.to_vec(),
        raised,
    };
    if let Err(e) = event.emit(app) {
        warn!("Failed to emit notices-changed: {e}");
    }
}

/// Replaces the source's previous notice. A no-op where no store is managed
/// (the headless `--transcribe-file` path), so failure sites need no guard.
pub fn raise(app: &AppHandle, notice: Notice) {
    let Some(store) = app.try_state::<NoticeStore>() else {
        return;
    };
    {
        let mut active = store.lock();
        if active.first() == Some(&notice) {
            // The same failure again: nothing changes, and no second toast.
            return;
        }
        insert(&mut active, notice.clone());
        publish(app, &active, Some(notice));
    }
    tray::refresh_tray_icon(app);
}

/// Call when `source` next succeeds, before raising anything about that run.
pub fn clear(app: &AppHandle, source: NoticeSource) {
    clear_matching(app, source, None);
}

/// Clears the source's notice, but only if it has `code` when one is given.
pub fn clear_matching(app: &AppHandle, source: NoticeSource, code: Option<NoticeCode>) {
    let Some(store) = app.try_state::<NoticeStore>() else {
        return;
    };
    let cleared = {
        let mut active = store.lock();
        let cleared = remove(&mut active, source, code);
        if cleared {
            publish(app, &active, None);
        }
        cleared
    };
    if cleared {
        tray::refresh_tray_icon(app);
    }
}

/// The notice the tray shows, if any.
pub fn tray_headline(app: &AppHandle) -> Option<Notice> {
    let store = app.try_state::<NoticeStore>()?;
    let active = store.lock();
    headline(&active).cloned()
}

#[tauri::command]
#[specta::specta]
pub fn get_active_notices(store: State<'_, NoticeStore>) -> Vec<Notice> {
    store.lock().clone()
}

/// The user is done with a notice. Matching the code leaves alone a newer
/// notice that replaced it after the user's view was drawn.
#[tauri::command]
#[specta::specta]
pub fn dismiss_notice(app: AppHandle, source: NoticeSource, code: NoticeCode) {
    clear_matching(&app, source, Some(code));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dictation(code: NoticeCode) -> Notice {
        Notice::new(code, NoticeSource::Dictation)
    }

    fn file(code: NoticeCode) -> Notice {
        Notice::new(code, NoticeSource::File)
    }

    #[test]
    fn each_source_keeps_only_its_newest_notice_newest_first() {
        let mut active = Vec::new();
        insert(&mut active, dictation(NoticeCode::PasteFailed));
        insert(&mut active, file(NoticeCode::FileTranscriptionFailed));
        insert(&mut active, dictation(NoticeCode::PostProcessFailed));

        assert_eq!(
            active,
            vec![
                dictation(NoticeCode::PostProcessFailed),
                file(NoticeCode::FileTranscriptionFailed),
            ]
        );
    }

    #[test]
    fn clearing_removes_only_that_source_and_only_a_matching_code() {
        let mut active = vec![
            dictation(NoticeCode::TranscriptionFailed),
            file(NoticeCode::FileTranscriptionFailed),
        ];
        // A dismissal drawn before a newer notice replaced it.
        assert!(!remove(
            &mut active,
            NoticeSource::Dictation,
            Some(NoticeCode::PasteFailed)
        ));
        assert_eq!(active.len(), 2);
        assert!(remove(&mut active, NoticeSource::Dictation, None));
        assert_eq!(active, vec![file(NoticeCode::FileTranscriptionFailed)]);
        assert!(!remove(&mut active, NoticeSource::Dictation, None));
    }

    #[test]
    fn the_headline_is_the_newest_error_before_any_warning() {
        let warning = Notice::new(NoticeCode::GpuFallback, NoticeSource::Gpu);
        let error = file(NoticeCode::FileTranscriptionFailed);
        assert_eq!(headline(&[]), None);
        assert_eq!(headline(std::slice::from_ref(&warning)), Some(&warning));
        assert_eq!(headline(&[warning.clone(), error.clone()]), Some(&error));
    }

    #[test]
    fn levels_follow_whether_a_result_came_out() {
        assert_eq!(NoticeCode::TranscriptionFailed.level(), NoticeLevel::Error);
        assert_eq!(NoticeCode::NoModelSelected.level(), NoticeLevel::Error);
        assert_eq!(NoticeCode::PostProcessFailed.level(), NoticeLevel::Warning);
        assert_eq!(NoticeCode::MergeFailed.level(), NoticeLevel::Warning);
        assert_eq!(NoticeCode::GpuFallback.level(), NoticeLevel::Warning);
    }

    #[test]
    fn the_source_serializes_as_the_frontend_spells_it() {
        assert_eq!(
            serde_json::to_string(&NoticeSource::LocalLlm).unwrap(),
            "\"local_llm\""
        );
        assert_eq!(
            serde_json::to_string(&NoticeSource::Dictation).unwrap(),
            "\"dictation\""
        );
    }
}
