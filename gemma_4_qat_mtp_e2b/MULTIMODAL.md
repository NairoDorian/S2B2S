# Gemma 4 E2B — Multimodal Inputs & MTP Optimization

## The n=13 Discovery

After triple-validated benchmarking (3 sweeps × 21 runs = 63 runs per config, n=1..32), **`--spec-draft-n-max 13`** is the clear winner on this RTX 4070 Laptop 8GB.

```
n   | Steady tok/s | Notes
----|-------------|-------------------------------------------------------
1   | 112.5       | Baseline — MTP doing almost nothing
2   | 128.1       | Unsloth's recommended starting point
3   | 137.1       |
4   | 146.2       |
5   | 136.1       | Odd dip — MTP architecture quirk
6   | 160.0       |
7   | 149.5       | Odd dip
8-12| 157-170     | Steady climb
13  | 216.3       | MASSIVE jump — MTP model architecture boundary
14-17| 200-206     | Above 200 tok/s club
18-23| 189-195     | Strong but declining
24+| 167-183      | Diminishing returns
```

**Why n=13?** Likely aligns with the MTP draft model's internal processing width. The draft model (`mtp-gemma-4-E2B-it.gguf`) has `n_embd=1536`. The Hadamard/CUDA FWHT kernels operate on power-of-2 sizes. 13 tokens × 128 = 1664, which maps efficiently to the GPU's warp/wavefront size (32) × something. The exact reason is model-internal, but the data is clear and reproducible.

**The minimum is ALWAYS run #1** — cold-start prompt cache effect. Discard it.

---

## Image Input

The model supports image understanding via the `--mmproj` (multimodal projector). The `mmproj-F16.gguf` file (~940 MB) must be loaded at server startup.

### Server Setup

```cmd
llama-server.exe ^
    -m "model\gemma-4-E2B-it-qat-GGUF\gemma-4-E2B-it-qat-UD-Q4_K_XL.gguf" ^
    --mmproj "model\gemma-4-E2B-it-qat-GGUF\mmproj-F16.gguf" ^
    --port 62966
```

### API Format (OpenAI-compatible)

Send the image as a **base64 data URI** in the content array:

```json
{
  "messages": [
    {
      "role": "user",
      "content": [
        {"type": "image_url", "image_url": {"url": "data:image/png;base64,iVBORw0KGgo..."}},
        {"type": "text", "text": "Describe this image in detail."}
      ]
    }
  ],
  "max_tokens": 300,
  "temperature": 1.0,
  "top_p": 0.95,
  "top_k": 64,
  "stream": false
}
```

### PowerShell Example

```powershell
$imgBytes = [System.IO.File]::ReadAllBytes("path\to\image.png")
$b64 = [Convert]::ToBase64String($imgBytes)

$body = @{
    messages = @(
        @{role = "user"; content = @(
            @{type = "image_url"; image_url = @{url = "data:image/png;base64,$b64"}}
            @{type = "text"; text = "Describe this image in detail."}
        )}
    )
    max_tokens = 300
    temperature = 1.0
    top_p = 0.95
    top_k = 64
    stream = $false
} | ConvertTo-Json -Depth 5

$r = Invoke-RestMethod "http://127.0.0.1:62966/v1/chat/completions" `
    -Method POST -Body $body -ContentType "application/json"
$r.choices[0].message.content
```

### Important Notes

- **Keep images under ~10 MB** to avoid HTTP 413 Payload Too Large errors. Resize if needed.
- Place image content **before** text in the content array for best quality (per Gemma 4 docs).
- Supported token budgets: 70, 140, 280, 560, 1120 tokens per image (use higher for OCR/documents).
- The `--mmproj` loads the CLIP vision encoder (~150M params, ~940 MB VRAM).

### Example Output

**Input:** Portrait photo of a man smiling, glasses, patterned shirt
**Output:**
> *"This is a portrait photograph of a man with dark hair, smiling, and wearing glasses and a patterned shirt against a dark background. He appears to be middle-aged. He is wearing rectangular eyeglasses with dark frames. He has small earrings visible. He is wearing a dark, possibly purple or maroon, collared, button-up shirt with a subtle geometric pattern."*

---

## Audio Input

The model supports audio transcription (ASR) via the same `--mmproj`. Audio support is marked **experimental** in the server logs.

### API Format

Send the audio as **raw base64** (NOT as a data URI — the `data:audio/wav;base64,` prefix causes a parse error):

```json
{
  "messages": [
    {
      "role": "user",
      "content": [
        {"type": "input_audio", "input_audio": {"data": "UklGRiQAAABXQVZFZm10...", "format": "wav"}},
        {"type": "text", "text": "Transcribe this audio. What is being said?"}
      ]
    }
  ],
  "max_tokens": 200,
  "stream": false
}
```

### PowerShell Example

```powershell
$audioBytes = [System.IO.File]::ReadAllBytes("path\to\audio.wav")
$b64 = [Convert]::ToBase64String($audioBytes)

$body = @{
    messages = @(
        @{role = "user"; content = @(
            @{type = "input_audio"; input_audio = @{data = $b64; format = "wav"}}
            @{type = "text"; text = "Transcribe this audio."}
        )}
    )
    max_tokens = 200
    stream = $false
} | ConvertTo-Json -Depth 5

$r = Invoke-RestMethod "http://127.0.0.1:62966/v1/chat/completions" `
    -Method POST -Body $body -ContentType "application/json"
$r.choices[0].message.content
```

### Important Notes

- **`data` must be raw base64** — do NOT prepend `data:audio/wav;base64,`. That prefix causes "Failed to load image or audio file".
- **`format` field is required** — must be `"wav"` or `"mp3"`. Without it you get: `"input_audio.format must be either 'wav' or 'mp3'"`.
- Place audio content **after** text in the content array (per Gemma 4 docs).
- Maximum audio duration: ~30 seconds.
- The audio encoder adds ~300M parameters to VRAM usage.

### Example Output

**Input:** WAV file saying "Test one one."
**Output:**
> *"Test one one."*

---

## MTP + Multimodal Performance Notes

- With `--mmproj` loaded, VRAM usage is ~3700 MiB (model + mmproj + draft + cache).
- Without `--mmproj` (text-only), VRAM drops to ~2554 MiB.
- MTP and multimodal work together — image/audio prompts still benefit from MTP speculative decoding.
- First multimodal request is slower (cold-start on the encoder).
- Generation speed drops to ~64-85 tok/s for image inputs (encoder processing overhead), vs ~216 tok/s for pure text.

---

## Common Errors & Fixes

| Error | Cause | Fix |
|-------|-------|-----|
| `413 Payload Too Large` | Image too big (>10 MB) | Resize image before sending |
| `Failed to load image or audio file` | Invalid data URI format | Use raw base64 for audio; use `data:image/...` for images |
| `input_audio.format must be either 'wav' or 'mp3'` | Missing `format` field | Add `"format": "wav"` to input_audio |
| `Failed to initialize the context: Gemma4Assistant requires ctx_other` | Normal during memory fitting | Ignore — server handles it automatically |
