use hf_hub::api::tokio::CancellationToken;
use log::{info, warn};
use specta::Type;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Type)]
pub struct LlamaRelease {
    pub tag: String,
    pub name: String,
    pub assets: Vec<LlamaAsset>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Type)]
pub struct LlamaAsset {
    pub name: String,
    pub backend: String,
    pub os: String,
    pub arch: String,
    pub download_url: String,
    #[specta(type = u32)]
    pub size_bytes: u64,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Type)]
pub struct DownloadedServer {
    pub backend: String,
    pub release_tag: String,
    pub path: String,
    #[specta(type = u32)]
    pub size_bytes: u64,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Type)]
pub struct LlamaServerConfig {
    pub backend: String,     // "cuda", "vulkan", "cpu"
    pub release_tag: String, // e.g. "b9601"
}

impl Default for LlamaServerConfig {
    fn default() -> Self {
        Self {
            backend: detect_preferred_backend(),
            release_tag: String::new(),
        }
    }
}

fn detect_preferred_backend() -> String {
    #[cfg(target_os = "windows")]
    {
        // Check for NVIDIA GPU via nvidia-smi
        if let Ok(output) = Command::new("nvidia-smi")
            .arg("--query-gpu=name")
            .arg("--format=csv,noheader")
            .output()
        {
            if output.status.success() && !output.stdout.is_empty() {
                return "cuda".to_string();
            }
        }
        // Check CUDA_PATH
        if std::env::var("CUDA_PATH").is_ok() {
            return "cuda".to_string();
        }
        // Check Vulkan
        if std::env::var("VULKAN_SDK").is_ok() || Path::new("C:\\VulkanSDK").exists() {
            return "vulkan".to_string();
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(output) = Command::new("nvidia-smi").output() {
            if output.status.success() {
                return "cuda".to_string();
            }
        }
        if Path::new("/usr/local/cuda").exists() {
            return "cuda".to_string();
        }
    }
    #[cfg(target_os = "macos")]
    {
        // macOS uses Metal via Accelerate framework, treated as CPU-ish but with GPU acceleration
        // We expose "cpu" but Metal acceleration is built into the standard binary
    }
    "cpu".to_string()
}

pub struct LlamaServerManager {
    app: AppHandle,
    /// Cancellation tokens for in-flight server downloads, keyed by
    /// "{backend}-{tag}" — also serves as the in-flight dedupe map.
    active_downloads: Mutex<HashMap<String, CancellationToken>>,
}

impl LlamaServerManager {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            active_downloads: Mutex::new(HashMap::new()),
        }
    }

    /// Cancel an in-flight server download. Returns whether one was active.
    pub fn cancel_download(&self, id: &str) -> bool {
        let active = self
            .active_downloads
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(token) = active.get(id) {
            token.cancel();
            true
        } else {
            false
        }
    }

    fn servers_dir(&self) -> Result<PathBuf, String> {
        let dir = crate::portable::app_data_dir(&self.app)
            .map_err(|e| format!("Failed to resolve app data dir: {}", e))?
            .join("llama_cpp_servers");
        if !dir.exists() {
            fs::create_dir_all(&dir).map_err(|e| format!("Failed to create servers dir: {}", e))?;
        }
        Ok(dir)
    }

    fn current_os_key(&self) -> &str {
        if cfg!(target_os = "windows") {
            "windows"
        } else if cfg!(target_os = "linux") {
            "linux"
        } else {
            "macos"
        }
    }

    fn current_arch_key(&self) -> &str {
        if cfg!(target_arch = "x86_64") {
            "x64"
        } else if cfg!(target_arch = "aarch64") {
            "arm64"
        } else {
            "x64"
        }
    }

    fn server_binary_name(&self) -> &str {
        if cfg!(windows) {
            "llama-server.exe"
        } else {
            "llama-server"
        }
    }

    /// Fetch available releases from GitHub
    pub async fn fetch_releases(&self) -> Result<Vec<LlamaRelease>, String> {
        let client = reqwest::Client::new();
        let url = "https://api.github.com/repos/ggml-org/llama.cpp/releases?per_page=5";
        let response = client
            .get(url)
            .header("User-Agent", "s2b2s-llama-server-manager")
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
            .map_err(|e| format!("Failed to fetch releases: {}", e))?;

        let releases: Vec<serde_json::Value> = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse releases: {}", e))?;

        let os_key = self.current_os_key();
        let arch_key = self.current_arch_key();
        let mut result = Vec::new();

        for rel in releases {
            let tag = rel["tag_name"].as_str().unwrap_or("").to_string();
            let name = rel["name"].as_str().unwrap_or(&tag).to_string();
            let assets = rel["assets"].as_array().cloned().unwrap_or_default();
            let mut parsed_assets = Vec::new();

            for asset in assets {
                let asset_name = asset["name"].as_str().unwrap_or("").to_string();
                let download_url = asset["browser_download_url"]
                    .as_str()
                    .unwrap_or("")
                    .to_string();
                let size = asset["size"].as_u64().unwrap_or(0);

                if let Some((backend, asset_os, asset_arch)) = parse_asset_name(&asset_name.clone())
                {
                    if asset_os == os_key && asset_arch == arch_key {
                        parsed_assets.push(LlamaAsset {
                            name: asset_name,
                            backend,
                            os: asset_os.to_string(),
                            arch: asset_arch.to_string(),
                            download_url,
                            size_bytes: size,
                        });
                    }
                }
            }

            if !parsed_assets.is_empty() {
                // Deduplicate by backend: keep only the first asset per backend
                // (preferred: CUDA 13 > CUDA 12 > Vulkan > CPU)
                let mut seen = std::collections::HashSet::new();
                parsed_assets.retain(|a| seen.insert(a.backend.clone()));
                result.push(LlamaRelease {
                    tag,
                    name,
                    assets: parsed_assets,
                });
            }
        }

        Ok(result)
    }

    /// Look up the GitHub download URL for a specific backend+tag asset by
    /// fetching releases and matching the OS/arch filters.
    pub async fn find_release_download_url(
        &self,
        backend: &str,
        release_tag: &str,
    ) -> Result<String, String> {
        let releases = self.fetch_releases().await?;
        for rel in &releases {
            if rel.tag == release_tag {
                for asset in &rel.assets {
                    if asset.backend == backend {
                        return Ok(asset.download_url.clone());
                    }
                }
            }
        }
        Err(format!(
            "No release asset found for backend '{backend}' tag '{release_tag}'"
        ))
    }

    /// Download a specific server binary
    pub async fn download_server(
        &self,
        backend: &str,
        release_tag: &str,
        download_url: &str,
    ) -> Result<(), String> {
        let servers_dir = self.servers_dir()?;
        let install_dir = servers_dir.join(format!("{}-{}", backend, release_tag));
        let hub_id = format!("{}-{}", backend, release_tag);
        let hub_name = format!("llama.cpp {backend} {release_tag}");

        // Skip the ~200 MB download entirely when this exact build is already
        // installed — a re-download would only churn identical bytes.
        if install_dir.join(self.server_binary_name()).exists() {
            info!(
                "[LlamaServerManager] {} server {} already installed — skipping download",
                backend, release_tag
            );
            return Ok(());
        }

        // In-flight dedupe: one download per build at a time.
        {
            let mut active = self
                .active_downloads
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if active.contains_key(&hub_id) {
                return Err(format!("A download of {hub_id} is already in progress"));
            }
            active.insert(hub_id.clone(), CancellationToken::new());
        }
        // Take the token back out for the run (registry entry stays as the
        // in-flight marker until the download finishes).
        let cancel = self
            .active_downloads
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&hub_id)
            .cloned();

        let result = match cancel {
            Some(cancel) => {
                self.download_server_inner(
                    backend,
                    release_tag,
                    download_url,
                    &install_dir,
                    &hub_id,
                    &hub_name,
                    &cancel,
                )
                .await
            }
            None => Err("download token lost".to_string()),
        };

        self.active_downloads
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&hub_id);

        match result {
            Ok(crate::model_hub::transport::TransportOutcome::Completed) => {
                crate::model_hub::notify(
                    &self.app,
                    crate::model_hub::ModelCollection::Runtime,
                    &hub_id,
                    &hub_name,
                    crate::model_hub::HubNotificationKind::Completed,
                    None,
                );
                Ok(())
            }
            Ok(crate::model_hub::transport::TransportOutcome::Cancelled) => {
                info!("[LlamaServerManager] Download of {hub_id} cancelled");
                crate::model_hub::notify(
                    &self.app,
                    crate::model_hub::ModelCollection::Runtime,
                    &hub_id,
                    &hub_name,
                    crate::model_hub::HubNotificationKind::Cancelled,
                    None,
                );
                Ok(())
            }
            Err(e) => {
                warn!("[LlamaServerManager] Download of {hub_id} failed: {e}");
                crate::model_hub::notify(
                    &self.app,
                    crate::model_hub::ModelCollection::Runtime,
                    &hub_id,
                    &hub_name,
                    crate::model_hub::HubNotificationKind::Failed,
                    Some(e.clone()),
                );
                Err(e)
            }
        }
    }

    async fn download_server_inner(
        &self,
        backend: &str,
        release_tag: &str,
        download_url: &str,
        install_dir: &Path,
        hub_id: &str,
        hub_name: &str,
        cancel: &CancellationToken,
    ) -> Result<crate::model_hub::transport::TransportOutcome, String> {
        if !install_dir.exists() {
            fs::create_dir_all(install_dir).map_err(|e| format!("Failed to create: {}", e))?;
        }

        info!(
            "[LlamaServerManager] Downloading {} server {} from {}",
            backend, release_tag, download_url
        );

        // Download the archive through the shared hub transport (resumable,
        // stall-guarded, progress events) into a temp dir.
        let temp_dir = std::env::temp_dir().join(format!("s2b2s_llama_dl_{}", release_tag));
        if temp_dir.exists() {
            let _ = fs::remove_dir_all(&temp_dir);
        }
        fs::create_dir_all(&temp_dir).map_err(|e| format!("Failed to create temp dir: {}", e))?;

        let (archive_path, extract): (PathBuf, fn(&Path, &Path) -> Result<(), String>) =
            if download_url.ends_with(".zip") {
                (temp_dir.join("archive.zip"), extract_zip)
            } else if download_url.ends_with(".tar.gz") || download_url.ends_with(".tgz") {
                (temp_dir.join("archive.tar.gz"), extract_tgz)
            } else {
                return Err(format!("Unsupported archive format: {}", download_url));
            };

        let app = self.app.clone();
        let id = hub_id.to_string();
        let name = hub_name.to_string();
        let emit = move |event: crate::model_hub::transport::TransportEvent| {
            if let crate::model_hub::transport::TransportEvent::Progress(p) = event {
                let percent = if p.total > 0 {
                    (p.downloaded as f64 / p.total as f64) * 100.0
                } else {
                    0.0
                };
                crate::model_hub::emit_progress(
                    &app,
                    crate::model_hub::ModelHubDownloadProgress {
                        collection: crate::model_hub::ModelCollection::Runtime,
                        id: id.clone(),
                        name: name.clone(),
                        file: None,
                        downloaded_mb: p.downloaded as f64 / (1024.0 * 1024.0),
                        total_mb: p.total as f64 / (1024.0 * 1024.0),
                        percent,
                        speed_mbps: p.speed_mbps,
                        status: crate::model_hub::HubDownloadStatus::Downloading,
                        error: None,
                    },
                );
            }
        };

        crate::model_hub::transport::download_file_resumable(
            &format!("llama-server {backend}-{release_tag}"),
            download_url,
            &archive_path,
            None,
            cancel,
            &emit,
        )
        .await
        .map_err(|e| e.to_string())?;

        if cancel.is_cancelled() {
            let _ = fs::remove_dir_all(&temp_dir);
            return Ok(crate::model_hub::transport::TransportOutcome::Cancelled);
        }

        extract(&archive_path, &temp_dir)?;

        // Find llama-server binary to verify extraction succeeded
        let binary_name = self.server_binary_name();
        let _server_bin = find_file(&temp_dir, binary_name)
            .ok_or_else(|| format!("{} not found in downloaded archive", binary_name))?;

        // Copy ALL files from extracted archive to install directory
        copy_dir_contents(&temp_dir, install_dir)?;

        // Make executable on Unix
        #[cfg(unix)]
        {
            let dest_bin = install_dir.join(binary_name);
            if dest_bin.exists() {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = fs::metadata(&dest_bin)
                    .map_err(|e| e.to_string())?
                    .permissions();
                perms.set_mode(0o755);
                fs::set_permissions(&dest_bin, perms).map_err(|e| e.to_string())?;
            }
        }

        // Cleanup temp
        let _ = fs::remove_dir_all(&temp_dir);

        info!(
            "[LlamaServerManager] Successfully installed {} server {} to {}",
            backend,
            release_tag,
            install_dir.display()
        );
        Ok(crate::model_hub::transport::TransportOutcome::Completed)
    }

    /// Get path to the currently configured server binary
    pub fn get_active_server_path(&self) -> Result<PathBuf, String> {
        let settings = crate::settings::get_settings(&self.app);
        let config = &settings.llama_server;

        // If configured server exists, use it
        if !config.backend.is_empty() && !config.release_tag.is_empty() {
            let servers_dir = self.servers_dir()?;
            let server_dir = servers_dir.join(format!("{}-{}", config.backend, config.release_tag));
            let binary = server_dir.join(self.server_binary_name());
            if binary.exists() {
                return Ok(binary);
            }
        }

        // Auto-pick: find any installed server, prefer CUDA > Vulkan > CPU
        let installed = self.list_downloaded_servers().unwrap_or_default();
        let preferred_order = ["cuda", "vulkan", "cpu"];
        for backend_prefix in preferred_order {
            for srv in &installed {
                if srv.backend.starts_with(backend_prefix) {
                    let binary = Path::new(&srv.path).join(self.server_binary_name());
                    if binary.exists() {
                        info!(
                            "[LlamaServerManager] Auto-selected {}-{} server",
                            srv.backend, srv.release_tag
                        );
                        return Ok(binary);
                    }
                }
            }
        }

        Err(
            "No llama.cpp server downloaded. Go to Settings > Llama.cpp to download one."
                .to_string(),
        )
    }

    /// List all downloaded servers
    pub fn list_downloaded_servers(&self) -> Result<Vec<DownloadedServer>, String> {
        let servers_dir = self.servers_dir()?;
        let mut servers = Vec::new();

        if !servers_dir.exists() {
            return Ok(servers);
        }

        let binary_name = self.server_binary_name();
        for entry in fs::read_dir(&servers_dir)
            .map_err(|e| e.to_string())?
            .flatten()
        {
            let path = entry.path();
            if path.is_dir() {
                let folder_name = path.file_name().unwrap().to_string_lossy().to_string();
                let binary_path = path.join(binary_name);
                if binary_path.exists() {
                    // Split on LAST hyphen: backend may contain hyphens (e.g. "cuda-13.3"), tag is always suffix (e.g. "b9741")
                    let (backend, tag) = folder_name.rsplit_once('-').unwrap_or((&folder_name, ""));
                    let backend = backend.to_string();
                    let tag = tag.to_string();
                    let size = fs::metadata(&binary_path).map(|m| m.len()).unwrap_or(0);
                    servers.push(DownloadedServer {
                        backend,
                        release_tag: tag,
                        path: path.to_string_lossy().to_string(),
                        size_bytes: size,
                    });
                }
            }
        }

        Ok(servers)
    }

    /// Remove a downloaded server
    pub fn remove_server(&self, backend: &str, release_tag: &str) -> Result<(), String> {
        let servers_dir = self.servers_dir()?;
        let server_dir = servers_dir.join(format!("{}-{}", backend, release_tag));

        // Never delete the install a live llama-server is running from —
        // removing files under a running process breaks it (Windows also
        // refuses to delete a locked executable).
        if let Some(running_dir) = self
            .app
            .try_state::<std::sync::Arc<crate::brain::llama_manager::LlamaManager>>()
            .and_then(|m| m.running_server_dir())
        {
            if running_dir == server_dir {
                return Err(
                    "This server build is currently running. Stop the Brain server before removing it."
                        .to_string(),
                );
            }
        }

        if server_dir.exists() {
            fs::remove_dir_all(&server_dir).map_err(|e| format!("Failed to remove: {}", e))?;
            info!(
                "[LlamaServerManager] Removed server {}-{}",
                backend, release_tag
            );
        }
        Ok(())
    }

    /// Detect GPU type for UI
    pub fn detect_gpu(&self) -> String {
        detect_preferred_backend()
    }
}

fn parse_asset_name(name: &str) -> Option<(String, &str, &str)> {
    let name_lower = name.to_lowercase();

    // Determine OS
    let os = if name_lower.contains("win") {
        "windows"
    } else if name_lower.contains("ubuntu") || name_lower.contains("linux") {
        "linux"
    } else if name_lower.contains("macos") || name_lower.contains("mac") {
        "macos"
    } else {
        return None;
    };

    // Determine arch
    let arch = if name_lower.contains("arm64") || name_lower.contains("aarch64") {
        "arm64"
    } else {
        "x64"
    };

    // Determine backend — include CUDA version for differentiation
    let backend = if name_lower.contains("cuda") || name_lower.contains("cudart") {
        // Extract CUDA version, e.g. "cuda-12.4" or "cuda-13.3"
        let cuda_ver = name_lower
            .split("cuda-")
            .nth(1)
            .and_then(|s| s.split('-').next())
            .unwrap_or("13");
        format!("cuda-{}", cuda_ver)
    } else if name_lower.contains("vulkan") {
        "vulkan".to_string()
    } else {
        "cpu".to_string()
    };

    // Skip cudart-llama variants — they bundle the CUDA runtime separately.
    // The regular llama-b9601-bin-win-cuda-* variants already have CUDA support.
    if name_lower.starts_with("cudart") {
        return None;
    }

    Some((backend, os, arch))
}

fn extract_zip(zip_path: &Path, dest: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        let status = Command::new("powershell")
            .args([
                "-Command",
                &format!(
                    "Expand-Archive -Path '{}' -DestinationPath '{}' -Force",
                    zip_path.display(),
                    dest.display()
                ),
            ])
            .status()
            .map_err(|e| format!("Failed to run Expand-Archive: {}", e))?;
        if !status.success() {
            return Err("Expand-Archive failed".to_string());
        }
    }
    #[cfg(not(windows))]
    {
        let status = Command::new("unzip")
            .args(&[
                "-o",
                &zip_path.to_string_lossy(),
                "-d",
                &dest.to_string_lossy(),
            ])
            .status()
            .map_err(|e| format!("Failed to run unzip: {}", e))?;
        if !status.success() {
            return Err("unzip failed".to_string());
        }
    }
    Ok(())
}

fn extract_tgz(tar_path: &Path, dest: &Path) -> Result<(), String> {
    fs::create_dir_all(dest).map_err(|e| format!("mkdir: {}", e))?;
    let status = Command::new("tar")
        .args([
            "-xzf",
            &tar_path.to_string_lossy(),
            "-C",
            &dest.to_string_lossy(),
        ])
        .status()
        .map_err(|e| format!("Failed to run tar: {}", e))?;
    if !status.success() {
        return Err("tar extract failed".to_string());
    }
    Ok(())
}

fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(found) = find_file(&path, name) {
                    return Some(found);
                }
            } else if path.file_name().map(|n| n == name).unwrap_or(false) {
                return Some(path);
            }
        }
    }
    None
}

fn copy_dir_contents(src: &Path, dest: &Path) -> Result<(), String> {
    fs::create_dir_all(dest).map_err(|e| format!("Failed to create dest dir: {}", e))?;
    if let Ok(entries) = fs::read_dir(src) {
        for entry in entries.flatten() {
            let path = entry.path();
            let dest_path = dest.join(path.file_name().unwrap());
            if path.is_dir() {
                copy_dir_contents(&path, &dest_path)?;
            } else {
                fs::copy(&path, &dest_path)
                    .map_err(|e| format!("Failed to copy {}: {}", path.display(), e))?;
            }
        }
    }
    Ok(())
}
