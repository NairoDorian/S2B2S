//! Overlay preview: Settings → Overlay's "Preview" button drives a real
//! recording whose only output is the overlay itself.
//!
//! The primary model runs exactly as a dictation would — loaded, and streaming
//! live text when the model can stream — but nothing downstream of the overlay
//! happens: the audio is discarded (`discard_audio`, so the session never
//! grows), the stream is *cancelled* at stop rather than finalized (no final
//! decode, no paste, no clipboard, no direct-stream typing, no history row),
//! and the statistics run started for the stream worker is finished as
//! `Cancelled`. The overlay is forced on for the duration, even for a user
//! whose `overlay_style` is `None` — the preview exists to show the card while
//! its settings are changed in real time.
//!
//! Interactions mirror the other live-consumer sessions (VAD test, Live FFT):
//! while it runs the transcription hotkeys get "Already recording" from the
//! audio manager, and the cancel hotkey ends it through the ordinary
//! `cancel_current_operation`, whose `stop` hook below cleans the preview's
//! own state.

use log::{info, warn};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager};

use crate::audio_toolkit::VadPolicy;
use crate::managers::audio::{AudioRecordingManager, RecordingStartOptions};
use crate::managers::model::ModelManager;
use crate::managers::statistics::{StatisticsManager, StatisticsRunContext, StatisticsRunStatus};
use crate::managers::transcription::TranscriptionManager;
use crate::overlay;
use crate::settings::get_settings;
use crate::tray::{TrayIconState, set_tray_state};

/// Binding id the preview records under (shows up in logs).
pub const OVERLAY_PREVIEW_BINDING: &str = "overlay_preview";

static PREVIEW_ACTIVE: AtomicBool = AtomicBool::new(false);

/// The statistics run `start_stream` requires. Kept so stop can finish it —
/// a run abandoned active would sit in the Statistics page forever.
static PREVIEW_RUN: Mutex<Option<StatisticsRunContext>> = Mutex::new(None);

pub fn is_active() -> bool {
    PREVIEW_ACTIVE.load(Ordering::Acquire)
}

/// Start the preview recording. Fails while anything else records; the
/// transcription hotkeys stay blocked for the duration either way.
pub fn start(app: &AppHandle) -> Result<(), String> {
    if PREVIEW_ACTIVE.swap(true, Ordering::AcqRel) {
        return Err("Overlay preview is already running".to_string());
    }

    let rm = app.state::<Arc<AudioRecordingManager>>();
    if rm.is_recording() {
        PREVIEW_ACTIVE.store(false, Ordering::Release);
        return Err("Already recording".to_string());
    }

    let tm = app.state::<Arc<TranscriptionManager>>();
    let settings = get_settings(app);
    let model_supports_streaming = app
        .state::<Arc<ModelManager>>()
        .get_model_info(&settings.selected_model)
        .as_ref()
        .map(|m| m.supports_streaming)
        .unwrap_or(false);

    // The same policy a dictation with this model would use.
    let vad_policy = if !settings.vad_enabled {
        VadPolicy::Disabled
    } else if model_supports_streaming {
        VadPolicy::Streaming
    } else {
        VadPolicy::Offline
    };

    tm.initiate_model_load();

    let result = rm.try_start_recording_with_options(
        OVERLAY_PREVIEW_BINDING,
        vad_policy,
        RecordingStartOptions {
            capture_raw_override: Some(false),
            discard_audio: true,
        },
    );

    match result {
        Ok(readiness) => {
            if model_supports_streaming {
                let run = app
                    .state::<Arc<StatisticsManager>>()
                    .begin_normal_run(OVERLAY_PREVIEW_BINDING);
                // `live_typing` is false unconditionally: the preview types
                // nothing anywhere, whatever the paste method says.
                tm.start_stream(false, run.clone());
                *PREVIEW_RUN.lock().unwrap() = Some(run);
            }

            // Forced on: a user with the overlay disabled cannot preview it
            // otherwise. The show re-reads the overlay settings, so the card
            // comes up with whatever is currently configured.
            if model_supports_streaming {
                overlay::show_streaming_overlay(app);
            } else {
                overlay::show_recording_overlay(app);
            }

            // First real microphone samples: chime / mute / recording-ready,
            // the same cue a dictation gives, guarded by the readiness
            // generation so a stop before the first samples is silent.
            crate::actions::spawn_recording_ready_cue(app, &rm, readiness);

            crate::shortcut::register_cancel_shortcut(app);
            set_tray_state(app, TrayIconState::Recording);

            info!("Overlay preview started (streaming: {model_supports_streaming})");
            Ok(())
        }
        Err(e) => {
            PREVIEW_ACTIVE.store(false, Ordering::Release);
            tm.cancel_stream();
            warn!("Overlay preview failed to start: {e}");
            Err(e)
        }
    }
}

/// End the preview. Idempotent: every call site (the settings page, the cancel
/// hotkey via `cancel_current_operation`, the safety timer) may race, and a
/// preview that already stopped must not clean up a real recording's state.
pub fn stop(app: &AppHandle) -> Result<(), String> {
    if !PREVIEW_ACTIVE.swap(false, Ordering::AcqRel) {
        return Ok(());
    }

    // Cancel, not finalize: the final decode would produce text nobody
    // consumes, and the preview's whole point is that nothing is delivered.
    app.state::<Arc<TranscriptionManager>>().cancel_stream();
    app.state::<Arc<AudioRecordingManager>>()
        .cancel_recording_if_binding(OVERLAY_PREVIEW_BINDING);
    if let Some(run) = PREVIEW_RUN.lock().unwrap().take() {
        run.finish(StatisticsRunStatus::Cancelled);
    }

    crate::shortcut::unregister_cancel_shortcut(app);
    overlay::hide_recording_overlay(app);
    set_tray_state(app, TrayIconState::Idle);
    app.state::<Arc<TranscriptionManager>>()
        .maybe_unload_immediately("overlay preview stop");

    info!("Overlay preview stopped");
    Ok(())
}
