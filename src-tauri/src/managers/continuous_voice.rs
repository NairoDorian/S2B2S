use crate::brain::manager::BrainManager;
use crate::managers::audio::AudioRecordingManager;
use crate::managers::history::HistoryManager;
use crate::managers::transcription::TranscriptionManager;
use crate::settings::get_settings;
use crate::speculative_turns::SpeculativeTurnTracker;
use crate::tts::manager::TtsManager;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Listener, Manager};

/// Run the STT → Brain → TTS pipeline for one continuous-voice utterance.
///
/// `turn_id` / `revision` come from the recorder's speculative turn tracker:
/// every stage re-checks `is_latest` before publishing side effects, so a
/// reopened utterance (speech resumed inside the reopen grace) or a barge-in
/// makes this pipeline's remaining work stale at the earliest gate.
pub fn process_continuous_samples(
    app: &AppHandle,
    samples: Vec<f32>,
    turn_id: u64,
    revision: u32,
) -> Result<(), String> {
    let tracker = app
        .try_state::<Arc<SpeculativeTurnTracker>>()
        .map(|s| s.inner().clone());

    // Stale turn (barge-in / reopen happened between endpoint and spawn):
    // don't touch the listening state — a newer pipeline owns it now.
    if let Some(tracker) = &tracker {
        if !tracker.is_latest(turn_id, revision) {
            log::info!(
                "[ContinuousVoice] Dropping stale pipeline (turn {turn_id}, rev {revision})"
            );
            return Ok(());
        }
    }

    log::info!(
        "Continuous voice pipeline started with {} samples (turn {turn_id}, rev {revision})",
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

    let settings = get_settings(app);
    // Barge-in during TTS playback is only safe with headphones: on speakers,
    // the assistant's own voice echoes into the mic and would abort its own
    // turn. `headphone_mode` gates the barge-in abort listener.
    let barge_in_enabled = settings.brain.headphone_mode;

    // 1. Temporarily pause continuous listening (prevents new utterance queuing)
    //    but VAD stays active so we can detect barge-in speech during the
    //    whole pipeline (STT, Brain streaming, TTS playback).
    rm.set_continuous_mode_paused(true);

    // 2. Register the barge-in listener up-front so speech during ANY stage
    //    aborts this turn (speculative-turn semantics: the new utterance gets
    //    a fresh turn and this one is cancelled).
    let barge_aborted = Arc::new(AtomicBool::new(false));
    let barge_listener = if barge_in_enabled {
        let barge_aborted_clone = barge_aborted.clone();
        let app_for_barge = app.clone();
        let bm_for_barge = bm.clone();
        let tts_for_barge = tts.clone();
        let rm_for_barge = rm.clone();
        let tracker_for_barge = tracker.clone();

        Some(app.listen("continuous-voice:speech-started", move |_| {
            if !barge_aborted_clone.load(std::sync::atomic::Ordering::SeqCst) {
                barge_aborted_clone.store(true, std::sync::atomic::Ordering::SeqCst);
                log::info!("[Barge-in] User speech detected, aborting turn {turn_id}...");
                bm_for_barge.abort();
                tts_for_barge.stop();
                if let Some(tracker) = &tracker_for_barge {
                    tracker.cancel(turn_id);
                }
                // Unpause so the new utterance gets processed normally
                rm_for_barge.set_continuous_mode_paused(false);
                let _ = app_for_barge.emit("brain:barge-in", ());
            }
        }))
    } else {
        None
    };

    // 3. Transcribe
    let is_brain_only = settings.brain.brain_only_transcription;
    let stt_start = std::time::Instant::now();
    let transcription_result = if is_brain_only {
        Ok("[STT Bypassed]".to_string())
    } else {
        tm.transcribe(samples.clone())
    };
    let stt_ms = stt_start.elapsed().as_millis() as u64;

    // Turn superseded while STT ran (barge-in): discard the result.
    if let Some(tracker) = &tracker {
        if !tracker.is_latest(turn_id, revision) {
            log::info!("[ContinuousVoice] Turn {turn_id} superseded during STT; discarding");
            rm.set_continuous_mode_paused(false);
            if let Some(listener) = barge_listener {
                app.unlisten(listener);
            }
            return Ok(());
        }
    }

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
            if let Some(listener) = barge_listener {
                app.unlisten(listener);
            }
            return Err(format!("Transcription failed: {e}"));
        }
    };

    if transcription.is_empty() {
        log::info!("Empty transcription; skipping Brain query and resuming listening.");
        rm.set_continuous_mode_paused(false);
        if let Some(listener) = barge_listener {
            app.unlisten(listener);
        }
        return Ok(());
    }

    // ITN: spoken → written normalization for continuous voice (conversation mode)
    let transcription = crate::tts::sanitize::post_stt_normalize(&transcription);
    // User-defined text replacement rules.
    let transcription =
        crate::text_replacement::apply_replacements_from_settings(app, &transcription);

    // 4. Save STT entry in history
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

    // 5. Emit brain:asked to display on frontend with STT timing
    let asked_payload = serde_json::json!({
        "text": transcription,
        "stt_ms": stt_ms,
        "turn_id": turn_id,
    });
    let _ = app.emit("brain:asked", &asked_payload);

    // 6. Query Brain and play TTS
    let will_play_tts = settings.brain.read_aloud && settings.tts.enabled;
    let multimodal_audio = settings.brain.multimodal_audio_enabled;

    // When brain-only transcription is on, bypass STT output and send the
    // fixed transcription prompt + raw audio to the Brain instead.
    let brain_text = if is_brain_only {
        crate::settings::BRAIN_ONLY_TRANSCRIPTION_PROMPT.to_string()
    } else {
        transcription.clone()
    };
    // Brain-only mode always requires multimodal audio
    let multimodal_audio = is_brain_only || multimodal_audio;

    let reply_language = crate::actions::resolve_reply_language(app, &settings);
    let app_clone = app.clone();
    let bm_clone = bm.clone();
    let transcription_clone = brain_text.clone();
    let samples_for_brain = if multimodal_audio {
        Some(samples.clone())
    } else {
        None
    };

    // Clone for async block — rm used both inside and after
    let rm_for_after = rm.clone();

    // Run the async Brain/TTS pipeline
    tauri::async_runtime::block_on(async move {
        // Superseded before the Brain call? Don't even ask.
        if let Some(tracker) = &tracker {
            if !tracker.is_latest(turn_id, revision) {
                log::info!("[ContinuousVoice] Turn {turn_id} superseded before Brain ask");
                return;
            }
        }

        let ask_result = if multimodal_audio {
            match samples_for_brain {
                Some(samples) => {
                    if is_brain_only {
                        log::info!(
                            "[ContinuousVoice] Brain-only transcription mode — encoding {} samples ({:.2}s) to WAV, bypassing STT, sending fixed prompt + audio to Gemma 4",
                            samples.len(),
                            samples.len() as f64 / 16000.0
                        );
                    } else {
                        log::info!(
                            "[ContinuousVoice] Multimodal audio enabled — encoding {} samples ({:.2}s) to WAV for Gemma 4",
                            samples.len(),
                            samples.len() as f64 / 16000.0
                        );
                    }
                    match crate::audio_toolkit::encode_wav_bytes(&samples) {
                        Ok(wav_bytes) => {
                            use base64::Engine;
                            let b64 = base64::engine::general_purpose::STANDARD.encode(&wav_bytes);
                            log::info!(
                                "[ContinuousVoice] WAV encoded — {} bytes raw, {} base64 — sending to ask_multimodal",
                                wav_bytes.len(),
                                b64.len()
                            );
                            bm_clone
                                .ask_multimodal(
                                    transcription_clone,
                                    Some(b64),
                                    None,
                                    reply_language.clone(),
                                    Vec::new(),
                                )
                                .await
                        }
                        Err(e) => {
                            log::error!(
                                "[ContinuousVoice] Failed to encode WAV for multimodal brain: {e}"
                            );
                            bm_clone.ask(transcription_clone).await
                        }
                    }
                }
                None => {
                    log::error!("[ContinuousVoice] samples_for_brain unexpectedly missing; falling back to text-only ask");
                    bm_clone.ask(transcription_clone).await
                }
            }
        } else {
            log::info!("[ContinuousVoice] Multimodal audio disabled — text-only ask");
            bm_clone.ask(transcription_clone).await
        };
        // Whether the Brain produced anything to speak this turn.
        let has_reply = ask_result
            .as_ref()
            .map(|t| !t.trim().is_empty())
            .unwrap_or(false);

        // Commit gate (speculative turns): irreversible work (waiting out the
        // TTS playback and holding the mic) only starts if this revision is
        // still the newest. A barge-in makes the commit fail — the listener
        // already unpaused listening and cancelled the turn.
        if will_play_tts && has_reply {
            let committed = tracker
                .as_ref()
                .map(|t| t.commit_if_latest(turn_id, revision))
                .unwrap_or(true);
            if !committed {
                log::info!(
                    "[ContinuousVoice] Turn {turn_id} superseded after Brain; skipping TTS wait"
                );
                return;
            }
        }

        // Do NOT gate on `tts.is_playing()`: TTS synthesis is asynchronous (sentences
        // are queued during streaming and synthesized on a background thread), so when
        // ask() returns the audio often hasn't started playing yet and is_playing()
        // reads false. Gating on it skipped the wait/barge-in block entirely and made
        // the assistant listen over its own speech. The terminal TTS event
        // (tts:finished/stopped/error) for the LAST queued sentence fires after this
        // point, so registering the listeners now and waiting for it is race-free.
        if will_play_tts && has_reply {
            log::info!(
                "Waiting for TTS playback to finish{}...",
                if barge_in_enabled {
                    " (barge-in active)"
                } else {
                    " (barge-in disabled — headphone mode off)"
                }
            );

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

            if barge_aborted.load(std::sync::atomic::Ordering::Relaxed) {
                log::info!("TTS turn aborted by barge-in.");
            } else {
                log::info!("TTS playback finished normally.");
            }
        }
    });

    // 7. Unregister the pipeline-wide barge listener.
    if let Some(listener) = barge_listener {
        app.unlisten(listener);
    }

    // 8. Resume continuous listening
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
