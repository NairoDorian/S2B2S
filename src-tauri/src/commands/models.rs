//! Tauri commands for STT model management (list, download, delete, VRAM).

use crate::managers::model::{ModelInfo, ModelManager};
use crate::managers::transcription::{ModelStateEvent, TranscriptionManager};
use crate::settings::{get_settings, write_settings, ModelUnloadTimeout};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, State};

#[tauri::command]
#[specta::specta]
pub async fn get_available_models(
    model_manager: State<'_, Arc<ModelManager>>,
) -> Result<Vec<ModelInfo>, String> {
    Ok(model_manager.get_available_models())
}

#[tauri::command]
#[specta::specta]
pub async fn get_model_info(
    model_manager: State<'_, Arc<ModelManager>>,
    model_id: String,
) -> Result<Option<ModelInfo>, String> {
    Ok(model_manager.get_model_info(&model_id))
}

/// Re-scan local sources (custom models dir + shared HF cache) for models added
/// since launch
#[tauri::command]
#[specta::specta]
pub async fn rescan_local_models(
    model_manager: State<'_, Arc<ModelManager>>,
) -> Result<(), String> {
    let mm = model_manager.inner().clone();
    tokio::task::spawn_blocking(move || mm.rescan_local_models())
        .await
        .map_err(|e| format!("rescan task panicked: {e}"))?
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn download_model(
    app_handle: AppHandle,
    model_manager: State<'_, Arc<ModelManager>>,
    model_id: String,
) -> Result<(), String> {
    let result = model_manager
        .download_model(&model_id)
        .await
        .map_err(|e| e.to_string());

    if let Err(ref error) = result {
        let _ = app_handle.emit(
            "model-download-failed",
            serde_json::json!({ "model_id": &model_id, "error": error }),
        );
    }

    result
}

/// Whether any STT model is known to the app (built-in catalog or discovered).
#[tauri::command]
#[specta::specta]
pub async fn has_any_models_available(
    model_manager: State<'_, Arc<ModelManager>>,
) -> Result<bool, String> {
    Ok(!model_manager.get_available_models().is_empty())
}

/// Whether any STT model is already on disk (downloaded or discovered locally).
#[tauri::command]
#[specta::specta]
pub async fn has_any_models_or_downloads(
    model_manager: State<'_, Arc<ModelManager>>,
) -> Result<bool, String> {
    Ok(model_manager
        .get_available_models()
        .iter()
        .any(|m| m.is_downloaded || m.is_downloading))
}

#[tauri::command]
#[specta::specta]
pub async fn delete_model(
    app_handle: AppHandle,
    model_manager: State<'_, Arc<ModelManager>>,
    transcription_manager: State<'_, Arc<TranscriptionManager>>,
    model_id: String,
) -> Result<(), String> {
    // If deleting the active model, unload it and clear the setting
    let settings = get_settings(&app_handle);
    if settings.selected_model == model_id {
        transcription_manager
            .unload_model()
            .map_err(|e| format!("Failed to unload model: {}", e))?;

        let mut settings = get_settings(&app_handle);
        settings.selected_model = String::new();
        write_settings(&app_handle, settings);
    }

    model_manager
        .delete_model(&model_id)
        .map_err(|e| e.to_string())
}

/// Shared logic for switching the active model, used by both the Tauri command
/// and the tray menu handler.
///
/// Validates the model, updates the persisted setting, and loads the model
/// unless the unload timeout is set to "Immediately" (in which case the model
/// will be loaded on-demand during the next transcription).
pub fn switch_active_model(app: &AppHandle, model_id: &str) -> Result<(), String> {
    let model_manager = app.state::<Arc<ModelManager>>();
    let transcription_manager = app.state::<Arc<TranscriptionManager>>();

    // Atomically claim the loading slot — prevents concurrent model loads
    // from tray double-clicks or overlapping commands. The guard resets the
    // flag on drop (including early returns, errors, and panics).
    let _loading_guard = transcription_manager
        .try_start_loading()
        .ok_or_else(|| "Model load already in progress".to_string())?;

    // Check if model exists and is available
    let model_info = model_manager
        .get_model_info(model_id)
        .ok_or_else(|| format!("Model not found: {}", model_id))?;

    if !model_info.is_downloaded {
        return Err(format!("Model not downloaded: {}", model_id));
    }

    let settings = get_settings(app);
    let unload_timeout = settings.model_unload_timeout;
    let old_model = settings.selected_model.clone();
    let old_onboarding_completed = settings.onboarding_completed;

    // Persist the new selection early so the frontend sees the correct model
    // when it reacts to events emitted by load_model.
    let mut settings = settings;
    settings.selected_model = model_id.to_string();
    settings.onboarding_completed = true;

    write_settings(app, settings);

    // Skip eager loading if unload is set to "Immediately" — the model
    // will be loaded on-demand during the next transcription.
    if unload_timeout == ModelUnloadTimeout::Immediately {
        // Notify frontend — load_model won't be called so no events
        // would otherwise be emitted.
        let _ = app.emit(
            "model-state-changed",
            ModelStateEvent {
                event_type: "selection_changed".to_string(),
                model_id: Some(model_id.to_string()),
                model_name: Some(model_info.name.clone()),
                error: None,
            },
        );
        log::info!(
            "Model selection changed to {} (not loading — unload set to Immediately).",
            model_id
        );
        return Ok(());
    }

    // Load the model. On failure, revert the persisted selection.
    if let Err(e) = transcription_manager.load_model(model_id) {
        let mut settings = get_settings(app);
        settings.selected_model = old_model;
        settings.onboarding_completed = old_onboarding_completed;
        write_settings(app, settings);
        return Err(e.to_string());
    }

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn set_active_model(
    app_handle: AppHandle,
    _model_manager: State<'_, Arc<ModelManager>>,
    _transcription_manager: State<'_, Arc<TranscriptionManager>>,
    model_id: String,
) -> Result<(), String> {
    switch_active_model(&app_handle, &model_id)
}

#[tauri::command]
#[specta::specta]
pub async fn get_current_model(app_handle: AppHandle) -> Result<String, String> {
    let settings = get_settings(&app_handle);
    Ok(settings.selected_model)
}

#[tauri::command]
#[specta::specta]
pub async fn get_transcription_model_status(
    transcription_manager: State<'_, Arc<TranscriptionManager>>,
) -> Result<Option<String>, String> {
    Ok(transcription_manager.get_current_model())
}

#[tauri::command]
#[specta::specta]
pub async fn is_model_loading(
    transcription_manager: State<'_, Arc<TranscriptionManager>>,
) -> Result<bool, String> {
    // Report the real in-progress load flag. (Previously returned
    // `current_model.is_none()`, which was inverted — it said "loading" whenever
    // no model was loaded and "ready" while a load was actually running.)
    Ok(transcription_manager.is_loading())
}

#[tauri::command]
#[specta::specta]
pub async fn cancel_download(
    model_manager: State<'_, Arc<ModelManager>>,
    model_id: String,
) -> Result<(), String> {
    model_manager
        .cancel_download(&model_id)
        .map_err(|e| e.to_string())
}

#[derive(serde::Serialize, specta::Type)]
pub struct LlmServerInfo {
    pub name: String,
    pub pid: u32,
}

#[derive(serde::Serialize, specta::Type)]
pub struct GpuVramStatus {
    pub is_supported: bool,
    pub adapter_name: Option<String>,
    /// Total dedicated VRAM on the GPU in MB
    pub total_vram_mb: u32,
    /// System-wide VRAM currently in use (matches Task Manager)
    pub used_vram_mb: u32,
    /// System-wide free VRAM
    pub free_vram_mb: u32,
    /// This process's VRAM budget usage (from DXGI)
    pub process_used_mb: u32,
    /// This process's VRAM budget
    pub process_budget_mb: u32,
    /// Detected LLM server processes consuming VRAM
    pub llm_servers: Vec<LlmServerInfo>,
    /// Unix ms timestamp of when this snapshot was taken
    pub updated_at_unix_ms: f64,
    pub error: Option<String>,
}

fn unix_ms_now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as f64
}

#[cfg(target_os = "windows")]
fn wide_to_string(wide: &[u16]) -> String {
    let end = wide.iter().position(|&ch| ch == 0).unwrap_or(wide.len());
    String::from_utf16_lossy(&wide[..end]).trim().to_string()
}

#[cfg(target_os = "windows")]
fn nt_success(status: windows::Win32::Foundation::NTSTATUS) -> bool {
    status.0 >= 0
}

#[cfg(target_os = "windows")]
struct ActiveGpuVramSnapshot {
    adapter_name: String,
    adapter_luid: windows::Win32::Foundation::LUID,
    process_used_bytes: u64,
    process_budget_bytes: u64,
    total_vram_bytes: u64,
}

#[cfg(target_os = "windows")]
fn query_system_gpu_usage_bytes(adapter_luid: windows::Win32::Foundation::LUID) -> Option<u64> {
    use windows::Wdk::Graphics::Direct3D::{
        D3DKMTCloseAdapter, D3DKMTOpenAdapterFromLuid, D3DKMTQueryVideoMemoryInfo,
        D3DKMT_CLOSEADAPTER, D3DKMT_MEMORY_SEGMENT_GROUP_LOCAL, D3DKMT_OPENADAPTERFROMLUID,
        D3DKMT_QUERYVIDEOMEMORYINFO,
    };
    use windows::Win32::Foundation::HANDLE;

    unsafe {
        let mut open = D3DKMT_OPENADAPTERFROMLUID {
            AdapterLuid: adapter_luid,
            ..Default::default()
        };
        let open_status = D3DKMTOpenAdapterFromLuid(&mut open);
        if !nt_success(open_status) {
            log::warn!(
                "[VRAM] D3DKMTOpenAdapterFromLuid failed: NTSTATUS=0x{:x}",
                open_status.0
            );
            return None;
        }
        if open.hAdapter == 0 {
            log::warn!("[VRAM] D3DKMTOpenAdapterFromLuid returned null adapter handle");
            return None;
        }

        let mut query = D3DKMT_QUERYVIDEOMEMORYINFO {
            hProcess: HANDLE(std::ptr::null_mut()),
            hAdapter: open.hAdapter,
            MemorySegmentGroup: D3DKMT_MEMORY_SEGMENT_GROUP_LOCAL,
            PhysicalAdapterIndex: 0,
            ..Default::default()
        };
        let query_status = D3DKMTQueryVideoMemoryInfo(&mut query);

        let _ = D3DKMTCloseAdapter(&D3DKMT_CLOSEADAPTER {
            hAdapter: open.hAdapter,
        });

        if !nt_success(query_status) {
            log::warn!(
                "[VRAM] D3DKMTQueryVideoMemoryInfo failed: NTSTATUS=0x{:x}",
                query_status.0
            );
            return None;
        }

        let usage_mb = query.CurrentUsage / (1024 * 1024);
        log::info!(
            "[VRAM] D3DKMT system-wide VRAM: {} MB (CurrentUsage={}, Budget={})",
            usage_mb,
            query.CurrentUsage,
            query.Budget
        );
        Some(query.CurrentUsage)
    }
}

#[cfg(target_os = "windows")]
fn query_active_gpu_vram() -> Result<ActiveGpuVramSnapshot, String> {
    use windows::core::Interface;
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory1, IDXGIAdapter1, IDXGIAdapter3, IDXGIFactory1, IDXGIFactory6,
        DXGI_ADAPTER_FLAG_SOFTWARE, DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE,
        DXGI_MEMORY_SEGMENT_GROUP_LOCAL, DXGI_QUERY_VIDEO_MEMORY_INFO,
    };

    unsafe {
        // Try IDXGIFactory6 first (Windows 8.1+), fall back to IDXGIFactory1
        let factory1 = match CreateDXGIFactory1::<IDXGIFactory6>() {
            Ok(f6) => {
                // log::debug!("[VRAM] Created IDXGIFactory6 successfully");
                f6.cast::<IDXGIFactory1>()
                    .map_err(|e| format!("Failed to cast factory6 -> factory1: {e}"))?
            }
            Err(e) => {
                log::warn!(
                    "[VRAM] Could not create IDXGIFactory6 (falling back to IDXGIFactory1): {e}"
                );
                CreateDXGIFactory1::<IDXGIFactory1>()
                    .map_err(|e| format!("Failed to create IDXGIFactory1: {e}"))?
            }
        };

        // Try to upgrade to IDXGIFactory6 for GPU preference enumeration
        let factory6 = factory1.cast::<IDXGIFactory6>().ok();

        let mut best: Option<ActiveGpuVramSnapshot> = None;
        let mut adapter_index = 0u32;

        // Try to use GPU preference enumeration if available
        if let Some(ref f6) = factory6 {
            // log::debug!("[VRAM] Using EnumAdapterByGpuPreference for adapter enumeration");
            loop {
                let adapter: IDXGIAdapter1 = match f6
                    .EnumAdapterByGpuPreference(adapter_index, DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE)
                {
                    Ok(adapter) => adapter,
                    Err(_) => break,
                };
                adapter_index += 1;

                let desc = match adapter.GetDesc1() {
                    Ok(desc) => desc,
                    Err(_) => continue,
                };

                if (desc.Flags & (DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32)) != 0 {
                    // log::debug!(
                    //     "[VRAM] Skipping software adapter at index {}",
                    //     adapter_index - 1
                    // );
                    continue;
                }

                let adapter3: IDXGIAdapter3 = match adapter.cast::<IDXGIAdapter3>() {
                    Ok(adapter3) => adapter3,
                    Err(_) => continue,
                };

                let mut memory_info = DXGI_QUERY_VIDEO_MEMORY_INFO::default();
                if adapter3
                    .QueryVideoMemoryInfo(0, DXGI_MEMORY_SEGMENT_GROUP_LOCAL, &mut memory_info)
                    .is_err()
                {
                    continue;
                }

                let total_vram = if desc.DedicatedVideoMemory > 0 {
                    desc.DedicatedVideoMemory as u64
                } else {
                    memory_info.Budget
                };
                let used = memory_info.CurrentUsage;
                let budget = if memory_info.Budget > 0 {
                    memory_info.Budget
                } else {
                    total_vram
                };
                let adapter_name = wide_to_string(&desc.Description);
                let adapter_name = if adapter_name.is_empty() {
                    format!("GPU {}", adapter_index - 1)
                } else {
                    adapter_name
                };

                // log::debug!(
                //     "[VRAM] Adapter {}: {} - Total: {} MB, Used: {} MB",
                //     adapter_index - 1,
                //     adapter_name,
                //     total_vram / (1024 * 1024),
                //     used / (1024 * 1024)
                // );

                let should_replace = match best {
                    None => true,
                    Some(ref best_snapshot) => {
                        total_vram > best_snapshot.total_vram_bytes
                            || (total_vram == best_snapshot.total_vram_bytes
                                && used > best_snapshot.process_used_bytes)
                    }
                };

                if should_replace {
                    best = Some(ActiveGpuVramSnapshot {
                        adapter_name,
                        adapter_luid: desc.AdapterLuid,
                        process_used_bytes: used,
                        process_budget_bytes: budget,
                        total_vram_bytes: total_vram,
                    });
                }
            }
        } else {
            // Fallback: use basic EnumAdapters1
            // log::debug!("[VRAM] Using EnumAdapters1 (no GPU preference support)");
            loop {
                let adapter = match factory1.EnumAdapters1(adapter_index) {
                    Ok(adapter) => adapter,
                    Err(_) => break,
                };
                adapter_index += 1;

                let desc = match adapter.GetDesc1() {
                    Ok(desc) => desc,
                    Err(_) => continue,
                };

                if (desc.Flags & (DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32)) != 0 {
                    // log::debug!(
                    //     "[VRAM] Skipping software adapter at index {}",
                    //     adapter_index - 1
                    // );
                    continue;
                }

                let total_vram = desc.DedicatedVideoMemory as u64;
                let adapter_name = wide_to_string(&desc.Description);
                let adapter_name = if adapter_name.is_empty() {
                    format!("GPU {}", adapter_index - 1)
                } else {
                    adapter_name
                };

                // Try to get memory info via IDXGIAdapter3
                let (used, budget) = match adapter.cast::<IDXGIAdapter3>() {
                    Ok(adapter3) => {
                        let mut mi = DXGI_QUERY_VIDEO_MEMORY_INFO::default();
                        if adapter3
                            .QueryVideoMemoryInfo(0, DXGI_MEMORY_SEGMENT_GROUP_LOCAL, &mut mi)
                            .is_ok()
                        {
                            (
                                mi.CurrentUsage,
                                if mi.Budget > 0 { mi.Budget } else { total_vram },
                            )
                        } else {
                            (0u64, total_vram)
                        }
                    }
                    Err(_) => (0u64, total_vram),
                };

                // log::debug!(
                //     "[VRAM] Adapter {}: {} - Total: {} MB, Used: {} MB",
                //     adapter_index - 1,
                //     adapter_name,
                //     total_vram / (1024 * 1024),
                //     used / (1024 * 1024)
                // );

                let should_replace = match best {
                    None => true,
                    Some(ref best_snapshot) => total_vram > best_snapshot.total_vram_bytes,
                };

                if should_replace {
                    best = Some(ActiveGpuVramSnapshot {
                        adapter_name,
                        adapter_luid: desc.AdapterLuid,
                        process_used_bytes: used,
                        process_budget_bytes: budget,
                        total_vram_bytes: total_vram,
                    });
                }
            }
        }

        let snapshot = best.ok_or_else(|| "No active hardware GPU adapter detected".to_string())?;
        // log::debug!(
        //     "[VRAM] Selected adapter: {} - Total: {} MB, Used: {} MB, Budget: {} MB",
        //     snapshot.adapter_name,
        //     snapshot.total_vram_bytes / (1024 * 1024),
        //     snapshot.process_used_bytes / (1024 * 1024),
        //     snapshot.process_budget_bytes / (1024 * 1024)
        // );
        Ok(snapshot)
    }
}

#[cfg(target_os = "windows")]
fn query_system_gpu_usage_bytes_fallback() -> Option<u64> {
    // Try nvidia-smi first (most accurate, works with any GPU API)
    if let Some(usage) = get_nvidia_vram_usage() {
        return Some(usage);
    }
    // Fallback: DXGI process usage (may be 0 for apps using CUDA/ORT)
    let (_, usage) = get_best_adapter_dxgi_info()?;
    Some(usage)
}

#[cfg(target_os = "windows")]
fn get_total_vram_fallback() -> Option<u64> {
    // Try nvidia-smi first
    if let Some(total) = get_nvidia_total_vram() {
        return Some(total);
    }
    // Fallback: DXGI adapter DedicatedVideoMemory
    let (total, _) = get_best_adapter_dxgi_info()?;
    Some(total)
}

#[cfg(target_os = "windows")]
fn get_best_adapter_dxgi_info() -> Option<(u64, u64)> {
    // Returns (total_vram_bytes, current_usage_bytes) for the best adapter
    use windows::core::Interface;
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory1, IDXGIAdapter3, IDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE,
        DXGI_MEMORY_SEGMENT_GROUP_LOCAL, DXGI_QUERY_VIDEO_MEMORY_INFO,
    };

    unsafe {
        let factory1 = CreateDXGIFactory1::<IDXGIFactory1>().ok()?;
        let mut best_total = 0u64;
        let mut best_usage = 0u64;

        for i in 0.. {
            let adapter = match factory1.EnumAdapters1(i) {
                Ok(a) => a,
                Err(_) => break,
            };
            let desc = adapter.GetDesc1().ok()?;
            if (desc.Flags & (DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32)) != 0 {
                continue;
            }
            let total = desc.DedicatedVideoMemory as u64;
            if total > best_total {
                best_total = total;
                if let Ok(adapter3) = adapter.cast::<IDXGIAdapter3>() {
                    let mut mi = DXGI_QUERY_VIDEO_MEMORY_INFO::default();
                    if adapter3
                        .QueryVideoMemoryInfo(0, DXGI_MEMORY_SEGMENT_GROUP_LOCAL, &mut mi)
                        .is_ok()
                    {
                        best_usage = mi.CurrentUsage;
                    }
                }
            }
        }
        if best_total > 0 {
            Some((best_total, best_usage))
        } else {
            None
        }
    }
}

#[cfg(target_os = "windows")]
fn get_nvidia_vram_usage() -> Option<u64> {
    use std::process::Command;
    let output = Command::new("nvidia-smi")
        .args([
            "--query-gpu=memory.used,memory.total",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let first_line = stdout.lines().next()?;
    let parts: Vec<&str> = first_line.split(',').map(|s| s.trim()).collect();
    if parts.len() < 2 {
        return None;
    }
    let used_mb: u64 = parts[0].parse().ok()?;
    // log::debug!("[VRAM] nvidia-smi used: {} MB", used_mb);
    Some(used_mb * 1024 * 1024)
}

#[cfg(target_os = "windows")]
fn get_nvidia_total_vram() -> Option<u64> {
    use std::process::Command;
    let output = Command::new("nvidia-smi")
        .args(["--query-gpu=memory.total", "--format=csv,noheader,nounits"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let total_mb: u64 = stdout.trim().parse().ok()?;
    Some(total_mb * 1024 * 1024)
}

#[cfg(target_os = "windows")]
fn detect_llm_servers() -> Vec<LlmServerInfo> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };

    let llm_process_names: &[(&str, &str)] = &[
        ("ollama.exe", "Ollama"),
        ("lms.exe", "LM Studio"),
        ("llama-server.exe", "llama.cpp"),
        ("llama-cli.exe", "llama.cpp"),
        ("server.exe", "llama.cpp"),
    ];

    let mut servers: Vec<LlmServerInfo> = Vec::new();

    unsafe {
        let snapshot = match CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
            Ok(snapshot) => snapshot,
            Err(_) => return servers,
        };

        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };

        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                let exe_name = OsString::from_wide(&entry.szExeFile)
                    .to_string_lossy()
                    .to_lowercase();

                if let Some(&(_, display_name)) = llm_process_names
                    .iter()
                    .find(|&&(name, _)| exe_name == name)
                {
                    let pid = entry.th32ProcessID;

                    if !servers.iter().any(|s| s.name == display_name) {
                        servers.push(LlmServerInfo {
                            name: display_name.to_string(),
                            pid,
                        });
                    }
                }

                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }

        let _ = CloseHandle(snapshot);
    }

    servers
}

#[cfg(not(target_os = "windows"))]
fn detect_llm_servers() -> Vec<LlmServerInfo> {
    Vec::new()
}

#[tauri::command]
#[specta::specta]
pub fn get_active_gpu_vram_status() -> Result<GpuVramStatus, String> {
    let updated_at_unix_ms = unix_ms_now();
    let llm_servers = detect_llm_servers();

    #[cfg(target_os = "windows")]
    {
        // System-wide usage: nvidia-smi first (true GPU memory), then D3DKMT, then DXGI fallback
        let nvidia_usage = get_nvidia_vram_usage();
        let nvidia_total = get_nvidia_total_vram();

        match query_active_gpu_vram() {
            Ok(snapshot) => {
                let system_used_bytes = nvidia_usage
                    .or_else(|| query_system_gpu_usage_bytes(snapshot.adapter_luid))
                    .or_else(query_system_gpu_usage_bytes_fallback)
                    .unwrap_or(snapshot.process_used_bytes);

                let system_total_bytes = nvidia_total.unwrap_or(snapshot.total_vram_bytes);

                let system_free_bytes = system_total_bytes.saturating_sub(system_used_bytes);

                // log::debug!(
                //     "[VRAM] Final: Adapter={}, Total={}MB, Used={}MB, Free={}MB, Process={}/{}MB",
                //     snapshot.adapter_name,
                //     system_total_bytes / (1024 * 1024),
                //     system_used_bytes / (1024 * 1024),
                //     system_free_bytes / (1024 * 1024),
                //     snapshot.process_used_bytes / (1024 * 1024),
                //     snapshot.process_budget_bytes / (1024 * 1024)
                // );

                Ok(GpuVramStatus {
                    is_supported: true,
                    adapter_name: Some(snapshot.adapter_name),
                    total_vram_mb: (system_total_bytes / (1024 * 1024)) as u32,
                    used_vram_mb: (system_used_bytes / (1024 * 1024)) as u32,
                    free_vram_mb: (system_free_bytes / (1024 * 1024)) as u32,
                    process_used_mb: (snapshot.process_used_bytes / (1024 * 1024)) as u32,
                    process_budget_mb: (snapshot.process_budget_bytes / (1024 * 1024)) as u32,
                    llm_servers,
                    updated_at_unix_ms,
                    error: None,
                })
            }
            Err(error) => {
                log::error!("[VRAM] Query failed: {error}");
                if let Some(usage) = nvidia_usage.or_else(query_system_gpu_usage_bytes_fallback) {
                    let total = nvidia_total
                        .or_else(get_total_vram_fallback)
                        .unwrap_or(usage);
                    let free = total.saturating_sub(usage);
                    return Ok(GpuVramStatus {
                        is_supported: true,
                        adapter_name: None,
                        total_vram_mb: (total / (1024 * 1024)) as u32,
                        used_vram_mb: (usage / (1024 * 1024)) as u32,
                        free_vram_mb: (free / (1024 * 1024)) as u32,
                        process_used_mb: (usage / (1024 * 1024)) as u32,
                        process_budget_mb: (total / (1024 * 1024)) as u32,
                        llm_servers,
                        updated_at_unix_ms,
                        error: Some(format!("Fallback: {}", error)),
                    });
                }
                Ok(GpuVramStatus {
                    is_supported: false,
                    adapter_name: None,
                    total_vram_mb: 0,
                    used_vram_mb: 0,
                    free_vram_mb: 0,
                    process_used_mb: 0,
                    process_budget_mb: 0,
                    llm_servers,
                    updated_at_unix_ms,
                    error: Some(error),
                })
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        Ok(GpuVramStatus {
            is_supported: false,
            adapter_name: None,
            total_vram_mb: 0,
            used_vram_mb: 0,
            free_vram_mb: 0,
            process_used_mb: 0,
            process_budget_mb: 0,
            llm_servers,
            updated_at_unix_ms,
            error: Some("VRAM meter is only available on Windows".to_string()),
        })
    }
}

#[tauri::command]
#[specta::specta]
pub fn change_native_streaming_live_output_model_setting(
    app: AppHandle,
    model_id: String,
    enabled: bool,
) -> Result<bool, String> {
    let model_id = model_id.trim();
    if model_id.is_empty() {
        return Err("A model ID is required for native streaming live output".to_string());
    }

    let mut settings = get_settings(&app);
    let preview_was_disabled = false;
    settings
        .native_streaming_live_output_models
        .retain(|configured_model_id| configured_model_id != model_id);
    if enabled {
        settings
            .native_streaming_live_output_models
            .push(model_id.to_string());
    }
    write_settings(&app, settings);
    Ok(preview_was_disabled)
}

#[tauri::command]
#[specta::specta]
pub fn change_native_streaming_show_interim_longer_setting(
    app: AppHandle,
    enabled: bool,
) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.native_streaming_show_interim_longer = enabled;
    write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_native_streaming_latency_preset_setting(
    app: AppHandle,
    model_id: String,
    preset: crate::settings::NativeStreamingLatencyPreset,
) -> Result<(), String> {
    let model_id = model_id.trim();
    if model_id.is_empty() {
        return Err("A model ID is required for native streaming latency".to_string());
    }

    let model_manager = app.state::<Arc<ModelManager>>();
    let model = model_manager
        .get_model_info(model_id)
        .ok_or_else(|| format!("Unknown model ID: {model_id}"))?;
    if model.native_streaming_latency_kind.is_none() {
        return Err(format!(
            "Model '{model_id}' does not support configurable native streaming latency"
        ));
    }

    let mut settings = get_settings(&app);
    if preset == crate::settings::NativeStreamingLatencyPreset::Accurate {
        settings.native_streaming_latency_presets.remove(model_id);
    } else {
        settings
            .native_streaming_latency_presets
            .insert(model_id.to_string(), preset);
    }
    write_settings(&app, settings);
    Ok(())
}
