# Reference Links — STT · Brain (LLM) · TTS · On-Device Android

A curated, descriptive reference of open-source projects relevant to **[NairoDorian/S2B2S](https://github.com/NairoDorian/S2B2S)** (Speech → Brain → Speech). The goal of S2B2S is to merge the best ideas from these projects into one **local-first** (but online-capable) voice-native app — first on desktop, and soon as a **true standalone Android app** doing on-device STT + a local "brain" LLM + TTS.

> **About the dates.** "Last commit" and "Latest release" were pulled from each repo's public Atom feeds and READMEs on **19 June 2026**. "Release: commits only" means the project ships from `main`/`master` without tagged GitHub Releases. Treat dates as a freshness signal, not an endorsement. Descriptions are paraphrased from each project's README/About.

> **No rankings here.** This document only _describes_ each project; head-to-head comparisons are intentionally left out.

---

## Table of Contents

1. [S2B2S core & NairoDorian's own projects](#1-s2b2s-core--nairodorians-own-projects)
2. [STT / ASR — desktop apps (Handy lineage & friends)](#2-stt--asr--desktop-apps-handy-lineage--friends)
3. [STT / ASR — engines & libraries](#3-stt--asr--engines--libraries)
4. [TTS — engines, models & desktop apps](#4-tts--engines-models--desktop-apps)
5. [Combined voice assistants (STT + Brain + TTS) — desktop / server](#5-combined-voice-assistants-stt--brain--tts--desktop--server)
6. [Full-duplex speech-to-speech models](#6-full-duplex-speech-to-speech-models)
7. [Local LLM "brain" runtimes — desktop / server](#7-local-llm-brain-runtimes--desktop--server)
8. [On-device Android LLM inference engines](#8-on-device-android-llm-inference-engines)
9. [Google AI Edge stack (LiteRT · Gemma · MTP · QAT)](#9-google-ai-edge-stack-litert--gemma--mtp--qat)
10. [NPU / low-bit / specialized on-device engines](#10-npu--low-bit--specialized-on-device-engines)
11. [Android STT — keyboards & voice input](#11-android-stt--keyboards--voice-input)
12. [Android TTS — engines & apps](#12-android-tts--engines--apps)
13. [Android voice assistants & full AI suites](#13-android-voice-assistants--full-ai-suites)
14. [Cross-platform voice I/O studios](#14-cross-platform-voice-io-studios)
15. [Upstream models & shared dependencies](#15-upstream-models--shared-dependencies)
16. [Curated "awesome" lists](#16-curated-awesome-lists)

---

## 1. S2B2S core & NairoDorian's own projects

### NairoDorian/S2B2S — SpeechToBrainToSpeech

**The project everything here feeds into.** A local-first STT → Brain → TTS desktop app for Windows 11, macOS, and Linux: dictate anywhere, read anything aloud, and hold a natural spoken conversation with a local AI, almost keyboard-free.

- **What it does:** Unifies three pipelines — dictation (STT), read-aloud (TTS), and a streaming LLM conversation mode. Highlights from the README: **8 TTS backends** (Piper, Kokoro, Kitten, Pocket, SAPI, OpenAI, ElevenLabs, Cartesia) with a RAM-persistent "warm" model lifecycle; a streaming LLM chat with **20-turn memory across 10 providers**; a **pre-compiled llama.cpp** CUDA/Vulkan/CPU server with GPU-VRAM offloading; per-message performance metrics (tokens/sec, STT/TTS latency); sentence streaming for fast time-to-first-audio; a double-copy clipboard trigger; RAM/VRAM footer indicators; Pocket voice cloning; a full text-normalization pipeline (ITN + TN + markdown stripping); and a brain overlay with a 3D avatar.
- **Tech stack:** Rust + Tauri (forked from the [Handy](https://github.com/cjpais/Handy) skeleton, MIT), Bun/Vite front-end, llama.cpp brain, `transcribe-rs`-class STT engines, mixed ONNX/CLI TTS. Nix-based dev env. The repo already contains a `gemma_4_qat_mtp_e2b/` folder — i.e. experiments with Gemma-4 QAT + MTP at the E2B size.
- **Why it matters:** This is the integration target; the other entries are the menu of components and reference designs to pull from.
- **State:** Last commit **2026-06-19** (actively developed, 776 commits) · Release: commits only
- **Links:** [GitHub](https://github.com/NairoDorian/S2B2S) · upstream skeleton → [cjpais/Handy](https://github.com/cjpais/Handy)

### NairoDorian/copyspeak-tts — CopySpeak TTS

**Lightweight Windows clipboard-to-speech reader.** Copy the same text twice quickly and it reads it aloud — no manual paste, no hotkey juggling.

- **What it does:** Multiple trigger modes (double-copy with a 1.5 s window, hotkey, or manual), 5 TTS engines (Kitten TTS default — CPU ONNX, 8 voices; Piper; Kokoro; OpenAI; ElevenLabs), a floating HUD with live waveform, persistent history, EN/ES i18n, speed/pitch/volume controls, markdown stripping + text normalization, and an auto-updater. Effectively the read-aloud half of S2B2S prototyped as a standalone tool.
- **Tech stack:** Tauri + Bun (TypeScript/React), ONNX (Kitten), CLI engines (Piper/Kokoro).
- **State:** Last commit **2026-06-02** · Release: commits only (README shows v0.1.5)
- **Links:** [GitHub](https://github.com/NairoDorian/copyspeak-tts)

### NairoDorian/Cross_Platform_Rust_WebGPU_CursorFX — Cross-Platform CursorFX

**GPU-accelerated cursor-effects overlay.** A transparent, fullscreen overlay that renders particle trails, click ripples, orbiting satellites, and glow around the cursor in real time.

- **Why it's here:** Not a voice tool, but a proving ground for the same cross-platform desktop shell S2B2S uses ("1 program fits all"). WGSL shaders run particle/ripple/ribbon simulation on the GPU at display refresh (60–144 Hz+); a separate React panel controls settings live.
- **Tech stack:** Tauri V2 + Bun + Vite 6 + React + TailwindCSS 4, **wgpu 24** (Metal/Vulkan/DX12). Windows 11 / macOS / Linux.
- **State:** Last commit **2026-06-12** · Release: commits only
- **Links:** [GitHub](https://github.com/NairoDorian/Cross_Platform_Rust_WebGPU_CursorFX)

### NairoDorian/TD_Web_Trail — Web Trail V7

**Physics-based cursor/touch trail that streams coordinates into TouchDesigner** in real time over a custom binary WebSocket protocol.

- **Why it's here:** A low-latency, zero-dependency rendering + streaming experiment (spring-friction physics chain, dual-canvas bloom rendering, Bézier/Catmull-Rom curve modes, multi-touch ripples). Prior art for S2B2S's real-time overlay/visualization and binary-protocol plumbing (the kind of local socket transport a desktop↔mobile bridge would use).
- **Tech stack:** Zero-dependency Bun.js WebSocket server + custom binary protocol; canvas rendering.
- **State:** Last commit **2026-05-27** · Release: commits only
- **Links:** [GitHub](https://github.com/NairoDorian/TD_Web_Trail)

---

## 2. STT / ASR — desktop apps (Handy lineage & friends)

> S2B2S forks **Handy**, and several entries below are _also_ Handy forks — useful to see how others extended the same base.

### cjpais/Handy — the forkable STT app

**Free, open-source, offline push-to-talk dictation for desktop.** Press a shortcut, speak, and your words land in any text field — entirely on-device via Whisper + VAD. Its stated goal is to be "the most forkable" STT tool, which is why S2B2S, Parler, and AivoRelay all build on it.

- **Tech stack:** Rust + Tauri, [`transcribe-rs`](https://github.com/cjpais/transcribe-rs) engines, whisper.cpp.
- **State:** Last commit **2026-06-18** · Release **v0.8.3** (2026-04-28)
- **Links:** [GitHub](https://github.com/cjpais/Handy) · [handy.computer](https://handy.computer)

### Melvynx/Parler

**A Handy fork** (its README is Handy's) maintained by Melvynx — same offline shortcut-driven dictation core, packaged separately. A parallel reference fork to compare integration choices against S2B2S.

- **State:** Last commit **2026-06-18** · Release **v0.9.1** (2026-06-18, actively developed)
- **Links:** [GitHub](https://github.com/Melvynx/Parler)

### MaxITService/AIVORelay — AivoRelay

**"AI Voice Relay" for Windows — a Handy fork with extra integrations.** Place the cursor anywhere, press a hotkey, speak, and text appears; the transcript can then be post-processed by an LLM. Many models (cloud or local); fully local/free if you use only local STT. Extras: a portable NVIDIA/CUDA build, a Microsoft Store listing (signed, no admin rights), multiple release branches.

- **State:** Last commit **2026-06-18** · Release **v1.0.20** (2026-06-18, actively developed)
- **Links:** [GitHub](https://github.com/MaxITService/AIVORelay) · [Microsoft Store](https://apps.microsoft.com/detail/9ppfkfh2zn1l)

### TheSethRose/Parakeet-Realtime-Transcriber

**Real-time system-audio + microphone transcriber** built around NVIDIA's Parakeet TDT 0.6B V2, with VAD, intelligent sentence grouping, a rolling 3 s overlap buffer, and duplicate filtering. Stores timestamped segments/sessions in a Neon PostgreSQL database for searchable history. Good reference for capturing _any_ audio playing on the machine (meetings, videos), not just the mic.

- **Tech stack:** Python, NVIDIA Parakeet TDT 0.6B V2, Neon Postgres.
- **State:** Last commit **2025-06-13** · Release: commits only
- **Links:** [GitHub](https://github.com/TheSethRose/Parakeet-Realtime-Transcriber)

### homelab-00/TranscriptionSuite

**Fully local, private STT app** with cross-platform support, **speaker diarization**, an "Audio Notebook" mode, **LM Studio integration**, and both long-form and live transcription. Interesting for S2B2S's diarization/note-taking ambitions and as a model for wiring a local LLM (via LM Studio) into a transcription workflow.

- **State:** Last commit **2026-06-16** · Release **v1.3.6** (2026-06-09)
- **Links:** [GitHub](https://github.com/homelab-00/TranscriptionSuite)

### EpicenterHQ/epicenter — Whispering

**Local-first apps that write to files you own** (plain Markdown + SQLite you can grep, version, open in Obsidian). The flagship is **Whispering**, an installable desktop STT app. The "your data outlives the app" philosophy aligns closely with S2B2S's local-first stance.

- **Tech stack:** Monorepo (STT app in `apps/whispering`); local-first storage.
- **State:** Last commit **2026-06-19** · Release **@epicenter/workspace@0.3.0** (2026-06-14)
- **Links:** [GitHub](https://github.com/EpicenterHQ/epicenter) · [Whispering app dir](https://github.com/EpicenterHQ/epicenter/tree/main/apps/whispering) · [epicenter.so](https://epicenter.so)

### asrjs/speech-recognition — @asrjs/speech-recognition

**A speech-first TypeScript runtime for browser and local Node.js inference.** One npm package with intentional subpath entry points; built around explicit runtime composition, architecture-based model families, branded presets, and reusable realtime/browser helpers. Notable for **WebGPU + Whisper fp16** browser inference — relevant if S2B2S ever exposes a web/WASM path.

- **State:** Last commit **2026-06-16** · Release: rolling branch tags
- **Links:** [GitHub](https://github.com/asrjs/speech-recognition) · `npm install @asrjs/speech-recognition`

---

## 3. STT / ASR — engines & libraries

### cjpais/transcribe-rs — the STT engine behind Handy/S2B2S

**Multi-engine speech-to-text library for Rust.** One crate, many backends: **Parakeet, Canary, Cohere, Moonshine, SenseVoice, GigaAM** (via ONNX Runtime), **Whisper** (GGML/whisper.cpp with Metal/Vulkan/CUDA), Whisperfile, and OpenAI. Feature-gated so you pull only the engines you need. The library S2B2S's STT half is built on, so its roadmap directly affects the app.

- **State:** Last commit **2026-04-08** · Release: commits only (crate `0.3.x`)
- **Links:** [GitHub](https://github.com/cjpais/transcribe-rs) · [crates.io](https://crates.io/crates/transcribe-rs)

### KoljaB/RealtimeSTT

**Python STT library for low-latency, always-listening apps:** VAD, fast transcription, optional realtime text updates, wake words, raw audio-stream access in a few lines. Default path uses `faster_whisper`; also ships native support for **Kroko/Banafo** (`kroko_onnx`) local streaming ASR. A common building block for DIY voice assistants.

- **State:** Last commit **2026-06-12** · Release **v1.0.2** (2026-05-31)
- **Links:** [GitHub](https://github.com/KoljaB/RealtimeSTT)

### istupakov/onnx-asr

**Pure-Python ASR on ONNX with minimal dependencies** (no PyTorch, Transformers, or FFmpeg). Runs from tiny IoT/edge devices to GPU servers across Windows/Linux/macOS on x86 + Arm, with CUDA/TensorRT/CoreML/DirectML/ROCm/WebGPU backends. Loads (incl. quantized) models from HF or local dirs, long-form via VAD, batch processing, token-level timestamps. A clean, lightweight ASR option.

- **State:** Last commit **2026-04-19** · Release **v0.11.0** (2026-03-23)
- **Links:** [GitHub](https://github.com/istupakov/onnx-asr) · [Docs](https://istupakov.github.io/onnx-asr/)

### k2-fsa/sherpa-onnx — the unified on-device STT/TTS C API

**The Swiss-army knife of on-device speech.** One C++/ONNX-Runtime stack covering speech recognition, **speech synthesis**, source separation, speaker ID/diarization/verification, spoken-language ID, audio tagging, VAD, keyword spotting, punctuation, and speech enhancement — with no internet. Runs on Android, iOS, Windows, macOS, Linux, HarmonyOS, Raspberry Pi, RISC-V, and **RK / Axera / Ascend / Qualcomm-QNN NPUs**, via ~12 languages. The backbone of many Android TTS/STT apps here and a strong candidate for the on-device engine in the S2B2S Android app.

- **State:** Last commit **2026-06-18** · Release **asr-models-qnn-binary-2** (2026-06-18; new Qualcomm-QNN NPU model binaries)
- **Links:** [GitHub](https://github.com/k2-fsa/sherpa-onnx) · [Android demos](https://github.com/k2-fsa/sherpa-onnx/tree/master/android)

### speechbrain/speechbrain

**A PyTorch toolkit for Conversational AI** — the research-grade foundation behind speech assistants, chatbots, and speech/text models, with broad recipes for ASR, speaker tasks, enhancement, and more. A training/research toolkit rather than a deployment runtime, but invaluable for building or fine-tuning custom models.

- **State:** Last commit **2026-06-15** · Release **v1.1.0** (2026-03-31)
- **Links:** [GitHub](https://github.com/speechbrain/speechbrain) · [Website](https://speechbrain.github.io/) · [Docs](https://speechbrain.readthedocs.io) · [Hugging Face](https://huggingface.co/speechbrain)

### ggml-org/whisper.cpp

**The dependency-free C/C++ port of OpenAI Whisper** that made on-device STT ubiquitous. Apple-Silicon-first (NEON/Accelerate/Metal/Core ML), plus AVX, Vulkan, NVIDIA CUDA, AMD ROCm, OpenVINO, Ascend NPU, and Moore Threads GPUs; integer quantization, zero runtime allocations, built-in VAD, clean C API. Ships **iOS and Android** examples. Underpins `transcribe-rs`, Transcribro, FUTO, and many others.

- **State:** Last commit **2026-06-19** · Release **v1.9.1** (2026-06-19, actively developed)
- **Links:** [GitHub](https://github.com/ggml-org/whisper.cpp) · [whisper.h C API](https://github.com/ggml-org/whisper.cpp/blob/master/include/whisper.h)

### SYSTRAN/faster-whisper

**Whisper reimplemented on CTranslate2** for up to ~4× speed at the same accuracy with lower memory, further improvable with 8-bit quantization (CPU + GPU). The default engine behind WhisperLive, RealtimeSTT, and many real-time pipelines.

- **State:** Last commit **2025-11-19** · Release **1.2.1** (2025-10-31)
- **Links:** [GitHub](https://github.com/SYSTRAN/faster-whisper)

### CrispStrobe/CrispASR

**One C++ binary, 28 ASR backends + 10 TTS engines + multilingual text translation, zero Python.** A whisper.cpp fork extended into a unified ggml speech engine: pick the backend on the CLI or let it auto-detect from the GGUF (Whisper, **Parakeet, Canary, Voxtral, Qwen3-ASR**…; TTS via Kokoro/Qwen3-TTS/Orpheus/Chatterbox/etc.). All backends compile to a **4.3 MB WebAssembly** build that runs client-side. Pairs with CrispTTS. Compelling for a single-binary, no-Python STT+TTS core.

- **State:** Last commit **2026-06-19** · Release **v0.7.2** (2026-06-15, actively developed)
- **Links:** [GitHub](https://github.com/CrispStrobe/CrispASR)

---

## 4. TTS — engines, models & desktop apps

### rishiskhare/parrot — Parrot

**Free, offline, private read-aloud for the desktop.** Highlight text in any app, press a shortcut, and hear it instantly. The model is only ~115 MB and runs on any modern CPU with no GPU; supports 9 languages. Directly parallels the S2B2S/CopySpeak read-aloud feature, with a notably small footprint and a Rust backend.

- **State:** Last commit **2026-04-28** · Release **v26.2.4** (2026-02-25)
- **Links:** [GitHub](https://github.com/rishiskhare/parrot) · [tryparrot.vercel.app](https://tryparrot.vercel.app)

### mrtozner/vox

**Local-first voice-AI _framework_ in Rust** wiring the whole loop: Mic → VAD (Silero) → STT (Whisper / Distil-Whisper / Sherpa-ONNX) → Speaker ID → your code → TTS (Kokoro / Piper / Qwen3 / Pocket / Chatterbox) → speaker. Adds experimental real-time speaker diarization (voice embeddings, auto-enrollment, persistent DB) and "Live Talk" barge-in voice chat. Referenced in S2B2S notes for its **voice-over-WebSocket** protocol design.

- **State:** Last commit **2026-04-12** · Release **v0.6.0** (2026-04-12)
- **Links:** [GitHub](https://github.com/mrtozner/vox) · [crates.io](https://crates.io/crates/vox)

### ai-joe-git/pocket-tts-server

**Real-time voice-cloning + chat server with an OpenAI-compatible API.** Clone any voice from ~20 s of audio and immediately chat with an LLM in that voice; drag-and-drop voice library with auto MP3/OGG/FLAC→WAV, configurable LLM backend, one-click Windows installer. Built on Kyutai's `pocket-tts` weights. A ready-made blueprint for the "talk in a cloned voice" path.

- **State:** Last commit **2026-05-26** · Release: commits only (README v1.0)
- **Links:** [GitHub](https://github.com/ai-joe-git/pocket-tts-server) · model → [kyutai/pocket-tts](https://huggingface.co/kyutai/pocket-tts)

### cool-japan/voirs — VoiRS

**Pure-Rust neural speech synthesis**, unifying the cool-japan crates (SciRS2/NumRS2/PandRS/TrustformeRS) into one memory-safe TTS stack. VITS + DiffWave models (MOS 4.4+), ≤0.3× RTF on CPUs / ≤0.05× on GPUs, streaming synthesis, full SSML, 20+ languages with pluggable G2P, and SafeTensors. Multi-platform: x86_64/aarch64/WASM/CUDA/Metal. Attractive for an all-Rust TTS engine matching the S2B2S/Tauri stack.

- **State:** Last commit **2026-03-26** · Release **0.1.0 RC-1** (2026-03-26)
- **Links:** [GitHub](https://github.com/cool-japan/voirs)

### danielclough/vibevoice-rs — VibeVoice-RS

**Rust implementation of VibeVoice TTS** with voice cloning and multi-speaker dialogue synthesis, GPU acceleration (Metal/CUDA), and realtime streaming. Cleanly split into crates: core `vibevoice`, `-cli`, `-server` (HTTP + SSE), `-web` (Leptos), and `-tauri` (desktop). The Tauri crate is a useful reference for embedding a TTS engine in a Tauri app like S2B2S.

- **State:** Last commit **2025-12-22** · Release **v0.1.2** (2025-12-22)
- **Links:** [GitHub](https://github.com/danielclough/vibevoice-rs)

### diodiogod/TTS-Audio-Suite

**Universal multi-engine TTS extension for ComfyUI** (evolved from the ChatterBox Voice project). One node graph spanning **15 engines** — ChatterBox, F5-TTS, Higgs Audio 2/v3, Step Audio EditX, MOSS-TTS, Echo-TTS, RVC voice conversion, and more — with runtime isolation for fragile legacy stacks and a modern Transformers 5 environment. Strong subtitle/SRT workflows (transcribe→SRT, rebuild from edits). A great catalog of current TTS engines to evaluate for S2B2S backends.

- **State:** Last commit **2026-06-17** · Release **v5.0.0** (2026-06-14; README header shows v5.1.1)
- **Links:** [GitHub](https://github.com/diodiogod/TTS-Audio-Suite) · origin → [ComfyUI_ChatterBox_SRT_Voice](https://github.com/diodiogod/ComfyUI_ChatterBox_SRT_Voice)

### hexgrad/kokoro — Kokoro-82M (inference library)

**The reference inference library for Kokoro-82M**, an open-weight TTS model with only 82 M parameters that punches well above its size, with Apache-licensed weights deployable anywhere. `pip install kokoro`. Kokoro is one of the most-used voices across this entire list (S2B2S, Vox, maise, soniqo, the Android apps), so the upstream lib is worth tracking.

- **State:** Last commit **2025-08-06** · Release: commits only (stable lib)
- **Links:** [GitHub](https://github.com/hexgrad/kokoro) · [HF model](https://huggingface.co/hexgrad/Kokoro-82M) · [PyPI](https://pypi.org/project/kokoro/)

### thewh1teagle/kokoro-onnx

**Kokoro TTS packaged for ONNX Runtime** — multi-language, near-real-time on macOS M1, lightweight (~300 MB, ~80 MB quantized), with downloadable v1.0 model + voices files. The ONNX build that on-device apps (e.g. Kokoro-82M-Android) actually ship. The practical path to running Kokoro on Android/edge without PyTorch.

- **State:** Last commit **2026-01-30** · Release **model-files-v1.1** (2025-03-01)
- **Links:** [GitHub](https://github.com/thewh1teagle/kokoro-onnx)

### rsxdalv/TTS-WebUI

**A single Gradio + React WebUI** wrapping a huge roster of TTS/audio extensions — Kokoro, Piper, XTTSv2, DIA, OpenVoice, StyleTTS2, GPT-SoVITS, CosyVoice, ParlerTTS, Bark, plus music/codec tools (ACE-Step, MusicGen, RVC, Demucs…). Docker + Colab, SillyTavern integration, an in-UI extension manager. A convenient bench for auditioning many engines before wiring one into S2B2S.

- **State:** Last commit **2026-05-14** · Release **1.5.1** (2026-05-14)
- **Links:** [GitHub](https://github.com/rsxdalv/TTS-WebUI)

### CrispStrobe/CrispTTS

**Python command-line TTS, German-focused but broad,** fronting many endpoints: Orpheus, Piper, OuteTTS, Kokoro (incl. ONNX), CSM, Edge, Coqui (XTTS/VITS), Chatterbox, IndexTTS, VoxCPM2, F5-TTS, plus EU-AI-Act-style audio watermarking/provenance. The TTS sibling to CrispASR.

- **State:** Last commit **2026-06-07** · Release **v0.3.0** (2026-05-30)
- **Links:** [GitHub](https://github.com/CrispStrobe/CrispTTS)

### kyutai-labs/pocket-tts — Pocket TTS (upstream)

**Tiny CPU-only TTS, ~100 M params**, that turns "TTS on the edge" into a `pip install` + function call — no GPU, no web API. ~200 ms to first audio chunk, ~6× faster than real-time on a MacBook Air M4 using just 2 CPU cores, with audio streaming, voice cloning, and multi-language support. The upstream model behind `pocket-tts-server` and `pocket-tts-unity`, and one of the S2B2S TTS backends ("Pocket").

- **State:** Last commit **2026-06-03** · Release **v2.1.0** (2026-05-04)
- **Links:** [GitHub](https://github.com/kyutai-labs/pocket-tts) · [Demo](https://kyutai.org/pocket-tts) · [HF](https://huggingface.co/kyutai/pocket-tts) · [Docs](https://kyutai-labs.github.io/pocket-tts/) · [Paper](https://arxiv.org/abs/2509.06926)

---

## 5. Combined voice assistants (STT + Brain + TTS) — desktop / server

> Reference designs for the full STT→Brain→TTS loop. Most are Python and target desktop/Pi/server, several with barge-in (interruption) handling.

### vndee/local-talking-llm

**The classic "build your own offline Jarvis."** Whisper (STT) + Ollama (LLM) + **Chatterbox** (TTS — voice cloning, emotion control, watermarking). Companion blog post explains the architecture end-to-end; a good didactic baseline for the loop.

- **State:** Last commit **2026-04-04** · Release: commits only
- **Links:** [GitHub](https://github.com/vndee/local-talking-llm) · [Article](https://blog.duy-huynh.com/build-your-own-voice-assistant-and-run-it-locally/)

### eauchs/speech-to-speech-pipeline

**Real-time, interruptible (barge-in) STT-LLM-TTS pipeline** optimized for Apple Silicon (MLX). Faster-Whisper (small int8) STT, any local OpenAI-compatible API (LM Studio/Ollama) via SSE for the LLM, and **MLX-Audio Kokoro** (4-bit) TTS — with the assistant's speech cut off the moment the user speaks again. Strong reference for low-latency barge-in design.

- **State:** Last commit **2025-10-19** · Release: commits only
- **Links:** [GitHub](https://github.com/eauchs/speech-to-speech-pipeline)

### m15-ai/Local-Voice

**Lightweight, wake-free, fully offline voice assistant for Raspberry Pi / Linux.** PyAudio + **Vosk** (STT) + **Piper** (TTS) + local LLMs via **Ollama** (`gemma2:2b`, `qwen2.5:0.5b`), with optional SoX noise FX and ALSA volume control. Modular Python. Reportedly sub-second STT and ~0.8–1.6 s TTS on a CPU laptop.

- **State:** Last commit **2025-05-13** · Release: commits only
- **Links:** [GitHub](https://github.com/m15-ai/Local-Voice)

### m15-ai/Faster-Local-Voice-AI

**Same author, tuned for an 8 GB no-GPU Ubuntu laptop, sub-second STT→TTS.** WebSocket client/server architecture, **Gemma3:1b** via Ollama, Vosk STT, Piper TTS, and **JACK/PipeWire** for low-latency I/O, with streaming responses and audio fading. (No interruption logic yet.) A good study in squeezing latency out of modest hardware.

- **State:** Last commit **2025-07-21** · Release: commits only
- **Links:** [GitHub](https://github.com/m15-ai/Faster-Local-Voice-AI)

### HaxxorCialtion/local-ASR-TTS-LLM-realtime

**High-performance client/server assistant** that puts the heavy models on a local GPU server and runs the client on a low-power embedded board (e.g. Orange Pi). **SenseVoice (FunASR)** STT + **Ollama Qwen2.5:7B FP8** + **index-tts-vllm** (high-quality TTS with voice cloning), TTFA under ~2 s, detailed per-stage performance monitoring, highly tunable multi-level VAD. (Bilingual README, Chinese + English.)

- **State:** Last commit **2025-10-18** · Release: commits only
- **Links:** [GitHub](https://github.com/HaxxorCialtion/local-ASR-TTS-LLM-realtime)

### cnbison/louchat — LouChat

**Chinese offline voice agent** with a main-process + worker-threads architecture: **Ollama (Qwen3:8B)** + **Vosk** (streaming ASR) + **Coqui TTS** + **Porcupine** wake-word + an **MCP plugin system** + a Gradio Web UI showing synchronized text/voice. Useful reference for wake-word + plugin/tool calling in a local loop.

- **State:** Last commit **2025-10-14** · Release: commits only
- **Links:** [GitHub](https://github.com/cnbison/louchat)

### kyutai-labs/unmute — Unmute

**Make any text LLM listen and speak** by wrapping it in Kyutai STT + Kyutai TTS, both tuned for low latency, over a real-time WebSocket backend/frontend. Works with whatever text LLM you like. A clean cascaded reference (and a counterpoint to Moshi's end-to-end approach).

- **State:** Last commit **2026-06-05** · Release: commits only
- **Links:** [GitHub](https://github.com/kyutai-labs/unmute) · [unmute.sh](https://unmute.sh) · models → [delayed-streams-modeling](https://github.com/kyutai-labs/delayed-streams-modeling)

---

## 6. Full-duplex speech-to-speech models

> A different paradigm from the cascaded loop: a single model that listens _and_ speaks at once (no explicit speaker turns), preserving prosody/emotion. Heavy today, but the direction worth watching for a future S2B2S "natural conversation" mode.

### kyutai-labs/moshi — Moshi

**The first real-time full-duplex spoken LLM** (~200 ms practical latency). It models its own and the user's audio as parallel streams plus a time-aligned text "inner monologue," using the **Mimi** streaming neural codec. Three inference stacks in one repo: **PyTorch** (research), **MLX** (on-device iPhone/Mac), and **Rust** (production, with `rustymimi` bindings). Fine-tuning via a separate repo.

- **State:** Last commit **2026-05-16** · Release **moshi-v0.2.12** (2026-01-08)
- **Links:** [GitHub](https://github.com/kyutai-labs/moshi) · [moshi.chat demo](https://moshi.chat) · [HF collection](https://huggingface.co/collections/kyutai/moshi-v01-release-66eaeaf3302bef6bd9ad7acd) · [fine-tune](https://github.com/kyutai-labs/moshi-finetune)

### kyutai-labs/delayed-streams-modeling — Kyutai STT & TTS

**The streaming framework formalizing Moshi/Hibiki ("Delayed Streams Modeling").** Instructions + examples for running Kyutai's standalone **streaming STT** and **TTS** models — the building blocks if you want Kyutai's low-latency speech components without the full duplex model. Underlies Unmute.

- **State:** Last commit **2026-01-26** · Release: commits only
- **Links:** [GitHub](https://github.com/kyutai-labs/delayed-streams-modeling) · [STT models (HF)](https://huggingface.co/collections/kyutai/speech-to-text-685403682cf8a23ab9466886) · [DSM pre-print](https://arxiv.org/abs/2509.08753)

### NVIDIA/personaplex — PersonaPlex

**Real-time full-duplex speech-to-speech with persona control** — text role prompts + audio voice-conditioning for a consistent persona, built on the Moshi architecture and weights. Trained on synthetic + real conversations for natural, low-latency interaction.

- **State:** Last commit **2026-03-02** · Release: commits only
- **Links:** [GitHub](https://github.com/NVIDIA/personaplex) · [model (HF)](https://huggingface.co/nvidia/personaplex)

---

## 7. Local LLM "brain" runtimes — desktop / server

### mudler/LocalAI

**Self-hosted, OpenAI-compatible inference server** for LLM, vision, image, and audio (incl. TTS/STT) workloads on local/on-prem hardware — a drop-in API replacement that lets S2B2S target one stable endpoint for many model types and providers. Large, active community project.

- **State:** Last commit **2026-06-19** · Release **v4.4.3** (2026-06-13, actively developed)
- **Links:** [GitHub](https://github.com/mudler/LocalAI)

### ggml-org/llama.cpp

**The C/C++ LLM inference engine** that S2B2S already pre-compiles for its brain (CUDA/Vulkan/CPU). Minimal-setup, dependency-free, runs GGUF models across an enormous hardware range with 1.5–8-bit quantization, an OpenAI-compatible `llama-server`, a built-in WebUI, MTP/speculative decoding for supported models, and first-class **Android** build support. The single most important upstream for the local-brain and on-device-LLM story.

- **State:** Last commit **2026-06-19** · Release **b9727** (2026-06-19, multiple releases/day)
- **Links:** [GitHub](https://github.com/ggml-org/llama.cpp) · [ggml](https://github.com/ggml-org/ggml)

---

## 8. On-device Android LLM inference engines

> The core of the _standalone Android app_ goal: running the "brain" locally on the phone. Mix of libraries (drop into your own app) and reference apps.

### ferranpons/Llamatik — Llamatik

**True Kotlin Multiplatform on-device AI** (Android, iOS, Desktop, JVM, WASM) behind a single Kotlin API — LLMs via **llama.cpp**, STT via **whisper.cpp**, image-gen via **stable-diffusion.cpp**. Offline-first, MIT-licensed, on Maven Central, with a JetBrains-IDE plugin. Notably advertises **Multi-Token Prediction (MTP) speculative drafting** for supported models (Qwen3.5, GLM-4), concurrent sessions, chat-template introspection, and fine-grained sampling. The closest fit to "STT + LLM (+image) as one cross-platform Kotlin library," and a strong candidate to anchor the S2B2S Android app.

- **State:** Last commit **2026-06-16** · Release **v1.8.1** (2026-06-16, actively developed)
- **Links:** [GitHub](https://github.com/ferranpons/Llamatik) · [llamatik.com](https://www.llamatik.com) · [Maven Central](https://central.sonatype.com/artifact/com.llamatik/library) · [JetBrains plugin](https://plugins.jetbrains.com/plugin/31304)

### Aatricks/llmedge — llmedge

**Lightweight Android library for running GGUF models fully on-device** via llama.cpp (JNI). Features: HF model downloads + caching, **low-end presets** (Microsoft BitNet b1.58 2B4T, SmolVLM2-256M), **on-device Safetensors→GGUF conversion** with optional Q8_0/Q4_K_M/IQ2_BN quantization, native KV-cache reuse, streaming/blocking generation, separate prompt vs generation thread tuning, plus evolving Vision/VLM, RAG, and OCR flows. (Credits Shubham Panchal / SmolChat.) Useful for the "download + quantize + run a GGUF on the phone" pipeline.

- **State:** Last commit **2026-06-04** · Release **V0.4.0beta** (2026-06-03)
- **Links:** [GitHub](https://github.com/Aatricks/llmedge) · [examples](https://github.com/Aatricks/llmedge-examples) · [site](https://aatricks.github.io/llmedge/)

### shubham0204/SmolChat-Android — SmolChat

**Run any GGUF SLM/LLM locally on Android.** Loads/executes GGUF models through llama.cpp via JNI (a small `smollm.cpp` class), renders Markdown responses (Markwon + Prism4j), keeps chat history on-device. Widely referenced as a clean reference implementation; on Google Play, GitHub Releases, and Obtainium.

- **State:** Last commit **2026-06-17** · Release **v15** (2026-04-17)
- **Links:** [GitHub](https://github.com/shubham0204/SmolChat-Android) · [Google Play](https://play.google.com/store/apps/details?id=io.shubham0204.smollmandroid)

### Siddhesh2377/llama.cpp-android — Tool-Neuron GGML backend

**A production llama.cpp fork stripped to the CPU backend and ARM-optimized for Android.** All GPU backends removed; on top sit custom engine layers used by the Tool-Neuron app: `GGMLEngine` (load/generate/KV-cache), `ThreadEngine` (big.LITTLE-aware power_saving/balanced/performance modes), a **VLM engine** (vision/audio, 20+ architectures), and a **RAG engine** (late chunking, binary-quantized retrieval). Uses NEON/i8mm/dotprod/fp16/bf16 and **KleidiAI** ARM micro-kernels. A good reference for a tuned CPU-only Android engine with VLM + RAG.

- **State:** Last commit **2026-05-16** · Release: commits only
- **Links:** [GitHub](https://github.com/Siddhesh2377/llama.cpp-android) · app → [ToolNeuron](https://github.com/Siddhesh2377/ToolNeuron)

### Siddhesh2377/ToolNeuron — ToolNeuron

**On-device AI suite for Android** — no Google Play services, no telemetry, no cloud. Chat against any GGUF model (streaming, multi-turn, optional thinking mode, per-turn tok/s + TTFT + peak-memory), **vision** via colocated `mmproj` projectors, **RAG** over many document formats (PDF/DOCX/XLSX/PPTX/ODT/EPUB/RTF/MD/HTML/JSON/XML/CSV/TXT), and a **voice loop** through sherpa-onnx with sentence-chunked streaming TTS, plus a plugin runtime — all runnable "with the radio off." Built on the Tool-Neuron GGML backend above. A close cousin of the standalone Android app S2B2S wants to become.

- **State:** Last commit **2026-05-18** · Release **Tool Neuron v3.0** (2026-05-16)
- **Links:** [GitHub](https://github.com/Siddhesh2377/ToolNeuron) · [Play Store](https://play.google.com/store/apps/details?id=com.dark.tool_neuron)

### FilipFan/PolyEngineInfer — Poly Engine Inference

**One experimental Android app that runs multiple inference engines side by side** — **llama.cpp, ExecuTorch, LiteRT, and ONNX** — auto-selecting the engine from the model file's extension/structure, with adjustable top-k/top-p/temperature and detailed metrics (TTFT, prefill speed, decode speed). The ideal harness for benchmarking which engine to ship in the S2B2S Android app.

- **State:** Last commit **2026-03-29** · Release: commits only
- **Links:** [GitHub](https://github.com/FilipFan/PolyEngineInfer)

### alibaba/MNN — MNN / MNN-LLM

**A blazing-fast, lightweight inference engine battle-tested by Alibaba**, powering high-performance on-device LLMs and edge AI. Converts from TensorFlow/Caffe/ONNX/TorchScript; FP16/Int8 quantization; CPU + GPU (Metal/OpenCL/Vulkan) on iOS 8+/Android 4.3+. The **MNN-LLM** Android chat app supports Qwen (incl. Qwen3.5), Gemma, Llama, DeepSeek, Phi and more; the **MNN TaoAvatar** app runs a full **offline 3D-avatar conversation** (LLM + ASR + TTS + A2BS + NNR) on-device. A heavyweight, full-featured on-device option with its own avatar precedent for S2B2S's 3D-avatar overlay.

- **State:** Last commit **2026-06-18** · Release **3.6.0** (2026-06-16)
- **Links:** [GitHub](https://github.com/alibaba/MNN) · [MNN-LLM Android](https://github.com/alibaba/MNN/blob/master/apps/Android/MnnLlmChat/README.md) · [MNN TaoAvatar](https://github.com/alibaba/MNN/blob/master/apps/Android/Mnn3dAvatar/README.md)

### mlc-ai/mlc-llm — MLC LLM

**Universal LLM deployment engine with ML compilation (TVM).** Compiles models to a unified high-performance `MLCEngine` with an OpenAI-compatible API across REST/Python/JS/**iOS/Android**, plus **WebGPU/WASM** in the browser and AMD/NVIDIA/Apple/Intel GPUs. The compiler-based route to portable on-device inference.

- **State:** Last commit **2026-05-11** · Release **v0.19.0** (2025-02-11)
- **Links:** [GitHub](https://github.com/mlc-ai/mlc-llm) · [Docs](https://llm.mlc.ai/docs) · [Blog](https://blog.mlc.ai/)

### pytorch/executorch — ExecuTorch

**PyTorch's official end-to-end on-device runtime** for mobile/edge (down to microcontrollers), part of PyTorch Edge. ~50 KB base runtime, 12+ hardware backends (Apple, **Qualcomm QNN/Hexagon**, ARM, MediaTek, Vulkan…), **Quantization-Aware Training (QAT)** export via torchao, memory planning, selective build, and multimodal LLM runners (text/image/audio). Powers on-device AI in Instagram/WhatsApp/Quest/Ray-Ban Meta. Its **LlamaDemo** Android app shows Llama-3.2 quantized over CPU (XNNPACK) or Qualcomm QNN. The QAT + NPU-delegation story is directly relevant to your QAT-on-phone goal.

- **State:** Last commit **2026-06-19** (actively developed) · Release: versioned via docs (latest CI tags on GitHub)
- **Links:** [GitHub](https://github.com/pytorch/executorch) · [executorch.ai](https://executorch.ai/) · [Docs](https://pytorch.org/executorch/stable/index.html)

### software-mansion/react-native-executorch — React Native ExecuTorch

**Software Mansion's React Native wrapper for ExecuTorch**, bringing on-device LLMs/vision/speech (Llama 3.x, Qwen 3, Phi-4-mini, LiquidAI LFM2…) to RN apps with familiar PyTorch export. The RN bridge to on-device inference if the Android app goes RN.

- **State:** Last commit **2026-06-18** · Release **v0.9.2** (2026-06-17, actively developed)
- **Links:** [GitHub](https://github.com/software-mansion/react-native-executorch)

### JackZeng0208/llama.cpp-android-tutorial

**A practical tutorial for running llama.cpp on Android via the Adreno GPU (OpenCL).** Covers building llama.cpp + `llama-cpp-python` with a custom Adreno/OpenCL backend for Snapdragon SoCs (8 Gen 1/2/3, 8 Elite), with hardware notes (e.g. OnePlus 13 / SD 8 Elite). Good when you want GPU offload on Qualcomm phones rather than CPU-only.

- **State:** Last commit **2026-03-21** · Release: commits only
- **Links:** [GitHub](https://github.com/JackZeng0208/llama.cpp-android-tutorial)

---

## 9. Google AI Edge stack (LiteRT · Gemma · MTP · QAT)

> Google's engine is the one its own docs now point to for edge, and it's where **Gemma-4 MTP + QAT** lands first — highly relevant to the `gemma_4_qat_mtp_e2b/` experiments already in the S2B2S repo.

### google-ai-edge/LiteRT-LM

**Google's production-ready orchestration layer to run LLMs with LiteRT** across platforms — the recommended successor to the now maintenance-only MediaPipe LLM Inference API. Broad model support (Gemma, Llama, Phi-4, Qwen…), powers GenAI in Chrome/Chromebook Plus/Pixel Watch. Recent releases add **Gemma-4 12B**, an **OpenAI-API-compatible server**, a Swift package for macOS/iOS, **Agent-skill support** to scaffold a standalone Android demo, and a CLI that runs **Gemma-4-E4B with MTP** (`--enable-speculative-decoding`) on Linux/macOS/Windows/Raspberry Pi. The most direct route to MTP + Gemma-4-QAT on-device.

- **State:** Last commit **2026-06-19** · Release **v0.14.0-alpha.0** (2026-06-18, actively developed)
- **Links:** [GitHub](https://github.com/google-ai-edge/LiteRT-LM) · [Product site](https://ai.google.dev/edge/litert-lm) · [CLI guide](https://ai.google.dev/edge/litert-lm/cli)

### google-ai-edge/litert — LiteRT

**Google's on-device runtime for high-performance ML & GenAI** (the successor to TensorFlow Lite). Adds a Compiled Model API with automated accelerator selection, **unified NPU acceleration across chipset vendors**, and faster GPU via ML Drift. The general-purpose runtime under LiteRT-LM and MediaPipe.

- **State:** Last commit **2026-06-19** · Release **v2.1.5** (2026-06-08, actively developed)
- **Links:** [GitHub](https://github.com/google-ai-edge/litert) · [Docs](https://ai.google.dev/edge/litert)

### google-ai-edge/gallery — Google AI Edge Gallery

**Google's open showcase app for running open-source LLMs fully on-device** (Android + iOS + macOS), now featuring **Gemma 4** with thinking mode, an "Ask Image" multimodal mode, real-time **on-device transcription ("Audio Scribe")**, a Prompt Lab, FunctionGemma-270m tool/agent skills, and model benchmarking. The canonical reference app for the LiteRT-LM API and a sandbox for Gemma-4 on a phone.

- **State:** Last commit **2026-06-19** · Release **1.0.15** (2026-05-21, actively developed)
- **Links:** [GitHub](https://github.com/google-ai-edge/gallery) · [Google Play](https://play.google.com/store/apps/details?id=com.google.ai.edge.gallery)

### hung-yueh/react-native-litert-lm

**High-performance on-device LLM inference for React Native, powered by LiteRT-LM + Nitro Modules**, optimized for **Gemma 4**. Native Swift bridge (iOS) and stateless Kotlin/JSI bridge (Android), a **zero-copy multimodal API** (image/audio mapped straight to the engine), **speculative decoding via built-in MTP heads**, function/tool calling, GPU acceleration (Metal / OpenCL on Pixel), streaming, and OS-level memory metrics. Demoed running **Gemma-4 E2B on a Galaxy S22 (SD 8 Gen 1, 4 GB)**. The RN path to LiteRT-LM + MTP on the phone.

- **State:** Last commit **2026-06-14** · Release **0.4.2** (2026-06-02)
- **Links:** [GitHub](https://github.com/hung-yueh/react-native-litert-lm)

---

## 10. NPU / low-bit / specialized on-device engines

> Engines aimed at squeezing larger or faster models onto the phone via NPUs, sparsity, or low-bit kernels — the frontier for making a capable local brain feel instant.

### UbiquitousLearning/mllm — mllm

**Fast and lightweight multimodal LLM inference engine for mobile and edge.** Pure C++ core with an in-app client/server design; supports Qwen3 / Qwen3-VL / Qwen3.5 with W4A16 / W8A8 serving, **Qualcomm QNN AOT full-graph NPU execution**, an **Ascend NPU** backend, and Jetson CUDA paths. Reports up to ~3.12× prefill speedup for Qwen3-VL-2B W8A8 on AGX Orin. The reference for getting multimodal LLMs onto the **Hexagon NPU**.

- **State:** Last commit **2026-06-09** · Release **MLLM-V2 V2.0.0** (2026-02-16)
- **Links:** [GitHub](https://github.com/UbiquitousLearning/mllm) · [Docs](https://ubiquitouslearning.github.io/mllm/) · [QNN AOT guide](https://ubiquitouslearning.github.io/mllm/qnn_backend/aot_execute.html)

### SJTU-IPADS/PowerInfer — PowerInfer / PowerInfer-2

**A CPU/GPU LLM inference engine exploiting activation locality.** **PowerInfer-2** is the smartphone-targeted framework (with TurboSparse models) using NPU + activation sparsity to run very large models on phones; the team also ships SmallThinker models and the "Tiiny AI Pocket Lab." The research-forward route to large-model-on-phone via sparsity.

- **State:** Active project (commit feed unavailable at fetch time) · Release: commits only
- **Links:** [GitHub](https://github.com/SJTU-IPADS/PowerInfer) · [PowerInfer-2 paper](https://arxiv.org/abs/2406.06282)

### microsoft/T-MAC — T-MAC

**LUT-based low-bit GEMM kernels** ("CPU renaissance via table lookup") that accelerate 1/2/4-bit quantized models of (almost) any architecture in GPTQ format, integrated into a llama.cpp fork, with prefill speedups and a **t-man** path that extends the idea to NPUs. Powers Microsoft's BitNet. Relevant for fast BitNet/low-bit inference on edge CPUs.

- **State:** Last commit **2025-06-03** · Release **T-MAC 1.0.0a5** (2025-05-27)
- **Links:** [GitHub](https://github.com/microsoft/T-MAC) · related → [microsoft/BitNet](https://github.com/microsoft/BitNet)

### microsoft/onnxruntime-genai — ONNX Runtime GenAI

**The generative-AI loop for ONNX Runtime** — pre/post-processing, inference, logits processing, search/sampling, KV-cache management, and grammar/tool-calling — giving an easy, performant way to run LLMs on device. Powers Foundry Local, Windows ML, and the VS Code AI Toolkit. Supports Gemma, Llama, Mistral, Phi (language + vision), Qwen (language + vision), SmolLM3, DeepSeek, gpt-oss, Whisper and more, via Python/C#/C++/Java on Linux/Windows/Mac/**Android** with CPU/CUDA/DirectML.

- **State:** Last commit **2026-06-18** · Release **v0.14.0** (2026-05-29, actively developed)
- **Links:** [GitHub](https://github.com/microsoft/onnxruntime-genai) · [Docs](https://onnxruntime.ai/docs/genai)

### onnxruntime/onnxruntime-qnn — ONNX Runtime QNN Execution Provider

**A plugin execution provider that brings Qualcomm hardware acceleration to ONNX Runtime** via the Qualcomm AI Runtime SDK (QAIRT), enabling high-performance inference on Snapdragon NPUs/DSPs. Since v2.0.0 it ships as a standalone plugin (`onnxruntime-qnn`) that works with any standard ORT install — no custom build. Maintained by Qualcomm (the general ORT project lives at [microsoft/onnxruntime](https://github.com/microsoft/onnxruntime)). The supported route to the Hexagon NPU from the ONNX side.

- **State:** Last commit **2026-06-19** · Release **QNN EP v2.2.0** (2026-05-26, actively developed)
- **Links:** [GitHub](https://github.com/onnxruntime/onnxruntime-qnn) · [Plugin EP docs](https://onnxruntime.ai/docs/execution-providers/plugin-ep-libraries/) · [QAIRT SDK](https://qpm.qualcomm.com/#/main/tools/details/Qualcomm_AI_Runtime_SDK)

---

## 11. Android STT — keyboards & voice input

> On-device dictation on the phone — the STT half of the standalone Android app, as keyboards/IMEs or system speech providers.

### futo-org/android-keyboard — FUTO Keyboard

**An offline, privacy-respecting Android keyboard** (a heavily modified fork of AOSP LatinIME) with **voice input built in** (Whisper-based). The recommended way to use FUTO's on-device dictation; layouts repo is Apache-licensed while the app uses the FUTO Source First License.

- **State:** Last commit **2026-06-18** · Release **v0.1.29.1-rc2** (2026-06-18, actively developed)
- **Links:** [GitHub](https://github.com/futo-org/android-keyboard) · [keyboard.futo.org](https://keyboard.futo.org/) · [layouts](https://github.com/futo-org/futo-keyboard-layouts)

### futo-org/voice-input — FUTO Voice Input

**A standalone Android speech-to-text app** that plugs into third-party keyboards/apps via the generic STT APIs (RECOGNIZE_SPEECH intent and IME voice subtype). Whisper-based, offline. Development has largely shifted into the FUTO Keyboard app, but it remains available for use with other keyboards.

- **State:** Last commit **2025-09-16** · Release **v1.3.6** (2025-07-19)
- **Links:** [GitHub](https://github.com/futo-org/voice-input) · [voiceinput.futo.org](https://voiceinput.futo.org/)

### soupslurpr/Transcribro — Transcribro

**A private, on-device speech-recognition keyboard and service for Android** using **whisper.cpp** + **Silero VAD**. Works as a voice keyboard and as a system-wide STT provider other apps can call. Currently English-only (more languages planned). Distributed via the Accrescent app store and GitHub releases.

- **State:** Last commit **2025-08-29** · Release **7** (2025-08-07)
- **Links:** [GitHub](https://github.com/soupslurpr/Transcribro) · [Accrescent](https://accrescent.app/app/dev.soupslurpr.transcribro)

### notune/android_transcribe_app — Offline Voice Input (Android)

**An offline, privacy-focused STT tool for Android built in Rust on top of `transcribe-rs`** (so it's directly in the S2B2S toolchain). Tap the mic on the keyboard you already use and your speech is transcribed on-device with the **Parakeet TDT** model; also includes live subtitles for any audio/video and an optional dedicated voice keyboard. 25 languages, nothing leaves the phone. A near-perfect reference for wiring `transcribe-rs`/Parakeet into an Android build.

- **State:** Last commit **2026-06-12** · Release **v0.1.17** (2026-06-12, actively developed)
- **Links:** [GitHub](https://github.com/notune/android_transcribe_app) · [Google Play](https://play.google.com/store/apps/details?id=dev.notune.transcribe) · engine → [transcribe-rs](https://github.com/cjpais/transcribe-rs)

### gabrimatic/local-whisper — Local Whisper

**Local-first dictation for macOS, iOS, and Android** — recorder app + keyboard with model packs and history kept on-device. On Android it records local WAV and transcribes through **`sherpa_onnx`** with **Parakeet-TDT v3 INT8 ONNX** as the default pack and **Qwen3-ASR 0.6B INT8 ONNX** as the broader multilingual pack; iOS uses WhisperKit/Core ML. No cloud speech fallback, no account, no telemetry. A clean multi-platform on-device STT design.

- **State:** Last commit **2026-06-16** · Release **v1.6.14** (2026-06-16, actively developed)
- **Links:** [GitHub](https://github.com/gabrimatic/local-whisper)

### alex-vt/WhisperInput — WhisperInput

**An experimental offline voice-input panel & keyboard with punctuation for Android,** powered by Whisper + Kõnele components. Works as a voice keyboard (IME), a voice input panel, or an assistant app, with on-device recognition. (Older project; English-focused.)

- **State:** Last commit **2023-03-24** · Release: commits only
- **Links:** [GitHub](https://github.com/alex-vt/WhisperInput)

### MichaelMcCulloch/WhisperVoiceKeyboard — Whisper-based Voice Keyboard

**An early integration of Whisper into an Android keyboard** using a `whisper.tflite` model with a Rust + NDK build. Predates whisper.cpp integration (the README notes it should be rebuilt on whisper.cpp), so it's mainly of historical/reference interest for the TFLite path.

- **State:** Last commit **2023-03-13** · Release **0.1.4** (2022-12-27)
- **Links:** [GitHub](https://github.com/MichaelMcCulloch/WhisperVoiceKeyboard)

---

## 12. Android TTS — engines & apps

> On-device read-aloud on the phone — the TTS half of the standalone Android app. Several register as the _system_ TTS engine, so any app's `TextToSpeech` API can use them.

### woheller69/ttsEngine — SherpaTTS

**An Android _system_ TTS engine based on Next-gen Kaldi (sherpa-onnx),** using **Piper** or **Coqui** voices. Because it registers as a system TTS service, it works in any app that uses the standard Android TTS API. On F-Droid. (Note: the README warns it may stop working on certified Android devices after Google's 2026/2027 developer-identity requirement.)

- **State:** Last commit **2026-05-28** · Release **V3.2** (2026-06-04, actively developed)
- **Links:** [GitHub](https://github.com/woheller69/ttsEngine) · [F-Droid](https://f-droid.org/packages/org.woheller69.ttsengine/)

### siva-sub/NekoSpeak — NekoSpeak

**A high-performance, 100% offline on-device TTS engine for Android** that bridges modern AI voice synthesis to the standard Android TTS API. Multi-engine with an **OmniVoice** engine and **Misaki G2P** ports; motivated by accessibility (custom voices) and natural audiobook reading (ReadEra/Librera/MoonReader compatibility). Plans to experiment with quantized ONNX Qwen3-TTS.

- **State:** Last commit **2026-05-19** · Release **v1.5.0** (2026-04-15)
- **Links:** [GitHub](https://github.com/siva-sub/NekoSpeak)

### Mobile-Artificial-Intelligence/maise — Maise

**An open-source Android speech engine providing on-device TTS _and_ ASR.** The TTS side is a standard Android system TTS service (works out of the box with any app's `TextToSpeech` API); the ASR side is a `RecognitionService` for the `SpeechRecognizer` API. All processing runs on **ONNX Runtime**: text normalization → **OpenPhonemizer** → **Kokoro** synthesis → streaming 24 kHz PCM playback via a producer/consumer pipeline. From the team behind the MAID app. A strong reference for shipping both system-level TTS and ASR.

- **State:** Last commit **2026-03-09** · Release **v1.0.0** (2026-02-25)
- **Links:** [GitHub](https://github.com/Mobile-Artificial-Intelligence/maise) · [OpenPhonemizer](https://github.com/NeuralVox/OpenPhonemizer) · related app → [maid](https://github.com/Mobile-Artificial-Intelligence/maid)

### soniqo/speech-android — Speech Android

**An on-device speech SDK for Android** (Kotlin SDK + JNI + demo app) powered by **ONNX Runtime** and the shared `speech-core` engine. Bundles **speech recognition (114 languages, Parakeet), text-to-speech (8 languages, Kokoro), VAD (Silero), and noise cancellation (DeepFilterNet3)** — all local, nothing leaves the device. The C++ engine + ONNX wrappers live in `speech-core`; an Apple counterpart is `speech-swift`. A clean, packaged full-speech SDK to study or reuse.

- **State:** Last commit **2026-06-05** · Release **v0.0.9** (2026-05-10)
- **Links:** [GitHub](https://github.com/soniqo/speech-android) · engine → [speech-core](https://github.com/soniqo/speech-core) · Apple → [speech-swift](https://github.com/soniqo/speech-swift) · [models (HF)](https://huggingface.co/collections/aufklarer/speech-android-models-69bb8a156cac0b96a2247f26)

### CodeBySonu95/VoxSherpa-TTS — VoxSherpa TTS

**Studio-quality offline neural TTS for Android** running two on-device engines — **Kokoro-82M** and **Piper** — via **sherpa-onnx**, with Hindi/English/British/Japanese/Chinese + many more. Listed in the official sherpa-onnx README; available on Google Play.

- **State:** Last commit **2026-06-16** · Release **piper-en-fp32** (2026-06-02, actively developed)
- **Links:** [GitHub](https://github.com/CodeBySonu95/VoxSherpa-TTS) · [Google Play](https://play.google.com/store/apps/details?id=com.CodeBySonu.VoxSherpa)

### puff-dayo/Kokoro-82M-Android — Kokoro-82M-Android

**A minimal Android demo app for the Kokoro-82M TTS model in int8 quantization,** using `thewh1teagle/kokoro-onnx`. Includes a voice-style mixer; intentionally a simple on-device inference demo rather than a full product. A good starting point for embedding Kokoro on Android.

- **State:** Last commit **2025-02-05** · Release **New UI and a voice style mixer** (2025-02-05)
- **Links:** [GitHub](https://github.com/puff-dayo/Kokoro-82M-Android) · model → [kokoro-onnx](https://github.com/thewh1teagle/kokoro-onnx)

### lookbe/pocket-tts-unity — Pocket TTS for Unity

**A Unity 6 integration of Kyutai's Pocket-TTS** for **Windows and Android**, via ONNX Runtime Unity (asus4). Ships profiling logs (AR/Flow/Mimi timings, real-time ratio) and mobile tuning guidance for balancing stutter vs latency vs quality. Useful if any S2B2S surface is built in Unity, or as a reference for Pocket-TTS on Android.

- **State:** Last commit **2026-05-01** · Release **0.0.2** (2026-02-05)
- **Links:** [GitHub](https://github.com/lookbe/pocket-tts-unity) · model → [kyutai-labs/pocket-tts](https://github.com/kyutai-labs/pocket-tts)

---

## 13. Android voice assistants & full AI suites

### jegly/Box — Box

**Billed as a fully offline, client-side AI suite for Android.** Distributed via Obtainium with separate builds for stock Android and custom ROMs (GrapheneOS/LineageOS/CalyxOS, no Google services) and an in-app updater. A useful reference for packaging/distribution of a privacy-first on-device AI app across ROM types.

- **State:** Last commit **2026-06-19** · Release **Box v3.2.0** (2026-06-19, actively developed)
- **Links:** [GitHub](https://github.com/jegly/Box) · [jegly.xyz](https://jegly.xyz)

### Open-LLM-VTuber/Open-LLM-VTuber — Open-LLM-VTuber

**Talk to any LLM hands-free with voice interaction, voice interruption, and a Live2D talking face, running locally across platforms.** Modular STT/LLM/TTS backends; a v2.0 rewrite is in planning. (The personal lab repo `t41372/Open-LLM-VTuber` now redirects here.) A reference for the "animated avatar + local voice loop" experience S2B2S is exploring with its 3D avatar.

- **State:** Last commit **2026-05-15** · Release **v1.2.1** (2025-08-26)
- **Links:** [GitHub](https://github.com/Open-LLM-VTuber/Open-LLM-VTuber) · [Docs](https://open-llm-vtuber.github.io/docs/quick-start)

---

## 14. Cross-platform voice I/O studios

### jamiepine/voicebox — Voicebox

**"The open-source AI voice studio" — the full voice I/O stack running locally.** Clone any voice, generate speech, dictate into any app, and talk to agents in voices you own. Positioned as a complete local voice-input/output toolkit (the same STT + TTS + agent surface S2B2S targets, as a single desktop app).

- **State:** Last commit **2026-04-26** · Release **v0.5.0** (2026-04-25)
- **Links:** [GitHub](https://github.com/jamiepine/voicebox) · [DeepWiki](https://deepwiki.com/jamiepine/voicebox)

---

## 15. Upstream models & shared dependencies

These aren't standalone apps, but they're the **shared building blocks** referenced again and again above. Tracking them upstream helps when an integrated app lags behind a model release.

- **rhasspy/piper** — fast, local neural TTS (ONNX/VITS) used by Handy-likes, SherpaTTS, the Pi assistants, S2B2S, and more → <https://github.com/rhasspy/piper> · voice samples → <https://rhasspy.github.io/piper-samples/>
- **resemble-ai/chatterbox** — Chatterbox TTS (0.5B, voice cloning + emotion + watermarking), used by local-talking-llm, Vox, TTS-Audio-Suite → <https://github.com/resemble-ai/chatterbox>
- **alphacephei/vosk-api** — Vosk offline streaming ASR (lightweight, many languages), the STT in several Pi/desktop assistants here → <https://github.com/alphacephei/vosk-api> · <https://alphacephei.com/vosk/>
- **NVIDIA NeMo / Parakeet & Canary** — the Parakeet TDT and Canary ASR model families used by transcribe-rs, CrispASR, the Android STT apps, and many real-time transcribers → models on <https://huggingface.co/nvidia>
- **soniqo/speech-core** — the shared C++ + ONNX pipeline engine (Silero VAD, Parakeet STT, Kokoro TTS, DeepFilterNet3) behind `speech-android`/`speech-swift` → <https://github.com/soniqo/speech-core>
- **kyutai-labs/hibiki** — streaming/simultaneous speech translation built on the Moshi multi-stream architecture → in the Moshi/DSM repos under <https://github.com/kyutai-labs>
- **Aatricks/llmedge-examples** — sample usage for the llmedge Android library → <https://github.com/Aatricks/llmedge-examples>
- **ggml-org/ggml** — the tensor library underpinning llama.cpp and whisper.cpp → <https://github.com/ggml-org/ggml>

**On the model side (MTP + QAT, your stated interest):** Gemma 4 ships both **Multi-Token Prediction (MTP)** drafters and **Quantization-Aware Training (QAT)** checkpoints (including a quantization format specialized for mobile), runnable on-device through **LiteRT-LM** and **llama.cpp**, with quantizing/fine-tuning tooling in **unslothai/unsloth** (which publishes `mtp-` prefixed and QAT-derived GGUFs). Qwen3.x-class models also expose MTP.

- **unslothai/unsloth** — local run + train studio; MTP/QAT GGUFs and fine-tuning → <https://github.com/unslothai/unsloth> · last commit **2026-06-19** · release **GLM 5.2 + Model Hub + 3x longer contexts** (2026-06-19) · [docs](https://unsloth.ai/docs)

---

## 16. Curated "awesome" lists

### stevelaskaridis/awesome-mobile-llm

**The most complete index of LLMs and studies targeted at mobile/embedded hardware** — sections for mobile-first LLMs, on-device deployment infrastructure, benchmarking, mobile-specific optimizations, applications, multimodal LLMs, on-device training, and efficiency surveys. Mine it for anything not covered above.

- **State:** Last commit **2026-05-31** (README "last update: 1st June 2026") · Release: commits only
- **Links:** [GitHub](https://github.com/stevelaskaridis/awesome-mobile-llm)

### jeho-lee/Awesome-On-Device-AI-Systems

**Bridges systems-research papers with practical deployment frameworks** for on-device AI: inference engines (general ML, LLM/GenAI specialized, and vendor NPU/DSP SDKs — Qualcomm QNN, etc.), plus research on LLM inference on mobile SoCs, processor characterization, compiler-based optimization, and attention acceleration. Great for the NPU/low-bit frontier.

- **State:** Last commit **2026-06-02** · Release: commits only
- **Links:** [GitHub](https://github.com/jeho-lee/Awesome-On-Device-AI-Systems)

---

_Compiled 19 June 2026. Dates reflect public repo activity at fetch time and will drift — re-check before relying on "latest release." Descriptions paraphrased from project READMEs/About pages; see each linked repo for authoritative details and licenses._
