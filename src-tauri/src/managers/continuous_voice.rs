use crate::brain::manager::BrainManager;
use crate::managers::audio::AudioRecordingManager;
use crate::managers::history::HistoryManager;
use crate::managers::transcription::TranscriptionManager;
use crate::settings::get_settings;
use crate::tts::manager::TtsManager;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Listener, Manager};

pub fn process_continuous_samples(app: &AppHandle, samples: Vec<f32>) -> Result<(), String> {
    log::info!(
        "Continuous voice pipeline started with {} samples",
        samples.len()
    );

    let rm = app
        .try_state::<Arc<AudioRecordingManager>>()
        .ok_or_else(|| "AudioRecordingManager not registered".to_string())?
        .inner()
        .clone();
    let tm = app
        .try_state::<Arc<TranscriptionManager>>()
        .ok_or_else(|| "TranscriptionManager not registered".to_string())?
        .inner()
        .clone();
    let hm = app
        .try_state::<Arc<HistoryManager>>()
        .ok_or_else(|| "HistoryManager not registered".to_string())?
        .inner()
        .clone();
    let bm = app
        .try_state::<Arc<BrainManager>>()
        .ok_or_else(|| "BrainManager not registered".to_string())?
        .inner()
        .clone();
    let tts = app
        .try_state::<Arc<TtsManager>>()
        .ok_or_else(|| "TtsManager not registered".to_string())?
        .inner()
        .clone();

    // 1. Temporarily pause continuous listening
    rm.set_continuous_mode_paused(true);

    // 2. Transcribe
    let transcription_result = tm.transcribe(samples.clone());

    let file_name = format!("s2b2s-{}.wav", chrono::Utc::now().timestamp());
    let wav_path = hm.recordings_dir().join(&file_name);
    let mut wav_saved = false;
    match crate::audio_toolkit::save_wav_file(&wav_path, &samples) {
        Ok(()) => {
            if crate::audio_toolkit::verify_wav_file(&wav_path, samples.len()).is_ok() {
                wav_saved = true;
            }
        }
        Err(e) => {
            log::error!("Failed to save WAV file for continuous voice: {}", e);
        }
    }

    let transcription = match transcription_result {
        Ok(text) => text.trim().to_string(),
        Err(e) => {
            log::error!("Continuous voice transcription failed: {}", e);
            rm.set_continuous_mode_paused(false);
            return Err(format!("Transcription failed: {e}"));
        }
    };

    if transcription.is_empty() {
        log::info!("Empty transcription; skipping Brain query and resuming listening.");
        rm.set_continuous_mode_paused(false);
        return Ok(());
    }

    // ITN: spoken → written normalization for continuous voice (conversation mode)
    let transcription = crate::tts::sanitize::post_stt_normalize(&transcription);

    // 3. Save STT entry in history
    if wav_saved {
        let stt_model = tm.get_current_model();
        if let Err(err) = hm.save_entry(
            file_name,
            transcription.clone(),
            false,
            None,
            None,
            "stt".to_string(),
            stt_model,
            None,
            None,
        ) {
            log::error!("Failed to save history entry for continuous voice: {}", err);
        }
    }

    // 4. Emit brain:asked to display on frontend
    let _ = app.emit("brain:asked", &transcription);

    // 5. Query Brain and play TTS
    let settings = get_settings(app);
    let will_play_tts = settings.brain.read_aloud && settings.tts.enabled;

    let app_clone = app.clone();
    let bm_clone = bm.clone();
    let transcription_clone = transcription.clone();

    // Run the async Brain/TTS pipeline
    tauri::async_runtime::block_on(async move {
        let _ask_result = bm_clone.ask(transcription_clone).await;

        if will_play_tts && tts.is_playing() {
            log::info!("Waiting for TTS playback to finish...");
            let (tx, rx) = std::sync::mpsc::channel::<()>();

            let tx_finished = tx.clone();
            let id_finished = app_clone.once("tts:finished", move |_event| {
                let _ = tx_finished.send(());
            });

            let tx_stopped = tx.clone();
            let id_stopped = app_clone.once("tts:stopped", move |_event| {
                let _ = tx_stopped.send(());
            });

            let tx_error = tx.clone();
            let id_error = app_clone.once("tts:error", move |_event| {
                let _ = tx_error.send(());
            });

            struct EventCleanup {
                app: tauri::AppHandle,
                ids: Vec<tauri::EventId>,
            }
            impl Drop for EventCleanup {
                fn drop(&mut self) {
                    for id in &self.ids {
                        self.app.unlisten(*id);
                    }
                }
            }
            let _cleanup = EventCleanup {
                app: app_clone.clone(),
                ids: vec![id_finished, id_stopped, id_error],
            };

            let _ = rx.recv_timeout(std::time::Duration::from_secs(60));
            log::info!("TTS playback finished or timed out.");
        }
    });

    // 6. Resume continuous listening
    // Check if auto-listen is enabled; if not, automatically restart
    let settings = get_settings(app);
    if settings.brain.auto_listen {
        rm.set_continuous_mode_paused(false);
        log::info!("Continuous listening resumed (auto-listen ON).");
    } else {
        // Re-arm listening after a 250ms grace period to avoid capturing room reverb
        let app_clone = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(250));
            let rm = app_clone.state::<Arc<AudioRecordingManager>>();
            rm.set_continuous_mode_paused(false);
            log::info!("Continuous listening resumed after 250ms grace.");
        });
    }

    Ok(())
}
