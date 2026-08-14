//! In-app voice-cloning reference recorder (M3.5).
//!
//! Records a fixed number of seconds from the configured microphone with VAD
//! disabled (raw capture), saves it as WAV and imports it through the same
//! `import_cloned_voice` paths the WAV file picker uses, so Pocket and Qwen3
//! cloned voices work identically whether the reference came from a file or
//! was recorded live.

use crate::settings::get_settings;
use crate::tts::Voice;
use tauri::{AppHandle, Manager};

/// Supported target engines for a recorded clone reference.
fn validate_engine(engine: &str) -> Result<(), String> {
    match engine {
        "pocket" | "qwen3" => Ok(()),
        other => Err(format!(
            "Voice cloning by recording is only supported for pocket or qwen3, not '{other}'"
        )),
    }
}

/// Record `duration_secs` of raw microphone audio (VAD disabled) and import it
/// as a cloned voice for the given engine. Returns the imported voice.
///
/// The blocking capture runs on the async runtime's blocking pool so the
/// command never stalls the main thread; the UI shows a countdown meanwhile.
#[tauri::command]
#[specta::specta]
pub async fn record_clone_reference(
    app: AppHandle,
    engine: String,
    duration_secs: u32,
) -> Result<Voice, String> {
    validate_engine(&engine)?;
    let duration_secs = duration_secs.clamp(3, 60);

    // Don't fight the main dictation/conversation stream for the microphone.
    if let Some(audio_manager) =
        app.try_state::<std::sync::Arc<crate::managers::audio::AudioRecordingManager>>()
    {
        if audio_manager.inner().is_recording() {
            return Err(
                "Cannot record a voice reference while dictation is active. Stop recording first."
                    .to_string(),
            );
        }
    }

    // Resolve the user's configured microphone (fall back to system default).
    let device = {
        let settings = get_settings(&app);
        let desired = settings.selected_microphone.clone();
        match crate::audio_toolkit::audio::list_input_devices() {
            Ok(devices) => desired.filter(|name| !name.is_empty()).and_then(|name| {
                devices
                    .into_iter()
                    .find(|d| d.name == name)
                    .map(|d| d.device)
            }),
            Err(e) => {
                log::warn!("[CloneRecorder] Failed to enumerate input devices: {e}");
                None
            }
        }
    };

    let app_for_task = app.clone();
    let samples = tauri::async_runtime::spawn_blocking(move || {
        record_samples(&app_for_task, device, duration_secs)
    })
    .await
    .map_err(|e| format!("Voice reference recording panicked: {e}"))??;

    if samples.is_empty() {
        return Err("Recording produced no audio. Check your microphone.".to_string());
    }
    let recorded_secs = samples.len() as f64 / 16000.0;
    log::info!(
        "[CloneRecorder] Recorded {:.2}s of reference audio for engine '{engine}'",
        recorded_secs
    );

    // Persist to a temp WAV, then import through the standard path so the
    // copied file lands in the engine's cloned-voices dir with a clean stem.
    let stem = format!(
        "recorded-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    );
    let temp_path = std::env::temp_dir().join(format!("{stem}.wav"));
    crate::audio_toolkit::save_wav_file(&temp_path, &samples)
        .map_err(|e| format!("Failed to save recorded WAV: {e}"))?;

    let voice = match engine.as_str() {
        "pocket" => crate::tts::backends::pocket::PocketBackend::import_cloned_voice(
            &app,
            temp_path.as_path(),
        ),
        "qwen3" => crate::tts::backends::qwen3::Qwen3Backend::import_cloned_voice(
            &app,
            temp_path.as_path(),
        ),
        _ => unreachable!("engine validated above"),
    };

    // Best-effort cleanup of the temporary reference WAV.
    if let Err(e) = std::fs::remove_file(&temp_path) {
        log::warn!(
            "[CloneRecorder] Failed to remove temp WAV {}: {e}",
            temp_path.display()
        );
    }

    voice
}

/// Raw capture without VAD: opens a fresh recorder (independent of the shared
/// dictation stream), records the requested duration, closes it and returns
/// the 16 kHz mono samples.
fn record_samples(
    app: &AppHandle,
    device: Option<cpal::Device>,
    duration_secs: u32,
) -> Result<Vec<f32>, String> {
    use crate::audio_toolkit::audio::{AudioRecorder, VadPolicy};

    let mut recorder = AudioRecorder::new()
        .map_err(|e| format!("Failed to create recorder: {e}"))?
        .with_app_handle(app.clone());

    recorder
        .open(device)
        .map_err(|e| format!("Failed to open microphone: {e}"))?;

    // Ensure the recorder is always closed, even on early returns.
    struct CloseGuard<'a>(&'a mut AudioRecorder);
    impl Drop for CloseGuard<'_> {
        fn drop(&mut self) {
            let _ = self.0.close();
        }
    }
    let recorder = &mut CloseGuard(&mut recorder);

    recorder
        .0
        .start(VadPolicy::Disabled)
        .map_err(|e| format!("Failed to start recording: {e}"))?
        .recv()
        .map_err(|_| {
            "Microphone produced no first samples (recorder channel closed)".to_string()
        })?;

    std::thread::sleep(std::time::Duration::from_secs(u64::from(duration_secs)));

    recorder
        .0
        .stop()
        .map_err(|e| format!("Failed to stop recording: {e}"))
}
