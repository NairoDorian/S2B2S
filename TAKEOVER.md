# S2B2S Agent Takeover & Living Memory Protocol

> **Purpose:** This file is the official living handoff document between sequential AI coding agents.  
> **Rule of Maintenance:** The section headers in this file **NEVER** change. Every agent taking over or finishing a turn must update the *content* beneath the headers following the lifecycle rules defined below.

---

## 1. Living Protocol (Rules for Future Agents)

1. **Section 2 (`Active Session Pass-Over`) is TRANSIENT:**
   - On every session end or handoff, **completely replace or prune** the text under Section 2.
   - Remove obsolete notes, failed temporary attempts, or resolved bug notes.
   - Keep only the exact immediate build status, active work in progress, and next immediate action.

2. **Section 3 (`Project Memory & Durable Knowledge`) is ACCUMULATING:**
   - Add new verified facts, model quirks, hardware fixes, and architectural decisions.
   - Refine, synthesize, or modify existing notes as understanding deepens (prevent unbounded bloat).

3. **Section 4 (`Sequential Roadmap & Task Tracker`) is STATEFUL:**
   - Mark completed tasks with `[x]`, in-progress tasks with `[/]`, and pending tasks with `[ ]`.
   - Never skip ahead across milestones unless unblocked.

4. **Section 5 (`Subsystem Reference & Pre-Commit Routine`) is FIXED SCHEMA:**
   - Update file paths only when files are created, renamed, or refactored.

---

## 2. Active Session Pass-Over (Transient State)

### Immediate Status & Build Health
- **App Version:** `0.1.4` (cargo, package.json, tauri.conf.json in sync).
- **Build Status:** All 319 Rust unit tests pass (`cargo test`). `bun run validate` passes (23 languages synced with 793 keys, TypeScript clean, ESLint/Prettier clean, Cargo check clean).
- **Backend Runtime:** Multi-STT default merge prompt updated to `"Merge and Clean"`. Model path resolution fixed for HF cache vs local models.
- **Frontend Theme:** Golden style implemented (`--color-logo-primary: #f59e0b` / `#d97706`). All pink removed. Bottom status bar brain emoji replaced with `src/assets/icon.png`. 3D Avatar shaders updated to gold.

### Current Task in Progress
- Planning and preparation for **Milestone 1: AI Selection Transformation & Dynamic Context Variables**.

### Next Immediate Action for Takeover
1. Begin **Milestone 1 Task 1.1**: Add `ai_replace_selection` action in [`src-tauri/src/actions.rs`](file:///src-tauri/src/actions.rs) and simulate selection capture via Enigo in [`src-tauri/src/clipboard.rs`](file:///src-tauri/src/clipboard.rs).
2. Wire global shortcut for AI Selection Edit in [`src-tauri/src/shortcut/handler.rs`](file:///src-tauri/src/shortcut/handler.rs).

### Active Traps & Blockers
- **None currently.** Build directory locks release normally. Keep LLM calls pointing strictly to local `llama-server.exe` (port 8001/8080) with Gemma 4 or Qwen models.

---

## 3. Project Memory & Durable Knowledge (Accumulating)

### STT & transcribe.cpp Invariants
- **Local Engine**: `transcribe.cpp` uses GGUF models. Primary models: Nemotron 3.5 Streaming (Vulkan/CUDA cache-aware), Parakeet TDT 0.6B v3, Whisper Turbo, SenseVoice.
- **Multi-STT Slot Resolution**: Model paths must be resolved via `model_manager.get_model_path(&model_id)` to handle both `models/` directory and HuggingFace cache gracefully without panics.
- **Multimodal Ground-Truth ASR**: Gemma 4 2B running on `llama-server.exe` with `mmproj-F16.gguf` accepts raw 16kHz WAV audio in `input_audio` payload to provide a 2nd independent acoustic hypothesis in ~580ms.
- **Consensus Fusion**: When Multi-STT is enabled, `DEFAULT_MULTI_STT_MERGE_PROMPT` ("Merge and Clean") automatically fuses hypotheses `${output}`, `${output2}`, `${output3}` into a final clean transcript.

### Brain, Llama.cpp & Local Model Rules
- **Local Server Lifecycle**: Pre-compiled `llama-server.exe` managed by [`src-tauri/src/llama_server/manager.rs`](file:///src-tauri/src/llama_server/manager.rs). Auto-detects GPU backend (Vulkan0, CUDA 12/13, Apple Metal, CPU AVX2).
- **Zero-Cloud Default**: All features default to offline local operation. No external API keys required for core functionality.
- **Prompt Variables**: Variables like `${output}`, `${selected_text}`, `${active_app}`, `${time_local}` must be replaced before LLM dispatch.

### TTS Engines & Runtime Bindings
- **Offline Engines (6 local)**:
  - `Qwen3-TTS`: Local GGML or PyTorch CUDA Graphs (`faster-qwen3-tts`).
  - `Kokoro-82M`: Local persistent ONNX HTTP server (54 voices, 9 languages).
  - `Pocket TTS`: Local persistent server with zero-shot voice cloning.
  - `Piper`: Local ONNX neural voice engine.
  - `Kitten TTS`: Local lightweight engine.
  - `SAPI`: Native Windows COM interop fallback.
- **Audio Output**: `rodio` streaming gapless audio player (`src-tauri/src/tts/player.rs`) with sub-20ms instant flush on barge-in.

### Cross-Platform & UI Design Rules
- **Cross-Platform Mandate**: Windows 11 (Top priority), macOS (First-class), Linux (First-class). Every `#[cfg(target_os = "...")]` must provide fallbacks.
- **Color Theme**: Golden Accent Palette (`--color-logo-primary: #f59e0b` / `#d97706`, `--dark-color-logo-stroke: #fef3c7`, UI background accent: `#d97706`). **No pink or purple accents.**
- **Internationalization**: 23 languages in `src/i18n/locales/`. Every new user-facing string must be in `en/translation.json` and synced via `bun run sync:translations`.

---

## 4. Sequential Roadmap & Task Tracker (Stateful Milestones)

### Milestone 1: AI Selection Transformation & Dynamic Context
- [ ] **Task 1.1**: Implement OS selection capture in [`src-tauri/src/clipboard.rs`](file:///src-tauri/src/clipboard.rs) & [`src-tauri/src/actions.rs`](file:///src-tauri/src/actions.rs).
- [ ] **Task 1.2**: Implement dynamic prompt variable replacer (`${selected_text}`, `${active_app}`, `${clipboard}`, `${time_local}`).
- [ ] **Task 1.3**: Add `ai_replace_selection` action & global shortcut in [`src-tauri/src/shortcut/handler.rs`](file:///src-tauri/src/shortcut/handler.rs).
- [ ] **Task 1.4**: Add frontend AI Replace settings card in `src/components/settings/` and sync all 23 translation files.

### Milestone 2: Multi-Profile Transcription System
- [ ] **Task 2.1**: Define `TranscriptionProfile` struct in [`src-tauri/src/settings.rs`](file:///src-tauri/src/settings.rs).
- [ ] **Task 2.2**: Implement profile cycle hotkey and dedicated per-profile shortcut triggers.
- [ ] **Task 2.3**: Build frontend Profile Management UI with active profile status indicator in footer.

### Milestone 3: Local OpenAI Realtime WebSocket Gateway
- [ ] **Task 3.1**: Implement local WebSocket server (`ws://127.0.0.1:8765/v1/realtime`) using `axum` + `tokio-tungstenite`.
- [ ] **Task 3.2**: Implement standard OpenAI Realtime events (`session.update`, `input_audio_buffer.append`, `response.create`, `response.audio.delta`).
- [ ] **Task 3.3**: Implement zero-latency hardware barge-in queue cancellation.

### Milestone 4: Dual-Channel System Loopback Audio Recorder
- [ ] **Task 4.1**: Implement WASAPI loopback (Windows) & CoreAudio/PulseAudio capture in `audio_toolkit`.
- [ ] **Task 4.2**: Dual-channel mixer: Channel 0 (Mic) + Channel 1 (System Speaker/Meeting Audio).
- [ ] **Task 4.3**: Real-time meeting transcription window with Markdown minutes export.

### Milestone 5: Local Batch Media & Document Transcriber
- [ ] **Task 5.1**: Drag-and-drop batch audio/video transcriber (export `.srt`, `.vtt`, `.md`).
- [ ] **Task 5.2**: Long document-to-speech synthesizer (Audiobook mode with chapter chunking).

### Milestone 6: Local Zero-Shot Voice Cloning Studio
- [ ] **Task 6.1**: Visual 3-second reference voice recorder in TTS settings.
- [ ] **Task 6.2**: Save reference embeddings and route to Qwen3-TTS / Pocket TTS prompt audio.

---

## 5. Subsystem Reference & Pre-Commit Routine (Fixed Schema)

### Subsystem File Map
| Area | Path | Responsibility |
|---|---|---|
| **STT Engine** | [`src-tauri/src/managers/transcription.rs`](file:///src-tauri/src/managers/transcription.rs) | transcribe.cpp streaming, VAD feeding, model switches |
| **Multi-STT** | [`src-tauri/src/stt/multi_stt.rs`](file:///src-tauri/src/stt/multi_stt.rs) | Gemma 4 ASR (`mmproj`), parallel models, LLM merge |
| **Local LLM** | [`src-tauri/src/llama_server/manager.rs`](file:///src-tauri/src/llama_server/manager.rs) | llama-server.exe process lifecycle, GPU offload |
| **Brain** | [`src-tauri/src/brain/manager.rs`](file:///src-tauri/src/brain/manager.rs) | Turn history, sentence splitting, TTS queueing |
| **TTS** | [`src-tauri/src/tts/manager.rs`](file:///src-tauri/src/tts/manager.rs) | Local TTS servers, gapless audio playback |
| **Audio/VAD** | [`src-tauri/src/audio_toolkit/`](file:///src-tauri/src/audio_toolkit/) | cpal recording, Silero VAD ONNX, RNNoise |
| **Shortcuts** | [`src-tauri/src/shortcut/handler.rs`](file:///src-tauri/src/shortcut/handler.rs) | Global hotkey dispatcher and event handling |
| **UI Theme** | [`src/styles/theme.css`](file:///src/styles/theme.css) | Golden theme tokens and color mappings |

### Mandatory Pre-Commit Commands
```bash
bun run sync:translations    # 1. Sync all 23 translation languages
bun run check:translations   # 2. Verify all translation keys exist
bunx tsc --noEmit            # 3. TypeScript type checking
bun run format               # 4. Prettier + cargo fmt formatting
bun run lint:fix             # 5. ESLint auto-fix
cargo test                   # 6. Rust backend tests (319 tests)
bun run validate             # 7. Automated pre-commit verification gate
```
