mod actions;
mod active_app;
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod apple_intelligence;
mod audio_feedback;
pub mod audio_toolkit;
mod brain;
mod catalog;

pub mod cli;
mod clipboard;
mod commands;
mod control_server;
mod crash_logging;
mod crypto;
mod helpers;
mod input;
mod input_source;
pub mod job_object;
mod llama_server;
mod llm_client;
mod llm_operation;
mod managers;
mod overlay;
mod overlay_fx;
pub mod portable;
mod recording_auto_stop;
mod recording_session;
mod settings;
mod shortcut;
mod signal_handle;
mod stt;
mod temp_artifacts;
mod text_replacement_decapitalize;
mod transcription_coordinator;
mod tray;
mod tray_i18n;
mod tts;
mod url_security;
mod utils;
mod wake_word;
mod webview_hardening;

pub use cli::CliArgs;
#[cfg(debug_assertions)]
use specta_typescript::Typescript;
use tauri_specta::{collect_commands, collect_events, Builder};

use env_filter::Builder as EnvFilterBuilder;
use managers::audio::AudioRecordingManager;
use managers::history::HistoryManager;
use managers::model::ModelManager;
use managers::transcription::TranscriptionManager;
#[cfg(unix)]
use signal_hook::consts::{SIGUSR1, SIGUSR2};
#[cfg(unix)]
use signal_hook::iterator::Signals;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use tauri::image::Image;
pub use transcription_coordinator::TranscriptionCoordinator;

use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Listener, Manager};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tauri_plugin_log::{Builder as LogBuilder, RotationStrategy, Target, TargetKind};

use crate::settings::get_settings;

// Global atomic to store the file log level filter
// We use u8 to store the log::LevelFilter as a number
pub static FILE_LOG_LEVEL: AtomicU8 = AtomicU8::new(log::LevelFilter::Debug as u8);

/// When `true`, log records are also forwarded to the webview via the
/// `log://log` event for the debug panel's live log viewer. Gated on debug
/// mode — the live log viewer is its only consumer and only exists in debug
/// mode — so normal runs never broadcast log records (which can include file
/// paths or transcribed text) onto the frontend event bus. Synced at startup
/// and whenever debug mode is toggled (see `shortcut::change_debug_mode_setting`).
pub static WEBVIEW_LOG_STREAMING: AtomicBool = AtomicBool::new(false);

fn level_filter_from_u8(value: u8) -> log::LevelFilter {
    match value {
        0 => log::LevelFilter::Off,
        1 => log::LevelFilter::Error,
        2 => log::LevelFilter::Warn,
        3 => log::LevelFilter::Info,
        4 => log::LevelFilter::Debug,
        5 => log::LevelFilter::Trace,
        _ => log::LevelFilter::Trace,
    }
}

fn build_console_filter() -> env_filter::Filter {
    let mut builder = EnvFilterBuilder::new();

    match std::env::var("RUST_LOG") {
        Ok(spec) if !spec.trim().is_empty() => {
            if let Err(err) = builder.try_parse(&spec) {
                log::warn!(
                    "Ignoring invalid RUST_LOG value '{}': {}. Falling back to info-level console logging",
                    spec,
                    err
                );
                builder.filter_level(log::LevelFilter::Info);
            }
        }
        _ => {
            builder.filter_level(log::LevelFilter::Info);
        }
    }

    builder.build()
}

fn show_main_window(app: &AppHandle) {
    if let Some(main_window) = app.get_webview_window("main") {
        if let Err(e) = main_window.unminimize() {
            log::error!("Failed to unminimize webview window: {}", e);
        }
        if let Err(e) = main_window.show() {
            log::error!("Failed to show webview window: {}", e);
        }
        if let Err(e) = main_window.set_focus() {
            log::error!("Failed to focus webview window: {}", e);
        }
        #[cfg(target_os = "macos")]
        {
            if let Err(e) = app.set_activation_policy(tauri::ActivationPolicy::Regular) {
                log::error!("Failed to set activation policy to Regular: {}", e);
            }
        }
        return;
    }

    let webview_labels = app.webview_windows().keys().cloned().collect::<Vec<_>>();
    log::error!(
        "Main window not found. Webview labels: {:?}",
        webview_labels
    );
}

#[allow(unused_variables)]
fn should_force_show_permissions_window(app: &AppHandle) -> bool {
    #[cfg(target_os = "windows")]
    {
        let model_manager = app.state::<Arc<ModelManager>>();
        let has_downloaded_models = model_manager
            .get_available_models()
            .iter()
            .any(|model| model.is_downloaded);

        if !has_downloaded_models {
            return false;
        }

        let status = commands::audio::get_windows_microphone_permission_status();
        if status.supported && status.overall_access == commands::audio::PermissionAccess::Denied {
            log::info!(
                "Windows microphone permissions are denied; forcing main window visible for onboarding"
            );
            return true;
        }
    }

    false
}

fn initialize_core_logic(app_handle: &AppHandle) {
    // Note: Enigo (keyboard/mouse simulation) is NOT initialized here.
    // The frontend is responsible for calling the `initialize_enigo` command
    // after onboarding completes. This avoids triggering permission dialogs
    // on macOS before the user is ready.

    // Initialize the managers. The audio recorder receives the streaming router
    // explicitly, so always-on microphone startup can wire live-preview frames
    // even before Tauri state is populated.
    let model_manager =
        Arc::new(ModelManager::new(app_handle).expect("Failed to initialize model manager"));
    let transcription_manager = Arc::new(
        TranscriptionManager::new(app_handle, model_manager.clone())
            .expect("Failed to initialize transcription manager"),
    );
    let recording_manager = Arc::new(
        AudioRecordingManager::new(app_handle, transcription_manager.stream_router())
            .expect("Failed to initialize recording manager"),
    );
    let history_manager =
        Arc::new(HistoryManager::new(app_handle).expect("Failed to initialize history manager"));
    let tts_manager = Arc::new(crate::tts::manager::TtsManager::new(app_handle.clone()));
    let tts_telemetry = Arc::new(crate::tts::telemetry::Telemetry::new());
    let brain_manager = Arc::new(crate::brain::manager::BrainManager::new(app_handle.clone()));
    let llama_manager = Arc::new(crate::brain::llama_manager::LlamaManager::new(
        app_handle.clone(),
    ));
    let llama_server_manager = Arc::new(crate::llama_server::manager::LlamaServerManager::new(
        app_handle.clone(),
    ));

    // Initialize the transcribe-cpp native backend (logging + backend module
    // registration) once, before any whisper model is loaded.
    managers::transcription::init_transcribe_backend();

    // Apply accelerator preferences before any model loads
    managers::transcription::apply_accelerator_settings(app_handle);

    // Add managers to Tauri's managed state
    app_handle.manage(recording_manager.clone());
    app_handle.manage(model_manager.clone());
    app_handle.manage(transcription_manager.clone());
    app_handle.manage(history_manager.clone());
    app_handle.manage(tts_manager.clone());
    app_handle.manage(tts_telemetry.clone());
    app_handle.manage(brain_manager.clone());
    app_handle.manage(llama_manager.clone());
    app_handle.manage(llama_server_manager.clone());

    // CopySpeak double-copy trigger (idles cheaply while disabled).
    crate::tts::clipboard_watch::start(app_handle.clone());

    // Set AppHandle for persistent Piper server status emissions.
    crate::tts::backends::piper_server::set_app_handle(app_handle.clone());
    crate::tts::local_tts_server::set_local_tts_app_handle(app_handle.clone());

    // Note: Shortcuts are NOT initialized here.
    // The frontend is responsible for calling the `initialize_shortcuts` command
    // after permissions are confirmed (on macOS) or after onboarding completes.
    // This matches the pattern used for Enigo initialization.

    #[cfg(unix)]
    let signals = Signals::new([SIGUSR1, SIGUSR2]).unwrap();
    // Set up signal handlers for toggling transcription
    #[cfg(unix)]
    signal_handle::setup_signal_handler(app_handle.clone(), signals);

    // Apply macOS Accessory policy if starting hidden and tray is available.
    // If the tray icon is disabled, keep the dock icon so the user can reopen.
    #[cfg(target_os = "macos")]
    {
        let settings = settings::get_settings(app_handle);
        if settings.start_hidden && settings.show_tray_icon {
            let _ = app_handle.set_activation_policy(tauri::ActivationPolicy::Accessory);
        }
    }
    // Get the current theme to set the appropriate initial icon
    let initial_theme = tray::get_current_theme(app_handle);

    // Choose the appropriate initial icon based on theme
    let initial_icon_path = tray::get_icon_path(initial_theme, tray::TrayIconState::Idle);

    let tray = TrayIconBuilder::new()
        .icon(
            Image::from_path(
                app_handle
                    .path()
                    .resolve(initial_icon_path, tauri::path::BaseDirectory::Resource)
                    .unwrap(),
            )
            .unwrap(),
        )
        .tooltip(tray::tray_tooltip())
        .show_menu_on_left_click(true)
        .icon_as_template(true)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "settings" => {
                show_main_window(app);
            }
            "check_updates" => {
                let settings = settings::get_settings(app);
                if settings.update_checks_enabled {
                    show_main_window(app);
                    let _ = app.emit("check-for-updates", ());
                }
            }
            "copy_last_transcript" => {
                tray::copy_last_transcript(app);
            }
            "unload_model" => {
                let transcription_manager = app.state::<Arc<TranscriptionManager>>();
                if !transcription_manager.is_model_loaded() {
                    log::warn!("No model is currently loaded.");
                    return;
                }
                match transcription_manager.unload_model() {
                    Ok(()) => log::info!("Model unloaded via tray."),
                    Err(e) => log::error!("Failed to unload model via tray: {}", e),
                }
            }
            "cancel" => {
                use crate::utils::cancel_current_operation;

                // Use centralized cancellation that handles all operations
                cancel_current_operation(app);
            }
            "quit" => {
                app.exit(0);
            }
            id if id.starts_with("model_select:") => {
                let model_id = id.strip_prefix("model_select:").unwrap().to_string();
                let current_model = settings::get_settings(app).selected_model;
                if model_id == current_model {
                    return;
                }
                let app_clone = app.clone();
                std::thread::spawn(move || {
                    match commands::models::switch_active_model(&app_clone, &model_id) {
                        Ok(()) => {
                            log::info!("Model switched to {} via tray.", model_id);
                        }
                        Err(e) => {
                            log::error!("Failed to switch model via tray: {}", e);
                        }
                    }
                    tray::update_tray_menu(&app_clone, &tray::TrayIconState::Idle, None);
                });
            }
            _ => {}
        })
        .build(app_handle)
        .unwrap();
    app_handle.manage(tray);
    app_handle.manage(tray::CurrentTrayIconState::new());

    // Initialize tray menu with idle state
    tray::update_tray_menu(app_handle, None);

    // Apply show_tray_icon setting
    let settings = settings::get_settings(app_handle);
    if !settings.show_tray_icon {
        tray::set_tray_visibility(app_handle, false);
    }

    // Refresh tray menu when model state changes
    let app_handle_for_listener = app_handle.clone();
    app_handle.listen("model-state-changed", move |_| {
        tray::update_tray_menu(&app_handle_for_listener, None);
    });

    // Get the autostart manager and configure based on user setting
    let autostart_manager = app_handle.autolaunch();
    let settings = settings::get_settings(app_handle);

    if settings.autostart_enabled {
        // Enable autostart if user has opted in
        let _ = autostart_manager.enable();
    } else {
        // Disable autostart if user has opted out
        let _ = autostart_manager.disable();
    }

    // Create the recording overlay window (hidden by default)
    utils::create_recording_overlay(app_handle);

    // Create the brain overlay window (hidden, shown on converse trigger)
    if let Err(e) = crate::overlay_fx::window::create_brain_overlay(app_handle) {
        log::error!("Failed to create brain overlay window: {}", e);
    }

    // Start control TCP server
    control_server::start(app_handle.clone());
}

#[tauri::command]
#[specta::specta]
fn trigger_update_check(app: AppHandle) -> Result<(), String> {
    let settings = settings::get_settings(&app);
    if !settings.update_checks_enabled {
        return Ok(());
    }
    app.emit("check-for-updates", ())
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
fn show_main_window_command(app: AppHandle) -> Result<(), String> {
    show_main_window(&app);
    Ok(())
}


/// Headless one-shot transcription for the `--transcribe-file` / `--list-devices`
/// path. Drives the same `TranscriptionManager::transcribe` the app uses; no
/// mic, no VAD, no download. Returns a process exit code (0 ok, 1 runtime
/// failure, 2 bad input/usage).
fn run_headless_transcription(app: &AppHandle, args: &CliArgs) -> i32 {
    use std::time::Instant;

    // --list-devices: print registered compute devices (with indices) and exit.
    // Useful on multi-GPU machines to discover the index for --device-index.
    if args.list_devices {
        let devices = crate::managers::transcription::describe_compute_devices();
        if devices.is_empty() {
            println!("No transcribe-cpp compute devices registered.");
        } else {
            println!("transcribe-cpp compute devices:");
            for d in &devices {
                println!("  {}", d);
            }
        }
        if args.transcribe_file.is_none() {
            return 0;
        }
    }

    // --list-models: print the model registry (catalog + on-disk + custom) with
    // their ids — the same ids `--model` accepts — then exit. `--json` emits the
    // full ModelInfo array for scripting.
    if args.list_models {
        let model_manager = app.state::<Arc<ModelManager>>();
        let models = model_manager.get_available_models();
        if args.json {
            match serde_json::to_string_pretty(&models) {
                Ok(s) => println!("{}", s),
                Err(e) => {
                    eprintln!("error: failed to serialize models: {}", e);
                    return 1;
                }
            }
        } else if models.is_empty() {
            println!("No models available.");
        } else {
            println!("Available models (✓ = installed):");
            let width = models.iter().map(|m| m.id.len()).max().unwrap_or(0);
            for m in &models {
                let mark = if m.is_downloaded { "✓" } else { " " };
                let rec = if m.is_recommended {
                    "  [recommended]"
                } else {
                    ""
                };
                println!(
                    "  {}  {:<width$}  {}{}",
                    mark,
                    m.id,
                    m.name,
                    rec,
                    width = width
                );
            }
        }
        if args.transcribe_file.is_none() {
            return 0;
        }
    }

    let Some(wav) = args.transcribe_file.clone() else {
        return 0;
    };

    // read_wav_samples reads 16-bit int samples and does no validation; the app
    // only ever saves 16 kHz mono 16-bit PCM, so reject anything else rather than
    // transcribe garbage / mis-time / mis-decode.
    match hound::WavReader::open(&wav) {
        Ok(reader) => {
            let spec = reader.spec();
            if spec.sample_rate != 16_000
                || spec.channels != 1
                || spec.bits_per_sample != 16
                || spec.sample_format != hound::SampleFormat::Int
            {
                eprintln!(
                    "error: expected 16 kHz mono 16-bit PCM WAV, got {} Hz / {} ch / {}-bit {:?}",
                    spec.sample_rate, spec.channels, spec.bits_per_sample, spec.sample_format
                );
                return 2;
            }
        }
        Err(e) => {
            eprintln!("error: cannot open {}: {}", wav.display(), e);
            return 2;
        }
    }

    let samples = match crate::audio_toolkit::read_wav_samples(&wav) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: failed to read {}: {}", wav.display(), e);
            return 2;
        }
    };
    let audio_secs = samples.len() as f64 / 16_000.0;

    let tm = app.state::<Arc<TranscriptionManager>>();

    let model_id = args
        .model
        .clone()
        .unwrap_or_else(|| get_settings(app).selected_model);
    if model_id.is_empty() {
        eprintln!("error: no model selected (pass --model or pick one in the app)");
        return 2;
    }

    // --device-index hard-selects a compute device by its --list-devices registry
    // index (transcribe-cpp / whisper-family models only; not persisted). Omit it
    // to use the persisted accelerator setting.
    let device_index = args.device_index;
    let requested_device = match device_index {
        Some(idx) => format!("index {}", idx),
        None => "settings".to_string(),
    };

    // Cold load (timed).
    let load_start = Instant::now();
    if let Err(e) = tm.load_model_with_device(&model_id, device_index) {
        eprintln!("error: load_model('{}') failed: {}", model_id, e);
        return 1;
    }
    let load_ms = load_start.elapsed().as_millis() as u64;
    let bound_backend = tm.current_backend();

    let runs = args.repeat.unwrap_or(1).max(1);
    let mut times_ms: Vec<u64> = Vec::new();
    let mut text = String::new();
    for i in 0..runs {
        // If the model's unload-timeout is "Immediately", transcribe() unloads
        // the engine after each run; reload (untimed) so repeats keep working
        // and the inference timing below stays clean.
        if !tm.is_model_loaded() {
            if let Err(e) = tm.load_model_with_device(&model_id, device_index) {
                eprintln!("error: reload before run {} failed: {}", i + 1, e);
                return 1;
            }
        }
        let t = Instant::now();
        match tm.transcribe(samples.clone()) {
            Ok(out) => text = out,
            Err(e) => {
                eprintln!("error: transcribe failed: {}", e);
                return 1;
            }
        }
        times_ms.push(t.elapsed().as_millis() as u64);
    }
    let best_ms = times_ms.iter().copied().min().unwrap_or(0);
    let rtf = if best_ms > 0 {
        audio_secs / (best_ms as f64 / 1000.0)
    } else {
        0.0
    };

    if args.json {
        println!(
            "{}",
            serde_json::json!({
                "model": model_id,
                "requested_device": requested_device,
                "bound_backend": bound_backend,
                "audio_secs": audio_secs,
                "load_ms": load_ms,
                "transcribe_ms": times_ms,
                "best_ms": best_ms,
                "rtf": rtf,
                "text": text,
            })
        );
    } else {
        println!(
            "model={} device={} backend={} audio={:.2}s load={}ms best={}ms rtf={:.2}x",
            model_id,
            requested_device,
            bound_backend.as_deref().unwrap_or("?"),
            audio_secs,
            load_ms,
            best_ms,
            rtf,
        );
        println!("text: {}", text);
    }
    0
}

/// Typed IPC surface (commands + events), shared by `run()` and the
/// `export_bindings` test so ../src/bindings.ts can be regenerated headlessly
/// via `cargo test export_bindings` (no GUI launch needed).
fn specta_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new()

        .commands(collect_commands![
            shortcut::change_binding,
            shortcut::reset_binding,
            shortcut::change_ptt_setting,
            shortcut::change_audio_feedback_setting,
            shortcut::change_audio_feedback_volume_setting,
            shortcut::change_sound_theme_setting,
            shortcut::change_start_hidden_setting,
            shortcut::change_autostart_setting,
            shortcut::change_translate_to_english_setting,
            shortcut::change_selected_language_setting,
            shortcut::change_overlay_position_setting,
            shortcut::change_overlay_style_setting,
            shortcut::change_debug_mode_setting,
            shortcut::change_word_correction_threshold_setting,
            shortcut::change_extra_recording_buffer_setting,
            shortcut::change_paste_delay_ms_setting,
            shortcut::change_paste_delay_after_ms_setting,
            shortcut::change_paste_method_setting,
            shortcut::get_available_typing_tools,
            shortcut::change_typing_tool_setting,
            shortcut::change_external_script_path_setting,
            shortcut::change_clipboard_handling_setting,
            shortcut::change_auto_submit_setting,
            shortcut::change_auto_submit_key_setting,
            shortcut::change_post_process_enabled_setting,
            shortcut::change_experimental_enabled_setting,
            shortcut::change_post_process_base_url_setting,
            shortcut::change_post_process_api_key_setting,
            shortcut::change_post_process_model_setting,
            shortcut::set_post_process_provider,
            shortcut::fetch_post_process_models,
            shortcut::add_post_process_prompt,
            shortcut::update_post_process_prompt,
            shortcut::delete_post_process_prompt,
            shortcut::set_post_process_selected_prompt,
            shortcut::add_llm_model,
            shortcut::delete_llm_model,
            shortcut::add_post_process_action,
            shortcut::update_post_process_action,
            shortcut::delete_post_process_action,
            shortcut::update_custom_words,
            shortcut::suspend_binding,
            shortcut::resume_binding,
            shortcut::change_mute_while_recording_setting,
            shortcut::change_append_trailing_space_setting,
            shortcut::change_lazy_stream_close_setting,
            shortcut::change_vad_enabled_setting,
            shortcut::change_app_language_setting,
            shortcut::change_update_checks_setting,
            shortcut::change_show_whats_new_on_update_setting,
            shortcut::change_whats_new_last_seen_version_setting,
            shortcut::change_keyboard_implementation_setting,
            shortcut::get_keyboard_implementation,
            shortcut::change_show_tray_icon_setting,
            shortcut::change_transcribe_accelerator_setting,
            shortcut::change_ort_accelerator_setting,

            shortcut::change_parakeet_streaming_setting,
            shortcut::change_whisper_gpu_device,

            shortcut::get_available_accelerators,
            shortcut::change_text_replacement_decapitalize_after_edit_key_enabled_setting,
            shortcut::change_text_replacement_decapitalize_after_edit_key_setting,
            shortcut::change_text_replacement_decapitalize_after_edit_secondary_key_enabled_setting,
            shortcut::change_text_replacement_decapitalize_after_edit_secondary_key_setting,
            shortcut::change_text_replacement_decapitalize_timeout_ms_setting,
            shortcut::change_text_replacement_decapitalize_standard_post_recording_monitor_ms_setting,
            shortcut::get_text_replacement_decapitalize_overlay_state,
            shortcut::key_listener::start_key_listener_recording,
            shortcut::key_listener::stop_key_listener_recording,
            trigger_update_check,
            show_main_window_command,
            commands::cancel_operation,
            commands::is_portable,
            commands::get_app_dir_path,
            commands::export_settings,
            commands::import_settings,
            commands::get_app_settings,
            commands::get_default_settings,
            commands::get_log_dir_path,
            commands::set_log_level,
            commands::get_recent_logs,
            commands::clear_logs,
            commands::open_recordings_folder,
            commands::open_log_dir,
            commands::open_app_data_dir,
            commands::check_apple_intelligence_available,
            commands::initialize_enigo,
            commands::initialize_shortcuts,
            commands::models::get_available_models,
            commands::models::get_model_info,
            commands::models::download_model,
            commands::models::delete_model,
            commands::models::cancel_download,
            commands::models::set_active_model,
            commands::models::get_current_model,
            commands::models::get_transcription_model_status,
            commands::models::is_model_loading,

            commands::models::has_any_models_available,
            commands::models::has_any_models_or_downloads,
            commands::models::get_active_gpu_vram_status,
commands::models::rescan_local_models,

            commands::audio::update_microphone_mode,
            commands::audio::get_microphone_mode,
            commands::audio::get_windows_microphone_permission_status,
            commands::audio::open_microphone_privacy_settings,
            commands::audio::get_available_microphones,
            commands::audio::set_selected_microphone,
            commands::audio::get_selected_microphone,
            commands::audio::get_available_output_devices,
            commands::audio::set_selected_output_device,
            commands::audio::get_selected_output_device,
            commands::audio::play_test_sound,
            commands::audio::check_custom_sounds,
            commands::audio::set_clamshell_microphone,
            commands::audio::get_clamshell_microphone,
            commands::audio::is_recording,
            commands::audio::toggle_recording_pause,
            commands::audio::is_recording_paused,
            commands::audio::set_noise_suppression_enabled,
            commands::audio::set_vad_mode,
            commands::audio::start_continuous_voice_mode,
            commands::audio::stop_continuous_voice_mode,
            commands::audio::set_recording_auto_stop,
            commands::transcription::set_model_unload_timeout,
            commands::transcription::get_model_load_status,
            commands::transcription::unload_model_manually,
            commands::transcription::set_long_audio_model,
            commands::transcription::set_long_audio_threshold,
            commands::history::get_history_entries,
            commands::history::toggle_history_entry_saved,
            commands::history::get_audio_file_path,
            commands::history::delete_all_history_entries,
            commands::history::delete_history_entry,
            commands::history::retry_history_entry_transcription,
            commands::history::apply_action_to_history_entry,
            commands::history::update_history_limit,
            commands::history::update_recording_retention_period,
            commands::history::delete_history_entries,
            commands::history::export_history_entries,
            commands::history::regenerate_history_entry,
            commands::tts::tts_speak,
            commands::tts::tts_speak_clipboard,
            commands::tts::tts_stop,
            commands::tts::tts_pause,
            commands::tts::tts_resume,
            commands::tts::tts_is_playing,
            commands::tts::tts_get_voices,
            commands::tts::tts_unload_engine,
            commands::tts::get_piper_server_status,
            commands::tts::get_local_tts_status,
            commands::tts::pocket_import_cloned_voice,
            commands::tts::change_tts_config,
            commands::tts::tts_play_greeting,
            commands::tts::tts_save_to_file,
            commands::brain::brain_ask,
            commands::brain::ai_replace_selection,
            commands::brain::brain_abort,
            commands::brain::brain_clear_history,
            commands::brain::fetch_brain_models,
            commands::brain::change_brain_config,
            commands::brain::set_brain_provider,
            commands::brain::change_brain_base_url_setting,
            commands::brain::change_brain_api_key_setting,
            commands::brain::change_brain_model_setting,
            commands::brain::download_llama_models,
            commands::brain::get_llama_models_status,
            commands::brain::is_llama_downloading,
            commands::llama_server::fetch_llama_releases,
            commands::llama_server::download_llama_server,
            commands::llama_server::get_downloaded_llama_servers,
            commands::llama_server::remove_llama_server,
            commands::llama_server::set_llama_server_active,
            commands::llama_server::get_llama_server_config,
            commands::llama_server::detect_gpu_type,
            commands::discovery::discover_local_brains,
            commands::discovery::is_ollama_running,
            commands::wake_word::wake_word_start,
            commands::wake_word::wake_word_stop,
            commands::wake_word::wake_word_set_config,
            commands::wake_word::wake_word_status,
            helpers::clamshell::is_laptop,
            commands::system::get_system_ram,
            commands::system::check_speech_runtime_installed,
            commands::system::install_speech_runtime,
            crate::overlay_fx::commands::overlay_fx_probe_capabilities,
            crate::overlay_fx::commands::overlay_fx_show_conversation,
            crate::overlay_fx::commands::overlay_fx_dismiss,
        ])

        .events(collect_events![
            managers::history::HistoryUpdatePayload,
            managers::transcription::StreamTextEvent,
            managers::transcription::StreamPhaseEvent,
        ])
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run(cli_args: CliArgs) {
    // Detect portable mode before anything else
    portable::init();

    // Parse console logging directives from RUST_LOG, falling back to info-level logging
    // when the variable is unset
    let console_filter = build_console_filter();

    let specta_builder = specta_builder();


    #[cfg(debug_assertions)] // <- Only export on non-release builds
    if let Err(e) = specta_builder.export(Typescript::default(), "../src/bindings.ts") {
        eprintln!("Warning: Failed to export TS bindings (ignored): {e}");
    }

    let invoke_handler = specta_builder.invoke_handler();

    // The headless path must run as its own instance (see the single-instance
    // note below), not forward to an already-running app.
    let headless_mode =
        cli_args.transcribe_file.is_some() || cli_args.list_devices || cli_args.list_models;

    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default()
        .device_event_filter(tauri::DeviceEventFilter::Always)
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            LogBuilder::new()
                .level(log::LevelFilter::Trace) // Set to most verbose level globally
                .max_file_size(500_000)
                .rotation_strategy(RotationStrategy::KeepOne)
                .clear_targets()
                .targets([
                    // Console output respects RUST_LOG environment variable. In
                    // headless mode (--transcribe-file/--list-devices/--list-models)
                    // stdout carries only the result (JSON or plain), so send console
                    // logs to stderr instead to keep stdout clean for CI parsing.
                    Target::new(if headless_mode {
                        TargetKind::Stderr
                    } else {
                        TargetKind::Stdout
                    })
                    .filter({
                        let console_filter = console_filter.clone();
                        move |metadata| console_filter.enabled(metadata)
                    }),
                    // File logs respect the user's settings (stored in FILE_LOG_LEVEL atomic)
                    Target::new(if let Some(data_dir) = portable::data_dir() {
                        TargetKind::Folder {
                            path: data_dir.join("logs"),
                            file_name: Some("s2b2s".into()),
                        }
                    } else {
                        TargetKind::LogDir {
                            file_name: Some("s2b2s".into()),
                        }
                    })
                    .filter(|metadata| {
                        let file_level = FILE_LOG_LEVEL.load(Ordering::Relaxed);
                        metadata.level() <= level_filter_from_u8(file_level)
                    }),
                    // Stream logs to the webview (via the `log://log` event) so the
                    // debug panel's live log viewer can show them in real time. Only
                    // active while debug mode is on (its sole consumer), and shares the
                    // file log level so the "Log Level" setting controls verbosity.
                    Target::new(TargetKind::Webview).filter(|metadata| {
                        WEBVIEW_LOG_STREAMING.load(Ordering::Relaxed)
                            && metadata.level()
                                <= level_filter_from_u8(FILE_LOG_LEVEL.load(Ordering::Relaxed))
                    }),
                ])
                .build(),
        );

    #[cfg(target_os = "macos")]
    {
        builder = builder.plugin(tauri_nspanel::init());
    }

    // Single-instance forwards CLI args to an already-running Handy and exits.
    // That would make the headless path
    // (--transcribe-file/--list-devices/--list-models) a silent no-op whenever the
    // app is already open, so skip it in headless mode and run a standalone
    // instance instead.
    if !headless_mode {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if args.iter().any(|a| a == "--toggle-transcription") {
                signal_handle::send_transcription_input(app, "transcribe", "CLI");
            } else if args.iter().any(|a| a == "--toggle-post-process") {
                signal_handle::send_transcription_input(app, "transcribe_with_post_process", "CLI");
            } else if args.iter().any(|a| a == "--cancel") {
                crate::utils::cancel_current_operation(app);
            } else {
                show_main_window(app);
            }
        }));
    }

    builder
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_macos_permissions::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![]),
        ))
        .manage(cli_args.clone())
        .setup(move |app| {
            crate::portable::set_app_handle(app.handle().clone());
            specta_builder.mount_events(app);

            // Headless one-shot path (`--transcribe-file` / `--list-devices` /
            // `--list-models`): initialize only what transcription needs — the
            // store/paths plugins, the model + transcription managers, and the
            // transcribe-cpp backend + accelerator settings — then run on a worker
            // thread and exit. Deliberately skips the window, tray, overlay, audio
            // recorder (so it never opens the mic, even with always_on_microphone),
            // signal handlers, and autostart that initialize_core_logic sets up.
            if headless_mode {
                let app_handle = app.handle().clone();
                let model_manager = Arc::new(
                    ModelManager::new(&app_handle).expect("Failed to initialize model manager"),
                );
                let transcription_manager = Arc::new(
                    TranscriptionManager::new(&app_handle, model_manager.clone())
                        .expect("Failed to initialize transcription manager"),
                );
                app_handle.manage(model_manager);
                app_handle.manage(transcription_manager);
                managers::transcription::init_transcribe_backend();
                managers::transcription::apply_accelerator_settings(&app_handle);

                let handle = app_handle.clone();
                let args = cli_args.clone();
                std::thread::spawn(move || {
                    let code = run_headless_transcription(&handle, &args);
                    // Drop the loaded engine before teardown: ggml-metal's global
                    // device free asserts (SIGABRT) if a model's Metal resources
                    // are still alive at C++ static-destructor time.
                    if let Some(tm) = handle.try_state::<Arc<TranscriptionManager>>() {
                        let _ = tm.unload_model();
                    }
                    // process::exit (not app.exit, which exits 0 regardless) so the
                    // exit code propagates to the shell for CI gating. Flush first
                    // since process::exit runs no destructors / buffer flushes.
                    use std::io::Write;
                    let _ = std::io::stdout().flush();
                    let _ = std::io::stderr().flush();
                    std::process::exit(code);
                });
                return Ok(());
            }

            // Create main window programmatically so we can set data_directory
            // for portable mode (redirects WebView2 cache to portable Data dir).
            // Dev builds (productName ends with "Dev") get a distinct window
            // title so the window can't be mistaken for a production build.
            let window_title = if app.package_info().name.ends_with("Dev") {
                "S2B2S Dev"
            } else {
                "S2B2S"
            };
            let mut win_builder =
                tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::App("/".into()))
                    .title(window_title)
                    .inner_size(680.0, 570.0)
                    .min_inner_size(680.0, 570.0)
                    .resizable(true)
                    .maximizable(false)
                    .visible(true);

            if let Some(data_dir) = portable::data_dir() {
                win_builder = win_builder.data_directory(data_dir.join("webview"));
            }

            let main_window_created = if let Ok(main_win) = win_builder.build() {
                crate::webview_hardening::disable_browser_accelerator_keys(&main_win);
                true
            } else {
                log::error!("Failed to build main window — continuing without it");
                false
            };

            if let Err(e) = crypto::initialize(app.handle()) {
                log::error!("Failed to initialize cryptography module: {}", e);
            }
            let mut settings = get_settings(app.handle());

            // CLI --debug flag overrides debug_mode and log level (runtime-only, not persisted)
            if cli_args.debug {
                settings.debug_mode = true;
                settings.log_level = settings::LogLevel::Trace;
            }

            let tauri_log_level: tauri_plugin_log::LogLevel = settings.log_level.into();
            let file_log_level: log::Level = tauri_log_level.into();
            // Store the file log level in the atomic for the filter to use
            FILE_LOG_LEVEL.store(file_log_level.to_level_filter() as u8, Ordering::Relaxed);
            // Only forward logs to the webview while debug mode is on (the live log
            // viewer is the sole consumer and only exists in debug mode). This also
            // honors the runtime `--debug` override applied to `settings` above.
            WEBVIEW_LOG_STREAMING.store(settings.debug_mode, Ordering::Relaxed);
            let app_handle = app.handle().clone();

            // Install crash/panic logging early in the setup
            if let Err(e) = crash_logging::install_panic_logging(&app_handle) {
                eprintln!("Failed to install panic logging: {e}");
            }

            app.manage(actions::ActiveActionState(std::sync::Mutex::new(None)));
            app.manage(TranscriptionCoordinator::new(app_handle.clone()));
            app.manage(recording_session::ManagedSessionState::default());
            app.manage(recording_auto_stop::new_managed_state());
            app.manage(std::sync::Arc::new(
                crate::llm_operation::LlmOperationTracker::new(),
            ));

            // (TTS telemetry is registered above as `Arc<Telemetry>` — the bare
            // `Telemetry` that used to be managed here was a dead duplicate with a
            // different type key that no consumer ever read.)

            // Register the wake word detector (inactive by default).
            app.manage(std::sync::Arc::new(
                crate::wake_word::WakeWordDetector::new(),
            ));

            initialize_core_logic(&app_handle);

            // Populate the overlay-enabled cache from initial settings so the
            // audio path (overlay::emit_levels, called ~24 Hz during recording)
            // can do a single atomic load instead of reading the Tauri store.
            // Kept in sync by shortcut::change_overlay_style_setting.
            overlay::update_overlay_enabled_cache(
                settings.overlay_style != settings::OverlayStyle::None,
            );

            // Pre-warm GPU/accelerator enumeration on a background thread. The first
            // get_available_accelerators call enumerates ORT execution providers and
            // transcribe-cpp compute devices, which can take a moment; without this
            // the cost is paid synchronously when the user first opens Advanced
            // settings, freezing the UI. Result is cached in a OnceLock.
            std::thread::spawn(|| {
                let _ = crate::managers::transcription::get_available_accelerators();
            });

            // Start the Piper idle watcher (checks ModelUnloadTimeout every 15s)
            crate::tts::backends::piper_server::start_idle_watcher(app_handle.clone());

            // Fire all AI models in parallel immediately — no waiting, no ordering.
            // Brain (LLM) is the most latency-critical; STT and TTS load concurrently.
            let startup_app_handle = app_handle.clone();

            // 1. Brain: warm up the AI Brain model as early as possible
            {
                let brain_handle = startup_app_handle.clone();
                std::thread::spawn(move || {
                    if let Some(brain) = brain_handle
                        .try_state::<Arc<crate::brain::manager::BrainManager>>()
                        .map(|s| s.inner().clone())
                    {
                        log::info!("[Startup] Warming up AI Brain model...");
                        tauri::async_runtime::block_on(async {
                            if let Err(e) = brain.warmup().await {
                                log::error!("[Startup] Brain warm up request failed: {}", e);
                            } else {
                                log::info!("[Startup] Brain warm up complete.");
                            }
                        });
                    }
                });
            }

            // 2. TTS: Pre-load the configured TTS engine
            {
                let tts_handle = startup_app_handle.clone();
                std::thread::spawn(move || {
                    let settings = crate::settings::get_settings(&tts_handle);
                    let voice = if settings.tts.voice.is_empty() {
                        "en_US-joe-medium".to_string()
                    } else {
                        settings.tts.voice.clone()
                    };

                    if settings.tts.engine == crate::settings::TtsEngine::Piper {
                        log::info!("[Startup] Auto-loading Piper TTS persistent server...");
                        match crate::tts::backends::piper_server::ensure_running(
                            voice,
                            settings.tts.piper.cuda,
                        ) {
                            Ok(_) => {
                                log::info!(
                                    "[Startup] Piper TTS persistent server loaded successfully."
                                );
                            }
                            Err(e) => {
                                log::error!("[Startup] Failed to auto-load Piper server: {}", e);
                            }
                        }
                    }

                    if settings.tts.engine == crate::settings::TtsEngine::Kokoro {
                        log::info!("[Startup] Pre-warming Kokoro TTS engine...");
                        let script_args =
                            crate::tts::backends::kokoro::KokoroBackend::kokoro_model_args();
                        match crate::tts::local_tts_server::ensure_running(
                            "kokoro",
                            "python".to_string(),
                            script_args,
                        ) {
                            Ok(_) => log::info!(
                                "[Startup] Kokoro persistent server loaded successfully."
                            ),
                            Err(e) => log::error!("[Startup] Failed to auto-load Kokoro: {}", e),
                        }
                    }
                    if settings.tts.engine == crate::settings::TtsEngine::Kitten {
                        log::info!("[Startup] Pre-warming Kitten TTS engine...");
                        match crate::tts::local_tts_server::ensure_running(
                            "kitten",
                            "python".to_string(),
                            vec![],
                        ) {
                            Ok(_) => log::info!(
                                "[Startup] Kitten persistent server loaded successfully."
                            ),
                            Err(e) => log::error!("[Startup] Failed to auto-load Kitten: {}", e),
                        }
                    }
                    if settings.tts.engine == crate::settings::TtsEngine::Pocket {
                        log::info!("[Startup] Pre-warming Pocket TTS engine...");
                        match crate::tts::local_tts_server::ensure_running(
                            "pocket",
                            "python".to_string(),
                            vec![],
                        ) {
                            Ok(_) => log::info!(
                                "[Startup] Pocket persistent server loaded successfully."
                            ),
                            Err(e) => log::error!("[Startup] Failed to auto-load Pocket: {}", e),
                        }
                    }

                    // Play the startup greeting once TTS is loaded
                    if settings.tts.play_startup_greeting {
                        if let Some(tts) = tts_handle
                            .try_state::<Arc<crate::tts::manager::TtsManager>>()
                            .map(|s| s.inner().clone())
                        {
                            tts.play_greeting();
                        }
                    }
                });
            }

            // 3. STT: Pre-load the transcription model
            {
                let stt_handle = startup_app_handle;
                std::thread::spawn(move || {
                    log::info!("[Startup] Auto-loading STT model...");
                    if let Some(transcription_manager) = stt_handle
                        .try_state::<Arc<crate::managers::transcription::TranscriptionManager>>()
                        .map(|s| s.inner().clone())
                    {
                        transcription_manager.initiate_model_load();
                    }
                });
            }

            // Hide tray icon if --no-tray was passed
            if cli_args.no_tray {
                tray::set_tray_visibility(&app_handle, false);
            }

            // Window starts visible. Hide it if configured to start hidden
            // AND a tray icon is available (so the user can reopen later).
            // CLI --start-hidden flag overrides the setting.
            // If permission onboarding is required, keep it visible regardless.
            if main_window_created {
                let should_hide = settings.start_hidden || cli_args.start_hidden;
                let should_force_show = should_force_show_permissions_window(&app_handle);
                let tray_available = settings.show_tray_icon && !cli_args.no_tray;

                if should_hide && tray_available && !should_force_show {
                    if let Some(win) = app.get_webview_window("main") {
                        let _ = win.hide();
                    }
                }
            }

            // If tray is not available and main window failed, log a clear error
            if !main_window_created {
                let tray_available = settings.show_tray_icon && !cli_args.no_tray;
                if !tray_available {
                    log::error!("No main window and no tray icon — app is completely inaccessible");
                }
            }

            Ok(())
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _res = window.hide();

                #[cfg(target_os = "macos")]
                {
                    let settings = get_settings(window.app_handle());
                    let tray_visible =
                        settings.show_tray_icon && !window.app_handle().state::<CliArgs>().no_tray;
                    if tray_visible {
                        // Tray is available: hide the dock icon, app lives in the tray
                        let res = window
                            .app_handle()
                            .set_activation_policy(tauri::ActivationPolicy::Accessory);
                        if let Err(e) = res {
                            log::error!("Failed to set activation policy: {}", e);
                        }
                    }
                    // No tray: keep the dock icon visible so the user can reopen
                }
            }
            tauri::WindowEvent::ThemeChanged(theme) => {
                log::info!("Theme changed to: {:?}", theme);
                // Re-apply the current tray state with the new theme's icon set
                tray::refresh_tray_icon(window.app_handle());
            }
            _ => {}
        })
        .invoke_handler(invoke_handler)
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| match &event {
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen { .. } => {
                show_main_window(app);
            }
            tauri::RunEvent::Exit => {
                log::info!("[Shutdown] Cleaning up all TTS engines...");
                crate::tts::backends::piper_server::unload_piper_model();
                crate::tts::local_tts_server::unload_all();
                if let Some(llama_manager) =
                    app.try_state::<Arc<crate::brain::llama_manager::LlamaManager>>()
                {
                    llama_manager.stop();
                }
                if let Some(tm) = app.try_state::<Arc<TranscriptionManager>>() {
                    let _ = tm.unload_model();
                }
                log::info!("[Shutdown] Cleanup complete.");
            }
            _ => {}
        });
}

#[cfg(test)]
mod bindings_export_tests {
    #[test]
    fn export_bindings() {
        super::specta_builder()
            .export(
                specta_typescript::Typescript::default(),
                "../src/bindings.ts",
            )
            .expect("Failed to export typescript bindings");
    }
}
