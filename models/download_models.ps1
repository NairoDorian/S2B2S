# S2B2S Model Downloader (Windows)
# Downloads STT, TTS, and Brain model files into a structured models/ directory.
#
# Usage:
#   .\download_models.ps1                                        # download all TTS models
#   .\download_models.ps1 -Model kokoro                          # download only Kokoro
#   .\download_models.ps1 -Model piper,pocket                    # download Piper + Pocket
#   .\download_models.ps1 -Model stt                             # download STT models
#   .\download_models.ps1 -Model brain                           # download Brain models
#   .\download_models.ps1 -Model all                             # download everything
#   .\download_models.ps1 -Path C:\my\models                     # custom target directory
#   .\download_models.ps1 -SetupVenv                             # also setup Python venv
#   .\download_models.ps1 -CleanVenv                             # clean and recreate venv
#   .\download_models.ps1 -Force                                 # re-download existing files
#   .\download_models.ps1 -DryRun                                # show what would happen
#
# Directory structure created:
#   <path>/
#     STT/           Speech-to-text models
#     Brain/         LLM / llama.cpp GGUF models
#     TTS/           Text-to-speech models
#       kokoro/      Kokoro-82M ONNX
#       piper-voices/  Piper voice files
#       pocket/      Pocket TTS (auto-downloaded by Python)
#       kitten/      Kitten TTS (auto-downloaded by Python)

param(
    [string]$Path,
    [string[]]$Model,
    [switch]$SetupVenv,
    [switch]$CleanVenv,
    [switch]$Force,
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
if (-not $Path) { $Path = $ScriptDir }
$ProjectRoot = Resolve-Path "$ScriptDir\.."

# Default to all TTS models if no -Model specified
if (-not $Model -or $Model.Count -eq 0) {
    $Model = @("piper", "kokoro", "pocket", "kitten")
}

# Expand aliases
$Expanded = @()
foreach ($m in $Model) {
    switch ($m.ToLower()) {
        "all"  { $Expanded += @("stt", "brain", "piper", "kokoro", "pocket", "kitten") }
        "tts"  { $Expanded += @("piper", "kokoro", "pocket", "kitten") }
        default { $Expanded += $m }
    }
}
$Model = $Expanded | Select-Object -Unique

# ── Helpers ───────────────────────────────────────────────────────────────
$TotalDownloaded = 0
$TotalSkipped = 0
$TotalFailed = 0

function Ensure-Dir($dir) {
    if (-not (Test-Path $dir)) {
        New-Item -ItemType Directory -Path $dir -Force | Out-Null
        Write-Host "  created: $dir"
    }
}

function Download-File($Url, $DestPath, $Description) {
    Ensure-Dir (Split-Path -Parent $DestPath)

    if ((Test-Path $DestPath) -and (-not $Force)) {
        $size = [math]::Round((Get-Item $DestPath).Length / 1MB, 2)
        Write-Host "  SKIP: $Description ($size MB) - already exists" -ForegroundColor Yellow
        $script:TotalSkipped++
        return
    }

    if ($DryRun) {
        Write-Host "  WOULD DOWNLOAD: $Description -> $DestPath"
        return
    }

    Write-Host "  DOWNLOAD: $Description..." -ForegroundColor Green
    try {
        Invoke-WebRequest -Uri $Url -OutFile $DestPath -UseBasicParsing
        $size = [math]::Round((Get-Item $DestPath).Length / 1MB, 2)
        Write-Host "    -> $size MB downloaded" -ForegroundColor Gray
        $script:TotalDownloaded++
    }
    catch {
        Write-Host "    -> FAILED: $_" -ForegroundColor Red
        $script:TotalFailed++
    }
}

# ── Venv Setup ────────────────────────────────────────────────────────────
if ($SetupVenv -or $CleanVenv) {
    Write-Host ""
    Write-Host "=== Python Virtual Environment ===" -ForegroundColor Cyan
    $VenvDir = Join-Path $ProjectRoot "venv"

    if ($CleanVenv -and (Test-Path $VenvDir)) {
        Write-Host "  Cleaning existing venv at: $VenvDir" -ForegroundColor Yellow
        Remove-Item -Recurse -Force $VenvDir
    }

    $setupScript = Join-Path $ProjectRoot "scripts\setup_tts_venv.ps1"
    if (Test-Path $setupScript) {
        Write-Host "  Running setup_tts_venv.ps1..." -ForegroundColor Green
        & $setupScript
    }
    else {
        Write-Host "  ERROR: setup_tts_venv.ps1 not found at $setupScript" -ForegroundColor Red
    }
}

# ── STT Models ────────────────────────────────────────────────────────────
if ($Model -contains "stt") {
    Write-Host ""
    Write-Host "============================================================" -ForegroundColor Magenta
    Write-Host "  STT Models -> $Path\STT\" -ForegroundColor Magenta
    Write-Host "============================================================" -ForegroundColor Magenta

    $SttDir = Join-Path $Path "STT"
    Ensure-Dir (Join-Path $SttDir "silero_vad")

    # Silero VAD (~1.7 MB)
    Download-File `
        -Url "https://blob.handy.computer/silero_vad_v4.onnx" `
        -DestPath (Join-Path $SttDir "silero_vad\silero_vad_v4.onnx") `
        -Description "Silero VAD v4 (~1.7 MB)"

    # Parakeet V3 (~600 MB)
    $parakeetTar = Join-Path $SttDir "parakeet-tdt-0.6b-v3-int8.tar.gz"
    $parakeetDir = Join-Path $SttDir "parakeet-tdt-0.6b-v3-int8"

    if ((Test-Path $parakeetDir) -and (-not $Force)) {
        Write-Host "  SKIP: Parakeet V3 (extracted) - already exists" -ForegroundColor Yellow
        $TotalSkipped++
    }
    else {
        if ((-not (Test-Path $parakeetTar)) -or $Force) {
            Download-File `
                -Url "https://blob.handy.computer/parakeet-v3-int8.tar.gz" `
                -DestPath $parakeetTar `
                -Description "Parakeet V3 (~600 MB)"
        }

        if ((Test-Path $parakeetTar) -and (-not $DryRun)) {
            Write-Host "  EXTRACT: Parakeet V3..." -ForegroundColor Green
            if (Test-Path $parakeetDir) { Remove-Item -Recurse -Force $parakeetDir }
            Ensure-Dir $parakeetDir
            & tar -xzf $parakeetTar -C $parakeetDir
            if ($?) {
                Write-Host "    -> Extracted to: $parakeetDir" -ForegroundColor Gray
                Remove-Item $parakeetTar -Force -ErrorAction SilentlyContinue
            }
        }
    }
}

# ── Brain Models ──────────────────────────────────────────────────────────
if ($Model -contains "brain") {
    Write-Host ""
    Write-Host "============================================================" -ForegroundColor Cyan
    Write-Host "  Brain Models -> $Path\Brain\llama_cpp\" -ForegroundColor Cyan
    Write-Host "============================================================" -ForegroundColor Cyan

    $BrainDir = Join-Path $Path "Brain\llama_cpp"
    Ensure-Dir $BrainDir

    $gemmaBase = "https://huggingface.co/unsloth/gemma-4-E2B-it-qat-GGUF/resolve/main"
    $gemmaFiles = @(
        "gemma-4-E2B-it-qat-UD-Q4_K_XL.gguf",
        "mmproj-F16.gguf",
        "mtp-gemma-4-E2B-it.gguf"
    )
    foreach ($f in $gemmaFiles) {
        Download-File `
            -Url "$gemmaBase/$f" `
            -DestPath (Join-Path $BrainDir $f) `
            -Description "Brain: $f"
    }

    Write-Host ""
    Write-Host "  NOTE: Place additional GGUF model files in: $BrainDir" -ForegroundColor Gray
}

# ── TTS Models ────────────────────────────────────────────────────────────
$TtsDir = Join-Path $Path "TTS"
Ensure-Dir $TtsDir

# --- Kokoro ---
if ($Model -contains "kokoro") {
    Write-Host ""
    Write-Host "============================================================" -ForegroundColor Cyan
    Write-Host "  Kokoro-82M -> $TtsDir\kokoro\" -ForegroundColor Cyan
    Write-Host "============================================================" -ForegroundColor Cyan

    $kokoroDir = Join-Path $TtsDir "kokoro"
    Ensure-Dir $kokoroDir

    Download-File `
        -Url "https://huggingface.co/hexgrad/Kokoro-82M/resolve/main/kokoro-v1.0.onnx" `
        -DestPath (Join-Path $kokoroDir "kokoro-v1.0.onnx") `
        -Description "Kokoro ONNX model (~330 MB)"

    Download-File `
        -Url "https://huggingface.co/hexgrad/Kokoro-82M/resolve/main/voices-v1.0.bin" `
        -DestPath (Join-Path $kokoroDir "voices-v1.0.bin") `
        -Description "Kokoro voices data (~330 MB)"
}

# --- Piper Voices ---
if ($Model -contains "piper") {
    Write-Host ""
    Write-Host "============================================================" -ForegroundColor Cyan
    Write-Host "  Piper Voices -> $TtsDir\piper-voices\" -ForegroundColor Cyan
    Write-Host "============================================================" -ForegroundColor Cyan

    $piperDir = Join-Path $TtsDir "piper-voices"
    Ensure-Dir $piperDir

    $piperVoices = @(
        "en_US-amy-low", "en_US-amy-medium",
        "en_US-arctic-medium",
        "en_US-bryce-medium",
        "en_US-danny-low",
        "en_US-hfc_female-medium", "en_US-hfc_male-medium",
        "en_US-joe-medium",
        "en_US-john-medium",
        "en_US-kathleen-low",
        "en_US-kristin-medium",
        "en_US-kusal-medium",
        "en_US-l2arctic-medium",
        "en_US-lessac-high", "en_US-lessac-low", "en_US-lessac-medium",
        "en_US-libritts-high", "en_US-libritts_r-medium",
        "en_US-ljspeech-high", "en_US-ljspeech-medium",
        "en_US-norman-medium",
        "en_US-reza_ibrahim-medium",
        "en_US-ryan-high", "en_US-ryan-low", "en_US-ryan-medium",
        "en_US-sam-medium"
    )

    $piperBase = "https://huggingface.co/rhasspy/piper-voices/resolve/main/en/en_US"

    foreach ($voice in $piperVoices) {
        $voiceName = $voice -replace "^en_US-", ""
        $quality = "medium"
        if ($voice -match "-low$") { $quality = "low" }
        if ($voice -match "-high$") { $quality = "high" }

        Download-File `
            -Url "$piperBase/$quality/$voiceName/$voiceName.onnx" `
            -DestPath (Join-Path $piperDir "$voice.onnx") `
            -Description "Piper: $voice"

        Download-File `
            -Url "$piperBase/$quality/$voiceName/$voiceName.onnx.json" `
            -DestPath (Join-Path $piperDir "$voice.onnx.json") `
            -Description "Piper config: $voice"
    }
}

# --- Pocket TTS ---
if ($Model -contains "pocket") {
    Write-Host ""
    Write-Host "============================================================" -ForegroundColor Cyan
    Write-Host "  Pocket TTS -> $TtsDir\pocket\" -ForegroundColor Cyan
    Write-Host "============================================================" -ForegroundColor Cyan

    $pocketDir = Join-Path $TtsDir "pocket"
    Ensure-Dir $pocketDir

    Write-Host "  NOTE: Pocket TTS model files are auto-downloaded by the" -ForegroundColor Gray
    Write-Host "        pocket_tts Python package on first use." -ForegroundColor Gray
    Write-Host "        HF_HOME is set to: $TtsDir" -ForegroundColor Gray
}

# --- Kitten TTS ---
if ($Model -contains "kitten") {
    Write-Host ""
    Write-Host "============================================================" -ForegroundColor Cyan
    Write-Host "  Kitten TTS -> $TtsDir\kitten\" -ForegroundColor Cyan
    Write-Host "============================================================" -ForegroundColor Cyan

    $kittenDir = Join-Path $TtsDir "kitten"
    Ensure-Dir $kittenDir

    Write-Host "  NOTE: Kitten TTS model files are auto-downloaded by the" -ForegroundColor Gray
    Write-Host "        kittentts Python package on first use." -ForegroundColor Gray
}

# ── Summary ───────────────────────────────────────────────────────────────
Write-Host ""
Write-Host "============================================================" -ForegroundColor Green
Write-Host "  Download Complete" -ForegroundColor Green
Write-Host "============================================================" -ForegroundColor Green
Write-Host "  Downloaded: $TotalDownloaded files" -ForegroundColor White
Write-Host "  Skipped:    $TotalSkipped files (already present)" -ForegroundColor Yellow
if ($TotalFailed -gt 0) {
    Write-Host "  FAILED:     $TotalFailed files" -ForegroundColor Red
}
Write-Host ""
Write-Host "  Models path: $Path" -ForegroundColor White
Write-Host "  Structure:" -ForegroundColor White
Write-Host "    STT/   - speech-to-text (Parakeet, Silero VAD)"
Write-Host "    Brain/ - LLM models (llama.cpp GGUF)"
Write-Host "    TTS/   - text-to-speech (Kokoro, Piper, Pocket, Kitten)"
Write-Host ""
Write-Host "  Next step - setup Python venv for TTS engines:" -ForegroundColor Cyan
Write-Host "    .\scripts\setup_tts_venv.ps1" -ForegroundColor White
Write-Host "============================================================" -ForegroundColor Green
