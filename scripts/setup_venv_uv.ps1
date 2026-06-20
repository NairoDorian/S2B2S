# S2B2S TTS Virtual Environment Setup (uv + Python 3.12)
# Uses uv to manage Python + venv, installs all TTS/STT engine deps.
# Run: powershell -ExecutionPolicy Bypass .\scripts\setup_venv_uv.ps1

$ErrorActionPreference = "Continue"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProjectRoot = Resolve-Path "$ScriptDir\.."
$VenvDir = Join-Path $ProjectRoot "venv"

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  S2B2S - uv Venv Setup (Python 3.12)  " -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan

# 1. Ensure uv is available
if (-not (Get-Command uv -ErrorAction SilentlyContinue)) {
    Write-Host "ERROR: uv not found. Install from https://docs.astral.sh/uv/" -ForegroundColor Red
    exit 1
}
Write-Host "[1/6] uv version: $(uv --version)" -ForegroundColor Green

# 2. Install Python 3.12 via uv (no system-wide install)
Write-Host "[2/6] Ensuring Python 3.12..." -ForegroundColor Yellow
$pyCheck = uv python list 2>&1 | Select-String "3.12"
if (-not $pyCheck) {
    uv python install 3.12 2>&1
    if ($LASTEXITCODE -ne 0) { exit 1 }
} else {
    Write-Host "  Python 3.12 already available" -ForegroundColor Gray
}

# 3. Create venv
Write-Host "[3/6] Creating venv..." -ForegroundColor Yellow
if (Test-Path $VenvDir) {
    Remove-Item -Recurse -Force $VenvDir -ErrorAction Stop
}
uv venv --python 3.12 $VenvDir 2>&1 | Out-Host
if ($LASTEXITCODE -ne 0) { exit 1 }
$env:VIRTUAL_ENV = $VenvDir
$VenvPython = Join-Path $VenvDir "Scripts\python.exe"

Write-Host "[4/6] Installing packages..." -ForegroundColor Yellow

function Install-Pkg([string]$Label, [string[]]$Packages) {
    Write-Host "  -> $Label" -ForegroundColor Gray
    uv pip install @Packages 2>&1 | Out-Host
    if ($LASTEXITCODE -ne 0) { Write-Host "  FAILED: $Label" -ForegroundColor Red; exit 1 }
}

function Uninstall-Pkg([string]$Label, [string]$Package) {
    Write-Host "  -> $Label" -ForegroundColor Gray
    uv pip uninstall $Package --quiet 2>&1 | Out-Host
}

# ── Install non-piper deps first ──
Install-Pkg "kokoro-tts"           @("kokoro-tts")
Install-Pkg "pocket-tts"           @("pocket-tts")
Install-Pkg "kittentts (wheel)"    @("https://github.com/KittenML/KittenTTS/releases/download/0.8.1/kittentts-0.8.1-py3-none-any.whl")
Install-Pkg "sherpa-onnx"          @("sherpa-onnx")
Install-Pkg "torch (CPU)"          @("torch")
Install-Pkg "soundfile, numpy"     @("soundfile", "numpy")

# ── Install piper LAST (pulls CPU onnxruntime) ──
Install-Pkg "piper-tts[http]"      @("piper-tts[http]")

# ── Remove CPU onnxruntime, install GPU + CUDA DLLs ──
Uninstall-Pkg "removing CPU onnxruntime" "onnxruntime"
Install-Pkg "onnxruntime-gpu, sentencepiece" @("onnxruntime-gpu>=1.26.0", "sentencepiece")
Install-Pkg "nvidia CUDA runtime (cu12)" @(
    "nvidia-cuda-runtime-cu12",
    "nvidia-cudnn-cu12",
    "nvidia-cublas-cu12",
    "nvidia-cufft-cu12",
    "nvidia-curand-cu12",
    "nvidia-cusolver-cu12",
    "nvidia-cusparse-cu12",
    "nvidia-nvjitlink-cu12"
)

# ── Final safety: force GPU and purge any leftover CPU ──
Write-Host "  -> onnxruntime-gpu (final override)" -ForegroundColor Gray
uv pip install --force-reinstall "onnxruntime-gpu>=1.26.0" 2>&1 | Out-Host
uv pip uninstall onnxruntime --quiet 2>&1 | Out-Host

Write-Host "[5/6] Verifying CUDA..." -ForegroundColor Yellow
$verify = & $VenvPython -c @"
import onnxruntime as ort
print(f'ONNX Runtime: {ort.__version__}')
print(f'Device: {ort.get_device()}')
providers = ort.get_available_providers()
for p in providers:
    print(f'  - {p}')
if 'CUDAExecutionProvider' in providers:
    print('CUDA OK')
else:
    print('CUDA MISSING')
    exit(1)
"@ 2>&1
Write-Host $verify -ForegroundColor White
if ($LASTEXITCODE -ne 0) { Write-Host "CUDA verification FAILED" -ForegroundColor Red; exit 1 }

Write-Host "[6/6] Setup complete!" -ForegroundColor Green
Write-Host "Venv: $VenvDir" -ForegroundColor White
Write-Host "Python: $VenvPython" -ForegroundColor White
