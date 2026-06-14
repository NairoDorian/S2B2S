# pocket-tts-server -- Research/Reference Project

> Repo: `ai-joe-git/pocket-tts-server` · HEAD: N/A (local snapshot) · License: MIT · Author: ai-joe-git · Platforms: Windows 10/11 (primary), Linux/macOS (manual setup)
> Nature: independent (wraps `kyutai-labs/pocket-tts` Python library)
> Role for S2B2S: Reference for Voice Cloning -- S2B2S inherited the core TTS inference model via `pocket_server.py` and integrated it as the Pocket voice-cloning backend. The standalone server's web UI, LLM chat, streaming SSE, multi-format audio conversion, and OpenAI-compatible API surface are studied but not directly used.

---

## 1. What pocket-tts-server Is

pocket-tts-server is a standalone, self-contained Python application that wraps the [kyutai-labs/pocket-tts](https://github.com/kyutai-labs/pocket-tts) library (a voice-cloning TTS model by Kyutai) behind a user-friendly web interface and an OpenAI-compatible REST API. It supports voice cloning from as little as 15-20 seconds of reference audio, real-time voice chat with an LLM backend, and serves as a drop-in replacement for the OpenAI TTS API.

The project targets Windows users who want a double-click install experience -- it ships `.bat` installers, auto-downloads PyTorch CPU, creates a venv, installs ffmpeg, and handles HuggingFace authentication. It ships with 3 celebrity voice samples (Donald Trump, Barack Obama, Joe - original) and supports uploading custom voices through drag-and-drop in the web UI.

The project is primarily a UX/API wrapper: all actual TTS inference is delegated to the `pocket-tts` Python package (from Kyutai), which loads a Duck model (Mimi neural audio codec + autoregressive transformer) from HuggingFace. Voice cloning works by feeding a reference WAV through the model's encoder to produce a "voice state" embedding that is then used as conditioning for text-to-speech synthesis.

---

## 2. Tech Stack

### 2.1 Frontend
| Layer | Choice | Purpose |
|-------|--------|---------|
| Web server templates | Jinja2-free static HTML | Single `templates/index.html` (1728 lines) with embedded CSS + vanilla JS |
| Web client (standalone) | `web_client.html` (445 lines) | Lighter alternative interface without LLM chat |
| Styling | Custom CSS (dark theme) | No framework; hand-rolled gradient-based dark UI |
| Audio playback | HTML5 `<audio>` + base64 WAV | Sequential audio queue for sentence-by-sentence playback |
| Streaming | SSE (Server-Sent Events) | Real-time text streaming + per-sentence audio chunks |

### 2.2 Backend / Core
| Layer | Choice | Purpose |
|-------|--------|---------|
| HTTP framework | FastAPI 0.100+ | REST API + SSE streaming + file uploads |
| ASGI server | Uvicorn 0.23+ | Production-grade async server |
| TTS inference | `pocket-tts` (PyPI) | The kyutai-labs voice-cloning model |
| Audio I/O | scipy.io.wavfile + pydub | WAV read/write + MP3/OGG/FLAC conversion |
| Torch | PyTorch 2.5+ (CPU pref.) | Model runtime |
| LLM client | requests (sync) | HTTP calls to llama.cpp / Ollama / any OpenAI-compatible server |
| Data validation | Pydantic 2.0+ | Request/response models |
| CORS | FastAPI CORSMiddleware | All origins allowed (`*`) |

### 2.3 Key Dependencies (non-obvious ones)
| Dependency | Why it matters |
|-----------|---------------|
| `huggingface-hub` | Required for auth; `pocket_tts` downloads model weights from `kyutai/pocket-tts` on HuggingFace (gated repo -- must accept terms) |
| `audioop-lts` | Backport of Python's removed `audioop` module (PEP 594); needed for pydub on Python 3.13+ |
| `einops` | Used by the Duck model internally for tensor reshaping operations |
| `safetensors` | Model weight loading format used by pocket-tts |
| `sentencepiece` | Tokenizer for the TTS model's text encoder |

---

## 3. Architecture & Source Map

```
pocket-tts-server/
|
|-- pocket_tts_api.py          (1082 lines)  **THE CORE** -- FastAPI server with all endpoints
|   |  - config load/save (35 lines)
|   |  - TTSModel singleton lifecycle (lines 96-114)
|   |  - Voice scanning & auto-conversion (120-162)
|   |  - Voice state caching (165-187)
|   |  - Audio format conversion WAV/MP3/OGG/FLAC -> 24kHz mono WAV (190-306)
|   |  - FastAPI lifespan (309-324)
|   |  - /v1/audio/speech       OpenAI-compatible TTS (347-420)
|   |  - /v1/audio/voices       OpenAI-compatible voice list (423-438)
|   |  - /v1/chat/completions   Chat + TTS (non-streaming) (794-859)
|   |  - /v1/chat/completions/stream  SSE streaming chat (779-791)
|   |  - /api/config            Config get/set (865-890)
|   |  - /api/voices/upload     Voice upload + auto-conversion (941-1047)
|   |  - /health                Health check (927-935)
|   |  - / (root)               Static HTML serve (895-925)
|   |  - call_llm()             Sync LLM client (444-545)
|   |  - stream_llm_tokens()    Async SSE token generator (548-625)
|   |  - stream_chat_response() Real-time text+audio SSE generator (672-776)
|   |  - split_into_sentences() Regex sentence boundary detector (644-651)
|   |  - generate_sentence_audio_sync()  Per-sentence TTS in thread pool (654-669)
|
|-- audio_utils.py              (93 lines)     Standalone WAV conversion helpers (partially redundant with pocket_tts_api.py)
|   |  - ensure_wav_format()    Force audio to 24kHz mono WAV
|   |  - convert_uploaded_audio()  Upload -> WAV pipeline
|
|-- templates/index.html        (1728 lines)   Full web UI: Voice Chat, TTS, Settings, Voice Library
|   |  - Dark theme, sidebar nav, modal voice selector
|   |  - Streaming SSE client (ReadableStream + SSE parsing, lines 1561-1640)
|   |  - Sequential audio queue with HTML5 Audio API (lines 1459-1516)
|   |  - Voice upload with drag-and-drop (lines 1387-1441)
|   |  - LLM settings form with test-connection (lines 1307-1379)
|   |  - Chat history (last 20 messages, lines 1161-1163)
|
|-- web_client.html             (445 lines)    Simplified standalone client (no LLM chat)
|-- openai_client_example.py    (209 lines)    Python SDK example using requests
|-- config.json                 (44 lines)     Server config, LLM endpoint, system prompt
|-- requirements.txt            (20 lines)     Python dependencies
|-- LLM_INTEGRATION.md          (99 lines)     Guide for llama.cpp, Ollama, LM Studio
|
|-- [Batch scripts - Windows installer ecosystem]
|   |-- install_pocket_tts.bat (171 lines)   Full venv creation, PyTorch CPU, dependencies
|   |-- run_pocket_tts.bat     (115 lines)   Venv activation, port config, server launch
|   |-- fix_dependencies.bat   (51 lines)    Force-reinstall critical packages
|   |-- preflight_checks.bat   (74 lines)    ffmpeg install + HuggingFace auth wizard
|
|-- voices-celebrities/         (3 files)     Active voice WAV files + stray txt.txt
|-- screenshots/                (3 images)    UI screenshots for README
```

---

## 4. Feature Inventory

### 4.1 Voice Cloning Pipeline
**What:** Upload 15-20s of audio (WAV/MP3/OGG/FLAC) to clone a voice. **How:**
1. Upload via web form (`POST /api/voices/upload`) or manual copy to `voices-celebrities/`
2. pydub converts to 24kHz mono 16-bit WAV (lines 1003-1014 of `pocket_tts_api.py`)
3. Audio trimmed to 20s max to prevent gibberish (lines 256-262)
4. Original file archived to `voices-celebrities-archive/`
5. On first use, `tts_model.get_state_for_audio_prompt(wav_file)` computes the voice embedding (line 178)
6. Voice state cached in `voice_states` dict to avoid recomputation

**Files:** `pocket_tts_api.py` lines 120-306 (scanning + conversion), lines 165-187 (state loading/caching), lines 941-1047 (upload endpoint)

### 4.2 OpenAI-Compatible TTS API
**What:** Drop-in replacement for OpenAI's `/v1/audio/speech` endpoint. **How:**
- `POST /v1/audio/speech` accepts `model`, `input`, `voice`, `response_format` (mp3/wav), `speed` (0.25-4.0)
- `GET /v1/audio/voices` returns voice list in OpenAI format
- Voice state loaded on-demand if not in cache
- Output converted to requested format (WAV via scipy, MP3 via pydub)

**Files:** `pocket_tts_api.py` lines 347-438

### 4.3 Voice Chat with LLM (Non-Streaming)
**What:** Send messages to an LLM, get text response + TTS audio in one call. **How:**
- `POST /v1/chat/completions` calls LLM via `call_llm()` (sync requests)
- Falls back to echo mode if LLM disabled
- Response includes both `choices[0].message.content` and `audio.data` (base64 WAV)
- 4000 max tokens, 180s timeout

**Files:** `pocket_tts_api.py` lines 444-545 (call_llm), 794-859 (chat_completions endpoint)

### 4.4 Streaming Voice Chat (SSE)
**What:** Real-time text token streaming + per-sentence audio as soon as each sentence completes. **How:**
- `POST /v1/chat/completions/stream` returns `text/event-stream`
- Client receives SSE events: `{"type":"text","content":"word "}` (streaming text), `{"type":"audio","data":"base64...","chunk":N}` (per-sentence WAV), `{"type":"done"}` (end)
- Sentence boundary detection via regex `split_into_sentences()` -- splits on `[.!?。！？\n]`
- Audio for each sentence generated synchronously in a thread pool executor (`run_in_executor`)
- Pending sentences cleared from buffer after TTS generation

**Files:** `pocket_tts_api.py` lines 644-776 (server-side generator), `templates/index.html` lines 1561-1640 (client-side SSE reader)

### 4.5 Audio Format Conversion
**What:** Auto-conversion of uploaded audio to 24kHz mono 16-bit WAV. **How:**
- `convert_to_wav()` (lines 190-306): checks if already WAV, converts MP3/OGG/FLAC via pydub
- Handles: stereo->mono, any sample rate->24000Hz, any bit depth->16-bit
- Trims to 20s max (20000ms)
- Archives original files to `voices-celebrities-archive/`
- Supports Python 3.13 via `audioop-lts` backport

**Files:** `pocket_tts_api.py` lines 190-306, `audio_utils.py` lines 19-93

### 4.6 Configuration System
**What:** Persistent JSON config with web UI. **How:**
- `config.json` with sections: `server`, `paths`, `llm`, `audio`, `voice`, `processing`, `features`
- Loaded at startup with default merging (lines 53-94)
- `GET/POST /api/config` for reading/updating
- LLM config: `enabled`, `api_url`, `api_key`, `model`, `system_prompt`
- Web UI settings tab with "Test Connection" button

**Files:** `pocket_tts_api.py` lines 53-94 (load_config), 865-890 (config endpoints), `config.json`

### 4.7 Windows Installer Ecosystem
**What:** Batch script-based installation for non-technical users. **How:**
- `install_pocket_tts.bat`: creates venv, installs PyTorch CPU, all deps, runs preflight checks
- `run_pocket_tts.bat`: activates venv, extracts host/port from config.json, launches server
- `fix_dependencies.bat`: force-reinstalls pocket-tts, soundfile, scipy, numpy, pydub, audioop-lts
- `preflight_checks.bat`: installs ffmpeg via winget, guides HuggingFace login
- All scripts use `choice /c` for interactive menus

**Files:** All 4 `.bat` files (411 lines total)

---

## 5. Key Code Patterns & Techniques

### 5.1 Voice State Caching Pattern
**File:** `pocket_tts_api.py` lines 165-187
Voice embeddings are expensive to compute (one full encoder pass per voice). The server caches them in a global `voice_states: dict` dictionary. On first use, `get_voice_state()` loads via `tts_model.get_state_for_audio_prompt(wav_file)`. Subsequent requests hit the cache. This is a classic lazy-loading pattern with infinite TTL -- no eviction, no LRU. For a desktop app with <100 voices this is fine.

### 5.2 Sentence-Level TTS Streaming
**File:** `pocket_tts_api.py` lines 672-776
The streaming chat response (`stream_chat_response`) is the most sophisticated pattern:
1. Tokens stream from LLM via `stream_llm_tokens()` (async generator)
2. Each token is yielded as SSE text event immediately
3. Tokens accumulate in `sentence_buffer`
4. On sentence-end character (`.!?。！？\n`), `split_into_sentences()` extracts complete sentences
5. Each sentence >5 chars triggers TTS synthesis in a thread pool (`loop.run_in_executor`)
6. Audio is base64-encoded and yielded as SSE audio event
7. The sentence_buffer is cleared, and processing continues with remaining partial text

This pattern achieves "first audio in 2-3 seconds" by synthesizing as soon as the first sentence completes, without waiting for the full LLM response.

### 5.3 Regex Sentence Splitter
**File:** `pocket_tts_api.py` lines 644-651
```python
sentences = re.split(r"(?<=[.!?])\s+", text.strip())
```
Simple regex-based sentence splitting that preserves punctuation. Handles ASCII sentence enders. The `.` in "Mr." or "Dr." will cause false splits -- this is a known limitation (no abbreviation handling).

### 5.4 Sync-in-Async Pattern
**File:** `pocket_tts_api.py` lines 729-735
The TTS model's `generate_audio()` is synchronous (blocking). The FastAPI async handler wraps it in `loop.run_in_executor(None, generate_sentence_audio_sync, ...)` to avoid blocking the event loop. This is the standard FastAPI pattern for CPU-bound work.

### 5.5 Config Merging with Defaults
**File:** `pocket_tts_api.py` lines 53-91
Deep-merge of user config with hardcoded defaults. Only fills in missing keys; preserves all user-set values. This ensures the config file is always complete and new config options get defaults automatically when the server upgrades.

### 5.6 Graceful Degradation
**File:** `pocket_tts_api.py` lines 31-49, multiple locations
The server handles missing dependencies gracefully:
- `pocket_tts` not installed -> TTS endpoints return 503
- `pydub` not installed -> voice conversion disabled, warning logged
- LLM disabled -> echo mode fallback
- LLM unreachable -> error message in chat response (not 500)
- Template not found -> inline fallback HTML

---

## 6. Relation to S2B2S

S2B2S does NOT use pocket-tts-server directly. Instead, it:
1. Extracted the core TTS inference into `pocket_server.py` (189 lines) -- a minimal, dependency-light HTTP server using only Python stdlib `http.server`
2. Built a Rust `PocketBackend` (184 lines) implementing the `TtsBackend` trait
3. Manages the Python child process lifecycle via `local_tts_server.rs` (648 lines) -- a shared engine manager for Kokoro, Kitten, and Pocket

### What S2B2S took from pocket-tts-server:
- The core voice-cloning API: `TTSModel.load_model()` + `get_state_for_audio_prompt()` + `generate_audio()`
- The fixed set of 8 built-in voices (alba, marius, javert, jean, fantine, cosette, eponine, azelma)
- The concept of 24kHz mono 16-bit WAV as the standard format

### What S2B2S created independently:
- A minimal HTTP contract: `POST /` with `{"text","voice","length_scale"}` -> WAV bytes, `GET /voices` -> voice list
- Full child process lifecycle management (genesis-based spawn/kill, port allocation, health polling, warmup synthesis)
- Integration with S2B2S's TTS pipeline (sanitize -> paginate -> synthesize -> streaming gapless playback)
- Cloned voice WAV import and persistent storage in `models/TTS/pocket-cloned-voices/`

### What S2B2S deliberately omitted:
- The FastAPI web framework (replaced with stdlib http.server for zero extra deps)
- All web UI (`templates/index.html`, `web_client.html`)
- LLM integration (`call_llm`, `stream_llm_tokens`) -- S2B2S has its own Brain subsystem
- SSE streaming chat -- S2B2S handles streaming at the Tauri event layer
- OpenAI API compatibility layer -- not needed since TTS is called internally
- Multi-format audio conversion (MP3 output) -- S2B2S uses WAV internally
- The Windows batch installer ecosystem

| Aspect | pocket-tts-server | S2B2S Pocket Backend | Verdict |
|--------|-------------------|---------------------|---------|
| HTTP framework | FastAPI + Uvicorn | Python stdlib `http.server` | S2B2S simpler, zero deps |
| Server scope | Full web app + API | Minimal inference server | S2B2S focused on what matters |
| Voice state caching | Lazy dict (infinite TTL) | Per-request `get_state_for_audio_prompt()` | S2B2S simpler, always fresh |
| Synthesis API | `generate_audio()` (full) | `generate_audio_stream()` + torch.cat | S2B2S uses streaming API |
| Audio format | WAV + MP3 output | WAV only (internal) | S2B2S avoids pydub dependency |
| LLM integration | Built-in (requests + SSE) | None (Brain subsystem separate) | Cleaner separation |
| Web UI | 1728-line vanilla JS SPA | None (React/TypeScript frontend) | Different UI stacks |
| Process lifecycle | Manual (`python pocket_tts_api.py`) | Automated via `local_tts_server.rs` | S2B2S superior UX |
| Voice management | File-system scan + upload | File-system scan + import API | Similar pattern |
| Error handling | 503/500 HTTP responses | Rust Result<> + process kill on error | S2B2S more robust |
| Platform support | Windows primary, manual Linux/Mac | Cross-platform (Windows/macOS/Linux) | S2B2S broader |

---

## 7. Harvest List (Features Worth Copying)

| Feature to harvest | From file | Effort | Why valuable for S2B2S |
|-------------------|-----------|--------|----------------------|
| 20s audio auto-trim for voice cloning | `pocket_tts_api.py:190-239` | S | Prevents garbled audio from overly long reference samples. S2B2S could add this to `pocket.rs` voice import. |
| Multi-format audio upload (MP3/OGG/FLAC -> WAV) | `pocket_tts_api.py:190-306` | M | S2B2S currently expects WAV only. Adding pydub-based conversion would improve UX for cloned voice import. |
| Sentence-level TTS streaming with SSE | `pocket_tts_api.py:672-776` | L | S2B2S already has sentence-level playback via `brain/manager.rs`. The per-sentence pre-synthesis pattern (generate audio as sentences arrive) is conceptually similar but S2B2S's approach is more robust. |
| Health check endpoint pattern | `pocket_tts_api.py:927-935` | XS | `pocket_server.py` already has `/voices` as health check. Could add explicit `/health` for consistency. |
| Graceful fallback when model unavailable | `pocket_tts_api.py:352-356` | XS | Return clear error messages instead of crashing. S2B2S's `ensure_running` already handles this well. |

---

## 8. Known Issues, Caveats & Limitations

| Issue | Severity | Impact |
|-------|----------|--------|
| No abbreviation handling in sentence splitter | Medium | `Mr. Smith` or `Dr. Jones` will split incorrectly, producing TTS at wrong boundaries in streaming mode |
| Voice state cache never evicted | Low | Memory grows unbounded with many voices; acceptable for desktop with <100 voices |
| Windows-only installer ecosystem | Medium | Linux/macOS users must install manually; mentions "Linux/Mac supported with manual setup" but no scripts |
| Stale `txt.txt` in voices-celebrities | Low | Leftover file; scanned but skipped by WAV-only glob |
| `audio_utils.py` partially duplicates `pocket_tts_api.py` | Low | `ensure_wav_format()` and `convert_to_wav()` overlap; `audio_utils.py` appears unused by the main server |
| No GPU support in installer | Medium | Installer forces PyTorch CPU. GPU users must manually reinstall torch with CUDA |
| Sync `requests` library blocks event loop | Medium | `call_llm()` uses sync `requests.post()` in an async handler (line 499). FastAPI runs it in the default thread pool, but it's not explicit |
| HuggingFace auth required for all voice cloning | High | Model weights are gated; users who skip the HF login step get "Voice not found" errors with no clear explanation in the web UI (fixed in README troubleshooting) |
| SSE stream parsing in client is fragile | Medium | `templates/index.html` line 1585 splits on `\n\n` but SSE allows `\r\n\r\n` -- may fail with certain proxies |
| No authentication/API key protection | Medium | CORS allows all origins (`*`), no auth on any endpoint; fine for localhost, dangerous if exposed |

---

## 9. Strengths & Weaknesses

### Strengths
1. **Voice cloning with minimal audio**: 15-20 seconds of reference audio produces a recognizable cloned voice. The underlying pocket-tts model (Kyutai Duck) is genuinely good at zero-shot voice adaptation.
2. **Excellent Windows DX**: The batch installer ecosystem (4 scripts, 411 lines) handles Python installation, venv creation, PyTorch CPU, HuggingFace auth, and ffmpeg -- a non-technical user can be running in minutes.
3. **Streaming architecture done right**: Token-level text streaming combined with per-sentence TTS achieves the coveted "first audio in 2-3 seconds" experience. The SSE-based approach is simple and effective.
4. **OpenAI API compatibility**: Drop-in replacement for any tool that speaks the OpenAI TTS API (OpenWebUI, SillyTavern, etc.). This dramatically increases the project's utility.
5. **Graceful degradation everywhere**: Missing dependencies, broken LLM connections, absent templates -- the server degrades instead of crashing. Every error path returns a meaningful response.
6. **Auto-conversion pipeline**: Upload any audio format, get 24kHz mono WAV automatically. Handles stereo, resampling, trimming, and archiving in one pass.

### Weaknesses
1. **Tight coupling**: The entire application is in one 1082-line Python file. Voice scanning, audio conversion, TTS synthesis, LLM integration, and config management are all interleaved. Hard to test individual components.
2. **Sync LLM client in async handler**: `call_llm()` is synchronous but called from async endpoints. While FastAPI handles this gracefully, it's architecturally inconsistent with the otherwise async design.
3. **No test infrastructure**: Zero tests. No pytest, no test files, no CI. The project is entirely "tested in production."
4. **Memory management**: No model unloading mechanism. The TTS model stays in RAM for the entire server lifetime. Voice states accumulate indefinitely.
5. **Limited sentence splitting**: The regex splitter has no abbreviation awareness, no quotation handling, no language-specific rules. Fine for English demo, breaks on edge cases.
6. **Windows-centric**: All automation is `.bat` files. The README claims Linux/Mac support but provides no scripts, no Dockerfile, no systemd service.
7. **No streaming voice cloning**: The voice state must be pre-computed from a WAV file. You cannot clone a voice from a live microphone stream (unlike the raw pocket-tts library which supports this).

---

## 10. Bottom Line / Verdict

pocket-tts-server is a polished wrapper around Kyutai's voice-cloning TTS model, optimized for Windows non-technical users who want a double-click install experience. Its real value to S2B2S was in proving that the pocket-tts Python library works well enough for production use and in demonstrating the practical voice-cloning pipeline: upload WAV, compute voice state, synthesize with that state. S2B2S wisely extracted only the core inference logic into `pocket_server.py` (a minimal 189-line HTTP server using Python stdlib) and wrapped it in Rust's robust process lifecycle management (`local_tts_server.rs`, 648 lines). The single most valuable idea from this project is the per-sentence TTS streaming pattern -- synthesizing audio as soon as each sentence boundary is detected, rather than waiting for the full response -- which S2B2S implements independently in its Brain subsystem.

**Worth studying for:** Voice cloning pipeline design, graceful degradation patterns, Windows installer UX for ML-powered desktop apps.
**Not worth copying:** The monolithic single-file architecture, the sync-in-async LLM client, the Windows-only deployment approach.

---

*Analysis generated: 2026-06-14. Category: D (Research/Reference). Every source file read and referenced.*
