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

use crate::audio_toolkit::{
    VoiceActivityDetector,
    audio::{AudioVisualiser, FrameResampler},
    constants,
    vad::{self, VadFrame},
};

/// The audio result returned when a recording session completes.
#[derive(Clone, Debug)]
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

enum Cmd {
    /// Begin capturing. Carries the pause tolerance for this session's speech
    /// clock and the send timestamp so the consumer can log how long the command
    /// sat in the channel, plus a one-shot acknowledgement sent only after the
    /// first microphone sample chunk is processed.
    Start(VadPolicy, u32, Instant, mpsc::Sender<()>),
    Stop(mpsc::Sender<RecordedAudio>),
    Shutdown,
}

/// Length of one resampled frame handed to the VAD. Not a free choice: Silero
/// v5/v6 accepts exactly 512 samples at 16 kHz, so the capture pipeline is
/// framed to match and one frame is one VAD decision. Shared with the speech
/// clock so the two can never disagree about how much audio a decision covers.
#[cfg(test)]
const FRAME_MS: u64 = constants::VAD_FRAME_MS;

/// Default pause tolerance: how long silence must last before the speech clock
/// stops counting. Long enough to ride out the gaps between words and a breath,
/// short enough that a real pause registers promptly. Overridable per session
/// via the `speech_pause_hold_ms` setting.
pub const DEFAULT_SPEECH_PAUSE_HOLD_MS: u32 = 500;

/// How often the speech clock reports in while speech is ongoing. State flips
/// are always reported immediately, so this only bounds how stale a running
/// timer can look: ~6.7 Hz, roughly a fifth of the mic-level rate.
const SPEECH_HEARTBEAT_MS: u64 = 150;

enum AudioChunk {
    Samples(Vec<f32>),
    EndOfStream,
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

/// A single VAD engine plus the two hangover-tail lengths its smoothing wrapper
/// should use. The offline and streaming policies are never active
/// concurrently, so one detector is reconfigured per session (see `Cmd::Start`)
/// rather than kept as two resident engines.
#[derive(Clone)]
struct VadConfig {
    detector: Arc<Mutex<Box<dyn vad::VoiceActivityDetector>>>,
    frame_samples: usize,
    offline_hangover_frames: usize,
    streaming_hangover_frames: usize,
    onset_frames: usize,
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

/// Tracks how long the user has actually been speaking, in frame-sized steps.
///
/// Driven by [`VoiceActivityDetector::last_frame_voiced`] — the *raw* per-frame
/// verdict — rather than by what `push_frame` returns. The smoothing wrapper
/// keeps reporting speech through a hangover tail up to 1.76 s long in
/// streaming mode, and counting that would inflate the measured speech time by
/// more than a second per pause, which in turn deflates the words-per-minute
/// figure computed from it.
///
/// Gaps shorter than `hold_ms` are treated as part of the same utterance and
/// counted retroactively once speech resumes; gaps that reach `hold_ms` end the
/// utterance and are never counted. The clock therefore only ever moves
/// forward — resuming after a short gap makes it catch up by at most `hold_ms`,
/// which is far less jarring than a timer that visibly rewinds.
#[derive(Debug)]
pub struct SpeechClock {
    /// Milliseconds per audio frame (e.g. 32ms for Silero, 16ms for Earshot).
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
    /// thread. The speech-activity callback is gated on overlay settings, so
    /// callers that need the number regardless — like the decision whether a
    /// recording contains enough speech to be worth transcribing — read this.
    published: Arc<AtomicU64>,
    ms_since_emit: u64,
    /// `speech_ms` as of the last emission, so a heartbeat is skipped when the
    /// clock has not actually moved — notably through the whole pause-tolerance
    /// window, where the state is still "speaking" but the timer is frozen.
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
        // moved and gone stale. Silence — including the pause-tolerance window,
        // where the state still reads as speaking — produces no traffic.
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
    level_cb: Option<Arc<dyn Fn(Vec<f32>) + Send + Sync + 'static>>,
    audio_cb: Option<AudioFrameCallback>,
    speech_cb: Option<SpeechActivityCallback>,
    /// Milliseconds of speech in the most recent recording, published by the
    /// consumer thread's [`SpeechClock`]. Final once `stop()` has returned,
    /// since the consumer only replies after draining every captured frame.
    speech_ms: Arc<AtomicU64>,
    /// Which input channel to use. None = average all (original behavior).
    selected_channel: Option<usize>,
    /// Preferred stream config cached per device name. The two HAL property
    /// queries in `get_preferred_config` cost ~40-85ms per open (worse on
    /// USB/Bluetooth), which lands on the keypress->capture path in on-demand
    /// mode. Keyed by name so a system-default change misses naturally;
    /// cleared whenever an open fails so a stale rate/format self-heals on the
    /// caller's retry.
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
    /// streaming hangover tail. The two policies are mutually exclusive within a
    /// recording, so one engine covers both instead of two resident instances.
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
    /// VAD policy has been applied. Frames arrive in real time, in order, on the
    /// recorder's consumer thread — keep the callback cheap (e.g. forward to a
    /// channel) so it never stalls capture.
    pub fn with_audio_callback<F>(mut self, cb: F) -> Self
    where
        F: Fn(&[f32]) + Send + Sync + 'static,
    {
        self.audio_cb = Some(Arc::new(cb));
        self
    }

    /// Register a callback that receives speech-clock updates while recording:
    /// whether the user is currently speaking, and how much speech this session
    /// has accumulated. Fires on state flips and on a ~150 ms heartbeat while
    /// speech is ongoing, and stays quiet through silence.
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

    pub fn open(&mut self, device: Option<Device>) -> Result<(), Box<dyn std::error::Error>> {
        if self.worker_handle.is_some() {
            if !self.needs_reopen() {
                return Ok(()); // already open
            }
            log::warn!("Capture stream failed; rebuilding microphone stream");
            let _ = self.close();
        }

        self.stream_error.store(false, Ordering::Relaxed);

        let (sample_tx, sample_rx) = mpsc::channel::<AudioChunk>();
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
        // Move the optional level callback into the worker thread
        let level_cb = self.level_cb.clone();
        // Move the optional real-time audio frame callback into the worker thread
        let audio_cb = self.audio_cb.clone();
        let speech_cb = self.speech_cb.clone();
        let speech_ms = Arc::clone(&self.speech_ms);
        let selected_channel = self.selected_channel;
        let config_cache = Arc::clone(&self.config_cache);
        let stream_error = Arc::clone(&self.stream_error);

        let worker = std::thread::spawn(move || {
            #[cfg(target_os = "windows")]
            let _mmcss = {
                let handle = MmcssHandle::register("Capture");
                if handle.0.is_none() {
                    MmcssHandle::register("Audio")
                } else {
                    handle
                }
            };

            let stop_flag = Arc::new(AtomicBool::new(false));
            let stop_flag_for_stream = stop_flag.clone();
            let init_result = (|| -> Result<(cpal::Stream, u32, cpal::SampleFormat), String> {
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
                let channels = config.channels() as usize;

                log::info!(
                    "Using device: {:?}\nSample rate: {}\nChannels: {}\nFormat: {:?}",
                    thread_device
                        .description()
                        .map(|d| d.name().to_string())
                        .unwrap_or_default(),
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
                let stream = match config.sample_format() {
                    cpal::SampleFormat::U8 => AudioRecorder::build_stream::<u8>(
                        &thread_device,
                        &config,
                        sample_tx,
                        channels,
                        selected_channel,
                        stop_flag_for_stream,
                        Arc::clone(&stream_error),
                    )
                    .map_err(|e| format!("Failed to build input stream: {e}"))?,
                    cpal::SampleFormat::I8 => AudioRecorder::build_stream::<i8>(
                        &thread_device,
                        &config,
                        sample_tx,
                        channels,
                        selected_channel,
                        stop_flag_for_stream,
                        Arc::clone(&stream_error),
                    )
                    .map_err(|e| format!("Failed to build input stream: {e}"))?,
                    cpal::SampleFormat::I16 => AudioRecorder::build_stream::<i16>(
                        &thread_device,
                        &config,
                        sample_tx,
                        channels,
                        selected_channel,
                        stop_flag_for_stream,
                        Arc::clone(&stream_error),
                    )
                    .map_err(|e| format!("Failed to build input stream: {e}"))?,
                    cpal::SampleFormat::I32 => AudioRecorder::build_stream::<i32>(
                        &thread_device,
                        &config,
                        sample_tx,
                        channels,
                        selected_channel,
                        stop_flag_for_stream,
                        Arc::clone(&stream_error),
                    )
                    .map_err(|e| format!("Failed to build input stream: {e}"))?,
                    cpal::SampleFormat::F32 => AudioRecorder::build_stream::<f32>(
                        &thread_device,
                        &config,
                        sample_tx,
                        channels,
                        selected_channel,
                        stop_flag_for_stream,
                        Arc::clone(&stream_error),
                    )
                    .map_err(|e| format!("Failed to build input stream: {e}"))?,
                    sample_format => {
                        return Err(format!("Unsupported sample format: {sample_format:?}"));
                    }
                };
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

                let sample_format = config.sample_format();
                // The device accepted this config; remember it so the next
                // open skips the HAL property queries entirely.
                if !config_was_cached && !device_name.is_empty() {
                    *config_cache.lock().unwrap() = Some((device_name, config));
                }

                Ok((stream, sample_rate, sample_format))
            })();

            match init_result {
                Ok((stream, sample_rate, sample_format)) => {
                    let _ = init_tx.send(Ok(()));
                    // Timestamp for the play()-returned -> first-samples gap the
                    // init handshake can't see (hardware dependent).
                    let stream_running_at = Instant::now();
                    // Keep the stream alive while we process samples.
                    run_consumer(
                        sample_rate,
                        sample_format,
                        vad,
                        sample_rx,
                        cmd_rx,
                        level_cb,
                        audio_cb,
                        speech_cb,
                        speech_ms,
                        stop_flag,
                        stream_running_at,
                    );
                    drop(stream);
                }
                Err(error_message) => {
                    // A failed open may mean the cached config went stale
                    // (device re-plugged, rate/format changed in the OS).
                    // Drop it so the next attempt re-queries the device.
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

    /// Queue a recording start and return a one-shot receiver that resolves only
    /// after the first real microphone sample chunk has entered the capture path.
    /// `Stream::play()` returning is not sufficient: some Bluetooth and USB
    /// devices take much longer to begin delivering callbacks.
    /// Begin a recording session. `pause_hold_ms` is how long silence must
    /// last before this session's speech clock stops counting (see
    /// [`SpeechClock`]).
    pub fn start(
        &self,
        vad_policy: VadPolicy,
        pause_hold_ms: u32,
    ) -> Result<mpsc::Receiver<()>, Box<dyn std::error::Error>> {
        let tx = self
            .cmd_tx
            .as_ref()
            .ok_or_else(|| Error::other("Recorder is not open"))?;
        let (ready_tx, ready_rx) = mpsc::channel();
        tx.send(Cmd::Start(
            vad_policy,
            pause_hold_ms,
            Instant::now(),
            ready_tx,
        ))?;
        Ok(ready_rx)
    }

    /// Milliseconds of actual speech in the most recent recording, as measured
    /// by the VAD. Zero when VAD is disabled for the session is not possible —
    /// every frame counts as speech in that mode — so a low value here really
    /// does mean the microphone heard nothing worth decoding.
    pub fn speech_ms(&self) -> u64 {
        self.speech_ms.load(Ordering::Relaxed)
    }

    pub fn stop(&self) -> Result<RecordedAudio, Box<dyn std::error::Error>> {
        let (resp_tx, resp_rx) = mpsc::channel();
        if let Some(tx) = &self.cmd_tx {
            tx.send(Cmd::Stop(resp_tx))?;
        }
        Ok(resp_rx.recv()?) // wait for the samples
    }

    /// True when the active capture stream must be rebuilt.
    ///
    /// cpal may report a device disconnect asynchronously without closing its
    /// callback channel, so also honor the error callback's explicit flag.
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
        if let Some(h) = self.worker_handle.take() {
            let _ = h.join();
        }
        self.device = None;
        Ok(())
    }

    fn build_stream<T>(
        device: &cpal::Device,
        config: &cpal::SupportedStreamConfig,
        sample_tx: mpsc::Sender<AudioChunk>,
        channels: usize,
        selected_channel: Option<usize>,
        stop_flag: Arc<AtomicBool>,
        stream_error: Arc<AtomicBool>,
    ) -> Result<cpal::Stream, cpal::Error>
    where
        T: Sample + SizedSample + Send + 'static,
        f32: cpal::FromSample<T>,
    {
        // Resolve the effective channel to use. If the selected channel is
        // out of range for this device, fall back to averaging all channels.
        let use_channel: Option<usize> = match selected_channel {
            Some(ch) if ch < channels => Some(ch),
            Some(_) => None, // out of range, fall back to average
            None => None,    // user chose "average all"
        };

        let make_stream_cb = |sample_tx: mpsc::Sender<AudioChunk>, stop_flag: Arc<AtomicBool>| {
            let mut output_buffer = Vec::new();
            let mut eos_sent = false;

            move |data: &[T], _: &cpal::InputCallbackInfo| {
                handle_input_block(
                    data,
                    channels,
                    use_channel,
                    &stop_flag,
                    &mut eos_sent,
                    &mut output_buffer,
                    &sample_tx,
                );
            }
        };

        let mut stream_config: cpal::StreamConfig = config.clone().into();
        let requested_min = match config.buffer_size() {
            cpal::SupportedBufferSize::Range { min, .. } if *min > 0 => {
                stream_config.buffer_size = cpal::BufferSize::Fixed(*min);
                log::debug!("Requesting minimum stream buffer size: {} frames", min);
                Some(*min)
            }
            _ => None,
        };

        let stream_cb = make_stream_cb(sample_tx.clone(), Arc::clone(&stop_flag));
        let stream_error_clone = Arc::clone(&stream_error);
        let err_cb = move |err| {
            log::error!("Stream error: {}", err);
            stream_error_clone.store(true, Ordering::Relaxed);
        };

        match device.build_input_stream(stream_config.clone(), stream_cb, err_cb, None) {
            Ok(stream) => Ok(stream),
            Err(e) if requested_min.is_some() => {
                log::warn!(
                    "Failed to build input stream with minimum buffer size ({e}); falling back to default buffer size"
                );
                let fallback_cb = make_stream_cb(sample_tx, stop_flag);
                let mut fallback_config = stream_config;
                fallback_config.buffer_size = cpal::BufferSize::Default;
                device.build_input_stream(
                    fallback_config,
                    fallback_cb,
                    move |err| {
                        log::error!("Stream error: {}", err);
                        stream_error.store(true, Ordering::Relaxed);
                    },
                    None,
                )
            }
            Err(e) => Err(e),
        }
    }

    pub fn preferred_input_channel_count(
        device: &cpal::Device,
    ) -> Result<u16, Box<dyn std::error::Error>> {
        Ok(Self::get_preferred_config(device)?.channels())
    }

    fn get_preferred_config(
        device: &cpal::Device,
    ) -> Result<cpal::SupportedStreamConfig, Box<dyn std::error::Error>> {
        // Use the device's native/default sample rate and let the FrameResampler
        // in run_consumer() downsample to 16kHz. This avoids forcing hardware into
        // a non-native rate which can cause issues on some devices (Bluetooth
        // codecs, certain ALSA drivers, etc.).
        let default_config = device.default_input_config()?;
        let target_rate = default_config.sample_rate();

        // Try to find the best sample format at the device's default rate
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
                        // Prioritize F32 > I16 > I32 > others
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

        // Fall back to device default if no config matched (exotic/virtual devices)
        log::warn!(
            "No supported config matched device default rate {:?}, using default config",
            target_rate
        );
        Ok(default_config)
    }
}

/// Body of the cpal input callback, extracted for testing without a device.
/// Converts the block to mono and forwards it. The block that first observes
/// the stop flag was captured before the stop, so it is still forwarded —
/// dropping it loses up to a callback period of tail audio (worst on
/// Bluetooth) — followed by the end-of-stream sentinel; later blocks are
/// dropped until the flag clears.
fn handle_input_block<T>(
    data: &[T],
    channels: usize,
    use_channel: Option<usize>,
    stop_flag: &AtomicBool,
    eos_sent: &mut bool,
    output_buffer: &mut Vec<f32>,
    sample_tx: &mpsc::Sender<AudioChunk>,
) where
    T: Sample,
    f32: cpal::FromSample<T>,
{
    let stopping = stop_flag.load(Ordering::Relaxed);
    if stopping && *eos_sent {
        return;
    }

    output_buffer.clear();

    if channels == 1 {
        output_buffer.extend(data.iter().map(|&sample| sample.to_sample::<f32>()));
    } else {
        let frame_count = data.len() / channels;
        output_buffer.reserve(frame_count);

        if let Some(ch) = use_channel {
            for frame in data.chunks_exact(channels) {
                let mono_sample = frame[ch].to_sample::<f32>();
                output_buffer.push(mono_sample);
            }
        } else {
            for frame in data.chunks_exact(channels) {
                let mono_sample = frame
                    .iter()
                    .map(|&sample| sample.to_sample::<f32>())
                    .sum::<f32>()
                    / channels as f32;
                output_buffer.push(mono_sample);
            }
        }
    }

    // A failed send means the consumer thread is gone. During shutdown that is
    // expected (the consumer exits before the stream is dropped), so only
    // report it when capture was supposed to be live.
    if sample_tx
        .send(AudioChunk::Samples(output_buffer.clone()))
        .is_err()
        && !stopping
    {
        log::error!("Failed to send samples");
    }

    if stopping {
        let _ = sample_tx.send(AudioChunk::EndOfStream);
        *eos_sent = true;
    } else {
        *eos_sent = false;
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

#[cfg(test)]
mod tests {
    use super::{
        AudioChunk, AudioRecorder, Cmd, FRAME_MS, SPEECH_HEARTBEAT_MS, SpeechClock,
        handle_input_block, run_consumer,
    };
    use std::{
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, AtomicU64, Ordering},
            mpsc,
        },
        thread,
        time::{Duration, Instant},
    };

    #[test]
    fn unopened_recorder_does_not_need_reopen() {
        // No worker has been spawned yet, so there is nothing to reap. Guards
        // against inverting the "no worker" case, which would make every first
        // open() take the rebuild path.
        let recorder = AudioRecorder::new().expect("recorder");
        assert!(!recorder.needs_reopen());
    }

    #[test]
    fn stream_error_requires_reopen() {
        let recorder = AudioRecorder::new().expect("recorder");
        recorder.stream_error.store(true, Ordering::Relaxed);
        assert!(recorder.needs_reopen());
    }

    #[test]
    fn boundary_block_forwarded_before_eos() {
        let (tx, rx) = mpsc::channel();
        let stop_flag = AtomicBool::new(false);
        let mut eos_sent = false;
        let mut scratch = Vec::new();
        let mut push = |flag: &AtomicBool, eos: &mut bool, block: &[f32]| {
            handle_input_block::<f32>(block, 1, None, flag, eos, &mut scratch, &tx)
        };

        // Running: blocks forwarded, no sentinel.
        push(&stop_flag, &mut eos_sent, &[0.1]);
        assert!(matches!(rx.try_recv(), Ok(AudioChunk::Samples(_))));
        assert!(rx.try_recv().is_err());

        // The block observing the stop flag is still forwarded, then EOS.
        stop_flag.store(true, Ordering::Relaxed);
        push(&stop_flag, &mut eos_sent, &[0.5, 0.5]);
        match rx.try_recv() {
            Ok(AudioChunk::Samples(samples)) => assert_eq!(samples, vec![0.5, 0.5]),
            _ => panic!("boundary block must be forwarded, not dropped"),
        }
        assert!(matches!(rx.try_recv(), Ok(AudioChunk::EndOfStream)));

        // Later blocks are dropped until the flag clears, then capture resumes.
        push(&stop_flag, &mut eos_sent, &[0.9]);
        assert!(rx.try_recv().is_err(), "blocks after EOS must be dropped");
        stop_flag.store(false, Ordering::Relaxed);
        push(&stop_flag, &mut eos_sent, &[0.2]);
        assert!(matches!(rx.try_recv(), Ok(AudioChunk::Samples(_))));
        assert!(rx.try_recv().is_err(), "no sentinel while running");
    }

    #[test]
    fn shutdown_is_processed_without_audio_samples() {
        let (sample_tx, sample_rx) = mpsc::channel();
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            run_consumer(
                48_000,
                cpal::SampleFormat::F32,
                None,
                sample_rx,
                cmd_rx,
                None,
                None,
                None,
                Arc::new(AtomicU64::new(0)),
                Arc::new(AtomicBool::new(false)),
                Instant::now(),
            );
            let _ = done_tx.send(());
        });

        cmd_tx.send(Cmd::Shutdown).expect("send shutdown");
        let stopped = done_rx.recv_timeout(Duration::from_secs(1));

        // Unblock the old implementation so a failing test still exits cleanly.
        drop(sample_tx);
        worker.join().expect("join consumer");
        assert!(stopped.is_ok(), "shutdown waited for an audio sample");
    }

    /// A clock wired to a throwaway published counter.
    fn clock(hold_ms: u64) -> SpeechClock {
        SpeechClock::new(2, hold_ms, FRAME_MS, Arc::new(AtomicU64::new(0)))
    }

    /// Feed `n` frames of one verdict, discarding emissions.
    fn feed(clock: &mut SpeechClock, voiced: bool, frames: usize) {
        for _ in 0..frames {
            clock.tick(voiced);
        }
    }

    #[test]
    fn speech_clock_needs_onset_frames_before_counting() {
        let mut clock = clock(500);
        // A lone voiced frame is noise, not speech.
        assert_eq!(clock.tick(true), None);
        assert_eq!(clock.snapshot().speech_ms, 0);
        assert!(!clock.snapshot().speaking);

        // The second consecutive voiced frame opens the utterance, and both
        // onset frames are billed.
        let activity = clock.tick(true).expect("onset flips the state");
        assert!(activity.speaking);
        assert_eq!(activity.speech_ms, 2 * FRAME_MS);
    }

    #[test]
    fn speech_clock_earshot_16ms_frames_measure_time_accurately() {
        let published = Arc::new(AtomicU64::new(0));
        // Earshot uses 16ms frames (256 samples at 16kHz)
        let mut earshot_clock = SpeechClock::new(2, 500, 16, Arc::clone(&published));
        assert_eq!(earshot_clock.frame_ms(), 16);

        // 2 onset frames = 2 * 16ms = 32ms
        feed(&mut earshot_clock, true, 2);
        assert_eq!(earshot_clock.snapshot().speech_ms, 32);

        // 50 voiced frames = 50 * 16ms = 800ms of audio
        feed(&mut earshot_clock, true, 48); // total 50 frames
        assert_eq!(earshot_clock.snapshot().speech_ms, 50 * 16);
        assert_eq!(published.load(Ordering::Relaxed), 800);
    }

    #[test]
    fn speech_clock_bridges_gaps_shorter_than_the_hold() {
        let mut clock = clock(500);
        let ten = 10 * FRAME_MS;
        feed(&mut clock, true, 10);
        assert_eq!(clock.snapshot().speech_ms, ten);

        // Ten frames of silence is under the 500ms tolerance: the clock
        // freezes...
        feed(&mut clock, false, 10);
        assert_eq!(clock.snapshot().speech_ms, ten);
        assert!(clock.snapshot().speaking, "still mid-utterance");

        // ...and the bridged gap is billed once speech resumes, so the utterance
        // measures end-to-end rather than dropping its internal pauses.
        clock.tick(true);
        assert_eq!(clock.snapshot().speech_ms, ten + ten + FRAME_MS);
    }

    #[test]
    fn speech_clock_discards_pauses_that_reach_the_hold() {
        let mut clock = clock(500);
        let ten = 10 * FRAME_MS;
        feed(&mut clock, true, 10);
        feed(&mut clock, false, 20); // well past the 500ms tolerance
        assert!(!clock.snapshot().speaking);
        assert_eq!(clock.snapshot().speech_ms, ten, "the pause is not speech");

        // The next utterance re-runs the onset debounce and resumes from there.
        feed(&mut clock, true, 2);
        assert_eq!(clock.snapshot().speech_ms, ten + 2 * FRAME_MS);
    }

    #[test]
    fn speech_clock_never_moves_backwards() {
        let mut clock = clock(500);
        let mut last = 0;
        // Alternating verdicts are the worst case for the bridging logic.
        for i in 0..200 {
            clock.tick(i % 3 != 0);
            let now = clock.snapshot().speech_ms;
            assert!(now >= last, "speech clock rewound: {last} -> {now}");
            last = now;
        }
    }

    #[test]
    fn speech_clock_stays_quiet_through_silence() {
        let mut clock = clock(500);
        // Silence before any speech produces no traffic at all.
        for _ in 0..100 {
            assert_eq!(clock.tick(false), None);
        }

        feed(&mut clock, true, 2); // open an utterance
        // Only the flip to silent is reported; the rest of the pause is quiet.
        let mut emissions = 0;
        for _ in 0..100 {
            if clock.tick(false).is_some() {
                emissions += 1;
            }
        }
        assert_eq!(emissions, 1);
    }

    #[test]
    fn speech_clock_heartbeats_while_speech_continues() {
        let mut clock = clock(500);
        feed(&mut clock, true, 2); // opening flip

        let frames = 100;
        let emissions = (0..frames).filter(|_| clock.tick(true).is_some()).count();
        // One report per heartbeat window, not one per frame.
        let frames_per_emit = SPEECH_HEARTBEAT_MS.div_ceil(FRAME_MS) as usize;
        assert_eq!(emissions, frames / frames_per_emit);
    }

    #[test]
    fn speech_clock_publishes_its_total_for_readers_off_thread() {
        // The skip-decode guard reads this after stop(), independently of the
        // overlay callback, which is gated on settings.
        let published = Arc::new(AtomicU64::new(0));
        let mut clock = SpeechClock::new(2, 500, FRAME_MS, Arc::clone(&published));

        feed(&mut clock, true, 10);
        assert_eq!(
            published.load(Ordering::Relaxed),
            clock.snapshot().speech_ms
        );
        assert!(published.load(Ordering::Relaxed) > 0);

        // Silence past the tolerance freezes both, and never rewinds either.
        let frozen = published.load(Ordering::Relaxed);
        feed(&mut clock, false, 30);
        assert_eq!(published.load(Ordering::Relaxed), frozen);

        clock.reset(500);
        assert_eq!(published.load(Ordering::Relaxed), 0, "reset must clear it");
    }

    #[test]
    fn speech_clock_reset_adopts_the_new_hold() {
        let mut clock = clock(500);
        feed(&mut clock, true, 10);
        clock.reset(150);
        assert_eq!(clock.snapshot().speech_ms, 0);
        assert!(!clock.snapshot().speaking);

        feed(&mut clock, true, 2);
        // 150ms of silence now ends the utterance rather than being bridged.
        feed(&mut clock, false, 5);
        assert!(!clock.snapshot().speaking);
        assert_eq!(clock.snapshot().speech_ms, 2 * FRAME_MS);
    }

    /// End-to-end through the consumer thread: Cmd::Start, real sample chunks,
    /// speech-activity callbacks out. Guards the whole plumbing path (builder ->
    /// worker clone -> run_consumer -> handle_frame -> callback), which unit
    /// tests on SpeechClock alone cannot see.
    #[test]
    fn consumer_reports_speech_activity_while_recording() {
        let (sample_tx, sample_rx) = mpsc::channel();
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();

        let seen: Arc<Mutex<Vec<super::SpeechActivity>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        let speech_cb: super::SpeechActivityCallback = Arc::new(move |activity| {
            sink.lock().unwrap().push(activity);
        });

        let worker = thread::spawn(move || {
            run_consumer(
                16_000,
                cpal::SampleFormat::I16,
                None, // no VAD: every frame counts as speech
                sample_rx,
                cmd_rx,
                None,
                None,
                Some(speech_cb),
                Arc::new(AtomicU64::new(0)),
                Arc::new(AtomicBool::new(false)),
                Instant::now(),
            );
            let _ = done_tx.send(());
        });

        let (ready_tx, _ready_rx) = mpsc::channel();
        cmd_tx
            .send(Cmd::Start(
                super::VadPolicy::Offline,
                500,
                Instant::now(),
                ready_tx,
            ))
            .expect("send start");

        // One second of audio at 16 kHz, in 100 ms chunks.
        for _ in 0..10 {
            sample_tx
                .send(super::AudioChunk::Samples(vec![0.1f32; 1_600]))
                .expect("send samples");
        }

        let (stop_tx, stop_rx) = mpsc::channel();
        cmd_tx.send(Cmd::Stop(stop_tx)).expect("send stop");
        sample_tx
            .send(super::AudioChunk::EndOfStream)
            .expect("send eos");
        let captured = stop_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("stop reply");

        cmd_tx.send(Cmd::Shutdown).expect("send shutdown");
        let _ = done_rx.recv_timeout(Duration::from_secs(2));
        drop(sample_tx);
        worker.join().expect("join consumer");

        assert!(!captured.stt_samples.is_empty(), "no audio captured");

        let activity = seen.lock().unwrap().clone();
        assert!(
            !activity.is_empty(),
            "consumer never reported speech activity"
        );
        assert!(
            activity.iter().any(|a| a.speaking),
            "speech activity never reported speaking"
        );
        let peak = activity.iter().map(|a| a.speech_ms).max().unwrap_or(0);
        assert!(
            peak >= 800,
            "expected ~1s of speech from 1s of audio, got {peak}ms"
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn run_consumer(
    in_sample_rate: u32,
    in_sample_format: cpal::SampleFormat,
    vad: Option<VadConfig>,
    sample_rx: mpsc::Receiver<AudioChunk>,
    cmd_rx: mpsc::Receiver<Cmd>,
    level_cb: Option<Arc<dyn Fn(Vec<f32>) + Send + Sync + 'static>>,
    audio_cb: Option<AudioFrameCallback>,
    speech_cb: Option<SpeechActivityCallback>,
    speech_ms: Arc<AtomicU64>,
    stop_flag: Arc<AtomicBool>,
    stream_running_at: Instant,
) {
    let frame_samples = vad.as_ref().map_or(
        (constants::WHISPER_SAMPLE_RATE * 30 / 1000) as usize,
        |config| config.frame_samples,
    );
    let frame_duration =
        Duration::from_secs_f64(frame_samples as f64 / constants::WHISPER_SAMPLE_RATE as f64);
    let mut frame_resampler = FrameResampler::new(
        in_sample_rate as usize,
        constants::WHISPER_SAMPLE_RATE as usize,
        frame_duration,
    );

    let frame_ms = (frame_samples as u64 * 1000) / constants::WHISPER_SAMPLE_RATE as u64;
    let onset_frames = vad.as_ref().map_or_else(
        || vad::frames_for_duration_ms(vad::VAD_ONSET_MS, frame_samples),
        |v| v.onset_frames,
    );

    let mut processed_samples = Vec::<f32>::new();
    let mut raw_captured_samples = Vec::<f32>::new();
    let mut recording = false;
    let mut vad_policy = VadPolicy::Offline;
    // Reset on every Cmd::Start, which also supplies the session pause tolerance.
    let mut speech_clock = SpeechClock::new(onset_frames, 0, frame_ms, speech_ms);
    // Per-session count of VAD failures, so a broken detector is reported once
    // rather than per frame.
    let mut vad_errors: u64 = 0;

    // ---------- latency instrumentation ---------------------------------- //
    // First-chunk arrival exposes the play()->samples-flowing gap; the
    // first-captured log confirms capture begins with the chunk in flight
    // when Cmd::Start lands.
    let mut first_chunk_logged = false;
    let mut awaiting_first_captured_chunk: Option<Instant> = None;
    let mut capture_ready_tx: Option<mpsc::Sender<()>> = None;

    // ---------- spectrum visualisation setup ---------------------------- //
    const BUCKETS: usize = 16;
    // Scale the FFT window to the device sample rate so the analysis window
    // (~33 ms) and frequency resolution (~30 Hz/bin) stay roughly constant
    // across devices. A fixed 512-sample window collapses the low vocal
    // buckets onto a single bin at 48 kHz (e.g. built-in laptop mics), and
    // would stutter at ~4-8 updates/sec on an 8-16 kHz Bluetooth headset.
    // Targets: 48 kHz -> 2048, 16 kHz -> 512, 8 kHz -> 256.
    let target_window = (f64::from(in_sample_rate) / 30.0).round() as usize;
    let window_size = [256usize, 512, 1024, 2048]
        .into_iter()
        .min_by_key(|w| w.abs_diff(target_window))
        .unwrap();
    let mut visualizer = AudioVisualiser::new(
        in_sample_rate,
        window_size,
        BUCKETS,
        400.0,  // vocal_min_hz
        4000.0, // vocal_max_hz
    );

    #[allow(clippy::too_many_arguments)]
    fn handle_frame(
        samples: &[f32],
        recording: bool,
        vad_policy: VadPolicy,
        vad: &Option<VadConfig>,
        audio_cb: &Option<AudioFrameCallback>,
        speech_clock: &mut SpeechClock,
        speech_cb: &Option<SpeechActivityCallback>,
        vad_errors: &mut u64,
        out_buf: &mut Vec<f32>,
    ) {
        if !recording {
            return;
        }

        let mut emit = |buf: &[f32]| {
            out_buf.extend_from_slice(buf);
            if let Some(cb) = audio_cb {
                cb(buf);
            }
        };

        // The raw per-frame verdict, which drives the speech clock. With VAD off
        // every frame is kept, so every frame counts as speech and the clock
        // degrades into a wall clock — the UI needs no separate case for it.
        let voiced = if vad_policy == VadPolicy::Disabled {
            emit(samples);
            true
        } else if let Some(cfg) = vad {
            let mut det = cfg.detector.lock().unwrap();
            // A detector error means "keep this audio" — losing speech is worse
            // than keeping silence. But it must never pass unnoticed: a VAD that
            // fails on every frame looks exactly like a VAD that is switched
            // off, which is how a model/wrapper mismatch once went unseen for a
            // long time. Log the first failure of each session loudly.
            let decision = match det.push_frame(samples) {
                Ok(frame) => frame,
                Err(e) => {
                    if *vad_errors == 0 {
                        log::error!(
                            "VAD failed on a frame; passing audio through unfiltered                              for the rest of this recording: {e}"
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
            // Deliberately not "did push_frame return Speech": that stays true
            // through the hangover tail. See SpeechClock's docs.
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

    // Poll commands even when a disconnected device stops producing samples
    // without closing its CoreAudio stream.
    loop {
        let mut pending = match sample_rx.recv_timeout(Duration::from_millis(50)) {
            Ok(chunk) => Some(chunk),
            Err(mpsc::RecvTimeoutError::Timeout) => None,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };

        // Handle pending commands BEFORE the in-flight chunk so a Start
        // captures it. Commands used to be polled after processing, which
        // silently dropped one buffer period of audio (~10ms built-in, up to
        // ~100ms on Bluetooth) at every recording start.
        while let Ok(cmd) = cmd_rx.try_recv() {
            match cmd {
                Cmd::Start(policy, pause_hold_ms, sent_at, ready_tx) => {
                    log::debug!(
                        "Cmd::Start processed {:?} after send; capture begins with {} chunk",
                        sent_at.elapsed(),
                        if pending.is_some() {
                            "the in-flight"
                        } else {
                            "the next available"
                        }
                    );
                    awaiting_first_captured_chunk = Some(Instant::now());
                    capture_ready_tx = Some(ready_tx);
                    stop_flag.store(false, Ordering::Relaxed);
                    vad_policy = policy;
                    processed_samples.clear();
                    raw_captured_samples.clear();
                    recording = true;
                    visualizer.reset();
                    frame_resampler.reset();
                    speech_clock.reset(u64::from(pause_hold_ms));
                    vad_errors = 0;
                    // Reconfigure the single VAD engine for this session's policy
                    // and clear its smoothing + recurrent state before it sees
                    // any frames.
                    if vad_policy != VadPolicy::Disabled {
                        if let Some(cfg) = &vad {
                            let mut det = cfg.detector.lock().unwrap();
                            det.set_hangover_frames(cfg.hangover_for(vad_policy));
                            det.reset();
                        }
                    }
                }
                Cmd::Stop(reply_tx) => {
                    recording = false;
                    // If Stop was queued before the first chunk, dropping this
                    // sender prevents a stale ready UI event or start chime.
                    capture_ready_tx = None;
                    awaiting_first_captured_chunk = None;
                    stop_flag.store(true, Ordering::Relaxed);

                    // The chunk in hand arrived before the stop; it belongs to
                    // the recording, so feed it ahead of the drain below.
                    if let Some(AudioChunk::Samples(raw)) = pending.take() {
                        raw_captured_samples.extend_from_slice(&raw);
                        frame_resampler.push(&raw, &mut |frame: &[f32]| {
                            handle_frame(
                                frame,
                                true,
                                vad_policy,
                                &vad,
                                &audio_cb,
                                &mut speech_clock,
                                &speech_cb,
                                &mut vad_errors,
                                &mut processed_samples,
                            )
                        });
                    }

                    // Drain all remaining audio until the producer confirms end-of-stream.
                    // The cpal callback sees the stop flag, sends EndOfStream, and goes
                    // silent — guaranteeing every captured sample is in the channel
                    // ahead of the sentinel.
                    loop {
                        match sample_rx.recv_timeout(Duration::from_secs(2)) {
                            Ok(AudioChunk::Samples(remaining)) => {
                                raw_captured_samples.extend_from_slice(&remaining);
                                frame_resampler.push(&remaining, &mut |frame: &[f32]| {
                                    handle_frame(
                                        frame,
                                        true,
                                        vad_policy,
                                        &vad,
                                        &audio_cb,
                                        &mut speech_clock,
                                        &speech_cb,
                                        &mut vad_errors,
                                        &mut processed_samples,
                                    )
                                });
                            }
                            Ok(AudioChunk::EndOfStream) => break,
                            Err(_) => {
                                log::warn!("Timed out waiting for EndOfStream from audio callback");
                                break;
                            }
                        }
                    }

                    frame_resampler.finish(&mut |frame: &[f32]| {
                        handle_frame(
                            frame,
                            true,
                            vad_policy,
                            &vad,
                            &audio_cb,
                            &mut speech_clock,
                            &speech_cb,
                            &mut vad_errors,
                            &mut processed_samples,
                        )
                    });

                    // Diagnostic only: evidence for whether the VAD was
                    // still withholding tail audio when capture stopped.
                    // Suggestive, not conclusive, in either direction.
                    if vad_policy != VadPolicy::Disabled {
                        if let Some(cfg) = &vad {
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

                    let _ = reply_tx.send(RecordedAudio {
                        stt_samples: std::mem::take(&mut processed_samples),
                        raw_samples: std::mem::take(&mut raw_captured_samples),
                        native_sample_rate: in_sample_rate,
                        native_sample_format: in_sample_format,
                    });

                    // Resume the audio callback so the consumer loop can continue
                    // receiving chunks (important for always-on microphone mode).
                    stop_flag.store(false, Ordering::Relaxed);
                }
                Cmd::Shutdown => {
                    stop_flag.store(true, Ordering::Relaxed);
                    return;
                }
            }
        }

        let raw = match pending.take() {
            Some(AudioChunk::Samples(s)) => s,
            // EndOfStream, or the chunk was consumed by a Stop above.
            _ => continue,
        };

        let chunk_ms = raw.len() as f64 * 1000.0 / in_sample_rate as f64;
        if !first_chunk_logged {
            first_chunk_logged = true;
            log::debug!(
                "first audio chunk arrived {:?} after stream start ({:.1}ms of audio)",
                stream_running_at.elapsed(),
                chunk_ms
            );
        }

        // ---------- recording-time processing ---------------------------- //
        // In always-on mode the capture stream stays open continuously for
        // zero-latency start, so while idle (not recording) there is nothing to
        // do with a chunk: handle_frame returns early when not recording, which
        // means the resampled output would be discarded, and the level meter has
        // no idle consumer. Skip both the level-meter FFT and the resampler while
        // idle to avoid doing unnecessary work whose output is thrown away. Both
        // are reset on Cmd::Start (visualizer.reset() / frame_resampler.reset()),
        // so they resume cleanly the moment recording begins.
        if recording {
            raw_captured_samples.extend_from_slice(&raw);

            if let Some(buckets) = visualizer.feed(&raw) {
                if let Some(cb) = &level_cb {
                    cb(buckets);
                }
            }

            frame_resampler.push(&raw, &mut |frame: &[f32]| {
                handle_frame(
                    frame,
                    recording,
                    vad_policy,
                    &vad,
                    &audio_cb,
                    &mut speech_clock,
                    &speech_cb,
                    &mut vad_errors,
                    &mut processed_samples,
                )
            });
        }

        if recording {
            if let Some(started) = awaiting_first_captured_chunk.take() {
                log::debug!(
                    "first captured chunk ({:.1}ms of audio) processed {:?} after Cmd::Start",
                    chunk_ms,
                    started.elapsed()
                );
            }
            if let Some(ready_tx) = capture_ready_tx.take() {
                // Signal only after this chunk has passed through the visualizer
                // and resampler. Silence still counts: readiness means the host
                // is delivering samples, not that VAD has detected speech.
                let _ = ready_tx.send(());
            }
        }
    }
}
