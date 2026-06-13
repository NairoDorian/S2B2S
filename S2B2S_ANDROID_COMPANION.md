# S2B2S Android Assistant Companion — Brainstorm & Plans

> Companion mobile app that extends the S2B2S desktop experience to Android.  
> Connects to your desktop S2B2S instance via LAN for STT, TTS, and Brain access from your phone.

---

## Vision

S2B2S runs your local AI stack (STT → Brain → TTS) on a powerful desktop with GPU. The Android companion is a **thin client** — it streams audio to/from your desktop, acting as a remote voice interface. Think of it as "Siri, but powered by your own local Gemma-4 with 8 TTS engines."

```
Phone Mic → WebSocket → Desktop S2B2S → Brain → TTS → Audio stream → Phone Speaker
```

---

## Architecture Options

### Option A: Native Android (Kotlin)
- **Pro:** Best performance, system integration, Play Store distribution
- **Con:** Separate codebase, more development effort
- **Tech:** Android MediaProjection, WebSocket client, ExoPlayer for audio

### Option B: Progressive Web App (PWA)
- **Pro:** Same React/TS codebase as desktop, fast iteration
- **Con:** Limited microphone access on Android, no background audio
- **Tech:** Service Worker, Web Audio API, Vite PWA plugin

### Option C: Tauri Mobile (Rust + WebView)
- **Pro:** Shares Rust backend code with desktop S2B2S
- **Con:** Tauri mobile is still early (alpha/beta), limited plugin ecosystem
- **Tech:** Tauri 2 mobile, same React frontend

**Recommendation:** Option B (PWA) for v0.1 (fastest path), Option A (Kotlin) for full-featured v1.0.

---

## Reference Projects (GitHub)

### Local TTS on Android (Pocket, Kokoro, Piper)

| Project | Engine | Approach | Notes |
|---|---|---|---|
| [pocket-tts-unity](https://github.com/lookbe/pocket-tts-unity) | Pocket TTS | Unity C# wrapper for Pocket TTS ONNX | Shows Pocket can run on mobile via ONNX Runtime |
| [Kokoro-82M-Android](https://github.com/puff-dayo/Kokoro-82M-Android) | Kokoro 82M | Native Android (C++/JNI) ONNX Runtime | Full Kokoro on Android. Model bundling, espeak-ng integration. |
| [SherpaTTS](https://github.com/woheller69/ttsEngine) | Piper + Sherpa | Android TTS engine replacing Google TTS | System-level TTS integration. Uses sherpa-onnx. |
| [VoxSherpa-TTS](https://github.com/CodeBySonu95/VoxSherpa-TTS) | Piper + Kokoro | React Native + sherpa-onnx | Cross-platform mobile TTS with React Native |

### Speech-to-Text / Voice Assistants on Android

| Project | Focus | Notes |
|---|---|---|
| [NekoSpeak](https://github.com/siva-sub/NekoSpeak) | Voice assistant | Android voice interface with LLM integration |
| [speech-android](https://github.com/soniqo/speech-android) | STT + TTS | Full speech pipeline on Android, good reference architecture |

### Key Observations

1. **Kokoro-82M runs on Android** — the C++/JNI approach from `Kokoro-82M-Android` proves it's viable. ONNX Runtime with XNNPACK backend works on ARM.
2. **Pocket TTS via Unity** — `pocket-tts-unity` shows Pocket's ONNX model runs on mobile. Could be ported to native Android.
3. **Sherpa-onnx is the bridge** — `SherpaTTS` and `VoxSherpa-TTS` both use sherpa-onnx, which supports Piper, Kokoro, and VITS models with a unified C API. This is the best approach for Android TTS.
4. **System-level TTS** — `woheller69/ttsEngine` replaces the system TTS engine, which means any app can use local TTS automatically.

---

## Feature Plan

### Phase 1 — Remote Companion (v0.1, PWA)

**Goal:** Stream audio to desktop S2B2S and receive TTS back. No local inference.

| Feature | Description | Status |
|---|---|---|
| LAN auto-discovery | Find desktop S2B2S via mDNS/Bonjour | 📋 Planned |
| WebSocket connection | Connect to S2B2S control_server WebSocket | 📋 Planned |
| Push-to-talk | Hold button → stream mic audio → desktop transcribes | 📋 Planned |
| Play TTS response | Receive audio stream from desktop, play on phone | 📋 Planned |
| Conversation mode | Full voice chat loop (STT → Brain → TTS) | 📋 Planned |
| Engine selector | Switch TTS engine/voice from phone | 📋 Planned |
| Audio feedback | Sound effects for start/stop/error | 📋 Planned |

### Phase 2 — Local Inference (v1.0, Native Android)

**Goal:** Run TTS and optionally STT directly on the phone. Offline-capable.

| Feature | Description | Status |
|---|---|---|
| Local Kokoro TTS | ONNX Runtime + espeak-ng on Android (like Kokoro-82M-Android) | 📋 Planned |
| Local Piper TTS | Via sherpa-onnx Android library | 📋 Planned |
| Local Pocket TTS | Candle/ONNX on Android | 📋 Planned |
| System TTS replacement | Register as Android system TTS engine | 📋 Planned |
| Local STT | Whisper.cpp or sherpa-onnx streaming STT | 📋 Later |
| Background service | Keep TTS running when app is backgrounded | 📋 Later |
| Notification listener | Read notifications aloud via TTS | 📋 Later |

### Phase 3 — Full Mobile Experience (v2.0)

| Feature | Description | Status |
|---|---|---|
| Widget | Home screen button for quick voice commands | 📋 Later |
| Wear OS companion | Voice interface on smartwatch | 📋 Later |
| Offline Brain | Run small LLM (Gemma-2B, Qwen2.5-1.5B) via llama.cpp Android | 📋 Later |
| Multi-device sync | Sync settings/history/personas between desktop and phone | 📋 Later |
| Bluetooth headset | Full headset button integration (tap to talk) | 📋 Later |

---

## Technical Architecture (v0.1 PWA)

```
┌─────────────────────────────────────┐
│         Android Phone (PWA)         │
│                                     │
│  React/TS Frontend (shared code)    │
│  ├─ WebSocket client                │
│  ├─ AudioRecorder (MediaRecorder)   │
│  ├─ AudioPlayer (Web Audio API)     │
│  └─ Push notification (Service Wkr) │
│                                     │
│  ───────── WebSocket ───────────────│
│            │                        │
└────────────│────────────────────────┘
             │
             │ LAN (WiFi)
             │
┌────────────│────────────────────────┐
│       Desktop S2B2S                 │
│                                     │
│  control_server.rs (axum)           │
│  ├─ /v1/mobile/listen  (WebSocket) │
│  ├─ /v1/mobile/speak   (WebSocket) │
│  ├─ /v1/mobile/converse(WebSocket) │
│  └─ /v1/mobile/config  (REST)      │
│                                     │
│  TTS Manager → Piper/Kokoro/etc.    │
│  Brain Manager → llama.cpp          │
│  STT Manager → Parakeet/Whisper     │
└─────────────────────────────────────┘
```

### WebSocket Protocol (Mobile)

Modeled after Vox's protocol:

**Client → Server (binary audio frames):**
```
PCM f32 LE, 16kHz, mono, 512-sample frames
```

**Server → Client (JSON events):**
```json
{"type":"ready", "engines":["piper","kokoro","kitten","pocket"]}
{"type":"speech_start"}
{"type":"transcript", "text":"Hello", "stt_ms":320}
{"type":"thinking"}
{"type":"sentence", "index":0, "text":"I'm doing well!", "tts_ms":180}
{"type":"turn_done", "tokens_per_sec":95.3}
```

**Audio playback:** Server sends WAV bytes as binary WebSocket frames interleaved with JSON events. Client plays via `AudioContext.decodeAudioData()`.

---

## Implementation Plan for Desktop Side

### 1. Upgrade control_server.rs for Mobile WebSocket

Current `control_server.rs` has a basic HTTP server. Extend it with:

```rust
// New WebSocket endpoint
.route("/v1/mobile/converse", get(mobile::converse_ws))
.route("/v1/mobile/speak", get(mobile::speak_ws))
.route("/v1/mobile/listen", get(mobile::listen_ws))
```

Reuse the existing `TtsManager`, `BrainManager`, and transcription pipeline from the desktop app — no need to reimplement anything.

### 2. mDNS Service Discovery

```toml
# Add to Cargo.toml
mdns-sd = "0.11"
```

```rust
// Advertise S2B2S on LAN
let service = mdns::ServiceInfo::new(
    "_s2b2s._tcp.local.",
    "s2b2s-desktop",
    "s2b2s-desktop.local.",
    "127.0.0.1",
    43117,
    Some(&[("version", "0.10")]),
)?;
```

The phone scans for `_s2b2s._tcp.local.` to find the desktop automatically.

### 3. Mobile PWA Setup

Add to existing React codebase:
```json
// vite.config.ts
VitePWA({
  registerType: 'autoUpdate',
  manifest: {
    name: 'S2B2S Companion',
    short_name: 'S2B2S',
    theme_color: '#7c3aed',
    icons: [/* ... */]
  }
})
```

---

## Brainstorm — Killer Mobile Features

- **Car Mode** — Large buttons, auto-answer. "Read my last WhatsApp message and reply"
- **Sleep sounds** — TTS reads a bedtime story in a calm voice (Kokoro `af_nova`)
- **Language tutor** — Speak in language X, get corrections read back in Piper voice
- **Meeting transcriber** — Record meeting, transcribe with speaker diarization, email summary
- **Walking assistant** — Offline TTS reads directions/notifications while walking (no data needed)
- **Home automation** — "Hey S2B2S, turn off the lights" → Brain → Home Assistant API
- **Audiobook maker** — Select any text/PDF → TTS reads it chapter by chapter → saves as MP3 audiobook

---

## Links & Resources

### Android TTS Engines
- [SherpaTTS — System TTS Engine](https://github.com/woheller69/ttsEngine) — Piper/Kokoro/VITS as Android system TTS
- [Kokoro-82M-Android](https://github.com/puff-dayo/Kokoro-82M-Android) — Native Kokoro on Android
- [VoxSherpa-TTS](https://github.com/CodeBySonu95/VoxSherpa-TTS) — React Native TTS with sherpa-onnx
- [pocket-tts-unity](https://github.com/lookbe/pocket-tts-unity) — Pocket TTS in Unity (Android/iOS)

### Android Voice Assistants
- [NekoSpeak](https://github.com/siva-sub/NekoSpeak) — Android voice assistant
- [speech-android](https://github.com/soniqo/speech-android) — Speech pipeline for Android

### Sherpa-onnx (Unified TTS/STT C API)
- [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx) — C++ inference for Piper, Kokoro, VITS, Whisper, Zipformer
- [sherpa-onnx Android demo](https://github.com/k2-fsa/sherpa-onnx/tree/master/android) — Official Android examples

### Desktop Integration
- [S2B2S control_server.rs](src-tauri/src/control_server.rs) — Existing local HTTP server (base for mobile API)
- [Vox WebSocket protocol](https://github.com/mrtozner/vox) — Reference for voice-over-WebSocket protocol design

### PWA / Mobile Web
- [Vite PWA Plugin](https://vite-pwa-org.netlify.app/) — Turn any Vite app into a PWA
- [Web Audio API](https://developer.mozilla.org/en-US/docs/Web/API/Web_Audio_API) — Browser audio processing
- [MediaRecorder API](https://developer.mozilla.org/en-US/docs/Web/API/MediaRecorder) — Browser microphone capture
