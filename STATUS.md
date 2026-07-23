# S2B2S Project Status, Scorecard & Roadmap

This document serves as the **single source of truth** for what is completed, partially done, stubbed, or planned in S2B2S. Last updated at version **0.1.5**.

---

## 1. Feature Scorecard (v0.1.5 Audit & Cleanup)

| Area                            | Status     | Notes                                                                                                          |
| ------------------------------- | ---------- | -------------------------------------------------------------------------------------------------------------- |
| **Core STT→Brain→TTS loop**     | ✅ Done    | Real, wired, thoughtfully layered and robust.                                                                  |
| **Dictation Pipeline**          | ✅ Done    | Mic → VAD → STT → Normalizer → Paste.                                                                          |
| **TripleVAD Engine**            | ✅ Done    | RMS → RNNoise → Silero ONNX.                                                                                   |
| **TTS Engine & Warm Lifecycle** | ✅ Done    | 9 backends (6 local, 3 cloud). Added local Qwen3-TTS engine with faster-qwen3-tts PyTorch CUDA Graphs support. |
| **pre-compiled llama.cpp**      | ✅ Done    | Auto-downloads releases, auto-starts, auto-detects CUDA/Vulkan/CPU.                                            |
| **Standalone Speech Runtime**   | ✅ Done    | Portable uv + Python 3.12 provisioned during onboarding via install-speech-runtime scripts.                    |
| **i18n Multi-Language**         | ✅ Done    | 20 languages supported, all synchronized with 724 keys (English fallback values for new keys).                 |
| **Streaming STT**               | 🟡 Partial | Works via Python server but has chunk boundary token edges. Not default.                                       |
| **Continuous Voice Mode**       | 🟡 Partial | Real hands-free conversation with barge-in support, but limited echo cancellation.                             |
| **Wake Word Engine**            | 🟡 Partial | VAD-energy based. Keyword spotting (KWS) requires Static/Dynamic CRT resolution.                               |
| **Playwright E2E Tests**        | ✅ Done    | Onboarding, dictation, and conversation pipelines covered with mock Tauri IPC layer.                           |
| **Panic Audit (hot paths)**     | ✅ Done    | Converted unwraps in audio recording, clipboard, IPC boundaries, and command handlers.                         |
| **Brain-Only STT Toggle**       | ✅ Done    | Inline switch in ConversationView to bypass local STT and feed audio directly to multimodal Brain.             |
| **Native WGPU Overlay**         | 🟡 Shelved | Track B (`overlay_fx/native/mod.rs`) is kept as a stub; native overlay feature is shelved for Tauri overlay.   |

---

## 2. Project Quality Scorecard

- **Core Loop Pipeline**: **A−** (Solid, well-layered architecture)
- **Backend Code Quality**: **B+** (Panic audit reduced crash surface; hot-path unwraps converted to handled errors; 5 god-files remain)
- **Frontend Code Quality**: **B+** (Playwright E2E suites added for onboarding, dictation, and conversation pipelines)
- **Documentation Honesty**: **A−** (Doc sprawl cleaned from 66 files/24.5K lines to ~18 files/5K lines; STATUS.md established as single truth)
- **Nix & Cross-Platform Support**: **C+** (Standalone speech runtime scripts reduce but don't eliminate Python venv fragility for local TTS)

---

## 3. Ordered Roadmap (Phases 0–4)

```
[Phase 0: De-sprawl] (Genuinely Closed)
        |
        v
[Phase 1: Bulletproof Core] (Venv / Standalone Python choice, panic audit, onboarding, E2E tests)
        |
        v
[Phase 2: Sweep the Partials] (Implement or formally shelve wgpu overlay, sync i18n translations)
        |
        v
[Phase 3: Refactoring] (Split god-files like model.rs, settings.rs, shortcut/mod.rs)
        |
        v
[Phase 4: Ambition] (Profiles, MCP Tool use, Full-Duplex AEC, Android app release)
```

### Phase 0 — Stop the Bleeding

- **Status**: ✅ Completed.
- **Tasks**: Delete/consolidate 6 competing roadmaps, merge redundant files, ignore generated snapshots, and create `STATUS.md` as the unified index.

### Phase 1 — Make the Core Bulletproof

- **Status**: ✅ Completed (v0.1.4).
- [x] **Address the Python/venv dependency**: Bundled standalone Python runtime via `scripts/install-speech-runtime.ps1`/`.sh` — portable uv + Python 3.12 + venv provisioned during onboarding.
- [x] **Hot Path Panic Audit**: Triage `.unwrap()` and `.expect()` calls in audio recording, clipboard, IPC boundaries, and command handlers — converted to handled errors.
- [x] **Playwright E2E Tests**: Added spec suites for onboarding, dictation, and conversation pipelines with mock Tauri IPC layer (`tests/helpers/tauri-mock.ts`).
- [x] **Onboarding Polish**: Modified `Onboarding.tsx` to execute and display installation progress of the portable speech runtime.
- [x] **Settings Persistence Fix**: Added `store.save()` after toggle changes to prevent reverting.
- [x] **Piper CUDA Fixes**: Resolved DLL path resolution bug and added NVIDIA CUDA runtime packages to venv setup.
- [x] **Brain-Only STT Toggle**: Inline switch in ConversationView to bypass local STT and feed audio directly to multimodal Brain.
- [x] **Multimodal WAV Transmission**: Switched from MP3 to raw WAV, removed `mp3lame-encoder` dependency.

### Phase 2 — Sweep the Partials

- [x] **Native WGPU Overlay (Track B)**: Shelved in favor of Track A (Tauri overlay) to reduce binary size and complexity.
- [x] **i18n Sync**: Fully synchronized all 19 non-English translation keys (724 keys matched) with CI gate checks passing.
- [x] **De-sprawl / Project Cleanup**: Removed obsolete scratch/experiment folders (gemma, temp ONNX, 0.1.3 review, and stale descriptions).
- [x] **Qwen3-TTS GGML Backend**: Compiled native C++ `qwentts.cpp` shared libraries for Windows 11 with CUDA 13.3 support, resolved ctypes DLL loading dependencies, and integrated the `qwen3` engine option in settings.
- [ ] **Streaming STT**: Stabilize chunk-boundary token generation or label as experimental.

### Phase 3 — Reduce the Maintenance Surface

- [ ] **Split the God Files**: Refactor `settings.rs` (2,048 lines), `managers/model.rs` (2,012 lines), `actions.rs` (1,347 lines), `shortcut/mod.rs` (1,327 lines), and `clipboard.rs` (1,034 lines) into smaller, single-responsibility modules.
- [ ] **Settings Schema Versioning**: Group settings into sub-structs (audio, brain, etc.) and add explicit migrations.
- [ ] **Extract Model Catalog**: Move hardcoded model definitions from `managers/model.rs` to a JSON/TOML manifest (addresses `// TODO` at line 149).

### Phase 4 — Ambitious Features

- [ ] Application profiles (context-aware settings).
- [ ] MCP tool use integration for the Brain.
- [ ] On-device Android voice assistant application.
