# Integration Plan — Inspired by `huggingface/speech-to-speech` & `handy-computer/transcribe.cpp`

> Status: **planning doc** — no code yet. Each item maps a feature from the two repos to
> a concrete target in the S2B2S codebase (`src-tauri/src`, `src/`) and is scoped so it can
> be opened as a Discussion → PR per `AGENTS.md`. Everything stays cross-platform
> (Windows / macOS / Linux) per the Cross-Platform Mandate.

## 1. TL;DR of what's interesting

| Repo | Standout feature for S2B2S |
| --- | --- |
| `huggingface/speech-to-speech` | (a) OpenAI **Realtime-compatible WebSocket API** so any standard voice client can drive S2B2S; (b) `socket` thin-client mode → clean split for the **Android app as a thin client to the desktop brain**; (c) **Qwen3-TTS** local backend; (d) wire-protocol barge-in + tool-calling. |
| `handy-computer/transcribe.cpp` | A single **GGUF/ggml STT engine** with an **official Rust binding** that adds model families S2B2S lacks: **Qwen3-ASR, Voxtral (audio-LLM), Nemotron 3.5 ASR Streaming (40 locales), Canary-Qwen, Granite Speech, Cohere Transcribe** — all WER-verified, with a quantize tool. |

Both reinforce S2B2S's existing direction (cascaded VAD→STT→Brain→TTS, local-first, Tauri/Rust) rather than conflicting with it.

---

## 2. Proposed workstreams

### WS-1 — Expose S2B2S as an OpenAI-Realtime-compatible voice server
**Source:** `huggingface/speech-to-speech` Realtime API (`/v1/realtime`).
**Why:** S2B2S already has every pipeline stage (TripleVAD, STT, Brain/LLM, TTS). Wrapping it behind the open Realtime protocol turns the app into a standard, language-agnostic voice-agent server — and is the natural backend for the planned Android thin client.
**Target:**
- `src-tauri/src/control_server.rs` (axum) — add a `ws://…/v1/realtime` route.
- `src-tauri/src/brain/manager.rs` + `src-tauri/src/managers/transcription.rs` + `tts/` — bridge to the existing streaming pipeline.
**Protocol surface to implement (minimum viable):**
- Inbound: `session.update` (incl. `turn_detection` / `interrupt_response`), `input_audio_buffer.append`, `response.create`, `response.cancel`, `conversation.item.create`.
- Outbound: speech start/stop, `conversation.item.*` transcript (live + final), `response.audio.delta`, `response.done`, `response.function_call_arguments.delta`.
**Notes:** Reuse the existing barge-in in `brain/manager.rs` to satisfy `interrupt_response`. Reuse `llm_client.rs` (already OpenAI-compatible) as the LLM slot. Keep cross-platform — the WS server is pure Rust/axum, no OS deps.

### WS-2 — `socket` thin-client mode for the Android port
**Source:** `huggingface/speech-to-speech` `socket`/`websocket` run modes (model server + minimal mic/playback client).
**Why:** `docs/android.md` wants a standalone Android app. A pragmatic first step is the **desktop as the heavy engine** and the phone as a capture/playback client over raw PCM — exactly the `socket` pattern. Later the same client can target on-device engines.
**Target:**
- Desktop: extend `control_server.rs` with a `socket` (TCP) and `websocket` (raw 16 kHz int16 mono PCM) transport.
- Android client: a thin capture/playback PoC (see `docs/android.md`); reuse `sherpa-onnx`/Parakeet-Unified already in the stack for optional on-device path.
**Notes:** Define a tiny framing protocol (length-prefixed PCM chunks + control messages). No cloud, no telemetry — matches S2B2S local-first stance.

### WS-3 — Qwen3-TTS as a local TTS backend
**Source:** `huggingface/speech-to-speech` default TTS (Qwen3-TTS, GGML on non-macOS / mlx-audio on macOS).
**Why:** S2B2S's local TTS backends are Piper / Kokoro / Kitten / Pocket. Qwen3-TTS is high-quality, multilingual, streaming, and GGUF/GGML-based — fits the existing `local_tts_server.rs` warm-engine pattern.
**Target:** `src-tauri/src/tts/backends/` — new `qwen3_tts.rs` behind the `TtsBackend` trait, following `pocket.rs`/`kokoro.rs` lifecycle; add to `mod.rs` backend registry + settings + i18n strings.
**Notes:** Gated CPU/CUDA; provide a macOS `mlx-audio` path via `#[cfg(target_os = "macos")]`.

### WS-4 — New GGUF STT backend via `transcribe.cpp` (feature-gated)
**Source:** `handy-computer/transcribe.cpp` + its Rust binding `bindings/rust/transcribe-cpp`.
**Why:** Unlocks model families absent from `transcribe-rs` — notably **Qwen3-ASR**, **Voxtral** (audio-LLM transcription+translation), **Nemotron 3.5 ASR Streaming (40 locales)** for strong multilingual, **Canary-Qwen**, **Granite Speech**, **Cohere Transcribe**. Single GGUF engine, WER-verified quants, official Rust crate → matches the Tauri/Rust stack.
**Target:**
- `src-tauri/src/managers/transcription.rs` — add a `TranscribeCpp` backend behind a cargo feature (e.g. `stt-transcribe-cpp`), mirroring how `transcribe-rs` is wired.
- `src-tauri/src/stt/` — optionally extend `multi_stt.rs` to fan out to the new engine alongside existing ones.
- Settings: expose the new model families + quant selection (`F16/Q8_0/Q6_K/Q5_K_M/Q4_K_M`) in the STT model picker.
**Notes:** Must coexist with `transcribe-rs` (don't remove it). Provide macOS/Linux/Windows build for the ggml engine (Metal/Vulkan/CPU). Document the `transcribe-quantize` tool in `models/` scripts.

### WS-5 — Multilingual + live partial transcription upgrades — ✅ DONE (partial)
**Source:** both repos — `speech-to-speech` live transcription + `--language auto`; `transcribe.cpp` Nemotron 3.5 (40 locales) / Qwen3-ASR multilingual.
**Status:** Live partial transcription already existed in the tree (`StreamTextEvent` with committed/tentative in `managers/transcription.rs`; `transcribe-cpp` already partially wired). The new work added the **language-forwarding** half:
- `settings.rs` → `BrainConfig.reply_language` (default `"auto"`).
- `brain/manager.rs` → `ask_multimodal(..., reply_language: Option<String>)` prepends `"Please respond in <lang>."` to the model-facing user turn only (history stays clean), mirroring `--enable_lang_prompt`.
- `actions.rs` / `continuous_voice.rs` → resolve the effective STT language (`resolve_reply_language`) and pass it into the Brain. `auto` defers to the selected language / OS input source; a concrete code forces a fixed reply language; `en`/`auto`/`os_input` emit no hint.
**Follow-up (deferred, needs engine support):** per-utterance detected language. Requires STT engines to *return* a detected language from `transcribe()`; today `transcribe()` returns only `String`. Wire that through `TranscriptionManager` when the `transcribe-cpp`/Whisper engines expose detection.

### WS-6 — Silero VAD v5 bump
**Source:** `huggingface/speech-to-speech` uses Silero VAD **v5**; S2B2S currently ships **v4** (`silero_vad_v4.onnx`).
**Why:** v5 improves turn-taking robustness — directly benefits barge-in.
**Target:** `src-tauri/src/audio_toolkit/vad/silero.rs` + `models/download_models.*` (fetch v5 onnx). Validate against `triple_vad.rs`.
**Notes:** Keep v4 as a fallback behind a setting; cross-platform (ONNX Runtime).

### WS-7 — Wire-protocol tool/function calling (voice agents)
**Source:** `speech-to-speech` Realtime tool-call events.
**Why:** Extends the conversation from "chat" to "act" (e.g. voice-trigger S2B2S actions: read clipboard, open app). Builds on WS-1.
**Target:** `brain/` — add a tool registry + `response.function_call_arguments.delta` handling; expose a couple of local tools first.
**Notes:** Defer until WS-1 lands.

---

## 3. Suggested sequencing

1. **WS-6 (VAD v5)** — smallest, de-risks barge-in, no new deps.
2. **WS-4 (transcribe.cpp backend)** — high value (new models), isolated behind a feature flag.
3. **WS-3 (Qwen3-TTS)** — adds a quality local TTS backend, follows existing pattern.
4. **WS-1 (Realtime server)** — the architectural centerpiece; reuse WS-3/WS-4 engines.
5. **WS-2 (socket thin client)** — pairs with WS-1 for the Android PoC in `docs/android.md`.
6. **WS-5 (multilingual/live)** then **WS-7 (tools)** — polish on top of the protocol.

## 4. Open questions for Discussion

- WS-1: implement the *full* Realtime event set or a minimal subset first?
- WS-4: vendor the `transcribe-cpp` Rust crate via git dep, or FFI to the prebuilt `transcribe.h`? (Windows-friendly prebuilt DLL path preferred for the Cross-Platform Mandate.)
- WS-2: TCP `socket` vs WebSocket for the thin client — WS is simpler to firewall/secure.
- Licensing: both upstreams are MIT/Apache-2.0 — compatible with S2B2S MIT.
