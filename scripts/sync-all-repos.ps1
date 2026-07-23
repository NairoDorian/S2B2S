<#
  sync-all-repos.ps1 — Sync third-party dependency repositories and git packages for S2B2S

  Usage:
    .\scripts\sync-all-repos.ps1
#>

$ErrorActionPreference = "Continue"

$repoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $repoRoot

Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "  S2B2S Dependency Repository Auto-Sync" -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan
Write-Host ""

# 1. Sync faster-qwen3-tts
$FasterQwenDir = Join-Path $repoRoot "..\faster-qwen3-tts"
if (Test-Path $FasterQwenDir) {
    Write-Host "[1/2] Syncing faster-qwen3-tts (andimarafioti/faster-qwen3-tts)..." -ForegroundColor Yellow
    git -C $FasterQwenDir pull 2>&1 | Out-Host
    $VenvPython = Join-Path $repoRoot "venv\Scripts\python.exe"
    if ((Test-Path $VenvPython) -and (Get-Command uv -ErrorAction SilentlyContinue)) {
        Write-Host "      Updating venv linkage..." -ForegroundColor Gray
        uv pip install --no-deps -e $FasterQwenDir --python $VenvPython --quiet 2>&1 | Out-Host
    }
} else {
    Write-Host "[1/2] faster-qwen3-tts directory not found at $FasterQwenDir" -ForegroundColor Gray
}

# 2. Sync Cargo git dependencies (transcribe-cpp, transcribe-cpp-sys, hf-hub)
Write-Host "`n[2/2] Checking latest commits for Rust git dependencies (transcribe-cpp, hf-hub)..." -ForegroundColor Yellow
$SrcTauri = Join-Path $repoRoot "src-tauri"
if (Get-Command cargo -ErrorAction SilentlyContinue) {
    Push-Location $SrcTauri
    cargo update -p transcribe-cpp -p transcribe-cpp-sys -p hf-hub 2>&1 | Out-Host
    Pop-Location
}

Write-Host "`n[OK] Dependencies are synchronized to latest commits!" -ForegroundColor Green
