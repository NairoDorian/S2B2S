use crate::portable;
use anyhow::{Context, Result};
use log::info;
use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const SCRIPT_NAME: &str = "unified_parakeet_server.py";
const HEALTH_TIMEOUT_SECS: u64 = 60;
const REQUEST_TIMEOUT_SECS: u64 = 120;

pub struct UnifiedParakeetServer {
    process: Arc<Mutex<Option<Child>>>,
    port: u16,
    client: ureq::Agent,
    shutdown: Arc<AtomicBool>,
}

impl UnifiedParakeetServer {
    pub fn launch(model_dir: &str) -> Result<Self> {
        Self::launch_with_script(model_dir, SCRIPT_NAME)
    }

    fn launch_with_script(model_dir: &str, _script_name: &str) -> Result<Self> {
    let python = resolve_venv_python()?;
        let script = resolve_server_script()?;
        let port = get_free_port()?;
        let shutdown = Arc::new(AtomicBool::new(false));

        let mut cmd = Command::new(&python);
        cmd.args([
            &script.to_string_lossy(),
            "--port",
            &port.to_string(),
            "--host",
            "127.0.0.1",
            "--model-dir",
            model_dir,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("PYTHONIOENCODING", "utf-8");

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }

        info!(
            "[unified_parakeet] Spawning server: {} --port {} --model-dir {}",
            python, port, model_dir
        );

        let mut child = cmd
            .spawn()
            .context("Failed to spawn unified parakeet server")?;

        // Drain stdout/stderr in background threads
        if let Some(stdout) = child.stdout.take() {
            let shutdown_stdout = shutdown.clone();
            std::thread::spawn(move || {
                let reader = BufReader::new(stdout);
                for line in reader.lines() {
                    if shutdown_stdout.load(Ordering::Relaxed) {
                        break;
                    }
                    if let Ok(l) = line {
                        info!("[unified_parakeet stdout] {}", l);
                    }
                }
            });
        }
        if let Some(stderr) = child.stderr.take() {
            let shutdown_stderr = shutdown.clone();
            std::thread::spawn(move || {
                let reader = BufReader::new(stderr);
                for line in reader.lines() {
                    if shutdown_stderr.load(Ordering::Relaxed) {
                        break;
                    }
                    if let Ok(l) = line {
                        info!("[unified_parakeet] {}", l);
                    }
                }
            });
        }

        // Health check with exponential backoff — uses ureq (no tokio dependency)
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build();

        let health_url = format!("http://127.0.0.1:{}/health", port);
        let start = Instant::now();
        let mut backoff_ms = 100u64;

        loop {
            if shutdown.load(Ordering::Relaxed) {
                let _ = child.kill();
                anyhow::bail!("Server startup cancelled");
            }

            match child.try_wait() {
                Ok(Some(status)) => {
                    anyhow::bail!("Server process exited early with status: {:?}", status);
                }
                Ok(None) => {}
                Err(e) => {
                    let _ = child.kill();
                    anyhow::bail!("Failed to check server process: {}", e);
                }
            }

            match agent.get(&health_url).call() {
                Ok(_resp) => {
                    info!(
                        "[unified_parakeet] Server healthy at {} ({}ms)",
                        health_url,
                        start.elapsed().as_millis()
                    );
                    break;
                }
                Err(_) => {
                    if start.elapsed().as_secs() > HEALTH_TIMEOUT_SECS {
                        let _ = child.kill();
                        anyhow::bail!(
                            "Unified parakeet server health check timeout after {}s",
                            HEALTH_TIMEOUT_SECS
                        );
                    }
                    std::thread::sleep(Duration::from_millis(backoff_ms));
                    backoff_ms = (backoff_ms * 2).min(1600);
                }
            }
        }

        Ok(Self {
            process: Arc::new(Mutex::new(Some(child))),
            port,
            client: agent,
            shutdown,
        })
    }

    pub fn transcribe(&self, audio: &[f32]) -> Result<String> {
        let audio_bytes: Vec<u8> = audio.iter().flat_map(|s| s.to_le_bytes()).collect();
        let url = format!("http://127.0.0.1:{}/transcribe", self.port);

        let resp = self
            .client
            .post(&url)
            .send_bytes(&audio_bytes)
            .context("Failed to send audio to unified parakeet server")?;

        let json: serde_json::Value =
            serde_json::from_reader(resp.into_reader())
                .context("Failed to parse unified parakeet server response")?;

        Ok(json["text"].as_str().unwrap_or("").to_string())
    }

    pub fn stream_start(&self) -> Result<()> {
        let url = format!("http://127.0.0.1:{}/stream_start", self.port);
        self.client
            .post(&url)
            .send_bytes(&[])
            .context("Failed to start stream on unified parakeet server")?;
        Ok(())
    }

    pub fn stream_feed(&self, audio: &[f32]) -> Result<(String, bool)> {
        let audio_bytes: Vec<u8> = audio.iter().flat_map(|s| s.to_le_bytes()).collect();
        let url = format!("http://127.0.0.1:{}/stream_feed", self.port);

        let resp = self
            .client
            .post(&url)
            .send_bytes(&audio_bytes)
            .context("Failed to feed audio to streaming decoder")?;

        let json: serde_json::Value = serde_json::from_reader(resp.into_reader())?;
        let text = json["text"].as_str().unwrap_or("").to_string();
        let eou = json["eou"].as_bool().unwrap_or(false);
        Ok((text, eou))
    }

    pub fn stream_end(&self, audio: &[f32]) -> Result<(String, bool)> {
        let audio_bytes: Vec<u8> = audio.iter().flat_map(|s| s.to_le_bytes()).collect();
        let url = format!("http://127.0.0.1:{}/stream_end", self.port);

        let resp = self
            .client
            .post(&url)
            .send_bytes(&audio_bytes)
            .context("Failed to finalise stream on unified parakeet server")?;

        let json: serde_json::Value = serde_json::from_reader(resp.into_reader())?;
        let text = json["text"].as_str().unwrap_or("").to_string();
        let eou = json["eou"].as_bool().unwrap_or(false);
        Ok((text, eou))
    }

    pub fn port(&self) -> u16 {
        self.port
    }
}

impl Drop for UnifiedParakeetServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Ok(mut guard) = self.process.lock() {
            if let Some(ref mut child) = *guard {
                info!("[unified_parakeet] Killing server on port {}", self.port);
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}

fn resolve_venv_python() -> Result<String> {
    let exe_name = if cfg!(windows) { "python.exe" } else { "python3" };
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    let project_venv = manifest_dir.parent().unwrap().join("venv");
    let venv_python = if cfg!(windows) {
        project_venv.join("Scripts").join(exe_name)
    } else {
        project_venv.join("bin").join(exe_name)
    };
    if venv_python.exists() {
        return Ok(venv_python.to_string_lossy().to_string());
    }

    if let Some(data_dir) = portable::data_dir() {
        let app_venv = if cfg!(windows) {
            data_dir.join("venv").join("Scripts").join(exe_name)
        } else {
            data_dir.join("venv").join("bin").join(exe_name)
        };
        if app_venv.exists() {
            return Ok(app_venv.to_string_lossy().to_string());
        }
    }

    let sys_python = if cfg!(windows) { "python" } else { "python3" };
    Ok(sys_python.to_string())
}

fn resolve_server_script() -> Result<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    let dev_path = manifest_dir.join(SCRIPT_NAME);
    if dev_path.exists() {
        return Ok(dev_path);
    }

    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_default();
    let bundled_path = exe_dir.join(SCRIPT_NAME);
    if dev_path.exists() {
        return Ok(dev_path);
    }

    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_default();
    let bundled_path = exe_dir.join(SCRIPT_NAME);
    if bundled_path.exists() {
        return Ok(bundled_path);
    }

    if let Some(data_dir) = portable::data_dir() {
        let portable_path = data_dir.join(SCRIPT_NAME);
        if portable_path.exists() {
            return Ok(portable_path);
        }
    }

    anyhow::bail!("{} not found", SCRIPT_NAME)
}

fn get_free_port() -> Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    Ok(listener.local_addr()?.port())
}
