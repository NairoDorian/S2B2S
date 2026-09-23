//! The recording overlay's miniature analyser ("scope"). While a dictation
//! recording is shown, the tap and [`SpectrumPipeline`] of the Live FFT page
//! run on a worker thread with the page's settings, plus a rolling window of
//! the last `overlay_scope.wave_samples` samples of the same signal. The latest
//! frame is kept encoded as raw bytes ([`encode_scope_frame`]) and handed to
//! the overlay by the `overlay_scope_frame` command, which the overlay polls
//! at the analyser's update rate: one memcpy per poll, no JSON, no event per
//! frame (a 1024-bin spectrum plus 4096 samples is 20 KB, which serialised
//! as JSON would be ~60 KB per frame).
//!
//! The scope never owns the recording: it starts when the overlay shows a
//! recording state, waits for the microphone to open, and stops on its own
//! when the recording ends or when the overlay switches to a working state.
//! It always runs on its own thread, whatever `async_analysis` says, so a
//! dictation never carries the transform on the audio consumer thread.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use log::debug;
use rtrb::Consumer;

use super::dsp::SpectrumPipeline;
use super::{MAX_DRAIN, MAX_NAP, Shared, TAP, extend_f32_le, frame_seq, header_only};
use crate::managers::audio::AudioRecordingManager;
use crate::settings::{FftLoudnessMode, LiveFftSettings, OverlayScopeSettings};

/// Words (u32 / f32, little-endian) in the frame header.
pub const SCOPE_HEADER_WORDS: usize = 8;
/// Header flag: the analysed block was digital silence.
pub const SCOPE_FLAG_SILENT: u32 = 1;
/// Header flag: the scope is running. A frame without it is the idle
/// placeholder (header only).
pub const SCOPE_FLAG_ACTIVE: u32 = 2;
/// How long the scope waits for the recording to start: the overlay is
/// shown just before the microphone opens, and a device open can be slow.
const SCOPE_START_GRACE: Duration = Duration::from_secs(5);

/// Raised-cosine ramps of `ramp` samples at both ends of a `len`-sample
/// window, exactly 0 at the first and last sample and 1 in between.
pub fn taper_table(len: usize, ramp: usize) -> Vec<f32> {
    let ramp = ramp.min(len / 2);
    (0..len)
        .map(|i| {
            let edge = i.min(len.saturating_sub(1 + i));
            if ramp == 0 || edge >= ramp {
                1.0
            } else {
                let x = edge as f64 / ramp as f64;
                (0.5 * (1.0 - (std::f64::consts::PI * x).cos())) as f32
            }
        })
        .collect()
}

/// Rolling window of the most recent samples, read back oldest first with
/// the taper applied.
pub struct WaveRing {
    buf: Vec<f32>,
    /// Oldest sample, which is also the next write position.
    pos: usize,
    taper: Vec<f32>,
}

impl WaveRing {
    pub fn new(len: usize, ramp: usize) -> Self {
        Self {
            buf: vec![0.0; len.max(1)],
            pos: 0,
            taper: taper_table(len.max(1), ramp),
        }
    }

    pub fn push(&mut self, samples: &[f32]) {
        let len = self.buf.len();
        // Only the last `len` samples of a chunk can still be in the window.
        let samples = if samples.len() > len {
            &samples[samples.len() - len..]
        } else {
            samples
        };
        let first = (len - self.pos).min(samples.len());
        self.buf[self.pos..self.pos + first].copy_from_slice(&samples[..first]);
        let rest = samples.len() - first;
        if rest > 0 {
            self.buf[..rest].copy_from_slice(&samples[first..]);
        }
        self.pos = (self.pos + samples.len()) % len;
    }

    /// The window in time order, tapered, into `out`.
    pub fn snapshot(&self, out: &mut Vec<f32>) {
        let len = self.buf.len();
        out.clear();
        out.reserve(len);
        out.extend(
            self.buf[self.pos..]
                .iter()
                .chain(self.buf[..self.pos].iter())
                .zip(self.taper.iter())
                .map(|(s, t)| s * t),
        );
    }

    pub fn clear(&mut self) {
        self.buf.fill(0.0);
        self.pos = 0;
    }
}

/// What the overlay needs to draw a frame besides the two arrays.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScopeHeader {
    pub seq: u32,
    pub flags: u32,
    pub loudness_mode: FftLoudnessMode,
    /// Floor of the dB display, in dB below the reference.
    pub db_range: f32,
    pub sample_rate: u32,
    /// Magnitude of a full-scale sine under the current normalisation.
    pub full_scale_ref: f32,
}

impl Default for ScopeHeader {
    fn default() -> Self {
        Self {
            seq: 0,
            flags: 0,
            loudness_mode: FftLoudnessMode::Db,
            db_range: 90.0,
            sample_rate: 0,
            full_scale_ref: 1.0,
        }
    }
}

fn loudness_code(mode: FftLoudnessMode) -> u32 {
    match mode {
        FftLoudnessMode::Off => 0,
        FftLoudnessMode::Db => 1,
        FftLoudnessMode::DbNormalized => 2,
    }
}

/// Encode a frame as the overlay reads it: `SCOPE_HEADER_WORDS` little-endian
/// words — seq, flags, bins length, wave length, loudness mode (0 linear,
/// 1 dB, 2 dB normalised), dB range (f32), sample rate, full-scale reference
/// (f32) — followed by the bins and the waveform as f32s. The overlay maps
/// `Float32Array` views straight onto the buffer.
pub fn encode_scope_frame(out: &mut Vec<u8>, header: &ScopeHeader, bins: &[f32], wave: &[f32]) {
    out.clear();
    out.reserve((SCOPE_HEADER_WORDS + bins.len() + wave.len()) * 4);
    for word in [
        header.seq,
        header.flags,
        bins.len() as u32,
        wave.len() as u32,
        loudness_code(header.loudness_mode),
    ] {
        out.extend_from_slice(&word.to_le_bytes());
    }
    out.extend_from_slice(&header.db_range.to_le_bytes());
    out.extend_from_slice(&header.sample_rate.to_le_bytes());
    out.extend_from_slice(&header.full_scale_ref.to_le_bytes());
    extend_f32_le(out, bins);
    extend_f32_le(out, wave);
}

/// The frame the command returns while no scope runs.
pub fn idle_frame() -> Vec<u8> {
    let mut out = Vec::new();
    encode_scope_frame(&mut out, &ScopeHeader::default(), &[], &[]);
    out
}

/// State shared between the manager, the worker and the polling command.
pub struct ScopeShared {
    pub active: AtomicBool,
    pub stop_requested: AtomicBool,
    /// The overlay's picture settings (`overlay_scope`): the waveform window
    /// and its fade; bumped `overlay_version` tells the worker to rebuild.
    overlay: Mutex<OverlayScopeSettings>,
    overlay_version: AtomicU64,
    /// The latest encoded frame; swapped in whole by the worker, cloned by
    /// the command. Held for a memcpy either way.
    frame: Mutex<Vec<u8>>,
    /// Frame numbers continue across scope sessions (never 0), so an
    /// overlay poll carrying an old seq never matches a new frame.
    next_seq: AtomicU32,
}

impl ScopeShared {
    pub fn new(overlay: OverlayScopeSettings) -> Self {
        Self {
            active: AtomicBool::new(false),
            stop_requested: AtomicBool::new(false),
            overlay: Mutex::new(overlay.normalized()),
            overlay_version: AtomicU64::new(1),
            frame: Mutex::new(idle_frame()),
            next_seq: AtomicU32::new(1),
        }
    }

    fn next_seq(&self) -> u32 {
        loop {
            let v = self.next_seq.fetch_add(1, Ordering::Relaxed);
            if v != 0 {
                return v;
            }
        }
    }

    pub fn set_overlay(&self, overlay: OverlayScopeSettings) {
        *self.overlay.lock().unwrap() = overlay.normalized();
        self.overlay_version.fetch_add(1, Ordering::AcqRel);
    }

    fn overlay_snapshot(&self) -> (OverlayScopeSettings, u64) {
        let version = self.overlay_version.load(Ordering::Acquire);
        (self.overlay.lock().unwrap().clone(), version)
    }

    /// The latest frame, or only its header (bins and wave lengths 0) when
    /// the caller already holds `known_seq`.
    pub fn frame_bytes(&self, known_seq: Option<u32>) -> Vec<u8> {
        let frame = self.frame.lock().unwrap();
        if known_seq.is_some_and(|seq| seq == frame_seq(&frame)) {
            header_only(&frame, &[2, 3])
        } else {
            frame.clone()
        }
    }

    fn publish(&self, encoded: &mut Vec<u8>) {
        let mut frame = self.frame.lock().unwrap();
        std::mem::swap(&mut *frame, encoded);
    }

    /// Back to the idle placeholder; called by the worker as it ends.
    pub fn finish(&self) {
        *self.frame.lock().unwrap() = idle_frame();
        self.active.store(false, Ordering::Release);
    }
}

/// Pipeline, waveform ring and settings snapshot; owned by the worker.
pub(super) struct ScopeEngine {
    shared: Arc<Shared>,
    scope: Arc<ScopeShared>,
    pipeline: SpectrumPipeline,
    settings: LiveFftSettings,
    settings_version: u64,
    overlay_version: u64,
    period: Duration,
    bins: Vec<f32>,
    wave: WaveRing,
    wave_out: Vec<f32>,
    encoded: Vec<u8>,
    last_process: Option<Instant>,
}

/// Most bins the overlay scope computes and ships per frame. The page may
/// ask for up to 65536 (Auto or Raw at N = 65536, or a large Fixed count),
/// but the overlay draws a few hundred columns during every dictation, so
/// anything above this is capped (see `SpectrumPipeline::with_output_bin_cap`)
/// to keep a changed poll near the ~32 KB it cost before the ceiling was raised.
const OVERLAY_MAX_BINS: usize = 8192;

/// The page's settings as the scope runs them: the overlay never shows the
/// spectral features, so they are not computed for it.
fn scope_settings(mut settings: LiveFftSettings) -> LiveFftSettings {
    settings.spectral_features = false;
    settings
}

impl ScopeEngine {
    pub(super) fn new(shared: Arc<Shared>, scope: Arc<ScopeShared>) -> Self {
        let (settings, settings_version) = shared.settings_snapshot();
        let settings = scope_settings(settings);
        let (overlay, overlay_version) = scope.overlay_snapshot();
        let period = Duration::from_secs_f64(1.0 / f64::from(settings.update_rate_hz.max(1)));
        Self {
            shared,
            scope,
            pipeline: SpectrumPipeline::new().with_output_bin_cap(OVERLAY_MAX_BINS),
            settings,
            settings_version,
            overlay_version,
            period,
            bins: Vec::new(),
            wave: WaveRing::new(
                overlay.wave_samples as usize,
                overlay.wave_taper_samples as usize,
            ),
            wave_out: Vec::with_capacity(overlay.wave_samples as usize),
            encoded: Vec::new(),
            last_process: None,
        }
    }

    /// The page's settings are shared; pick up a change on the next frame.
    /// The overlay's waveform window is rebuilt when its setting changes.
    fn refresh_settings(&mut self) {
        let version = self.shared.settings_version.load(Ordering::Acquire);
        if version != self.settings_version {
            let (settings, version) = self.shared.settings_snapshot();
            self.settings = scope_settings(settings);
            self.settings_version = version;
            self.period =
                Duration::from_secs_f64(1.0 / f64::from(self.settings.update_rate_hz.max(1)));
        }
        let overlay_version = self.scope.overlay_version.load(Ordering::Acquire);
        if overlay_version != self.overlay_version {
            let (overlay, version) = self.scope.overlay_snapshot();
            self.overlay_version = version;
            self.wave = WaveRing::new(
                overlay.wave_samples as usize,
                overlay.wave_taper_samples as usize,
            );
        }
    }

    pub fn ingest(&mut self, samples: &[f32], sample_rate: u32) {
        self.refresh_settings();
        self.pipeline.ingest(samples, sample_rate, &self.settings);
        self.wave.push(samples);
    }

    /// Time until the next frame is due (zero when it is).
    pub fn time_to_next(&self, now: Instant) -> Duration {
        match self.last_process {
            Some(last) => self.period.saturating_sub(now.duration_since(last)),
            None => Duration::ZERO,
        }
    }

    /// Run a frame when one is due.
    pub fn maybe_process(&mut self, now: Instant) {
        if !self.time_to_next(now).is_zero() {
            return;
        }
        self.refresh_settings();
        let dt_ms = self
            .last_process
            .map(|last| now.duration_since(last).as_secs_f64() * 1000.0)
            .unwrap_or(1000.0 / f64::from(self.settings.update_rate_hz.max(1)));
        self.last_process = Some(now);

        let stats = self.pipeline.process(&self.settings, dt_ms, &mut self.bins);
        self.wave.snapshot(&mut self.wave_out);
        let status = self.pipeline.status(&self.settings);
        let header = ScopeHeader {
            seq: self.scope.next_seq(),
            flags: SCOPE_FLAG_ACTIVE | if stats.silent { SCOPE_FLAG_SILENT } else { 0 },
            loudness_mode: self.settings.loudness_mode,
            db_range: self.settings.db_range,
            sample_rate: status.sample_rate,
            full_scale_ref: status.full_scale_ref,
        };
        encode_scope_frame(&mut self.encoded, &header, &self.bins, &self.wave_out);
        self.scope.publish(&mut self.encoded);
    }
}

/// Worker body: drain the tap, analyse when due, nap until the next frame.
/// Ends on a stop request, when the recording it was shown for ends, or
/// when no recording starts within [`SCOPE_START_GRACE`]. Returns the ring
/// consumer so the caller can park it back in the tap.
pub(super) fn run_scope(
    engine: &mut ScopeEngine,
    mut consumer: Consumer<f32>,
    rm: &AudioRecordingManager,
) -> Consumer<f32> {
    let started = Instant::now();
    let mut seen_recording = false;
    loop {
        if engine.scope.stop_requested.load(Ordering::Acquire) {
            break;
        }
        if rm.is_recording() {
            seen_recording = true;
        } else if seen_recording {
            debug!("Overlay scope: the recording ended; stopping");
            break;
        } else if started.elapsed() >= SCOPE_START_GRACE {
            debug!("Overlay scope: no recording started within {SCOPE_START_GRACE:?}; stopping");
            break;
        }

        let now = Instant::now();
        let rate = TAP.sample_rate();
        let available = consumer.slots().min(MAX_DRAIN);
        if available > 0
            && let Ok(chunk) = consumer.read_chunk(available)
        {
            let (first, second) = chunk.as_slices();
            if !first.is_empty() {
                engine.ingest(first, rate);
            }
            if !second.is_empty() {
                engine.ingest(second, rate);
            }
            chunk.commit_all();
        }
        if rate > 0 && seen_recording {
            engine.maybe_process(now);
        }
        let nap = engine.time_to_next(Instant::now()).min(MAX_NAP);
        thread::sleep(nap.max(Duration::from_millis(1)));
    }
    engine.wave.clear();
    consumer
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taper_is_zero_at_the_ends_one_in_the_middle_and_symmetric() {
        let t = taper_table(4096, 512);
        assert_eq!(t.len(), 4096);
        assert_eq!(t[0], 0.0);
        assert_eq!(t[4095], 0.0);
        assert!((t[512] - 1.0).abs() < 1e-6);
        assert_eq!(t[2048], 1.0);
        for i in 0..512 {
            assert!((t[i] - t[4095 - i]).abs() < 1e-6, "asymmetric at {i}");
            assert!(t[i] <= t[i + 1] + 1e-6, "not monotonic at {i}");
        }
    }

    #[test]
    fn taper_without_ramp_is_flat() {
        assert!(taper_table(16, 0).iter().all(|&v| v == 1.0));
    }

    #[test]
    fn wave_ring_keeps_the_last_samples_in_order() {
        let mut ring = WaveRing::new(8, 0);
        ring.push(&[1.0, 2.0, 3.0]);
        ring.push(&[4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0]);
        let mut out = Vec::new();
        ring.snapshot(&mut out);
        assert_eq!(out, vec![3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0]);
        // A chunk longer than the window leaves only its tail.
        let big: Vec<f32> = (0..20).map(|i| i as f32).collect();
        ring.push(&big);
        ring.snapshot(&mut out);
        assert_eq!(out, (12..20).map(|i| i as f32).collect::<Vec<_>>());
    }

    #[test]
    fn wave_ring_snapshot_applies_the_taper() {
        let mut ring = WaveRing::new(8, 2);
        ring.push(&[1.0; 8]);
        let mut out = Vec::new();
        ring.snapshot(&mut out);
        assert_eq!(out[0], 0.0);
        assert_eq!(out[7], 0.0);
        assert!((out[1] - 0.5).abs() < 1e-6);
        assert_eq!(out[3], 1.0);
    }

    #[test]
    fn scope_frame_round_trips() {
        let header = ScopeHeader {
            seq: 7,
            flags: SCOPE_FLAG_ACTIVE | SCOPE_FLAG_SILENT,
            loudness_mode: FftLoudnessMode::DbNormalized,
            db_range: 80.0,
            sample_rate: 48_000,
            full_scale_ref: 1587.5,
        };
        let bins = [0.25f32, 0.5, 0.75];
        let wave = [-1.0f32, 0.0, 1.0, 0.5];
        let mut out = Vec::new();
        encode_scope_frame(&mut out, &header, &bins, &wave);
        assert_eq!(
            out.len(),
            (SCOPE_HEADER_WORDS + bins.len() + wave.len()) * 4
        );
        let word = |i: usize| u32::from_le_bytes(out[i * 4..i * 4 + 4].try_into().unwrap());
        let float = |i: usize| f32::from_le_bytes(out[i * 4..i * 4 + 4].try_into().unwrap());
        assert_eq!(word(0), 7);
        assert_eq!(word(1), SCOPE_FLAG_ACTIVE | SCOPE_FLAG_SILENT);
        assert_eq!(word(2), 3);
        assert_eq!(word(3), 4);
        assert_eq!(word(4), 2);
        assert_eq!(float(5), 80.0);
        assert_eq!(word(6), 48_000);
        assert_eq!(float(7), 1587.5);
        let values: Vec<f32> = (SCOPE_HEADER_WORDS..SCOPE_HEADER_WORDS + 7)
            .map(float)
            .collect();
        assert_eq!(values, vec![0.25, 0.5, 0.75, -1.0, 0.0, 1.0, 0.5]);
    }

    #[test]
    fn engine_publishes_a_frame_with_the_page_settings_and_a_tapered_wave() {
        let settings = LiveFftSettings::default();
        let shared = Arc::new(Shared::new(settings.clone()));
        let scope = Arc::new(ScopeShared::new(OverlayScopeSettings::default()));
        let mut engine = ScopeEngine::new(Arc::clone(&shared), Arc::clone(&scope));
        // 200 ms of a 1 kHz sine at 48 kHz in 10 ms chunks (10 whole cycles
        // each, so the repeats join without a discontinuity).
        let rate = 48_000u32;
        let chunk: Vec<f32> = (0..480)
            .map(|i| (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / rate as f32).sin() * 0.5)
            .collect();
        for _ in 0..20 {
            engine.ingest(&chunk, rate);
        }
        engine.maybe_process(Instant::now());

        let bytes = scope.frame_bytes(None);
        let word = |i: usize| u32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap());
        let float = |i: usize| f32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap());
        assert_eq!(word(1) & SCOPE_FLAG_ACTIVE, SCOPE_FLAG_ACTIVE);
        // A poll that already holds this frame gets the bare header.
        let again = scope.frame_bytes(Some(word(0)));
        assert_eq!(again.len(), SCOPE_HEADER_WORDS * 4);
        assert_eq!(again[..8], bytes[..8], "same seq and flags");
        assert_eq!(again[8..16], [0u8; 8], "bins and wave lengths zeroed");
        assert_eq!(again[16..], bytes[16..SCOPE_HEADER_WORDS * 4]);
        assert_eq!(scope.frame_bytes(Some(word(0).wrapping_add(1))), bytes);
        assert_eq!(word(2), settings.output_bins);
        assert_eq!(word(3), OverlayScopeSettings::default().wave_samples);
        assert_eq!(word(4), 1, "the default loudness mode is dB");
        assert_eq!(float(5), settings.db_range);
        assert_eq!(word(6), rate);

        let bins_start = SCOPE_HEADER_WORDS;
        let peak_db = (0..settings.output_bins as usize)
            .map(|i| float(bins_start + i))
            .fold(f32::MIN, f32::max);
        assert!(
            peak_db > -20.0,
            "a half-scale tone should peak well above the floor, got {peak_db} dB"
        );

        let wave_samples = OverlayScopeSettings::default().wave_samples as usize;
        let wave_start = bins_start + settings.output_bins as usize;
        assert_eq!(float(wave_start), 0.0, "tapered start");
        assert_eq!(float(wave_start + wave_samples - 1), 0.0, "tapered end");
        let middle_peak = (1024..3072)
            .map(|i| float(wave_start + i).abs())
            .fold(0.0f32, f32::max);
        assert!(
            middle_peak > 0.45,
            "the untapered middle carries the tone, got {middle_peak}"
        );
    }

    /// The overlay must show exactly what the Live FFT page would show for
    /// the same settings: same pipeline, same snapshot, same frame timing.
    /// Feeds one signal to a page pipeline and to a scope engine and compares
    /// the bins bit for bit, with the EQ shelves, an A-weighting, a Kaiser
    /// window with auto β, a Mel scale with cubic interpolation and RMS
    /// aggregation, an Auto output size over an unpadded transform and
    /// millisecond ballistics all switched on.
    #[test]
    fn scope_bins_equal_the_page_pipeline_for_the_same_settings() {
        use crate::settings::{
            FftBallisticsMode, FftKaiserBetaMode, FftOutputBinsMode, FftScale, FftWarpAggregation,
            FftWarpInterp, FftWeighting, FftWindowType, LiveFftSettings,
        };
        let settings = LiveFftSettings {
            eq_enabled: true,
            high_gain_db: 12.0,
            low_gain_db: -6.0,
            weighting: FftWeighting::A,
            window_type: FftWindowType::Kaiser,
            kaiser_beta_mode: FftKaiserBetaMode::Auto,
            scale: FftScale::Mel,
            warp_interpolation: FftWarpInterp::Cubic,
            warp_aggregation: FftWarpAggregation::Rms,
            output_bins_mode: FftOutputBinsMode::Auto,
            zero_padding: false,
            spectral_features: true,
            ballistics_enabled: true,
            ballistics_mode: FftBallisticsMode::Milliseconds,
            ..LiveFftSettings::default()
        }
        .normalized();
        let shared = Arc::new(Shared::new(settings.clone()));
        let scope = Arc::new(ScopeShared::new(OverlayScopeSettings::default()));
        let mut engine = ScopeEngine::new(shared, Arc::clone(&scope));
        let mut page = SpectrumPipeline::new();

        // 250 ms of two tones plus a deterministic noise floor, in 10 ms chunks.
        let rate = 48_000u32;
        let mut x = 0.375f32;
        let signal: Vec<f32> = (0..12_000)
            .map(|i| {
                x = (x * 9_301.0 + 49_297.0) % 233_280.0;
                let t = i as f32 / rate as f32;
                0.4 * (2.0 * std::f32::consts::PI * 440.0 * t).sin()
                    + 0.2 * (2.0 * std::f32::consts::PI * 6_000.0 * t).sin()
                    + 0.02 * (x / 233_280.0 - 0.5)
            })
            .collect();
        for chunk in signal.chunks(480) {
            page.ingest(chunk, rate, &settings);
            engine.ingest(chunk, rate);
        }
        // The first frame of both uses the nominal period as its `dt`.
        let mut page_bins = Vec::new();
        page.process(
            &settings,
            1000.0 / f64::from(settings.update_rate_hz),
            &mut page_bins,
        );
        engine.maybe_process(Instant::now());
        // 3175-sample window, no padding: N = 3176, Auto = 1589 bins.
        assert_eq!(page_bins.len(), 1589);

        let bytes = scope.frame_bytes(None);
        let word = |i: usize| u32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap());
        let float = |i: usize| f32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap());
        assert_eq!(word(2) as usize, page_bins.len());
        assert_eq!(word(4), 1, "dB mode travels in the header");
        assert_eq!(float(5), settings.db_range);
        let scope_bins: Vec<f32> = (0..page_bins.len())
            .map(|i| float(SCOPE_HEADER_WORDS + i))
            .collect();
        assert_eq!(scope_bins, page_bins, "overlay and page spectra differ");
        // Sanity: the settings actually shaped the frame (a real spectrum, not
        // a flat floor), so the comparison above is not vacuous.
        let (lo, hi) = page_bins
            .iter()
            .fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
        assert!(
            hi - lo > 20.0,
            "expected a spectrum with contrast, got {lo}..{hi} dB"
        );
    }

    #[test]
    fn idle_frame_is_a_bare_inactive_header() {
        let out = idle_frame();
        assert_eq!(out.len(), SCOPE_HEADER_WORDS * 4);
        let flags = u32::from_le_bytes(out[4..8].try_into().unwrap());
        assert_eq!(flags & SCOPE_FLAG_ACTIVE, 0);
    }
}
