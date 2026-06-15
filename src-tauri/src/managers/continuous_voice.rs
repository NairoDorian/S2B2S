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

    // 1. Temporarily pause continuous listening (prevents new utterance queuing)
    //    but VAD stays active so we can detect barge-in speech during TTS playback
    rm.set_continuous_mode_paused(true);

    // 2. Transcribe
    let stt_start = std::time::Instant::now();
    let transcription_result = tm.transcribe(samples.clone());
    let stt_ms = stt_start.elapsed().as_millis() as u64;

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

    // 4. Emit brain:asked to display on frontend with STT timing
    let asked_payload = serde_json::json!({
        "text": transcription,
        "stt_ms": stt_ms,
    });
    let _ = app.emit("brain:asked", &asked_payload);

    // 5. Query Brain and play TTS
    let settings = get_settings(app);
    let will_play_tts = settings.brain.read_aloud && settings.tts.enabled;

    let app_clone = app.clone();
    let bm_clone = bm.clone();
    let transcription_clone = transcription.clone();

    // Clone for async block — rm used both inside and after
    let rm_for_after = rm.clone();

    // Run the async Brain/TTS pipeline
    tauri::async_runtime::block_on(async move {
        let ask_result = bm_clone.ask(transcription_clone).await;
        // Whether the Brain produced anything to speak this turn.
        let has_reply = ask_result
            .as_ref()
            .map(|t| !t.trim().is_empty())
            .unwrap_or(false);

        // Do NOT gate on `tts.is_playing()`: TTS synthesis is asynchronous (sentences
        // are queued during streaming and synthesized on a background thread), so when
        // ask() returns the audio often hasn't started playing yet and is_playing()
        // reads false. Gating on it skipped the wait/barge-in block entirely and made
        // the assistant listen over its own speech. The terminal TTS event
        // (tts:finished/stopped/error) for the LAST queued sentence fires after this
        // point, so registering the listeners now and waiting for it is race-free.
        if will_play_tts && has_reply {
            log::info!("Waiting for TTS playback to finish (barge-in active)...");

            // Barge-in: if user speaks during TTS, abort current turn
            let barge_aborted = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let barge_aborted_clone = barge_aborted.clone();

            let app_for_barge = app_clone.clone();
            let bm_for_barge = bm_clone.clone();
            let tts_for_barge = tts.clone();
            let rm_for_barge = rm.clone();

            // Listen for speech-start events while TTS is playing
            let barge_listener = app_clone.listen("continuous-voice:speech-started", move |_| {
                if !barge_aborted_clone.load(std::sync::atomic::Ordering::SeqCst) {
                    barge_aborted_clone.store(true, std::sync::atomic::Ordering::SeqCst);
                    log::info!("[Barge-in] User speech detected during TTS, aborting turn...");
                    let _ = bm_for_barge.abort();
                    tts_for_barge.stop();
                    // Unpause so the new utterance gets processed normally
                    rm_for_barge.set_continuous_mode_paused(false);
                    let _ = app_for_barge.emit("brain:barge-in", ());
                }
            });

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
            app_clone.unlisten(barge_listener);

            if barge_aborted.load(std::sync::atomic::Ordering::Relaxed) {
                log::info!("TTS turn aborted by barge-in.");
            } else {
                log::info!("TTS playback finished normally.");
            }
        }
    });

    // 6. Resume continuous listening
    // Check if auto-listen is enabled; if not, automatically restart
    let settings = get_settings(app);
    if settings.brain.auto_listen {
        rm_for_after.set_continuous_mode_paused(false);
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
