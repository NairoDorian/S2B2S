# Build Instructions

This guide covers how to set up the development environment and build S2B2S from source across different platforms.

---

## Prerequisites

### All Platforms

- [Rust](https://rustup.rs/) (latest stable) — MSRV 1.87
- [Bun](https://bun.sh/) package manager (v1.x)
- [Tauri Prerequisites](https://tauri.app/start/prerequisites/) for your platform

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
- Install via: `bun install` will pull all JS dependencies; Rust dependencies via `cargo`

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

### 5. Build for Production

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

```bash
bun run dev       # Start Vite dev server on :1420
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

## Environment Variables

| Variable                           | Purpose                                               |
| ---------------------------------- | ----------------------------------------------------- |
| `ORT_LIB_LOCATION`                 | Path to ONNX Runtime library (Intel Mac only)         |
| `ORT_PREFER_DYNAMIC_LINK=1`        | Use dynamic linking for ONNX Runtime (Intel Mac only) |
| `CMAKE_POLICY_VERSION_MINIMUM=3.5` | Fix cmake errors on macOS                             |
| `S2B2S_NO_GTK_LAYER_SHELL=1`       | Disable GTK layer shell on Linux (Wayland workaround) |
| `WEBKIT_DISABLE_DMABUF_RENDERER=1` | Fix WebKit rendering on some GPU/driver combos        |
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
