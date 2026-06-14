# Export Unified Parakeet 0.6B to sherpa-onnx streaming INT8 format (560ms chunks, Windows).
#
# Prerequisites:
#   1. Run setup_venv.ps1 first (installs NeMo + deps)
#   2. Requires ~10 GB free disk space for the NeMo checkpoint + ONNX export
#
# Output:
#   sherpa-onnx-nemo-parakeet-unified-en-0.6b-int8-streaming-560ms/
#     encoder.int8.onnx    (~500 MB)
#     decoder.int8.onnx    (~20 MB)
#     joiner.int8.onnx     (~20 MB)
#     tokens.txt

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$VenvPython = Join-Path $ScriptDir "venv" "Scripts" "python.exe"

# Step 1: Download NeMo checkpoint if not present
$NemoFile = Join-Path $ScriptDir "parakeet-unified-en-0.6b.nemo"
if (-not (Test-Path $NemoFile)) {
    Write-Host "=== Downloading NeMo checkpoint (~2.6 GB) ===" -ForegroundColor Yellow
    $Url = "https://huggingface.co/nvidia/parakeet-unified-en-0.6b/resolve/main/parakeet-unified-en-0.6b.nemo"
    Invoke-WebRequest -Uri $Url -OutFile $NemoFile
} else {
    Write-Host "=== NeMo checkpoint already downloaded ===" -ForegroundColor Green
}

# Step 2: Run export
Write-Host "=== Exporting to ONNX (int8, streaming 560ms) ===" -ForegroundColor Yellow
Push-Location $ScriptDir
& $VenvPython export_onnx_streaming.py --latency 560ms
Pop-Location

# Step 3: Organize output
$OutDir = Join-Path $ScriptDir "sherpa-onnx-nemo-parakeet-unified-en-0.6b-int8-streaming-560ms"
New-Item -ItemType Directory -Path $OutDir -Force | Out-Null
$OutTestWavs = Join-Path $OutDir "test_wavs"
New-Item -ItemType Directory -Path $OutTestWavs -Force | Out-Null

Get-ChildItem -Path $ScriptDir -Filter "encoder.int8.onnx" | Move-Item -Destination $OutDir -Force
Get-ChildItem -Path $ScriptDir -Filter "decoder.int8.onnx" | Move-Item -Destination $OutDir -Force
Get-ChildItem -Path $ScriptDir -Filter "joiner.int8.onnx" | Move-Item -Destination $OutDir -Force
Get-ChildItem -Path $ScriptDir -Filter "tokens.txt" | Move-Item -Destination $OutDir -Force

Write-Host "=== Done! Output: $OutDir ===" -ForegroundColor Green
Get-ChildItem $OutDir | Format-Table Name, Length
