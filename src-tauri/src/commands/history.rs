use crate::actions::process_transcription_output;
use crate::managers::{
    history::{HistoryEntry, HistoryManager, PaginatedHistory},
    transcription::TranscriptionManager,
};
use std::sync::Arc;
use tauri::{AppHandle, State};

/// Decode a history recording on the blocking pool: a raw native-rate WAV is
/// resampled to 16 kHz here, too much work to hold an async worker for.
async fn load_recording_samples(audio_path: std::path::PathBuf) -> Result<Vec<f32>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::audio_toolkit::read_wav_samples(&audio_path)
    })
    .await
    .map_err(|e| format!("Audio load task panicked: {e}"))?
    .map_err(|e| format!("Failed to load audio: {}", e))
}

/// Run one history database call on the blocking pool: every query is
/// synchronous rusqlite work (and may wait on the 5 s busy timeout), which must
/// not hold an async worker.
async fn on_db<T: Send + 'static>(
    history_manager: &Arc<HistoryManager>,
    task: impl FnOnce(&HistoryManager) -> anyhow::Result<T> + Send + 'static,
) -> Result<T, String> {
    let hm = Arc::clone(history_manager);
    tauri::async_runtime::spawn_blocking(move || task(&hm))
        .await
        .map_err(|e| format!("History task panicked: {e}"))?
        .map_err(|e| e.to_string())
}

/// Apply the retention settings on the blocking pool: row deletes, recording
/// file deletes and a VACUUM.
async fn cleanup_history(history_manager: &Arc<HistoryManager>) -> Result<(), String> {
    let hm = Arc::clone(history_manager);
    tauri::async_runtime::spawn_blocking(move || hm.cleanup_old_entries())
        .await
        .map_err(|e| format!("History cleanup task panicked: {e}"))?
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn get_history_entries(
    _app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
    cursor: Option<i32>,
    limit: Option<u32>,
) -> Result<PaginatedHistory, String> {
    on_db(&history_manager, move |hm| {
        hm.get_history_entries(cursor, limit)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn toggle_history_entry_saved(
    _app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
    id: i32,
) -> Result<(), String> {
    on_db(&history_manager, move |hm| hm.toggle_saved_status(id)).await
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
    let hm = Arc::clone(history_manager.inner());
    tauri::async_runtime::spawn_blocking(move || hm.delete_entry(id))
        .await
        .map_err(|e| format!("History delete task panicked: {e}"))?
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn delete_all_recordings(
    _app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
) -> Result<(), String> {
    let hm = Arc::clone(history_manager.inner());
    tauri::async_runtime::spawn_blocking(move || hm.delete_all_recordings())
        .await
        .map_err(|e| format!("History delete task panicked: {e}"))?
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
    let entry = on_db(&history_manager, move |hm| hm.get_entry_by_id(id))
        .await?
        .ok_or_else(|| format!("History entry {} not found", id))?;

    let audio_path = history_manager.get_audio_file_path(&entry.file_name);
    let samples = load_recording_samples(audio_path).await?;

    if samples.is_empty() {
        return Err("Recording has no audio samples".to_string());
    }

    let settings = crate::settings::get_settings(&app);
    let statistics_run = statistics_manager.begin_history_retry(id.into());
    let speech_ms = entry
        .speech_duration_ms
        .map(|d| d as i64)
        .unwrap_or_else(|| (samples.len() as i64 * 1000) / 16000);
    statistics_run.set_speech_audio_duration_ms(speech_ms, 16000);
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

    on_db(&history_manager, move |hm| {
        hm.update_entry_full(
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
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn post_process_history_entry(
    app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
    id: i32,
) -> Result<(), String> {
    let entry = on_db(&history_manager, move |hm| hm.get_entry_by_id(id))
        .await?
        .ok_or_else(|| format!("History entry {} not found", id))?;

    // A Multi-STT entry keeps the per-model dump in transcription_text; the
    // text the user reads (and wants polished) is the merged result. Feeding
    // the dump to the LLM produced a "cleanup" of four transcripts at once.
    let source = post_process_source(&entry).trim().to_string();
    if source.is_empty() {
        return Err("Transcription text is empty".to_string());
    }

    let post_process_start = std::time::Instant::now();
    let processed = process_transcription_output(&app, &source, true).await;
    let post_processing_latency_ms = Some(post_process_start.elapsed().as_secs_f64() * 1000.0);

    // No result means the provider was unreachable, disabled, or returned
    // nothing. Leave the entry exactly as it was — writing None here used to
    // erase the merged Multi-STT text and expose the raw dump.
    let Some(polished) = processed.post_processed_text else {
        return Err(
            "Post-processing returned nothing: the LLM provider is unreachable or disabled. \
             The entry was left unchanged."
                .to_string(),
        );
    };

    on_db(&history_manager, move |hm| {
        hm.update_entry_full(
            id,
            entry.transcription_text,
            Some(polished),
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
    })
    .await
}

/// The text a History post-process pass should polish: the raw transcript
/// for a normal entry, the merged result for a Multi-STT entry (its
/// `transcription_text` is the per-model dump). Falls back to the dump's
/// "=== Merged ===" section when no merged text is stored.
fn post_process_source(entry: &crate::managers::history::HistoryEntry) -> String {
    if entry.mode.as_deref() != Some("multi_stt") {
        return entry.transcription_text.clone();
    }
    if let Some(merged) = entry
        .post_processed_text
        .as_deref()
        .filter(|t| !t.trim().is_empty())
    {
        return merged.to_string();
    }
    merged_section_of_dump(&entry.transcription_text).to_string()
}

/// Text after the "=== Merged ===" marker of a Multi-STT dump, or the whole
/// input when the marker is absent.
fn merged_section_of_dump(dump: &str) -> &str {
    const MARKER: &str = "=== Merged ===";
    match dump.rfind(MARKER) {
        Some(idx) => dump[idx + MARKER.len()..].trim_start_matches(['\r', '\n']),
        None => dump,
    }
}

#[cfg(test)]
mod post_process_source_tests {
    use super::merged_section_of_dump;

    #[test]
    fn merged_section_is_extracted_from_a_dump() {
        let dump = "=== Multi-STT Results ===\nModel 1: a\nhello\nModel 2: b\nhello there\n=== Merged ===\nhello there";
        assert_eq!(merged_section_of_dump(dump), "hello there");
        assert_eq!(merged_section_of_dump("plain text"), "plain text");
    }
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
    let entry = on_db(&history_manager, move |hm| hm.get_entry_by_id(id))
        .await?
        .ok_or_else(|| format!("History entry {} not found", id))?;

    let audio_path = history_manager.get_audio_file_path(&entry.file_name);
    let samples = load_recording_samples(audio_path).await?;

    if samples.is_empty() {
        return Err("Recording has no audio samples".to_string());
    }

    let settings = crate::settings::get_settings(&app);
    // Slot-positional: `${output2}` is always model 2, whatever else is set.
    let extra_models: Vec<Option<String>> = settings.multi_stt_extra_model_ids();

    // The primary may have been unloaded while idle: start its load (a no-op
    // when it is loaded; `transcribe_tracked` waits for it) alongside the
    // extras'. A primary that fails instantly would otherwise end the shared
    // statistics run before the extras could register their attempts.
    transcription_manager.initiate_model_load();
    let load_handles: Vec<_> = extra_models
        .iter()
        .flatten()
        .filter(|m| !transcription_manager.is_extra_model_loaded(m))
        .map(|m| {
            let tm = Arc::clone(&transcription_manager);
            let m = m.clone();
            tauri::async_runtime::spawn_blocking(move || {
                let _ = tm.load_extra_model(&m);
            })
        })
        .collect();
    for handle in load_handles {
        let _ = handle.await;
    }

    let statistics_run = statistics_manager.begin_history_retry(id.into());
    let speech_ms = entry
        .speech_duration_ms
        .map(|d| d as i64)
        .unwrap_or_else(|| (samples.len() as i64 * 1000) / 16000);
    statistics_run.set_speech_audio_duration_ms(speech_ms, 16000);
    statistics_run.mark_input_stopped(std::time::Instant::now(), true);

    let transcribe_start = std::time::Instant::now();
    let primary_task = {
        let tm = Arc::clone(&transcription_manager);
        let samples = samples.clone();
        let stats = statistics_run.clone();
        tauri::async_runtime::spawn_blocking(move || tm.transcribe_tracked(samples, stats).ok())
    };
    let extra_tasks: Vec<_> = extra_models
        .iter()
        .map(|model| {
            model.clone().map(|m| {
                let tm = Arc::clone(&transcription_manager);
                let samples = samples.clone();
                let stats = statistics_run.clone();
                tauri::async_runtime::spawn_blocking(move || {
                    if tm.is_extra_model_loaded(&m) {
                        tm.transcribe_with_extra_tracked(&m, samples, stats).ok()
                    } else {
                        None
                    }
                })
            })
        })
        .collect();

    let primary_tracked = primary_task.await.unwrap_or(None);
    let mut extra_tracked = Vec::with_capacity(extra_tasks.len());
    for task in extra_tasks {
        extra_tracked.push(match task {
            Some(t) => t.await.unwrap_or(None),
            None => None,
        });
    }

    let text_of = |tracked: &Option<crate::managers::transcription::TrackedTranscription>| {
        tracked.as_ref().map(|t| t.text.clone()).unwrap_or_default()
    };
    // One text per slot, Model 1 first.
    let outputs: Vec<String> = std::iter::once(text_of(&primary_tracked))
        .chain(extra_tracked.iter().map(text_of))
        .collect();
    let output_refs: Vec<&str> = outputs.iter().map(String::as_str).collect();

    let multi_transcription_latency_ms = transcribe_start.elapsed().as_secs_f64() * 1000.0;

    let merge_start = std::time::Instant::now();
    let merge_outcome =
        crate::actions::multi_stt_merge_transcriptions(&settings, &output_refs, None).await;

    let latency = merge_start.elapsed().as_secs_f64() * 1000.0;
    let merge_latency_ms = Some(latency);

    // Close every attempt and the run, as `MultiSttAction` does: an attempt
    // left open is never written, so the reprocess never reached Statistics.
    for tracked in std::iter::once(primary_tracked)
        .chain(extra_tracked)
        .flatten()
    {
        tracked.attempt.complete_post_processing();
        tracked
            .attempt
            .finish(crate::managers::statistics::StatisticsRunStatus::Success);
    }
    statistics_run.finish(crate::managers::statistics::StatisticsRunStatus::Success);

    let (merged, brain_details) = match merge_outcome {
        Some(outcome) => {
            let brain = crate::actions::MultiSttHistoryBrain {
                provider_id: outcome.provider_id,
                provider_label: outcome.provider_label,
                model_name: outcome.model_name,
                prompt_name: outcome.prompt_name,
                latency_ms: Some(latency),
                raw_output: outcome.raw_text,
                cleaned_output: outcome.cleaned_text.clone(),
            };
            (outcome.cleaned_text, Some(brain))
        }
        None => (crate::actions::join_nonempty_lines(&output_refs), None),
    };

    let model_ids: Vec<String> = std::iter::once(settings.selected_model.clone())
        .chain(
            extra_models
                .iter()
                .map(|id| id.clone().unwrap_or_else(|| "none".to_string())),
        )
        .collect();
    let history_models: Vec<(&str, &str)> = model_ids
        .iter()
        .map(String::as_str)
        .zip(output_refs.iter().copied())
        .collect();
    let multi_transcript = crate::actions::format_multi_stt_history_transcript(
        &history_models,
        brain_details,
        &merged,
    );

    let merge_prompt_text = settings
        .multi_stt_merge_prompt
        .as_ref()
        .map(|p| p.prompt.clone());

    let extra_models_list: Vec<String> = extra_models.iter().flatten().cloned().collect();

    on_db(&history_manager, move |hm| {
        hm.update_entry_full(
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
    })
    .await
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

    cleanup_history(&history_manager).await
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

    cleanup_history(&history_manager).await
}

/// Return the most recent history entry with non-empty transcription text,
/// so the frontend can tell the user which recording will be used as the
/// benchmark reference before starting the run.
#[tauri::command]
#[specta::specta]
pub async fn get_latest_recording_info(
    history_manager: State<'_, Arc<HistoryManager>>,
) -> Result<Option<HistoryEntry>, String> {
    on_db(&history_manager, |hm| hm.get_latest_completed_entry()).await
}
