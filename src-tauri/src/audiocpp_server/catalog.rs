//! Audio.cpp Model Catalog, Variant Management & Background Downloader
//!
//! Provides inspection of model specifications, discovery of on-disk GGUF packages,
//! quantization variants, and async streaming downloads from Hugging Face.

use anyhow::Result;
use futures_util::StreamExt;
use log::{info, warn};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

use super::manager::{resolve_model_specs_dir, resolve_models_dir};
use crate::settings::get_settings;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AudioCppPackageVariant {
    pub id: String,
    pub display_name: String,
    pub precision: String,
    pub format: String,
    pub target_directory: String,
    pub files: Vec<String>,
    pub repo: String,
    pub revision: String,
    pub is_default: bool,
    pub is_downloaded: bool,
    pub is_downloading: bool,
    #[specta(type = u32)]
    pub size_mb: u64,
    pub local_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AudioCppModelFamily {
    pub family: String,
    pub display_name: String,
    pub description: String,
    pub category: String,
    pub tasks: Vec<String>,
    pub modes: Vec<String>,
    pub languages: Vec<String>,
    pub capabilities: Vec<String>,
    pub recommended_package: Option<String>,
    pub packages: Vec<AudioCppPackageVariant>,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct AudioCppDownloadProgress {
    pub package_id: String,
    #[specta(type = f64)]
    pub downloaded_bytes: u64,
    #[specta(type = f64)]
    pub total_bytes: u64,
    pub percent: f32,
    pub speed_mbps: f32,
    pub status: String,
    pub error: Option<String>,
}

use once_cell::sync::Lazy;

static ACTIVE_DOWNLOADS: Lazy<Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>> =
    Lazy::new(|| Arc::new(Mutex::new(HashMap::new())));

/// Approximate package sizes for prominent audio.cpp models (in MB) when remote size query is skipped.
fn approximate_package_size_mb(package_id: &str, precision: &str) -> u64 {
    match package_id {
        id if id.contains("supertonic") => {
            if precision == "f16" {
                313
            } else {
                454
            }
        }
        id if id.contains("pocket") => {
            if precision == "f16" {
                260
            } else {
                130
            }
        }
        id if id.contains("qwen3_tts_1_7b") => {
            if precision == "bf16" || precision == "f16" {
                3400
            } else {
                1800
            }
        }
        id if id.contains("qwen3_tts_0_6b") => 600,
        id if id.contains("chatterbox") => {
            if precision == "f16" {
                3744
            } else {
                2088
            }
        }
        id if id.contains("dots_tts") => 850,
        id if id.contains("moss_tts_nano") => 120,
        id if id.contains("moss_tts_local") => 1600,
        id if id.contains("fish_audio") => 1100,
        id if id.contains("irodori_tts_500m") => 520,
        id if id.contains("irodori_tts_v4") => 350,
        id if id.contains("omnivoice") => 750,
        id if id.contains("voxcpm2") => 890,
        id if id.contains("vibevoice") => 1500,
        id if id.contains("neutts") => 480,
        id if id.contains("miotts") => 1700,
        id if id.contains("index_tts2") => 900,
        id if id.contains("magpie") => 357,
        _ => 500,
    }
}

/// Returns all candidate directories where model weights may reside.
pub fn get_candidate_model_roots(app: &AppHandle) -> Vec<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    vec![
        crate::portable::app_data_dir(app)
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("models"),
        crate::portable::app_data_dir(app)
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("models")
            .join("TTS")
            .join("audiocpp"),
        manifest_dir
            .join("..")
            .join("..")
            .join("audio.cpp")
            .join("models"),
        manifest_dir.join("..").join("models"),
    ]
}

/// Check if a package's primary file exists on disk.
pub fn check_package_installed(
    app: &AppHandle,
    target_dir: &str,
    files: &[String],
    strip_prefix: &str,
) -> Option<PathBuf> {
    let roots = get_candidate_model_roots(app);
    for root in &roots {
        for remote_file in files {
            let relative_file = if !strip_prefix.is_empty() && remote_file.starts_with(strip_prefix)
            {
                remote_file[strip_prefix.len()..]
                    .trim_start_matches('/')
                    .trim_start_matches('\\')
            } else {
                Path::new(remote_file)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(remote_file)
            };

            let candidates = [
                root.join(target_dir).join(relative_file),
                root.join(target_dir).join(remote_file),
                root.join(relative_file),
                root.join(target_dir),
            ];

            for candidate in candidates {
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

/// Parse all available TTS model specifications into rich model family representations.
pub fn get_audiocpp_catalog(app: &AppHandle) -> Result<Vec<AudioCppModelFamily>, String> {
    let specs_dir = resolve_model_specs_dir(app)
        .ok_or_else(|| "Could not locate audio.cpp model_specs directory".to_string())?;

    let settings = get_settings(app);
    let active_model_id = if settings.tts.audiocpp.model.is_empty() {
        "supertonic"
    } else {
        &settings.tts.audiocpp.model
    };

    let downloading_ids: HashSet<String> = {
        let active = ACTIVE_DOWNLOADS.lock().unwrap();
        active.keys().cloned().collect()
    };

    let mut families = Vec::new();

    let entries = fs::read_dir(&specs_dir)
        .map_err(|e| format!("Failed to read model_specs directory: {e}"))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }

        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let json: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let family = json["family"].as_str().unwrap_or("").to_string();
        if family.is_empty() {
            continue;
        }

        let category = json["category"].as_str().unwrap_or("").to_string();
        let tasks: Vec<String> = json["tasks"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        // Only surface TTS / speech synthesis / cloning / design families
        let is_tts_family = category == "tts"
            || tasks
                .iter()
                .any(|t| t == "tts" || t == "clone" || t == "vc" || t == "design");

        if !is_tts_family {
            continue;
        }

        let display_name = json["display_name"].as_str().unwrap_or(&family).to_string();
        let description = json["description"].as_str().unwrap_or("").to_string();
        let modes: Vec<String> = json["modes"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let languages: Vec<String> = json["languages"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let capabilities: Vec<String> = if let Some(cap_obj) = json["capabilities"].as_object() {
            cap_obj.keys().cloned().collect()
        } else {
            Vec::new()
        };

        let recommended_package = json["ui"]["recommended_package"].as_str().map(String::from);

        let default_repo = json["package_defaults"]["download"]["repo"]
            .as_str()
            .unwrap_or("audio-cpp/audio.cpp-gguf")
            .to_string();
        let default_rev = json["package_defaults"]["download"]["revision"]
            .as_str()
            .unwrap_or("main")
            .to_string();

        let mut package_variants = Vec::new();

        if let Some(packages_arr) = json["packages"].as_array() {
            for pkg in packages_arr {
                let pkg_id = pkg["id"].as_str().unwrap_or("").to_string();
                if pkg_id.is_empty() {
                    continue;
                }

                let pkg_name = pkg["display_name"].as_str().unwrap_or(&pkg_id).to_string();
                let precision = pkg["precision"].as_str().unwrap_or("q8_0").to_string();
                let format = pkg["format"].as_str().unwrap_or("gguf").to_string();
                let target_dir = pkg["target_directory"]
                    .as_str()
                    .unwrap_or(&family)
                    .to_string();
                let strip_prefix = pkg["strip_prefix"].as_str().unwrap_or("").to_string();
                let is_default = pkg["default"].as_bool().unwrap_or(false);

                let files: Vec<String> = pkg["files"]
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();

                let repo = pkg["download"]["repo"]
                    .as_str()
                    .unwrap_or(&default_repo)
                    .to_string();
                let revision = pkg["download"]["revision"]
                    .as_str()
                    .unwrap_or(&default_rev)
                    .to_string();

                let local_path = check_package_installed(app, &target_dir, &files, &strip_prefix);
                let is_downloaded = local_path.is_some();
                let is_downloading = downloading_ids.contains(&pkg_id);
                let size_mb = approximate_package_size_mb(&pkg_id, &precision);

                package_variants.push(AudioCppPackageVariant {
                    id: pkg_id,
                    display_name: pkg_name,
                    precision,
                    format,
                    target_directory: target_dir,
                    files,
                    repo,
                    revision,
                    is_default,
                    is_downloaded,
                    is_downloading,
                    size_mb,
                    local_path: local_path.map(|p| p.to_string_lossy().to_string()),
                });
            }
        }

        let is_active = family == active_model_id;

        families.push(AudioCppModelFamily {
            family,
            display_name,
            description,
            category,
            tasks,
            modes,
            languages,
            capabilities,
            recommended_package,
            packages: package_variants,
            is_active,
        });
    }

    // Sort families alphabetically with popular ones first
    families.sort_by(|a, b| {
        let rank = |fam: &str| match fam {
            "supertonic" => 1,
            "qwen3_tts" => 2,
            "pocket_tts" => 3,
            "chatterbox" => 4,
            "dots_tts" => 5,
            "moss_tts_local" | "moss_tts_nano" => 6,
            "fish_audio" => 7,
            "irodori_tts" => 8,
            "omnivoice" => 9,
            "voxcpm2" => 10,
            _ => 50,
        };
        rank(&a.family)
            .cmp(&rank(&b.family))
            .then_with(|| a.display_name.cmp(&b.display_name))
    });

    Ok(families)
}

/// Download a model package variant in the background with live progress updates.
pub fn start_package_download(app: AppHandle, package_id: String) -> Result<(), String> {
    let catalog = get_audiocpp_catalog(&app)?;

    let mut target_pkg: Option<AudioCppPackageVariant> = None;
    for fam in &catalog {
        for pkg in &fam.packages {
            if pkg.id == package_id {
                target_pkg = Some(pkg.clone());
                break;
            }
        }
        if target_pkg.is_some() {
            break;
        }
    }

    let pkg = target_pkg.ok_or_else(|| format!("Package '{package_id}' not found in catalog"))?;

    // Verify if already downloading
    let cancel_token = Arc::new(AtomicBool::new(false));
    {
        let mut active = ACTIVE_DOWNLOADS.lock().unwrap();
        if active.contains_key(&package_id) {
            return Err(format!("Package '{package_id}' is already downloading"));
        }
        active.insert(package_id.clone(), cancel_token.clone());
    }

    let app_clone = app.clone();
    let pkg_id_clone = package_id.clone();

    tauri::async_runtime::spawn(async move {
        let result = download_package_task(&app_clone, &pkg, &cancel_token).await;

        // Cleanup active downloads
        {
            let mut active = ACTIVE_DOWNLOADS.lock().unwrap();
            active.remove(&pkg_id_clone);
        }

        match result {
            Ok(()) => {
                info!(
                    "[AudioCppDownload] Package '{}' downloaded successfully",
                    pkg_id_clone
                );
                let _ = app_clone.emit(
                    "audiocpp-download-progress",
                    AudioCppDownloadProgress {
                        package_id: pkg_id_clone,
                        downloaded_bytes: 100,
                        total_bytes: 100,
                        percent: 100.0,
                        speed_mbps: 0.0,
                        status: "completed".to_string(),
                        error: None,
                    },
                );
            }
            Err(err) => {
                let is_cancelled = cancel_token.load(Ordering::SeqCst);
                let status = if is_cancelled { "canceled" } else { "error" };
                warn!(
                    "[AudioCppDownload] Package '{}' finished with {status}: {err}",
                    pkg_id_clone
                );
                let _ = app_clone.emit(
                    "audiocpp-download-progress",
                    AudioCppDownloadProgress {
                        package_id: pkg_id_clone,
                        downloaded_bytes: 0,
                        total_bytes: 0,
                        percent: 0.0,
                        speed_mbps: 0.0,
                        status: status.to_string(),
                        error: Some(err),
                    },
                );
            }
        }
    });

    Ok(())
}

async fn download_package_task(
    app: &AppHandle,
    pkg: &AudioCppPackageVariant,
    cancel_token: &Arc<AtomicBool>,
) -> Result<(), String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {e}"))?;

    let destination_root = resolve_models_dir(app);
    let target_dir = destination_root.join(&pkg.target_directory);
    fs::create_dir_all(&target_dir)
        .map_err(|e| format!("Failed to create directory {}: {e}", target_dir.display()))?;

    for remote_file in &pkg.files {
        if cancel_token.load(Ordering::SeqCst) {
            return Err("Download canceled by user".to_string());
        }

        let filename = Path::new(remote_file)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(remote_file);
        let dest_file = target_dir.join(filename);
        let temp_file = target_dir.join(format!("{filename}.download"));

        let url = format!(
            "https://huggingface.co/{}/resolve/{}/{}",
            pkg.repo, pkg.revision, remote_file
        );

        info!(
            "[AudioCppDownload] Downloading '{}' from {} -> {}",
            pkg.id,
            url,
            dest_file.display()
        );

        let resp = client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("HTTP request failed for {url}: {e}"))?;

        if !resp.status().is_success() {
            return Err(format!("Failed to download {url}: HTTP {}", resp.status()));
        }

        let total_size = resp.content_length().unwrap_or(pkg.size_mb * 1024 * 1024);
        let mut downloaded: u64 = 0;
        let start_time = Instant::now();
        let mut last_emit = Instant::now();

        let mut file = File::create(&temp_file)
            .map_err(|e| format!("Failed to create temp file {}: {e}", temp_file.display()))?;

        let mut stream = resp.bytes_stream();
        while let Some(chunk_res) = stream.next().await {
            if cancel_token.load(Ordering::SeqCst) {
                let _ = fs::remove_file(&temp_file);
                return Err("Download canceled by user".to_string());
            }

            let chunk = chunk_res.map_err(|e| format!("Error reading response stream: {e}"))?;
            file.write_all(&chunk)
                .map_err(|e| format!("Error writing chunk: {e}"))?;
            downloaded += chunk.len() as u64;

            if last_emit.elapsed() >= Duration::from_millis(200) {
                let elapsed_secs = start_time.elapsed().as_secs_f32();
                let speed_mbps = if elapsed_secs > 0.0 {
                    (downloaded as f32 / (1024.0 * 1024.0)) / elapsed_secs
                } else {
                    0.0
                };
                let percent = if total_size > 0 {
                    (downloaded as f32 / total_size as f32) * 100.0
                } else {
                    0.0
                };

                let _ = app.emit(
                    "audiocpp-download-progress",
                    AudioCppDownloadProgress {
                        package_id: pkg.id.clone(),
                        downloaded_bytes: downloaded,
                        total_bytes: total_size,
                        percent: percent.clamp(0.0, 99.9),
                        speed_mbps,
                        status: "downloading".to_string(),
                        error: None,
                    },
                );
                last_emit = Instant::now();
            }
        }

        file.flush()
            .map_err(|e| format!("Failed to flush temp file: {e}"))?;
        drop(file);

        // Atomically rename temp file to final destination
        if dest_file.exists() {
            let _ = fs::remove_file(&dest_file);
        }
        fs::rename(&temp_file, &dest_file)
            .map_err(|e| format!("Failed to rename temp file to destination: {e}"))?;
    }

    Ok(())
}

/// Cancel an ongoing download.
pub fn cancel_package_download(package_id: &str) -> bool {
    let active = ACTIVE_DOWNLOADS.lock().unwrap();
    if let Some(token) = active.get(package_id) {
        token.store(true, Ordering::SeqCst);
        true
    } else {
        false
    }
}

/// Delete a downloaded package from disk.
pub fn delete_package(app: &AppHandle, package_id: &str) -> Result<(), String> {
    let catalog = get_audiocpp_catalog(app)?;
    for fam in catalog {
        for pkg in fam.packages {
            if pkg.id == package_id {
                if let Some(path_str) = pkg.local_path {
                    let path = PathBuf::from(path_str);
                    if path.is_file() {
                        fs::remove_file(&path).map_err(|e| {
                            format!("Failed to delete file {}: {e}", path.display())
                        })?;
                        info!("[AudioCppCatalog] Deleted package file: {}", path.display());
                        return Ok(());
                    } else if path.is_dir() {
                        fs::remove_dir_all(&path).map_err(|e| {
                            format!("Failed to delete directory {}: {e}", path.display())
                        })?;
                        info!("[AudioCppCatalog] Deleted package dir: {}", path.display());
                        return Ok(());
                    }
                }
            }
        }
    }
    Err(format!("Package '{package_id}' not found or not on disk"))
}
