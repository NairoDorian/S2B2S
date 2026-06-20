# S2B2S Speech Runtime Installer (Windows)
# Downloads portable uv, installs portable Python 3.12.13, creates venv, and installs dependencies.
# Used during the onboarding flow.

param(
    [Parameter(Mandatory=$true)]
    [string]$TargetDir
)

$ErrorActionPreference = "Stop"

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  S2B2S - Speech Runtime Installer" -ForegroundColor Cyan
Write-Host "  Target Directory: $TargetDir" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan

# 1. Ensure Target Directory exists
if (-not (Test-Path $TargetDir)) {
    Write-Host "Creating target directory: $TargetDir" -ForegroundColor Yellow
    New-Item -ItemType Directory -Force -Path $TargetDir | Out-Null
}

$VenvDir = Join-Path $TargetDir "venv"
$UvExe = Join-Path $TargetDir "uv.exe"

# 2. Download portable uv if not already present
if (-not (Test-Path $UvExe)) {
    Write-Host "[1/5] Downloading portable uv..." -ForegroundColor Yellow
    
    # Determine architecture
    $Arch = "x86_64"
    if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64" -or $env:PROCESSOR_ARCHITEW6432 -eq "ARM64") {
        $Arch = "aarch64"
        Write-Host "Detected ARM64 architecture" -ForegroundColor Gray
    } else {
        Write-Host "Detected x64 architecture" -ForegroundColor Gray
    }
    
    $UvVersion = "0.5.21"
    $UvUrl = "https://github.com/astral-sh/uv/releases/download/$UvVersion/uv-$Arch-pc-windows-msvc.zip"
    $TempZip = [System.IO.Path]::GetTempFileName() + ".zip"
    
    try {
        Write-Host "Downloading uv from: $UvUrl" -ForegroundColor Gray
        [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
        Invoke-WebRequest -Uri $UvUrl -OutFile $TempZip -UseBasicParsing
        
        Write-Host "Extracting uv..." -ForegroundColor Gray
        $TempExtract = Join-Path ([System.IO.Path]::GetTempPath()) ([System.Guid]::NewGuid().ToString())
        Expand-Archive -Path $TempZip -DestinationPath $TempExtract -Force
        
        # Find uv.exe in the extracted folder (it might be nested under uv-x86_64-pc-windows-msvc)
        $ExtractedUv = Get-ChildItem -Path $TempExtract -Filter "uv.exe" -Recurse | Select-Object -First 1
        if (-not $ExtractedUv) {
            throw "Failed to find uv.exe in extracted archive"
        }
        
        Copy-Item -Path $ExtractedUv.FullName -Destination $UvExe -Force
        Write-Host "Portable uv installed successfully at: $UvExe" -ForegroundColor Green
    }
    catch {
        Write-Host "ERROR downloading/extracting uv: $_" -ForegroundColor Red
        if (Test-Path $TempZip) { Remove-Item $TempZip -ErrorAction SilentlyContinue }
        if (Test-Path $TempExtract) { Remove-Item -Recurse $TempExtract -ErrorAction SilentlyContinue }
        exit 1
    }
    finally {
        if (Test-Path $TempZip) { Remove-Item $TempZip -ErrorAction SilentlyContinue }
        if (Test-Path $TempExtract) { Remove-Item -Recurse $TempExtract -ErrorAction SilentlyContinue }
    }
} else {
    Write-Host "[1/5] Portable uv already exists at: $UvExe" -ForegroundColor Green
}

# Verify uv runs
& $UvExe --version | Out-Host
if ($LASTEXITCODE -ne 0) {
    Write-Host "ERROR: Installed uv did not execute correctly." -ForegroundColor Red
    exit 1
}

# 3. Download/Ensure Python 3.12.13 via uv
Write-Host "[2/5] Installing portable Python 3.12.13..." -ForegroundColor Yellow
& $UvExe python install 3.12.13 2>&1 | Out-Host
if ($LASTEXITCODE -ne 0) {
    Write-Host "ERROR: Failed to install Python 3.12.13 via uv" -ForegroundColor Red
    exit 1
}

# 4. Create virtual environment
Write-Host "[3/5] Creating standalone virtual environment..." -ForegroundColor Yellow
if (Test-Path $VenvDir) {
    Write-Host "Removing existing venv folder..." -ForegroundColor Gray
    Remove-Item -Recurse -Force $VenvDir -ErrorAction Continue
}

# We use --copy to make the virtual environment fully relocatable and self-contained
& $UvExe venv --python 3.12.13 --copy $VenvDir 2>&1 | Out-Host
if ($LASTEXITCODE -ne 0) {
    Write-Host "ERROR: Failed to create venv" -ForegroundColor Red
    exit 1
}

$VenvPython = Join-Path $VenvDir "Scripts\python.exe"
Write-Host "Venv created successfully using Python: $(& $VenvPython --version)" -ForegroundColor Green

# Set env var so uv knows which venv to target
$env:VIRTUAL_ENV = $VenvDir

# Helper to run uv pip
function Run-UvPip([string]$Label, [string[]]$ArgsList) {
    Write-Host "Installing $Label..." -ForegroundColor Yellow
    # Pass --python explicitly to target our venv
    & $UvExe pip install --python $VenvPython @ArgsList 2>&1 | Out-Host
    if ($LASTEXITCODE -ne 0) {
        Write-Host "ERROR: Failed to install $Label" -ForegroundColor Red
        exit 1
    }
}

# 5. Install Speech dependencies
Write-Host "[4/5] Installing pip dependencies..." -ForegroundColor Yellow

# Install core/non-piper packages first
Run-UvPip "kokoro-tts" @("kokoro-tts")
Run-UvPip "pocket-tts" @("pocket-tts")
Run-UvPip "kittentts" @("https://github.com/KittenML/KittenTTS/releases/download/0.8.1/kittentts-0.8.1-py3-none-any.whl")
Run-UvPip "sherpa-onnx" @("sherpa-onnx")

# Install CPU PyTorch to keep it lightweight for pocket-tts voice cloning
Run-UvPip "torch (CPU)" @("torch", "--index-url", "https://download.pytorch.org/whl/cpu")

# Install soundfile and numpy
Run-UvPip "soundfile & numpy" @("soundfile", "numpy")

# Install piper-tts (which pulls CPU onnxruntime)
Run-UvPip "piper-tts" @("piper-tts[http]")

# Remove conflicting CPU onnxruntime (pulled in by piper-tts)
Write-Host "Removing conflicting CPU onnxruntime (if any)..." -ForegroundColor Gray
& $UvExe pip uninstall --python $VenvPython onnxruntime --yes --quiet 2>&1 | Out-Null

# Install onnxruntime-gpu dependencies first (required by the CUDA 13 nightly build)
Run-UvPip "onnxruntime-gpu dependencies" @("coloredlogs", "flatbuffers", "numpy", "packaging", "protobuf", "sympy", "sentencepiece")

# Install onnxruntime-gpu nightly for CUDA 13.3 from the Azure DevOps feed
Write-Host "Installing onnxruntime-gpu (CUDA 13.3 nightly)..." -ForegroundColor Yellow
& $UvExe pip install --python $VenvPython `
    --pre `
    --index-url "https://aiinfra.pkgs.visualstudio.com/PublicPackages/_packaging/ort-cuda-13-nightly/pypi/simple/" `
    onnxruntime-gpu 2>&1 | Out-Host
if ($LASTEXITCODE -ne 0) {
    Write-Host "ERROR: Failed to install onnxruntime-gpu from CUDA 13.3 nightly feed" -ForegroundColor Red
    exit 1
}

# Final safety check: ensure CPU onnxruntime has not been re-added
Write-Host "Verifying final GPU configuration..." -ForegroundColor Gray
& $UvExe pip uninstall --python $VenvPython onnxruntime --yes --quiet 2>&1 | Out-Null

# 6. Verify environment
Write-Host "[5/5] Verifying environment and CUDA availability..." -ForegroundColor Yellow
$VerifyCode = @"
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
if 'CUDAExecutionProvider' in providers:
    print('CUDA Status: OK')
else:
    print('CUDA Status: MISSING (falling back to CPU)')
"@

& $VenvPython -c $VerifyCode 2>&1 | Out-Host

Write-Host "========================================" -ForegroundColor Green
Write-Host "  S2B2S Speech Runtime Setup Complete!" -ForegroundColor Green
Write-Host "========================================" -ForegroundColor Green
