# S2B2S TTS Virtual Environment Setup (Windows)
# Creates a Python venv and installs all TTS engine dependencies.
# Run this once after cloning the repo or after pulling new TTS features.

param(
    [switch]$SkipPipUpgrade
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProjectRoot = Resolve-Path "$ScriptDir\.."
$VenvDir = Join-Path $ProjectRoot "venv"

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  S2B2S — TTS Virtual Environment Setup" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

# Find system Python
$PythonCmd = $null
foreach ($candidate in @("python", "python3", "py")) {
    try {
        $null = & $candidate --version 2>&1
        $PythonCmd = $candidate
        break
    } catch { }
}
if (-not $PythonCmd) {
    Write-Host "ERROR: Python not found. Install Python 3.8+ from https://www.python.org/downloads/" -ForegroundColor Red
    exit 1
}
Write-Host "[1/5] Using Python: $( & $PythonCmd --version 2>&1 )" -ForegroundColor Green

# Create venv
if (-not (Test-Path $VenvDir)) {
    Write-Host "[2/5] Creating virtual environment at: $VenvDir" -ForegroundColor Yellow
    & $PythonCmd -m venv $VenvDir
} else {
    Write-Host "[2/5] Virtual environment already exists at: $VenvDir" -ForegroundColor Yellow
}

# Activate venv and get paths
$VenvPython = Join-Path $VenvDir "Scripts\python.exe"
$VenvPip = Join-Path $VenvDir "Scripts\pip.exe"

if (-not $SkipPipUpgrade) {
    Write-Host "[3/5] Upgrading pip..." -ForegroundColor Yellow
    & $VenvPython -m pip install --upgrade pip --quiet
}

# Install TTS engines
Write-Host "[4/5] Installing TTS engine packages..." -ForegroundColor Yellow

# Piper TTS (with HTTP server support)
Write-Host "  -> piper-tts[http]" -ForegroundColor Gray
& $VenvPip install "piper-tts[http]" --quiet

# Kokoro TTS
Write-Host "  -> kokoro-tts" -ForegroundColor Gray
& $VenvPip install kokoro-tts --quiet

# Pocket TTS
Write-Host "  -> pocket-tts" -ForegroundColor Gray
& $VenvPip install pocket-tts --quiet

# Kitten TTS (from wheel)
Write-Host "  -> kittentts (from wheel)" -ForegroundColor Gray
& $VenvPip install "https://github.com/KittenML/KittenTTS/releases/download/0.8.1/kittentts-0.8.1-py3-none-any.whl" --quiet

# Audio dependencies
Write-Host "  -> soundfile, numpy" -ForegroundColor Gray
& $VenvPip install soundfile numpy --quiet

# Pocket-specific dependencies (PyTorch for CPU)
Write-Host "  -> torch (CPU)" -ForegroundColor Gray
& $VenvPip install torch --quiet

Write-Host "[5/5] Setup complete!" -ForegroundColor Green
Write-Host ""
Write-Host "Virtual environment: $VenvDir" -ForegroundColor White
Write-Host "Python: $VenvPython" -ForegroundColor White
Write-Host ""
Write-Host "Installed packages:" -ForegroundColor Cyan
& $VenvPip list | Select-String -Pattern "piper|kokoro|pocket|kittentts|torch|soundfile"

Write-Host ""
Write-Host "S2B2S will automatically use this venv for all TTS engines." -ForegroundColor Green
