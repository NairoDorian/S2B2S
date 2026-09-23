use std::{
    io::Error,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

use cpal::{
    Device, Sample, SizedSample,
    traits::{DeviceTrait, StreamTrait},
};
use rtrb::{Consumer, Producer, RingBuffer};

use crate::audio_toolkit::{
    VoiceActivityDetector,
    audio::{
        ChunkTap, DenoiseChain, DenoiseControls, DenoiseParams, FrameResampler, RNNOISE_SAMPLE_RATE,
    },
    constants,
    vad::{self, VadFrame},
};

/// The audio result returned when a recording session completes.
#[derive(Clone, Debug, PartialEq)]
pub struct RecordedAudio {
    /// 16 kHz mono VAD-filtered audio samples for transcription models.
    pub stt_samples: Vec<f32>,
    /// Native uncompressed raw audio samples before resampling and VAD filtering.
    pub raw_samples: Vec<f32>,
    /// Native input sample rate of the microphone hardware.
    pub native_sample_rate: u32,
    /// Native sample format of the microphone hardware.
    pub native_sample_format: cpal::SampleFormat,
}

impl RecordedAudio {
    pub fn len(&self) -> usize {
        self.stt_samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.stt_samples.is_empty()
    }
}

impl std::ops::Deref for RecordedAudio {
    type Target = [f32];
    fn deref(&self) -> &Self::Target {
        &self.stt_samples
    }
}

pub(crate) enum Cmd {
    Start {
        vad_policy: VadPolicy,
        pause_hold_ms: u32,
        capture_raw: bool,
        /// Keep nothing: the session only feeds live consumers (VAD test,
        /// Live FFT), so the 16 kHz audio is not accumulated either.
        discard_audio: bool,
        sent_at: Instant,
        ready_tx: mpsc::Sender<()>,
    },
    Stop(mpsc::Sender<RecordedAudio>),
    Shutdown,
}

#[cfg(test)]
impl Cmd {
    pub(crate) fn start(
        vad_policy: VadPolicy,
        sent_at: Instant,
        ready_tx: mpsc::Sender<()>,
    ) -> Self {
        Cmd::Start {
            vad_policy,
            pause_hold_ms: DEFAULT_SPEECH_PAUSE_HOLD_MS,
            capture_raw: false,
            discard_audio: false,
            sent_at,
            ready_tx,
        }
    }
}

pub const DEFAULT_SPEECH_PAUSE_HOLD_MS: u32 = 500;
const SPEECH_HEARTBEAT_MS: u64 = 150;

// Two seconds of ring capacity absorbs consumer stalls without adding latency
// during normal 10 ms drains.
const AUDIO_RING_SECONDS: usize = 2;
const CONSUMER_POLL_INTERVAL: Duration = Duration::from_millis(10);
const MAX_DRAIN_CHUNK: Duration = Duration::from_millis(50);
const PAUSE_ACK_TIMEOUT: Duration = Duration::from_secs(2);

/// Atomics shared by the callback and consumer; audio uses a wait-free SPSC ring.
/// The callback must remain allocation-, lock-, logging-, and blocking-free.
#[derive(Default)]
struct CaptureTransportState {
    pause_requested: AtomicBool,
    /// Set after forwarding a pause's boundary block; subsequent callbacks
    /// remain silent until the consumer clears the request.
    pause_acknowledged: AtomicBool,
    overrun_samples: AtomicU64,
}

/// How 16 kHz mono frames should be filtered for one recording session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VadPolicy {
    /// Bypass VAD and forward every frame.
    Disabled,
    /// Current offline-tuned VAD profile.
    Offline,
    /// VAD profile with a longer post-speech tail for streaming-capable models.
    Streaming,
}

/// A single VAD engine plus the hangover-tail lengths and onset frames its smoothing wrapper
/// should use. The offline and streaming policies are never active
/// concurrently, so one detector is reconfigured per session (see `Cmd::Start`)
/// rather than kept as two resident engines.
#[derive(Clone)]
pub(crate) struct VadConfig {
    pub(crate) detector: Arc<Mutex<Box<dyn vad::VoiceActivityDetector>>>,
    pub(crate) frame_samples: usize,
    pub(crate) offline_hangover_frames: usize,
    pub(crate) streaming_hangover_frames: usize,
    pub(crate) onset_frames: usize,
}

impl VadConfig {
    /// Post-speech hangover tail (in backend-sized frames) for the given policy.
    /// `Disabled` never reaches the detector, so it maps to the offline value.
    fn hangover_for(&self, policy: VadPolicy) -> usize {
        match policy {
            VadPolicy::Streaming => self.streaming_hangover_frames,
            VadPolicy::Offline | VadPolicy::Disabled => self.offline_hangover_frames,
        }
    }
}

/// Callback invoked with each 16 kHz mono frame that passes the active capture
/// policy while recording. Used to feed a live streaming transcription as audio arrives.
pub type AudioFrameCallback = Arc<dyn Fn(&[f32]) + Send + Sync + 'static>;

/// A snapshot of how much of this recording has actually been speech.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpeechActivity {
    /// Whether the user is currently considered to be speaking. Debounced by
    /// the session's pause tolerance, so it does not flicker on the gaps
    /// between words.
    pub speaking: bool,
    /// Milliseconds of speech in this recording, excluding confirmed pauses.
    pub speech_ms: u64,
}

/// Callback invoked when the speech clock has news: the speaking/silent state
/// flipped, or the heartbeat interval elapsed while speech is ongoing. Runs on
/// the recorder's consumer thread — keep it cheap.
pub type SpeechActivityCallback = Arc<dyn Fn(SpeechActivity) + Send + Sync + 'static>;

/// One VAD decision, as seen by the live VAD test in Settings → Advanced.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VadFrameReport {
    /// Raw 0–1 speech score of the frame, before hysteresis. `None` when the
    /// detector has no score (VAD disabled, or a detector without one).
    pub score: Option<f32>,
    /// The detector's own verdict after hysteresis — what the speech clock
    /// counts.
    pub voiced: bool,
    /// Whether audio reached the recording on this frame, i.e. the verdict
    /// after prefill / onset / hangover smoothing — what a decoder would hear.
    pub kept: bool,
    /// Peak absolute sample of the 16 kHz frame (0–1), a cheap input level.
    pub level: f32,
    /// RNNoise's own speech probability of the latest 10 ms frame, when the
    /// suppressor ran on this audio (the gate threshold compares against it).
    pub denoise_prob: Option<f32>,
}

/// Receives a [`VadFrameReport`] for every 16 kHz frame while a recording is
/// active. Only used by the live VAD test; the callback gates itself.
pub type VadFrameCallback = Arc<dyn Fn(VadFrameReport) + Send + Sync + 'static>;

/// A consumer of the microphone signal that is not the recording itself
/// (the Live FFT page). Three tap points: the native-rate mono chunk, before
/// resampling and noise suppression; the 48 kHz frames as they leave RNNoise
/// (the native chunk again while suppression is off, so toggling it is a
/// direct A/B); and the 16 kHz frames after both and before the VAD. The
/// `wants_*` checks run per chunk / per frame on the audio consumer thread, so
/// implementations gate with atomics, and `push` must never block on that
/// thread; it must not allocate either, except in the opt-in inline analysis
/// mode (Live FFT's `async_analysis` off), which runs the transform and emits
/// its frame event there on purpose.
pub trait AnalysisSink: Send + Sync {
    fn wants_native(&self) -> bool;
    fn wants_denoised(&self) -> bool;
    fn wants_processed(&self) -> bool;
    fn push(&self, samples: &[f32], sample_rate: u32);
}

pub type AnalysisSinkRef = Arc<dyn AnalysisSink>;

/// Tracks how long the user has actually been speaking, in frame-sized steps.
///
/// Driven by [`VoiceActivityDetector::last_frame_voiced`] — the *raw* per-frame
/// verdict — rather than by what `push_frame` returns. The smoothing wrapper
/// keeps reporting speech through a hangover tail of ~1.66 s in streaming mode
/// (`VAD_STREAMING_HANGOVER_MS` rounded up to whole frames), and counting that
/// would inflate the measured speech time by more than a second per pause,
/// which in turn deflates the words-per-minute figure computed from it.
///
/// Gaps shorter than `hold_ms` are treated as part of the same utterance and
/// counted retroactively once speech resumes; gaps that reach `hold_ms` end the
/// utterance and are never counted. The clock therefore only ever moves
/// forward — resuming after a short gap makes it catch up by at most `hold_ms`,
/// which is far less jarring than a timer that visibly rewinds.
#[derive(Debug)]
pub struct SpeechClock {
    /// Milliseconds per audio frame (16 ms for Earshot; `VAD_FRAME_MS`).
    frame_ms: u64,
    /// Consecutive voiced frames needed to enter speech, matching the VAD's own
    /// onset debounce so a single noisy frame cannot start the clock.
    onset_frames: usize,
    /// How long silence must persist before the utterance is considered over.
    hold_ms: u64,

    speaking: bool,
    onset_counter: usize,
    /// Silence accumulated since the last voiced frame, still provisional: it
    /// becomes speech if the user resumes, or is discarded if it reaches
    /// `hold_ms`.
    pending_silence_ms: u64,
    speech_ms: u64,
    /// Lock-free mirror of `speech_ms`, readable from outside the consumer
    /// thread.
    published: Arc<AtomicU64>,
    ms_since_emit: u64,
    /// `speech_ms` as of the last emission, so a heartbeat is skipped when the
    /// clock has not actually moved.
    last_emitted_ms: u64,
}

impl SpeechClock {
    pub fn new(
        onset_frames: usize,
        hold_ms: u64,
        frame_ms: u64,
        published: Arc<AtomicU64>,
    ) -> Self {
        published.store(0, Ordering::Relaxed);
        Self {
            frame_ms: frame_ms.max(1),
            onset_frames: onset_frames.max(1),
            hold_ms,
            published,
            speaking: false,
            onset_counter: 0,
            pending_silence_ms: 0,
            speech_ms: 0,
            ms_since_emit: 0,
            last_emitted_ms: 0,
        }
    }

    #[cfg(test)]
    pub fn frame_ms(&self) -> u64 {
        self.frame_ms
    }

    /// Clear all state for a new recording, adopting a new pause tolerance.
    pub fn reset(&mut self, hold_ms: u64) {
        self.published.store(0, Ordering::Relaxed);
        self.hold_ms = hold_ms;
        self.speaking = false;
        self.onset_counter = 0;
        self.pending_silence_ms = 0;
        self.speech_ms = 0;
        self.ms_since_emit = 0;
        self.last_emitted_ms = 0;
    }

    pub fn snapshot(&self) -> SpeechActivity {
        SpeechActivity {
            speaking: self.speaking,
            speech_ms: self.speech_ms,
        }
    }

    /// Advance the clock by one frame. Returns a snapshot only when the UI has
    /// something new to show, so a silent stretch costs nothing.
    pub fn tick(&mut self, voiced: bool) -> Option<SpeechActivity> {
        let was_speaking = self.speaking;
        self.advance(voiced);
        self.published.store(self.speech_ms, Ordering::Relaxed);
        self.ms_since_emit += self.frame_ms;

        // Report a flip immediately; otherwise only once the timer has both
        // moved and gone stale.
        let due = self.speaking != was_speaking
            || (self.speech_ms != self.last_emitted_ms
                && self.ms_since_emit >= SPEECH_HEARTBEAT_MS);
        if due {
            self.ms_since_emit = 0;
            self.last_emitted_ms = self.speech_ms;
            Some(self.snapshot())
        } else {
            None
        }
    }

    fn advance(&mut self, voiced: bool) {
        if voiced {
            if self.speaking {
                // Crossing a gap shorter than the tolerance: bill this frame
                // plus the gap we just bridged.
                self.speech_ms += self.frame_ms + self.pending_silence_ms;
                self.pending_silence_ms = 0;
            } else {
                self.onset_counter += 1;
                if self.onset_counter >= self.onset_frames {
                    self.speaking = true;
                    // Count every frame that established the onset, not just
                    // the one that crossed the threshold.
                    self.speech_ms += self.frame_ms * self.onset_counter as u64;
                    self.onset_counter = 0;
                    self.pending_silence_ms = 0;
                }
            }
        } else {
            self.onset_counter = 0;
            if self.speaking {
                self.pending_silence_ms += self.frame_ms;
                if self.pending_silence_ms >= self.hold_ms {
                    self.speaking = false;
                    self.pending_silence_ms = 0;
                }
            }
        }
    }
}

pub struct AudioRecorder {
    device: Option<Device>,
    cmd_tx: Option<mpsc::Sender<Cmd>>,
    worker_handle: Option<std::thread::JoinHandle<()>>,
    vad: Option<VadConfig>,
    audio_cb: Option<AudioFrameCallback>,
    speech_cb: Option<SpeechActivityCallback>,
    vad_frame_cb: Option<VadFrameCallback>,
    /// Live FFT tap; gates itself per chunk with atomics.
    analysis: Option<AnalysisSinkRef>,
    /// Mid-recording audio tap for the experimental Multi-STT streaming mode.
    /// Gates itself per frame with one relaxed atomic load ([`ChunkTap`]).
    chunk_tap: Option<Arc<ChunkTap>>,
    /// RNNoise suppression on/off, read by the consumer thread per chunk so a
    /// toggle applies mid-recording without reopening anything.
    denoise_enabled: Arc<AtomicBool>,
    /// RNNoise's tunables (strength, gate), read per chunk the same way.
    denoise_controls: Arc<DenoiseControls>,
    /// Milliseconds of speech in the most recent recording, published by the
    /// consumer thread's [`SpeechClock`]. Final once `stop()` has returned.
    speech_ms: Arc<AtomicU64>,
    /// Which input channel to use. None = average all (original behavior).
    selected_channel: Option<usize>,
    /// Preferred stream config cached per device name.
    config_cache: Arc<Mutex<Option<(String, cpal::SupportedStreamConfig)>>>,
    /// Set by cpal when the active input stream can no longer capture.
    stream_error: Arc<AtomicBool>,
}

impl AudioRecorder {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(AudioRecorder {
            device: None,
            cmd_tx: None,
            worker_handle: None,
            vad: None,
            audio_cb: None,
            speech_cb: None,
            vad_frame_cb: None,
            analysis: None,
            chunk_tap: None,
            denoise_enabled: Arc::new(AtomicBool::new(false)),
            denoise_controls: Arc::new(DenoiseControls::new(DenoiseParams::default())),
            speech_ms: Arc::new(AtomicU64::new(0)),
            selected_channel: None,
            config_cache: Arc::new(Mutex::new(None)),
            stream_error: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Attach the Live FFT tap. It sees the native-rate chunk (before
    /// resampling and noise suppression), the 48 kHz frames after RNNoise, or
    /// the 16 kHz frames the model hears, whichever it asks for, and only while
    /// a recording is active.
    pub fn with_analysis_sink(mut self, sink: AnalysisSinkRef) -> Self {
        self.analysis = Some(sink);
        self
    }

    /// Attach the mid-recording audio tap used by the experimental Multi-STT
    /// streaming mode. It receives the same VAD-kept 16 kHz frames the decoder
    /// and the batch buffer get, and only while a session holds it.
    pub fn with_chunk_tap(mut self, tap: Arc<ChunkTap>) -> Self {
        self.chunk_tap = Some(tap);
        self
    }

    /// Attach a single VAD engine, reconfigured per session for the offline vs
    /// streaming hangover tail.
    pub fn with_vad(
        mut self,
        detector: Box<dyn VoiceActivityDetector>,
        offline_hangover_frames: usize,
        streaming_hangover_frames: usize,
        onset_frames: usize,
    ) -> Self {
        let frame_samples = detector.frame_samples();
        assert!(frame_samples > 0, "VAD frame size must be non-zero");
        self.vad = Some(VadConfig {
            detector: Arc::new(Mutex::new(detector)),
            frame_samples,
            offline_hangover_frames,
            streaming_hangover_frames,
            onset_frames,
        });
        self
    }

    /// Register a callback that receives real-time 16 kHz frames after the active
    /// VAD policy has been applied.
    pub fn with_audio_callback<F>(mut self, cb: F) -> Self
    where
        F: Fn(&[f32]) + Send + Sync + 'static,
    {
        self.audio_cb = Some(Arc::new(cb));
        self
    }

    pub fn with_speech_activity_callback<F>(mut self, cb: F) -> Self
    where
        F: Fn(SpeechActivity) + Send + Sync + 'static,
    {
        self.speech_cb = Some(Arc::new(cb));
        self
    }

    /// Register a callback that sees every VAD decision (raw score, verdict,
    /// kept-or-dropped, level). Drives the live VAD test; it must be cheap and
    /// gate itself, since it runs on the audio consumer thread per 16 ms frame.
    pub fn with_vad_frame_callback<F>(mut self, cb: F) -> Self
    where
        F: Fn(VadFrameReport) + Send + Sync + 'static,
    {
        self.vad_frame_cb = Some(Arc::new(cb));
        self
    }

    /// Initial RNNoise suppression state (see [`Self::set_denoise_enabled`]).
    pub fn with_denoise_enabled(self, enabled: bool) -> Self {
        self.denoise_enabled.store(enabled, Ordering::Relaxed);
        self
    }

    /// Turn RNNoise suppression on or off. Applies from the next drained
    /// chunk, also mid-recording; the capture path swaps between the direct
    /// and the denoised resampling chain and resets the one it leaves.
    pub fn set_denoise_enabled(&self, enabled: bool) {
        self.denoise_enabled.store(enabled, Ordering::Relaxed);
    }

    /// Initial RNNoise tunables (see [`Self::set_denoise_params`]).
    pub fn with_denoise_params(self, params: DenoiseParams) -> Self {
        self.denoise_controls.set(params);
        self
    }

    /// Change RNNoise's strength / gate. Applies from the next drained chunk,
    /// mid-recording included.
    pub fn set_denoise_params(&self, params: DenoiseParams) {
        self.denoise_controls.set(params);
    }

    /// Change the detector's speech threshold in place. Takes effect on the
    /// next frame, including mid-recording, and does not touch the stream.
    pub fn set_vad_threshold(&self, threshold: f32) {
        if let Some(cfg) = &self.vad {
            cfg.detector.lock().unwrap().set_threshold(threshold);
        }
    }

    pub fn with_selected_channel(mut self, channel: Option<u16>) -> Self {
        self.set_selected_channel(channel);
        self
    }

    pub fn set_selected_channel(&mut self, channel: Option<u16>) {
        self.selected_channel = channel.map(usize::from);
    }

    pub fn speech_ms(&self) -> u64 {
        self.speech_ms.load(Ordering::Relaxed)
    }

    pub fn open(&mut self, device: Option<Device>) -> Result<(), Box<dyn std::error::Error>> {
        if self.worker_handle.is_some() {
            if !self.needs_reopen() {
                return Ok(()); // already open
            }
            log::warn!("Capture stream failed; rebuilding microphone stream");
            self.close()?;
        }

        self.stream_error.store(false, Ordering::Relaxed);

        let (cmd_tx, cmd_rx) = mpsc::channel::<Cmd>();
        let (init_tx, init_rx) = mpsc::sync_channel::<Result<(), String>>(1);

        let device = match device {
            Some(dev) => dev,
            // The concrete endpoint, never cpal's virtual default handle (see
            // `default_input_endpoint`).
            None => super::device::default_input_endpoint()
                .map(|endpoint| endpoint.device)
                .ok_or_else(|| Error::new(std::io::ErrorKind::NotFound, "No input device found"))?,
        };

        let thread_device = device.clone();
        let vad = self.vad.clone();
        let audio_cb = self.audio_cb.clone();
        let speech_cb = self.speech_cb.clone();
        let vad_frame_cb = self.vad_frame_cb.clone();
        let analysis = self.analysis.clone();
        let chunk_tap = self.chunk_tap.clone();
        let denoise_enabled = Arc::clone(&self.denoise_enabled);
        let denoise_controls = Arc::clone(&self.denoise_controls);
        let speech_ms = Arc::clone(&self.speech_ms);
        let selected_channel = self.selected_channel;
        let config_cache = Arc::clone(&self.config_cache);
        let stream_error = Arc::clone(&self.stream_error);

        let worker = std::thread::spawn(move || {
            let transport = Arc::new(CaptureTransportState::default());
            let init_result =
                (|| -> Result<(cpal::Stream, u32, cpal::SampleFormat, Consumer<f32>), String> {
                    let config_started = Instant::now();
                    let device_name = thread_device
                        .description()
                        .map(|d| d.name().to_string())
                        .unwrap_or_default();
                    let cached_config = config_cache
                        .lock()
                        .unwrap()
                        .as_ref()
                        .filter(|(name, _)| !device_name.is_empty() && *name == device_name)
                        .map(|(_, cfg)| *cfg);
                    let config_was_cached = cached_config.is_some();
                    let config = match cached_config {
                        Some(cfg) => cfg,
                        None => AudioRecorder::get_preferred_config(&thread_device)
                            .map_err(|e| format!("Failed to fetch preferred config: {e}"))?,
                    };
                    let config_elapsed = config_started.elapsed();

                    let sample_rate = config.sample_rate();
                    let sample_format = config.sample_format();
                    let channels = config.channels() as usize;
                    // The persisted channel outlives a microphone change; an
                    // index past this device's channels would panic in the
                    // callback, so it averages instead (the log below says so).
                    let usable_channel = usable_input_channel(selected_channel, channels);

                    log::info!(
                        "Using device: {:?} (sample rate {}, channels {}, format {:?})",
                        device_name,
                        sample_rate,
                        channels,
                        config.sample_format()
                    );

                    if let Some(channel) = selected_channel {
                        if channel < channels {
                            log::info!("Using selected input channel: {}", channel + 1);
                        } else {
                            log::warn!(
                                "Selected input channel {} is out of range for a {}-channel device; averaging all channels instead",
                                channel + 1,
                                channels
                            );
                        }
                    } else {
                        log::info!("Averaging all {} input channels", channels);
                    }

                    let build_started = Instant::now();
                    let (stream, sample_consumer) = match config.sample_format() {
                        cpal::SampleFormat::U8 => AudioRecorder::build_stream::<u8>(
                            &thread_device,
                            &config,
                            channels,
                            usable_channel,
                            Arc::clone(&transport),
                            Arc::clone(&stream_error),
                        ),
                        cpal::SampleFormat::I8 => AudioRecorder::build_stream::<i8>(
                            &thread_device,
                            &config,
                            channels,
                            usable_channel,
                            Arc::clone(&transport),
                            Arc::clone(&stream_error),
                        ),
                        cpal::SampleFormat::I16 => AudioRecorder::build_stream::<i16>(
                            &thread_device,
                            &config,
                            channels,
                            usable_channel,
                            Arc::clone(&transport),
                            Arc::clone(&stream_error),
                        ),
                        cpal::SampleFormat::I32 => AudioRecorder::build_stream::<i32>(
                            &thread_device,
                            &config,
                            channels,
                            usable_channel,
                            Arc::clone(&transport),
                            Arc::clone(&stream_error),
                        ),
                        cpal::SampleFormat::F32 => AudioRecorder::build_stream::<f32>(
                            &thread_device,
                            &config,
                            channels,
                            usable_channel,
                            Arc::clone(&transport),
                            Arc::clone(&stream_error),
                        ),
                        sample_format => {
                            return Err(format!("Unsupported sample format: {sample_format:?}"));
                        }
                    }
                    .map_err(|e| format!("Failed to build input stream: {e}"))?;
                    let build_elapsed = build_started.elapsed();

                    let play_started = Instant::now();
                    stream
                        .play()
                        .map_err(|e| format!("Failed to start microphone stream: {e}"))?;
                    log::debug!(
                        "mic worker init: fetch_config={:?} (cached={}) build_stream={:?} play={:?}",
                        config_elapsed,
                        config_was_cached,
                        build_elapsed,
                        play_started.elapsed()
                    );

                    // The device accepted this config; remember it so the next
                    // open skips the HAL property queries entirely.
                    if !config_was_cached && !device_name.is_empty() {
                        *config_cache.lock().unwrap() = Some((device_name, config));
                    }

                    Ok((stream, sample_rate, sample_format, sample_consumer))
                })();

            match init_result {
                Ok((stream, sample_rate, sample_format, sample_consumer)) => {
                    let _ = init_tx.send(Ok(()));
                    let stream_running_at = Instant::now();
                    let processor = CaptureProcessor::with_options(
                        sample_rate,
                        sample_format,
                        vad,
                        audio_cb,
                        speech_cb,
                        vad_frame_cb,
                        analysis,
                        chunk_tap,
                        denoise_enabled,
                        denoise_controls,
                        speech_ms,
                        stream_running_at,
                    );
                    run_consumer(
                        processor,
                        sample_consumer,
                        cmd_rx,
                        transport,
                        Arc::clone(&stream_error),
                    );
                    drop(stream);
                }
                Err(error_message) => {
                    *config_cache.lock().unwrap() = None;
                    log::error!("{error_message}");
                    let _ = init_tx.send(Err(error_message));
                }
            }
        });

        match init_rx.recv() {
            Ok(Ok(())) => {
                self.device = Some(device);
                self.cmd_tx = Some(cmd_tx);
                self.worker_handle = Some(worker);
                Ok(())
            }
            Ok(Err(error_message)) => {
                let _ = worker.join();
                let kind = if is_microphone_access_denied(&error_message) {
                    std::io::ErrorKind::PermissionDenied
                } else {
                    std::io::ErrorKind::Other
                };
                Err(Box::new(Error::new(kind, error_message)))
            }
            Err(recv_error) => {
                let _ = worker.join();
                Err(Box::new(Error::other(format!(
                    "Failed to initialize microphone worker: {recv_error}"
                ))))
            }
        }
    }

    /// Queue a recording start and return a receiver that resolves after the
    /// first real microphone sample chunk has entered the capture path.
    pub fn start(
        &self,
        vad_policy: VadPolicy,
        pause_hold_ms: u32,
        capture_raw: bool,
    ) -> Result<mpsc::Receiver<()>, Box<dyn std::error::Error>> {
        self.start_with_options(vad_policy, pause_hold_ms, capture_raw, false)
    }

    /// Like [`Self::start`]; with `discard_audio` the session keeps no
    /// samples at all (live consumers only), so it can run for hours
    /// without growing.
    pub fn start_with_options(
        &self,
        vad_policy: VadPolicy,
        pause_hold_ms: u32,
        capture_raw: bool,
        discard_audio: bool,
    ) -> Result<mpsc::Receiver<()>, Box<dyn std::error::Error>> {
        let tx = self
            .cmd_tx
            .as_ref()
            .ok_or_else(|| Error::other("Recorder is not open"))?;
        let (ready_tx, ready_rx) = mpsc::channel();
        tx.send(Cmd::Start {
            vad_policy,
            pause_hold_ms,
            capture_raw: capture_raw && !discard_audio,
            discard_audio,
            sent_at: Instant::now(),
            ready_tx,
        })?;
        Ok(ready_rx)
    }

    pub fn stop(&self) -> Result<RecordedAudio, Box<dyn std::error::Error>> {
        let tx = self
            .cmd_tx
            .as_ref()
            .ok_or_else(|| Error::other("Recorder is not open"))?;
        let (resp_tx, resp_rx) = mpsc::channel();
        tx.send(Cmd::Stop(resp_tx))?;
        Ok(resp_rx.recv()?)
    }

    /// True when the active capture stream must be rebuilt.
    pub fn needs_reopen(&self) -> bool {
        self.stream_error.load(Ordering::Relaxed)
            || self
                .worker_handle
                .as_ref()
                .is_some_and(|handle| handle.is_finished())
    }

    pub fn close(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(tx) = self.cmd_tx.take() {
            let _ = tx.send(Cmd::Shutdown);
        }
        if let Some(handle) = self.worker_handle.take() {
            let _ = handle.join();
        }
        self.device = None;
        Ok(())
    }

    fn build_stream<T>(
        device: &cpal::Device,
        config: &cpal::SupportedStreamConfig,
        channels: usize,
        selected_channel: Option<usize>,
        transport: Arc<CaptureTransportState>,
        stream_error: Arc<AtomicBool>,
    ) -> Result<(cpal::Stream, Consumer<f32>), cpal::Error>
    where
        T: Sample + SizedSample + Copy + Send + 'static,
        f32: cpal::FromSample<T>,
    {
        let ring_capacity = config.sample_rate() as usize * AUDIO_RING_SECONDS;
        let (mut sample_producer, mut sample_consumer) = RingBuffer::new(ring_capacity);

        // Touch rtrb's uninitialized pages before the stream starts to reduce
        // callback page faults. This does not pin them.
        {
            let chunk = sample_producer
                .write_chunk(ring_capacity)
                .expect("new audio ring has its full capacity available");
            chunk.commit_all();
        }
        {
            let chunk = sample_consumer
                .read_chunk(ring_capacity)
                .expect("new audio ring has its full capacity available");
            chunk.commit_all();
        }

        let stream = device.build_input_stream(
            (*config).into(),
            move |data: &[T], _: &_| {
                AudioRecorder::write_input_to_ring(
                    data,
                    channels,
                    selected_channel,
                    &mut sample_producer,
                    &transport,
                );
            },
            move |_err| {
                // Error callbacks may share the platform audio thread. Defer
                // logging and recovery to the consumer/manager path.
                stream_error.store(true, Ordering::Release);
            },
            None,
        )?;
        Ok((stream, sample_consumer))
    }

    /// Real-time callback body. Keep this allocation-free, wait-free, and free
    /// of locks, logging, clocks, and system calls.
    fn write_input_to_ring<T>(
        data: &[T],
        channels: usize,
        use_channel: Option<usize>,
        producer: &mut Producer<f32>,
        transport: &CaptureTransportState,
    ) where
        T: Sample + SizedSample + Copy,
        f32: cpal::FromSample<T>,
    {
        // Forward the first block that observes a pause; once acknowledged,
        // remain silent until the consumer resumes capture.
        if transport.pause_requested.load(Ordering::Acquire)
            && transport.pause_acknowledged.load(Ordering::Acquire)
        {
            return;
        }

        let frame_count = data.len() / channels;
        let writable_frames = producer.slots().min(frame_count);
        let written = if writable_frames == 0 {
            0
        } else {
            let chunk = producer
                .write_chunk_uninit(writable_frames)
                .expect("the producer just reported this many writable slots");
            if channels == 1 {
                chunk.fill_from_iter(
                    data.iter()
                        .take(writable_frames)
                        .map(|&sample| sample.to_sample::<f32>()),
                )
            } else if let Some(channel) = use_channel {
                chunk.fill_from_iter(
                    data.chunks_exact(channels)
                        .take(writable_frames)
                        .map(|frame| frame[channel].to_sample::<f32>()),
                )
            } else {
                chunk.fill_from_iter(data.chunks_exact(channels).take(writable_frames).map(
                    |frame| {
                        frame
                            .iter()
                            .map(|&sample| sample.to_sample::<f32>())
                            .sum::<f32>()
                            / channels as f32
                    },
                ))
            }
        };
        debug_assert_eq!(written, writable_frames);

        let dropped = frame_count - written;
        if dropped > 0 {
            transport
                .overrun_samples
                .fetch_add(dropped as u64, Ordering::Relaxed);
        }

        // Publish the boundary write before acknowledging, including when the
        // pause request arrives during the write.
        acknowledge_pause_after_write(transport);
    }

    pub fn preferred_input_channel_count(
        device: &cpal::Device,
    ) -> Result<u16, Box<dyn std::error::Error>> {
        Ok(Self::get_preferred_config(device)?.channels())
    }

    fn get_preferred_config(
        device: &cpal::Device,
    ) -> Result<cpal::SupportedStreamConfig, Box<dyn std::error::Error>> {
        let default_config = device.default_input_config()?;
        let target_rate = default_config.sample_rate();

        let supported_configs = match device.supported_input_configs() {
            Ok(configs) => configs,
            Err(e) => {
                log::warn!("Could not enumerate input configs ({e}), using device default");
                return Ok(default_config);
            }
        };
        let mut best_config: Option<cpal::SupportedStreamConfigRange> = None;

        for config_range in supported_configs {
            if config_range.min_sample_rate() <= target_rate
                && config_range.max_sample_rate() >= target_rate
            {
                match best_config {
                    None => best_config = Some(config_range),
                    Some(ref current) => {
                        let score = |fmt: cpal::SampleFormat| match fmt {
                            cpal::SampleFormat::F32 => 4,
                            cpal::SampleFormat::I16 => 3,
                            cpal::SampleFormat::I32 => 2,
                            _ => 1,
                        };

                        if score(config_range.sample_format()) > score(current.sample_format()) {
                            best_config = Some(config_range);
                        }
                    }
                }
            }
        }

        if let Some(config) = best_config {
            return Ok(config.with_sample_rate(target_rate));
        }

        log::warn!(
            "No supported config matched device default rate {:?}, using default config",
            target_rate
        );
        Ok(default_config)
    }
}

/// The channel the callback may index: `selected` when the device has it,
/// otherwise `None` (average all channels).
fn usable_input_channel(selected: Option<usize>, channels: usize) -> Option<usize> {
    selected.filter(|&channel| channel < channels)
}

fn acknowledge_pause_after_write(transport: &CaptureTransportState) {
    if transport.pause_requested.load(Ordering::Acquire) {
        transport.pause_acknowledged.store(true, Ordering::Release);
    }
}

pub fn is_microphone_access_denied(error_message: &str) -> bool {
    let normalized = error_message.to_lowercase();
    normalized.contains("access is denied")
        || normalized.contains("permission denied")
        || normalized.contains("0x80070005")
}

pub fn is_no_input_device_error(error_message: &str) -> bool {
    let normalized = error_message.to_lowercase();
    normalized.contains("no input device found")
        || (normalized.contains("failed to fetch preferred config")
            && normalized.contains("coreaudio"))
}

/// Route one 16 kHz frame through VAD to recording and live outputs, and drive the speech clock.
#[allow(clippy::too_many_arguments)]
fn handle_frame(
    samples: &[f32],
    vad_policy: VadPolicy,
    vad: &Option<VadConfig>,
    audio_cb: &Option<AudioFrameCallback>,
    speech_clock: &mut SpeechClock,
    speech_cb: &Option<SpeechActivityCallback>,
    vad_frame_cb: &Option<VadFrameCallback>,
    vad_errors: &mut u64,
    keep_audio: bool,
    out_buf: &mut Vec<f32>,
    chunk_tap: Option<&ChunkTap>,
    denoise_prob: Option<f32>,
) {
    let mut kept = false;
    let mut emit = |buf: &[f32]| {
        kept = true;
        if keep_audio {
            out_buf.extend_from_slice(buf);
            // Mid-recording tap, on the *same* frames and in the same order as
            // the batch buffer above and the stream feed below — that identical
            // timeline makes a chunk's audio the speech the stream decoded.
            if let Some(tap) = chunk_tap
                && tap.is_active()
            {
                tap.push(buf);
            }
        }
        if let Some(cb) = audio_cb {
            cb(buf);
        }
    };
    let mut score = None;

    let voiced = if vad_policy == VadPolicy::Disabled {
        emit(samples);
        true
    } else if let Some(cfg) = vad {
        let mut det = cfg.detector.lock().unwrap();
        let decision = match det.push_frame(samples) {
            Ok(frame) => frame,
            Err(e) => {
                if *vad_errors == 0 {
                    log::error!(
                        "VAD failed on a frame; passing failed frames through unfiltered (logged once per recording): {e}"
                    );
                }
                *vad_errors += 1;
                VadFrame::Speech(samples)
            }
        };
        match decision {
            VadFrame::Speech(buf) => emit(buf),
            VadFrame::Noise => {}
        }
        score = det.last_frame_score();
        det.last_frame_voiced()
    } else {
        emit(samples);
        true
    };

    if let Some(activity) = speech_clock.tick(voiced)
        && let Some(cb) = speech_cb
    {
        cb(activity);
    }

    if let Some(cb) = vad_frame_cb {
        let level = samples.iter().fold(0.0f32, |peak, s| peak.max(s.abs()));
        cb(VadFrameReport {
            score,
            voiced,
            kept,
            level: level.min(1.0),
            denoise_prob,
        });
    }
}

fn drain_available_samples(
    consumer: &mut Consumer<f32>,
    max_samples: usize,
    mut process: impl FnMut(&[f32]),
) -> usize {
    let available = consumer.slots().min(max_samples);
    if available == 0 {
        return 0;
    }

    let chunk = consumer
        .read_chunk(available)
        .expect("reported audio ring slots must be readable");
    let (first, second) = chunk.as_slices();
    if !first.is_empty() {
        process(first);
    }
    if !second.is_empty() {
        process(second);
    }
    chunk.commit_all();
    available
}

/// What to do with a chunk drained from the ring.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ChunkDisposition {
    /// Process as active recording audio, including during the final stop drain.
    Capture,
    /// Consume idle audio without processing it.
    Discard,
}

/// Converts raw ring samples into 16 kHz frames across recording sessions.
pub(crate) struct CaptureProcessor {
    in_sample_rate: u32,
    in_sample_format: cpal::SampleFormat,
    vad: Option<VadConfig>,
    audio_cb: Option<AudioFrameCallback>,
    speech_cb: Option<SpeechActivityCallback>,
    vad_frame_cb: Option<VadFrameCallback>,
    analysis: Option<AnalysisSinkRef>,
    /// Mid-recording copy of the VAD-filtered 16 kHz frames, drained by the
    /// experimental Multi-STT streaming coordinator. See `ChunkTap`.
    chunk_tap: Option<Arc<ChunkTap>>,
    speech_clock: SpeechClock,
    stream_running_at: Instant,
    /// Direct path: native rate → 16 kHz VAD frames.
    frame_resampler: FrameResampler,
    /// Denoised path: native rate → 48 kHz → RNNoise → 16 kHz VAD frames.
    /// Built on first use so a user who never enables suppression pays
    /// nothing for it.
    denoise: Option<DenoiseChain>,
    denoise_enabled: Arc<AtomicBool>,
    denoise_controls: Arc<DenoiseControls>,
    /// Which path carried the previous chunk; a change resets both so no
    /// buffered tail from the other path leaks out later.
    denoise_active: bool,
    out_frame_duration: Duration,
    max_drain_samples: usize,
    first_chunk_logged: bool,
    vad_policy: VadPolicy,
    capture_raw: bool,
    /// Keep no audio at all (VAD test, Live FFT): the frames still reach
    /// every live consumer, they are just not accumulated.
    discard_audio: bool,
    raw_captured_samples: Vec<f32>,
    processed_samples: Vec<f32>,
    vad_errors: u64,
    awaiting_first_captured_chunk: Option<Instant>,
    capture_ready_tx: Option<mpsc::Sender<()>>,
    total_dropped_samples: u64,
    overrun_warning_logged: bool,
    pipeline_elapsed: std::time::Duration,
    frame_elapsed: std::time::Duration,
    chunk_max_elapsed: std::time::Duration,
    timed_frames: u64,
}

impl CaptureProcessor {
    #[cfg(test)]
    pub(crate) fn new(
        in_sample_rate: u32,
        vad: Option<VadConfig>,
        audio_cb: Option<AudioFrameCallback>,
        stream_running_at: Instant,
    ) -> Self {
        Self::with_options(
            in_sample_rate,
            cpal::SampleFormat::F32,
            vad,
            audio_cb,
            None,
            None,
            None,
            // Tests exercise the VAD and denoise chains, not the mid-recording
            // tap the Multi-STT streaming coordinator owns.
            None,
            Arc::new(AtomicBool::new(false)),
            Arc::new(DenoiseControls::new(DenoiseParams::default())),
            Arc::new(AtomicU64::new(0)),
            stream_running_at,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn with_options(
        in_sample_rate: u32,
        in_sample_format: cpal::SampleFormat,
        vad: Option<VadConfig>,
        audio_cb: Option<AudioFrameCallback>,
        speech_cb: Option<SpeechActivityCallback>,
        vad_frame_cb: Option<VadFrameCallback>,
        analysis: Option<AnalysisSinkRef>,
        chunk_tap: Option<Arc<ChunkTap>>,
        denoise_enabled: Arc<AtomicBool>,
        denoise_controls: Arc<DenoiseControls>,
        speech_clock_total: Arc<AtomicU64>,
        stream_running_at: Instant,
    ) -> Self {
        let frame_samples = vad.as_ref().map_or(
            (constants::WHISPER_SAMPLE_RATE * 30 / 1000) as usize,
            |cfg| cfg.frame_samples,
        );
        let frame_duration =
            Duration::from_secs_f64(frame_samples as f64 / constants::WHISPER_SAMPLE_RATE as f64);
        let frame_ms = (frame_samples as u64 * 1000) / constants::WHISPER_SAMPLE_RATE as u64;
        let onset_frames = vad.as_ref().map(|v| v.onset_frames).unwrap_or(1);
        let speech_clock = SpeechClock::new(
            onset_frames,
            u64::from(DEFAULT_SPEECH_PAUSE_HOLD_MS),
            frame_ms,
            speech_clock_total,
        );

        let frame_resampler = FrameResampler::new(
            in_sample_rate as usize,
            constants::WHISPER_SAMPLE_RATE as usize,
            frame_duration,
        );

        let max_drain_samples =
            ((in_sample_rate as u128 * MAX_DRAIN_CHUNK.as_millis()) / 1_000).max(1) as usize;

        Self {
            in_sample_rate,
            in_sample_format,
            vad,
            audio_cb,
            speech_cb,
            vad_frame_cb,
            analysis,
            chunk_tap,
            speech_clock,
            stream_running_at,
            frame_resampler,
            denoise: None,
            denoise_active: denoise_enabled.load(Ordering::Relaxed),
            denoise_enabled,
            denoise_controls,
            out_frame_duration: frame_duration,
            max_drain_samples,
            first_chunk_logged: false,
            vad_policy: VadPolicy::Offline,
            capture_raw: false,
            discard_audio: false,
            raw_captured_samples: Vec::new(),
            processed_samples: Vec::new(),
            vad_errors: 0,
            awaiting_first_captured_chunk: None,
            capture_ready_tx: None,
            total_dropped_samples: 0,
            overrun_warning_logged: false,
            pipeline_elapsed: std::time::Duration::ZERO,
            frame_elapsed: std::time::Duration::ZERO,
            chunk_max_elapsed: std::time::Duration::ZERO,
            timed_frames: 0,
        }
    }

    #[cfg(test)]
    pub(crate) fn begin_recording(&mut self, policy: VadPolicy, ready_tx: mpsc::Sender<()>) {
        self.begin_recording_full(policy, DEFAULT_SPEECH_PAUSE_HOLD_MS, false, false, ready_tx);
    }

    /// Reset per-recording state and arm the first-sample acknowledgement.
    pub(crate) fn begin_recording_full(
        &mut self,
        policy: VadPolicy,
        pause_hold_ms: u32,
        capture_raw: bool,
        discard_audio: bool,
        ready_tx: mpsc::Sender<()>,
    ) {
        self.awaiting_first_captured_chunk = Some(Instant::now());
        self.capture_ready_tx = Some(ready_tx);
        self.total_dropped_samples = 0;
        self.overrun_warning_logged = false;
        self.vad_policy = policy;
        self.capture_raw = capture_raw && !discard_audio;
        self.discard_audio = discard_audio;
        self.raw_captured_samples.clear();
        self.processed_samples.clear();
        self.vad_errors = 0;
        self.pipeline_elapsed = std::time::Duration::ZERO;
        self.frame_elapsed = std::time::Duration::ZERO;
        self.chunk_max_elapsed = std::time::Duration::ZERO;
        self.timed_frames = 0;
        self.frame_resampler.reset();
        if let Some(chain) = &mut self.denoise {
            chain.reset();
        }
        self.denoise_active = self.denoise_enabled.load(Ordering::Relaxed);
        self.speech_clock.reset(u64::from(pause_hold_ms));
        if policy != VadPolicy::Disabled
            && let Some(cfg) = &self.vad
        {
            let mut detector = cfg.detector.lock().unwrap();
            detector.set_hangover_frames(cfg.hangover_for(policy));
            detector.reset();
        }
    }

    /// Drop a pending first-sample acknowledgement.
    fn cancel_ready_signal(&mut self) {
        self.capture_ready_tx = None;
        self.awaiting_first_captured_chunk = None;
    }

    /// Drain up to one bounded chunk from the ring.
    fn drain(&mut self, consumer: &mut Consumer<f32>, disposition: ChunkDisposition) -> usize {
        let max_samples = self.max_drain_samples;
        drain_available_samples(consumer, max_samples, |raw| {
            self.process_raw_chunk(raw, disposition)
        })
    }

    fn process_raw_chunk(&mut self, raw: &[f32], disposition: ChunkDisposition) {
        let chunk_ms = raw.len() as f64 * 1000.0 / self.in_sample_rate as f64;
        if !self.first_chunk_logged {
            self.first_chunk_logged = true;
            log::debug!(
                "first audio samples arrived {:?} after stream start ({:.1}ms drained)",
                self.stream_running_at.elapsed(),
                chunk_ms
            );
        }

        if disposition == ChunkDisposition::Discard {
            return;
        }

        let pipeline_start = Instant::now();
        if self.capture_raw {
            self.raw_captured_samples.extend_from_slice(raw);
        }

        // Live FFT native tap: the untouched microphone chunk, like the raw
        // tap above. One atomic load per chunk while neither the page nor
        // the overlay scope is listening.
        if let Some(sink) = &self.analysis
            && sink.wants_native()
        {
            sink.push(raw, self.in_sample_rate);
        }

        // Noise suppression runs here, after the raw tap and the native
        // analysis tap (both deliberately see the untouched microphone) and
        // before the VAD, so the detector, the speech clock and the live VAD
        // test all work on the denoised signal.
        let denoise_on = self.denoise_enabled.load(Ordering::Relaxed);
        if denoise_on != self.denoise_active {
            self.frame_resampler.reset();
            if let Some(chain) = &mut self.denoise {
                chain.reset();
            }
            self.denoise_active = denoise_on;
            log::debug!(
                "Noise suppression {} mid-recording",
                if denoise_on { "enabled" } else { "disabled" }
            );
        }
        if denoise_on && self.denoise.is_none() {
            self.denoise = Some(DenoiseChain::new(
                self.in_sample_rate as usize,
                constants::WHISPER_SAMPLE_RATE as usize,
                self.out_frame_duration,
            ));
        }

        let vad_policy = self.vad_policy;
        let vad = &self.vad;
        let audio_cb = &self.audio_cb;
        let speech_cb = &self.speech_cb;
        let vad_frame_cb = &self.vad_frame_cb;
        let speech_clock = &mut self.speech_clock;
        let vad_errors = &mut self.vad_errors;
        let processed_samples = &mut self.processed_samples;
        let analysis = &self.analysis;
        let chunk_tap = self.chunk_tap.as_deref();
        let keep_audio = !self.discard_audio;

        let frame_elapsed = &mut self.frame_elapsed;
        let timed_frames = &mut self.timed_frames;
        let mut on_frame = |frame: &[f32], denoise_prob: Option<f32>| {
            let frame_start = Instant::now();
            // Live FFT processed tap: the 16 kHz frames a model hears, after
            // resampling / noise suppression and before the VAD.
            if let Some(sink) = analysis
                && sink.wants_processed()
            {
                sink.push(frame, constants::WHISPER_SAMPLE_RATE);
            }
            handle_frame(
                frame,
                vad_policy,
                vad,
                audio_cb,
                speech_clock,
                speech_cb,
                vad_frame_cb,
                vad_errors,
                keep_audio,
                processed_samples,
                chunk_tap,
                denoise_prob,
            );
            *frame_elapsed += frame_start.elapsed();
            *timed_frames += 1;
        };
        match (denoise_on, self.denoise.as_mut()) {
            (true, Some(chain)) => {
                let params = self.denoise_controls.get();
                chain.push(
                    raw,
                    params,
                    |denoised| {
                        // Live FFT "after noise suppression" tap: the 48 kHz
                        // frames as they leave RNNoise.
                        if let Some(sink) = analysis
                            && sink.wants_denoised()
                        {
                            sink.push(denoised, RNNOISE_SAMPLE_RATE as u32);
                        }
                    },
                    |frame, prob| on_frame(frame, Some(prob)),
                );
            }
            _ => {
                // Suppression off: that tap shows the untouched microphone,
                // so toggling it on the Live FFT page is a direct A/B.
                if let Some(sink) = analysis
                    && sink.wants_denoised()
                {
                    sink.push(raw, self.in_sample_rate);
                }
                self.frame_resampler
                    .push(raw, |frame| on_frame(frame, None));
            }
        }

        let elapsed = pipeline_start.elapsed();
        self.pipeline_elapsed += elapsed;
        self.chunk_max_elapsed = self.chunk_max_elapsed.max(elapsed);
        if let Some(started) = self.awaiting_first_captured_chunk.take() {
            log::debug!(
                "first captured samples ({:.1}ms) processed {:?} after Cmd::Start",
                chunk_ms,
                started.elapsed()
            );
        }
        if let Some(ready_tx) = self.capture_ready_tx.take() {
            let _ = ready_tx.send(());
        }
    }

    /// Account for samples the callback could not fit into the ring during
    /// the active recording. Warns once per recording.
    fn observe_overrun(&mut self, samples: u64) {
        if samples == 0 {
            return;
        }

        self.total_dropped_samples = self.total_dropped_samples.saturating_add(samples);
        if !self.overrun_warning_logged {
            self.overrun_warning_logged = true;
            log::warn!(
                "Microphone capture ring dropped {samples} samples; continuing the active recording"
            );
        }
    }

    /// Flush the resampler tail and hand back the finished recording.
    fn finish_recording(&mut self) -> RecordedAudio {
        let finish_start = Instant::now();
        let vad_policy = self.vad_policy;
        let vad = &self.vad;
        let audio_cb = &self.audio_cb;
        let speech_cb = &self.speech_cb;
        let vad_frame_cb = &self.vad_frame_cb;
        let speech_clock = &mut self.speech_clock;
        let vad_errors = &mut self.vad_errors;
        let processed_samples = &mut self.processed_samples;
        let chunk_tap = self.chunk_tap.as_deref();
        let keep_audio = !self.discard_audio;

        let frame_elapsed = &mut self.frame_elapsed;
        let timed_frames = &mut self.timed_frames;
        let mut on_frame = |frame: &[f32], denoise_prob: Option<f32>| {
            let frame_start = Instant::now();
            handle_frame(
                frame,
                vad_policy,
                vad,
                audio_cb,
                speech_clock,
                speech_cb,
                vad_frame_cb,
                vad_errors,
                keep_audio,
                processed_samples,
                chunk_tap,
                denoise_prob,
            );
            *frame_elapsed += frame_start.elapsed();
            *timed_frames += 1;
        };
        match (self.denoise_active, self.denoise.as_mut()) {
            (true, Some(chain)) => {
                let params = self.denoise_controls.get();
                chain.finish(params, |_| {}, |frame, prob| on_frame(frame, Some(prob)));
            }
            _ => self.frame_resampler.finish(|frame| on_frame(frame, None)),
        }

        if vad_policy != VadPolicy::Disabled
            && let Some(cfg) = &self.vad
        {
            let report = cfg.detector.lock().unwrap().tail_report();
            if let Some(report) = report {
                log::debug!(
                    "VAD at stop: withheld tail {} frames (~{}ms, {} voiced), in_speech={}, onset_counter={}, hangover_counter={}",
                    report.withheld_frames,
                    report.withheld_frames * cfg.frame_samples * 1000
                        / constants::WHISPER_SAMPLE_RATE as usize,
                    report.withheld_voiced_frames,
                    report.in_speech,
                    report.onset_counter,
                    report.hangover_counter
                );
            }
        }

        if self.total_dropped_samples > 0 {
            log::warn!(
                "Active recording completed after dropping {} microphone samples",
                self.total_dropped_samples
            );
        }

        let finish_elapsed = finish_start.elapsed();
        self.pipeline_elapsed += finish_elapsed;
        log::info!(target: "pipeline", "capture sample_rate={} frames={} total_ms={:.3} frame_vad_route_ms={:.3} frontend_taps_ms={:.3} chunk_max_ms={:.3} finish_ms={:.3} dropped_samples={} vad_errors={} denoise={}",
            self.in_sample_rate, self.timed_frames,
            self.pipeline_elapsed.as_secs_f64()*1000.0, self.frame_elapsed.as_secs_f64()*1000.0,
            self.pipeline_elapsed.saturating_sub(self.frame_elapsed).as_secs_f64()*1000.0,
            self.chunk_max_elapsed.as_secs_f64()*1000.0, finish_elapsed.as_secs_f64()*1000.0,
            self.total_dropped_samples, self.vad_errors, self.denoise_active);
        RecordedAudio {
            stt_samples: std::mem::take(&mut self.processed_samples),
            raw_samples: std::mem::take(&mut self.raw_captured_samples),
            native_sample_rate: self.in_sample_rate,
            native_sample_format: self.in_sample_format,
        }
    }
}

fn run_consumer(
    mut processor: CaptureProcessor,
    mut sample_consumer: Consumer<f32>,
    cmd_rx: mpsc::Receiver<Cmd>,
    transport: Arc<CaptureTransportState>,
    stream_error: Arc<AtomicBool>,
) {
    let mut recording = false;
    let mut stream_error_logged = false;

    loop {
        let mut command = if sample_consumer.slots() > 0 {
            match cmd_rx.try_recv() {
                Ok(command) => Some(command),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => return,
            }
        } else {
            match cmd_rx.recv_timeout(CONSUMER_POLL_INTERVAL) {
                Ok(command) => Some(command),
                Err(mpsc::RecvTimeoutError::Timeout) => None,
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
        };

        loop {
            if let Some(cmd) = command.take() {
                match cmd {
                    Cmd::Start {
                        vad_policy: policy,
                        pause_hold_ms,
                        capture_raw,
                        discard_audio,
                        sent_at,
                        ready_tx,
                    } => {
                        log::debug!(
                            "Cmd::Start processed {:?} after send; capture begins with {} samples",
                            sent_at.elapsed(),
                            if sample_consumer.slots() > 0 {
                                "the in-flight"
                            } else {
                                "the next available"
                            }
                        );
                        transport.overrun_samples.store(0, Ordering::Release);
                        processor.begin_recording_full(
                            policy,
                            pause_hold_ms,
                            capture_raw,
                            discard_audio,
                            ready_tx,
                        );
                        recording = true;
                    }
                    Cmd::Stop(reply_tx) => {
                        processor
                            .observe_overrun(transport.overrun_samples.swap(0, Ordering::AcqRel));
                        recording = false;
                        processor.cancel_ready_signal();

                        transport.pause_acknowledged.store(false, Ordering::Relaxed);
                        transport.pause_requested.store(true, Ordering::Release);
                        let pause_started = Instant::now();
                        while !transport.pause_acknowledged.load(Ordering::Acquire)
                            && pause_started.elapsed() < PAUSE_ACK_TIMEOUT
                        {
                            let drained =
                                processor.drain(&mut sample_consumer, ChunkDisposition::Capture);
                            if drained == 0 {
                                std::thread::sleep(Duration::from_millis(1));
                            }
                        }

                        let pause_timed_out = !transport.pause_acknowledged.load(Ordering::Acquire);
                        if pause_timed_out {
                            log::warn!("Timed out waiting for the microphone callback to pause");
                            stream_error.store(true, Ordering::Release);
                        }

                        while processor.drain(&mut sample_consumer, ChunkDisposition::Capture) > 0 {
                        }

                        processor
                            .observe_overrun(transport.overrun_samples.swap(0, Ordering::AcqRel));
                        let recorded = processor.finish_recording();
                        if !pause_timed_out {
                            transport.pause_acknowledged.store(false, Ordering::Relaxed);
                            transport.pause_requested.store(false, Ordering::Release);
                        }
                        let _ = reply_tx.send(recorded);

                        if pause_timed_out {
                            return;
                        }
                    }
                    Cmd::Shutdown => {
                        transport.pause_requested.store(true, Ordering::Release);
                        return;
                    }
                }
            }

            command = match cmd_rx.try_recv() {
                Ok(command) => Some(command),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => return,
            };
        }

        let disposition = if recording {
            ChunkDisposition::Capture
        } else {
            ChunkDisposition::Discard
        };
        processor.drain(&mut sample_consumer, disposition);

        let overrun_samples = transport.overrun_samples.swap(0, Ordering::AcqRel);
        if recording {
            processor.observe_overrun(overrun_samples);
        }

        if stream_error.load(Ordering::Acquire) && !stream_error_logged {
            log::error!("Microphone backend reported a stream error; it will be rebuilt");
            stream_error_logged = true;
        }
    }
}

#[cfg(test)]
mod tests;
