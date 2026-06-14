# Comparative Analysis — All Reference Projects vs S2B2S

> **Date:** June 2026 | **Scope:** 23 projects compared against S2B2S
> **Purpose:** Identify features worth copying, patterns worth adopting, and gaps to fill.
> **Methodology:** Every file in every project was read by dedicated agents using a standardized template.

---

## 0. Project Universe at a Glance

### By Lineage
| Cluster | Projects | Relationship |
|---------|----------|-------------|
| **Handy family** | Handy, Parler, AIVORelay, Parrot | Forks — share Tauri skeleton, settings system, shortcut engine, specta IPC |
| **S2B2S** | S2B2S (this project) | Fork of Handy + Parrot + CopySpeak — merged all three |
| **Independent TTS** | CopySpeak TTS, voicebox, vox, vibevoice-rs, TTS-Audio-Suite, voirs | Independent architectures, various stacks |
| **Independent STT** | whispering, TranscriptionSuite, Parakeet-Realtime-Transcriber | Independent architectures |
| **Frameworks/Libs** | transcribe-rs, sherpa-onnx, onnx-asr, speech-recognition, speechbrain | Reusable engines |
| **Visual/Utility** | Cross_Platform_Rust_WebGPU_CursorFX, TD_Web_Trail | For S2B2S overlay/avatar |
| **Servers/Tools** | pocket-tts-server, LocalAI | Supporting infrastructure |
| **LLM/AI Server** | LocalAI | OpenAI-compatible local AI server (Go), 36+ backends for LLM/STT/TTS/Vision |

### By Role for S2B2S
| Role | Projects |
|------|----------|
| **Inherited from** | Handy (skeleton), Parler (features), Parrot (TTS), CopySpeak (TTS engine trait) |
| **Can copy code** (MIT) | Parler, AIVORelay, Parrot, CopySpeak, Handy |
| **Concept donor only** (GPL/AGPL/no-license) | TranscriptionSuite (GPL), whispering (AGPL), Parakeet-RT (no license) |
| **Dependency already** | transcribe-rs (STT engine) |
| **Integration planned** | CursorFX (overlay), TD_Web_Trail (tether physics) |
| **Research reference** | sherpa-onnx, onnx-asr, speechbrain, speech-recognition, vibevoice-rs, voirs, TTS-Audio-Suite |

---

## 1. STT Ecosystem Comparison

### 1.1 STT Pipeline Features

| Feature | Handy | Parler | AIVORelay | S2B2S | Winner |
|---------|-------|--------|-----------|-------|--------|
| Local engines | 8 families | 8 families (same) | 8 families (same) | 8 families + Moonshine streaming | S2B2S |
| Cloud engines | OpenAI-compat LLM | Gemini STT (deprecated) | Soniox, Deepgram, OpenAI Realtime | OpenAI-compat (same as Handy) | AIVORelay |
| **Streaming STT** | Moonshine streaming | Moonshine streaming | Soniox/Deepgram/OpenAI Realtime WS | Moonshine streaming | **AIVORelay** |
| VAD | Silero (smoothed) | Silero (smoothed) | Silero + RNNoise denoise | **TripleVAD** (RMS + RNNoise + Silero) | **S2B2S** |
| Pause/resume recording | No | Yes (F6) | No | No | **Parler** |
| Long-audio model switch | No | Yes (threshold 10s) | No | No | **Parler** |
| System audio capture | No | No | WASAPI loopback + Both mix | No | **AIVORelay** |
| Model auto-unload | Yes (timeout) | Yes | Yes | Yes | Tie |
| GPU accel | Metal/Vulkan/DirectML | Same + thin-LTO fix | Same | Same + CUDA llama.cpp | S2B2S |
| ITN normalization | No | No | No | text-processing crate | **S2B2S** |

### 1.2 STT Architecture Patterns Worth Adopting

1. **AIVORelay's streaming WS managers** — Bounded queues (256 cap), interim/final token split, exponential backoff reconnect, per-provider WS protocol handling. This is the reference for adding cloud streaming STT to S2B2S.
2. **Parler's pause/resume** — `AudioRecorder::with_pause_flag(Arc<AtomicBool>)` — frames captured but dropped. 89 lines in recorder.rs. Directly portable.
3. **Parler's long-audio routing** — `if duration > threshold { swap_engine }` — template for S2B2S's "short → Moonshine streaming, long → Whisper Turbo" policy.
4. **Parakeet-RT's three-trigger endpointing** — Pause (0.8s no speech + 5s segment), max-duration (20s), silence-flush (1.5s + 1s minimum). This IS the conversational turn-detection skeleton.

### 1.3 STT Gaps in S2B2S
- No streaming STT from cloud (AIVORelay has 3 implementations)
- No pause/resume recording (Parler has it)
- No system audio capture (AIVORelay has WASAPI loopback)
- No recording auto-stop watchdog (AIVORelay has it)
- No RNNoise denoise stage (AIVORelay's nnnoiseless crate)

---

## 2. TTS Ecosystem Comparison

### 2.1 TTS Engine Matrix

| Engine | Parrot | CopySpeak | voicebox | vox | S2B2S | Best Implementation |
|--------|--------|-----------|----------|-----|-------|---------------------|
| **Kokoro** | In-process (tts-rs) ONNX | CLI subprocess | Via Python | Via Qwen3 | CLI + subprocess wrapper | **Parrot** (in-process beats subprocess) |
| **Piper** | No | Persistent HTTP server + CUDA | Via Python | Via Qwen3 | Persistent HTTP server (copied from CopySpeak) | **CopySpeak** (originator) |
| Kitten | No | CLI subprocess | No | No | CLI subprocess | CopySpeak |
| OpenAI TTS | No | 9 voices, pooled client | Yes | Via Qwen3 | Yes | Tie |
| ElevenLabs | No | Voice library, MP3 native | Yes | Via Qwen3 | Yes | **voicebox** (most voices) |
| Cartesia | No | Pooled client | No | No | Yes | S2B2S |
| SAPI | No | Stub (unfinished) | No | No | Full implementation | **S2B2S** |
| Pocket TTS | No | No | No | No | Python server + voice cloning | **S2B2S** (only one with it) |
| Qwen3-TTS | No | No | Via Qwen3 | Deep vendor integration | No | **vox** (deepest integration) |
| RVC (voice cloning) | No | No | Yes | No | No | **voicebox** |
| Piper TTS (fork) | No | No | Yes | No | No | **voicebox** |

### 2.2 TTS Architecture Comparison

| Aspect | Parrot | CopySpeak | S2B2S | Best Pattern |
|--------|--------|-----------|-------|--------------|
| Engine abstraction | Kokoro-specific (no trait) | **TtsBackend trait** | Copied TtsBackend trait | **CopySpeak** (originator) |
| Engine lifecycle | Pool + checkout/return | Create per-speak | Persistent server + warm/cool | **CopySpeak** (persistent server) |
| Text chunking | Sentence-based + shorten-first | Char-budget + adaptive telemetry | Sentence splitter + paginate | **Parrot** (shorten-first) + **CopySpeak** (adaptive) |
| Crossfade | 240-sample linear | None (sequential) | Reused CopySpeak | **Parrot** (only one with crossfade) |
| TTS pre-warm | No | Piper pre-warm + hidden synthesis | Yes (from CopySpeak) | **CopySpeak** |
| Audio caching | No | History-based cache lookup | No | **CopySpeak** |
| Voice discovery | espeak-ng phoneme data | Dynamic `.onnx` scan | Multiple backends | **CopySpeak** (dynamic scan) |
| Progress estimation | No | **Telemetry** chars/ms EMA | No | **CopySpeak** |
| Effects | No | WalkieTalkie, GameBoy | No | **CopySpeak** |
| Multi-language | 9 via Kokoro | EN/ES | 20 via i18n, TTS varies | **S2B2S** (UI) / **Parrot** (voices) |

### 2.3 Key TTS Technique: shorten_first_chunk (Parrot, managers/tts.rs ~line 800)

The most important latency technique in any project:
```
FIRST_CHUNK_TARGET_CHARS = 150 (vs CHUNK_TARGET_CHARS = 800)
split_at_clause_boundary() splits on [,.!?;:] before hard substrings
Result: audio starts playing in 150 chars worth of synthesis, not 800
```
**S2B2S SHOULD ADOPT THIS.** Currently, long sentences wait for full synthesis.

### 2.4 Key TTS Technique: Crossfade Blending (Parrot, managers/tts.rs ~line 600)

```rust
fn apply_crossfade(prev_tail: &[f32], samples: &mut Vec<f32>) {
    let cross_len = 240.min(prev_tail.len()).min(samples.len());
    // Linear crossfade: prev_tail fades out, chunk fades in
}
```
**S2B2S DOES NOT DO THIS.** Chunk joins have audible seams.

### 2.5 TTS Gaps in S2B2S
- No shorten_first_chunk (Parrot)
- No crossfade blending (Parrot)
- No telemetry-driven progress (CopySpeak)
- No audio caching (CopySpeak)
- No voice cloning from WAV (voicebox, vox via Qwen3)
- No in-process Kokoro (Parrot uses tts-rs, S2B2S uses subprocess)
- No effects system (CopySpeak GameBoy/WalkieTalkie)

---

## 3. Voice I/O (STT + TTS + Brain Combined)

### 3.1 Full Pipeline Comparison

| Capability | AIVORelay | voicebox | S2B2S | Winner |
|------------|-----------|----------|-------|--------|
| STT + TTS + Brain | STT + LLM post, no TTS | STT + TTS + LLM (Qwen3) | STT + TTS + Brain (streaming) | **S2B2S** |
| Conversation loop | No | No | Yes (continuous, barge-in) | **S2B2S** |
| Streaming LLM | No | No (batch inference) | Yes (SSE streaming) | **S2B2S** |
| Barge-in / duplex | No | No | Yes (current_abort) | **S2B2S** |
| Voice cloning | No | Yes (RVC) | No | **voicebox** |
| Profiles/modes | Yes (TranscriptionProfiles) | No | No | **AIVORelay** |
| Vision (screen capture) | Region capture | No | No (planned) | **AIVORelay** |
| Voice commands | PowerShell gateway | No | No | **AIVORelay** |
| MCP server | No | Yes (per-client voice) | No | **voicebox** |
| Clipboard trigger | No | No | No | Tie (CopySpeak has double-copy) |
| AI Replace Selection | Yes (select+speak→LLM replace) | No | Yes (AI Replace Selection) | Tie |
| Diarization | Yes (Deepgram) | No | No | **AIVORelay** |
| File→SRT/VTT | Yes | No | No | **AIVORelay** |

### 3.2 Conversation Architecture

S2B2S is the **only** project with a true conversation loop:
- Continuous voice with VAD-based turn segmentation
- BrainManager with streaming tokens
- TTS that speaks sentences as they stream (speak-before-finish)
- Barge-in (abort current turn, start new recording)

**AIVORelay** has the closest concept: "Live Preview window" + AI Replace.
**voicebox** has STT+TTS but no conversation loop.
**No other project** has barge-in, streaming brain, or speak-before-finish.

### 3.3 The Missing Piece: AIVORelay's Profiles

AIVORelay's `TranscriptionProfile { id, name, language, system_prompt, stt_prompt_override, llm_settings }` is the only per-context mode system. S2B2S has no equivalent — you manually change brain settings. **This is the single most important feature S2B2S should adopt from any reference project.**

---

## 4. Brain / LLM Comparison

| Capability | Handy | Parler | AIVORelay | S2B2S | Winner |
|------------|-------|--------|-----------|-------|--------|
| LLM post-processing | Yes (7 providers) | Yes (7 + Gemini) | Yes (llm_operation) | Brain (streaming, multi-turn) | **S2B2S** |
| Structured output | Yes (json_schema) | Yes | No | No | **Handy** |
| Reasoning controls | Yes (reasoning_effort) | Yes | No | No | **Handy** |
| Streaming | No | No | No | Yes (SSE) | **S2B2S** |
| Multi-turn history | No | No | No | Yes (context_turns) | **S2B2S** |
| Local model server | No | No | No | llama.cpp auto-manage | **S2B2S** |
| Apple Intelligence | Yes (Swift bridge) | Yes | No | No | **Handy** |
| Prompt variables | No | No | ${output}, ${current_app}, ${time_local} | No | **AIVORelay** |
| Post-LLM regex replace | No | No | Yes (text_output_hooks) | No | **AIVORelay** |
| System prompt library | Yes (LLMPrompt) | Yes | Per-profile | Per-brain-config | Tie |

### 4.1 LocalAI as Brain Backend Alternative

LocalAI (added June 2026) is a Go-based OpenAI-compatible local AI server with 36+ backends spanning LLM, STT, TTS, Vision, and more. It's directly relevant to S2B2S's Brain subsystem as a potential alternative to the current llama.cpp server approach.

| Aspect | S2B2S (llama.cpp) | LocalAI | Advantage |
|--------|-------------------|---------|-----------|
| LLM backends | llama.cpp only (CUDA/Vulkan/CPU) | 9 backends: llama.cpp + transformers + rwkv + diffusers + bark + whisper + vllm + v-llm + mlx | **LocalAI** |
| STT backends | transcribe-rs (8 engines) | 8 backends: whisper.cpp variants + vosk + huggingface | Tie |
| TTS backends | 8 (Piper, Kokoro, Kitten, Pocket, SAPI, OpenAI, ElevenLabs, Cartesia) | 14 backends: Piper, Coqui, ESpeak, Bark, Parler, transformer-based + cloud | **LocalAI** (more backends) |
| API compatibility | Own Tauri commands | **OpenAI / Anthropic / Ollama / ElevenLabs compatible** | **LocalAI** (ecosystem integration) |
| Image generation | No | Yes (Stable Diffusion, Flux, etc.) | **LocalAI** |
| Vision / multimodal | Planned (screen capture) | Yes (LLaVA, etc.) | **LocalAI** |
| Container/deploy | Pre-compiled binary + auto-download | Docker / containerd / single binary | Tie |
| GPU support | CUDA, Vulkan, Metal (llama.cpp) | CUDA, Vulkan, Metal, HIP, SYCL, multi-GPU, GPU auto-detection | **LocalAI** |
| Model config | Hardcoded in Rust settings.rs | **Declarative YAML model definitions** (gallery system, 78+ models) | **LocalAI** |
| Project scope | Desktop app focused on conversation | Universal AI server (web, API, CLI, WebRTC) | Different niches |

**S2B2S recommendation:** Do NOT replace llama.cpp with LocalAI — that would add a Go+containerd dependency to a Tauri desktop app. Instead:
1. **Harvest the YAML model config pattern** — S2B2S should adopt declarative backend+model definitions
2. **Support LocalAI as a Brain provider** — add `base_url: http://localhost:8080/v1` as a provider option
3. **Harvest the GPU auto-detection logic** — adapt for S2B2S's llama.cpp manager
4. **Consider LocalAI's Qwen3-TTS backend** — 14 TTS backends is a richer set than S2B2S's 8

---

## 5. User Interface & Experience

### 5.1 Overlay Systems

| Feature | Handy | Parler | AIVORelay | Parrot | CopySpeak | S2B2S |
|---------|-------|--------|-----------|--------|-----------|-------|
| Recording overlay | Yes (pill) | Yes (improved) | Yes + Live Preview | Speaking overlay | HUD with waveform | Recording pill + Speaking HUD |
| Multi-monitor | Basic | scale-factor fix | Follow mouse | Basic | No | Yes (get_monitor_with_cursor) |
| Overlay positions | Top/Bottom/None | Same + focused display | Fixed corner + follow | Top/Bottom | Floating | Configurable |
| Live waveform | No | No | No | Live spoken text | Waveform | Mic visualizer |
| Click-through | No | No | No | No | No | No (planned) |
| NSPanel (macOS) | Yes | Yes | No (Win only) | Yes | No (Win only) | Yes |
| GTK layer-shell | Yes | Yes | No (Win only) | No | No (Win only) | Yes |

### 5.2 Hotkey Systems

| Feature | Best Implementation |
|---------|---------------------|
| Dual shortcut engine | Handy (tauri + handy-keys/rdev) |
| Per-profile hotkeys | AIVORelay (dedicated keys per profile) |
| Pause/resume binding | Parler (F6) |
| Hotkey guide / conflict detector | AIVORelay |
| "Click to set" for unbound | Parler |
| Bindings backfill on settings read | **Parler** — CRITICAL pattern for any new hotkey |

### 5.3 Frontend Architecture

| Aspect | Handy family | CopySpeak | whispering | Best Pattern |
|--------|-------------|-----------|------------|--------------|
| Framework | React 18 + TS | Svelte 5 runes + SvelteKit | Svelte | React (S2B2S is React) |
| State management | Zustand + Immer | Svelte runes stores | $state/$derived | Zustand (already used) |
| Styling | Tailwind CSS 4 | Tailwind 4 + shadcn-svelte | Tailwind | Tailwind (already used) |
| Typed IPC | tauri-specta | Manual (tauri::command) | Manual (tauri::command) | **tauri-specta** (S2B2S uses) |
| i18n | 20 locales (i18next) | EN/ES only | Unknown | **Handy** (20 locales) |
| Onboarding | Yes (permission checks) | No | No | **Handy** |

---

## 6. Settings & Configuration Systems

### 6.1 Settings Architecture

| Feature | Best Implementation | File |
|---------|---------------------|------|
| Serde-default pattern for safe schema evolution | Handy | settings.rs (~1400 lines) |
| Bindings backfill on read (new defaults → stored) | **Parler** | settings.rs lines 943-953 |
| Settings export/import (JSON dump/restore) | **Parler** | ExportImportSettings.tsx |
| Dev flavor config (side-by-side dev/stable) | **Parler** | tauri.dev.conf.json |
| Capability-gated settings (grey out unsupported) | S2B2S (planned) | overlay_fx/capabilities.rs |
| Declarative ModelConfig registry | **voicebox** | backend config |
| Profile system (per-mode settings) | **AIVORelay** | TranscriptionProfile |

### 6.2 Settings S2B2S Should Add
1. **Bindings backfill** (Parler) — every new setting gets a default, merged on read
2. **Export/Import** (Parler) — trivial, loved by users
3. **Dev flavor** (Parler) — run S2B2S-dev next to stable
4. **Profiles** (AIVORelay) — per-context brain/voice/language presets

---

## 7. Platform & Performance

### 7.1 Platform Support Matrix

| Project | Win | Mac | Linux | Wayland | Mobile |
|---------|-----|-----|-------|---------|--------|
| Handy | Yes | Yes | Yes | Yes (layer-shell) | No |
| Parler | Yes | Yes (primary) | Yes | Yes | No |
| AIVORelay | **Yes (only)** | No | No | No | No |
| Parrot | Yes | Yes | Yes | No | No |
| CopySpeak | **Yes (only)** | No | No | No | No |
| voicebox | Yes | Yes | Yes | Partial | No |
| S2B2S | Yes | Yes | Yes | **Yes (layer-shell)** | No |

### 7.2 Performance Techniques

| Technique | Source | Value |
|-----------|--------|-------|
| **shorten_first_chunk** (first sentence plays 150 chars, not 800) | Parrot | Time-to-first-audio down 75% |
| **Telemetry-driven pagination** (EMA chars/ms, adaptive budget) | CopySpeak | Honest progress bars + optimal chunk sizes |
| **Pre-warm + hidden synthesis** (Piper JIT warm-up before first use) | CopySpeak | Kills 1.6s first-request penalty |
| **Thread pool auto-tuning** (infer_kokoro_tuning_for_cpu_count) | Parrot | Scales across user hardware |
| **Zero-gap playback** (pre-decode next fragment during current playback) | CopySpeak | Inaudible chunk transitions |
| **On-demand render loop** (0 frames hidden, idle-sleep after 2 still frames) | CursorFX + TD_Web_Trail | 0% CPU at rest |
| **WNCHITTEST→HTTRANSPARENT re-asserted every frame** | CursorFX | Click-through survives Z-order thieves |
| **Model unload timeout** (lazy load, auto-unload after N s) | Handy (pattern reused by all forks) | RAM efficiency across the family |

### 7.3 Windows-Specific Techniques (CopySpeak + CursorFX + AIVORelay)

| Technique | Project | Detail |
|-----------|---------|--------|
| 200ms near-silent preroll | CopySpeak | Works around output-device wake-up clipping |
| Vulkan, NOT DX12 for transparent overlay | CursorFX | DX12 OOMs (RTX 4070); Vulkan + NVAPI Prefer Native fix |
| WASAPI loopback capture | AIVORelay | System audio with "Both" mic+loopback mixing |
| AddClipboardFormatListener thread | CopySpeak | Zero-polling clipboard detection |
| Win32 WndProc subclass | CursorFX | WM_NCHITTEST, WM_SETCURSOR, WM_ERASEBKGND overrides |
| NVAPI DRS session | CursorFX | OGL_CPL_PREFER_DXPRESENT → Prefer Native (0x20324987=0) |
| thin-LTO fix | Parler | Full LTO caused transcription crash on Windows |

---

## 8. Security, Privacy & Reliability

### 8.1 Credential Management

| Technique | Source | Quality |
|-----------|--------|---------|
| AES-GCM encrypted API keys | AIVORelay (secure_keys.rs) | **Best** — per-key encryption at rest |
| Control token (non-crypto, non-constant compare) | CopySpeak | **Weak** — needs upgrade |
| ECDH P-256 + HKDF + AES-GCM channel | AIVORelay (connector.rs) | **Best** — local bridge crypto |
| Credential Manager / Keychain / keyring | S2B2S | Platform-native per OS |

### 8.2 Reliability Patterns

| Pattern | Source |
|---------|--------|
| lock_or_recover! macro (mutex poison recovery) | CopySpeak |
| Crash logging (panic capture to file) | Parler |
| Recording auto-stop watchdog (silence timeout) | AIVORelay |
| Microphone auto-switch (follows default device change) | AIVORelay |
| Recording session manager (serialization via mpsc) | AIVORelay |

---

## 9. The CursorFX + TD_Web_Trail Overlay Future

These two projects are critical for S2B2S's planned overlay/avatar mode.

### 9.1 What CursorFX Provides (Proven, Cross-Platform)

| Component | File | Lines | What it does |
|-----------|------|-------|--------------|
| Transparent overlay window | overlay/mod.rs | ~500 | Tauri window + raw handle → wgpu Surface |
| Windows click-through | overlay/mod.rs (platform) | ~200 | WndProc subclass, WS_EX styles, per-frame guard |
| NVAPI fix | overlay/mod.rs (platform) | ~60 | nvapi64.dll DRS session, Prefer Native preset |
| Ribbon pipeline | overlay/renderer.rs | ~400 | Trail rendering with Catmull-Rom interpolation |
| SDF circle pipeline | overlay/renderer.rs | ~400 | Particles, ripples, satellites via fragment shader |
| WGSL shaders | overlay/shader.wgsl | ~120 | HSL conversion, SDF circles, ribbon fragment |
| On-demand render | overlay/renderer.rs | ~100 | Idle-sleep, frame pacing, surface recreation |
| Config panel | React components | ~400 | RON-based config with live Tauri IPC |

### 9.2 What TD_Web_Trail Provides (Physics + Aesthetic)

| Component | File | Lines | What it does |
|-----------|------|-------|--------------|
| Spring-friction chain | index.html (script) | ~200 | N-point chain chasing cursor with damped springs |
| Distance constraint solver | index.html (script) | ~80 | Keeps chain from stretching, "rope/skeleton" feel |
| 4-pass tapered glow | index.html (script) | ~150 | Bloom aura → body → dark mask → bright core |
| Catmull-Rom / Bézier splines | index.html (script) | ~100 | Upsampling physics points into smooth curves |
| Binary streaming | relay.js | ~80 | 8-byte LE coordinate frames, sub-millisecond serialization |
| Bun.js relay | relay.js | ~60 | Zero-dependency WebSocket relay to TouchDesigner |
| Performance discipline | index.html (script) | ~50 | Desync canvas, idle sleep after 2 still frames, 0% CPU |

### 9.3 Synthesis: What S2B2S Builds From Both

```
CursorFX (overlay window + wgpu)  +  TD_Web_Trail (physics + glow)
              │                                    │
              └──────────────┬─────────────────────┘
                             ▼
              S2B2S "Brain Overlay" (Conversation 2.0)
              ├── Transparent, click-through, always-on-top window
              ├── 3D avatar (Three.js in webview) ← HerLoading DNA
              ├── Cursor→avatar tether (spring-friction + Catmull-Rom)
              ├── Neon glow (4-pass taper + HSL drift)
              ├── Reply bubble (streaming text, markdown, metrics)
              └── Quick actions (Insert, Copy, Regenerate, Screenshot)
```

---

## 10. Feature Harvest Priority Matrix

### TIER 1: Must Copy (High Value / Low Effort)

| # | Feature | From | Effort | Why |
|---|---------|------|--------|-----|
| 1 | **Bindings backfill on read** | Parler (settings.rs:943) | XS | Prevents undefined binding bugs |
| 2 | **Settings export/import** | Parler | XS | Trivial, high user satisfaction |
| 3 | **Crash logging** | Parler (crash_logging.rs, 80l) | XS | Essential for debugging |
| 4 | **Pause/resume recording** | Parler (+89 lines recorder.rs) | S | Immediate conversation ergonomics |
| 5 | **shorten_first_chunk TTS** | Parrot (managers/tts.rs:800) | S | Time-to-first-audio down 75% |
| 6 | **Crossfade blending** | Parrot (managers/tts.rs:600) | S | Eliminates chunk-join seams |
| 7 | **TTS pre-warm at startup** | CopySpeak (piper_server.rs) | S | Kills 1.6s first-request latency |

### TIER 2: Should Copy (High Value / Medium Effort)

| # | Feature | From | Effort | Why |
|---|---------|------|--------|-----|
| 8 | **Transcription Profiles** | AIVORelay (TranscriptionProfile) | M | Per-context modes (casual/coding/translate) |
| 9 | **Long-audio model switching** | Parler (transcription.rs:465) | M | Short→Moonshine, long→Whisper Turbo |
| 10 | **Telemetry-driven pagination** | CopySpeak (telemetry.rs, 370l) | M | Honest progress bars + adaptive chunking |
| 11 | **Audio cache (history-based)** | CopySpeak (history.rs) | M | Instant replay without synthesis |
| 12 | **Prompt variables** ($output, ${current_app}) | AIVORelay | M | Context-aware Brain prompts |
| 13 | **Selection capture (AX API + sentinel)** | Parrot (actions.rs:298) | M | Speak selection without clipboard loss |
| 14 | **MCP server** | voicebox | M | Per-client voice binding, bidirectional |
| 15 | **YAML model configs** | LocalAI (gallery system) | M | Declarative backend+model definitions, replace hardcoded Rust structs |
| 16 | **LocalAI as Brain provider** | LocalAI | M | Add `base_url` option to talk to LocalAI instead of managing llama.cpp |

### TIER 3: Nice to Have (High Value / Large Effort)

| # | Feature | From | Effort | Why |
|---|---------|------|--------|-----|
| 17 | **Streaming cloud STT (WS)** | AIVORelay (3 WS managers) | L | Real-time cloud transcription |
| 18 | **System audio capture** | AIVORelay (WASAPI loopback) | L | Hear podcasts/meetings |
| 19 | **Voice cloning (RVC)** | voicebox | L | Clone any voice from WAV |
| 20 | **RNNoise denoise** | AIVORelay (nnnoiseless) | M | Cleaner STT input |
| 21 | **Voice commands (gated)** | AIVORelay (voice_command.rs) | L | S2B2S takes action |
| 22 | **File→SRT/VTT + diarization** | AIVORelay | L | Batch utility mode |

### TIER 4: Overlay Future (Phased)

| # | Feature | From | Phase | Why |
|---|---------|------|-------|-----|
| 23 | **wgpu transparent overlay** | CursorFX | Phase 4 (Track B) | GPU trail + tether |
| 24 | **Catmull-Rom tether** | TD_Web_Trail + CursorFX | Phase 4 | Cursor→avatar physics chain |
| 25 | **4-pass neon glow** | TD_Web_Trail | Phase 4 | Cyberpunk aesthetic |
| 24 | **Screen capture** (planned in futuristic_analysis) | AIVORelay (region_capture) | Phase 3 | Vision pillar |

---

## 11. Project-by-Project One-Line Verdicts

| Project | Verdict |
|---------|---------|
| **Handy** | The skeleton. Every fork inherits its Tauri shell, shortcuts, VAD, STT, model manager, settings, i18n, tray. S2B2S owes its existence to this codebase. |
| **Parler** | The polish fork. Pause/resume, long-audio routing, settings backfill, export/import, crash logging, multi-monitor overlay, bindings backfill — all small, all portable. |
| **AIVORelay** | The idea quarry. Streaming STT, system audio, profiles, AI Replace, encrypted connector, prompt variables, RNNoise, voice commands. Windows-only but conceptually universal. |
| **Parrot** | The TTS proof. Proves Handy's skeleton carries a TTS subsystem. Kokoro in-process, shorten_first_chunk, crossfade, AX-API selection capture. The single most important reference for S2B2S's TTS leg. |
| **CopySpeak** | The engine design donor. TtsBackend trait, persistent Piper server, telemetry, pre-warm, audio caching. S2B2S's TTS architecture is CopySpeak's design. |
| **voicebox** | The connector. MCP server, voice cloning, multi-engine Python backend + Rust Tauri frontend. Split-process architecture. |
| **vox** | The Qwen3 reference. Deep vendor integration of a 2208-line TTS model crate with Raspberry Pi optimizations. |
| **vibevoice-rs** | The monorepo reference. 5-crate pure-Rust TTS workspace, Candle custom fork, SSE streaming, voice cloning via safetensors. |
| **TTS-Audio-Suite** | The engine zoo. 13+ engines, streaming coordinator, viseme/facial analysis, RVC integration, declarative EngineCapabilities. |
| **voirs** | The full-spectrum framework. 16-crate Rust workspace covering G2P, acoustic, vocoder, cloning, conversion, emotion, singing, spatial, ASR. Not an app — a builder's toolkit. |
| **pocket-tts-server** | The simplest TTS server. Monolithic FastAPI, SSE streaming, voice cloning from WAV. S2B2S integrated it via pocket_server.py. |
| **LocalAI** | The AI Swiss Army knife. Go-based OpenAI-compatible server, 36+ backends (LLM/STT/TTS/Vision/Image), 78+ gallery models, YAML config. Harvest the model config pattern and add as a provider option — don't replace llama.cpp with it. |
| **whispering** | The provider matrix pattern. Svelte-based STT app with `as const satisfies` provider registry. AGPL — concept donor only. |
| **TranscriptionSuite** | The engineering marvel. GPL Electron app with whisper.cpp Vulkan sidecar, persist-before-deliver, watch-folder, diarization review, massive in-app updater. Concept donor only. |
| **Parakeet-RT** | The endpointing reference. Three-trigger VAD policy (pause/max-duration/silence-flush), producer/consumer threading, duplicate filter. 1,931 lines — read in one sitting. |
| **transcribe-rs** | S2B2S's STT engine. ONNX-based, 7 model families, per-platform GPU backends. S2B2S uses it suboptimally (load+drop per call, not persistent). |
| **sherpa-onnx** | The Swiss Army knife. 26+ model families, 7 OSes, 4 NPU stacks, 12 language bindings. Honest assessment: too broad for S2B2S to adopt, but learn from the design. |
| **onnx-asr** | The ONNX reference. 15+ models, 13 API endpoints, TensorRT up to 1,500x speedup. Python-only — good for understanding, not for Rust integration. |
| **speech-recognition** | The browser-ASR reference. WASM/WebGPU/WebNN/WebGL backends (but WASM+WebGPU are stubs). TypeScript-first runtime, 5-stage progressive pipeline. |
| **speechbrain** | The research framework. 70K+ lines of PyTorch, 44 recipe datasets, 17 inference interfaces, hyperpyyaml DSL. Too heavy for S2B2S but the definitive speech ML reference. |
| **CursorFX** | The overlay proof. Tauri + wgpu transparent window, Vulkan + NVAPI fix, per-frame click-through guard. The blueprint for S2B2S's Track B native overlay. |
| **TD_Web_Trail** | The aesthetic reference. Spring-friction physics, 4-pass neon glow, Catmull-Rom/Bézier, idle-sleep perf. The tether connecting the cursor to the avatar. |
| **S2B2S** | The synthesis. Handy (skeleton) + Parler (features) + Parrot (TTS) + CopySpeak (engine design) = the only project with streaming Brain, conversation loop, barge-in, and full STT+TTS+LLM integration. |

---

## 12. Final Synthesis: What Makes S2B2S Unique

S2B2S is **not** the best at any single subsystem:
- AIVORelay has more STT features
- Parrot has better TTS latency
- CopySpeak has better TTS engine design
- voicebox has more cloud voice options
- LocalAI has more LLM/STT/TTS/Vision backends (36+) and YAML model configs
- CursorFX has proven GPU overlay

**But S2B2S is the only project that combines ALL of them:**
- STT from Handy family (8 local engines + cloud)
- TTS from CopySpeak design + Parrot patterns (Kokoro, Piper, OpenAI, ElevenLabs, Cartesia, SAPI, Kitten, Pocket)
- **Streaming Brain** (no other project has this — SSE streaming + multi-turn context + local llama.cpp + barge-in)
- **Conversation loop** (VAD → STT → Brain → TTS → barge-in — no other project has the full loop)
- **Cross-platform** (Windows + macOS + Linux + Wayland honesty — most others are single-OS)
- **20 i18n locales with RTL** (Handy's foundation)
- Planned: GPU transparent overlay + 3D avatar + screen vision (CursorFX + TD_Web_Trail synthesis)

### The Architecture S2B2S Should Grow Toward

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           S2B2S FUTURE                                  │
│                                                                         │
│  Handy skeleton ──────► Cross-platform Tauri shell, shortcuts, i18n    │
│  Parlet features ────► Pause, long-audio, settings backfill, crash log │
│  Parrot TTS ─────────► Shorten-first-chunk, crossfade, AX-API capture │
│  CopySpeak design ───► TtsBackend trait, persistent server, telemetry  │
│  AIVORelay concepts ─► Profiles, prompt vars, streaming cloud STT     │
│  LocalAI concepts ────► YAML model configs, provider option, GPU detect │
│  CursorFX overlay ───► GPU transparent window, Vulkan, NVAPI fix       │
│  TD_Web_Trail ───────► Spring-friction tether, 4-pass neon glow        │
│  voicebox ───────────► MCP server, voice cloning                       │
│                                                                         │
│  S2B2S UNIQUE:                                                          │
│  ├── TripleVAD (RMS + RNNoise + Silero)                                │
│  ├── Streaming Brain (SSE + multi-turn + barge-in)                     │
│  ├── Conversation loop (continuous voice mode)                         │
│  ├── Local llama.cpp server (auto-download, GPU offload)                │
│  ├── Speak-before-finish (TTS starts on first sentence)                │
│  ├── 20-locale RTL i18n                                                │
│  └── Planned: 3D avatar + screen vision + GPU overlay                  │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## 13. Next Steps (Actionable)

1. **Immediately port from Parler:** bindings backfill, crash logging, export/import (Phase 0, <1 day)
2. **Immediately port from Parrot:** shorten_first_chunk, crossfade (Phase 0, ~2 days)
3. **Design profiles system** based on AIVORelay's TranscriptionProfile (Phase 1, ~1 week)
4. **Adopt telemetry from CopySpeak** for honest TTS progress (Phase 1, ~3 days)
5. **Vendor CursorFX** into overlay_fx/native/ behind feature flag (Phase 4, per futuristic_analysis)
6. **Port TD_Web_Trail physics** for cursor→avatar tether (Phase 4)
7. **Long-term:** MCP server (voicebox pattern), streaming cloud STT (AIVORelay patterns), voice cloning

---

*This comparative analysis is the synthesis of 22 individual project analyses totaling approximately 500,000 characters of documentation. Each analysis was produced by reading every source file in every project. See the individual `*_review.md` files for deep dives into each project.*
