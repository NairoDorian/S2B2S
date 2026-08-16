# S2B2S Project Status, Scorecard & Roadmap

This document serves as the **single source of truth** for what is completed, partially done, stubbed, or planned in S2B2S. Last updated at version **0.1.5**.
For the active, sequential local-first engineering roadmap and agent takeover instructions, see **[`TAKEOVER.md`](TAKEOVER.md)**.

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

---

## 2. Project Quality Scorecard

- **Core Loop Pipeline**: **A−** (Solid, well-layered architecture)
- **Backend Code Quality**: **B+** (Panic audit reduced crash surface; hot-path unwraps converted to handled errors; 401 unwraps + 7 god-files remain)
- **Frontend Code Quality**: **B** (Playwright E2E suites added; 11 dead components, double-rendered permission banner, unwired backend features)
- **Documentation Honesty**: **A−** (Doc sprawl cleaned; several stored settings (`endpoint_preset`, `headphone_mode`, wake-word `keyword`) promise more than the backend delivers — M0 addresses)
- **Nix & Cross-Platform Support**: **C+** (Standalone speech runtime scripts reduce but don't eliminate Python venv fragility for local TTS)
- **Upstream Health**: **A** (0 commits behind `cjpais/Handy:main`; portable HF_HOME fix merged as `6505e0fc`)

---

## 3. Ordered Roadmap (Phases 0–4)

```
[Phase 0: De-sprawl] (Genuinely Closed)
        |
        v
[Phase 1: Bulletproof Core] (Venv / Standalone Python choice, panic audit, onboarding, E2E tests)
        |
        v
[Phase 2: Sweep the Partials] (Implement or formally shelve remaining partials, sync i18n translations)
        |
        v
[Phase 3: Refactoring] (Split god-files like model.rs, settings.rs, shortcut/mod.rs)
        |
        v
[Phase 4: Ambition] (Profiles, MCP Tool use, Full-Duplex AEC, Android app release)
```

### Active Feature Roadmap (Milestones M0–M5)

The consolidated feature roadmap — synthesized from deep reviews of AIVORelay, copyspeak, and
speech-to-speech — lives in **[`TAKEOVER.md`](TAKEOVER.md#4-sequential-roadmap--task-tracker-stateful-milestones)**
and is the authoritative, sequential task tracker:

| Milestone | Theme                                                                                                                                                   | Source Inspiration |
| --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------ |
| **M0**    | Honesty & Hygiene Sweep (wire dead settings, kill dead code, fix UI bugs)                                                                               | S2B2S self-review  |
| **M1**    | Complete AI Replace Selection (selection capture, prompt variables, shortcut wiring)                                                                    | AIVORelay          |
| **M2**    | Smart Conversation Core (speculative turns, two-tier endpointing, compaction, tool calling, realtime gateway)                                           | speech-to-speech   |
| **M3**    | Dictation Ecosystem (subtitle export, batch file transcription, profiles, text replacement, cloning recorder, resumable TTS, region capture, connector) | AIVORelay          |
| **M4**    | TTS UX & History (audio effects, HUD hardening, history search/export, voice profiles, health checks)                                                   | copyspeak          |
| **M5**    | Infrastructure Consolidation (god-file splits, schema versioning, subsystem unification, unwrap audit)                                                  | S2B2S Phase 3      |

The historical AIVO plans (`AIVO_RELAY_IMPLEMENTATION_PLAN.md`, `AIVO_update_integration_plan.md`) are
superseded by the M0–M5 tracker.

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

- [x] **i18n Sync**: Fully synchronized all 19 non-English translation keys (724 keys matched) with CI gate checks passing.
- [x] **De-sprawl / Project Cleanup**: Removed obsolete scratch/experiment folders (gemma, temp ONNX, 0.1.3 review, and stale descriptions).
- [x] **Qwen3-TTS GGML Backend**: Compiled native C++ `qwentts.cpp` shared libraries for Windows 11 with CUDA 13.3 support, resolved ctypes DLL loading dependencies, and integrated the `qwen3` engine option in settings.
- [ ] **Streaming STT**: Stabilize chunk-boundary token generation or label as experimental.

### Phase 3 — Reduce the Maintenance Surface

- [ ] **Split the God Files**: Refactor `settings.rs` (2,048 lines), `managers/model.rs` (2,012 lines), `actions.rs` (1,347 lines), `shortcut/mod.rs` (1,327 lines), and `clipboard.rs` (1,034 lines) into smaller, single-responsibility modules. _(Progress: extracted a new `model_hub/` module with a shared resumable download transport and typed cross-collection events, reducing the future god-file scope; `managers/model.rs` split still pending.)_
- [ ] **Settings Schema Versioning**: Group settings into sub-structs (audio, brain, etc.) and add explicit migrations.
- [ ] **Extract Model Catalog**: Move hardcoded model definitions from `managers/model.rs` to a JSON/TOML manifest (addresses `// TODO` at line 149). _(Progress: STT is already driven by `catalog.json`; the new `model_hub/transport.rs` centralizes download lifecycle; Brain hard-coded fallback catalog extraction deferred.)_

### Phase 4 — Ambitious Features

- [ ] Application profiles (context-aware settings) — now Milestone M3.3.
- [ ] MCP tool use integration for the Brain — now Milestone M2.5 (offline `<code>`-block tool calls first).
- [ ] On-device Android voice assistant application.
