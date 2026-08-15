use log::{error, info};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tauri::{AppHandle, Emitter, Manager};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

pub fn set_app_handle(handle: AppHandle) {
    let _ = APP_HANDLE.set(handle);
}

fn emit_status(phase: &str, error: Option<&str>) {
    if let Some(app) = APP_HANDLE.get() {
        let payload = serde_json::json!({
            "engine": "audiocpp",
            "phase": phase,
            "error": error,
        });
        let _ = app.emit("local-tts-status-changed", payload);
    }
}

#[derive(Clone)]
pub struct ServerHandle {
    pub port: u16,
    pub client: reqwest::blocking::Client,
}

#[allow(dead_code)]
struct ActiveServer {
    child: Mutex<std::process::Child>,
    port: u16,
    backend: String,
    client: reqwest::blocking::Client,
}

#[allow(dead_code)]
enum ServerState {
    Stopped,
    Starting { generation: u64 },
    Ready(Arc<ActiveServer>),
    Failed(String),
}

struct ServerSlot {
    generation: AtomicU64,
    state: OnceLock<Mutex<ServerState>>,
}

impl ServerSlot {
    const fn new() -> Self {
        Self {
            generation: AtomicU64::new(0),
            state: OnceLock::new(),
        }
    }

    fn state(&self) -> &Mutex<ServerState> {
        self.state.get_or_init(|| Mutex::new(ServerState::Stopped))
    }
}

static SLOT: ServerSlot = ServerSlot::new();

fn get_http_client() -> &'static reqwest::blocking::Client {
    static CLIENT: OnceLock<reqwest::blocking::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::blocking::Client::builder()
            .tcp_nodelay(true)
            .connect_timeout(std::time::Duration::from_secs(3))
            .pool_max_idle_per_host(4)
            .build()
            .expect("Failed to build HTTP client for audio.cpp")
    })
}

fn get_free_port() -> Option<u16> {
    std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .map(|addr| addr.port())
        .ok()
}

pub fn resolve_server_binary(app: &AppHandle) -> Option<PathBuf> {
    let binary_name = if cfg!(windows) {
        "audiocpp_server.exe"
    } else {
        "audiocpp_server"
    };

    // 1. Check App resources directory
    if let Ok(res_dir) = app.path().resource_dir() {
        let p = res_dir.join("resources").join("binaries").join(binary_name);
        if p.is_file() {
            return Some(p);
        }
        let p2 = res_dir.join(binary_name);
        if p2.is_file() {
            return Some(p2);
        }
    }

    // 2. Check S2B2S resources/binaries relative to manifest
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let staged = manifest_dir
        .join("resources")
        .join("binaries")
        .join(binary_name);
    if staged.is_file() {
        return Some(staged);
    }

    // 3. Check adjacent audio.cpp build output folders
    let candidate_dirs = [
        manifest_dir
            .join("..")
            .join("..")
            .join("audio.cpp")
            .join("build")
            .join("windows-cuda-release")
            .join("bin"),
        manifest_dir
            .join("..")
            .join("..")
            .join("audio.cpp")
            .join("build")
            .join("windows-vulkan-release")
            .join("bin"),
        manifest_dir
            .join("..")
            .join("..")
            .join("audio.cpp")
            .join("build")
            .join("windows-cpu-release")
            .join("bin"),
        manifest_dir
            .join("..")
            .join("..")
            .join("audio.cpp")
            .join("build")
            .join("linux-cuda-release")
            .join("bin"),
        manifest_dir
            .join("..")
            .join("..")
            .join("audio.cpp")
            .join("build")
            .join("macos-release")
            .join("bin"),
        manifest_dir
            .join("..")
            .join("..")
            .join("audio.cpp")
            .join("build")
            .join("Release")
            .join("bin"),
        manifest_dir
            .join("..")
            .join("..")
            .join("audio.cpp")
            .join("build")
            .join("bin"),
    ];

    for dir in &candidate_dirs {
        let p = dir.join(binary_name);
        if p.is_file() {
            return Some(p);
        }
    }

    // 4. Check PATH
    if let Some(paths) = std::env::var_os("PATH") {
        for path in std::env::split_paths(&paths) {
            let p = path.join(binary_name);
            if p.is_file() {
                return Some(p);
            }
        }
    }

    None
}

pub fn resolve_models_dir(app: &AppHandle) -> PathBuf {
    crate::portable::app_data_dir(app)
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("models")
        .join("TTS")
        .join("audiocpp")
}

pub fn get_ready_port() -> Option<u16> {
    let state = SLOT.state().lock().unwrap();
    match &*state {
        ServerState::Ready(server) => Some(server.port),
        _ => None,
    }
}

pub fn get_engine_status() -> Option<String> {
    let state = SLOT.state().lock().unwrap();
    match &*state {
        ServerState::Ready(_) => Some("ready".to_string()),
        ServerState::Starting { .. } => Some("loading".to_string()),
        ServerState::Failed(_) => Some("error".to_string()),
        ServerState::Stopped => Some("stopped".to_string()),
    }
}

pub fn unload() -> bool {
    let mut state = SLOT.state().lock().unwrap();
    SLOT.generation.fetch_add(1, Ordering::SeqCst);
    if let ServerState::Ready(server) = std::mem::replace(&mut *state, ServerState::Stopped) {
        info!(
            "[AudioCppServer] Unloading audio.cpp server on port {}",
            server.port
        );
        if let Ok(mut child) = server.child.lock() {
            let _ = child.kill();
            let _ = child.wait();
        }
        emit_status("stopped", None);
        true
    } else {
        *state = ServerState::Stopped;
        emit_status("stopped", None);
        false
    }
}

pub fn resolve_model_specs_dir(app: &AppHandle) -> Option<PathBuf> {
    // 1. Check App resources directory
    if let Ok(res_dir) = app.path().resource_dir() {
        let p = res_dir.join("resources").join("model_specs");
        if p.is_dir() {
            return Some(p);
        }
        let p2 = res_dir.join("model_specs");
        if p2.is_dir() {
            return Some(p2);
        }
    }

    // 2. Check S2B2S resources/model_specs relative to manifest
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let staged = manifest_dir.join("resources").join("model_specs");
    if staged.is_dir() {
        return Some(staged);
    }

    // 3. Check adjacent audio.cpp model_specs
    let adjacent = manifest_dir
        .join("..")
        .join("..")
        .join("audio.cpp")
        .join("model_specs");
    if adjacent.is_dir() {
        return Some(adjacent);
    }

    None
}

pub fn resolve_demo_voices_dir(app: &AppHandle) -> Option<PathBuf> {
    // 1. Check App resources directory
    if let Ok(res_dir) = app.path().resource_dir() {
        let p = res_dir.join("resources").join("demo_voices");
        if p.is_dir() {
            return Some(p);
        }
        let p2 = res_dir.join("demo_voices");
        if p2.is_dir() {
            return Some(p2);
        }
    }

    // 2. Check S2B2S resources/demo_voices relative to manifest
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let staged = manifest_dir.join("resources").join("demo_voices");
    if staged.is_dir() {
        return Some(staged);
    }

    // 3. Check adjacent audio.cpp demo_voices
    let adjacent = manifest_dir
        .join("..")
        .join("..")
        .join("audio.cpp")
        .join("webui")
        .join("native")
        .join("demo_voices");
    if adjacent.is_dir() {
        return Some(adjacent);
    }

    None
}

pub fn resolve_task_for_family(family: &str) -> &'static str {
    match family {
        "chatterbox" => "clon",
        "miocodec" => "vc",
        "ace_step" | "stable_audio" => "gen",
        "fun_asr_nano" | "nemotron_asr" | "qwen3_asr" | "vibevoice_asr" | "voxtral_realtime" => {
            "asr"
        }
        "qwen3_forced_aligner" => "align",
        _ => "tts",
    }
}

fn generate_server_config(app: &AppHandle, port: u16, backend: &str) -> PathBuf {
    let specs_dir = resolve_model_specs_dir(app);
    let demo_voices_dir = resolve_demo_voices_dir(app);
    let settings = crate::settings::get_settings(app);
    let active_quant = settings.tts.audiocpp.quantization.clone();
    let mut model_entries = Vec::new();
    let mut registered_ids = std::collections::HashSet::new();

    if let Ok(catalog) = super::catalog::get_audiocpp_catalog(app) {
        for fam in catalog {
            // Sort packages so that the active quantization package is registered as the family alias
            let mut pkgs = fam.packages.clone();
            pkgs.sort_by(|a, b| {
                let a_is_active = a.id == active_quant;
                let b_is_active = b.id == active_quant;
                b_is_active.cmp(&a_is_active)
            });

            for pkg in &pkgs {
                if let Some(ref local_path) = pkg.local_path {
                    let task = resolve_task_for_family(&fam.family);

                    // Build optional voice presets for cloning models
                    let mut voice_presets_obj = None;
                    if task == "clon" {
                        if let Some(ref v_dir) = demo_voices_dir {
                            let mut presets = serde_json::Map::new();
                            for name in
                                &["demo_1_man", "demo_2_man", "demo_3_woman", "demo_4_woman"]
                            {
                                let wav_path = v_dir.join(format!("{name}.wav"));
                                if wav_path.is_file() {
                                    presets.insert(
                                        name.to_string(),
                                        serde_json::json!({
                                            "voice_ref": wav_path.to_string_lossy().to_string(),
                                            "reference_text": "Hello, how can I help?"
                                        }),
                                    );
                                }
                            }
                            if !presets.is_empty() {
                                if let Some(first_preset) = presets.get("demo_1_man").cloned() {
                                    presets.insert("default".to_string(), first_preset);
                                }
                                voice_presets_obj = Some(serde_json::Value::Object(presets));
                            }
                        }
                    }

                    // Register the family ID (e.g. "supertonic", "qwen3_tts", "chatterbox")
                    if registered_ids.insert(fam.family.clone()) {
                        let mut obj = serde_json::json!({
                            "id": fam.family.clone(),
                            "family": fam.family.clone(),
                            "path": local_path,
                            "task": task,
                            "mode": "offline",
                            "lazy": true
                        });
                        if let Some(ref s) = specs_dir {
                            obj["model_spec_override"] = serde_json::Value::String(
                                s.join(format!("{}.json", fam.family))
                                    .to_string_lossy()
                                    .to_string(),
                            );
                        }
                        if let Some(ref presets) = voice_presets_obj {
                            obj["voice_presets"] = presets.clone();
                            obj["default_voice_preset"] = serde_json::json!("demo_1_man");
                        }
                        model_entries.push(obj);
                    }

                    // Register package-specific ID if distinct
                    if registered_ids.insert(pkg.id.clone()) {
                        let mut pkg_obj = serde_json::json!({
                            "id": pkg.id.clone(),
                            "family": fam.family.clone(),
                            "path": local_path,
                            "task": task,
                            "mode": "offline",
                            "lazy": true
                        });
                        if let Some(ref s) = specs_dir {
                            pkg_obj["model_spec_override"] = serde_json::Value::String(
                                s.join(format!("{}.json", fam.family))
                                    .to_string_lossy()
                                    .to_string(),
                            );
                        }
                        if let Some(ref presets) = voice_presets_obj {
                            pkg_obj["voice_presets"] = presets.clone();
                            pkg_obj["default_voice_preset"] = serde_json::json!("demo_1_man");
                        }
                        model_entries.push(pkg_obj);
                    }
                }
            }
        }
    }

    let config_map = serde_json::json!({
        "host": "127.0.0.1",
        "port": port,
        "backend": backend,
        "lazy_load": true,
        "ui_management": true,
        "models": model_entries
    });

    let config_path = std::env::temp_dir().join(format!("s2b2s_audiocpp_config_{port}.json"));
    let _ = std::fs::write(&config_path, config_map.to_string());
    config_path
}

pub fn ensure_running(app: &AppHandle, backend_preference: &str) -> Result<ServerHandle, String> {
    let client = get_http_client();
    let backend = if backend_preference.trim().is_empty() {
        "cuda"
    } else {
        backend_preference
    };

    let mut state = SLOT.state().lock().unwrap();
    if let ServerState::Ready(server) = &*state {
        if server.backend == backend {
            let url = format!("http://127.0.0.1:{}/health", server.port);
            if let Ok(resp) = client
                .get(&url)
                .timeout(std::time::Duration::from_millis(800))
                .send()
            {
                if resp.status().is_success() {
                    return Ok(ServerHandle {
                        port: server.port,
                        client: client.clone(),
                    });
                }
            }
        }
    }

    let bin_path = resolve_server_binary(app).ok_or_else(|| {
        "audiocpp_server executable not found. Please compile it via scripts/compile-audiocpp.ps1".to_string()
    })?;

    let port = get_free_port()
        .ok_or_else(|| "No free TCP port available for audiocpp_server".to_string())?;
    let generation = SLOT.generation.fetch_add(1, Ordering::SeqCst) + 1;
    *state = ServerState::Starting { generation };
    emit_status("loading", None);

    info!(
        "[AudioCppServer] Spawning audio.cpp server on port {} (backend: {}) — binary: {}",
        port,
        backend,
        bin_path.display()
    );

    let models_dir = resolve_models_dir(app);
    let _ = std::fs::create_dir_all(&models_dir);
    let config_path = generate_server_config(app, port, backend);

    let mut cmd = Command::new(&bin_path);
    cmd.arg("--config")
        .arg(&config_path)
        .arg("--ui")
        .arg("--ui-management")
        .arg("--backend")
        .arg(backend);

    if let Some(specs_dir) = resolve_model_specs_dir(app) {
        if let Some(parent) = specs_dir.parent() {
            cmd.current_dir(parent);
        }
        cmd.arg("--model-spec-override").arg(&specs_dir);
    }

    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let child = cmd.spawn().map_err(|e| {
        let err = format!("Failed to spawn audiocpp_server: {e}");
        error!("[AudioCppServer] {err}");
        emit_status("error", Some(&err));
        err
    })?;

    let active = Arc::new(ActiveServer {
        child: Mutex::new(child),
        port,
        backend: backend.to_string(),
        client: client.clone(),
    });

    let active_clone = active.clone();
    let health_url = format!("http://127.0.0.1:{port}/health");

    // Poll /health
    let mut ready = false;
    let start_time = std::time::Instant::now();
    while start_time.elapsed() < std::time::Duration::from_secs(30) {
        std::thread::sleep(std::time::Duration::from_millis(250));
        if let Ok(resp) = client
            .get(&health_url)
            .timeout(std::time::Duration::from_millis(500))
            .send()
        {
            if resp.status().is_success() {
                ready = true;
                break;
            }
        }
    }

    if !ready {
        let _ = unload();
        let err = "audiocpp_server failed to become healthy within 30 seconds".to_string();
        error!("[AudioCppServer] {err}");
        emit_status("error", Some(&err));
        return Err(err);
    }

    *state = ServerState::Ready(active_clone);
    emit_status("ready", None);
    info!("[AudioCppServer] Server ready on port {}", port);

    Ok(ServerHandle {
        port,
        client: client.clone(),
    })
}

pub fn list_voices(_app: &AppHandle, model_id: &str) -> Vec<crate::tts::Voice> {
    let mut voices = match model_id {
        "chatterbox" => vec![
            crate::tts::Voice {
                id: "demo_1_man".to_string(),
                name: "Demo 1 (Male)".to_string(),
                language: Some("en".to_string()),
            },
            crate::tts::Voice {
                id: "demo_2_man".to_string(),
                name: "Demo 2 (Male)".to_string(),
                language: Some("en".to_string()),
            },
            crate::tts::Voice {
                id: "demo_3_woman".to_string(),
                name: "Demo 3 (Female)".to_string(),
                language: Some("en".to_string()),
            },
            crate::tts::Voice {
                id: "demo_4_woman".to_string(),
                name: "Demo 4 (Female)".to_string(),
                language: Some("en".to_string()),
            },
        ],
        "supertonic" => vec![
            crate::tts::Voice {
                id: "M1".to_string(),
                name: "M1 (Male)".to_string(),
                language: Some("en".to_string()),
            },
            crate::tts::Voice {
                id: "M2".to_string(),
                name: "M2 (Male)".to_string(),
                language: Some("en".to_string()),
            },
            crate::tts::Voice {
                id: "F1".to_string(),
                name: "F1 (Female)".to_string(),
                language: Some("en".to_string()),
            },
            crate::tts::Voice {
                id: "F2".to_string(),
                name: "F2 (Female)".to_string(),
                language: Some("en".to_string()),
            },
        ],
        "qwen3_tts" => vec![
            crate::tts::Voice {
                id: "Vivian".to_string(),
                name: "Vivian (Female)".to_string(),
                language: Some("en".to_string()),
            },
            crate::tts::Voice {
                id: "Serena".to_string(),
                name: "Serena (Female)".to_string(),
                language: Some("en".to_string()),
            },
            crate::tts::Voice {
                id: "Dylan".to_string(),
                name: "Dylan (Male)".to_string(),
                language: Some("en".to_string()),
            },
            crate::tts::Voice {
                id: "Eric".to_string(),
                name: "Eric (Male)".to_string(),
                language: Some("en".to_string()),
            },
            crate::tts::Voice {
                id: "Ryan".to_string(),
                name: "Ryan (Male)".to_string(),
                language: Some("en".to_string()),
            },
            crate::tts::Voice {
                id: "Aiden".to_string(),
                name: "Aiden (Male)".to_string(),
                language: Some("en".to_string()),
            },
            crate::tts::Voice {
                id: "Uncle_Fu".to_string(),
                name: "Uncle Fu (Male)".to_string(),
                language: Some("zh".to_string()),
            },
            crate::tts::Voice {
                id: "Ono_Anna".to_string(),
                name: "Ono Anna (Female)".to_string(),
                language: Some("ja".to_string()),
            },
            crate::tts::Voice {
                id: "Sohee".to_string(),
                name: "Sohee (Female)".to_string(),
                language: Some("ko".to_string()),
            },
        ],
        _ => vec![
            crate::tts::Voice {
                id: "default".to_string(),
                name: "Default".to_string(),
                language: Some("en".to_string()),
            },
            crate::tts::Voice {
                id: "alba".to_string(),
                name: "Alba".to_string(),
                language: Some("en".to_string()),
            },
            crate::tts::Voice {
                id: "cosette".to_string(),
                name: "Cosette".to_string(),
                language: Some("en".to_string()),
            },
            crate::tts::Voice {
                id: "marius".to_string(),
                name: "Marius".to_string(),
                language: Some("en".to_string()),
            },
        ],
    };

    if let Some(port) = get_ready_port() {
        let client = get_http_client();
        let url = format!("http://127.0.0.1:{port}/v1/audio/voices?model={model_id}");
        if let Ok(resp) = client
            .get(&url)
            .timeout(std::time::Duration::from_secs(2))
            .send()
        {
            if resp.status().is_success() {
                if let Ok(json) = resp.json::<serde_json::Value>() {
                    if let Some(list) = json.get("voices").and_then(|v| v.as_array()) {
                        let live: Vec<crate::tts::Voice> = list
                            .iter()
                            .filter_map(|v| {
                                v.as_str().map(|s| crate::tts::Voice {
                                    id: s.to_string(),
                                    name: s.to_string(),
                                    language: Some("en".to_string()),
                                })
                            })
                            .collect();
                        if !live.is_empty() {
                            voices = live;
                        }
                    }
                }
            }
        }
    }

    voices
}

pub struct AudioCppServerManager {
    app: AppHandle,
}

impl AudioCppServerManager {
    pub fn new(app: AppHandle) -> Self {
        set_app_handle(app.clone());
        Self { app }
    }

    pub fn ensure_running(&self, backend: &str) -> Result<ServerHandle, String> {
        ensure_running(&self.app, backend)
    }

    pub fn unload(&self) -> bool {
        unload()
    }
}
