use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tauri::{Emitter, Manager};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

static APP_HANDLE: OnceLock<tauri::AppHandle> = OnceLock::new();

pub fn set_app_handle(handle: tauri::AppHandle) {
    let _ = APP_HANDLE.set(handle);
}

fn emit_model_status(phase: &str, model: Option<&str>, cuda: bool, error: Option<&str>) {
    if let Some(app) = APP_HANDLE.get() {
        let payload = serde_json::json!({
            "phase": phase,
            "model": model,
            "cuda": cuda,
            "error": error,
        });
        let _ = app.emit("piper-status-changed", payload);
    }
}

#[derive(Clone)]
pub struct ServerHandle {
    pub port: u16,
    pub client: reqwest::blocking::Client,
}

pub struct ActiveServer {
    pub child: Mutex<std::process::Child>,
    pub port: u16,
    pub model_name: String,
    pub cuda: bool,
    pub client: reqwest::blocking::Client,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PythonCommand {
    pub executable: String,
    pub args: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StartingConfig {
    pub command: PythonCommand,
    pub data_dir: String,
    pub cuda: bool,
    pub voice: String,
}

pub enum ServerState {
    Stopped,
    Starting {
        _generation: u64,
        config: StartingConfig,
        stderr_tail: Arc<Mutex<Vec<String>>>,
    },
    Ready(Arc<ActiveServer>),
}

static CURRENT_GENERATION: AtomicU64 = AtomicU64::new(0);
static SERVER_STATE: OnceLock<Mutex<ServerState>> = OnceLock::new();

fn get_server_state() -> &'static Mutex<ServerState> {
    SERVER_STATE.get_or_init(|| Mutex::new(ServerState::Stopped))
}

fn get_piper_client() -> &'static reqwest::blocking::Client {
    static CLIENT: OnceLock<reqwest::blocking::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::blocking::Client::builder()
            .tcp_nodelay(true)
            .connect_timeout(std::time::Duration::from_secs(2))
            .pool_max_idle_per_host(2)
            .build()
            .expect("Failed to build Piper HTTP client")
    })
}

fn get_free_port() -> Option<u16> {
    std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .map(|addr| addr.port())
        .ok()
}

#[cfg(windows)]
fn get_expanded_path() -> String {
    static EXPANDED_PATH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    EXPANDED_PATH
        .get_or_init(|| {
            use std::collections::HashSet;
            use std::env;

            let current_path = env::var("PATH").unwrap_or_default();
            let mut paths: Vec<String> = current_path.split(';').map(|s| s.to_string()).collect();
            let mut seen: HashSet<String> = paths.iter().cloned().collect();

            let home = env::var("USERPROFILE").unwrap_or_default();
            let home_path = std::path::Path::new(&home);

            let mut extra_paths = Vec::new();

            if !home.is_empty() {
                extra_paths.push(home_path.join(".local").join("bin"));
                extra_paths.push(
                    home_path
                        .join("AppData")
                        .join("Roaming")
                        .join("uv")
                        .join("tools")
                        .join("piper")
                        .join("Scripts"),
                );
                extra_paths.push(home_path.join("AppData").join("Local").join("bin"));

                // Add user Python installation Scripts and Python directories
                for ver in &["314", "313", "312", "311", "310"] {
                    extra_paths.push(
                        home_path
                            .join("AppData")
                            .join("Roaming")
                            .join("Python")
                            .join(format!("Python{}", ver))
                            .join("Scripts"),
                    );
                    extra_paths.push(
                        home_path
                            .join("AppData")
                            .join("Local")
                            .join("Python")
                            .join(format!("pythoncore-3.{}-64", &ver[1..]))
                            .join("Scripts"),
                    );
                    extra_paths.push(
                        home_path
                            .join("AppData")
                            .join("Local")
                            .join("Programs")
                            .join("Python")
                            .join(format!("Python{}", ver)),
                    );
                }
            }

            // Global Python paths
            for ver in &["314", "313", "312", "311", "310"] {
                extra_paths.push(std::path::PathBuf::from(format!(
                    r"C:\Python{}\Scripts",
                    ver
                )));
                extra_paths.push(std::path::PathBuf::from(format!(r"C:\Python{}", ver)));
            }

            for p in extra_paths {
                let p_str = p.to_string_lossy().into_owned();
                if !seen.contains(&p_str) {
                    seen.insert(p_str.clone());
                    paths.push(p_str);
                }
            }

            paths.join(";")
        })
        .clone()
}

#[cfg(not(windows))]
fn get_expanded_path() -> String {
    std::env::var("PATH").unwrap_or_default()
}

pub fn resolve_piper_voices_dir(app: Option<&tauri::AppHandle>) -> std::path::PathBuf {
    // 1. Check if we have a portable or app data directory (models/TTS/piper-voices/)
    if let Some(app) = app {
        if let Ok(app_data) = crate::portable::app_data_dir(app) {
            let path = app_data.join("models").join("TTS").join("piper-voices");
            if path.exists() {
                return path;
            }
        }
    }

    // 2. Check current working directory / models / TTS / piper-voices (dev mode)
    if let Ok(cwd) = std::env::current_dir() {
        let path = cwd.join("models").join("TTS").join("piper-voices");
        if path.exists() {
            return path;
        }

        // Also check if CWD is src-tauri
        let path_parent = cwd
            .join("..")
            .join("models")
            .join("TTS")
            .join("piper-voices");
        if path_parent.exists() {
            return path_parent;
        }

        // Legacy compat: old flat models/piper-voices/
        let path_legacy = cwd.join("models").join("piper-voices");
        if path_legacy.exists() {
            return path_legacy;
        }
    }

    // 3. Fallback to resource directory if bundled
    if let Some(app) = app {
        if let Ok(res_path) = app.path().resolve(
            "resources/models/TTS/piper-voices",
            tauri::path::BaseDirectory::Resource,
        ) {
            if res_path.exists() {
                return res_path;
            }
        }
    }

    // 4. Default to project-local models/TTS/piper-voices (always inside S2B2S folder)
    //    Even if the directory doesn't exist yet, return this path so the user
    //    gets a clear error: "place .onnx files in models/TTS/piper-voices/"
    if let Ok(cwd) = std::env::current_dir() {
        let path = cwd.join("models").join("TTS").join("piper-voices");
        return path;
    }
    if let Some(app) = app {
        if let Ok(app_data) = crate::portable::app_data_dir(app) {
            return app_data.join("models").join("TTS").join("piper-voices");
        }
    }
    std::path::PathBuf::from("models")
        .join("TTS")
        .join("piper-voices")
}

#[cfg(windows)]
pub(crate) fn get_nvidia_dll_paths(command: &PythonCommand) -> Option<String> {
    static NVIDIA_PATHS: OnceLock<Option<String>> = OnceLock::new();
    NVIDIA_PATHS
        .get_or_init(|| {
            let mut cmd = Command::new(&command.executable);
            cmd.args(&command.args);
            cmd.args([
                "-c",
                "import os, nvidia; print(';'.join([os.path.join(os.path.dirname(nvidia.__file__), p, 'bin') for p in ['cublas', 'cuda_nvrtc', 'cuda_runtime', 'cudnn', 'cufft', 'curand', 'cusolver', 'cusparse', 'nvjitlink'] if os.path.exists(os.path.join(os.path.dirname(nvidia.__file__), p, 'bin'))]))"
            ]);
            cmd.creation_flags(CREATE_NO_WINDOW);
            let output = cmd.output().ok()?;

            if output.status.success() {
                let paths_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !paths_str.is_empty() {
                    return Some(paths_str);
                }
            }
            None
        })
        .clone()
}

/// Restart/Start the server in background.
fn spawn_start_thread(
    generation: u64,
    command: PythonCommand,
    voice: String,
    data_dir: String,
    cuda: bool,
    stderr_tail: Arc<Mutex<Vec<String>>>,
) {
    std::thread::spawn(move || {
        let voice_file = if voice.ends_with(".onnx") {
            voice.clone()
        } else {
            format!("{}.onnx", voice)
        };
        let mut model_path = std::path::PathBuf::from(&data_dir).join(&voice_file);
        if !model_path.exists() {
            let alt_path = std::path::PathBuf::from(&voice);
            if alt_path.exists() {
                model_path = alt_path;
            } else {
                log::warn!(
                    "[Piper] Start failed: model file not found at {}",
                    model_path.display()
                );
                emit_model_status("error", Some(&voice), cuda, Some("Model file not found"));
                let mut state = get_server_state().lock().unwrap_or_else(|p| p.into_inner());
                if CURRENT_GENERATION.load(Ordering::SeqCst) == generation {
                    *state = ServerState::Stopped;
                }
                return;
            }
        }

        let port = match get_free_port() {
            Some(p) => p,
            None => {
                log::warn!("[Piper] Start failed: no free port available");
                emit_model_status("error", Some(&voice), cuda, Some("No free port available"));
                let mut state = get_server_state().lock().unwrap_or_else(|p| p.into_inner());
                if CURRENT_GENERATION.load(Ordering::SeqCst) == generation {
                    *state = ServerState::Stopped;
                }
                return;
            }
        };

        log::info!(
            "[Piper] Starting HTTP server on port {} — model: {}, cuda: {}",
            port,
            model_path.display(),
            cuda
        );

        let mut cmd = std::process::Command::new(&command.executable);
        let mut args = Vec::new();
        args.extend(command.args.clone());
        args.extend(vec![
            "-m".to_string(),
            "piper.http_server".to_string(),
            "-m".to_string(),
            model_path.to_string_lossy().to_string(),
            "--port".to_string(),
            port.to_string(),
            "--host".to_string(),
            "127.0.0.1".to_string(),
        ]);

        if !data_dir.is_empty() {
            args.push("--data-dir".to_string());
            args.push(data_dir.clone());
        }

        if cuda {
            args.push("--cuda".to_string());
        }

        cmd.args(&args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        #[cfg(windows)]
        {
            cmd.creation_flags(CREATE_NO_WINDOW);
            if cuda {
                if let Some(nvidia_paths) = get_nvidia_dll_paths(&command) {
                    let current_path = get_expanded_path();
                    let new_path = format!("{};{}", nvidia_paths, current_path);
                    cmd.env("PATH", new_path);
                }
            }
        }

        emit_model_status("loading", Some(&voice), cuda, None);

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                log::warn!("[Piper] Start failed: spawn error — {}", e);
                emit_model_status(
                    "error",
                    Some(&voice),
                    cuda,
                    Some(&format!("Spawn error: {}", e)),
                );
                let mut state = get_server_state().lock().unwrap_or_else(|p| p.into_inner());
                if CURRENT_GENERATION.load(Ordering::SeqCst) == generation {
                    *state = ServerState::Stopped;
                }
                return;
            }
        };

        crate::job_object::register(&mut child);

        // Drain stdout in background
        if let Some(stdout) = child.stdout.take() {
            std::thread::spawn(move || {
                use std::io::BufRead;
                let reader = std::io::BufReader::new(stdout);
                for line in reader.lines() {
                    match line {
                        Ok(line) => log::debug!("[piper-server] {}", line),
                        Err(_) => break,
                    }
                }
            });
        }

        // Drain stderr to tail buffer and debug log
        let stderr_tail_clone = stderr_tail.clone();
        if let Some(stderr) = child.stderr.take() {
            std::thread::spawn(move || {
                use std::io::BufRead;
                let reader = std::io::BufReader::new(stderr);
                for line in reader.lines() {
                    if let Ok(line) = line {
                        log::debug!("[piper-server] {}", line);
                        let mut buffer =
                            stderr_tail_clone.lock().unwrap_or_else(|p| p.into_inner());
                        buffer.push(line);
                        if buffer.len() > 30 {
                            buffer.remove(0);
                        }
                    } else {
                        break;
                    }
                }
            });
        }

        // Health check client
        let health_client = match reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_millis(1000))
            .connect_timeout(std::time::Duration::from_millis(500))
            .build()
        {
            Ok(c) => c,
            Err(_) => {
                let _ = child.kill();
                log::warn!("[Piper] Start failed: could not build health-check client");
                emit_model_status(
                    "error",
                    Some(&voice),
                    cuda,
                    Some("Health client build failed"),
                );
                let mut state = get_server_state().lock().unwrap_or_else(|p| p.into_inner());
                if CURRENT_GENERATION.load(Ordering::SeqCst) == generation {
                    *state = ServerState::Stopped;
                }
                return;
            }
        };

        let url = format!("http://127.0.0.1:{}/voices", port);
        let poll_start = std::time::Instant::now();
        let mut poll_delay_ms = 100u64;
        let max_poll_delay_ms = 1600u64;

        loop {
            // Check if generation superseded
            if CURRENT_GENERATION.load(Ordering::SeqCst) != generation {
                log::info!(
                    "[Piper] Generation {} superseded. Killing child.",
                    generation
                );
                let _ = child.kill();
                return;
            }

            if let Ok(Some(status)) = child.try_wait() {
                let err_tail = {
                    let buffer = stderr_tail.lock().unwrap_or_else(|p| p.into_inner());
                    buffer.join("\n")
                };
                log::warn!(
                    "[Piper] Server exited prematurely with code {:?}. Stderr tail:\n{}",
                    status.code(),
                    err_tail
                );
                emit_model_status(
                    "error",
                    Some(&voice),
                    cuda,
                    Some("Server exited prematurely"),
                );
                let mut state = get_server_state().lock().unwrap_or_else(|p| p.into_inner());
                if CURRENT_GENERATION.load(Ordering::SeqCst) == generation {
                    *state = ServerState::Stopped;
                }
                return;
            }

            if let Ok(resp) = health_client.get(&url).send() {
                if resp.status().is_success() {
                    break;
                }
            }

            if poll_start.elapsed().as_millis() >= 10000 {
                log::info!(
                    "[Piper] Still waiting for server on port {} ({:.0}s elapsed)...",
                    port,
                    poll_start.elapsed().as_secs_f64()
                );
            }

            std::thread::sleep(std::time::Duration::from_millis(poll_delay_ms));
            poll_delay_ms = (poll_delay_ms * 2).min(max_poll_delay_ms);
        }

        log::info!(
            "[Piper] Server ready on port {} (generation {})",
            port,
            generation
        );

        // Substantial CUDA warmup sentence to compile JIT kernels
        emit_model_status("warming_up", Some(&voice), cuda, None);
        let warmup_text = "Hello, how can I help?";

        let warmup_client = get_piper_client();
        let warmup_url = format!("http://127.0.0.1:{}/", port);
        let warmup_body = serde_json::json!({ "text": warmup_text, "length_scale": 1.111111 });
        let warmup_start = std::time::Instant::now();
        match warmup_client.post(&warmup_url).json(&warmup_body).send() {
            Ok(resp) => {
                if let Ok(_bytes) = resp.bytes() {
                    log::info!(
                        "[Piper] Warmup completed in {:.1}s",
                        warmup_start.elapsed().as_secs_f64()
                    );
                }
            }
            Err(e) => {
                log::warn!(
                    "[Piper] Warmup failed: {}. First synthesis will be slower.",
                    e
                );
            }
        }

        // Lock state and store ready server
        let mut state = get_server_state().lock().unwrap_or_else(|p| p.into_inner());
        if CURRENT_GENERATION.load(Ordering::SeqCst) == generation {
            *state = ServerState::Ready(Arc::new(ActiveServer {
                child: Mutex::new(child),
                port,
                model_name: voice.clone(),
                cuda,
                client: get_piper_client().clone(),
            }));
            emit_model_status("ready", Some(&voice), cuda, None);
        } else {
            log::info!(
                "[Piper] Server on port {} was superseded during warmup. Killing.",
                port
            );
            let _ = child.kill();
        }
    });
}

fn test_python_import_piper(executable: &str) -> bool {
    let mut cmd = std::process::Command::new(executable);
    cmd.args(["-c", "import piper"]);
    #[cfg(windows)]
    {
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd.env("PATH", get_expanded_path());
    }
    match cmd.output() {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}

pub fn ensure_running(voice: String, cuda: bool) -> Result<ServerHandle, String> {
    let app = APP_HANDLE.get().ok_or("AppHandle not initialized")?;
    let mut python_exe = crate::tts::local_tts_server::resolve_venv_python();

    // Verify the resolved Python actually has piper-tts installed.
    // If not, fall back to system Python.
    if !test_python_import_piper(&python_exe) {
        let sys_fallback = if cfg!(windows) { "python" } else { "python3" };
        if python_exe != sys_fallback {
            log::warn!(
                "[Piper] Resolved Python '{}' does not have piper-tts installed. Falling back to system '{}'.",
                python_exe, sys_fallback
            );
            if test_python_import_piper(sys_fallback) {
                python_exe = sys_fallback.to_string();
            } else {
                return Err(format!(
                    "piper-tts is not installed.\n\
                     Install it with: pip install piper-tts[http]\n\
                     (checked venv at '{}' and system '{}')",
                    python_exe, sys_fallback
                ));
            }
        }
    }

    let command = PythonCommand {
        executable: python_exe,
        args: vec![
            "-u".to_string(),
            "-m".to_string(),
            "piper.http_server".to_string(),
        ],
    };
    let data_dir = resolve_piper_voices_dir(Some(app))
        .to_string_lossy()
        .to_string();

    loop {
        let mut state = get_server_state().lock().unwrap_or_else(|p| p.into_inner());
        match &mut *state {
            ServerState::Ready(server) => {
                let active = server.clone();
                drop(state);

                let is_alive = matches!(
                    active
                        .child
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .try_wait(),
                    Ok(None)
                );
                if is_alive && active.cuda == cuda && active.model_name == voice {
                    return Ok(ServerHandle {
                        port: active.port,
                        client: active.client.clone(),
                    });
                } else {
                    let mut state = get_server_state().lock().unwrap_or_else(|p| p.into_inner());
                    // Re-verify under lock in case someone changed it
                    if let ServerState::Ready(curr) = &*state {
                        if Arc::ptr_eq(curr, &active) {
                            log::info!(
                                "[Piper] Killing dead/mismatched/changed server on port {}",
                                active.port
                            );
                            {
                                let mut child =
                                    active.child.lock().unwrap_or_else(|p| p.into_inner());
                                let _ = child.kill();
                                let _ = child.wait(); // reap to avoid zombies on Unix
                            }
                            CURRENT_GENERATION.fetch_add(1, Ordering::SeqCst);
                            *state = ServerState::Stopped;
                        }
                    }
                }
            }
            ServerState::Starting {
                _generation: _,
                config: starting_config,
                stderr_tail: _,
            } => {
                if starting_config.command == command
                    && starting_config.data_dir == data_dir
                    && starting_config.cuda == cuda
                    && starting_config.voice == voice
                {
                    // Wait for it
                    drop(state);
                    std::thread::sleep(std::time::Duration::from_millis(200));
                } else {
                    // Mismatched configuration! Trigger new start
                    let new_gen = CURRENT_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
                    let tail = Arc::new(Mutex::new(Vec::new()));
                    *state = ServerState::Starting {
                        _generation: new_gen,
                        config: StartingConfig {
                            command: command.clone(),
                            data_dir: data_dir.clone(),
                            cuda,
                            voice: voice.clone(),
                        },
                        stderr_tail: tail.clone(),
                    };
                    drop(state);
                    spawn_start_thread(
                        new_gen,
                        command.clone(),
                        voice.clone(),
                        data_dir.clone(),
                        cuda,
                        tail,
                    );
                }
            }
            ServerState::Stopped => {
                let new_gen = CURRENT_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
                let tail = Arc::new(Mutex::new(Vec::new()));
                *state = ServerState::Starting {
                    _generation: new_gen,
                    config: StartingConfig {
                        command: command.clone(),
                        data_dir: data_dir.clone(),
                        cuda,
                        voice: voice.clone(),
                    },
                    stderr_tail: tail.clone(),
                };
                drop(state);
                spawn_start_thread(
                    new_gen,
                    command.clone(),
                    voice.clone(),
                    data_dir.clone(),
                    cuda,
                    tail,
                );
            }
        }
    }
}

/// Spawn a background thread that periodically checks the Piper server
/// idle state and unloads it when the configured ModelUnloadTimeout has
/// expired since the last synthesize request.
pub fn start_idle_watcher(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_secs(15));
            let settings = crate::settings::get_settings(&app);
            let timeout = settings.model_unload_timeout;
            match timeout.to_seconds() {
                Some(secs) if secs > 0 => {
                    // Check if server is ready
                    let status = get_piper_server_status();
                    if !status.ready {
                        continue;
                    }
                    // Check if Piper backend has been idle
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .ok()
                        .map(|d| d.as_millis() as u64)
                        .unwrap_or(0);
                    if app
                        .try_state::<std::sync::Arc<crate::tts::manager::TtsManager>>()
                        .is_some()
                    {
                        // Get the last_used from the idle atomic
                        let idle_ms = crate::settings::ModelUnloadTimeout::Sec15
                            .to_seconds()
                            .map(|check_secs| check_secs * 1000)
                            .unwrap_or(300_000);
                        let since_last_build =
                            crate::tts::backends::piper_server::get_last_synth_ms()
                                .map(|last| now.saturating_sub(last))
                                .unwrap_or(0);
                        if since_last_build > idle_ms && since_last_build > secs * 1000 {
                            log::info!("[Piper] Idle timeout ({secs}s) — unloading model");
                            unload_piper_model();
                        }
                    }
                }
                _ => {} // Never or Immediately — don't idle-watch
            }
        }
    });
}

static LAST_SYNTH_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Record the timestamp of a synthesis request (called from PiperBackend).
pub fn mark_synth() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    LAST_SYNTH_MS.store(now, std::sync::atomic::Ordering::Release);
}

/// Get the last synthesis timestamp for idle checking.
pub fn get_last_synth_ms() -> Option<u64> {
    let val = LAST_SYNTH_MS.load(std::sync::atomic::Ordering::Acquire);
    if val == 0 {
        None
    } else {
        Some(val)
    }
}

pub fn unload_piper_model() -> bool {
    let mut state = get_server_state().lock().unwrap_or_else(|p| p.into_inner());
    match &*state {
        ServerState::Ready(server) => {
            log::info!("[Piper] Unloading model on port {}", server.port);
            {
                let mut child = server.child.lock().unwrap_or_else(|p| p.into_inner());
                let _ = child.kill();
                let _ = child.wait();
            }
            *state = ServerState::Stopped;
            emit_model_status("stopped", None, false, None);
            true
        }
        ServerState::Starting { .. } => {
            log::info!("[Piper] Cancelling in-flight start via generation bump");
            CURRENT_GENERATION.fetch_add(1, Ordering::SeqCst);
            *state = ServerState::Stopped;
            emit_model_status("stopped", None, false, None);
            true
        }
        ServerState::Stopped => false,
    }
}

#[derive(serde::Serialize, Clone, Debug, specta::Type)]
pub struct PiperServerStatus {
    pub running: bool,
    pub model: Option<String>,
    pub port: Option<u16>,
    pub cuda: bool,
    pub ready: bool,
}

pub fn get_piper_server_status() -> PiperServerStatus {
    let state = get_server_state().lock().unwrap_or_else(|p| p.into_inner());
    match &*state {
        ServerState::Ready(server) => PiperServerStatus {
            running: true,
            model: Some(server.model_name.clone()),
            port: Some(server.port),
            cuda: server.cuda,
            ready: true,
        },
        ServerState::Starting { config, .. } => PiperServerStatus {
            running: true,
            model: None,
            port: None,
            cuda: config.cuda,
            ready: false,
        },
        ServerState::Stopped => PiperServerStatus {
            running: false,
            model: None,
            port: None,
            cuda: false,
            ready: false,
        },
    }
}
