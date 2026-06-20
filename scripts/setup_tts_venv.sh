#!/usr/bin/env bash
# S2B2S TTS Virtual Environment Setup (macOS / Linux)
# Creates a Python venv and installs all TTS engine dependencies.
# Run: bash scripts/setup_tts_venv.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
VENV_DIR="$PROJECT_ROOT/venv"

echo "========================================"
echo "  S2B2S — TTS Virtual Environment Setup"
echo "========================================"
echo ""

# Find system Python
PYTHON_CMD=""
for candidate in python3 python python3.12 python3.11 python3.10; do
    if command -v "$candidate" &>/dev/null; then
        PYTHON_CMD="$candidate"
        break
    fi
done
if [ -z "$PYTHON_CMD" ]; then
    echo "ERROR: Python not found. Install Python 3.8+ (apt install python3, brew install python, etc.)"
    exit 1
fi
echo "[1/5] Using Python: $($PYTHON_CMD --version)"

# Create venv
if [ ! -d "$VENV_DIR" ]; then
    echo "[2/5] Creating virtual environment at: $VENV_DIR"
    $PYTHON_CMD -m venv "$VENV_DIR"
else
    echo "[2/5] Virtual environment already exists at: $VENV_DIR"
fi

# Activate venv
source "$VENV_DIR/bin/activate"
echo "[3/5] Upgrading pip..."
pip install --upgrade pip --quiet

# Install TTS engines
echo "[4/5] Installing TTS engine packages..."

echo "  -> piper-tts[http]"
pip install "piper-tts[http]" --quiet

echo "  -> kokoro-tts"
pip install kokoro-tts --quiet

echo "  -> pocket-tts"
pip install pocket-tts --quiet

echo "  -> kittentts (from wheel)"
pip install "https://github.com/KittenML/KittenTTS/releases/download/0.8.1/kittentts-0.8.1-py3-none-any.whl" --quiet

echo "  -> soundfile, numpy"
pip install soundfile numpy --quiet

# STT: Unified Parakeet + Piper TTS — ONNX Runtime
echo "  -> onnxruntime>=1.26.0, sentencepiece"
# NOTE: On Windows the .ps1 script installs onnxruntime-gpu for CUDA
pip install "onnxruntime>=1.26.0" sentencepiece --quiet

# STT: Nemotron (sherpa-onnx)
echo "  -> sherpa-onnx"
pip install sherpa-onnx --quiet

# PyTorch CPU-only to keep venv size reasonable
echo "  -> torch (CPU)"
pip install torch --quiet

echo "[5/5] Setup complete!"
echo ""
echo "Virtual environment: $VENV_DIR"
echo "Python: $VENV_DIR/bin/python"
echo ""
echo "Installed packages:"
pip list | grep -iE "piper|kokoro|pocket|kittentts|torch|soundfile"
echo ""
echo "S2B2S will automatically use this venv for all TTS engines."
