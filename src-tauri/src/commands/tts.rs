//! Tauri commands for the TTS ("Read Anywhere") subsystem.

use crate::settings::{get_settings, write_settings, TtsConfig};
use crate::tts::backends::piper;
use crate::tts::manager::TtsManager;
use crate::tts::Voice;
use std::sync::Arc;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_clipboard_manager::ClipboardExt;

/// Speak arbitrary text aloud (sanitize → paginate → streaming synthesis).
#[tauri::command]
#[specta::specta]
pub fn tts_speak(tts: State<'_, Arc<TtsManager>>, text: String) -> Result<(), String> {
    tts.speak(text);
    Ok(())
}

/// Speak the current clipboard text.
#[tauri::command]
#[specta::specta]
pub fn tts_speak_clipboard(app: AppHandle, tts: State<'_, Arc<TtsManager>>) -> Result<(), String> {
    let text = app
        .clipboard()
        .read_text()
        .map_err(|e| format!("Failed to read clipboard: {e}"))?;
    if text.trim().is_empty() {
        return Err("Clipboard is empty".into());
    }
    tts.speak(text);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn tts_stop(tts: State<'_, Arc<TtsManager>>) -> Result<(), String> {
    tts.stop();
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn tts_pause(tts: State<'_, Arc<TtsManager>>) -> Result<(), String> {
    tts.pause();
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn tts_resume(tts: State<'_, Arc<TtsManager>>) -> Result<(), String> {
    tts.resume();
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn tts_is_playing(tts: State<'_, Arc<TtsManager>>) -> Result<bool, String> {
    Ok(tts.is_playing())
}

/// Enumerate available voices for the configured engine.
#[tauri::command]
#[specta::specta]
pub fn tts_get_voices(tts: State<'_, Arc<TtsManager>>) -> Result<Vec<Voice>, String> {
    Ok(tts.list_voices())
}

/// Unload the warm TTS model/server (tray "Unload model" parity).
#[tauri::command]
#[specta::specta]
pub fn tts_unload_engine() -> Result<bool, String> {
    Ok(crate::tts::backends::piper_server::unload_piper_model())
}

#[tauri::command]
#[specta::specta]
pub fn get_piper_server_status() -> Result<crate::tts::backends::piper_server::PiperServerStatus, String> {
    Ok(crate::tts::backends::piper_server::get_piper_server_status())
}

/// Replace the whole TTS configuration (engine, voice, speed, volume, toggles).
#[tauri::command]
#[specta::specta]
pub fn change_tts_config(app: AppHandle, config: TtsConfig) -> Result<(), String> {
    let mut settings = get_settings(&app);
    let was_enabled = settings.tts.enabled;
    let volume = config.volume;
    let now_enabled = config.enabled;
    
    let old_cuda = settings.tts.piper.cuda;
    let old_voice = settings.tts.voice.clone();
    let old_engine = settings.tts.engine;

    let new_cuda = config.piper.cuda;
    let new_voice = config.voice.clone();
    let new_engine = config.engine;

    settings.tts = config;
    write_settings(&app, settings.clone());

    // Apply live-effective bits immediately.
    if let Some(tts) = app.try_state::<Arc<TtsManager>>() {
        tts.set_volume(volume);
    }

    // Register/unregister the speak-selection shortcut with the feature toggle.
    if was_enabled != now_enabled {
        if let Some(binding) = settings.bindings.get("speak_selection").cloned() {
            if now_enabled {
                let _ = crate::shortcut::register_shortcut(&app, binding);
            } else {
                let _ = crate::shortcut::unregister_shortcut(&app, binding);
            }
        }
    }

    // If the engine is Piper, and either the voice or CUDA setting changed, reload in background.
    if new_engine == crate::settings::TtsEngine::Piper && now_enabled {
        if old_cuda != new_cuda || old_voice != new_voice || old_engine != new_engine || was_enabled != now_enabled {
            log::info!("[TTS] Piper config changed (voice or CUDA). Reloading server in background...");
            let voice_to_load = if new_voice.is_empty() {
                "en_US-joe-medium".to_string()
            } else {
                new_voice
            };
            std::thread::spawn(move || {
                match crate::tts::backends::piper_server::ensure_running(voice_to_load, new_cuda) {
                    Ok(_) => log::info!("[TTS] Piper server reloaded successfully with new configuration."),
                    Err(e) => log::error!("[TTS] Failed to reload Piper server: {}", e),
                }
            });
        }
    } else if old_engine == crate::settings::TtsEngine::Piper && (new_engine != crate::settings::TtsEngine::Piper || !now_enabled) {
        // Unload Piper if engine switched or disabled
        log::info!("[TTS] Piper engine disabled or switched. Unloading model...");
        crate::tts::backends::piper_server::unload_piper_model();
    }

    Ok(())
}
