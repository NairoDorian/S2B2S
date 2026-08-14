//! Tauri commands for transcription profiles: named model+language presets
//! that apply with one click (switching the active STT model when needed).

use crate::settings::{get_settings, write_settings, AppSettings, TranscriptionProfile};
use tauri::AppHandle;

/// Apply a profile's model and language to the live transcription settings.
/// Switches the loaded STT model when the profile overrides it.
pub fn apply_transcription_profile(
    app: &AppHandle,
    profile: &TranscriptionProfile,
) -> Result<(), String> {
    let mut settings = get_settings(app);

    if let Some(model) = profile.model.as_deref() {
        if !model.is_empty() && model != settings.selected_model {
            crate::commands::models::switch_active_model(app, model)?;
            settings = get_settings(app); // switch_active_model persisted a new snapshot
        }
    }
    if let Some(lang) = profile.language.as_deref() {
        if !lang.is_empty() {
            settings.selected_language = lang.to_string();
        }
    }
    write_settings(app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn get_transcription_profiles(app: AppHandle) -> Result<Vec<TranscriptionProfile>, String> {
    Ok(get_settings(&app).transcription_profiles)
}

/// Upsert a profile (id empty → generate one) and return the saved list.
#[tauri::command]
#[specta::specta]
pub fn save_transcription_profile(
    app: AppHandle,
    profile: TranscriptionProfile,
) -> Result<Vec<TranscriptionProfile>, String> {
    let mut settings = get_settings(&app);
    let mut profile = profile;
    if profile.id.trim().is_empty() {
        profile.id = format!(
            "profile-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        );
    }
    if profile.name.trim().is_empty() {
        return Err("Profile name is required".to_string());
    }
    if let Some(existing) = settings
        .transcription_profiles
        .iter_mut()
        .find(|p| p.id == profile.id)
    {
        *existing = profile;
    } else {
        settings.transcription_profiles.push(profile);
    }
    write_settings(&app, settings.clone());
    Ok(settings.transcription_profiles)
}

#[tauri::command]
#[specta::specta]
pub fn delete_transcription_profile(
    app: AppHandle,
    profile_id: String,
) -> Result<Vec<TranscriptionProfile>, String> {
    let mut settings = get_settings(&app);
    settings
        .transcription_profiles
        .retain(|p| p.id != profile_id);
    if settings.active_transcription_profile_id.as_deref() == Some(profile_id.as_str()) {
        settings.active_transcription_profile_id = None;
    }
    write_settings(&app, settings.clone());
    Ok(settings.transcription_profiles)
}

/// Activate a profile (applying model/language) or clear the active profile.
#[tauri::command]
#[specta::specta]
pub fn set_active_transcription_profile(
    app: AppHandle,
    profile_id: Option<String>,
) -> Result<(), String> {
    let mut settings: AppSettings = get_settings(&app);
    if let Some(id) = profile_id.as_deref() {
        let profile = settings
            .transcription_profiles
            .iter()
            .find(|p| p.id == id)
            .cloned()
            .ok_or_else(|| format!("Profile '{id}' not found"))?;
        apply_transcription_profile(&app, &profile)?;
        settings = get_settings(&app);
        settings.active_transcription_profile_id = Some(id.to_string());
    } else {
        settings.active_transcription_profile_id = None;
    }
    write_settings(&app, settings);
    Ok(())
}
