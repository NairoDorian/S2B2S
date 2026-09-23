//! Recall dictate: record, transcribe, *return* the text.
//!
//! The Recall page's editor hosts a dictate button: start records the
//! microphone exactly like a dictation would, stop batch-transcribes the
//! captured samples with the primary model (post-processing applied when
//! the user has it enabled) and hands the text back to the webview so the
//! page can insert it at the editor's caret. Nothing is typed or pasted
//! anywhere, and no history row is written — the note the text lands in
//! *is* the destination.
//!
//! Modeled on `overlay_preview`: static session state, the same VAD policy
//! a dictation with this model would use, and the cancel hotkey ends the
//! session through `cancel_current_operation` (which calls [`cancel`]
//! below). The transcription hotkeys get "Already recording" for the
//! duration, as with every other live-consumer session.

use log::{info, warn};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager};

use crate::audio_toolkit::VadPolicy;
use crate::managers::audio::{AudioRecordingManager, RecordingStartOptions};
use crate::managers::transcription::TranscriptionManager;
use crate::settings::get_settings;
use crate::tray::{TrayIconState, set_tray_state};

/// Binding id the dictate session records under (shows up in logs).
pub const DICTATE_BINDING: &str = "recall_dictate";

/// Below this much measured speech the take is refused, not transcribed —
/// the same guard every paste path uses, since a silent decode would only
/// invite a model hallucination into the note.
const MIN_SPEECH_MS: u64 = crate::actions::MIN_SPEECH_MS_TO_TRANSCRIBE;

static DICTATE_ACTIVE: AtomicBool = AtomicBool::new(false);
/// The cancel generation captured at start, so a hotkey cancel during the
/// stop path's extra-buffer wait is honored by `stop_recording`.
static DICTATE_CANCEL_GEN: Mutex<Option<u64>> = Mutex::new(None);

/// Start the dictate recording. Fails while anything else records.
pub fn start(app: &AppHandle) -> Result<(), String> {
    if DICTATE_ACTIVE.swap(true, Ordering::AcqRel) {
        return Err("Dictation is already running".to_string());
    }

    let rm = app.state::<Arc<AudioRecordingManager>>();
    if rm.is_recording() {
        DICTATE_ACTIVE.store(false, Ordering::Release);
        return Err("Already recording".to_string());
    }

    let tm = app.state::<Arc<TranscriptionManager>>();
    let settings = get_settings(app);
    let model_supports_streaming = app
        .state::<Arc<crate::managers::model::ModelManager>>()
        .get_model_info(&settings.selected_model)
        .as_ref()
        .map(|m| m.supports_streaming)
        .unwrap_or(false);

    let vad_policy = if !settings.vad_enabled {
        VadPolicy::Disabled
    } else if model_supports_streaming {
        VadPolicy::Streaming
    } else {
        VadPolicy::Offline
    };

    tm.initiate_model_load();

    match rm.try_start_recording_with_options(
        DICTATE_BINDING,
        vad_policy,
        RecordingStartOptions {
            capture_raw_override: Some(false),
            discard_audio: false,
        },
    ) {
        Ok(readiness) => {
            *DICTATE_CANCEL_GEN.lock().unwrap() = Some(rm.cancel_generation());

            // First real microphone samples: the same cue a dictation gives,
            // guarded by the readiness generation.
            crate::actions::spawn_recording_ready_cue(app, &rm, readiness);

            crate::shortcut::register_cancel_shortcut(app);
            set_tray_state(app, TrayIconState::Recording);

            info!("Recall dictate started");
            Ok(())
        }
        Err(e) => {
            DICTATE_ACTIVE.store(false, Ordering::Release);
            warn!("Recall dictate failed to start: {e}");
            Err(e)
        }
    }
}

/// End the session and discard the take — the cancel hotkey's path.
/// Idempotent, like `overlay_preview::stop`.
pub fn cancel(app: &AppHandle) -> Result<(), String> {
    if !DICTATE_ACTIVE.swap(false, Ordering::AcqRel) {
        return Ok(());
    }
    app.state::<Arc<AudioRecordingManager>>()
        .cancel_recording_if_binding(DICTATE_BINDING);
    finish_session(app);
    unload_if_immediate(app);
    info!("Recall dictate cancelled");
    Ok(())
}

/// Shared teardown once the recording is over, however it ended.
fn finish_session(app: &AppHandle) {
    *DICTATE_CANCEL_GEN.lock().unwrap() = None;
    crate::shortcut::unregister_cancel_shortcut(app);
    crate::overlay::hide_recording_overlay(app);
    set_tray_state(app, TrayIconState::Idle);
}

/// Honor an "Immediately" unload timeout on the paths that end without a
/// completed decode. A take that is transcribed is unloaded by
/// `transcribe()` itself, after the decode — unloading before it would leave
/// the decode no model.
fn unload_if_immediate(app: &AppHandle) {
    app.state::<Arc<TranscriptionManager>>()
        .maybe_unload_immediately("recall dictate stop");
}

/// Stop the recording and transcribe it. Blocking work (the stop drain,
/// the decode) runs on the blocking pool; the async post-processing step
/// is awaited after. Returns the final text for the editor's caret.
pub async fn stop_and_transcribe(app: &AppHandle) -> Result<String, String> {
    if !DICTATE_ACTIVE.swap(false, Ordering::AcqRel) {
        return Err("No dictation is running".to_string());
    }

    let cancel_gen = DICTATE_CANCEL_GEN.lock().unwrap().unwrap_or(0);
    let rm = Arc::clone(&app.state::<Arc<AudioRecordingManager>>());
    let tm = Arc::clone(&app.state::<Arc<TranscriptionManager>>());

    let (stop_res, speech_ms) = tauri::async_runtime::spawn_blocking(move || {
        let res = rm.stop_recording(DICTATE_BINDING, cancel_gen);
        // Valid once stop has returned: the recorder holds the session's
        // measured speech until the next recording replaces it.
        let speech_ms = rm.last_speech_ms();
        (res, speech_ms)
    })
    .await
    .map_err(|e| format!("Stop task panicked: {e}"))?;

    finish_session(app);

    let recorded = match stop_res {
        crate::managers::audio::StopRecordingResult::Captured { recorded, .. } => recorded,
        crate::managers::audio::StopRecordingResult::Cancelled => {
            unload_if_immediate(app);
            return Err("Dictation was cancelled".to_string());
        }
        crate::managers::audio::StopRecordingResult::NotActive => {
            unload_if_immediate(app);
            return Err("No dictation is running".to_string());
        }
        crate::managers::audio::StopRecordingResult::Failed(e) => {
            unload_if_immediate(app);
            return Err(e);
        }
    };

    if recorded.stt_samples.is_empty() || speech_ms < MIN_SPEECH_MS {
        unload_if_immediate(app);
        return Err("Recording contains no speech".to_string());
    }

    let tm_for_task = Arc::clone(&tm);
    let decoded =
        tauri::async_runtime::spawn_blocking(move || tm_for_task.transcribe(recorded.stt_samples))
            .await
            .map_err(|e| format!("Transcription task panicked: {e}"))
            .and_then(|result| result.map_err(|e| e.to_string()));
    let text = match decoded {
        Ok(text) => text,
        Err(e) => {
            // A failed decode returns before `transcribe()` unloads.
            unload_if_immediate(app);
            return Err(e);
        }
    };

    if text.trim().is_empty() {
        return Err("Recording contains no speech".to_string());
    }

    let post_process = get_settings(app).post_process_enabled;
    let processed = crate::actions::process_transcription_output(app, &text, post_process).await;

    info!(
        "Recall dictate finished: {} chars{}",
        processed.final_text.chars().count(),
        if post_process { ", post-processed" } else { "" }
    );
    Ok(processed.final_text)
}
