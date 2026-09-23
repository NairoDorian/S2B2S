//! Live FFT (fork feature): a real-time spectrum analyser page fed by the
//! microphone (`dsp.rs` holds the DSP; this file is the plumbing).
//!
//! Threading ("Async analysis" on the page):
//!
//! * The audio consumer thread pushes each drained chunk into a wait-free
//!   SPSC ring ([`AnalysisTap`], an [`AnalysisSink`] the recorder calls).
//!   When the page is closed that costs one atomic load per chunk; when it
//!   is open, one `try_lock` on an uncontended mutex and a memcpy. Nothing
//!   on that thread ever waits for this module.
//! * A `live-fft` worker thread drains the ring, runs the pipeline at
//!   `update_rate_hz` and encodes each frame as raw little-endian bytes
//!   ([`encode_frame`]) into a slot the page polls with the `live_fft_frame`
//!   command (like the overlay's scope): no JSON, no event per frame, and a
//!   poll that already has the newest frame gets a 32-byte header. Latest
//!   wins: a slow webview never backs up the analysis. The Hz of every
//!   output bin is fetched separately (`live_fft_axis`) when a frame's axis
//!   version changes.
//! * With `async_analysis` off the pipeline runs inline on the consumer
//!   thread; the worker then only supervises. On that thread every lock is
//!   a `try_lock` (a contended settings refresh, status write or frame
//!   publish is skipped and retried on the next frame) and nothing is
//!   emitted.
//! * The worker also checks once a second that the recording is still ours
//!   (the cancel hotkey ends it), that the main window is visible (frames
//!   are not computed for a hidden window, and a session hidden for two
//!   minutes is stopped so the microphone is not held for nothing), and
//!   publishes a status heartbeat.
//!
//! A session is a normal recording under the `live_fft` binding with
//! the VAD off (or in `Streaming` policy when `show_vad` drives the
//! voice-detection view) and the captured audio discarded (nothing is
//! transcribed or saved), so the transcription hotkeys get "Already recording" while it
//! runs, exactly like the live VAD test.

pub mod dsp;
pub mod scope;

use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, PoisonError, TryLockError};
use std::thread;
use std::time::{Duration, Instant};

use log::{debug, info, warn};
use rtrb::{Consumer, CopyToUninit, Producer, RingBuffer};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};
use tauri_specta::Event;

use crate::audio_toolkit::VadPolicy;
use crate::audio_toolkit::audio::AnalysisSink;
use crate::managers::audio::{
    AudioRecordingManager, RecordingStartOptions, VAD_REPORT_LIVE_FFT, set_vad_reporting,
};
use crate::settings::{FftSource, LiveFftSettings, get_settings};
use dsp::SpectrumPipeline;
use scope::ScopeShared;

/// Binding id the analyser records under (shows up in logs).
pub const LIVE_FFT_BINDING: &str = "live_fft";
/// Ring capacity between the audio consumer thread and the worker: two
/// seconds at 96 kHz (768 KB), the same slack the capture ring has.
const RING_CAPACITY: usize = 192_000;
/// Supervision / status heartbeat interval.
const SUPERVISE_TICK: Duration = Duration::from_millis(1000);
/// A session whose window has been hidden this long is stopped.
const HIDDEN_AUTO_STOP: Duration = Duration::from_secs(120);
/// Longest nap of the worker between ticks, so a rate change or a stop
/// request is noticed promptly.
const MAX_NAP: Duration = Duration::from_millis(20);
/// Largest slice drained from the ring per pass (a full ring is two of them).
const MAX_DRAIN: usize = RING_CAPACITY / 2;

/* ───────────────────────── events & status ───────────────────────── */

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum LiveFftPhase {
    #[default]
    Idle,
    Starting,
    Running,
    Stopping,
    Error,
}

/// Why the last session ended on its own.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum LiveFftStopReason {
    /// The recording was ended elsewhere (cancel hotkey, tray).
    Cancelled,
    /// The main window stayed hidden for [`HIDDEN_AUTO_STOP`].
    WindowHidden,
}

/// Status snapshot — the telemetry rows the page shows.
#[derive(Serialize, Deserialize, Clone, Debug, Default, Type)]
pub struct LiveFftStatus {
    pub phase: LiveFftPhase,
    pub error: Option<String>,
    pub stop_reason: Option<LiveFftStopReason>,
    /// Rate of the analysed signal (native microphone rate or 16 kHz).
    pub sample_rate: u32,
    pub fft_size: u32,
    pub window_samples: u32,
    pub linear_bins: u32,
    /// Magnitude bins actually computed.
    pub magnitude_bins: u32,
    pub output_bins: u32,
    pub identity_warp: bool,
    /// Magnitude of a full-scale sine under the current normalisation.
    pub full_scale_ref: f32,
    /// Highest frequency on the axis after the Nyquist clamp.
    pub display_max_hz: f32,
    pub nyquist_hz: f32,
    pub update_rate_hz: u32,
    pub async_analysis: bool,
    /// Frames analysed this session.
    pub frames: u32,
    /// Samples the ring could not take (the worker fell behind).
    pub dropped_samples: u32,
    pub dsp_us_last: f32,
    pub dsp_us_avg: f32,
    pub dsp_us_max: f32,
    pub started_at_ms: Option<f64>,
    /// Version of the output axis (`LiveFftAxis.version`); bumps whenever
    /// the Hz of the output bins change.
    pub axis_version: u32,
    /// `display_max_hz / (output_bins − 1)`: the spacing of a uniform axis,
    /// the mean spacing of a perceptual one (0 below two bins).
    pub hz_per_bin: f32,
    /// Output bins formed by peak / RMS aggregation over the FFT bins they
    /// own rather than by interpolation.
    pub aggregated_bins: u32,
    /// β of the Kaiser window in use, after the Auto rule (0 for any other
    /// window).
    pub kaiser_beta: f32,
    /// Half the window plus, with async analysis, one frame period: how
    /// far behind the audio a frame's centre is.
    pub analysis_latency_ms: f32,
    /// The frames are the raw rfft magnitudes.
    pub raw_bins: bool,
}

/// Emitted on every phase change and roughly once a second while running.
#[derive(Serialize, Deserialize, Clone, Debug, Type, tauri_specta::Event)]
pub struct LiveFftStateEvent {
    pub status: LiveFftStatus,
}

/// The frequency of every output bin, fetched by the page when a frame's
/// axis version differs from the one it holds.
#[derive(Serialize, Deserialize, Clone, Debug, Default, Type)]
pub struct LiveFftAxis {
    /// Matches word 3 of a frame header; 0 = no axis yet.
    pub version: u32,
    /// Centre frequency of each output bin, in Hz.
    pub hz: Vec<f32>,
}

/* ───────────────────────── frame transport ───────────────────────── */

/// Words (u32 / f32, little-endian) in a page frame header.
pub const FRAME_HEADER_WORDS: usize = 8;
/// Header flag: the analysed window was digital silence.
pub const FRAME_FLAG_SILENT: u32 = 1;
/// Header flag: a running session produced the frame.
pub const FRAME_FLAG_ACTIVE: u32 = 2;
/// Header flag: the spectral features follow the bins.
pub const FRAME_FLAG_HAS_FEATURES: u32 = 4;
/// Spectral features per frame when they are shipped.
pub const FRAME_FEATURES: usize = 8;

/// What a page frame carries besides its arrays.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrameHeader {
    pub seq: u32,
    pub flags: u32,
    pub axis_version: u32,
    pub peak_hz: f32,
    /// In output units (linear, dB or 0…1).
    pub peak_value: f32,
    pub dsp_us: f32,
}

/// Append `values` as little-endian f32 bytes, in one copy on a
/// little-endian target.
pub(crate) fn extend_f32_le(out: &mut Vec<u8>, values: &[f32]) {
    #[cfg(target_endian = "little")]
    {
        // SAFETY: `f32` has no padding and no invalid bit patterns as bytes,
        // `u8` has alignment 1, and the byte slice covers exactly the memory
        // of `values`, which outlives it.
        let bytes = unsafe {
            std::slice::from_raw_parts(values.as_ptr().cast::<u8>(), std::mem::size_of_val(values))
        };
        out.extend_from_slice(bytes);
    }
    #[cfg(not(target_endian = "little"))]
    for v in values {
        out.extend_from_slice(&v.to_le_bytes());
    }
}

/// Encode a page frame: [`FRAME_HEADER_WORDS`] little-endian words — seq,
/// flags, bins length, axis version, peak Hz (f32), peak value (f32), DSP
/// time in µs (f32), features length (0 or 8) — then the bins and the
/// features (centroid Hz, rolloff Hz, flatness, flux, RMS dB, bass / mid /
/// high dB) as f32. The page maps `Float32Array` views onto the buffer.
pub fn encode_frame(
    out: &mut Vec<u8>,
    header: &FrameHeader,
    bins: &[f32],
    features: Option<&[f32; FRAME_FEATURES]>,
) {
    let features: &[f32] = match features {
        Some(f) => f,
        None => &[],
    };
    out.clear();
    out.reserve((FRAME_HEADER_WORDS + bins.len() + features.len()) * 4);
    for word in [
        header.seq,
        header.flags,
        bins.len() as u32,
        header.axis_version,
        header.peak_hz.to_bits(),
        header.peak_value.to_bits(),
        header.dsp_us.to_bits(),
        features.len() as u32,
    ] {
        out.extend_from_slice(&word.to_le_bytes());
    }
    extend_f32_le(out, bins);
    extend_f32_le(out, features);
}

/// The header of an encoded frame alone, with the payload-length words
/// `len_words` set to 0: the reply to a poll that already holds this seq.
pub(crate) fn header_only(frame: &[u8], len_words: &[usize]) -> Vec<u8> {
    let mut out = frame[..frame.len().min(FRAME_HEADER_WORDS * 4)].to_vec();
    for &w in len_words {
        if let Some(word) = out.get_mut(w * 4..w * 4 + 4) {
            word.fill(0);
        }
    }
    out
}

/// Word 0 of an encoded frame.
pub(crate) fn frame_seq(frame: &[u8]) -> u32 {
    frame
        .get(..4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .unwrap_or(0)
}

/// Header-only frame with no ACTIVE flag: what the page reads before the
/// first frame and after a session.
fn idle_frame(seq: u32) -> Vec<u8> {
    let mut out = Vec::new();
    encode_frame(
        &mut out,
        &FrameHeader {
            seq,
            ..FrameHeader::default()
        },
        &[],
        None,
    );
    out
}

/// Process-wide frame numbers (never 0, which means "no frame"), so a poll
/// carrying a seq from an earlier session can never match a new frame.
static NEXT_FRAME_SEQ: AtomicU32 = AtomicU32::new(1);

fn next_frame_seq() -> u32 {
    loop {
        let v = NEXT_FRAME_SEQ.fetch_add(1, Ordering::Relaxed);
        if v != 0 {
            return v;
        }
    }
}

/// `lock()` that shrugs off poisoning. Every value these mutexes guard stays
/// usable after a panic mid-update (telemetry, settings snapshots, the parked
/// ring consumer, join handles), and a panic in inline analysis on the audio
/// thread must not turn the session's own teardown into a second panic.
pub(super) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// `try_lock()` that shrugs off poisoning; `None` only when contended.
fn try_lock<T>(m: &Mutex<T>) -> Option<MutexGuard<'_, T>> {
    match m.try_lock() {
        Ok(guard) => Some(guard),
        Err(TryLockError::Poisoned(p)) => Some(p.into_inner()),
        Err(TryLockError::WouldBlock) => None,
    }
}

/* ───────────────────────── the tap ───────────────────────── */

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum TapSource {
    Native = 0,
    Processed = 1,
    Denoised = 2,
}

impl From<FftSource> for TapSource {
    fn from(source: FftSource) -> Self {
        match source {
            FftSource::Microphone => TapSource::Native,
            FftSource::Denoised => TapSource::Denoised,
            FftSource::Processed => TapSource::Processed,
        }
    }
}

/// What the tap drives when the pipeline runs inline on the audio consumer
/// thread. A trait object rather than the concrete [`Engine`] so the tap —
/// and its tests — never reference Tauri: linking `Engine` into a test
/// binary drags in the window runtime, which cannot start without the
/// application manifest.
trait InlineRunner: Send {
    fn feed(&mut self, samples: &[f32], sample_rate: u32);
}

/// The recorder-side end of the analyser: a wait-free ring plus the
/// gates the consumer thread reads. Process-wide, so the recorder can be
/// built before the manager exists (the always-on microphone opens on its
/// own thread during startup).
pub struct AnalysisTap {
    active: AtomicBool,
    source: AtomicU8,
    inline: AtomicBool,
    sample_rate: AtomicU32,
    dropped: AtomicU64,
    /// Only the audio consumer thread locks this, so it is never contended;
    /// `try_lock` keeps the guarantee that the thread cannot block here.
    producer: Mutex<Producer<f32>>,
    /// Parked here between sessions; the worker takes it while it runs.
    consumer: Mutex<Option<Consumer<f32>>>,
    /// The pipeline when it runs inline on the consumer thread.
    inline_engine: Mutex<Option<Box<dyn InlineRunner>>>,
}

static TAP: LazyLock<Arc<AnalysisTap>> = LazyLock::new(|| Arc::new(AnalysisTap::new()));

/// The process-wide tap the recorder is built with.
pub fn tap() -> Arc<AnalysisTap> {
    Arc::clone(&TAP)
}

impl AnalysisTap {
    fn new() -> Self {
        let (producer, consumer) = RingBuffer::new(RING_CAPACITY);
        Self {
            active: AtomicBool::new(false),
            source: AtomicU8::new(TapSource::Native as u8),
            inline: AtomicBool::new(false),
            sample_rate: AtomicU32::new(0),
            dropped: AtomicU64::new(0),
            producer: Mutex::new(producer),
            consumer: Mutex::new(Some(consumer)),
            inline_engine: Mutex::new(None),
        }
    }

    fn arm(&self, source: TapSource, inline: bool, runner: Option<Box<dyn InlineRunner>>) {
        self.source.store(source as u8, Ordering::Relaxed);
        self.inline.store(inline, Ordering::Relaxed);
        self.dropped.store(0, Ordering::Relaxed);
        // Unknown until the first chunk of this session: the previous
        // session's rate (another device, another source) must not be used.
        self.sample_rate.store(0, Ordering::Relaxed);
        *lock(&self.inline_engine) = runner;
        // Stale audio from a previous session must not reach the new one.
        if let Some(consumer) = lock(&self.consumer).as_mut() {
            let n = consumer.slots();
            if n > 0
                && let Ok(chunk) = consumer.read_chunk(n)
            {
                chunk.commit_all();
            }
        }
        self.active.store(true, Ordering::Release);
    }

    fn disarm(&self) {
        self.active.store(false, Ordering::Release);
        *lock(&self.inline_engine) = None;
    }

    fn take_consumer(&self) -> Option<Consumer<f32>> {
        lock(&self.consumer).take()
    }

    fn return_consumer(&self, consumer: Consumer<f32>) {
        *lock(&self.consumer) = Some(consumer);
    }

    fn set_source(&self, source: TapSource) {
        self.source.store(source as u8, Ordering::Relaxed);
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate.load(Ordering::Relaxed)
    }

    fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}

impl AnalysisSink for AnalysisTap {
    #[inline]
    fn wants_native(&self) -> bool {
        self.active.load(Ordering::Relaxed)
            && self.source.load(Ordering::Relaxed) == TapSource::Native as u8
    }

    #[inline]
    fn wants_denoised(&self) -> bool {
        self.active.load(Ordering::Relaxed)
            && self.source.load(Ordering::Relaxed) == TapSource::Denoised as u8
    }

    #[inline]
    fn wants_processed(&self) -> bool {
        self.active.load(Ordering::Relaxed)
            && self.source.load(Ordering::Relaxed) == TapSource::Processed as u8
    }

    fn push(&self, samples: &[f32], sample_rate: u32) {
        self.sample_rate.store(sample_rate, Ordering::Relaxed);
        if self.inline.load(Ordering::Relaxed) {
            if let Ok(mut runner) = self.inline_engine.try_lock()
                && let Some(runner) = runner.as_mut()
            {
                runner.feed(samples, sample_rate);
            }
            return;
        }
        let Ok(mut producer) = self.producer.try_lock() else {
            self.dropped
                .fetch_add(samples.len() as u64, Ordering::Relaxed);
            return;
        };
        let writable = producer.slots().min(samples.len());
        if writable > 0
            && let Ok(mut chunk) = producer.write_chunk_uninit(writable)
        {
            // Two slice copies (the ring may wrap), not one push per sample.
            let (first, second) = chunk.as_mut_slices();
            let split = first.len();
            samples[..split].copy_to_uninit(first);
            samples[split..writable].copy_to_uninit(second);
            // SAFETY: the two slices, i.e. every slot of the chunk, were
            // just written.
            unsafe { chunk.commit_all() };
        }
        if writable < samples.len() {
            self.dropped
                .fetch_add((samples.len() - writable) as u64, Ordering::Relaxed);
        }
    }
}

/* ───────────────────────── shared session state ───────────────────────── */

struct Shared {
    settings: Mutex<LiveFftSettings>,
    settings_version: AtomicU64,
    status: Mutex<LiveFftStatus>,
    active: AtomicBool,
    stop_requested: AtomicBool,
    reset_requested: AtomicBool,
    /// Cleared while the main window is hidden: frames are neither computed
    /// nor sent (docs/PERFORMANCE.md rule 4 / 9).
    emit_enabled: AtomicBool,
    /// The latest encoded page frame ([`encode_frame`]); swapped in whole by
    /// the engine, copied out by the `live_fft_frame` poll.
    frame: Mutex<Vec<u8>>,
    /// The Hz of every output bin, rewritten when the axis version changes.
    axis: Mutex<LiveFftAxis>,
}

impl Shared {
    fn new(settings: LiveFftSettings) -> Self {
        Self {
            settings: Mutex::new(settings),
            settings_version: AtomicU64::new(1),
            status: Mutex::new(LiveFftStatus::default()),
            active: AtomicBool::new(false),
            stop_requested: AtomicBool::new(false),
            reset_requested: AtomicBool::new(false),
            emit_enabled: AtomicBool::new(true),
            frame: Mutex::new(idle_frame(0)),
            axis: Mutex::new(LiveFftAxis::default()),
        }
    }

    fn settings_snapshot(&self) -> (LiveFftSettings, u64) {
        let version = self.settings_version.load(Ordering::Acquire);
        (lock(&self.settings).clone(), version)
    }

    /// [`Shared::settings_snapshot`] that gives up instead of waiting.
    fn try_settings_snapshot(&self) -> Option<(LiveFftSettings, u64)> {
        let version = self.settings_version.load(Ordering::Acquire);
        try_lock(&self.settings).map(|s| (s.clone(), version))
    }

    /// The latest frame, or its bare header when the caller already has it.
    fn frame_reply(&self, known_seq: Option<u32>) -> Vec<u8> {
        let frame = lock(&self.frame);
        if known_seq.is_some_and(|seq| seq == frame_seq(&frame)) {
            header_only(&frame, &[2, 7])
        } else {
            frame.clone()
        }
    }
}

/* ───────────────────────── the engine ───────────────────────── */

/// Pipeline + settings snapshot + telemetry; owned by the worker (async)
/// or parked inside the tap (inline).
struct Engine {
    shared: Arc<Shared>,
    /// Runs on the audio consumer thread: every lock is a `try_lock`.
    inline: bool,
    pipeline: SpectrumPipeline,
    settings: LiveFftSettings,
    settings_version: u64,
    period: Duration,
    out: Vec<f32>,
    /// Reused encode buffer; swapped with the shared slot on publish.
    encoded: Vec<u8>,
    last_process: Option<Instant>,
    frames: u32,
    dsp_avg: f32,
    dsp_max: f32,
    status_version_seen: u64,
    last_status_write: Instant,
    /// Axis version last written to the shared axis.
    axis_published: u32,
}

impl Engine {
    fn new(shared: Arc<Shared>, inline: bool) -> Self {
        let (settings, settings_version) = shared.settings_snapshot();
        let period = Duration::from_secs_f64(1.0 / f64::from(settings.update_rate_hz.max(1)));
        Self {
            shared,
            inline,
            pipeline: SpectrumPipeline::new(),
            settings,
            settings_version,
            period,
            out: Vec::new(),
            encoded: Vec::new(),
            last_process: None,
            frames: 0,
            dsp_avg: 0.0,
            dsp_max: 0.0,
            status_version_seen: u64::MAX,
            last_status_write: Instant::now(),
            axis_published: 0,
        }
    }

    /// Lock a shared slot: blocking on the worker, `try_lock` inline.
    fn guard<'a, T>(&self, m: &'a Mutex<T>) -> Option<MutexGuard<'a, T>> {
        if self.inline {
            try_lock(m)
        } else {
            Some(lock(m))
        }
    }

    fn refresh_settings(&mut self) {
        let version = self.shared.settings_version.load(Ordering::Acquire);
        if version != self.settings_version {
            let snapshot = if self.inline {
                // Contended (a settings write is in flight): keep the
                // previous snapshot and try again on the next call.
                self.shared.try_settings_snapshot()
            } else {
                Some(self.shared.settings_snapshot())
            };
            if let Some((settings, version)) = snapshot {
                self.settings = settings;
                self.settings_version = version;
                self.period =
                    Duration::from_secs_f64(1.0 / f64::from(self.settings.update_rate_hz.max(1)));
            }
        }
    }

    fn ingest(&mut self, samples: &[f32], sample_rate: u32) {
        self.refresh_settings();
        // Before the EQ filters these samples, so a Reset clears its state
        // at once (Plugin_FFT's Reset pulse) rather than one frame late.
        self.apply_pending_reset();
        self.pipeline.ingest(samples, sample_rate, &self.settings);
    }

    /// Consume a pending Reset (the page's button). Called from both
    /// `ingest` and `process`, which run on the same thread; the swap
    /// consumes each request exactly once, whichever comes first, and a
    /// request made between the two is taken by `process`.
    fn apply_pending_reset(&mut self) {
        if self.shared.reset_requested.swap(false, Ordering::AcqRel) {
            self.pipeline.reset_state();
        }
    }

    /// Time until the next frame is due (zero when it is).
    fn time_to_next(&self, now: Instant) -> Duration {
        match self.last_process {
            Some(last) => self.period.saturating_sub(now.duration_since(last)),
            None => Duration::ZERO,
        }
    }

    /// Run a frame when one is due. Returns whether it ran.
    fn maybe_process(&mut self, now: Instant) -> bool {
        if !self.time_to_next(now).is_zero() {
            return false;
        }
        self.process(now);
        true
    }

    fn process(&mut self, now: Instant) {
        self.refresh_settings();
        self.apply_pending_reset();
        let dt_ms = self
            .last_process
            .map(|last| now.duration_since(last).as_secs_f64() * 1000.0)
            .unwrap_or(1000.0 / f64::from(self.settings.update_rate_hz.max(1)));
        self.last_process = Some(now);

        // Hidden window: keep the FIFO warm (ingest already happened) but
        // compute and send nothing.
        if !self.shared.emit_enabled.load(Ordering::Relaxed) {
            return;
        }

        let stats = self.pipeline.process(&self.settings, dt_ms, &mut self.out);
        self.frames = self.frames.saturating_add(1);
        self.dsp_max = self.dsp_max.max(stats.dsp_us);
        self.dsp_avg = if self.frames == 1 {
            stats.dsp_us
        } else {
            self.dsp_avg + (stats.dsp_us - self.dsp_avg) * 0.05
        };

        // The axis goes out before the first frame that refers to it, so a
        // page fetching it on a version change finds it.
        let axis_version = self.pipeline.axis_version();
        if axis_version != self.axis_published {
            self.publish_axis(axis_version);
        }

        let features = stats.features.map(|f| f.to_array());
        let header = FrameHeader {
            seq: next_frame_seq(),
            flags: FRAME_FLAG_ACTIVE
                | if stats.silent { FRAME_FLAG_SILENT } else { 0 }
                | if features.is_some() {
                    FRAME_FLAG_HAS_FEATURES
                } else {
                    0
                },
            axis_version,
            peak_hz: stats.peak_hz,
            peak_value: stats.peak_value,
            dsp_us: stats.dsp_us,
        };
        encode_frame(&mut self.encoded, &header, &self.out, features.as_ref());
        // Latest wins: a frame whose slot is busy (inline only) is dropped.
        if let Some(mut slot) = self.guard(&self.shared.frame) {
            std::mem::swap(&mut *slot, &mut self.encoded);
        }

        // Telemetry into the shared status: on every rebuild, else ~1 Hz.
        let rebuilt = self.pipeline.status_version() != self.status_version_seen;
        if (rebuilt || now.duration_since(self.last_status_write) >= SUPERVISE_TICK)
            && self.write_status(rebuilt, stats.dsp_us)
        {
            self.last_status_write = now;
        }
    }

    /// Rewrite the shared axis in place (its buffer is reused). Retried on
    /// the next frame when the slot is busy.
    fn publish_axis(&mut self, version: u32) {
        let Some(mut axis) = self.guard(&self.shared.axis) else {
            return;
        };
        axis.version = version;
        axis.hz.clear();
        axis.hz
            .extend(self.pipeline.target_hz().iter().map(|hz| *hz as f32));
        drop(axis);
        self.axis_published = version;
    }

    /// Returns false when the status was busy (inline) and nothing was
    /// written; the caller retries on the next frame.
    fn write_status(&mut self, rebuilt: bool, dsp_us_last: f32) -> bool {
        let ps = self.pipeline.status(&self.settings);
        let Some(mut status) = self.guard(&self.shared.status) else {
            return false;
        };
        status.sample_rate = ps.sample_rate;
        status.fft_size = ps.fft_size;
        status.window_samples = ps.window_samples;
        status.linear_bins = ps.linear_bins;
        status.magnitude_bins = ps.magnitude_bins;
        status.output_bins = ps.output_bins;
        status.identity_warp = ps.identity_warp;
        status.full_scale_ref = ps.full_scale_ref;
        status.display_max_hz = ps.display_max_hz;
        status.nyquist_hz = ps.sample_rate as f32 / 2.0;
        status.update_rate_hz = self.settings.update_rate_hz;
        status.async_analysis = self.settings.async_analysis;
        status.frames = self.frames;
        status.dropped_samples = TAP.dropped().min(u64::from(u32::MAX)) as u32;
        status.dsp_us_last = dsp_us_last;
        status.dsp_us_avg = self.dsp_avg;
        status.dsp_us_max = self.dsp_max;
        status.axis_version = ps.axis_version;
        status.hz_per_bin = ps.hz_per_bin;
        status.aggregated_bins = ps.aggregated_bins;
        status.kaiser_beta = ps.kaiser_beta;
        status.raw_bins = ps.raw_bins;
        let half_window_ms = if ps.sample_rate > 0 {
            0.5 * ps.window_samples as f32 / ps.sample_rate as f32 * 1000.0
        } else {
            0.0
        };
        status.analysis_latency_ms = half_window_ms
            + if self.settings.async_analysis {
                self.period.as_secs_f32() * 1000.0
            } else {
                0.0
            };
        drop(status);
        if rebuilt {
            self.status_version_seen = self.pipeline.status_version();
            debug!(
                "Live FFT: transform / window / warp rebuilt — {} Hz, N={}, window {} samples, β {:.2}, {} bins ({} magnitudes computed, {} aggregated, identity={}, raw={})",
                ps.sample_rate,
                ps.fft_size,
                ps.window_samples,
                ps.kaiser_beta,
                ps.output_bins,
                ps.magnitude_bins,
                ps.aggregated_bins,
                ps.identity_warp,
                ps.raw_bins
            );
        }
        true
    }
}

impl InlineRunner for Engine {
    fn feed(&mut self, samples: &[f32], sample_rate: u32) {
        self.ingest(samples, sample_rate);
        self.maybe_process(Instant::now());
    }
}

/* ───────────────────────── the manager ───────────────────────── */

pub struct LiveFftManager {
    app: AppHandle,
    shared: Arc<Shared>,
    worker: Mutex<Option<thread::JoinHandle<()>>>,
    /// The recording overlay's miniature analyser (see `scope`).
    scope: Arc<ScopeShared>,
    scope_worker: Mutex<Option<thread::JoinHandle<()>>>,
}

impl LiveFftManager {
    pub fn new(app: AppHandle) -> Self {
        let all = get_settings(&app);
        let settings = all.live_fft.normalized();
        let overlay_scope = all.overlay_scope;
        Self {
            app,
            shared: Arc::new(Shared::new(settings)),
            worker: Mutex::new(None),
            scope: Arc::new(ScopeShared::new(overlay_scope)),
            scope_worker: Mutex::new(None),
        }
    }

    pub fn is_active(&self) -> bool {
        self.shared.active.load(Ordering::Acquire)
    }

    pub fn status(&self) -> LiveFftStatus {
        lock(&self.shared.status).clone()
    }

    /// The latest page frame, encoded (see [`encode_frame`]); only its
    /// header when `known_seq` is already the newest.
    pub fn frame(&self, known_seq: Option<u32>) -> Vec<u8> {
        self.shared.frame_reply(known_seq)
    }

    /// The Hz of every output bin of the current axis.
    pub fn axis(&self) -> LiveFftAxis {
        lock(&self.shared.axis).clone()
    }

    /// Adopt new settings; the running engine picks them up on its next
    /// tick. Only the threading mode and the voice-detection view
    /// (`show_vad`, which picks the VAD policy) need a restart of the session.
    pub fn update_settings(self: &Arc<Self>, settings: LiveFftSettings) {
        let settings = settings.normalized();
        let restart = {
            let mut current = lock(&self.shared.settings);
            // The threading mode and the VAD policy are fixed per session.
            let restart = self.is_active()
                && (current.async_analysis != settings.async_analysis
                    || current.show_vad != settings.show_vad);
            *current = settings.clone();
            restart
        };
        self.shared.settings_version.fetch_add(1, Ordering::AcqRel);
        TAP.set_source(settings.source.into());
        if restart {
            info!("Live FFT: threading mode or voice detection changed; restarting the session");
            let _ = self.stop();
            self.join_worker();
            if let Err(e) = self.start() {
                warn!("Live FFT: restart after a threading or voice-detection change failed: {e}");
            }
        }
    }

    /// The page's Reset button: clears ballistics, AGC and EQ state.
    pub fn reset(&self) {
        self.shared.reset_requested.store(true, Ordering::Release);
    }

    fn publish_status(&self, f: impl FnOnce(&mut LiveFftStatus)) {
        let snapshot = {
            let mut status = lock(&self.shared.status);
            f(&mut status);
            status.clone()
        };
        let _ = LiveFftStateEvent { status: snapshot }.emit_to(&self.app, "main");
    }

    fn join_worker(&self) {
        if let Some(handle) = lock(&self.worker).take() {
            let _ = handle.join();
        }
    }

    /* ───────────── the recording overlay's scope ───────────── */

    /// Start analysing the dictation recording the overlay is about to
    /// show (see `scope`). A no-op while the page's own session owns the
    /// tap, or while a scope already runs.
    pub fn start_overlay_scope(self: &Arc<Self>) {
        if self.is_active() {
            debug!("Overlay scope: the Live FFT page owns the analyser; not starting");
            return;
        }
        if self
            .scope
            .active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        self.join_scope_worker();
        self.scope.stop_requested.store(false, Ordering::Release);
        let settings = lock(&self.shared.settings).clone();
        TAP.arm(settings.source.into(), false, None);

        let mut engine = scope::ScopeEngine::new(Arc::clone(&self.shared), Arc::clone(&self.scope));
        let rm = self
            .app
            .state::<Arc<AudioRecordingManager>>()
            .inner()
            .clone();
        let scope = Arc::clone(&self.scope);
        let handle = thread::Builder::new()
            .name("overlay-scope".into())
            .spawn(move || {
                // Taken here, like the page worker does, so a failed spawn
                // can never drop the ring's only consumer.
                if let Some(consumer) = TAP.take_consumer() {
                    let consumer = scope::run_scope(&mut engine, consumer, &rm);
                    TAP.return_consumer(consumer);
                }
                TAP.disarm();
                scope.finish();
                debug!("Overlay scope stopped");
            });
        match handle {
            Ok(handle) => {
                *lock(&self.scope_worker) = Some(handle);
                debug!(
                    "Overlay scope started ({} Hz, {} bins, N={}, source {:?})",
                    settings.update_rate_hz,
                    settings.output_bins,
                    settings.fft_size,
                    settings.source
                );
            }
            Err(e) => {
                TAP.disarm();
                self.scope.finish();
                warn!("Failed to start the overlay scope thread: {e}");
            }
        }
    }

    /// Ask a running scope to stop; it ends within one nap.
    pub fn stop_overlay_scope(&self) {
        if self.scope.active.load(Ordering::Acquire) {
            self.scope.stop_requested.store(true, Ordering::Release);
        }
    }

    fn join_scope_worker(&self) {
        if let Some(handle) = lock(&self.scope_worker).take() {
            let _ = handle.join();
        }
    }

    /// Stop the scope and wait for it, so the tap can be re-armed.
    fn stop_overlay_scope_and_join(&self) {
        self.stop_overlay_scope();
        self.join_scope_worker();
    }

    /// The latest overlay frame, encoded (see `scope::encode_scope_frame`);
    /// only its header when `known_seq` is already the newest.
    pub fn scope_frame(&self, known_seq: Option<u32>) -> Vec<u8> {
        self.scope.frame_bytes(known_seq)
    }

    /// The overlay's picture settings changed (waveform window, fade); a
    /// running scope rebuilds its ring on the next frame.
    pub fn update_overlay_scope_settings(&self, settings: crate::settings::OverlayScopeSettings) {
        self.scope.set_overlay(settings);
    }

    pub fn start(self: &Arc<Self>) -> Result<(), String> {
        if self
            .shared
            .active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err("Live FFT is already running".to_string());
        }
        self.join_worker();
        // The page session owns the tap; an overlay scope must be gone first.
        self.stop_overlay_scope_and_join();

        let fail = |msg: String| -> Result<(), String> {
            self.shared.active.store(false, Ordering::Release);
            self.publish_status(|s| {
                s.phase = LiveFftPhase::Error;
                s.error = Some(msg.clone());
            });
            Err(msg)
        };

        let settings = lock(&self.shared.settings).clone();
        let rm = self
            .app
            .state::<Arc<AudioRecordingManager>>()
            .inner()
            .clone();
        if rm.is_recording() {
            return fail("A recording is in progress; wait for it to finish".to_string());
        }
        // The audio is never kept. The detector runs only for the page's
        // voice-detection view (`show_vad`): its per-frame verdicts then
        // stream to the page as `VadTestEvent`s, like the Advanced page's test.
        let vad_policy = if settings.show_vad {
            VadPolicy::Streaming
        } else {
            VadPolicy::Disabled
        };
        if let Err(e) = rm.try_start_recording_with_options(
            LIVE_FFT_BINDING,
            vad_policy,
            RecordingStartOptions {
                capture_raw_override: Some(false),
                discard_audio: true,
            },
        ) {
            return fail(format!("Could not open the microphone: {e}"));
        }
        set_vad_reporting(VAD_REPORT_LIVE_FFT, settings.show_vad);

        self.shared.stop_requested.store(false, Ordering::Release);
        self.shared.emit_enabled.store(true, Ordering::Release);
        self.publish_status(|s| {
            *s = LiveFftStatus {
                phase: LiveFftPhase::Starting,
                update_rate_hz: settings.update_rate_hz,
                async_analysis: settings.async_analysis,
                started_at_ms: Some(chrono::Utc::now().timestamp_millis() as f64),
                ..LiveFftStatus::default()
            };
        });

        let inline = !settings.async_analysis;
        let engine = Engine::new(Arc::clone(&self.shared), inline);
        let (worker_engine, tap_runner): (Option<Engine>, Option<Box<dyn InlineRunner>>) = if inline
        {
            (None, Some(Box::new(engine)))
        } else {
            (Some(engine), None)
        };
        TAP.arm(settings.source.into(), inline, tap_runner);

        let manager = Arc::clone(self);
        let worker_rm = Arc::clone(&rm);
        let handle = thread::Builder::new()
            .name("live-fft".into())
            .spawn(move || {
                let reason = manager.run(worker_engine, worker_rm);
                manager.finish(reason);
            })
            .map_err(|e| {
                TAP.disarm();
                set_vad_reporting(VAD_REPORT_LIVE_FFT, false);
                rm.cancel_recording_if_binding(LIVE_FFT_BINDING);
                format!("Failed to start the Live FFT thread: {e}")
            });
        match handle {
            Ok(handle) => {
                *lock(&self.worker) = Some(handle);
                info!(
                    "Live FFT started ({} analysis, {} Hz, {} bins, N={}, source {:?}, VAD {})",
                    if inline { "inline" } else { "async" },
                    settings.update_rate_hz,
                    settings.output_bins,
                    settings.fft_size,
                    settings.source,
                    if settings.show_vad { "on" } else { "off" }
                );
                Ok(())
            }
            Err(e) => fail(e),
        }
    }

    pub fn stop(&self) -> Result<(), String> {
        if !self.is_active() {
            return Err("Live FFT is not running".to_string());
        }
        self.shared.stop_requested.store(true, Ordering::Release);
        self.publish_status(|s| s.phase = LiveFftPhase::Stopping);
        Ok(())
    }

    /// Worker body. Returns why the session ended (`None` = stop requested).
    fn run(
        &self,
        mut engine: Option<Engine>,
        rm: Arc<AudioRecordingManager>,
    ) -> Option<LiveFftStopReason> {
        let mut consumer = if engine.is_some() {
            TAP.take_consumer()
        } else {
            None
        };
        let mut hidden_since: Option<Instant> = None;
        let mut last_supervise = Instant::now() - SUPERVISE_TICK;
        let mut reason = None;
        self.publish_status(|s| s.phase = LiveFftPhase::Running);

        loop {
            if self.shared.stop_requested.load(Ordering::Acquire) {
                break;
            }
            let now = Instant::now();

            // --- supervision + heartbeat, once a second ---
            if now.duration_since(last_supervise) >= SUPERVISE_TICK {
                last_supervise = now;
                if !rm.is_recording() {
                    info!("Live FFT: the recording ended elsewhere; stopping");
                    reason = Some(LiveFftStopReason::Cancelled);
                    break;
                }
                let visible = main_window_visible(&self.app);
                self.shared.emit_enabled.store(visible, Ordering::Release);
                if visible {
                    hidden_since = None;
                } else {
                    let since = *hidden_since.get_or_insert(now);
                    if now.duration_since(since) >= HIDDEN_AUTO_STOP {
                        info!("Live FFT: main window hidden for {HIDDEN_AUTO_STOP:?}; stopping");
                        reason = Some(LiveFftStopReason::WindowHidden);
                        break;
                    }
                }
                if visible {
                    self.publish_status(|_| {});
                }
            }

            match (engine.as_mut(), consumer.as_mut()) {
                (Some(engine), Some(consumer)) => {
                    // --- async: drain, analyse when due, nap until the next frame ---
                    let available = consumer.slots().min(MAX_DRAIN);
                    if available > 0
                        && let Ok(chunk) = consumer.read_chunk(available)
                    {
                        // Read after the chunk: the producer stores the rate
                        // before the ring's commit, so this is these samples'
                        // rate, never the previous session's.
                        let rate = TAP.sample_rate();
                        let (first, second) = chunk.as_slices();
                        if rate > 0 && !first.is_empty() {
                            engine.ingest(first, rate);
                        }
                        if rate > 0 && !second.is_empty() {
                            engine.ingest(second, rate);
                        }
                        chunk.commit_all();
                    }
                    if TAP.sample_rate() > 0 {
                        engine.maybe_process(now);
                    }
                    let nap = engine.time_to_next(Instant::now()).min(MAX_NAP);
                    thread::sleep(nap.max(Duration::from_millis(1)));
                }
                _ => {
                    // --- inline: the tap runs the pipeline; only supervise ---
                    thread::sleep(MAX_NAP);
                }
            }
        }

        if let Some(consumer) = consumer.take() {
            TAP.return_consumer(consumer);
        }
        reason
    }

    fn finish(&self, reason: Option<LiveFftStopReason>) {
        TAP.disarm();
        set_vad_reporting(VAD_REPORT_LIVE_FFT, false);
        let rm = self.app.state::<Arc<AudioRecordingManager>>();
        rm.cancel_recording_if_binding(LIVE_FFT_BINDING);
        // A fresh seq, no ACTIVE flag: a poll after the session sees it end.
        *lock(&self.shared.frame) = idle_frame(next_frame_seq());
        self.shared.active.store(false, Ordering::Release);
        self.publish_status(|s| {
            s.phase = LiveFftPhase::Idle;
            s.error = None;
            s.stop_reason = reason;
            s.started_at_ms = None;
        });
        info!(
            "Live FFT stopped{}",
            match reason {
                Some(LiveFftStopReason::Cancelled) => " (recording cancelled)",
                Some(LiveFftStopReason::WindowHidden) => " (window hidden)",
                None => "",
            }
        );
    }
}

fn main_window_visible(app: &AppHandle) -> bool {
    app.get_webview_window("main")
        .map(|w| w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drain(consumer: &mut Consumer<f32>) -> Vec<f32> {
        let n = consumer.slots();
        let chunk = consumer.read_chunk(n).expect("readable");
        let (a, b) = chunk.as_slices();
        let mut out = a.to_vec();
        out.extend_from_slice(b);
        chunk.commit_all();
        out
    }

    #[test]
    fn tap_ignores_pushes_until_armed_and_gates_on_source() {
        let tap = AnalysisTap::new();
        assert!(!tap.wants_native());
        assert!(!tap.wants_processed());
        tap.arm(TapSource::Native, false, None);
        assert!(tap.wants_native());
        assert!(!tap.wants_processed());
        tap.set_source(TapSource::Processed);
        assert!(!tap.wants_native());
        assert!(tap.wants_processed());
        tap.disarm();
        assert!(!tap.wants_processed());
    }

    #[test]
    fn tap_delivers_samples_in_order_and_counts_overflow() {
        let tap = AnalysisTap::new();
        tap.arm(TapSource::Native, false, None);
        tap.push(&[1.0, 2.0, 3.0], 48_000);
        tap.push(&[4.0], 48_000);
        assert_eq!(tap.sample_rate(), 48_000);
        let mut consumer = tap.take_consumer().expect("consumer parked");
        assert_eq!(drain(&mut consumer), vec![1.0, 2.0, 3.0, 4.0]);
        // Overfill: the ring keeps what fits and counts the rest as dropped.
        let big = vec![0.5f32; RING_CAPACITY + 100];
        tap.push(&big, 48_000);
        assert_eq!(tap.dropped(), 100);
        assert_eq!(drain(&mut consumer).len(), RING_CAPACITY);
        tap.return_consumer(consumer);
        // Re-arming discards anything left over from the previous session.
        tap.push(&[9.0; 10], 48_000);
        tap.arm(TapSource::Native, false, None);
        assert_eq!(tap.dropped(), 0);
        // ... and forgets its rate until the new session's first chunk.
        assert_eq!(tap.sample_rate(), 0);
        let mut consumer = tap.take_consumer().expect("consumer parked");
        assert!(drain(&mut consumer).is_empty());
        tap.push(&[1.0], 16_000);
        assert_eq!(tap.sample_rate(), 16_000);
    }

    /// A Reset clears the EQ state before the next chunk is filtered (like
    /// Plugin_FFT's Reset pulse), not one frame later: after a Reset, a
    /// window's worth of new samples gives bit for bit what a fresh pipeline
    /// gives. The request is consumed once — by `ingest` here — and a request
    /// made between `ingest` and `process` is taken by `process`.
    #[test]
    fn engine_reset_clears_the_eq_before_the_next_ingest() {
        use crate::settings::{FftLoudnessMode, FftWindowType};
        let settings = LiveFftSettings {
            eq_enabled: true,
            high_gain_db: 12.0,
            low_gain_db: -9.0,
            // Rectangular: the window's first samples (where a stale EQ
            // state would show) weigh as much as any other.
            window_type: FftWindowType::Rectangular,
            loudness_mode: FftLoudnessMode::Off,
            ballistics_enabled: false,
            fft_size: 4096,
            window_samples: 2048,
            ..LiveFftSettings::default()
        }
        .normalized();
        let rate = 48_000u32;
        let tone = |hz: f32, n: usize| -> Vec<f32> {
            (0..n)
                .map(|i| 0.5 * (2.0 * std::f32::consts::PI * hz * i as f32 / rate as f32).sin())
                .collect()
        };
        let shared = Arc::new(Shared::new(settings.clone()));
        let mut engine = Engine::new(Arc::clone(&shared), false);
        // Build up EQ state with a loud low tone.
        engine.ingest(&tone(90.0, 4800), rate);
        engine.process(Instant::now());

        shared.reset_requested.store(true, Ordering::Release);
        let fresh_block = tone(7000.0, 2048);
        engine.ingest(&fresh_block, rate);
        assert!(
            !shared.reset_requested.load(Ordering::Acquire),
            "ingest consumed the request"
        );
        engine.process(Instant::now());

        let mut reference = SpectrumPipeline::new();
        let mut want = Vec::new();
        reference.ingest(&fresh_block, rate, &settings);
        reference.process(&settings, 33.0, &mut want);
        assert_eq!(engine.out, want, "the EQ restarted from zero state");

        // Without a Reset the stale state shows: the same block after the
        // low tone differs from the fresh pipeline.
        let mut stale = Engine::new(Arc::new(Shared::new(settings.clone())), false);
        stale.ingest(&tone(90.0, 4800), rate);
        stale.process(Instant::now());
        stale.ingest(&fresh_block, rate);
        stale.process(Instant::now());
        assert_ne!(stale.out, want, "the test can see a stale EQ state");

        // A request made after ingest is applied by process, once.
        shared.reset_requested.store(true, Ordering::Release);
        engine.process(Instant::now());
        assert!(!shared.reset_requested.load(Ordering::Acquire));
    }

    #[test]
    fn tap_source_follows_the_setting() {
        assert_eq!(TapSource::from(FftSource::Microphone), TapSource::Native);
        assert_eq!(TapSource::from(FftSource::Denoised), TapSource::Denoised);
        assert_eq!(TapSource::from(FftSource::Processed), TapSource::Processed);
    }
}
