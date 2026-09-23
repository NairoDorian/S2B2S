use crate::audio_toolkit::audio::{DenoiseParams, default_input_endpoint};
use crate::audio_toolkit::{
    AudioRecorder, VadPolicy, VoiceActivityDetector, list_input_devices,
    vad::{
        EarshotVad, SmoothedVad, VAD_OFFLINE_HANGOVER_MS, VAD_ONSET_MS, VAD_PREFILL_MS,
        VAD_STREAMING_HANGOVER_MS, frames_for_duration_ms,
    },
};
use crate::helpers::clamshell;
use crate::managers::transcription::StreamRouter;
use crate::settings::{AppSettings, MicIdleTimeoutUnit, get_settings, write_settings};
use crate::utils;
use log::{debug, error, info, trace, warn};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager};
use tauri_specta::Event;

fn get_idle_timeout(app: &tauri::AppHandle) -> Duration {
    let settings = get_settings(app);
    if settings.mic_idle_infinite {
        return Duration::from_secs(u64::MAX);
    }
    let base = settings.mic_idle_timeout_value as u64;
    match settings.mic_idle_timeout_unit {
        MicIdleTimeoutUnit::Seconds => Duration::from_secs(base.max(1)),
        MicIdleTimeoutUnit::Minutes => Duration::from_secs(base.max(1) * 60),
    }
}

fn set_mute(mute: bool) {
    // Expected behavior:
    // - Windows: works on most systems using standard audio drivers.
    // - Linux: works on many systems (PipeWire, PulseAudio, ALSA),
    //   but some distros may lack the tools used.
    // - macOS: works on most standard setups via AppleScript.
    // If unsupported, fails silently.

    #[cfg(target_os = "windows")]
    {
        unsafe {
            use windows::Win32::{
                Media::Audio::{
                    Endpoints::IAudioEndpointVolume, IMMDeviceEnumerator, MMDeviceEnumerator,
                    eMultimedia, eRender,
                },
                System::Com::{CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx},
            };

            macro_rules! unwrap_or_return {
                ($expr:expr) => {
                    match $expr {
                        Ok(val) => val,
                        Err(_) => return,
                    }
                };
            }

            // Initialize the COM library for this thread.
            // If already initialized (e.g., by another library like Tauri), this does nothing.
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

            let all_devices: IMMDeviceEnumerator =
                unwrap_or_return!(CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL));
            let default_device =
                unwrap_or_return!(all_devices.GetDefaultAudioEndpoint(eRender, eMultimedia));
            let volume_interface = unwrap_or_return!(
                default_device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None)
            );

            let _ = volume_interface.SetMute(mute, std::ptr::null());
        }
    }

    #[cfg(target_os = "linux")]
    {
        use std::process::Command;

        let mute_val = if mute { "1" } else { "0" };
        let amixer_state = if mute { "mute" } else { "unmute" };

        // Try multiple backends to increase compatibility
        // 1. PipeWire (wpctl)
        if Command::new("wpctl")
            .args(["set-mute", "@DEFAULT_AUDIO_SINK@", mute_val])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return;
        }

        // 2. PulseAudio (pactl)
        if Command::new("pactl")
            .args(["set-sink-mute", "@DEFAULT_SINK@", mute_val])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return;
        }

        // 3. ALSA (amixer)
        let _ = Command::new("amixer")
            .args(["set", "Master", amixer_state])
            .output();
    }

    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        let script = format!(
            "set volume output muted {}",
            if mute { "true" } else { "false" }
        );
        let _ = Command::new("osascript").args(["-e", &script]).output();
    }
}

/// Reads the current system output mute state, mirroring `set_mute`'s backends.
///
/// Returns `Some(true)`/`Some(false)` when the state could be determined, or
/// `None` when it couldn't (unsupported platform, missing CLI tools, or an
/// error). Callers treat `None` as "unknown" and fall back to unmuting on stop,
/// so we never strand the user's audio muted.
#[cfg(target_os = "windows")]
fn get_mute() -> Option<bool> {
    unsafe {
        use windows::Win32::{
            Media::Audio::{
                Endpoints::IAudioEndpointVolume, IMMDeviceEnumerator, MMDeviceEnumerator,
                eMultimedia, eRender,
            },
            System::Com::{CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx},
        };

        // Matches set_mute: no-op if COM is already initialized on this thread.
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

        let all_devices: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let default_device = all_devices
            .GetDefaultAudioEndpoint(eRender, eMultimedia)
            .ok()?;
        let volume_interface = default_device
            .Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None)
            .ok()?;

        Some(volume_interface.GetMute().ok()?.as_bool())
    }
}

#[cfg(target_os = "linux")]
fn get_mute() -> Option<bool> {
    use std::process::Command;

    // 1. PipeWire (wpctl): prints "[MUTED]" in the volume line when muted.
    if let Ok(out) = Command::new("wpctl")
        .args(["get-volume", "@DEFAULT_AUDIO_SINK@"])
        .output()
    {
        if out.status.success() {
            return Some(String::from_utf8_lossy(&out.stdout).contains("[MUTED]"));
        }
    }

    // 2. PulseAudio (pactl): prints "Mute: yes" / "Mute: no".
    // Force LC_ALL=C so a localized system still emits the parseable English
    // "yes"/"no" instead of e.g. "ja"/"nein".
    if let Ok(out) = Command::new("pactl")
        .env("LC_ALL", "C")
        .args(["get-sink-mute", "@DEFAULT_SINK@"])
        .output()
    {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).to_lowercase();
            if s.contains("yes") {
                return Some(true);
            }
            if s.contains("no") {
                return Some(false);
            }
        }
    }

    // 3. ALSA (amixer): prints "[off]" for muted channels, "[on]" otherwise.
    // LC_ALL=C keeps the "[on]"/"[off]" tokens stable across locales.
    if let Ok(out) = Command::new("amixer")
        .env("LC_ALL", "C")
        .args(["get", "Master"])
        .output()
    {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout);
            if s.contains("[off]") {
                return Some(true);
            }
            if s.contains("[on]") {
                return Some(false);
            }
        }
    }

    None
}

#[cfg(target_os = "macos")]
fn get_mute() -> Option<bool> {
    use std::process::Command;

    let out = Command::new("osascript")
        .args(["-e", "output muted of (get volume settings)"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    match String::from_utf8_lossy(&out.stdout).trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
fn get_mute() -> Option<bool> {
    None
}

/// Restores the system mute state after our forced mute, given the state
/// captured just before we muted. We only ever need to unmute — and only when
/// the system was NOT already muted beforehand. If the prior state was muted,
/// we leave it muted (the user's own state). If it's unknown (`None`), we
/// default to unmuting so audio is never left stranded muted by us.
fn restore_mute(prev_muted: Option<bool>) {
    if prev_muted != Some(true) {
        set_mute(false);
    }
}

const WHISPER_SAMPLE_RATE: usize = crate::audio_toolkit::constants::WHISPER_SAMPLE_RATE as usize;

pub enum StopRecordingResult {
    Captured {
        recorded: crate::audio_toolkit::RecordedAudio,
        captured_sample_count: usize,
        sample_rate: u32,
    },
    Cancelled,
    NotActive,
    Failed(String),
}

/* ──────────────────────────────────────────────────────────────── */

#[derive(Clone, Debug)]
pub enum RecordingState {
    Idle,
    Recording { binding_id: String },
    Stopping,
}

#[derive(Clone, Debug)]
pub enum MicrophoneMode {
    AlwaysOn,
    OnDemand,
}

/// Tracks our forced "mute while recording" so we can restore the user's audio
/// exactly as it was. `did_mute` is true while our mute is active; `prev_muted`
/// is the system mute state captured just before we muted, used to decide
/// whether to unmute on stop (so a system that was already muted stays muted).
#[derive(Debug, Default, Clone, Copy)]
struct MuteState {
    did_mute: bool,
    prev_muted: Option<bool>,
}

/// The persisted microphone preference currently in effect. Clamshell and
/// regular selections are kept distinct so losing a clamshell-only device does
/// not erase the user's normal microphone preference.
enum DesiredMicrophone {
    Default,
    Selected(String),
    Clamshell(String),
}

/// Result of resolving the persisted preference to a live cpal device.
/// `device: None` when nothing resolved: no system default, or a named
/// microphone that enumeration did not find (or enumeration failed). The
/// recorder then opens the concrete default endpoint
/// (`default_input_endpoint`), never cpal's virtual default handle. The
/// unavailable name is populated only when enumeration succeeded and confirmed
/// that the user's regular selected microphone is missing.
struct MicrophoneResolution {
    device: Option<cpal::Device>,
    unavailable_selected_microphone: Option<String>,
}

/* ──────────────────────────────────────────────────────────────── */

/// Binding id the live VAD test records under.
pub const VAD_TEST_BINDING: &str = "vad_test";

/// Who wants the detector's per-frame reports (`VadTestEvent`): bit
/// [`VAD_REPORT_TEST`] for the live VAD test (Settings → Advanced), bit
/// [`VAD_REPORT_LIVE_FFT`] for the Live FFT page's voice-detection view.
/// Read on the audio consumer thread for every frame, so it is an atomic
/// rather than a lock; when it is zero the per-frame report costs one load
/// and returns.
static VAD_REPORT_FLAGS: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
pub const VAD_REPORT_TEST: u8 = 1;
pub const VAD_REPORT_LIVE_FFT: u8 = 2;

/// Turn one consumer's interest in `VadTestEvent`s on or off.
pub fn set_vad_reporting(flag: u8, on: bool) {
    if on {
        VAD_REPORT_FLAGS.fetch_or(flag, Ordering::Relaxed);
    } else {
        VAD_REPORT_FLAGS.fetch_and(!flag, Ordering::Relaxed);
    }
}

/// Every N-th frame is reported while the test runs: 16 ms frames at 1:2 give
/// ~31 updates per second, plenty for a meter and light on the webview.
const VAD_TEST_REPORT_EVERY: u64 = 2;

/// One update of the live VAD test: what the detector thought of the latest
/// microphone frame. Emitted only while the Advanced page's test or the Live
/// FFT page's voice-detection view runs, never during normal dictation.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct VadTestEvent {
    /// Raw 0–1 speech score before hysteresis (`None` with VAD disabled).
    pub score: Option<f32>,
    /// Verdict after hysteresis — the threshold the slider sets.
    pub voiced: bool,
    /// Whether the frame reached the recording after smoothing (prefill /
    /// hangover), i.e. what a model would have heard.
    pub kept: bool,
    /// Peak input level of the frame, 0–1.
    pub level: f32,
    /// RNNoise's own speech probability of the latest 10 ms frame while
    /// suppression is on; its gate threshold compares against it.
    pub denoise_prob: Option<f32>,
}

/// RNNoise's tunables from the persisted settings, clamped.
pub(crate) fn denoise_params(settings: &AppSettings) -> DenoiseParams {
    DenoiseParams {
        strength: settings.denoise_strength,
        vad_threshold: settings.denoise_vad_threshold,
        vad_grace_ms: settings.denoise_vad_grace_ms,
    }
    .normalized()
}

/// Speech-probability threshold the Earshot detector should be built with:
/// the user's persisted value, clamped to the sane range. Default lives in
/// `settings.rs` (`DEFAULT_VAD_THRESHOLD_EARSHOT`).
fn vad_threshold(settings: &AppSettings) -> f32 {
    crate::settings::clamp_vad_threshold(settings.vad_threshold_earshot)
}

fn create_audio_recorder(
    app_handle: &tauri::AppHandle,
    selected_channel: Option<u16>,
    stream_router: Arc<StreamRouter>,
) -> Result<AudioRecorder, anyhow::Error> {
    let settings = get_settings(app_handle);
    let threshold = vad_threshold(&settings);
    let detector: Box<dyn VoiceActivityDetector> = Box::new(
        EarshotVad::new(threshold)
            .map_err(|e| anyhow::anyhow!("Failed to create EarshotVad: {e}"))?,
    );

    let frame_samples = detector.frame_samples();
    let prefill_frames = frames_for_duration_ms(VAD_PREFILL_MS, frame_samples);
    let offline_hangover_frames = frames_for_duration_ms(VAD_OFFLINE_HANGOVER_MS, frame_samples);
    let streaming_hangover_frames =
        frames_for_duration_ms(VAD_STREAMING_HANGOVER_MS, frame_samples);
    let onset_frames = frames_for_duration_ms(VAD_ONSET_MS, frame_samples);
    let smoothed_vad = SmoothedVad::new(
        detector,
        prefill_frames,
        offline_hangover_frames,
        onset_frames,
    );

    info!(
        "Initialized Earshot VAD ({} samples/frame, threshold {:.2})",
        frame_samples, threshold
    );

    // Recorder with VAD, speech-activity and VAD-report callbacks, the Live FFT
    // and Multi-STT taps, and an audio-frame callback that feeds live streaming
    // via a shared `StreamRouter` (captured directly, not via Tauri state — see
    // its docs).
    let recorder = AudioRecorder::new()
        .map_err(|e| anyhow::anyhow!("Failed to create AudioRecorder: {}", e))?
        .with_vad(
            Box::new(smoothed_vad),
            offline_hangover_frames,
            streaming_hangover_frames,
            onset_frames,
        )
        .with_selected_channel(selected_channel)
        .with_denoise_enabled(settings.denoise_enabled)
        .with_denoise_params(denoise_params(&settings))
        .with_speech_activity_callback({
            let app_handle = app_handle.clone();
            move |activity| {
                utils::emit_speech_activity(&app_handle, activity);
            }
        })
        .with_vad_frame_callback({
            let app_handle = app_handle.clone();
            let counter = AtomicU64::new(0);
            move |report| {
                if VAD_REPORT_FLAGS.load(Ordering::Relaxed) == 0 {
                    return;
                }
                if !counter
                    .fetch_add(1, Ordering::Relaxed)
                    .is_multiple_of(VAD_TEST_REPORT_EVERY)
                {
                    return;
                }
                let _ = VadTestEvent {
                    score: report.score,
                    voiced: report.voiced,
                    kept: report.kept,
                    level: report.level,
                    denoise_prob: report.denoise_prob,
                }
                .emit(&app_handle);
            }
        })
        .with_analysis_sink(crate::live_fft::tap())
        // Mid-recording audio for the experimental Multi-STT streaming mode.
        // Always attached, inert unless a session holds it.
        .with_chunk_tap(crate::audio_toolkit::audio::chunk_tap())
        .with_audio_callback({
            let router = stream_router;
            move |frame| {
                router.feed(frame);
            }
        });

    Ok(recorder)
}

/* ──────────────────────────────────────────────────────────────── */

/// Per-session capture options beyond the VAD policy.
#[derive(Clone, Copy, Debug, Default)]
pub struct RecordingStartOptions {
    /// Force the native-rate raw tap on or off instead of following
    /// `save_raw_audio`.
    pub capture_raw_override: Option<bool>,
    /// Keep no audio at all: the frames still reach every live consumer
    /// (VAD test meter, Live FFT), they are just never accumulated, so such
    /// a session can run for hours without growing.
    pub discard_audio: bool,
}

/// One recording session's first-sample notification. Waiting on this never
/// blocks the shortcut coordinator: callers hand it to a dedicated worker.
pub struct RecordingReadiness {
    receiver: mpsc::Receiver<()>,
    generation: u64,
}

impl RecordingReadiness {
    pub fn wait(self) -> bool {
        self.receiver.recv().is_ok()
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }
}

#[derive(Clone)]
pub struct AudioRecordingManager {
    /// Never assign through this directly — route every write through
    /// `set_state()`, which keeps `recording_active` in sync.
    state: Arc<Mutex<RecordingState>>,
    mode: Arc<Mutex<MicrophoneMode>>,
    app_handle: tauri::AppHandle,

    recorder: Arc<Mutex<Option<AudioRecorder>>>,
    is_open: Arc<Mutex<bool>>,
    is_recording: Arc<Mutex<bool>>,
    mute_state: Arc<Mutex<MuteState>>,
    close_generation: Arc<AtomicU64>,
    cancel_generation: Arc<AtomicU64>,
    stream_router: Arc<StreamRouter>,
    /// Lock-free mirror of "is the state in {Recording, Stopping}",
    /// maintained by `set_state()`. The hot-path `is_recording()` reads THIS
    /// instead of the std `state` mutex, so a UI poll can no longer deadlock
    /// the main/webview thread when a worker holds `state` across a slow
    /// CoreAudio open/close.
    recording_active: Arc<AtomicBool>,
    /// Invalidates asynchronous first-sample UI/chime work when a recording is
    /// stopped or cancelled. This prevents a slow device from producing a late
    /// "ready" indication for a session the user already ended.
    capture_generation: Arc<AtomicU64>,
    /// Resolution of a *named* microphone (selected or clamshell) to its cpal
    /// device, cached so on-demand recording starts skip the full device
    /// enumeration (~40-110ms). Keyed by the resolved name, so a settings
    /// change misses naturally; cleared when an open fails (device unplugged)
    /// so the retry re-enumerates. The system-default case is never cached,
    /// so a changed default is picked up by the next open.
    cached_device: Arc<Mutex<Option<(String, cpal::Device)>>>,
    /// A microphone change that arrived during a recording; the stream is
    /// restarted on the new device once that recording ends (see
    /// `update_selected_device`).
    device_change_pending: Arc<AtomicBool>,
}

impl AudioRecordingManager {
    /* ---------- construction ------------------------------------------------ */

    pub fn new(
        app: &tauri::AppHandle,
        stream_router: Arc<StreamRouter>,
    ) -> Result<Self, anyhow::Error> {
        let settings = get_settings(app);
        let mode = if settings.always_on_microphone {
            MicrophoneMode::AlwaysOn
        } else {
            MicrophoneMode::OnDemand
        };

        let manager = Self {
            state: Arc::new(Mutex::new(RecordingState::Idle)),
            mode: Arc::new(Mutex::new(mode.clone())),
            app_handle: app.clone(),

            recorder: Arc::new(Mutex::new(None)),
            is_open: Arc::new(Mutex::new(false)),
            is_recording: Arc::new(Mutex::new(false)),
            mute_state: Arc::new(Mutex::new(MuteState::default())),
            close_generation: Arc::new(AtomicU64::new(0)),
            cancel_generation: Arc::new(AtomicU64::new(0)),
            stream_router,
            recording_active: Arc::new(AtomicBool::new(false)),
            capture_generation: Arc::new(AtomicU64::new(0)),
            cached_device: Arc::new(Mutex::new(None)),
            device_change_pending: Arc::new(AtomicBool::new(false)),
        };

        // An always-on microphone is opened by `open_if_always_on`, which the
        // app runs on a background thread: the open costs ~0.9 s on Windows
        // (device resolve + VAD + WASAPI stream) and used to stall everything
        // that followed it at startup — history, transcribe.cpp, shortcuts.

        Ok(manager)
    }

    /// Open the stream now when the mode is always-on. Safe to call from any
    /// thread; a failure is logged and the on-demand path opens it later.
    pub fn open_if_always_on(&self) {
        if !matches!(*self.mode.lock().unwrap(), MicrophoneMode::AlwaysOn) {
            return;
        }
        if let Err(e) = self.start_microphone_stream() {
            warn!("Always-on microphone could not be opened at startup: {e}");
        }
    }

    /* ---------- helper methods --------------------------------------------- */

    /// The persisted microphone preference currently in effect. Only runs the
    /// clamshell probe (an `ioreg` subprocess, ~10-20ms) when a clamshell
    /// microphone is actually configured.
    fn desired_microphone(&self, settings: &AppSettings) -> DesiredMicrophone {
        if let Some(clamshell_microphone) = &settings.clamshell_microphone {
            let clamshell_started = Instant::now();
            let is_clamshell = clamshell::is_clamshell().unwrap_or(false);
            debug!(
                "device resolve: clamshell_check={:?} (clamshell={})",
                clamshell_started.elapsed(),
                is_clamshell
            );
            if is_clamshell {
                return DesiredMicrophone::Clamshell(clamshell_microphone.clone());
            }
        }
        match &settings.selected_microphone {
            Some(name) => DesiredMicrophone::Selected(name.clone()),
            None => DesiredMicrophone::Default,
        }
    }

    pub fn invalidate_device_cache(&self) {
        *self.cached_device.lock().unwrap() = None;
    }

    fn resolve_microphone_device(&self, settings: &AppSettings) -> MicrophoneResolution {
        let desired = self.desired_microphone(settings);
        let (device_name, selected_microphone) = match desired {
            DesiredMicrophone::Default => {
                // The concrete endpoint, never cpal's virtual default handle
                // (see `default_input_endpoint`). Not cached on purpose: a
                // changed system default is picked up by the next open.
                let resolve_started = Instant::now();
                let endpoint = default_input_endpoint();
                debug!(
                    "device resolve: no mic configured -> system default {:?} ({:?})",
                    endpoint.as_ref().map(|e| e.name.as_str()),
                    resolve_started.elapsed()
                );
                return MicrophoneResolution {
                    device: endpoint.map(|e| e.device),
                    unavailable_selected_microphone: None,
                };
            }
            DesiredMicrophone::Selected(name) => (name.clone(), Some(name)),
            DesiredMicrophone::Clamshell(name) => (name, None),
        };

        // Cache hit: skip the full enumeration. A stale device (unplugged)
        // fails at open, where the caller invalidates and retries fresh.
        if let Some((cached_name, device)) = self.cached_device.lock().unwrap().as_ref()
            && *cached_name == device_name
        {
            debug!("device resolve: cache hit for '{}'", device_name);
            return MicrophoneResolution {
                device: Some(device.clone()),
                unavailable_selected_microphone: None,
            };
        }

        // Only report a selected microphone as unavailable when enumeration
        // itself succeeded. A backend enumeration error may be transient and
        // must not erase the user's persisted preference.
        let enumerate_started = Instant::now();
        let (device, enumeration_succeeded) = match list_input_devices() {
            Ok(devices) => (
                devices
                    .into_iter()
                    .find(|d| d.name == device_name)
                    .map(|d| d.device),
                true,
            ),
            Err(e) => {
                debug!("Failed to list devices, using default: {}", e);
                (None, false)
            }
        };
        debug!(
            "device resolve: enumerate={:?} (found={})",
            enumerate_started.elapsed(),
            device.is_some()
        );
        if let Some(d) = &device {
            *self.cached_device.lock().unwrap() = Some((device_name, d.clone()));
        }

        let unavailable_selected_microphone = if enumeration_succeeded && device.is_none() {
            selected_microphone
        } else {
            None
        };
        MicrophoneResolution {
            device,
            unavailable_selected_microphone,
        }
    }

    /// Keep persisted settings and the UI aligned with a successful runtime
    /// fallback. Re-read first so recovery cannot clear a microphone the user
    /// selected concurrently while the stream was being rebuilt.
    fn persist_default_microphone_after_fallback(&self, unavailable_name: &str) {
        let mut settings = get_settings(&self.app_handle);
        if settings.selected_microphone.as_deref() != Some(unavailable_name) {
            return;
        }

        settings.selected_microphone = None;
        write_settings(&self.app_handle, settings);
        let _ = self.app_handle.emit(
            "settings-changed",
            serde_json::json!({
                "setting": "selected_microphone",
                "value": "Default"
            }),
        );
    }

    fn schedule_lazy_close(&self, idle_timeout: Duration) {
        // Bumping the generation also invalidates any close that was scheduled
        // before the user switched the timeout to "infinite".
        let generation_id = self.close_generation.fetch_add(1, Ordering::SeqCst) + 1;
        if idle_timeout == Duration::from_secs(u64::MAX) {
            // "Never close": don't park a thread per recording that would
            // sleep forever.
            return;
        }
        let app = self.app_handle.clone();
        std::thread::spawn(move || {
            std::thread::sleep(idle_timeout);
            let rm = app.state::<Arc<AudioRecordingManager>>();
            // Hold state lock across the check AND close to serialize against
            // try_start_recording, preventing a race where the stream is closed
            // under an active recording.
            let state = rm.state.lock().unwrap();
            if rm.close_generation.load(Ordering::SeqCst) == generation_id
                && matches!(*state, RecordingState::Idle)
            {
                // stop_microphone_stream does not acquire the state lock,
                // so holding it here is safe (no deadlock).
                info!("Closing idle microphone stream after {:?}", idle_timeout);
                rm.stop_microphone_stream();
            }
        });
    }

    /* ---------- microphone life-cycle -------------------------------------- */

    /// Applies mute if mute_while_recording is enabled and stream is open.
    /// Snapshots the system's prior mute state first so `remove_mute` can
    /// restore it instead of unconditionally unmuting.
    pub fn apply_mute(&self) {
        let settings = get_settings(&self.app_handle);
        if !settings.mute_while_recording {
            return;
        }

        // Lock order: is_open before mute_state (matches stop_microphone_stream).
        let is_open = self.is_open.lock().unwrap();
        let mut mute_guard = self.mute_state.lock().unwrap();
        // Already muted this session — don't re-snapshot, or a duplicate/late
        // apply would overwrite prev_muted with our own forced-muted state and
        // strand audio muted on stop.
        if mute_guard.did_mute {
            return;
        }
        if *is_open {
            mute_guard.prev_muted = get_mute();
            set_mute(true);
            mute_guard.did_mute = true;
            debug!("Mute applied (prev_muted={:?})", mute_guard.prev_muted);
        }
    }

    /// Removes mute if it was applied, restoring the system's prior mute state
    /// (a system already muted before recording stays muted).
    pub fn remove_mute(&self) {
        let mut mute_guard = self.mute_state.lock().unwrap();
        if mute_guard.did_mute {
            restore_mute(mute_guard.prev_muted);
            mute_guard.did_mute = false;
            debug!(
                "Mute removed (restored prev_muted={:?})",
                mute_guard.prev_muted
            );
        }
    }

    pub fn preload_vad(&self) -> Result<(), anyhow::Error> {
        let mut recorder_opt = self.recorder.lock().unwrap();
        if recorder_opt.is_none() {
            let settings = get_settings(&self.app_handle);
            info!("Preloading Earshot VAD...");
            *recorder_opt = Some(create_audio_recorder(
                &self.app_handle,
                settings.selected_channel,
                Arc::clone(&self.stream_router),
            )?);
            info!("Earshot VAD preloaded successfully");
        }
        Ok(())
    }

    pub fn start_microphone_stream(&self) -> Result<(), anyhow::Error> {
        let mut open_flag = self.is_open.lock().unwrap();
        if *open_flag {
            // `is_open` only records that we opened a stream at some point, not
            // that one is still running. If capture has since failed (mic
            // unplugged mid-session, USB dropout), rebuild it before the next
            // recording instead of handing the caller a stalled recorder.
            let needs_reopen = self
                .recorder
                .lock()
                .unwrap()
                .as_ref()
                .is_some_and(|rec| rec.needs_reopen());

            if !needs_reopen {
                // trace, not debug: with the aliveness check in
                // try_start_recording this now fires on every keypress in
                // always-on mode.
                trace!("Microphone stream already active");
                return Ok(());
            }

            warn!("Microphone stream is no longer running (device disconnected?); reopening");

            // Torn down inline rather than via stop_microphone_stream(), which
            // takes the `is_open` lock we are already holding.
            {
                let mut mute_guard = self.mute_state.lock().unwrap();
                if mute_guard.did_mute {
                    restore_mute(mute_guard.prev_muted);
                    mute_guard.did_mute = false;
                }
            }
            if let Some(rec) = self.recorder.lock().unwrap().as_mut() {
                let _ = rec.close();
            }
            *self.is_recording.lock().unwrap() = false;
            *open_flag = false;
            self.invalidate_device_cache();
            // Fall through to the same fresh resolution and fallback path used
            // when an on-demand stream opens after its device was unplugged.
        }

        let start_time = Instant::now();

        // Don't mute immediately - caller will handle muting after audio feedback.
        // The previous stream restored audio on close, so did_mute should already
        // be false here; if it somehow isn't, restore rather than just clearing the
        // flag, which would strand system audio muted.
        {
            let mut mute_guard = self.mute_state.lock().unwrap();
            if mute_guard.did_mute {
                restore_mute(mute_guard.prev_muted);
                mute_guard.did_mute = false;
            }
        }

        // Get the selected device from settings, considering clamshell mode.
        // No separate pre-check: the default case resolves the concrete
        // endpoint in resolve_microphone_device, and a machine with no input
        // devices at all fails inside open() with the same
        // "No input device found" error this used to check for.
        let settings = get_settings(&self.app_handle);
        let resolve_started = Instant::now();
        let mut resolution = self.resolve_microphone_device(&settings);
        let resolve_elapsed = resolve_started.elapsed();

        // Ensure VAD is loaded if it wasn't for whatever reason
        let vad_started = Instant::now();
        self.preload_vad()?;
        let vad_elapsed = vad_started.elapsed();

        let open_started = Instant::now();
        let mut recorder_opt = self.recorder.lock().unwrap();
        if let Some(rec) = recorder_opt.as_mut()
            && let Err(first_err) = rec.open(resolution.device.clone())
        {
            // A cached device or config may have gone stale (unplugged,
            // rate/format changed). Re-resolve from a fresh enumeration and
            // retry once before surfacing the error.
            warn!("Recorder open failed ({first_err}); re-resolving device and retrying once");
            self.invalidate_device_cache();
            resolution = self.resolve_microphone_device(&settings);
            rec.open(resolution.device.clone())
                .map_err(|e| anyhow::anyhow!("Failed to open recorder: {}", e))?;
        }
        debug!(
            "mic stream breakdown: device_resolve={:?} vad_ensure={:?} open={:?}",
            resolve_elapsed,
            vad_elapsed,
            open_started.elapsed()
        );
        drop(recorder_opt);

        *open_flag = true;
        if let Some(unavailable_name) = resolution.unavailable_selected_microphone {
            // Do this only after the default stream opened successfully. A
            // failed fallback must not erase the user's microphone preference.
            self.persist_default_microphone_after_fallback(&unavailable_name);
        }
        // This timing covers through cpal's stream.play() returning — i.e. the
        // point cpal surfaces as "stream running." It does NOT guarantee the
        // host audio device is producing samples yet; the first input callback
        // fires asynchronously one buffer period later (hardware dependent,
        // typically ~10–200ms on macOS, longer on Bluetooth/USB).
        info!(
            "Microphone stream initialized in {:?}",
            start_time.elapsed()
        );
        Ok(())
    }

    pub fn stop_microphone_stream(&self) {
        let mut open_flag = self.is_open.lock().unwrap();
        if !*open_flag {
            return;
        }

        {
            let mut mute_guard = self.mute_state.lock().unwrap();
            if mute_guard.did_mute {
                restore_mute(mute_guard.prev_muted);
            }
            mute_guard.did_mute = false;
        }

        if let Some(rec) = self.recorder.lock().unwrap().as_mut() {
            // If still recording, stop first.
            if *self.is_recording.lock().unwrap() {
                let _ = rec.stop();
                *self.is_recording.lock().unwrap() = false;
            }
            let _ = rec.close();
        }

        *open_flag = false;
        debug!("Microphone stream stopped");
    }

    /* ---------- mode switching --------------------------------------------- */

    pub fn update_mode(&self, new_mode: MicrophoneMode) -> Result<(), anyhow::Error> {
        let cur_mode = self.mode.lock().unwrap().clone();

        match (cur_mode, &new_mode) {
            (MicrophoneMode::AlwaysOn, MicrophoneMode::OnDemand) => {
                if matches!(*self.state.lock().unwrap(), RecordingState::Idle) {
                    self.close_generation.fetch_add(1, Ordering::SeqCst);
                    self.stop_microphone_stream();
                }
            }
            (MicrophoneMode::OnDemand, MicrophoneMode::AlwaysOn) => {
                self.close_generation.fetch_add(1, Ordering::SeqCst);
                self.start_microphone_stream()?;
            }
            _ => {}
        }

        *self.mode.lock().unwrap() = new_mode;
        Ok(())
    }

    /* ---------- recording --------------------------------------------------- */

    /// The one place `state` is written. Derives `recording_active` (the
    /// lock-free mirror read by `is_recording()`) from the new value itself,
    /// so the two can never drift: a new `RecordingState` variant only needs
    /// its active-set membership decided here, once.
    fn set_state(&self, guard: &mut RecordingState, new_state: RecordingState) {
        *guard = new_state;
        self.recording_active.store(
            matches!(
                *guard,
                RecordingState::Recording { .. } | RecordingState::Stopping
            ),
            Ordering::SeqCst,
        );
    }

    pub fn try_start_recording(
        &self,
        binding_id: &str,
        vad_policy: VadPolicy,
    ) -> Result<RecordingReadiness, String> {
        self.try_start_recording_with_raw(binding_id, vad_policy, None)
    }

    /// Like [`Self::try_start_recording`], but lets the caller force the
    /// native-rate raw tap on or off instead of following `save_raw_audio`.
    /// Live Mode ties it to its own `save_audio` option (raw audio for its chunk
    /// files), whatever the history setting says.
    pub fn try_start_recording_with_raw(
        &self,
        binding_id: &str,
        vad_policy: VadPolicy,
        capture_raw_override: Option<bool>,
    ) -> Result<RecordingReadiness, String> {
        self.try_start_recording_with_options(
            binding_id,
            vad_policy,
            RecordingStartOptions {
                capture_raw_override,
                discard_audio: false,
            },
        )
    }

    /// The general form: raw-tap override plus `discard_audio` for sessions
    /// that only feed live consumers (VAD test, Live FFT) and must not grow.
    pub fn try_start_recording_with_options(
        &self,
        binding_id: &str,
        vad_policy: VadPolicy,
        options: RecordingStartOptions,
    ) -> Result<RecordingReadiness, String> {
        let capture_raw_override = options.capture_raw_override;
        let mut state = self.state.lock().unwrap();

        if let RecordingState::Idle = *state {
            // Cancel any pending lazy close (no-op in always-on mode, where
            // closes are never scheduled).
            self.close_generation.fetch_add(1, Ordering::SeqCst);
            // Opens the stream in on-demand mode. In always-on mode the stream
            // is normally already open and this is a cheap aliveness check —
            // but if the capture worker died (device disconnect), it rebuilds
            // the stream instead of leaving every subsequent start wedged on
            // "Recorder not available".
            if let Err(e) = self.start_microphone_stream() {
                let msg = format!("{e}");
                error!("Failed to open microphone stream: {msg}");
                return Err(msg);
            }

            // Read once per session rather than per frame: the speech clock
            // keeps the tolerance it was started with for the whole recording.
            let session_settings = get_settings(&self.app_handle);
            let pause_hold_ms = session_settings.speech_pause_hold_ms;
            // Only accumulate the native-rate raw tap when the user asked to
            // save it; at 48 kHz float it is ~11 MB per minute otherwise wasted.
            let capture_raw = capture_raw_override.unwrap_or(session_settings.save_raw_audio);
            if let Some(rec) = self.recorder.lock().unwrap().as_ref() {
                match rec.start_with_options(
                    vad_policy,
                    pause_hold_ms,
                    capture_raw,
                    options.discard_audio,
                ) {
                    Ok(receiver) => {
                        let generation = self.capture_generation.fetch_add(1, Ordering::AcqRel) + 1;
                        *self.is_recording.lock().unwrap() = true;
                        self.set_state(
                            &mut state,
                            RecordingState::Recording {
                                binding_id: binding_id.to_string(),
                            },
                        );
                        debug!("Recording requested for binding {binding_id}");
                        return Ok(RecordingReadiness {
                            receiver,
                            generation,
                        });
                    }
                    Err(error) => return Err(format!("Failed to start recorder: {error}")),
                }
            }
            Err("Recorder not available".to_string())
        } else {
            Err("Already recording".to_string())
        }
    }

    /// Apply a new speech threshold to the live detector. Takes effect on the
    /// next frame — also mid-recording, which is what lets the slider be tuned
    /// while the live VAD test runs. A recorder built later reads the
    /// persisted setting itself, so nothing needs doing when none exists yet.
    pub fn set_vad_threshold(&self, threshold: f32) {
        let threshold = crate::settings::clamp_vad_threshold(threshold);
        if let Some(rec) = self.recorder.lock().unwrap().as_ref() {
            rec.set_vad_threshold(threshold);
            info!("VAD threshold set to {threshold:.2}");
        }
    }

    /// Push RNNoise's strength / gate to the live recorder; applies from
    /// the next chunk. A recorder built later reads the persisted settings.
    pub fn set_denoise_params(&self, params: DenoiseParams) {
        if let Some(rec) = self.recorder.lock().unwrap().as_ref() {
            rec.set_denoise_params(params);
            info!(
                "Noise suppression: strength {:.0}%, gate {}, grace {} ms",
                params.strength * 100.0,
                if params.vad_threshold > 0.0 {
                    format!("at {:.0}%", params.vad_threshold * 100.0)
                } else {
                    "off".to_string()
                },
                params.vad_grace_ms
            );
        }
    }

    /// Turn RNNoise suppression on or off on the live recorder. Applies from
    /// the next chunk, mid-recording included, so the live VAD test shows the
    /// effect immediately. A recorder built later reads the persisted setting.
    pub fn set_denoise_enabled(&self, enabled: bool) {
        if let Some(rec) = self.recorder.lock().unwrap().as_ref() {
            rec.set_denoise_enabled(enabled);
            info!(
                "Noise suppression {}",
                if enabled { "enabled" } else { "disabled" }
            );
        }
    }

    /// Start the live VAD test: open the microphone as a normal recording
    /// under the `vad_test` binding and stream a [`VadTestEvent`] per reported
    /// frame. No model is loaded and nothing is transcribed or saved — the
    /// captured audio is discarded on stop. While it runs, the transcription
    /// hotkeys get "Already recording", exactly like Live Mode.
    pub fn start_vad_test(&self) -> Result<(), String> {
        // Nothing is kept: the audio would otherwise accumulate for the
        // whole test (up to five minutes) with no reader.
        let readiness = self.try_start_recording_with_options(
            VAD_TEST_BINDING,
            VadPolicy::Streaming,
            RecordingStartOptions {
                capture_raw_override: Some(false),
                discard_audio: true,
            },
        )?;
        // The first-sample notification is only needed by the chime path.
        drop(readiness);
        set_vad_reporting(VAD_REPORT_TEST, true);
        info!("Live VAD test started");
        Ok(())
    }

    /// Stop the live VAD test and discard its audio. A no-op when the test is
    /// not running (including when the cancel hotkey already ended it).
    pub fn stop_vad_test(&self) {
        set_vad_reporting(VAD_REPORT_TEST, false);
        if self.cancel_recording_if_binding(VAD_TEST_BINDING) {
            info!("Live VAD test stopped");
        }
    }

    /// Cancel the active recording only if it belongs to `binding_id`: a
    /// page ending its own session must never cancel a dictation that
    /// started in the meantime. Returns whether anything was cancelled.
    pub fn cancel_recording_if_binding(&self, binding_id: &str) -> bool {
        let is_ours = matches!(
            &*self.state.lock().unwrap(),
            RecordingState::Recording { binding_id: active } if active == binding_id
        );
        if is_ours {
            self.cancel_recording();
        }
        is_ours
    }

    /// Whether the active recording belongs to `binding_id`.
    pub fn is_recording_under(&self, binding_id: &str) -> bool {
        matches!(
            &*self.state.lock().unwrap(),
            RecordingState::Recording { binding_id: active } if active == binding_id
        )
    }

    /// Whether the live VAD test currently owns the recorder (the Live FFT
    /// page's voice-detection view records under its own binding and must
    /// not be mistaken for it).
    pub fn is_vad_test_running(&self) -> bool {
        VAD_REPORT_FLAGS.load(Ordering::Relaxed) & VAD_REPORT_TEST != 0
            && self.is_recording_under(VAD_TEST_BINDING)
    }

    pub fn update_selected_device(&self) -> Result<(), anyhow::Error> {
        // Device settings changed; re-enumerate the device and restart capture.
        self.invalidate_device_cache();
        // Serialize against recording start/stop, like update_selected_channel:
        // restarting an active capture would discard its samples and leave the
        // manager's recording state out of sync with the new recorder. The
        // setting is already persisted; the restart waits for the recording to
        // end (`apply_pending_device_change`), and any later open resolves the
        // new device anyway.
        let state = self.state.lock().unwrap();
        if !matches!(*state, RecordingState::Idle) {
            self.device_change_pending.store(true, Ordering::SeqCst);
            debug!("Microphone changed during a recording; switching when it ends");
            return Ok(());
        }
        self.device_change_pending.store(false, Ordering::SeqCst);
        let was_open = *self.is_open.lock().unwrap();
        if was_open {
            self.close_generation.fetch_add(1, Ordering::SeqCst);
            self.stop_microphone_stream();
            self.start_microphone_stream()?;
            // Bumping the generation cancelled any pending lazy close. In
            // on-demand mode an idle open stream is only ever a lazily-closing
            // one, so re-arm that close for the new device rather than leaving
            // the microphone (and the OS privacy indicator) on indefinitely.
            self.release_stream_after_recording();
        }
        drop(state);
        Ok(())
    }

    /// In on-demand mode, close the microphone once a recording is over
    /// (lazily when `lazy_stream_close` is on). Always-on streams stay open.
    fn release_stream_after_recording(&self) {
        if matches!(*self.mode.lock().unwrap(), MicrophoneMode::OnDemand) {
            if get_settings(&self.app_handle).lazy_stream_close {
                let timeout = get_idle_timeout(&self.app_handle);
                self.schedule_lazy_close(timeout);
            } else {
                self.stop_microphone_stream();
            }
        }
    }

    /// Apply a microphone change `update_selected_device` deferred because a
    /// recording was active. Runs on its own short-lived thread: the callers
    /// are the stop / cancel paths, which must not wait for a device reopen.
    fn apply_pending_device_change(&self) {
        if !self.device_change_pending.swap(false, Ordering::SeqCst) {
            return;
        }
        let app = self.app_handle.clone();
        std::thread::spawn(move || {
            let rm = app.state::<Arc<AudioRecordingManager>>();
            if let Err(e) = rm.update_selected_device() {
                warn!("Failed to switch to the newly selected microphone: {e}");
            }
        });
    }

    pub fn update_selected_channel(
        &self,
        selected_channel: Option<u16>,
    ) -> Result<(), anyhow::Error> {
        // Serialize against recording start/stop. Restarting an active capture
        // would discard its samples and leave the manager's recording state out
        // of sync with the new recorder.
        let state = self.state.lock().unwrap();
        if !matches!(*state, RecordingState::Idle) {
            return Err(anyhow::anyhow!(
                "Cannot change the input channel while recording"
            ));
        }

        let previous_channel = get_settings(&self.app_handle).selected_channel;
        let was_open = *self.is_open.lock().unwrap();
        if was_open {
            self.close_generation.fetch_add(1, Ordering::SeqCst);
            self.stop_microphone_stream();
        }
        if let Some(recorder) = self.recorder.lock().unwrap().as_mut() {
            recorder.set_selected_channel(selected_channel);
        }
        if was_open && let Err(error) = self.start_microphone_stream() {
            if let Some(recorder) = self.recorder.lock().unwrap().as_mut() {
                recorder.set_selected_channel(previous_channel);
            }
            return Err(error);
        }
        drop(state);
        Ok(())
    }

    /// Invalidate pending first-sample UI and audio-feedback work immediately.
    /// Called at the beginning of stop, before the slower capture drain starts.
    pub fn invalidate_recording_readiness(&self) {
        self.capture_generation.fetch_add(1, Ordering::AcqRel);
    }

    pub fn is_recording_readiness_current(&self, generation: u64) -> bool {
        self.capture_generation.load(Ordering::Acquire) == generation
    }

    pub fn cancel_generation(&self) -> u64 {
        self.cancel_generation.load(Ordering::Acquire)
    }

    pub fn was_cancelled_since(&self, generation: u64) -> bool {
        self.cancel_generation.load(Ordering::Acquire) != generation
    }

    /// Milliseconds of speech the VAD measured in the most recent recording.
    /// Valid once [`Self::stop_recording`] has returned.
    pub fn last_speech_ms(&self) -> u64 {
        self.recorder
            .lock()
            .unwrap()
            .as_ref()
            .map(|rec| rec.speech_ms())
            .unwrap_or(0)
    }

    pub fn stop_recording(&self, binding_id: &str, cancel_generation: u64) -> StopRecordingResult {
        self.invalidate_recording_readiness();
        let mut state = self.state.lock().unwrap();

        match *state {
            RecordingState::Recording {
                binding_id: ref active,
            } if active == binding_id => {
                self.set_state(&mut state, RecordingState::Stopping);
                drop(state);

                // Optionally keep recording for a bit longer to capture trailing audio.
                // This is only the explicit user setting; streaming VAD must not add
                // hidden post-release capture time.
                let settings = get_settings(&self.app_handle);
                let buffer_ms = settings.extra_recording_buffer_ms;
                if buffer_ms > 0 {
                    debug!(
                        "Extra recording buffer: sleeping {}ms before stopping",
                        buffer_ms
                    );
                    let started = Instant::now();
                    let buffer = Duration::from_millis(buffer_ms.into());
                    while started.elapsed() < buffer {
                        if self.was_cancelled_since(cancel_generation) {
                            debug!("Recording stop cancelled during extra buffer");
                            break;
                        }
                        let remaining = buffer.saturating_sub(started.elapsed());
                        std::thread::sleep(remaining.min(Duration::from_millis(25)));
                    }
                }

                let recorded_res = if let Some(rec) = self.recorder.lock().unwrap().as_ref() {
                    rec.stop().map_err(|e| e.to_string())
                } else {
                    Err("Recorder not available".to_string())
                };

                *self.is_recording.lock().unwrap() = false;
                self.set_state(&mut self.state.lock().unwrap(), RecordingState::Idle);

                self.release_stream_after_recording();
                self.apply_pending_device_change();

                if self.was_cancelled_since(cancel_generation) {
                    debug!("Recording stop cancelled; discarding captured samples");
                    return StopRecordingResult::Cancelled;
                }

                let mut recorded = match recorded_res {
                    Ok(r) => r,
                    Err(err) => return StopRecordingResult::Failed(err),
                };

                let captured_sample_count = recorded.stt_samples.len();

                // Pad if very short
                let s_len = recorded.stt_samples.len();
                if s_len < WHISPER_SAMPLE_RATE && s_len > 0 {
                    recorded
                        .stt_samples
                        .resize(WHISPER_SAMPLE_RATE * 5 / 4, 0.0);
                }
                StopRecordingResult::Captured {
                    recorded,
                    captured_sample_count,
                    sample_rate: WHISPER_SAMPLE_RATE as u32,
                }
            }
            _ => StopRecordingResult::NotActive,
        }
    }
    pub fn is_recording(&self) -> bool {
        // Lock-free: mirrors the `state` {Recording, Stopping} membership via
        // an atomic maintained by `set_state()`. Polled from the webview/main
        // thread, so it MUST NOT take the `state` mutex (a worker can hold it
        // across a slow CoreAudio open/close → main-thread deadlock / UI
        // freeze).
        self.recording_active.load(Ordering::SeqCst)
    }

    /// Cancel any ongoing recording without returning audio samples
    pub fn cancel_recording(&self) {
        self.invalidate_recording_readiness();
        self.cancel_generation.fetch_add(1, Ordering::AcqRel);
        let mut state = self.state.lock().unwrap();

        match *state {
            RecordingState::Recording { ref binding_id } => {
                // Whoever ends the live VAD test (its page, or the cancel
                // hotkey) must also end its per-frame reports, or they keep
                // flowing through every later dictation.
                if binding_id == VAD_TEST_BINDING {
                    set_vad_reporting(VAD_REPORT_TEST, false);
                }
                // Hold `Stopping` until the recorder has actually stopped,
                // like stop_recording: a start landing in between would
                // otherwise have its fresh recording stopped by this cancel.
                self.set_state(&mut state, RecordingState::Stopping);
                drop(state);

                if let Some(rec) = self.recorder.lock().unwrap().as_ref() {
                    let _ = rec.stop(); // Discard the result
                }

                *self.is_recording.lock().unwrap() = false;
                self.set_state(&mut self.state.lock().unwrap(), RecordingState::Idle);

                self.release_stream_after_recording();
                self.apply_pending_device_change();
            }
            RecordingState::Stopping => {
                debug!("Cancellation requested while recording is stopping");
            }
            RecordingState::Idle => {}
        }
    }
}
