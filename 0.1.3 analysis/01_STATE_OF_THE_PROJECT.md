# 01 — State of the Project (what's done vs claimed)

[← back to START HERE](00_START_HERE.md) · next → [Roadmap](02_ROADMAP_IN_ORDER.md)

Judged against the **code at `1332d3c` (v0.1.3)**, not the docs. Labels: ✅ Done · 🟡 Partial · 🔴 Claimed-but-stub · 📋 Planned.

---

## ✅ What's genuinely done (and good)

These are wired end-to-end and look production-shaped. Don't touch them except to harden.

| Area                                        | Evidence / notes                                                                                                                                                              |
| ------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Core Tauri 2 + typed IPC**                | `tauri-specta` bindings (`src/bindings.ts`, 873 lines) auto-generated via `cargo test export_bindings`. This is the backbone and it's solid.                                  |
| **Dictation pipeline**                      | Mic → VAD → STT → normalize → paste. Real path through `managers/transcription.rs`, `actions.rs`, `clipboard.rs`.                                                             |
| **TripleVAD**                               | RMS → RNNoise → Silero, in `audio_toolkit/vad/`. Actually implemented, not a label.                                                                                           |
| **STT via transcribe-rs**                   | Parakeet V3 + Whisper through `transcribe-rs 0.3.11`; `stt/unified_parakeet.rs` is the primary engine and is wired in `managers/transcription.rs`.                            |
| **TTS backend trait + 5 local / 3 cloud**   | `tts/backends/` has real impls: piper, kokoro, kitten, pocket, sapi, openai, elevenlabs, cartesia. The trait/lifecycle (`WarmEngine`) is clean.                               |
| **TTS sidecar architecture**                | `tts/local_tts_server.rs` spawns Python HTTP servers and manages venv resolution. The `backends/piper.rs → piper_server.rs` split is a _correct_ separation, not duplication. |
| **Brain (LLM) streaming**                   | SSE streaming in `brain/client.rs` (495 lines), turn history + barge-in in `brain/manager.rs`, llama.cpp bridge in `brain/llama_manager.rs`.                                  |
| **llama.cpp server management**             | Auto-download/launch/health-check + GPU offload in `llama_server/manager.rs`.                                                                                                 |
| **Text normalization**                      | ITN/TN via `text-processing-rs` + markdown strip in `tts/sanitize/`. Real 5-stage pipeline.                                                                                   |
| **SQLite history + migrations**             | `managers/history.rs` (790 lines) via rusqlite.                                                                                                                               |
| **Secrets in OS keychain + storage crypto** | `crypto.rs`; v0.1.2 added storage encryption.                                                                                                                                 |
| **Global shortcuts**                        | `shortcut/` (1,500+ lines) with two implementations (rdev key-listener + tauri global-shortcut).                                                                              |
| **3D avatar**                               | `src/brain-overlay/avatar/Avatar3D.tsx` (226 lines, Three.js) — actually renders and reacts to mic level/phase. Not a placeholder.                                            |
| **Crash logging**                           | `crash_logging.rs` with backtraces.                                                                                                                                           |
| **Single-instance + CLI/signal control**    | `cli.rs`, `control_server.rs`, `signal_handle.rs`.                                                                                                                            |
| **Test culture (backend)**                  | **206** `#[test]`/`#[tokio::test]`/`#[cfg(test)]` annotations across Rust. Genuinely better than typical alpha.                                                               |
| **CI exists**                               | 9 GitHub workflows (though redundant — see [03](03_CLEANUP_KILL_LIST.md#ci)).                                                                                                 |

**Takeaway:** the spine of the app is real. Your instinct that "everything is half-broken" is mostly the docs talking, not the code.

---

## 🟡 What's partially done (real code, real gaps)

These have working code but should **not** be called "Complete." Each needs a decision: finish, or shelve behind a flag.

| Feature                                         | What exists                                                                                                                      | What's missing / the gap                                                                                                                                                           |
| ----------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Streaming STT** (README: "Partial" — correct) | WebSocket path + EOU 120M via `unified_parakeet_server.py`; `transcription.rs` has streaming branches for UnifiedParakeet models | Not the default; relies on the Python server; correctness/finalization edges. Honestly labeled but buried among ✅ rows.                                                           |
| **i18n (20 languages)** (README: "✅ Complete") | 20 locale files, `i18next` wired, `check-translations.ts` tooling exists                                                         | **English = 663 keys; every other language = exactly 477.** ~28% of UI untranslated in all 19 non-English locales, and frozen at one snapshot. See [04](04_CODE_FINDINGS.md#i18n). |
| **multi_stt (parallel STT)**                    | Wired behind `multi_stt_enabled` setting; `stt/multi_stt.rs::transcribe_parallel` called from `actions.rs`                       | It's a real feature gated off by default — fine, but it adds surface area and another STT code path to maintain. Confirm it's tested and documented or shelve it.                  |
| **Native OS overlay vs Tauri overlay**          | Tauri-window overlay works; there's an OS-native mode toggle                                                                     | The native _wgpu_ track is a stub (next section). The "toggle" can point at something that does nothing.                                                                           |
| **Continuous voice / hands-free / barge-in**    | `managers/continuous_voice.rs` (real), barge-in in brain manager                                                                 | Marked Complete; verify behavior under echo/full-duplex — README lists full-duplex + AEC as "Later," so today's continuous mode likely has known echo limits.                      |
| **Wake word**                                   | `wake_word.rs` + `commands/wake_word.rs`, VAD-based                                                                              | VAD-energy wake word ≠ keyword spotting. Works, but manage expectations vs a real "Hey X" model.                                                                                   |

---

## 🔴 Claimed done, but actually a stub or absent {#claimed-but-not-done}

This is the trust-eroding category. Fix the labels first, the code second.

| Item                                              | Claimed                                                             | Reality (evidence)                                                                                                                                                                                                                                                                                  |
| ------------------------------------------------- | ------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **GPU overlay — native wgpu rendering (Track B)** | README has it as 🚧 _and_ the overlay system as ✅ in adjacent rows | `overlay_fx/native/mod.rs`: `NativeTrailOverlay::start()` just `log::info!(...)` and returns `Ok(Self{ _private: () })`. `stop()` is empty. The doc-comment references a `render_loop_stub.rs` **that doesn't exist** in the directory (only `mod.rs` + `shader.wgsl`). This is a pure placeholder. |
| **i18n "Complete"**                               | ✅ in roadmap                                                       | 72% complete, frozen (see above).                                                                                                                                                                                                                                                                   |
| **"Local-first, works offline, just download"**   | README "Why S2B2S"                                                  | True for STT and SAPI; **false for 4 of 5 local TTS engines**, which require a Python venv + pip installs on first run. See [04](04_CODE_FINDINGS.md#python-venv).                                                                                                                                  |
| **Roadmap table accuracy generally**              | ~40 ✅ rows                                                         | The table is hand-maintained and has drifted from reality. It should be generated or trimmed, not trusted.                                                                                                                                                                                          |

> None of these are scandals — they're the normal result of a fast-moving solo/small project where the docs are written aspirationally. The fix is cheap: **tell the truth in the labels.** That single change will do more for your sense of control than any code work.

---

## 📋 Planned-only (lives in docs, not code)

From the README roadmap + the various plan docs. These are _fine_ as plans — just keep them in **one** place (see [03](03_CLEANUP_KILL_LIST.md)).

- Profiles (per-application settings) — 📋
- Full-duplex conversation with acoustic echo cancellation — 📋
- Local speaker diarization — 📋
- MCP tool use for the Brain — 📋
- Plugin/API ecosystem — 📋
- Android/mobile companion — 📋 (has its own plan: `android-port-plan.md`, `S2B2S_ANDROID_COMPANION.md`)
- The entire `futuristic_analysis/` vision (transparent overlay, screen understanding, avatar v2) — 📋, explicitly aspirational

---

## Scorecard

| Dimension                     | Grade  | One-line reason                                  |
| ----------------------------- | ------ | ------------------------------------------------ |
| Core pipeline (STT→Brain→TTS) | **A−** | Real, wired, thoughtfully layered                |
| Backend code quality          | **B**  | Good patterns, but 5 god-files + 292 unwraps     |
| Frontend code quality         | **B**  | Clean structure, but ~1 test total               |
| Honesty of docs vs code       | **D**  | Overclaims "Complete"; this is the core pain     |
| Documentation _organization_  | **D−** | 66 files, 6 roadmaps, 1 MB of competitor reviews |
| Cross-platform "just works"   | **C**  | Python-venv dependency undercuts the promise     |
| Test coverage                 | **C+** | Strong backend, near-zero frontend/e2e           |
| **Overall**                   | **B−** | Good bones, lost in its own paperwork            |

Next: **[02 — Roadmap in order →](02_ROADMAP_IN_ORDER.md)**
