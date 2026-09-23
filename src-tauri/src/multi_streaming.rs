//! Experimental Multi Streaming STT: the merge-and-clean mode over live streams.
//!
//! The nested toggle inside *Experimental Live Streaming Merge & Clean*, and the
//! mode the parent mode was always pointing at. With it on, the parent's
//! coordinator runs — same chunks, same pauses, same brain model, same result —
//! but its extras' texts no longer come from re-decoding anything: every other
//! Multi-STT slot that can stream is started on its own live stream at the same
//! moment as the primary, and at each pause the models' own live texts go to the
//! merge. No batch decode runs during the session at all, and none at stop
//! either.
//!
//! # What this module is, and is not
//!
//! There is no second coordinator here. The coordinator is
//! [`crate::multi_stt_stream`]'s, and the only thing that changes between the
//! parent mode and this one is [`TextSource`](crate::multi_stt_stream::TextSource):
//! `ReDecode` there, `Live` here. That is deliberate — the pause test, the chunk
//! bookkeeping, the merge, the fallback, the overlay's two views and the final
//! paste are all the same behaviour, and a second implementation of them would
//! be a second set of bugs.
//!
//! What is left for this module is the part the coordinator cannot do for
//! itself: the extras' models are not the app's own model, so *someone* has to
//! decide which slots stream, lease each of their engines for the session, open
//! each stream at the right moment, and give the engines back at the end. That
//! someone is this module, and it is all this module is.
//!
//! # Which slots, and when
//!
//! Every **streaming-capable** extra slot (models 2, 3, …), in list order — the
//! user's own list, read the way it is read everywhere else. A slot that cannot
//! stream is skipped rather than loaded: the point of the mode is a live text to
//! merge, and a model that cannot stream would only sit in memory. So the mode
//! runs two models, or three, or more, depending on what the list holds — with a
//! single streaming slot it still works, as two texts (the primary's and that
//! one's). With none, the mode refuses and the parent's own coordinator arms
//! instead, which is the batch-extras fallback.
//!
//! The extras' engines are usually still loading when the recording starts: the
//! preload runs in the background so the user can begin speaking immediately. So
//! arming spawns one small waiter thread per slot; each leases its engine and
//! opens its stream as soon as its model is up. Frames pushed before a stream
//! opens queue on that slot's channel and are not lost (see
//! [`StreamRouter`](crate::managers::transcription::StreamRouter)), so a stream
//! that comes up late still hears the whole recording. A load that fails, or a
//! session that ends first, leaves the mode with one column fewer: the recording
//! is unaffected, and the merge simply has one text less to reconcile.
//!
//! # Ordering, which is the whole of the session's ending
//!
//! At stop the three steps have to happen in this order, and the action is what
//! performs them:
//!
//! 1. the primary stream is finalized (its final text reaches the coordinator
//!    through the sink),
//! 2. every extra stream is finalized by [`finish_extras`] — the coordinator's
//!    last chunk is built from those models' live text, and each finalize is what
//!    delivers its model's last words,
//! 3. [`crate::multi_stt_stream::finish`] closes the session and merges the last
//!    chunk.
//!
//! Finalizing the extras after the session had closed would hand those words to a
//! coordinator that no longer has a chunk to put them in.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use log::{info, warn};
use tauri::{AppHandle, Manager};

use crate::managers::audio::AudioRecordingManager;
use crate::managers::model::ModelManager;
use crate::managers::statistics::StatisticsRunContext;
use crate::managers::transcription::{
    PRIMARY_STREAM_SLOT, StreamFinalization, TrackedTranscription, TranscriptionManager,
};
use crate::multi_stt_stream::TextSource;
use crate::settings::{AppSettings, get_settings};

/// How long a waiter thread gives its extra model's load before giving up.
///
/// Generous on purpose: the model is loading while the user speaks, so a slow
/// first load costs nothing but that column's first words. A load that has not
/// finished in this long has failed or is wedged, and waiting further only holds
/// the thread.
const LOAD_WAIT: Duration = Duration::from_secs(120);

/// How many extra models the Multi-STT list can hold (models 2 to 9).
pub const EXTRA_MODELS: usize = crate::settings::MULTI_STT_MAX_EXTRA_MODELS;

/// The stream slot a Multi-STT model slot runs on.
///
/// Stream slot 0 is the primary's, owned by the app's own stream worker, so the
/// extras follow at 1, 2, 3, … — one per model slot, in the list's own order. See
/// [`STREAM_SLOTS`](crate::managers::transcription::STREAM_SLOTS): the array is
/// sized for exactly this.
pub fn stream_slot_of(model_index: usize) -> u8 {
    (model_index + 1) as u8
}

/// Every Multi-STT slot that can stream, as `(stream slot, model id)`, in the
/// list's order.
///
/// Streaming capability comes from the catalog entry, the same
/// `supports_streaming` the app uses to decide whether the primary model may
/// stream at all.
///
/// Public because the *preload* needs the answer before the session is armed: a
/// slot the mode will not stream from is a model that must not be loaded at all,
/// and the preload runs while the user is still getting ready to speak.
pub fn streaming_slots(app: &AppHandle, settings: &AppSettings) -> Vec<(u8, String)> {
    let model_manager = app.state::<Arc<ModelManager>>();
    let mut slots = Vec::new();
    for (index, candidate) in settings.multi_stt_extra_model_ids().into_iter().enumerate() {
        let Some(candidate) = candidate else {
            continue;
        };
        match model_manager.get_model_info(&candidate) {
            Some(info) if info.supports_streaming => slots.push((stream_slot_of(index), candidate)),
            Some(_) => info!(
                "Multi streaming STT: slot model '{}' cannot stream natively; skipping it",
                candidate
            ),
            None => warn!(
                "Multi streaming STT: slot model '{}' is not in the catalog; skipping it",
                candidate
            ),
        }
    }
    slots
}

/// The extra models this session streams with, and the threads bringing their
/// streams up.
struct Session {
    /// The slots this session streams from, `(stream slot, model id)`, in the
    /// list's order. What [`finish_extras`] finalizes and what [`cancel`]
    /// cancels.
    slots: Vec<(u8, String)>,
    /// Set before the session is dropped, so the waiters stop waiting for models
    /// whose streams nobody will collect.
    stop: Arc<AtomicBool>,
    /// The slots whose streams the waiters actually opened — a model that never
    /// finished loading is not one of them, and asking an unopened slot for a
    /// result would come back `NeverStarted` and look like a failure. Written by
    /// the waiters, read by the stop path.
    started: Arc<Mutex<Vec<u8>>>,
    waiters: Vec<JoinHandle<()>>,
}

/// The one running session, a process-wide slot like the parent mode's: only one
/// recording exists at a time.
static SESSION: LazyLock<Mutex<Option<Session>>> = LazyLock::new(|| Mutex::new(None));

fn take_session() -> Option<Session> {
    SESSION.lock().unwrap().take()
}

/// Whether the nested Multi Streaming STT mode owns a session. Read by
/// `MultiSttAction` to decide whether the extras are batch-decoded at stop (they
/// are not, in this mode: they streamed) and whether the session result is the
/// merged text (it is).
pub fn is_active() -> bool {
    SESSION.lock().unwrap().is_some()
}

/// Arm the mode for a recording that is about to start, if every precondition
/// holds. Returns whether the session — the coordinator included — is now armed.
///
/// Call this *before* `try_start_recording`, instead of the parent mode's
/// `start`: the two are one coordinator, and this one is the parent's own arming
/// with [`TextSource::Live`] and the extra slots. A refusal here is not a
/// failure: the caller arms the parent mode next, which is exactly the fallback
/// this mode wants (batch-decoded extras and all).
///
/// `slots` is [`streaming_slots`]'s answer, passed in rather than picked here so
/// the preload and the session cannot disagree about which models run.
pub fn start(
    app: &AppHandle,
    tm: &Arc<TranscriptionManager>,
    rm: &Arc<AudioRecordingManager>,
    slots: &[(u8, String)],
    primary_supports_streaming: bool,
    statistics: StatisticsRunContext,
) -> bool {
    // A session left over from a recording that never reached `stop` must not
    // survive into this one: its waiters would start streams nobody collects.
    cancel(tm);

    let settings = get_settings(app);
    if !settings.multi_stt_streaming_first_enabled || !settings.multi_stt_streaming_multi_enabled {
        return false;
    }
    if slots.is_empty() {
        warn!(
            "Multi streaming STT: none of the Multi-STT slots holds a streaming-capable model, so \
             there is no second live text to merge; the parent mode's own coordinator covers the \
             session with its batch extras"
        );
        return false;
    }

    // The coordinator is the parent's, told where its chunk texts come from. It
    // refuses for the same reasons the parent mode does (a primary that cannot
    // stream, no merge prompt), and its refusal is this mode's refusal — the
    // caller then arms the parent mode, which will refuse for the same reason or
    // arm with the batch extras.
    let slot_ids: Vec<u8> = slots.iter().map(|(slot, _)| *slot).collect();
    if !crate::multi_stt_stream::start(
        app,
        tm,
        rm,
        primary_supports_streaming,
        TextSource::Live,
        &slot_ids,
    ) {
        return false;
    }

    let stop = Arc::new(AtomicBool::new(false));
    let started = Arc::new(Mutex::new(Vec::new()));
    let waiters = slots
        .iter()
        .map(|(slot, model_id)| {
            let stop = Arc::clone(&stop);
            let started = Arc::clone(&started);
            let tm = Arc::clone(tm);
            let slot = *slot;
            let model_id = model_id.clone();
            let statistics = statistics.clone();
            thread::spawn(move || {
                wait_for_engine_and_start(&tm, slot, &model_id, &stop, &started, statistics)
            })
        })
        .collect();

    info!(
        "Multi streaming STT: armed — {} model(s) stream beside the primary '{}' as {:?}; each \
         pause merges their live texts with no re-decode, and the merged text is the result",
        slots.len(),
        settings.selected_model,
        slots
            .iter()
            .map(|(slot, model_id)| format!("{}: {}", slot, model_id))
            .collect::<Vec<_>>()
    );
    *SESSION.lock().unwrap() = Some(Session {
        slots: slots.to_vec(),
        stop,
        started,
        waiters,
    });
    true
}

/// Wait for one extra model's engine and open its stream.
///
/// The wait is the whole reason this runs on its own thread: the engine is
/// usually still loading when the recording starts, and a live stream holds its
/// engine for the session, so it cannot be taken after the stream has begun nor
/// before the model is up. Both of those steps belong to the manager, which owns
/// both pieces of state — what is left here is the thread to wait on it from.
fn wait_for_engine_and_start(
    tm: &Arc<TranscriptionManager>,
    slot: u8,
    model_id: &str,
    stop: &AtomicBool,
    started: &Mutex<Vec<u8>>,
    statistics: StatisticsRunContext,
) {
    if tm.start_extra_stream_when_loaded(slot, model_id, stop, LOAD_WAIT, statistics) {
        // Recorded only on success, so the stop path asks exactly the slots that
        // have a stream to finalize.
        started.lock().unwrap().push(slot);
    }
}

/// Finalize every extra stream this session opened, in slot order, and take each
/// one's result.
///
/// Returns one entry per extra model slot — index 0 is model 2's, index 1 model
/// 3's, index 2 model 4's — so the caller can finish each stream's statistics
/// attempt exactly the way it finishes the primary's. `None` means the slot
/// never streamed: a model that never loaded, or one whose stream failed.
///
/// Must be called *after* the primary stream's finalize and *before*
/// [`crate::multi_stt_stream::finish`]; see the module docs for why the order is
/// the whole of the session's ending.
///
/// Blocks the calling thread — call it from `spawn_blocking`, never from an async
/// task. `MultiSttAction::stop` runs one finalize per stream, and each waits for
/// its own worker to drain and report.
pub fn finish_extras(
    tm: &Arc<TranscriptionManager>,
) -> [Option<TrackedTranscription>; EXTRA_MODELS] {
    let mut results: [Option<TrackedTranscription>; EXTRA_MODELS] = Default::default();
    let Some(mut session) = take_session() else {
        return results;
    };
    // The waiters are stopped before the finalizes, not after: a lease taken
    // while this is running would open a stream on a slot whose finalize has
    // already happened, and its text would be dropped with the session.
    session.stop.store(true, Ordering::Release);
    for waiter in session.waiters.drain(..) {
        let _ = waiter.join();
    }

    let started = session.started.lock().unwrap().clone();
    for (slot, model_id) in &session.slots {
        if !started.contains(slot) {
            info!(
                "Multi streaming STT: '{}' never streamed on slot {}, so the merge has one text \
                 less",
                model_id, slot
            );
            continue;
        }
        let index = *slot as usize;
        if index == PRIMARY_STREAM_SLOT as usize || index > EXTRA_MODELS {
            continue;
        }
        match tm.finalize_stream_on(*slot) {
            StreamFinalization::Completed(tracked) => {
                info!(
                    "Multi streaming STT: slot {} ('{}') produced {} chars live",
                    slot,
                    model_id,
                    tracked.text.chars().count()
                );
                results[index - 1] = Some(tracked);
            }
            // Not a failure of the session: the coordinator already has this
            // model's text through the sink, and the merge the session ran is
            // what the result is built from. This only decides whether the
            // stream's own statistics attempt has a text to report.
            StreamFinalization::NeverStarted => info!(
                "Multi streaming STT: slot {} ('{}') decoded nothing live",
                slot, model_id
            ),
            StreamFinalization::Failed(error) => warn!(
                "Multi streaming STT: slot {} ('{}') failed to finalize: {}",
                slot, model_id, error
            ),
            StreamFinalization::Timeout(error) => warn!(
                "Multi streaming STT: slot {} ('{}') did not finalize in time: {}",
                slot, model_id, error
            ),
        }
    }
    results
}

/// Drop the session, cancelling every extra stream and freeing its engine. A
/// no-op when none is armed.
///
/// Does not touch the coordinator: that is [`crate::multi_stt_stream::cancel`]'s,
/// and the action calls both — this one when the recording never became a
/// session, that one on every path that drops a coordinator.
pub fn cancel(tm: &Arc<TranscriptionManager>) {
    let Some(mut session) = take_session() else {
        return;
    };
    session.stop.store(true, Ordering::Release);
    for waiter in session.waiters.drain(..) {
        let _ = waiter.join();
    }
    let started = session.started.lock().unwrap().clone();
    for (slot, _) in &session.slots {
        if !started.contains(slot) {
            continue;
        }
        // The worker returns the engine to `extra_engines` as it exits.
        tm.cancel_stream_on(*slot);
    }
}
