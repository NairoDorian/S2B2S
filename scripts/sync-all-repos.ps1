<#
  sync-all-repos.ps1 — Sync all third-party repositories and git dependencies for S2B2S

  Usage:
    .\scripts\sync-all-repos.ps1
#>

$ErrorActionPreference = "Continue"

$repoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $repoRoot

Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "  S2B2S Repository & Upstream Dependency Auto-Sync" -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan
Write-Host ""

# 1. Sync faster-qwen3-tts
$FasterQwenDir = Join-Path $repoRoot "..\faster-qwen3-tts"
if (Test-Path $FasterQwenDir) {
    Write-Host "[1/3] Syncing faster-qwen3-tts (andimarafioti/faster-qwen3-tts)..." -ForegroundColor Yellow
    git -C $FasterQwenDir pull 2>&1 | Out-Host
    $VenvPython = Join-Path $repoRoot "venv\Scripts\python.exe"
    if ((Test-Path $VenvPython) -and (Get-Command uv -ErrorAction SilentlyContinue)) {
        Write-Host "      Updating venv linkage..." -ForegroundColor Gray
        uv pip install --no-deps -e $FasterQwenDir --python $VenvPython --quiet 2>&1 | Out-Host
    }
} else {
    Write-Host "[1/3] faster-qwen3-tts directory not found at $FasterQwenDir" -ForegroundColor Gray
}

# 2. Sync Cargo git dependencies (transcribe-cpp, transcribe-cpp-sys, hf-hub, tao)
Write-Host "`n[2/3] Checking latest commits for Rust git dependencies (transcribe-cpp, hf-hub, tao)..." -ForegroundColor Yellow
$SrcTauri = Join-Path $repoRoot "src-tauri"
if (Get-Command cargo -ErrorAction SilentlyContinue) {
    Push-Location $SrcTauri
    cargo update -p transcribe-cpp -p transcribe-cpp-sys -p hf-hub 2>&1 | Out-Host
    Pop-Location
}

# 3. Sync sibling project repositories if present in parent folder
Write-Host "`n[3/3] Checking sibling repositories in workspace..." -ForegroundColor Yellow
$ParentDir = Split-Path -Parent $repoRoot
$SiblingRepos = @("Handy", "copyspeak", "AIVORelay", "parler", "transcribe.cpp", "qwentts.cpp")

foreach ($sibling in $SiblingRepos) {
    $siblingPath = Join-Path $ParentDir $sibling
    if ((Test-Path $siblingPath) -and (Test-Path (Join-Path $siblingPath ".git"))) {
        Write-Host "  -> Syncing $sibling..." -ForegroundColor Cyan
        git -C $siblingPath pull 2>&1 | Out-Host
    }
}

Write-Host "`n[OK] All repositories and git dependencies are synchronized to latest commits!" -ForegroundColor Green
