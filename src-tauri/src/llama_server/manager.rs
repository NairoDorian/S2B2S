use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::{AppHandle};
use log::info;
use specta::Type;

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
    pub backend: String,      // "cuda", "vulkan", "cpu"
    pub release_tag: String,  // e.g. "b9601"
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
        if let Ok(output) = Command::new("nvidia-smi").arg("--query-gpu=name").arg("--format=csv,noheader").output() {
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
            if output.status.success() { return "cuda".to_string(); }
        }
        if Path::new("/usr/local/cuda").exists() { return "cuda".to_string(); }
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
}

impl LlamaServerManager {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
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
        if cfg!(target_os = "windows") { "windows" }
        else if cfg!(target_os = "linux") { "linux" }
        else { "macos" }
    }

    fn current_arch_key(&self) -> &str {
        if cfg!(target_arch = "x86_64") { "x64" }
        else if cfg!(target_arch = "aarch64") { "arm64" }
        else { "x64" }
    }

    fn server_binary_name(&self) -> &str {
        if cfg!(windows) { "llama-server.exe" } else { "llama-server" }
    }

    /// Fetch available releases from GitHub
    pub async fn fetch_releases(&self) -> Result<Vec<LlamaRelease>, String> {
        let client = reqwest::Client::new();
        let url = "https://api.github.com/repos/ggml-org/llama.cpp/releases?per_page=5";
        let response = client.get(url)
            .header("User-Agent", "s2b2s-llama-server-manager")
            .header("Accept", "application/vnd.github+json")
            .send().await
            .map_err(|e| format!("Failed to fetch releases: {}", e))?;

        let releases: Vec<serde_json::Value> = response.json().await
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
                let download_url = asset["browser_download_url"].as_str().unwrap_or("").to_string();
                let size = asset["size"].as_u64().unwrap_or(0);

                if let Some((backend, asset_os, asset_arch)) = parse_asset_name(&asset_name.clone()) {
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
                result.push(LlamaRelease { tag, name, assets: parsed_assets });
            }
        }

        Ok(result)
    }

    /// Download a specific server binary
    pub async fn download_server(&self, backend: &str, release_tag: &str, download_url: &str) -> Result<(), String> {
        let servers_dir = self.servers_dir()?;
        let install_dir = servers_dir.join(format!("{}-{}", backend, release_tag));
        if !install_dir.exists() {
            fs::create_dir_all(&install_dir).map_err(|e| format!("Failed to create: {}", e))?;
        }

        // Download the archive
        info!("[LlamaServerManager] Downloading {} server {} from {}", backend, release_tag, download_url);
        let client = reqwest::Client::new();
        let response = client.get(download_url)
            .header("User-Agent", "s2b2s-llama-server-manager")
            .send().await
            .map_err(|e| format!("Download failed: {}", e))?;

        let bytes = response.bytes().await
            .map_err(|e| format!("Download read failed: {}", e))?;

        // For now, write to a temp file and extract
        let temp_dir = std::env::temp_dir().join(format!("s2b2s_llama_dl_{}", release_tag));
        if temp_dir.exists() { let _ = fs::remove_dir_all(&temp_dir); }
        fs::create_dir_all(&temp_dir).map_err(|e| format!("Failed to create temp dir: {}", e))?;

        if download_url.ends_with(".zip") {
            let zip_path = temp_dir.join("archive.zip");
            fs::write(&zip_path, &bytes).map_err(|e| format!("Failed to write zip: {}", e))?;
            extract_zip(&zip_path, &temp_dir)?;
        } else if download_url.ends_with(".tar.gz") || download_url.ends_with(".tgz") {
            let tar_path = temp_dir.join("archive.tar.gz");
            fs::write(&tar_path, &bytes).map_err(|e| format!("Failed to write tar: {}", e))?;
            extract_tgz(&tar_path, &temp_dir)?;
        } else {
            return Err(format!("Unsupported archive format: {}", download_url));
        }

        // Find llama-server binary to verify extraction succeeded
        let binary_name = self.server_binary_name();
        let _server_bin = find_file(&temp_dir, binary_name)
            .ok_or_else(|| format!("{} not found in downloaded archive", binary_name))?;

        // Copy ALL files from extracted archive to install directory
        copy_dir_contents(&temp_dir, &install_dir)?;

        // Make executable on Unix
        #[cfg(unix)]
        {
            let dest_bin = install_dir.join(binary_name);
            if dest_bin.exists() {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = fs::metadata(&dest_bin).map_err(|e| e.to_string())?.permissions();
                perms.set_mode(0o755);
                fs::set_permissions(&dest_bin, perms).map_err(|e| e.to_string())?;
            }
        }

        // Cleanup temp
        let _ = fs::remove_dir_all(&temp_dir);

        info!("[LlamaServerManager] Successfully installed {} server {} to {}", backend, release_tag, install_dir.display());
        Ok(())
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
                        info!("[LlamaServerManager] Auto-selected {}-{} server", srv.backend, srv.release_tag);
                        return Ok(binary);
                    }
                }
            }
        }

        Err("No llama.cpp server downloaded. Go to Settings > Llama.cpp to download one.".to_string())
    }

    /// List all downloaded servers
    pub fn list_downloaded_servers(&self) -> Result<Vec<DownloadedServer>, String> {
        let servers_dir = self.servers_dir()?;
        let mut servers = Vec::new();

        if !servers_dir.exists() {
            return Ok(servers);
        }

        let binary_name = self.server_binary_name();
        for entry in fs::read_dir(&servers_dir).map_err(|e| e.to_string())?.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let folder_name = path.file_name().unwrap().to_string_lossy().to_string();
                let binary_path = path.join(binary_name);
                if binary_path.exists() {
                    let parts: Vec<&str> = folder_name.splitn(2, '-').collect();
                    let backend = parts.first().unwrap_or(&"").to_string();
                    let tag = parts.get(1).unwrap_or(&"").to_string();
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
        if server_dir.exists() {
            fs::remove_dir_all(&server_dir).map_err(|e| format!("Failed to remove: {}", e))?;
            info!("[LlamaServerManager] Removed server {}-{}", backend, release_tag);
        }
        Ok(())
    }

    /// Check if the configured server has GPU support
    pub fn has_gpu_support(&self) -> bool {
        let settings = crate::settings::get_settings(&self.app);
        matches!(settings.llama_server.backend.as_str(), "cuda" | "vulkan")
    }

    /// Detect GPU type for UI
    pub fn detect_gpu(&self) -> String {
        detect_preferred_backend()
    }
}

fn parse_asset_name(name: &str) -> Option<(String, &str, &str)> {
    let name_lower = name.to_lowercase();

    // Determine OS
    let os = if name_lower.contains("win") { "windows" }
        else if name_lower.contains("ubuntu") || name_lower.contains("linux") { "linux" }
        else if name_lower.contains("macos") { "macos" }
        else if name_lower.contains("mac") { "macos" }
        else { return None };

    // Determine arch
    let arch = if name_lower.contains("arm64") || name_lower.contains("aarch64") { "arm64" }
        else { "x64" };

    // Determine backend — include CUDA version for differentiation
    let backend = if name_lower.contains("cuda") || name_lower.contains("cudart") {
        // Extract CUDA version, e.g. "cuda-12.4" or "cuda-13.3"
        let cuda_ver = name_lower.split("cuda-").nth(1)
            .and_then(|s| s.split('-').next())
            .unwrap_or("13");
        format!("cuda-{}", cuda_ver)
    } else if name_lower.contains("vulkan") {
        "vulkan".to_string()
    } else if name_lower.contains("cpu") || name_lower.contains("opencl") || name_lower.contains("hip") || name_lower.contains("rocm") || name_lower.contains("openvino") {
        "cpu".to_string()
    } else if !name_lower.contains("cuda") && !name_lower.contains("vulkan") {
        "cpu".to_string()
    } else {
        return None;
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
            .args(&["-Command", &format!("Expand-Archive -Path '{}' -DestinationPath '{}' -Force", zip_path.display(), dest.display())])
            .status()
            .map_err(|e| format!("Failed to run Expand-Archive: {}", e))?;
        if !status.success() {
            return Err("Expand-Archive failed".to_string());
        }
    }
    #[cfg(not(windows))]
    {
        let status = Command::new("unzip")
            .args(&["-o", &zip_path.to_string_lossy(), "-d", &dest.to_string_lossy()])
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
        .args(&["-xzf", &tar_path.to_string_lossy(), "-C", &dest.to_string_lossy()])
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
                fs::copy(&path, &dest_path).map_err(|e| format!("Failed to copy {}: {}", path.display(), e))?;
            }
        }
    }
    Ok(())
}
