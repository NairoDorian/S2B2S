#!/usr/bin/env bash
# S2B2S Speech Runtime Installer (macOS & Linux)
# Downloads portable uv, installs portable Python 3.12.13, creates venv, and installs dependencies.
# Used during the onboarding flow.

set -euo pipefail

if [ "$#" -ne 1 ]; then
    echo "Usage: $0 <target_directory>"
    exit 1
fi

TARGET_DIR="$1"
VENV_DIR="$TARGET_DIR/venv"
UV_EXE="$TARGET_DIR/uv"

echo "========================================"
echo "  S2B2S - Speech Runtime Installer"
echo "  Target Directory: $TARGET_DIR"
echo "========================================"

# 1. Ensure Target Directory exists
mkdir -p "$TARGET_DIR"

# 2. Download portable uv if not already present
if [ ! -f "$UV_EXE" ]; then
    echo "[1/5] Downloading portable uv..."
    
    OS_TYPE="$(uname -s)"
    ARCH_TYPE="$(uname -m)"
    
    UV_PLATFORM=""
    if [ "$OS_TYPE" = "Darwin" ]; then
        if [ "$ARCH_TYPE" = "arm64" ] || [ "$ARCH_TYPE" = "aarch64" ]; then
            UV_PLATFORM="aarch64-apple-darwin"
        else
            UV_PLATFORM="x86_64-apple-darwin"
        fi
    elif [ "$OS_TYPE" = "Linux" ]; then
        if [ "$ARCH_TYPE" = "arm64" ] || [ "$ARCH_TYPE" = "aarch64" ]; then
            UV_PLATFORM="aarch64-unknown-linux-gnu"
        else
            UV_PLATFORM="x86_64-unknown-linux-gnu"
        fi
    else
        echo "ERROR: Unsupported operating system: $OS_TYPE"
        exit 1
    fi
    
    echo "Detected platform: $UV_PLATFORM"
    
    UV_VERSION="0.5.21"
    UV_URL="https://github.com/astral-sh/uv/releases/download/$UV_VERSION/uv-$UV_PLATFORM.tar.gz"
    TEMP_TAR=$(mktemp)
    TEMP_EXTRACT=$(mktemp -d)
    
    echo "Downloading uv from: $UV_URL"
    if command -v curl >/dev/null 2>&1; then
        curl -LsSf "$UV_URL" -o "$TEMP_TAR"
    elif command -v wget >/dev/null 2>&1; then
        wget -qO "$TEMP_TAR" "$UV_URL"
    else
        echo "ERROR: Neither curl nor wget was found."
        exit 1
    fi
    
    echo "Extracting uv..."
    tar -xzf "$TEMP_TAR" -C "$TEMP_EXTRACT"
    
    # Locate the uv binary in the extracted directories
    EXTRACTED_UV=$(find "$TEMP_EXTRACT" -name "uv" -type f | head -n 1)
    if [ -z "$EXTRACTED_UV" ]; then
        echo "ERROR: Failed to find uv binary in extracted files"
        rm -f "$TEMP_TAR"
        rm -rf "$TEMP_EXTRACT"
        exit 1
    fi
    
    cp "$EXTRACTED_UV" "$UV_EXE"
    chmod +x "$UV_EXE"
    
    rm -f "$TEMP_TAR"
    rm -rf "$TEMP_EXTRACT"
    echo "Portable uv installed successfully at: $UV_EXE"
else
    echo "[1/5] Portable uv already exists at: $UV_EXE"
fi

# Verify uv runs
"$UV_EXE" --version

# 3. Download/Ensure Python 3.12.13 via uv
echo "[2/5] Installing portable Python 3.12.13..."
"$UV_EXE" python install 3.12.13

# 4. Create virtual environment
echo "[3/5] Creating standalone virtual environment..."
if [ -d "$VENV_DIR" ]; then
    echo "Removing existing venv folder..."
    rm -rf "$VENV_DIR"
fi

# We use --copy to make the virtual environment fully relocatable and self-contained
"$UV_EXE" venv --python 3.12.13 --copy "$VENV_DIR"

VENV_PYTHON="$VENV_DIR/bin/python"
echo "Venv created successfully using Python: $($VENV_PYTHON --version)"

# Set env var so uv knows which venv to target
export VIRTUAL_ENV="$VENV_DIR"

# Helper to run uv pip
run_uv_pip() {
    local label="$1"
    shift
    echo "Installing $label..."
    "$UV_EXE" pip install --python "$VENV_PYTHON" "$@"
}

# 5. Install Speech dependencies
echo "[4/5] Installing pip dependencies..."

# Install core/non-piper packages first
run_uv_pip "kokoro-tts" "kokoro-tts"
run_uv_pip "pocket-tts" "pocket-tts"
run_uv_pip "kittentts" "https://github.com/KittenML/KittenTTS/releases/download/0.8.1/kittentts-0.8.1-py3-none-any.whl"
run_uv_pip "sherpa-onnx" "sherpa-onnx"

# Install CPU PyTorch to keep it lightweight for pocket-tts voice cloning
run_uv_pip "torch (CPU)" "torch"

# Install soundfile and numpy
run_uv_pip "soundfile & numpy" "soundfile" "numpy"

# Install piper-tts
run_uv_pip "piper-tts" "piper-tts[http]"

# On macOS and standard Linux, we use the standard onnxruntime CPU package
# On Linux with CUDA, we can optionally use onnxruntime-gpu if nvidia-smi is available
IS_LINUX_CUDA=false
if [ "$(uname -s)" = "Linux" ] && command -v nvidia-smi >/dev/null 2>&1; then
    IS_LINUX_CUDA=true
fi

if [ "$IS_LINUX_CUDA" = true ]; then
    echo "NVIDIA GPU detected on Linux. Setting up GPU acceleration (CUDA 13.3 nightly)..."
    
    # Remove conflicting CPU onnxruntime (pulled in by piper-tts)
    echo "Removing conflicting CPU onnxruntime..."
    "$UV_EXE" pip uninstall --python "$VENV_PYTHON" onnxruntime --yes --quiet || true
    
    # Install onnxruntime-gpu dependencies first (required by the CUDA 13 nightly build)
    run_uv_pip "onnxruntime-gpu dependencies" \
        "coloredlogs" "flatbuffers" "numpy" "packaging" "protobuf" "sympy" "sentencepiece"
    
    # Install onnxruntime-gpu nightly for CUDA 13.3 from the Azure DevOps feed
    echo "Installing onnxruntime-gpu (CUDA 13.3 nightly)..."
    "$UV_EXE" pip install --python "$VENV_PYTHON" \
        --pre \
        --index-url "https://aiinfra.pkgs.visualstudio.com/PublicPackages/_packaging/ort-cuda-13-nightly/pypi/simple/" \
        onnxruntime-gpu
    
    # Final safety check: ensure CPU onnxruntime has not been re-added
    "$UV_EXE" pip uninstall --python "$VENV_PYTHON" onnxruntime --yes --quiet || true
else
    echo "Installing standard ONNX Runtime..."
    # Uninstall any leftover onnxruntime-gpu
    "$UV_EXE" pip uninstall --python "$VENV_PYTHON" onnxruntime-gpu --yes --quiet || true
    run_uv_pip "onnxruntime & sentencepiece" "onnxruntime>=1.26.0" "sentencepiece"
fi

# 6. Verify environment
echo "[5/5] Verifying environment..."
$VENV_PYTHON -c "
import sys
import onnxruntime as ort
import soundfile
import numpy
print(f'Python version: {sys.version}')
print(f'ONNX Runtime: {ort.__version__}')
print(f'Device: {ort.get_device()}')
providers = ort.get_available_providers()
print('Available providers:')
for p in providers:
    print(f'  - {p}')
"

echo "========================================"
echo "  S2B2S Speech Runtime Setup Complete!"
echo "========================================"
