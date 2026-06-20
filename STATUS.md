# S2B2S Project Status, Scorecard & Roadmap

This document serves as the **single source of truth** for what is completed, partially done, stubbed, or planned in S2B2S. It reflects the codebase status audited at version **0.1.3**.

---

## 1. Feature Scorecard (v0.1.3 Audit)

| Area                            | Status     | Notes                                                                                                            |
| ------------------------------- | ---------- | ---------------------------------------------------------------------------------------------------------------- |
| **Core STT→Brain→TTS loop**     | ✅ Done    | Real, wired, thoughtfully layered and robust.                                                                    |
| **Dictation Pipeline**          | ✅ Done    | Mic → VAD → STT → Normalizer → Paste.                                                                            |
| **TripleVAD Engine**            | ✅ Done    | RMS → RNNoise → Silero ONNX.                                                                                     |
| **TTS Engine & Warm Lifecycle** | ✅ Done    | 8 backends (5 local, 3 cloud). WarmEngine trait/lifecycle is clean.                                              |
| **pre-compiled llama.cpp**      | ✅ Done    | Auto-downloads releases, auto-starts, auto-detects CUDA/Vulkan/CPU.                                              |
| **i18n Multi-Language**         | 🟡 Partial | 20 languages supported, but only English has all 663 keys. All other 19 languages have 477 keys (~72% complete). |
| **Streaming STT**               | 🟡 Partial | Works via Python server but has chunk boundary token edges. Not default.                                         |
| **Continuous Voice Mode**       | 🟡 Partial | Real hands-free conversation with barge-in support, but limited echo cancellation.                               |
| **Wake Word Engine**            | 🟡 Partial | VAD-energy based. Keyword spotting (KWS) requires Static/Dynamic CRT resolution.                                 |
| **Native WGPU Overlay**         | 🔴 Stub    | Track B (`overlay_fx/native/mod.rs`) is a pure placeholder logging a line.                                       |

---

## 2. Project Quality Scorecard

- **Core Loop Pipeline**: **A−** (Solid, well-layered architecture)
- **Backend Code Quality**: **B** (Good Rust patterns, but several high-complexity files + 200+ unwraps)
- **Frontend Code Quality**: **B** (Clean TSX/Zustand structure, but lacking UI/E2E test coverage)
- **Documentation Honesty**: **B** (Restored to truth-telling by removing misleading tables and stub claims)
- **Nix & Cross-Platform Support**: **C** (Python venv requirement for local TTS adds platform fragility)

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

### Phase 0 — Stop the Bleeding (Current Phase — Genuinely Closed)

- **Status**: ✅ Completed.
- **Tasks**: Delete/consolidate 6 competing roadmaps, merge redundant files, ignore generated snapshots, and create `STATUS.md` as the unified index.

### Phase 1 — Make the Core Bulletproof (Upcoming Phase)

- [ ] **Address the Python/venv dependency**: Either make Piper-via-Rust / SAPI the REAL zero-dependency defaults (Option A) or bundle a standalone Python build (Option B).
- [ ] **Hot Path Panic Audit**: Triage `.unwrap()` and `.expect()` calls in audio recording, clipboard, and IPC boundaries to prevent application crashes.
- [ ] **Playwright E2E Tests**: Add Playwright coverage for onboarding and the 3 main voice pipelines.
- [ ] **Onboarding Polish**: Ensure a fresh-machine installation walks a user through mic permissions and model downloads gracefully.

### Phase 2 — Sweep the Partials

- [ ] **Native WGPU Overlay (Track B)**: Either implement the native renderer fully (via WebGPU cursor references) or delete the placeholder and keep only the Tauri overlay.
- [ ] **i18n Sync**: Machine-translate and sync the missing 186 keys across all 19 non-English languages and add translation gate checks to CI.
- [ ] **Streaming STT**: Stabilize chunk-boundary token generation or label as experimental.

### Phase 3 — Reduce the Maintenance Surface

- [ ] **Split the God Files**: Refactor `model.rs` (2,230 lines), `settings.rs` (2,180 lines), `shortcut/mod.rs` (1,500 lines), and `actions.rs` (1,390 lines) into smaller, single-responsibility modules.
- [ ] **Settings Schema Versioning**: Group settings into sub-structs (audio, brain, etc.) and add explicit migrations.

### Phase 4 — Ambitious Features

- [ ] Application profiles (context-aware settings).
- [ ] MCP tool use integration for the Brain.
- [ ] On-device Android voice assistant application.
