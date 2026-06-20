# 04 — Code Findings (with evidence)

[← Cleanup Kill-List](03_CLEANUP_KILL_LIST.md) · [back to START HERE](00_START_HERE.md)

Concrete, code-level issues with `file:line` evidence so you (or an assistant) can act without re-investigating. Ordered by impact.

---

## 1. "Local-first" depends on a Python venv {#python-venv}

**The single biggest architectural risk.** The README promises offline, download-and-run. But:

- `tts/local_tts_server.rs::resolve_venv_python()` looks for `venv/Scripts/python.exe` (Win) / `venv/bin/python` and spawns Python HTTP servers.
- These backends require it: `tts/backends/kokoro.rs`, `kitten.rs`, `pocket.rs`, `piper.rs`/`piper_server.rs` (4 of 5 "local" engines).
- The Python servers themselves: `src-tauri/kokoro_server.py`, `kitten_server.py`, `pocket_server.py`, `unified_parakeet_server.py`.
- First-run needs `scripts/setup_tts_venv.{ps1,sh}` / `setup_venv_uv.ps1` + network pip installs.

**Why it matters:** every Python/venv/CUDA/pip permutation is a support ticket. It breaks the "just works offline" promise and complicates packaging on all three OSes.

**Fix (pick in Phase 1):**

- **A:** Make a Rust/ONNX TTS path the default (Piper has Rust options; `sherpa-onnx` is already in your reference set), or default to **SAPI** on Windows (it's pure Rust COM, no venv). Python engines → opt-in "advanced."
- **B:** Bundle a pinned standalone Python + prebuilt wheels so it's truly one-click offline.
- Either way, **change the README claim to match reality.**

---

## 2. God-files concentrate the risk {#god-files}

Five files hold a disproportionate share of complexity. This is where bugs hide and where _you_ feel lost in the code.

| File                | Lines | Split into                                                                                    |
| ------------------- | ----- | --------------------------------------------------------------------------------------------- |
| `managers/model.rs` | 2,233 | per-model-family modules + a registry; move the hardcoded model list to a data file (see #5). |
| `settings.rs`       | 2,185 | sub-structs by domain (audio/stt/tts/brain/overlay) + a migrations module. → #3               |
| `shortcut/mod.rs`   | 1,513 | parsing, registration, and platform glue as separate files.                                   |
| `actions.rs`        | 1,391 | one module per action category (dictation, replace-selection, read-aloud, conversation).      |
| `clipboard.rs`      | 1,167 | platform impls split out; separate the double-copy watcher logic.                             |

**Rule of thumb to adopt:** no source file over ~800 lines without a written reason. Do this in **Phase 3**, after the core is stable — refactoring unstable code just moves the bugs.

---

## 3. Settings is a growing god-object {#settings}

`settings.rs` (2,185 lines) is where every feature bolts on a flat field — e.g. `multi_stt_enabled`, `multi_stt_models`, `multi_stt_prompt`, `WgpuTrailConfig`, etc. all live at the top level.

**Problems:** no grouping, easy to break serialization, hard to reason about, and migrations are implicit.

**Fix:** group into nested structs (`settings.audio`, `.stt`, `.tts`, `.brain`, `.overlay`), add an explicit `schema_version` + a migrations module, and keep `#[serde(default)]` discipline so old configs load. Pairs with the model-list extraction in #5.

---

## 4. 292 `.unwrap()`/`.expect()` outside tests {#panics}

Each is a potential panic → crash in an always-on desktop app.

```
grep -rn "\.unwrap()\|\.expect(" src-tauri/src --include="*.rs" | grep -v test | wc -l   # → 292
```

Not all are dangerous (mutex locks, compile-time-known-good). But triage by blast radius:

- **High priority:** audio capture/playback (`audio_toolkit/audio/recorder.rs`, `tts/player.rs`), clipboard (`clipboard.rs`), server spawning (`local_tts_server.rs`, `llama_server/manager.rs`), and any IPC command handler in `commands/` — a panic here can take down recording or paste mid-use.
- **Lower:** startup-only paths where a panic is effectively a clear fatal error.

**Fix (Phase 1):** convert hot-path `unwrap`s to `?`/handled errors with user-visible messages; leave a deliberate `.expect("reason")` only where failure truly is unrecoverable.

---

## 5. Hardcoded model list with a self-aware TODO {#model-list}

`managers/model.rs:149`:

```rust
// TODO this should be read from a JSON file or something..
```

The model catalog is baked into a 2,233-line Rust file. Extracting it to a JSON/TOML manifest shrinks the god-file, lets you add models without recompiling, and makes the catalog testable. Do this as part of #2.

---

## 6. i18n is frozen at ~72% {#i18n}

- English: **663** keys. Every other locale (ar, bg, cs, de, es, fr, he, it, ja, ko, pl, pt, ru, sv, tr, uk, vi, zh, zh-TW): **exactly 477**.
- The uniform 477 means they were translated in one batch and never re-synced as English grew by 186 keys.
- You already have `scripts/check-translations.ts` — it just isn't gating anything.

**Fix (Phase 2):** run `check-translations` in CI as a hard gate; backfill missing keys (machine-translate + mark for human review); or honestly relabel as "English + partial community translations." Stop calling it "✅ Complete." → [01](01_STATE_OF_THE_PROJECT.md#claimed-but-not-done)

---

## 7. The native overlay stub references a file that doesn't exist {#native-overlay}

`overlay_fx/native/mod.rs`:

- `NativeTrailOverlay::start()` only `log::info!(...)` and returns `Ok`. `stop()` is empty.
- The doc-comment says "The render loop skeleton is in `render_loop_stub.rs`" — but the directory contains only `mod.rs` + `shader.wgsl`. **That file doesn't exist.**

**Fix (Phase 2):** implement it (vendor from `Cross_Platform_Rust_WebGPU_CursorFX`, wire the wgpu-29 surface) **or** delete `overlay_fx/native/` and the OS-native toggle. Don't ship a stub the docs call complete.

---

## 8. Testing is lopsided {#tests}

- **Backend:** 206 test annotations — genuinely good.
- **Frontend:** `tests/app.spec.ts` is the _only_ frontend test (a Playwright smoke test). 108 components, ~17k LOC, ~1 test.

**Fix (Phase 1):** Playwright coverage for the three pipelines + onboarding. Aim for "the money paths can't silently break," not a coverage percentage.

---

## 9. Two STT code paths {#multi-stt}

`stt/unified_parakeet.rs` (primary) + `stt/multi_stt.rs` (parallel, gated behind `multi_stt_enabled`, called from `actions.rs:972`). Both are wired and real. Maintaining two transcription paths is a real tax — keep it only if `multi_stt` is tested and documented; otherwise shelve it behind a clearly-experimental flag.

---

## 10. Minor / housekeeping

- **Generated snapshots committed:** `S2B2S_repomix*.txt` go stale immediately — gitignore them. → [03](03_CLEANUP_KILL_LIST.md)
- **`analysys/` is gitignored but still tracked** — confusing half-state; `git rm -r` it (README says it's superseded).
- **`helpers/clamshell.rs`** has intentional no-op stubs for non-macOS — that's _fine_ (platform shims), not a defect; noted so you don't flag it later.
- **Version strings are consistent** (`0.1.3` across `package.json`, `Cargo.toml`, `tauri.conf.json`) — good, keep them in sync via a release script.

---

## Quick reference — where the bodies are buried

| Concern           | Look here                                                                                          |
| ----------------- | -------------------------------------------------------------------------------------------------- |
| Python dependency | `tts/local_tts_server.rs`, `tts/backends/{kokoro,kitten,pocket,piper}.rs`, `src-tauri/*_server.py` |
| Biggest files     | `managers/model.rs`, `settings.rs`, `shortcut/mod.rs`, `actions.rs`, `clipboard.rs`                |
| Panic surface     | `audio_toolkit/audio/recorder.rs`, `tts/player.rs`, `clipboard.rs`, `commands/`                    |
| Stub claimed done | `overlay_fx/native/mod.rs`                                                                         |
| i18n gap          | `src/i18n/locales/*/translation.json` (en=663, rest=477)                                           |
| Self-aware TODO   | `managers/model.rs:149`                                                                            |

[← back to START HERE](00_START_HERE.md)
