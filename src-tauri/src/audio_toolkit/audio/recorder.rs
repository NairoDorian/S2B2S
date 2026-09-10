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
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use rtrb::{Consumer, Producer, RingBuffer};

use crate::audio_toolkit::{
    VoiceActivityDetector,
    audio::{AudioVisualiser, FrameResampler},
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

#[cfg(target_os = "windows")]
struct MmcssHandle(Option<windows::Win32::Foundation::HANDLE>);

#[cfg(target_os = "windows")]
impl MmcssHandle {
    fn register(task_name: &str) -> Self {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        use windows::Win32::System::Threading::AvSetMmThreadCharacteristicsW;
        use windows::core::PCWSTR;

        let wide: Vec<u16> = OsStr::new(task_name).encode_wide().chain(Some(0)).collect();
        let mut task_index = 0u32;
        unsafe {
            match AvSetMmThreadCharacteristicsW(PCWSTR(wide.as_ptr()), &mut task_index) {
                Ok(h) => {
                    log::info!(
                        "MMCSS task '{task_name}' registered for audio capture worker (task_index={task_index})"
                    );
                    MmcssHandle(Some(h))
                }
                Err(e) => {
                    log::warn!("Failed to register MMCSS task '{task_name}': {e}");
                    MmcssHandle(None)
                }
            }
        }
    }
}

#[cfg(target_os = "windows")]
impl Drop for MmcssHandle {
    fn drop(&mut self) {
        if let Some(h) = self.0.take() {
            unsafe {
                use windows::Win32::System::Threading::AvRevertMmThreadCharacteristics;
                let _ = AvRevertMmThreadCharacteristics(h);
                log::debug!("MMCSS task reverted on audio worker thread exit");
            }
        }
    }
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
pub type LevelCallback = Arc<dyn Fn(Vec<f32>) + Send + Sync + 'static>;

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
    level_cb: Option<LevelCallback>,
    audio_cb: Option<AudioFrameCallback>,
    speech_cb: Option<SpeechActivityCallback>,
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
            level_cb: None,
            audio_cb: None,
            speech_cb: None,
            speech_ms: Arc::new(AtomicU64::new(0)),
            selected_channel: None,
            config_cache: Arc::new(Mutex::new(None)),
            stream_error: Arc::new(AtomicBool::new(false)),
        })
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

    pub fn with_level_callback<F>(mut self, cb: F) -> Self
    where
        F: Fn(Vec<f32>) + Send + Sync + 'static,
    {
        self.level_cb = Some(Arc::new(cb));
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

        let host = crate::audio_toolkit::get_cpal_host();
        let device = match device {
            Some(dev) => dev,
            None => host
                .default_input_device()
                .ok_or_else(|| Error::new(std::io::ErrorKind::NotFound, "No input device found"))?,
        };

        let thread_device = device.clone();
        let vad = self.vad.clone();
        let level_cb = self.level_cb.clone();
        let audio_cb = self.audio_cb.clone();
        let speech_cb = self.speech_cb.clone();
        let speech_ms = Arc::clone(&self.speech_ms);
        let selected_channel = self.selected_channel;
        let config_cache = Arc::clone(&self.config_cache);
        let stream_error = Arc::clone(&self.stream_error);

        let worker = std::thread::spawn(move || {
            #[cfg(target_os = "windows")]
            let _mmcss = MmcssHandle::register("Audio");

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
                        .map(|(_, cfg)| cfg.clone());
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

                    log::info!(
                        "Using device: {:?}\nSample rate: {}\nChannels: {}\nFormat: {:?}",
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
                            selected_channel,
                            Arc::clone(&transport),
                            Arc::clone(&stream_error),
                        ),
                        cpal::SampleFormat::I8 => AudioRecorder::build_stream::<i8>(
                            &thread_device,
                            &config,
                            channels,
                            selected_channel,
                            Arc::clone(&transport),
                            Arc::clone(&stream_error),
                        ),
                        cpal::SampleFormat::I16 => AudioRecorder::build_stream::<i16>(
                            &thread_device,
                            &config,
                            channels,
                            selected_channel,
                            Arc::clone(&transport),
                            Arc::clone(&stream_error),
                        ),
                        cpal::SampleFormat::I32 => AudioRecorder::build_stream::<i32>(
                            &thread_device,
                            &config,
                            channels,
                            selected_channel,
                            Arc::clone(&transport),
                            Arc::clone(&stream_error),
                        ),
                        cpal::SampleFormat::F32 => AudioRecorder::build_stream::<f32>(
                            &thread_device,
                            &config,
                            channels,
                            selected_channel,
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
                        level_cb,
                        audio_cb,
                        speech_cb,
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
        let tx = self
            .cmd_tx
            .as_ref()
            .ok_or_else(|| Error::other("Recorder is not open"))?;
        let (ready_tx, ready_rx) = mpsc::channel();
        tx.send(Cmd::Start {
            vad_policy,
            pause_hold_ms,
            capture_raw,
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
            config.clone().into(),
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
    vad_errors: &mut u64,
    out_buf: &mut Vec<f32>,
) {
    let mut emit = |buf: &[f32]| {
        out_buf.extend_from_slice(buf);
        if let Some(cb) = audio_cb {
            cb(buf);
        }
    };

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
                        "VAD failed on a frame; passing audio through unfiltered for the rest of this recording: {e}"
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
        det.last_frame_voiced()
    } else {
        emit(samples);
        true
    };

    if let Some(activity) = speech_clock.tick(voiced) {
        if let Some(cb) = speech_cb {
            cb(activity);
        }
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
    level_cb: Option<LevelCallback>,
    audio_cb: Option<AudioFrameCallback>,
    speech_cb: Option<SpeechActivityCallback>,
    speech_clock: SpeechClock,
    stream_running_at: Instant,
    visualizer: AudioVisualiser,
    frame_resampler: FrameResampler,
    max_drain_samples: usize,
    first_chunk_logged: bool,
    vad_policy: VadPolicy,
    capture_raw: bool,
    raw_captured_samples: Vec<f32>,
    processed_samples: Vec<f32>,
    vad_errors: u64,
    awaiting_first_captured_chunk: Option<Instant>,
    capture_ready_tx: Option<mpsc::Sender<()>>,
    total_dropped_samples: u64,
    overrun_warning_logged: bool,
}

impl CaptureProcessor {
    #[cfg(test)]
    pub(crate) fn new(
        in_sample_rate: u32,
        vad: Option<VadConfig>,
        level_cb: Option<LevelCallback>,
        audio_cb: Option<AudioFrameCallback>,
        stream_running_at: Instant,
    ) -> Self {
        Self::with_options(
            in_sample_rate,
            cpal::SampleFormat::F32,
            vad,
            level_cb,
            audio_cb,
            None,
            Arc::new(AtomicU64::new(0)),
            stream_running_at,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn with_options(
        in_sample_rate: u32,
        in_sample_format: cpal::SampleFormat,
        vad: Option<VadConfig>,
        level_cb: Option<LevelCallback>,
        audio_cb: Option<AudioFrameCallback>,
        speech_cb: Option<SpeechActivityCallback>,
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

        const BUCKETS: usize = 16;
        let target_window = (f64::from(in_sample_rate) / 30.0).round() as usize;
        let window_size = [256usize, 512, 1024, 2048]
            .into_iter()
            .min_by_key(|w| w.abs_diff(target_window))
            .unwrap();
        let visualizer = AudioVisualiser::new(in_sample_rate, window_size, BUCKETS, 400.0, 4000.0);

        let max_drain_samples =
            ((in_sample_rate as u128 * MAX_DRAIN_CHUNK.as_millis()) / 1_000).max(1) as usize;

        Self {
            in_sample_rate,
            in_sample_format,
            vad,
            level_cb,
            audio_cb,
            speech_cb,
            speech_clock,
            stream_running_at,
            visualizer,
            frame_resampler,
            max_drain_samples,
            first_chunk_logged: false,
            vad_policy: VadPolicy::Offline,
            capture_raw: false,
            raw_captured_samples: Vec::new(),
            processed_samples: Vec::new(),
            vad_errors: 0,
            awaiting_first_captured_chunk: None,
            capture_ready_tx: None,
            total_dropped_samples: 0,
            overrun_warning_logged: false,
        }
    }

    #[cfg(test)]
    pub(crate) fn begin_recording(&mut self, policy: VadPolicy, ready_tx: mpsc::Sender<()>) {
        self.begin_recording_full(policy, DEFAULT_SPEECH_PAUSE_HOLD_MS, false, ready_tx);
    }

    /// Reset per-recording state and arm the first-sample acknowledgement.
    pub(crate) fn begin_recording_full(
        &mut self,
        policy: VadPolicy,
        pause_hold_ms: u32,
        capture_raw: bool,
        ready_tx: mpsc::Sender<()>,
    ) {
        self.awaiting_first_captured_chunk = Some(Instant::now());
        self.capture_ready_tx = Some(ready_tx);
        self.total_dropped_samples = 0;
        self.overrun_warning_logged = false;
        self.vad_policy = policy;
        self.capture_raw = capture_raw;
        self.raw_captured_samples.clear();
        self.processed_samples.clear();
        self.vad_errors = 0;
        self.visualizer.reset();
        self.frame_resampler.reset();
        self.speech_clock.reset(u64::from(pause_hold_ms));
        if policy != VadPolicy::Disabled {
            if let Some(cfg) = &self.vad {
                let mut detector = cfg.detector.lock().unwrap();
                detector.set_hangover_frames(cfg.hangover_for(policy));
                detector.reset();
            }
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

        if self.capture_raw {
            self.raw_captured_samples.extend_from_slice(raw);
        }

        if let Some(buckets) = self.visualizer.feed(raw) {
            if let Some(callback) = &self.level_cb {
                callback(buckets);
            }
        }

        let vad_policy = self.vad_policy;
        let vad = &self.vad;
        let audio_cb = &self.audio_cb;
        let speech_cb = &self.speech_cb;
        let speech_clock = &mut self.speech_clock;
        let vad_errors = &mut self.vad_errors;
        let processed_samples = &mut self.processed_samples;

        self.frame_resampler.push(raw, |frame: &[f32]| {
            handle_frame(
                frame,
                vad_policy,
                vad,
                audio_cb,
                speech_clock,
                speech_cb,
                vad_errors,
                processed_samples,
            )
        });

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
        let vad_policy = self.vad_policy;
        let vad = &self.vad;
        let audio_cb = &self.audio_cb;
        let speech_cb = &self.speech_cb;
        let speech_clock = &mut self.speech_clock;
        let vad_errors = &mut self.vad_errors;
        let processed_samples = &mut self.processed_samples;

        self.frame_resampler.finish(|frame: &[f32]| {
            handle_frame(
                frame,
                vad_policy,
                vad,
                audio_cb,
                speech_clock,
                speech_cb,
                vad_errors,
                processed_samples,
            )
        });

        if vad_policy != VadPolicy::Disabled {
            if let Some(cfg) = &self.vad {
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
        }

        if self.total_dropped_samples > 0 {
            log::warn!(
                "Active recording completed after dropping {} microphone samples",
                self.total_dropped_samples
            );
        }

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
