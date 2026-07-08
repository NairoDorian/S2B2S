# Build Instructions

This guide covers how to set up the development environment and build S2B2S from source across different platforms.

---

## Prerequisites

### All Platforms

- [Rust](https://rustup.rs/) (latest stable) — MSRV 1.87
- [Bun](https://bun.sh/) package manager (v1.x)
- [Tauri Prerequisites](https://tauri.app/start/prerequisites/) for your platform
- **Python 3.12** (for TTS engines — see [Python Version](#3-set-up-python-virtual-environment-for-tts-engines) section)

### macOS

```bash
xcode-select --install
```

#### Intel Mac (x86_64) — ONNX Runtime

Prebuilt ONNX Runtime binaries aren't available for Intel Macs. Install via Homebrew and link dynamically:

```bash
brew install onnxruntime
ORT_LIB_LOCATION=$(brew --prefix onnxruntime)/lib ORT_PREFER_DYNAMIC_LINK=1 bun run tauri dev
ORT_LIB_LOCATION=$(brew --prefix onnxruntime)/lib ORT_PREFER_DYNAMIC_LINK=1 bun run tauri build
```

#### Apple Silicon (M1/M2/M3/M4) — works out of the box with bundled ONNX Runtime.

### Windows
- Microsoft C++ Build Tools or Visual Studio 2019/2022 with "Desktop development with C++" workload
- WebView2 (included with Windows 11, available on Windows 10)
- **Python 3.12** (for TTS engines — see [Python Version](#python-version) below)
- **Vulkan SDK** (for the `transcribe.cpp` Vulkan STT backend): install from [vulkan.lunarg.com](https://vulkan.lunarg.com/sdk/home). The installer adds `VulkanSDK\*\Bin` (containing `glslc.exe`) to `PATH`; `glslc` compiles the ggml Vulkan shaders at build time. If you build `tauri dev` from a plain shell, also set `VULKAN_SDK` (e.g. `set VULKAN_SDK=C:\VulkanSDK\1.4.350.0`). See [transcribe.cpp source](#transcribecpp-stt-backend--source--build-cache).
- **libclang.dll** (build-time dep for whisper-rs-sys bindgen):
> [!IMPORTANT]
> Windows' 260-character path limit can break the native build (the Vulkan
> shader generator nests very deep). If `bun run tauri build` fails with
> `MSB3491` / "path exceeds the OS max path limit", see
> [Windows build fails with `MSB3491`](#windows-build-fails-with-msb3491--path-exceeds-260-characters)
> in Troubleshooting.
#### Linux
  The easiest way on a fresh machine:
  ```powershell
  winget install -e --id LLVM.LLVM
```bash
  # Ubuntu/Debian
  sudo apt update
  sudo apt install build-essential libasound2-dev pkg-config libssl-dev libvulkan-dev vulkan-tools glslc spirv-headers glslang-tools libgtk-3-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libgtk-layer-shell0 libgtk-layer-shell-dev patchelf cmake
  # Fedora/RHEL
  sudo dnf groupinstall "Development Tools"
  sudo dnf install alsa-lib-devel pkgconf openssl-devel vulkan-devel \
    spirv-headers-devel spirv-tools-devel glslang glslc \
    gtk3-devel webkit2gtk4.1-devel libappindicator-gtk3-devel librsvg2-devel \
    gtk-layer-shell gtk-layer-shell-devel \
    cmake
  # Arch Linux
  sudo pacman -S base-devel alsa-lib pkgconf openssl vulkan-devel \
    spirv-headers glslang shaderc \
    gtk3 webkit2gtk-4.1 libappindicator-gtk3 librsvg gtk-layer-shell \
    cmake  ```

  This installs LLVM and adds `libclang.dll` to PATH. No extra env vars needed.

  Or run the helper script for guided install:

  ```powershell
  .\scripts\download-libclang.ps1
  ```

- Install deps: `bun install` pulls JS deps; Rust deps via `cargo`

### Linux

```bash
# Ubuntu/Debian
sudo apt update
sudo apt install build-essential libasound2-dev pkg-config libssl-dev \
  libvulkan-dev vulkan-tools glslc libgtk-3-dev libwebkit2gtk-4.1-dev \
  libayatana-appindicator3-dev librsvg2-dev libgtk-layer-shell0 \
  libgtk-layer-shell-dev patchelf cmake

# Fedora/RHEL
sudo dnf groupinstall "Development Tools"
sudo dnf install alsa-lib-devel pkgconf openssl-devel vulkan-devel \
  gtk3-devel webkit2gtk4.1-devel libappindicator-gtk3-devel \
  librsvg2-devel gtk-layer-shell gtk-layer-shell-devel cmake

# Arch Linux
sudo pacman -S base-devel alsa-lib pkgconf openssl vulkan-devel \
  gtk3 webkit2gtk-4.1 libappindicator-gtk3 librsvg gtk-layer-shell cmake
```

**Nix/NixOS:** A `flake.nix` is provided for reproducible builds on Linux.

---

## Setup Instructions

### 1. Clone the Repository

```bash
git clone git@github.com:NairoDorian/S2B2S.git
cd S2B2S
```

### 2. Install Dependencies

```bash
bun install
```

### 3. Set Up Python Virtual Environment (for TTS Engines)

> **Python version: Use 3.12.** The `kittentts` wheel (v0.8.1) only ships pre-built binaries for Python 3.8–3.12. Python 3.13+ will not work. All other dependencies (`torch`, `onnxruntime`, `numpy`, etc.) are compatible with 3.12. Install from [python.org](https://www.python.org/downloads/release/python-31210/) or via winget:
>
> ```powershell
> winget install Python.Python.3.12
> ```

All local TTS engines (Piper, Kokoro, Kitten, Pocket) run inside a project-local Python venv. The app automatically resolves the venv Python first, falling back to system Python only if no venv is found.

**Windows:**

```powershell
.\scripts\setup_tts_venv.ps1
```

**macOS / Linux:**

```bash
bash scripts/setup_tts_venv.sh
```

This creates `venv/` at the project root and installs: `piper-tts[http]`, `kokoro-tts`, `pocket-tts`, `kittentts`, `torch`, `numpy`, `soundfile`. All packages are installed exclusively in the venv — never on the system Python.

### 4. Download Required Models

Downloads STT, TTS, and Brain model files into the structured `models/` directory (`STT/`, `Brain/`, `TTS/` subfolders):

**Windows:**

```powershell
.\models\download_models.ps1 -Model all    # download everything
.\models\download_models.ps1 -Model kokoro # download only Kokoro TTS
.\models\download_models.ps1 -Model piper,pocket,stt  # specific models
.\models\download_models.ps1 -SetupVenv    # also setup Python venv
.\models\download_models.ps1 -Path C:\my\models  # custom target path
```

**macOS / Linux:**

```bash
bash models/download_models.sh --model all
bash models/download_models.sh --model kokoro
bash models/download_models.sh --model piper,pocket,stt
bash models/download_models.sh --setup-venv
bash models/download_models.sh --path /my/models
```

Options: `-Force`/`--force` to re-download, `-DryRun`/`--dry-run` to preview. Available models: `stt`, `brain`, `piper`, `kokoro`, `pocket`, `kitten`, `tts` (all TTS), `all` (everything).

For the minimal VAD model only (used in development):

```bash
mkdir -p src-tauri/resources/models
curl -o src-tauri/resources/models/silero_vad_v4.onnx https://blob.handy.computer/silero_vad_v4.onnx
```

**Model sources:**
| Model | Size | Source |
|-------|------|--------|
| Silero VAD v4 | ~1.7 MB | blob.handy.computer |
| Parakeet V3 (STT) | ~600 MB | blob.handy.computer |
| Kokoro-82M (TTS) | ~330 MB | HuggingFace hexgrad/Kokoro-82M |
| Piper en_US voices | ~30-70 MB each | HuggingFace rhasspy/piper-voices |
| Pocket TTS | ~100 MB | Auto-downloaded by pocket-tts package |

### 5. Start Dev Server

```bash
bun run tauri dev
# On macOS if you encounter cmake errors:
CMAKE_POLICY_VERSION_MINIMUM=3.5 bun run tauri dev
```

### 6. Build for Production

```bash
bun run tauri build
```

This compiles a release binary and generates platform-specific bundles:

- **Windows**: NSIS installer (`.exe`), MSI
- **macOS**: DMG
- **Linux**: deb, rpm, AppImage

---

## Frontend-Only Development

When working on UI only (no Rust changes needed):

```bashbun run dev       # Start Vite dev server on :1420
bun run build     # Build frontend (TypeScript + Vite)
bun run preview   # Preview built frontend
```

---

## Linting and Formatting

```bash
bun run lint              # ESLint for frontend
bun run lint:fix          # ESLint with auto-fix
bun run format            # Prettier + cargo fmt
bun run format:check      # Check formatting without changes
bun run format:frontend   # Prettier only
bun run format:backend    # cargo fmt only
```

---
## TypeScript Type Checking & Bindings

```bash
bunx tsc --noEmit                    # TypeScript type checking
cargo test export_bindings           # Regenerate src/bindings.ts (headless, no GUI launch)
```

---

## Testing

```bash
# Playwright E2E tests
bun run test:playwright
bun run test:playwright:ui           # With UI

# Rust unit tests
cd src-tauri && cargo test

# Translation check
bun run check:translations
```

---

## Common Issues

### AppImage build fails on Arch / rolling-release distros

`linuxdeploy` bundles an old `strip` binary that can't process libraries built with newer toolchains.

**Workaround:** Build with deb/rpm only:

```bash
bun run tauri build -- --bundles deb
```
### macOS cmake errors

```bash
CMAKE_POLICY_VERSION_MINIMUM=3.5 bun run tauri dev
```

### Windows test executables fail with `STATUS_ENTRYPOINT_NOT_FOUND`

The `build.rs` now embeds Common-Controls v6 manifest into test binaries. If you still see this issue, ensure you have the latest Visual C++ Redistributables.

---

## Linux Installation (from source build)

The raw binary at `src-tauri/target/release/s2b2s` cannot run standalone — it needs Tauri resource files (tray icons, sounds, VAD model) co-located.

**Install from the deb bundle:**

```bash
cd /tmp
ar x /path/to/S2B2S/src-tauri/target/release/bundle/deb/s2b2s_*_amd64.deb data.tar.gz
tar xzf data.tar.gz
sudo cp usr/bin/s2b2s /usr/bin/
sudo cp -r usr/lib/s2b2s /usr/lib/
sudo cp -r usr/share/icons/hicolor/* /usr/share/icons/hicolor/
sudo cp usr/share/applications/s2b2s.desktop /usr/share/applications/
```

After rebuilding, only the binary needs re-copying:

```bash
sudo cp src-tauri/target/release/s2b2s /usr/bin/
```

---

## Llama.cpp Pre-Compiled Server Integration

S2B2S integrates **pre-compiled `llama-server` binaries** from [llama.cpp GitHub releases](https://github.com/ggml-org/llama.cpp/releases) to provide GPU-accelerated local LLM inference out of the box (no CMake, source compilation, or Python dependencies required).

### 1. Architecture & Execution Flow

- **Auto-download**: Fetches appropriate pre-compiled binaries (e.g., `llama-b9601-bin-win-cuda-cu12.4-x64.zip`) directly from GitHub.
- **Auto-install**: Extracts server archives to the `llama_cpp_servers/` subdirectory in the app data directory.
- **Backend Detection**: Probes the system on launch to select the optimal GPU backend: **CUDA > Vulkan > CPU**. CUDA is detected via `nvidia-smi` / `CUDA_PATH`, and Vulkan is verified via `vulkaninfo`.
- **GPU VRAM Offload**: Passes `-ngl all` to offload all model layers into VRAM when GPU acceleration is active.
- **Health Checks & Auto-start**: Polls `/v1/models` on start. `BrainManager::warmup()` ensures the server runs before executing the warmup prompt.

### 2. Supported Backends

| Backend       | Platform       | Suffix / Detail                      |
| ------------- | -------------- | ------------------------------------ |
| **CUDA 12.4** | Windows, Linux | `cuda-cu12.4`                        |
| **CUDA 13.3** | Windows, Linux | `cuda-cu13.3`                        |
| **Vulkan**    | Windows, Linux | `vulkan`                             |
| **CPU**       | Windows, Linux | `cpu`                                |
| **Metal**     | macOS          | Built-in (no separate binary needed) |

### 3. Setup & Model File

1. Place your GGUF model file in the `models/Brain/` directory.
2. Select the **Llama.cpp (Local)** provider under S2B2S Brain Settings (default endpoint: `http://localhost:8001/v1`).
3. Toggle Brain **OFF** in Settings to terminate the local server process immediately.

---

## transcribe.cpp (STT backend) — source & build cache

Speech-to-text is performed by [`transcribe-cpp`](https://github.com/handy-computer/transcribe.cpp), a Rust/ggml wrapper around the `transcribe.cpp` C++ engine (Whisper-family + 16+ model families, Vulkan/CUDA/CPU backends). It is **pulled directly from its GitHub repository**, not from crates.io, so the build always tracks the latest upstream code:

```toml
# src-tauri/Cargo.toml  →  [patch.crates-io]
transcribe-cpp    = { git = "https://github.com/handy-computer/transcribe.cpp", branch = "main" }
transcribe-cpp-sys = { git = "https://github.com/handy-computer/transcribe.cpp", branch = "main" }
```

The `-sys` crate vendors the full C++ tree (ggml, `src/`, cmake) so Cargo can compile `libtranscribe` from source on any machine.

### Caching — why repeat builds are fast

Cargo resolves the `main` branch to an exact commit and records it in `Cargo.lock`. As long as that commit is unchanged, `transcribe-cpp-sys` is **not** recompiled and `bun run tauri dev` is fast on repeat runs (only your app crate + changed deps rebuild). The git checkout is cached under `CARGO_HOME`/registry, and the heavy C++ build output lives in `CARGO_TARGET_DIR` (use `set CARGO_TARGET_DIR=C:\bt` to keep it off the deep `src-tauri\target` path and avoid Windows' 260-char limit).

### Pulling the latest upstream commits

The `[patch]` points at the `main` tip, but Cargo only re-resolves a git dependency when asked — it caches the locked commit otherwise. To fetch newer `transcribe.cpp` commits and rebuild:

```bash
cargo update -p transcribe-cpp -p transcribe-cpp-sys
bun run tauri dev
```

(`cargo update` refreshes everything.) If the upstream `main` moves, only `transcribe-cpp-sys` recompiles the C++; unchanged commits stay cached.

### Known build failure: stale Vulkan shader stubs

`transcribe-cpp-sys` builds the ggml Vulkan backend by generating SPIR-V from `.comp` shaders with `glslc`. The generator writes one generated `.cpp` per shader and **skips regeneration when the output file already exists**. If a prior build was interrupted or `glslc` was missing, you can be left with stale ~36-byte stub files (e.g. `scale.comp.cpp`, `dequant_iq3_xxs.comp.cpp`) containing no shader data. Linking then fails with:

```
ggml-vulkan.obj : error LNK2019: unresolved external symbol "unsigned __int64 const scale_f32_len"
ggml-vulkan.obj : error LNK2019: unresolved external symbol "unsigned char const * const dequant_iq3_xxs_data"
... fatal error LNK1120: 4 unresolved externals
```

**Fix:** force a clean rebuild of just that crate so the shaders regenerate from source (this recompiles the C++ once, then caches):

```bash
cargo clean -p transcribe-cpp-sys
bun run tauri dev
```

This was the blocker after the Handy 0.9 source merge: the first build produced empty shader stubs, and incremental builds kept reusing them. A `cargo clean -p transcribe-cpp-sys` (or a full `cargo clean`) resolves it.

---

## Environment Variables

| Variable                           | Purpose                                               |
| ---------------------------------- | ----------------------------------------------------- |
| `ORT_LIB_LOCATION`                 | Path to ONNX Runtime library (Intel Mac only)         |
| `ORT_PREFER_DYNAMIC_LINK=1`        | Use dynamic linking for ONNX Runtime (Intel Mac only) |
| `CMAKE_POLICY_VERSION_MINIMUM=3.5` | Fix cmake errors on macOS                             |
| `S2B2S_NO_GTK_LAYER_SHELL=1`       | Disable GTK layer shell on Linux (Wayland workaround) |
| `WEBKIT_DISABLE_DMABUF_RENDERER=1` | Fix WebKit rendering on some GPU/driver combos        |
| `VULKAN_SDK`                       | Path to the Vulkan SDK (e.g. `C:\VulkanSDK\1.4.350.0`); ensures `glslc` is found when building the `transcribe.cpp` Vulkan backend from a plain shell |
| `CARGO_TARGET_DIR`                 | Redirect Cargo build output (e.g. `C:\bt`) to avoid the Windows 260-char path limit during the deep ggml shader generation |
| `RUST_LOG`                         | Set Rust log level (e.g., `debug`, `trace`)           |

---

## Continuous Integration

CI is configured via GitHub Actions in `.github/workflows/`:

| Workflow            | Triggers     | Purpose                                       |
| ------------------- | ------------ | --------------------------------------------- |
| `test.yml`          | Push/PR      | Unit tests + lint                             |
| `build.yml`         | Push/PR      | Build on Windows, macOS, Linux                |
| `build-test.yml`    | Push/PR      | Build + test                                  |
| `release.yml`       | Manual       | Create draft release + build platform bundles |
| `playwright.yml`    | Push/PR      | E2E tests                                     |
| `code-quality.yml`  | Push/PR      | ESLint, Prettier, Clippy                      |
| `pr-test-build.yml` | PR           | PR build verification                         |
| `nix-check.yml`     | Push/PR      | Nix flake check                               |
| `main-build.yml`    | Push to main | Main branch build                             |

---

## Project Structure Overview

```
S2B2S/
├── src/                   # Frontend (React/TypeScript)
│   ├── App.tsx
│   ├── components/        # UI components
│   ├── hooks/             # React hooks (useSettings, useOsType, useProviderState, useLlamaState)
│   ├── stores/            # Zustand stores (settings, model)
│   ├── i18n/              # Translation files (20 languages)
│   ├── lib/               # Utilities, types, constants
│   └── ...
├── src-tauri/             # Backend (Rust)
│   ├── src/               # Rust source
│   │   ├── managers/      # Business logic (audio, model, transcription, history, continuous_voice)
│   │   ├── tts/           # TTS subsystem (8 backends)
│   │   ├── brain/         # LLM subsystem (SSE client + llama.cpp bridge)
│   │   ├── llama_server/  # Pre-compiled llama.cpp manager
│   │   ├── audio_toolkit/ # Audio processing + VAD
│   │   ├── stt/           # Python ONNX Runtime STT pipeline (Parakeet Unified)
│   │   ├── overlay_fx/    # GPU overlay system (cursor trail, brain overlay)
│   │   ├── commands/      # Tauri command handlers
│   │   └── ...
│   ├── resources/         # Static resources (models, icons)
│   ├── Cargo.toml         # Rust dependencies
│   └── tauri.conf.json    # Tauri configuration
├── venv/                  # Python virtual environment (created by setup script)
├── models/                # Models in STT/Brain/TTS subdirs (downloaded via scripts)
│   ├── STT/               #   Parakeet, Whisper, Silero VAD
│   ├── Brain/             #   llama.cpp GGUF
│   └── TTS/               #   Kokoro, Piper, Pocket, Kitten
├── scripts/               # Utility scripts (venv setup, dep checks, translations)
├── tests/                 # E2E tests
├── flake.nix              # Nix flake (Linux reproducible builds)
├── package.json           # Node/JS dependencies
└── vite.config.ts         # Vite configuration
```

For the detailed architecture overview, see [AGENTS.md](AGENTS.md) and [S2B2S_REVIEW.md](S2B2S_REVIEW.md).