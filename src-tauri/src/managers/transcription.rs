use crate::audio_toolkit::{
    OutputLanguageEvidence, apply_custom_words, detect_output_language,
    normalize_transcription_output, remove_filler_words,
};
use crate::managers::audio::AudioRecordingManager;
use crate::managers::model::ModelManager;
use crate::managers::statistics::{
    PendingStatisticsAttempt, StatisticsRunContext, StatisticsRunStatus,
};
use crate::settings::{
    AppSettings, ModelBackendSetting, ModelUnloadTimeout, TranscribeAcceleratorSetting,
    get_settings,
};
use crate::utils;
use anyhow::Result;
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{HashMap, HashSet};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, mpsc};
use std::thread;
use std::time::{Duration, Instant, SystemTime};
use tauri::{AppHandle, Emitter, Manager};
use tauri_specta::Event;
use transcribe_cpp::{
    Backend, Device, Feature, Model, ModelOptions, RunExtension, RunOptions, Session,
    StreamOptions, Task, WhisperRunOptions,
};

const STREAM_PERF_LOG_INTERVAL: Duration = Duration::from_secs(5);
const STREAM_FINALIZE_REPLY_TIMEOUT: Duration = Duration::from_secs(30);

fn panic_payload_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "unknown panic".to_string()
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ModelStateEvent {
    pub event_type: String,
    pub model_id: Option<String>,
    pub model_name: Option<String>,
    pub error: Option<String>,
}

/// Number of timed transcription runs averaged per quantization variant when
/// the caller does not pick one. A warmup run always precedes these and is
/// never averaged in — see [`benchmark_timed_runs`].
pub const BENCHMARK_DEFAULT_RUNS: usize = 5;

/// Inclusive lower bound of the selectable timed-run count.
pub const BENCHMARK_MIN_RUNS: usize = 2;

/// Inclusive upper bound of the selectable timed-run count.
pub const BENCHMARK_MAX_RUNS: usize = 10;

/// Resolve the number of *timed* runs a benchmark should average: the
/// caller's request when it is present, otherwise
/// [`BENCHMARK_DEFAULT_RUNS`], clamped into
/// `[BENCHMARK_MIN_RUNS, BENCHMARK_MAX_RUNS]`. The warmup run is a fixed
/// extra pass and is never part of this count.
pub fn benchmark_timed_runs(requested: Option<usize>) -> usize {
    requested
        .unwrap_or(BENCHMARK_DEFAULT_RUNS)
        .clamp(BENCHMARK_MIN_RUNS, BENCHMARK_MAX_RUNS)
}

/// Feed granularity of the streaming benchmark replay, in milliseconds of
/// 16 kHz audio: the same 256-sample frame the live path feeds after the VAD,
/// so a replay measures the call pattern the app actually runs rather than an
/// idealised packet size.
pub const STREAM_BENCHMARK_FEED_MS: usize = 16;

/// Result of benchmarking a single quantization variant — returned to the
/// frontend so it can render per-quant timings next to each variant chip.
#[derive(Debug, Clone, Serialize, Type)]
pub struct BenchmarkResult {
    pub quant: String,
    pub model_id: String,
    pub filename: String,
    pub size_mb: u32,
    pub avg_time_ms: f64,
    /// Duration of the reference recording, so the frontend can render a
    /// real-time factor (`audio_secs / (avg_time_ms / 1000)`) alongside the
    /// raw timing.
    pub audio_secs: f64,
    pub is_default: bool,
}

/// Result of benchmarking a model's native streaming mode — the same numbers
/// the transcribe-fork `streaming-benchmark` driver reports, reduced to what
/// the status-bar panel needs to say how fast the model keeps up with speech.
///
/// The headline is [`compute_xrt`](Self::compute_xrt): audio seconds per
/// compute second, where "compute" is the sum of the begin, feed and finalize
/// call durations and excludes model loading and any pacing. Above 1.0 the
/// model decodes faster than the audio arrives, which is the condition for a
/// live stream that never falls behind.
#[derive(Debug, Clone, Serialize, Type)]
pub struct StreamingBenchmarkResult {
    pub model_id: String,
    /// Timed runs averaged. The discarded warmup is not part of this count.
    pub runs: u32,
    /// Duration of the reference recording being replayed.
    pub audio_secs: f64,
    /// Mean per-run compute time: begin + every feed + finalize.
    pub avg_compute_ms: f64,
    /// `audio_secs / (avg_compute_ms / 1000)` — the real-time speed factor.
    pub compute_xrt: f64,
    /// Mean per-run wall time, which also covers the feed loop's own overhead.
    pub avg_wall_ms: f64,
    /// p95 over every feed of every timed run (cheap buffer feeds included).
    pub feed_p95_ms: f64,
    /// p95 over feeds of at least 1 ms — the driver's heuristic "busy" filter.
    pub busy_feed_p95_ms: f64,
    /// Slowest single feed across every timed run.
    pub feed_max_ms: f64,
    /// Mean tail-flush cost of `finalize`.
    pub avg_finalize_ms: f64,
    /// Mean audio horizon, in milliseconds of audio already fed, at which
    /// text first appeared; `None` when no timed run produced text before
    /// `finalize`. This is how far behind the first word is, not wall clock.
    pub avg_first_text_audio_ms: Option<f64>,
    /// Feed granularity of the replay, in milliseconds (see
    /// [`STREAM_BENCHMARK_FEED_MS`]).
    pub feed_chunk_ms: u32,
    /// The resolved stream extension (family + cadence / right context), so a
    /// stored result still says which operating point was measured.
    pub stream_extension: String,
    /// The streaming latency the run was measured at, in ms of audio — read
    /// from the settings the status-bar latency slider writes, in the same
    /// unit for every family (R2T2's chunk, Nemotron's `(right + 1) × 80 ms`,
    /// Parakeet Unified's `chunk + right`; see
    /// [`latency_point`](crate::managers::native_streaming_latency::latency_point)).
    /// `None` for a model with no latency control. The extension above is the
    /// raw twin that proves what the runtime was handed.
    pub latency_ms: Option<u32>,
    /// The lookahead part of [`Self::latency_ms`]; 0 for R2T2.
    pub lookahead_ms: Option<u32>,
}

/// Timing of one streaming replay pass, kept so several passes can be averaged
/// into a [`StreamingBenchmarkResult`].
///
/// `compute_ms` is defined exactly as the transcribe-fork
/// `streaming-benchmark` driver defines it: the sum of the begin, feed and
/// finalize call durations — no model load, no pacing, no loop overhead.
#[derive(Debug, Clone)]
struct StreamReplayMetrics {
    compute_ms: f64,
    wall_ms: f64,
    feed_ms: Vec<f64>,
    finalize_ms: f64,
    /// Audio horizon of the first text, in milliseconds of audio fed.
    first_text_audio_ms: Option<f64>,
    /// Debug of the resolved stream extension, identical for every pass.
    stream_extension: String,
}

/// Progress event emitted from the backend during a benchmark run so the
/// frontend can show live status (which variant is being tested, etc.).
///
/// Every field except `event_type` is optional; construct with
/// `..Default::default()` and only set what the event actually carries.
#[derive(Debug, Clone, Default, Serialize)]
pub struct BenchmarkProgressEvent {
    pub event_type: String,
    pub quant: Option<String>,
    pub model_id: Option<String>,
    /// For `run_completed` this is the elapsed time of that single run; for
    /// `variant_completed` it is the average across all timed runs.
    pub avg_time_ms: Option<f64>,
    /// Duration of the reference recording (see [`BenchmarkResult::audio_secs`]).
    pub audio_secs: Option<f64>,
    /// 1-based index of the run that just finished (`run_completed` only);
    /// 0 for `warmup_completed`, the discarded first pass.
    pub run_index: Option<u32>,
    /// Number of timed runs averaged per variant — the discarded warmup is
    /// not part of this count, so the UI can render "2 / 5".
    pub total_runs: Option<u32>,
    pub error: Option<String>,
}

/// Live transcription snapshot emitted to the overlay during a streaming run.
/// `committed` is the append-only, flicker-free prefix; `tentative` is the
/// volatile suffix the model may still rewrite.
#[derive(Clone, Debug, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct StreamTextEvent {
    pub committed: String,
    pub tentative: String,
    /// Which live stream this text came from. Absent on every path but the
    /// experimental Multi Streaming STT mode, so the plain path serializes
    /// byte-identically: `None` is the primary model's stream (what the overlay
    /// has always shown), `Some(n)` (n = 1..STREAM_SLOTS-1) an extra model
    /// streaming beside it. The overlay renders each as its own column and
    /// routes every other reader of this event to the primary only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot: Option<u8>,
    /// Experimental Multi-STT streaming mode only (`None` everywhere else, so
    /// the plain path serializes byte-identically): how many of the session's
    /// chunks ended in a merge failure. The overlay shows a badge for a
    /// non-zero count — the failure is never written into the text itself,
    /// because with `DirectStreaming` that text is typed into the user's
    /// document and a marker would be typed with it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed_chunks: Option<u32>,
    /// Whether this text is the whole session's, composed chunk by chunk (the
    /// experimental Multi-STT streaming mode). The overlay uses it to grow its
    /// card with the text and read the backend's height cap, which only makes
    /// sense when nothing is being hidden: `false` on every other path, so the
    /// plain overlay's fixed cap is untouched.
    #[serde(default, skip_serializing_if = "is_false")]
    pub whole_session: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// Phase of the streaming overlay card, emitted to drive its UI state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum StreamPhase {
    /// Receiving audio / live text (or waiting for the stream to begin). Rust
    /// does not emit this today; the frontend starts in this phase and Rust only
    /// emits transitions away from it.
    Listening,
    /// Finalizing or post-processing — show a spinner.
    Working,
}

/// Semantic kind of "working" phase, used to localize the spinner label.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum StreamWorkKind {
    Transcribing,
    Polishing,
}

/// Emitted to switch the streaming overlay to a working spinner.
#[derive(Clone, Debug, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct StreamPhaseEvent {
    pub phase: StreamPhase,
    /// Present only when `phase` is `Working`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<StreamWorkKind>,
}

enum StreamCmd {
    Feed {
        pcm: Vec<f32>,
        queued_at: Instant,
    },
    /// Flush the stream and reply with the outcome (`NeverStarted` if no
    /// stream was ever active — the caller falls back to batch).
    Finalize(mpsc::Sender<StreamWorkerResult>),
    Cancel,
}

enum StreamWorkerResult {
    NeverStarted,
    Completed(FinalizedStreamText),
    Failed(String),
}

struct FinalizedStreamText {
    text: String,
    output_language: OutputLanguageEvidence,
    /// The streaming model's supported languages, for text-based detection.
    supported_languages: Vec<String>,
}

#[derive(Clone)]
pub struct TrackedTranscription {
    pub text: String,
    pub attempt: PendingStatisticsAttempt,
}

pub enum StreamFinalization {
    NeverStarted,
    Completed(TrackedTranscription),
    Failed(String),
    Timeout(String),
}

/// Slot of the primary model's live stream — the one the app has always had,
/// and the only one on every path but the experimental Multi Streaming STT
/// mode.
pub const PRIMARY_STREAM_SLOT: u8 = 0;
/// How many live streams may be routed at once.
///
/// One per Multi-STT model slot: the primary's stream is slot 0 and the extras
/// follow it in the order the user configured them, so stream slot `n` carries
/// "Model n+1" (`multi_stt_extra_models[n - 1]`). A mode that runs "two or more" streaming models
/// therefore has a slot for each of them rather than a hardcoded pair — which is
/// all this costs: the per-slot state below is three atomics and an `Option` per
/// slot (plus one shared router flag), and a slot that no model occupies is
/// never opened, never leased and never fed.
pub const STREAM_SLOTS: usize = 1 + crate::settings::MULTI_STT_MAX_EXTRA_MODELS;

/// How often [`TranscriptionManager::start_extra_stream_when_loaded`] looks for a
/// still-loading extra model. Short enough that the second column starts within a
/// frame or two of the load finishing, which is what the user sees as "it came up
/// with me"; the check is a hash lookup, so the cost of asking often is nothing.
const EXTRA_ENGINE_RETRY_POLL: Duration = Duration::from_millis(50);

/// Routes real-time audio frames to the active streaming workers. Shared between
/// the [`TranscriptionManager`] (opens/closes the route) and the audio recorder's
/// per-frame callback (feeds frames). The recorder holds an `Arc<StreamRouter>`
/// directly, so a frame with no stream pending costs a single relaxed atomic
/// load — no Tauri state lookup, no mutex lock.
///
/// A *slot* per stream rather than one route: the experimental Multi Streaming
/// STT mode runs several models (the primary plus up to eight extras) over the
/// same microphone at the same time, and the recorder feeds one frame to each.
/// Every other path opens exactly one slot, so the cost of the others is a loop
/// over a one-element list.
pub struct StreamRouter {
    /// Command channels to the active streaming workers, at most one per slot,
    /// present from `start_stream` until `finalize_stream`/`cancel_stream`.
    tx: Mutex<Vec<(u8, mpsc::Sender<StreamCmd>)>>,
    /// True while any stream is pending or active. The audio callback checks
    /// this first to avoid the mutex lock when no stream runs.
    open: Arc<AtomicBool>,
}

impl StreamRouter {
    fn new() -> Self {
        Self {
            tx: Mutex::new(Vec::new()),
            open: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Open a fresh command channel for a new streaming session on `slot`,
    /// returning the receiver that worker should drain. Caller must ensure no
    /// prior channel is still open on that slot.
    fn open(&self, slot: u8) -> mpsc::Receiver<StreamCmd> {
        let (tx, rx) = mpsc::channel::<StreamCmd>();
        let mut senders = self.tx.lock().unwrap();
        senders.retain(|(open, _)| *open != slot);
        senders.push((slot, tx));
        self.open.store(true, Ordering::Relaxed);
        rx
    }

    /// Take a slot's sender out (closing that route to new feeds). Returns the
    /// sender so the caller can send the final `Finalize`/`Cancel` command.
    fn take(&self, slot: u8) -> Option<mpsc::Sender<StreamCmd>> {
        let mut senders = self.tx.lock().unwrap();
        let taken = senders
            .iter()
            .position(|(open, _)| *open == slot)
            .map(|index| senders.remove(index).1);
        self.open.store(!senders.is_empty(), Ordering::Relaxed);
        taken
    }

    /// Drop a slot's channel and mark it closed without sending a final command
    /// (used when the worker exits without a finalize/cancel handshake).
    fn clear(&self, slot: u8) {
        let mut senders = self.tx.lock().unwrap();
        senders.retain(|(open, _)| *open != slot);
        self.open.store(!senders.is_empty(), Ordering::Relaxed);
    }

    /// Forward a 16 kHz frame to every active streaming worker. Cheap no-op (a
    /// single relaxed atomic load) when no stream is pending.
    pub fn feed(&self, frame: &[f32]) {
        if !self.open.load(Ordering::Relaxed) {
            return;
        }
        let senders = self.tx.lock().unwrap();
        for (_, tx) in senders.iter() {
            let _ = tx.send(StreamCmd::Feed {
                pcm: frame.to_vec(),
                queued_at: Instant::now(),
            });
        }
    }
}

/// A loaded inference engine. Every model the app can load (Whisper family,
/// Parakeet, Moonshine, Canary, … as GGUF) runs through transcribe-cpp, so
/// this is a single-variant enum: it keeps the `match`/`if let` sites that
/// Multi-STT's `extra_engines` share with the primary path readable, and
/// leaves room for a future non-transcribe-cpp runtime without reshaping them.
enum LoadedEngine {
    /// Holds the live `Session`, which keeps its `Model` alive internally, so
    /// repeated dictation reuses the session without reloading.
    TranscribeCpp(Session),
}

/// RAII guard that clears the `is_loading` flag and notifies waiters on drop.
/// Ensures the loading flag is always reset, even on early returns or panics.
pub struct LoadingGuard {
    is_loading: Arc<Mutex<bool>>,
    loading_condvar: Arc<Condvar>,
}

impl Drop for LoadingGuard {
    fn drop(&mut self) {
        // Recover from a poisoned mutex instead of panicking —
        // a panic inside Drop calls abort().
        let mut is_loading = match self.is_loading.lock() {
            Ok(g) => g,
            Err(e) => {
                warn!(
                    "Recovered poisoned is_loading mutex during LoadingGuard drop — a panic occurred earlier this session"
                );
                e.into_inner()
            }
        };
        *is_loading = false;
        self.loading_condvar.notify_all();
    }
}

/// RAII guard that clears the streaming worker/lease flags on any worker exit -
/// normal return, early return, or a panic in an engine call that unwinds the
/// detached worker thread. Tokens prevent an older worker from clearing a newer
/// worker's state if a start/finalize race ever slips through.
struct StreamWorkerGuard {
    slot: u8,
    worker_id: u64,
    active_stream_worker: Arc<[AtomicU64; STREAM_SLOTS]>,
    active_engine_lease: Arc<[AtomicU64; STREAM_SLOTS]>,
    stream_active: Arc<[AtomicBool; STREAM_SLOTS]>,
}

impl Drop for StreamWorkerGuard {
    fn drop(&mut self) {
        let slot = self.slot as usize;
        if self.active_stream_worker[slot].load(Ordering::Acquire) == self.worker_id {
            self.stream_active[slot].store(false, Ordering::Release);
        }
        let _ = self.active_engine_lease[slot].compare_exchange(
            self.worker_id,
            0,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
        let _ = self.active_stream_worker[slot].compare_exchange(
            self.worker_id,
            0,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }
}

#[derive(Clone)]
pub struct TranscriptionManager {
    engine: Arc<Mutex<Option<LoadedEngine>>>,
    model_manager: Arc<ModelManager>,
    app_handle: AppHandle,
    current_model_id: Arc<Mutex<Option<String>>>,
    last_activity: Arc<AtomicU64>,
    shutdown_signal: Arc<AtomicBool>,
    watcher_handle: Arc<Mutex<Option<thread::JoinHandle<()>>>>,
    is_loading: Arc<Mutex<bool>>,
    loading_condvar: Arc<Condvar>,
    reload_model_on_next_use: Arc<AtomicBool>,
    /// Routes real-time audio frames to the active streaming worker; see
    /// [`StreamRouter`]. Shared with the audio recorder so per-frame feeds skip
    /// Tauri state and the manager lock.
    router: Arc<StreamRouter>,
    /// Extra transcription engines for multi-STT mode, keyed by model ID.
    /// These are loaded alongside the primary engine for simultaneous multi-model transcription.
    extra_engines: Arc<Mutex<HashMap<String, LoadedEngine>>>,
    /// Extra models whose unload was requested while their engine was leased
    /// out for transcription (not present in `extra_engines`). The engine is
    /// dropped instead of re-inserted when the in-flight transcription returns.
    extra_unload_requests: Arc<Mutex<HashSet<String>>>,
    // Live typing (`PasteMethod::DirectStreaming`) is deliberately *not* a field
    // here: it is a property of one stream, not of the manager. It used to be a
    // single `AtomicBool` written by every `start_stream*` call, which was
    // correct while at most one stream could run; with the primary and an extra
    // streaming side by side, the extra's `false` would land after the primary's
    // stream began and silently turn the primary's live typing off — the one
    // thing the plain path's `DirectStreaming` exists for. It travels to the
    // worker as an argument instead.
    /// Extra model ids whose engine is currently being built. `load_extra_model`
    /// coalesces concurrent requests for the same id (the pre-load in
    /// `MultiSttAction::start` races the load in `stop()` on short recordings)
    /// instead of building — and briefly holding — two copies.
    extra_loading: Arc<(Mutex<HashSet<String>>, std::sync::Condvar)>,
    /// True only while a transcribe-cpp `Stream` is actually in flight (set by
    /// the worker once `stream()` succeeds). Used for overlay/UI decisions.
    stream_active: Arc<[AtomicBool; STREAM_SLOTS]>,
    /// Streaming uses four independent flags (the router's is shared, the other
    /// three are per slot): router open = frames should route, worker active =
    /// no second worker may start on that slot, engine lease = engine is out of
    /// the mutex, stream active = UI should show a live session. One entry per
    /// slot, because the experimental Multi Streaming STT mode runs the primary
    /// and its streaming extras side by side.
    ///
    /// Monotonic id source for stream workers; zero means "no worker".
    next_stream_worker_id: Arc<AtomicU64>,
    /// Nonzero while a worker exists on that slot, even if it has not leased the
    /// engine yet. This prevents a second worker from starting on the same slot
    /// after finalize/cancel closes the router but before the first worker has
    /// fully exited.
    active_stream_worker: Arc<[AtomicU64; STREAM_SLOTS]>,
    /// Nonzero while that slot's worker has taken an engine out of `engine` (the
    /// primary) or out of `extra_engines` (the streaming extra).
    /// `is_model_loaded()` consults this so the model still reports "loaded"
    /// while the worker holds it.
    active_engine_lease: Arc<[AtomicU64; STREAM_SLOTS]>,
    /// Pending statistics attempt for an in-flight live stream, per slot. One
    /// per stream rather than one for the process: with several streams live, a
    /// later one's attempt would otherwise overwrite the primary's and the primary's
    /// finalize would complete the wrong one.
    stream_attempt: Arc<Mutex<[Option<PendingStatisticsAttempt>; STREAM_SLOTS]>>,
    /// Optional in-process observer of the live text, called on the stream
    /// worker thread alongside the overlay event. Live Mode installs one to
    /// mirror the stream into its transcript file; the experimental Multi-STT
    /// streaming mode installs one to watch every live model and re-merge their
    /// text at each pause. It sees every slot, so a sink watching more than one
    /// stream can tell them apart.
    stream_text_sink: Arc<Mutex<Option<StreamTextSink>>>,
    /// When set, `emit_stream_text` calls the sink and skips the overlay event:
    /// the sink owns what the overlay displays and emits the composed text
    /// itself. Read on the audio-fed worker thread, so a relaxed atomic.
    stream_text_sink_exclusive: Arc<AtomicBool>,
    /// Process-lifetime running totals of the stream worker's feed and compute
    /// time, readable from outside the worker thread (see [`StreamTiming`]).
    /// The experimental Multi-STT streaming coordinator reports the primary
    /// model's rate from these at each chunk close.
    stream_timing: Arc<StreamTiming>,
    /// One small record per completed batch; no work on the capture callback.
    last_pipeline_metrics: Arc<Mutex<serde_json::Value>>,
}

/// Callback receiving every live-text update:
/// `(slot, committed, tentative, audio_committed_ms, input_received_ms)`.
///
/// The **slot** is what makes the callback a stream observer rather than the
/// primary model's alone. A sink installed by the experimental Multi Streaming
/// STT mode is watching every live model in the session — that is the whole
/// point of the mode — and a second model's text arriving unlabelled would be
/// read as the first's.
///
/// `audio_committed_ms` is the family's own statement of how much audio the
/// committed text accounts for — a *hint* with family-dependent granularity,
/// not a byte boundary into `committed` — which the Multi-STT streaming
/// coordinator reads as a drain hint (never as a cut point). `input_received_ms`
/// is the total audio the stream has been fed since it began, which, differenced
/// with `audio_committed_ms`, is the un-drained audio the close gate tests.
pub type StreamTextSink = Arc<dyn Fn(u8, &str, &str, i64, i64) + Send + Sync>;

impl TranscriptionManager {
    pub fn new(app_handle: &AppHandle, model_manager: Arc<ModelManager>) -> Result<Self> {
        let manager = Self {
            engine: Arc::new(Mutex::new(None)),
            model_manager,
            app_handle: app_handle.clone(),
            current_model_id: Arc::new(Mutex::new(None)),
            last_activity: Arc::new(AtomicU64::new(Self::now_ms())),
            shutdown_signal: Arc::new(AtomicBool::new(false)),
            watcher_handle: Arc::new(Mutex::new(None)),
            is_loading: Arc::new(Mutex::new(false)),
            loading_condvar: Arc::new(Condvar::new()),
            reload_model_on_next_use: Arc::new(AtomicBool::new(false)),
            router: Arc::new(StreamRouter::new()),
            extra_engines: Arc::new(Mutex::new(HashMap::new())),
            extra_unload_requests: Arc::new(Mutex::new(HashSet::new())),
            extra_loading: Arc::new((Mutex::new(HashSet::new()), std::sync::Condvar::new())),
            stream_active: Arc::new(std::array::from_fn(|_| AtomicBool::new(false))),
            next_stream_worker_id: Arc::new(AtomicU64::new(1)),
            active_stream_worker: Arc::new(std::array::from_fn(|_| AtomicU64::new(0))),
            active_engine_lease: Arc::new(std::array::from_fn(|_| AtomicU64::new(0))),
            stream_attempt: Arc::new(Mutex::new(std::array::from_fn(|_| None))),
            stream_text_sink: Arc::new(Mutex::new(None)),
            stream_text_sink_exclusive: Arc::new(AtomicBool::new(false)),
            stream_timing: Arc::new(StreamTiming::default()),
            last_pipeline_metrics: Arc::new(Mutex::new(serde_json::Value::Null)),
        };

        // Start the idle watcher
        {
            let app_handle_cloned = app_handle.clone();
            let manager_cloned = manager.clone();
            let shutdown_signal = manager.shutdown_signal.clone();
            let handle = thread::spawn(move || {
                debug!("Idle watcher thread started");
                while !shutdown_signal.load(Ordering::Relaxed) {
                    thread::sleep(Duration::from_secs(10)); // Check every 10 seconds

                    // Check shutdown signal again after sleep
                    if shutdown_signal.load(Ordering::Relaxed) {
                        break;
                    }

                    let settings = get_settings(&app_handle_cloned);
                    let timeout = settings.model_unload_timeout;

                    // Skip Immediately — that variant is handled by
                    // maybe_unload_immediately() after each transcription.
                    // Treating it as 0s here would unload the model mid-recording.
                    if timeout == ModelUnloadTimeout::Immediately {
                        continue;
                    }

                    // While recording, keep the idle timer fresh so the
                    // model is never unloaded mid-session.
                    let is_recording = app_handle_cloned
                        .try_state::<Arc<AudioRecordingManager>>()
                        .is_some_and(|a| a.is_recording());
                    if is_recording {
                        manager_cloned.touch_activity();
                        continue;
                    }

                    if let Some(limit_seconds) = timeout.to_seconds() {
                        let last = manager_cloned.last_activity.load(Ordering::Relaxed);
                        let now_ms = TranscriptionManager::now_ms();
                        let idle_ms = now_ms.saturating_sub(last);
                        let limit_ms = limit_seconds * 1000;

                        if idle_ms > limit_ms {
                            // idle -> unload primary model
                            if manager_cloned.is_model_loaded() {
                                let unload_start = std::time::Instant::now();
                                info!(
                                    "Model idle for {}s (limit: {}s), unloading",
                                    idle_ms / 1000,
                                    limit_seconds
                                );
                                match manager_cloned.unload_model() {
                                    Ok(()) => {
                                        let unload_duration = unload_start.elapsed();
                                        info!(
                                            "Model unloaded due to inactivity (took {}ms)",
                                            unload_duration.as_millis()
                                        );
                                    }
                                    Err(e) => {
                                        error!("Failed to unload idle model: {}", e);
                                    }
                                }
                            }

                            // Also unload any extra (multi-STT) engines
                            let extra_ids: Vec<String> = manager_cloned.get_extra_loaded_models();
                            for model_id in extra_ids {
                                info!(
                                    "Extra model '{}' idle for {}s (limit: {}s), unloading",
                                    model_id,
                                    idle_ms / 1000,
                                    limit_seconds
                                );
                                if let Err(e) = manager_cloned.unload_extra_model(&model_id) {
                                    error!("Failed to unload extra model '{}': {}", model_id, e);
                                }
                            }
                        }
                    }
                }
                debug!("Idle watcher thread shutting down gracefully");
            });
            *manager.watcher_handle.lock().unwrap() = Some(handle);
        }

        Ok(manager)
    }

    /// Lock the engine mutex, recovering from poison if a previous transcription panicked.
    fn lock_engine(&self) -> MutexGuard<'_, Option<LoadedEngine>> {
        self.engine.lock().unwrap_or_else(|poisoned| {
            warn!("Engine mutex was poisoned by a previous panic, recovering");
            poisoned.into_inner()
        })
    }

    pub fn is_model_loaded(&self) -> bool {
        // The engine may be leased out to the streaming worker (taken out of
        // the mutex). It's still loaded, just in use, so report true.
        self.lock_engine().is_some()
            || self
                .active_engine_lease
                .iter()
                .any(|leased| leased.load(Ordering::Acquire) != 0)
    }

    /// Accelerator changes should not disturb the current transcription. Mark
    /// the cached engine stale; the next model-use path reloads it with the
    /// latest settings.
    pub fn reload_model_on_next_use(&self) {
        self.reload_model_on_next_use.store(true, Ordering::Release);
    }

    /// Atomically check whether a model load is in progress and, if not, mark
    /// one as starting. Returns a [`LoadingGuard`] whose [`Drop`] impl will
    /// clear the flag and wake waiters. Returns `None` if a load is already in
    /// progress.
    pub fn try_start_loading(&self) -> Option<LoadingGuard> {
        let mut is_loading = self.is_loading.lock().unwrap();
        if *is_loading {
            return None;
        }
        *is_loading = true;
        Some(LoadingGuard {
            is_loading: self.is_loading.clone(),
            loading_condvar: self.loading_condvar.clone(),
        })
    }

    pub fn unload_model(&self) -> Result<()> {
        let unload_start = std::time::Instant::now();
        debug!("Starting to unload model");

        {
            let mut engine = self.lock_engine();
            // Dropping the engine frees all resources
            *engine = None;
        }
        {
            let mut current_model = self.current_model_id.lock().unwrap();
            *current_model = None;
        }

        // Emit unloaded event
        let _ = self.app_handle.emit(
            "model-state-changed",
            ModelStateEvent {
                event_type: "unloaded".to_string(),
                model_id: None,
                model_name: None,
                error: None,
            },
        );

        let unload_duration = unload_start.elapsed();
        debug!(
            "Model unloaded manually (took {}ms)",
            unload_duration.as_millis()
        );
        Ok(())
    }

    fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }

    /// Reset the idle timer to now.
    fn touch_activity(&self) {
        self.last_activity.store(Self::now_ms(), Ordering::Relaxed);
    }

    pub fn pipeline_metrics(&self) -> serde_json::Value {
        self.last_pipeline_metrics.lock().unwrap().clone()
    }

    /// Unloads the model immediately if the setting is enabled and the model is loaded
    pub fn maybe_unload_immediately(&self, context: &str) {
        if self
            .app_handle
            .try_state::<crate::cli::CliArgs>()
            .is_some_and(|args| args.transcribe_file.is_some())
        {
            return; // Three benchmark runs must share one warmed model.
        }
        let settings = get_settings(&self.app_handle);
        if settings.model_unload_timeout == ModelUnloadTimeout::Immediately
            && self.is_model_loaded()
        {
            info!("Immediately unloading model after {}", context);
            if let Err(e) = self.unload_model() {
                warn!("Failed to immediately unload model: {}", e);
            }
        }
    }

    pub fn load_model(&self, model_id: &str) -> Result<()> {
        self.load_model_with_device(model_id, None)
    }

    /// Like [`load_model`](Self::load_model), but lets a caller hard-select the
    /// compute device for this one load by its `transcribe_cpp::devices()`
    /// registry index (the index shown by `--list-devices`). `None` keeps the
    /// persisted accelerator setting (which may be Auto). The selection is not
    /// persisted.
    pub fn load_model_with_device(
        &self,
        model_id: &str,
        device_index: Option<usize>,
    ) -> Result<()> {
        apply_accelerator_settings(&self.app_handle);

        let load_start = std::time::Instant::now();
        debug!("Starting to load model: {}", model_id);

        // Emit loading started event
        let _ = self.app_handle.emit(
            "model-state-changed",
            ModelStateEvent {
                event_type: "loading_started".to_string(),
                model_id: Some(model_id.to_string()),
                model_name: None,
                error: None,
            },
        );

        let model_info = match self.model_manager.get_model_info(model_id) {
            Some(model_info) => model_info,
            None => {
                let error_msg = format!("Model not found: {}", model_id);
                let _ = self.app_handle.emit(
                    "model-state-changed",
                    ModelStateEvent {
                        event_type: "loading_failed".to_string(),
                        model_id: Some(model_id.to_string()),
                        model_name: None,
                        error: Some(error_msg.clone()),
                    },
                );
                return Err(anyhow::anyhow!(error_msg));
            }
        };

        // Every failure after loading starts must emit a terminal event so the
        // frontend can never remain in its loading state.
        let emit_loading_failed = |error_msg: &str| {
            let _ = self.app_handle.emit(
                "model-state-changed",
                ModelStateEvent {
                    event_type: "loading_failed".to_string(),
                    model_id: Some(model_id.to_string()),
                    model_name: Some(model_info.name.clone()),
                    error: Some(error_msg.to_string()),
                },
            );
        };

        if !model_info.is_downloaded {
            let error_msg = "Model not downloaded";
            emit_loading_failed(error_msg);
            return Err(anyhow::anyhow!(error_msg));
        }

        let model_path = self
            .model_manager
            .get_model_path(model_id)
            .inspect_err(|error| emit_loading_failed(&error.to_string()))?;

        // Drop the current engine BEFORE building the new one so transcribe-cpp
        // frees the previous native context first — avoids holding two models at
        // once (peak memory on large GGUFs). Clear the id too: if the new load
        // fails, status should read "no loaded model", not the dropped engine.
        {
            let mut engine = self.lock_engine();
            *engine = None;
        }
        {
            let mut current_model = self.current_model_id.lock().unwrap();
            *current_model = None;
        }

        let loaded_engine = {
            // The backend is chosen at load time (transcribe-cpp has no
            // runtime global). With an explicit `device_index` (the
            // --device-index flag) hard-select that registered device;
            // otherwise ask for the backend this model resolves to — a
            // per-model override when one is set, the persisted accelerator
            // preference otherwise (so an accelerator change marked for reload
            // takes effect here).
            let (backend, device) = match device_index {
                Some(index) => resolve_device_index(index).inspect_err(|e| {
                    emit_loading_failed(&e.to_string());
                })?,
                None => resolve_model_backend(&get_settings(&self.app_handle), model_id),
            };
            let requested_device = device
                .as_ref()
                .map(transcribe_device_label)
                .unwrap_or_else(|| "automatic".to_string());
            let model_options = ModelOptions { backend, device };

            let model = self
                .load_transcribe_model(
                    model_id,
                    Some(&model_info.filename),
                    &model_path,
                    &model_options,
                )
                .inspect_err(|e| emit_loading_failed(&e.to_string()))?;
            // The bound backend may differ from the request (e.g. CPU
            // fallback under Auto); log what actually loaded.
            let bound_backend = model.backend();
            let session = model.session().map_err(|e| {
                let error_msg = format!("Failed to create session for model {}: {}", model_id, e);
                emit_loading_failed(&error_msg);
                anyhow::anyhow!(error_msg)
            })?;
            // Reconcile the registry's advertised capabilities with the
            // loaded model's real ones (GGUF metadata) so badges/gating
            // reflect runtime truth, not the pre-download probe. The
            // load-completed event below triggers the frontend refresh.
            let caps = session.model().capabilities();
            self.model_manager.set_runtime_capabilities(
                model_id,
                caps.supports_streaming,
                caps.supports_translate,
                caps.supports_language_detect,
                caps.languages.clone(),
            );
            let bound_device = model
                .device()
                .map(|device| transcribe_device_label(&device))
                .unwrap_or_else(|_| "unknown".to_string());
            info!(
                "Loaded model '{}' (requested {:?}, requested device '{}', \
                     bound backend '{}', bound device '{}', supports_streaming={}, \
                     supports_translate={}, supports_language_detect={})",
                model_id,
                backend,
                requested_device,
                bound_backend,
                bound_device,
                caps.supports_streaming,
                caps.supports_translate,
                caps.supports_language_detect
            );
            LoadedEngine::TranscribeCpp(session)
        };

        // Update the current engine and model ID
        {
            let mut engine = self.lock_engine();
            *engine = Some(loaded_engine);
        }
        {
            let mut current_model = self.current_model_id.lock().unwrap();
            *current_model = Some(model_id.to_string());
        }

        // Reset idle timer so the watcher doesn't immediately unload a just-loaded model
        self.touch_activity();

        // Emit loading completed event
        let _ = self.app_handle.emit(
            "model-state-changed",
            ModelStateEvent {
                event_type: "loading_completed".to_string(),
                model_id: Some(model_id.to_string()),
                model_name: Some(model_info.name.clone()),
                error: None,
            },
        );

        let load_duration = load_start.elapsed();
        debug!(
            "Successfully loaded transcription model: {} (took {}ms)",
            model_id,
            load_duration.as_millis()
        );
        Ok(())
    }

    /// Kicks off the model loading in a background thread if it's not already loaded
    pub fn initiate_model_load(&self) {
        let mut is_loading = self.is_loading.lock().unwrap();
        if *is_loading {
            return;
        }

        let reload_pending = self.reload_model_on_next_use.load(Ordering::Acquire);
        if !reload_pending && self.is_model_loaded() {
            return;
        }

        *is_loading = true;
        // Cleared on drop, so a panic in the load (FFI, a poisoned lock) cannot
        // leave `is_loading` set and every waiter on the condvar parked forever.
        let guard = LoadingGuard {
            is_loading: self.is_loading.clone(),
            loading_condvar: self.loading_condvar.clone(),
        };
        drop(is_loading);
        let self_clone = self.clone();
        thread::spawn(move || {
            let _guard = guard;
            if reload_pending {
                self_clone
                    .reload_model_on_next_use
                    .store(false, Ordering::Release);
            }
            let settings = get_settings(&self_clone.app_handle);
            if let Err(e) = self_clone.load_model(&settings.selected_model) {
                error!("Failed to load model: {}", e);
            }
        });
    }

    pub fn get_current_model(&self) -> Option<String> {
        let current_model = self.current_model_id.lock().unwrap();
        current_model.clone()
    }

    /// The compute backend the currently-loaded engine is bound to, for
    /// diagnostics (e.g. confirming `--device-index` actually bound a GPU rather
    /// than falling back to CPU/auto). transcribe-cpp reports its real backend
    /// string; `None` when no model is loaded.
    pub fn current_backend(&self) -> Option<String> {
        self.lock_engine()
            .as_ref()
            .map(|LoadedEngine::TranscribeCpp(session)| session.model().backend().to_string())
    }

    /// Whether a live streaming run is currently in flight (on any slot — the
    /// overlay shows a live session while any column is streaming).
    pub fn is_streaming(&self) -> bool {
        self.stream_active
            .iter()
            .any(|active| active.load(Ordering::Acquire))
    }

    /// Shared handle to the stream router, used by the audio recorder to feed
    /// real-time frames without going through Tauri state on every frame.
    pub fn stream_router(&self) -> Arc<StreamRouter> {
        Arc::clone(&self.router)
    }

    /// Begin a live streaming transcription on the held engine's session.
    /// Audio frames pushed via [`StreamRouter::feed`] (captured directly by the
    /// audio recorder) are decoded incrementally and emitted to the overlay as
    /// [`StreamTextEvent`].
    ///
    /// Non-blocking: spawns a worker that waits for any in-progress model load,
    /// verifies the model supports streaming, then begins the stream. If the
    /// model can't stream, the worker idles until finalize/cancel and reports
    /// `NeverStarted` so the caller falls back to batch transcription. Frames
    /// sent before the stream begins queue on the channel and are not lost.
    ///
    /// `live_typing` allows the worker to type the live text into the
    /// foreground app when the paste method is `DirectStreaming`; pass `false`
    /// for post-processing / Multi-STT, where the stream is only a preview for
    /// the overlay and the final text is pasted afterwards.
    pub fn start_stream(&self, live_typing: bool, statistics: StatisticsRunContext) {
        self.start_stream_on(PRIMARY_STREAM_SLOT, live_typing, statistics, None);
    }

    /// Begin a live stream on an extra model's engine, beside the primary's.
    /// The experimental Multi Streaming STT mode is the only caller: the extra's
    /// engine is leased for the whole session rather than per decode (see
    /// [`Self::lease_extra_engine`]), because an unrelated batch decode taking it
    /// out from under a live stream is exactly what leasing prevents.
    ///
    /// `slot` is the stream slot, one per extra model, in the order the mode
    /// configured them; the range is the caller's business (see [`STREAM_SLOTS`])
    /// and the primary's own slot is not one of them.
    ///
    /// The engine is supplied by the caller — see
    /// [`Self::start_extra_stream_when_loaded`], which is the only one, and which
    /// is what makes the supplied engine this module's own type.
    fn start_extra_stream(
        &self,
        slot: u8,
        model_id: &str,
        engine: LoadedEngine,
        statistics: StatisticsRunContext,
    ) {
        debug_assert!(
            slot != PRIMARY_STREAM_SLOT,
            "the primary stream is not an extra"
        );
        self.start_stream_on(
            slot,
            // Never types: an extra's text is another column, and with
            // `DirectStreaming` a second writer would race the primary into the
            // same document.
            false,
            statistics,
            Some((model_id.to_string(), Some(engine))),
        );
    }

    /// Wait for an extra model's engine, lease it, and start its stream — the
    /// three steps an early second column needs, in one call.
    ///
    /// Only the Multi Streaming STT mode uses this, and only when an early start
    /// is impossible: its extra is preloaded in the background so the user can
    /// begin speaking at once, so the engine is usually still loading when the
    /// recording starts. The stream does not have to wait for the model, though —
    /// frames pushed before it opens queue on the slot's channel — so this polls
    /// until the load lands and then opens the stream, which is why it must run
    /// on a thread of its own rather than on the recording path.
    ///
    /// `stop` is polled alongside the load: a recording that has already ended
    /// must not have a stream opened under it, or the second column would start
    /// on audio nobody is going to collect.
    ///
    /// It lives here rather than in its caller because the lease and the start
    /// are two steps over this manager's own state, and because [`LoadedEngine`]
    /// is this module's own: a caller holding one would be holding an inference
    /// handle it has no use for. Returns whether a stream is now running.
    ///
    /// Blocks the calling thread.
    pub fn start_extra_stream_when_loaded(
        &self,
        slot: u8,
        model_id: &str,
        stop: &AtomicBool,
        retry_window: Duration,
        statistics: StatisticsRunContext,
    ) -> bool {
        let deadline = Instant::now() + retry_window;
        let engine = loop {
            if stop.load(Ordering::Acquire) {
                return false;
            }
            if self.is_extra_model_loaded(model_id) {
                match self.lease_extra_engine(model_id) {
                    Ok(engine) => break engine,
                    Err(error) => {
                        warn!(
                            "Multi streaming STT: '{}' reports loaded but its engine could not be \
                             leased: {}; slot {}'s column stays empty",
                            model_id, error, slot
                        );
                        return false;
                    }
                }
            }
            if Instant::now() >= deadline {
                warn!(
                    "Multi streaming STT: '{}' was still not loaded after {:?}; slot {}'s column \
                     stays empty",
                    model_id, retry_window, slot
                );
                return false;
            }
            thread::sleep(EXTRA_ENGINE_RETRY_POLL);
        };

        // Between the lease and the start there is nothing that can block, so the
        // engine cannot be stranded: `start_stream_on` gives it back itself if the
        // slot is already held.
        self.start_extra_stream(slot, model_id, engine, statistics);
        true
    }

    /// Open a stream worker on `slot`. `supplied` is `None` for the primary (the
    /// worker leases the app's own engine) and names the extra model plus the
    /// engine the caller leased for it otherwise.
    fn start_stream_on(
        &self,
        slot: u8,
        live_typing: bool,
        statistics: StatisticsRunContext,
        mut supplied: Option<(String, Option<LoadedEngine>)>,
    ) {
        // An engine the caller leased but this worker will never use — the slot
        // is taken — belongs back in `extra_engines`: dropping it would unload
        // the model the user asked to keep.
        macro_rules! give_back {
            () => {
                if let Some((model_id, Some(engine))) = supplied.take() {
                    self.return_extra_engine(&model_id, engine, "its live stream");
                }
            };
        }

        // The test is per slot, deliberately. The router's shared `open` flag
        // says *a* stream is running and cannot be the guard here: the whole
        // point of the slot array is that the primary's stream may already be
        // open when an extra's starts, and testing the router as a whole would
        // refuse the extra on every multi-streaming session — silently, since
        // the caller's only signal is an empty column. One worker per slot is
        // what has to hold, and that is what the second half tests.
        let index = slot as usize;
        if self.active_stream_worker[index].load(Ordering::Acquire) != 0 {
            warn!(
                "start_stream called for slot {} while a stream worker is already active there",
                slot
            );
            give_back!();
            return;
        }
        let worker_id = self.next_stream_worker_id.fetch_add(1, Ordering::Relaxed);
        if self.active_stream_worker[index]
            .compare_exchange(0, worker_id, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            warn!("start_stream lost a race with another stream worker");
            give_back!();
            return;
        }
        let rx = self.router.open(slot);
        self.stream_active[index].store(false, Ordering::Release);

        let manager = self.clone();
        thread::spawn(move || {
            manager.run_stream_worker(rx, slot, worker_id, live_typing, statistics, supplied)
        });
    }

    /// Put a stream worker's engine back where it was leased from: the app's own
    /// engine for the primary slot, `extra_engines` for every other — the test is
    /// the primary's slot rather than the extra's, so it holds for however many
    /// extra slots the mode runs and keys the extra by the id the caller supplied
    /// rather than by the slot.
    fn restore_stream_engine(&self, slot: u8, engine: LoadedEngine, model_id: &str) {
        if slot == PRIMARY_STREAM_SLOT {
            self.return_engine(engine, model_id);
        } else {
            self.return_extra_engine(model_id, engine, "its live stream");
        }
    }

    /// Give back an engine the caller leased for a worker that never ran — the
    /// slot was already held, so nothing else will ever return it.
    fn give_back_supplied_engine(&self, slot: u8, engine: Option<LoadedEngine>, model_id: &str) {
        if let Some(engine) = engine {
            self.restore_stream_engine(slot, engine, model_id);
        }
    }

    /// `live_typing` is this stream's own permission to type into the foreground
    /// app; see the note where the field used to be. It is a parameter rather than
    /// shared state because several streams are live at once in the Multi Streaming
    /// STT mode and the primary's permission is not the extra's.
    fn run_stream_worker(
        &self,
        rx: mpsc::Receiver<StreamCmd>,
        slot: u8,
        worker_id: u64,
        live_typing: bool,
        statistics: StatisticsRunContext,
        supplied: Option<(String, Option<LoadedEngine>)>,
    ) {
        let _worker = StreamWorkerGuard {
            slot,
            worker_id,
            active_stream_worker: Arc::clone(&self.active_stream_worker),
            active_engine_lease: Arc::clone(&self.active_engine_lease),
            stream_active: Arc::clone(&self.stream_active),
        };

        // Wait for any in-progress model load to finish (start_stream races the
        // background load kicked off when recording starts).
        {
            let mut is_loading = self.is_loading.lock().unwrap();
            while *is_loading {
                is_loading = self.loading_condvar.wait(is_loading).unwrap();
            }
        }

        // The extra's model and its pre-leased engine, or the app's own model
        // and `None` for the primary.
        let (model_id, supplied_engine) = match supplied {
            Some((model_id, engine)) => (model_id, engine),
            None => (self.get_current_model().unwrap_or_default(), None),
        };
        let index = slot as usize;

        // Take the engine out of the mutex so we own it during streaming,
        // structurally excluding any concurrent batch transcription (which
        // transcribe-cpp's compute_lock would refuse anyway). Returned when the
        // worker exits, or dropped if the model was switched/unloaded mid-stream.
        // The extra's engine is already out of `extra_engines` — it was leased
        // by the caller, for the whole session rather than per decode.
        if self.active_engine_lease[index]
            .compare_exchange(0, worker_id, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            warn!("Live preview: another worker already holds the transcription engine");
            self.router.clear(slot);
            self.give_back_supplied_engine(slot, supplied_engine, &model_id);
            drain_until_finalize(rx, StreamWorkerResult::NeverStarted);
            return;
        }
        let mut engine = match supplied_engine {
            Some(engine) => engine,
            None => match self.lock_engine().take() {
                Some(e) => e,
                None => {
                    info!(
                        "Live preview: model '{}' was unloaded before streaming could begin; \
                         falling back to batch transcription",
                        model_id
                    );
                    let _ = self.active_engine_lease[index].compare_exchange(
                        worker_id,
                        0,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    );
                    self.router.clear(slot);
                    drain_until_finalize(rx, StreamWorkerResult::NeverStarted);
                    return;
                }
            },
        };

        // The loaded session (not the ModelManager copy) is the source of truth
        // for run-path capabilities.
        let (supports_streaming, supports_translate, languages) = match &engine {
            LoadedEngine::TranscribeCpp(session) => {
                let model = session.model();
                let caps = model.capabilities();
                info!(
                    "Live preview: model '{}' slot={} arch='{}' variant='{}' \
                     supports_streaming={} supports_translate={} languages={:?}",
                    model_id,
                    slot,
                    model.arch(),
                    model.variant(),
                    caps.supports_streaming,
                    caps.supports_translate,
                    caps.languages,
                );
                (
                    caps.supports_streaming,
                    caps.supports_translate,
                    caps.languages,
                )
            }
        };

        if !supports_streaming {
            self.restore_stream_engine(slot, engine, &model_id);
            self.router.clear(slot);
            drain_until_finalize(rx, StreamWorkerResult::NeverStarted);
            return;
        }

        // Build run options mirroring the offline transcribe-cpp path: task +
        // language gated against what the model actually advertises.
        let mut settings = get_settings(&self.app_handle);
        // A streaming extra is an extra like any other: the per-slot language and
        // translate preferences the Multi-STT panel sets for it apply here, or a
        // second column would be transcribed with the primary's settings. Every
        // slot but the primary's, not just the first extra: the mode runs two or
        // more models beside the primary, and `apply_extra_model_settings`
        // resolves the right per-slot settings from the model id itself.
        if slot != PRIMARY_STREAM_SLOT {
            apply_extra_model_settings(&mut settings, &model_id);
        }
        let effective_language =
            effective_language_for_model(&settings, self.model_manager.as_ref(), &model_id);
        let run_plan = transcribe_cpp_run_plan(
            settings.translate_to_english,
            &effective_language,
            &languages,
            supports_translate,
        );
        let output_language = resolve_output_language_evidence(
            &settings,
            run_plan.language.as_deref(),
            &languages,
            run_plan.target_language.as_deref() == Some("en"),
        );
        let run_options = RunOptions {
            task: run_plan.task,
            language: run_plan.language,
            target_language: run_plan.target_language,
            ..Default::default()
        };

        // Run the stream on the held session. The Stream borrows the session
        // (and thus the engine) for its lifetime, so the feed/finalize loop
        // lives in a labeled block — when it exits, the borrow is released and
        // the engine can be moved into return_engine().
        let mut finalize_reply: Option<mpsc::Sender<StreamWorkerResult>> = None;
        let mut finalize_result: Option<StreamWorkerResult> = None;
        let mut start_failure: Option<String> = None;
        let stream_started = 'stream: {
            let LoadedEngine::TranscribeCpp(session) = &mut engine;

            // Read the backend string before beginning the stream — the
            // `Stream` borrows `session` mutably for its lifetime, so we can't
            // call `session.model()` once it exists.
            let backend = session.model().backend();

            if statistics.is_terminal() {
                break 'stream false;
            }

            let inference_started_at_ms = chrono::Utc::now().timestamp_millis();
            let Some(attempt) = statistics.begin_attempt(
                Some(model_id.clone()),
                Some("transcribe_cpp".to_string()),
                Some(backend.to_string()),
                inference_started_at_ms,
            ) else {
                break 'stream false;
            };
            self.stream_attempt.lock().unwrap()[index] = Some(attempt.clone());

            // Resolve family-specific streaming extension (e.g. Parakeet Buffered,
            // Nemotron cache-aware) from the user's latency preset, if any.
            // R2T2 resolves through the same dispatcher but reads its chunk size
            // from `native_streaming_chunk_ms` instead (a preset has no meaning
            // for a continuous millisecond range).
            let stream_ext = self.resolved_stream_extension(&settings, &session.model(), &model_id);
            let stream_options = StreamOptions {
                family: stream_ext,
                ..Default::default()
            };
            info!(target: "pipeline", "stream model={} backend={} options={:?} run={:?}", model_id, backend, stream_options, run_options);
            let begin_start = Instant::now();
            let mut stream = match session.stream(&run_options, &stream_options) {
                Ok(s) => s,
                Err(e) => {
                    error!("Failed to begin stream: {}", e);
                    let error = e.to_string();
                    attempt.complete_canonical("");
                    attempt.finish(StatisticsRunStatus::Failed);
                    start_failure = Some(error);
                    break 'stream false;
                }
            };

            info!(target: "pipeline", "stream begin_ms={:.3}", begin_start.elapsed().as_secs_f64()*1000.0);
            self.stream_active[index].store(true, Ordering::Release);
            self.touch_activity();
            info!(
                "Live streaming transcription started (model '{}', slot {}, backend '{}')",
                model_id, slot, backend
            );
            // Tell the detailed view this model's column exists, now that its
            // stream does. Without this the column would appear only with the
            // model's first word — seconds later for a chunked streaming model,
            // and never for one that is running but saying nothing — which is
            // exactly the "the second block is missing" this view must not do.
            self.announce_stream_slot(slot);

            let is_direct_streaming_paste = settings.paste_method
                == crate::settings::PasteMethod::DirectStreaming
                && live_typing;
            let mut direct_writer = if is_direct_streaming_paste {
                Some(crate::direct_stream_writer::DirectStreamWriter::new(
                    self.app_handle.clone(),
                    settings.direct_streaming_speed,
                    settings.clone(),
                ))
            } else {
                None
            };

            let mut perf = StreamPerf::new(Arc::clone(&self.stream_timing));
            // A command pulled out of the queue while coalescing a backlog.
            // Only one can ever be held: the drain stops at the first
            // non-Feed command, which is handled on the next pass.
            let mut deferred: Option<StreamCmd> = None;
            loop {
                let cmd = match deferred.take() {
                    Some(cmd) => cmd,
                    None => match rx.recv() {
                        Ok(cmd) => cmd,
                        Err(_) => break,
                    },
                };
                match cmd {
                    StreamCmd::Feed { pcm, queued_at } => {
                        perf.queue_max = perf.queue_max.max(queued_at.elapsed());
                        // Take every frame that is already waiting and hand the
                        // library one buffer instead of one frame per call.
                        //
                        // The audio callback keeps delivering while a tick is
                        // still running, so when a tick costs more than the
                        // chunk cadence the queue never empties and each feed
                        // decodes exactly one chunk: the decoder stays one tick
                        // behind per chunk and the lag grows with the utterance
                        // (seconds behind the speaker, then a long stall after
                        // they stop — what "streaming got slow" looks like from
                        // the outside). Handing over the backlog lets the
                        // library fold it into a single tick, so the effective
                        // cadence settles at what the machine can actually
                        // sustain and the lag stays bounded at about one tick.
                        let mut pcm = pcm;
                        loop {
                            match rx.try_recv() {
                                Ok(StreamCmd::Feed {
                                    pcm: more,
                                    queued_at: more_at,
                                }) => {
                                    perf.queue_max = perf.queue_max.max(more_at.elapsed());
                                    pcm.extend_from_slice(&more);
                                }
                                Ok(other) => {
                                    deferred = Some(other);
                                    break;
                                }
                                // Empty (caught up) or disconnected: either way
                                // there is nothing more to coalesce.
                                Err(_) => break,
                            }
                        }
                        self.touch_activity();
                        perf.record_feed(pcm.len());
                        let feed_start = Instant::now();
                        match stream.feed(&pcm) {
                            Ok(update) => {
                                perf.record_compute(feed_start.elapsed());
                                perf.record_update(
                                    update.revision,
                                    update.input_received_ms,
                                    update.audio_committed_ms,
                                    update.buffered_ms,
                                );
                                if update.committed_changed || update.tentative_changed {
                                    let text = stream.text();
                                    perf.record_emit();
                                    self.emit_stream_text(
                                        slot,
                                        &text.committed,
                                        &text.tentative,
                                        update.audio_committed_ms,
                                        update.input_received_ms,
                                    );
                                    if let Some(writer) = &direct_writer {
                                        writer.update_target(text.display());
                                    }
                                }
                                perf.maybe_log();
                            }
                            Err(e) => {
                                perf.record_compute(feed_start.elapsed());
                                warn!("stream feed failed: {}", e);
                            }
                        }
                    }
                    StreamCmd::Finalize(reply) => {
                        let finalize_start = Instant::now();
                        let result = match stream.finalize() {
                            // After finalize the committed prefix holds the full
                            // text; display() = committed + tentative is the safe read.
                            Ok(update) => {
                                perf.record_compute(finalize_start.elapsed());
                                perf.record_update(
                                    update.revision,
                                    update.input_received_ms,
                                    update.audio_committed_ms,
                                    update.buffered_ms,
                                );
                                let text = stream.text();
                                let finalized_text = text.display();
                                // After finalize the committed prefix holds the
                                // whole text, so a sink that has been tracking
                                // the chunk boundaries gets its last words here.
                                // Named by slot: every live stream in the
                                // session finalizes, and a coordinator that
                                // watches more than one has to be able to tell
                                // which of them just delivered its tail.
                                self.notify_stream_text_sink(
                                    slot,
                                    &text.committed,
                                    &text.tentative,
                                    update.audio_committed_ms,
                                    update.input_received_ms,
                                );
                                if let Some(writer) = direct_writer.take() {
                                    writer.flush(Some(finalized_text.clone()));
                                }
                                // In auto mode the model's own LID is the best
                                // remaining evidence; the snapshot is only
                                // materialized when it can change the outcome.
                                let output_language = match &output_language {
                                    OutputLanguageEvidence::Unknown => {
                                        with_model_detected_language(
                                            OutputLanguageEvidence::Unknown,
                                            stream.snapshot().language,
                                        )
                                    }
                                    resolved => resolved.clone(),
                                };
                                StreamWorkerResult::Completed(FinalizedStreamText {
                                    text: finalized_text,
                                    output_language,
                                    supported_languages: languages.clone(),
                                })
                            }
                            Err(e) => {
                                if let Some(writer) = direct_writer.take() {
                                    writer.cancel();
                                }
                                perf.record_compute(finalize_start.elapsed());
                                error!(
                                    "stream finalize failed: {}; falling back to batch transcription",
                                    e
                                );
                                StreamWorkerResult::Failed(e.to_string())
                            }
                        };
                        let chars = match &result {
                            StreamWorkerResult::Completed(finalized) => finalized.text.len(),
                            _ => 0,
                        };
                        info!(target: "pipeline", "stream finalize_ms={:.3} native={:?}",
                            finalize_start.elapsed().as_secs_f64()*1000.0, stream.snapshot().timings);
                        perf.log_finalized(chars);
                        finalize_reply = Some(reply);
                        finalize_result = Some(result);
                        break;
                    }
                    StreamCmd::Cancel => {
                        if let Some(writer) = direct_writer.take() {
                            writer.cancel();
                        }
                        stream.reset();
                        break;
                    }
                }
            }

            if let Some(writer) = direct_writer.take() {
                writer.cancel();
            }

            true
        };
        // `stream` + the `&mut engine` borrow are released here.

        if !stream_started {
            // Stream never began (the statistics run was already terminal, or
            // `begin_attempt` / `session.stream()` failed); drain so the finalize handshake still completes and the
            // caller falls back to batch transcription. Return the engine first
            // so the fallback can immediately use it.
            self.restore_stream_engine(slot, engine, &model_id);
            let result = start_failure
                .map(StreamWorkerResult::Failed)
                .unwrap_or(StreamWorkerResult::NeverStarted);
            drain_until_finalize(rx, result);
            return;
        }

        self.restore_stream_engine(slot, engine, &model_id);
        if let (Some(reply), Some(result)) = (finalize_reply, finalize_result) {
            let _ = reply.send(result);
        }
        // `_worker` drops here, clearing this worker's active/lease flags after
        // the engine has been returned to the pool.
    }

    /// Deterministic headless replay through the loaded engine and app settings.
    /// Captured PCM bypasses microphone/VAD/UI; no user history is written.
    pub fn benchmark_stream(
        &self,
        audio: &[f32],
        chunk_ms: usize,
        att_right: Option<i32>,
        r2t2_chunk_ms: Option<u32>,
    ) -> Result<(String, serde_json::Value)> {
        anyhow::ensure!(chunk_ms > 0, "stream chunk must be positive");
        let model_id = self
            .get_current_model()
            .ok_or_else(|| anyhow::anyhow!("No loaded model"))?;
        let settings = get_settings(&self.app_handle);
        let language =
            effective_language_for_model(&settings, self.model_manager.as_ref(), &model_id);
        let mut engine = self
            .lock_engine()
            .take()
            .ok_or_else(|| anyhow::anyhow!("Engine is busy"))?;
        let result = (|| -> Result<(String, serde_json::Value)> {
            let LoadedEngine::TranscribeCpp(session) = &mut engine;
            let model = session.model();
            let caps = model.capabilities();
            let plan = transcribe_cpp_run_plan(
                settings.translate_to_english,
                &language,
                &caps.languages,
                caps.supports_translate,
            );
            let options = RunOptions {
                task: plan.task,
                language: plan.language,
                target_language: plan.target_language,
                ..Default::default()
            };
            let family = if let Some(ms) = r2t2_chunk_ms {
                // A runtime-only override for latency sweeps: the persisted
                // per-model chunk is left untouched.
                anyhow::ensure!(
                    crate::managers::native_streaming_latency::r2t2_chunk_ms_is_valid(ms),
                    "R2T2 chunk {} ms is outside {}..={} ms",
                    ms,
                    crate::managers::native_streaming_latency::R2T2_CHUNK_MS_MIN,
                    crate::managers::native_streaming_latency::R2T2_CHUNK_MS_MAX
                );
                Some(transcribe_cpp::StreamExtension::R2T2(
                    transcribe_cpp::R2T2StreamOptions {
                        chunk_size_ms: Some(ms),
                    },
                ))
            } else if let Some(right) = att_right {
                Some(transcribe_cpp::StreamExtension::ParakeetStream(
                    transcribe_cpp::ParakeetStreamOptions {
                        att_context_right: Some(right),
                    },
                ))
            } else {
                // Same dispatcher and same resolved values as the primary live
                // path above, so a headless benchmark cannot silently measure a
                // different cadence than the app actually runs.
                self.resolved_stream_extension(&settings, &model, &model_id)
            };
            let stream_options = StreamOptions {
                family,
                ..Default::default()
            };
            let start = Instant::now();
            let mut stream = session.stream(&options, &stream_options)?;
            let begin_ms = start.elapsed().as_secs_f64() * 1000.0;
            let mut feeds = Vec::new();
            let mut first_text_ms = None;
            for chunk in audio.chunks(chunk_ms.saturating_mul(16)) {
                let tick = Instant::now();
                let update = stream.feed(chunk)?;
                feeds.push(tick.elapsed().as_secs_f64() * 1000.0);
                if first_text_ms.is_none() && (update.committed_changed || update.tentative_changed)
                {
                    first_text_ms = Some(start.elapsed().as_secs_f64() * 1000.0);
                }
            }
            let tick = Instant::now();
            stream.finalize()?;
            let finalize_ms = tick.elapsed().as_secs_f64() * 1000.0;
            let snapshot = stream.snapshot();
            let native = &snapshot.timings;
            Ok((
                snapshot.text,
                serde_json::json!({
                    "begin_ms": begin_ms, "feed_ms": feeds, "first_text_compute_ms": first_text_ms,
                    "finalize_ms": finalize_ms, "wall_ms": start.elapsed().as_secs_f64()*1000.0,
                    "timings": { "mel_ms": native.mel_ms, "encode_ms": native.encode_ms, "decode_ms": native.decode_ms },
                    "stream_options": format!("{:?}", stream_options), "language": options.language,
                }),
            ))
        })();
        self.return_engine(engine, &model_id);
        result
    }

    /// Return the leased engine to the mutex, unless the model was switched or
    /// unloaded during transcription (in which case the stale engine is dropped).
    fn return_engine(&self, engine: LoadedEngine, expected_model_id: &str) {
        let still_current =
            self.current_model_id.lock().unwrap().as_deref() == Some(expected_model_id);
        if still_current {
            *self.lock_engine() = Some(engine);
        } else {
            info!(
                "Model changed/unloaded during transcription; dropping stale engine (was '{}')",
                expected_model_id
            );
            // `engine` drops here, freeing its resources.
        }
    }

    /// Flush the active stream and return its final, post-filtered text.
    ///
    /// A never-started, empty, failed, and timed-out stream remain distinct so
    /// callers can apply the fallback policy without losing attempt outcomes.
    pub fn finalize_stream(&self) -> StreamFinalization {
        self.finalize_stream_on(PRIMARY_STREAM_SLOT)
    }

    /// Finalize one slot's stream and post-process its text. Every non-primary
    /// slot is an experimental Multi Streaming STT mode extra; its text is a
    /// column of its own and never becomes the session's transcript.
    pub fn finalize_stream_on(&self, slot: u8) -> StreamFinalization {
        let index = slot as usize;
        let Some(tx) = self.router.take(slot) else {
            return StreamFinalization::NeverStarted;
        };
        let (reply_tx, reply_rx) = mpsc::channel();
        if tx.send(StreamCmd::Finalize(reply_tx)).is_err() {
            return self.failed_or_never_started_stream(
                slot,
                "Live transcription worker stopped before finalization",
            );
        }
        let finalized = match reply_rx.recv_timeout(STREAM_FINALIZE_REPLY_TIMEOUT) {
            Ok(StreamWorkerResult::Completed(finalized)) => finalized,
            Ok(StreamWorkerResult::NeverStarted) => {
                self.stream_attempt.lock().unwrap()[index].take();
                return StreamFinalization::NeverStarted;
            }
            Ok(StreamWorkerResult::Failed(error)) => {
                if let Some(attempt) = self.stream_attempt.lock().unwrap()[index].take() {
                    attempt.complete_canonical("");
                    attempt.finish(StatisticsRunStatus::Failed);
                }
                return StreamFinalization::Failed(error);
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return self.failed_or_never_started_stream(
                    slot,
                    "Live transcription worker disconnected during finalization",
                );
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                self.stream_active[index].store(false, Ordering::Release);
                if let Some(attempt) = self.stream_attempt.lock().unwrap()[index].take() {
                    attempt.complete_canonical("");
                    attempt.finish(StatisticsRunStatus::Failed);
                }
                return StreamFinalization::Timeout(format!(
                    "Timed out waiting {:?} for live transcription to finalize",
                    STREAM_FINALIZE_REPLY_TIMEOUT
                ));
            }
        };

        let settings = get_settings(&self.app_handle);
        // Streaming models do not receive a decode prompt, so custom words
        // always go through the shared fuzzy post-correction path.
        let filtered = post_process_transcription_text(
            finalized.text,
            &settings,
            false,
            &finalized.output_language,
            &finalized.supported_languages,
        );
        let Some(attempt) = self.stream_attempt.lock().unwrap()[index].take() else {
            return StreamFinalization::Failed(
                "Live transcription completed without an attempt context".to_string(),
            );
        };
        attempt.complete_canonical(&filtered);
        if filtered.trim().is_empty() {
            attempt.finish(StatisticsRunStatus::Empty);
            // An extra that streamed a whole session and said nothing is almost
            // never a broken stream: it is a setting that does not match the
            // speech, and the model that does it is prompt-conditioned — told
            // which language to transcribe, and silent when the answer is wrong
            // (Nemotron's streaming checkpoint is one, and its own docs say the
            // language must be given). Worth a warning rather than a debug-view
            // column alone, because in the production view this model has no
            // column at all: its text exists only as merge input, so a silent
            // extra shows up as a merge that quietly had one text less. The
            // primary is not one of the panel's models, so `multi_stt_extra_model`
            // answers `None` for it and its own empty result stays unreported.
            if let Some(extra_model) = multi_stt_extra_model(&settings, slot) {
                let mut extra_settings = settings.clone();
                apply_extra_model_settings(&mut extra_settings, &extra_model);
                let hint = effective_language_for_model(
                    &extra_settings,
                    self.model_manager.as_ref(),
                    &extra_model,
                );
                warn!(
                    "Multi streaming STT: slot {} ('{}') streamed the whole session and produced \
                     no text under the language hint '{}'. Nothing about the stream failed — check \
                     that model's language in the Multi-STT settings: a prompt-conditioned \
                     streaming model transcribes nothing when the hint does not match the speech",
                    slot, extra_model, hint
                );
            }
        }

        // Only the primary's stream is the app's own transcription; an extra's
        // unload policy belongs to the Multi-STT paths that started it.
        if slot == PRIMARY_STREAM_SLOT {
            self.maybe_unload_immediately("streaming transcription");
        }
        StreamFinalization::Completed(TrackedTranscription {
            text: filtered,
            attempt,
        })
    }

    /// Abandon any active stream without producing text (e.g. on cancel).
    pub fn cancel_stream(&self) {
        self.cancel_stream_on(PRIMARY_STREAM_SLOT);
    }

    /// Abandon one slot's stream without producing text. An extra slot's stream
    /// ends here on every path that drops the multi-streaming session.
    pub fn cancel_stream_on(&self, slot: u8) {
        let index = slot as usize;
        if let Some(tx) = self.router.take(slot) {
            let _ = tx.send(StreamCmd::Cancel);
        }
        self.stream_attempt.lock().unwrap()[index].take();
        self.stream_active[index].store(false, Ordering::Release);
    }

    fn failed_or_never_started_stream(&self, slot: u8, error: &str) -> StreamFinalization {
        match self.stream_attempt.lock().unwrap()[slot as usize].take() {
            Some(attempt) => {
                attempt.complete_canonical("");
                attempt.finish(StatisticsRunStatus::Failed);
                StreamFinalization::Failed(error.to_string())
            }
            None => StreamFinalization::NeverStarted,
        }
    }

    /// Emit a working-phase event to the streaming overlay (spinner + label).
    pub fn emit_stream_working(&self, kind: StreamWorkKind) {
        let _ = StreamPhaseEvent {
            phase: StreamPhase::Working,
            kind: Some(kind),
        }
        .emit(&self.app_handle);
    }

    /// Hand a live-text update to the installed sink, if any. Split out of
    /// [`Self::emit_stream_text`] because the finalize path needs the sink and
    /// only the sink: a sink that composes its own text (the experimental
    /// Multi-STT streaming mode) is still holding a chunk open when the stream
    /// ends, and without the final text its last words would be missing from
    /// that chunk's merge.
    fn notify_stream_text_sink(
        &self,
        slot: u8,
        committed: &str,
        tentative: &str,
        audio_committed_ms: i64,
        input_received_ms: i64,
    ) {
        let sink = self.stream_text_sink.lock().unwrap().clone();
        if let Some(sink) = sink {
            sink(
                slot,
                committed,
                tentative,
                audio_committed_ms,
                input_received_ms,
            );
        }
    }

    /// Declare a stream slot to the overlay's detailed view, with no text.
    ///
    /// Called once per stream, the moment it is live. The detailed view gives
    /// every *running* model a column, so the column has to exist before the
    /// model has said anything: a chunked streaming model closes its first
    /// chunk seconds after it starts, and a model whose settings do not match
    /// the speech (a wrong language hint on a prompt-conditioned model, say)
    /// never says anything at all — in both cases a column that waited for
    /// text would be indistinguishable from a model that was never started,
    /// which is the one thing this view is for. An empty-text event is what
    /// makes the column; the model's own text then fills it in.
    ///
    /// Only the extras are announced: column 1 is the primary model's, and it
    /// is on screen whether or not anything is streaming.
    ///
    /// Gated exactly like the emission in [`Self::emit_stream_text`], because
    /// it is the same event on the same channel: an exclusive sink composes the
    /// production view's single block itself, so this is a no-op there and no
    /// numbered slot ever reaches the overlay outside the detailed view.
    fn announce_stream_slot(&self, slot: u8) {
        if slot == PRIMARY_STREAM_SLOT || self.stream_text_sink_exclusive.load(Ordering::Acquire) {
            return;
        }
        let _ = StreamTextEvent {
            committed: String::new(),
            tentative: String::new(),
            slot: Some(slot),
            failed_chunks: None,
            whole_session: false,
        }
        .emit(&self.app_handle);
    }

    fn emit_stream_text(
        &self,
        slot: u8,
        committed: &str,
        tentative: &str,
        audio_committed_ms: i64,
        input_received_ms: i64,
    ) {
        // Every slot reaches the sink, labelled with the stream it came from.
        // The experimental Multi-STT streaming coordinator watches the primary
        // and, in its nested Multi Streaming STT form, the extras as well: it
        // takes each model's text for the chunk that closes, so a second model's
        // text has to arrive here rather than only in its own overlay column.
        // The sink is `None` on every path that has no coordinator, so the plain
        // session's extra slot pays one mutex read per update and nothing else.
        self.notify_stream_text_sink(
            slot,
            committed,
            tentative,
            audio_committed_ms,
            input_received_ms,
        );
        // An exclusive sink composes the displayed text itself (it re-chunks
        // the stream), so emitting the worker's raw text here would race it.
        if self.stream_text_sink_exclusive.load(Ordering::Acquire) {
            return;
        }
        let _ = StreamTextEvent {
            committed: committed.to_string(),
            tentative: tentative.to_string(),
            slot: (slot != PRIMARY_STREAM_SLOT).then_some(slot),
            failed_chunks: None,
            whole_session: false,
        }
        .emit(&self.app_handle);
    }

    /// Publish a [`StreamTextEvent`] composed elsewhere (the experimental
    /// Multi-STT streaming coordinator). The plain stream worker uses
    /// [`Self::emit_stream_text`].
    ///
    /// `slot` says which block of the overlay the text belongs to, and the
    /// coordinator has two shapes:
    ///
    /// - `None` is the primary model's own column, whose rough text the
    ///   coordinator replaces in place with the merged one. This is the mode's
    ///   production view and the only shape a parent-mode session emits; it goes
    ///   with an exclusive sink, so the raw text of that column never reaches the
    ///   overlay at all.
    /// - `Some(n)` is a column the coordinator owns *beside* the streaming
    ///   models' own — the merged-and-cleaned block of the Multi Streaming STT
    ///   mode's debug view, under the live columns. A session in that view leaves
    ///   the sink non-exclusive, so the models' raw text keeps arriving on its own
    ///   slots and this is the only text on the block.
    ///
    /// Either way the event is the whole session's text, which is what tells the
    /// overlay to grow its card and read a height cap back — a per-block flag
    /// would have to be carried separately to say the same thing.
    pub fn emit_composed_stream_text(
        &self,
        committed: &str,
        tentative: &str,
        failed_chunks: Option<u32>,
        slot: Option<u8>,
    ) {
        let _ = StreamTextEvent {
            committed: committed.to_string(),
            tentative: tentative.to_string(),
            slot,
            failed_chunks,
            whole_session: true,
        }
        .emit(&self.app_handle);
    }

    /// Install (or, with `None`, remove) the in-process live-text observer.
    /// Only one sink exists at a time; Live Mode owns it for the duration of a
    /// session and clears it on stop.
    ///
    /// `exclusive` makes the sink responsible for the overlay's text as well:
    /// see [`Self::emit_composed_stream_text`]. The experimental Multi-STT
    /// streaming mode asks for it in its production view, where it replaces the
    /// streaming model's rough text with the merged one in place — the overlay
    /// must show the composed version and never the raw one. In its debug view
    /// the same session asks for a non-exclusive sink instead: the models' raw
    /// text *is* what that view displays, in a block each, and suppressing it
    /// would leave every live column empty.
    pub fn set_stream_text_sink(&self, sink: Option<StreamTextSink>, exclusive: bool) {
        *self.stream_text_sink.lock().unwrap() = sink;
        self.stream_text_sink_exclusive
            .store(exclusive, Ordering::Release);
    }

    /// Remove the live-text observer only if it is still `sink` — for an owner
    /// that may outlive its session (the Multi-STT streaming coordinator after
    /// a finish timeout) and must not clear a sink someone else installed since.
    pub fn clear_stream_text_sink_if(&self, sink: &StreamTextSink) {
        let mut current = self.stream_text_sink.lock().unwrap();
        if current.as_ref().is_some_and(|c| Arc::ptr_eq(c, sink)) {
            *current = None;
            self.stream_text_sink_exclusive
                .store(false, Ordering::Release);
        }
    }

    /// Running totals of the live stream worker's feed and compute time, for
    /// callers that need to report the streaming model's rate but do not run on
    /// the worker thread ([`StreamTiming`]). The experimental Multi-STT
    /// streaming coordinator is the only caller: the primary model streams
    /// continuously rather than decoding chunk by chunk, so the only way to say
    /// how fast it ran over a chunk is to difference these totals across the
    /// chunk's lifetime.
    pub fn stream_timing(&self) -> Arc<StreamTiming> {
        Arc::clone(&self.stream_timing)
    }

    pub fn transcribe(&self, audio: Vec<f32>) -> Result<String> {
        self.transcribe_internal(audio, None).map(|(text, _)| text)
    }

    pub fn transcribe_tracked(
        &self,
        audio: Vec<f32>,
        run: StatisticsRunContext,
    ) -> Result<TrackedTranscription> {
        match self.transcribe_internal(audio, Some(&run)) {
            Ok((text, Some(attempt))) => Ok(TrackedTranscription { text, attempt }),
            Ok(_) => {
                run.finish(StatisticsRunStatus::Failed);
                Err(anyhow::anyhow!(
                    "Tracked transcription ended before an inference attempt began"
                ))
            }
            Err(error) => {
                run.finish(StatisticsRunStatus::Failed);
                Err(error)
            }
        }
    }

    fn transcribe_internal(
        &self,
        audio: Vec<f32>,
        statistics: Option<&StatisticsRunContext>,
    ) -> Result<(String, Option<PendingStatisticsAttempt>)> {
        // Development-only fault injection: set (to any value) to make every
        // transcription fail, for exercising the failure paths.
        #[cfg(debug_assertions)]
        if crate::utils::app_env_var("FORCE_TRANSCRIPTION_FAILURE").is_some() {
            return Err(anyhow::anyhow!(
                "Simulated transcription failure: {}FORCE_TRANSCRIPTION_FAILURE is set",
                crate::app_identity::ENV_PREFIX
            ));
        }

        // Update last activity timestamp
        self.touch_activity();

        let st = std::time::Instant::now();
        let audio_len = audio.len();

        debug!("Audio vector length: {}", audio_len);

        if audio.is_empty() {
            debug!("Empty audio vector");
            self.maybe_unload_immediately("empty audio");
            return Ok((String::new(), None));
        }

        // Wait for any in-progress load, then require a loaded engine.
        {
            // If the model is loading, wait for it to complete.
            let mut is_loading = self.is_loading.lock().unwrap();
            while *is_loading {
                is_loading = self.loading_condvar.wait(is_loading).unwrap();
            }

            let engine_guard = self.lock_engine();
            if engine_guard.is_none() {
                return Err(anyhow::anyhow!("Model is not loaded for transcription."));
            }
        }

        // Get current settings for configuration
        let settings = get_settings(&self.app_handle);

        // Validate selected language against the model's supported languages.
        // If the language isn't supported, fall back to "auto" to prevent errors.
        // Validate against the model that's actually loaded (which can differ
        // from settings.selected_model when a caller loaded a specific model —
        // e.g. the --transcribe-file path's --model), not the persisted
        // selection.
        let active_model = self
            .get_current_model()
            .unwrap_or_else(|| settings.selected_model.clone());
        // Resolve the persisted language *intent* into the language this model
        // will actually use. The coercion is capability-aware (a must-pick model
        // never receives "auto") and computed fresh here — it is never written
        // back to settings, so the intent survives switching models and back.
        let validated_language =
            effective_language_for_model(&settings, self.model_manager.as_ref(), &active_model);
        if validated_language != settings.selected_language {
            debug!(
                "Language intent '{}' resolved to '{}' for model '{}'",
                settings.selected_language, validated_language, active_model
            );
        }

        // Whether the loaded model is actually whisper-family (arch string).
        // Non-whisper archs (e.g. Voxtral Small) can advertise
        // Feature::InitialPrompt yet reject the whisper-kind run extension
        // with INVALID_ARG, so the whisper extension must be gated on the
        // arch, not on the feature (see #1601).
        let model_is_whisper;

        // Perform transcription with the appropriate engine.
        // We use catch_unwind to prevent engine panics from poisoning the mutex,
        // which would make the app hang indefinitely on subsequent operations.
        let (result, output_language, model_languages, statistics_attempt) = {
            let mut engine_guard = self.lock_engine();

            // Take the engine out so we own it during transcription.
            // If the engine panics, we simply don't put it back (effectively unloading it)
            // instead of poisoning the mutex.
            let mut engine = match engine_guard.take() {
                Some(e) => e,
                None => {
                    return Err(anyhow::anyhow!(
                        "Model is not available for transcription (unloaded or in use). Please try again."
                    ));
                }
            };

            // Release the lock before transcribing — no mutex held during the engine call
            drop(engine_guard);

            let inference_started_at_ms = chrono::Utc::now().timestamp_millis();
            let (engine_name, backend_name) = match &engine {
                LoadedEngine::TranscribeCpp(session) => (
                    "transcribe_cpp".to_string(),
                    session.model().backend().to_string(),
                ),
            };

            let statistics_attempt = statistics.and_then(|run| {
                run.begin_attempt(
                    Some(active_model.clone()),
                    Some(engine_name),
                    Some(backend_name),
                    inference_started_at_ms,
                )
            });

            // Probe live transcribe-cpp capabilities once (cheap GGUF-metadata
            // reads); the loaded session is the source of truth, not the
            // ModelManager copy. The whisper run extension is kind-tagged, so
            // non-whisper archs (parakeet, voxtral, …) reject it with
            // INVALID_ARG; attach it — and translate — only where supported.
            let mut output_was_translated = false;
            let mut applied_language_hint: Option<String> = None;
            let mut model_detected_language: Option<String> = None;
            let (model_supports_translate, model_languages) = {
                let LoadedEngine::TranscribeCpp(session) = &engine;
                let model = session.model();
                let caps = model.capabilities();
                let model_takes_initial_prompt = model.supports(Feature::InitialPrompt);
                model_is_whisper = model.arch() == "whisper";
                debug!(
                    "transcribe-cpp model '{}' on '{}': initial_prompt={}, translate={}, languages={:?}",
                    active_model,
                    model.backend(),
                    model_takes_initial_prompt,
                    caps.supports_translate,
                    caps.languages
                );
                (caps.supports_translate, caps.languages)
            };

            let transcribe_result = catch_unwind(AssertUnwindSafe(|| -> Result<String> {
                match &mut engine {
                    LoadedEngine::TranscribeCpp(session) => {
                        // Custom words become the initial prompt ONLY for models
                        // that accept one (whisper family). Attaching the
                        // whisper run extension to a non-whisper arch is rejected
                        // with INVALID_ARG, so skip it there and let the fuzzy
                        // post-correction handle custom words instead.
                        let family = if settings.custom_words.is_empty() || !model_is_whisper {
                            None
                        } else {
                            Some(RunExtension::Whisper(WhisperRunOptions {
                                initial_prompt: Some(settings.custom_words.join(", ")),
                                ..Default::default()
                            }))
                        };

                        let run_plan = transcribe_cpp_run_plan(
                            settings.translate_to_english,
                            &validated_language,
                            &model_languages,
                            model_supports_translate,
                        );
                        output_was_translated = run_plan.target_language.as_deref() == Some("en");
                        applied_language_hint = run_plan.language.clone();

                        let run_options = RunOptions {
                            task: run_plan.task,
                            language: run_plan.language,
                            target_language: run_plan.target_language,
                            family,
                            ..Default::default()
                        };

                        debug!(
                            "transcribe-cpp run: task={:?}, language={:?}, initial_prompt={}",
                            run_options.task,
                            run_options.language,
                            run_options.family.is_some()
                        );

                        session
                            .run(&audio, &run_options)
                            .map(|t| {
                                // Whisper's audio-based LID (auto mode only;
                                // `None` when a language hint was passed).
                                info!(target: "pipeline", "batch model={} audio_ms={:.3} native={:?}",
                                    active_model, audio.len() as f64 / 16.0, t.timings);
                                model_detected_language = t.language;
                                *self.last_pipeline_metrics.lock().unwrap() = serde_json::json!({
                                    "timings": { "mel_ms": t.timings.mel_ms,
                                        "encode_ms": t.timings.encode_ms, "decode_ms": t.timings.decode_ms },
                                    "language": run_options.language,
                                });
                                t.text
                            })
                            .map_err(|e| {
                                anyhow::anyhow!("transcribe-cpp transcription failed: {}", e)
                            })
                    }
                }
            }));

            let text = match transcribe_result {
                Ok(inner_result) => {
                    // Success or normal error: return the engine unless a model
                    // switch/unload invalidated it while it was in use.
                    self.return_engine(engine, &active_model);
                    match inner_result {
                        Ok(t) => t,
                        Err(e) => {
                            if let Some(attempt) = &statistics_attempt {
                                attempt.complete_canonical("");
                                attempt.finish(StatisticsRunStatus::Failed);
                            }
                            return Err(e);
                        }
                    }
                }
                Err(panic_payload) => {
                    // Engine panicked — do NOT put it back (it's in an unknown state).
                    // The engine is dropped here, effectively unloading it.
                    if let Some(attempt) = &statistics_attempt {
                        attempt.complete_canonical("");
                        attempt.finish(StatisticsRunStatus::Failed);
                    }

                    let panic_msg = panic_payload_message(panic_payload.as_ref());
                    error!(
                        "Transcription engine panicked: {}. Model has been unloaded.",
                        panic_msg
                    );

                    // Clear the model ID so it will be reloaded on next attempt
                    {
                        let mut current_model = self
                            .current_model_id
                            .lock()
                            .unwrap_or_else(|e| e.into_inner());
                        *current_model = None;
                    }

                    let _ = self.app_handle.emit(
                        "model-state-changed",
                        ModelStateEvent {
                            event_type: "unloaded".to_string(),
                            model_id: None,
                            model_name: None,
                            error: Some(format!("Engine panicked: {}", panic_msg)),
                        },
                    );

                    return Err(anyhow::anyhow!(
                        "Transcription engine panicked: {}. The model has been unloaded and will reload on next attempt.",
                        panic_msg
                    ));
                }
            };

            let output_language = with_model_detected_language(
                resolve_output_language_evidence(
                    &settings,
                    applied_language_hint.as_deref(),
                    &model_languages,
                    output_was_translated,
                ),
                model_detected_language,
            );
            debug!("Output language evidence: {:?}", output_language);

            (text, output_language, model_languages, statistics_attempt)
        };

        // Apply fuzzy word correction if custom words are configured — UNLESS the
        // words were already handed to the model as an initial prompt (whisper
        // family). We don't pass a prompt to non-whisper models (it requires the
        // whisper-kind run extension), so they still get fuzzy correction here.
        let filtered_result = post_process_transcription_text(
            result,
            &settings,
            model_is_whisper,
            &output_language,
            &model_languages,
        );

        let et = std::time::Instant::now();
        let translation_note =
            if matches!(output_language, OutputLanguageEvidence::TranslatedToEnglish) {
                " (translated)"
            } else {
                ""
            };
        // Real-time factor. Input PCM is 16 kHz mono, so audio length in seconds
        // is samples / 16000. `speedup` is audio_secs / elapsed_secs — e.g. 4.00x
        // means transcribed 4x faster than real time
        let elapsed_secs = (et - st).as_secs_f64();
        let audio_secs = audio_len as f64 / 16_000.0;
        let speedup = real_time_factor(audio_secs, elapsed_secs);
        info!(
            "Transcription completed in {:.2}s for {:.2}s of audio ({:.2}x real-time){}",
            elapsed_secs, audio_secs, speedup, translation_note
        );

        let final_result = filtered_result;

        if final_result.is_empty() {
            info!("Transcription result is empty");
        } else {
            info!(
                "Transcription result: {}",
                crate::utils::redact_text(&final_result)
            );
        }

        if let Some(attempt) = &statistics_attempt {
            attempt.complete_canonical(&final_result);
            if final_result.trim().is_empty() {
                attempt.finish(StatisticsRunStatus::Empty);
            }
        }

        self.maybe_unload_immediately("transcription");

        Ok((final_result, statistics_attempt))
    }
}

/// Process-lifetime totals of what the stream worker has consumed and spent.
///
/// [`StreamPerf`] is worker-local: it is created inside `run_stream_worker`
/// and dies with it, so nothing outside that thread can read how much audio the
/// live stream has been fed or how much model compute it has burned. The
/// experimental Multi-STT streaming coordinator needs exactly those two numbers
/// to report the primary model's rate at each chunk close — it cannot time a
/// per-chunk decode because the primary does not decode per chunk, it streams
/// continuously.
///
/// Two relaxed atomic adds per feed, read on demand. Never reset: a reader
/// takes a baseline once and reports deltas, so a counter that outlives a
/// session is harmless and a reset racing a reader is not. Both counters are
/// monotonic, so a delta is always `now.saturating_sub(baseline)`.
#[derive(Debug, Default)]
pub struct StreamTiming {
    /// 16 kHz samples handed to `stream.feed()`.
    fed_samples: AtomicU64,
    /// Microseconds spent inside `feed()` / `finalize()`.
    compute_micros: AtomicU64,
}

impl StreamTiming {
    fn record_feed(&self, samples: usize) {
        self.fed_samples
            .fetch_add(samples as u64, Ordering::Relaxed);
    }

    fn record_compute(&self, elapsed: Duration) {
        self.compute_micros
            .fetch_add(elapsed.as_micros() as u64, Ordering::Relaxed);
    }

    /// `(fed_samples, compute_micros)` as one consistent-enough read. The two
    /// loads are separate, so a feed landing between them reports the sample
    /// count of a slightly later moment than the compute time; at a chunk
    /// boundary that is at most one 16 ms frame and only ever makes the
    /// reported rate marginally conservative.
    pub fn totals(&self) -> (u64, u64) {
        (
            self.fed_samples.load(Ordering::Relaxed),
            self.compute_micros.load(Ordering::Relaxed),
        )
    }

    /// Turn a `(fed_samples, compute_micros)` delta into `(audio_secs,
    /// compute_secs)`, the pair every rate line in the log is built from.
    pub fn secs(delta: (u64, u64)) -> (f64, f64) {
        (delta.0 as f64 / 16_000.0, delta.1 as f64 / 1_000_000.0)
    }
}

struct StreamPerf {
    /// Shared with the manager so a reader outside the worker thread can see
    /// the same totals this struct accumulates for its own periodic log.
    timing: Arc<StreamTiming>,
    feed_count: u64,
    emit_count: u64,
    streamed_samples: u64,
    stream_compute_elapsed: Duration,
    last_log: Instant,
    queue_max: Duration,
    call_max: Duration,
    latest_revision: i32,
    latest_input_received_ms: i64,
    latest_audio_committed_ms: i64,
    latest_buffered_ms: i64,
}

impl StreamPerf {
    fn new(timing: Arc<StreamTiming>) -> Self {
        Self {
            timing,
            feed_count: 0,
            emit_count: 0,
            streamed_samples: 0,
            stream_compute_elapsed: Duration::ZERO,
            last_log: Instant::now(),
            queue_max: Duration::ZERO,
            call_max: Duration::ZERO,
            latest_revision: 0,
            latest_input_received_ms: 0,
            latest_audio_committed_ms: 0,
            latest_buffered_ms: 0,
        }
    }

    fn record_feed(&mut self, samples: usize) {
        self.feed_count += 1;
        self.streamed_samples += samples as u64;
        self.timing.record_feed(samples);
    }

    fn record_compute(&mut self, elapsed: Duration) {
        self.call_max = self.call_max.max(elapsed);
        self.stream_compute_elapsed += elapsed;
        self.timing.record_compute(elapsed);
    }

    fn record_update(
        &mut self,
        revision: i32,
        input_received_ms: i64,
        audio_committed_ms: i64,
        buffered_ms: i64,
    ) {
        self.latest_revision = revision;
        self.latest_input_received_ms = input_received_ms;
        self.latest_audio_committed_ms = audio_committed_ms;
        self.latest_buffered_ms = buffered_ms;
    }

    fn record_emit(&mut self) {
        self.emit_count += 1;
    }

    fn maybe_log(&mut self) {
        if self.last_log.elapsed() < STREAM_PERF_LOG_INTERVAL {
            return;
        }

        let audio_secs = self.audio_secs();
        let compute_secs = self.compute_secs();
        debug!(
            "Live preview perf: {:.2}s streamed audio, {:.2}s model compute ({:.2}x real-time), \
             input_received={:.2}s, committed_audio={:.2}s, buffered={}ms, revision={}, \
             {} frames fed, {} updates emitted",
            audio_secs,
            compute_secs,
            real_time_factor(audio_secs, compute_secs),
            self.latest_input_received_ms as f64 / 1000.0,
            self.latest_audio_committed_ms as f64 / 1000.0,
            self.latest_buffered_ms,
            self.latest_revision,
            self.feed_count,
            self.emit_count,
        );
        self.last_log = Instant::now();
    }

    fn log_finalized(&self, chars: usize) {
        info!(target: "pipeline", "stream queue_max_ms={:.3} call_max_ms={:.3} audio_ms={:.3} compute_ms={:.3}",
            self.queue_max.as_secs_f64()*1000.0, self.call_max.as_secs_f64()*1000.0,
            self.audio_secs()*1000.0, self.compute_secs()*1000.0);
        let audio_secs = self.audio_secs();
        let compute_secs = self.compute_secs();
        info!(
            "Live preview finalized in {:.2}s model compute for {:.2}s streamed audio ({:.2}x real-time): \
             input_received={:.2}s, committed_audio={:.2}s, buffered={}ms, revision={}, \
             {} frames fed, {} updates emitted, {} chars",
            compute_secs,
            audio_secs,
            real_time_factor(audio_secs, compute_secs),
            self.latest_input_received_ms as f64 / 1000.0,
            self.latest_audio_committed_ms as f64 / 1000.0,
            self.latest_buffered_ms,
            self.latest_revision,
            self.feed_count,
            self.emit_count,
            chars
        );
    }

    fn audio_secs(&self) -> f64 {
        self.streamed_samples as f64 / 16_000.0
    }

    fn compute_secs(&self) -> f64 {
        self.stream_compute_elapsed.as_secs_f64()
    }
}

/// Duration of benchmark reference audio. Recordings handed to the benchmark
/// are always already resampled to the 16 kHz the engines consume.
fn benchmark_audio_secs(audio: &[f32]) -> f64 {
    audio.len() as f64 / 16_000.0
}

/// Audio seconds per compute second — the `Nx real-time` every rate line in the
/// log reports. `0.0` when nothing was computed (reported as such rather than
/// as an infinite rate). Shared with the experimental Multi-STT streaming
/// coordinator so its per-chunk primary-model line and this module's
/// `Live preview perf` line mean the same thing by the same arithmetic.
pub(crate) fn real_time_factor(audio_secs: f64, compute_secs: f64) -> f64 {
    if compute_secs > 0.0 {
        audio_secs / compute_secs
    } else {
        0.0
    }
}

/// Nearest-rank percentile of an already ascending-sorted list of durations.
///
/// `0.0` for an empty list rather than NaN: an empty list means nothing
/// qualified (no feed reached the "busy" threshold), and a reported zero is
/// interpretable while a NaN is not.
fn percentile_ms(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let rank = (p * sorted.len() as f64).ceil() as usize;
    sorted[rank.saturating_sub(1).min(sorted.len() - 1)]
}

/// Log form of a streaming latency setting: `560 ms (480 ms lookahead)`,
/// `320 ms` for R2T2, or `model default` when the model has no control.
fn describe_latency(
    point: Option<crate::managers::native_streaming_latency::LatencyPoint>,
) -> String {
    match point {
        None => "model default".to_string(),
        Some(point) if point.lookahead_ms == 0 => format!("{} ms", point.latency_ms),
        Some(point) => format!(
            "{} ms ({} ms lookahead)",
            point.latency_ms, point.lookahead_ms
        ),
    }
}

pub(crate) fn normalize_cjk_language(language: &str) -> &str {
    match language {
        "zh-Hans" | "zh-Hant" => "zh",
        other => other,
    }
}

/// Resolve the persisted language intent into the language a specific model can
/// use without writing the coerced value back to settings.
fn effective_language_for_model(
    settings: &AppSettings,
    model_manager: &ModelManager,
    model_id: &str,
) -> String {
    match model_manager.get_model_info(model_id) {
        Some(info) => crate::managers::model::effective_language(
            &settings.selected_language,
            &info.supported_languages,
            info.supports_language_detection,
        ),
        None => settings.selected_language.clone(),
    }
}

/// Resolve how confidently the app knows the language of the text produced by a
/// transcription run. The UI language is deliberately not part of this
/// decision.
fn resolve_output_language_evidence(
    settings: &AppSettings,
    applied_language_hint: Option<&str>,
    supported_languages: &[String],
    translated_to_english: bool,
) -> OutputLanguageEvidence {
    if translated_to_english {
        return OutputLanguageEvidence::TranslatedToEnglish;
    }

    // Stored language intent is only evidence when this specific engine run
    // actually received the hint. Some multilingual engines (notably Parakeet
    // V3) always auto-detect and ignore the app's selection; transcribe-cpp also
    // drops a requested hint when the loaded model does not advertise it.
    if let Some(language) = applied_language_hint.filter(|lang| !lang.is_empty() && *lang != "auto")
    {
        if settings.selected_language != "auto"
            && crate::managers::model::canonical_language_code(&settings.selected_language)
                == crate::managers::model::canonical_language_code(language)
        {
            return OutputLanguageEvidence::UserSelected(language.to_string());
        }

        // The engine may have required a concrete fallback even though the
        // user's persisted language was auto or unsupported.
        return OutputLanguageEvidence::ModelConstrained(language.to_string());
    }

    // A single-language model has a known output language without needing a
    // selectable language hint.
    if let [language] = supported_languages {
        return OutputLanguageEvidence::ModelConstrained(language.clone());
    }

    OutputLanguageEvidence::Unknown
}

/// Upgrade [`OutputLanguageEvidence::Unknown`] with the language the model
/// itself detected during the run (audio-based LID, e.g. Whisper in auto
/// mode). Stronger evidence resolved before the run is never overridden.
fn with_model_detected_language(
    evidence: OutputLanguageEvidence,
    detected: Option<String>,
) -> OutputLanguageEvidence {
    match (evidence, detected) {
        (OutputLanguageEvidence::Unknown, Some(language))
            if !language.is_empty() && language != "auto" =>
        {
            OutputLanguageEvidence::ModelDetected(language)
        }
        (evidence, _) => evidence,
    }
}

struct TranscribeCppRunPlan {
    task: Task,
    language: Option<String>,
    target_language: Option<String>,
}

/// Build the transcribe-cpp language/task options shared by batch and live
/// streaming paths.
fn transcribe_cpp_run_plan(
    translate_to_english: bool,
    effective_language: &str,
    model_languages: &[String],
    model_supports_translate: bool,
) -> TranscribeCppRunPlan {
    let requested_language = match effective_language {
        "auto" => None,
        other => Some(normalize_cjk_language(other).to_string()),
    };
    // Only pass a language the loaded model actually advertises (per
    // capabilities().languages); otherwise auto-detect rather than failing with
    // UNSUPPORTED_LANGUAGE. Language-agnostic models report an empty list, so
    // they always stay on auto.
    let language = requested_language.filter(|lang| model_languages.iter().any(|l| l == lang));
    let (task, target_language) = cpp_translation_task(
        translate_to_english,
        model_supports_translate,
        language.as_deref(),
    );

    TranscribeCppRunPlan {
        task,
        language,
        target_language,
    }
}

/// The Multi-STT panel's model for a nested-mode stream slot, if one is set
/// there: slot 1 is the panel's second model, slot 2 its third, and so on — the
/// same `slot = list position + 1` numbering the overlay's columns are marked
/// with. `None` for the primary, for a slot past the panel's models, and for a
/// slot whose model was cleared.
///
/// The inverse of [`apply_extra_model_settings`], which resolves the other way
/// (from a model id, because that is what a decoder has in hand). This direction
/// exists for the account a finished stream has to give of itself, where the
/// slot is what the caller has.
fn multi_stt_extra_model(settings: &AppSettings, slot: u8) -> Option<String> {
    let index = slot.checked_sub(1)? as usize;
    settings
        .multi_stt_extra_model_ids()
        .into_iter()
        .nth(index)?
}

/// Fold a Multi-STT slot's own preferences into the settings a decode of that
/// slot's model runs with: the language the user pinned for that model and
/// whether it should translate to English. Shared by the batch extra path and
/// the experimental Multi Streaming STT mode's second stream, so a model used
/// both ways behaves the same.
///
/// A model that is not one of the configured slots keeps the global settings.
fn apply_extra_model_settings(settings: &mut AppSettings, model_id: &str) {
    let Some(slot) = settings.multi_stt_extra_slot_for(model_id).cloned() else {
        return;
    };
    if let Some(language) = slot.language {
        settings.selected_language = language;
    }
    settings.translate_to_english = slot.translate;
    // The slot's own latency wins over the per-model entry, which is shared
    // with every quant sibling (possibly the primary). Written under the exact
    // id, which `get_effective_latency_preset` / `get_effective_r2t2_chunk_ms`
    // consult first, so `resolved_stream_extension` needs no slot awareness.
    if let Some(preset) = slot.latency_preset {
        settings
            .native_streaming_latency_presets
            .insert(model_id.to_string(), preset);
    }
    if let Some(chunk_ms) = slot.chunk_ms {
        settings
            .native_streaming_chunk_ms
            .insert(model_id.to_string(), chunk_ms);
    }
}

fn post_process_transcription_text(
    raw: String,
    settings: &AppSettings,
    custom_words_already_prompted: bool,
    output_language: &OutputLanguageEvidence,
    supported_languages: &[String],
) -> String {
    fail_open_text_transform(raw, |raw| {
        let corrected = if !settings.custom_words.is_empty() && !custom_words_already_prompted {
            apply_custom_words(
                &raw,
                &settings.custom_words,
                settings.word_correction_threshold,
            )
        } else {
            raw
        };

        // Last-resort language evidence: confidence-gated detection from the
        // transcribed text itself, constrained to the model's languages. Only
        // consulted when it can change the outcome (built-in gated fillers).
        let output_language = match output_language {
            OutputLanguageEvidence::Unknown
                if settings.filler_word_removal_enabled
                    && settings.custom_filler_words.is_none() =>
            {
                match detect_output_language(&corrected, supported_languages) {
                    Some(language) => {
                        debug!("Text-based language detection resolved '{}'", language);
                        OutputLanguageEvidence::TextDetected(language)
                    }
                    None => OutputLanguageEvidence::Unknown,
                }
            }
            other => other.clone(),
        };

        let without_fillers = remove_filler_words(
            &corrected,
            &output_language,
            &settings.custom_filler_words,
            settings.filler_word_removal_enabled,
        );

        normalize_transcription_output(&without_fillers)
    })
}

/// Optional text cleanup must never discard a successful model result. The
/// transform is pure and owns its input, so recovering the untouched text is
/// safe even if a bug in custom-word or filler filtering unwinds.
fn fail_open_text_transform<F>(raw: String, transform: F) -> String
where
    F: FnOnce(String) -> String,
{
    let fallback = raw.clone();
    match catch_unwind(AssertUnwindSafe(|| transform(raw))) {
        Ok(processed) => processed,
        Err(payload) => {
            error!(
                "Optional transcription text post-processing panicked: {}; using the raw transcription",
                panic_payload_message(payload.as_ref())
            );
            fallback
        }
    }
}

/// Decide a transcribe-cpp run's task + translation target from settings.
///
/// "Translate to English" only fires where the model advertises translation.
/// transcribe-cpp requires an explicit `target_language`: a null target defaults to the *source*, so a non-English
/// source silently becomes e.g. es→es and Canary rejects the unadvertised pair.
/// An English source is skipped entirely — en→en is not a real translation, and
/// it's reachable by default since auto-detect-less models coerce intent to "en".
///
/// Returns `(task, target_language)` ready to drop into `RunOptions`.
fn cpp_translation_task(
    translate_to_english: bool,
    model_supports_translate: bool,
    source_language: Option<&str>,
) -> (Task, Option<String>) {
    let translate_to_en =
        translate_to_english && model_supports_translate && source_language != Some("en");
    if translate_to_en {
        (Task::Translate, Some("en".to_string()))
    } else {
        (Task::Transcribe, None)
    }
}

/// Drain a stream command channel, ignoring fed audio, until the caller
/// finalizes or cancels. Used when streaming can't actually run (model not
/// loaded / not streaming-capable) so the finalize handshake still completes
/// and the caller falls back to batch transcription.
fn drain_until_finalize(rx: mpsc::Receiver<StreamCmd>, result: StreamWorkerResult) {
    while let Ok(cmd) = rx.recv() {
        match cmd {
            StreamCmd::Feed { .. } => {}
            StreamCmd::Finalize(reply) => {
                let _ = reply.send(result);
                break;
            }
            StreamCmd::Cancel => break,
        }
    }
}

// ── Multi-STT extra model methods ────────────────────────────────────────────

impl TranscriptionManager {
    /// Load an additional model engine for multi-STT mode.
    /// Returns the loaded engine's model name.
    pub fn load_extra_model(&self, model_id: &str) -> Result<String> {
        // Coalesce concurrent loads of the same id: the second caller waits
        // for the first to finish and shares its outcome.
        {
            let (lock, cvar) = &*self.extra_loading;
            let mut loading = lock.lock().unwrap();
            if loading.contains(model_id) {
                info!(
                    "Extra model '{}' is already being loaded; waiting for that load",
                    model_id
                );
                while loading.contains(model_id) {
                    loading = cvar.wait(loading).unwrap();
                }
                drop(loading);
                return if self.is_extra_model_loaded(model_id) {
                    Ok(self
                        .model_manager
                        .get_model_info(model_id)
                        .map(|info| info.name)
                        .unwrap_or_else(|| model_id.to_string()))
                } else {
                    Err(anyhow::anyhow!(
                        "Concurrent load of extra model '{}' failed",
                        model_id
                    ))
                };
            }
            loading.insert(model_id.to_string());
        }

        let result = self.load_extra_model_uncoalesced(model_id);

        {
            let (lock, cvar) = &*self.extra_loading;
            lock.lock().unwrap().remove(model_id);
            cvar.notify_all();
        }
        result
    }

    fn load_extra_model_uncoalesced(&self, model_id: &str) -> Result<String> {
        let load_start = std::time::Instant::now();
        info!("Starting to load extra model for multi-STT: {}", model_id);

        let load_result = (|| {
            let model_info = self
                .model_manager
                .get_model_info(model_id)
                .ok_or_else(|| anyhow::anyhow!("Extra model not found: {}", model_id))?;

            if !model_info.is_downloaded {
                return Err(anyhow::anyhow!(
                    "Extra model '{}' is not downloaded",
                    model_id
                ));
            }

            let model_path = self.model_manager.get_model_path(model_id)?;

            let loaded_engine = self.create_engine(model_id, &model_path)?;

            {
                let mut extra = self.extra_engines.lock().unwrap();
                // A fresh load supersedes any pending unload request.
                self.extra_unload_requests.lock().unwrap().remove(model_id);
                extra.insert(model_id.to_string(), loaded_engine);
            }

            Ok(model_info.name)
        })();

        match load_result {
            Ok(model_name) => {
                let load_duration = load_start.elapsed();
                info!(
                    "Loaded extra model '{}' for multi-STT (took {}ms)",
                    model_id,
                    load_duration.as_millis()
                );

                // Emit event for frontend
                let _ = self.app_handle.emit(
                    "model-state-changed",
                    ModelStateEvent {
                        event_type: "multi_stt_model_loaded".to_string(),
                        model_id: Some(model_id.to_string()),
                        model_name: Some(model_name.clone()),
                        error: None,
                    },
                );

                Ok(model_name)
            }
            Err(e) => {
                // Distinct event type: the App-level listener toasts this while
                // the primary model selector's state stays untouched.
                let _ = self.app_handle.emit(
                    "model-state-changed",
                    ModelStateEvent {
                        event_type: "multi_stt_model_load_failed".to_string(),
                        model_id: Some(model_id.to_string()),
                        model_name: None,
                        error: Some(e.to_string()),
                    },
                );
                Err(e)
            }
        }
    }

    /// Unload an extra model engine.
    ///
    /// If the engine is currently leased out (an in-flight decode or a live
    /// stream) it cannot be dropped here; the unload request is recorded instead
    /// and the engine is dropped when `return_extra_engine` gets it back.
    pub fn unload_extra_model(&self, model_id: &str) -> Result<()> {
        info!("Unloading extra model: {}", model_id);
        let removed = {
            let mut extra = self.extra_engines.lock().unwrap();
            let removed = extra.remove(model_id).is_some();
            if !removed {
                self.extra_unload_requests
                    .lock()
                    .unwrap()
                    .insert(model_id.to_string());
            }
            removed
        };
        if removed {
            info!("Extra model '{}' unloaded", model_id);
        } else {
            info!(
                "Extra model '{}' is not in the map (leased or not loaded); unload deferred until its lease ends",
                model_id
            );
        }

        let _ = self.app_handle.emit(
            "model-state-changed",
            ModelStateEvent {
                event_type: "multi_stt_model_unloaded".to_string(),
                model_id: Some(model_id.to_string()),
                model_name: None,
                error: None,
            },
        );

        Ok(())
    }

    /// Unload all extra (multi-STT) model engines at once. Used during app
    /// shutdown so the 2nd, 3rd, and 4th models are freed just like the primary model.
    ///
    /// Engines sitting in `extra_engines` are dropped directly. Engines
    /// currently leased out (an in-flight decode or a live stream) are not in
    /// the map, so every configured Multi-STT slot gets an entry in
    /// `extra_unload_requests` — `return_extra_engine` then drops a leased
    /// engine instead of re-inserting it. A request for a slot model that is
    /// not loaded at all is harmless: the next `load_extra_model` clears it.
    pub fn unload_all_extra_models(&self) {
        let to_unload: Vec<String> = {
            let mut extra = self.extra_engines.lock().unwrap();
            let ids: Vec<String> = extra.keys().cloned().collect();
            extra.clear();
            ids
        };

        let settings = get_settings(&self.app_handle);
        {
            let mut pending = self.extra_unload_requests.lock().unwrap();
            for id in settings
                .multi_stt_extra_model_ids()
                .into_iter()
                .flatten()
                .filter(|id| !to_unload.contains(id))
            {
                pending.insert(id);
            }
        }

        for model_id in &to_unload {
            info!("Extra model '{}' unloaded during full cleanup", model_id);
            let _ = self.app_handle.emit(
                "model-state-changed",
                ModelStateEvent {
                    event_type: "multi_stt_model_unloaded".to_string(),
                    model_id: Some(model_id.clone()),
                    model_name: None,
                    error: None,
                },
            );
        }
    }

    /// Lease one extra model's engine for the whole of a live streaming session.
    ///
    /// [`Self::transcribe_with_extra`] leases per decode — it removes the engine
    /// from the map, decodes, and puts it back — which is right for a batch decode
    /// and wrong for a stream: an unrelated decode while the extra's stream is
    /// live would take the very engine that stream is reading from. The streaming
    /// session takes it out once, at arm time, and holds it until the session ends.
    ///
    /// Private because a lease is only meaningful to the worker that will hold it:
    /// [`Self::start_extra_stream_when_loaded`] leases and starts in one step, so
    /// no caller ever holds an engine of its own.
    fn lease_extra_engine(&self, model_id: &str) -> Result<LoadedEngine> {
        let mut extra = self.extra_engines.lock().unwrap();
        extra
            .remove(model_id)
            .ok_or_else(|| anyhow::anyhow!("Extra model '{}' is not loaded", model_id))
    }

    /// Give a leased engine back — after a per-decode lease
    /// ([`Self::transcribe_with_extra_internal`], `during` = "transcription") or
    /// a whole live stream (`during` = "its live stream"). An unload that was
    /// asked for while it was out is honoured here rather than lost: the request
    /// flag, or `Immediately` with the Multi-STT pin off (mirroring the primary
    /// model's `maybe_unload_immediately`), frees the engine instead of
    /// reinserting it.
    fn return_extra_engine(&self, model_id: &str, engine: LoadedEngine, during: &str) {
        let settings = get_settings(&self.app_handle);
        let mut extra = self.extra_engines.lock().unwrap();
        let unload_requested = self.extra_unload_requests.lock().unwrap().remove(model_id);
        let unload_immediately = settings.model_unload_timeout == ModelUnloadTimeout::Immediately
            && !settings.multi_stt_keep_extra_models_loaded;
        if unload_requested || unload_immediately {
            if unload_immediately {
                info!(
                    "Immediately unloading extra model '{}' after {}",
                    model_id, during
                );
            } else {
                info!(
                    "Extra model '{}' was unloaded during {}; freeing its engine",
                    model_id, during
                );
            }
            drop(extra);
            drop(engine);
            self.emit_extra_model_unloaded(model_id);
        } else {
            extra.insert(model_id.to_string(), engine);
        }
    }

    /// The family-specific stream extension for `model_id` under the user's
    /// settings: the latency preset (default `Accurate`), or for R2T2 the
    /// `native_streaming_chunk_ms` value. One resolver for the live path and
    /// the headless benchmark, so the two can never measure different cadences.
    fn resolved_stream_extension(
        &self,
        settings: &AppSettings,
        model: &transcribe_cpp::Model,
        model_id: &str,
    ) -> Option<transcribe_cpp::StreamExtension> {
        let latency_kind = self
            .model_manager
            .get_model_info(model_id)
            .and_then(|info| info.native_streaming_latency_kind);
        let preset = get_effective_latency_preset(settings, model_id);
        let chunk_ms = get_effective_r2t2_chunk_ms(settings, model_id);
        crate::managers::native_streaming_latency::stream_extension_for(
            model,
            model_id,
            latency_kind,
            preset,
            chunk_ms,
        )
    }

    fn emit_extra_model_unloaded(&self, model_id: &str) {
        let _ = self.app_handle.emit(
            "model-state-changed",
            ModelStateEvent {
                event_type: "multi_stt_model_unloaded".to_string(),
                model_id: Some(model_id.to_string()),
                model_name: None,
                error: None,
            },
        );
    }

    /// Transcribe audio with one of the extra model engines.
    /// The engine is temporarily removed from the map, used, and returned.
    pub fn transcribe_with_extra(&self, model_id: &str, audio: Vec<f32>) -> Result<String> {
        self.transcribe_with_extra_internal(model_id, audio, None)
            .map(|(text, _)| text)
    }

    pub fn transcribe_with_extra_tracked(
        &self,
        model_id: &str,
        audio: Vec<f32>,
        run: StatisticsRunContext,
    ) -> Result<TrackedTranscription> {
        match self.transcribe_with_extra_internal(model_id, audio, Some(&run)) {
            Ok((text, Some(attempt))) => Ok(TrackedTranscription { text, attempt }),
            Ok(_) => {
                run.finish(StatisticsRunStatus::Failed);
                Err(anyhow::anyhow!(
                    "Tracked extra transcription ended before an inference attempt began"
                ))
            }
            Err(error) => {
                run.finish(StatisticsRunStatus::Failed);
                Err(error)
            }
        }
    }

    fn transcribe_with_extra_internal(
        &self,
        model_id: &str,
        audio: Vec<f32>,
        statistics: Option<&StatisticsRunContext>,
    ) -> Result<(String, Option<PendingStatisticsAttempt>)> {
        self.touch_activity();

        if audio.is_empty() {
            debug!("Empty audio vector for extra model '{}'", model_id);
            return Ok((String::new(), None));
        }

        let mut engine = {
            let mut extra = self.extra_engines.lock().unwrap();
            extra
                .remove(model_id)
                .ok_or_else(|| anyhow::anyhow!("Extra model '{}' is not loaded", model_id))?
        };

        let inference_started_at_ms = chrono::Utc::now().timestamp_millis();
        let (engine_name, backend_name) = match &engine {
            LoadedEngine::TranscribeCpp(session) => (
                "transcribe_cpp".to_string(),
                session.model().backend().to_string(),
            ),
        };

        let statistics_attempt = statistics.and_then(|run| {
            run.begin_attempt(
                Some(model_id.to_string()),
                Some(engine_name),
                Some(backend_name),
                inference_started_at_ms,
            )
        });

        let st = std::time::Instant::now();
        let audio_len = audio.len();
        let mut settings = get_settings(&self.app_handle);
        apply_extra_model_settings(&mut settings, model_id);

        let result = catch_unwind(AssertUnwindSafe(|| {
            transcribe_with_engine(
                &mut engine,
                &settings,
                &audio,
                model_id,
                &self.model_manager,
            )
        }));

        let (transcription, engine_to_return) = match result {
            Ok(Ok(output)) => {
                let filtered = post_process_transcription_text(
                    output.text,
                    &settings,
                    output.custom_words_already_prompted,
                    &output.output_language,
                    &output.supported_languages,
                );
                (Ok(filtered), Some(engine))
            }
            Ok(Err(e)) => (Err(e), Some(engine)),
            Err(panic_payload) => {
                let panic_msg = panic_payload_message(panic_payload.as_ref());
                error!(
                    "Extra model '{}' engine panicked: {}. Model has been unloaded.",
                    model_id, panic_msg
                );
                (
                    Err(anyhow::anyhow!(
                        "Extra model '{}' engine panicked: {}",
                        model_id,
                        panic_msg
                    )),
                    None,
                )
            }
        };

        // The surviving engine goes back to the map, or is freed when an unload
        // was requested while it was leased out (or Immediately with the pin off).
        // A panicked engine is already gone: tell the UI, and drop any request
        // that was waiting for it.
        match engine_to_return {
            Some(eng) => self.return_extra_engine(model_id, eng, "transcription"),
            None => {
                self.extra_unload_requests.lock().unwrap().remove(model_id);
                self.emit_extra_model_unloaded(model_id);
            }
        }

        match transcription {
            Ok(ref text) => {
                let et = std::time::Instant::now();
                let elapsed_secs = (et - st).as_secs_f64();
                let audio_secs = audio_len as f64 / 16_000.0;
                let speedup = real_time_factor(audio_secs, elapsed_secs);
                info!(
                    "Multi-STT: extra model '{}' transcribed in {:.2}s for {:.2}s of audio ({:.2}x real-time): '{}'",
                    model_id,
                    elapsed_secs,
                    audio_secs,
                    speedup,
                    utils::redact_text(text)
                );
                if let Some(ref attempt) = statistics_attempt {
                    attempt.complete_canonical(text);
                    if text.trim().is_empty() {
                        attempt.finish(StatisticsRunStatus::Empty);
                    }
                }
                Ok((text.clone(), statistics_attempt))
            }
            Err(e) => {
                if let Some(ref attempt) = statistics_attempt {
                    attempt.complete_canonical("");
                    attempt.finish(StatisticsRunStatus::Failed);
                }
                Err(e)
            }
        }
    }

    /// Emit a `benchmark-progress` event to the frontend, ignoring transport
    /// errors (a closed window must never abort a benchmark run).
    fn emit_benchmark_progress(&self, event: BenchmarkProgressEvent) {
        let _ = self.app_handle.emit("benchmark-progress", event);
    }

    // Seven is clippy's default argument ceiling; the eighth is `timed_runs`,
    // the run count the UI picks, and bundling the rest into a plan struct
    // would only move the same fields one level down.
    #[allow(clippy::too_many_arguments)]
    /// Load `variant_id` on a throwaway engine, run one discarded warmup pass
    /// and `timed_runs` timed passes over `audio`, then drop the engine and
    /// let the OS reclaim its memory.
    ///
    /// The warmup pass is emitted as `warmup_completed` and its timing is
    /// never pushed into `times_ms`, so it can never reach the average. Each
    /// timed run emits a `run_completed` event so the UI can show live
    /// progress. A failing or panicking run emits `variant_error` and stops
    /// the remaining runs; whatever timings were already collected are
    /// returned, so an empty vector means the variant produced no usable
    /// measurement.
    fn time_variant_runs(
        &self,
        variant_id: &str,
        quant: &str,
        model_path: &std::path::Path,
        settings: &AppSettings,
        model_options: &ModelOptions,
        audio: &[f32],
        timed_runs: usize,
    ) -> Result<Vec<f64>> {
        let audio_secs = benchmark_audio_secs(audio);

        // A temporary engine configured with the benchmark's target backend and device,
        // so every quantization variant is evaluated on the exact same compute backend.
        let mut engine = self.create_engine_with_options(variant_id, model_path, model_options)?;

        // Warmup: the first transcription after a load is slow (cold caches,
        // lazy initialization). Per the benchmark spec it is discarded — its
        // timing is dropped on the floor below, never recorded.
        let warmup = catch_unwind(AssertUnwindSafe(|| {
            transcribe_with_engine(
                &mut engine,
                settings,
                audio,
                variant_id,
                &self.model_manager,
            )
        }));
        match warmup {
            Ok(Ok(_)) => debug!(
                "Benchmark [{}] warmup run finished (discarded, not averaged)",
                quant
            ),
            Ok(Err(e)) => warn!(
                "Benchmark [{}] warmup run failed, continuing with the timed runs: {}",
                quant, e
            ),
            Err(panic_payload) => warn!(
                "Benchmark [{}] warmup run panicked, continuing with the timed runs: {}",
                quant,
                panic_payload_message(panic_payload.as_ref())
            ),
        }
        self.emit_benchmark_progress(BenchmarkProgressEvent {
            event_type: "warmup_completed".to_string(),
            quant: Some(quant.to_string()),
            model_id: Some(variant_id.to_string()),
            audio_secs: Some(audio_secs),
            run_index: Some(0),
            total_runs: Some(timed_runs as u32),
            ..Default::default()
        });

        let mut times_ms: Vec<f64> = Vec::with_capacity(timed_runs);
        for run_idx in 0..timed_runs {
            let start = Instant::now();
            let result = catch_unwind(AssertUnwindSafe(|| {
                transcribe_with_engine(
                    &mut engine,
                    settings,
                    audio,
                    variant_id,
                    &self.model_manager,
                )
            }));
            let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;

            let failure = match result {
                Ok(Ok(_)) => {
                    times_ms.push(elapsed_ms);
                    debug!(
                        "Benchmark [{}] run {}/{}: {:.0}ms",
                        quant,
                        run_idx + 1,
                        timed_runs,
                        elapsed_ms
                    );
                    None
                }
                Ok(Err(e)) => Some(format!("Transcription run {} failed: {}", run_idx + 1, e)),
                Err(panic_payload) => Some(format!(
                    "Engine panicked during run {}: {}",
                    run_idx + 1,
                    panic_payload_message(panic_payload.as_ref())
                )),
            };

            if let Some(message) = failure {
                error!("Benchmark [{}]: {}", quant, message);
                self.emit_benchmark_progress(BenchmarkProgressEvent {
                    event_type: "variant_error".to_string(),
                    quant: Some(quant.to_string()),
                    model_id: Some(variant_id.to_string()),
                    error: Some(message),
                    ..Default::default()
                });
                break;
            }

            self.emit_benchmark_progress(BenchmarkProgressEvent {
                event_type: "run_completed".to_string(),
                quant: Some(quant.to_string()),
                model_id: Some(variant_id.to_string()),
                avg_time_ms: Some(elapsed_ms),
                audio_secs: Some(audio_secs),
                run_index: Some(run_idx as u32 + 1),
                total_runs: Some(timed_runs as u32),
                ..Default::default()
            });
        }

        // Free GPU/CPU memory before the next variant is loaded, then pause
        // briefly so the OS can actually release it — memory pressure carried
        // into the next load would skew its timings.
        drop(engine);
        thread::sleep(Duration::from_millis(200));

        Ok(times_ms)
    }

    /// Turn a set of timed runs into a [`BenchmarkResult`], logging the summary.
    ///
    /// `times_ms` holds only the timed runs — the warmup pass was already
    /// dropped by [`Self::time_variant_runs`], so the average below can never
    /// include it.
    fn summarize_benchmark(
        quant_file: &crate::managers::model::QuantFile,
        variant_id: &str,
        is_default: bool,
        times_ms: &[f64],
        audio_secs: f64,
    ) -> BenchmarkResult {
        let avg_ms: f64 = times_ms.iter().sum::<f64>() / times_ms.len() as f64;

        info!(
            "Benchmark: {} transcribed {:.2}s of audio in {:.0}ms (average of {} timed runs, warmup discarded, {:.2}x real-time)",
            quant_file.quant,
            audio_secs,
            avg_ms,
            times_ms.len(),
            real_time_factor(audio_secs, avg_ms / 1000.0)
        );

        BenchmarkResult {
            quant: quant_file.quant.clone(),
            model_id: variant_id.to_string(),
            filename: quant_file.filename.clone(),
            size_mb: (quant_file.size_bytes.div_ceil(1024 * 1024)) as u32,
            avg_time_ms: avg_ms,
            audio_secs,
            is_default,
        }
    }

    /// Benchmark all downloaded quantization variants of a model family.
    ///
    /// For each downloaded quant (e.g. Q4_K_M, Q5_K_M, Q8_0) of the model
    /// identified by `model_id`, this method:
    ///   1. Loads the quant's engine on a temporary basis (not the primary slot).
    ///   2. Runs one warmup transcription (discarded, never averaged).
    ///   3. Runs `timed_runs` timed transcriptions
    ///      (clamped to `[BENCHMARK_MIN_RUNS, BENCHMARK_MAX_RUNS]`).
    ///   4. Averages the timings of those timed runs only.
    ///   5. Drops the engine to free memory before moving to the next variant.
    ///
    /// The primary model is unloaded first so every variant is measured from
    /// the same clean state; it is reloaded in the background afterwards if it
    /// was loaded when the run started.
    ///
    /// Progress is reported via `benchmark-progress` events. Results are also
    /// returned so the caller can present a complete table at once.
    ///
    /// A run that aborts before it can report per-variant progress still emits
    /// `benchmark_failed`, so listeners that only watch the event stream (rather
    /// than awaiting this call) can clear their in-progress state.
    pub fn benchmark_quantizations(
        &self,
        model_id: &str,
        audio: &[f32],
        timed_runs: Option<usize>,
    ) -> Result<Vec<BenchmarkResult>> {
        self.benchmark_quantizations_inner(model_id, audio, timed_runs)
            .inspect_err(|e| self.emit_benchmark_failed(model_id, e))
    }

    /// Emit the terminal `benchmark_failed` event for an aborted run.
    fn emit_benchmark_failed(&self, model_id: &str, error: &anyhow::Error) {
        error!("Benchmark aborted for '{}': {}", model_id, error);
        self.emit_benchmark_progress(BenchmarkProgressEvent {
            event_type: "benchmark_failed".to_string(),
            model_id: Some(model_id.to_string()),
            error: Some(error.to_string()),
            ..Default::default()
        });
    }

    fn benchmark_quantizations_inner(
        &self,
        model_id: &str,
        audio: &[f32],
        timed_runs: Option<usize>,
    ) -> Result<Vec<BenchmarkResult>> {
        // The warmup pass is fixed at one and never counted; this is how many
        // runs follow it and get averaged.
        let timed_runs = benchmark_timed_runs(timed_runs);

        // Split the model_id into repo_id + filename to look up the catalog
        // descriptor that owns all quantization variants for this model family.
        let (repo_id, filename) = model_id
            .rsplit_once('/')
            .ok_or_else(|| anyhow::anyhow!("Invalid model id: {}", model_id))?;

        let (descriptor, _) = crate::catalog::file_in_catalog(filename, Some(repo_id))
            .ok_or_else(|| anyhow::anyhow!("Model '{}' is not a catalog model", model_id))?;

        let settings = get_settings(&self.app_handle);
        let audio_secs = benchmark_audio_secs(audio);
        let default_filename = crate::managers::model::default_quant_file(
            &descriptor.files,
            descriptor.default_quant.as_deref(),
        )
        .map(|f| f.filename.as_str());

        // Resolve target backend once for the model being benchmarked so every
        // quantization variant is evaluated on the exact same compute backend and device.
        let (target_backend, target_device) = resolve_model_backend(&settings, model_id);
        let benchmark_model_options = ModelOptions {
            backend: target_backend,
            device: target_device,
        };
        info!(
            "Benchmarking quantizations for '{}' using target backend {:?} (device: {:?}), {} timed runs after one discarded warmup",
            model_id, target_backend, benchmark_model_options.device, timed_runs
        );

        // Remember whether the user had a model resident so we can restore it
        // once the run is over instead of leaving them with a cold start.
        let had_primary_model = self.is_model_loaded();

        self.emit_benchmark_progress(BenchmarkProgressEvent {
            event_type: "benchmark_started".to_string(),
            audio_secs: Some(audio_secs),
            total_runs: Some(timed_runs as u32),
            ..Default::default()
        });

        let mut results: Vec<BenchmarkResult> = Vec::new();

        for file in &descriptor.files {
            let variant_id = format!("{}/{}", repo_id, file.filename);

            // Skip variants that aren't downloaded (not registered or not on disk)
            match self.model_manager.get_model_info(&variant_id) {
                Some(info) if info.is_downloaded => {}
                _ => continue,
            }

            self.emit_benchmark_progress(BenchmarkProgressEvent {
                event_type: "variant_started".to_string(),
                quant: Some(file.quant.clone()),
                model_id: Some(variant_id.clone()),
                audio_secs: Some(audio_secs),
                total_runs: Some(timed_runs as u32),
                ..Default::default()
            });

            let model_path = match self.model_manager.get_model_path(&variant_id) {
                Ok(p) => p,
                Err(e) => {
                    self.emit_benchmark_progress(BenchmarkProgressEvent {
                        event_type: "variant_error".to_string(),
                        quant: Some(file.quant.clone()),
                        model_id: Some(variant_id.clone()),
                        error: Some(e.to_string()),
                        ..Default::default()
                    });
                    continue;
                }
            };

            // Ensure the primary engine (if any) is unloaded so GPU/CPU memory
            // is freed before loading the benchmark variant. This isolates each
            // benchmark run and prevents the primary model from interfering with
            // timing measurements.
            let _ = self.unload_model();

            let times_ms = match self.time_variant_runs(
                &variant_id,
                &file.quant,
                &model_path,
                &settings,
                &benchmark_model_options,
                audio,
                timed_runs,
            ) {
                Ok(times) => times,
                Err(e) => {
                    self.emit_benchmark_progress(BenchmarkProgressEvent {
                        event_type: "variant_error".to_string(),
                        quant: Some(file.quant.clone()),
                        model_id: Some(variant_id.clone()),
                        error: Some(format!("Failed to load model: {}", e)),
                        ..Default::default()
                    });
                    continue;
                }
            };

            if times_ms.is_empty() {
                continue;
            }

            let result = Self::summarize_benchmark(
                file,
                &variant_id,
                Some(file.filename.as_str()) == default_filename,
                &times_ms,
                audio_secs,
            );

            self.emit_benchmark_progress(BenchmarkProgressEvent {
                event_type: "variant_completed".to_string(),
                quant: Some(result.quant.clone()),
                model_id: Some(result.model_id.clone()),
                avg_time_ms: Some(result.avg_time_ms),
                audio_secs: Some(audio_secs),
                ..Default::default()
            });

            results.push(result);
        }

        if had_primary_model {
            self.initiate_model_load();
        }

        self.emit_benchmark_progress(BenchmarkProgressEvent {
            event_type: "benchmark_completed".to_string(),
            ..Default::default()
        });

        Ok(results)
    }

    /// Benchmark a single quantization variant.
    ///
    /// Loads the model from a clean state (unloading any primary engine first),
    /// runs one warmup transcription (discarded, never averaged), then performs
    /// `timed_runs` timed transcription runs (clamped to
    /// `[BENCHMARK_MIN_RUNS, BENCHMARK_MAX_RUNS]`) and returns the average of
    /// those timed runs only. The primary model is reloaded in the background
    /// afterwards if it was loaded when the run started.
    ///
    /// Like [`benchmark_quantizations`](Self::benchmark_quantizations), an abort
    /// emits `benchmark_failed` so event-only listeners can recover.
    pub fn benchmark_single_quantization(
        &self,
        model_id: &str,
        audio: &[f32],
        timed_runs: Option<usize>,
    ) -> Result<BenchmarkResult> {
        self.benchmark_single_quantization_inner(model_id, audio, timed_runs)
            .inspect_err(|e| self.emit_benchmark_failed(model_id, e))
    }

    fn benchmark_single_quantization_inner(
        &self,
        model_id: &str,
        audio: &[f32],
        timed_runs: Option<usize>,
    ) -> Result<BenchmarkResult> {
        let timed_runs = benchmark_timed_runs(timed_runs);

        // Split model_id into repo_id + filename for catalog lookup.
        let (repo_id, filename) = model_id
            .rsplit_once('/')
            .ok_or_else(|| anyhow::anyhow!("Invalid model id: {}", model_id))?;

        let (descriptor, file) = crate::catalog::file_in_catalog(filename, Some(repo_id))
            .ok_or_else(|| anyhow::anyhow!("Model '{}' is not a catalog model", model_id))?;

        // The model must be registered before its path can be resolved.
        self.model_manager
            .get_model_info(model_id)
            .ok_or_else(|| anyhow::anyhow!("Model '{}' is not registered", model_id))?;

        let model_path = self.model_manager.get_model_path(model_id)?;

        let settings = get_settings(&self.app_handle);
        let audio_secs = benchmark_audio_secs(audio);

        let had_primary_model = self.is_model_loaded();

        // Unload any currently loaded primary model to free GPU/CPU memory, so
        // this variant is measured from the same clean state a full run uses.
        let _ = self.unload_model();

        self.emit_benchmark_progress(BenchmarkProgressEvent {
            event_type: "variant_started".to_string(),
            quant: Some(file.quant.clone()),
            model_id: Some(model_id.to_string()),
            audio_secs: Some(audio_secs),
            total_runs: Some(timed_runs as u32),
            ..Default::default()
        });

        let (target_backend, target_device) = resolve_model_backend(&settings, model_id);
        let benchmark_model_options = ModelOptions {
            backend: target_backend,
            device: target_device,
        };
        info!(
            "Benchmarking single quantization '{}' using target backend {:?} (device: {:?}), {} timed runs after one discarded warmup",
            model_id, target_backend, benchmark_model_options.device, timed_runs
        );

        let times_ms = self.time_variant_runs(
            model_id,
            &file.quant,
            &model_path,
            &settings,
            &benchmark_model_options,
            audio,
            timed_runs,
        );

        if had_primary_model {
            self.initiate_model_load();
        }

        let times_ms = times_ms?;

        if times_ms.is_empty() {
            return Err(anyhow::anyhow!(
                "All transcription runs failed for quant '{}'",
                file.quant
            ));
        }

        let result = Self::summarize_benchmark(
            file,
            model_id,
            crate::managers::model::default_quant_file(
                &descriptor.files,
                descriptor.default_quant.as_deref(),
            )
            .is_some_and(|default| default.filename == file.filename),
            &times_ms,
            audio_secs,
        );

        self.emit_benchmark_progress(BenchmarkProgressEvent {
            event_type: "variant_completed".to_string(),
            quant: Some(result.quant.clone()),
            model_id: Some(result.model_id.clone()),
            avg_time_ms: Some(result.avg_time_ms),
            audio_secs: Some(audio_secs),
            ..Default::default()
        });

        Ok(result)
    }

    /// The latency a streaming run will be measured at, in ms of audio, or
    /// `None` when the model carries no latency control at all.
    ///
    /// Read from the same settings the live path resolves through, so a value
    /// just written by the status-bar latency slider is what a run started
    /// afterwards measures.
    fn latency_setting_point(
        settings: &AppSettings,
        model_id: &str,
        kind: Option<crate::managers::model::NativeStreamingLatencyKind>,
    ) -> Option<crate::managers::native_streaming_latency::LatencyPoint> {
        kind.map(|kind| {
            crate::managers::native_streaming_latency::latency_point(
                kind,
                get_effective_latency_preset(settings, model_id),
                get_effective_r2t2_chunk_ms(settings, model_id),
            )
        })
    }

    /// Replay `audio` through a native stream on `engine` and time the pass.
    ///
    /// The extension is resolved by [`Self::resolved_stream_extension`] — the
    /// same dispatcher the live path uses — so this measures the cadence and
    /// right context the app would really run for this model, exactly like the
    /// headless `--stream-chunk-ms` replay does.
    fn replay_stream_once(
        &self,
        engine: &mut LoadedEngine,
        settings: &AppSettings,
        model_id: &str,
        audio: &[f32],
    ) -> Result<StreamReplayMetrics> {
        let language =
            effective_language_for_model(settings, self.model_manager.as_ref(), model_id);
        let LoadedEngine::TranscribeCpp(session) = engine;
        let model = session.model();
        let caps = model.capabilities();
        let plan = transcribe_cpp_run_plan(
            settings.translate_to_english,
            &language,
            &caps.languages,
            caps.supports_translate,
        );
        let options = RunOptions {
            task: plan.task,
            language: plan.language,
            target_language: plan.target_language,
            ..Default::default()
        };
        let family = self.resolved_stream_extension(settings, &model, model_id);
        let stream_extension = format!("{:?}", family);
        let stream_options = StreamOptions {
            family,
            ..Default::default()
        };

        let wall = Instant::now();
        let begin_start = Instant::now();
        let mut stream = session.stream(&options, &stream_options)?;
        let begin_ms = begin_start.elapsed().as_secs_f64() * 1000.0;

        let mut feed_ms = Vec::with_capacity(audio.len() / (STREAM_BENCHMARK_FEED_MS * 16) + 1);
        let mut first_text_audio_ms = None;
        let mut samples_fed = 0usize;
        for chunk in audio.chunks(STREAM_BENCHMARK_FEED_MS * 16) {
            let tick = Instant::now();
            let update = stream.feed(chunk)?;
            feed_ms.push(tick.elapsed().as_secs_f64() * 1000.0);
            samples_fed += chunk.len();
            if first_text_audio_ms.is_none()
                && (update.committed_changed || update.tentative_changed)
            {
                // The audio horizon, not the clock: how much audio had been
                // handed to the decoder when text first appeared.
                first_text_audio_ms = Some(samples_fed as f64 / 16.0);
            }
        }
        let tick = Instant::now();
        stream.finalize()?;
        let finalize_ms = tick.elapsed().as_secs_f64() * 1000.0;

        let compute_ms = begin_ms + feed_ms.iter().sum::<f64>() + finalize_ms;
        Ok(StreamReplayMetrics {
            compute_ms,
            wall_ms: wall.elapsed().as_secs_f64() * 1000.0,
            feed_ms,
            finalize_ms,
            first_text_audio_ms,
            stream_extension,
        })
    }

    /// Average a set of timed streaming passes into a
    /// [`StreamingBenchmarkResult`]. `runs` holds only the timed passes — the
    /// warmup was already dropped by the caller.
    fn summarize_stream_benchmark(
        model_id: &str,
        runs: &[StreamReplayMetrics],
        audio_secs: f64,
        latency: Option<crate::managers::native_streaming_latency::LatencyPoint>,
    ) -> StreamingBenchmarkResult {
        let n = runs.len().max(1) as f64;
        let avg_compute_ms = runs.iter().map(|r| r.compute_ms).sum::<f64>() / n;
        let mut feeds: Vec<f64> = runs
            .iter()
            .flat_map(|r| r.feed_ms.iter().copied())
            .collect();
        feeds.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let busy: Vec<f64> = feeds.iter().copied().filter(|ms| *ms >= 1.0).collect();
        let first_texts: Vec<f64> = runs.iter().filter_map(|r| r.first_text_audio_ms).collect();

        StreamingBenchmarkResult {
            model_id: model_id.to_string(),
            runs: runs.len() as u32,
            audio_secs,
            avg_compute_ms,
            compute_xrt: real_time_factor(audio_secs, avg_compute_ms / 1000.0),
            avg_wall_ms: runs.iter().map(|r| r.wall_ms).sum::<f64>() / n,
            feed_p95_ms: percentile_ms(&feeds, 0.95),
            busy_feed_p95_ms: percentile_ms(&busy, 0.95),
            feed_max_ms: feeds.last().copied().unwrap_or(0.0),
            avg_finalize_ms: runs.iter().map(|r| r.finalize_ms).sum::<f64>() / n,
            avg_first_text_audio_ms: if first_texts.is_empty() {
                None
            } else {
                Some(first_texts.iter().sum::<f64>() / first_texts.len() as f64)
            },
            feed_chunk_ms: STREAM_BENCHMARK_FEED_MS as u32,
            stream_extension: runs[0].stream_extension.clone(),
            latency_ms: latency.map(|point| point.latency_ms),
            lookahead_ms: latency.map(|point| point.lookahead_ms),
        }
    }

    /// Benchmark `model_id`'s native streaming mode: replay the reference
    /// audio through `stream_begin` / `feed` / `finalize` and report how fast
    /// it runs relative to real time.
    ///
    /// Same contract as the quantization benchmark — one warmup replay that is
    /// discarded and never averaged, then `timed_runs` timed replays clamped
    /// to `[BENCHMARK_MIN_RUNS, BENCHMARK_MAX_RUNS]` — and the same engine
    /// lifecycle: the primary model is unloaded first so the model is measured
    /// from a clean state, and it is reloaded in the background afterwards if
    /// it was resident when the run started.
    ///
    /// Progress is reported on the `benchmark-progress` channel as `stream_*`
    /// events, which the quantization listener ignores by construction.
    pub fn benchmark_streaming(
        &self,
        model_id: &str,
        audio: &[f32],
        timed_runs: Option<usize>,
    ) -> Result<StreamingBenchmarkResult> {
        self.benchmark_streaming_inner(model_id, audio, timed_runs)
            .inspect_err(|error| {
                error!("Streaming benchmark aborted for '{}': {}", model_id, error);
                self.emit_benchmark_progress(BenchmarkProgressEvent {
                    event_type: "stream_failed".to_string(),
                    model_id: Some(model_id.to_string()),
                    error: Some(error.to_string()),
                    ..Default::default()
                });
            })
    }

    fn benchmark_streaming_inner(
        &self,
        model_id: &str,
        audio: &[f32],
        timed_runs: Option<usize>,
    ) -> Result<StreamingBenchmarkResult> {
        // One warmup pass runs first and is fixed outside this count.
        let timed_runs = benchmark_timed_runs(timed_runs);
        anyhow::ensure!(!audio.is_empty(), "Recording has no audio samples");

        let info = self
            .model_manager
            .get_model_info(model_id)
            .ok_or_else(|| anyhow::anyhow!("Model '{}' is not registered", model_id))?;
        anyhow::ensure!(
            info.supports_streaming,
            "Model '{}' does not support streaming",
            model_id
        );
        let model_path = self.model_manager.get_model_path(model_id)?;

        // Settings are read once, here, at the start of this run: the latency
        // panel in the status bar writes them to the store, so whatever the
        // user picked a moment ago is what every pass below is measured at.
        // Read once rather than per pass so one result can never mix two
        // operating points.
        let settings = get_settings(&self.app_handle);
        let latency =
            Self::latency_setting_point(&settings, model_id, info.native_streaming_latency_kind);
        let latency_label = describe_latency(latency);
        let audio_secs = benchmark_audio_secs(audio);
        let had_primary_model = self.is_model_loaded();
        let _ = self.unload_model();

        self.emit_benchmark_progress(BenchmarkProgressEvent {
            event_type: "stream_started".to_string(),
            model_id: Some(model_id.to_string()),
            audio_secs: Some(audio_secs),
            total_runs: Some(timed_runs as u32),
            ..Default::default()
        });

        let (target_backend, target_device) = resolve_model_backend(&settings, model_id);
        let model_options = ModelOptions {
            backend: target_backend,
            device: target_device,
        };
        info!(
            "Streaming benchmark for '{}' using target backend {:?} (device: {:?}) at latency '{}', {} timed runs of {} ms feeds after one discarded warmup",
            model_id,
            target_backend,
            model_options.device,
            latency_label,
            timed_runs,
            STREAM_BENCHMARK_FEED_MS
        );

        let mut engine =
            match self.create_engine_with_options(model_id, &model_path, &model_options) {
                Ok(engine) => engine,
                Err(e) => {
                    if had_primary_model {
                        self.initiate_model_load();
                    }
                    return Err(anyhow::anyhow!("Failed to load model: {}", e));
                }
            };

        // Warmup: the first stream after a load pays for cold caches and lazy
        // initialization. Its timing is dropped on the floor — never recorded,
        // never averaged — and a failure here does not abort the timed runs.
        let warmup = catch_unwind(AssertUnwindSafe(|| {
            self.replay_stream_once(&mut engine, &settings, model_id, audio)
        }));
        match &warmup {
            Ok(Ok(_)) => debug!(
                "Streaming benchmark warmup finished (discarded, not averaged, {} ms feeds)",
                STREAM_BENCHMARK_FEED_MS
            ),
            Ok(Err(e)) => warn!(
                "Streaming benchmark warmup failed, continuing with the timed runs: {}",
                e
            ),
            Err(panic_payload) => warn!(
                "Streaming benchmark warmup panicked, continuing with the timed runs: {}",
                panic_payload_message(panic_payload.as_ref())
            ),
        }
        self.emit_benchmark_progress(BenchmarkProgressEvent {
            event_type: "stream_warmup_completed".to_string(),
            model_id: Some(model_id.to_string()),
            audio_secs: Some(audio_secs),
            run_index: Some(0),
            total_runs: Some(timed_runs as u32),
            ..Default::default()
        });

        let mut runs: Vec<StreamReplayMetrics> = Vec::with_capacity(timed_runs);
        let mut failure: Option<String> = None;
        for run_idx in 0..timed_runs {
            let result = catch_unwind(AssertUnwindSafe(|| {
                self.replay_stream_once(&mut engine, &settings, model_id, audio)
            }));
            let metrics = match result {
                Ok(Ok(metrics)) => metrics,
                Ok(Err(e)) => {
                    failure = Some(format!("Streaming run {} failed: {}", run_idx + 1, e));
                    break;
                }
                Err(panic_payload) => {
                    failure = Some(format!(
                        "Engine panicked during streaming run {}: {}",
                        run_idx + 1,
                        panic_payload_message(panic_payload.as_ref())
                    ));
                    break;
                }
            };
            debug!(
                "Streaming benchmark run {}/{}: {:.0}ms compute ({:.2}x real-time)",
                run_idx + 1,
                timed_runs,
                metrics.compute_ms,
                real_time_factor(audio_secs, metrics.compute_ms / 1000.0)
            );
            self.emit_benchmark_progress(BenchmarkProgressEvent {
                event_type: "stream_run_completed".to_string(),
                model_id: Some(model_id.to_string()),
                avg_time_ms: Some(metrics.compute_ms),
                audio_secs: Some(audio_secs),
                run_index: Some(run_idx as u32 + 1),
                total_runs: Some(timed_runs as u32),
                ..Default::default()
            });
            runs.push(metrics);
        }

        // Free GPU/CPU memory and let the OS release it, then restore whatever
        // the user had resident — on every path out, including the ones below.
        drop(engine);
        thread::sleep(Duration::from_millis(200));
        if had_primary_model {
            self.initiate_model_load();
        }

        if runs.is_empty() {
            let message =
                failure.unwrap_or_else(|| "No streaming run produced a measurement".to_string());
            error!("Streaming benchmark: {}", message);
            return Err(anyhow::anyhow!(message));
        }
        if let Some(message) = failure {
            warn!("Streaming benchmark stopped early: {}", message);
        }

        let result = Self::summarize_stream_benchmark(model_id, &runs, audio_secs, latency);
        info!(
            "Streaming benchmark: {} replayed {:.2}s of audio at {:.2}x real-time (average of {} timed runs, warmup discarded, latency '{}', p95 feed {:.1}ms, first text {})",
            model_id,
            audio_secs,
            result.compute_xrt,
            result.runs,
            latency_label,
            result.feed_p95_ms,
            result
                .avg_first_text_audio_ms
                .map(|ms| format!("{:.0}ms of audio", ms))
                .unwrap_or_else(|| "only at finalize".to_string())
        );

        self.emit_benchmark_progress(BenchmarkProgressEvent {
            event_type: "stream_completed".to_string(),
            model_id: Some(model_id.to_string()),
            avg_time_ms: Some(result.avg_compute_ms),
            audio_secs: Some(audio_secs),
            total_runs: Some(result.runs),
            ..Default::default()
        });

        Ok(result)
    }

    /// Get the list of extra loaded model IDs.
    pub fn get_extra_loaded_models(&self) -> Vec<String> {
        let extra = self.extra_engines.lock().unwrap();
        extra.keys().cloned().collect()
    }

    /// Check if a specific extra model is loaded.
    pub fn is_extra_model_loaded(&self, model_id: &str) -> bool {
        let extra = self.extra_engines.lock().unwrap();
        extra.contains_key(model_id)
    }

    /// Load a transcribe-cpp model file — the one `Model::load_with` wrapper,
    /// shared by the primary load and [`Self::create_engine`].
    ///
    /// When the catalog names an architecture for `filename`, its external
    /// plugin is activated first (`ensure_arch_plugin_for_model`, a no-op in
    /// every shipped posture, where arch-dl is off and the family is built in). A load failure that looks like a missing plugin is
    /// reported as such; anything else as a plain load failure.
    fn load_transcribe_model(
        &self,
        model_id: &str,
        filename: Option<&str>,
        model_path: &std::path::Path,
        model_options: &ModelOptions,
    ) -> Result<Model> {
        let arch_hint = filename
            .and_then(|filename| crate::catalog::file_in_catalog(filename, None))
            .and_then(|(descriptor, _)| descriptor.caps.architecture.clone())
            .unwrap_or_default();
        if !arch_hint.is_empty()
            && let Err(e) = crate::managers::arch_plugins::ensure_arch_plugin_for_model(
                &arch_hint,
                &self.app_handle,
            )
        {
            warn!(
                "Architecture plugin for '{}' could not be loaded: {}",
                arch_hint, e
            );
        }

        Model::load_with(model_path, model_options).map_err(|e| {
            let err_str = e.to_string();
            if err_str.contains("unsupported architecture") {
                anyhow::anyhow!(
                    "Failed to load model {}: architecture requires a dynamic plugin (transcribe-arch-*.dll) in your plugins directory. Error: {}",
                    model_id,
                    e
                )
            } else {
                anyhow::anyhow!("Failed to load model {}: {}", model_id, e)
            }
        })
    }

    /// Create a LoadedEngine for a model file with explicit ModelOptions.
    fn create_engine_with_options(
        &self,
        model_id: &str,
        model_path: &std::path::Path,
        model_options: &ModelOptions,
    ) -> Result<LoadedEngine> {
        let filename = self
            .model_manager
            .get_model_info(model_id)
            .map(|info| info.filename);
        let model =
            self.load_transcribe_model(model_id, filename.as_deref(), model_path, model_options)?;
        let session = model
            .session()
            .map_err(|e| anyhow::anyhow!("Failed to create session for {}: {}", model_id, e))?;
        Ok(LoadedEngine::TranscribeCpp(session))
    }

    /// Create a LoadedEngine for a Multi-STT extra model file.
    fn create_engine(&self, model_id: &str, model_path: &std::path::Path) -> Result<LoadedEngine> {
        // Same resolution as the primary load, so a per-model backend override
        // applies to a Multi-STT extra exactly as it does to the primary model.
        let (backend, device) = resolve_model_backend(&get_settings(&self.app_handle), model_id);
        let model_options = ModelOptions { backend, device };
        self.create_engine_with_options(model_id, model_path, &model_options)
    }
}

/// Result of transcribing audio with a single engine, carrying enough context
/// for the shared post-processing pipeline to behave identically to the primary
/// batch path (language evidence, supported languages, custom-word prompt flag).
struct EngineTranscribeOutput {
    text: String,
    output_language: OutputLanguageEvidence,
    supported_languages: Vec<String>,
    /// True when custom words were handed to the model as a decode prompt
    /// (whisper family), so the fuzzy correction pass must be skipped.
    custom_words_already_prompted: bool,
}

/// Transcribe audio with a given engine (shared between primary and extra models).
fn transcribe_with_engine(
    engine: &mut LoadedEngine,
    settings: &AppSettings,
    audio: &[f32],
    model_id: &str,
    model_manager: &ModelManager,
) -> Result<EngineTranscribeOutput> {
    // Coerce the requested language to what this particular model supports
    // (unsupported → auto/en). Without this a Multi-STT extra model inherits
    // the primary model's language verbatim, and e.g. Canary rejects "zh-Hant"
    // → that slot yields nothing.
    let effective_language = effective_language_for_model(settings, model_manager, model_id);

    let LoadedEngine::TranscribeCpp(session) = engine;
    let model = session.model();
    let caps = model.capabilities();
    let model_supports_translate = caps.supports_translate;
    let supported_languages = caps.languages.clone();
    let model_is_whisper = model.arch() == "whisper";

    // Custom words become the initial prompt ONLY for models that accept one
    // (whisper family); other archs get the fuzzy post-correction instead.
    let family = if settings.custom_words.is_empty() || !model_is_whisper {
        None
    } else {
        Some(RunExtension::Whisper(WhisperRunOptions {
            initial_prompt: Some(settings.custom_words.join(", ")),
            ..Default::default()
        }))
    };
    let custom_words_already_prompted = family.is_some();

    let run_plan = transcribe_cpp_run_plan(
        settings.translate_to_english,
        &effective_language,
        &supported_languages,
        model_supports_translate,
    );
    let output_was_translated = run_plan.target_language.as_deref() == Some("en");
    let applied_language_hint = run_plan.language.clone();

    let run_options = RunOptions {
        task: run_plan.task,
        language: run_plan.language,
        target_language: run_plan.target_language,
        family,
        ..Default::default()
    };

    let mut model_detected_language: Option<String> = None;
    let text = session
        .run(audio, &run_options)
        .map(|t| {
            model_detected_language = t.language;
            t.text
        })
        .map_err(|e| anyhow::anyhow!("transcribe-cpp transcription failed: {}", e))?;

    let output_language = with_model_detected_language(
        resolve_output_language_evidence(
            settings,
            applied_language_hint.as_deref(),
            &supported_languages,
            output_was_translated,
        ),
        model_detected_language,
    );

    Ok(EngineTranscribeOutput {
        text,
        output_language,
        supported_languages,
        custom_words_already_prompted,
    })
}

/// The native library's build id (a process-lifetime static string), or
/// "unknown".
pub fn native_build_identity() -> String {
    // Native API returns a process-lifetime static string.
    let ptr = unsafe { transcribe_cpp::sys::transcribe_build_id() };
    if ptr.is_null() {
        return "unknown".to_string();
    }
    unsafe { std::ffi::CStr::from_ptr(ptr) }
        .to_string_lossy()
        .trim()
        .to_string()
}

/// Initialize the transcribe-cpp native backend once at startup: route native +
/// ggml diagnostics into the `log` facade and register compute backend modules.
/// In a static build (macOS Metal) `init_backends_default` is a harmless no-op;
/// in a `dynamic-backends` build it loads the per-ISA CPU / GPU modules. Must run
/// before the first model load.
pub fn init_transcribe_backend() {
    transcribe_cpp::init_logging();
    info!(target: "pipeline", "native={} executable={:?}",
        native_build_identity(), std::env::current_exe());
    match transcribe_cpp::init_backends_default() {
        Ok(()) => {
            if transcribe_gpu_disabled_for_host() {
                warn!(
                    "Windows x64 build is running under emulation on an ARM64 host; \
                     disabling transcribe.cpp GPU acceleration and using CPU"
                );
            }
            let devices = transcribe_compute_devices();
            info!(
                "transcribe-cpp initialized with {} compute device(s): [{}]",
                devices.len(),
                devices
                    .iter()
                    .map(|d| format!("{} ({})", d.name, d.kind))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        Err(e) => warn!("Failed to initialize transcribe-cpp backends: {}", e),
    }
}

/// Human-readable list of the transcribe-cpp compute devices registered at
/// startup, for the `--list-devices` flag. The reported `index` is the
/// value to pass to `--device-index`. Backends must be initialized first
/// (see [`init_transcribe_backend`]).
pub fn describe_compute_devices() -> Vec<String> {
    transcribe_compute_devices()
        .into_iter()
        .map(|d| {
            let idx = d
                .index
                .map(|i| i.to_string())
                .unwrap_or_else(|| "-".to_string());
            let name = if d.description.is_empty() {
                d.name
            } else {
                d.description
            };
            let vram_mb = d.memory_total / (1024 * 1024);
            format!(
                "index={} kind={} name={} vram={}MB",
                idx, d.kind, name, vram_mb
            )
        })
        .collect()
}

/// Resolve a `--list-devices` registry index to an exact opaque device handle
/// for a transcribe-cpp model load (the `--device-index` flag). In 0.2 index 0
/// is an exact selection too; only an omitted index requests automatic device
/// selection. Errors if the index isn't a registered, loadable primary device.
fn resolve_device_index(index: usize) -> Result<(Backend, Option<Device>)> {
    let device = transcribe_compute_devices()
        .into_iter()
        .find(|d| d.index == Some(index))
        .ok_or_else(|| {
            anyhow::anyhow!("No compute device with index {index} (see --list-devices)")
        })?;
    if matches!(
        device.device_type,
        transcribe_cpp::DeviceType::Accel | transcribe_cpp::DeviceType::Unknown
    ) {
        return Err(anyhow::anyhow!(
            "Device index {index} ({}) cannot host a model",
            device.kind
        ));
    }

    // 0.2's opaque handle makes every index, including zero, an exact
    // selection. Backend::Auto accepts any primary device and cannot conflict
    // with the selected device's vendor backend.
    Ok((Backend::Auto, Some(device)))
}

/// Map the transcribe accelerator setting to a transcribe-cpp [`Backend`].
///
/// `Auto` lets the library pick the best device (with CPU fallback). `Cpu` forces
/// strict CPU. `Gpu` requests the platform GPU backend, but only if a device for
/// it is actually registered — otherwise it falls back to `Auto` so the load
/// never fails outright on a machine without that GPU backend. An emulated x64
/// process on Windows ARM64 forces strict CPU for every setting.
fn select_transcribe_backend(setting: TranscribeAcceleratorSetting) -> Backend {
    match effective_transcribe_accelerator(setting, transcribe_gpu_disabled_for_host()) {
        TranscribeAcceleratorSetting::Cpu => Backend::Cpu,
        TranscribeAcceleratorSetting::Auto => Backend::Auto,
        TranscribeAcceleratorSetting::Gpu => {
            #[cfg(target_os = "macos")]
            let candidates = [Backend::Metal];
            #[cfg(not(target_os = "macos"))]
            let candidates = [Backend::Cuda, Backend::Rocm, Backend::Vulkan];

            match candidates
                .into_iter()
                .find(|&b| transcribe_cpp::backend_available(b))
            {
                Some(b) => b,
                None => {
                    #[cfg(target_os = "linux")]
                    warn!(
                        "GPU acceleration was requested, but no transcribe.cpp GPU backend is \
                         registered; falling back to Auto (usually CPU). Run with \
                         --list-devices to inspect detected devices; VK_LOADER_DEBUG=error can \
                         reveal Vulkan loader or driver failures"
                    );
                    #[cfg(not(target_os = "linux"))]
                    warn!(
                        "GPU acceleration was requested, but no transcribe.cpp GPU backend is \
                         registered; falling back to Auto (usually CPU). Run with \
                         --list-devices to inspect detected devices"
                    );
                    Backend::Auto
                }
            }
        }
    }
}

/// Resolve the user's persisted GPU identity to a fresh opaque 0.2 device
/// handle. Registry indices and handles are process-local, so settings store a
/// key based on the backend's stable `device_id` (falling back to name for
/// backends such as Metal that do not report one).
fn resolve_gpu_device(
    setting: TranscribeAcceleratorSetting,
    gpu_device: Option<&str>,
) -> Option<Device> {
    if transcribe_gpu_disabled_for_host() || setting != TranscribeAcceleratorSetting::Gpu {
        return None;
    }
    let gpu_device = gpu_device?;
    let resolved = transcribe_compute_devices().into_iter().find(|device| {
        is_transcribe_gpu_device(device) && transcribe_device_key(device) == gpu_device
    });
    if resolved.is_none() {
        warn!(
            "Stored transcribe GPU device '{}' is no longer available; \
             using automatic GPU selection",
            gpu_device
        );
    }
    resolved
}

/// Map a per-model backend preference onto an engine [`Backend`].
///
/// `Auto` never reaches here — it means "no override", handled by the caller.
/// The names line up one-to-one with the engine's variants, which is why the
/// preference enum can be spelled in user-facing terms (`cuda`, `vulkan`,
/// `metal`, `rocm`) instead of the coarser `auto | cpu | gpu` of the global
/// setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VulkanTargetDevice {
    #[allow(dead_code)]
    Any,
    Nvidia,
    Intel,
}

/// Find a specific Vulkan compute device using a prioritized matching strategy:
/// 1. Primary match: vendor name in device description or name (case-insensitive).
/// 2. Secondary match: device_type classification (Igpu for Intel, Gpu for discrete NVIDIA).
fn find_vulkan_device(target: VulkanTargetDevice) -> Option<transcribe_cpp::Device> {
    let devices = transcribe_compute_devices();
    let vulkan_devices: Vec<_> = devices
        .into_iter()
        .filter(|d| d.kind.eq_ignore_ascii_case("vulkan"))
        .collect();

    match target {
        VulkanTargetDevice::Any => vulkan_devices.into_iter().next(),
        VulkanTargetDevice::Nvidia => vulkan_devices
            .iter()
            .find(|d| {
                let desc = d.description.to_ascii_lowercase();
                let name = d.name.to_ascii_lowercase();
                desc.contains("nvidia")
                    || name.contains("nvidia")
                    || desc.contains("geforce")
                    || desc.contains("quadro")
            })
            .cloned()
            .or_else(|| {
                // Secondary fallback: discrete GPU if no description matched
                vulkan_devices
                    .iter()
                    .find(|d| d.device_type == transcribe_cpp::DeviceType::Gpu)
                    .cloned()
            }),
        VulkanTargetDevice::Intel => vulkan_devices
            .iter()
            .find(|d| {
                let desc = d.description.to_ascii_lowercase();
                let name = d.name.to_ascii_lowercase();
                desc.contains("intel")
                    || name.contains("intel")
                    || desc.contains("iris")
                    || desc.contains("arc")
            })
            .cloned()
            .or_else(|| {
                // Secondary fallback: any registered integrated GPU
                vulkan_devices
                    .iter()
                    .find(|d| d.device_type == transcribe_cpp::DeviceType::Igpu)
                    .cloned()
            }),
    }
}

/// Resolve the `(backend, device)` a **specific** model should load with.
///
/// This is the single place the two load sites (the primary model and
/// `create_engine`, which every Multi-STT extra goes through) get their options
/// from, so a per-model override cannot apply on one path and not the other.
///
/// The override, when there is one, is a hard request: an explicit backend has
/// no silent fallback inside the engine, and a model that asks for a backend
/// this build or this machine does not have would fail to load outright. So an
/// unavailable request is refused here, with a warning naming the model, and the
/// global policy is applied instead — a preference is not worth a dead model.
/// Resolve the effective per-model backend setting for a model id.
///
/// Looks up `model_id` in `per_model_backends`. If not found directly and `model_id`
/// is a quant file (e.g. `repo/filename.gguf`), checks the base repo ID and sibling
/// quants. If `model_id` is a base repo without filename, checks if any variant of
/// that repo is configured.
pub fn get_effective_model_backend(settings: &AppSettings, model_id: &str) -> ModelBackendSetting {
    if let Some(&backend) = settings.per_model_backends.get(model_id) {
        return backend;
    }
    if model_id.ends_with(".gguf") {
        if let Some((base_repo, _)) = model_id.rsplit_once('/') {
            if let Some(&backend) = settings.per_model_backends.get(base_repo) {
                return backend;
            }
            for (k, &backend) in &settings.per_model_backends {
                if let Some((k_repo, _)) = k.rsplit_once('/') {
                    if k_repo == base_repo {
                        return backend;
                    }
                }
            }
        }
    } else {
        for (k, &backend) in &settings.per_model_backends {
            if k.starts_with(&format!("{}/", model_id)) {
                return backend;
            }
        }
    }
    ModelBackendSetting::Auto
}

/// Resolve the effective latency preset for a model id.
pub fn get_effective_latency_preset(
    settings: &AppSettings,
    model_id: &str,
) -> crate::settings::NativeStreamingLatencyPreset {
    if let Some(&preset) = settings.native_streaming_latency_presets.get(model_id) {
        return preset;
    }
    if model_id.ends_with(".gguf") {
        if let Some((base_repo, _)) = model_id.rsplit_once('/') {
            if let Some(&preset) = settings.native_streaming_latency_presets.get(base_repo) {
                return preset;
            }
            for (k, &preset) in &settings.native_streaming_latency_presets {
                if let Some((k_repo, _)) = k.rsplit_once('/') {
                    if k_repo == base_repo {
                        return preset;
                    }
                }
            }
        }
    } else {
        for (k, &preset) in &settings.native_streaming_latency_presets {
            if k.starts_with(&format!("{}/", model_id)) {
                return preset;
            }
        }
    }
    crate::settings::NativeStreamingLatencyPreset::Accurate
}

/// Resolve the effective R2T2 streaming chunk size for a model id.
pub fn get_effective_r2t2_chunk_ms(settings: &AppSettings, model_id: &str) -> u32 {
    if let Some(&chunk) = settings.native_streaming_chunk_ms.get(model_id) {
        return chunk;
    }
    if model_id.ends_with(".gguf") {
        if let Some((base_repo, _)) = model_id.rsplit_once('/') {
            if let Some(&chunk) = settings.native_streaming_chunk_ms.get(base_repo) {
                return chunk;
            }
            for (k, &chunk) in &settings.native_streaming_chunk_ms {
                if let Some((k_repo, _)) = k.rsplit_once('/') {
                    if k_repo == base_repo {
                        return chunk;
                    }
                }
            }
        }
    } else {
        for (k, &chunk) in &settings.native_streaming_chunk_ms {
            if k.starts_with(&format!("{}/", model_id)) {
                return chunk;
            }
        }
    }
    crate::managers::native_streaming_latency::R2T2_CHUNK_MS_DEFAULT
}

fn resolve_model_backend(settings: &AppSettings, model_id: &str) -> (Backend, Option<Device>) {
    let accelerator = settings.transcribe_accelerator;
    let requested = get_effective_model_backend(settings, model_id);
    // An emulated x64 process on Windows ARM64 has no GPU backends at all, so
    // every GPU request — global or per-model — collapses to CPU there.
    let requested = if transcribe_gpu_disabled_for_host() {
        ModelBackendSetting::Cpu
    } else {
        requested
    };

    let global = || {
        let device = resolve_gpu_device(accelerator, settings.transcribe_gpu_device.as_deref());
        let backend = if device.is_some() {
            Backend::Auto
        } else {
            select_transcribe_backend(accelerator)
        };
        (backend, device)
    };

    match requested {
        ModelBackendSetting::Auto => global(),
        ModelBackendSetting::Cpu => (Backend::Cpu, None),
        ModelBackendSetting::Cuda => {
            if transcribe_cpp::backend_available(Backend::Cuda) {
                (Backend::Cuda, None)
            } else {
                warn!(
                    "Model '{}' requested CUDA, but CUDA is unavailable; using global policy",
                    model_id
                );
                global()
            }
        }
        ModelBackendSetting::Vulkan => {
            if transcribe_cpp::backend_available(Backend::Vulkan) {
                (Backend::Vulkan, None)
            } else {
                warn!(
                    "Model '{}' requested Vulkan, but Vulkan is unavailable; using global policy",
                    model_id
                );
                global()
            }
        }
        ModelBackendSetting::VulkanNvidia => {
            if transcribe_cpp::backend_available(Backend::Vulkan) {
                if let Some(dev) = find_vulkan_device(VulkanTargetDevice::Nvidia) {
                    (Backend::Vulkan, Some(dev))
                } else {
                    warn!(
                        "Model '{}' requested Vulkan (NVIDIA), but no NVIDIA Vulkan device was found; falling back to default Vulkan",
                        model_id
                    );
                    (Backend::Vulkan, None)
                }
            } else {
                warn!(
                    "Model '{}' requested Vulkan (NVIDIA), but Vulkan is unavailable; using global policy",
                    model_id
                );
                global()
            }
        }
        ModelBackendSetting::VulkanIntel => {
            if transcribe_cpp::backend_available(Backend::Vulkan) {
                if let Some(dev) = find_vulkan_device(VulkanTargetDevice::Intel) {
                    (Backend::Vulkan, Some(dev))
                } else {
                    warn!(
                        "Model '{}' requested Vulkan (Intel), but no Intel Vulkan device was found; falling back to default Vulkan",
                        model_id
                    );
                    (Backend::Vulkan, None)
                }
            } else {
                warn!(
                    "Model '{}' requested Vulkan (Intel), but Vulkan is unavailable; using global policy",
                    model_id
                );
                global()
            }
        }
        ModelBackendSetting::Metal => {
            if transcribe_cpp::backend_available(Backend::Metal) {
                (Backend::Metal, None)
            } else {
                warn!(
                    "Model '{}' requested Metal, but Metal is unavailable; using global policy",
                    model_id
                );
                global()
            }
        }
        ModelBackendSetting::Rocm => {
            if transcribe_cpp::backend_available(Backend::Rocm) {
                (Backend::Rocm, None)
            } else {
                warn!(
                    "Model '{}' requested ROCm, but ROCm is unavailable; using global policy",
                    model_id
                );
                global()
            }
        }
    }
}

fn transcribe_device_key(device: &transcribe_cpp::Device) -> String {
    let (identity_kind, identity) = match device.device_id.as_deref() {
        Some(device_id) => ("id", device_id),
        None => ("name", device.name.as_str()),
    };
    serde_json::to_string(&(device.kind.as_str(), identity_kind, identity))
        .expect("transcribe device identity is always JSON serializable")
}

fn transcribe_device_label(device: &transcribe_cpp::Device) -> String {
    if device.description.is_empty() {
        device.name.clone()
    } else {
        device.description.clone()
    }
}

/// Log the user's accelerator preference. Called on startup and before
/// loading a model.
///
/// The transcribe.cpp backend is not set here: it is chosen at model-load time
/// from [`select_transcribe_backend`], so changing the accelerator only needs a
/// model reload (see `reload_model_on_next_use`). There is no process-wide
/// accelerator state left to apply.
pub fn apply_accelerator_settings(app: &tauri::AppHandle) {
    let settings = get_settings(app);
    info!(
        "transcribe.cpp accelerator preference: {:?} (applied on next model load)",
        settings.transcribe_accelerator
    );
}

#[derive(Serialize, Clone, Debug, Type)]
pub struct GpuDeviceOption {
    pub id: String,
    pub name: String,
    pub total_vram_mb: u32,
}

static GPU_DEVICES: OnceLock<Vec<GpuDeviceOption>> = OnceLock::new();

fn transcribe_gpu_disabled_for_host() -> bool {
    crate::utils::is_windows_x64_emulated_on_arm64()
}

fn effective_transcribe_accelerator(
    setting: TranscribeAcceleratorSetting,
    gpu_disabled: bool,
) -> TranscribeAcceleratorSetting {
    if gpu_disabled {
        TranscribeAcceleratorSetting::Cpu
    } else {
        setting
    }
}

fn is_transcribe_gpu_device(device: &transcribe_cpp::Device) -> bool {
    matches!(
        device.device_type,
        transcribe_cpp::DeviceType::Gpu | transcribe_cpp::DeviceType::Igpu
    )
}

fn transcribe_device_allowed(kind: &str, gpu_disabled: bool) -> bool {
    !gpu_disabled || matches!(kind, "cpu" | "accel")
}

fn transcribe_compute_devices() -> Vec<transcribe_cpp::Device> {
    let devices = transcribe_cpp::devices();
    let gpu_disabled = transcribe_gpu_disabled_for_host();
    if !gpu_disabled {
        return devices;
    }

    devices
        .into_iter()
        .filter(|device| transcribe_device_allowed(&device.kind, gpu_disabled))
        .collect()
}

fn available_transcribe_accelerators(gpu_disabled: bool) -> Vec<String> {
    if gpu_disabled {
        vec!["cpu".to_string()]
    } else {
        vec!["auto".to_string(), "cpu".to_string(), "gpu".to_string()]
    }
}

fn cached_gpu_devices() -> &'static [GpuDeviceOption] {
    // GPU compute devices transcribe-cpp registered at startup. `id` is a
    // persistent identity key, never the process-local registry index. It uses
    // the backend's device_id where available and its name otherwise (Metal).
    // `total_vram_mb` is 0 when the backend does not report capacity.
    GPU_DEVICES.get_or_init(|| {
        transcribe_compute_devices()
            .into_iter()
            .filter(is_transcribe_gpu_device)
            .map(|d| GpuDeviceOption {
                id: transcribe_device_key(&d),
                name: transcribe_device_label(&d),
                total_vram_mb: (d.memory_total / (1024 * 1024)) as u32,
            })
            .collect()
    })
}

/// The compute backends this process can actually load a model on, as
/// [`crate::settings::ModelBackendSetting`] wire names, for the per-model
/// backend dropdown.
///
/// `auto` comes first because it is not a backend but the "follow the global
/// accelerator setting" choice; `cpu` is always offered (the CPU backend is
/// compiled in unconditionally and is the engine's own fallback). A GPU entry
/// appears only when the engine reports that backend as loadable in this build
/// — which is the same predicate `resolve_model_backend` checks before honouring
/// an override, so what the dropdown offers and what a load will accept cannot
/// drift apart.
///
/// Also validates a stored choice (`set_model_backend_setting`). Cheap by
/// construction — `backend_available` reads the backend registry, it does not
/// enumerate devices, so this does not pay the first-call GPU probe that
/// `get_available_accelerators` does.
pub fn available_model_backends() -> Vec<String> {
    let mut out = vec!["auto".to_string(), "cpu".to_string()];
    if transcribe_gpu_disabled_for_host() {
        return out;
    }
    if transcribe_cpp::backend_available(Backend::Cuda) {
        out.push("cuda".to_string());
    }
    if transcribe_cpp::backend_available(Backend::Vulkan) {
        out.push("vulkan".to_string());

        let has_nvidia = find_vulkan_device(VulkanTargetDevice::Nvidia).is_some();
        let has_intel = find_vulkan_device(VulkanTargetDevice::Intel).is_some();

        if has_nvidia {
            out.push("vulkan_nvidia".to_string());
        }
        if has_intel {
            out.push("vulkan_intel".to_string());
        }
    }
    if transcribe_cpp::backend_available(Backend::Metal) {
        out.push("metal".to_string());
    }
    if transcribe_cpp::backend_available(Backend::Rocm) {
        out.push("rocm".to_string());
    }
    out
}

#[derive(Serialize, Clone, Debug, Type)]
pub struct AvailableAccelerators {
    pub transcribe: Vec<String>,
    pub gpu_devices: Vec<GpuDeviceOption>,
    /// Per-model backend choices this process can honour, best first (see
    /// `available_model_backends`).
    pub model_backends: Vec<String>,
}

/// Return the accelerators available to this process on its current host.
pub fn get_available_accelerators() -> AvailableAccelerators {
    let transcribe_options = available_transcribe_accelerators(transcribe_gpu_disabled_for_host());

    AvailableAccelerators {
        transcribe: transcribe_options,
        gpu_devices: cached_gpu_devices().to_vec(),
        model_backends: available_model_backends(),
    }
}

impl Drop for TranscriptionManager {
    fn drop(&mut self) {
        // Skip shutdown unless this is the very last clone. TranscriptionManager
        // is cloned by initiate_model_load() and the watcher thread — those
        // clones dropping must not kill the watcher. The idle watcher owns a
        // clone for its whole life, so strong_count never drops to 1 while it
        // runs and this early return always fires; the watcher is torn down
        // with the process. Kept as a guard in case the watcher ever stops
        // holding a clone.
        if Arc::strong_count(&self.engine) > 1 {
            return;
        }

        // Signal the watcher thread to shutdown
        self.shutdown_signal.store(true, Ordering::Relaxed);

        // Wait for the thread to finish gracefully.
        // Use match instead of unwrap to avoid panicking if the mutex is
        // poisoned — a panic inside Drop calls abort().
        let mut guard = match self.watcher_handle.lock() {
            Ok(g) => g,
            Err(e) => {
                warn!(
                    "Recovered poisoned watcher_handle mutex during TranscriptionManager drop — a panic occurred earlier this session"
                );
                e.into_inner()
            }
        };
        if let Some(handle) = guard.take() {
            if let Err(e) = handle.join() {
                warn!("Failed to join idle watcher thread: {:?}", e);
            } else {
                debug!("Idle watcher thread joined successfully");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn languages(codes: &[&str]) -> Vec<String> {
        codes.iter().map(|code| (*code).to_string()).collect()
    }

    /// [`StreamTiming`] is what the Multi-STT streaming coordinator differences
    /// across a chunk to report the primary model's rate, so the two properties
    /// that make that arithmetic safe are pinned here: the counters only ever
    /// grow, and a delta converts to the same `(audio_secs, compute_secs)` pair
    /// the worker's own `Live preview perf` line uses.
    #[test]
    fn stream_timing_accumulates_and_converts_a_delta_to_seconds() {
        let timing = StreamTiming::default();
        assert_eq!(timing.totals(), (0, 0));

        // One second of 16 kHz audio, split across two feeds of a different
        // size each, and 250 ms of compute — so a passing test cannot come from
        // a single coincidentally-equal write.
        timing.record_feed(6_000);
        timing.record_compute(Duration::from_millis(100));
        timing.record_feed(10_000);
        timing.record_compute(Duration::from_millis(150));
        assert_eq!(timing.totals(), (16_000, 250_000));

        let (audio_secs, compute_secs) = StreamTiming::secs(timing.totals());
        assert!((audio_secs - 1.0).abs() < f64::EPSILON);
        assert!((compute_secs - 0.25).abs() < f64::EPSILON);
        assert!((real_time_factor(audio_secs, compute_secs) - 4.0).abs() < 1e-9);

        // A second read with nothing fed in between differences to zero, which
        // is what makes the coordinator's line stay silent for a chunk with no
        // new audio behind it rather than reporting a stale rate.
        let delta = (timing.totals().0 - 16_000, timing.totals().1 - 250_000);
        assert_eq!(StreamTiming::secs(delta), (0.0, 0.0));
    }

    /// The rate helper is shared with `multi_stt_stream`, and its zero-compute
    /// arm is what keeps a feed that failed before touching the model from
    /// reading as an infinite rate.
    #[test]
    fn real_time_factor_is_zero_when_nothing_was_computed() {
        assert_eq!(real_time_factor(5.0, 0.0), 0.0);
        assert_eq!(real_time_factor(0.0, 5.0), 0.0);
        assert!((real_time_factor(10.0, 2.0) - 5.0).abs() < 1e-9);
    }

    #[test]
    fn normal_hosts_preserve_every_transcribe_accelerator_setting() {
        for setting in [
            TranscribeAcceleratorSetting::Auto,
            TranscribeAcceleratorSetting::Cpu,
            TranscribeAcceleratorSetting::Gpu,
        ] {
            assert_eq!(effective_transcribe_accelerator(setting, false), setting);
        }
        assert_eq!(
            available_transcribe_accelerators(false),
            ["auto", "cpu", "gpu"]
        );
        for kind in ["cpu", "accel", "metal", "cuda", "vulkan", "gpu"] {
            assert!(transcribe_device_allowed(kind, false));
        }
    }

    #[test]
    fn emulated_x64_on_arm64_forces_every_transcribe_setting_to_cpu() {
        for setting in [
            TranscribeAcceleratorSetting::Auto,
            TranscribeAcceleratorSetting::Cpu,
            TranscribeAcceleratorSetting::Gpu,
        ] {
            assert_eq!(
                effective_transcribe_accelerator(setting, true),
                TranscribeAcceleratorSetting::Cpu
            );
        }
        assert_eq!(available_transcribe_accelerators(true), ["cpu"]);
        assert!(transcribe_device_allowed("cpu", true));
        assert!(transcribe_device_allowed("accel", true));
        for kind in ["metal", "cuda", "vulkan", "gpu", "unknown"] {
            assert!(!transcribe_device_allowed(kind, true));
        }
    }

    #[test]
    fn optional_text_transform_falls_back_to_raw_text_after_panic() {
        let raw = "原始轉錄。".to_string();
        let result = fail_open_text_transform(raw.clone(), |_| {
            panic!("simulated optional cleanup failure")
        });

        assert_eq!(result, raw);
    }

    #[test]
    fn portuguese_transcription_does_not_use_english_ui_filler_words() {
        let settings = AppSettings {
            app_language: "en".to_string(),
            selected_language: "pt-BR".to_string(),
            ..Default::default()
        };
        let supported = languages(&["en", "pt"]);
        let evidence = resolve_output_language_evidence(&settings, Some("pt"), &supported, false);

        let result = post_process_transcription_text(
            "eu vi um carro".to_string(),
            &settings,
            false,
            &evidence,
            &supported,
        );

        assert_eq!(
            evidence,
            OutputLanguageEvidence::UserSelected("pt".to_string())
        );
        assert_eq!(result, "eu vi um carro");
    }

    #[test]
    fn norwegian_alias_is_recorded_as_user_selected_evidence() {
        let settings = AppSettings {
            selected_language: "no".to_string(),
            ..Default::default()
        };

        let evidence =
            resolve_output_language_evidence(&settings, Some("nb"), &languages(&["nb"]), false);

        assert_eq!(
            evidence,
            OutputLanguageEvidence::UserSelected("nb".to_string())
        );
    }

    #[test]
    fn auto_language_without_detection_skips_gated_filler_removal() {
        let settings = AppSettings {
            selected_language: "auto".to_string(),
            ..Default::default()
        };
        let evidence =
            resolve_output_language_evidence(&settings, None, &languages(&["en", "pt"]), false);

        // Too short for a reliable text detection, so the gated "um" must
        // survive; the universal "uhm" is removed regardless.
        let result = post_process_transcription_text(
            "um uhm ok".to_string(),
            &settings,
            false,
            &evidence,
            &languages(&["en", "pt"]),
        );

        assert_eq!(evidence, OutputLanguageEvidence::Unknown);
        assert_eq!(result, "um ok");
    }

    #[test]
    fn unknown_evidence_with_confident_text_detection_removes_gated_fillers() {
        let settings = AppSettings {
            selected_language: "auto".to_string(),
            ..Default::default()
        };

        let result = post_process_transcription_text(
            "um so the weather forecast said it would probably rain throughout the whole weekend"
                .to_string(),
            &settings,
            false,
            &OutputLanguageEvidence::Unknown,
            &languages(&["en", "pt", "es", "de"]),
        );

        assert_eq!(
            result,
            "so the weather forecast said it would probably rain throughout the whole weekend"
        );
    }

    #[test]
    fn unknown_evidence_with_portuguese_text_preserves_um() {
        let settings = AppSettings {
            selected_language: "auto".to_string(),
            ..Default::default()
        };

        let result = post_process_transcription_text(
            "eu vi um carro na rua ontem de manhã quando fui ao mercado".to_string(),
            &settings,
            false,
            &OutputLanguageEvidence::Unknown,
            &languages(&["en", "pt", "es", "de"]),
        );

        assert_eq!(
            result,
            "eu vi um carro na rua ontem de manhã quando fui ao mercado"
        );
    }

    #[test]
    fn model_detected_language_upgrades_unknown_evidence_only() {
        assert_eq!(
            with_model_detected_language(OutputLanguageEvidence::Unknown, Some("en".to_string())),
            OutputLanguageEvidence::ModelDetected("en".to_string())
        );
        assert_eq!(
            with_model_detected_language(OutputLanguageEvidence::Unknown, Some("auto".to_string())),
            OutputLanguageEvidence::Unknown
        );
        assert_eq!(
            with_model_detected_language(OutputLanguageEvidence::Unknown, None),
            OutputLanguageEvidence::Unknown
        );
        assert_eq!(
            with_model_detected_language(
                OutputLanguageEvidence::UserSelected("pt".to_string()),
                Some("en".to_string())
            ),
            OutputLanguageEvidence::UserSelected("pt".to_string())
        );
    }

    #[test]
    fn auto_language_uses_single_language_model_as_evidence() {
        let settings = AppSettings {
            selected_language: "auto".to_string(),
            ..Default::default()
        };

        let evidence =
            resolve_output_language_evidence(&settings, None, &languages(&["en"]), false);

        assert_eq!(
            evidence,
            OutputLanguageEvidence::ModelConstrained("en".to_string())
        );
    }

    #[test]
    fn unsupported_explicit_language_uses_model_fallback_as_evidence() {
        let settings = AppSettings {
            selected_language: "pt".to_string(),
            ..Default::default()
        };

        let evidence = resolve_output_language_evidence(
            &settings,
            Some("en"),
            &languages(&["en", "de"]),
            false,
        );

        assert_eq!(
            evidence,
            OutputLanguageEvidence::ModelConstrained("en".to_string())
        );
    }

    #[test]
    fn ignored_user_language_is_not_output_evidence() {
        let settings = AppSettings {
            // Parakeet V3 ignores language hints and auto-detects even when a
            // selection from the previously active model remains persisted.
            selected_language: "en".to_string(),
            ..Default::default()
        };
        let supported = languages(&["en", "de", "pt"]);

        let evidence = resolve_output_language_evidence(&settings, None, &supported, false);
        assert_eq!(evidence, OutputLanguageEvidence::Unknown);

        let result = post_process_transcription_text(
            "eu vi um carro".to_string(),
            &settings,
            false,
            &evidence,
            &supported,
        );
        assert_eq!(result, "eu vi um carro");
    }

    #[test]
    fn unapplied_transcribe_cpp_language_is_not_output_evidence() {
        let settings = AppSettings {
            selected_language: "en".to_string(),
            ..Default::default()
        };
        let supported = languages(&[]);
        let plan = transcribe_cpp_run_plan(false, "en", &supported, false);

        assert_eq!(plan.language, None);
        assert_eq!(
            resolve_output_language_evidence(
                &settings,
                plan.language.as_deref(),
                &supported,
                false,
            ),
            OutputLanguageEvidence::Unknown
        );
    }

    #[test]
    fn translated_output_is_treated_as_english() {
        let settings = AppSettings {
            selected_language: "pt".to_string(),
            ..Default::default()
        };

        let evidence = resolve_output_language_evidence(
            &settings,
            Some("pt"),
            &languages(&["en", "pt"]),
            true,
        );

        assert_eq!(evidence, OutputLanguageEvidence::TranslatedToEnglish);
    }

    #[test]
    fn transcribe_cpp_run_plan_maps_chinese_variants() {
        let plan = transcribe_cpp_run_plan(false, "zh-Hant", &languages(&["zh"]), true);

        assert!(matches!(plan.task, Task::Transcribe));
        assert_eq!(plan.language.as_deref(), Some("zh"));
        assert_eq!(plan.target_language, None);
    }

    #[test]
    fn transcribe_cpp_run_plan_skips_english_translation() {
        let plan = transcribe_cpp_run_plan(true, "en", &languages(&["en", "es"]), true);

        assert!(matches!(plan.task, Task::Transcribe));
        assert_eq!(plan.language.as_deref(), Some("en"));
        assert_eq!(plan.target_language, None);
    }

    #[test]
    fn transcribe_cpp_run_plan_translates_supported_non_english() {
        let plan = transcribe_cpp_run_plan(true, "es", &languages(&["en", "es"]), true);

        assert!(matches!(plan.task, Task::Translate));
        assert_eq!(plan.language.as_deref(), Some("es"));
        assert_eq!(plan.target_language.as_deref(), Some("en"));
    }

    #[test]
    fn transcribe_cpp_run_plan_requires_model_translation_support() {
        let plan = transcribe_cpp_run_plan(true, "es", &languages(&["en", "es"]), false);

        assert!(matches!(plan.task, Task::Transcribe));
        assert_eq!(plan.language.as_deref(), Some("es"));
        assert_eq!(plan.target_language, None);
    }

    /// The two directions of the Multi-STT panel's slot numbering have to agree:
    /// [`multi_stt_extra_model`] answers "which model is slot n" (for the account
    /// a finished stream gives of itself) and [`apply_extra_model_settings`]
    /// answers "which settings are this model's" (for the decode). A slot that
    /// resolved to one model on the way in and another on the way out would give
    /// the merge a text from the wrong settings and report it under the wrong
    /// name, and both failures are silent.
    #[test]
    fn multi_stt_slots_and_models_resolve_to_each_other() {
        let slot = |id: &str| crate::settings::MultiSttExtraModel {
            model_id: Some(id.into()),
            ..Default::default()
        };
        let mut settings = AppSettings {
            multi_stt_extra_models: vec![
                slot("model-two"),
                slot("model-three"),
                slot("model-four"),
            ],
            ..Default::default()
        };

        // The primary is not one of the panel's models: its stream is the app's
        // own transcription, so nothing may be attributed to it.
        assert_eq!(multi_stt_extra_model(&settings, PRIMARY_STREAM_SLOT), None);
        assert_eq!(
            multi_stt_extra_model(&settings, 1).as_deref(),
            Some("model-two")
        );
        assert_eq!(
            multi_stt_extra_model(&settings, 2).as_deref(),
            Some("model-three")
        );
        assert_eq!(
            multi_stt_extra_model(&settings, 3).as_deref(),
            Some("model-four")
        );
        // Past the configured extras there is no further model to name.
        assert_eq!(multi_stt_extra_model(&settings, 4), None);

        // And each of those ids folds its own slot's preferences back in.
        settings.multi_stt_extra_models[1].language = Some("de".into());
        settings.selected_language = "en".into();
        let mut resolved = settings.clone();
        apply_extra_model_settings(&mut resolved, "model-three");
        assert_eq!(resolved.selected_language, "de");
        // A model that is not in the panel keeps the global language.
        let mut untouched = settings.clone();
        apply_extra_model_settings(&mut untouched, "some-other-model");
        assert_eq!(untouched.selected_language, "en");
    }

    /// Per-model latency is shared across quant siblings, so a Nemotron Q8
    /// primary and a Nemotron Q4 extra would stream at one setting. The slot's
    /// own override is what the extra's stream worker must resolve, and a slot
    /// without one keeps following the model's entry.
    #[test]
    fn extra_slot_latency_overrides_the_shared_model_setting() {
        use crate::settings::NativeStreamingLatencyPreset as P;
        let repo = "handy-computer/nemotron-3.5-asr-streaming-0.6b-gguf";
        let primary = format!("{repo}/nemotron-3.5-asr-streaming-0.6b-Q8_0.gguf");
        let extra = format!("{repo}/nemotron-3.5-asr-streaming-0.6b-Q4_K_M.gguf");
        let r2t2 = "davidxifeng/Confucius4-R2T2-gguf/r2t2-q8_0.gguf";
        let mut settings = AppSettings {
            multi_stt_extra_models: vec![
                crate::settings::MultiSttExtraModel {
                    model_id: Some(extra.clone()),
                    latency_preset: Some(P::Fastest),
                    ..Default::default()
                },
                crate::settings::MultiSttExtraModel {
                    model_id: Some(r2t2.into()),
                    chunk_ms: Some(160),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        settings
            .native_streaming_latency_presets
            .insert(primary.clone(), P::Balanced);
        settings
            .native_streaming_chunk_ms
            .insert(r2t2.to_string(), 960);

        // Without the slot overlay the extra inherits the primary's sibling value.
        assert_eq!(get_effective_latency_preset(&settings, &extra), P::Balanced);

        let mut resolved = settings.clone();
        apply_extra_model_settings(&mut resolved, &extra);
        assert_eq!(get_effective_latency_preset(&resolved, &extra), P::Fastest);
        assert_eq!(
            get_effective_latency_preset(&resolved, &primary),
            P::Balanced
        );

        let mut resolved = settings.clone();
        apply_extra_model_settings(&mut resolved, r2t2);
        assert_eq!(get_effective_r2t2_chunk_ms(&resolved, r2t2), 160);

        settings.multi_stt_extra_models[1].chunk_ms = None;
        let mut resolved = settings.clone();
        apply_extra_model_settings(&mut resolved, r2t2);
        assert_eq!(get_effective_r2t2_chunk_ms(&resolved, r2t2), 960);
    }

    #[test]
    fn available_model_backends_contains_defaults() {
        let backends = available_model_backends();
        assert!(backends.contains(&"auto".to_string()));
        assert!(backends.contains(&"cpu".to_string()));
    }

    #[test]
    fn effective_model_backend_inherits_across_quant_variants() {
        let mut settings = AppSettings::default();
        let base = "davidxifeng/Confucius4-R2T2-gguf";
        let q8 = "davidxifeng/Confucius4-R2T2-gguf/r2t2-q8_0.gguf";
        let q4 = "davidxifeng/Confucius4-R2T2-gguf/r2t2-q4_k_m.gguf";

        // Without settings, both resolve to Auto
        assert_eq!(
            get_effective_model_backend(&settings, q8),
            ModelBackendSetting::Auto
        );
        assert_eq!(
            get_effective_model_backend(&settings, q4),
            ModelBackendSetting::Auto
        );

        // Setting on base repo applies to all quants
        settings
            .per_model_backends
            .insert(base.to_string(), ModelBackendSetting::VulkanIntel);
        assert_eq!(
            get_effective_model_backend(&settings, q8),
            ModelBackendSetting::VulkanIntel
        );
        assert_eq!(
            get_effective_model_backend(&settings, q4),
            ModelBackendSetting::VulkanIntel
        );

        // Setting on sibling quant (q8) inherits to q4 when base is absent
        settings.per_model_backends.clear();
        settings
            .per_model_backends
            .insert(q8.to_string(), ModelBackendSetting::VulkanNvidia);
        assert_eq!(
            get_effective_model_backend(&settings, q4),
            ModelBackendSetting::VulkanNvidia
        );
        assert_eq!(
            get_effective_model_backend(&settings, base),
            ModelBackendSetting::VulkanNvidia
        );

        // Specific quant override takes precedence
        settings
            .per_model_backends
            .insert(q4.to_string(), ModelBackendSetting::Cpu);
        assert_eq!(
            get_effective_model_backend(&settings, q4),
            ModelBackendSetting::Cpu
        );
        assert_eq!(
            get_effective_model_backend(&settings, q8),
            ModelBackendSetting::VulkanNvidia
        );
    }

    #[test]
    fn effective_r2t2_chunk_ms_inherits_across_quant_variants() {
        let mut settings = AppSettings::default();
        let base = "davidxifeng/Confucius4-R2T2-gguf";
        let q8 = "davidxifeng/Confucius4-R2T2-gguf/r2t2-q8_0.gguf";
        let q4 = "davidxifeng/Confucius4-R2T2-gguf/r2t2-q4_k_m.gguf";

        assert_eq!(get_effective_r2t2_chunk_ms(&settings, q4), 320);

        settings
            .native_streaming_chunk_ms
            .insert(q8.to_string(), 160);
        assert_eq!(get_effective_r2t2_chunk_ms(&settings, q4), 160);
        assert_eq!(get_effective_r2t2_chunk_ms(&settings, base), 160);

        settings
            .native_streaming_chunk_ms
            .insert(q4.to_string(), 80);
        assert_eq!(get_effective_r2t2_chunk_ms(&settings, q4), 80);
        assert_eq!(get_effective_r2t2_chunk_ms(&settings, q8), 160);
    }
}
