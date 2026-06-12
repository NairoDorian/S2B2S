//! Tauri commands for the Brain (streaming LLM) subsystem.

use crate::brain::manager::BrainManager;
use crate::settings::{get_settings, write_settings, BrainConfig};
use std::sync::Arc;
use tauri::{AppHandle, State, Manager};

/// AI Replace: speak an instruction to rewrite selected text via the Brain.
/// Returns the rewritten text; caller pastes it at the cursor.
#[tauri::command]
#[specta::specta]
pub async fn ai_replace_selection(
    app: AppHandle,
    instruction: String,
    selected_text: String,
) -> Result<String, String> {
    let settings = crate::settings::get_settings(&app);
    let brain_cfg = &settings.brain;
    if !brain_cfg.enabled {
        return Err("The Brain is disabled".to_string());
    }
    if brain_cfg.provider_id == "llama_cpp" {
        if let Some(llama_manager) = app.try_state::<Arc<crate::brain::llama_manager::LlamaManager>>() {
            llama_manager.ensure_server_running().await?;
        }
    }
    let api_key = brain_cfg.active_api_key();
    let model = brain_cfg.active_model();
    let provider = brain_cfg.active_provider().ok_or("No Brain provider")?;
    let base_url = provider.base_url.clone();
    let system_prompt = "You rewrite text according to the user's instruction. \
        Output ONLY the rewritten text — no preamble, no explanation, no markdown formatting. \
        Preserve the original meaning unless the instruction changes it.";

    let prompt =
        format!("TEXT:\n{selected_text}\n\nINSTRUCTION:\n{instruction}\n\nREWRITTEN TEXT:");

    let messages = vec![
        crate::brain::client::ChatMessage {
            role: "system".to_string(),
            content: system_prompt.to_string(),
        },
        crate::brain::client::ChatMessage {
            role: "user".to_string(),
            content: prompt,
        },
    ];

    let client = crate::brain::client::BrainClient::new();
    let abort = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut result = String::new();

    // Non-streaming request for simplicity
    let full = client
        .stream_chat(
            &base_url,
            &api_key,
            &model,
            &messages,
            abort,
            |token| {
                result.push_str(token);
            },
            |_sentence| {},
        )
        .await?;

    Ok(full.text.trim().to_string())
}

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
    let provider = brain_cfg
        .active_provider()
        .ok_or_else(|| "No active Brain provider configured".to_string())?;

    // Get API key
    let api_key = brain_cfg.active_api_key();

    // Skip fetching if no API key for providers that typically need one
    if api_key.trim().is_empty() && provider.id != "custom" && provider.id != "llama_cpp" {
        return Err(format!(
            "API key is required for {}. Please add an API key to list available models.",
            provider.label
        ));
    }

    if provider.id == "llama_cpp" {
        if let Some(llama_manager) = app.try_state::<Arc<crate::brain::llama_manager::LlamaManager>>() {
            llama_manager.ensure_server_running().await?;
        }
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
        // Stop llama.cpp server when Brain is disabled
        if !now_enabled && settings.brain.provider_id == "llama_cpp" {
            if let Some(llama_mgr) = app.try_state::<std::sync::Arc<crate::brain::llama_manager::LlamaManager>>() {
                llama_mgr.stop();
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
    let provider = settings
        .brain
        .provider_mut(&provider_id)
        .ok_or_else(|| format!("Provider '{}' not found", provider_id))?;

    if !provider.allow_base_url_edit {
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

#[tauri::command]
#[specta::specta]
pub async fn download_llama_models(
    llama_manager: State<'_, Arc<crate::brain::llama_manager::LlamaManager>>,
) -> Result<(), String> {
    llama_manager.inner().clone().start_download_in_background();
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn get_llama_models_status(
    llama_manager: State<'_, Arc<crate::brain::llama_manager::LlamaManager>>,
) -> Result<bool, String> {
    llama_manager.get_models_status()
}

#[tauri::command]
#[specta::specta]
pub fn is_llama_downloading(
    llama_manager: State<'_, Arc<crate::brain::llama_manager::LlamaManager>>,
) -> Result<bool, String> {
    Ok(llama_manager.is_downloading())
}
