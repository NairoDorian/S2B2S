use std::{
    io::Error,
    sync::{
        atomic::{AtomicBool, AtomicUsize, AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    time::{Duration, Instant},
};

use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Device, Sample, SizedSample,
};
use rtrb::{Consumer, Producer, RingBuffer};

use crate::audio_toolkit::{
    audio::{AudioVisualiser, FrameResampler, NoiseSuppressor},
    constants,
    vad::{self, VadFrame},
    VoiceActivityDetector,
};
use tauri::{Emitter, Manager};

/// Reopen-grace window after a VAD silence endpoint, per `endpoint_preset`.
/// While the grace is open, resumed speech reopens the same turn (revision+1)
/// instead of starting a new one; when it expires the turn finalizes.
/// (Two-tier endpointing without the SmartTurn classifier: the preset doubles
/// as the confidence tier — snappy 400ms / balanced 800ms / patient 2000ms.)
fn reopen_grace_ms_for_preset(preset: &str) -> u64 {
    match preset.to_ascii_lowercase().as_str() {
        "snappy" => 400,
        "patient" => 2000,
        _ => 800,
    }
}

enum Cmd {
    /// Begin capturing. Carries the send timestamp so the consumer can log how
    /// long the command sat in the channel, plus a one-shot first-sample acknowledgement.
    Start(VadPolicy, Instant, mpsc::Sender<()>),
    Stop(mpsc::Sender<Vec<f32>>),
    Shutdown,
}

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

pub struct AudioRecorder {
    device: Option<Device>,
    cmd_tx: Option<mpsc::Sender<Cmd>>,
    worker_handle: Option<std::thread::JoinHandle<()>>,
    vad: Option<VadConfig>,
    level_cb: Option<LevelCallback>,
    audio_cb: Option<AudioFrameCallback>,
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
    // S2B2S-specific fields
    pause_flag: Option<Arc<AtomicBool>>,
    noise_suppression_enabled: Option<Arc<AtomicBool>>,
    use_standalone_ns: bool,
    app_handle: Option<tauri::AppHandle>,
    continuous_mode: Option<Arc<AtomicBool>>,
    continuous_mode_paused: Option<Arc<AtomicBool>>,
    /// Silence frames (30 ms each) required to endpoint an utterance in
    /// continuous-voice mode. Mirrors `BrainConfig::endpoint_preset`
    /// (snappy/balanced/patient) so the setting applies without reopening
    /// the stream. Defaults to 40 frames (~1.2 s) when unset.
    endpoint_silence_frames: Option<Arc<AtomicUsize>>,
    wake_word_detector: Option<Arc<crate::wake_word::WakeWordDetector>>,
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
            selected_channel: None,
            config_cache: Arc::new(Mutex::new(None)),
            stream_error: Arc::new(AtomicBool::new(false)),
            pause_flag: None,
            noise_suppression_enabled: None,
            use_standalone_ns: false,
            app_handle: None,
            continuous_mode: None,
            continuous_mode_paused: None,
            endpoint_silence_frames: None,
            wake_word_detector: None,
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
    ) -> Self {
        let frame_samples = detector.frame_samples();
        assert!(frame_samples > 0, "VAD frame size must be non-zero");
        self.vad = Some(VadConfig {
            detector: Arc::new(Mutex::new(detector)),
            frame_samples,
            offline_hangover_frames,
            streaming_hangover_frames,
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

    // S2B2S-specific builder methods
    pub fn with_pause_flag(mut self, flag: Arc<AtomicBool>) -> Self {
        self.pause_flag = Some(flag);
        self
    }

    pub fn with_noise_suppression(mut self, flag: Arc<AtomicBool>, use_standalone: bool) -> Self {
        self.noise_suppression_enabled = Some(flag);
        self.use_standalone_ns = use_standalone;
        self
    }

    pub fn with_app_handle(mut self, app: tauri::AppHandle) -> Self {
        self.app_handle = Some(app);
        self
    }

    pub fn with_continuous_mode(
        mut self,
        enabled: Arc<AtomicBool>,
        paused: Arc<AtomicBool>,
    ) -> Self {
        self.continuous_mode = Some(enabled);
        self.continuous_mode_paused = Some(paused);
        self
    }

    pub fn with_endpoint_silence_frames(mut self, frames: Arc<AtomicUsize>) -> Self {
        self.endpoint_silence_frames = Some(frames);
        self
    }

    pub fn with_wake_word_detector(
        mut self,
        detector: Arc<crate::wake_word::WakeWordDetector>,
    ) -> Self {
        self.wake_word_detector = Some(detector);
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
        // Move the optional level callback into the worker thread
        let level_cb = self.level_cb.clone();
        // Move the optional real-time audio frame callback into the worker thread
        let audio_cb = self.audio_cb.clone();
        let selected_channel = self.selected_channel;
        let config_cache = Arc::clone(&self.config_cache);
        let stream_error = Arc::clone(&self.stream_error);
        // S2B2S-specific state
        let pause_flag = self.pause_flag.clone();
        let noise_suppression_enabled = self.noise_suppression_enabled.clone();
        let use_standalone_ns = self.use_standalone_ns;
        let app_handle = self.app_handle.clone();
        let continuous_mode = self.continuous_mode.clone();
        let continuous_mode_paused = self.continuous_mode_paused.clone();
        let endpoint_silence_frames = self.endpoint_silence_frames.clone();
        let wake_word_detector = self.wake_word_detector.clone();

        let worker = std::thread::spawn(move || {
            let transport = Arc::new(CaptureTransportState::default());
            let init_result = (|| -> Result<(cpal::Stream, u32, Consumer<f32>), String> {
                let config_started = Instant::now();
                let device_name = thread_device
                    .description()
                    .map(|desc| desc.name().to_string())
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

                // cpal 0.17: SampleRate is now a `u32` alias (no tuple field).
                let sample_rate = config.sample_rate();
                let channels = config.channels() as usize;

                log::info!(
                    "Using device: {}\nSample rate: {}\nChannels: {}\nFormat: {:?}",
                    thread_device
                        .description()
                        .map(|d| d.name().to_string())
                        .unwrap_or_else(|_| "Unknown".to_string()),
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

                Ok((stream, sample_rate, sample_consumer))
            })();

            match init_result {
                Ok((stream, sample_rate, sample_consumer)) => {
                    let _ = init_tx.send(Ok(()));
                    // Timestamp for the play()-returned -> first-samples gap the
                    // init handshake can't see (hardware dependent).
                    let stream_running_at = Instant::now();
                    let processor = CaptureProcessor::new(
                        sample_rate,
                        vad,
                        level_cb,
                        audio_cb,
                        stream_running_at,
                    )
                    .with_s2b2s(
                        pause_flag,
                        noise_suppression_enabled,
                        use_standalone_ns,
                        app_handle,
                        continuous_mode,
                        continuous_mode_paused,
                        endpoint_silence_frames,
                        wake_word_detector,
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
    pub fn start(
        &self,
        vad_policy: VadPolicy,
    ) -> Result<mpsc::Receiver<()>, Box<dyn std::error::Error>> {
        let tx = self
            .cmd_tx
            .as_ref()
            .ok_or_else(|| Error::other("Recorder is not open"))?;
        let (ready_tx, ready_rx) = mpsc::channel();
        tx.send(Cmd::Start(vad_policy, Instant::now(), ready_tx))?;
        Ok(ready_rx)
    }

    pub fn stop(&self) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
        let tx = self
            .cmd_tx
            .as_ref()
            .ok_or_else(|| Error::other("Recorder is not open"))?;
        let (resp_tx, resp_rx) = mpsc::channel();
        tx.send(Cmd::Stop(resp_tx))?;
        Ok(resp_rx.recv()?)
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
    ) -> Result<(cpal::Stream, Consumer<f32>), cpal::BuildStreamError>
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
                .expect("pre-filled audio ring is readable");
            chunk.commit_all();
        }

        // Resolve the effective channel to use. If the selected channel is
        // out of range for this device, fall back to averaging all channels.
        let use_channel = selected_channel.filter(|&channel| channel < channels);
        let callback_transport = Arc::clone(&transport);
        let stream_cb = move |data: &[T], _: &cpal::InputCallbackInfo| {
            Self::write_input_to_ring(
                data,
                channels,
                use_channel,
                &mut sample_producer,
                &callback_transport,
            );
        };

        let stream = device.build_input_stream(
            &config.clone().into(),
            stream_cb,
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

/// Route one 16 kHz frame through VAD to recording and live outputs.
/// Kept free-standing to permit disjoint borrows around resampler callbacks.
fn handle_frame(
    samples: &[f32],
    vad_policy: VadPolicy,
    vad: &Option<VadConfig>,
    audio_cb: &Option<AudioFrameCallback>,
    out_buf: &mut Vec<f32>,
) {
    let mut emit = |buf: &[f32]| {
        out_buf.extend_from_slice(buf);
        if let Some(cb) = audio_cb {
            cb(buf);
        }
    };

    if vad_policy == VadPolicy::Disabled {
        emit(samples);
        return;
    }

    if let Some(cfg) = vad {
        let mut detector = cfg.detector.lock().unwrap();
        match detector
            .push_frame(samples)
            .unwrap_or(VadFrame::Speech(samples))
        {
            VadFrame::Speech(buf) => emit(buf),
            VadFrame::Noise => {}
        }
    } else {
        emit(samples);
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
/// Ring transport stays outside to avoid conflicting borrows during drains.
struct CaptureProcessor {
    // ---- stream-scoped: fixed for the life of the input stream ---------- //
    in_sample_rate: u32,
    vad: Option<VadConfig>,
    level_cb: Option<LevelCallback>,
    audio_cb: Option<AudioFrameCallback>,
    stream_running_at: Instant,
    visualizer: AudioVisualiser,
    frame_resampler: FrameResampler,
    max_drain_samples: usize,
    first_chunk_logged: bool,

    // ---- S2B2S stream-scoped fields ------------------------------------- //
    pause_flag: Option<Arc<AtomicBool>>,
    noise_suppression_enabled: Option<Arc<AtomicBool>>,
    standalone_ns: Option<NoiseSuppressor>,
    app_handle: Option<tauri::AppHandle>,
    continuous_mode: Option<Arc<AtomicBool>>,
    continuous_mode_paused: Option<Arc<AtomicBool>>,
    endpoint_silence_frames: Option<Arc<AtomicUsize>>,
    wake_word_detector: Option<Arc<crate::wake_word::WakeWordDetector>>,
    tracker: Option<Arc<crate::speculative_turns::SpeculativeTurnTracker>>,

    // ---- continuous-mode runtime state ---------------------------------- //
    continuous_recording: bool,
    continuous_silence_frames: usize,
    pending_turn: Option<(u64, u32)>,
    pending_end: Option<Instant>,

    // ---- recording-scoped: reset by `begin_recording` ------------------- //
    vad_policy: VadPolicy,
    processed_samples: Vec<f32>,
    awaiting_first_captured_chunk: Option<Instant>,
    capture_ready_tx: Option<mpsc::Sender<()>>,
    total_dropped_samples: u64,
    overrun_warning_logged: bool,
    is_ptt_recording: bool,
}

impl CaptureProcessor {
    fn new(
        in_sample_rate: u32,
        vad: Option<VadConfig>,
        level_cb: Option<LevelCallback>,
        audio_cb: Option<AudioFrameCallback>,
        stream_running_at: Instant,
    ) -> Self {
        // Resample into frames sized for the active VAD backend (30 ms when
        // no detector is attached) so the detector never sees a partial frame.
        let frame_samples = vad.as_ref().map_or(
            (constants::WHISPER_SAMPLE_RATE * 30 / 1000) as usize,
            |config| config.frame_samples,
        );
        let frame_duration =
            Duration::from_secs_f64(frame_samples as f64 / constants::WHISPER_SAMPLE_RATE as f64);
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
            vad,
            level_cb,
            audio_cb,
            stream_running_at,
            visualizer,
            frame_resampler,
            max_drain_samples,
            first_chunk_logged: false,
            vad_policy: VadPolicy::Offline,
            processed_samples: Vec::new(),
            awaiting_first_captured_chunk: None,
            capture_ready_tx: None,
            total_dropped_samples: 0,
            overrun_warning_logged: false,
            is_ptt_recording: false,
            pause_flag: None,
            noise_suppression_enabled: None,
            standalone_ns: None,
            app_handle: None,
            continuous_mode: None,
            continuous_mode_paused: None,
            endpoint_silence_frames: None,
            wake_word_detector: None,
            tracker: None,
            continuous_recording: false,
            continuous_silence_frames: 0,
            pending_turn: None,
            pending_end: None,
        }
    }

    fn with_s2b2s(
        mut self,
        pause_flag: Option<Arc<AtomicBool>>,
        noise_suppression_enabled: Option<Arc<AtomicBool>>,
        use_standalone_ns: bool,
        app_handle: Option<tauri::AppHandle>,
        continuous_mode: Option<Arc<AtomicBool>>,
        continuous_mode_paused: Option<Arc<AtomicBool>>,
        endpoint_silence_frames: Option<Arc<AtomicUsize>>,
        wake_word_detector: Option<Arc<crate::wake_word::WakeWordDetector>>,
    ) -> Self {
        self.standalone_ns = if use_standalone_ns {
            NoiseSuppressor::new_16khz().ok()
        } else {
            None
        };
        self.tracker = app_handle.as_ref().and_then(|app| {
            app.try_state::<Arc<crate::speculative_turns::SpeculativeTurnTracker>>()
                .map(|state| state.inner().clone())
        });
        self.pause_flag = pause_flag;
        self.noise_suppression_enabled = noise_suppression_enabled;
        self.app_handle = app_handle;
        self.continuous_mode = continuous_mode;
        self.continuous_mode_paused = continuous_mode_paused;
        self.endpoint_silence_frames = endpoint_silence_frames;
        self.wake_word_detector = wake_word_detector;
        self
    }

    fn is_continuous_active(&self) -> bool {
        self.continuous_mode
            .as_ref()
            .map(|f| f.load(Ordering::Relaxed))
            .unwrap_or(false)
    }

    fn is_wake_word_active(&self) -> bool {
        self.wake_word_detector
            .as_ref()
            .map(|d| d.active.load(Ordering::SeqCst))
            .unwrap_or(false)
    }

    fn endpoint_frames(&self) -> usize {
        self.endpoint_silence_frames
            .as_ref()
            .map(|f| f.load(Ordering::Relaxed).max(1))
            .unwrap_or(40)
    }

    /// Reset per-recording state and arm the first-sample acknowledgement.
    fn begin_recording(&mut self, policy: VadPolicy, ready_tx: mpsc::Sender<()>) {
        self.awaiting_first_captured_chunk = Some(Instant::now());
        self.capture_ready_tx = Some(ready_tx);
        self.total_dropped_samples = 0;
        self.overrun_warning_logged = false;
        self.vad_policy = policy;
        self.processed_samples.clear();
        self.visualizer.reset();
        self.frame_resampler.reset();
        self.is_ptt_recording = true;
        if policy != VadPolicy::Disabled {
            if let Some(cfg) = &self.vad {
                let mut detector = cfg.detector.lock().unwrap();
                detector.set_hangover_frames(cfg.hangover_for(policy));
                detector.reset();
            }
        }
    }

    /// Drop a pending first-sample acknowledgement. If Stop was queued before
    /// the first chunk, this prevents a stale ready UI event or start chime.
    fn cancel_ready_signal(&mut self) {
        self.capture_ready_tx = None;
        self.awaiting_first_captured_chunk = None;
    }

    /// Drain up to one bounded chunk from the ring. Returns the number of
    /// samples consumed so callers can tell an empty ring from a busy one.
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

        // Check pause state (S2B2S continuous mode)
        let is_paused = self
            .pause_flag
            .as_ref()
            .map(|f| f.load(Ordering::Relaxed))
            .unwrap_or(false);

        let is_continuous = self
            .continuous_mode
            .as_ref()
            .map(|f| f.load(Ordering::Relaxed))
            .unwrap_or(false);

        let has_wake_word = self
            .wake_word_detector
            .as_ref()
            .map(|d| d.active.load(Ordering::SeqCst))
            .unwrap_or(false);

        let is_capturing = disposition == ChunkDisposition::Capture;

        if !is_capturing && !is_continuous && !has_wake_word {
            return;
        }

        let visualizer_input = if is_paused {
            vec![0.0f32; raw.len()]
        } else {
            raw.to_vec()
        };
        if let Some(buckets) = self.visualizer.feed(&visualizer_input) {
            if let Some(callback) = &self.level_cb {
                callback(buckets);
            }
        }

        if is_paused {
            return;
        }

        let is_continuous_paused = self
            .continuous_mode_paused
            .as_ref()
            .map(|f| f.load(Ordering::Relaxed))
            .unwrap_or(false);

        if is_continuous && is_continuous_paused {
            self.processed_samples.clear();
            self.continuous_recording = false;
            self.continuous_silence_frames = 0;
            self.pending_turn = None;
            self.pending_end = None;
            if let Some(cfg) = &self.vad {
                cfg.detector.lock().unwrap().reset();
            }
        }

        let vad_policy = self.vad_policy;
        let is_ptt = self.is_ptt_recording;
        let endpoint_frames = self.endpoint_frames();
        let ns_on = self
            .noise_suppression_enabled
            .as_ref()
            .map(|f| f.load(Ordering::Relaxed))
            .unwrap_or(false);

        let standalone_ns = &mut self.standalone_ns;
        let wake_word_detector = &self.wake_word_detector;
        let vad = &self.vad;
        let audio_cb = &self.audio_cb;
        let processed_samples = &mut self.processed_samples;
        let app_handle = &self.app_handle;
        let tracker = &self.tracker;
        let continuous_recording = &mut self.continuous_recording;
        let continuous_silence_frames = &mut self.continuous_silence_frames;
        let pending_turn = &mut self.pending_turn;
        let pending_end = &mut self.pending_end;

        self.frame_resampler.push(raw, |frame: &[f32]| {
            let processed: Vec<f32> = if ns_on {
                if let Some(ns) = standalone_ns {
                    let (denoised, _) = ns.process_16khz_frame(frame);
                    denoised
                } else {
                    frame.to_vec()
                }
            } else {
                frame.to_vec()
            };

            if let Some(detector) = wake_word_detector {
                if detector.active.load(Ordering::SeqCst) {
                    detector.feed_audio(&processed);
                }
            }

            if is_continuous {
                if is_continuous_paused {
                    if let Some(cfg) = vad {
                        let mut det = cfg.detector.lock().unwrap();
                        if det.push_frame(&processed).unwrap_or(VadFrame::Noise).is_speech() {
                            if !*continuous_recording {
                                *continuous_recording = true;
                                *continuous_silence_frames = 0;
                                log::info!("[ContinuousVoice Barge-in] VAD detected user speech start during TTS!");
                                if let Some(app) = app_handle {
                                    let _ = app.emit("continuous-voice:speech-started", ());
                                }
                            }
                        } else if *continuous_recording {
                            *continuous_silence_frames += 1;
                            if *continuous_silence_frames >= endpoint_frames {
                                *continuous_recording = false;
                                *continuous_silence_frames = 0;
                                if let Some(cfg2) = vad {
                                    cfg2.detector.lock().unwrap().reset();
                                }
                            }
                        }
                    }
                } else if let Some(cfg) = vad {
                    let mut det = cfg.detector.lock().unwrap();
                    match det.push_frame(&processed).unwrap_or(VadFrame::Noise) {
                        VadFrame::Speech(buf) => {
                            if !*continuous_recording {
                                *continuous_recording = true;
                                processed_samples.clear();
                                processed_samples.extend_from_slice(buf);
                                let (turn_id, revision) = tracker
                                    .as_ref()
                                    .map(|t| t.new_turn())
                                    .unwrap_or((0, 0));
                                *pending_turn = Some((turn_id, revision));
                                log::info!(
                                    "[ContinuousVoice] VAD detected speech START! (turn {turn_id}, rev {revision})"
                                );
                                if let Some(app) = app_handle {
                                    if let Some(tm) = app
                                        .try_state::<Arc<crate::managers::transcription::TranscriptionManager>>()
                                        .map(|s| s.inner().clone())
                                    {
                                        let settings = crate::settings::get_settings(app);
                                        let supports_streaming = app
                                            .try_state::<Arc<crate::managers::model::ModelManager>>()
                                            .and_then(|mm| {
                                                mm.get_model_info(&settings.selected_model)
                                                    .map(|m| m.supports_streaming)
                                            })
                                            .unwrap_or(false);
                                        if supports_streaming {
                                            tm.start_stream();
                                        }
                                    }
                                    let _ = app.emit(
                                        "continuous-voice:speech-started",
                                        serde_json::json!({
                                            "turn_id": turn_id,
                                            "revision": revision,
                                        }),
                                    );
                                }
                            } else {
                                processed_samples.extend_from_slice(buf);
                                let reopen = pending_end
                                    .as_ref()
                                    .and_then(|deadline| {
                                        (Instant::now() < *deadline).then_some(())
                                    })
                                    .and_then(|()| {
                                        pending_turn.as_ref().map(|(turn_id, _)| *turn_id)
                                    });
                                if let Some(turn_id) = reopen {
                                    let revision = tracker
                                        .as_ref()
                                        .map(|t| t.reopen(turn_id))
                                        .unwrap_or(0);
                                    *pending_turn = Some((turn_id, revision));
                                    log::info!(
                                        "[ContinuousVoice] Turn reopened during grace (turn {turn_id}, rev {revision})"
                                    );
                                }
                            }
                            if let Some(cb) = audio_cb {
                                cb(buf);
                            }
                            *pending_end = None;
                            *continuous_silence_frames = 0;
                        }
                        VadFrame::Noise => {
                            if *continuous_recording {
                                *continuous_silence_frames += 1;
                                if *continuous_silence_frames >= endpoint_frames {
                                    match pending_end {
                                        None => {
                                            let grace_ms = reopen_grace_ms_for_preset(
                                                &app_handle
                                                    .as_ref()
                                                    .map(|app| {
                                                        crate::settings::get_settings(app)
                                                            .brain
                                                            .endpoint_preset
                                                    })
                                                    .unwrap_or_default(),
                                            );
                                            *pending_end = Some(
                                                Instant::now() + Duration::from_millis(grace_ms),
                                            );
                                            log::info!(
                                                "[ContinuousVoice] Silence endpoint — reopen grace armed ({grace_ms}ms)"
                                            );
                                        }
                                        Some(deadline) if Instant::now() >= *deadline => {
                                            let (turn_id, revision) =
                                                pending_turn.take().unwrap_or((0, 0));
                                            *continuous_recording = false;
                                            let duration_secs =
                                                processed_samples.len() as f64 / 16000.0;
                                            log::info!(
                                                "[ContinuousVoice] VAD detected speech END! Captured {} samples ({:.2}s) — turn {turn_id}, rev {revision}",
                                                processed_samples.len(),
                                                duration_secs
                                            );
                                            if let Some(app) = app_handle {
                                                let _ = app.emit(
                                                    "continuous-voice:speech-ended",
                                                    serde_json::json!({
                                                        "turn_id": turn_id,
                                                        "revision": revision,
                                                    }),
                                                );
                                            }
                                            let samples = std::mem::take(processed_samples);
                                            if let Some(app) = app_handle {
                                                let app_clone = app.clone();
                                                std::thread::spawn(move || {
                                                    if let Err(e) =
                                                        crate::managers::continuous_voice::process_continuous_samples(
                                                            &app_clone,
                                                            samples,
                                                            turn_id,
                                                            revision,
                                                        )
                                                    {
                                                        log::error!(
                                                            "Error in continuous voice pipeline: {}",
                                                            e
                                                        );
                                                    }
                                                });
                                            }
                                            *continuous_silence_frames = 0;
                                            *pending_end = None;
                                        }
                                        Some(_) => {}
                                    }
                                }
                            }
                        }
                    }
                } else {
                    processed_samples.extend_from_slice(&processed);
                }
            } else if is_ptt && is_capturing {
                handle_frame(
                    &processed,
                    vad_policy,
                    vad,
                    audio_cb,
                    processed_samples,
                );
            }
        });

        if is_ptt && is_capturing {
            if let Some(started) = self.awaiting_first_captured_chunk.take() {
                log::debug!(
                    "first captured samples ({:.1}ms) processed {:?} after Cmd::Start",
                    chunk_ms,
                    started.elapsed()
                );
            }
            if let Some(ready_tx) = self.capture_ready_tx.take() {
                // Silence still counts: readiness means the host is delivering samples,
                // not that VAD has detected speech.
                let _ = ready_tx.send(());
            }
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
    fn finish_recording(&mut self) -> Vec<f32> {
        self.is_ptt_recording = false;
        let vad_policy = self.vad_policy;
        let ns_on = self
            .noise_suppression_enabled
            .as_ref()
            .map(|f| f.load(Ordering::Relaxed))
            .unwrap_or(false);
        let standalone_ns = &mut self.standalone_ns;
        let vad = &self.vad;
        let audio_cb = &self.audio_cb;
        let processed_samples = &mut self.processed_samples;

        self.frame_resampler.finish(|frame: &[f32]| {
            let processed: Vec<f32> = if ns_on {
                if let Some(ns) = standalone_ns {
                    let (denoised, _) = ns.process_16khz_frame(frame);
                    denoised
                } else {
                    frame.to_vec()
                }
            } else {
                frame.to_vec()
            };
            handle_frame(
                &processed,
                vad_policy,
                vad,
                audio_cb,
                processed_samples,
            )
        });

        // Diagnostic for VAD audio still withheld when capture stopped; it is
        // not conclusive in either direction.
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
        std::mem::take(&mut self.processed_samples)
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
        // Avoid sleeping with queued audio; check commands before each bounded
        // drain so Stop cannot sit behind a multi-second backlog.
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
                    Cmd::Start(policy, sent_at, ready_tx) => {
                        log::debug!(
                            "Cmd::Start processed {:?} after send; capture begins with {} samples",
                            sent_at.elapsed(),
                            if sample_consumer.slots() > 0 {
                                "the in-flight"
                            } else {
                                "the next available"
                            }
                        );
                        // Ignore overruns accumulated while the always-on stream
                        // was idle; only active-capture loss is relevant.
                        transport.overrun_samples.store(0, Ordering::Release);
                        processor.begin_recording(policy, ready_tx);
                        recording = true;
                    }
                    Cmd::Stop(reply_tx) => {
                        processor
                            .observe_overrun(transport.overrun_samples.swap(0, Ordering::AcqRel));
                        recording = false;
                        processor.cancel_ready_signal();

                        // Request a pause that forwards one boundary block, then drain
                        // all audio committed before the acknowledgement.
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
                            // Preserve the existing recovery model: finish this
                            // stop, then rebuild the stream on the next start.
                            stream_error.store(true, Ordering::Release);
                        }

                        // Everything still in the ring, including the boundary
                        // block, belongs to this recording.
                        while processor.drain(&mut sample_consumer, ChunkDisposition::Capture) > 0 {
                        }

                        // Include drops that raced with the pause request.
                        processor
                            .observe_overrun(transport.overrun_samples.swap(0, Ordering::AcqRel));
                        let samples = processor.finish_recording();
                        if !pause_timed_out {
                            // Resume before stop() returns so an immediate recording
                            // cannot lose its first callback to this pause request.
                            transport.pause_acknowledged.store(false, Ordering::Relaxed);
                            transport.pause_requested.store(false, Ordering::Release);
                        }
                        let _ = reply_tx.send(samples);

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

        let is_continuous = processor.is_continuous_active();
        let has_wake_word = processor.is_wake_word_active();
        let disposition = if recording || is_continuous || has_wake_word {
            ChunkDisposition::Capture
        } else {
            ChunkDisposition::Discard
        };
        processor.drain(&mut sample_consumer, disposition);

        let overrun_samples = transport.overrun_samples.swap(0, Ordering::AcqRel);
        if recording {
            processor.observe_overrun(overrun_samples);
        }

        // The CPAL error callback only sets an atomic; log here and rebuild the
        // stream on the next start.
        if stream_error.load(Ordering::Acquire) && !stream_error_logged {
            log::error!("Microphone backend reported a stream error; it will be rebuilt");
            stream_error_logged = true;
        }
    }
}

#[cfg(test)]
mod tests;
