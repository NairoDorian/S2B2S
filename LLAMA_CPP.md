# Llama.cpp & Gemma-4 MTP Integration Reference

This document acts as the official reference for the Llama.cpp integration inside the S2B2S project, preserving the exact commands and configurations for running Gemma-4 with Multi-Token Prediction (MTP) and Flash Attention.

---

## 1. System Dependencies and Build Commands

S2B2S's build pipeline (`src-tauri/build.rs`) automates these steps. However, for manual building or system reference, the dependencies and CMake commands are:

```bash
# Install required system dependencies
apt-get install pciutils build-essential cmake curl libcurl4-openssl-dev -y

# Clone the repository
git clone https://github.com/ggml-org/llama.cpp

# Configure the build directory (enabling CUDA acceleration, disabling shared libraries)
cmake llama.cpp -B llama.cpp/build \
    -DBUILD_SHARED_LIBS=OFF -DGGML_CUDA=ON

# Compile the target binaries
cmake --build llama.cpp/build --config Release -j --clean-first \
    --target llama-cli --target llama-mtmd-cli --target llama-server --target llama-gguf-split

# Copy the compiled binaries to the llama.cpp root folder
cp llama.cpp/build/bin/llama-* llama.cpp
```

---

## 2. Downloading the Models (Hugging Face)

The model and draft model files are downloaded using the official Hugging Face Hub CLI. First, make sure you have the required CLI tools installed:

```bash
# Install Hugging Face Hub with accelerated transfer support
pip install huggingface_hub hf_transfer
```

Then, download the Gemma-4-E2B-it-qat GGUF model along with its vision project and Multi-Token Prediction (MTP) draft models:

```bash
# Download files into the local dir matching the include filters
hf download unsloth/gemma-4-E2B-it-qat-GGUF \
    --local-dir unsloth/gemma-4-E2B-it-qat-GGUF \
    --include "*mmproj-F16*" \
    --include "mtp-*" \
    --include "*UD-Q4_K_XL*"
```

---

## 3. Running Llama-server with MTP and Flash Attention

To run the OpenAI-compatible local API server using the downloaded Gemma-4 model, its vision projection, and the MTP draft model:

```bash
./llama.cpp/llama-server \
    --model unsloth/gemma-4-E2B-it-qat-GGUF/gemma-4-E2B-it-qat-UD-Q4_K_XL.gguf \
    --mmproj unsloth/gemma-4-E2B-it-qat-GGUF/mmproj-F16.gguf \
    --model-draft unsloth/gemma-4-E2B-it-qat-GGUF/mtp-gemma-4-E2B-it.gguf \
    --temp 1.0 \
    --top-p 0.95 \
    --top-k 64 \
    --alias "unsloth/gemma-4-e2b-it-qat-GGUF" \
    --port 8001 \
    --chat-template-kwargs '{"enable_thinking":false}'
```

### Context Size & Flash Attention Options
To run the server with a customized context size (e.g., `4096` tokens) and enable Flash Attention (`-fa` shortcut):

```bash
# Set context length and enable flash attention
./llama-server -m model.gguf --ctx-size 4096 --port 8080 --flash-attn on
```

---

## 4. Default CLI Usage (Testing)

For testing generations locally through the CLI:

```bash
export LLAMA_CACHE="unsloth/gemma-4-E2B-it-qat-GGUF"
./llama.cpp/llama-cli \
    -hf unsloth/gemma-4-E2B-it-qat-GGUF:UD-Q4_K_XL \
    --temp 1.0 \
    --top-p 0.95 \
    --top-k 64  \
    --spec-type draft-mtp --spec-draft-n-max 2 \
    --chat-template-kwargs '{"enable_thinking":false}'
```

---

## 5. Integration Architecture inside S2B2S

- **External Host**: S2B2S communicates with the local server over HTTP. The user spins up `llama-server` using the commands in Section 3.
- **Provider Routing**: In the S2B2S settings under **Brain** or **Post-Process**, select the **Llama.cpp (Local)** provider. S2B2S will attempt to communicate with it on port `8001` (default base URL: `http://localhost:8001/v1`).
- **MTP / Drafts**: Because the draft model parameter (`--model-draft`) is handled by the `llama-server` process itself, using this provider with a correctly launched server leverages Multi-Token Prediction (MTP) acceleration automatically.
