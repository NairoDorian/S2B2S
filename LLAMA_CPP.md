# Llama.cpp Pre-Compiled Server Integration

S2B2S integrates **pre-compiled `llama-server` binaries** from [llama.cpp GitHub releases](https://github.com/ggml-org/llama.cpp/releases), providing drop-in GPU acceleration for local LLM inference — no CMake, no source compilation, no Python dependencies.

---

## 1. Architecture

The integration is managed by **`LlamaServerManager`** in `src-tauri/src/llama_server/manager.rs` and `commands/llama_server.rs`:

- **Auto-download** — Fetches pre-compiled binaries (`llama-b9601-bin-win-cuda-cu12.4-x64.zip`, etc.) from GitHub releases. Filters duplicate backend variants automatically.
- **Auto-install** — Extracts archives to `llama_cpp_servers/` in the app data directory. ZIP files deleted after extraction.
- **Backend auto-detection** — Selects best available GPU backend at startup: **CUDA > Vulkan > CPU**. CUDA is detected via `nvidia-smi` or `CUDA_PATH` environment variable. Vulkan is probed via `vulkaninfo`.
- **GPU VRAM offloading** — Server launch uses `-ngl all` to load all model layers into GPU VRAM when a CUDA or Vulkan binary is active.
- **Health check** — Polls the server's `/v1/models` endpoint to confirm readiness.
- **Auto-start** — `BrainManager::warmup()` calls `ensure_server_running()` for llama_cpp provider before sending the warmup prompt.

## 2. Supported Backends

| Backend | Platform | Binary Suffix |
|---------|----------|---------------|
| CUDA 12.4 | Windows, Linux | `cuda-cu12.4` |
| CUDA 13.3 | Windows, Linux | `cuda-cu13.3` |
| Vulkan | Windows, Linux | `vulkan` |
| CPU | Windows, Linux | `cpu` |
| Metal | macOS | Built-in (no separate binary needed) |

## 3. Settings & UI

The **Llama.cpp settings tab** (`src/components/settings/llama-cpp/LlamaCppSettings.tsx`) provides:

- **GPU detection display** — shows detected GPU type and VRAM
- **Release browser** — fetches available releases from GitHub, with per-backend download buttons and progress indicators
- **Installed servers list** — shows version tags (e.g., `b9601`) per backend, with Remove/Use buttons
- **VRAM usage indicator** — green (<75%), yellow (75-90%), red (>90%) with hover tooltip showing used/total MB. Polls GPU VRAM every 5 seconds.
- **Footer Brain dot** — orange pulsing (loading), green (ready), gray (disabled)

## 4. Provider Configuration

In S2B2S settings under **Brain** or **Post-Process**, select the **Llama.cpp (Local)** provider:

- Default base URL: `http://localhost:8001/v1`
- The `llama-server` process is managed automatically by S2B2S — no manual server launch needed.
- Toggling Brain OFF terminates the llama-server process immediately.

## 5. Model File

Place your GGUF model file in the `models/` directory, then select it in the llama.cpp settings UI. The default model alias is **Gemma-4 2B (Local)**.

## 6. Performance Metrics

The `brain:done` event carries per-response timing from the llama.cpp server:

- `tokens_per_sec` — tokens/second throughput
- `total_ms` — total generation time
- Displayed in Conversation view next to each assistant message

---

*The old CMake-based `build_llama_cpp()` pipeline in `build.rs` was fully removed in favor of this pre-compiled binary approach.*
