# S2B2S Evolution Plan — GPU Transparent Overlay, Conversation Mode 2.0, and the S2B2S Avatar

> **Status:** Proposal / planning document — June 2026
> **Scope:** Additive only. The existing app (Settings window, Conversation tab, dictation, read-aloud, tray) stays exactly as it is. Everything below is a **new Overlay Mode** layered on top.

---

## The Vision in One Paragraph

Today, talking to the Brain means opening the S2B2S window and using the Conversation tab. Tomorrow, S2B2S lives **on top of everything**: press the converse hotkey (or say the wake word) anywhere — in your IDE, browser, or a game — and a small, friendly, GPU-rendered **avatar** appears next to your cursor. It listens (ripples to your voice), thinks (orbits), and the Brain's reply **streams into a glass bubble right where you're looking**, spoken aloud at the same time. One keypress inserts the answer at your cursor; one keypress dismisses it. No window switching, no focus stealing, ever. This is **Conversation Mode 2.0**, and the avatar is the face of S2B2S.

```
            TODAY (Conversation 1.0)                TOMORROW (Conversation 2.0)
  ┌──────────────────────────────┐        ┌─────────────────────────────────────────┐
  │  S2B2S main window           │        │  ANY app, ANY screen                    │
  │  ┌────────────────────────┐  │        │                                  ▗▄▄▖   │
  │  │ Conversation tab       │  │        │   your cursor ─►  ▌            ◉(  ◡ )◉ │
  │  │ chat transcript        │  │   ──►  │                   ╔════════════════════╗│
  │  │ [mic] [text input]     │  │        │                   ║ Sure — the fix is… ║│
  │  └────────────────────────┘  │        │                   ║ ▍streaming tokens  ║│
  └──────────────────────────────┘        │                   ╚════════════════════╝│
     you go to the app                    │      the app comes to you               │
                                          └─────────────────────────────────────────┘
```

---

## Source Review Status

| Repo | Status | What was done |
| --- | --- | --- |
| `NairoDorian/S2B2S` | ✅ Cloned & reviewed in depth | Full read of `overlay.rs`, `brain/` (manager, client, llama_manager), `continuous_voice.rs`, `input.rs`, `ConversationView.tsx`, `RecordingOverlay.tsx`, `HerLoading.tsx`, settings (`BrainConfig`, `OverlayPosition`), Tauri config, README/CHANGELOG/CLAUDE.md/AGENTS.md conventions. See **01_REPO_REVIEW.md**. |
| `NairoDorian/Cross_Platform_Rust_WebGPU_CursorFX` | ⚠️ **Not accessible** (clone prompts for credentials → repo is private or renamed; not found via web search) | Plan proceeds on **stated assumptions** (it is a Rust + winit + wgpu transparent cursor-effects overlay). A dedicated integration seam is defined so its code can be vendored as a workspace crate the moment access is granted. See **01_REPO_REVIEW.md §3** and **02_GPU_OVERLAY_ARCHITECTURE.md §6**. |

**Action item for you:** make `Cross_Platform_Rust_WebGPU_CursorFX` public, add it as a private submodule with a deploy key, or paste its `main.rs` / window-creation / surface-config code into a follow-up — the plan reserves an exact slot for it.

---

## The Three Pillars

### 1. Multi cross-platform GPU transparent overlay (`02_GPU_OVERLAY_ARCHITECTURE.md`)

A new always-on-top, transparent, **click-through**, non-activating overlay layer that can follow the cursor across monitors on Windows 11, macOS, and Linux (X11 fully; Wayland in a documented degraded mode). Delivered in two tracks so value ships early:

- **Track A (ship first):** a second Tauri webview window — `brain_overlay` — built with the *exact same per-platform machinery already proven in `overlay.rs`* (NSPanel on macOS, `HWND_TOPMOST` + layered window on Windows, GTK Layer Shell on Linux), plus `set_ignore_cursor_events(true)` for click-through and a cursor-follow positioning loop reusing `input::get_cursor_position()` + `get_monitor_with_cursor()`. Avatar rendered with WebGL (Three.js is already a dependency via `HerLoading`).
- **Track B (the CursorFX integration):** a native **winit + wgpu** overlay (Vulkan/Metal/DX12 via wgpu) for per-pixel-alpha shader rendering, cursor particle FX, and minimal latency — this is where `Cross_Platform_Rust_WebGPU_CursorFX` gets vendored as a crate. Track A's UI contract (events, settings) is designed so Track B is a drop-in renderer swap.

### 2. Conversation Mode 2.0 (`03_CONVERSATION_MODE_2.md`)

The existing Brain pipeline is already perfect for this — it streams `brain:thinking` / `brain:token` / `brain:sentence` / `brain:done` events with barge-in abort. Conversation 2.0 **fans those same events out to the overlay window** and adds: a streaming reply bubble anchored to the cursor, quick actions (Insert at cursor / Copy / Open in Conversation tab / Regenerate / Dismiss), barge-in from the overlay, pin/follow modes, auto-hide, and full reuse of `conversation_mode` (push-to-talk / toggle / hands-free), `endpoint_preset`, `headphone_mode`, wake word, and per-message metrics (t/s, ms). **No changes to `BrainManager` semantics — only one new event sink.**

### 3. The S2B2S Avatar (`04_AVATAR_SPEC.md`)

S2B2S gets a face: **"Orbi"** (working name) — a soft, glowing, procedurally-rendered orb that is the direct evolution of the existing *Her*-style loading curve (`HerLoading.tsx`). Five emotional states mapped 1:1 to pipeline states already emitted by the backend:

| State | Trigger (existing event/signal) | Animation |
| --- | --- | --- |
| Idle | overlay shown, nothing active | slow breathing glow |
| Listening | `mic-level` events (already emitted by `overlay::emit_levels`) | ripples react to your voice |
| Thinking | `brain:thinking` | the *Her* curve orbits the orb |
| Speaking | new `tts:level` event (small tap in `tts/player.rs`) | waveform mouth pulses with TTS audio |
| Error / Done | `brain:error` / `brain:done` | color shift / settle |

Procedural-first (Canvas/WebGL shader in Track A → WGSL port in Track B): zero asset pipeline, themable, ~5 KB, and identical on all three OSes. Optional Rive-based "character" skin later.

---

## Document Map

| File | Contents |
| --- | --- |
| `00_OVERVIEW.md` | This file — the big note. |
| `01_REPO_REVIEW.md` | What was reviewed in S2B2S (subsystem by subsystem), what Conversation 1.0 and the current overlay actually do, and the CursorFX assumptions. |
| `02_GPU_OVERLAY_ARCHITECTURE.md` | The cross-platform transparent overlay: per-OS techniques, Track A vs Track B, cursor-follow loop, multi-monitor/DPI, click-through, the CursorFX integration seam, performance budget. |
| `03_CONVERSATION_MODE_2.md` | UX spec, state machine, event/IPC contract, new settings, new Rust commands, barge-in, quick actions, i18n/RTL/accessibility. |
| `04_AVATAR_SPEC.md` | Visual design, state machine, audio reactivity, implementation options compared, asset strategy. |
| `05_IMPLEMENTATION_ROADMAP.md` | Phases 0–4 with concrete file-level tasks, risk register, test matrix, performance targets, definition of done. |

---

## Guiding Principles (non-negotiable)

1. **The app stays the same.** Overlay Mode is a new, optional layer — default behavior, settings, and windows are untouched. `OverlayPosition::None` users see zero change.
2. **Cross-platform mandate** (per `AGENTS.md`): every feature works on Windows 11 (primary), macOS, and Linux, with explicit documented fallbacks (especially Wayland).
3. **Never steal focus.** The overlay must be non-activating and click-through by default. Dictation into the focused app must keep working *while* the overlay is visible.
4. **Reuse before rebuild.** The brain event stream, VAD, conversation modes, wake word, mic-level fan-out, cursor position API, and per-platform window code all exist — Conversation 2.0 is mostly *plumbing + rendering*, not new pipeline work.
5. **Local-first & quiet.** No new network dependencies. The avatar is calm by default — no bouncing, no nagging, instant dismiss.
6. **Typed IPC** via tauri-specta (`cargo test export_bindings`) for every new command/event, matching project convention.
