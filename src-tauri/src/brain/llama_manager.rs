use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager};

/// A file in the local Brain models directory (`models/Brain/llama_cpp`),
/// listed by `list_downloaded_models` for the Models hub Brain tab.
#[derive(Serialize, Deserialize, Clone, Debug, Type)]
#[serde(rename_all = "camelCase")]
pub struct BrainModelFile {
    pub filename: String,
    pub size_mb: f64,
    /// Rough classification for UI grouping: "model" | "mmproj" | "mtp".
    pub kind: String,
}

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
    model_file: String,
    mmproj_loaded: bool,
    reasoning_enabled: bool,
    backend: String,
    port: u16,
}

/// Hugging Face repo hosting the unsloth Gemma 4 2B (E2B) GGUF quants, MTP drafts and
/// mmproj files for the local llama.cpp Brain.
pub const GEMMA4_HF_REPO: &str = "unsloth/gemma-4-E2B-it-qat-GGUF";

/// Hugging Face repo hosting the unsloth Gemma 4 4B (E4B) GGUF quants, MTP drafts and
/// mmproj files for the local llama.cpp Brain.
pub const GEMMA4_E4B_HF_REPO: &str = "unsloth/gemma-4-E4B-it-qat-GGUF";

/// Mobile-optimized variant (Google's mobile QAT checkpoint) — only
/// UD-Q2_K_XL is published, plus the same shared mmproj/MTP files.
pub const GEMMA4_MOBILE_HF_REPO: &str = "unsloth/gemma-4-E2B-it-qat-mobile-GGUF";

/// The HF repo for a model variant id ("standard" | "2b" | "4b" | "e4b" | "mobile").
pub fn gemma4_repo(variant: &str) -> &'static str {
    match variant {
        "4b" | "e4b" => GEMMA4_E4B_HF_REPO,
        "mobile" => GEMMA4_MOBILE_HF_REPO,
        _ => GEMMA4_HF_REPO,
    }
}

/// File name the main Gemma 4 model GGUF has INSIDE its HF repo.
fn remote_model_file_name(variant: &str, quant: &str) -> String {
    match variant {
        "4b" | "e4b" => format!("gemma-4-E4B-it-qat-UD-{quant}.gguf"),
        _ => format!("gemma-4-E2B-it-qat-UD-{quant}.gguf"),
    }
}

/// Local file name of the main Gemma 4 model GGUF.
pub fn model_file_name(variant: &str, quant: &str) -> String {
    match variant {
        "4b" | "e4b" => format!("gemma-4-E4B-it-qat-UD-{quant}.gguf"),
        "mobile" => format!("gemma-4-E2B-it-qat-mobile-UD-{quant}.gguf"),
        _ => format!("gemma-4-E2B-it-qat-UD-{quant}.gguf"),
    }
}

/// File name the multimodal projector GGUF has INSIDE its HF repo.
pub fn remote_mmproj_file_name(quant: &str) -> String {
    format!("mmproj-{quant}.gguf")
}

/// Local file name of the multimodal projector for a given model variant and precision id.
pub fn mmproj_file_name(variant: &str, quant: &str) -> String {
    match variant {
        "4b" | "e4b" => format!("mmproj-gemma-4-E4B-{quant}.gguf"),
        _ => format!("mmproj-gemma-4-E2B-{quant}.gguf"),
    }
}

/// Resolve the multimodal projector path. Legacy 2B installs saved it as bare `mmproj-{quant}.gguf`
/// — reuse that file for 2B/standard/mobile so existing users don't re-download 940 MB.
pub fn mmproj_path(models_dir: &std::path::Path, variant: &str, quant: &str) -> PathBuf {
    let new_path = models_dir.join(mmproj_file_name(variant, quant));
    if new_path.exists() {
        return new_path;
    }
    if variant == "standard" || variant == "mobile" || variant == "2b" {
        let legacy = models_dir.join(format!("mmproj-{quant}.gguf"));
        if legacy.exists() {
            return legacy;
        }
    }
    new_path
}

/// Local file name of the MTP draft model for a quantization id.
pub fn mtp_file_name(variant: &str, quant: &str) -> String {
    match variant {
        "4b" | "e4b" => format!("mtp-gemma-4-E4B-it-{quant}.gguf"),
        _ => format!("mtp-gemma-4-E2B-it-{quant}.gguf"),
    }
}

/// Resolve the MTP draft path. Q4_0 was historically stored as the bare
/// root file — reuse it so existing installs don't re-download the same bytes.
pub fn mtp_path(models_dir: &std::path::Path, variant: &str, quant: &str) -> PathBuf {
    let new_path = models_dir.join(mtp_file_name(variant, quant));
    if (variant == "standard" || variant == "mobile" || variant == "2b")
        && quant == "Q4_0"
        && !new_path.exists()
    {
        let legacy = models_dir.join("mtp-gemma-4-E2B-it.gguf");
        if legacy.exists() {
            return legacy;
        }
    }
    if (variant == "4b" || variant == "e4b") && quant == "Q4_0" && !new_path.exists() {
        let legacy = models_dir.join("mtp-gemma-4-E4B-it.gguf");
        if legacy.exists() {
            return legacy;
        }
    }
    new_path
}

/// One selectable quantization option for the Gemma 4 model, mmproj or MTP
/// draft. `id` is the quant string embedded in the file name.
#[derive(Serialize, Deserialize, Clone, Debug, Type)]
pub struct Gemma4QuantOption {
    pub id: String,
    pub label: String,
    pub size_mb: f64,
}

/// The full set of downloadable Gemma 4 quantization choices, grouped by the
/// three files the local Brain needs (model, mmproj, MTP draft).
#[derive(Serialize, Deserialize, Clone, Debug, Type)]
pub struct Gemma4QuantCatalog {
    pub model: Vec<Gemma4QuantOption>,
    pub mmproj: Vec<Gemma4QuantOption>,
    pub mtp: Vec<Gemma4QuantOption>,
}

/// Hardcoded fallback catalog (current contents of the unsloth repo) used
/// when the HF API is unreachable.
fn fallback_quant_catalog(variant: &str) -> Gemma4QuantCatalog {
    if variant == "4b" || variant == "e4b" {
        Gemma4QuantCatalog {
            model: vec![
                Gemma4QuantOption {
                    id: "Q2_K_XL".to_string(),
                    label: "Q2_K_XL".to_string(),
                    size_mb: 3070.4,
                },
                Gemma4QuantOption {
                    id: "Q4_K_XL".to_string(),
                    label: "Q4_K_XL".to_string(),
                    size_mb: 4020.4,
                },
            ],
            mmproj: vec![
                Gemma4QuantOption {
                    id: "BF16".to_string(),
                    label: "BF16".to_string(),
                    size_mb: 945.6,
                },
                Gemma4QuantOption {
                    id: "F16".to_string(),
                    label: "F16".to_string(),
                    size_mb: 944.5,
                },
                Gemma4QuantOption {
                    id: "F32".to_string(),
                    label: "F32".to_string(),
                    size_mb: 1823.9,
                },
            ],
            mtp: vec![
                Gemma4QuantOption {
                    id: "Q4_0".to_string(),
                    label: "Q4_0".to_string(),
                    size_mb: 56.9,
                },
                Gemma4QuantOption {
                    id: "Q8_0".to_string(),
                    label: "Q8_0".to_string(),
                    size_mb: 94.1,
                },
                Gemma4QuantOption {
                    id: "F16".to_string(),
                    label: "F16".to_string(),
                    size_mb: 163.8,
                },
                Gemma4QuantOption {
                    id: "BF16".to_string(),
                    label: "BF16".to_string(),
                    size_mb: 163.8,
                },
            ],
        }
    } else {
        Gemma4QuantCatalog {
            model: vec![
                Gemma4QuantOption {
                    id: "Q2_K_XL".to_string(),
                    label: "Q2_K_XL".to_string(),
                    size_mb: 2085.0,
                },
                Gemma4QuantOption {
                    id: "Q4_K_XL".to_string(),
                    label: "Q4_K_XL".to_string(),
                    size_mb: 2499.0,
                },
            ],
            mmproj: vec![
                Gemma4QuantOption {
                    id: "BF16".to_string(),
                    label: "BF16".to_string(),
                    size_mb: 941.0,
                },
                Gemma4QuantOption {
                    id: "F16".to_string(),
                    label: "F16".to_string(),
                    size_mb: 940.0,
                },
                Gemma4QuantOption {
                    id: "F32".to_string(),
                    label: "F32".to_string(),
                    size_mb: 1815.0,
                },
            ],
            mtp: vec![
                Gemma4QuantOption {
                    id: "Q4_0".to_string(),
                    label: "Q4_0".to_string(),
                    size_mb: 56.5,
                },
                Gemma4QuantOption {
                    id: "Q8_0".to_string(),
                    label: "Q8_0".to_string(),
                    size_mb: 93.3,
                },
                Gemma4QuantOption {
                    id: "F16".to_string(),
                    label: "F16".to_string(),
                    size_mb: 162.3,
                },
                Gemma4QuantOption {
                    id: "BF16".to_string(),
                    label: "BF16".to_string(),
                    size_mb: 162.3,
                },
            ],
        }
    }
}

#[derive(Deserialize)]
struct HfTreeEntry {
    path: String,
    #[serde(default)]
    size: u64,
    #[serde(rename = "type")]
    entry_type: String,
    /// Present for LFS-backed files (GGUFs): `oid` is the content SHA256.
    #[serde(default)]
    lfs: Option<HfLfsInfo>,
}

#[derive(Deserialize)]
struct HfLfsInfo {
    oid: String,
}

async fn fetch_hf_tree(
    client: &reqwest::Client,
    repo: &str,
    subdir: &str,
) -> Result<Vec<HfTreeEntry>, String> {
    let url = if subdir.is_empty() {
        format!("https://huggingface.co/api/models/{repo}/tree/main")
    } else {
        format!("https://huggingface.co/api/models/{repo}/tree/main/{subdir}")
    };
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to reach Hugging Face API: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("Hugging Face API returned HTTP {}", resp.status()));
    }
    resp.json::<Vec<HfTreeEntry>>()
        .await
        .map_err(|e| format!("Failed to parse Hugging Face API response: {e}"))
}

/// Fetch the available Gemma 4 quantizations from the unsloth Hugging Face
/// repo (model GGUF quants, mmproj precisions, MTP draft quants). Falls back
/// to a hardcoded snapshot when the HF API is unreachable.
pub async fn fetch_gemma4_quant_catalog(variant: Option<&str>) -> Gemma4QuantCatalog {
    let var = variant.unwrap_or("standard");
    let repo = gemma4_repo(var);
    let model_prefix = if var == "4b" || var == "e4b" {
        "gemma-4-E4B-it-qat-UD-"
    } else {
        "gemma-4-E2B-it-qat-UD-"
    };
    let mtp_prefix = if var == "4b" || var == "e4b" {
        "mtp-gemma-4-E4B-it-"
    } else {
        "mtp-gemma-4-E2B-it-"
    };

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            log::warn!(
                "[LlamaManager] Failed to build HTTP client: {e}; using fallback quant catalog"
            );
            return fallback_quant_catalog(var);
        }
    };

    let (root_entries, mtp_entries) = match (
        fetch_hf_tree(&client, repo, "").await,
        fetch_hf_tree(&client, repo, "MTP").await,
    ) {
        (Ok(root), Ok(mtp)) => (root, mtp),
        (root_res, mtp_res) => {
            log::warn!(
                "[LlamaManager] HF quant catalog unavailable for {repo} (root={}, mtp={}); using fallback",
                root_res.is_ok(),
                mtp_res.is_ok()
            );
            return fallback_quant_catalog(var);
        }
    };

    let to_option =
        |entries: &[HfTreeEntry], prefix: &str, suffix: &str| -> Vec<Gemma4QuantOption> {
            entries
                .iter()
                .filter(|e| e.entry_type == "file")
                .filter_map(|e| {
                    let name = e.path.rsplit('/').next()?;
                    name.strip_prefix(prefix)?
                        .strip_suffix(suffix)
                        .map(|id| Gemma4QuantOption {
                            id: id.to_string(),
                            label: id.to_string(),
                            size_mb: e.size as f64 / (1024.0 * 1024.0),
                        })
                })
                .collect()
        };

    let mut model = to_option(&root_entries, model_prefix, ".gguf");
    let mut mmproj = to_option(&root_entries, "mmproj-", ".gguf");
    let mut mtp = to_option(&mtp_entries, mtp_prefix, ".gguf");

    model.sort_by(|a, b| a.id.cmp(&b.id));
    mmproj.sort_by(|a, b| a.id.cmp(&b.id));
    mtp.sort_by(|a, b| a.id.cmp(&b.id));

    if model.is_empty() || mmproj.is_empty() || mtp.is_empty() {
        log::warn!(
            "[LlamaManager] HF quant catalog parsed empty for {repo} (model={}, mmproj={}, mtp={}); using fallback",
            model.len(),
            mmproj.len(),
            mtp.len()
        );
        return fallback_quant_catalog(var);
    }

    Gemma4QuantCatalog { model, mmproj, mtp }
}

pub struct LlamaManager {
    app: AppHandle,
    child: Mutex<Option<std::process::Child>>,
    /// Path of the llama-server binary we spawned (None when stopped), so the
    /// server manager can refuse to delete the install of a running server.
    spawned_bin: Mutex<Option<PathBuf>>,
    downloading: Arc<AtomicBool>,
    /// Cancellation token for the active model download (None when idle).
    download_cancel: Mutex<Option<hf_hub::api::tokio::CancellationToken>>,
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
            spawned_bin: Mutex::new(None),
            downloading: Arc::new(AtomicBool::new(false)),
            download_cancel: Mutex::new(None),
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
        let settings = crate::settings::get_settings(&self.app);
        let variant = &settings.brain.llama_model_variant;
        let model = models_dir.join(model_file_name(variant, &settings.brain.llama_model_quant));
        if !model.exists() {
            return Ok(false);
        }

        let mmproj_needed = settings.brain.llama_mmproj_enabled
            && settings.brain.llama_mmproj_quant.to_lowercase() != "disabled";
        if mmproj_needed {
            let mmproj = mmproj_path(&models_dir, variant, &settings.brain.llama_mmproj_quant);
            if !mmproj.exists() {
                return Ok(false);
            }
        }

        let draft_needed = settings.brain.llama_mtp_enabled
            && settings.brain.llama_mtp_quant.to_lowercase() != "disabled";
        if draft_needed {
            let draft = mtp_path(&models_dir, variant, &settings.brain.llama_mtp_quant);
            if !draft.exists() {
                return Ok(false);
            }
        }

        Ok(true)
    }

    pub fn is_downloading(&self) -> bool {
        self.downloading.load(Ordering::SeqCst)
    }

    /// Cancel the in-flight model download (keeps partials for resume).
    /// Returns whether a download was actually active.
    pub fn cancel_download(&self) -> bool {
        if let Some(token) = self
            .download_cancel
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            token.cancel();
            true
        } else {
            false
        }
    }

    /// Delete one downloaded model file from the Brain models dir by
    /// filename (GGUF model, mmproj or MTP draft). Refuses path traversal.
    pub fn delete_model_file(&self, filename: &str) -> Result<(), String> {
        if filename.is_empty()
            || filename.contains('/')
            || filename.contains('\\')
            || filename.contains("..")
        {
            return Err(format!("Invalid model filename '{filename}'"));
        }
        let dir = self.get_models_dir()?;
        let path = dir.join(filename);
        if !path.exists() {
            return Err(format!("'{filename}' is not downloaded"));
        }
        fs::remove_file(&path).map_err(|e| format!("Failed to delete {filename}: {e}"))?;
        info!("[LlamaManager] Deleted brain model file {filename}");
        Ok(())
    }

    /// List downloaded Brain model files (GGUFs) with sizes and a coarse
    /// kind classification, for the Models hub Brain tab.
    pub fn list_downloaded_models(&self) -> Vec<BrainModelFile> {
        let Ok(dir) = self.get_models_dir() else {
            return Vec::new();
        };
        let mut files = Vec::new();
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = match path.file_name().and_then(|n| n.to_str()) {
                    Some(n) => n.to_string(),
                    None => continue,
                };
                if !name.ends_with(".gguf") {
                    continue;
                }
                let kind = if name.starts_with("mmproj-") {
                    "mmproj"
                } else if name.starts_with("mtp-") {
                    "mtp"
                } else {
                    "model"
                };
                let size_mb =
                    entry.metadata().map(|m| m.len()).unwrap_or(0) as f64 / (1024.0 * 1024.0);
                files.push(BrainModelFile {
                    filename: name,
                    size_mb,
                    kind: kind.to_string(),
                });
            }
        }
        files.sort_by(|a, b| a.filename.cmp(&b.filename));
        files
    }

    pub fn stop(&self) {
        let mut guard = self.child.lock().unwrap();
        if let Some(mut child) = guard.take() {
            info!("[LlamaManager] Terminating llama-server process...");
            let _ = child.kill();
            // Don't block on wait() — the Drop impl ensures cleanup,
            // and child.wait() can hang if the process is stuck.
        }
        *self.spawned_bin.lock().unwrap() = None;
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
    /// server was started without mmproj.
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
        let variant = &settings.brain.llama_model_variant;
        let target_model_file = model_file_name(variant, &settings.brain.llama_model_quant);

        let mmproj_requested = settings.brain.llama_mmproj_enabled
            && settings.brain.llama_mmproj_quant.to_lowercase() != "disabled"
            && mmproj_required;

        // Check if responding
        if self.is_port_responding(port).await {
            let needs_restart = {
                let state = self.server_state.lock().unwrap();
                match state.as_ref() {
                    Some(s) => {
                        let model_changed = s.model_file != target_model_file;
                        let mmproj_upgrade = mmproj_requested && !s.mmproj_loaded;
                        let reasoning_changed =
                            s.reasoning_enabled != settings.brain.reasoning_enabled;
                        model_changed || mmproj_upgrade || reasoning_changed
                    }
                    None => false,
                }
            };
            if !needs_restart {
                info!(
                    "[LlamaManager] llama-server is already running on port {}",
                    port
                );
                return Ok(());
            }
            info!(
                "[LlamaManager] llama-server configuration changed (model/reasoning/mmproj) — restarting"
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
                Some(s) => {
                    s.model_file == target_model_file
                        && !(mmproj_requested && !s.mmproj_loaded)
                        && s.reasoning_enabled == settings.brain.reasoning_enabled
                }
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
            let variant = &settings.brain.llama_model_variant;
            return Err(format!(
                "Gemma 4 ({variant}) models are missing. Please download them in settings first."
            ));
        }

        self.ensure_server_running_internal(port, mmproj_required)
            .await
    }

    async fn ensure_server_running_internal(
        &self,
        port: u16,
        _mmproj_required: bool,
    ) -> Result<(), String> {
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
        let settings = crate::settings::get_settings(&self.app);
        let variant = &settings.brain.llama_model_variant;
        let model_file = model_file_name(variant, &settings.brain.llama_model_quant);
        let model_path = models_dir.join(&model_file);
        let mmproj_file = mmproj_path(&models_dir, variant, &settings.brain.llama_mmproj_quant);
        let draft_file = mtp_path(&models_dir, variant, &settings.brain.llama_mtp_quant);

        let mmproj_opt_in = settings.brain.llama_mmproj_enabled
            && settings.brain.llama_mmproj_quant.to_lowercase() != "disabled";

        // `llama_mmproj_enabled` is the single toggle for all multimodal input
        // (audio, image, video). When the user turns it on we always load mmproj
        // so the server is ready for any modality without requiring a restart.
        // The `mmproj_required` flag is still used by ensure_server_running_with()
        // for the upgrade-from-text-only restart path, but at spawn time we honour
        // the toggle unconditionally.
        let multimodal_enabled = mmproj_opt_in && mmproj_file.exists();

        let draft_opt_in = settings.brain.llama_mtp_enabled
            && settings.brain.llama_mtp_quant.to_lowercase() != "disabled"
            && draft_file.exists();

        let has_draft = draft_opt_in;
        let mut spawn_title = format!("[LlamaManager] Spawning llama-server on port {port}");
        if has_draft {
            spawn_title.push_str(" with MTP...");
        } else {
            spawn_title.push_str(" without draft acceleration...");
        }
        info!("{spawn_title}");

        let _ = self.app.emit(
            "llama-server-status",
            LlamaServerStatus {
                running: false,
                state: "loading".to_string(),
                model: Some(model_file.clone()),
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

        let reasoning_arg = if settings.brain.reasoning_enabled {
            "on"
        } else {
            "off"
        };

        let model_alias = match settings.brain.llama_model_variant.as_str() {
            "4b" | "e4b" => "unsloth/gemma-4-e4b-it-qat-GGUF".to_string(),
            "mobile" => "unsloth/gemma-4-e2b-it-qat-mobile-GGUF".to_string(),
            _ => "unsloth/gemma-4-e2b-it-qat-GGUF".to_string(),
        };

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
            reasoning_arg,
            "--alias",
            &model_alias,
            "--metrics",
            "-ctk",
            "f16",
            "-ctv",
            "f16",
        ]);

        if has_draft {
            cmd.args([
                "--model-draft",
                &draft_file.to_string_lossy(),
                "--spec-type",
                "draft-mtp",
                "--spec-draft-n-max",
                "2",
            ]);
        }

        // Load mmproj only when multimodal features are enabled.
        // Skipping mmproj saves ~1 GB VRAM and avoids ~3% speed penalty
        // from attention rotation overhead on short prompts.
        if multimodal_enabled {
            info!(
                "[LlamaManager] Multimodal mode — loading {} for audio/image input",
                mmproj_file.display()
            );
            cmd.args(["--mmproj", &mmproj_file.to_string_lossy()]);
        } else {
            info!("[LlamaManager] Text-only mode — skipping mmproj (saves ~1 GB VRAM, ~3% speed gain)");
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
        loop {
            if self.is_port_responding(port).await {
                info!("[LlamaManager] llama-server started successfully and is responding. (took {:.1}s)", start.elapsed().as_secs_f64());
                {
                    *self.server_state.lock().unwrap() = Some(ServerState {
                        model_file: model_file.clone(),
                        mmproj_loaded: multimodal_enabled,
                        reasoning_enabled: settings.brain.reasoning_enabled,
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
        *self.spawned_bin.lock().unwrap() = Some(server_bin);
        Ok(())
    }

    /// Directory of the llama-server install currently running (if any).
    /// Used to guard against deleting a live server's files.
    pub fn running_server_dir(&self) -> Option<PathBuf> {
        self.spawned_bin
            .lock()
            .ok()?
            .as_ref()
            .and_then(|bin| bin.parent().map(|p| p.to_path_buf()))
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
                model: Some(state.model_file),
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

        // Fresh cancellation token for this run.
        let token = hf_hub::api::tokio::CancellationToken::new();
        *self
            .download_cancel
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(token.clone());

        let manager = self.clone();
        tauri::async_runtime::spawn(async move {
            let result = manager.download_all_files(&token).await;
            manager.downloading.store(false, Ordering::SeqCst);
            *manager
                .download_cancel
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = None;

            let cancelled = token.is_cancelled();
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
                    crate::model_hub::notify(
                        &manager.app,
                        crate::model_hub::ModelCollection::Brain,
                        &manager.hub_download_id(),
                        "Gemma 4",
                        crate::model_hub::HubNotificationKind::Completed,
                        None,
                    );
                }
                Err(e) => {
                    error!("[LlamaManager] Download failed: {}", e);
                    let status = if cancelled { "cancelled" } else { "error" };
                    let _ = manager.app.emit(
                        "llama-download-state",
                        DownloadProgressPayload {
                            status: status.to_string(),
                            file: "".to_string(),
                            percentage: 0.0,
                            speed_mbps: 0.0,
                            error: Some(e.clone()),
                        },
                    );
                    crate::model_hub::notify(
                        &manager.app,
                        crate::model_hub::ModelCollection::Brain,
                        &manager.hub_download_id(),
                        "Gemma 4",
                        if cancelled {
                            crate::model_hub::HubNotificationKind::Cancelled
                        } else {
                            crate::model_hub::HubNotificationKind::Failed
                        },
                        if cancelled { None } else { Some(e) },
                    );
                }
            }
        });
    }

    /// Stable hub id for the currently-configured Brain download target.
    fn hub_download_id(&self) -> String {
        let settings = crate::settings::get_settings(&self.app);
        let variant = &settings.brain.llama_model_variant;
        model_file_name(variant, &settings.brain.llama_model_quant)
    }

    /// SHA256 hashes for every LFS file in `repo`, keyed by repo-relative
    /// path. Best-effort: an unreachable API yields an empty map and the
    /// downloads proceed unverified (as before).
    async fn hf_file_hashes(repo: &str) -> std::collections::HashMap<String, String> {
        let mut hashes = std::collections::HashMap::new();
        let Ok(client) = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
        else {
            return hashes;
        };
        let mut entries = match fetch_hf_tree(&client, repo, "").await {
            Ok(e) => e,
            Err(e) => {
                warn!(
                    "[LlamaManager] HF tree unavailable for {repo} ({e}); skipping sha256 pinning"
                );
                return hashes;
            }
        };
        if let Ok(mtp) = fetch_hf_tree(&client, repo, "MTP").await {
            entries.extend(mtp.into_iter().map(|mut e| {
                e.path = format!("MTP/{}", e.path);
                e
            }));
        }
        for entry in entries {
            if let Some(lfs) = entry.lfs {
                hashes.insert(entry.path, lfs.oid);
            }
        }
        hashes
    }

    async fn download_all_files(
        &self,
        cancel: &hf_hub::api::tokio::CancellationToken,
    ) -> Result<(), String> {
        let settings = crate::settings::get_settings(&self.app);
        let variant = settings.brain.llama_model_variant.clone();
        let model_quant = settings.brain.llama_model_quant.clone();
        let model_name = model_file_name(&variant, &model_quant);
        let model_repo = gemma4_repo(&variant);
        let model_url_name = remote_model_file_name(&variant, &model_quant);
        let hub_id = self.hub_download_id();

        // (local name, repo-relative remote path)
        let mut files: Vec<(String, String)> = Vec::new();

        // 1. Model GGUF
        files.push((model_name.clone(), model_url_name));

        // 2. Multimodal projector (if enabled)
        let mmproj_needed = settings.brain.llama_mmproj_enabled
            && settings.brain.llama_mmproj_quant.to_lowercase() != "disabled";
        if mmproj_needed {
            files.push((
                mmproj_file_name(&variant, &settings.brain.llama_mmproj_quant),
                remote_mmproj_file_name(&settings.brain.llama_mmproj_quant),
            ));
        }

        // 3. MTP Speculative draft (if enabled)
        let draft_needed = settings.brain.llama_mtp_enabled
            && settings.brain.llama_mtp_quant.to_lowercase() != "disabled";
        if draft_needed {
            let mtp_local_name = mtp_file_name(&variant, &settings.brain.llama_mtp_quant);
            files.push((mtp_local_name.clone(), format!("MTP/{mtp_local_name}")));
        }

        let models_dir = self.get_models_dir()?;

        // Pin revisions with LFS sha256 hashes so every file is verified
        // after download (mirrors the STT catalog trust model).
        let hashes = Self::hf_file_hashes(&model_repo).await;

        for (name, remote_path) in &files {
            if cancel.is_cancelled() {
                return Err("cancelled".to_string());
            }
            let dest_path = models_dir.join(name);
            // Check legacy files so existing installs don't re-download:
            // 1. MTP Q4_0 draft legacy names
            let legacy_mtp = (name == "mtp-gemma-4-E2B-it-Q4_0.gguf"
                && models_dir.join("mtp-gemma-4-E2B-it.gguf").exists())
                || (name == "mtp-gemma-4-E4B-it-Q4_0.gguf"
                    && models_dir.join("mtp-gemma-4-E4B-it.gguf").exists());
            // 2. 2B mmproj legacy name `mmproj-{quant}.gguf` (e.g. `mmproj-F16.gguf` for 2B)
            let legacy_mmproj = name.starts_with("mmproj-gemma-4-E2B-")
                && models_dir
                    .join(name.replacen("mmproj-gemma-4-E2B-", "mmproj-", 1))
                    .exists();

            if dest_path.exists() || legacy_mtp || legacy_mmproj {
                info!(
                    "[LlamaManager] File {} already exists, skipping download.",
                    name
                );
                continue;
            }

            let url = format!("https://huggingface.co/{model_repo}/resolve/main/{remote_path}");
            let partial_path = models_dir.join(format!("{name}.partial"));
            let sha256 = hashes.get(remote_path).cloned();
            info!("[LlamaManager] Downloading {} from {}", name, url);

            // Emit both the legacy per-file event and the typed hub event.
            let app = self.app.clone();
            let file_label = name.clone();
            let hub_id_label = hub_id.clone();
            let emit = move |event: crate::model_hub::transport::TransportEvent| match event {
                crate::model_hub::transport::TransportEvent::Progress(p) => {
                    let percent = if p.total > 0 {
                        (p.downloaded as f64 / p.total as f64) * 100.0
                    } else {
                        0.0
                    };
                    let _ = app.emit(
                        "llama-download-state",
                        DownloadProgressPayload {
                            status: "downloading".to_string(),
                            file: file_label.clone(),
                            percentage: percent,
                            speed_mbps: p.speed_mbps,
                            error: None,
                        },
                    );
                    crate::model_hub::emit_progress(
                        &app,
                        crate::model_hub::ModelHubDownloadProgress {
                            collection: crate::model_hub::ModelCollection::Brain,
                            id: hub_id_label.clone(),
                            name: "Gemma 4".to_string(),
                            file: Some(file_label.clone()),
                            downloaded_mb: p.downloaded as f64 / (1024.0 * 1024.0),
                            total_mb: p.total as f64 / (1024.0 * 1024.0),
                            percent,
                            speed_mbps: p.speed_mbps,
                            status: crate::model_hub::HubDownloadStatus::Downloading,
                            error: None,
                        },
                    );
                }
                crate::model_hub::transport::TransportEvent::VerifyStart => {
                    crate::model_hub::emit_progress(
                        &app,
                        crate::model_hub::ModelHubDownloadProgress {
                            collection: crate::model_hub::ModelCollection::Brain,
                            id: hub_id_label.clone(),
                            name: "Gemma 4".to_string(),
                            file: Some(file_label.clone()),
                            downloaded_mb: 0.0,
                            total_mb: 0.0,
                            percent: 100.0,
                            speed_mbps: 0.0,
                            status: crate::model_hub::HubDownloadStatus::Verifying,
                            error: None,
                        },
                    );
                }
                crate::model_hub::transport::TransportEvent::VerifyEnd => {}
            };

            match crate::model_hub::transport::download_file_resumable(
                name,
                &url,
                &partial_path,
                sha256.as_deref(),
                cancel,
                &emit,
            )
            .await
            {
                Ok(crate::model_hub::transport::TransportOutcome::Completed) => {
                    fs::rename(&partial_path, &dest_path).map_err(|e| {
                        format!("Failed to finalize downloaded file {}: {}", name, e)
                    })?;
                    info!("[LlamaManager] Completed download of {}", name);
                }
                Ok(crate::model_hub::transport::TransportOutcome::Cancelled) => {
                    return Err("cancelled".to_string());
                }
                Err(e) => {
                    return Err(format!("Failed to download {name}: {e}"));
                }
            }
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
