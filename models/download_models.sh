#!/usr/bin/env bash
# S2B2S Model Downloader (macOS / Linux)
# Downloads STT, TTS, and Brain model files into a structured models/ directory.
#
# Usage:
#   bash download_models.sh                              # download all TTS models
#   bash download_models.sh --model kokoro               # download only Kokoro
#   bash download_models.sh --model piper --model pocket # download Piper + Pocket
#   bash download_models.sh --model stt                  # download STT models
#   bash download_models.sh --model brain                # download Brain models
#   bash download_models.sh --model all                  # download everything
#   bash download_models.sh --path /path/to/models       # custom target directory
#   bash download_models.sh --setup-venv                 # also setup Python venv
#   bash download_models.sh --clean-venv                 # clean and recreate venv
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

set -euo pipefail

# ── Defaults ──────────────────────────────────────────────────────────────
MODELS_PATH=""
MODELS=()
SETUP_VENV=false
CLEAN_VENV=false
FORCE=false
DRY_RUN=false

# ── Parse args ────────────────────────────────────────────────────────────
while [[ $# -gt 0 ]]; do
    case "$1" in
        --path) MODELS_PATH="$2"; shift 2 ;;
        --model) MODELS+=("$2"); shift 2 ;;
        --setup-venv) SETUP_VENV=true; shift ;;
        --clean-venv) CLEAN_VENV=true; shift ;;
        --force) FORCE=true; shift ;;
        --dry-run) DRY_RUN=true; shift ;;
        *) echo "Unknown option: $1"; exit 1 ;;
    esac
done

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [ -z "$MODELS_PATH" ]; then
    MODELS_PATH="$SCRIPT_DIR"
fi
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Default to all TTS models if no --model specified
if [ ${#MODELS[@]} -eq 0 ]; then
    MODELS=("piper" "kokoro" "pocket" "kitten")
fi

# Expand "all" and "tts" aliases
EXPANDED=()
for m in "${MODELS[@]}"; do
    case "$m" in
        all)   EXPANDED+=("stt" "brain" "piper" "kokoro" "pocket" "kitten") ;;
        tts)   EXPANDED+=("piper" "kokoro" "pocket" "kitten") ;;
        *)     EXPANDED+=("$m") ;;
    esac
done
MODELS=("${EXPANDED[@]}")

# Deduplicate
MODELS=($(printf '%s\n' "${MODELS[@]}" | sort -u))

# ── Helpers ───────────────────────────────────────────────────────────────
TOTAL_DOWNLOADED=0
TOTAL_SKIPPED=0
TOTAL_FAILED=0

mkdir_p() {
    if [ ! -d "$1" ]; then
        mkdir -p "$1"
        echo "  created: $1"
    fi
}

download_file() {
    local url="$1" dest="$2" desc="$3"
    mkdir -p "$(dirname "$dest")"

    if [ -f "$dest" ] && [ "$FORCE" != true ]; then
        local size
        size=$(du -h "$dest" 2>/dev/null | cut -f1)
        echo "  SKIP: $desc ($size) — already exists"
        TOTAL_SKIPPED=$((TOTAL_SKIPPED + 1))
        return 0
    fi

    if [ "$DRY_RUN" = true ]; then
        echo "  WOULD DOWNLOAD: $desc -> $dest"
        return 0
    fi

    echo "  DOWNLOAD: $desc..."
    if curl -fL --progress-bar -o "$dest" "$url"; then
        local size
        size=$(du -h "$dest" 2>/dev/null | cut -f1)
        echo "    -> $size downloaded"
        TOTAL_DOWNLOADED=$((TOTAL_DOWNLOADED + 1))
    else
        echo "    -> FAILED: $desc"
        TOTAL_FAILED=$((TOTAL_FAILED + 1))
    fi
}

# ── Venv Setup ────────────────────────────────────────────────────────────
if [ "$SETUP_VENV" = true ] || [ "$CLEAN_VENV" = true ]; then
    echo ""
    echo "=== Python Virtual Environment ==="
    VENV_DIR="$PROJECT_ROOT/venv"

    if [ "$CLEAN_VENV" = true ] && [ -d "$VENV_DIR" ]; then
        echo "  Cleaning existing venv at: $VENV_DIR"
        rm -rf "$VENV_DIR"
    fi

    if [ -f "$PROJECT_ROOT/scripts/setup_tts_venv.sh" ]; then
        echo "  Running setup_tts_venv.sh..."
        bash "$PROJECT_ROOT/scripts/setup_tts_venv.sh"
    else
        echo "  ERROR: setup_tts_venv.sh not found at $PROJECT_ROOT/scripts/"
        echo "  Please run: bash scripts/setup_tts_venv.sh"
    fi
fi

# ── STT Models ────────────────────────────────────────────────────────────
if printf '%s\n' "${MODELS[@]}" | grep -qx "stt"; then
    echo ""
    echo "============================================================"
    echo "  STT Models -> $MODELS_PATH/STT/"
    echo "============================================================"

    # Silero VAD
    STT_DIR="$MODELS_PATH/STT"
    mkdir_p "$STT_DIR/silero_vad"
    download_file \
        "https://blob.handy.computer/silero_vad_v4.onnx" \
        "$STT_DIR/silero_vad/silero_vad_v4.onnx" \
        "Silero VAD v4 (~1.7 MB)"

    # Parakeet V3 (~600 MB)
    PARAKEET_TAR="$STT_DIR/parakeet-tdt-0.6b-v3-int8.tar.gz"
    PARAKEET_DIR="$STT_DIR/parakeet-tdt-0.6b-v3-int8"

    if [ -d "$PARAKEET_DIR" ] && [ "$FORCE" != true ]; then
        echo "  SKIP: Parakeet V3 (extracted) — already exists"
        TOTAL_SKIPPED=$((TOTAL_SKIPPED + 1))
    else
        if [ ! -f "$PARAKEET_TAR" ] || [ "$FORCE" = true ]; then
            download_file \
                "https://blob.handy.computer/parakeet-v3-int8.tar.gz" \
                "$PARAKEET_TAR" \
                "Parakeet V3 (~600 MB)"
        fi
        if [ -f "$PARAKEET_TAR" ] && [ "$DRY_RUN" != true ]; then
            echo "  EXTRACT: Parakeet V3..."
            rm -rf "$PARAKEET_DIR"
            mkdir -p "$PARAKEET_DIR"
            tar -xzf "$PARAKEET_TAR" -C "$PARAKEET_DIR"
            echo "    -> Extracted"
            rm -f "$PARAKEET_TAR"
        fi
    fi
fi

# ── Brain Models ──────────────────────────────────────────────────────────
if printf '%s\n' "${MODELS[@]}" | grep -qx "brain"; then
    echo ""
    echo "============================================================"
    echo "  Brain Models -> $MODELS_PATH/Brain/llama_cpp/"
    echo "============================================================"

    BRAIN_DIR="$MODELS_PATH/Brain/llama_cpp"
    mkdir_p "$BRAIN_DIR"

    # Gemma-4-E2B GGUF files (~2 GB each)
    GEMMA_BASE="https://huggingface.co/unsloth/gemma-4-E2B-it-qat-GGUF/resolve/main"
    GEMMA_FILES=(
        "gemma-4-E2B-it-qat-UD-Q4_K_XL.gguf"
        "mmproj-F16.gguf"
        "mtp-gemma-4-E2B-it.gguf"
    )
    for f in "${GEMMA_FILES[@]}"; do
        download_file \
            "$GEMMA_BASE/$f" \
            "$BRAIN_DIR/$f" \
            "Brain: $f"
    done

    echo ""
    echo "  NOTE: Place additional GGUF model files in: $BRAIN_DIR"
    echo "        They will be auto-discovered by the llama.cpp settings UI."
fi

# ── TTS Models ────────────────────────────────────────────────────────────
TTS_DIR="$MODELS_PATH/TTS"
mkdir_p "$TTS_DIR"

# --- Kokoro ---
if printf '%s\n' "${MODELS[@]}" | grep -qx "kokoro"; then
    echo ""
    echo "============================================================"
    echo "  Kokoro-82M -> $TTS_DIR/kokoro/"
    echo "============================================================"

    KOKORO_DIR="$TTS_DIR/kokoro"
    mkdir_p "$KOKORO_DIR"

    download_file \
        "https://huggingface.co/hexgrad/Kokoro-82M/resolve/main/kokoro-v1.0.onnx" \
        "$KOKORO_DIR/kokoro-v1.0.onnx" \
        "Kokoro ONNX model (~330 MB)"

    download_file \
        "https://huggingface.co/hexgrad/Kokoro-82M/resolve/main/voices-v1.0.bin" \
        "$KOKORO_DIR/voices-v1.0.bin" \
        "Kokoro voices data (~330 MB)"
fi

# --- Piper Voices ---
if printf '%s\n' "${MODELS[@]}" | grep -qx "piper"; then
    echo ""
    echo "============================================================"
    echo "  Piper Voices -> $TTS_DIR/piper-voices/"
    echo "============================================================"

    PIPER_DIR="$TTS_DIR/piper-voices"
    mkdir_p "$PIPER_DIR"

    PIPER_VOICES=(
        "en_US-amy-low" "en_US-amy-medium"
        "en_US-arctic-medium"
        "en_US-bryce-medium"
        "en_US-danny-low"
        "en_US-hfc_female-medium" "en_US-hfc_male-medium"
        "en_US-joe-medium"
        "en_US-john-medium"
        "en_US-kathleen-low"
        "en_US-kristin-medium"
        "en_US-kusal-medium"
        "en_US-l2arctic-medium"
        "en_US-lessac-high" "en_US-lessac-low" "en_US-lessac-medium"
        "en_US-libritts-high" "en_US-libritts_r-medium"
        "en_US-ljspeech-high" "en_US-ljspeech-medium"
        "en_US-norman-medium"
        "en_US-reza_ibrahim-medium"
        "en_US-ryan-high" "en_US-ryan-low" "en_US-ryan-medium"
        "en_US-sam-medium"
    )

    PIPER_BASE="https://huggingface.co/rhasspy/piper-voices/resolve/main/en/en_US"

    for voice in "${PIPER_VOICES[@]}"; do
        voice_name="${voice#en_US-}"
        quality="medium"
        case "$voice" in
            *-low) quality="low" ;;
            *-high) quality="high" ;;
        esac

        download_file \
            "$PIPER_BASE/$quality/$voice_name/$voice_name.onnx" \
            "$PIPER_DIR/$voice.onnx" \
            "Piper: $voice"

        download_file \
            "$PIPER_BASE/$quality/$voice_name/$voice_name.onnx.json" \
            "$PIPER_DIR/$voice.onnx.json" \
            "Piper config: $voice"
    done
fi

# --- Pocket TTS ---
if printf '%s\n' "${MODELS[@]}" | grep -qx "pocket"; then
    echo ""
    echo "============================================================"
    echo "  Pocket TTS -> $TTS_DIR/pocket/"
    echo "============================================================"

    POCKET_DIR="$TTS_DIR/pocket"
    mkdir_p "$POCKET_DIR"

    echo "  NOTE: Pocket TTS model files are auto-downloaded by the"
    echo "        pocket_tts Python package on first use."
    echo "        HF_HOME is set to: $TTS_DIR"
    echo "        Cache will appear in: $POCKET_DIR/hub/"
    echo ""
    echo "        To pre-download, run this Python snippet after venv setup:"
    echo "          python -c \"import os; os.environ['HF_HOME']='$TTS_DIR';"
    echo "          from pocket_tts.models.tts_model import TTSModel;"
    echo "          TTSModel.load_model(language='english')\""
fi

# --- Kitten TTS ---
if printf '%s\n' "${MODELS[@]}" | grep -qx "kitten"; then
    echo ""
    echo "============================================================"
    echo "  Kitten TTS -> $TTS_DIR/kitten/"
    echo "============================================================"

    KITTEN_DIR="$TTS_DIR/kitten"
    mkdir_p "$KITTEN_DIR"

    echo "  NOTE: Kitten TTS model files are auto-downloaded by the"
    echo "        kittentts Python package on first use."
    echo "        HF_HOME is set to: $TTS_DIR"
    echo "        Cache will be created automatically on first synthesis."
fi

# ── Summary ───────────────────────────────────────────────────────────────
echo ""
echo "============================================================"
echo "  Download Complete"
echo "============================================================"
echo "  Downloaded: $TOTAL_DOWNLOADED"
echo "  Skipped:    $TOTAL_SKIPPED (already present)"
if [ "$TOTAL_FAILED" -gt 0 ]; then
    echo "  FAILED:     $TOTAL_FAILED"
fi
echo ""
echo "  Models path: $MODELS_PATH"
echo "  Structure:"
echo "    STT/   — speech-to-text (Parakeet, Silero VAD)"
echo "    Brain/ — LLM models (llama.cpp GGUF)"
echo "    TTS/   — text-to-speech (Kokoro, Piper, Pocket, Kitten)"
echo ""
echo "  Next step — setup Python venv for TTS engines:"
echo "    bash scripts/setup_tts_venv.sh"
echo "============================================================"
