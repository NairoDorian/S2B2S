mod actions;
pub mod app_identity;
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod apple_intelligence;
mod audio_feedback;
pub mod audio_toolkit;
mod autostart;
mod catalog;
pub mod cli;
mod clipboard;
mod commands;
pub mod direct_stream_writer;
mod file_transcription;
mod helpers;
mod input;
mod job_object;
mod live_fft;
mod live_mode;
mod llama_releases;
mod llama_server;
mod llm_client;
mod managers;
mod memory;
mod multi_streaming;
mod multi_stt_stream;
mod overlay;
mod overlay_preview;
mod paste_tx;
pub mod portable;
pub mod recall;
mod secure_input;
mod settings;
mod shortcut;
mod signal_handle;
mod system_monitor;
mod transcription_coordinator;
mod tray;
mod tray_i18n;
mod utils;
mod webview_hardening;

pub use cli::CliArgs;
#[cfg(debug_assertions)]
use specta_typescript::Typescript;
use tauri_specta::{Builder, collect_commands, collect_events};
pub use utils::{app_env_flag, app_env_var, env_flag_enabled};

use managers::audio::AudioRecordingManager;
use managers::history::HistoryManager;
use managers::model::ModelManager;
use managers::transcription::TranscriptionManager;
use std::sync::Arc;
pub use transcription_coordinator::TranscriptionCoordinator;

use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Listener, Manager};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_log::{Builder as LogBuilder, RotationStrategy, Target, TargetKind};

use crate::settings::get_settings;

/// The stream the console target writes to: **stdout** for the interactive app,
/// **stderr** for the headless one-shots.
///
/// Headless is the easy case, and the original reason. `--transcribe-file` /
/// `--list-devices` / `--list-models` put the *result* on stdout — plain text, or
/// JSON under `--json` — and CI parses it, so a log line there is corruption
/// rather than noise. stderr keeps the two separable with a plain `2>/dev/null`.
///
/// The interactive case was assumed to be the mirror image of that, on the
/// theory that the Tauri CLI follows the build by reading cargo's stdout and so
/// leaves the child a pipe there. **It is not, and the assumption cost every
/// console record the app wrote.** Measured from inside the app, in a real
/// `bun run dev:fast` terminal — the line this process emits about its own fds,
/// read back out of the file log, the target that always works:
///
/// ```text
/// [2026-09-12][20:30:04][app_lib][DEBUG] Console log stream: ... (stdout is a
/// terminal: true, stderr is a terminal: false)
/// ```
///
/// fd 1 is the terminal and fd 2 is a pipe — the exact opposite of the theory —
/// so records written to stderr went into something nobody displays: the user
/// saw `Running target\debug\zer0.exe` and then silence, while the same records
/// filled the file log and made the app look like it was logging fine. Something
/// in the launch chain (bun → `scripts/tauri-runner.ts` → the Tauri CLI → cargo →
/// this process) captures the child's stderr — the Tauri CLI interleaves the
/// child's output with its own, and its own lines arrive through the same path —
/// while stdout comes through untouched. Whatever the mechanism, the measurement
/// is what decides this, and it points at stdout.
///
/// So the choice is neither symmetric nor derivable: the fd this process holds is
/// all it can read, and a pipe an intermediate reads and discards is
/// indistinguishable from a pipe a consumer asked for. It has to be measured, and
/// re-measured if the launch chain changes (the startup line above is emitted
/// every run for exactly that). Until then: stdout for the app a user watches,
/// stderr for the modes whose result is on stdout.
fn console_stream_kind(headless_mode: bool) -> TargetKind {
    if headless_mode {
        TargetKind::Stderr
    } else {
        TargetKind::Stdout
    }
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

/// Choose the macOS activation policy the process *launches* with.
///
/// Must run between `build()` and `run()`: that is the only point where
/// `App::set_activation_policy` sets tao's initial policy, which
/// `applicationDidFinishLaunching` then applies directly. Calling the
/// `AppHandle` variant from `setup` (which Tauri runs on `RunEvent::Ready`,
/// i.e. after launch) is instead a runtime Regular → Accessory demotion of an
/// already-activated foreground app — the transition Apple documents as
/// unreliable, and what left a Dock icon behind for start-hidden and
/// login-item launches on macOS 26+ (#1787). Launching as Accessory avoids the
/// transition entirely; showing the window later promotes to Regular, which is
/// the supported direction.
///
/// Mirrors the show-window decision in `setup`: the app launches without a
/// Dock icon only when it will start hidden (setting or `--start-hidden`) AND a
/// tray icon is available (setting and not `--no-tray`). With no tray the Dock
/// icon stays as the only way back into the app (#903). Headless one-shot
/// runs are left alone.
#[cfg(target_os = "macos")]
fn apply_startup_activation_policy(app: &mut tauri::App, headless_mode: bool) {
    if headless_mode {
        return;
    }

    let cli_args = app.state::<CliArgs>().inner().clone();
    let settings = settings::get_settings(app.handle());

    let should_hide = settings.start_hidden || cli_args.start_hidden;
    let tray_available = settings.show_tray_icon && !cli_args.no_tray;

    if should_hide && tray_available {
        log::info!("Starting hidden with tray available: launching as Accessory (no Dock icon)");
        app.set_activation_policy(tauri::ActivationPolicy::Accessory);
    }
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
    let startup_started = std::time::Instant::now();
    // Before anything reads a path: move a pre-rename install's data dir,
    // cache and logs to where this version looks for them. A no-op on every
    // start after the first. See `portable::migrate_legacy_app_data`.
    portable::migrate_legacy_app_data(app_handle);
    log::info!(
        "{} {} starting on {} {} — data dir {}",
        app_identity::NAME,
        app_handle.package_info().version,
        std::env::consts::OS,
        std::env::consts::ARCH,
        portable::app_data_dir(app_handle)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "?".into())
    );
    // Note: on macOS, Enigo (keyboard/mouse simulation) and the shortcuts are
    // NOT initialized here: the frontend calls `initialize_enigo` /
    // `initialize_shortcuts` after onboarding, so no permission dialog appears
    // before the user is ready. Other platforms initialize both at the end of
    // this function (see below), so the hotkeys work seconds before the
    // webview has even loaded.

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
    // Always-on microphone: open off the startup path (≈0.9 s on Windows).
    {
        let rm = Arc::clone(&recording_manager);
        std::thread::Builder::new()
            .name("mic-open".into())
            .spawn(move || rm.open_if_always_on())
            .expect("spawn mic-open thread");
    }
    let history_manager =
        Arc::new(HistoryManager::new(app_handle).expect("Failed to initialize history manager"));
    let statistics_manager = Arc::new(managers::statistics::StatisticsManager::new(
        app_handle,
        history_manager.database_path().to_path_buf(),
    ));

    // Initialize the transcribe-cpp native backend (logging + backend module
    // registration) once, before any whisper model is loaded.
    managers::transcription::init_transcribe_backend();
    managers::arch_plugins::init_arch_plugin_dirs(app_handle);

    // Apply accelerator preferences before any model loads
    managers::transcription::apply_accelerator_settings(app_handle);

    // Add managers to Tauri's managed state
    app_handle.manage(recording_manager.clone());
    app_handle.manage(model_manager.clone());
    app_handle.manage(transcription_manager.clone());
    app_handle.manage(history_manager.clone());
    app_handle.manage(statistics_manager.clone());
    app_handle.manage(tray::TrayState::new());
    app_handle.manage(Arc::new(file_transcription::FileTranscriptionManager::new()));
    // In-app llama.cpp server: adopt an existing install on first run, then
    // optionally start it in the background so the first post-process request
    // finds a warm server. The status-bar meters start their sampler thread.
    llama_server::adopt_detected_install_if_unconfigured(app_handle);
    let llama_manager = llama_server::init(app_handle);
    app_handle.manage(Arc::clone(&llama_manager));
    // A port or alias changed while the app was closed leaves the `custom`
    // provider pointing at the old address; repair it here so the first
    // post-processing request of the session does not fail to connect.
    llama_manager.relink_custom_provider(false);
    {
        let llama_settings = settings::get_settings(app_handle).llama;
        if llama_settings.autostart {
            log::info!(
                "llama-server: starting in the background ({} is on)",
                "llama.autostart"
            );
            std::thread::spawn(move || {
                if let Err(e) = llama_manager.start() {
                    log::warn!("llama-server autostart failed: {e}");
                }
            });
        } else {
            log::info!(
                "llama-server: idle — autostart is off; on-demand start is {} (port {}, model {})",
                if llama_settings.start_on_demand {
                    "on"
                } else {
                    "off"
                },
                llama_settings.port,
                llama_settings
                    .model_path
                    .as_deref()
                    .and_then(|p| std::path::Path::new(p).file_name())
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "none selected".into())
            );
        }
    }
    system_monitor::start(app_handle.clone());
    app_handle.manage(Arc::new(live_mode::LiveModeManager::new(
        app_handle.clone(),
    )));
    // Live FFT: no thread until a session starts; the recorder-side tap is
    // a process-wide static so the always-on microphone (opened above on
    // its own thread) is already wired to it.
    app_handle.manage(Arc::new(live_fft::LiveFftManager::new(app_handle.clone())));

    // Windows / Linux need no accessibility permission: register the hotkeys
    // and the paste input now, instead of ~8 s later when the webview mounts
    // and calls the (idempotent) commands itself. macOS keeps the
    // frontend-driven order so the permission prompt comes at the right time.
    #[cfg(not(target_os = "macos"))]
    {
        if let Err(e) = commands::initialize_enigo(app_handle.clone()) {
            log::warn!("Early Enigo initialization failed: {e}");
        }
        if let Err(e) = commands::initialize_shortcuts(app_handle.clone()) {
            log::warn!("Early shortcut initialization failed: {e}");
        }
    }

    log::info!(
        "Core startup done in {:?} (microphone, llama-server and meters continue in the background)",
        startup_started.elapsed()
    );

    // Set up signal handlers for toggling transcription. On Linux, SIGUSR1 is
    // deliberately not handled — it belongs to WebKitGTK's garbage collector
    // (#1660) — see signal_handle.rs.
    #[cfg(unix)]
    signal_handle::setup_signal_handler(app_handle.clone());

    // The macOS activation policy for a start-hidden launch is applied before
    // the event loop runs (see `apply_startup_activation_policy`), not here:
    // by the time `setup` runs the app has already launched as a Regular
    // (Dock) app, and demoting it at runtime is unreliable (#1787).

    // Get the current theme to set the appropriate initial icon
    let initial_theme = tray::get_current_theme(app_handle);

    // Choose the appropriate initial icon based on theme
    let initial_icon_path = tray::get_icon_path(initial_theme, tray::TrayIconState::Idle, false);

    let mut tray_builder = TrayIconBuilder::new()
        .tooltip(tray::tray_tooltip())
        .icon_as_template(true);

    // The initial tray icon is loaded best-effort: a resolve or decode failure
    // must not panic the app at startup. Ongoing icon updates go through
    // `sync_tray_with` which already handles this with `load_tray_icon`.
    match tray::load_tray_icon(
        app_handle
            .path()
            .resolve(initial_icon_path, tauri::path::BaseDirectory::Resource),
    ) {
        Ok(icon) => {
            tray_builder = tray_builder.icon(icon);
        }
        Err(err) => {
            log::warn!(
                "Failed to load initial tray icon '{}': {err}",
                initial_icon_path
            );
        }
    }

    // Windows notification-area convention: left click opens the app, right click
    // shows the menu. Elsewhere (macOS menu bar, Linux) the menu stays on left click.
    #[cfg(target_os = "windows")]
    {
        tray_builder = tray_builder
            .show_menu_on_left_click(false)
            .on_tray_icon_event(|tray, event| {
                use tauri::tray::{MouseButton, MouseButtonState, TrayIconEvent};
                let opens_window = matches!(
                    event,
                    TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } | TrayIconEvent::DoubleClick {
                        button: MouseButton::Left,
                        ..
                    }
                );
                if opens_window {
                    show_main_window(tray.app_handle());
                }
            });
    }
    #[cfg(not(target_os = "windows"))]
    {
        tray_builder = tray_builder.show_menu_on_left_click(true);
    }

    let tray = tray_builder
        .on_menu_event(|app, event| match event.id.as_ref() {
            "settings" => {
                show_main_window(app);
            }
            "secure_input_warning" => {
                // Full explanation lives in the settings-window banner
                show_main_window(app);
            }
            "check_updates" => {
                let settings = settings::get_settings(app);
                if settings::update_checks_effectively_enabled(&settings) {
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
                    tray::update_tray_menu(&app_clone);
                });
            }
            _ => {}
        })
        .build(app_handle)
        .unwrap();
    app_handle.manage(tray);

    // Initialize tray menu with idle state
    tray::update_tray_menu(app_handle);

    // Apply show_tray_icon setting
    let settings = settings::get_settings(app_handle);
    if !settings.show_tray_icon {
        tray::set_tray_visibility(app_handle, false);
    }

    // Refresh tray menu when model state changes
    let app_handle_for_listener = app_handle.clone();
    app_handle.listen("model-state-changed", move |_| {
        tray::update_tray_menu(&app_handle_for_listener);
    });

    // Apply the autostart preference (SMAppService login item on macOS 13+,
    // tauri-plugin-autostart elsewhere)
    autostart::apply_autostart(app_handle, settings.autostart_enabled);

    // Create the recording overlay window (hidden by default)
    utils::create_recording_overlay(app_handle);
}

#[tauri::command]
#[specta::specta]
fn trigger_update_check(app: AppHandle) -> Result<(), String> {
    let settings = settings::get_settings(&app);
    if !settings::update_checks_effectively_enabled(&settings) {
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

/// Convert an unexpected panic on the headless worker into a normal CLI
/// failure. Without this guard the Tauri event loop remains alive after the
/// worker exits, leaving `--transcribe-file` hung indefinitely.
fn run_headless_guarded<F>(operation: F) -> i32
where
    F: FnOnce() -> i32,
{
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(operation)) {
        Ok(code) => code,
        Err(payload) => {
            let message = if let Some(message) = payload.downcast_ref::<&str>() {
                (*message).to_string()
            } else if let Some(message) = payload.downcast_ref::<String>() {
                message.clone()
            } else {
                "unknown panic".to_string()
            };
            eprintln!("error: headless transcription panicked: {message}");
            1
        }
    }
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

    // read_wav_samples decodes 16/24-bit PCM and 32-bit float at any sample rate
    // and resamples to 16 kHz, which is what lets the app's own raw recordings
    // (e.g. 48 kHz float when `save_raw_audio` is on) round-trip through here.
    // Only channel count is unhandled, so that is the one thing rejected.
    match hound::WavReader::open(&wav) {
        Ok(reader) => {
            let spec = reader.spec();
            if spec.channels != 1 {
                eprintln!(
                    "error: expected a mono WAV, got {} channels ({} Hz / {}-bit {:?})",
                    spec.channels, spec.sample_rate, spec.bits_per_sample, spec.sample_format
                );
                return 2;
            }
        }
        Err(e) => {
            eprintln!("error: cannot open {}: {}", wav.display(), e);
            return 2;
        }
    }

    let read_start = Instant::now();
    let samples = match crate::audio_toolkit::read_wav_samples(&wav) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: failed to read {}: {}", wav.display(), e);
            return 2;
        }
    };
    let wav_read_ms = read_start.elapsed().as_secs_f64() * 1000.0;
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

    if args.repeat.is_some_and(|runs| runs != 3) {
        eprintln!("error: benchmarks require exactly 3 runs (warm-up, measured, measured)");
        return 2;
    }
    let runs = 3;
    let mut times_ms: Vec<f64> = Vec::new();
    let mut pipeline_runs = Vec::new();
    let mut text = String::new();
    for i in 0..runs {
        if !tm.is_model_loaded() {
            eprintln!(
                "error: model was unloaded before run {}; warm benchmark invalid",
                i + 1
            );
            return 1;
        }
        let t = Instant::now();
        let result = if let Some(chunk_ms) = args.stream_chunk_ms {
            tm.benchmark_stream(&samples, chunk_ms as usize, args.stream_att_right)
                .map(|(text, metrics)| {
                    pipeline_runs.push(metrics);
                    text
                })
        } else {
            tm.transcribe(samples.clone()).map(|text| {
                pipeline_runs.push(tm.pipeline_metrics());
                text
            })
        };
        match result {
            Ok(out) => text = out,
            Err(e) => {
                eprintln!("error: transcribe failed: {}", e);
                return 1;
            }
        }
        times_ms.push(t.elapsed().as_secs_f64() * 1000.0);
        pipeline_runs[i]["excluded_warmup"] = serde_json::json!(i == 0);
        pipeline_runs[i]["text"] = serde_json::json!(text);
        pipeline_runs[i]["wall_ms"] = serde_json::json!(times_ms[i]);
    }
    let warm_mean_ms = (times_ms[1] + times_ms[2]) / 2.0;
    let best_ms = times_ms[1].min(times_ms[2]);
    let rtf = if warm_mean_ms > 0.0 {
        audio_secs / (warm_mean_ms / 1000.0)
    } else {
        0.0
    };

    if args.json {
        println!(
            "{}",
            serde_json::json!({
                "schema_version": 1,
                "native_commit": transcribe_cpp::version_commit(),
                "native_build_id": managers::transcription::native_build_identity(),
                "app_version": env!("CARGO_PKG_VERSION"),
                "debug_build": cfg!(debug_assertions),
                "wav_read_ms": wav_read_ms,
                "pipeline_runs": pipeline_runs,
                "stream_chunk_ms": args.stream_chunk_ms,
                "rtf_compute_over_audio": if audio_secs > 0.0 { warm_mean_ms / (audio_secs*1000.0) } else { 0.0 },
                "warm_mean_ms": warm_mean_ms,
                "excluded_warmup_run": 1,
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
            "model={} device={} backend={} audio={:.2}s load={}ms warm_mean={}ms speed={:.2}x",
            model_id,
            requested_device,
            bound_backend.as_deref().unwrap_or("?"),
            audio_secs,
            load_ms,
            warm_mean_ms,
            rtf,
        );
        println!("text: {}", text);
    }
    0
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run(cli_args: CliArgs) {
    // Avoid ggml-metal residency-set teardown assertions when a native engine
    // outlives the Tauri shutdown sequence (#1902). This must happen before
    // transcribe-cpp initializes its Metal device. Advanced users can restore
    // upstream residency behavior with <prefix>METAL_RESIDENCY=1, where the
    // prefix is `app_identity::ENV_PREFIX`.
    #[cfg(target_os = "macos")]
    if utils::app_env_var("METAL_RESIDENCY").as_deref() == Some("1") {
        // ggml treats GGML_METAL_NO_RESIDENCY as presence-based, so remove an
        // inherited value as well when explicitly opting back in.
        unsafe { std::env::remove_var("GGML_METAL_NO_RESIDENCY") };
    } else {
        unsafe { std::env::set_var("GGML_METAL_NO_RESIDENCY", "1") };
    }

    // Pin glibc's dynamic mmap threshold before the first large allocation,
    // so per-dictation transient buffers are returned to the OS on free
    // instead of accumulating in malloc arenas (#1792). No-op off Linux/glibc.
    memory::init_allocator();

    // Detect portable mode before anything else
    portable::init();

    let specta_builder = Builder::<tauri::DynRuntime>::new()
        .dangerously_cast_bigints_to_number()
        .commands(collect_commands![
            shortcut::change_binding,
            shortcut::reset_binding,
            shortcut::change_shortcut_activation_setting,
            shortcut::change_hold_threshold_ms_setting,
            shortcut::change_audio_feedback_setting,
            shortcut::change_audio_feedback_volume_setting,
            shortcut::change_sound_theme_setting,
            shortcut::change_theme_setting,
            shortcut::change_custom_accent_color_setting,
            shortcut::change_ui_scale_setting,
            shortcut::change_start_hidden_setting,
            shortcut::change_autostart_setting,
            shortcut::change_translate_to_english_setting,
            shortcut::change_selected_language_setting,
            shortcut::change_overlay_position_setting,
            shortcut::change_overlay_style_setting,
            shortcut::change_overlay_direct_mode_setting,
            shortcut::change_overlay_direct_speed_setting,
            shortcut::change_overlay_back_correction_setting,
            shortcut::change_overlay_window_fade_ms_setting,
            shortcut::change_overlay_window_corner_radius_setting,
            shortcut::change_overlay_speech_stats_setting,
            shortcut::change_speech_pause_hold_setting,
            shortcut::change_direct_streaming_speed_setting,
            shortcut::change_debug_mode_setting,
            shortcut::change_word_correction_threshold_setting,
            shortcut::change_extra_recording_buffer_setting,
            shortcut::change_paste_delay_ms_setting,
            shortcut::change_paste_delay_after_ms_setting,
            shortcut::change_reliable_paste_setting,
            shortcut::change_paste_method_setting,
            shortcut::get_available_typing_tools,
            shortcut::change_typing_tool_setting,
            shortcut::change_external_script_path_setting,
            shortcut::change_clipboard_handling_setting,
            shortcut::change_auto_submit_setting,
            shortcut::change_auto_submit_key_setting,
            shortcut::change_multi_stt_enabled_setting,
            shortcut::change_multi_stt_extra_model,
            shortcut::change_multi_stt_extra_model_language,
            shortcut::change_multi_stt_merge_prompt,
            shortcut::change_multi_stt_streaming_first_enabled_setting,
            shortcut::change_multi_stt_streaming_pause_ms_setting,
            shortcut::change_multi_stt_streaming_context_chunks_setting,
            shortcut::change_multi_stt_streaming_multi_enabled_setting,
            shortcut::change_multi_stt_streaming_multi_debug_view_setting,
            shortcut::change_multi_stt_translate_model_2,
            shortcut::change_multi_stt_translate_model_3,
            shortcut::change_multi_stt_translate_model_4,
            shortcut::change_multi_stt_keep_extra_models_loaded_setting,
            shortcut::change_multi_stt_performance_mode_enabled_setting,
            shortcut::change_multi_stt_performance_mode_trigger_on_start_setting,
            shortcut::change_multi_stt_performance_mode_full_power_shortcut,
            shortcut::change_multi_stt_performance_mode_normal_shortcut,
            shortcut::change_mic_idle_timeout_settings,
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
            shortcut::update_custom_words,
            shortcut::suspend_all_bindings,
            shortcut::resume_all_bindings,
            shortcut::change_mute_while_recording_setting,
            shortcut::change_append_trailing_space_setting,
            shortcut::change_append_trailing_newline_setting,
            shortcut::change_lazy_stream_close_setting,
            shortcut::change_save_raw_audio_setting,
            shortcut::change_vad_enabled_setting,
            shortcut::change_overlay_scope_settings,
            shortcut::change_denoise_enabled_setting,
            shortcut::change_denoise_strength_setting,
            shortcut::change_denoise_vad_threshold_setting,
            shortcut::change_denoise_vad_grace_setting,
            shortcut::change_filler_word_removal_enabled_setting,
            shortcut::change_app_language_setting,
            shortcut::change_update_checks_setting,
            shortcut::change_show_whats_new_on_update_setting,
            shortcut::change_whats_new_last_seen_version_setting,
            shortcut::change_keyboard_implementation_setting,
            shortcut::get_keyboard_implementation,
            shortcut::change_show_tray_icon_setting,
            shortcut::change_transcribe_accelerator_setting,
            shortcut::change_transcribe_gpu_device,
            shortcut::set_model_backend_setting,
            shortcut::get_available_accelerators,
            shortcut::native_keys::start_native_keys_recording,
            shortcut::native_keys::stop_native_keys_recording,
            secure_input::get_secure_input_status,
            secure_input::run_keyboard_diagnostic,
            trigger_update_check,
            show_main_window_command,
            commands::cancel_operation,
            commands::is_portable,
            commands::audio::start_vad_test,
            commands::audio::stop_vad_test,
            commands::audio::start_overlay_preview,
            commands::audio::stop_overlay_preview,
            commands::llama::get_llama_server_state,
            commands::llama::get_llama_server_logs,
            commands::llama::start_llama_server,
            commands::llama::stop_llama_server,
            commands::llama::restart_llama_server,
            commands::llama::change_llama_settings,
            commands::llama::get_llama_command_preview,
            commands::llama::list_gguf_files,
            commands::llama::detect_llama_install,
            commands::llama::apply_llama_to_post_processing,
            commands::llama::fetch_llama_releases,
            commands::llama::detect_llama_backend,
            commands::llama::list_installed_llama_servers,
            commands::llama::install_llama_release,
            commands::llama::remove_installed_llama_server,
            commands::llama::detect_cuda_toolkit,
            commands::llama::remove_bundled_cuda_runtime,
            commands::system::get_system_stats,
            commands::is_update_checks_locked,
            commands::get_app_dir_path,
            commands::get_app_settings,
            commands::get_default_settings,
            commands::get_log_dir_path,
            commands::get_recent_logs,
            commands::clear_logs,
            commands::set_log_level,
            commands::open_recordings_folder,
            commands::open_models_folder,
            commands::open_plugins_folder,
            commands::open_log_dir,
            commands::open_app_data_dir,
            commands::check_apple_intelligence_available,
            commands::initialize_enigo,
            commands::initialize_shortcuts,
            commands::models::get_arch_plugins,
            commands::models::load_arch_plugin,
            commands::models::register_arch_dir,
            commands::models::get_available_models,
            commands::models::get_model_info,
            commands::models::download_model,
            commands::models::download_model_quant,
            commands::models::get_model_quant_variants,
            commands::models::change_native_streaming_latency_preset_setting,
            commands::models::change_native_streaming_chunk_ms_setting,
            commands::models::delete_model,
            commands::models::cancel_download,
            commands::models::set_active_model,
            commands::models::get_current_model,
            commands::models::get_transcription_model_status,
            commands::models::is_model_loading,
            commands::models::rescan_local_models,
            commands::models::benchmark_model_quantizations,
            commands::models::benchmark_single_quantization,
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
            commands::audio::get_microphone_channels,
            commands::audio::set_selected_channel,
            commands::transcription::set_model_unload_timeout,
            commands::transcription::get_model_load_status,
            commands::transcription::unload_model_manually,
            commands::transcription::unload_extra_model,
            commands::transcription::get_extra_loaded_models,
            commands::transcription::load_extra_model,
            commands::history::get_history_entries,
            commands::history::toggle_history_entry_saved,
            commands::history::get_audio_file_path,
            commands::history::delete_history_entry,
            commands::history::delete_all_recordings,
            commands::history::retry_history_entry_transcription,
            commands::history::post_process_history_entry,
            commands::history::multi_stt_history_entry,
            commands::history::update_history_limit,
            commands::history::update_recording_retention_period,
            commands::history::get_latest_recording_info,
            commands::statistics::get_statistics_summary,
            commands::statistics::reset_statistics,
            helpers::clamshell::is_laptop,
            shortcut::change_vad_threshold_setting,
            commands::file_transcription::change_file_transcription_settings,
            commands::file_transcription::list_audio_files_in_folder,
            commands::file_transcription::start_file_transcription,
            commands::file_transcription::cancel_file_transcription,
            commands::file_transcription::get_file_transcription_status,
            commands::file_transcription::reveal_path_in_file_manager,
            commands::file_transcription::read_text_file,
            commands::live_mode::change_live_mode_settings,
            commands::live_mode::live_mode_start,
            commands::live_mode::live_mode_stop,
            commands::live_mode::live_mode_status,
            commands::live_mode::live_mode_list_sessions,
            commands::live_mode::live_mode_default_output_dir,
            commands::recall::change_recall_settings,
            commands::recall::recall_vault_info,
            commands::recall::recall_default_vault_dir,
            commands::recall::recall_list_notes,
            commands::recall::recall_read_note,
            commands::recall::recall_create_note,
            commands::recall::recall_write_note,
            commands::recall::recall_delete_note,
            commands::recall::recall_save_transcription,
            commands::recall::recall_open_vault_folder,
            commands::recall::recall_dictate_start,
            commands::recall::recall_dictate_stop,
            commands::recall::recall_dictate_cancel,
            commands::recall::recall_set_insertion_mode,
            commands::recall::recall_encryption_status,
            commands::recall::recall_enable_encryption,
            commands::recall::recall_unlock_vault,
            commands::recall::recall_lock_vault,
            commands::recall::recall_disable_encryption,
            commands::live_fft::change_live_fft_settings,
            commands::live_fft::live_fft_start,
            commands::live_fft::live_fft_stop,
            commands::live_fft::live_fft_status,
            commands::live_fft::live_fft_reset,
            commands::live_fft::live_fft_raw_defaults,
            overlay::overlay_stream_text_height,
            overlay::remember_recording_overlay_window_position,
            overlay::reset_recording_overlay_manual_position,
        ])
        .events(collect_events![
            managers::history::HistoryUpdatePayload,
            managers::transcription::StreamTextEvent,
            managers::transcription::StreamPhaseEvent,
            managers::statistics::StatisticsUpdatedEvent,
            overlay::SpeechActivityEvent,
            managers::audio::VadTestEvent,
            llama_server::LlamaServerStateEvent,
            llama_releases::LlamaDownloadEvent,
            system_monitor::SystemStatsEvent,
            file_transcription::FileTranscriptionEvent,
            live_mode::LiveModeStateEvent,
            live_mode::LiveModeTranscriptEvent,
            live_fft::LiveFftStateEvent,
            live_fft::LiveFftFrameEvent,
            multi_stt_stream::MultiSttStreamChunkFailedEvent,
            recall::insertion::RecallInsertTextEvent,
        ]);

    #[cfg(debug_assertions)] // <- Only export on non-release builds
    specta_builder
        .export(Typescript::default(), "../src/bindings.ts")
        .expect("Failed to export typescript bindings");

    let typed_handler = specta_builder.invoke_handler();
    // `overlay_scope_frame` answers with raw bytes (`tauri::ipc::Response`),
    // which tauri-specta cannot type, so it is dispatched beside the typed
    // commands instead of through `collect_commands!`. The helper hands the
    // macro closure the signature it needs to infer the runtime type.
    fn handler_for_runtime<F: Fn(tauri::ipc::Invoke<tauri::DynRuntime>) -> bool>(f: F) -> F {
        f
    }
    let binary_handler = handler_for_runtime(tauri::generate_handler![
        commands::live_fft::overlay_scope_frame
    ]);
    let invoke_handler = move |invoke: tauri::ipc::Invoke<tauri::DynRuntime>| -> bool {
        if invoke.message.command() == "overlay_scope_frame" {
            binary_handler(invoke)
        } else {
            typed_handler(invoke)
        }
    };

    // The headless path must run as its own instance (see the single-instance
    // note below), not forward to an already-running app.
    let headless_mode =
        cli_args.transcribe_file.is_some() || cli_args.list_devices || cli_args.list_models;

    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default()
        .runtime(tauri_runtime_wry::Wry::default())
        .device_event_filter(tauri::DeviceEventFilter::Always)
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            LogBuilder::new()
                .level(log::LevelFilter::Trace) // Set to most verbose level globally
                .max_file_size(10_000_000)
                .rotation_strategy(RotationStrategy::KeepOne)
                .clear_targets()
                .targets([
                    // Interactive console output goes to stdout, headless to
                    // stderr; see `console_stream_kind`.
                    Target::new(console_stream_kind(headless_mode)),
                    // The durable console uses the same capture policy as the terminal.
                    Target::new(if let Some(data_dir) = portable::data_dir() {
                        TargetKind::Folder {
                            path: data_dir.join("logs"),
                            file_name: Some(app_identity::RECORDING_BASENAME.into()),
                        }
                    } else {
                        TargetKind::LogDir {
                            file_name: Some(app_identity::RECORDING_BASENAME.into()),
                        }
                    }),
                ])
                .build(),
        );

    #[cfg(target_os = "macos")]
    {
        builder = builder.plugin(tauri_nspanel::init());
    }

    // Single-instance forwards CLI args to an already-running instance and exits.
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
                // A second process was launched without remote-control flags
                // (e.g. the binary run from a shell). On macOS, relaunching the
                // bundle from Spotlight/Finder/Dock does not start a process —
                // it arrives as RunEvent::Reopen below — but treat this the
                // same way: raise the window and recreate a possibly vanished
                // tray icon (#1948).
                #[cfg(target_os = "macos")]
                tray::recreate_tray_icon(app);
                show_main_window(app);
            }
        }));
    }

    #[allow(unused_mut)]
    let mut app = builder
        .plugin(tauri_plugin_fs::init())
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
            #[cfg(target_os = "windows")]
            log::info!(
                "Vulkan layer policy: VK_LOADER_LAYERS_DISABLE={:?}, {}KEEP_VULKAN_IMPLICIT_LAYERS={}",
                std::env::var_os("VK_LOADER_LAYERS_DISABLE"),
                app_identity::ENV_PREFIX,
                utils::app_env_flag("KEEP_VULKAN_IMPLICIT_LAYERS"),
            );

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
                managers::arch_plugins::init_arch_plugin_dirs(&app_handle);
                managers::transcription::apply_accelerator_settings(&app_handle);

                let handle = app_handle.clone();
                let args = cli_args.clone();
                std::thread::spawn(move || {
                    let code = run_headless_guarded(|| run_headless_transcription(&handle, &args));
                    // Drop the loaded engine before teardown: ggml-metal's global
                    // device free asserts (SIGABRT) if a model's Metal resources
                    // are still alive at C++ static-destructor time.
                    if let Some(tm) = handle.try_state::<Arc<TranscriptionManager>>() {
                        let _ = tm.unload_model();
                        tm.unload_all_extra_models();
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
            // for portable mode (redirects WebView2 cache to portable Data dir)
            let mut win_builder =
                tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::App("/".into()))
                    .title(app_identity::NAME)
                    // Sized so the 13-entry sidebar is fully visible without
                    // scrolling (13 × 44 px + logo) and the status bar — model
                    // pill, quantization, streaming latency, brain, CPU / RAM /
                    // GPU / VRAM meters, updater — fits on one line next to the
                    // default 208 px sidebar (≈ 1010 px of bar + sidebar). The
                    // sidebar scrolls and collapses below that anyway.
                    .inner_size(1060.0, 720.0)
                    .min_inner_size(1060.0, 720.0)
                    .resizable(true)
                    .maximizable(true)
                    .visible(false);

            if let Some(data_dir) = portable::data_dir() {
                win_builder = win_builder.data_directory(data_dir.join("webview"));

                // The asset protocol's static scope only knows `$APPDATA`, which
                // Tauri resolves from the bundle identifier — not from the
                // portable `Data/` directory beside the executable. Portable
                // history playback (`convertFileSrc` on a recording's path)
                // therefore needs the recordings directory allowed at runtime,
                // per the asset-protocol docs' runtime-scope route. Non-portable
                // installs need nothing: `$APPDATA/**/*` already covers them.
                app.asset_protocol_scope()
                    .allow_directory(data_dir.join("recordings"), false)
                    .map_err(|e| {
                        log::warn!(
                            "portable mode: could not extend the asset protocol \
                             scope to the recordings directory: {e}"
                        )
                    })
                    .ok();
            }

            // Only used on Windows, to disable WebView2 browser accelerators.
            #[cfg_attr(not(target_os = "windows"), allow(unused_variables))]
            let main_window = win_builder.build()?;

            // Disable WebView2 browser accelerators (F5, F6, Ctrl+F, F12, ...).
            // A settings window has no use for them, and pressing F6 while
            // recording a shortcut was reported to turn the whole window white
            // (upstream issue #1940), likely by triggering WebView2 focus
            // cycling. DevTools stays enabled; only the F12 accelerator is
            // lost.
            #[cfg(target_os = "windows")]
            {
                let main_window_label = main_window.label().to_string();
                let _ = main_window.with_webview(move |webview| unsafe {
                    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings3;
                    // In Tauri 3 the app runs under the type-erased DynRuntime,
                    // so `with_webview` hands us a PlatformWebview<DynRuntime>.
                    // Downcast to the concrete wry Webview to reach `controller()`
                    // and the WebView2 COM interfaces.
                    use windows_core::Interface;

                    let Some(webview) = webview.downcast_ref::<tauri_runtime_wry::Webview>() else {
                        log::warn!("Failed to downcast webview to wry runtime for '{}'", main_window_label);
                        return;
                    };

                    let result = webview
                        .controller()
                        .CoreWebView2()
                        .and_then(|core| core.Settings())
                        .and_then(|settings| settings.cast::<ICoreWebView2Settings3>())
                        .and_then(|settings| settings.SetAreBrowserAcceleratorKeysEnabled(false));

                    if let Err(error) = result {
                        log::warn!("Failed to disable WebView2 browser accelerators: {error}");
                    }
                });
            }

            let mut settings = get_settings(app.handle());

            // Apply the persisted appearance theme to the native title bar before
            // the window is shown, so it matches the in-app palette without a flash
            // of the wrong theme. See `apply_window_theme` for what this does per
            // platform.
            #[cfg(any(target_os = "windows", target_os = "macos"))]
            shortcut::apply_window_theme(app.handle(), settings.theme);

            // CLI --debug flag overrides debug_mode and log level (runtime-only, not persisted)
            if cli_args.debug {
                settings.debug_mode = true;
                settings.log_level = settings::LogLevel::Trace;
            }

            let file_log_level = log::LevelFilter::Trace;
            if let Ok(config_dir) = app.path().app_config_dir() {
                log::info!(
                    "Settings store: {} (log level {:?}, debug mode {})",
                    config_dir
                        .join(portable::store_path(settings::SETTINGS_STORE_PATH))
                        .display(),
                    settings.log_level,
                    settings.debug_mode,
                );
            }
            {
                use std::io::IsTerminal;
                log::info!(
                    "Console log stream: {} (stdout is a terminal: {}, stderr is a terminal: {}); \
                     default capture {}, viewer reads durable file (debug mode {})",
                    if headless_mode { "stderr" } else { "stdout" },
                    std::io::stdout().is_terminal(),
                    std::io::stderr().is_terminal(),
                    file_log_level,
                    settings.debug_mode,
                );
            }
            let app_handle = app.handle().clone();
            app.manage(TranscriptionCoordinator::new(app_handle.clone()));

            initialize_core_logic(&app_handle);

            // Secure Input monitor (macOS): detects stuck secure input that
            // silently blocks keyed shortcuts, warns the user, and activates
            // the Carbon fallback. See secure_input.rs and issue #1578.
            secure_input::init(&app_handle);

            // Populate the overlay-enabled cache from initial settings so the
            // audio path (overlay::emit_speech_activity, on every speech flip)
            // can do a single atomic load instead of reading the Tauri store.
            // Kept in sync by shortcut::change_overlay_style_setting.
            overlay::update_overlay_enabled_cache(
                settings.overlay_style != settings::OverlayStyle::None,
            );
            overlay::update_speech_stats_enabled_cache(settings.overlay_speech_stats);
            overlay::update_overlay_scope_cache(&settings.overlay_scope);

            // Pre-warm GPU/accelerator enumeration on a background thread. The first
            // get_available_accelerators call enumerates transcribe-cpp compute
            // devices, which can take a moment; without this
            // the cost is paid synchronously when the user first opens Advanced
            // settings, freezing the UI. Result is cached in a OnceLock.
            std::thread::spawn(|| {
                let _ = crate::managers::transcription::get_available_accelerators();
            });

            // Hide tray icon if --no-tray was passed
            if cli_args.no_tray {
                tray::set_tray_visibility(&app_handle, false);
            }

            // Show main window only if not starting hidden.
            // CLI --start-hidden flag overrides the setting.
            // But if permission onboarding is required, always show the window.
            let should_hide = settings.start_hidden || cli_args.start_hidden;
            let should_force_show = should_force_show_permissions_window(&app_handle);

            // If start_hidden but tray is disabled, we must show the window
            // anyway. Without a tray icon, the dock is the only way back in.
            // Keep in sync with `apply_startup_activation_policy` (macOS).
            let tray_available = settings.show_tray_icon && !cli_args.no_tray;
            if should_force_show || !should_hide || !tray_available {
                show_main_window(&app_handle);
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
        .expect("error while building tauri application");

    // Must sit between build() and run(): see the doc comment.
    #[cfg(target_os = "macos")]
    apply_startup_activation_policy(&mut app, headless_mode);

    app.run(|app, event| match &event {
        #[cfg(target_os = "macos")]
        tauri::RunEvent::Reopen { .. } => {
            // Fired when the already-running bundle is launched again from
            // Spotlight/Finder or the Dock icon is clicked. If the settings
            // window is hidden, the user is likely looking for a tray icon
            // that vanished (#1948): recreate it. When the window is
            // already visible this is just a focus request and the tray is
            // left alone.
            let window_visible = app
                .get_webview_window("main")
                .and_then(|w| w.is_visible().ok())
                .unwrap_or(false);
            if !window_visible {
                tray::recreate_tray_icon(app);
            }
            show_main_window(app);
        }
        // Teardown transcribe.cpp before exit
        tauri::RunEvent::Exit => {
            if let Some(fft) = app.try_state::<Arc<live_fft::LiveFftManager>>() {
                fft.stop_overlay_scope();
                let _ = fft.stop();
            }
            if let Some(tm) = app.try_state::<Arc<TranscriptionManager>>() {
                let _ = tm.unload_model();
                tm.unload_all_extra_models();
            }
            // The job object would end llama-server anyway; stop it cleanly
            // first so the port is released before the process goes.
            if settings::get_settings(app).llama.stop_on_exit
                && let Some(llama) = llama_server::global()
            {
                llama.stop();
            }
        }
        _ => {}
    });
}

#[cfg(test)]
mod console_logging_tests {
    /// Which stream the console target writes to is a *measured* decision, not a
    /// derivable one (see `console_stream_kind`), so it is pinned here. It
    /// regressed once already — stderr reads as the safer choice and was silent —
    /// and nothing else in the suite would have noticed, because the file target
    /// kept recording every one of the records the terminal never received.
    #[test]
    fn the_interactive_console_writes_where_the_terminal_shows_it() {
        use super::console_stream_kind;
        use tauri_plugin_log::TargetKind;

        // The app a user watches: stdout, because that is the fd their terminal
        // was measured to own.
        assert!(matches!(console_stream_kind(false), TargetKind::Stdout));
        // Headless one-shots: stderr, so `--list-models` / `--json` stay parseable.
        assert!(matches!(console_stream_kind(true), TargetKind::Stderr));
    }
}

#[cfg(test)]
mod headless_guard_tests {
    use super::run_headless_guarded;

    #[test]
    fn preserves_normal_exit_codes() {
        assert_eq!(run_headless_guarded(|| 2), 2);
    }

    #[test]
    fn converts_worker_panics_to_runtime_failures() {
        assert_eq!(run_headless_guarded(|| panic!("simulated failure")), 1);
    }
}
