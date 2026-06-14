#!/usr/bin/env bash
# Export Parakeet models to sherpa-onnx streaming format.
#
# Usage:
#   bash setup_venv.sh          # one-time: create venv + install deps
#   bash export_unified.sh      # export Unified 0.6B (int8 streaming)
#   bash export_eou.sh          # export EOU 120M (int8 streaming, if nemo checkpoint available)

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
VENV_DIR="$SCRIPT_DIR/venv"

if [ ! -d "$VENV_DIR" ]; then
    echo "=== Creating venv ==="
    python3 -m venv "$VENV_DIR"
fi

source "$VENV_DIR/bin/activate"

echo "=== Installing dependencies ==="
pip install --upgrade pip

# NeMo from source (needed for model loading + ONNX export)
pip install "nemo_toolkit[asr] @ git+https://github.com/NVIDIA/NeMo.git"

# Additional deps for ONNX export
pip install "numpy<2" \
    kaldi-native-fbank \
    librosa \
    onnx \
    onnxruntime \
    soundfile \
    ipython

echo "=== Venv ready at $VENV_DIR ==="
echo "Run: source $VENV_DIR/bin/activate && python3 export_onnx_streaming.py --help"
