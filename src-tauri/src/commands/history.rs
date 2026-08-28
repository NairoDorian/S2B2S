use crate::actions::process_transcription_output;
use crate::managers::{
    history::{HistoryEntry, HistoryManager, PaginatedHistory},
    transcription::TranscriptionManager,
};
use std::sync::Arc;
use tauri::{AppHandle, State};

#[tauri::command]
#[specta::specta]
pub async fn get_history_entries(
    _app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
    cursor: Option<i32>,
    limit: Option<u32>,
) -> Result<PaginatedHistory, String> {
    history_manager
        .get_history_entries(cursor, limit)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn toggle_history_entry_saved(
    _app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
    id: i32,
) -> Result<(), String> {
    history_manager
        .toggle_saved_status(id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn get_audio_file_path(
    _app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
    file_name: String,
) -> Result<String, String> {
    let path = history_manager.get_audio_file_path(&file_name);
    path.to_str()
        .ok_or_else(|| "Invalid file path".to_string())
        .map(|s| s.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn delete_history_entry(
    _app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
    id: i32,
) -> Result<(), String> {
    history_manager
        .delete_entry(id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn delete_all_recordings(
    _app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
) -> Result<(), String> {
    history_manager
        .delete_all_recordings()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn retry_history_entry_transcription(
    app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
    transcription_manager: State<'_, Arc<TranscriptionManager>>,
    statistics_manager: State<'_, Arc<crate::managers::statistics::StatisticsManager>>,
    id: i32,
) -> Result<(), String> {
    let entry = history_manager
        .get_entry_by_id(id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("History entry {} not found", id))?;

    let audio_path = history_manager.get_audio_file_path(&entry.file_name);
    let samples = crate::audio_toolkit::read_wav_samples(&audio_path)
        .map_err(|e| format!("Failed to load audio: {}", e))?;

    if samples.is_empty() {
        return Err("Recording has no audio samples".to_string());
    }

    let settings = crate::settings::get_settings(&app);
    let statistics_run = statistics_manager.begin_history_retry(id.into());
    statistics_run.set_audio(samples.len(), 16000);
    statistics_run.mark_input_stopped(std::time::Instant::now(), entry.post_process_requested);

    transcription_manager.initiate_model_load();

    let stt_start = std::time::Instant::now();
    let tm = Arc::clone(&transcription_manager);
    let tracked_res = tauri::async_runtime::spawn_blocking(move || {
        tm.transcribe_tracked(samples, statistics_run.clone())
    })
    .await
    .map_err(|e| format!("Transcription task panicked: {}", e))?;

    let tracked = match tracked_res {
        Ok(t) => t,
        Err(e) => {
            return Err(e.to_string());
        }
    };

    let transcription = tracked.text;
    if transcription.is_empty() {
        tracked
            .attempt
            .finish(crate::managers::statistics::StatisticsRunStatus::Empty);
        return Err("Recording contains no speech".to_string());
    }

    let stt_latency_ms = stt_start.elapsed().as_secs_f64() * 1000.0;
    let post_process_start = std::time::Instant::now();

    let processed =
        process_transcription_output(&app, &transcription, entry.post_process_requested).await;

    let post_processing_latency_ms = if entry.post_process_requested {
        Some(post_process_start.elapsed().as_secs_f64() * 1000.0)
    } else {
        None
    };

    tracked.attempt.complete_post_processing();
    tracked
        .attempt
        .finish(crate::managers::statistics::StatisticsRunStatus::Success);

    history_manager
        .update_entry_full(
            id,
            transcription,
            processed.post_processed_text,
            processed.post_process_prompt,
            Some(entry.post_process_requested),
            Some(settings.selected_model),
            None,
            Some(stt_latency_ms),
            post_processing_latency_ms,
            Some("single".to_string()),
            None,
        )
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn post_process_history_entry(
    app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
    id: i32,
) -> Result<(), String> {
    let entry = history_manager
        .get_entry_by_id(id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("History entry {} not found", id))?;

    if entry.transcription_text.trim().is_empty() {
        return Err("Transcription text is empty".to_string());
    }

    let post_process_start = std::time::Instant::now();
    let processed = process_transcription_output(&app, &entry.transcription_text, true).await;
    let post_processing_latency_ms = Some(post_process_start.elapsed().as_secs_f64() * 1000.0);

    history_manager
        .update_entry_full(
            id,
            entry.transcription_text,
            processed.post_processed_text,
            processed.post_process_prompt,
            Some(true),
            None,
            None,
            None,
            post_processing_latency_ms,
            None,
            None,
        )
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn multi_stt_history_entry(
    app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
    transcription_manager: State<'_, Arc<TranscriptionManager>>,
    statistics_manager: State<'_, Arc<crate::managers::statistics::StatisticsManager>>,
    id: i32,
) -> Result<(), String> {
    let entry = history_manager
        .get_entry_by_id(id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("History entry {} not found", id))?;

    let audio_path = history_manager.get_audio_file_path(&entry.file_name);
    let samples = crate::audio_toolkit::read_wav_samples(&audio_path)
        .map_err(|e| format!("Failed to load audio: {}", e))?;

    if samples.is_empty() {
        return Err("Recording has no audio samples".to_string());
    }

    let settings = crate::settings::get_settings(&app);
    let extra_model_2 = settings.multi_stt_model_2.clone().filter(|m| !m.is_empty());
    let extra_model_3 = settings.multi_stt_model_3.clone().filter(|m| !m.is_empty());
    let extra_model_4 = settings.multi_stt_model_4.clone().filter(|m| !m.is_empty());

    // Preload models if needed
    let tm_load_2 = Arc::clone(&transcription_manager);
    let tm_load_3 = Arc::clone(&transcription_manager);
    let tm_load_4 = Arc::clone(&transcription_manager);

    let need_load_2 = extra_model_2
        .as_ref()
        .is_some_and(|m| !tm_load_2.is_extra_model_loaded(m));
    let need_load_3 = extra_model_3
        .as_ref()
        .is_some_and(|m| !tm_load_3.is_extra_model_loaded(m));
    let need_load_4 = extra_model_4
        .as_ref()
        .is_some_and(|m| !tm_load_4.is_extra_model_loaded(m));

    let load_handle_2 = if need_load_2 {
        let m = extra_model_2.clone().unwrap();
        Some(tauri::async_runtime::spawn_blocking(move || {
            let _ = tm_load_2.load_extra_model(&m);
        }))
    } else {
        None
    };
    let load_handle_3 = if need_load_3 {
        let m = extra_model_3.clone().unwrap();
        Some(tauri::async_runtime::spawn_blocking(move || {
            let _ = tm_load_3.load_extra_model(&m);
        }))
    } else {
        None
    };
    let load_handle_4 = if need_load_4 {
        let m = extra_model_4.clone().unwrap();
        Some(tauri::async_runtime::spawn_blocking(move || {
            let _ = tm_load_4.load_extra_model(&m);
        }))
    } else {
        None
    };

    if let Some(h) = load_handle_2 {
        let _ = h.await;
    }
    if let Some(h) = load_handle_3 {
        let _ = h.await;
    }
    if let Some(h) = load_handle_4 {
        let _ = h.await;
    }

    let statistics_run = statistics_manager.begin_history_retry(id.into());
    statistics_run.set_audio(samples.len(), 16000);
    statistics_run.mark_input_stopped(std::time::Instant::now(), true);

    let transcribe_start = std::time::Instant::now();
    let tm1 = Arc::clone(&transcription_manager);
    let tm2 = Arc::clone(&transcription_manager);
    let tm3 = Arc::clone(&transcription_manager);
    let tm4 = Arc::clone(&transcription_manager);

    let s1 = samples.clone();
    let s2 = samples.clone();
    let s3 = samples.clone();
    let s4 = samples.clone();

    let stats1 = statistics_run.clone();
    let stats2 = statistics_run.clone();
    let stats3 = statistics_run.clone();
    let stats4 = statistics_run.clone();

    let task1 =
        tauri::async_runtime::spawn_blocking(move || tm1.transcribe_tracked(s1, stats1).ok());

    let task2 = extra_model_2.clone().map(|m| {
        tauri::async_runtime::spawn_blocking(move || {
            if tm2.is_extra_model_loaded(&m) {
                tm2.transcribe_with_extra_tracked(&m, s2, stats2).ok()
            } else {
                None
            }
        })
    });

    let task3 = extra_model_3.clone().map(|m| {
        tauri::async_runtime::spawn_blocking(move || {
            if tm3.is_extra_model_loaded(&m) {
                tm3.transcribe_with_extra_tracked(&m, s3, stats3).ok()
            } else {
                None
            }
        })
    });

    let task4 = extra_model_4.clone().map(|m| {
        tauri::async_runtime::spawn_blocking(move || {
            if tm4.is_extra_model_loaded(&m) {
                tm4.transcribe_with_extra_tracked(&m, s4, stats4).ok()
            } else {
                None
            }
        })
    });

    let tracked1 = task1.await.unwrap_or(None);
    let tracked2 = match task2 {
        Some(t) => t.await.unwrap_or(None),
        None => None,
    };
    let tracked3 = match task3 {
        Some(t) => t.await.unwrap_or(None),
        None => None,
    };
    let tracked4 = match task4 {
        Some(t) => t.await.unwrap_or(None),
        None => None,
    };

    let output1 = tracked1
        .as_ref()
        .map(|t| t.text.as_str())
        .unwrap_or("")
        .to_string();
    let output2 = tracked2
        .as_ref()
        .map(|t| t.text.as_str())
        .unwrap_or("")
        .to_string();
    let output3 = tracked3
        .as_ref()
        .map(|t| t.text.as_str())
        .unwrap_or("")
        .to_string();
    let output4 = tracked4
        .as_ref()
        .map(|t| t.text.as_str())
        .unwrap_or("")
        .to_string();

    let multi_transcription_latency_ms = transcribe_start.elapsed().as_secs_f64() * 1000.0;

    let merge_start = std::time::Instant::now();
    let merged_opt = crate::actions::multi_stt_merge_transcriptions(
        &settings, &output1, &output2, &output3, &output4,
    )
    .await;

    let merged = merged_opt.unwrap_or_else(|| {
        let mut combined = output1.clone();
        for out in [&output2, &output3, &output4] {
            if !out.is_empty() {
                if !combined.is_empty() {
                    combined.push('\n');
                }
                combined.push_str(out);
            }
        }
        combined
    });

    let merge_latency_ms = Some(merge_start.elapsed().as_secs_f64() * 1000.0);

    let multi_transcript = format!(
        "=== Multi-STT Results ===\nModel 1: {}\n{}\nModel 2: {}\n{}\nModel 3: {}\n{}\nModel 4: {}\n{}\n=== Merged ===\n{}",
        settings.selected_model,
        output1,
        settings.multi_stt_model_2.as_deref().unwrap_or("none"),
        output2,
        settings.multi_stt_model_3.as_deref().unwrap_or("none"),
        output3,
        settings.multi_stt_model_4.as_deref().unwrap_or("none"),
        output4,
        merged
    );

    let merge_prompt_text = settings
        .multi_stt_merge_prompt
        .as_ref()
        .map(|p| p.prompt.clone());

    let extra_models_list: Vec<String> = [
        settings.multi_stt_model_2.clone(),
        settings.multi_stt_model_3.clone(),
        settings.multi_stt_model_4.clone(),
    ]
    .into_iter()
    .flatten()
    .collect();

    history_manager
        .update_entry_full(
            id,
            multi_transcript,
            Some(merged),
            merge_prompt_text,
            Some(true),
            Some(settings.selected_model.clone()),
            None,
            Some(multi_transcription_latency_ms),
            merge_latency_ms,
            Some("multi_stt".to_string()),
            Some(extra_models_list),
        )
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn update_history_limit(
    app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
    limit: u32,
) -> Result<(), String> {
    let mut settings = crate::settings::get_settings(&app);
    settings.history_limit = limit;
    crate::settings::write_settings(&app, settings);

    history_manager
        .cleanup_old_entries()
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn update_recording_retention_period(
    app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
    period: String,
) -> Result<(), String> {
    use crate::settings::RecordingRetentionPeriod;

    let retention_period = match period.as_str() {
        "never" => RecordingRetentionPeriod::Never,
        "preserve_limit" => RecordingRetentionPeriod::PreserveLimit,
        "days3" => RecordingRetentionPeriod::Days3,
        "weeks2" => RecordingRetentionPeriod::Weeks2,
        "months3" => RecordingRetentionPeriod::Months3,
        _ => return Err(format!("Invalid retention period: {}", period)),
    };

    let mut settings = crate::settings::get_settings(&app);
    settings.recording_retention_period = retention_period;
    crate::settings::write_settings(&app, settings);

    history_manager
        .cleanup_old_entries()
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Return the most recent history entry with non-empty transcription text,
/// so the frontend can tell the user which recording will be used as the
/// benchmark reference before starting the run.
#[tauri::command]
#[specta::specta]
pub async fn get_latest_recording_info(
    history_manager: State<'_, Arc<HistoryManager>>,
) -> Result<Option<HistoryEntry>, String> {
    history_manager
        .get_latest_completed_entry()
        .map_err(|e| e.to_string())
}
