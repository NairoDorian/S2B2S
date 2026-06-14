# Handy Family Fork Lineage

> How S2B2S was born — the complete genealogy of the Handy fork family.

---

## The Family Tree

```
                                Handy (cjpais)
                             MIT · Tauri 2 · React 18
                             "the most forkable STT app"
                                      │
          ┌───────────────────────────┼───────────────────────────┐
          │                           │                           │
          ▼                           ▼                           ▼
      Parler (Melvynx)          AIVORelay (MaxITService)      Parrot (rishiskhare)
   MIT · 66 commits ahead     MIT · 841 commits ahead     MIT · squashed history
   "Gentle fork"              "Wild fork"             "Inverted fork"
   + pause/resume             + streaming STT         + TTS (Kokoro)
   + Gemini STT               + system audio          - STT removed
   + long-audio routing       + profiles              + crossfade
   + settings export/import   + AI Replace            + shorten-first-chunk
   + crash logging            + encrypted connector   + AX-API capture
   + multi-monitor overlay    + voice commands        + pulldown-cmark
          │                    + RNNoise denoise
          │                           │
          │                           │
          └───────────┬───────────────┘
                      │
                      ▼
              ┌───────────────────┐
              │                   │
              │      S2B2S        │ ◄── + CopySpeak TTS (engine trait)
              │   (NairoDorian)   │ ◄── + transcribe-rs (STT engine)
              │        MIT        │ ◄── + pocket-tts-server (voice cloning)
              │                   │
              │  The Synthesis:   │
              │  STT from Handy   │
              │  + Parler features│
              │  + Parrot TTS     │
              │  + CopySpeak trait│
              │  + Streaming Brain│
              │  + Conversation   │
              │  + 3D overlay     │
              │  + Screen vision  │
              └───────────────────┘
```

---

## What S2B2S Inherited From Each Project

### From Handy (the skeleton)
- Tauri 2 application shell (patched runtime)
- Global shortcut system (tauri + handy-keys/rdev dual backend)
- Audio capture (cpal 16kHz mono)
- VAD (Silero via vad-rs)
- STT (8 engine families via transcribe-rs)
- Model download manager (checksums, resume, unpack)
- LLM client (OpenAI-compatible post-processing)
- Clipboard / paste pipeline (enigo + clipboard manager)
- Tray icon with localized menus
- Settings store (serde JSON, specta typed IPC)
- Recording overlay (pill) with NSPanel/layer-shell
- i18n (20 locales with RTL support)
- History (SQLite + audio retention)
- Updater (tauri-plugin-updater)
- Onboarding / permissions flow
- CLI remote control (single-instance flags)
- Debug mode
- Two shortcut engines, clamshell mic, sounds, autostart, portable mode

### From Parler (the polish)
- Long-audio model switching (duration threshold → different engine)
- Gemini STT / provider model-listing adapter pattern
- Pause/resume recording (F6)
- Settings export/import (JSON dump/restore)
- Crash logging (panic capture to file)
- Multi-monitor overlay fixes (scale-factor-correct positioning)
- Bindings backfill on settings read
- History: dual raw+post-process text + prompt used
- History reprocessing (re-run LLM on past transcripts)
- Dev flavor config (tauri.dev.conf.json)
- Windows thin-LTO CI fix
- macOS signing/notarization CI

### From Parrot (the TTS)
- TTS manager pattern (engine pool, worker auto-tuning, lifecycle)
- Streaming playback (shorten-first-chunk, chunk ordering)
- Selection capture (macOS AX API + sentinel clipboard probe)
- pulldown-cmark text normalization (markdown → speakable text)
- espeak-ng resource packaging
- Kokoro-82M integration (54 voices, 9 languages)
- Crossfade blending
- Model manager repurposed for TTS
- Speaking overlay with live spoken-text captioning
- CPU auto-tuning (infer_kokoro_tuning_for_cpu_count)

### From CopySpeak TTS (the engine design)
- **TtsBackend trait** — the engine abstraction for all 8 backends
- Persistent Piper server with pre-warm and hidden warm-up synthesis
- Pagination system (text → speakable fragments)
- Fragment queue for ordered playback
- lock_or_recover! macro (mutex poison recovery)
- Telemetry (chars/ms EMA estimation)
- Audio caching (history-based replay)
- Control server (localhost HTTP API, modeled after)
- TTS lifecycle events (loading → ready → error)

### Unique to S2B2S (built from scratch)
- **TripleVAD** (RMS gate + RNNoise voice probability + Silero confirmation)
- **Conversation system** (continuous voice loop, multi-turn, barge-in)
- **Streaming Brain** (SSE tokens → sentence splitter → TTS speak-before-finish)
- **Local llama.cpp server** (auto-download, GPU offload, CUDA/Vulkan/CPU)
- **ITN normalization** (text-processing crate)
- **SAPI TTS backend** (Windows native speech, not a stub)
- **Pocket TTS backend** with voice cloning from WAV
- **HerLoading.tsx** (Three.js 3D loading animation → avatar DNA)
- **Brain event stream** (brain:thinking/token/sentence/done/error/latency)
- **Gemma-4 model orchestration**
- **Cross-platform with Wayland layer-shell** (env kill-switch + graceful fallback)
- **Planned:** GPU transparent overlay + 3D avatar + screen vision

---

## Inheritance Architecture

```
S2B2S = Handy (skeleton) 
      + Parler (pause, crash, settings, long-audio) 
      + Parrot (TTS engine pool, selection capture, text norm) 
      + CopySpeak (TtsBackend trait, Piper server, pagination, telemetry)
      + Unique (TripleVAD, Streaming Brain, Conversation, llama.cpp, HerLoading, ITN, SAPI, Pocket)
```

**The result:** a single app that does STT (8 local + cloud), TTS (8 engines including local), streaming LLM conversation (local + cloud), cross-platform with 20 i18n locales, and a planned GPU overlay mode. No other project in the ecosystem combines all four poles (STT + TTS + Brain + Cross-platform).

---

## Commits From Each Source

| Source | Contributions | Notable |
|--------|--------------|---------|
| Handy | Skeleton + STT pipeline | ~10,000 Rust lines inherited |
| Parler | Features ported | ~500 lines adopted |
| Parrot | TTS subsystem | ~3,000 lines adapted |
| CopySpeak | Engine design | ~2,000 lines adapted + TtsBackend copied |
| S2B2S unique | Brain + Conversation + Overlay | ~15,000+ new lines |

---

*This lineage analysis underpins the importance of each reference project review. See `00_COMPARATIVE_ANALYSIS.md` for cross-project feature comparison.*
