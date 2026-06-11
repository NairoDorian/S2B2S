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

    // Persist the new selection early so the frontend sees the correct model
    // when it reacts to events emitted by load_model.
    let mut settings = settings;
    settings.selected_model = model_id.to_string();

    // Reset language to auto if the new model doesn't support the currently selected language.
    // This prevents stale language settings from causing errors (e.g. Canary receiving zh-Hans)
    // and stops downstream processing (e.g. OpenCC) from running on an irrelevant language.
    if settings.selected_language != "auto"
        && !model_info.supported_languages.is_empty()
        && !model_info
            .supported_languages
            .contains(&settings.selected_language)
    {
        log::info!(
            "Resetting language from '{}' to 'auto' (not supported by {})",
            settings.selected_language,
            model_id
        );
        settings.selected_language = "auto".to_string();
    }

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
    // Check if transcription manager has a loaded model
    let current_model = transcription_manager.get_current_model();
    Ok(current_model.is_none())
}

#[tauri::command]
#[specta::specta]
pub async fn has_any_models_available(
    model_manager: State<'_, Arc<ModelManager>>,
) -> Result<bool, String> {
    let models = model_manager.get_available_models();
    Ok(models.iter().any(|m| m.is_downloaded))
}

#[tauri::command]
#[specta::specta]
pub async fn has_any_models_or_downloads(
    model_manager: State<'_, Arc<ModelManager>>,
) -> Result<bool, String> {
    let models = model_manager.get_available_models();
    // Return true if any models are downloaded OR if any downloads are in progress
    Ok(models.iter().any(|m| m.is_downloaded))
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
        if !nt_success(open_status) || open.hAdapter == 0 {
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
            return None;
        }

        Some(query.CurrentUsage)
    }
}

#[cfg(target_os = "windows")]
fn query_active_gpu_vram() -> Result<ActiveGpuVramSnapshot, String> {
    use windows::core::Interface;
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory1, IDXGIAdapter1, IDXGIAdapter3, IDXGIFactory6,
        DXGI_ADAPTER_FLAG_SOFTWARE, DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE,
        DXGI_MEMORY_SEGMENT_GROUP_LOCAL, DXGI_QUERY_VIDEO_MEMORY_INFO,
    };

    unsafe {
        let factory: IDXGIFactory6 = CreateDXGIFactory1::<IDXGIFactory6>()
            .map_err(|e| format!("Failed to create DXGI factory: {e}"))?;

        let mut best: Option<ActiveGpuVramSnapshot> = None;
        let mut adapter_index = 0u32;

        loop {
            let adapter: IDXGIAdapter1 = match factory
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

            let budget_bytes = if memory_info.Budget > 0 {
                memory_info.Budget
            } else {
                desc.DedicatedVideoMemory as u64
            };
            let used_bytes = memory_info.CurrentUsage;
            let total_vram_bytes = if desc.DedicatedVideoMemory > 0 {
                desc.DedicatedVideoMemory as u64
            } else {
                budget_bytes
            };
            let adapter_name = wide_to_string(&desc.Description);
            let adapter_name = if adapter_name.is_empty() {
                format!("GPU {}", adapter_index)
            } else {
                adapter_name
            };

            // Prefer the adapter with the largest total VRAM
            let should_replace = match best {
                None => true,
                Some(ref best_snapshot) => {
                    total_vram_bytes > best_snapshot.total_vram_bytes
                        || (total_vram_bytes == best_snapshot.total_vram_bytes
                            && (used_bytes > best_snapshot.process_used_bytes
                                || (used_bytes == best_snapshot.process_used_bytes
                                    && budget_bytes > best_snapshot.process_budget_bytes)))
                }
            };

            if should_replace {
                best = Some(ActiveGpuVramSnapshot {
                    adapter_name,
                    adapter_luid: desc.AdapterLuid,
                    process_used_bytes: used_bytes,
                    process_budget_bytes: budget_bytes,
                    total_vram_bytes,
                });
            }
        }

        best.ok_or_else(|| "No active hardware GPU adapter detected".to_string())
    }
}

#[cfg(target_os = "windows")]
fn detect_llm_servers() -> Vec<LlmServerInfo> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW,
        PROCESSENTRY32W, TH32CS_SNAPPROCESS,
    };
    use windows::Win32::Foundation::CloseHandle;

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

                if let Some(&(_, display_name)) = llm_process_names.iter().find(|&&(name, _)| exe_name == name) {
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
        match query_active_gpu_vram() {
            Ok(snapshot) => {
                let system_used_bytes = query_system_gpu_usage_bytes(snapshot.adapter_luid)
                    .unwrap_or(snapshot.process_used_bytes);
                let system_free_bytes = snapshot.total_vram_bytes.saturating_sub(system_used_bytes);

                Ok(GpuVramStatus {
                    is_supported: true,
                    adapter_name: Some(snapshot.adapter_name),
                    total_vram_mb: (snapshot.total_vram_bytes / (1024 * 1024)) as u32,
                    used_vram_mb: (system_used_bytes / (1024 * 1024)) as u32,
                    free_vram_mb: (system_free_bytes / (1024 * 1024)) as u32,
                    process_used_mb: (snapshot.process_used_bytes / (1024 * 1024)) as u32,
                    process_budget_mb: (snapshot.process_budget_bytes / (1024 * 1024)) as u32,
                    llm_servers,
                    updated_at_unix_ms,
                    error: None,
                })
            }
            Err(error) => Ok(GpuVramStatus {
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
            }),
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
