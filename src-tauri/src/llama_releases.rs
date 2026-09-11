//! llama.cpp release discovery, download and install — `download-llama_v2.ps1`
//! inside the app.
//!
//! Releases come from the GitHub API (cached for ten minutes: the tab's
//! refresh and every install would otherwise burn the 60 req/h unauthenticated
//! budget). Asset selection follows the script: an exact backend match, then
//! the fallbacks (any CUDA → Vulkan → CPU). SemVer releases that carry no
//! binaries are resolved to their backing nightly build through the
//! `nightly-tag.txt` asset. Archives are streamed to disk with progress
//! events and unpacked with the `zip` crate — no PowerShell, no console
//! window — into `<app data>/llama_cpp/<backend>-<tag>/`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use log::{info, warn};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;
use tauri_specta::Event;

const REPO: &str = "ggml-org/llama.cpp";
const USER_AGENT: &str = "Handy-llama-manager/1.0";
const CACHE_TTL: Duration = Duration::from_secs(600);
const PROGRESS_INTERVAL: Duration = Duration::from_millis(150);

#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct LlamaReleaseAsset {
    pub name: String,
    pub size_bytes: f64,
    pub url: String,
    /// `cuda-13.3`, `cuda-12.4`, `vulkan`, `cpu`, … parsed from the name.
    pub backend: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct LlamaRelease {
    pub tag: String,
    pub name: String,
    pub published_at: String,
    pub prerelease: bool,
    /// `b<number>` of this release or of the nightly it points to.
    pub build_number: u32,
    /// Windows x64 binary assets (no cudart packages).
    pub assets: Vec<LlamaReleaseAsset>,
    /// For SemVer releases without binaries: the nightly tag that has them.
    pub backing_tag: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct InstalledLlamaServer {
    pub dir: String,
    pub name: String,
    pub backend: String,
    pub tag: String,
    pub has_server: bool,
    pub size_mb: u32,
    /// Size of a bundled CUDA runtime (cudart/cuBLAS DLLs), 0 when none.
    pub cuda_runtime_mb: u32,
}

/// Progress of one install, from download start to done/error.
#[derive(Serialize, Deserialize, Debug, Clone, Type, tauri_specta::Event)]
pub struct LlamaDownloadEvent {
    pub tag: String,
    pub backend: String,
    /// `downloading`, `extracting`, `done`, `error`
    pub phase: String,
    pub downloaded_bytes: f64,
    pub total_bytes: f64,
    pub message: Option<String>,
    /// Install folder, once `done`.
    pub dir: Option<String>,
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    size: u64,
    browser_download_url: String,
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

static CACHE: Mutex<Option<(Instant, Vec<LlamaRelease>)>> = Mutex::new(None);

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(Duration::from_secs(15))
        .build()
        .expect("reqwest client")
}

fn build_number(release: &GhRelease) -> u32 {
    let from = |s: &str| -> Option<u32> {
        let idx = s.find(|c: char| c == 'b')?;
        let digits: String = s[idx + 1..]
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        digits.parse().ok()
    };
    if let Some(n) = release
        .tag_name
        .strip_prefix('b')
        .and_then(|d| d.parse().ok())
    {
        return n;
    }
    if let Some(body) = &release.body {
        if let Some(pos) = body.find("releases/tag/b") {
            if let Some(n) = from(&body[pos + "releases/tag/".len()..]) {
                return n;
            }
        }
    }
    release.name.as_deref().and_then(from).unwrap_or(0)
}

fn parse_backend(asset_name: &str) -> Option<String> {
    let lower = asset_name.to_lowercase();
    if lower.starts_with("cudart") || !lower.ends_with(".zip") || !lower.contains("-bin-win-") {
        return None;
    }
    let after = lower.split("-bin-win-").nth(1)?;
    // e.g. "cuda-13.3-x64.zip", "vulkan-x64.zip", "cpu-x64.zip"
    let backend = after.trim_end_matches(".zip").trim_end_matches("-x64");
    if backend.contains("arm64") {
        return None;
    }
    Some(backend.to_string())
}

fn to_release(release: &GhRelease) -> LlamaRelease {
    let assets = release
        .assets
        .iter()
        .filter_map(|a| {
            let backend = parse_backend(&a.name)?;
            Some(LlamaReleaseAsset {
                name: a.name.clone(),
                size_bytes: a.size as f64,
                url: a.browser_download_url.clone(),
                backend,
            })
        })
        .collect::<Vec<_>>();
    let backing_tag = if assets.is_empty() {
        release
            .body
            .as_deref()
            .and_then(|b| {
                let pos = b.find("releases/tag/b")?;
                let rest = &b[pos + "releases/tag/".len()..];
                let tag: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric())
                    .collect();
                (tag.len() > 1).then_some(tag)
            })
            .or_else(|| {
                release
                    .assets
                    .iter()
                    .any(|a| a.name == "nightly-tag.txt")
                    .then(|| format!("nightly-tag:{}", release.tag_name))
            })
    } else {
        None
    };
    LlamaRelease {
        tag: release.tag_name.clone(),
        name: release
            .name
            .clone()
            .unwrap_or_else(|| release.tag_name.clone()),
        published_at: release.published_at.clone().unwrap_or_default(),
        prerelease: release.prerelease,
        build_number: build_number(release),
        assets,
        backing_tag,
    }
}

/// Recent releases, newest build first. `channel`: `latest` (highest build
/// number), `stable` (SemVer tags first), `nightly` (b-tags only).
pub async fn fetch_releases(channel: &str, force: bool) -> Result<Vec<LlamaRelease>, String> {
    let cached = if force {
        None
    } else {
        CACHE
            .lock()
            .unwrap()
            .as_ref()
            .filter(|(at, _)| at.elapsed() < CACHE_TTL)
            .map(|(_, list)| list.clone())
    };
    let mut releases = match cached {
        Some(list) => list,
        None => {
            let url = format!("https://api.github.com/repos/{REPO}/releases?per_page=30");
            let list: Vec<GhRelease> = client()
                .get(&url)
                .header("Accept", "application/vnd.github+json")
                .send()
                .await
                .map_err(|e| format!("GitHub request failed: {e}"))?
                .error_for_status()
                .map_err(|e| format!("GitHub API error: {e}"))?
                .json()
                .await
                .map_err(|e| format!("Unexpected GitHub response: {e}"))?;
            let parsed: Vec<LlamaRelease> = list.iter().map(to_release).collect();
            *CACHE.lock().unwrap() = Some((Instant::now(), parsed.clone()));
            parsed
        }
    };
    match channel {
        "stable" => {
            releases.retain(|r| !r.prerelease);
            releases.sort_by(|a, b| {
                let sa = a.tag.starts_with('v');
                let sb = b.tag.starts_with('v');
                sb.cmp(&sa).then(b.build_number.cmp(&a.build_number))
            });
        }
        "nightly" => {
            releases.retain(|r| r.tag.starts_with('b'));
            releases.sort_by(|a, b| b.build_number.cmp(&a.build_number));
        }
        _ => releases.sort_by(|a, b| b.build_number.cmp(&a.build_number)),
    }
    Ok(releases)
}

async fn fetch_release_by_tag(tag: &str) -> Result<LlamaRelease, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/tags/{tag}");
    let release: GhRelease = client()
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("GitHub request failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("Release {tag} not found: {e}"))?
        .json()
        .await
        .map_err(|e| format!("Unexpected GitHub response: {e}"))?;
    Ok(to_release(&release))
}

/// The asset to install for `backend`, with the script's fallback order.
pub fn pick_asset<'a>(release: &'a LlamaRelease, backend: &str) -> Option<&'a LlamaReleaseAsset> {
    let order: Vec<&str> = match backend {
        "cuda-13.3" => vec!["cuda-13.3", "cuda-", "vulkan", "cpu"],
        "cuda-12.4" => vec!["cuda-12.4", "cuda-", "vulkan", "cpu"],
        "vulkan" => vec!["vulkan", "cpu"],
        "cpu" => vec!["cpu"],
        other => vec![other, "vulkan", "cpu"],
    };
    for wanted in order {
        if let Some(a) = release.assets.iter().find(|a| {
            if wanted.ends_with('-') {
                a.backend.starts_with(wanted)
            } else {
                a.backend == wanted
            }
        }) {
            return Some(a);
        }
    }
    None
}

/// `cuda-13.3` / `cuda-12.4` from `nvidia-smi`, else `cpu`. Cached.
pub fn detect_backend() -> String {
    static DETECTED: Mutex<Option<String>> = Mutex::new(None);
    if let Some(b) = DETECTED.lock().unwrap().clone() {
        return b;
    }
    let detected = detect_backend_uncached();
    *DETECTED.lock().unwrap() = Some(detected.clone());
    detected
}

fn detect_backend_uncached() -> String {
    let mut cmd = std::process::Command::new("nvidia-smi");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let Ok(output) = cmd.output() else {
        return "cpu".into();
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let Some(pos) = text.find("CUDA Version:") else {
        return if output.status.success() {
            "cuda-13.3".into()
        } else {
            "cpu".into()
        };
    };
    let version: String = text[pos + "CUDA Version:".len()..]
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let mut parts = version.split('.');
    let major: u32 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    let minor: u32 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    if major > 13 || (major == 13 && minor >= 3) {
        "cuda-13.3".into()
    } else if major >= 12 {
        "cuda-12.4".into()
    } else {
        "cpu".into()
    }
}

pub fn servers_root(app: &AppHandle) -> Result<PathBuf, String> {
    crate::portable::app_data_dir(app)
        .map(|d| d.join("llama_cpp"))
        .map_err(|e| e.to_string())
}

pub fn list_installed(app: &AppHandle) -> Vec<InstalledLlamaServer> {
    let Ok(root) = servers_root(app) else {
        return vec![];
    };
    let mut out: Vec<InstalledLlamaServer> = std::fs::read_dir(&root)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter(|e| {
            let n = e.file_name().to_string_lossy().to_string();
            n != "downloads" && !n.ends_with(".extracting")
        })
        .map(|e| {
            let dir = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            let (backend, tag) = name
                .rsplit_once('-')
                .map(|(b, t)| (b.to_string(), t.to_string()))
                .unwrap_or((name.clone(), String::new()));
            let server_name = if cfg!(windows) {
                "llama-server.exe"
            } else {
                "llama-server"
            };
            InstalledLlamaServer {
                has_server: dir.join(server_name).is_file(),
                size_mb: dir_size_mb(&dir),
                cuda_runtime_mb: bundled_cuda_runtime_mb(&dir.to_string_lossy()),
                dir: dir.to_string_lossy().to_string(),
                name,
                backend,
                tag,
            }
        })
        .collect();
    out.sort_by(|a, b| b.name.cmp(&a.name));
    out
}

fn dir_size_mb(dir: &Path) -> u32 {
    let bytes: u64 = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.metadata().ok())
        .filter(|m| m.is_file())
        .map(|m| m.len())
        .sum();
    (bytes / 1_048_576) as u32
}

/// Download and unpack `tag` for `backend` (or the detected one for `auto`).
/// The CUDA runtime package (`cudart-*.zip`, ~500 MB of cuBLAS DLLs) is only
/// fetched when `include_cudart` is set — the same opt-in as the download
/// script's `-IncludeCudart`; a machine with the CUDA toolkit installed does
/// not need it.
pub async fn install(
    app: &AppHandle,
    tag: &str,
    backend: &str,
    include_cudart: bool,
) -> Result<InstalledLlamaServer, String> {
    let backend = if backend == "auto" {
        detect_backend()
    } else {
        backend.to_string()
    };
    let emit =
        |phase: &str, downloaded: f64, total: f64, message: Option<String>, dir: Option<String>| {
            let _ = LlamaDownloadEvent {
                tag: tag.to_string(),
                backend: backend.clone(),
                phase: phase.into(),
                downloaded_bytes: downloaded,
                total_bytes: total,
                message,
                dir,
            }
            .emit(app);
        };

    let result = install_inner(app, tag, &backend, include_cudart, &emit).await;
    match &result {
        Ok(installed) => emit("done", 1.0, 1.0, None, Some(installed.dir.clone())),
        Err(e) => emit("error", 0.0, 0.0, Some(e.clone()), None),
    }
    result
}

async fn install_inner(
    app: &AppHandle,
    tag: &str,
    backend: &str,
    include_cudart: bool,
    emit: &(dyn Fn(&str, f64, f64, Option<String>, Option<String>) + Sync),
) -> Result<InstalledLlamaServer, String> {
    // Resolve the release that actually carries binaries.
    let mut release = fetch_release_by_tag(tag).await?;
    if release.assets.is_empty() {
        let backing = match release.backing_tag.clone() {
            Some(t) if t.starts_with("nightly-tag:") => {
                let url = format!(
                    "https://github.com/{REPO}/releases/download/{}/nightly-tag.txt",
                    release.tag
                );
                let mut resolved = None;
                if let Ok(response) = client().get(&url).send().await {
                    if let Ok(response) = response.error_for_status() {
                        if let Ok(text) = response.text().await {
                            let text = text.trim().to_string();
                            if !text.is_empty() {
                                resolved = Some(text);
                            }
                        }
                    }
                }
                resolved
            }
            other => other,
        };
        let backing = backing.ok_or_else(|| {
            format!("Release {tag} has no Windows binaries and no backing nightly build")
        })?;
        info!("Release {tag} resolves to nightly build {backing}");
        release = fetch_release_by_tag(&backing).await?;
    }
    let asset = pick_asset(&release, backend)
        .ok_or_else(|| {
            format!(
                "No Windows x64 asset for backend {backend} in {}",
                release.tag
            )
        })?
        .clone();
    let cudart = (include_cudart && asset.backend.starts_with("cuda")).then(|| {
        let ver = asset.backend.trim_start_matches("cuda-");
        format!("cudart-llama-bin-win-cuda-{ver}-x64.zip")
    });

    let root = servers_root(app)?;
    let downloads = root.join("downloads");
    std::fs::create_dir_all(&downloads).map_err(|e| e.to_string())?;
    let install_name = format!("{}-{}", asset.backend, release.tag);
    let final_dir = root.join(&install_name);
    let extracting_dir = root.join(format!("{install_name}.extracting"));

    // Download the main archive with progress.
    let zip_path = downloads.join(&asset.name);
    download_with_progress(&asset.url, &zip_path, asset.size_bytes, emit).await?;

    // Optional cudart runtime next to it (best effort).
    let cudart_path = if let Some(name) = &cudart {
        let url = format!(
            "https://github.com/{REPO}/releases/download/{}/{name}",
            release.tag
        );
        let path = downloads.join(name);
        match download_with_progress(&url, &path, 0.0, emit).await {
            Ok(()) => Some(path),
            Err(e) => {
                warn!(
                    "cudart package not downloaded ({e}); the CUDA build may need the toolkit installed"
                );
                None
            }
        }
    } else {
        None
    };

    emit("extracting", 0.0, 0.0, Some(asset.name.clone()), None);
    let _ = std::fs::remove_dir_all(&extracting_dir);
    std::fs::create_dir_all(&extracting_dir).map_err(|e| e.to_string())?;
    let extract_root = extracting_dir.clone();
    let zip_for_extract = zip_path.clone();
    let cudart_for_extract = cudart_path.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        unzip_flat(&zip_for_extract, &extract_root)?;
        if let Some(c) = cudart_for_extract {
            unzip_flat(&c, &extract_root)?;
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())??;

    let server_name = if cfg!(windows) {
        "llama-server.exe"
    } else {
        "llama-server"
    };
    if !extracting_dir.join(server_name).is_file() {
        let _ = std::fs::remove_dir_all(&extracting_dir);
        return Err(format!("{server_name} not found in {}", asset.name));
    }
    let _ = std::fs::remove_dir_all(&final_dir);
    std::fs::rename(&extracting_dir, &final_dir).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&zip_path);
    if let Some(c) = cudart_path {
        let _ = std::fs::remove_file(c);
    }
    info!(
        "Installed llama.cpp {} into {}",
        release.tag,
        final_dir.display()
    );

    Ok(InstalledLlamaServer {
        has_server: true,
        size_mb: dir_size_mb(&final_dir),
        cuda_runtime_mb: bundled_cuda_runtime_mb(&final_dir.to_string_lossy()),
        dir: final_dir.to_string_lossy().to_string(),
        name: install_name,
        backend: asset.backend,
        tag: release.tag,
    })
}

async fn download_with_progress(
    url: &str,
    dest: &Path,
    expected_total: f64,
    emit: &(dyn Fn(&str, f64, f64, Option<String>, Option<String>) + Sync),
) -> Result<(), String> {
    let response = client()
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Download failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("Download failed: {e}"))?;
    let total = response
        .content_length()
        .map(|n| n as f64)
        .unwrap_or(expected_total);
    // Plain std file: the chunks are large and sequential, and this task runs
    // on the async pool, not on a thread anything latency-sensitive shares.
    let mut file = std::io::BufWriter::new(
        std::fs::File::create(dest).map_err(|e| format!("Cannot write {}: {e}", dest.display()))?,
    );
    let mut stream = response.bytes_stream();
    let mut downloaded = 0f64;
    let mut last_emit = Instant::now() - PROGRESS_INTERVAL;
    let name = dest
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    emit("downloading", 0.0, total, Some(name.clone()), None);
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Download interrupted: {e}"))?;
        std::io::Write::write_all(&mut file, &chunk).map_err(|e| e.to_string())?;
        downloaded += chunk.len() as f64;
        if last_emit.elapsed() >= PROGRESS_INTERVAL {
            last_emit = Instant::now();
            emit("downloading", downloaded, total, Some(name.clone()), None);
        }
    }
    std::io::Write::flush(&mut file).map_err(|e| e.to_string())?;
    emit(
        "downloading",
        downloaded,
        total.max(downloaded),
        Some(name),
        None,
    );
    Ok(())
}

/// Unpack `zip_path` into `dest`, flattening a single top-level folder so
/// `llama-server.exe` always lands directly in `dest`.
fn unzip_flat(zip_path: &Path, dest: &Path) -> Result<(), String> {
    let file = std::fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("Bad archive: {e}"))?;
    // Detect a common top-level directory.
    let mut prefixes: HashMap<String, usize> = HashMap::new();
    for i in 0..archive.len() {
        let entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().replace('\\', "/");
        let first = name.split('/').next().unwrap_or("").to_string();
        if name.contains('/') {
            *prefixes.entry(first).or_default() += 1;
        } else {
            *prefixes.entry(String::new()).or_default() += 1;
        }
    }
    let strip = (prefixes.len() == 1)
        .then(|| prefixes.keys().next().cloned())
        .flatten()
        .filter(|p| !p.is_empty());

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let raw = entry.name().replace('\\', "/");
        let rel = match &strip {
            Some(p) => raw
                .strip_prefix(&format!("{p}/"))
                .unwrap_or(&raw)
                .to_string(),
            None => raw.clone(),
        };
        if rel.is_empty() || rel.contains("..") {
            continue;
        }
        let out_path = dest.join(&rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out_path).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut out = std::fs::File::create(&out_path).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// DLL name prefixes that make up the `cudart` runtime package.
const CUDA_RUNTIME_DLL_PREFIXES: &[&str] = &["cudart64_", "cublas64_", "cublaslt64_"];

/// A system CUDA toolkit whose runtime DLLs a CUDA build can load instead of
/// the bundled `cudart` package.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Type)]
pub struct CudaToolkitInfo {
    /// Folder that holds `cudart64_*.dll` (CUDA 13 keeps it in `bin\x64`,
    /// CUDA 12 and older in `bin`).
    pub runtime_dir: String,
    /// Toolkit version as the installer names it (`13.3`), when known.
    pub version: Option<String>,
    /// Whether `runtime_dir` is on the PATH Handy was started with. When it is
    /// not, `LlamaServerManager::start` prepends it to the child's PATH.
    pub on_path: bool,
    /// Whether cuBLAS (`cublas64_*` + `cublasLt64_*`) sits next to cudart —
    /// llama.cpp's CUDA backend needs both.
    pub has_cublas: bool,
}

fn is_cudart_file(name: &str) -> bool {
    let n = name.to_lowercase();
    (n.starts_with("cudart64_") && n.ends_with(".dll")) || n.starts_with("libcudart.so")
}

fn dir_has_cudart(dir: &Path) -> bool {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .any(|e| is_cudart_file(&e.file_name().to_string_lossy()))
}

fn dir_has_cublas(dir: &Path) -> bool {
    let names: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_lowercase())
        .collect();
    let has = |p: &str| names.iter().any(|n| n.starts_with(p));
    (has("cublas64_") && has("cublaslt64_")) || (has("libcublas.so") && has("libcublaslt.so"))
}

/// `v13.3` / `CUDA\v12.8` → `13.3`; the `CUDA_PATH_V13_3` env name → `13.3`.
fn cuda_version_from_dir(dir: &Path) -> Option<String> {
    dir.ancestors().find_map(|p| {
        let name = p.file_name()?.to_string_lossy();
        let rest = name.strip_prefix('v')?;
        let ok = !rest.is_empty()
            && rest.chars().all(|c| c.is_ascii_digit() || c == '.')
            && rest.chars().next().is_some_and(|c| c.is_ascii_digit());
        ok.then(|| rest.to_string())
    })
}

fn path_entries() -> Vec<PathBuf> {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default()
}

fn same_dir(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| {
        p.to_string_lossy()
            .trim_end_matches(['\\', '/'])
            .to_lowercase()
            .replace('/', "\\")
    };
    norm(a) == norm(b)
}

/// Locate the CUDA runtime of an installed toolkit. Looked up, in order: the
/// `CUDA_PATH` root (and every `CUDA_PATH_V*`, newest first), each PATH
/// entry, then the default install folder — so a toolkit is found even when
/// the installer's environment variables are missing or stale. Inside a
/// root both `bin\x64` (CUDA 13) and `bin` (CUDA ≤ 12) are checked.
pub fn detect_cuda_toolkit() -> Option<CudaToolkitInfo> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(p) = std::env::var_os("CUDA_PATH") {
        roots.push(PathBuf::from(p));
    }
    let mut versioned: Vec<(String, PathBuf)> = std::env::vars_os()
        .filter_map(|(k, v)| {
            let k = k.to_string_lossy().to_string();
            k.starts_with("CUDA_PATH_V").then(|| (k, PathBuf::from(v)))
        })
        .collect();
    versioned.sort_by(|a, b| b.0.cmp(&a.0));
    roots.extend(versioned.into_iter().map(|(_, p)| p));
    #[cfg(windows)]
    {
        let base = std::env::var_os("ProgramFiles")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Program Files"))
            .join("NVIDIA GPU Computing Toolkit")
            .join("CUDA");
        let mut versions: Vec<PathBuf> = std::fs::read_dir(&base)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        versions.sort();
        versions.reverse();
        roots.extend(versions);
    }
    #[cfg(not(windows))]
    roots.push(PathBuf::from("/usr/local/cuda"));

    let path = path_entries();
    let mut candidates: Vec<PathBuf> = Vec::new();
    for root in &roots {
        candidates.push(root.join("bin").join("x64"));
        for sub in ["bin", "lib64", "lib"] {
            candidates.push(root.join(sub));
        }
    }
    candidates.extend(path.iter().cloned());

    let dir = candidates.into_iter().find(|c| dir_has_cudart(c))?;
    Some(CudaToolkitInfo {
        on_path: path.iter().any(|p| same_dir(p, &dir)),
        has_cublas: dir_has_cublas(&dir),
        version: cuda_version_from_dir(&dir).or_else(|| {
            std::fs::read_dir(&dir)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.file_name().to_string_lossy().to_lowercase())
                .find_map(|n| {
                    n.strip_prefix("cudart64_")?
                        .strip_suffix(".dll")
                        .map(str::to_string)
                })
        }),
        runtime_dir: dir.to_string_lossy().to_string(),
    })
}

/// Megabytes taken by a bundled CUDA runtime inside `dir`, 0 when absent.
pub fn bundled_cuda_runtime_mb(dir: &str) -> u32 {
    let bytes: u64 = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| {
            let n = e.file_name().to_string_lossy().to_lowercase();
            n.ends_with(".dll") && CUDA_RUNTIME_DLL_PREFIXES.iter().any(|p| n.starts_with(p))
        })
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum();
    (bytes / 1_048_576) as u32
}

/// Remove the bundled CUDA runtime DLLs from an install made by Handy — the
/// ~500 MB a CUDA build carries when the toolkit already provides them.
pub fn remove_bundled_cuda_runtime(app: &AppHandle, dir: &str) -> Result<u32, String> {
    let root = servers_root(app)?;
    let path = PathBuf::from(dir);
    if !path.starts_with(&root) {
        return Err("Only installs made by Handy can be trimmed here".into());
    }
    let freed = bundled_cuda_runtime_mb(dir);
    for entry in std::fs::read_dir(&path)
        .map_err(|e| e.to_string())?
        .flatten()
    {
        let n = entry.file_name().to_string_lossy().to_lowercase();
        if n.ends_with(".dll") && CUDA_RUNTIME_DLL_PREFIXES.iter().any(|p| n.starts_with(p)) {
            std::fs::remove_file(entry.path()).map_err(|e| e.to_string())?;
        }
    }
    info!("Removed bundled CUDA runtime from {dir} ({freed} MB)");
    Ok(freed)
}

/// Delete an installed server folder. Refuses the folder the settings point at.
pub fn remove_installed(app: &AppHandle, dir: &str) -> Result<(), String> {
    let root = servers_root(app)?;
    let path = PathBuf::from(dir);
    if !path.starts_with(&root) {
        return Err("Only installs made by Handy can be removed here".into());
    }
    let settings = crate::settings::get_settings(app);
    if settings.llama.server_dir.as_deref() == Some(dir) {
        return Err("This install is the active server; pick another one first".into());
    }
    std::fs::remove_dir_all(&path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cuda_version_is_read_from_the_toolkit_folder_name() {
        let p = Path::new(r"C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v13.3\bin\x64");
        assert_eq!(cuda_version_from_dir(p).as_deref(), Some("13.3"));
        let p = Path::new(r"C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v12.8\bin");
        assert_eq!(cuda_version_from_dir(p).as_deref(), Some("12.8"));
        assert_eq!(cuda_version_from_dir(Path::new(r"C:\tools\cuda\bin")), None);
        // "vulkan" must not parse as a version
        assert_eq!(cuda_version_from_dir(Path::new(r"C:\vulkan\bin")), None);
    }

    #[test]
    fn same_dir_ignores_case_separators_and_trailing_slashes() {
        assert!(same_dir(
            Path::new(r"C:\CUDA\v13.3\bin\x64\"),
            Path::new("c:/cuda/v13.3/BIN/x64")
        ));
        assert!(!same_dir(
            Path::new(r"C:\CUDA\bin"),
            Path::new(r"C:\CUDA\bin\x64")
        ));
    }

    /// Diagnostic: `cargo test -- --ignored --nocapture print_detected_cuda_toolkit`
    /// prints what this machine's detection resolves to.
    #[test]
    #[ignore]
    fn print_detected_cuda_toolkit() {
        println!("{:#?}", detect_cuda_toolkit());
    }

    #[test]
    fn cudart_file_names() {
        assert!(is_cudart_file("cudart64_13.dll"));
        assert!(is_cudart_file("CUDART64_12.DLL"));
        assert!(is_cudart_file("libcudart.so.12"));
        assert!(!is_cudart_file("cudart_static.lib"));
        assert!(!is_cudart_file("ggml-cuda.dll"));
    }

    #[test]
    fn parses_windows_asset_backends() {
        assert_eq!(
            parse_backend("llama-b10630-bin-win-cuda-13.3-x64.zip").as_deref(),
            Some("cuda-13.3")
        );
        assert_eq!(
            parse_backend("llama-b10630-bin-win-vulkan-x64.zip").as_deref(),
            Some("vulkan")
        );
        assert_eq!(
            parse_backend("llama-b10630-bin-win-cpu-x64.zip").as_deref(),
            Some("cpu")
        );
        assert_eq!(
            parse_backend("cudart-llama-bin-win-cuda-13.3-x64.zip"),
            None
        );
        assert_eq!(
            parse_backend("llama-b10630-bin-win-cuda-13.3-arm64.zip"),
            None
        );
        assert_eq!(parse_backend("llama-b10630-bin-ubuntu-x64.tar.gz"), None);
    }

    #[test]
    fn picks_with_script_fallback_order() {
        let mk = |b: &str| LlamaReleaseAsset {
            name: b.into(),
            size_bytes: 0.0,
            url: String::new(),
            backend: b.into(),
        };
        let release = LlamaRelease {
            tag: "b1".into(),
            name: "b1".into(),
            published_at: String::new(),
            prerelease: true,
            build_number: 1,
            assets: vec![mk("cpu"), mk("vulkan"), mk("cuda-12.4")],
            backing_tag: None,
        };
        assert_eq!(
            pick_asset(&release, "cuda-13.3").unwrap().backend,
            "cuda-12.4"
        );
        assert_eq!(
            pick_asset(&release, "cuda-12.4").unwrap().backend,
            "cuda-12.4"
        );
        assert_eq!(pick_asset(&release, "vulkan").unwrap().backend, "vulkan");
        assert_eq!(pick_asset(&release, "cpu").unwrap().backend, "cpu");
    }
}
