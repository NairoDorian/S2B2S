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
    uv pip install --python $VenvPython --no-cache --force-reinstall @Packages 2>&1 | Out-Host
    if ($LASTEXITCODE -ne 0) { Write-Host "  FAILED: $Label" -ForegroundColor Red; exit 1 }
}

function Uninstall-Pkg([string]$Label, [string]$Package) {
    Write-Host "  -> $Label" -ForegroundColor Gray
    uv pip uninstall --python $VenvPython $Package --quiet 2>&1 | Out-Host
}

# ── Install non-piper deps first ──
Install-Pkg "kokoro-tts"           @("kokoro-tts")
Install-Pkg "pocket-tts"           @("pocket-tts")
Install-Pkg "kittentts (wheel)"    @("https://github.com/KittenML/KittenTTS/releases/download/0.8.1/kittentts-0.8.1-py3-none-any.whl")
Install-Pkg "sherpa-onnx"          @("sherpa-onnx")
Install-Pkg "torch"                @("torch")
Install-Pkg "soundfile, numpy"     @("soundfile", "numpy")
Install-Pkg "librosa, numba"       @("librosa>=0.10.0", "numba>=0.59.0")
Install-Pkg "transformers, hub"    @("transformers>=4.57,<5", "huggingface-hub>=0.36.0,<1.0", "accelerate")
Install-Pkg "qwen-tts (--no-deps)" @("qwen-tts", "--no-deps")
$LocalFasterQwen = Join-Path $ProjectRoot "..\faster-qwen3-tts"
if (Test-Path $LocalFasterQwen) {
    Write-Host "  -> Updating local faster-qwen3-tts to latest commit..." -ForegroundColor Yellow
    git -C $LocalFasterQwen pull 2>&1 | Out-Host
    Install-Pkg "faster-qwen3-tts (local editable)" @("--no-deps", "-e", $LocalFasterQwen)
} else {
    Install-Pkg "faster-qwen3-tts (latest GitHub)" @("--no-deps", "git+https://github.com/andimarafioti/faster-qwen3-tts.git")
}



# ── Install piper LAST (pulls CPU onnxruntime) ──
Install-Pkg "piper-tts[http]"      @("piper-tts[http]")

# ── Install build/runtime deps ──
Install-Pkg "coloredlogs, flatbuffers, packaging, protobuf, sympy" @("coloredlogs", "flatbuffers", "packaging", "protobuf", "sympy")

# ── Remove CPU onnxruntime (pulled in by piper-tts) ──
Uninstall-Pkg "removing CPU onnxruntime" "onnxruntime"

# ── Install sentencepiece & onnxruntime-gpu (from PyPI) ──
Install-Pkg "sentencepiece" @("sentencepiece")
Install-Pkg "onnxruntime-gpu" @("onnxruntime-gpu")

# ── Install NVIDIA CUDA 13 runtime DLLs (for piper child process PATH injection) ──
$cuda13Packages = @(
    "nvidia-cuda-runtime"
    "nvidia-cudnn-cu13"
    "nvidia-cublas"
    "nvidia-cufft"
    "nvidia-cusolver"
    "nvidia-cusparse"
    "nvidia-nvjitlink"
)
foreach ($pkg in $cuda13Packages) {
    Install-Pkg "$pkg" @($pkg)
}

# ── Final safety: purge any leftover CPU onnxruntime ──
Write-Host "  -> onnxruntime (final purge)" -ForegroundColor Gray
uv pip uninstall --python $VenvPython onnxruntime --quiet 2>&1 | Out-Host

Write-Host "[5/6] Verifying CUDA..." -ForegroundColor Yellow
$verify = & $VenvPython -c @"
import os, glob, nvidia

# --- DLL injection (mirrors get_nvidia_dll_paths in piper_server.rs) ---
nvidia_dir = list(nvidia.__path__)[0]
bin_dirs = glob.glob(os.path.join(nvidia_dir, '*', 'bin'))
sub_dirs = glob.glob(os.path.join(nvidia_dir, '*', 'bin', '*'))
all_dirs = [p for p in bin_dirs + sub_dirs if os.path.isdir(p)]
print(f'NVIDIA DLL dirs: {len(all_dirs)}')
for d in all_dirs:
    print(f'  {d}')

# --- ORT provider check ---
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
