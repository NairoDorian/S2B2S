use crate::actions::ACTION_MAP;
use crate::managers::audio::AudioRecordingManager;
use crate::settings::{get_settings, PostProcessAction, ACTION_BINDING_PREFIX};
use log::{debug, error, warn};
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

const DEBOUNCE: Duration = Duration::from_millis(30);

/// Commands processed sequentially by the coordinator thread.
enum Command {
    Input {
        binding_id: String,
        hotkey_string: String,
        is_pressed: bool,
        push_to_talk: bool,
    },
    Cancel {
        recording_was_active: bool,
    },
    ProcessingFinished,
}

/// Pipeline lifecycle, owned exclusively by the coordinator thread.
enum Stage {
    Idle,
    Recording {
        binding_id: String,
        /// Id of the post-process action selected for this recording.
        selected_action: Option<String>,
    },
    Processing,
}

/// Serialises all transcription lifecycle events through a single thread
/// to eliminate race conditions between keyboard shortcuts, signals, and
/// the async transcribe-paste pipeline.
pub struct TranscriptionCoordinator {
    tx: Sender<Command>,
}

pub fn is_transcribe_binding(id: &str) -> bool {
    // "converse" records through the same pipeline; only its output routing
    // differs (Brain instead of paste).
    id == "transcribe"
        || id == "transcribe_with_post_process"
        || id == "converse"
        || id.starts_with(ACTION_BINDING_PREFIX)
}

pub fn is_action_binding(id: &str) -> bool {
    id.starts_with(ACTION_BINDING_PREFIX)
}

impl TranscriptionCoordinator {
    pub fn new(app: AppHandle) -> Self {
        let (tx, rx) = mpsc::channel();

        thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut stage = Stage::Idle;
                let mut last_press: Option<Instant> = None;

                while let Ok(cmd) = rx.recv() {
                    match cmd {
                        Command::Input {
                            binding_id,
                            hotkey_string,
                            is_pressed,
                            push_to_talk,
                        } => {
                            // Debounce rapid-fire press events (key repeat / double-tap).
                            // Releases always pass through for push-to-talk.
                            if is_pressed {
                                let now = Instant::now();
                                if last_press.is_some_and(|t| now.duration_since(t) < DEBOUNCE) {
                                    debug!("Debounced press for '{binding_id}'");
                                    continue;
                                }
                                last_press = Some(now);
                            }

                            if push_to_talk {
                                if is_pressed && matches!(stage, Stage::Idle) {
                                    start(&app, &mut stage, &binding_id, &hotkey_string);
                                } else if !is_pressed
                                    && matches!(&stage, Stage::Recording { binding_id: id, .. } if id == &binding_id)
                                {
                                    stop(&app, &mut stage, &binding_id, &hotkey_string);
                                }
                            } else if is_pressed {
                                match &stage {
                                    Stage::Idle => {
                                        start(&app, &mut stage, &binding_id, &hotkey_string);
                                    }
                                    Stage::Recording { binding_id: id, .. }
                                        if id == &binding_id =>
                                    {
                                        stop(&app, &mut stage, &binding_id, &hotkey_string);
                                    }
                                    _ => {
                                        debug!("Ignoring press for '{binding_id}': pipeline busy")
                                    }
                                }
                            }
                        }
                        Command::Cancel {
                            recording_was_active,
                        } => {
                            // Don't reset during processing — wait for the pipeline to finish.
                            if !matches!(stage, Stage::Processing)
                                && (recording_was_active
                                    || matches!(stage, Stage::Recording { .. }))
                            {
                                stage = Stage::Idle;
                            }
                        }
                        Command::ProcessingFinished => {
                            stage = Stage::Idle;
                        }
                    }
                }
                debug!("Transcription coordinator exited");
            }));
            if let Err(e) = result {
                error!("Transcription coordinator panicked: {e:?}");
            }
        });

        Self { tx }
    }

    /// Send a keyboard/signal input event for a transcribe binding.
    /// For signal-based toggles, use `is_pressed: true` and `push_to_talk: false`.
    pub fn send_input(
        &self,
        binding_id: &str,
        hotkey_string: &str,
        is_pressed: bool,
        push_to_talk: bool,
    ) {
        if self
            .tx
            .send(Command::Input {
                binding_id: binding_id.to_string(),
                hotkey_string: hotkey_string.to_string(),
                is_pressed,
                push_to_talk,
            })
            .is_err()
        {
            warn!("Transcription coordinator channel closed");
        }
    }

    pub fn notify_cancel(&self, recording_was_active: bool) {
        if self
            .tx
            .send(Command::Cancel {
                recording_was_active,
            })
            .is_err()
        {
            warn!("Transcription coordinator channel closed");
        }
    }

    pub fn notify_processing_finished(&self) {
        if self.tx.send(Command::ProcessingFinished).is_err() {
            warn!("Transcription coordinator channel closed");
        }
    }
}

/// Map a binding id to its ACTION_MAP key. Per-action shortcut bindings
/// (`ppa_<actionId>`) reuse the plain transcribe action; the post-processing
/// itself is driven by the selected action stored on the stage.
fn action_map_key(binding_id: &str) -> &str {
    if binding_id.starts_with(ACTION_BINDING_PREFIX) {
        "transcribe"
    } else {
        binding_id
    }
}

fn emit_action_selected(app: &AppHandle, action: &PostProcessAction) {
    let _ = app.emit(
        "action-selected",
        serde_json::json!({
            "key": action.trigger_key,
            "name": action.name,
            "icon": action.icon,
        }),
    );
}

fn emit_action_deselected(app: &AppHandle) {
    let _ = app.emit("action-deselected", ());
}

fn start(app: &AppHandle, stage: &mut Stage, binding_id: &str, hotkey_string: &str) {
    let Some(action) = ACTION_MAP.get(action_map_key(binding_id)) else {
        warn!("No action in ACTION_MAP for '{binding_id}'");
        return;
    };
    action.start(app, binding_id, hotkey_string);
    if app
        .try_state::<Arc<AudioRecordingManager>>()
        .is_some_and(|a| a.is_recording())
    {
        // Per-action shortcuts preselect their post-process action.
        let selected_action = binding_id
            .strip_prefix(ACTION_BINDING_PREFIX)
            .map(|action_id| action_id.to_string());

        if let Some(action_id) = &selected_action {
            let settings = get_settings(app);
            if let Some(pp_action) = settings.post_process_action(action_id) {
                emit_action_selected(app, pp_action);
            }
        }

        *stage = Stage::Recording {
            binding_id: binding_id.to_string(),
            selected_action,
        };
    } else {
        debug!("Start for '{binding_id}' did not begin recording; staying idle");
    }
}

fn stop(app: &AppHandle, stage: &mut Stage, binding_id: &str, hotkey_string: &str) {
    // Capture the selected action id before the stage transitions
    if let Stage::Recording {
        binding_id: _,
        selected_action,
    } = &stage
    {
        if let Some(state) = app.try_state::<crate::actions::ActiveActionState>() {
            *state.0.lock().unwrap() = selected_action.clone();
        }
    }

    let Some(action) = ACTION_MAP.get(action_map_key(binding_id)) else {
        warn!("No action in ACTION_MAP for '{binding_id}'");
        return;
    };
    action.stop(app, binding_id, hotkey_string);
    *stage = Stage::Processing;
}
