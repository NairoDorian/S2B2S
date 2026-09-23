//! CPU / RAM / GPU meters for the status bar.
//!
//! One sampler thread, one second tick, one event — the UI never polls a
//! command per render (docs/PERFORMANCE.md rule 8). While the main window is
//! hidden or minimised the thread only wakes every 3 s to check visibility and
//! keep the CPU baseline fresh — no RAM or NVML query and no event — so an app
//! living in the tray costs close to nothing. GPU figures come from NVML when the
//! driver library is present (NVIDIA on Windows/Linux); otherwise the GPU
//! fields are `None` and the UI hides that meter.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use log::{debug, info};
use serde::{Deserialize, Serialize};
use specta::Type;
use sysinfo::{MemoryRefreshKind, RefreshKind, System};
use tauri::{AppHandle, Manager};
use tauri_specta::Event;

const TICK: Duration = Duration::from_secs(1);
const HIDDEN_TICK: Duration = Duration::from_secs(3);

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type, tauri_specta::Event)]
pub struct SystemStatsEvent {
    pub cpu_percent: f32,
    pub mem_used_mb: u32,
    pub mem_total_mb: u32,
    pub gpu_name: Option<String>,
    pub gpu_percent: Option<f32>,
    pub vram_used_mb: Option<u32>,
    pub vram_total_mb: Option<u32>,
    pub gpu_temp_c: Option<u32>,
}

static LATEST: Mutex<Option<SystemStatsEvent>> = Mutex::new(None);
static STARTED: AtomicBool = AtomicBool::new(false);

/// Most recent sample, for a page that mounts between ticks.
pub fn latest() -> Option<SystemStatsEvent> {
    LATEST.lock().unwrap().clone()
}

pub fn start(app: AppHandle) {
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::Builder::new()
        .name("system-monitor".into())
        .spawn(move || run(app))
        .expect("spawn system monitor");
}

struct Gpu {
    nvml: nvml_wrapper::Nvml,
    name: String,
}

fn init_gpu() -> Option<Gpu> {
    match nvml_wrapper::Nvml::init() {
        Ok(nvml) => {
            let name = nvml
                .device_by_index(0)
                .and_then(|d| d.name())
                .unwrap_or_else(|_| "GPU".into());
            info!("System monitor: NVML available ({name})");
            Some(Gpu { nvml, name })
        }
        Err(e) => {
            debug!("System monitor: NVML unavailable ({e}); GPU meter hidden");
            None
        }
    }
}

fn main_window_visible(app: &AppHandle) -> bool {
    app.get_webview_window("main")
        .map(|w| w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false))
        .unwrap_or(false)
}

fn run(app: AppHandle) {
    let mut sys = System::new_with_specifics(
        RefreshKind::nothing()
            .with_cpu(sysinfo::CpuRefreshKind::nothing().with_cpu_usage())
            .with_memory(MemoryRefreshKind::nothing().with_ram()),
    );
    let gpu = init_gpu();
    // First CPU reading needs a baseline sample.
    sys.refresh_cpu_usage();
    std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);

    loop {
        if !main_window_visible(&app) {
            std::thread::sleep(HIDDEN_TICK);
            // Keep the CPU baseline fresh so the first visible tick is right.
            sys.refresh_cpu_usage();
            continue;
        }
        sys.refresh_cpu_usage();
        sys.refresh_memory();

        let (gpu_name, gpu_percent, vram_used_mb, vram_total_mb, gpu_temp_c) = match &gpu {
            Some(g) => match g.nvml.device_by_index(0) {
                Ok(device) => {
                    let util = device.utilization_rates().ok().map(|u| u.gpu as f32);
                    let mem = device.memory_info().ok();
                    let temp = device
                        .temperature(nvml_wrapper::enum_wrappers::device::TemperatureSensor::Gpu)
                        .ok();
                    (
                        Some(g.name.clone()),
                        util,
                        mem.as_ref().map(|m| (m.used / 1_048_576) as u32),
                        mem.as_ref().map(|m| (m.total / 1_048_576) as u32),
                        temp,
                    )
                }
                Err(_) => (Some(g.name.clone()), None, None, None, None),
            },
            None => (None, None, None, None, None),
        };

        let stats = SystemStatsEvent {
            cpu_percent: sys.global_cpu_usage(),
            mem_used_mb: (sys.used_memory() / 1_048_576) as u32,
            mem_total_mb: (sys.total_memory() / 1_048_576) as u32,
            gpu_name,
            gpu_percent,
            vram_used_mb,
            vram_total_mb,
            gpu_temp_c,
        };
        *LATEST.lock().unwrap() = Some(stats.clone());
        let _ = stats.emit_to(&app, "main");
        std::thread::sleep(TICK);
    }
}
