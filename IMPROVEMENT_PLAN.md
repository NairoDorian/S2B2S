# S2B2S — Comprehensive Improvement Plan v2

> **Base:** S2B2S (Handy fork) · **Donors:** CopySpeak tts-perf-v2, Parrot, AIVORelay, Parler
> **New additions:** `FluidInference/text-processing-rs`, RealtimeSTT/TTS patterns, Pocket TTS

---

## 0. Current State Audit

S2B2S already has a mature codebase — not just a plan, it works:

| Subsystem | Status | Needs |
|-----------|--------|-------|
| STT (Parakeet V3, Whisper, Moonshine) | ✅ | Streaming STT, long-audio routing |
| TTS Piper (persistent warm HTTP server) | ✅ | R-fix bug ledger, warm-up synthesis |
| TTS Kokoro (in-process via tts-rs) | ✅ | Worker pool + crossfade + shorten-first-chunk |
| TTS cloud (OpenAI, ElevenLabs, Cartesia) | ✅ | Parallel fragment synthesis |
| `TtsBackend` trait with enum dispatch | ✅ | Add Kitten, PocketTTS, SAPI |
| Brain (streaming LLM, OpenAI-compat) | ✅ | Profiles, prompt variables |
| Conversation mode + sentence splitter | ✅ | Hands-free mode, barge-in tiers |
| Clipboard watch (double-copy) | ✅ | macOS/Linux polling fallback |
| HUD overlay | ✅ | Waveform from AmplitudeEnvelope |
| Text sanitize pipeline | ⚠️ Regex-only | **↓ Major upgrade below ↓** |

---

## 1. Text Processing Pipeline — THE BIG UPGRADE

This is the single highest-impact improvement. Currently S2B2S uses regex-based sanitization. We replace it with a **3-engine multi-pass pipeline**:

```
                      ┌───────────────── POST-STT ─────────────────┐
                      │                                             │
  STT OUTPUT ──────► ITN ──► Custom Words ──► Sentence Normalize ──► Brain
                 (text-processing-rs)  (strsim/natural)  (capitalize, punctuation)

                      ┌───────────────── PRE-TTS ──────────────────┐
                      │                                             │
  BRAIN/LLM OUTPUT ──► pulldown-cmark ──► TN ──► Cleanup ──► Kokoro/Piper
                      (Parrot port)  (text-processing-rs)  (regex fallback)
```

### 1.1 Engine 1: ITN — Spoken → Written (Post-STT)
**Crate:** `text-processing-rs` (Apache 2.0, single dep: `lazy_static`)

Converts ASR-style spoken text to proper written form before feeding the Brain:

| Input (from STT) | Output (to Brain) |
|---|---|
| `two hundred thirty two` | `232` |
| `five dollars and fifty cents` | `$5.50` |
| `january fifth twenty twenty five` | `January 5, 2025` |
| `quarter past two pm` | `02:15 p.m.` |
| `one point five billion dollars` | `$1.5 billion` |
| `seventy two degrees fahrenheit` | `72 °F` |

98.6% compatible with NVIDIA NeMo test suite (1200/1217 tests). Categories: cardinal, ordinal, decimal, money, measurements, dates, time, email/URL, telephone/IP, whitelist.

**Config toggle:** `Settings → Dictation → "Normalize spoken numbers (ITN)"` — on by default.

### 1.2 Engine 2: pulldown-cmark — Markdown → Speakable (Pre-TTS)
**Port from:** Parrot's `text_normalization.rs` (617 lines, MIT)

Walks the markdown AST and produces natural spoken text:

| Input | Output |
|---|---|
| `# Introduction` | `Introduction.` |
| `**Bold text** is here` | `Bold text is here` |
| `- item 1\n- item 2` | `item 1. item 2.` |
| `[Link](https://...)` | `Link` (anchors read, URLs stripped) |
| `` `inline code` `` | `code: inline code` |
| ```` ```rust\nfn main(){}\n``` ```` | `(code block shown on screen)` — never spoken |
| `https://example.com/page` | `example dot com` (host-only reading) |
| HTML entities `&amp;` | `&` |

**Config toggle:** `Settings → Voice → "Strip markdown before speaking"` — on by default.

### 1.3 Engine 3: TN — Written → Spoken (Pre-TTS)
**Crate:** `text-processing-rs` (same crate, different mode)

Converts written numbers/units/dates to spoken form before TTS:

| Input | Output (spoken by Piper/Kokoro) |
|---|---|
| `123` | `one hundred twenty three` |
| `$5.50` | `five dollars and fifty cents` |
| `January 5, 2025` | `january fifth twenty twenty five` |
| `2:30 PM` | `two thirty p m` |
| `1st place` | `first place` |
| `200 km/h` | `two hundred kilometers per hour` |
| `test@gmail.com` | `t e s t at g m a i l dot c o m` |
| `Dr. Smith` | `doctor Smith` |

### 1.4 Engine 4: Regex Cleanup (Kept as Fallback)
**Current:** S2B2S `sanitize/cleanup.rs` — regex-based artifact removal.

Kept as the final scrub pass. Options:
- Sentence-level normalizer (`normalize_sentence`) scans within larger text
- Custom rules via `addRule(...)` API for domain-specific terms (e.g., "GPT" → keep as-is)
- Toggleable per-rule: some users want raw numbers spoken, others want them expanded

### 1.5 Pipeline Order & Toggles

```rust
// In settings.rs
pub struct SanitizeConfig {
    pub itn_enabled: bool,           // spoken → written after STT
    pub markdown_strip: bool,        // pulldown-cmark before TTS
    pub tn_enabled: bool,            // written → spoken before TTS
    pub regex_cleanup: bool,         // final artifact scrub
    pub custom_replacements: HashMap<String, String>, // user-defined rules
}
```

**Architecture:**
```
src-tauri/src/tts/sanitize/
├── mod.rs                 # Pipeline orchestrator (runs passes in order)
├── itn.rs                 # text-processing-rs ITN wrapper
├── tn.rs                  # text-processing-rs TN wrapper
├── markdown.rs            # pulldown-cmark port from Parrot
├── cleanup.rs             # existing regex scrub (kept)
└── tests/
    ├── itn_tests.rs       # 1217 NeMo compatibility tests
    ├── tn_tests.rs        # numeric → spoken regression
    ├── markdown_tests.rs  # Parrot's test suite
    └── pipeline_tests.rs  # integration: full multi-pass
```

---

## 2. RAM-Persistent Model Architecture

### 2.1 The Warm-Model Pattern (generalized from CopySpeak)

Every local engine follows the same lifecycle:

```
Stopped → Starting → WarmingUp → Ready
   ↑         ↑           ↓
   └─────────┴─────── Error
```

```rust
trait WarmEngine: TtsBackend {
    fn warm(&self) -> Result<(), String>;       // load model, run warm-up inference
    fn unload(&self) -> Result<(), String>;     // free RAM/VRAM
    fn status(&self) -> EngineStatus;           // Loading | WarmingUp | Ready | Error | Stopped
    fn is_ready(&self) -> bool;
}
```

| Engine | How It Stays Warm | RAM Cost | CUDA VRAM |
|--------|-------------------|----------|-----------|
| **Piper** | Persistent Python `piper.http_server` + warm-up synthesis at startup | ~100-200 MB per voice | ~200 MB (ONNX CUDA EP) |
| **Kokoro** | Worker pool (N × ONNX session), lazy-load + `model_unload_timeout` | ~115 MB + 50 MB per worker | ~115 MB (ONNX, single session shared) |
| **Kitten TTS** | Python CLI kept resident + warm-up request | ~25-200 MB | ~200 MB (ONNX) |
| **Pocket TTS** | Python subprocess, warm-up inference | ~500 MB (PyTorch CPU) | ~1 GB (PyTorch CUDA fork) |
| **Cloud engines** | Pooled `reqwest::Client` (keepalive 60s, ≤2 idle/host) | ~0 MB | 0 |
| **SAPI** | Always available via OS | ~0 MB | 0 |

### 2.2 Engine Pool (from Parrot)

```rust
struct KokoroPool {
    workers: Vec<KokoroEngine>,
    available: Semaphore,      // permits = tts_workers count
    generation: AtomicU64,     // bump on config change, stale workers abort
}

impl KokoroPool {
    fn take_engine(&self) -> PoolGuard;     // blocks if all busy
    fn return_engine(&self, engine);         // release semaphore
    fn warm_all(&self);                      // startup warm-up
    fn unload_all(&self);                    // free all RAM
}
```

Workers auto-tuned from CPU count (`tts_workers` setting, default: `max(2, num_cpus / 2)`).

### 2.3 Pre-Warm at Startup

```rust
// In lib.rs:run(), after settings loaded:
tokio::spawn(async {
    match settings.tts.engine {
        TtsEngine::Piper => { piper::prewarm(); }
        TtsEngine::Kokoro => { kokoro_pool.warm_all(); }
        TtsEngine::Kitten => { kitten::prewarm(); }
        TtsEngine::PocketTts => { pocket_tts::prewarm(); }
        _ => {} // cloud engines — no prewarm
    }
});
```

Warm-up synthesis sends a hidden sentence ("This is a system warm-up.") to force ONNX Runtime JIT/GPU kernel compilation. Without this, the first user request pays a 1-6 second penalty.

### 2.4 Model Unload Policy

- **On engine switch:** Unload previous model immediately (fixes CopySpeak R5 — currently leaks)
- **On idle:** `model_unload_timeout` (Handy setting: Never / 5min / 15min / 30min / OnAppExit)
- **On CUDA toggle:** Kill Piper server, respawn with CUDA EP
- **Manual:** Tray → "Unload Model" action
- **On voice change:** Does NOT restart (Piper loads voices per request; Kokoro shares ONNX session)

---

## 3. Kokoro Comparison & Best Method

| Aspect | Parrot's Kokoro | CopySpeak's Kokoro | S2B2S Current | **Winner** |
|--------|-----------------|-------------------|---------------|------------|
| Integration | `tts-rs` in-process Rust | CLI subprocess (`kokoro.exe`) | `tts-rs` in-process | **Parrot/S2B2S** |
| Engine pool | N workers, parallel chunks | Single process | Single engine | **Parrot** |
| Crossfade | 10ms @ 24kHz | None | None | **Parrot** |
| Shorten-first-chunk | Clause-split for fast TTFA | None | None | **Parrot** |
| Voice selection | Auto per-language, 54 voices | Manual | 54 listed, not wired | **Merge both** |
| Text normalization | `pulldown-cmark` | Regex | Regex | **Parrot + text-processing-rs** |
| Warm persistence | Model unload timeout | No (spawn per call) | Partial | **Add warm-up** |

**Bottom line:** Parrot's method is strictly better. S2B2S already uses it but is missing the pool, crossfade, and shorten-first-chunk. CopySpeak's contribution is the warm-server lifecycle pattern (state machine, generation tracking) which we apply to Kokoro too.

---

## 4. Kitten TTS Integration Plan

### Phase 1: CLI wrapper (immediate)
Use CopySpeak's existing `kittentts-cli.py` behind `TtsBackend` trait. Same pattern as current Piper — keep Python process warm.

```rust
struct KittenBackend {
    // Spawns: python kittentts-cli.py --serve --port RANDOM
    // Synthesizes via HTTP POST (same pattern as piper_server.rs)
    server: PiperServerLike,  // reuse the state machine!
}
```

Voices: 8 built-in (en-US), 3 model sizes (small/medium/large ONNX).

### Phase 2: In-process (P2)
Port to direct ONNX Runtime inference via `ort` crate, eliminating Python dependency. Reuse espeak-ng phoneme data from Kokoro.

---

## 5. Pocket TTS Integration Plan

### Source: Kyutai Labs (via RealtimeTTS)

**Built-in voices:** alba, marius, javert, jean, fantine, cosette, eponine, azelma
**Model:** CPU-oriented Torch model (~500 MB), optional CUDA fork (~1 GB VRAM)
**Capability:** Voice cloning from reference audio (5-10 second sample)

### Phase 1: Python CLI (immediate)
```rust
struct PocketTtsBackend {
    // Spawns: python -m pocket_tts.serve --port RANDOM --voice alba
    // OR for CUDA: python -m pocket_tts_gpu.serve --device cuda --voice alba
    server: PiperServerLike,
}
```

### Phase 2: In-process (P2)
Direct `ort` integration with Pocket TTS ONNX export, or keep as Python CLI if latency is acceptable (Pocket TTS is inherently slower than Piper/Kokoro — it's a quality/voice-cloning option, not a speed option).

### CUDA Path
```bash
# Setup script (ships with S2B2S)
pip install "realtimetts[pockettts-gpu]"
pip install torch --index-url https://download.pytorch.org/whl/cu126
pip install "git+https://github.com/Deveraux-Parker/kutai100temp.git@6beddc19c480da9ced9733ba0bb2f199f6e22ab4#subdirectory=pocket-tts-gpu"
```

---

---

## x. RNNoise — Current Implementation Audit & Fix Plan

### Status: ALREADY IMPLEMENTED

S2B2S ships a complete RNNoise integration but it's **not optimally wired**:

| Component | File | Status |
|-----------|------|--------|
| `NoiseSuppressor` struct | `audio_toolkit/audio/noise_suppression.rs:10` | ✅ Working (16kHz→48kHz upsample, denoise, downsample, voice probability) |
| `TripleVad` (3-stage cascade) | `audio_toolkit/vad/triple_vad.rs:13` | ✅ Working (RMS gate → RNNoise voice prob → Silero) |
| Settings toggle | `settings.rs:843` `noise_suppression_enabled` | ✅ Present (default: `false`) |
| VAD mode selector | `settings.rs:846` `vad_mode` | ✅ Present (default: `"silero"`, not `"triple"`) |
| Frontend UI | `src/.../AudioEnhancements.tsx:18` | ✅ Present (toggle + VAD mode dropdown) |
| Tauri command | `commands/audio.rs:332` `set_noise_suppression_enabled` | ✅ Wired |
| Tauri command | `commands/audio.rs:344` `set_vad_mode` | ✅ Wired |
| Conversation mode integration | `managers/audio.rs:132` | ✅ TripleVad created when `vad_mode == "triple"` |
| Dictation mode integration | `audio_toolkit/audio/recorder.rs:563` | ✅ Standalone NS used when enabled |
| Crate | `Cargo.toml` `nnnoiseless 0.5.2` | ✅ Already in tree |

### Pipeline (TripleVad mode)

```
Microphone (16kHz, 480-sample frames)
  │
  ▼
Stage 1: RMS Energy Gate (0.002 threshold) ←─ triple_vad.rs:55
  │  Rejects absolute silence immediately
  ▼
Stage 2: RNNoise Voice Probability (0.2 threshold) ←─ triple_vad.rs:66,74
  │  ns_enabled=true: passes denoised audio to Silero
  │  ns_enabled=false: passes raw audio to Silero (still gets voice prob)
  ▼
Stage 3: Silero VAD Confirmation (0.3 threshold) ←─ triple_vad.rs:80
  │  Final arbiter — must confirm speech
  ▼
VAD output: Speech / Noise
```

### Problems Found

| # | Problem | Location | Severity |
|---|---------|----------|----------|
| P1 | Default VAD is `"silero"`, not `"triple"` — RNNoise unused by default | `settings.rs:857` | **High** — TripleVad is the better detector, never activated |
| P2 | RNNoise voice prob threshold hardcoded at `0.2` | `managers/audio.rs:136` | **Medium** — Should be user-configurable, different environments need different sensitivity |
| P3 | Standalone NS in recorder denoises but doesn't feed voice probability to VAD | `audio_toolkit/audio/recorder.rs:567` | **Medium** — Lost opportunity; the NS knows if audio is voiced but the VAD doesn't get this signal |
| P4 | No visual feedback that RNNoise is active | Frontend | **Low** — Overlay/HUD should show "NS" badge when active |
| P5 | CUDA support for nnnoiseless not explored | `Cargo.toml` | **Low** — nnnoiseless is CPU-only; acceptable for latency budget |
| P6 | Recorder standalone NS and TripleVad NS are separate instances | `recorder.rs:567` + `triple_vad.rs:29` | **Low** — Duplicate denoiser if TripleVad mode + dictation both active; negligible RAM (~10MB) |

### Fixes (Phase 0.5 — Week 1)

| Fix | Description | Effort |
|-----|-------------|--------|
| **Default TripleVad** | Change `default_vad_mode()` to return `"triple"` for conversation mode. Dictation mode keeps `"silero"` (TripleVad adds ~2ms latency, acceptable for dictation but measurable). | XS |
| **Tunable voice prob threshold** | Add `rnnoise_voice_threshold: f64` to settings (range 0.1–0.9, default 0.2). Wire through `managers/audio.rs` and frontend slider in AudioEnhancements. | S |
| **NS voice prob passthrough** | When standalone NS is active in recorder, pass `voice_prob` from `process_16khz_frame` to the VAD detector as an additional signal for endpointing decisions. | M |
| **NS active indicator** | Emit event `audio:noise-suppression-changed` on toggle. Show in overlay when NS is actively denoising. | S |
| **Deduplicate NS instances** | In TripleVad mode, reuse the TripleVad's internal NS for the recorder path instead of creating a second instance. | S |
| **Per-mode defaults** | Conversation mode: TripleVad ON, NS ON. Dictation mode: TripleVad ON, NS OFF (lower latency). Push-to-talk: Silero only (fastest). Configurable per mode. | M |

### Post-Fix Architecture

```
mode: "conversation"  →  TripleVad (RMS + RNNoise prob + Silero), NS=ON, threshold=0.2
mode: "dictation"     →  TripleVad (RMS + RNNoise prob + Silero), NS=OFF, threshold=0.2
mode: "push-to-talk"  →  Silero only (fastest, user controls start/stop)
```

---

## 6. Phase Execution Plan

### Phase 0 — Foundation (Week 1)
| Task | Source | Effort |
|------|--------|--------|
| Settings-bindings **backfill on read** | Parler | XS |
| Crash logging | Parler | XS |
| Settings export/import | Parler | XS |
| Dev flavor Tauri config | Parler | XS |
| Declare MSRV 1.87 | CopySpeak H2 | XS |
| Add `lock_or_recover!` macro | CopySpeak | XS |
| **Add `text-processing-rs` to Cargo.toml** | New | XS |

### Phase 0.5 — RNNoise Wiring Fix (Week 1)
| Task | Source | Effort |
|------|--------|--------|
| Default TripleVad for conversation mode | Existing code | XS |
| Tunable RNNoise voice prob threshold setting | New | S |
| NS voice prob passthrough to VAD | Existing `noise_suppression.rs` | M |
| NS active indicator in overlay | New | S |
| Deduplicate NS instances (TripleVad + dictation) | Existing | S |
| Per-mode VAD/NS defaults | New | M |

### Phase 1 — Text Pipeline (Week 1-2)
| Task | Source | Effort |
|------|--------|--------|
| ITN wrapper (`sanitize/itn.rs`) | text-processing-rs | S |
| TN wrapper (`sanitize/tn.rs`) | text-processing-rs | S |
| Port `text_normalization.rs` (pulldown-cmark) | Parrot | M |
| Pipeline orchestrator (`sanitize/mod.rs`) | New | S |
| Regex cleanup keep/upgrade | Existing + CopySpeak | S |
| Sentence-level scanning | text-processing-rs `normalize_sentence` | S |
| Custom rules UI (user-defined replacements) | New | S |
| Test suite (ITN 1217 + TN tests + markdown + pipeline) | All sources | M |

### Phase 2 — Kokoro Worker Pool (Week 2-3)
| Task | Source | Effort |
|------|--------|--------|
| Engine pool with semaphore | Parrot `managers/tts.rs` | M |
| Crossfade 10ms @ 24kHz | Parrot | M |
| Shorten-first-chunk + clause split | Parrot | M |
| Parallel chunk synthesis | Parrot | M |
| CPU auto-tune (`tts_workers`) | Parrot | S |
| Voice-per-language auto-select | Parrot | S |

### Phase 3 — RAM Persistence (Week 3-4)
| Task | Source | Effort |
|------|--------|--------|
| `WarmEngine` trait | New (CopySpeak pattern) | S |
| Kokoro pre-warm at startup | Parrot + CopySpeak | S |
| Piper warm-up synthesis | CopySpeak | S |
| Engine-switch unload (R5 fix) | CopySpeak bugfix | S |
| Model unload timeout | Handy existing | S |
| CUDA auto-discovery (NVIDIA DLL PATH) | CopySpeak | S |
| Footer status indicator (loading/warm/ready/error) | CopySpeak | M |

### Phase 4 — New Engines (Week 4-5)
| Task | Source | Effort |
|------|--------|--------|
| Kitten TTS backend (CLI → `TtsBackend`) | CopySpeak `cli.rs` pattern | M |
| Pocket TTS backend (CLI → `TtsBackend`) | RealtimeTTS PocketTTSEngine | M |
| SAPI fallback (zero-download) | Windows TTS API | S |
| Parallel cloud synthesis (cap 3, JoinSet) | CopySpeak | S |

### Phase 5 — CopySpeak Feature Parity (Week 5-6)
| Task | Source | Effort |
|------|--------|--------|
| Telemetry-driven adaptive pagination | CopySpeak | M |
| Audio cache (history keyed by hash) | CopySpeak | M |
| Save-to-MP3/OGG/FLAC | CopySpeak (pure Rust encoders) | M |
| Effects (WalkieTalkie/GameBoy) | CopySpeak | M |
| HUD AmplitudeEnvelope waveform | CopySpeak `wav.rs` | S |
| Control HTTP API (axum, CSPRNG token) | AIVORelay + CopySpeak R4 fix | M |
| Agent harness (Python CLI + SKILL.md) | CopySpeak | S |

### Phase 6 — Streaming STT + Profiles (Week 6-8)
| Task | Source | Effort |
|------|--------|--------|
| OpenAI Realtime WS STT | AIVORelay | M |
| Deepgram WS STT | AIVORelay | M |
| Local MoonshineStreaming integration | Handy (already in transcribe-rs) | S |
| Profiles (per-mode language/prompt/model/hotkey) | AIVORelay | L |
| AI Replace Selection | AIVORelay | M |
| Prompt variables `${current_app}` `${time_local}` | AIVORelay | S |
| Long-audio model routing | Parler | S |
| WASAPI loopback capture | AIVORelay | M |

---

## 7. Text Pipeline Architecture Decision Record (ADR)

**ADR-014 · Text normalization: 4-pass pipeline with text-processing-rs + pulldown-cmark + regex fallback.**

- **Pass 1 (post-STT): ITN** via `text-processing-rs` — spoken form → written form. Converts "two hundred" → "200", dates, money, measurements, time. Toggleable per user preference.
- **Pass 2 (post-ITN): Custom words fuzzy correction** — existing `strsim`/`natural` pipeline for domain terms (kept).
- **Pass 3 (pre-TTS): pulldown-cmark** — markdown → speakable text. Parrot's proven approach, strict upgrade over regex markdown stripping.
- **Pass 4 (pre-TTS): TN** via `text-processing-rs` — written form → spoken form. Converts "123" → "one hundred twenty three", "$5.50" → "five dollars and fifty cents".
- **Pass 5 (pre-TTS): Regex cleanup** — kept as final scrub. Removes artifacts, normalizes whitespace, handles edge cases not covered by the structured passes.

*Rejected alternatives:*
- Regex-only: current approach, fails on every number/date/currency not explicitly matched.
- pulldown-cmark-only: Parrot's approach, good for markdown, doesn't handle numbers → speech.
- ITN-only: good for post-STT but insufficient for pre-TTS (TN is the complement).
- CopySpeak's regex sanitizer: strictly inferior to pulldown-cmark for markdown, no number handling.

*Mitigations:* every pass is toggleable. User can disable ITN (raw numbers to Brain), TN (raw numbers to TTS), or markdown stripping independently. Custom rules API lets users add domain-specific terms.

---

## 8. Dependency Matrix

### New Rust Crates

| Crate | Version | License | Purpose | Phase |
|-------|---------|---------|---------|-------|
| `text-processing-rs` | 0.2.2 | Apache 2.0 | ITN + TN (number/date/currency normalization) | P1 |
| `pulldown-cmark` | 0.13 | MIT | Markdown → speakable text | P1 |
| `lazy_static` | 1 | MIT/Apache | Already in tree via text-processing-rs | P1 |
| `axum` + `tower-http` | 0.8+ | MIT | Control HTTP server | P5 |
| `getrandom` | 0.2 | MIT/Apache | CSPRNG for control API token | P5 |
| `subtle` | 2 | BSD-3 | Constant-time token compare | P5 |

### Existing Crates (upgraded usage)

| Crate | New Usage |
|-------|-----------|
| `nnnoiseless` 0.5.2 | Already in tree (RNNoise) — fix wiring, add tunable threshold, per-mode defaults |
| `tts-rs` (kokoro) | Already present — add worker pool around it |
| `regex` | Kept as final scrub pass in text pipeline |
| `strsim` + `natural` | Kept for custom words fuzzy correction |
| `rodio` 0.22 | Already used for playback — crossfade integration |
| `rubato` 3.0 | Already used for RNNoise resampling (16kHz↔48kHz) — verified working |

### Python Sidecars (for CLI-based engines)

| Tool | Purpose | Setup Script |
|------|---------|-------------|
| `piper-tts[http]` | Piper HTTP server | `setup-piper-cpu.ps1` / `setup-piper-cuda.ps1` |
| `kokoro` | Already via tts-rs (in-process, no sidecar needed) | — |
| `kittentts-cli.py` | Kitten TTS CLI | `install-kittentts.ps1` |
| `pocket-tts` / `pocket-tts-gpu` | Pocket TTS | New `setup-pockettts.ps1` |

---

## 9. Not In Scope (v1)

- Full-duplex with AEC (echo cancellation) — half-duplex only, barge-in via hotkey
- Wake word ("Hey S2B2S") — requires sherpa-onnx KWS, backlog
- Local speaker diarization — cloud diarization only (Deepgram)
- Mobile companion app
- Plugin marketplace
- Non-Windows polish (macOS/Linux compile but not optimized)

---

## 10. Success Criteria

- **Dictation:** STT → ITN → Brain → TN → TTS produces correct numbers/dates/currency at every stage
- **Read-aloud:** selection → first audio < 700ms warm (Piper/Kokoro), numbers read as words
- **Conversation:** ≤ 1.5s end-of-speech → first audible reply (local 8B + warm Kokoro pool)
- **Text pipeline:** 1217 ITN tests + markdown test suite + regex cleanup tests all pass
- **Stability:** 500-synthesis soak with flat latency, persist-before-deliver verified by kill-test
- **RAM:** Idle < 500 MB with Kokoro pool warm (models loaded), < 250 MB with models unloaded
