#Requires -Version 5.1
<#
.SYNOPSIS
    Ensures libclang.dll is available for whisper-rs-sys bindgen build dependency.

.DESCRIPTION
    whisper-rs-sys uses bindgen which needs libclang.dll at build time.
    On Windows, this script offers two methods:

    Method 1 (recommended): Install LLVM via winget.
        winget install LLVM.LLVM   (~450 MB installer, sets PATH automatically)
    
    Method 2: Download LLVM binary tarball and extract just libclang.dll.
        Downloads from GitHub, extracts with 7-Zip or tar.

    After either method:
        cargo check   (clang-sys auto-detects via PATH)

    Build-time only — not needed at runtime.

.NOTES
    The NuGet package 'libclang' does NOT contain the actual DLL — it's
    an MSBuild meta-package that references system-installed LLVM.
    You must have the real LLVM binaries. Use winget or the tarball.
#>

$ErrorActionPreference = "Stop"

# ── Method 1: winget ──────────────────────────────────────────────────
function Install-ViaWinget {
    Write-Host "Installing LLVM via winget..." -ForegroundColor Cyan
    Write-Host "  This will download ~450 MB and install LLVM. It's the" -ForegroundColor Gray
    Write-Host "  recommended approach — bin/libclang.dll is added to PATH automatically." -ForegroundColor Gray

    $confirmed = Read-Host "Proceed? (Y/n)"
    if ($confirmed -eq "" -or $confirmed -eq "y" -or $confirmed -eq "Y") {
        winget install -e --id LLVM.LLVM --accept-package-agreements --accept-source-agreements
        if ($LASTEXITCODE -eq 0) {
            Write-Host "LLVM installed!" -ForegroundColor Green
            Write-Host "  You may need to restart your terminal, then run:" -ForegroundColor Yellow
            Write-Host "    cd src-tauri"
            Write-Host "    cargo check"
            return $true
        } else {
            Write-Host "winget install failed (exit code: $LASTEXITCODE)" -ForegroundColor Red
            return $false
        }
    } else {
        Write-Host "Skipping winget install." -ForegroundColor Yellow
        return $false
    }
}

# ── Method 2: Download binary tarball ─────────────────────────────────
function Install-ViaTarball {
    Write-Host "Downloading LLVM binary tarball (x86_64)..." -ForegroundColor Cyan
    Write-Host "  We'll extract only bin/libclang.dll (~5 MB)." -ForegroundColor Gray

    $TargetDir = Join-Path (Join-Path $env:USERPROFILE ".rustup") "libclang"
    New-Item -ItemType Directory -Path $TargetDir -Force | Out-Null
    $DllPath = Join-Path $TargetDir "libclang.dll"

    if (Test-Path $DllPath) {
        Write-Host "libclang.dll already exists at: $DllPath" -ForegroundColor Green
        return $true
    }

    # LLVM 22.1.8 x86_64 Windows binary tarball
    $Url = "https://github.com/llvm/llvm-project/releases/download/llvmorg-22.1.8/clang+llvm-22.1.8-x86_64-pc-windows-msvc.tar.xz"
    $ArchivePath = Join-Path $env:TEMP "llvm.tar.xz"
    $ExtractDir  = Join-Path $env:TEMP "llvm_extracted"

    Write-Host "  Downloading (862 MB — this will take a while)..." -ForegroundColor Yellow
    try {
        $wc = New-Object System.Net.WebClient
        $wc.DownloadFile($Url, $ArchivePath)
    } catch {
        Write-Host "Download failed: $_" -ForegroundColor Red
        return $false
    }

    Write-Host "  Extracting libclang.dll..." -ForegroundColor Cyan
    New-Item -ItemType Directory -Path $ExtractDir -Force | Out-Null
    
    # Check if tar.exe supports .tar.xz (Windows 10+ build 17063+)
    $tarResult = & tar -xf $ArchivePath -C $ExtractDir --wildcards "*/bin/libclang.dll" 2>&1
    if ($LASTEXITCODE -eq 0) {
        # tar succeeded — find the extracted DLL
        $found = Get-ChildItem -Path $ExtractDir -Recurse -Filter "libclang.dll" | Select-Object -First 1
        if ($found) {
            Copy-Item $found.FullName $DllPath -Force
        }
    } else {
        # tar doesn't support .xz — try 7-Zip
        $sz = Get-Command "7z" -ErrorAction SilentlyContinue
        if (-not $sz) {
            $sz = Get-Command "7za" -ErrorAction SilentlyContinue
        }
        if ($sz) {
            & $sz.Source x $ArchivePath "-o$ExtractDir" -y "*.dll" -r | Out-Null
        } else {
            Write-Host "Cannot extract .tar.xz. Install 7-Zip or use winget method." -ForegroundColor Red
            Remove-Item $ArchivePath -Force -ErrorAction SilentlyContinue
            return $false
        }
    }

    $found = Get-ChildItem -Path $ExtractDir -Recurse -Filter "libclang.dll" | Select-Object -First 1
    if (-not $found) {
        Write-Host "Could not find libclang.dll in the extracted archive." -ForegroundColor Red
        return $false
    }

    Copy-Item $found.FullName $DllPath -Force

    # Cleanup
    Remove-Item $ArchivePath -Force -ErrorAction SilentlyContinue
    Remove-Item $ExtractDir -Recurse -Force -ErrorAction SilentlyContinue

    Write-Host "Done!" -ForegroundColor Green
    Write-Host "  libclang.dll -> $DllPath ($((Get-Item $DllPath).Length / 1MB) MB)" -ForegroundColor Green
    Write-Host ""
    Write-Host "Set this env var and build:" -ForegroundColor Cyan
    Write-Host "  `$env:LIBCLANG_PATH = '$TargetDir'"
    Write-Host "  cd src-tauri"
    Write-Host "  cargo check"
    return $true
}

# ── Main ──────────────────────────────────────────────────────────────
Write-Host "=== libclang.dll for S2B2S build ===" -ForegroundColor Cyan
Write-Host ""
Write-Host "whisper-rs-sys (via bindgen) needs libclang.dll at build time."
Write-Host ""

# Check if already installed system-wide
$existing = Get-ChildItem -Path "$env:ProgramFiles\LLVM\bin\libclang.dll" -ErrorAction SilentlyContinue
if (-not $existing) {
    $existing = Get-ChildItem -Path "${env:ProgramFiles(x86)}\LLVM\bin\libclang.dll" -ErrorAction SilentlyContinue
}
if ($existing) {
    Write-Host "Found existing LLVM installation:" -ForegroundColor Green
    Write-Host "  $($existing.FullName)" -ForegroundColor Green
    Write-Host "  cargo check should detect this automatically via PATH." -ForegroundColor Green
    exit 0
}

Write-Host "Choose an option:" -ForegroundColor Yellow
Write-Host "  [1] Install via winget (recommended — sets PATH automatically)" -ForegroundColor White
Write-Host "  [2] Download binary tarball + extract just libclang.dll (862 MB download)" -ForegroundColor White
Write-Host "  [3] Skip — I'll install manually" -ForegroundColor White

$choice = Read-Host "Enter 1, 2, or 3"
switch ($choice) {
    "1" { $ok = Install-ViaWinget }
    "2" { $ok = Install-ViaTarball }
    default { $ok = $false }
}

if (-not $ok) {
    Write-Host ""
    Write-Host "Alternative — install manually:" -ForegroundColor Yellow
    Write-Host "  winget install -e --id LLVM.LLVM"
    Write-Host "  # or: scoop install llvm"
    Write-Host "  # or: choco install llvm"
    Write-Host "  # or: download from https://github.com/llvm/llvm-project/releases"
    Write-Host ""
    Write-Host "Then restart terminal and run:"
    Write-Host "  cd src-tauri"
    Write-Host "  cargo check"
}
