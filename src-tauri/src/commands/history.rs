use crate::actions::process_transcription_output;
use crate::managers::{
    history::{HistoryManager, PaginatedHistory},
    transcription::TranscriptionManager,
};
use std::sync::Arc;
use tauri::{AppHandle, Manager, State};

#[tauri::command]
#[specta::specta]
pub async fn get_history_entries(
    _app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
    cursor: Option<i32>,
    limit: Option<u32>,
) -> Result<PaginatedHistory, String> {
    // 32-bit over the IPC boundary: specta forbids 64-bit ints in TS bindings.
    history_manager
        .get_history_entries(cursor.map(i64::from), limit.map(|l| l as usize))
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
        .toggle_saved_status(i64::from(id))
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
pub async fn delete_all_history_entries(
    _app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
) -> Result<u32, String> {
    history_manager
        .delete_all_entries()
        .await
        .map(|c| c as u32)
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn delete_history_entry(
    _app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
    id: i32,
) -> Result<(), String> {
    history_manager
        .delete_entry(i64::from(id))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn retry_history_entry_transcription(
    app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
    transcription_manager: State<'_, Arc<TranscriptionManager>>,
    id: i32,
) -> Result<(), String> {
    let id = i64::from(id);
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

    transcription_manager.initiate_model_load();

    let tm = Arc::clone(&transcription_manager);
    let transcription = tauri::async_runtime::spawn_blocking(move || tm.transcribe(samples))
        .await
        .map_err(|e| format!("Transcription task panicked: {}", e))?
        .map_err(|e| e.to_string())?;

    if transcription.is_empty() {
        return Err("Recording contains no speech".to_string());
    }

    let processed =
        process_transcription_output(&app, &transcription, entry.post_process_requested).await;
    history_manager
        .update_transcription(
            id,
            transcription,
            processed.post_processed_text,
            processed.post_process_prompt,
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
    settings.history_limit = limit as usize;
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

#[tauri::command]
#[specta::specta]
pub async fn delete_history_entries(
    _app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
    ids: Vec<i32>,
) -> Result<(), String> {
    for id in ids {
        history_manager
            .delete_entry(i64::from(id))
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn export_history_entries(
    _app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
    ids: Vec<i32>,
    file_path: String,
) -> Result<(), String> {
    use std::fs::File;
    use std::io::Write;

    let mut markdown = format!(
        "# S2B2S History Export\nExported on: {}\n\n---\n\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    );

    for id in ids {
        let entry = history_manager
            .get_entry_by_id(i64::from(id))
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("History entry {} not found", id))?;

        let date_str = chrono::DateTime::from_timestamp(entry.timestamp, 0)
            .map(|dt| {
                dt.with_timezone(&chrono::Local)
                    .format("%Y-%m-%d %H:%M:%S")
                    .to_string()
            })
            .unwrap_or_else(|| "Unknown".to_string());

        let duration_str = entry
            .duration_ms
            .map(|ms| format!("{:.2}s", ms as f64 / 1000.0))
            .unwrap_or_else(|| "N/A".to_string());

        markdown.push_str(&format!(
            "## Entry #{} - [{}]\n",
            entry.id,
            entry.entry_type.to_uppercase()
        ));
        markdown.push_str(&format!("- **Timestamp:** {}\n", date_str));
        markdown.push_str(&format!(
            "- **Model:** {}\n",
            entry.model_name.as_deref().unwrap_or("Unknown")
        ));
        markdown.push_str(&format!("- **Duration:** {}\n\n", duration_str));

        markdown.push_str("### Raw Text\n");
        markdown.push_str(&format!("{}\n\n", entry.transcription_text));

        if let Some(ref ppt) = entry.post_processed_text {
            markdown.push_str("### Post-Processed Text\n");
            markdown.push_str(&format!("{}\n\n", ppt));
        }

        markdown.push_str("---\n\n");
    }

    let mut file =
        File::create(&file_path).map_err(|e| format!("Failed to create export file: {}", e))?;
    file.write_all(markdown.as_bytes())
        .map_err(|e| format!("Failed to write export file: {}", e))?;

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn regenerate_history_entry(
    app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
    transcription_manager: State<'_, Arc<TranscriptionManager>>,
    id: i32,
) -> Result<(), String> {
    let id_i64 = i64::from(id);
    let entry = history_manager
        .get_entry_by_id(id_i64)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("History entry {} not found", id))?;

    if entry.entry_type == "stt" {
        retry_history_entry_transcription(app, history_manager, transcription_manager, id).await?;
    } else if entry.entry_type == "tts" {
        if let Some(tts) = app.try_state::<Arc<crate::tts::manager::TtsManager>>() {
            let tts_text = entry.transcription_text.clone();
            tts.speak(tts_text);
        } else {
            return Err("TTS Manager not initialized".to_string());
        }
    } else {
        return Err(format!("Unsupported entry type: {}", entry.entry_type));
    }

    Ok(())
}
