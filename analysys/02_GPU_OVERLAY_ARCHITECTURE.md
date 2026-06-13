# 02 — Multi Cross-Platform GPU Transparent Overlay Architecture

The technical core: a transparent, always-on-top, **click-through**, non-activating, cursor-aware overlay layer on Windows 11, macOS, and Linux (X11 + Wayland), capable of GPU rendering (avatar + streaming text + FX).

---

## 1. Strategy: two tracks, one contract

Ship value early with the platform machinery S2B2S has already de-risked, while building toward the native wgpu renderer that `Cross_Platform_Rust_WebGPU_CursorFX` represents.

| | **Track A — Tauri Webview Overlay** (Phases 1–2) | **Track B — Native wgpu Overlay** (Phase 3, CursorFX) |
| --- | --- | --- |
| Window | 2nd Tauri webview window `brain_overlay` (sibling of `recording_overlay`) | winit (or raw) window owned by a dedicated render thread in the same process |
| Rendering | DOM + Canvas/WebGL (Three.js already a dep) | wgpu (Vulkan/Metal/DX12), WGSL shaders, `glyphon` for text |
| Per-pixel alpha | webview transparency (already proven) | surface `CompositeAlphaMode::PreMultiplied/PostMultiplied` |
| Click-through | `window.set_ignore_cursor_events(true)` | OS flags (see §4) + optional per-pixel hit testing |
| Cost | +1 webview (~30–80 MB), trivial dev effort, text rendering free (HTML) | ~10–20 MB, lowest latency, shader FX, but text stack + every OS quirk is ours |
| Risk | Low — reuses `overlay.rs` patterns verbatim | Medium-high — exactly what CursorFX code would de-risk |

**The contract that makes B a drop-in swap for A:** the overlay is a *dumb renderer* driven exclusively by (a) the event stream (`brain:*`, `mic-level`, `tts:level`, `overlay:*`) and (b) the `OverlayModeConfig` settings struct. No business logic lives in the overlay. Swap the renderer, keep everything else.

---

## 2. New backend module: `src-tauri/src/overlay_fx/`

```
src-tauri/src/overlay_fx/
├── mod.rs            // public API: init, show_conversation, hide, set_mode
├── window.rs         // Track A: brain_overlay window creation per OS
│                     //   (lifted from overlay.rs: NSPanel / HWND_TOPMOST / layer-shell)
├── cursor_follow.rs  // positioning service (see §3)
├── placement.rs      // bubble geometry: quadrant flip, monitor clamping, DPI
├── events.rs         // typed payloads: OverlayState, OverlayShowPayload, …
└── native/           // Track B (feature-gated `overlay-native`)
    ├── mod.rs
    ├── surface.rs    // wgpu instance/surface/config per backend
    ├── renderer.rs   // frame graph: avatar pass → text pass → fx pass
    ├── text.rs       // glyphon glyph cache
    └── platform/{win.rs, macos.rs, x11.rs, wayland.rs}
```

`overlay.rs` (the recording pill) is left untouched in Phase 1; in Phase 2 its shared helpers (`get_monitor_with_cursor`, `calculate_*`, topmost forcing) are extracted to `overlay_fx/shared.rs` and re-imported, as a pure refactor.

---

## 3. Cursor-follow positioning service (`cursor_follow.rs`)

A small loop, active **only while the overlay is visible** (zero idle cost):

1. Poll `input::get_cursor_position()` (enigo — already in tree) at 30 Hz while in `follow` mode; 0 Hz in `pinned` mode.
2. Identify monitor via the existing `get_monitor_with_cursor()` logic; convert to **logical** coordinates (project rule — tao cross-monitor physical bug).
3. Apply **placement policy** (`placement.rs`):
   - default anchor: bubble's top-left at cursor + (24, 24) logical px;
   - **quadrant flip** so the bubble never leaves the monitor (right half of screen → open leftwards; bottom half → open upwards);
   - clamp to monitor work area with 8 px margin; never overlap the recording pill's reserved band.
4. **Position freezing:** the moment the user starts speaking or a reply starts streaming, following pauses (frozen at last position) until the turn ends — text you're reading must not chase the mouse. A `Re-anchor` quick action / moving > re-anchor-threshold with the hotkey re-places it.
5. Smoothing: critically-damped spring (~120 ms settle) on the window position to avoid jitter; snap instantly on monitor change.
6. Window resize as content grows: width fixed per size setting (S/M/L = 280/360/460 logical px), height grows to `max_height` then inner-scrolls.

---

## 4. Per-platform technique matrix

### 4.1 Windows 11 (primary target)

| Requirement | Technique |
| --- | --- |
| Transparency | Tauri `transparent(true)` (Track A). Track B: `WS_EX_LAYERED` window; wgpu DX12 swapchain via **DirectComposition** for premultiplied alpha (`CompositeAlphaMode::PreMultiplied`); fallback Vulkan backend if DComp path misbehaves. |
| Always-on-top, durable | `HWND_TOPMOST` re-assert — **reuse `force_overlay_topmost()` verbatim**, call on every show + on a 2 s watchdog while visible (games/installers steal z-order). |
| Click-through | Track A: `set_ignore_cursor_events(true)` (maps to `WS_EX_TRANSPARENT`). Track B: `WS_EX_TRANSPARENT \| WS_EX_LAYERED`. Toggled off only while pointer is over an interactive region (see §5). |
| Never activate | `WS_EX_NOACTIVATE` + `SWP_NOACTIVATE` (already the pattern), `focused(false)`. |
| Hide from screen share (privacy setting, default off) | `SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE)`. |
| DPI | per-monitor-v2 awareness comes via Tauri; Track B must opt in via manifest and handle `WM_DPICHANGED`. |

### 4.2 macOS

| Requirement | Technique |
| --- | --- |
| Window class | **NSPanel via `tauri-nspanel`** — reuse the `RecordingOverlayPanel` recipe: `can_become_key_window: false`, `is_floating_panel`, `PanelLevel::Status` (raise to `.screenSaver` if needed over fullscreen video), `no_activate`. |
| All Spaces / fullscreen apps | `CollectionBehavior::can_join_all_spaces().full_screen_auxiliary()` — already proven in `overlay.rs`. |
| Click-through | `panel.set_ignores_mouse_events(true)`; flip to `false` only over interactive regions. |
| Track B rendering | `CAMetalLayer` with `isOpaque = false`, wgpu Metal backend, `PostMultiplied` alpha. |
| Screen-share privacy | `NSWindow.sharingType = .none`. |
| Permissions | cursor polling via enigo needs Accessibility — S2B2S already requests it (`AccessibilityPermissions.tsx`); add overlay to its explanation copy. |

### 4.3 Linux — X11

| Requirement | Technique |
| --- | --- |
| Transparency | ARGB visual (Tauri/WebKitGTK: requires a running compositor — detect, else fall back to opaque "card" style with solid bg). |
| Always-on-top | `_NET_WM_STATE_ABOVE` + `_NET_WM_WINDOW_TYPE_NOTIFICATION/UTILITY`; skip taskbar/pager. |
| Click-through | **XShape input region = empty** (`set_ignore_cursor_events` does this via GTK); per-region interactivity by setting the input shape to just the action-bar rect. |
| Cursor position | enigo/X11 global query — works. |

### 4.4 Linux — Wayland (degraded but honest)

Hard constraints: clients cannot read the **global** cursor position, cannot self-position arbitrary toplevels, and cannot float over other apps without a layer-shell.

- Window: **wlr-layer-shell** overlay layer (via the existing `gtk-layer-shell` path — `Layer::Overlay`, `KeyboardMode::None`, exclusive zone 0), as `overlay.rs` already does. GNOME (no layer-shell): fall back to a normal always-on-top dialog, best effort.
- **Cursor-follow is replaced by anchored placement:** user-chosen corner/edge anchor (default: bottom-right) with margins. The settings UI labels this clearly: *"Follow cursor (Windows, macOS, Linux/X11) — anchored position on Wayland."*
- Click-through: empty input region on the layer surface; interactive mode sets the input region to the action bar only.
- Env kill-switch parity: respect `S2B2S_NO_GTK_LAYER_SHELL`.

### 4.5 Capability detection

At startup, `overlay_fx::probe()` produces an `OverlayCapabilities { follow_cursor, click_through, transparency, layer_shell, capture_exclusion }` struct exposed to the frontend (typed binding) so the Settings UI can grey out unsupported options per machine instead of failing silently — same philosophy as the existing GPU/llama backend detection.

---

## 5. Click-through with interactive islands

The overlay is click-through **by default** (you can keep working "through" it). But the reply bubble has quick actions. Resolution:

1. While streaming/idle: fully click-through; all interaction is via keyboard (hotkeys: Insert / Copy / Dismiss / Open-in-app — see 03 §5).
2. A low-rate (60 ms) check compares the global cursor position against the bubble's **action-bar rect** only; on enter → `set_ignore_cursor_events(false)`, on leave → `true`. (Track B per-pixel variant: alpha-mask hit test.)
3. On Wayland (no global cursor): a thin always-interactive action bar at the bubble's edge; the rest of the surface keeps an empty input region.

---

## 6. Track B — native wgpu renderer & the CursorFX seam

Reference design (to be replaced/accelerated by `Cross_Platform_Rust_WebGPU_CursorFX` when accessible):

- **Crate layout:** vendor CursorFX as a workspace member `crates/cursorfx/` (or fork-merge into `overlay_fx/native/`). Feature flag `overlay-native`; Track A remains the fallback at runtime if surface creation fails.
- **Surface:** `wgpu::Instance` (PRIMARY backends) → surface from the raw window handle → pick `CompositeAlphaMode` from `surface.get_capabilities()`: prefer `PreMultiplied` (DX12/DComp, Metal) else `PostMultiplied` (Vulkan/X11) else inherit. All shaders output premultiplied alpha; convert at the end if the mode demands it.
- **Render loop:** *on-demand*, not continuous — request redraw on: state change, animation tick while an animation is active (cap 60 fps; idle-breathing at 24 fps; zero frames when hidden). Frame graph: `avatar pass (WGSL SDF orb) → text pass (glyphon) → fx pass (particles/trail from CursorFX shaders)`.
- **Text:** `glyphon` (cosmic-text + wgpu) — covers shaping, fallback fonts, RTL; markdown-lite (bold/code/links stripped to styled runs) done CPU-side.
- **What to lift from CursorFX on access** (checklist): window-creation flags per OS · alpha mode choices that were found to actually work per driver · cursor tracking method (poll vs hook) · DPI handling · the particle/trail pipeline · any per-pixel hit-test code.
- **Why bother (vs Track A):** sub-frame latency for cursor FX, ~50 MB less RAM, shader-native avatar identical across OSes, no WebKitGTK transparency roulette on Linux, and it operationalizes the CursorFX project instead of shelving it.

---

## 7. Performance & power budget

| Metric | Target |
| --- | --- |
| Hidden overlay cost | 0 frames, 0 timers (positioning loop stopped), webview suspended (`visibility:hidden` + rAF gated) |
| Visible idle | ≤ 24 fps avatar breathing, < 3 % of an entry-level iGPU, < 1 % CPU |
| Streaming | ≤ 60 fps, text relayout batched per animation frame (token events coalesced), no per-token reflow of prior lines |
| Memory | Track A ≤ +80 MB worst case; Track B ≤ +20 MB |
| Show latency (hotkey → avatar visible) | < 120 ms (window pre-created hidden at startup, like `recording_overlay`) |
| Battery rule | `prefers-reduced-motion` / low-power → static avatar states, no particles |

---

## 8. Failure & fallback ladder

1. Native (Track B) surface creation fails → Track A webview overlay.
2. Webview transparency unavailable (Linux no compositor) → opaque rounded "card" theme.
3. Layer-shell unsupported (GNOME Wayland) → normal on-top window, anchored.
4. Cursor position unavailable (Wayland) → anchored placement mode.
5. Overlay window creation fails entirely → Conversation 1.0 behavior (main window), with a one-time toast pointing at Settings → Overlay Mode for diagnostics. **The pipeline never breaks because the overlay can't draw.**
