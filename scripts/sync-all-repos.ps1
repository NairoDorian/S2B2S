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

# 2. Smart Sync Cargo git dependencies (transcribe-cpp, hf-hub)
#    Only runs `cargo update` when remote commit differs from Cargo.lock,
#    preventing costly recompilation of transcribe.cpp C++ artifacts.
Write-Host "`n[2/2] Checking latest commits for Rust git dependencies..." -ForegroundColor Yellow
$SrcTauri = Join-Path $repoRoot "src-tauri"
$CargoLockPath = Join-Path $SrcTauri "Cargo.lock"

if (Get-Command cargo -ErrorAction SilentlyContinue) {
    $needsUpdate = @()

    if (Test-Path $CargoLockPath) {
        $lockContent = Get-Content $CargoLockPath -Raw

        # Check transcribe.cpp
        $transcribeRemote = (git ls-remote https://github.com/handy-computer/transcribe.cpp refs/heads/main 2>$null)
        if ($transcribeRemote -match "([0-9a-fA-F]{40})") {
            $remoteTranscribeSha = $matches[1]
            if ($lockContent -match 'github\.com/handy-computer/transcribe\.cpp\?branch=main#([0-9a-fA-F]{40})') {
                $lockedTranscribeSha = $matches[1]
                if ($remoteTranscribeSha -eq $lockedTranscribeSha) {
                    Write-Host "      [transcribe.cpp] Up-to-date ($($remoteTranscribeSha.Substring(0,8))). Skipping rebuild/update." -ForegroundColor Green
                } else {
                    Write-Host "      [transcribe.cpp] Update available ($($lockedTranscribeSha.Substring(0,8)) -> $($remoteTranscribeSha.Substring(0,8)))." -ForegroundColor Yellow
                    $needsUpdate += @("-p", "transcribe-cpp", "-p", "transcribe-cpp-sys")
                }
            } else {
                $needsUpdate += @("-p", "transcribe-cpp", "-p", "transcribe-cpp-sys")
            }
        }

        # Check hf-hub
        $hfHubRemote = (git ls-remote https://github.com/cjpais/hf-hub refs/heads/cancellable-downloads 2>$null)
        if ($hfHubRemote -match "([0-9a-fA-F]{40})") {
            $remoteHfSha = $matches[1]
            if ($lockContent -match 'github\.com/cjpais/hf-hub\?branch=cancellable-downloads#([0-9a-fA-F]{40})') {
                $lockedHfSha = $matches[1]
                if ($remoteHfSha -eq $lockedHfSha) {
                    Write-Host "      [hf-hub] Up-to-date ($($remoteHfSha.Substring(0,8)))." -ForegroundColor Green
                } else {
                    Write-Host "      [hf-hub] Update available ($($lockedHfSha.Substring(0,8)) -> $($remoteHfSha.Substring(0,8)))." -ForegroundColor Yellow
                    $needsUpdate += @("-p", "hf-hub")
                }
            } else {
                $needsUpdate += @("-p", "hf-hub")
            }
        }
    } else {
        $needsUpdate += @("-p", "transcribe-cpp", "-p", "transcribe-cpp-sys", "-p", "hf-hub")
    }

    if ($needsUpdate.Count -gt 0) {
        Write-Host "      Updating out-of-date Cargo packages: $($needsUpdate -join ' ')..." -ForegroundColor Yellow
        Push-Location $SrcTauri
        cargo update $needsUpdate 2>&1 | Out-Host
        Pop-Location
    } else {
        Write-Host "      All Rust git packages are synchronized. Build cache is preserved!" -ForegroundColor Green
    }
}

Write-Host "`n[OK] Dependencies check complete!" -ForegroundColor Green
