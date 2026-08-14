use futures_util::StreamExt;
use log::{error, info};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager};

#[derive(serde::Serialize, Clone)]
struct DownloadProgressPayload {
    status: String,
    file: String,
    percentage: f64,
    speed_mbps: f64,
    error: Option<String>,
}

/// Public snapshot of the local llama.cpp server state.
/// Emitted on the typed `llama-server-status` event whenever the server
/// starts, finishes loading, gets restarted (e.g. mmproj upgrade) or stops.
#[derive(Serialize, Deserialize, Clone, Debug, Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct LlamaServerStatus {
    /// Whether the llama.cpp server is currently running and responding.
    pub running: bool,
    /// "stopped" | "loading" | "ready"
    pub state: String,
    /// Display name of the loaded model (llama.cpp alias).
    pub model: Option<String>,
    /// Whether the multimodal projector (mmproj) was loaded.
    pub mmproj_loaded: bool,
    /// Compute backend of the running server: "cuda" | "vulkan" | "cpu".
    pub backend: String,
    /// Port the server is listening on.
    pub port: Option<u16>,
}

/// Internal bookkeeping for the server we spawned (or detected as running).
#[derive(Clone)]
struct ServerState {
    model: String,
    mmproj_loaded: bool,
    backend: String,
    port: u16,
}

pub struct LlamaManager {
    app: AppHandle,
    child: Mutex<Option<std::process::Child>>,
    downloading: Arc<AtomicBool>,
    /// Serializes server startup so concurrent callers don't spawn duplicates.
    start_lock: tokio::sync::Mutex<()>,
    /// Cached details of the currently-running server, so status queries and
    /// mmproj-upgrade decisions don't need to probe the process.
    server_state: Mutex<Option<ServerState>>,
}

impl Drop for LlamaManager {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.child.lock() {
            if let Some(mut child) = guard.take() {
                info!("[LlamaManager] Drop — killing orphaned llama-server process...");
                let _ = child.kill();
                // Don't wait — avoid blocking shutdown; the OS will reap the process
            }
        }
    }
}

impl LlamaManager {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            child: Mutex::new(None),
            downloading: Arc::new(AtomicBool::new(false)),
            start_lock: tokio::sync::Mutex::new(()),
            server_state: Mutex::new(None),
        }
    }

    pub fn get_models_dir(&self) -> Result<PathBuf, String> {
        let models_dir = crate::portable::brain_models_dir(&self.app)
            .map_err(|e| format!("Failed to resolve brain models dir: {}", e))?
            .join("llama_cpp");
        if !models_dir.exists() {
            fs::create_dir_all(&models_dir)
                .map_err(|e| format!("Failed to create models folder: {}", e))?;
        }
        Ok(models_dir)
    }

    pub fn get_models_status(&self) -> Result<bool, String> {
        let models_dir = self.get_models_dir()?;
        let model = models_dir.join("gemma-4-E2B-it-qat-UD-Q4_K_XL.gguf");
        let mmproj = models_dir.join("mmproj-F16.gguf");
        let draft = models_dir.join("mtp-gemma-4-E2B-it.gguf");

        Ok(model.exists() && mmproj.exists() && draft.exists())
    }

    pub fn is_downloading(&self) -> bool {
        self.downloading.load(Ordering::SeqCst)
    }

    pub fn stop(&self) {
        let mut guard = self.child.lock().unwrap();
        if let Some(mut child) = guard.take() {
            info!("[LlamaManager] Terminating llama-server process...");
            let _ = child.kill();
            // Don't block on wait() — the Drop impl ensures cleanup,
            // and child.wait() can hang if the process is stuck.
        }
        *self.server_state.lock().unwrap() = None;
        let _ = self.app.emit(
            "llama-server-status",
            LlamaServerStatus {
                running: false,
                state: "stopped".to_string(),
                model: None,
                mmproj_loaded: false,
                backend: "".to_string(),
                port: None,
            },
        );
    }

    /// Ensure the llama.cpp server is running for plain text inference
    /// (Brain conversation, post-processing, merge without audio, …).
    pub async fn ensure_server_running(&self) -> Result<(), String> {
        self.ensure_server_running_with(false).await
    }

    /// Ensure the llama.cpp server is running, upgrading to an mmproj-enabled
    /// (multimodal) instance when `mmproj_required` is true and the current
    /// server was started text-only.
    ///
    /// Shared by every consumer of the local brain model:
    ///   - text-only  (conversation, warmup, post-processing, merge)      → mmproj_required = false
    ///   - multimodal (audio/image conversation, Gemma 4 STT, audio merge) → mmproj_required = true
    pub async fn ensure_server_running_with(&self, mmproj_required: bool) -> Result<(), String> {
        let settings = crate::settings::get_settings(&self.app);
        let provider = settings
            .brain
            .active_provider()
            .ok_or_else(|| "No active brain provider".to_string())?;

        if provider.id != "llama_cpp" {
            return Ok(());
        }

        let port = self.get_server_port(&provider.base_url);

        // Check if responding
        if self.is_port_responding(port).await {
            let needs_mmproj_upgrade = {
                let state = self.server_state.lock().unwrap();
                match state.as_ref() {
                    // We own the server (state was cached by a previous spawn):
                    // restart only if it is text-only but multimodal is required.
                    Some(s) => mmproj_required && !s.mmproj_loaded,
                    // External/unmanaged server on the same port: leave it alone.
                    None => false,
                }
            };
            if !needs_mmproj_upgrade {
                info!(
                    "[LlamaManager] llama-server is already running on port {}",
                    port
                );
                return Ok(());
            }
            info!(
                "[LlamaManager] llama-server is text-only but multimodal input is required — restarting with mmproj"
            );
        }

        // Serialize startup so concurrent callers (warmup, brain_ask, fetch_models, the
        // converse shortcut, multi-STT merge, …) don't each spawn a duplicate llama-server
        // and leak the first child handle. Held across the spawn+poll below (tokio mutex is await-safe).
        let _start_guard = self.start_lock.lock().await;
        // Double-checked: another caller may have brought the server up while we waited.
        let already_running_with_correct_mode = {
            let state = self.server_state.lock().unwrap();
            match state.as_ref() {
                Some(s) => !(mmproj_required && !s.mmproj_loaded),
                None => false,
            }
        };
        if already_running_with_correct_mode && self.is_port_responding(port).await {
            info!("[LlamaManager] llama-server was started by a concurrent caller; reusing it");
            return Ok(());
        }

        // Kill any old handle just in case (also handles the mmproj-upgrade restart)
        self.stop();

        // Check if models exist
        if !self.get_models_status()? {
            return Err(
                "Gemma-4 models are missing. Please download them in settings first.".to_string(),
            );
        }

        // Resolve the active pre-compiled llama-server path
        let server_bin = if let Some(mgr) = self
            .app
            .try_state::<std::sync::Arc<crate::llama_server::manager::LlamaServerManager>>()
        {
            mgr.get_active_server_path()?
        } else {
            // Fallback to resources (legacy)
            self.app
                .path()
                .resolve(
                    #[cfg(windows)]
                    "resources/llama-server.exe",
                    #[cfg(not(windows))]
                    "resources/llama-server",
                    tauri::path::BaseDirectory::Resource,
                )
                .map_err(|e| format!("Failed to resolve llama-server path: {}", e))?
        };

        if !server_bin.exists() {
            return Err(format!(
                "Bundled llama-server executable not found at: {}",
                server_bin.display()
            ));
        }

        info!("[LlamaManager] Server binary: {:?}", server_bin);
        let is_cuda_build = server_bin.to_string_lossy().to_lowercase().contains("cuda");
        let is_vulkan_build = server_bin
            .to_string_lossy()
            .to_lowercase()
            .contains("vulkan");
        let is_gpu_build = is_cuda_build || is_vulkan_build;
        let backend = if is_cuda_build {
            "cuda"
        } else if is_vulkan_build {
            "vulkan"
        } else {
            "cpu"
        };
        info!(
            "[LlamaManager] CUDA build: {}, Vulkan build: {}",
            is_cuda_build, is_vulkan_build
        );

        let models_dir = self.get_models_dir()?;
        let model_path = models_dir.join("gemma-4-E2B-it-qat-UD-Q4_K_XL.gguf");
        let mmproj_path = models_dir.join("mmproj-F16.gguf");
        let draft_path = models_dir.join("mtp-gemma-4-E2B-it.gguf");

        // Load mmproj when multimodal features are enabled (audio/image input,
        // Gemma 4 ASR, multimodal merge) OR when this caller requires it.
        let settings = crate::settings::get_settings(&self.app);
        let multimodal_enabled = settings.brain.multimodal_audio_enabled
            || settings.brain.multimodal_image_enabled
            || settings.multi_stt_gemma4_enabled
            || (settings.multi_stt_use_llama_merge && settings.multi_stt_merge_include_audio)
            || mmproj_required;

        info!(
            "[LlamaManager] Spawning llama-server on port {} with MTP...",
            port
        );
        let _ = self.app.emit(
            "llama-server-status",
            LlamaServerStatus {
                running: false,
                state: "loading".to_string(),
                model: Some("gemma-4-E2B-it-qat-UD-Q4_K_XL.gguf".to_string()),
                mmproj_loaded: multimodal_enabled,
                backend: backend.to_string(),
                port: Some(port),
            },
        );
        let _ = self.app.emit("brain:llama-loading", ());

        let mut cmd = Command::new(&server_bin);
        // Disable attention rotation — saves ~3-4% on short contexts (benchmarked: 203→211 tok/s).
        // Rotation helps at very large contexts (32K+) where quantized KV cache matters,
        // but on short prompts it's pure overhead from the Hadamard FWHT transform.
        cmd.env("LLAMA_ATTN_ROT_DISABLE", "1");

        // Base args
        cmd.args([
            "-m",
            &model_path.to_string_lossy(),
            "--port",
            &port.to_string(),
            "-c",
            "16384",
            "--parallel",
            "1",
            "--flash-attn",
            "on",
            "--no-context-shift",
            "-ngl",
            "-1",
            "--threads",
            "-1",
            "--jinja",
            "--reasoning",
            "off",
            "--model-draft",
            &draft_path.to_string_lossy(),
            "--spec-type",
            "draft-mtp",
            "--spec-draft-n-max",
            "2",
            "--alias",
            "unsloth/gemma-4-e2b-it-qat-GGUF",
            "--metrics",
            "-ctk",
            "f16",
            "-ctv",
            "f16",
        ]);

        // Load mmproj only when multimodal features are enabled.
        // Skipping mmproj saves ~940 MB VRAM and avoids ~3% speed penalty
        // from attention rotation overhead on short prompts.
        if multimodal_enabled {
            info!("[LlamaManager] Multimodal mode — loading mmproj-F16.gguf for audio/image input");
            cmd.args(["--mmproj", &mmproj_path.to_string_lossy()]);
        } else {
            info!("[LlamaManager] Text-only mode — skipping mmproj (saves ~940 MB VRAM, ~3% speed gain)");
        }

        if is_gpu_build {
            info!("[LlamaManager] GPU build — offloading all layers to GPU VRAM (-ngl -1)");
        } else {
            info!("[LlamaManager] CPU-only build — model will run in RAM");
        }

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to spawn llama-server: {}", e))?;
        crate::job_object::register(&mut child);

        // Wait for port response — poll until ready or child exits
        let start = Instant::now();
        let timeout = std::time::Duration::from_secs(90);
        let model_alias = "unsloth/gemma-4-e2b-it-qat-GGUF".to_string();
        loop {
            if self.is_port_responding(port).await {
                info!("[LlamaManager] llama-server started successfully and is responding. (took {:.1}s)", start.elapsed().as_secs_f64());
                {
                    *self.server_state.lock().unwrap() = Some(ServerState {
                        model: model_alias.clone(),
                        mmproj_loaded: multimodal_enabled,
                        backend: backend.to_string(),
                        port,
                    });
                }
                let _ = self.app.emit(
                    "llama-server-status",
                    LlamaServerStatus {
                        running: true,
                        state: "ready".to_string(),
                        model: Some(model_alias.clone()),
                        mmproj_loaded: multimodal_enabled,
                        backend: backend.to_string(),
                        port: Some(port),
                    },
                );
                let _ = self.app.emit("brain:llama-ready", ());
                break;
            }
            // Check if child process exited
            if let Ok(Some(status)) = child.try_wait() {
                let _ = self.app.emit(
                    "llama-server-status",
                    LlamaServerStatus {
                        running: false,
                        state: "error".to_string(),
                        model: None,
                        mmproj_loaded: false,
                        backend: backend.to_string(),
                        port: Some(port),
                    },
                );
                let _ = self.app.emit(
                    "brain:llama-error",
                    format!("llama-server exited with status {:?}", status),
                );
                return Err(format!("llama-server exited with status {:?}", status));
            }
            if start.elapsed() > timeout {
                let _ = self.app.emit(
                    "llama-server-status",
                    LlamaServerStatus {
                        running: false,
                        state: "error".to_string(),
                        model: None,
                        mmproj_loaded: false,
                        backend: backend.to_string(),
                        port: Some(port),
                    },
                );
                let _ = self.app.emit(
                    "brain:llama-error",
                    "llama-server startup timed out after 90s".to_string(),
                );
                return Err("llama-server failed to start within 90 seconds. Check the model files and VRAM availability.".to_string());
            }
            let elapsed = start.elapsed().as_secs_f64();
            if elapsed >= 10.0 {
                info!(
                    "[LlamaManager] Still waiting for llama-server ({:.0}s elapsed)...",
                    elapsed
                );
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }

        *self.child.lock().unwrap() = Some(child);
        Ok(())
    }

    /// Snapshot the current server state for the UI (footer Brain indicator).
    /// Works whether the server was spawned by us or detected externally.
    pub async fn status(&self) -> LlamaServerStatus {
        let settings = crate::settings::get_settings(&self.app);
        let provider = settings.brain.active_provider();

        // Only meaningful when the active brain provider is the local server.
        if !provider.as_ref().is_some_and(|p| p.id == "llama_cpp") {
            return LlamaServerStatus {
                running: false,
                state: "stopped".to_string(),
                model: None,
                mmproj_loaded: false,
                backend: "".to_string(),
                port: None,
            };
        }

        let port = provider
            .as_ref()
            .map(|p| self.get_server_port(&p.base_url))
            .unwrap_or(8001);

        let responding = self.is_port_responding(port).await;
        let cached = self.server_state.lock().unwrap().clone();

        match (responding, cached) {
            (true, Some(state)) => LlamaServerStatus {
                running: true,
                state: "ready".to_string(),
                model: Some(state.model),
                mmproj_loaded: state.mmproj_loaded,
                backend: state.backend,
                port: Some(state.port),
            },
            (true, None) => LlamaServerStatus {
                // Server on our port, but not started by this app instance
                // (external llama-server or stale process).
                running: true,
                state: "ready".to_string(),
                model: None,
                mmproj_loaded: false,
                backend: "".to_string(),
                port: Some(port),
            },
            (false, _) => LlamaServerStatus {
                running: false,
                state: "stopped".to_string(),
                model: None,
                mmproj_loaded: false,
                backend: "".to_string(),
                port: None,
            },
        }
    }

    pub fn start_download_in_background(self: Arc<Self>) {
        if self.downloading.swap(true, Ordering::SeqCst) {
            return; // Already downloading
        }

        let manager = self.clone();
        tauri::async_runtime::spawn(async move {
            let result = manager.download_all_files().await;
            manager.downloading.store(false, Ordering::SeqCst);

            match result {
                Ok(_) => {
                    let _ = manager.app.emit(
                        "llama-download-state",
                        DownloadProgressPayload {
                            status: "completed".to_string(),
                            file: "".to_string(),
                            percentage: 100.0,
                            speed_mbps: 0.0,
                            error: None,
                        },
                    );
                }
                Err(e) => {
                    error!("[LlamaManager] Download failed: {}", e);
                    let _ = manager.app.emit(
                        "llama-download-state",
                        DownloadProgressPayload {
                            status: "error".to_string(),
                            file: "".to_string(),
                            percentage: 0.0,
                            speed_mbps: 0.0,
                            error: Some(e),
                        },
                    );
                }
            }
        });
    }

    async fn download_all_files(&self) -> Result<(), String> {
        let files = &[
            ("gemma-4-E2B-it-qat-UD-Q4_K_XL.gguf", "https://huggingface.co/unsloth/gemma-4-E2B-it-qat-GGUF/resolve/main/gemma-4-E2B-it-qat-UD-Q4_K_XL.gguf"),
            ("mmproj-F16.gguf", "https://huggingface.co/unsloth/gemma-4-E2B-it-qat-GGUF/resolve/main/mmproj-F16.gguf"),
            ("mtp-gemma-4-E2B-it.gguf", "https://huggingface.co/unsloth/gemma-4-E2B-it-qat-GGUF/resolve/main/mtp-gemma-4-E2B-it.gguf"),
        ];

        let models_dir = self.get_models_dir()?;
        let client = reqwest::Client::new();

        for &(name, url) in files {
            let dest_path = models_dir.join(name);
            if dest_path.exists() {
                info!(
                    "[LlamaManager] File {} already exists, skipping download.",
                    name
                );
                continue;
            }

            info!("[LlamaManager] Downloading {} from {}", name, url);
            let response = client
                .get(url)
                .send()
                .await
                .map_err(|e| format!("Failed to initiate download for {}: {}", name, e))?;

            if !response.status().is_success() {
                return Err(format!(
                    "Server returned HTTP {} for {}",
                    response.status(),
                    name
                ));
            }

            let total_size = response.content_length().unwrap_or(0);
            let mut stream = response.bytes_stream();

            let partial_path = models_dir.join(format!("{}.partial", name));
            let mut file = File::create(&partial_path)
                .map_err(|e| format!("Failed to create partial file for {}: {}", name, e))?;

            let mut downloaded = 0u64;
            let start_time = Instant::now();
            let mut last_emit = Instant::now();

            while let Some(chunk_result) = stream.next().await {
                let chunk = chunk_result
                    .map_err(|e| format!("Stream error during download of {}: {}", name, e))?;
                file.write_all(&chunk)
                    .map_err(|e| format!("Failed to write chunk to disk for {}: {}", name, e))?;

                downloaded += chunk.len() as u64;

                // Emit progress every 300ms to avoid spamming Tauri events
                if last_emit.elapsed().as_millis() > 300 {
                    last_emit = Instant::now();
                    let elapsed = start_time.elapsed().as_secs_f64();
                    let speed = if elapsed > 0.0 {
                        (downloaded as f64 / 1024.0 / 1024.0) / elapsed
                    } else {
                        0.0
                    };

                    let percentage = if total_size > 0 {
                        (downloaded as f64 / total_size as f64) * 100.0
                    } else {
                        0.0
                    };

                    let _ = self.app.emit(
                        "llama-download-state",
                        DownloadProgressPayload {
                            status: "downloading".to_string(),
                            file: name.to_string(),
                            percentage,
                            speed_mbps: speed,
                            error: None,
                        },
                    );
                }
            }

            // Rename partial to final destination
            drop(file);
            fs::rename(&partial_path, &dest_path)
                .map_err(|e| format!("Failed to finalize downloaded file {}: {}", name, e))?;

            info!("[LlamaManager] Completed download of {}", name);
        }

        Ok(())
    }

    fn get_server_port(&self, base_url: &str) -> u16 {
        if let Ok(url) = reqwest::Url::parse(base_url) {
            url.port().unwrap_or(8001)
        } else {
            if base_url.contains(":8080") {
                8080
            } else {
                8001
            }
        }
    }

    async fn is_port_responding(&self, port: u16) -> bool {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(500))
            .build()
            .unwrap_or_default();

        let url = format!("http://127.0.0.1:{}/health", port);
        match client.get(&url).send().await {
            Ok(resp) => resp.status().is_success(),
            Err(_) => {
                let fallback_url = format!("http://127.0.0.1:{}/v1/models", port);
                match client.get(&fallback_url).send().await {
                    Ok(resp) => resp.status().is_success(),
                    Err(_) => false,
                }
            }
        }
    }
}
