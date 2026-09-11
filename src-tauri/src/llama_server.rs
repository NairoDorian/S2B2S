//! In-app llama.cpp server supervisor: the "brain" behind post-processing and
//! the Multi-STT merge.
//!
//! Replaces the hand-run `launch_server_E2B_Q4.ps1`: the same `llama-server`
//! command line is built from `LlamaSettings`, the child is spawned detached
//! (no console, tied to Handy through a Windows job object), its output is
//! kept in a ring buffer, readiness is polled on `/health`, and state changes
//! reach the UI as `LlamaServerStateEvent`. Requests to the local endpoint
//! call [`ensure_ready_for_provider`] first, so a cold start happens once and
//! the request path only ever talks to a warm server.
//!
//! Performance notes (see docs/PERFORMANCE.md): the supervisor is a plain
//! thread that owns the child; readers are two more threads that block on the
//! pipes; the request path reads one mutex-protected snapshot and never
//! probes the process. Health polls stop once the server is ready — after
//! that a 2 s `try_wait` is the only cost.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;
use tauri_specta::Event;

use crate::settings::{LlamaSettings, PostProcessProvider, get_settings, write_settings};

const LOG_CAPACITY: usize = 400;
/// Model load of a few GB from a cold disk can take a while; give up after this.
const START_TIMEOUT: Duration = Duration::from_secs(180);
const HEALTH_POLL: Duration = Duration::from_millis(300);
const ALIVE_POLL: Duration = Duration::from_secs(2);

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum LlamaStatus {
    #[default]
    Stopped,
    Starting,
    Ready,
    Error,
}

/// Snapshot of the supervised server. Also the payload of the state event.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type, tauri_specta::Event)]
pub struct LlamaServerStateEvent {
    pub status: LlamaStatus,
    /// Human-readable reason for `Error`, or the last log line while starting.
    pub message: Option<String>,
    pub pid: Option<u32>,
    pub port: u16,
    pub alias: String,
    /// File name of the loaded model.
    pub model: Option<String>,
    pub draft: bool,
    pub mmproj: bool,
    /// Unix milliseconds when the server became ready.
    pub ready_since_ms: Option<f64>,
    /// Backend guessed from the server folder name (cuda / vulkan / cpu).
    pub backend: Option<String>,
}

impl LlamaServerStateEvent {
    fn stopped(settings: &LlamaSettings) -> Self {
        Self {
            status: LlamaStatus::Stopped,
            message: None,
            pid: None,
            port: settings.port,
            alias: settings.alias.clone(),
            model: settings.model_path.as_deref().and_then(file_name),
            draft: settings.draft_model_path.is_some(),
            mmproj: settings.mmproj_enabled && settings.mmproj_path.is_some(),
            ready_since_ms: None,
            backend: settings.server_dir.as_deref().map(backend_from_dir),
        }
    }
}

fn file_name(path: &str) -> Option<String> {
    Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
}

fn backend_from_dir(dir: &str) -> String {
    let lower = dir.to_lowercase();
    if lower.contains("cuda") {
        "cuda".into()
    } else if lower.contains("vulkan") {
        "vulkan".into()
    } else if lower.contains("cpu") {
        "cpu".into()
    } else {
        "unknown".into()
    }
}

/// A GGUF file found in a folder, for the model / draft / mmproj pickers.
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct GgufFile {
    pub path: String,
    pub name: String,
    pub size_mb: u32,
    /// `model`, `draft` (MTP / speculative) or `mmproj`, guessed from the name.
    pub kind: String,
}

/// An existing llama.cpp install found outside Handy's data dir.
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct LlamaDetectedInstall {
    pub server_dir: String,
    pub model_path: Option<String>,
    pub draft_model_path: Option<String>,
    pub mmproj_path: Option<String>,
}

pub struct LlamaServerManager {
    app: AppHandle,
    state: Mutex<LlamaServerStateEvent>,
    child: Mutex<Option<Child>>,
    logs: Mutex<VecDeque<String>>,
    /// Bumped on every start/stop so a stale supervisor thread stands down.
    generation: AtomicU64,
    /// Serializes start/stop so two on-demand callers cannot spawn twice.
    lifecycle: Mutex<()>,
}

static GLOBAL: OnceLock<Arc<LlamaServerManager>> = OnceLock::new();

/// The process-wide supervisor, once `init` has run.
pub fn global() -> Option<Arc<LlamaServerManager>> {
    GLOBAL.get().cloned()
}

pub fn init(app: &AppHandle) -> Arc<LlamaServerManager> {
    let settings = get_settings(app).llama;
    let manager = Arc::new(LlamaServerManager {
        app: app.clone(),
        state: Mutex::new(LlamaServerStateEvent::stopped(&settings)),
        child: Mutex::new(None),
        logs: Mutex::new(VecDeque::with_capacity(LOG_CAPACITY)),
        generation: AtomicU64::new(0),
        lifecycle: Mutex::new(()),
    });
    let _ = GLOBAL.set(Arc::clone(&manager));
    manager
}

impl LlamaServerManager {
    pub fn snapshot(&self) -> LlamaServerStateEvent {
        self.state.lock().unwrap().clone()
    }

    pub fn logs(&self) -> Vec<String> {
        self.logs.lock().unwrap().iter().cloned().collect()
    }

    fn set_state(&self, update: impl FnOnce(&mut LlamaServerStateEvent)) {
        let snapshot = {
            let mut state = self.state.lock().unwrap();
            update(&mut state);
            state.clone()
        };
        let _ = snapshot.emit(&self.app);
    }

    fn push_log(&self, line: String) {
        let mut logs = self.logs.lock().unwrap();
        if logs.len() == LOG_CAPACITY {
            logs.pop_front();
        }
        logs.push_back(line);
    }

    /// Start the server with the persisted settings. Idempotent: a server
    /// that is starting or ready is left alone.
    pub fn start(&self) -> Result<(), String> {
        let _guard = self.lifecycle.lock().unwrap();
        let current = self.snapshot();
        if matches!(current.status, LlamaStatus::Starting | LlamaStatus::Ready)
            && self.child_alive()
        {
            return Ok(());
        }
        self.kill_child();

        let settings = get_settings(&self.app).llama.normalized();

        // A server someone else launched (the old PowerShell script, another
        // app) already answers on the port: use it rather than fail to bind.
        if health_ok(settings.port) {
            let stopped = LlamaServerStateEvent::stopped(&settings);
            self.set_state(|s| {
                *s = stopped;
                s.status = LlamaStatus::Ready;
                s.message = Some(format!(
                    "Using a llama-server already listening on port {} (not started by Handy)",
                    settings.port
                ));
            });
            info!(
                "llama-server already listening on port {}; adopting it",
                settings.port
            );
            return Ok(());
        }

        let exe = resolve_server_exe(&settings)
            .ok_or_else(|| "No llama-server executable configured. Install a release or pick the folder that contains llama-server.".to_string())?;
        let args = build_args(&settings)?;

        let mut cmd = Command::new(&exe);
        cmd.args(&args)
            .current_dir(exe.parent().unwrap_or(Path::new(".")))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if settings.attn_rot_disable {
            cmd.env("LLAMA_ATTN_ROT_DISABLE", "1");
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        self.logs.lock().unwrap().clear();
        self.push_log(format!(
            "$ {} {}",
            exe.display(),
            args.iter()
                .map(|a| if a.contains(' ') {
                    format!("\"{a}\"")
                } else {
                    a.clone()
                })
                .collect::<Vec<_>>()
                .join(" ")
        ));

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to spawn {}: {e}", exe.display()))?;
        crate::job_object::register(&child);
        let pid = child.id();
        let generation = self.generation.fetch_add(1, Ordering::AcqRel) + 1;

        // Pipe readers: one thread per pipe, blocking on the line reader.
        for (name, reader) in [
            (
                "out",
                child
                    .stdout
                    .take()
                    .map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
            ),
            (
                "err",
                child
                    .stderr
                    .take()
                    .map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
            ),
        ] {
            let Some(reader) = reader else { continue };
            let manager = global().expect("manager registered");
            std::thread::Builder::new()
                .name(format!("llama-{name}"))
                .spawn(move || {
                    for line in BufReader::new(reader).lines().map_while(Result::ok) {
                        if !line.trim().is_empty() {
                            manager.push_log(line);
                        }
                    }
                })
                .map_err(|e| e.to_string())?;
        }

        *self.child.lock().unwrap() = Some(child);
        let stopped = LlamaServerStateEvent::stopped(&settings);
        self.set_state(|s| {
            *s = stopped;
            s.status = LlamaStatus::Starting;
            s.pid = Some(pid);
            s.message = Some("Loading model…".into());
        });
        info!("llama-server started (pid {pid}) on port {}", settings.port);

        let manager = global().expect("manager registered");
        let port = settings.port;
        std::thread::Builder::new()
            .name("llama-supervisor".into())
            .spawn(move || manager.supervise(generation, port))
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Owns the child's lifetime: polls readiness, then watches for exit.
    fn supervise(&self, generation: u64, port: u16) {
        let is_current = || self.generation.load(Ordering::Acquire) == generation;
        let started = Instant::now();

        // Phase 1: wait for /health.
        loop {
            if !is_current() {
                return;
            }
            if let Some(code) = self.child_exit_code() {
                let tail = self.log_tail(6);
                self.set_state(|s| {
                    s.status = LlamaStatus::Error;
                    s.pid = None;
                    s.message = Some(format!(
                        "llama-server exited with {code} before becoming ready:\n{tail}"
                    ));
                });
                error!("llama-server exited early ({code})");
                return;
            }
            if started.elapsed() > START_TIMEOUT {
                self.kill_child();
                self.set_state(|s| {
                    s.status = LlamaStatus::Error;
                    s.pid = None;
                    s.message = Some(format!(
                        "llama-server did not answer /health within {}s",
                        START_TIMEOUT.as_secs()
                    ));
                });
                return;
            }
            let announced = self.server_announced_listening(port);
            if health_ok(port) || announced {
                if announced {
                    info!("llama-server announced it is listening; treating it as ready");
                }
                let now_ms = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_millis() as f64)
                    .unwrap_or(0.0);
                self.set_state(|s| {
                    s.status = LlamaStatus::Ready;
                    s.message = None;
                    s.ready_since_ms = Some(now_ms);
                });
                info!(
                    "llama-server ready on port {port} after {:.1}s",
                    started.elapsed().as_secs_f64()
                );
                break;
            }
            let last = self.log_tail(1);
            if !last.is_empty() {
                self.set_state(|s| s.message = Some(last));
            }
            std::thread::sleep(HEALTH_POLL);
        }

        // Phase 2: cheap liveness watch.
        loop {
            std::thread::sleep(ALIVE_POLL);
            if !is_current() {
                return;
            }
            if let Some(code) = self.child_exit_code() {
                let tail = self.log_tail(6);
                self.set_state(|s| {
                    s.status = LlamaStatus::Error;
                    s.pid = None;
                    s.ready_since_ms = None;
                    s.message = Some(format!("llama-server exited with {code}:\n{tail}"));
                });
                warn!("llama-server exited ({code})");
                return;
            }
        }
    }

    /// Whether the captured output already contains llama-server's own
    /// "listening on http://127.0.0.1:<port>" line — a readiness signal that
    /// does not depend on the probe.
    fn server_announced_listening(&self, port: u16) -> bool {
        let needle = format!("listening on http://127.0.0.1:{port}");
        self.logs
            .lock()
            .unwrap()
            .iter()
            .any(|l| l.contains(&needle))
    }

    fn log_tail(&self, n: usize) -> String {
        let logs = self.logs.lock().unwrap();
        logs.iter()
            .rev()
            .take(n)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn child_alive(&self) -> bool {
        let mut child = self.child.lock().unwrap();
        match child.as_mut() {
            Some(c) => matches!(c.try_wait(), Ok(None)),
            None => false,
        }
    }

    fn child_exit_code(&self) -> Option<String> {
        let mut child = self.child.lock().unwrap();
        match child.as_mut()?.try_wait() {
            Ok(Some(status)) => Some(status.to_string()),
            Ok(None) => None,
            Err(e) => Some(format!("unknown ({e})")),
        }
    }

    fn kill_child(&self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        if let Some(mut child) = self.child.lock().unwrap().take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    pub fn stop(&self) {
        let _guard = self.lifecycle.lock().unwrap();
        let was_running = self.child_alive();
        self.kill_child();
        let settings = get_settings(&self.app).llama;
        self.set_state(|s| *s = LlamaServerStateEvent::stopped(&settings));
        if was_running {
            info!("llama-server stopped");
        }
    }

    pub fn restart(&self) -> Result<(), String> {
        self.stop();
        self.start()
    }

    /// Make sure the server answers before a request goes out. Starts it when
    /// needed and waits, bounded by `timeout`.
    pub fn ensure_ready(&self, timeout: Duration) -> Result<(), String> {
        match self.snapshot().status {
            LlamaStatus::Ready if self.child_alive() => return Ok(()),
            // Adopted external server: trust the port, not a child handle.
            LlamaStatus::Ready if health_ok(self.snapshot().port) => return Ok(()),
            LlamaStatus::Starting => {}
            _ => self.start()?,
        }
        let deadline = Instant::now() + timeout;
        loop {
            let state = self.snapshot();
            match state.status {
                LlamaStatus::Ready => return Ok(()),
                LlamaStatus::Error | LlamaStatus::Stopped => {
                    return Err(state
                        .message
                        .unwrap_or_else(|| "llama-server is not running".into()));
                }
                LlamaStatus::Starting => {}
            }
            if Instant::now() > deadline {
                return Err("Timed out waiting for llama-server to become ready".into());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// Point post-processing (and therefore the Multi-STT merge) at this
    /// server: the `custom` provider gets the local base URL and the alias as
    /// its model, and becomes the active provider.
    pub fn apply_to_post_processing(&self) -> Result<(), String> {
        let mut settings = get_settings(&self.app);
        let base_url = format!("http://127.0.0.1:{}/v1", settings.llama.port);
        let alias = settings.llama.alias.clone();
        let Some(provider) = settings
            .post_process_providers
            .iter_mut()
            .find(|p| p.id == "custom")
        else {
            return Err("No custom provider in settings".into());
        };
        provider.base_url = base_url;
        settings
            .post_process_models
            .insert("custom".to_string(), alias);
        settings.post_process_provider_id = "custom".to_string();
        write_settings(&self.app, settings);
        Ok(())
    }
}

/// Before any request to a provider: if it targets the supervised server and
/// on-demand start is on, bring the server up first. Errors are logged, not
/// returned — the request then fails with the provider's own error, which the
/// caller already handles.
pub async fn ensure_ready_for_provider(provider: &PostProcessProvider) {
    let Some(manager) = global() else { return };
    let settings = get_settings(&manager.app).llama;
    if !settings.start_on_demand || !targets_local_server(&provider.base_url, settings.port) {
        return;
    }
    if manager.snapshot().status == LlamaStatus::Ready
        && (manager.child_alive() || health_ok(settings.port))
    {
        return;
    }
    let m = Arc::clone(&manager);
    let result = tauri::async_runtime::spawn_blocking(move || m.ensure_ready(START_TIMEOUT))
        .await
        .unwrap_or_else(|e| Err(e.to_string()));
    if let Err(e) = result {
        warn!("Local llama-server not ready for request: {e}");
    }
}

/// Blocking `GET /health` over a raw socket. The supervisor is a plain thread
/// with no async runtime, and a 60-byte HTTP exchange does not need one.
fn health_ok(port: u16) -> bool {
    use std::io::{Read, Write};
    use std::net::{SocketAddr, TcpStream};
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    // Windows reports a refused loopback connect as a timeout after ~1 s, so
    // a short budget here would misread "not yet" as "down" during startup.
    let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_millis(1000)) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_millis(600)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(300)));
    if stream
        .write_all(b"GET /health HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
        .is_err()
    {
        return false;
    }
    let mut head = [0u8; 64];
    let mut read = 0;
    while read < head.len() {
        match stream.read(&mut head[read..]) {
            Ok(0) => break,
            Ok(n) => {
                read += n;
                if read >= 12 {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    // "HTTP/1.1 200 ..." — llama-server answers 503 while the model loads.
    let head = String::from_utf8_lossy(&head[..read]);
    head.starts_with("HTTP/1.1 200") || head.starts_with("HTTP/1.0 200")
}

fn targets_local_server(base_url: &str, port: u16) -> bool {
    let lower = base_url.to_lowercase();
    let is_local = lower.contains("127.0.0.1") || lower.contains("localhost");
    is_local && lower.contains(&format!(":{port}"))
}

/// The `llama-server` binary for the configured folder, if present.
pub fn resolve_server_exe(settings: &LlamaSettings) -> Option<PathBuf> {
    let dir = PathBuf::from(settings.server_dir.as_deref()?);
    let name = if cfg!(windows) {
        "llama-server.exe"
    } else {
        "llama-server"
    };
    let direct = dir.join(name);
    if direct.is_file() {
        return Some(direct);
    }
    // Releases sometimes unpack into a nested folder.
    std::fs::read_dir(&dir).ok()?.flatten().find_map(|entry| {
        let candidate = entry.path().join(name);
        candidate.is_file().then_some(candidate)
    })
}

/// The command line, exactly as `launch_server_E2B_Q4.ps1` runs it, from the
/// settings. `custom_args` replaces everything after the executable.
pub fn build_args(settings: &LlamaSettings) -> Result<Vec<String>, String> {
    if let Some(custom) = settings
        .custom_args
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Ok(split_args(custom));
    }
    let model = settings
        .model_path
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .ok_or_else(|| "No model file selected".to_string())?;
    if !Path::new(model).is_file() {
        return Err(format!("Model file not found: {model}"));
    }

    let mut args: Vec<String> = vec![
        "-m".into(),
        model.into(),
        "--port".into(),
        settings.port.to_string(),
        "-c".into(),
        settings.context_size.to_string(),
        "--parallel".into(),
        "1".into(),
        "--flash-attn".into(),
        if settings.flash_attn { "on" } else { "off" }.into(),
        "--no-context-shift".into(),
        "-ngl".into(),
        settings.gpu_layers.to_string(),
        "--threads".into(),
        settings.threads.to_string(),
        "--jinja".into(),
        "--temp".into(),
        trim_float(settings.temperature),
        "--top-p".into(),
        trim_float(settings.top_p),
        "--top-k".into(),
        settings.top_k.to_string(),
        "--min-p".into(),
        trim_float(settings.min_p),
        "--reasoning".into(),
        if settings.reasoning { "on" } else { "off" }.into(),
    ];
    if let Some(draft) = settings
        .draft_model_path
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        if !Path::new(draft).is_file() {
            return Err(format!("Draft (MTP) model file not found: {draft}"));
        }
        args.extend([
            "--model-draft".into(),
            draft.into(),
            "--spec-type".into(),
            "draft-mtp".into(),
            "--spec-draft-n-max".into(),
            settings.spec_draft_n_max.to_string(),
        ]);
    }
    if settings.mmproj_enabled {
        if let Some(mmproj) = settings
            .mmproj_path
            .as_deref()
            .map(str::trim)
            .filter(|p| !p.is_empty())
        {
            if !Path::new(mmproj).is_file() {
                return Err(format!("mmproj file not found: {mmproj}"));
            }
            args.extend(["--mmproj".into(), mmproj.into()]);
        }
    }
    args.extend(["--alias".into(), settings.alias.clone(), "--metrics".into()]);
    args.extend(split_args(&settings.extra_args));
    Ok(args)
}

fn trim_float(v: f32) -> String {
    let s = format!("{v:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() {
        "0".into()
    } else {
        s.to_string()
    }
}

/// Split a free-form argument string, honouring double quotes.
pub fn split_args(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    for c in input.chars() {
        match c {
            '"' => in_quotes = !in_quotes,
            c if c.is_whitespace() && !in_quotes => {
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// GGUF files in `dir` (one level), sorted by name, with a kind guess.
pub fn list_gguf_files(dir: &str) -> Vec<GgufFile> {
    let mut files: Vec<GgufFile> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let name = path.file_name()?.to_string_lossy().to_string();
            if !name.to_lowercase().ends_with(".gguf") {
                return None;
            }
            let size_mb = entry
                .metadata()
                .ok()
                .map(|m| (m.len() / 1_048_576) as u32)?;
            Some(GgufFile {
                kind: gguf_kind(&name).into(),
                path: path.to_string_lossy().to_string(),
                name,
                size_mb,
            })
        })
        .collect();
    files.sort_by(|a, b| a.name.cmp(&b.name));
    files
}

fn gguf_kind(name: &str) -> &'static str {
    let lower = name.to_lowercase();
    if lower.starts_with("mmproj") || lower.contains("mmproj") {
        "mmproj"
    } else if lower.starts_with("mtp-") || lower.contains("-draft") || lower.contains("mtp") {
        "draft"
    } else {
        "model"
    }
}

/// Look for a llama.cpp folder the user already has (the layout of the
/// maintainer's download script: `<root>/llama/llama-server.exe` with models
/// under `<root>/model/**`), so first use needs no configuration.
pub fn detect_existing_install() -> Option<LlamaDetectedInstall> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join("Downloads").join("PROJECTS").join("Llama.cpp"));
        roots.push(home.join("llama.cpp"));
        roots.push(home.join("Llama.cpp"));
    }
    if let Some(data) = dirs::data_local_dir() {
        roots.push(data.join("llama.cpp"));
    }
    for root in roots {
        let server_dir = root.join("llama");
        let probe = LlamaSettings {
            server_dir: Some(server_dir.to_string_lossy().to_string()),
            ..LlamaSettings::default()
        };
        if resolve_server_exe(&probe).is_none() {
            continue;
        }
        let (model, draft, mmproj) = pick_default_models(&root.join("model"));
        return Some(LlamaDetectedInstall {
            server_dir: server_dir.to_string_lossy().to_string(),
            model_path: model,
            draft_model_path: draft,
            mmproj_path: mmproj,
        });
    }
    None
}

/// Prefer a Q4 main model, its Q4_0 MTP draft and the F16 mmproj — the
/// combination the launch script uses — from the first model folder found.
fn pick_default_models(model_root: &Path) -> (Option<String>, Option<String>, Option<String>) {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(model_root)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    // The E2B folder (smaller, faster) sorts before E4B; also consider the
    // root itself for flat layouts.
    dirs.insert(0, model_root.to_path_buf());
    for dir in dirs {
        let files = list_gguf_files(&dir.to_string_lossy());
        if files.is_empty() {
            continue;
        }
        let pick = |kind: &str, prefer: &[&str]| -> Option<String> {
            let candidates: Vec<&GgufFile> = files.iter().filter(|f| f.kind == kind).collect();
            for pref in prefer {
                if let Some(f) = candidates.iter().find(|f| f.name.contains(pref)) {
                    return Some(f.path.clone());
                }
            }
            candidates.first().map(|f| f.path.clone())
        };
        let model = pick("model", &["Q4_K_XL", "Q4", "Q5", "Q8"]);
        if model.is_none() {
            continue;
        }
        let draft = pick("draft", &["Q4_0", "Q8_0"]);
        let mmproj = pick("mmproj", &["F16", "BF16"]);
        return (model, draft, mmproj);
    }
    (None, None, None)
}

/// Adopt a detected install into the settings when nothing is configured yet.
pub fn adopt_detected_install_if_unconfigured(app: &AppHandle) {
    let settings = get_settings(app);
    if settings.llama.server_dir.is_some() || settings.llama.model_path.is_some() {
        return;
    }
    let Some(found) = detect_existing_install() else {
        return;
    };
    info!(
        "Adopting existing llama.cpp install at {} (model: {:?})",
        found.server_dir, found.model_path
    );
    let mut settings = settings;
    settings.llama.server_dir = Some(found.server_dir);
    settings.llama.model_path = found.model_path;
    settings.llama.draft_model_path = found.draft_model_path;
    settings.llama.mmproj_path = found.mmproj_path;
    write_settings(app, settings);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings_with(model: &Path) -> LlamaSettings {
        LlamaSettings {
            model_path: Some(model.to_string_lossy().to_string()),
            ..LlamaSettings::default()
        }
    }

    #[test]
    fn default_args_match_the_launch_script() {
        let dir = tempfile::tempdir().unwrap();
        let model = dir.path().join("gemma-4-E2B-it-qat-UD-Q4_K_XL.gguf");
        let draft = dir.path().join("mtp-gemma-4-E2B-it-Q4_0.gguf");
        std::fs::write(&model, b"x").unwrap();
        std::fs::write(&draft, b"x").unwrap();
        let mut s = settings_with(&model);
        s.draft_model_path = Some(draft.to_string_lossy().to_string());
        let args = build_args(&s).unwrap();
        let joined = args.join(" ");
        assert!(joined.starts_with(&format!("-m {} --port 62966 -c 8192 --parallel 1 --flash-attn on --no-context-shift -ngl -1 --threads -1 --jinja --temp 0.05 --top-p 0.35 --top-k 64 --min-p 0 --reasoning off --model-draft ", model.to_string_lossy())), "{joined}");
        assert!(
            joined.ends_with(
                "--spec-type draft-mtp --spec-draft-n-max 4 --alias gemma-4-E2B-Q4-MTP --metrics"
            ),
            "{joined}"
        );
    }

    #[test]
    fn missing_model_and_custom_args() {
        let mut s = LlamaSettings::default();
        assert!(build_args(&s).is_err());
        s.custom_args = Some(r#"-m "C:\a b\m.gguf" --port 1"#.into());
        assert_eq!(
            build_args(&s).unwrap(),
            vec!["-m", "C:\\a b\\m.gguf", "--port", "1"]
        );
    }

    #[test]
    fn gguf_kinds_and_local_target() {
        assert_eq!(gguf_kind("mtp-gemma-4-E2B-it-Q4_0.gguf"), "draft");
        assert_eq!(gguf_kind("mmproj-F16.gguf"), "mmproj");
        assert_eq!(gguf_kind("gemma-4-E2B-it-qat-UD-Q4_K_XL.gguf"), "model");
        assert!(targets_local_server("http://127.0.0.1:62966/v1", 62966));
        assert!(targets_local_server("http://localhost:62966/v1/", 62966));
        assert!(!targets_local_server("http://localhost:11434/v1", 62966));
        assert!(!targets_local_server("https://api.openai.com/v1", 62966));
    }
}
