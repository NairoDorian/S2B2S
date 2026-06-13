# 03 — Multi Cross‑Platform GPU Transparent Overlay Architecture

The technical core of pillar 1: a transparent, always‑on‑top, **click‑through**, non‑activating, cursor‑aware overlay on **Windows 11 (priority #1)**, macOS, and Linux (X11 fully; Wayland in a documented degraded mode), capable of GPU rendering (3D avatar + streaming text + cursor FX).

---

## 1. The rendering decision (resolved)

You said *"I think WebGPU is the solution."* It is — but there are **two distinct ways to get WebGPU on the desktop**, and they are good at *different* things. The honest plan uses **both**.

### 1.1 The three candidate renderers

| | **R1 — Native `wgpu`** (vendor CursorFX) | **R2 — Three.js `WebGPURenderer` in a webview** | **R3 — Canvas 2D / WebGL fallback** |
| --- | --- | --- | --- |
| API | Rust `wgpu` → Vulkan/Metal/DX12 | Browser WebGPU (auto WebGL2 fallback) | 2D/WebGL |
| Lives in | A render thread against a transparent Tauri window surface | A transparent Tauri webview window | same |
| Great at | Ambient **cursor trail, particles, glow**; lowest latency; ~10–20 MB | Rich **3D avatar** + **DOM text bubble** (i18n/RTL/markdown free); reuses `HerLoading` | Guaranteed‑works fallback |
| 3D character | Hard (hand‑written meshes/raymarch in WGSL) | **Easy** (Three.js meshes, GLTF, TSL shaders) | Limited |
| Text (20 locales, RTL, markdown) | Hard (`glyphon` + manual layout) | **Free** (HTML/CSS) | Free |
| Cross‑platform transparency | **Proven** by CursorFX (Win=Vulkan+NVAPI) | webview transparency (proven by `overlay.rs`) | proven |
| WebGPU availability | Always (native) | **WebView2 (Windows ✅)**; WebKit (macOS/Linux) still maturing → **WebGL2 fallback** | n/a |
| RAM | ~10–20 MB | ~30–80 MB (a webview) | ~30–80 MB |

### 1.2 The recommendation

**Hybrid, in two tracks (also the shipping order):**

- **Track A first — the avatar + bubble in a transparent webview (R2 → R3 fallback).** This is the fastest path to the face you want. It reuses `HerLoading.tsx`'s Three.js, gets the i18n/RTL/markdown text bubble for free, and uses `WebGPURenderer` where the webview supports it (true WebGPU on Windows/WebView2 — your #1 target) with an automatic **WebGL2 fallback** elsewhere. Ships the whole user‑visible experience early.
- **Track B next — the native `wgpu` cursor‑FX layer (R1), vendored from CursorFX.** A full‑screen, click‑through, ultra‑cheap native overlay for the **ambient cursor trail, glow, and the cursor→avatar tether** — where native wgpu's latency and RAM win, and where CursorFX already solved every OS quirk.

This is genuinely **"WebGPU everywhere"**: WebGPU for the avatar (WebView2/WebGPURenderer) **and** native wgpu for the FX — while still shipping in weeks, not quarters. If you'd rather have a single window, two simplifications are valid (see `§1.3`).

### 1.3 Two valid simplifications (if you prefer fewer moving parts)

- **Single‑webview everything (R2 only).** Put the avatar, the bubble, *and* the trail all inside one transparent webview, drawing the trail on a WebGPU/WebGL canvas behind the avatar (porting the TD_Web_Trail physics to the canvas). Simplest to build and ship; you lose only the last bit of native‑wgpu latency/RAM advantage. **Good default if Track B ever feels like too much.**
- **All‑native (R1 only).** Render the avatar in WGSL too (raymarched SDF head, `glyphon` text). Maximum performance and the purest "WebGPU" story, but the 3D character + 20‑locale text is materially more work. **Best as a long‑term endgame, not a starting point.**

The architecture below is written so these are **runtime/feature switches, not rewrites** — the overlay is a *dumb renderer* (see `§1.4`).

### 1.4 The contract that keeps renderers swappable

The overlay holds **no business logic**. It is driven entirely by **(a)** the event stream (`brain:*`, `mic-level`, `tts:level`, `overlay:*`, `vision:*`) and **(b)** the `OverlayModeConfig` settings struct. Swap R1↔R2↔R3, keep everything else. Same principle the old plan had — preserved.

---

## 2. New backend module: `src-tauri/src/overlay_fx/`

```
src-tauri/src/overlay_fx/
├── mod.rs            // public API: init, show_conversation, hide, set_mode, probe
├── window.rs         // Track A: brain_overlay window creation per OS
│                     //   (lifted from overlay.rs: NSPanel / HWND_TOPMOST / layer-shell)
├── cursor_follow.rs  // positioning service (§3) — reuses input::get_cursor_position
├── placement.rs      // bubble geometry: quadrant flip, monitor clamp, DPI
├── events.rs         // typed payloads: OverlayState, OverlayShowPayload, …
├── capabilities.rs   // OverlayCapabilities probe (§4.5)
└── native/           // Track B (feature `overlay-native`) — vendored CursorFX
    ├── mod.rs        //   render thread, surface lifecycle (SurfaceTargetUnsafe::RawHandle)
    ├── platform.rs   //   Windows WndProc subclass + WS_EX styles + NVAPI fix (verbatim CursorFX)
    ├── renderer.rs   //   ribbon (trail) + circle (particles) pipelines (from CursorFX)
    └── shader.wgsl   //   SDF + ribbon shaders
```

`overlay.rs` (the recording pill) is **untouched in Phase 1**. In Phase 2 its shared helpers (`get_monitor_with_cursor`, `calculate_*`, `force_overlay_topmost`) are extracted to `overlay_fx/shared.rs` and re‑imported by both — a **pure refactor** with byte‑identical pill behavior.

---

## 3. Cursor‑follow positioning service (`cursor_follow.rs`)

A small loop, active **only while the overlay is visible** (zero idle cost):

1. Poll `input::get_cursor_position()` (enigo, already in tree) at **30 Hz** in `follow` mode; **0 Hz** in `pinned`/`anchored` mode.
2. Identify the monitor via the existing `get_monitor_with_cursor()`; convert to **logical** coordinates (project rule: tao's cross‑monitor *physical* position bug).
3. Apply the **placement policy** (`placement.rs`):
   - default anchor = bubble top‑left at cursor + `(24, 24)` logical px;
   - **quadrant flip** so the bubble never leaves the monitor (cursor on the right half → open leftwards; bottom half → open upwards);
   - clamp to the monitor work area with an 8 px margin; never overlap the recording pill's reserved band.
4. **Position freezing:** the instant the user starts speaking *or* a reply starts streaming, following **pauses** (frozen at the last position) until the turn ends — text you're reading must not chase the mouse. A `Re‑anchor` action (or moving past a threshold then re‑triggering) re‑places it.
5. **Smoothing:** a critically‑damped spring (~120 ms settle) on the window position to avoid jitter (this is the TD_Web_Trail lazy‑brush idea applied to the *window*, not the trail); snap instantly on a monitor change.
6. **Resize as content grows:** width fixed per size setting (S/M/L = 280/360/460 logical px); height grows to `max_height` then inner‑scrolls.

> The **avatar itself** can lead the bubble slightly and **the trail connects the cursor to the avatar** (`06 §7`) — so even when the bubble is frozen for reading, the cursor stays visually linked to S2B2S.

---

## 4. Per‑platform technique matrix (corrected)

### 4.1 Windows 11 (primary target)

| Requirement | Technique |
| --- | --- |
| Transparency (Track A) | Tauri `transparent(true)` (proven by `overlay.rs`). |
| Transparency (Track B native wgpu) | **Vulkan backend** + surface `CompositeAlphaMode::PostMultiplied`/`PreMultiplied` from `caps`. **Do NOT use DX12** for the transparent overlay — CursorFX confirms DX12 OOMs on transparent surfaces (RTX 4070). Apply the **NVAPI "Prefer Native" present fix** (`02 §A.3.3`) so NVIDIA doesn't DXGI‑wrap Vulkan and kill transparency. |
| Always‑on‑top, durable | `HWND_TOPMOST` re‑assert — **reuse `force_overlay_topmost()` verbatim**; re‑apply on every show + a ~2 s watchdog while visible (games/installers steal Z‑order). For Track B, re‑apply the **WS_EX styles + WndProc** every frame (CursorFX pattern). |
| Click‑through | Track A: `set_ignore_cursor_events(true)` (→ `WS_EX_TRANSPARENT`). Track B: `WS_EX_TRANSPARENT \| WS_EX_LAYERED \| WS_EX_TOPMOST \| WS_EX_TOOLWINDOW \| WS_EX_NOACTIVATE` + WndProc `WM_NCHITTEST → HTTRANSPARENT` (verbatim CursorFX). Toggled off only over interactive regions (`§5`). |
| Never activate | `WS_EX_NOACTIVATE` + `SWP_NOACTIVATE` (already the pattern) + `focused(false)`. |
| Hide from screen share (opt‑in setting, default off) | `SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE)`. |
| DPI | per‑monitor‑v2 via Tauri; Track B handles `WM_DPICHANGED`. |

### 4.2 macOS

| Requirement | Technique |
| --- | --- |
| Window class | **NSPanel via `tauri-nspanel`** — reuse the `RecordingOverlayPanel` recipe: `can_become_key_window: false`, `is_floating_panel`, `PanelLevel::Status` (raise to `.screenSaver` to sit over fullscreen video), `no_activate(true)`. |
| All Spaces / fullscreen | `CollectionBehavior::can_join_all_spaces().full_screen_auxiliary()` — proven in `overlay.rs`. |
| Click‑through | `panel.set_ignores_mouse_events(true)`; flip to `false` only over interactive regions. |
| Track B rendering | `CAMetalLayer` with `isOpaque = false`, wgpu **Metal** backend, `PostMultiplied` alpha. |
| Screen‑share privacy | `NSWindow.sharingType = .none`. |
| Permissions | cursor polling (enigo) + screen capture both need **Accessibility / Screen Recording** — S2B2S already requests Accessibility (`tauri-plugin-macos-permissions`); add Screen‑Recording copy for vision (`05`). |

### 4.3 Linux — X11

| Requirement | Technique |
| --- | --- |
| Transparency | ARGB visual (needs a running compositor — detect; else fall back to an opaque rounded "card" theme). |
| Always‑on‑top | `_NET_WM_STATE_ABOVE` + `_NET_WM_WINDOW_TYPE_NOTIFICATION/UTILITY`; skip taskbar/pager. |
| Click‑through | **XShape empty input region** (`set_ignore_cursor_events` does this via GTK); per‑region interactivity = set the input shape to just the action‑bar rect. |
| Cursor position | enigo/X11 global query — works. |

### 4.4 Linux — Wayland (degraded but honest)

Hard constraints: clients can't read the **global** cursor position, can't self‑position arbitrary toplevels, and can't float over other apps without a layer‑shell.

- **Window:** **wlr‑layer‑shell** overlay layer via the existing `gtk-layer-shell` path (`Layer::Overlay`, `KeyboardMode::None`, exclusive zone 0). GNOME (no layer‑shell): fall back to a normal always‑on‑top dialog, best effort. Respect `S2B2S_NO_GTK_LAYER_SHELL`.
- **Cursor‑follow → anchored placement:** a user‑chosen corner/edge anchor (default bottom‑right) with margins. The Settings UI labels this clearly: *"Follow cursor (Windows, macOS, Linux/X11) — fixed anchor on Wayland."*
- **Click‑through:** empty input region on the layer surface; interactive mode sets the input region to the action bar only.
- **Screen capture on Wayland:** via the **`org.freedesktop.portal.ScreenCast`/`Screenshot` XDG portal** (PipeWire), not raw X11 grabs — see `05 §4`.

### 4.5 Capability detection

At startup `overlay_fx::probe()` returns an `OverlayCapabilities { follow_cursor, click_through, transparency, layer_shell, capture_exclusion, webgpu_in_webview, native_wgpu }` struct exposed to the frontend (typed binding) so the Settings UI **greys out unsupported options per machine** instead of failing silently — the same philosophy as the existing GPU/llama backend detection.

---

## 5. Click‑through with interactive islands

Click‑through **by default** (you keep working "through" the overlay). But the bubble has quick actions. Resolution:

1. While streaming/idle: **fully click‑through**; all interaction is keyboard (hotkeys: Insert / Copy / Dismiss / Open‑in‑app — `04 §5`).
2. A low‑rate (~60 ms) check compares the global cursor position to the bubble's **action‑bar rect only**; on enter → `set_ignore_cursor_events(false)`, on leave → `true`. (Track B per‑pixel variant: alpha‑mask hit test.)
3. On Wayland (no global cursor): a thin **always‑interactive action bar** at the bubble's edge; the rest keeps an empty input region.

Because the design is **keyboard‑first**, the mouse is optional — if the island toggling ever feels flaky (e.g. over remote desktop), pure click‑through is still fully usable.

---

## 6. Track B — the native wgpu layer & the CursorFX seam

- **Vendor:** add CursorFX as a workspace member `crates/cursorfx/` (or fork‑merge into `overlay_fx/native/`), behind feature `overlay-native`. **Regenerate `Cargo.lock` to wgpu 29** (`02 §A.4`). Track A remains the runtime fallback if native surface creation fails.
- **Surface:** `wgpu::Instance` (all backends) → `create_surface_unsafe(SurfaceTargetUnsafe::RawHandle{…})` from the transparent Tauri overlay window → pick an `is_srgb()` format and a `PostMultiplied`/`PreMultiplied` alpha mode from `caps`. All shaders output premultiplied alpha.
- **Render loop:** on‑demand (CursorFX's model) — redraw on state change / active animation / cursor move; idle‑breathing at ~24 fps; **zero frames when hidden**; recreate surface on `Outdated/Lost`.
- **What Track B renders here:** the **cursor trail + glow + particles + the cursor→avatar tether** (CursorFX ribbon + circle pipelines, TD_Web_Trail physics). The **avatar and text** stay in Track A's webview unless/until you go all‑native (R1, `§1.3`).
- **Text (only if all‑native):** `glyphon` (cosmic‑text + wgpu) for shaping/fallback fonts/RTL; markdown‑lite styled runs CPU‑side.

---

## 7. Performance & power budget

| Metric | Target |
| --- | --- |
| Hidden overlay | 0 frames, 0 timers (follow loop stopped), webview suspended (`visibility:hidden` + rAF gated), native render thread parked |
| Visible idle | ≤ 24 fps avatar breathing; native trail idle‑sleeps after 2 still frames (TD_Web_Trail rule); < 3% of an entry‑level iGPU, < 1% CPU |
| Streaming | ≤ 60 fps; **token events coalesced per `rAF`** (no per‑token reflow of prior lines); append‑only text buffer |
| Memory | Track A ≤ +80 MB worst case; Track B ≤ +20 MB |
| Show latency (hotkey → avatar visible) | < 120 ms (window pre‑created hidden at startup, like `recording_overlay`) |
| Battery rule | `prefers-reduced-motion` / low‑power → static avatar states, no particles, no trail |

---

## 8. Failure & fallback ladder

1. Native (Track B) surface creation fails → Track A webview overlay (avatar/bubble still work).
2. `WebGPURenderer` unavailable in the webview → **WebGL2** renderer (R3) — automatic in Three.js.
3. Webview transparency unavailable (Linux, no compositor) → opaque rounded **"card"** theme.
4. Layer‑shell unsupported (GNOME Wayland) → normal on‑top window, anchored.
5. Cursor position unavailable (Wayland) → **anchored** placement mode.
6. Overlay window creation fails entirely → **Conversation 1.0** behavior (main window), with a one‑time toast pointing at Settings → Overlay Mode for diagnostics.

**The pipeline never breaks because the overlay can't draw.** Every rung degrades to something usable; the bottom rung is exactly today's behavior.
