# Llama.cpp Gemma 4 MTP Setup Reference

## Directory Structure

```
C:\Users\Z\Downloads\PROJECTS\Llama.cpp\
├── llama-b9630\                    # llama.cpp b9630 binaries (CUDA 13.3)
│   ├── llama-server.exe
│   ├── llama-cli.exe
│   └── ...
├── model\
│   └── gemma-4-E2B-it-qat-GGUF\    # Downloaded model files
│       ├── gemma-4-E2B-it-qat-UD-Q4_K_XL.gguf   (2.44 GB)  # Main model
│       ├── mtp-gemma-4-E2B-it.gguf              (56.5 MB)  # MTP draft model
│       ├── mmproj-F16.gguf                      (940 MB)   # Multimodal projector (vision)
│       └── ...
├── run_server.bat                   # Quick-start batch file
└── REFERENCE.md                     # This file
```

## Hardware

- **GPU:** NVIDIA GeForce RTX 4070 Laptop GPU (8 GB VRAM)
- **CPU:** 13th Gen Intel Core i9-13900H (20 threads)
- **CUDA:** 13.3

## Quick Start

Run the batch file to start the server:

```cmd
C:\Users\Z\Downloads\PROJECTS\Llama.cpp\run_server.bat
```

Then send requests to `http://127.0.0.1:62966/v1/chat/completions`.

### Chat Completion Example (PowerShell)

```powershell
$body = @{
    messages = @(@{role="user"; content="Count from 1 to 100"})
    max_tokens = 500
    temperature = 1.0
    top_p = 0.95
    top_k = 64
    stream = $false
} | ConvertTo-Json
Invoke-RestMethod "http://127.0.0.1:62966/v1/chat/completions" -Method POST -Body $body -ContentType "application/json"
```

## Best Performance Found (Triple-Validated, 3 sweeps × 21 runs each)

| Setting         | Steady tok/s | Avg tok/s | MinExcl1  | Max       | Notes                           |
| --------------- | ------------ | --------- | --------- | --------- | ------------------------------- |
| **b9630, n=13** | **216.3**    | **207.9** | **184.5** | **221.6** | **CLEAR WINNER**                |
| b9630, n=15     | 206.6        | 200.7     | 176.7     | 216.0     |                                 |
| b9630, n=14     | 205.7        | 198.9     | 174.3     | 212.5     |                                 |
| b9630, n=16     | 203.1        | 194.3     | 162.6     | 210.7     |                                 |
| b9630, n=17     | 200.3        | 193.4     | 167.5     | 207.3     |                                 |
| b9630, n=18     | 195.3        | 188.4     | 154.2     | 202.6     |                                 |
| b9630, n=19     | 193.6        | 187.8     | 162.2     | 204.5     |                                 |
| b9630, n=20     | 193.5        | 187.4     | 161.7     | 199.3     |                                 |
| b9630, n=21     | 191.7        | 185.0     | 162.6     | 199.2     |                                 |
| b9630, n=23     | 190.7        | 183.6     | 153.7     | 197.1     |                                 |
| b9630, n=22     | 188.6        | 182.1     | 161.2     | 195.1     |                                 |
| b9630, n=24     | 183.2        | 176.9     | 153.1     | 191.0     |                                 |
| b9630, n=25     | 182.5        | 176.9     | 144.9     | 193.0     |                                 |
| b9630, n=27     | 179.3        | 173.9     | 150.6     | 185.7     |                                 |
| b9630, n=26     | 177.5        | 171.7     | 151.7     | 184.7     |                                 |
| b9630, n=28     | 174.5        | 167.7     | 139.7     | 180.7     |                                 |
| b9630, n=29     | 172.1        | 166.5     | 149.0     | 177.8     |                                 |
| b9630, n=30     | 170.9        | 166.3     | 150.8     | 177.1     |                                 |
| b9630, n=8      | 170.3        | 164.8     | 145.1     | 176.9     | Previous "best" from 1-10 sweep |
| b9630, n=32     | 167.6        | 161.8     | 141.3     | 172.8     |                                 |
| b9630, n=31     | 167.0        | 160.4     | 141.3     | 172.1     |                                 |
| b9630, n=9      | 166.2        | 161.7     | 139.3     | 173.9     |                                 |
| b9630, n=6      | 160.0        | 155.3     | 135.9     | 167.4     |                                 |
| b9630, n=10     | 158.6        | 156.7     | 131.1     | 168.7     |                                 |
| b9630, n=12     | 158.2        | 153.4     | 138.9     | 164.8     |                                 |
| b9630, n=11     | 157.8        | 152.7     | 131.4     | 162.9     |                                 |
| b9630, n=7      | 149.5        | 144.8     | 124.2     | 154.3     | Odd dip                         |
| b9630, n=4      | 146.2        | 142.3     | 132.4     | 151.3     |                                 |
| b9630, n=3      | 137.1        | 134.1     | 126.7     | 141.1     |                                 |
| b9630, n=5      | 136.1        | 130.9     | 116.5     | 140.9     | Odd dip                         |
| b9630, n=2      | 128.1        | 125.0     | 114.1     | 132.1     |                                 |
| b9630, n=1      | 112.5        | 109.7     | 103.0     | 116.0     | Baseline                        |

## Benchmark: --spec-draft-n-max Sweep (1-32) — Triple-Validated

3 independent sweeps with full server restart between each. 21 runs per config per sweep (discard first cold run = 20 good values). Total: 32 × 21 × 3 = 2,016 requests.

**The minimum is ALWAYS run #1** (cold-start prompt cache). `MinExcl1` = minimum of runs 2-21 (the real stable range).

| n      | Avg Steady(3) | Run1      | Run2      | Run3      | MinExcl1 Avg | Max Avg   |
| ------ | ------------- | --------- | --------- | --------- | ------------ | --------- |
| 1      | 112.5         | 114.4     | 109.1     | 114.0     | 103.0        | 116.0     |
| 2      | 128.1         | 129.9     | 127.5     | 127.0     | 114.1        | 132.1     |
| 3      | 137.1         | 140.3     | 132.5     | 138.4     | 126.7        | 141.1     |
| 4      | 146.2         | 147.4     | 144.4     | 146.8     | 132.4        | 151.3     |
| 5      | 136.1         | 136.4     | 135.2     | 136.6     | 116.5        | 140.9     |
| 6      | 160.0         | 161.4     | 163.0     | 155.7     | 135.9        | 167.4     |
| 7      | 149.5         | 150.8     | 149.7     | 147.9     | 124.2        | 154.3     |
| 8      | 170.3         | 173.1     | 173.5     | 164.4     | 145.1        | 176.9     |
| 9      | 166.2         | 167.7     | 170.1     | 160.9     | 139.3        | 173.9     |
| 10     | 158.6         | 164.5     | 151.8     | 159.5     | 131.1        | 168.7     |
| 11     | 157.8         | 159.6     | 160.4     | 153.5     | 131.4        | 162.9     |
| 12     | 158.2         | 160.5     | 158.3     | 155.7     | 138.9        | 164.8     |
| **13** | **216.3**     | **222.1** | **211.2** | **215.5** | **184.5**    | **221.6** |
| 14     | 205.7         | 206.8     | 207.3     | 203.0     | 174.3        | 212.5     |
| 15     | 206.6         | 213.6     | 200.0     | 206.2     | 176.7        | 216.0     |
| 16     | 203.1         | 203.8     | 209.7     | 195.8     | 162.6        | 210.7     |
| 17     | 200.3         | 196.4     | 205.0     | 199.6     | 167.5        | 207.3     |
| 18     | 195.3         | 195.4     | 196.5     | 193.9     | 154.2        | 202.6     |
| 19     | 193.6         | 192.7     | 196.6     | 191.5     | 162.2        | 204.5     |
| 20     | 193.5         | 198.4     | 192.3     | 189.7     | 161.7        | 199.3     |
| 21     | 191.7         | 194.8     | 189.2     | 191.0     | 162.6        | 199.2     |
| 22     | 188.6         | 193.7     | 190.2     | 182.0     | 161.2        | 195.1     |
| 23     | 190.7         | 192.9     | 191.8     | 187.4     | 153.7        | 197.1     |
| 24     | 183.2         | 184.1     | 183.9     | 181.5     | 153.1        | 191.0     |
| 25     | 182.5         | 184.0     | 185.8     | 177.6     | 144.9        | 193.0     |
| 26     | 177.5         | 182.9     | 174.7     | 174.9     | 151.7        | 184.7     |
| 27     | 179.3         | 177.5     | 182.8     | 177.6     | 150.6        | 185.7     |
| 28     | 174.5         | 175.2     | 178.4     | 170.0     | 139.7        | 180.7     |
| 29     | 172.1         | 176.4     | 169.1     | 170.7     | 149.0        | 177.8     |
| 30     | 170.9         | 166.0     | 175.1     | 171.7     | 150.8        | 177.1     |
| 31     | 167.0         | 167.5     | 167.0     | 166.6     | 141.3        | 172.1     |
| 32     | 167.6         | 167.6     | 168.1     | 167.1     | 141.3        | 172.8     |

### Key Findings

- **Massive jump at n=13**: from ~170 tok/s (n=8-12) to **216 tok/s** at n=13. Likely aligns with an MTP model architecture boundary.
- **Optimal range**: n=13-17 all above 200 tok/s. **n=13 is the peak.**
- **Odd dips at n=5, n=7, n=11**: consistently underperform neighbors — MTP model quirk.
- **Diminishing returns after n=20**: steady decline from ~190 to ~167 tok/s.
- **Minimum is always run #1**: cold-start prompt cache. Discard first result.

## Attention Rotation & KV Cache Optimization

All PRs below are **already merged and active in b9630** by default.

### Hadamard KV Cache Rotation (PR #21038 + #23615 CUDA FWHT)

- Rotates Q, K, V via Hadamard transform to reduce outliers for better KV cache quantization
- CUDA Fast Walsh-Hadamard Transform (PR #23615) speeds up the rotation by ~8-9% on generation
- Already **active by default in b9630** — disable with `set LLAMA_ATTN_ROT_DISABLE=1`
- On short prompts, rotation adds **~3-4% overhead** (203 vs 211 tok/s) — FWHT isn't free

### KV Cache Type Benchmarks (n=13, 21 runs each)

| Config                     | Steady tok/s | MinExcl1  | Max       | VRAM         | Notes                              |
| -------------------------- | ------------ | --------- | --------- | ------------ | ---------------------------------- |
| RotON+f16 (default)        | 203.4        | 170.7     | 214.6     | 3703 MiB     | Current default                    |
| **RotOFF+f16**             | **210.8**    | **171.4** | **218.1** | ~3700 MiB    | **Fastest for short ctx**          |
| RotON+q8_0                 | 167.1        | 145.9     | 183.3     | 3813 MiB     | Slower — rotation + quant overhead |
| RotOFF+q8_0                | 202.8        | 163.0     | 209.9     | ~3800 MiB    | Rotation is the cost, not q8       |
| RotON+q4_0                 | 183.8        | 162.9     | 192.8     | 3641 MiB     | Rotation + quant overhead          |
| RotOFF+q4_0                | 210.7        | 165.0     | 216.0     | ~3600 MiB    | q4_0 without rotation == f16 speed |
| **RotOFF+f16 (no mmproj)** | **211.1**    | **171.9** | **222.8** | **2554 MiB** | **Best config for text-only**      |

### Key Findings

- **Rotation costs ~3-4% speed** on short contexts — disable it if you don't need quantized KV cache
- **No mmproj saves ~1150 MiB VRAM** with same speed — always skip for text-only
- **Lower-bit cache (q4_0, q8_0) doesn't help speed on short prompts** — the quantization/dequantization overhead outweighs memory bandwidth savings
- **Rotation + lower-bit cache is WORSE than f16 without rotation** on this hardware for short ctx
- Rotation benefit would appear at **very large context sizes** (32K+) where KV cache bandwidth dominates

### Optimal Command for Text-Only (Fastest)

```cmd
set LLAMA_ATTN_ROT_DISABLE=1
llama-server.exe ^
    -m "model\gemma-4-E2B-it-qat-GGUF\gemma-4-E2B-it-qat-UD-Q4_K_XL.gguf" ^
    --port 62966 ^
    -c 131072 ^
    --parallel 1 ^
    --flash-attn on ^
    --no-context-shift ^
    -ngl -1 ^
    --threads -1 ^
    --jinja ^
    --reasoning off ^
    --model-draft "model\gemma-4-E2B-it-qat-GGUF\mtp-gemma-4-E2B-it.gguf" ^
    --spec-type draft-mtp ^
    --spec-draft-n-max 13 ^
    --metrics ^
    -ctk f16 -ctv f16
```

## Optimal Command (Best Performance)

```cmd
llama-server.exe ^
    -m "model\gemma-4-E2B-it-qat-GGUF\gemma-4-E2B-it-qat-UD-Q4_K_XL.gguf" ^
    --port 62966 ^
    -c 131072 ^
    --parallel 1 ^
    --flash-attn on ^
    --no-context-shift ^
    -ngl -1 ^
    --threads -1 ^
    --jinja ^
    --reasoning off ^
    --model-draft "model\gemma-4-E2B-it-qat-GGUF\mtp-gemma-4-E2B-it.gguf" ^
    --spec-type draft-mtp ^
    --spec-draft-n-max 13 ^
    --mmproj "model\gemma-4-E2B-it-qat-GGUF\mmproj-F16.gguf" ^
    --metrics
```

### Variant: 12B Model

```cmd
llama-server.exe ^
    -m "model\gemma-4-12B-it-qat-GGUF\gemma-4-12B-it-qat-UD-Q4_K_XL.gguf" ^
    --port 8001 ^
    -c 131072 ^
    --parallel 1 ^
    --flash-attn on ^
    --no-context-shift ^
    -ngl -1 ^
    --threads -1 ^
    --jinja ^
    --reasoning off ^
    --model-draft "model\gemma-4-12B-it-qat-GGUF\mtp-gemma-4-12B-it.gguf" ^
    --spec-type draft-mtp ^
    --spec-draft-n-max 6 ^
    --mmproj "model\gemma-4-12B-it-qat-GGUF\mmproj-F16.gguf" ^
    --metrics
```

### Variant: Without Vision (Skip mmproj)

For text-only tasks, the `--mmproj` flag can be omitted (saves ~940 MB VRAM):

```cmd
llama-server.exe ^
    -m "model\gemma-4-E2B-it-qat-GGUF\gemma-4-E2B-it-qat-UD-Q4_K_XL.gguf" ^
    --port 62966 ^
    -c 131072 ^
    --parallel 1 ^
    --flash-attn on ^
    --no-context-shift ^
    -ngl -1 ^
    --threads -1 ^
    --jinja ^
    --reasoning off ^
    --model-draft "model\gemma-4-E2B-it-qat-GGUF\mtp-gemma-4-E2B-it.gguf" ^
    --spec-type draft-mtp ^
    --spec-draft-n-max 6 ^
    --metrics
```

## Flag Reference

| Flag                     | Description                           | Recommended                         |
| ------------------------ | ------------------------------------- | ----------------------------------- |
| `-m, --model`            | Path to main model GGUF               | Required                            |
| `--model-draft`          | Path to MTP draft model GGUF          | Required for MTP                    |
| `--mmproj`               | Multimodal projector (vision)         | Optional for text-only              |
| `--spec-type draft-mtp`  | Enable MTP speculative decoding       | Required for MTP                    |
| `--spec-draft-n-max N`   | MTP draft tokens (1-32)               | **13 on this hardware** (test 1-32) |
| `--flash-attn on`        | Flash Attention                       | Highly recommended                  |
| `--parallel N`           | Parallel requests                     | 1 for dedicated single-user         |
| `-c N`                   | Context size                          | 131072 (default)                    |
| `--no-context-shift`     | Disable context shifting              | Recommended                         |
| `-ngl N`                 | GPU layers (-1 = all)                 | -1 for full offload                 |
| `--threads N`            | CPU threads (-1 = auto)               | -1                                  |
| `--jinja`                | Use Jinja chat template               | Keeps thinking template             |
| `--reasoning on/off`     | Enable/disable thinking (modern flag) | `off` for speed                     |
| `--chat-template-kwargs` | JSON template params (deprecated)     | Use `--reasoning` or env var        |
| `--metrics`              | Expose /metrics endpoint              | Useful for monitoring               |
| `--cache-ram N`          | RAM cache size                        | Omit (default) or 0 to disable      |
| `--temp`                 | Sampling temperature                  | 1.0                                 |
| `--top-p`                | Nucleus sampling                      | 0.95                                |
| `--top-k`                | Top-K sampling                        | 64                                  |

## --chat-template-kwargs on Windows (Deprecated)

**Use `--reasoning on` / `--reasoning off` instead** — the old `--chat-template-kwargs` is deprecated.

If you must use the old flag, PowerShell cannot pass `--chat-template-kwargs '{"enable_thinking":true}'` directly because single-quotes are passed literally to native executables.

**Workaround:** Use the environment variable:

```cmd
set LLAMA_ARG_CHAT_TEMPLATE_KWARGS={"enable_thinking":true}
llama-server.exe ...
```

## About mmproj (Multimodal Projector)

**Yes, `--mmproj` is only for vision/multimodal capabilities.** It loads the CLIP model required for processing images. For pure text tasks, you can skip it entirely. Benefits of skipping:

- Saves ~940 MB VRAM (important for 8 GB GPUs)
- Faster loading time
- Same text generation quality

Source: [unsloth.ai/docs/models/mtp](https://unsloth.ai/docs/models/mtp)

## MTP Optimization Notes

From [Unsloth MTP docs](https://unsloth.ai/docs/models/mtp):

- MTP makes generation **~1.4× to 2.2× faster** on GPUs
- MTP uses ~2 GB additional VRAM headroom
- `--spec-draft-n-max 2` is the recommended starting point, **but test 1-10**
- On this RTX 4070 Laptop 8GB, **n=13 is fastest** (~216 tok/s steady)
- Massive performance jump at n=13 — likely MTP model architecture boundary
- Optimal range: n=13-17 (all >200 tok/s), peak at n=13
- Odd values (n=5, n=7, n=11) underperform neighbors — confirmed MTP quirk
- Dense models benefit most from MTP
- The `mtp-` prefixed GGUF is the separately trained MTP head

## What Worked / What Didn't

| Attempt                     | Result                        | Fix                                                                  |
| --------------------------- | ----------------------------- | -------------------------------------------------------------------- |
| b9601 with Start-Process    | ✅ Working                    | -                                                                    |
| b9630 with Start-Process    | ❌ HTTP never responded       | Switch to `cmd /c start` batch file                                  |
| b9630 with batch file       | ✅ 186 tok/s                  | `run_server.bat` approach works                                      |
| b9630 without --cache-ram 0 | ❌ Server hung on init        | b9630 prompt cache init was slow, but works with batch approach      |
| --reasoning off             | ✅ Faster, no thinking tokens | Use `--reasoning off` instead of deprecated `--chat-template-kwargs` |
| --spec-draft-n-max 1        | 116 tok/s                     | Baseline, no real MTP benefit                                        |
| --spec-draft-n-max 2        | 132 tok/s                     | Good starting point                                                  |
| --spec-draft-n-max 3        | 147 tok/s                     | Solid improvement                                                    |
| --spec-draft-n-max 4        | 152 tok/s                     | Better                                                               |
| --spec-draft-n-max 5        | 140 tok/s                     | Worse than n=4 (lower acceptance rate?)                              |
| --spec-draft-n-max 6        | 163 tok/s                     | Good                                                                 |
| --spec-draft-n-max 7        | 153 tok/s                     | Dip at odd value                                                     |
| --spec-draft-n-max 8        | 170 tok/s                     | Previous "best" before full 1-32 sweep                               |
| --spec-draft-n-max 9        | 166 tok/s                     |                                                                      |
| --spec-draft-n-max 10       | 159 tok/s                     |                                                                      |
| --spec-draft-n-max 11-12    | ~158 tok/s                    |                                                                      |
| **--spec-draft-n-max 13**   | **216 tok/s**                 | **Best** — massive jump at n=13                                      |
| --spec-draft-n-max 14-17    | 200-206 tok/s                 | Above 200 club                                                       |
| --spec-draft-n-max 18-23    | 189-195 tok/s                 | Strong                                                               |
| --spec-draft-n-max 24+      | 167-183 tok/s                 | Declining                                                            |
| Skip --mmproj               | Saves 1150 MiB VRAM           | Same speed, only needed for vision                                   |
