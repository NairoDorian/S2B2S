//! Tauri commands for the Brain (streaming LLM) subsystem.

use crate::brain::manager::BrainManager;
use crate::settings::{get_settings, write_settings, BrainConfig, PostProcessProvider};
use std::sync::Arc;
use tauri::{AppHandle, State};

/// Ask the Brain; streams `brain:token` / `brain:sentence` events and returns the full reply.
#[tauri::command]
#[specta::specta]
pub async fn brain_ask(
    brain: State<'_, Arc<BrainManager>>,
    text: String,
) -> Result<String, String> {
    let brain = brain.inner().clone();
    brain.ask(text).await
}

/// Abort the in-flight Brain stream and stop any speech it queued (barge-in).
#[tauri::command]
#[specta::specta]
pub fn brain_abort(brain: State<'_, Arc<BrainManager>>) -> Result<(), String> {
    brain.abort();
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn brain_clear_history(brain: State<'_, Arc<BrainManager>>) -> Result<(), String> {
    brain.clear_history();
    Ok(())
}

/// List models available at the configured Brain endpoint (Ollama, LM Studio, cloud…).
#[tauri::command]
#[specta::specta]
pub async fn fetch_brain_models(app: AppHandle) -> Result<Vec<String>, String> {
    let settings = get_settings(&app);
    let brain_cfg = &settings.brain;
    let provider = brain_cfg.active_provider()
        .ok_or_else(|| "No active Brain provider configured".to_string())?;

    // Get API key
    let api_key = brain_cfg.active_api_key();

    // Skip fetching if no API key for providers that typically need one
    if api_key.trim().is_empty() && provider.id != "custom" {
        return Err(format!(
            "API key is required for {}. Please add an API key to list available models.",
            provider.label
        ));
    }

    crate::llm_client::fetch_models(provider, api_key).await
}

/// Replace the whole Brain configuration (endpoint, model, prompt, toggles).
#[tauri::command]
#[specta::specta]
pub fn change_brain_config(app: AppHandle, config: BrainConfig) -> Result<(), String> {
    let mut settings = get_settings(&app);
    let was_enabled = settings.brain.enabled;
    let now_enabled = config.enabled;
    settings.brain = config;
    write_settings(&app, settings.clone());

    // Register/unregister the converse shortcut with the feature toggle.
    if was_enabled != now_enabled {
        if let Some(binding) = settings.bindings.get("converse").cloned() {
            if now_enabled {
                let _ = crate::shortcut::register_shortcut(&app, binding);
            } else {
                let _ = crate::shortcut::unregister_shortcut(&app, binding);
            }
        }
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn set_brain_provider(app: AppHandle, provider_id: String) -> Result<(), String> {
    let mut settings = get_settings(&app);
    if !settings.brain.providers.iter().any(|p| p.id == provider_id) {
        return Err(format!("Provider '{}' not found", provider_id));
    }
    settings.brain.provider_id = provider_id;
    write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_brain_base_url_setting(
    app: AppHandle,
    provider_id: String,
    base_url: String,
) -> Result<(), String> {
    let mut settings = get_settings(&app);
    let provider = settings.brain.provider_mut(&provider_id)
        .ok_or_else(|| format!("Provider '{}' not found", provider_id))?;

    if provider.id != "custom" {
        return Err(format!(
            "Provider '{}' does not allow editing the base URL",
            provider.label
        ));
    }

    provider.base_url = base_url;
    write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_brain_api_key_setting(
    app: AppHandle,
    provider_id: String,
    api_key: String,
) -> Result<(), String> {
    let mut settings = get_settings(&app);
    if !settings.brain.providers.iter().any(|p| p.id == provider_id) {
        return Err(format!("Provider '{}' not found", provider_id));
    }
    settings.brain.api_keys.insert(provider_id, api_key);
    write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_brain_model_setting(
    app: AppHandle,
    provider_id: String,
    model: String,
) -> Result<(), String> {
    let mut settings = get_settings(&app);
    if !settings.brain.providers.iter().any(|p| p.id == provider_id) {
        return Err(format!("Provider '{}' not found", provider_id));
    }
    settings.brain.models.insert(provider_id, model);
    write_settings(&app, settings);
    Ok(())
}
