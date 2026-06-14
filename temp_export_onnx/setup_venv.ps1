# Export Parakeet models to sherpa-onnx streaming format (Windows).
#
# Usage:
#   powershell -File setup_venv.ps1           # one-time: create venv + install deps
#   powershell -File export_unified.ps1       # export Unified 0.6B (int8 streaming, 560ms)

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$VenvDir = Join-Path $ScriptDir "venv"

if (-not (Test-Path $VenvDir)) {
    Write-Host "=== Creating venv ===" -ForegroundColor Cyan
    python -m venv $VenvDir
}

$VenvPython = Join-Path $VenvDir "Scripts" "python.exe"
$VenvPip = Join-Path $VenvDir "Scripts" "pip.exe"

Write-Host "=== Installing NeMo (this takes a while...) ===" -ForegroundColor Yellow
& $VenvPip install --upgrade pip --quiet
& $VenvPip install "nemo_toolkit[asr] @ git+https://github.com/NVIDIA/NeMo.git"
& $VenvPip install "numpy<2" kaldi-native-fbank librosa onnx onnxruntime soundfile ipython --quiet

Write-Host "=== Venv ready at $VenvDir ===" -ForegroundColor Green
Write-Host "Run: & `"$VenvPython`" export_onnx_streaming.py --latency 560ms"
