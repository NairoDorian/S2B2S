use crate::audio_toolkit::{
    list_input_devices,
    vad::{
        SmoothedVad, TripleVad, VAD_OFFLINE_HANGOVER_FRAMES, VAD_ONSET_FRAMES, VAD_PREFILL_FRAMES,
        VAD_STREAMING_HANGOVER_FRAMES,
    },
    AudioRecorder, SileroVad, VadPolicy,
};
use crate::helpers::clamshell;
use crate::managers::transcription::StreamRouter;
use crate::settings::{get_settings, AppSettings};
use crate::utils;
use log::{debug, error, info, trace, warn};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::Manager;

const STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
const VAD_THRESHOLD: f32 = 0.3;

/// Translate the Brain's `endpoint_preset` into a silence frame count
/// (frames are 30 ms at 16 kHz):
/// - `snappy`  → 300 ms (10 frames)
/// - `patient` → 1200 ms (40 frames)
/// - anything else (`balanced` default) → 600 ms (20 frames)
pub fn endpoint_frames_for_preset(preset: &str) -> usize {
    match preset.to_ascii_lowercase().as_str() {
        "snappy" => 10,
        "patient" => 40,
        _ => 20,
    }
}

/// Classic glob matching (`*` and `?`) over chars, case-insensitive at the
/// call site. Backtracking on `*` keeps the implementation simple and correct
/// for short device names.
fn wildcard_match(pattern: &str, candidate: &str) -> bool {
    let pattern_chars = pattern.chars().collect::<Vec<_>>();
    let candidate_chars = candidate.chars().collect::<Vec<_>>();

    let mut pattern_index = 0usize;
    let mut candidate_index = 0usize;
    let mut star_index: Option<usize> = None;
    let mut match_index = 0usize;

    while candidate_index < candidate_chars.len() {
        if pattern_index < pattern_chars.len()
            && (pattern_chars[pattern_index] == '?'
                || pattern_chars[pattern_index] == candidate_chars[candidate_index])
        {
            pattern_index += 1;
            candidate_index += 1;
        } else if pattern_index < pattern_chars.len() && pattern_chars[pattern_index] == '*' {
            star_index = Some(pattern_index);
            match_index = candidate_index;
            pattern_index += 1;
        } else if let Some(star) = star_index {
            pattern_index = star + 1;
            match_index += 1;
            candidate_index = match_index;
        } else {
            return false;
        }
    }

    while pattern_index < pattern_chars.len() && pattern_chars[pattern_index] == '*' {
        pattern_index += 1;
    }

    pattern_index == pattern_chars.len()
}

/// Does a device name satisfy the auto-switch mask? Plain substrings (no `*`/
/// `?`) use containment; wildcards use glob matching. Case-insensitive.
fn matches_name_mask(device_name: &str, pattern: &str) -> bool {
    let trimmed = pattern.trim();
    if trimmed.is_empty() {
        return false;
    }

    let normalized_pattern = trimmed.to_lowercase();
    let normalized_name = device_name.to_lowercase();

    if normalized_pattern.contains('*') || normalized_pattern.contains('?') {
        wildcard_match(&normalized_pattern, &normalized_name)
    } else {
        normalized_name.contains(&normalized_pattern)
    }
}

/// First input device (in enumeration order) whose name matches the mask.
fn first_device_matching_mask<'a>(
    device_names: impl IntoIterator<Item = &'a str>,
    pattern: &str,
) -> Option<String> {
    device_names
        .into_iter()
        .find(|name| matches_name_mask(name, pattern))
        .map(|name| name.to_string())
}

/// Resolve the Silero VAD ONNX model path.
///
/// Preference order: v6.2 (newest, best accuracy) → v4 → legacy name.
/// The `SileroVad` struct auto-detects which model is loaded and uses the
/// correct ONNX tensor API, so any of these files will work correctly.
fn resolve_silero_vad_path(app_handle: &tauri::AppHandle) -> Result<PathBuf, anyhow::Error> {
    let candidates = [
        "resources/models/silero_vad_v6.2.onnx",
        "resources/models/silero_vad_v4.onnx",
        "resources/models/silero_vad.onnx",
    ];

    for rel in candidates {
        if let Ok(p) = app_handle
            .path()
            .resolve(rel, tauri::path::BaseDirectory::Resource)
        {
            if p.exists() {
                info!("Using Silero VAD model: {:?}", p);
                return Ok(p);
            }
        }
    }

    if let Ok(app_dir) = app_handle.path().app_data_dir() {
        let local_path = app_dir
            .join("models")
            .join("STT")
            .join("silero_vad")
            .join("silero_vad.onnx");
        if local_path.exists() {
            info!("Using local Silero VAD model: {:?}", local_path);
            return Ok(local_path);
        }
    }

    let project_fallback = PathBuf::from("models/STT/silero_vad/silero_vad.onnx");
    if project_fallback.exists() {
        info!("Using fallback Silero VAD model: {:?}", project_fallback);
        return Ok(project_fallback);
    }

    let primary = candidates[0];
    app_handle
        .path()
        .resolve(primary, tauri::path::BaseDirectory::Resource)
        .map_err(|e| anyhow::anyhow!("Failed to resolve VAD path: {}", e))
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
                    eMultimedia, eRender, Endpoints::IAudioEndpointVolume, IMMDeviceEnumerator,
                    MMDeviceEnumerator,
                },
                System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED},
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
                eMultimedia, eRender, Endpoints::IAudioEndpointVolume, IMMDeviceEnumerator,
                MMDeviceEnumerator,
            },
            System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED},
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

/* ──────────────────────────────────────────────────────────────── */
/* pause-media-while-recording (per-OS media session control)       */
/* ──────────────────────────────────────────────────────────────── */

#[cfg(target_os = "windows")]
fn pause_media_playback() -> Vec<String> {
    use std::future::IntoFuture;
    use windows::Media::Control::{
        GlobalSystemMediaTransportControlsSessionManager,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus,
    };

    let mut paused_sessions = Vec::new();

    // windows-rs 0.62 removed the blocking `get()` on IAsyncOperation; the
    // operation is a plain future now, so block on it via futures-executor
    // (already in the dependency tree).
    let manager = match GlobalSystemMediaTransportControlsSessionManager::RequestAsync() {
        Ok(operation) => match futures_executor::block_on(operation.into_future()) {
            Ok(manager) => manager,
            Err(err) => {
                debug!("Media pause unavailable: {}", err);
                return paused_sessions;
            }
        },
        Err(err) => {
            debug!("Media pause unavailable: {}", err);
            return paused_sessions;
        }
    };

    let sessions = match manager.GetSessions() {
        Ok(sessions) => sessions,
        Err(err) => {
            debug!("Media pause failed to enumerate sessions: {}", err);
            return paused_sessions;
        }
    };

    let session_count = sessions.Size().unwrap_or(0);
    for index in 0..session_count {
        let Ok(session) = sessions.GetAt(index) else {
            continue;
        };

        let is_playing = session
            .GetPlaybackInfo()
            .and_then(|info| info.PlaybackStatus())
            .map(|status| {
                status == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing
            })
            .unwrap_or(false);

        if !is_playing {
            continue;
        }

        let source_id = session
            .SourceAppUserModelId()
            .map(|id| id.to_string_lossy())
            .unwrap_or_default();

        let pause_result = session
            .TryPauseAsync()
            .map_err(|err| format!("{}", err))
            .and_then(|operation| {
                futures_executor::block_on(operation.into_future())
                    .map_err(|err| format!("{}", err))
            });
        match pause_result {
            Ok(true) => {
                if !source_id.is_empty() {
                    paused_sessions.push(source_id);
                }
            }
            Ok(false) => debug!("Media pause declined by session"),
            Err(err) => debug!("Media pause failed: {}", err),
        }
    }

    paused_sessions
}

#[cfg(target_os = "windows")]
fn resume_media_playback(paused_sessions: &[String]) {
    use std::collections::HashSet;
    use std::future::IntoFuture;
    use windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager;

    if paused_sessions.is_empty() {
        return;
    }

    let paused_ids: HashSet<&str> = paused_sessions.iter().map(String::as_str).collect();
    let manager = match GlobalSystemMediaTransportControlsSessionManager::RequestAsync() {
        Ok(operation) => match futures_executor::block_on(operation.into_future()) {
            Ok(manager) => manager,
            Err(err) => {
                debug!("Media resume unavailable: {}", err);
                return;
            }
        },
        Err(err) => {
            debug!("Media resume unavailable: {}", err);
            return;
        }
    };

    let sessions = match manager.GetSessions() {
        Ok(sessions) => sessions,
        Err(err) => {
            debug!("Media resume failed to enumerate sessions: {}", err);
            return;
        }
    };

    let session_count = sessions.Size().unwrap_or(0);
    for index in 0..session_count {
        let Ok(session) = sessions.GetAt(index) else {
            continue;
        };
        let source_id = session
            .SourceAppUserModelId()
            .map(|id| id.to_string_lossy())
            .unwrap_or_default();

        if !paused_ids.contains(source_id.as_str()) {
            continue;
        }

        let play_result = session
            .TryPlayAsync()
            .map_err(|err| format!("{}", err))
            .and_then(|operation| {
                futures_executor::block_on(operation.into_future())
                    .map_err(|err| format!("{}", err))
            });
        if let Err(err) = play_result {
            debug!("Media resume failed: {}", err);
        }
    }
}

#[cfg(target_os = "linux")]
fn pause_media_playback() -> Vec<String> {
    use std::process::Command;

    let mut paused_players = Vec::new();
    let output = match Command::new("playerctl").arg("-l").output() {
        Ok(output) if output.status.success() => output,
        Ok(_) | Err(_) => return paused_players,
    };

    let players = String::from_utf8_lossy(&output.stdout);
    for player in players.lines().map(str::trim).filter(|p| !p.is_empty()) {
        let status = Command::new("playerctl")
            .args(["-p", player, "status"])
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string());

        if status.as_deref() != Some("Playing") {
            continue;
        }

        if Command::new("playerctl")
            .args(["-p", player, "pause"])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
        {
            paused_players.push(player.to_string());
        }
    }

    paused_players
}

#[cfg(target_os = "linux")]
fn resume_media_playback(paused_players: &[String]) {
    use std::process::Command;

    for player in paused_players {
        let _ = Command::new("playerctl")
            .args(["-p", player, "play"])
            .output();
    }
}

#[cfg(target_os = "macos")]
fn run_osascript(script: &str) -> Option<String> {
    use std::process::Command;

    let output = Command::new("osascript")
        .args(["-e", script])
        .output()
        .ok()?;

    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

#[cfg(target_os = "macos")]
fn macos_process_is_running(process_name: &str) -> bool {
    let escaped_name = process_name.replace('"', "\\\"");
    let script = format!(
        "tell application \"System Events\" to (name of processes) contains \"{}\"",
        escaped_name
    );
    run_osascript(&script).as_deref() == Some("true")
}

#[cfg(target_os = "macos")]
fn pause_media_playback() -> Vec<String> {
    let mut paused_apps = Vec::new();
    for app_name in ["Music", "Spotify", "QuickTime Player"] {
        if !macos_process_is_running(app_name) {
            continue;
        }

        let escaped_name = app_name.replace('"', "\\\"");
        let state_script = format!(
            "tell application \"{}\" to player state as string",
            escaped_name
        );
        if run_osascript(&state_script).as_deref() != Some("playing") {
            continue;
        }

        let pause_script = format!("tell application \"{}\" to pause", escaped_name);
        if run_osascript(&pause_script).is_some() {
            paused_apps.push(app_name.to_string());
        }
    }
    paused_apps
}

#[cfg(target_os = "macos")]
fn resume_media_playback(paused_apps: &[String]) {
    for app_name in paused_apps {
        if !macos_process_is_running(app_name) {
            continue;
        }

        let escaped_name = app_name.replace('"', "\\\"");
        let play_script = format!("tell application \"{}\" to play", escaped_name);
        let _ = run_osascript(&play_script);
    }
}

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
fn pause_media_playback() -> Vec<String> {
    Vec::new()
}

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
fn resume_media_playback(_paused_sessions: &[String]) {}

const WHISPER_SAMPLE_RATE: usize = 16000;

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

/* ──────────────────────────────────────────────────────────────── */

fn create_audio_recorder(
    vad_path: &Path,
    app_handle: &tauri::AppHandle,
    selected_channel: Option<u16>,
    stream_router: Arc<StreamRouter>,
    continuous_mode: Arc<AtomicBool>,
    continuous_mode_paused: Arc<AtomicBool>,
    endpoint_silence_frames: Arc<AtomicUsize>,
) -> Result<AudioRecorder, anyhow::Error> {
    let settings = get_settings(app_handle);
    let silero = SileroVad::new(vad_path, VAD_THRESHOLD)
        .map_err(|e| anyhow::anyhow!("Failed to create SileroVad: {}", e))?;

    let base_vad: Box<dyn crate::audio_toolkit::VoiceActivityDetector> =
        if settings.vad_mode == "triple" {
            let ns_enabled = Arc::new(AtomicBool::new(settings.noise_suppression_enabled));
            Box::new(TripleVad::new(
                Box::new(silero),
                ns_enabled,
                settings.rnnoise_voice_threshold as f32,
                0.002,
            ))
        } else {
            Box::new(silero)
        };

    let smoothed_vad = SmoothedVad::new(
        base_vad,
        VAD_PREFILL_FRAMES,
        VAD_OFFLINE_HANGOVER_FRAMES,
        VAD_ONSET_FRAMES,
    );

    let mut recorder = AudioRecorder::new()
        .map_err(|e| anyhow::anyhow!("Failed to create AudioRecorder: {}", e))?
        .with_app_handle(app_handle.clone())
        .with_vad(
            Box::new(smoothed_vad),
            VAD_OFFLINE_HANGOVER_FRAMES,
            VAD_STREAMING_HANGOVER_FRAMES,
        )
        .with_continuous_mode(continuous_mode, continuous_mode_paused)
        .with_endpoint_silence_frames(endpoint_silence_frames)
        .with_selected_channel(selected_channel)
        .with_level_callback({
            let app_handle = app_handle.clone();
            move |levels| {
                utils::emit_levels(&app_handle, &levels);
            }
        })
        .with_audio_callback({
            let router = stream_router;
            move |frame| {
                router.feed(frame);
            }
        });

    if let Some(detector) = app_handle.try_state::<Arc<crate::wake_word::WakeWordDetector>>() {
        recorder = recorder.with_wake_word_detector(detector.inner().clone());
    }

    Ok(recorder)
}

/* ──────────────────────────────────────────────────────────────── */

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
    // S2B2S: TripleVAD / noise suppression / continuous mode
    is_paused: Arc<AtomicBool>,
    noise_suppression_enabled: Arc<AtomicBool>,
    continuous_mode: Arc<AtomicBool>,
    continuous_mode_paused: Arc<AtomicBool>,
    /// Silence frames (30 ms each) to endpoint a continuous-voice utterance.
    /// Live mirror of `BrainConfig::endpoint_preset`.
    endpoint_silence_frames: Arc<AtomicUsize>,
    /// Auto-stop: silence watchdog timer
    auto_stop_enabled: Arc<AtomicBool>,
    auto_stop_duration_secs: Arc<std::sync::atomic::AtomicU32>,
    /// Resolution of a *named* microphone cached to skip full device enumeration.
    cached_device: Arc<Mutex<Option<(String, cpal::Device)>>>,
    /// Media sessions this app paused for the active recording, restored on
    /// session cleanup (`resume_media_if_paused`).
    paused_media_sessions: Arc<Mutex<Vec<String>>>,
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

        let is_paused = Arc::new(AtomicBool::new(false));
        let noise_suppression_enabled =
            Arc::new(AtomicBool::new(settings.noise_suppression_enabled));
        let continuous_mode = Arc::new(AtomicBool::new(false));
        let continuous_mode_paused = Arc::new(AtomicBool::new(false));
        let endpoint_silence_frames = Arc::new(AtomicUsize::new(endpoint_frames_for_preset(
            &settings.brain.endpoint_preset,
        )));
        let auto_stop_enabled = Arc::new(AtomicBool::new(false));
        let auto_stop_duration_secs = Arc::new(std::sync::atomic::AtomicU32::new(30));

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
            is_paused,
            noise_suppression_enabled,
            continuous_mode,
            continuous_mode_paused,
            endpoint_silence_frames,
            auto_stop_enabled,
            auto_stop_duration_secs,
            cached_device: Arc::new(Mutex::new(None)),
            paused_media_sessions: Arc::new(Mutex::new(Vec::new())),
        };

        // Always-on?  Open immediately.
        if matches!(mode, MicrophoneMode::AlwaysOn) {
            manager.start_microphone_stream()?;
        }

        Ok(manager)
    }

    /* ---------- helper methods --------------------------------------------- */

    /// The microphone name the settings ask for, or `None` for the system
    /// default. Resolution order:
    ///   1. mic auto-switch: the first device matching
    ///      `selected_microphone_name_pattern` (when enabled and non-empty)
    ///   2. the clamshell probe (an `ioreg` subprocess, ~10-20ms) — only runs
    ///      when a clamshell microphone is actually configured
    ///   3. the manually selected microphone
    fn desired_device_name(&self, settings: &AppSettings) -> Option<String> {
        if settings.selected_microphone_auto_switch_enabled
            && !settings.selected_microphone_name_pattern.trim().is_empty()
        {
            match list_input_devices() {
                Ok(devices) => {
                    if let Some(matched) = first_device_matching_mask(
                        devices.iter().map(|device| device.name.as_str()),
                        &settings.selected_microphone_name_pattern,
                    ) {
                        debug!("mic auto-switch: mask matched '{}'", matched);
                        return Some(matched);
                    }
                    debug!("mic auto-switch: no device matches the mask, falling back");
                }
                Err(e) => {
                    debug!(
                        "mic auto-switch: device enumeration failed ({}), falling back",
                        e
                    );
                }
            }
        }

        if settings.clamshell_microphone.is_some() {
            let clamshell_started = Instant::now();
            let is_clamshell = clamshell::is_clamshell().unwrap_or(false);
            debug!(
                "device resolve: clamshell_check={:?} (clamshell={})",
                clamshell_started.elapsed(),
                is_clamshell
            );
            if is_clamshell {
                return settings.clamshell_microphone.clone();
            }
        }
        settings.selected_microphone.clone()
    }

    pub fn invalidate_device_cache(&self) {
        *self.cached_device.lock().unwrap() = None;
    }

    fn get_effective_microphone_device(&self, settings: &AppSettings) -> Option<cpal::Device> {
        let device_name = match self.desired_device_name(settings) {
            Some(name) => name,
            None => {
                debug!("device resolve: no mic configured -> system default");
                return None;
            }
        };

        // Cache hit: skip the full enumeration. A stale device (unplugged)
        // fails at open, where the caller invalidates and retries fresh.
        if let Some((cached_name, device)) = self.cached_device.lock().unwrap().as_ref() {
            if *cached_name == device_name {
                debug!("device resolve: cache hit for '{}'", device_name);
                return Some(device.clone());
            }
        }

        // Find the device by name
        let enumerate_started = Instant::now();
        let device = match list_input_devices() {
            Ok(devices) => devices
                .into_iter()
                .find(|d| d.name == device_name)
                .map(|d| d.device),
            Err(e) => {
                debug!("Failed to list devices, using default: {}", e);
                None
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
        device
    }

    fn schedule_lazy_close(&self) {
        let gen = self.close_generation.fetch_add(1, Ordering::SeqCst) + 1;
        let app = self.app_handle.clone();
        std::thread::spawn(move || {
            std::thread::sleep(STREAM_IDLE_TIMEOUT);
            let rm = app.state::<Arc<AudioRecordingManager>>();
            // Hold state lock across the check AND close to serialize against
            // try_start_recording, preventing a race where the stream is closed
            // under an active recording.
            let state = rm.state.lock().unwrap();
            if rm.close_generation.load(Ordering::SeqCst) == gen
                && matches!(*state, RecordingState::Idle)
            {
                // stop_microphone_stream does not acquire the state lock,
                // so holding it here is safe (no deadlock).
                info!(
                    "Closing idle microphone stream after {:?}",
                    STREAM_IDLE_TIMEOUT
                );
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

    /// Pauses active media if `pause_media_while_recording` is enabled.
    /// Tracks which sessions we paused so `resume_media_if_paused` can restore
    /// exactly those — and never pauses a second time within one recording.
    pub fn apply_media_pause(&self) {
        let settings = get_settings(&self.app_handle);
        if !settings.pause_media_while_recording {
            return;
        }

        // Before pausing, ensure we didn't cancel/stop recording while waiting.
        if !self.is_recording() {
            return;
        }

        let mut paused_guard = self.paused_media_sessions.lock().unwrap();
        if !paused_guard.is_empty() {
            return;
        }

        let paused_sessions = pause_media_playback();
        if !paused_sessions.is_empty() {
            debug!(
                "Paused {} media session(s) while recording",
                paused_sessions.len()
            );
        }
        *paused_guard = paused_sessions;
    }

    /// Resumes media sessions that this recording paused. Idempotent — a
    /// no-op when nothing was paused.
    pub fn resume_media_if_paused(&self) {
        let mut paused_guard = self.paused_media_sessions.lock().unwrap();
        if paused_guard.is_empty() {
            return;
        }

        resume_media_playback(&paused_guard);
        debug!(
            "Requested resume for {} media session(s)",
            paused_guard.len()
        );
        paused_guard.clear();
    }

    pub fn preload_vad(&self) -> Result<(), anyhow::Error> {
        let mut recorder_opt = self.recorder.lock().unwrap();
        if recorder_opt.is_none() {
            info!("Preloading Silero VAD model...");
            let vad_path = resolve_silero_vad_path(&self.app_handle)?;
            info!("Loading Silero VAD model: {}", vad_path.display());
            let settings = get_settings(&self.app_handle);
            *recorder_opt = Some(create_audio_recorder(
                &vad_path,
                &self.app_handle,
                settings.selected_channel,
                Arc::clone(&self.stream_router),
                Arc::clone(&self.continuous_mode),
                Arc::clone(&self.continuous_mode_paused),
                Arc::clone(&self.endpoint_silence_frames),
            )?);
            info!("Silero VAD model preloaded successfully");
        }
        Ok(())
    }

    pub fn start_microphone_stream(&self) -> Result<(), anyhow::Error> {
        let mut open_flag = self.is_open.lock().unwrap();
        if *open_flag {
            // `is_open` only records that we opened a stream at some point, not
            // that one is still running. If the capture worker has since exited
            // (mic unplugged mid-session, USB dropout), returning Ok here hands
            // the caller a dead recorder: it captures nothing, then fails in
            // stop() on the closed channel, and stays wedged until the
            // on-demand close timeout eventually resets the manager.
            let worker_dead = self
                .recorder
                .lock()
                .unwrap()
                .as_ref()
                .is_some_and(|rec| rec.is_capture_worker_dead());

            if !worker_dead {
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
                // Skipping rec.stop() here: the worker is gone, so the command
                // would only fail on the closed channel.
                let _ = rec.close();
            }
            *self.is_recording.lock().unwrap() = false;
            *open_flag = false;
            // Fall through and open a fresh stream.
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
        // No pre-flight enumeration here: when nothing is configured the
        // recorder resolves the system default itself, and a machine with no
        // input devices at all fails inside open() with the same
        // "No input device found" error this used to check for.
        let settings = get_settings(&self.app_handle);
        let resolve_started = Instant::now();
        let selected_device = self.get_effective_microphone_device(&settings);
        let resolve_elapsed = resolve_started.elapsed();

        // Ensure VAD is loaded if it wasn't for whatever reason
        let vad_started = Instant::now();
        self.preload_vad()?;
        let vad_elapsed = vad_started.elapsed();

        let open_started = Instant::now();
        let mut recorder_opt = self.recorder.lock().unwrap();
        if let Some(rec) = recorder_opt.as_mut() {
            if let Err(first_err) = rec.open(selected_device.clone()) {
                // A cached device or config may have gone stale (unplugged,
                // rate/format changed). Re-resolve from a fresh enumeration and
                // retry once before surfacing the error.
                warn!("Recorder open failed ({first_err}); re-resolving device and retrying once");
                self.invalidate_device_cache();
                let fresh_device = self.get_effective_microphone_device(&settings);
                rec.open(fresh_device)
                    .map_err(|e| anyhow::anyhow!("Failed to open recorder: {}", e))?;
            }
        }
        debug!(
            "mic stream breakdown: device_resolve={:?} vad_ensure={:?} open={:?}",
            resolve_elapsed,
            vad_elapsed,
            open_started.elapsed()
        );

        *open_flag = true;
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
        let mut state = self.state.lock().unwrap();

        if let RecordingState::Idle = *state {
            self.is_paused.store(false, Ordering::Relaxed);
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

            if let Some(rec) = self.recorder.lock().unwrap().as_ref() {
                match rec.start(vad_policy) {
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

    pub fn update_selected_device(&self) -> Result<(), anyhow::Error> {
        // Device settings changed; re-enumerate the device and restart capture.
        // Serialize against recording start/stop: never tear down the stream
        // under an active recording (its samples would be discarded).
        let state = self.state.lock().unwrap();
        if !matches!(*state, RecordingState::Idle) {
            return Ok(());
        }
        drop(state);

        self.invalidate_device_cache();
        let was_open = *self.is_open.lock().unwrap();
        if was_open {
            self.close_generation.fetch_add(1, Ordering::SeqCst);
            self.stop_microphone_stream();
            self.start_microphone_stream()?;
        }
        Ok(())
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
        if was_open {
            if let Err(error) = self.start_microphone_stream() {
                if let Some(recorder) = self.recorder.lock().unwrap().as_mut() {
                    recorder.set_selected_channel(previous_channel);
                }
                return Err(error);
            }
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

    pub fn stop_recording(&self, binding_id: &str, cancel_generation: u64) -> Option<Vec<f32>> {
        self.invalidate_recording_readiness();
        let mut state = self.state.lock().unwrap();

        match *state {
            RecordingState::Recording {
                binding_id: ref active,
            } if active == binding_id => {
                self.is_paused.store(false, Ordering::Relaxed);
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
                    let buffer = Duration::from_millis(buffer_ms);
                    while started.elapsed() < buffer {
                        if self.was_cancelled_since(cancel_generation) {
                            debug!("Recording stop cancelled during extra buffer");
                            break;
                        }
                        let remaining = buffer.saturating_sub(started.elapsed());
                        std::thread::sleep(remaining.min(Duration::from_millis(25)));
                    }
                }

                let samples = if let Some(rec) = self.recorder.lock().unwrap().as_ref() {
                    match rec.stop() {
                        Ok(buf) => buf,
                        Err(e) => {
                            error!("stop() failed: {e}");
                            Vec::new()
                        }
                    }
                } else {
                    error!("Recorder not available");
                    Vec::new()
                };

                *self.is_recording.lock().unwrap() = false;
                self.set_state(&mut self.state.lock().unwrap(), RecordingState::Idle);

                // In on-demand mode, close the mic (lazily if the setting is enabled)
                if matches!(*self.mode.lock().unwrap(), MicrophoneMode::OnDemand) {
                    if get_settings(&self.app_handle).lazy_stream_close {
                        self.schedule_lazy_close();
                    } else {
                        self.stop_microphone_stream();
                    }
                }

                if self.was_cancelled_since(cancel_generation) {
                    debug!("Recording stop cancelled; discarding captured samples");
                    return None;
                }

                // Pad if very short
                let s_len = samples.len();
                // debug!("Got {} samples", s_len);
                if s_len < WHISPER_SAMPLE_RATE && s_len > 0 {
                    let mut padded = samples;
                    padded.resize(WHISPER_SAMPLE_RATE * 5 / 4, 0.0);
                    Some(padded)
                } else {
                    Some(samples)
                }
            }
            _ => None,
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
            RecordingState::Recording { .. } => {
                self.is_paused.store(false, Ordering::Relaxed);
                self.set_state(&mut state, RecordingState::Idle);
                drop(state);

                if let Some(rec) = self.recorder.lock().unwrap().as_ref() {
                    let _ = rec.stop(); // Discard the result
                }

                *self.is_recording.lock().unwrap() = false;

                // In on-demand mode, close the mic (lazily if the setting is enabled)
                if matches!(*self.mode.lock().unwrap(), MicrophoneMode::OnDemand) {
                    if get_settings(&self.app_handle).lazy_stream_close {
                        self.schedule_lazy_close();
                    } else {
                        self.stop_microphone_stream();
                    }
                }
            }
            RecordingState::Stopping => {
                debug!("Cancellation requested while recording is stopping");
            }
            RecordingState::Idle => {}
        }
    }

    pub fn toggle_pause(&self) -> bool {
        let val = !self.is_paused.load(Ordering::Relaxed);
        self.is_paused.store(val, Ordering::Relaxed);
        val
    }

    pub fn is_recording_paused(&self) -> bool {
        self.is_paused.load(Ordering::Relaxed)
    }

    pub fn set_noise_suppression_enabled(&self, enabled: bool) {
        self.noise_suppression_enabled
            .store(enabled, Ordering::Relaxed);
    }

    pub fn set_continuous_mode(&self, enabled: bool) -> Result<(), anyhow::Error> {
        self.continuous_mode.store(enabled, Ordering::SeqCst);
        self.continuous_mode_paused.store(false, Ordering::SeqCst);

        if enabled {
            // Re-read the endpoint preset every time continuous mode starts so
            // the silence threshold always reflects current settings.
            let settings = get_settings(&self.app_handle);
            let frames = endpoint_frames_for_preset(&settings.brain.endpoint_preset);
            self.endpoint_silence_frames.store(frames, Ordering::SeqCst);
            self.update_mode(MicrophoneMode::AlwaysOn)?;
        } else {
            let settings = get_settings(&self.app_handle);
            let original_mode = if settings.always_on_microphone {
                MicrophoneMode::AlwaysOn
            } else {
                MicrophoneMode::OnDemand
            };
            self.update_mode(original_mode)?;
        }

        Ok(())
    }

    /// Live-update the continuous-voice endpoint threshold (in 30 ms silence
    /// frames) without restarting the stream. Called when the Brain's
    /// endpoint preset changes in settings.
    pub fn set_endpoint_silence_frames(&self, frames: usize) {
        self.endpoint_silence_frames
            .store(frames.max(1), Ordering::SeqCst);
        log::info!(
            "[ContinuousVoice] Endpoint silence set to {} frames",
            frames
        );
    }

    pub fn set_continuous_mode_paused(&self, paused: bool) {
        self.continuous_mode_paused.store(paused, Ordering::SeqCst);
    }

    pub fn enable_wake_word(&self, enabled: bool) {
        // Wake word detection uses the always-on microphone stream.
        // If the stream is not already open, open it in AlwaysOn mode.
        if enabled {
            let mode = self.mode.lock().unwrap().clone();
            if matches!(mode, MicrophoneMode::OnDemand) {
                let _ = self.update_mode(MicrophoneMode::AlwaysOn);
            }
            log::info!("[WakeWord] Always-on mic enabled for wake word detection");
        }
    }

    pub fn set_auto_stop(&self, enabled: bool, duration_secs: u32) {
        self.auto_stop_enabled.store(enabled, Ordering::SeqCst);
        self.auto_stop_duration_secs
            .store(duration_secs.max(5), Ordering::SeqCst);
        log::info!(
            "[AutoStop] {} ({}s silence)",
            if enabled { "enabled" } else { "disabled" },
            duration_secs
        );
    }

    pub fn update_vad_mode(&self, _mode: &str) -> Result<(), anyhow::Error> {
        // Clear the recorder so it gets re-created with the new VAD mode
        let mut recorder_opt = self.recorder.lock().unwrap();
        *recorder_opt = None;
        drop(recorder_opt);

        // If it was open, restart the stream to recreate the recorder
        if *self.is_open.lock().unwrap() {
            self.close_generation.fetch_add(1, Ordering::SeqCst);
            self.stop_microphone_stream();
            self.start_microphone_stream()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{first_device_matching_mask, matches_name_mask, wildcard_match};

    #[test]
    fn wildcard_match_basic() {
        assert!(wildcard_match("mic", "mic"));
        assert!(wildcard_match("mic*", "microphone"));
        assert!(wildcard_match("*mic*", "micmic"));
        assert!(wildcard_match("mic?one", "micxone"));
        assert!(wildcard_match("", ""));
        assert!(wildcard_match("*", "anything"));
    }

    #[test]
    fn wildcard_match_no_false_positives() {
        assert!(!wildcard_match("mic", "microphone"));
        assert!(!wildcard_match("mic*", "camera"));
        assert!(!wildcard_match("?mic", "mic"));
        assert!(!wildcard_match("*mic", "microphone"));
        assert!(!wildcard_match("mic?one", "micxophone"));
    }

    #[test]
    fn wildcard_match_trailing_and_multiple_stars() {
        assert!(wildcard_match("USB * Mic*", "USB 2.0 Microphone"));
        assert!(wildcard_match("*a*b*c*", "abc"));
        assert!(wildcard_match("*a*b*c*", "aXbYcZ"));
        assert!(!wildcard_match("*a*b*c*", "acb"));
    }

    #[test]
    fn matches_name_mask_plain_substring_is_containment() {
        assert!(matches_name_mask("USB Microphone", "micro"));
        assert!(matches_name_mask("USB Microphone", "  USB  "));
        assert!(matches_name_mask("Microphone Array", "MICROPHONE"));
        assert!(!matches_name_mask("USB Microphone", "camera"));
        assert!(!matches_name_mask("USB Microphone", ""));
        assert!(!matches_name_mask("USB Microphone", "   "));
    }

    #[test]
    fn matches_name_mask_wildcards_case_insensitive() {
        assert!(matches_name_mask("USB 2.0 Microphone", "usb * mic*"));
        assert!(matches_name_mask("Microphone Array", "*array"));
        assert!(!matches_name_mask("External Camera", "*mic*"));
    }

    #[test]
    fn first_device_matching_mask_picks_first_match() {
        let devices = [
            "External Camera",
            "USB 2.0 Microphone",
            "USB 2.0 Microphone (Monitor)",
        ];

        assert_eq!(
            first_device_matching_mask(devices, "usb*mic*"),
            Some("USB 2.0 Microphone".to_string())
        );
        assert_eq!(first_device_matching_mask(devices, "webcam"), None);
        assert_eq!(first_device_matching_mask(devices, ""), None);
    }
}
