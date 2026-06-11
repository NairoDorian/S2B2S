# S2B2S — Comprehensive Improvement Plan v3

> **Base:** S2B2S (Handy fork) · **Donors:** CopySpeak tts-perf-v2, Parrot, AIVORelay, Parler
> **New additions:** `FluidInference/text-processing-rs`, RealtimeSTT/TTS patterns, Pocket TTS
> **Last updated:** June 2026 — items marked ✅ are completed; 🚧 are in progress; 📋 are planned

---

## 0. Current State Audit

S2B2S already has a mature codebase — not just a plan, it works:

| Subsystem | Status | Needs |
|-----------|--------|-------|
| STT (Parakeet V3, Whisper, Moonshine) | ✅ | Streaming STT, long-audio routing |
| TTS Piper (persistent warm HTTP server) | ✅ | R-fix bug ledger |
| TTS Kokoro (in-process via tts-rs) | ✅ | Worker pool + crossfade + shorten-first-chunk |
| TTS cloud (OpenAI, ElevenLabs, Cartesia) | ✅ | Parallel fragment synthesis |
| TTS Kitten (skeleton via CLI) | ✅ | Full in-process integration |
| TTS SAPI (zero-download Windows fallback) | ✅ | Voice selection UI |
| `TtsBackend` trait with enum dispatch | ✅ | Add PocketTTS |
| `WarmEngine` trait (lifecycle states) | ✅ | Model unload timeout |
| Brain (streaming LLM, OpenAI-compat) | ✅ | Profiles, prompt variables |
| Conversation mode + sentence splitter | ✅ | Hands-free mode, barge-in tiers |
| Clipboard watch (double-copy) | ✅ | macOS/Linux polling fallback |
| HUD overlay | ✅ | Waveform from AmplitudeEnvelope |
| TripleVAD (RMS → RNNoise → Silero) | ✅ | Per-mode VAD/NS defaults |
| Tunable RNNoise voice prob threshold | ✅ | UI slider polish |
| Text sanitize pipeline | ✅ (ITN + TN + markdown) | Custom rules UI, sentence-level scanning |
| Crash logging (full backtraces) | ✅ | Sentry/telemetry opt-in |
| Debug mode UI toggle (Advanced settings) | ✅ | — |
| MSRV 1.87 declared | ✅ | — |
| History entries with TTS metadata | ✅ | Search/filter UI |
| Her-style 3D loading animation | ✅ | — |
| 20-language i18n | ✅ | — |

---

## 1. Text Processing Pipeline — ✅ IMPLEMENTED

**Status: COMPLETE.** ITN, TN, and markdown stripping are all live in `src-tauri/src/tts/sanitize/`. The pipeline order is: ITN (post-STT) → Custom Words → pulldown-cmark (pre-TTS) → TN → Regex Cleanup.

```
Post-STT:   ITN (text-processing-rs) → Custom Words (strsim/natural)
Pre-TTS:    pulldown-cmark → TN (text-processing-rs) → Regex Cleanup
```

### 1.1 Engine 1: ITN — Spoken → Written (Post-STT)
**Crate:** `text-processing-rs` (Apache 2.0). Converts ASR-style spoken text to proper written form before feeding the Brain.

| Input (from STT) | Output (to Brain) |
|---|---|
| `two hundred thirty two` | `232` |
| `five dollars and fifty cents` | `$5.50` |
| `january fifth twenty twenty five` | `January 5, 2025` |
| `quarter past two pm` | `02:15 p.m.` |
| `one point five billion dollars` | `$1.5 billion` |
| `seventy two degrees fahrenheit` | `72 °F` |

98.6% compatible with NVIDIA NeMo test suite (1200/1217 tests). Categories: cardinal, ordinal, decimal, money, measurements, dates, time, email/URL, telephone/IP, whitelist.

### 1.2 Engine 2: pulldown-cmark — Markdown → Speakable (Pre-TTS)
Walks the markdown AST and produces natural spoken text.

| Input | Output |
|---|---|
| `# Introduction` | `Introduction.` |
| `**Bold text** is here` | `Bold text is here` |
| `- item 1\n- item 2` | `item 1. item 2.` |
| `` `inline code` `` | `code: inline code` |
| Code blocks | Never spoken |
| `https://example.com/page` | `example dot com` |
| `&amp;` | `&` |

### 1.3 Engine 3: TN — Written → Spoken (Pre-TTS)
Converts written numbers/units/dates to spoken form before TTS.

| Input | Output (spoken by Piper/Kokoro) |
|---|---|
| `123` | `one hundred twenty three` |
| `$5.50` | `five dollars and fifty cents` |
| `January 5, 2025` | `january fifth twenty twenty five` |
| `2:30 PM` | `two thirty p m` |
| `1st place` | `first place` |
| `200 km/h` | `two hundred kilometers per hour` |
| `Dr. Smith` | `doctor Smith` |

### 1.4 Engine 4: Regex Cleanup (Kept as Fallback)
Final scrub pass in `sanitize/cleanup.rs` — removes artifacts, normalizes whitespace, handles edge cases.

---

## 2. RAM-Persistent Model Architecture

### 2.1 The Warm-Model Pattern

Every local engine follows the same lifecycle: `Stopped → Starting → WarmingUp → Ready → Error`

```rust
trait WarmEngine: TtsBackend {
    fn warm(&self) -> Result<(), String>;
    fn unload(&self) -> Result<(), String>;
    fn status(&self) -> EngineStatus;  // Loading | WarmingUp | Ready | Error | Stopped
    fn is_ready(&self) -> bool;
}
```

| Engine | RAM Cost | CUDA VRAM |
|--------|----------|-----------|
| Piper (persistent HTTP) | ~100-200 MB per voice | ~200 MB |
| Kokoro (tts-rs in-process) | ~115 MB + 50 MB per worker | ~115 MB |
| Kitten TTS (CLI skeleton) | ~25-200 MB | ~200 MB |
| Cloud engines (pooled reqwest) | ~0 MB | 0 |
| SAPI (OS always available) | ~0 MB | 0 |

### 2.2 Engine Pool (from Parrot)

```rust
struct KokoroPool {
    workers: Vec<KokoroEngine>,
    available: Semaphore,      // permits = tts_workers count
    generation: AtomicU64,     // bump on config change, stale workers abort
}
```

Workers auto-tuned from CPU count (`tts_workers` setting, default: `max(2, num_cpus / 2)`).

### 2.3 Pre-Warm at Startup

Warm-up synthesis sends a hidden sentence to force ONNX Runtime JIT/GPU kernel compilation. Without this, the first user request pays a 1-6 second penalty.

### 2.4 Model Unload Policy

- **On engine switch:** Unload previous model immediately
- **On idle:** Configurable timeout (Never / 5min / 15min / 30min / OnAppExit)
- **On CUDA toggle:** Kill Piper server, respawn with CUDA EP
- **Manual:** Tray → "Unload Model" action

---

## 3. Kokoro Comparison & Best Method

| Aspect | Parrot's Kokoro | CopySpeak's Kokoro | S2B2S Current | Winner |
|--------|----------------|-------------------|---------------|--------|
| Integration | `tts-rs` in-process Rust | CLI subprocess | `tts-rs` in-process | **S2B2S** |
| Engine pool | N workers, parallel | Single process | Worker pool setting | 🚧 In progress |
| Crossfade | 10ms @ 24kHz | None | In progress | 🚧 In progress |
| Shorten-first-chunk | Clause-split | None | Default ON | **S2B2S** |
| Voice selection | Auto per-language, 54 voices | Manual | 54 voices registered | **S2B2S** |
| Text normalization | pulldown-cmark | Regex | 4-pass pipeline | **S2B2S** |
| Warm persistence | Model unload timeout | No | WarmEngine + pre-warm | **S2B2S** |

**Bottom line:** S2B2S aggressively leads. ITN/TN/markdown pipeline is superior. Worker pool + crossfade are the remaining pieces.

---

## 4. RNNoise — Current Implementation Audit

### Status: ALREADY IMPLEMENTED & FIXED

All components wired and working:
- `NoiseSuppressor` struct (16kHz→48kHz upsample, denoise, downsample)
- `TripleVad` (RMS gate → RNNoise voice prob → Silero)
- Settings toggle + VAD mode selector + RNNoise threshold slider (0.05–0.9)
- Frontend UI in AudioEnhancements

### Pipeline (TripleVad mode)

```
Microphone (16kHz, 480-sample frames)
  │
  ▼
Stage 1: RMS Energy Gate (0.002 threshold)
  │  Rejects absolute silence
  ▼
Stage 2: RNNoise Voice Probability (configurable threshold 0.2 default)
  │  Passes denoised OR raw audio to Silero
  ▼
Stage 3: Silero VAD Confirmation (0.3 threshold)
  │  Final arbiter — confirms speech
  ▼
VAD output: Speech / Noise
```

---

## 5. Phase Execution Plan

### Phase 0 — Foundation ✅ COMPLETE
| Task | Status |
|------|--------|
| Settings-bindings backfill on read | ✅ |
| Crash logging | ✅ |
| Settings export/import | ✅ |
| Dev flavor Tauri config | ✅ |
| Declare MSRV 1.87 | ✅ |
| Add `lock_or_recover!` macro | ✅ |
| Add `text-processing-rs` | ✅ |

### Phase 0.5 — RNNoise Wiring Fix ✅ PARTIALLY COMPLETE
| Task | Status |
|------|--------|
| Default TripleVad for all modes | ✅ |
| Tunable RNNoise voice prob threshold | ✅ |
| NS voice prob passthrough to VAD | 📋 |
| NS active indicator in overlay | 📋 |
| Deduplicate NS instances | 📋 |
| Per-mode VAD/NS defaults | 📋 |

### Phase 1 — Text Pipeline ✅ COMPLETE
| Task | Status |
|------|--------|
| ITN wrapper | ✅ |
| TN wrapper | ✅ |
| Port pulldown-cmark (from Parrot) | ✅ |
| Pipeline orchestrator | ✅ |
| Regex cleanup keep/upgrade | ✅ |
| Sentence-level scanning | 📋 |
| Custom rules UI | 📋 |
| Test suite | 🚧 |

### Phase 2 — Kokoro Worker Pool 🚧 IN PROGRESS
| Task | Status |
|------|--------|
| Engine pool with semaphore | 🚧 |
| Crossfade 10ms @ 24kHz | 🚧 |
| Shorten-first-chunk + clause split | ✅ |
| Parallel chunk synthesis | 🚧 |
| CPU auto-tune (tts_workers) | ✅ |
| Voice-per-language auto-select | ✅ |

### Phase 3 — RAM Persistence 🚧 PARTIALLY COMPLETE
| Task | Status |
|------|--------|
| WarmEngine trait | ✅ |
| Kokoro pre-warm at startup | ✅ |
| Piper warm-up synthesis | ✅ |
| Engine-switch unload (R5 fix) | 🚧 |
| Model unload timeout | 📋 |
| CUDA auto-discovery | ✅ |
| Footer status indicator | ✅ |

### Phase 4 — New Engines 🚧 PARTIALLY COMPLETE
| Task | Status |
|------|--------|
| Kitten TTS backend (CLI → TtsBackend) | ✅ (skeleton) |
| Pocket TTS backend | 📋 |
| SAPI fallback (zero-download) | ✅ |
| Parallel cloud synthesis | 📋 |

### Phase 5 — CopySpeak Feature Parity 📋 PLANNED
| Task | Status |
|------|--------|
| Telemetry-driven adaptive pagination | ✅ |
| Audio cache (hash-keyed) | 📋 |
| Save-to-MP3/OGG/FLAC | 📋 |
| Effects (WalkieTalkie/GameBoy) | 📋 |
| HUD AmplitudeEnvelope waveform | 📋 |
| Control HTTP API (axum) | 📋 |
| Agent harness (Python CLI + SKILL) | 📋 |

### Phase 6 — Streaming STT + Profiles 📋 PLANNED
| Task | Status |
|------|--------|
| OpenAI Realtime WS STT | 📋 |
| Deepgram WS STT | 📋 |
| MoonshineStreaming integration | 📋 |
| Profiles (per-mode config) | 📋 |
| AI Replace Selection | 📋 |
| Prompt variables | 📋 |
| Long-audio model routing | 📋 |

---

## 6. Dependency Matrix

### Rust Crates
| Crate | Version | Purpose | Phase |
|-------|---------|---------|-------|
| `text-processing-rs` | 0.2.2 | ITN + TN normalization | P1 ✅ |
| `pulldown-cmark` | 0.13 | Markdown → speakable text | P1 ✅ |
| `nnnoiseless` | 0.5.2 | RNNoise suppression | P0.5 ✅ |
| `tts-rs` (kokoro) | — | Kokoro in-process TTS | P2 ✅ |
| `rodio` | 0.22 | Audio playback | ✅ |
| `rubato` | 3.0 | Audio resampling | ✅ |

### Python Sidecars (for CLI-based engines)
| Tool | Purpose |
|------|---------|
| piper_http.server | Piper TTS (persistent) |
| kokoro | Via tts-rs (in-process, no sidecar) |
| kittentts-cli.py | Kitten TTS CLI (skeleton) |
| pocket-tts | Pocket TTS (planned) |

---

## 7. Success Criteria

- **Dictation:** STT → ITN → Brain → TN → TTS produces correct numbers/dates/currency at every stage
- **Read-aloud:** selection → first audio < 700ms warm (Piper/Kokoro)
- **Conversation:** ≤ 1.5s end-of-speech → first audible reply (local 8B + warm Kokoro pool)
- **Text pipeline:** 1217 ITN tests + markdown test suite + regex cleanup tests all pass
- **Stability:** 500-synthesis soak with flat latency
- **RAM:** Idle < 500 MB with Kokoro pool warm, < 250 MB with models unloaded
