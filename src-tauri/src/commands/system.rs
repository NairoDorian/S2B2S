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
                total = line
                    .split_whitespace()
                    .nth(1)
                    .and_then(|s| s.parse().ok());
            }
            if line.starts_with("MemAvailable:") {
                available = line
                    .split_whitespace()
                    .nth(1)
                    .and_then(|s| s.parse().ok());
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
            .unwrap_or_else(|_| std::process::Command::new("sysctl")
                .args(["-n", "vm.pagesize"])
                .output()
                .unwrap());
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
