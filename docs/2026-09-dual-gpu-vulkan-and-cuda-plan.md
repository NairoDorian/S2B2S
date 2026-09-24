# Dual-GPU Architecture Plan: Concurrent CUDA & Vulkan (Intel & NVIDIA) Support in ZER0

**Target Project**: ZER0 (`Handy_V2`)  
**Companion Core**: `transcribe.cpp` (fork at `c:\Users\Z\Downloads\PROJECTS\transcribe-fork`)  
**Date**: 2026-09-24  
**Revision**: 2.0 (Exhaustive Architectural Specification & Edge-Case Blueprint)  
**Status**: Ready for Implementation

---

## 1. Executive Summary & Objective

Modern high-performance laptops and multi-GPU workstations feature two distinct graphics processors:

1. **Discrete GPU (dGPU)**: Dedicated high-throughput accelerator with dedicated VRAM (e.g., **NVIDIA GeForce RTX 4070 Laptop GPU**).
2. **Integrated GPU (iGPU)**: Low-power accelerator sharing high-speed system DDR5 RAM directly with the CPU (e.g., **Intel Iris Xe Graphics**).

### The Core Objective

Transform ZER0's speech transcription engine to support **true concurrent multi-GPU execution**:

- Run compute-heavy primary models (e.g. Whisper Large v3 Turbo, SenseVoice, Parakeet) on **CUDA (RTX 4070)**.
- Simultaneously run auxiliary or secondary streaming models (e.g. Moonshine, extra Multi-STT lanes) on **Vulkan (Intel Iris Xe iGPU)** without allocating a single megabyte of NVIDIA VRAM or stalling the NVIDIA compute queue.
- Expose granular, crystal-clear backend controls in the user interface:
  - **Auto**: Follows global accelerator preferences.
  - **CPU**: Runs on host CPU using vectorized SIMD (AVX2 / AVX-VNNI).
  - **CUDA**: Pins model to NVIDIA GPU via native CUDA runtime.
  - **Vulkan**: Runs on default Vulkan device.
  - **Vulkan (NVIDIA)**: Explicitly pins model to NVIDIA GPU via Vulkan.
  - **Vulkan (Intel)**: Explicitly pins model to Intel Iris Xe iGPU via Vulkan.
- Make all backend options selectable in:
  1. The **Primary Model Selector** popover panel in the status bar.
  2. The **Multi-STT Settings** dropdown for every extra model lane.

---

## 2. Hardware Topology & Concurrency Mechanics

### 2.1 Host Machine Profile

| Subsystem | Hardware Spec                                  | Available Backends           | Memory Model                    | Bandwidth / Capacity        |
| :-------- | :--------------------------------------------- | :--------------------------- | :------------------------------ | :-------------------------- |
| **dGPU**  | NVIDIA GeForce RTX 4070 Laptop (`sm_89`)       | CUDA 13.4, Vulkan 1.4        | Dedicated GDDR6 VRAM            | 8 GB @ 256 GB/s             |
| **iGPU**  | Intel(R) Iris(R) Xe Graphics (`0x8086:0xa7a0`) | Vulkan 1.4                   | Shared Unified System RAM (GTT) | Dynamic Host RAM @ ~80 GB/s |
| **CPU**   | 13th Gen Intel Core i9-13900H (14C/20T)        | CPU (`ggml-cpu-avx2 / vnni`) | Host System RAM                 | 32+ GB DDR5 Dual-Channel    |

### 2.2 Why Simultaneous Execution is Possible

1. **Dynamic Backend Loading (`dynamic-backends`)**:
   `transcribe.cpp` dynamically loads backend runtime modules via `ggml_backend_load_all()`. Both `ggml-cuda.dll` and `ggml-vulkan.dll` are loaded simultaneously into the `zer0.exe` process address space.
2. **Scheduler Isolation**:
   In `ggml`, each loaded model (`Model`) instantiates an independent `ggml_backend_sched_t`. Schedulers do not share state, scratch pads, or command queues.
3. **Driver & Hardware Separation**:
   - CUDA operations submit directly to the NVIDIA display driver stack (`nvwgf2um64.dll` / `nvcuda.dll`).
   - Intel Vulkan operations submit to the Intel Graphics driver (`igvk64.dll`).
   - The physical execution units (NVIDIA Ada Lovelace SMs vs. Intel Xe Execution Units) execute in parallel with independent hardware schedulers.

### 2.3 Memory Topology & Cross-Device Data Flow

```
                      ┌────────────────────────────────────────────────────────┐
                      │              ZER0 Application (Handy_V2)               │
                      │               Tokio / Tauri Runtime Async              │
                      └───────────────────────────┬────────────────────────────┘
                                                  │
                ┌─────────────────────────────────┴─────────────────────────────────┐
                ▼                                                                   ▼
    ┌────────────────────────┐                                          ┌────────────────────────┐
    │ Primary Model Session  │                                          │ Multi-STT Extra Session│
    │ (Whisper Large v3)     │                                          │ (Moonshine Base)       │
    │ Backend: CUDA          │                                          │ Backend: Vulkan (Intel)│
    └───────────┬────────────┘                                          └───────────┬────────────┘
                │                                                                   │
                ▼                                                                   ▼
    ┌────────────────────────┐                                          ┌────────────────────────┐
    │  ggml_backend_sched_t  │                                          │  ggml_backend_sched_t  │
    │   (CUDA Context 0)     │                                          │  (Vulkan Device 1)     │
    └───────────┬────────────┘                                          └───────────┬────────────┘
                │                                                                   │
                ▼                                                                   ▼
    ┌────────────────────────┐                                          ┌────────────────────────┐
    │ NVIDIA RTX 4070 (dGPU) │                                          │   Intel Iris Xe (iGPU) │
    │ 8 GB Dedicated GDDR6   │                                          │ Unified DDR5 Host RAM  │
    │ VRAM Usage: ~1.6 GB    │                                          │ Memory Usage: ~400 MB  │
    └────────────────────────┘                                          └────────────────────────┘
```

---

## 3. Four-Tier Implementation Blueprint

```
┌────────────────────────────────────────────────────────────────────────┐
│ Tier 4: UI & Frontend Presentation                                     │
│  - ModelBackendPanel.tsx (radio list with vendor subtitles)            │
│  - ModelBackendDropdown.tsx (Multi-STT slot dropdowns)                 │
│  - translations.json (i18n strings for NVIDIA & Intel options)         │
├────────────────────────────────────────────────────────────────────────┤
│ Tier 3: Rust Backend & Device Resolution Engine                        │
│  - transcription.rs: resolve_model_backend() device binding             │
│  - transcription.rs: available_model_backends() dynamic detection       │
│  - shortcut/mod.rs: set_model_backend_setting() IPC validation         │
├────────────────────────────────────────────────────────────────────────┤
│ Tier 2: Settings Schema & Specta Bindings                              │
│  - settings.rs: ModelBackendSetting enum extended with variants         │
│  - Unit tests for serde & wire name round-tripping                     │
│  - bindings.ts: Specta TypeScript bindings export                      │
├────────────────────────────────────────────────────────────────────────┤
│ Tier 1: Build System & Runtime Packaging                               │
│  - Cargo.toml: target dependencies feature flag: "vulkan"              │
│  - build.rs: stage_transcribe_runtime_libs packaging ggml-vulkan.dll   │
│  - scripts/build-options.ts: compiler cache & shader compiler paths    │
└────────────────────────────────────────────────────────────────────────┘
```

---

### Tier 1: Build System & Packaging Configuration

#### 1.1 `src-tauri/Cargo.toml`

Enable `"vulkan"` alongside `"cuda"` and `"dynamic-backends"` for Windows x86_64:

```toml
[target.'cfg(all(windows, target_arch = "x86_64"))'.dependencies]
transcribe-cpp = { version = "*", default-features = false, features = [
  "dynamic-backends",
  "cuda",
  "vulkan",
] }
```

#### 1.2 Native Build Script (`transcribe-cpp-sys/build.rs`)

The sys crate build script inspects `CARGO_FEATURE_VULKAN` and `CARGO_FEATURE_CUDA`:

- When both are enabled:
  - `TRANSCRIBE_GGML_BACKEND_DL=ON`
  - `TRANSCRIBE_CUDA=ON`
  - `TRANSCRIBE_VULKAN=ON`
- CMake automatically discovers the Vulkan SDK (`C:\VulkanSDK\1.4.350.0\Bin\glslc.exe`) and generates SPIR-V shader headers.
- `sccache` will cache C/C++ compilation of `ggml-vulkan.cpp` and `vulkan-shaders.cpp`, while `CMAKE_CUDA_COMPILER_LAUNCHER=""` protects nvcc.

#### 1.3 DLL Staging (`src-tauri/build.rs`)

`stage_transcribe_runtime_libs()` discovers runtime DLLs across `DEP_TRANSCRIBE_CPP_RUNTIME_DIR` and `DEP_TRANSCRIBE_CPP_MODULE_DIR`.
With `vulkan` active:

- `ggml-vulkan.dll` is staged into `src-tauri/transcribe-libs/`.
- Staged DLLs are placed beside `zer0.exe` for development and packaged by `makensis` for production.

---

### Tier 2: Settings Schema & Wire Protocol

#### 2.1 `src-tauri/src/settings.rs`

Extend `ModelBackendSetting`:

```rust
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum ModelBackendSetting {
    #[default]
    Auto,
    Cpu,
    Cuda,
    Vulkan,
    VulkanNvidia,
    VulkanIntel,
    Metal,
    Rocm,
}

impl ModelBackendSetting {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Cpu => "cpu",
            Self::Cuda => "cuda",
            Self::Vulkan => "vulkan",
            Self::VulkanNvidia => "vulkan_nvidia",
            Self::VulkanIntel => "vulkan_intel",
            Self::Metal => "metal",
            Self::Rocm => "rocm",
        }
    }

    #[cfg(test)]
    pub fn from_wire(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "cpu" => Self::Cpu,
            "cuda" => Self::Cuda,
            "vulkan" => Self::Vulkan,
            "vulkan_nvidia" => Self::VulkanNvidia,
            "vulkan_intel" => Self::VulkanIntel,
            "metal" => Self::Metal,
            "rocm" => Self::Rocm,
            _ => Self::Auto,
        }
    }
}
```

#### 2.2 Schema Roundtrip Unit Test

Update `model_backend_names_round_trip` in `settings.rs` to include `ModelBackendSetting::VulkanNvidia` and `ModelBackendSetting::VulkanIntel`.

---

### Tier 3: Native Device Resolution & Discovery Engine

#### 3.1 Robust Device Matching

In `src-tauri/src/managers/transcription.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VulkanTargetDevice {
    Any,
    Nvidia,
    Intel,
}

/// Find a specific Vulkan compute device using a prioritized matching strategy:
/// 1. Primary match: vendor name in device description or name (case-insensitive).
/// 2. Secondary match: device_type classification (Igpu for Intel, Gpu for discrete NVIDIA).
fn find_vulkan_device(target: VulkanTargetDevice) -> Option<transcribe_cpp::Device> {
    let devices = transcribe_compute_devices();
    let vulkan_devices: Vec<_> = devices
        .into_iter()
        .filter(|d| d.kind.eq_ignore_ascii_case("vulkan"))
        .collect();

    match target {
        VulkanTargetDevice::Any => vulkan_devices.into_iter().next(),
        VulkanTargetDevice::Nvidia => vulkan_devices
            .iter()
            .find(|d| {
                let desc = d.description.to_ascii_lowercase();
                let name = d.name.to_ascii_lowercase();
                desc.contains("nvidia") || name.contains("nvidia") || desc.contains("geforce") || desc.contains("quadro")
            })
            .cloned()
            .or_else(|| {
                // Secondary fallback: discrete GPU if no description matched
                vulkan_devices
                    .iter()
                    .find(|d| d.device_type == transcribe_cpp::DeviceType::Gpu)
                    .cloned()
            }),
        VulkanTargetDevice::Intel => vulkan_devices
            .iter()
            .find(|d| {
                let desc = d.description.to_ascii_lowercase();
                let name = d.name.to_ascii_lowercase();
                desc.contains("intel") || name.contains("intel") || desc.contains("iris") || desc.contains("arc")
            })
            .cloned()
            .or_else(|| {
                // Secondary fallback: any registered integrated GPU
                vulkan_devices
                    .iter()
                    .find(|d| d.device_type == transcribe_cpp::DeviceType::Igpu)
                    .cloned()
            }),
    }
}
```

#### 3.2 Updating `resolve_model_backend()`

```rust
fn resolve_model_backend(settings: &AppSettings, model_id: &str) -> (Backend, Option<Device>) {
    let accelerator = settings.transcribe_accelerator;
    let requested = settings
        .per_model_backends
        .get(model_id)
        .copied()
        .unwrap_or_default();

    let requested = if transcribe_gpu_disabled_for_host() {
        ModelBackendSetting::Cpu
    } else {
        requested
    };

    let global = || {
        let device = resolve_gpu_device(accelerator, settings.transcribe_gpu_device.as_deref());
        let backend = if device.is_some() {
            Backend::Auto
        } else {
            select_transcribe_backend(accelerator)
        };
        (backend, device)
    };

    match requested {
        ModelBackendSetting::Auto => global(),
        ModelBackendSetting::Cpu => (Backend::Cpu, None),
        ModelBackendSetting::Cuda => {
            if transcribe_cpp::backend_available(Backend::Cuda) {
                (Backend::Cuda, None)
            } else {
                warn!(
                    "Model '{}' requested CUDA, but CUDA is unavailable; using global policy",
                    model_id
                );
                global()
            }
        }
        ModelBackendSetting::Vulkan => {
            if transcribe_cpp::backend_available(Backend::Vulkan) {
                (Backend::Vulkan, None)
            } else {
                warn!(
                    "Model '{}' requested Vulkan, but Vulkan is unavailable; using global policy",
                    model_id
                );
                global()
            }
        }
        ModelBackendSetting::VulkanNvidia => {
            if transcribe_cpp::backend_available(Backend::Vulkan) {
                if let Some(dev) = find_vulkan_device(VulkanTargetDevice::Nvidia) {
                    (Backend::Vulkan, Some(dev))
                } else {
                    warn!(
                        "Model '{}' requested Vulkan (NVIDIA), but no NVIDIA Vulkan device was found; falling back to default Vulkan",
                        model_id
                    );
                    (Backend::Vulkan, None)
                }
            } else {
                global()
            }
        }
        ModelBackendSetting::VulkanIntel => {
            if transcribe_cpp::backend_available(Backend::Vulkan) {
                if let Some(dev) = find_vulkan_device(VulkanTargetDevice::Intel) {
                    (Backend::Vulkan, Some(dev))
                } else {
                    warn!(
                        "Model '{}' requested Vulkan (Intel), but no Intel Vulkan device was found; falling back to default Vulkan",
                        model_id
                    );
                    (Backend::Vulkan, None)
                }
            } else {
                global()
            }
        }
        ModelBackendSetting::Metal => {
            if transcribe_cpp::backend_available(Backend::Metal) {
                (Backend::Metal, None)
            } else {
                global()
            }
        }
        ModelBackendSetting::Rocm => {
            if transcribe_cpp::backend_available(Backend::Rocm) {
                (Backend::Rocm, None)
            } else {
                global()
            }
        }
    }
}
```

#### 3.3 Dynamic Discovery in `available_model_backends()`

```rust
pub fn available_model_backends() -> Vec<String> {
    let mut out = vec!["auto".to_string(), "cpu".to_string()];
    if transcribe_gpu_disabled_for_host() {
        return out;
    }
    if transcribe_cpp::backend_available(Backend::Cuda) {
        out.push("cuda".to_string());
    }
    if transcribe_cpp::backend_available(Backend::Vulkan) {
        out.push("vulkan".to_string());

        let devices = transcribe_compute_devices();
        let has_nvidia = devices.iter().any(|d| {
            d.kind.eq_ignore_ascii_case("vulkan")
                && (d.description.to_ascii_lowercase().contains("nvidia")
                    || d.name.to_ascii_lowercase().contains("nvidia"))
        });
        let has_intel = devices.iter().any(|d| {
            d.kind.eq_ignore_ascii_case("vulkan")
                && (d.device_type == transcribe_cpp::DeviceType::Igpu
                    || d.description.to_ascii_lowercase().contains("intel")
                    || d.name.to_ascii_lowercase().contains("intel"))
        });

        if has_nvidia {
            out.push("vulkan_nvidia".to_string());
        }
        if has_intel {
            out.push("vulkan_intel".to_string());
        }
    }
    if transcribe_cpp::backend_available(Backend::Metal) {
        out.push("metal".to_string());
    }
    if transcribe_cpp::backend_available(Backend::Rocm) {
        out.push("rocm".to_string());
    }
    out
}
```

---

### Tier 4: Frontend UI, Localization & Bindings

#### 4.1 UI Constants & Ordering (`src/components/model-selector/ModelBackendPanel.tsx`)

```typescript
export const BACKEND_ORDER: ModelBackendSetting[] = [
  "auto",
  "cpu",
  "cuda",
  "vulkan",
  "vulkan_nvidia",
  "vulkan_intel",
  "metal",
  "rocm",
];

const BACKEND_LITERAL: Partial<Record<ModelBackendSetting, string>> = {
  cuda: "CUDA",
  vulkan: "Vulkan",
  vulkan_nvidia: "Vulkan (NVIDIA)",
  vulkan_intel: "Vulkan (Intel)",
  metal: "Metal",
  rocm: "ROCm",
};
```

#### 4.2 Multi-STT Dropdown (`src/components/settings/multi-stt/ModelBackendDropdown.tsx`)

`ModelBackendDropdown` automatically inherits the new options through `loadAvailableBackends()` and `backendLabel()`.

#### 4.3 Localization (`src/i18n/locales/en/translation.json`)

```json
"backend": {
  "title": "Backend",
  "auto": "Auto",
  "cpu": "CPU",
  "appliesOnNextLoad": "Applies the next time this model is loaded.",
  "descriptions": {
    "auto": "Follow the global accelerator setting.",
    "cpu": "Run this model on the CPU.",
    "cuda": "Run this model on the NVIDIA GPU through CUDA (RTX 4070).",
    "vulkan": "Run this model on the default GPU through Vulkan.",
    "vulkan_nvidia": "Run this model on the NVIDIA GPU through Vulkan.",
    "vulkan_intel": "Run this model on the Intel integrated GPU through Vulkan (saves NVIDIA VRAM).",
    "metal": "Run this model on the Apple GPU through Metal.",
    "rocm": "Run this model on the AMD GPU through ROCm."
  }
}
```

#### 4.4 Specta TypeScript Bindings (`src/bindings.ts`)

```typescript
export type ModelBackendSetting =
  | "auto"
  | "cpu"
  | "cuda"
  | "vulkan"
  | "vulkan_nvidia"
  | "vulkan_intel"
  | "metal"
  | "rocm";
```

---

## 4. Multi-Model Concurrency & Thermal Profiles

| Concurrency Scenario       | Primary Model (Slot 0)             | Multi-STT Slot 1                | Multi-STT Slot 2     | GPU 0 (RTX 4070) Load     | GPU 1 (Intel Xe) Load    | Host CPU Load           |
| :------------------------- | :--------------------------------- | :------------------------------ | :------------------- | :------------------------ | :----------------------- | :---------------------- |
| **Dual GPU Balanced**      | Whisper Large v3 (`cuda`)          | Moonshine Base (`vulkan_intel`) | _(Empty)_            | ~1.6 GB VRAM, 25% compute | ~400 MB RAM, 45% compute | ~5% dispatch overhead   |
| **Triple Lane Hybrid**     | Whisper Large v3 (`cuda`)          | Moonshine Base (`vulkan_intel`) | Parakeet TDT (`cpu`) | ~1.6 GB VRAM, 25% compute | ~400 MB RAM, 45% compute | ~20% compute on P-cores |
| **Full Vulkan Comparison** | Whisper Large v3 (`vulkan_nvidia`) | Moonshine Base (`vulkan_intel`) | _(Empty)_            | ~1.6 GB VRAM, 28% compute | ~400 MB RAM, 45% compute | ~5% dispatch overhead   |

---

## 5. Potential Edge Cases & Robustness Matrix

| Potential Issue                       | Root Cause                                                                | Built-in Mitigation in Plan                                                                                                      |
| :------------------------------------ | :------------------------------------------------------------------------ | :------------------------------------------------------------------------------------------------------------------------------- |
| **Intel GPU Sleeping / Unplugged**    | Windows D3 power management powers down iGPU on aggressive battery saver. | `find_vulkan_device(Intel)` falls back to default Vulkan device or global policy with a clear diagnostic warning; never crashes. |
| **Vulkan Shader Stalling**            | First SPIR-V shader pipeline creation causes minor compilation stutter.   | Shaders are pre-compiled to SPIR-V bytecode at build time by `glslc`; pipeline caches persist in driver.                         |
| **Device ID Instability**             | Windows driver update changes Vulkan device index enumeration order.      | `find_vulkan_device` keys off vendor description substring and device classification, never raw registry indices.                |
| **Simultaneous Stream Dropouts**      | Audio queue starvation under dual inference load.                         | Audio dispatch uses non-blocking ring buffers in `multi_streaming.rs`. Inference runs on separate OS threads.                    |
| **Settings Deserialization Mismatch** | Stored preference contains older or unrecognised string.                  | `from_wire` and `unwrap_or_default()` map any unknown string safely to `Auto`.                                                   |

---

## 6. Verification & Quality Gates

### Gate 1: Build & DLL Verification

```bash
bun run tauri dev:fast
```

- Verify `ggml-cuda.dll` and `ggml-vulkan.dll` are compiled.
- Verify both DLLs are present in `src-tauri/transcribe-libs/`.

### Gate 2: Device Enumeration & IPC Test

- Launch app, verify dev tools / logs:

```
[info] transcribe.cpp registered compute devices:
  - Device 0: kind=cuda, name=CUDA0, desc="NVIDIA GeForce RTX 4070 Laptop GPU", type=Gpu
  - Device 1: kind=vulkan, name=Vulkan0, desc="NVIDIA GeForce RTX 4070 Laptop GPU", type=Gpu
  - Device 2: kind=vulkan, name=Vulkan1, desc="Intel(R) Iris(R) Xe Graphics", type=Igpu
  - Device 3: kind=cpu, name=CPU, desc="13th Gen Intel(R) Core(TM) i9-13900H", type=Cpu
```

### Gate 3: UI Dropdown & Settings Test

- Open Primary Model Selector -> Click Backend -> Verify options:
  - `Auto`, `CPU`, `CUDA`, `Vulkan`, `Vulkan (NVIDIA)`, `Vulkan (Intel)`.
- Open Settings -> Multi-STT -> Verify `ModelBackendDropdown` contains all options.
- Set Primary to `CUDA` and Extra Model to `Vulkan (Intel)`.

### Gate 4: Concurrent Audio Streaming Parity Test

- Start audio recording in Multi-STT streaming mode.
- Observe real-time transcription overlay:
  - Slot 0 (Whisper / CUDA) updates live text.
  - Slot 1 (Moonshine / Vulkan Intel) updates live text simultaneously.
  - On speech pause: texts merge cleanly without latency spikes.
