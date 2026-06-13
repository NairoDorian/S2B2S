# S2B2S Evolution — The Big Note
### GPU Transparent Overlay · Conversation Mode 2.0 · Screen Vision · A 3D Face for S2B2S

> **Status:** Planning / brainstorm — June 2026. **No code yet, text only.**
> **Scope rule (non‑negotiable):** *The app stays exactly the same.* Everything here is a **new, optional Overlay Mode** layered on top. A user with overlay disabled sees zero change.

---

## 0. Why this document set exists (and what it supersedes)

There is already a planning folder in the repo at **`/analysys`** (six files, good work). This set **supersedes and extends it**, for three concrete reasons discovered by re‑reading all three repositories from source:

1. **`Cross_Platform_Rust_WebGPU_CursorFX` is now fully readable** and the old plan's central assumption was wrong. The old plan wrote *"⚠️ Not accessible … the plan proceeds on stated assumptions (winit + wgpu)"* and reserved a from‑scratch "Track B." In reality CursorFX is **Tauri V2 + Bun + React + wgpu** — the **same application shell as S2B2S** — so the integration is dramatically simpler than the old plan assumed, and several specific technical choices in the old plan are now *provably wrong* (most importantly: on Windows, **Vulkan + an NVAPI fix** is the proven transparent‑overlay path, **not** DX12/DirectComposition, which OOMs on transparent overlays). See **`02_REFERENCE_PROJECTS.md`** and **`03_GPU_OVERLAY_ARCHITECTURE.md`**.
2. **A whole pillar was missing: screen vision.** The voice brief spends a lot of time on *"the model has an understanding of the visual aspect of my computer … screenshots, region‑select rectangles … sent to a vision brain model."* The current code has **no capture path** and `ChatMessage.content` is a plain `String`. This is a first‑class feature and gets its own spec: **`05_VISION_AND_SCREEN_UNDERSTANDING.md`**.
3. **The avatar brief is bigger than "a glowing orb."** The brief asks for a **3D entity** — *"it should be a 3D entity with rotating effects where we can see that it has a back and sides … the brain should be 3D, and the ears and the eyes and the mouth … futuristic, cyberpunk, cybernetic."* The old plan's "Orbi the 2D orb" undersells this. The new avatar is a true 3D cybernetic head whose **senses map 1:1 to pipeline states**. See **`06_AVATAR_SPEC.md`**.

Everything the old plan got right (the additive philosophy, reusing `overlay.rs`, the brain event fan‑out, the `tts:level` tap, keyboard‑first quick actions, Wayland honesty) is preserved and credited here.

---

## 1. The vision in one paragraph

Today, talking to the Brain means opening the S2B2S window and using the Conversation tab. Tomorrow, **S2B2S lives on top of everything**. Press the converse hotkey (or say the wake word) in your IDE, browser, or a game, and a small, friendly, **GPU‑rendered 3D avatar** appears near your cursor. It **hears** you (its ears react to your mic), it can **see** your screen (grab a full screenshot or drag a rectangle and it goes to a vision model), it **thinks** (its brain‑core spins, a *Her*‑style curve orbits it), and the Brain's reply **streams into a glass bubble right where you're looking**, spoken aloud at the same time. One keypress inserts the answer at your cursor; one keypress dismisses it. It never steals focus, never breaks your typing. This is **Conversation Mode 2.0**, and the 3D avatar is the **face of S2B2S** — a Clippy 2.0 with real senses.

```
            TODAY (Conversation 1.0)                 TOMORROW (Conversation 2.0)
  ┌──────────────────────────────┐        ┌──────────────────────────────────────────────┐
  │  S2B2S main window           │        │  ANY app · ANY screen · ANY OS                 │
  │  ┌────────────────────────┐  │        │                                    ╔═══════╗   │
  │  │ Conversation tab       │  │        │   cursor ─►  •~~~~~~~~~~~~~~~~~► ◖ 3D avatar ◗  │
  │  │ chat transcript        │  │   ──►  │              (spring trail)        ╚═══╤═══╝   │
  │  │ [mic] [text input]     │  │        │                       ╔═══════════════╧══════╗ │
  │  └────────────────────────┘  │        │                       ║ Sure — the fix is…   ║ │
  └──────────────────────────────┘        │                       ║ ▍streaming tokens    ║ │
     you go to the app                    │                       ╚══════════════════════╝ │
                                          │       the app comes to you                     │
                                          └──────────────────────────────────────────────┘
```

---

## 2. The four pillars

| # | Pillar | Doc | One‑liner |
| - | --- | --- | --- |
| 1 | **Multi cross‑platform GPU transparent overlay** | `03` | Always‑on‑top, transparent, click‑through, cursor‑aware layer on Windows / macOS / Linux, GPU‑rendered. |
| 2 | **Conversation Mode 2.0** | `04` | The Brain's existing streaming events fanned out to the overlay: reply bubble at the cursor, quick actions, barge‑in, hands‑free — *zero changes to brain logic.* |
| 3 | **Screen Vision ("the eyes")** | `05` | Full‑screen and rectangle‑region screenshots → multimodal Brain turn → vision model answers about what's on screen. **New capability.** |
| 4 | **The 3D Avatar ("the face & senses")** | `06` | A rotating, cyberpunk 3D entity — brain, eyes, ears, mouth — each sense bound to a real pipeline signal. |

These are not independent: the **avatar is the visible state machine** for pillars 1–3, the **overlay is its stage**, and the **trail** (from your two cursor‑FX projects) tethers it to your cursor.

---

## 3. How the three repositories combine (the synthesis)

You gave me three projects. They are not redundant — each contributes a distinct layer:

```
   ┌─────────────────────────────────────────────────────────────────────────┐
   │                         S2B2S  Overlay Mode                               │
   │                                                                           │
   │   S2B2S (this repo)            CursorFX                 TD_Web_Trail       │
   │   ───────────────────          ───────────────          ─────────────     │
   │   • Brain event stream         • Tauri V2 + wgpu         • Spring‑friction │
   │     (brain:thinking/token/       transparent, click‑       physics chain   │
   │      sentence/done/error)        through overlay        • Multi‑pass glow  │
   │   • VAD / STT / TTS            • Per‑OS window flags       ribbon render    │
   │   • Conversation modes          (WndProc, NSPanel,      • Catmull‑Rom /    │
   │   • enigo cursor + paste         layer‑shell)             Bézier splines   │
   │   • overlay.rs window           • NVAPI Vulkan fix      • Binary 8‑byte    │
   │     machinery (proven)        • wgpu ribbon + SDF         stream protocol  │
   │   • HerLoading.tsx (Three.js)    circle pipelines         (latency recipe) │
   │           │                          │                         │          │
   │           ▼                          ▼                         ▼          │
   │     the PIPELINE  ⊕  the cross‑platform GPU WINDOW  ⊕  the TRAIL aesthetic │
   │                              ⊕  a NEW 3D AVATAR                            │
   └─────────────────────────────────────────────────────────────────────────┘
```

- **S2B2S** is the brain, voice, and the *already‑solved* hard parts of cross‑platform overlay windows (`overlay.rs`).
- **CursorFX** is the proven recipe for a **native, cross‑platform, transparent, click‑through wgpu overlay** — the exact thing pillar 1 needs, already debugged on Windows/macOS/Linux.
- **TD_Web_Trail** is the **physics + rendering recipe** for the cursor→avatar **tether** and any ambient cursor trail, plus a battle‑tested **low‑latency streaming protocol** idea reusable for the avatar's audio/state IPC.

---

## 4. The one decision that shapes everything: how to render

You said *"I think WebGPU is the solution."* You're right — but "WebGPU" has **two legitimate meanings here**, and the honest answer is *use both, for what each is best at.* Full analysis in `03 §2`; the short version:

| Option | What it is | Best for | Cross‑platform reality |
| --- | --- | --- | --- |
| **Native wgpu** (vendor CursorFX) | Rust `wgpu` → Vulkan/Metal/DX12, rendering into a transparent Tauri window's surface | Ambient **cursor trail + particles + glow**; lowest latency; tiny RAM | Proven on all 3 OSes by CursorFX (Windows = Vulkan+NVAPI fix) |
| **Three.js `WebGPURenderer`** in a transparent webview | Browser WebGPU (falls back to WebGL2) via the Three.js you *already ship* | The **rich 3D avatar** + the **DOM text bubble** (i18n, RTL, markdown for free) | WebGPU works in WebView2 (Windows, your #1 target); WebGL2 fallback on macOS/Linux webviews |

**Recommendation:** ship the **avatar + bubble in a transparent webview** with `WebGPURenderer` (WebGL2 fallback) first — fastest path to the face you want, reuses `HerLoading` DNA and all your i18n/markdown. Add the **native wgpu CursorFX layer for the cursor trail/FX** as the second track. This is genuinely "WebGPU everywhere" *and* it ships early.

---

## 5. Document map

| File | Contents |
| --- | --- |
| `00_README_START_HERE.md` | **This file** — the big note: vision, pillars, repo synthesis, the rendering decision, principles. |
| `01_S2B2S_REVIEW.md` | What S2B2S actually is today, verified from source: the brain event stream, `overlay.rs`, Conversation 1.0, input/paste, TTS, settings, the avatar DNA — and the honest gaps. |
| `02_REFERENCE_PROJECTS.md` | What `TD_Web_Trail` and `CursorFX` actually are (from source) and the **exact techniques to lift** from each. Corrects the old plan's CursorFX assumptions. **Both repos are now cloned at `../../TD_Web_Trail/` and `../../Cross_Platform_Rust_WebGPU_CursorFX/` for live reference.** |
| `03_GPU_OVERLAY_ARCHITECTURE.md` | The cross‑platform transparent overlay: rendering decision, two tracks, per‑OS technique matrix (corrected), cursor‑follow, click‑through islands, multi‑monitor/DPI, perf budget, failure ladder. |
| `04_CONVERSATION_MODE_2.md` | UX, state machine, the IPC/event contract (reusing `brain:*`), the bubble, keyboard‑first quick actions, new settings, barge‑in, coexistence, edge cases. |
| `05_VISION_AND_SCREEN_UNDERSTANDING.md` | **New pillar.** Screen capture (full + rectangle), the multimodal `ChatMessage` upgrade, vision‑model settings, cross‑platform capture (incl. Wayland portal), privacy, the "eyes" tie‑in. |
| `06_AVATAR_SPEC.md` | The **3D** avatar: character brief, the senses↔pipeline map, the cyberpunk visual system, rendering (Three.js → WGSL), audio reactivity, the cursor tether, config, skins, acceptance criteria. |
| `07_IMPLEMENTATION_ROADMAP.md` | Phases 0–5 with file‑level tasks, risk register, test matrix, performance targets, definition of done. |
| `08_TRANSPARENT_OVERLAY_IMPL_PLAN.md` | **Concrete code‑level plan.** Exact file map, code patterns from CursorFX + TD_Web_Trail, exact APIs to use, per‑phase deliverables, performance budget, risk register. **Read this when ready to code.** |

Suggested reading order: **00 → 01 → 02 → 03 → 04 → 05 → 06 → 07 → 08**. If you only read two: this file and `03`. **If you're coding: read `08` first** (it bridges the analysis to actual code patterns from the cloned repos).

---

## 6. Guiding principles (carry through every doc)

1. **The app stays the same.** Overlay Mode is additive and optional. Default settings, windows, tray, and the Conversation tab are untouched. New code lands in **new files/modules**; existing files get only tiny, additive registration touch‑points.
2. **Cross‑platform mandate** (`AGENTS.md`): every feature works on **Windows 11 (priority #1)**, macOS, and Linux, with **explicit, documented fallbacks** — especially Wayland (no global cursor; screencast via portal).
3. **Never steal focus, never break typing.** The overlay is non‑activating and click‑through by default. Dictation into the focused app must keep working *while* the overlay is visible. This is the cardinal sin to never commit.
4. **Reuse before rebuild.** The brain event stream, VAD, conversation modes, wake word, mic‑level fan‑out, `enigo` cursor/paste, the per‑OS window code, and Three.js all already exist. Conversation 2.0 is mostly *plumbing + rendering*.
5. **Local‑first & calm.** No new network dependencies for core features. The avatar breathes; it does not bounce, nag, or demand attention. Instant dismiss, always.
6. **Privacy is explicit.** Screen capture is opt‑in, visible (the avatar's eyes light up while it "sees"), never silent, and honors a screen‑share exclusion setting.
7. **Typed IPC** via `tauri‑specta` (`cargo test export_bindings`) for every new command and event — matching the existing convention.
8. **Honesty over hype.** Where a platform can't do something (Wayland global cursor, exclusive‑fullscreen capture), the plan says so and degrades gracefully rather than pretending.

---

## 7. What's genuinely new vs. what already exists (at a glance)

| Capability | Already in S2B2S? | Work needed |
| --- | --- | --- |
| Cross‑platform transparent on‑top window | ✅ `overlay.rs` (recording pill) | Add click‑through + cursor‑follow + a second window |
| Brain streaming events | ✅ `brain:thinking/token/sentence/done/error` | **None** — just add the overlay as a listener |
| Cursor position + paste at cursor | ✅ `input.rs` (`enigo`) | Reuse for "Insert at cursor" + follow |
| Background residency (model stays loaded when hidden) | ✅ `api.prevent_close()` → hide‑to‑tray | **None** — overlay just must work while main window is hidden |
| 3D rendering in‑app | ✅ `HerLoading.tsx` (Three.js, alpha) | Grow into the avatar |
| Native wgpu transparent overlay | ⚠️ In **CursorFX**, not yet in S2B2S | Vendor CursorFX as the trail/FX track |
| **Screen capture (full + region)** | ❌ | **New subsystem** (`05`) |
| **Multimodal Brain (images)** | ❌ (`content: String`) | **Upgrade `ChatMessage`** (`05`) |
| **`tts:level` (speaking amplitude)** | ❌ | Tiny RMS tap in `tts/player.rs` (`06`) |
| **3D avatar with senses** | ❌ | **New** (`06`) |

Most of the foundation is already there. The four pillars are reach, not rebuild.
