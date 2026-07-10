<# Build S2B2S on Windows via `bun tauri build` (or `bun tauri dev`) with the
# environment that makes the transcribe-cpp-sys (ggml-vulkan) native build succeed.
#
# Why this script exists:
#   transcribe-cpp-sys compiles ggml + the Vulkan backend with CMake. On Windows
#   that build fails in two environment-dependent ways that Handy's own CI works
#   around (see .github/workflows/build.yml):
#     1. The CMake build tree produces paths longer than Windows MAX_PATH (260).
#        cl.exe then cannot create its .pdb and reports C1041 "cannot open program
#        database", and the nested vulkan-shaders-gen ExternalProject's compiler
#        detection fails ("No CMAKE_C_COMPILER"). Fix: build into a SHORT target
#        directory (Handy sets CARGO_TARGET_DIR to "<drive>\t").
#     2. The nested shader-gen sub-build must inherit the MSVC tools (cl.exe,
#        INCLUDE, LIB). Fix: run the whole build inside a VS developer prompt.
#   VULKAN_SDK must be set so ggml-vulkan finds Vulkan + SPIRV-Headers.
#
# Usage:  .\scripts\build-windows.ps1            # => bun tauri build
#         .\scripts\build-windows.ps1 dev         # => bun tauri dev
#         .\scripts\build-windows.ps1 build --debug
#>
param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$TauriArgs = @("build")
)

$ErrorActionPreference = "Stop"

# Always run from the repository root so `bun tauri build` finds package.json.
$repoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $repoRoot

# Locate the VS developer prompt for the installed Visual Studio.
$vsRoot = "C:\Program Files\Microsoft Visual Studio\18\Community"
if (-not (Test-Path $vsRoot)) {
    $vsRoot = "C:\Program Files\Microsoft Visual Studio\17\Community"
}
$vcvars = Join-Path $vsRoot "VC\Auxiliary\Build\vcvarsall.bat"
if (-not (Test-Path $vcvars)) {
    # Fall back to any VS install under Program Files.
    $found = Get-ChildItem "C:\Program Files\Microsoft Visual Studio" -Recurse -Filter vcvarsall.bat -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($found) { $vcvars = $found.FullName } else { throw "vcvarsall.bat not found; install Visual Studio 2022/2019 with C++ desktop workload." }
}

# Short build directory to stay under MAX_PATH (mirrors Handy CI).
# Use the drive letter of the repo root and build an explicit "C:\bt" path
# (Split-Path -Qualifier returns "C:" and Join-Path produced a trailing space).
$driveLetter = (Get-Location).Drive.Name
$targetDir = "$driveLetter`:\bt"
New-Item -ItemType Directory -Force -Path $targetDir | Out-Null

# Default Vulkan SDK if the caller hasn't set it.
$vulkan = if ($env:VULKAN_SDK) { $env:VULKAN_SDK } else { "C:\VulkanSDK\1.4.350.0" }

$tauriCmd = "bun tauri " + ($TauriArgs -join " ")

# Run the entire build inside a VS developer prompt so the nested CMake
# ExternalProject inherits cl.exe + INCLUDE + LIB.
# NOTE: do NOT overwrite PATH here. cmd.exe inherits the PowerShell PATH
# (which already has bun/cargo), and `call vcvarsall x64` then appends the
# MSVC tools. Setting PATH=$env:PATH would clobber vcvars' additions (cl.exe,
# INCLUDE, LIB on PATH) and break the nested shader-gen sub-build.
$inner = "call `"$vcvars`" x64 && " +
         "set `"VULKAN_SDK=$vulkan`" && " +
         "set `"CARGO_TARGET_DIR=$targetDir`" && " +
         "bun tauri $($TauriArgs -join ' ')"

Write-Host "=== S2B2S Windows build ==="
Write-Host "vcvars : $vcvars"
Write-Host "target : $targetDir"
Write-Host "vulkan : $vulkan"
Write-Host "cmd    : $tauriCmd"
cmd.exe /c $inner
