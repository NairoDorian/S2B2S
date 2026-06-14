# Architecture Patterns Catalog

> The best technical patterns from all 22 reference projects.
> Organized by subsystem so S2B2S developers can find the right pattern quickly.

---

## 1. TTS Engine Patterns

### 1.1 Engine Trait Abstraction (CopySpeak → S2B2S)
```
trait TtsBackend {
    fn synthesize(text, voice, speed) -> Vec<u8>;  // WAV bytes
    fn health_check() -> bool;
    fn file_extension() -> &str;  // "wav" | "mp3" | "ogg"
    fn voice_display_name() -> String;
}
```
**File:** copyspeak-tts `src-tauri/src/tts/mod.rs` (72 lines)
**S2B2S status:** Adopted — `src-tauri/src/tts/mod.rs` TtsBackend trait

### 1.2 Persistent Server Pattern (CopySpeak)
```
State machine: Stopped → Starting{generation, config, stderr_tail} → Ready(ActiveServer)
Guarded by CURRENT_GENERATION counter to prevent stale starters clobbering newer ones.
Pre-warm at startup + hidden warm-up synthesis kills 1.6s first-request penalty.
Health polling with exponential backoff 100→1600ms.
```
**File:** copyspeak-tts `src-tauri/src/tts/piper_server.rs` (498 lines)
**S2B2S status:** Adopted for Piper, Kokoro, Kitten, Pocket

### 1.3 Engine Pool with Checkout/Return (Parrot)
```
Pool of N KokoroEngine instances (N = CPU cores).
take_engine_for_active_request() → return_engine_to_slot().
Mutex-free during synthesis (engines are per-request, not shared across threads).
```
**File:** parrot `src-tauri/src/managers/tts.rs` (~lines 200-400)
**S2B2S status:** NOT adopted — S2B2S uses persistent HTTP servers per engine, not pools

### 1.4 shorten_first_chunk Trick (Parrot)
```
FIRST_CHUNK_TARGET_CHARS = 150 (vs CHUNK_TARGET_CHARS = 800)
split_at_clause_boundary() on [,.!?;:)] before hard substring split
Result: audio starts in ~150 chars worth of synthesis instead of waiting for 800
```
**File:** parrot `src-tauri/src/managers/tts.rs` (~line 800)
**S2B2S status:** NOT adopted — HIGH PRIORITY to adopt. Reduces time-to-first-audio by ~75%.

### 1.5 Crossfade Blending (Parrot)
```
apply_crossfade(prev_tail, samples):
    cross_len = 240.min(prev_tail.len()).min(samples.len())
    Linear fade: prev_tail[i] * (1 - t) + samples[i] * t where t = i / cross_len
```
**File:** parrot `src-tauri/src/managers/tts.rs` (~line 600)
**S2B2S status:** NOT adopted — chunk joins have audible seams without this

### 1.6 Telemetry-Driven Pagination (CopySpeak)
```
EMA chars_per_ms per engine (α=0.2, update per synthesis).
With ≥3 samples: fast engines (>1.0 c/ms) → ×3 budget capped 2000.
Moderate (0.3-1.0) → ×2 capped 1500. Slow/unknown → default.
Estimated duration = char_count / chars_per_ms → honest progress bar.
```
**File:** copyspeak-tts `src-tauri/src/telemetry.rs` (370 lines)
**S2B2S status:** Partially adopted (telemetry.rs exists, progress estimation not wired)

### 1.7 Audio Caching (CopySpeak)
```
Before synthesizing: scan history newest-first for entry with identical (text, voice, engine).
If the audio file still exists on disk → replay without synthesis.
```
**File:** copyspeak-tts `src-tauri/src/history.rs` (897 lines)
**S2B2S status:** NOT adopted — every request synthesizes fresh

### 1.8 Qwen3-TTS Vendor Pattern (vox)
```
vendored crate at vendor/qwen3-tts/ (2,208 lines lib.rs).
Full model ownership: talker generation (1030 lines), code prediction (566 lines),
KV cache, fused ops, config (875 lines), codec (10 files), platform-specific layers.
Build-time feature gating: platform/rpi5, platform/cpu, platform/metal.
```
**File:** vox `vendor/qwen3-tts/` (entire vendored crate)
**S2B2S status:** NOT adopted — reference for vendoring ML models

---

## 2. STT Engine Patterns

### 2.1 Streaming WebSocket Managers (AIVORelay)
```
Per-provider WS manager pattern:
- Bounded send queue (256 cap) for audio chunks
- interim_token / final_token split
- Exponential backoff reconnect
- Connect/read timeouts
- Thread-safe state via Arc<AtomicBool> + Mutex<State>
```
**Files:** AIVORelay `src-tauri/src/managers/{soniox_realtime,deepgram_realtime,openai_realtime_whisper}.rs`
**S2B2S status:** NOT adopted — S2B2S only has local streaming (Moonshine)

### 2.2 Persistent Engine Instances (transcribe-rs)
```
Current S2B2S usage (suboptimal): creates engine per call → loads model → transcribes → drops engine.
Optimal usage (from the crate's design): create engine once → reuse for all calls.
VadChunked transcriber: prefill-aware onset capture for real-time.
GreedyDecoder repetition guard: 127-line self-contained drop-in improvement.
```
**File:** transcribe-rs `src/` (various)
**S2B2S status:** Suboptimal — loads-and-drops per call. Fix: persist engine instances.

### 2.3 Three-Trigger VAD Endpointing (Parakeet-RT)
```
1. Natural pause: ≥0.8s since last speech AND segment ≥5s → cut
2. Max duration: segment reaches 20s → cut
3. Silence flush: ≥1.5s silence AND segment ≥1s → cut
```
**File:** Parakeet-RT `audio_capture.py` (193 lines)
**S2B2S status:** TripleVAD is better for detection; this pattern is for **turn segmentation** policy

---

## 3. Text Processing Patterns

### 3.1 pulldown-cmark Normalizer (Parrot)
```
Event walker over CommonMark AST:
- Strips emphasis/headers (not vocalized)
- Renders lists as flowing sentences
- Simplifies URLs to "example dot com"
- Simplifies inline code
- Decodes HTML entities (named + numeric)
- Ensures terminal punctuation per block
Strictly superior to regex-based sanitizers.
```
**File:** parrot `src-tauri/src/text_normalization.rs` (617 lines) + unit tests
**S2B2S status:** NOT adopted — S2B2S uses regex-based markdown stripping. Should adopt this.

### 3.2 Post-LLM Regex Replace Hook (AIVORelay)
```
text_output_hooks.rs: Find&Replace with \n/\t, case-insensitive toggle, regex with capture groups ($1).
Applied after LLM post-processing — final word on output text.
```
**File:** AIVORelay `src-tauri/src/text_output_hooks.rs`
**S2B2S status:** NOT adopted — deterministic final-output control

### 3.3 Decapitalize-After-Edit (AIVORelay)
```
Passive Backspace listener → next inserted chunk starts lowercase (one-shot, configurable timeout).
Fixes "mid-sentence correction gets a capital letter" papercut.
Works in live-streaming mode too.
```
**File:** AIVORelay `src-tauri/src/text_replacement_decapitalize.rs`
**S2B2S status:** NOT adopted

---

## 4. Brain / LLM Patterns

### 4.1 Prompt Variables (AIVORelay)
```
Template variables in prompts:
${output} → the transcript text
${current_app} → foreground application name (via active_app.rs)
${time_local} → current local time
```
**File:** AIVORelay `src-tauri/src/transcript_context.rs`
**S2B2S status:** NOT adopted — would make Brain prompts context-aware

### 4.2 Transcription Profiles (AIVORelay)
```
TranscriptionProfile { id, name, language, system_prompt, stt_prompt_override, llm_settings }
Per-profile dedicated hotkeys + cycle-profile key.
Auto-switch with Windows keyboard input language.
```
**File:** AIVORelay `src-tauri/src/managers/*` (profile system)
**S2B2S status:** NOT adopted — HIGH PRIORITY. This is the "persona/mode" system.

---

## 5. Audio & Capture Patterns

### 5.1 WASAPI Loopback Capture (AIVORelay)
```
WASAPI loopback of system audio (speakers), microphone, or "Both" mode.
Both mixing strategy: mic callback is the clock; loopback samples buffered and mixed in.
```
**File:** AIVORelay `src-tauri/src/managers/live_sound_audio.rs` (528 lines)
**S2B2S status:** NOT adopted — would let S2B2S hear podcasts/meetings

### 5.2 RNNoise Denoise Stage (AIVORelay → S2B2S)
```
nnnoiseless crate: pre-STT denoise on the audio buffer.
```
**File:** AIVORelay uses `nnnoiseless`; S2B2S's `audio_toolkit/audio/noise_suppression.rs` also uses it
**S2B2S status:** Adopted — used in TripleVAD stage 2 (voice probability)

### 5.3 200ms Preroll on Windows (CopySpeak)
```
Windows output device wake-up clips first phonemes. 200ms near-silent preroll fixes it.
rodio Sink → append 200ms of near-silence → then play real audio.
Was 1200ms in earlier versions — tuned down.
```
**File:** copyspeak-tts `src-tauri/src/audio/player.rs` (274 lines)
**S2B2S status:** Check if adopted — relevant if Windows TTS has onset clipping

---

## 6. Selection & Input Patterns

### 6.1 macOS AX API Selection Capture (Parrot)
```
Direct accessibility API read (NO clipboard touched):
AXUIElementCreateSystemWide() → focused element → AXSelectedText.
Retry at 0/40/90ms because some apps expose selection late.
```
**File:** parrot `src-tauri/src/actions.rs` (298 lines)
**S2B2S status:** Partially adopted? Check actions.rs

### 6.2 Sentinel Clipboard Probe Fallback (Parrot)
```
Windows/Linux fallback when AX API unavailable:
1. Write unique sentinel to clipboard
2. Synthesize Cmd/Ctrl+C
3. Poll until clipboard ≠ sentinel
4. Read selection
5. RESTORE user's original clipboard
Recent fix: prevents terminal flash & focus-steal on Windows during Ctrl+C.
```
**File:** parrot `src-tauri/src/actions.rs` (same file)
**S2B2S status:** NOT adopted — S2B2S uses clipboard directly

### 6.3 Double-Copy Trigger (CopySpeak)
```
Dedicated Win32 thread: AddClipboardFormatListener on a message-only window.
WM_CLIPBOARDUPDATE pump → same text twice within double_copy_window_ms (1.5s) → emit SpeakRequest.
Zero-polling — message-driven, not a timer loop.
```
**File:** copyspeak-tts `src-tauri/src/clipboard.rs` (487 lines)
**S2B2S status:** Partially adopted (clipboard_watch.rs exists)

---

## 7. Settings & Config Patterns

### 7.1 Bindings Backfill on Read (Parler)
```
On settings load: check for newly-introduced default bindings.
If stored settings lack them → merge defaults in → persist.
Fixes "undefined binding" UI bugs and prevents accessibility prompt spam.
```
**File:** Parler `src-tauri/src/settings.rs` lines 943-953
**S2B2S status:** NOT adopted — CRITICAL. Every new hotkey needs this.

### 7.2 Settings Export/Import (Parler)
```
JSON dump/restore of entire settings store.
Trivial implementation, loved by users.
```
**File:** Parler `src/components/settings/ExportImportSettings.tsx`
**S2B2S status:** NOT adopted — simple QoL feature

### 7.3 Serde-Default Pattern (Handy → all forks)
```
Every new setting field gets #[serde(default = "default_value")].
Safely evolves the schema without migration scripts.
Old stored JSONs with missing fields just get the defaults.
```
**File:** Handy `src-tauri/src/settings.rs` (~1400 lines)
**S2B2S status:** Adopted (inherited from Handy)

### 7.4 Capability-Gated Settings (S2B2S planned → from CursorFX)
```
OverlayCapabilities { follow_cursor, click_through, transparency, layer_shell, webgpu_in_webview, native_wgpu }
Frontend greys out unsupported options per machine instead of failing silently.
```
**File:** S2B2S `src-tauri/src/overlay_fx/capabilities.rs` (planned)
**S2B2S status:** Planned in futuristic_analysis — not yet implemented

---

## 8. Overlay & GPU Patterns

### 8.1 Per-Frame Style Guard (CursorFX)
```
Windows transparent overlay window:
WS_EX_TRANSPARENT | WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE
WndProc subclass: WM_NCHITTEST → HTTRANSPARENT, WM_SETCURSOR → 1, WM_ERASEBKGND → 1
Re-assert styles + topmost ON EVERY FRAME.
Games/installers/other apps steal Z-order and reset styles.
```
**File:** CursorFX `src-tauri/src/overlay/mod.rs` (~500 lines)
**S2B2S status:** Planned for Phases 1-4 overlay feature

### 8.2 Vulkan, NOT DX12 (CursorFX)
```
DX12 backend fails with OutOfMemory on transparent overlay (tested on RTX 4070).
Proven path: Vulkan backend + NVAPI "Prefer Native" present fix.
NVAPI fix: nvapi64.dll, DRS session → base profile → set 0x20324987 = 0 → save.
```
**File:** CursorFX `src-tauri/src/overlay/mod.rs` (platform section, ~60 lines for NVAPI)
**S2B2S status:** Planned — critical for Track B native overlay

### 8.3 On-Demand Render Loop (CursorFX + TD_Web_Trail)
```
Zero frames when hidden (window not visible → render loop parked).
Idle sleep after 2 still frames (no mouse move/state change → 0% CPU).
Frame pacing: animate at 24fps idle breathing, up to monitor refresh when active.
Surface recreation on Outdated/Lost (handle display changes).
```
**Files:** CursorFX `overlay/renderer.rs`, TD_Web_Trail `index.html`
**S2B2S status:** Planned for overlay feature

### 8.4 wgpu Surface from Tauri Window (CursorFX)
```
let surface = instance.create_surface_unsafe(
    SurfaceTargetUnsafe::RawHandle { raw_display_handle, raw_window_handle }
);
Pick surface format: caps.formats.iter().find(|f| f.is_srgb());
Pick alpha mode: caps.alpha_modes.iter().find(|m| *m == PostMultiplied || *m == PreMultiplied);
Clear to transparent: LoadOp::Clear(Color { r: 0, g: 0, b: 0, a: 0 });
Present mode: Fifo, desired_maximum_frame_latency: 2.
```
**File:** CursorFX `src-tauri/src/overlay/mod.rs`
**S2B2S status:** Planned for Track B

### 8.5 Spring-Friction Tether Physics (TD_Web_Trail)
```
Per-point per-frame:
    v[i] = v[i] * friction + (target[i] - pos[i]) * spring
    pos[i] = pos[i] + v[i]
spring = 0.39, friction = 0.5

Distance constraint solver: keeps chain from stretching unboundedly.
Catmull-Rom smooth: upsample physics points into smooth spline (catmull_steps = 4).
```
**File:** TD_Web_Trail `index.html` (script section)
**S2B2S status:** Planned for avatar tether

### 8.6 4-Pass Tapered Neon Glow (TD_Web_Trail)
```
Pass 1: blurred canvas, width×1.5, color→black at tail (glow aura)
Pass 2: main canvas, width×1.0, color→black (body)
Pass 3: main canvas, width×0.7, black with alpha (depth mask)
Pass 4: main canvas, width×0.3, solid color, alpha fades (bright core)

Width taper: w(p) = base_width * (1 - p)^1.5 where p = i/(N-1)
```
**File:** TD_Web_Trail `index.html` (script section)
**S2B2S status:** Planned for cursor trail + avatar glow

---

## 9. Security & Lifecycle Patterns

### 9.1 lock_or_recover! Macro (CopySpeak)
```
Macro that wraps Mutex.lock() → if poisoned, recovers and logs a warning.
Applied across synthesis, queue, telemetry, piper_server mutexes.
Prevents one poisoned mutex from deadlocking the entire app.
```
**File:** copyspeak-tts `src-tauri/src/main.rs` (~line 50, macro definition)
**S2B2S status:** NOT checked — may or may not be adopted

### 9.2 Encrypted Local Bridge (AIVORelay)
```
ECDH P-256 handshake → HKDF-derived AES-GCM payload encryption → HMAC integrity.
Localhost-only bind (127.0.0.1), minimal route set, CORS configured for extension.
Not a toy — genuinely engineered local channel for browser extension comms.
```
**File:** AIVORelay `src-tauri/src/managers/connector.rs`
**S2B2S status:** NOT adopted — reference for control server upgrade

### 9.3 Crash Logging (Parler)
```
Panic capture to file: set_hook → write backtrace + metadata to crash_log.txt.
Lightweight (80 lines), early in startup so it catches init panics too.
```
**File:** Parler `src-tauri/src/crash_logging.rs` (80 lines)
**S2B2S status:** NOT checked — may already have something similar

---

### 4.3 YAML Model Config (LocalAI)

```
name: whisper-base
backend: whisper
parameters:
  model: whisper-base.bin
gRPC contract → implementation: single Go interface, all 36+ backends implement it.
Gallery system: 78+ YAML files → auto-download + config + launch.
Importer strategy pattern: per-model-type importers (huggingface, ollama, local file).
```
**File:** LocalAI `core/config/model_config.go` (1,610 lines), `gallery/` (78 YAML files)
**S2B2S status:** NOT adopted — S2B2S hardcodes backends in Rust settings.rs. Adopt the declarative pattern.

### 4.4 GPU Auto-Detection (LocalAI)

```
Detect available GPUs at startup → map to backend-specific config.
Supports: CUDA, Vulkan, Metal, HIP, SYCL, multi-GPU.
Per-backend GPU offload configuration from a single detection pass.
```
**File:** LocalAI `core/backend/gpu.go` (pattern)
**S2B2S status:** Partially — llama.cpp manager detects CUDA/Vulkan. LocalAI's approach is richer.


## 10. Frontend Patterns

### 10.1 Provider Matrix Pattern (whispering)
```
const PROVIDERS = [
    { id: 'whisper-local', name: 'Whisper (Local)', ... },
    { id: 'openai', name: 'OpenAI', ... },
] as const satisfies ProviderRegistry[];

type ProviderId = (typeof PROVIDERS)[number]['id']; // "whisper-local" | "openai" | ...
Dispatch table maps ProviderId → implementation.
```
**File:** whispering `src/lib/providers.ts` (261 lines)
**S2B2S status:** Concept donor (AGPL) — S2B2S uses a different pattern

### 10.2 Canonical Transcript Contract (speech-recognition)
```
TranscriptResult as cross-model POJO:
{ text, segments: [{ text, start, end, words: [{ text, start, end, confidence }] }] }
All model families normalize to this single output format.
S2B2S equivalent: TranscriptionResult in transcribe-rs.
```
**File:** speech-recognition `src/core/transcript.ts`
**S2B2S status:** transcribe-rs has its own format

### 10.3 Progressive 5-Stage Pipeline (speech-recognition)
```
1. IN_PROGRESS — raw token stream
2. PARTIAL — assembled tokens, not final
3. FINAL — complete utterance
4. POST_PROCESSED — after formatting rules
5. COMPLETE — fully cleaned output
```
**File:** speech-recognition — pipeline architecture
**S2B2S status:** Similar but different stages in the Brain pipeline

---

## Quick Reference: Where to Find Each Pattern

| What you need | Look in |
|---------------|---------|
| **TTS engine design** | copyspeak-tts_review.md + copyspeak-tts src-tauri/src/tts/ |
| **TTS latency tricks** | Parrot_review.md + parrot src-tauri/src/managers/tts.rs |
| **Streaming STT (cloud WS)** | AIVORelay_review.md + AIVORelay managers/soniox_realtime.rs |
| **VAD / turn detection** | Parakeet-RT_review.md + Parakeet-RT audio_capture.py |
| **STT engine persistence** | transcribe-rs_review.md |
| **Text normalization** | Parrot_review.md + parrot src-tauri/src/text_normalization.rs |
| **Profiles / modes system** | AIVORelay_review.md |
| **GPU overlay window** | CursorFX_review.md + CursorFX src-tauri/src/overlay/ |
| **Cursor trail physics** | TD_Web_Trail_review.md + TD_Web_Trail index.html |
| **Settings backfill** | Parler_review.md + Parler settings.rs lines 943-953 |
| **Prompt variables** | AIVORelay_review.md |
| **Security / crypto** | AIVORelay_review.md + AIVORelay managers/connector.rs |
| **Voice cloning** | voicebox_review.md + pocket-tts-server_review.md |
| **MCP server** | voicebox_review.md |
| **OpenAI-compatible server** | LocalAI_review.md |
| **YAML model configs** | LocalAI_review.md |
| **GPU auto-detection** | LocalAI_review.md |
| **Qwen3-TTS integration** | vox_review.md + vox vendor/qwen3-tts/ |
| **Multi-engine TTS registry** | TTS-Audio-Suite_review.md |
| **Rust TTS monorepo** | vibevoice-rs_review.md + voirs_review.md |

---

*This catalog collects the single best pattern from each reference project. For full context, read the individual project reviews in `references_comparative_analysis_md/*_review.md`.*
