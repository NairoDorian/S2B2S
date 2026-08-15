<#
  compile-audiocpp.ps1 — Compile audio.cpp native server/CLI for S2B2S on Windows (CUDA, Vulkan, or CPU)

  Usage:
    .\scripts\compile-audiocpp.ps1 -Backend cuda
    .\scripts\compile-audiocpp.ps1 -Backend vulkan -Target audiocpp_server
    .\scripts\compile-audiocpp.ps1 -Backend cpu
#>

param(
    [ValidateSet("cuda", "vulkan", "cpu")]
    [string]$Backend = "cuda",

    [ValidateSet("audiocpp_server", "audiocpp_cli", "all")]
    [string]$Target = "audiocpp_server",

    [string]$ModelSet = "full",

    [string]$AudioCppDir = "..\audio.cpp"
)

$ErrorActionPreference = "Stop"

Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "  audio.cpp Native Inference Compiler (Backend: $Backend)" -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan

# 1. Resolve repository directory
$S2B2S_Dir = Split-Path -Parent $PSScriptRoot
$ResolvedAudioCpp = Join-Path $S2B2S_Dir $AudioCppDir

if (-not (Test-Path $ResolvedAudioCpp)) {
    Write-Host "audio.cpp repository not found at $ResolvedAudioCpp. Cloning..." -ForegroundColor Yellow
    & git clone --recurse-submodules https://github.com/0xShug0/audio.cpp $ResolvedAudioCpp
}

# 2. Select CMake Preset
$Preset = switch ($Backend) {
    "cuda"   { "windows-cuda-release" }
    "vulkan" { "windows-vulkan-release" }
    "cpu"    { "windows-cpu-release" }
}

Write-Host "Building target '$Target' with preset '$Preset' in $ResolvedAudioCpp..." -ForegroundColor Yellow

$BuildWindowsScript = Join-Path $ResolvedAudioCpp "scripts\build_windows.ps1"
if (-not (Test-Path $BuildWindowsScript)) {
    Write-Error "build_windows.ps1 not found at $BuildWindowsScript"
}

# 3. Execute audio.cpp build script (with -DeploymentBuild to bake model contract specs directly into the binary)
if ($Target -eq "all") {
    & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $BuildWindowsScript -Preset $Preset -ModelSet $ModelSet -DeploymentBuild
} else {
    & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $BuildWindowsScript -Preset $Preset -Target $Target -ModelSet $ModelSet -DeploymentBuild
}

if ($LASTEXITCODE -ne 0) {
    Write-Error "Compilation of audio.cpp failed with exit code $LASTEXITCODE"
}

# 4. Stage compiled binaries into S2B2S resources directory
$BuildOutputDir = Join-Path $ResolvedAudioCpp "build\$Preset\bin"
if (-not (Test-Path $BuildOutputDir)) {
    $BuildOutputDir = Join-Path $ResolvedAudioCpp "build\$Preset\Release\bin"
}
if (-not (Test-Path $BuildOutputDir)) {
    $BuildOutputDir = Join-Path $ResolvedAudioCpp "build\$Preset\Release"
}

$S2B2S_BinDir = Join-Path $S2B2S_Dir "src-tauri\resources\binaries"
if (-not (Test-Path $S2B2S_BinDir)) {
    New-Item -ItemType Directory -Force -Path $S2B2S_BinDir | Out-Null
}

Write-Host "`nStaging compiled binaries from $BuildOutputDir to $S2B2S_BinDir..." -ForegroundColor Yellow
if (Test-Path $BuildOutputDir) {
    Get-ChildItem -Path $BuildOutputDir -Filter "*.exe" | ForEach-Object {
        Copy-Item -Path $_.FullName -Destination $S2B2S_BinDir -Force
        Write-Host "  -> Staged $($_.Name)" -ForegroundColor Green
    }
    Get-ChildItem -Path $BuildOutputDir -Filter "*.dll" | ForEach-Object {
        Copy-Item -Path $_.FullName -Destination $S2B2S_BinDir -Force
        Write-Host "  -> Staged $($_.Name)" -ForegroundColor Green
    }
}

# 5. Stage model_specs directory
$AudioCppSpecsDir = Join-Path $ResolvedAudioCpp "model_specs"
$S2B2S_SpecsDir = Join-Path $S2B2S_Dir "src-tauri\resources\model_specs"
if (Test-Path $AudioCppSpecsDir) {
    if (-not (Test-Path $S2B2S_SpecsDir)) {
        New-Item -ItemType Directory -Force -Path $S2B2S_SpecsDir | Out-Null
    }
    Copy-Item -Path (Join-Path $AudioCppSpecsDir "*") -Destination $S2B2S_SpecsDir -Force
    Write-Host "  -> Staged model_specs catalog" -ForegroundColor Green
}

Write-Host "`n[OK] audio.cpp ($Backend) compilation and staging complete!" -ForegroundColor Green
