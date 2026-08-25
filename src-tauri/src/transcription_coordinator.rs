use crate::actions::ACTION_MAP;
use crate::managers::audio::AudioRecordingManager;
use log::{debug, error, warn};
use std::sync::Arc;
use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

const DEBOUNCE: Duration = Duration::from_millis(30);
const RELEASE_GRACE: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PttAction {
    Passthrough,
    DeferRelease,
    CancelRelease,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingRelease {
    binding_id: String,
    hotkey_string: String,
    deadline: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingPress {
    binding_id: String,
    hotkey_string: String,
}

/// Commands processed sequentially by the coordinator thread.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Command {
    Input {
        binding_id: String,
        hotkey_string: String,
        is_pressed: bool,
        push_to_talk: bool,
    },
    ExternalInput {
        binding_id: String,
    },
    Cancel {
        recording_was_active: bool,
    },
    ProcessingFinished,
    Timeout,
}

/// Pipeline lifecycle, owned exclusively by the coordinator thread.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
enum Stage {
    #[default]
    Idle,
    Recording(String), // binding_id
    Processing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Effect {
    Start {
        binding_id: String,
        hotkey_string: String,
    },
    Stop {
        binding_id: String,
        hotkey_string: String,
    },
}

#[derive(Debug, Default)]
struct CoordinatorState {
    stage: Stage,
    last_press: Option<Instant>,
    pending_release: Option<PendingRelease>,
    pending_press: Option<PendingPress>,
}

impl CoordinatorState {
    fn timeout_deadline(&self) -> Option<Instant> {
        self.pending_release.as_ref().map(|p| p.deadline)
    }

    fn step(&mut self, cmd: Command, now: Instant) -> Option<Effect> {
        match cmd {
            Command::Timeout => {
                if let Some(pending) = self.pending_release.take() {
                    if matches!(&self.stage, Stage::Recording(id) if id == &pending.binding_id) {
                        self.stage = Stage::Processing;
                        return Some(Effect::Stop {
                            binding_id: pending.binding_id,
                            hotkey_string: pending.hotkey_string,
                        });
                    }
                }
                None
            }
            Command::ExternalInput { binding_id } => {
                // External input (CLI / signal / RPC) is always toggle-mode and
                // uses a dummy hotkey string since no physical key was pressed.
                self.step(
                    Command::Input {
                        binding_id,
                        hotkey_string: String::new(),
                        is_pressed: true,
                        push_to_talk: false,
                    },
                    now,
                )
            }
            Command::Input {
                binding_id,
                hotkey_string,
                is_pressed,
                push_to_talk,
            } => {
                let pending_release_binding = self
                    .pending_release
                    .as_ref()
                    .map(|pending| pending.binding_id.as_str());
                let recording_binding = match &self.stage {
                    Stage::Recording(id) => Some(id.as_str()),
                    _ => None,
                };

                match classify_ptt_event(
                    pending_release_binding,
                    is_pressed,
                    push_to_talk,
                    &binding_id,
                    recording_binding,
                ) {
                    PttAction::CancelRelease => {
                        self.pending_release = None;
                        return None;
                    }
                    PttAction::DeferRelease => {
                        self.pending_release = Some(PendingRelease {
                            binding_id,
                            hotkey_string,
                            deadline: now + RELEASE_GRACE,
                        });
                        return None;
                    }
                    PttAction::Passthrough => {}
                }

                // Debounce rapid-fire press events (key repeat / double-tap).
                // Push-to-talk releases may be deferred above to absorb X11 auto-repeat.
                if is_pressed {
                    if self
                        .last_press
                        .is_some_and(|t| now.duration_since(t) < DEBOUNCE)
                    {
                        debug!("Debounced press for '{binding_id}'");
                        return None;
                    }
                    self.last_press = Some(now);
                }

                if push_to_talk {
                    if is_pressed && matches!(self.stage, Stage::Idle) {
                        self.stage = Stage::Recording(binding_id.clone());
                        Some(Effect::Start {
                            binding_id,
                            hotkey_string,
                        })
                    } else if !is_pressed
                        && matches!(&self.stage, Stage::Recording(id) if id == &binding_id)
                    {
                        self.stage = Stage::Processing;
                        Some(Effect::Stop {
                            binding_id,
                            hotkey_string,
                        })
                    } else {
                        None
                    }
                } else if is_pressed {
                    match &self.stage {
                        Stage::Idle => {
                            self.stage = Stage::Recording(binding_id.clone());
                            Some(Effect::Start {
                                binding_id,
                                hotkey_string,
                            })
                        }
                        Stage::Recording(id) if id == &binding_id => {
                            self.stage = Stage::Processing;
                            Some(Effect::Stop {
                                binding_id,
                                hotkey_string,
                            })
                        }
                        Stage::Processing => {
                            // Queue a single pending press while processing. If the
                            // user presses the shortcut again mid-inference or
                            // paste, they want to start a new recording as soon as
                            // the pipeline becomes idle, rather than having their
                            // press dropped and having to guess when to press again.
                            if self.pending_press.is_none() {
                                debug!(
                                    "Queued pending press for '{binding_id}': pipeline currently processing"
                                );
                                self.pending_press = Some(PendingPress {
                                    binding_id,
                                    hotkey_string,
                                });
                            } else {
                                debug!(
                                    "Ignoring additional press for '{binding_id}': already have pending press"
                                );
                            }
                            None
                        }
                        _ => {
                            debug!("Ignoring press for '{binding_id}': different binding active");
                            None
                        }
                    }
                } else {
                    None
                }
            }
            Command::Cancel {
                recording_was_active,
            } => {
                self.pending_release = None;
                self.pending_press = None;
                // Don't reset during processing — wait for the pipeline to finish.
                if !matches!(self.stage, Stage::Processing)
                    && (recording_was_active || matches!(self.stage, Stage::Recording(_)))
                {
                    self.stage = Stage::Idle;
                }
                None
            }
            Command::ProcessingFinished => {
                self.stage = Stage::Idle;
                if let Some(pending) = self.pending_press.take() {
                    debug!("Dispatching queued press for '{}'", pending.binding_id);
                    self.stage = Stage::Recording(pending.binding_id.clone());
                    Some(Effect::Start {
                        binding_id: pending.binding_id,
                        hotkey_string: pending.hotkey_string,
                    })
                } else {
                    None
                }
            }
        }
    }
}

fn classify_ptt_event(
    pending_release_binding: Option<&str>,
    is_pressed: bool,
    push_to_talk: bool,
    binding_id: &str,
    recording_binding: Option<&str>,
) -> PttAction {
    if !push_to_talk {
        return PttAction::Passthrough;
    }

    if is_pressed {
        if pending_release_binding == Some(binding_id) {
            PttAction::CancelRelease
        } else {
            PttAction::Passthrough
        }
    } else if recording_binding == Some(binding_id) && pending_release_binding.is_none() {
        PttAction::DeferRelease
    } else {
        PttAction::Passthrough
    }
}

/// Serialises all transcription lifecycle events through a single thread
/// to eliminate race conditions between keyboard shortcuts, signals, and
/// the async transcribe-paste pipeline.
pub struct TranscriptionCoordinator {
    tx: Sender<Command>,
}

pub fn is_transcribe_binding(id: &str) -> bool {
    id == "transcribe" || id == "transcribe_with_post_process" || id == "multi_stt_transcribe"
}

impl TranscriptionCoordinator {
    pub fn new(app: AppHandle) -> Self {
        let (tx, rx) = mpsc::channel();

        thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut state = CoordinatorState::default();

                loop {
                    let cmd = if let Some(deadline) = state.timeout_deadline() {
                        match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                            Ok(cmd) => cmd,
                            Err(mpsc::RecvTimeoutError::Timeout) => Command::Timeout,
                            Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        }
                    } else {
                        match rx.recv() {
                            Ok(cmd) => cmd,
                            Err(_) => break,
                        }
                    };

                    if let Some(effect) = state.step(cmd, Instant::now()) {
                        match effect {
                            Effect::Start {
                                binding_id,
                                hotkey_string,
                            } => {
                                start(&app, &mut state.stage, &binding_id, &hotkey_string);
                            }
                            Effect::Stop {
                                binding_id,
                                hotkey_string,
                            } => {
                                stop(&app, &mut state.stage, &binding_id, &hotkey_string);
                            }
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

    /// Send a keyboard input event for a transcribe binding.
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

    /// Send an external (CLI/signal/RPC) transcription trigger.
    pub fn send_external_input(&self, binding_id: &str) {
        if self
            .tx
            .send(Command::ExternalInput {
                binding_id: binding_id.to_string(),
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

fn start(app: &AppHandle, stage: &mut Stage, binding_id: &str, hotkey_string: &str) {
    let Some(action) = ACTION_MAP.get(binding_id) else {
        warn!("No action in ACTION_MAP for '{binding_id}'");
        return;
    };
    action.start(app, binding_id, hotkey_string);
    if app
        .try_state::<Arc<AudioRecordingManager>>()
        .is_some_and(|a| a.is_recording())
    {
        *stage = Stage::Recording(binding_id.to_string());
    } else {
        debug!("Start for '{binding_id}' did not begin recording; staying idle");
    }
}

fn stop(app: &AppHandle, stage: &mut Stage, binding_id: &str, hotkey_string: &str) {
    let Some(action) = ACTION_MAP.get(binding_id) else {
        warn!("No action in ACTION_MAP for '{binding_id}'");
        return;
    };
    action.stop(app, binding_id, hotkey_string);
    *stage = Stage::Processing;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_to_talk_release_while_recording_defers_release() {
        assert_eq!(
            classify_ptt_event(None, false, true, "transcribe", Some("transcribe")),
            PttAction::DeferRelease
        );
    }

    #[test]
    fn push_to_talk_press_matching_pending_release_cancels_release() {
        assert_eq!(
            classify_ptt_event(
                Some("transcribe"),
                true,
                true,
                "transcribe",
                Some("transcribe")
            ),
            PttAction::CancelRelease
        );
    }

    #[test]
    fn toggle_mode_press_and_release_pass_through() {
        assert_eq!(
            classify_ptt_event(
                Some("transcribe"),
                true,
                false,
                "transcribe",
                Some("transcribe")
            ),
            PttAction::Passthrough
        );
        assert_eq!(
            classify_ptt_event(None, false, false, "transcribe", Some("transcribe")),
            PttAction::Passthrough
        );
    }

    #[test]
    fn press_for_different_binding_than_pending_release_passes_through() {
        assert_eq!(
            classify_ptt_event(
                Some("transcribe"),
                true,
                true,
                "transcribe_with_post_process",
                Some("transcribe")
            ),
            PttAction::Passthrough
        );
    }

    #[test]
    fn press_matching_pending_release_cancels_without_recording_state() {
        assert_eq!(
            classify_ptt_event(Some("transcribe"), true, true, "transcribe", None),
            PttAction::CancelRelease
        );
    }

    // ---------------------------------------------------------------------
    // Sequence-level regression coverage for issue #1539.
    //
    // Under X11 key auto-repeat, holding a push-to-talk key does not emit one
    // long press. It emits the initial press followed by a stream of
    // synthesized release/press pairs, then a single genuine release on key-up.
    // Before the fix, every synthesized release passed straight through and
    // stopped recording, so holding the key "rapidly toggled" recording on and
    // off. The fix defers each release for a short grace window and cancels it
    // when the matching auto-repeat press arrives.
    //
    // The unit tests above assert `classify_ptt_event` in isolation. The
    // simulator below threads that classifier through the same `pending_release`
    // / `stage` state transitions the coordinator loop performs (lines that
    // handle `Command::Input` and the `recv_timeout` grace expiry), so a whole
    // event burst can be exercised deterministically without a Tauri AppHandle
    // or real timers.
    // ---------------------------------------------------------------------

    const BINDING: &str = "transcribe";

    #[derive(Clone, Copy)]
    enum Ev {
        /// A key-down event (real initial press or a synthesized auto-repeat press).
        Press,
        /// A key-up event (synthesized auto-repeat release or the genuine key-up).
        Release,
        /// The `RELEASE_GRACE` window elapsed with no cancelling press arriving.
        Grace,
    }

    #[derive(Debug, PartialEq, Eq)]
    enum SimStage {
        Idle,
        Recording,
        Processing,
    }

    struct SimResult {
        starts: u32,
        stops: u32,
        stage: SimStage,
    }

    /// Mirror of the coordinator loop's decision logic for a single push-to-talk
    /// binding: it calls the real `classify_ptt_event` and applies the exact same
    /// Defer / Cancel / debounce / start / stop transitions.
    fn simulate(events: &[Ev]) -> SimResult {
        let mut stage = SimStage::Idle;
        let mut pending: Option<String> = None;
        let mut last_press_ms: Option<u64> = None;
        let mut clock_ms: u64 = 0;
        let mut starts = 0u32;
        let mut stops = 0u32;
        let debounce_ms = DEBOUNCE.as_millis() as u64;

        for ev in events {
            // Auto-repeat events arrive a few ms apart, well inside DEBOUNCE.
            clock_ms += 5;

            match ev {
                Ev::Grace => {
                    // Coordinator's `RecvTimeoutError::Timeout` arm: fire the
                    // deferred release iff we are still recording that binding.
                    if let Some(pending_binding) = pending.take() {
                        if stage == SimStage::Recording && pending_binding == BINDING {
                            stage = SimStage::Processing;
                            stops += 1;
                        }
                    }
                }
                Ev::Press | Ev::Release => {
                    let is_pressed = matches!(ev, Ev::Press);
                    let pending_binding = pending.as_deref();
                    let recording_binding = if stage == SimStage::Recording {
                        Some(BINDING)
                    } else {
                        None
                    };

                    match classify_ptt_event(
                        pending_binding,
                        is_pressed,
                        true, // push_to_talk
                        BINDING,
                        recording_binding,
                    ) {
                        PttAction::CancelRelease => {
                            pending = None;
                            continue;
                        }
                        PttAction::DeferRelease => {
                            pending = Some(BINDING.to_string());
                            continue;
                        }
                        PttAction::Passthrough => {}
                    }

                    if is_pressed {
                        if last_press_ms.is_some_and(|t| clock_ms - t < debounce_ms) {
                            continue;
                        }
                        last_press_ms = Some(clock_ms);
                    }

                    if is_pressed && stage == SimStage::Idle {
                        stage = SimStage::Recording;
                        starts += 1;
                    } else if !is_pressed && stage == SimStage::Recording {
                        stage = SimStage::Processing;
                        stops += 1;
                    }
                }
            }
        }

        SimResult {
            starts,
            stops,
            stage,
        }
    }

    /// Initial press plus several synthesized release/press pairs, as X11 emits
    /// while a push-to-talk key is held down.
    fn autorepeat_burst() -> Vec<Ev> {
        let mut events = vec![Ev::Press];
        for _ in 0..6 {
            events.push(Ev::Release);
            events.push(Ev::Press);
        }
        events
    }

    /// Regression for #1539: a burst of X11 auto-repeat release/press pairs must
    /// not stop recording. Before the fix the first synthesized release stopped
    /// recording immediately (stops == 1, stage left Recording), which produced
    /// the rapid on/off toggling. With the fix the releases are coalesced and
    /// recording stays continuously active for the whole burst.
    #[test]
    fn x11_autorepeat_burst_does_not_toggle_recording() {
        let result = simulate(&autorepeat_burst());
        assert_eq!(result.starts, 1, "recording should start exactly once");
        assert_eq!(
            result.stops, 0,
            "synthesized auto-repeat releases must not stop recording mid-burst"
        );
        assert_eq!(
            result.stage,
            SimStage::Recording,
            "recording must remain active across the entire auto-repeat burst"
        );
    }

    /// Complements the burst test: once the key is genuinely released and the
    /// grace window elapses with no re-press, recording stops exactly once. This
    /// proves the debounce only coalesces synthesized releases and does not wedge
    /// the coordinator or swallow the real key-up.
    #[test]
    fn genuine_release_after_grace_stops_recording_once() {
        let mut events = autorepeat_burst();
        events.push(Ev::Release); // genuine key-up
        events.push(Ev::Grace); // grace window elapses, no cancelling press
        let result = simulate(&events);
        assert_eq!(result.starts, 1, "recording should start exactly once");
        assert_eq!(
            result.stops, 1,
            "a genuine release should stop recording exactly once"
        );
        assert_eq!(result.stage, SimStage::Processing);
    }

    #[test]
    fn toggle_press_while_processing_queues_pending_press_and_starts_after_processing_finished() {
        let mut state = CoordinatorState::default();
        let now = Instant::now();

        // Start recording
        let effect = state.step(
            Command::Input {
                binding_id: "transcribe".to_string(),
                hotkey_string: "ctrl+space".to_string(),
                is_pressed: true,
                push_to_talk: false,
            },
            now,
        );
        assert_eq!(
            effect,
            Some(Effect::Start {
                binding_id: "transcribe".to_string(),
                hotkey_string: "ctrl+space".to_string(),
            })
        );
        assert_eq!(state.stage, Stage::Recording("transcribe".to_string()));

        // Stop recording -> transitions to Processing
        let effect = state.step(
            Command::Input {
                binding_id: "transcribe".to_string(),
                hotkey_string: "ctrl+space".to_string(),
                is_pressed: true,
                push_to_talk: false,
            },
            now + Duration::from_millis(100),
        );
        assert_eq!(
            effect,
            Some(Effect::Stop {
                binding_id: "transcribe".to_string(),
                hotkey_string: "ctrl+space".to_string(),
            })
        );
        assert_eq!(state.stage, Stage::Processing);

        // Press again while processing -> should queue pending press and return None
        let effect = state.step(
            Command::Input {
                binding_id: "transcribe".to_string(),
                hotkey_string: "ctrl+space".to_string(),
                is_pressed: true,
                push_to_talk: false,
            },
            now + Duration::from_millis(200),
        );
        assert_eq!(effect, None);
        assert_eq!(state.stage, Stage::Processing);
        assert!(state.pending_press.is_some());

        // Processing completes -> should dispatch queued start effect
        let effect = state.step(
            Command::ProcessingFinished,
            now + Duration::from_millis(300),
        );
        assert_eq!(
            effect,
            Some(Effect::Start {
                binding_id: "transcribe".to_string(),
                hotkey_string: "ctrl+space".to_string(),
            })
        );
        assert_eq!(state.stage, Stage::Recording("transcribe".to_string()));
        assert!(state.pending_press.is_none());
    }

    #[test]
    fn subsequent_toggle_presses_while_processing_are_ignored() {
        let mut state = CoordinatorState::default();
        let now = Instant::now();

        state.stage = Stage::Processing;

        // First press while processing queues
        state.step(
            Command::Input {
                binding_id: "transcribe".to_string(),
                hotkey_string: "ctrl+space".to_string(),
                is_pressed: true,
                push_to_talk: false,
            },
            now,
        );
        assert_eq!(
            state.pending_press,
            Some(PendingPress {
                binding_id: "transcribe".to_string(),
                hotkey_string: "ctrl+space".to_string(),
            })
        );

        // Second press while processing is ignored
        state.step(
            Command::Input {
                binding_id: "transcribe".to_string(),
                hotkey_string: "ctrl+space".to_string(),
                is_pressed: true,
                push_to_talk: false,
            },
            now + Duration::from_millis(100),
        );
        assert_eq!(
            state.pending_press,
            Some(PendingPress {
                binding_id: "transcribe".to_string(),
                hotkey_string: "ctrl+space".to_string(),
            })
        );
    }

    #[test]
    fn cancel_clears_pending_press() {
        let mut state = CoordinatorState::default();
        let now = Instant::now();

        state.stage = Stage::Processing;
        state.step(
            Command::Input {
                binding_id: "transcribe".to_string(),
                hotkey_string: "ctrl+space".to_string(),
                is_pressed: true,
                push_to_talk: false,
            },
            now,
        );
        assert!(state.pending_press.is_some());

        state.step(
            Command::Cancel {
                recording_was_active: false,
            },
            now + Duration::from_millis(50),
        );
        assert!(state.pending_press.is_none());
    }

    #[test]
    fn processing_finished_without_pending_press_returns_to_idle() {
        let mut state = CoordinatorState::default();
        let now = Instant::now();

        state.stage = Stage::Processing;
        let effect = state.step(Command::ProcessingFinished, now);
        assert_eq!(effect, None);
        assert_eq!(state.stage, Stage::Idle);
    }
}
