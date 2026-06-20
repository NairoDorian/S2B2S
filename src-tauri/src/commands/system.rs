#[tauri::command]
#[specta::specta]
pub fn get_system_ram() -> Result<SystemRamInfo, String> {
    let (total_kb, free_kb) = get_ram_info()?;
    Ok(SystemRamInfo {
        total_mb: total_kb / 1024,
        used_mb: (total_kb.saturating_sub(free_kb)) / 1024,
        free_mb: free_kb / 1024,
    })
}

#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct SystemRamInfo {
    #[specta(type = u32)]
    pub total_mb: u64,
    #[specta(type = u32)]
    pub used_mb: u64,
    #[specta(type = u32)]
    pub free_mb: u64,
}

fn get_ram_info() -> Result<(u64, u64), String> {
    #[cfg(target_os = "windows")]
    {
        let output = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "$os = Get-CimInstance Win32_OperatingSystem; Write-Output \"Total=$($os.TotalVisibleMemorySize)\"; Write-Output \"Free=$($os.FreePhysicalMemory)\"",
            ])
            .output()
            .map_err(|e| format!("Failed to get RAM info: {e}"))?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut total: Option<u64> = None;
        let mut free: Option<u64> = None;
        for line in stdout.lines() {
            let line = line.trim();
            if let Some(val) = line.strip_prefix("Total=") {
                total = val.trim().parse().ok();
            }
            if let Some(val) = line.strip_prefix("Free=") {
                free = val.trim().parse().ok();
            }
        }
        let total = total.ok_or("Failed to parse total RAM")?;
        let free = free.ok_or("Failed to parse free RAM")?;
        Ok((total, free))
    }

    #[cfg(target_os = "linux")]
    {
        let content = std::fs::read_to_string("/proc/meminfo")
            .map_err(|e| format!("Failed to read /proc/meminfo: {e}"))?;
        let mut total: Option<u64> = None;
        let mut available: Option<u64> = None;
        for line in content.lines() {
            if line.starts_with("MemTotal:") {
                total = line.split_whitespace().nth(1).and_then(|s| s.parse().ok());
            }
            if line.starts_with("MemAvailable:") {
                available = line.split_whitespace().nth(1).and_then(|s| s.parse().ok());
            }
        }
        let total = total.ok_or("Failed to parse MemTotal")?;
        let available = available.ok_or("Failed to parse MemAvailable")?;
        Ok((total, available))
    }

    #[cfg(target_os = "macos")]
    {
        let total_output = std::process::Command::new("sysctl")
            .args(["-n", "hw.memsize"])
            .output()
            .map_err(|e| format!("Failed to get total RAM: {e}"))?;
        let total_bytes: u64 = String::from_utf8_lossy(&total_output.stdout)
            .trim()
            .parse()
            .map_err(|e| format!("Failed to parse total RAM: {e}"))?;
        let total_kb = total_bytes / 1024;

        let vm_output = std::process::Command::new("vm_stat")
            .output()
            .map_err(|e| format!("Failed to run vm_stat: {e}"))?;
        let vm_stdout = String::from_utf8_lossy(&vm_output.stdout);
        let page_size_output = std::process::Command::new("pagesize")
            .output()
            .unwrap_or_else(|_| {
                std::process::Command::new("sysctl")
                    .args(["-n", "vm.pagesize"])
                    .output()
                    .unwrap_or_default()
            });
        let page_size: u64 = String::from_utf8_lossy(&page_size_output.stdout)
            .trim()
            .parse()
            .unwrap_or(16384);

        let mut free_pages: u64 = 0;
        for line in vm_stdout.lines() {
            if line.contains("free") || line.contains("Pages free") {
                if let Some(val) = line.split(':').nth(1) {
                    if let Ok(pages) = val.trim().trim_end_matches('.').parse::<u64>() {
                        free_pages += pages;
                    }
                }
            }
        }
        let free_kb = (free_pages * page_size) / 1024;
        Ok((total_kb, free_kb))
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        Err("Unsupported OS".into())
    }
}

fn resolve_resource_path(resource_name: &str) -> Option<std::path::PathBuf> {
    // 1. Dev mode: check manifest dir
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let dev_path = std::path::PathBuf::from(manifest_dir).join("..").join(resource_name);
        if dev_path.exists() {
            return Some(dev_path);
        }
    }
    
    // 2. Installed mode: check next to executable
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for candidate in [
                dir.join(resource_name),
                dir.join("resources").join(resource_name),
                dir.join("..").join("Resources").join(resource_name),
            ] {
                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }
    }
    
    None
}

#[tauri::command]
#[specta::specta]
pub fn check_speech_runtime_installed() -> bool {
    let python = crate::portable::resolve_venv_python();
    let path_str = python.to_string_lossy();
    (path_str.contains("venv") || path_str.contains("VENV")) && python.exists()
}

#[tauri::command]
#[specta::specta]
pub async fn install_speech_runtime(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Emitter;
    
    let app_data_dir = crate::portable::app_data_dir(&app)
        .map_err(|e| format!("Failed to get app data directory: {}", e))?;
    
    let target_dir_str = app_data_dir.to_string_lossy().to_string();
    
    let script_name = if cfg!(windows) {
        "scripts/install-speech-runtime.ps1"
    } else {
        "scripts/install-speech-runtime.sh"
    };
    
    let script_path = resolve_resource_path(script_name)
        .ok_or_else(|| format!("Could not find runtime install script: {}", script_name))?;
        
    log::info!("Running runtime install script at {} with target dir {}", script_path.display(), target_dir_str);
    
    let app_clone = app.clone();
    std::thread::spawn(move || {
        let mut cmd = if cfg!(windows) {
            let mut c = std::process::Command::new("powershell");
            c.args([
                "-NoProfile",
                "-ExecutionPolicy", "Bypass",
                "-File", &script_path.to_string_lossy().to_string(),
                "-TargetDir", &target_dir_str,
            ]);
            c
        } else {
            let mut c = std::process::Command::new("bash");
            c.args([
                &script_path.to_string_lossy().to_string(),
                &target_dir_str,
            ]);
            c
        };
        
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }
        
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                let err_msg = format!("Failed to start install process: {}", e);
                log::error!("{}", err_msg);
                let _ = app_clone.emit("runtime-install-failed", err_msg);
                return;
            }
        };
        
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        
        let app_emit = app_clone.clone();
        let stdout_thread = std::thread::spawn(move || {
            use std::io::{BufRead, BufReader};
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                if let Ok(l) = line {
                    log::info!("[RuntimeInstall] {}", l);
                    let payload = serde_json::json!({ "message": l });
                    let _ = app_emit.emit("runtime-install-progress", payload);
                }
            }
        });
        
        let stderr_thread = std::thread::spawn(move || {
            use std::io::{BufRead, BufReader};
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                if let Ok(l) = line {
                    log::warn!("[RuntimeInstall Error] {}", l);
                }
            }
        });
        
        let status = child.wait();
        let _ = stdout_thread.join();
        let _ = stderr_thread.join();
        
        match status {
            Ok(s) if s.success() => {
                log::info!("Runtime installation completed successfully");
                let _ = app_clone.emit("runtime-install-success", ());
            }
            Ok(s) => {
                let err_msg = format!("Install process exited with status: {}", s);
                log::error!("{}", err_msg);
                let _ = app_clone.emit("runtime-install-failed", err_msg);
            }
            Err(e) => {
                let err_msg = format!("Failed to wait for install process: {}", e);
                log::error!("{}", err_msg);
                let _ = app_clone.emit("runtime-install-failed", err_msg);
            }
        }
    });

    Ok(())
}

