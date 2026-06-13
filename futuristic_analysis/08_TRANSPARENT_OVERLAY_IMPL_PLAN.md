# 08 — Transparent Overlay Implementation Plan

**Synthesized from:** `00–07` analysis + live source reading of `Cross_Platform_Rust_WebGPU_CursorFX` & `TD_Web_Trail`.  
**Goal:** Give S2B2S a transparent, click-through, always-on-top overlay with a 3D avatar + streaming reply bubble — without touching the existing app.

---

## 0. Architecture (the "two-track" overlay)

```
                                App Shell (Tauri 2)
                                      │
              ┌───────────────────────┼───────────────────────┐
              ▼                       ▼                       ▼
        Main window           Track A Webview            Track B Native wgpu
     (unchanged today       (avatar + bubble)           (cursor trail + FX)
      + Conversation 1.0)    Three.js / WebGPU          CursorFX ribbon + SDF
                              ┌─────┐                    ┌──────────────────┐
            ▲                 │Her  │                    │  spring chain    │
            │ listen only     │core │                    │  Catmull-Rom     │
     ┌──────┴──────┐          │eyes │ ←───── tether ──── │  4-pass glow     │
     │  Brain      │          │ears │                    │  NVAPI fix       │
     │  TTS        │          │mouth│                    │  Vulkan surface  │
     │  STT        │          ├─────┤                    └──────────────────┘
     │  VAD        │          │bubble│ ←─ streaming text
     │  mic-level   │          ├─────┤
     └─────────────┘          │actions│ ←─ Insert / Copy / Esc
                               └─────┘
```

- **Track A (Phase 1):** Transparent Tauri webview — fastest path, reuses `overlay.rs` recipes + `HerLoading.tsx` DNA.
- **Track B (Phase 4):** Native wgpu layer — vendored from CursorFX, for the ambient cursor trail + tether.
- **Both listen to the same event bus** (`brain:*`, `mic-level`, `tts:level`). Zero brain logic changes.

---

## 1. Phase 0 — Groundwork (invisible, 1–2 days)

### 1.1 Add config structs to `settings.rs`

```rust
// ADD to settings.rs — same serde-default pattern as BrainConfig

pub struct OverlayModeConfig {
    pub enabled: bool,           // default false → app byte-identical to today
    pub trigger: String,         // "converse_hotkey" | "wake_word" | "both"
    pub placement: String,       // "follow" | "pinned" | "anchored"
    pub anchor_corner: String,   // for anchored/Wayland: "br" | "bl" | "tr" | "tl"
    pub size: String,            // "S" | "M" | "L" → 280/360/460 logical px bubble width
    pub auto_hide_secs: u32,     // 0 = never; fade after N s inactivity
    pub exclude_from_capture: bool,
    pub reduced_motion: bool,
    pub show_trail: bool,        // Track B toggle
    pub renderer: String,        // "auto" | "webgpu" | "webgl" | "native"
    pub avatar: AvatarConfig,
    pub vision: VisionConfig,
}

pub struct AvatarConfig {
    pub style: String,           // "cyber" | "orb" | "glyph"
    pub size: String,            // "S" | "M" | "L" → 56/72/96 logical px
    pub accent: String,          // hex neon accent
    pub show_eyes: bool,
    pub show_tether: bool,
    pub face_toward_cursor: bool,
    pub idle_rotation: bool,
    pub reduced_motion: bool,
    pub quality: String,         // "auto" | "high" | "lite"
}

pub struct VisionConfig {
    pub enabled: bool,
    pub model_id: Option<String>,
    pub default_mode: String,    // "region" | "full"
    pub max_long_edge_px: u32,   // default 1568
    pub format: String,          // "auto" | "png" | "jpeg"
    pub max_images_per_turn: u8, // default 1
    pub keep_after_turn: bool,
    pub redact_prompt: bool,
}

// All with #[serde(default)] — existing values unchanged
```

### 1.2 The `tts:level` RMS tap (the only new audio code)

File: `src-tauri/src/tts/player.rs`

```rust
// ADD a rodio Source wrapper (~30 lines):
struct RmsSource<I: Source<Item = f32>> {
    inner: I,
    app_handle: AppHandle,
    accumulator: f32,
    sample_count: u32,
    samples_per_emit: u32,  // sample_rate / 30 (≈30 Hz)
}

impl<I: Source<Item = f32>> Iterator for RmsSource<I> {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        self.inner.next().map(|sample| {
            self.accumulator += sample * sample;
            self.sample_count += 1;
            if self.sample_count >= self.samples_per_emit {
                let rms = (self.accumulator / self.sample_count as f32).sqrt();
                let _ = self.app_handle.emit("tts:level", rms);
                self.accumulator = 0.0;
                self.sample_count = 0;
            }
            sample  // pass through unchanged — no audible effect
        })
    }
}

// On playback stop, emit tts:level(0.0) so the avatar's mouth closes
```

### 1.3 Skeleton module `overlay_fx/`

```
src-tauri/src/overlay_fx/
├── mod.rs              // init(), show_conversation(), hide(), probe()
├── window.rs           // brain_overlay window creation per OS
├── cursor_follow.rs    // 30 Hz follow loop (reuses enigo)
├── placement.rs        // bubble geometry, quadrant flip, DPI clamp
├── events.rs           // OverlayState enum + event payloads
├── capabilities.rs     // OverlayCapabilities probe per OS
└── native/             // Track B (feature-gated, compiles as stub for now)
    ├── mod.rs
    ├── platform.rs
    ├── renderer.rs
    └── shader.wgsl
```

### 1.4 Typed bindings

```
cargo test export_bindings   # regenerates src/bindings.ts with new types
```

**Exit check:** app builds and behaves exactly as today. `OverlayModeConfig.enabled = false` everywhere.

---

## 2. Phase 1 — Avatar v1 + Converse Loop (Track A, the visible win, 1–2 weeks)

### 2.1 Create the `brain_overlay` window (exact CursorFX + overlay.rs hybrid)

File: `overlay_fx/window.rs`

```rust
pub fn create_brain_overlay(app: &AppHandle) -> Result<WebviewWindow, Box<dyn Error>> {
    let mut builder = WebviewWindowBuilder::new(
        app,
        "brain_overlay",
        WebviewUrl::External("about:blank".parse().unwrap()),
    )
    .transparent(true)          // ALPHA compositing
    .decorations(false)         // no chrome
    .always_on_top(true)        // Z-order topmost
    .skip_taskbar(true)         // not in taskbar
    .focused(false)             // never steal focus
    .visible(false);            // hidden at startup, shown on trigger

    // OS-specific window flags (lifted from overlay.rs):
    #[cfg(target_os = "macos")] {
        // tauri-nspanel: PanelLevel::Status, no_activate, can_join_all_spaces
    }
    #[cfg(target_os = "windows")] {
        // HWND_TOPMOST + force_overlay_topmost() re-assert pattern
        // set_ignore_cursor_events(true) for click-through
    }
    #[cfg(target_os = "linux")] {
        // GTK layer-shell or normal topmost fallback
    }

    let window = builder.build()?;

    // IMMEDIATELY set click-through (reuse CursorFX pattern)
    let _ = window.set_ignore_cursor_events(true);

    Ok(window)
}
```

**Key: reuse `overlay.rs` patterns verbatim:**
- `force_overlay_topmost()` — Win32 `SetWindowPos(HWND_TOPMOST, …, SWP_NOACTIVATE)`
- NSPanel `can_become_key_window: false`, `no_activate(true)`
- GTK `Layer::Overlay`, `KeyboardMode::None`

### 2.2 Cursor-follow positioning service

File: `overlay_fx/cursor_follow.rs`

```rust
pub struct CursorFollow {
    // 30 Hz poll loop — uses enigo::get_cursor_position() (already in tree)
    // Gets monitor via get_monitor_with_cursor() (already in overlay.rs)
    // Converts to logical coordinates (tao cross-monitor physical bug rule)
}

pub fn start_follow(window_label: &str, mode: FollowMode) { /* ... */ }
pub fn stop_follow() { /* ... */ }
```

File: `overlay_fx/placement.rs`

```rust
// Quadrant flip: cursor on right half → bubble opens leftwards
// Bottom half → bubble opens upwards
// Clamp to monitor work area with 8 px margin
// Never overlap recording pill's top/bottom band
// DPI-safe: always LogicalPosition
```

### 2.3 Avatar 3D rendering (Three.js in the webview)

File: `src/brain-overlay/avatar/Avatar.tsx`

```typescript
// extends HerLoading.tsx DNA:
// - WebGPURenderer (with WebGL2 auto-fallback in Three.js 0.184)
// - alpha: true, premultipliedAlpha
// - CatmullRomCurve3 orbit (the "Her curve" — shared visual language)

// 7 states driven ONLY by events (dumb renderer contract):
// IDLE → LISTENING → THINKING → SPEAKING → DONE → ERROR → HIDDEN

// Senses wiring:
// - Ears: mic-level (already fanned out by emit_levels)
// - Brain: brain:thinking → core spin-up + Her-curve orbit
// - Eyes: vision:capture-started → brighten + scanline (Phase 3)
// - Mouth: tts:level → waveform/aperture
```

### 2.4 Reply bubble

File: `src/brain-overlay/bubble/ReplyBubble.tsx`

```
- Glass/frosted panel, cyberpunk accent tint
- Streaming append (coalesce per rAF — no per-token reflow)
- Markdown-lite (bold, code, links — free in the webview)
- Header: "🎤 you said: ..." + stt_ms
- Footer: "42 t/s · 1.3 s · ⚡280 ms" metrics chips
- RTL: dir from getLanguageDirection (mirror bubble + actions)
- Long replies: grow to max_height, then inner-scroll
```

### 2.5 Converse trigger + command handlers

File: `src-tauri/src/commands/overlay.rs`

```rust
#[tauri::command]
#[specta]
fn overlay_converse_trigger(app: AppHandle, mode: String) -> Result<(), String> {
    // 1. Get cursor position + monitor (reuse overlay.rs helpers)
    // 2. emit("overlay:show-conversation", { anchor, mode })
    // 3. Show pre-created brain_overlay window
    // 4. Drive existing continuous_voice / BrainManager::ask
    //    (NO brain changes — just call the same functions Conversation 1.0 calls)
}

#[tauri::command]
#[specta]
fn overlay_insert_at_cursor(app: AppHandle, text: String) -> Result<(), String> {
    // Reuse input.rs paste pipeline verbatim
}

#[tauri::command]
#[specta]
fn overlay_dismiss(app: AppHandle) -> Result<(), String> {
    // Fade → hide; abort if streaming (reuse current_abort)
}
```

### 2.6 Extend mic-level fan-out (one line)

File: `src-tauri/src/overlay.rs` → `emit_levels()`

```rust
// ADD one line:
let _ = app.emit_to("brain_overlay", "mic-level", &levels);
// existing:
let _ = app.emit_to("recording_overlay", "mic-level", &levels);
let _ = app.emit_to("main", "mic-level", &levels);
```

### 2.7 Quick-actions keyboard layer

```rust
// Registered ONLY while the overlay is shown, deregistered on hide:
// Enter (with overlay-modifier) → Insert at cursor
// C → Copy reply   |  O → Open in Conversation tab
// R → Regenerate   |  P → Pin/unpin
// S → Screenshot    |  Esc → Dismiss / barge-in
// ↑/↓ → Scroll bubble
```

### 2.8 Coexistence: suppress recording pill while conversing

```
While brain_overlay is visible:
  - Hide the recording pill (the avatar subsumes its states)
  - If overlay is off, the pill behaves exactly as today
```

**Exit:** converse hotkey → avatar appears at cursor → hears you → thinks → streams reply + speaks it → Insert types at cursor → Esc dismisses. Main window can stay in the tray.

---

## 3. Phase 2 — Conversation 2.0 Polish + Shared-Helper Refactor (3–5 days)

### 3.1 Extract shared window helpers (pure refactor)

```rust
// MOVE from overlay.rs → overlay_fx/shared.rs:
// - get_monitor_with_cursor()
// - calculate_* positioning helpers
// - force_overlay_topmost()
//
// BOTH pill and brain_overlay import from shared.rs
// Regression test: pill behavior is byte-identical
```

### 3.2 Replace the 300 ms sleep-then-hide race

```rust
// OLD (overlay.rs): thread::sleep(300ms); window.hide();
// NEW: webview emits "overlay:hidden" ack after CSS fade-out
//       → Rust listens → window.hide()
// Applies to BOTH pill and brain_overlay
```

### 3.3 Pinned / anchored / Wayland modes

```
- Pinned: stop cursor-follow, bubble stays put
- Anchored (Wayland): user-chosen corner (br/bl/tr/tl) with margin
- Honest labeling in Settings: "Follow cursor (Win/macOS/Linux-X11) — fixed anchor on Wayland"
```

### 3.4 Open-in-Conversation-tab handoff

```rust
// overlay_open_in_conversation() hands the active turn to the main window's
// Conversation tab — the overlay disappears, the conversation continues there
```

---

## 4. Phase 3 — Screen Vision (the eyes, 1–2 weeks)

### 4.1 Capture backend

File: `src-tauri/src/vision/`

```
src-tauri/src/vision/
├── mod.rs         // capture(full|region) → RgbaImage; resize; encode
├── capture.rs     // per-OS capture abstraction
├── region.rs      // bridges region-select overlay → physical-pixel rect
├── encode.rs      // downscale + PNG/JPEG + base64 data URI + token guard
└── platform/
    ├── win.rs     // DXGI Desktop Duplication or GDI BitBlt
    ├── macos.rs   // ScreenCaptureKit (needs Screen Recording permission)
    ├── x11.rs     // XGetImage / XShm
    └── wayland.rs // XDG Desktop Portal (ashpd)
```

**Use `xcap` crate** (Win/macOS/X11 in one API) + `ashpd` for Wayland.

### 4.2 Multimodal ChatMessage upgrade (additive)

File: `src-tauri/src/brain/client.rs`

```rust
// CHANGE:
//   pub struct ChatMessage { pub role: String, pub content: String }
// TO:
#[derive(Serialize, Deserialize, Clone)]
#[serde(untagged)]  // <-- back-compatible: Text serializes identically to today
pub enum MessageContent {
    Text(String),
    Parts(Vec<ContentPart>),
}

pub struct ChatMessage {
    pub role: String,
    pub content: MessageContent,
}
```

**Without images:** serializes as `"content": "..."` — byte-identical on the wire.  
**With images:** serializes as `"content": [{"type":"text","text":"..."}, {"type":"image_url","image_url":{"url":"data:image/png;base64,..."}}]`.

---

## 5. Phase 4 — Native wgpu FX (Track B, the "powerful GPU" layer, 1–2 weeks)

### 5.1 Vendor CursorFX into S2B2S

```
Copy from: C:\Users\Z\Downloads\PROJECTS\AZ\Cross_Platform_Rust_WebGPU_CursorFX\
Into:      src-tauri/src/overlay_fx/native/

Files to vendor:
├── mod.rs          → render thread, surface lifecycle
├── platform.rs     → WndProc subclass + WS_EX styles + NVAPI fix
├── renderer.rs     → ribbon + circle SDF pipelines
├── shader.wgsl     → vertex/fragment shaders
└── tracker.rs      → mouse polling (adapt to use enigo instead of device_query)
```

### 5.2 The exact CursorFX patterns to lift

| Pattern | Exact API |
|---------|-----------|
| **Transparent overlay window** | `WebviewWindowBuilder::new().transparent(true).decorations(false).always_on_top(true).skip_taskbar(true).focused(false).visible(false)` |
| **wgpu surface from Tauri window** | `HasWindowHandle::window_handle()` + `HasDisplayHandle::display_handle()` → `instance.create_surface_unsafe(SurfaceTargetUnsafe::RawHandle{raw_display_handle, raw_window_handle})` |
| **Pick alpha mode** | `caps.alpha_modes.iter().find(\|m\| *m == PostMultiplied \|\| *m == PreMultiplied)` |
| **Clear to transparent** | `LoadOp::Clear(Color { r: 0, g: 0, b: 0, a: 0 })` |
| **Windows click-through** | `WS_EX_TRANSPARENT \| WS_EX_LAYERED \| WS_EX_TOPMOST \| WS_EX_TOOLWINDOW \| WS_EX_NOACTIVATE` + WndProc `WM_NCHITTEST → HTTRANSPARENT` — **re-apply every frame** |
| **NVAPI present fix** | `nvapi64.dll` → DRS session → set `0x20324987 = 0` (Prefer Native) — run once as admin |
| **On-demand render loop** | Draw only on mouse move / state change / animation; idle-sleep after 2 still frames → 0% CPU |
| **Surface recreation** | On `Outdated \| Lost \| Validation` → `surface = None` → lazy recreate next loop iteration |

### 5.3 Cursor trail + tether (TD_Web_Trail physics port)

```rust
// Port these exact formulas into renderer.rs:

// Spring-friction chain (per point per frame):
//   v[i] = v[i] * friction + (target[i] - pos[i]) * spring
//   pos[i] = pos[i] + v[i]
//   spring = 0.39, friction = 0.5

// Catmull-Rom spline (upsample physics points for smooth curve):
//   p(t) = 0.5 * (2*P1 + (-P0+P2)*t + (2*P0-5*P1+4*P2-P3)*t² + (-P0+3*P1-3*P2+P3)*t³)
//   catmull_steps = 4

// 4-pass glow (matches the existing WGSL ribbon + circle pipelines):
//   Pass 1: blurred canvas, width×1.5, colour→black at tail (glow aura)
//   Pass 2: main canvas, width×1.0, colour→black (body)
//   Pass 3: main canvas, width×0.7, black with alpha (depth mask)
//   Pass 4: main canvas, width×0.3, solid colour, alpha fades (core)

// Width taper: w(p) = base_width * (1 - p)^1.5  where p = i/(N-1)
```

### 5.4 Key gotcha: Cargo.lock is stale

CursorFX's `Cargo.toml` says `wgpu = "29"` but `Cargo.lock` pins `0.19.4`. **On vendor-in: `cargo update wgpu` to regenerate the lock at wgpu 29.** The source code is written against wgpu 29 APIs.

### 5.5 Windows: Vulkan, NOT DX12

CursorFX confirms: **DX12 backend OOMs on transparent overlay (RTX 4070)**. The proven path:
1. Force wgpu to use **Vulkan** backend on Windows
2. Apply the **NVAPI present fix** (section 5.2 above) so NVIDIA doesn't DXGI-wrap Vulkan

---

## 6. Phase 5 — Polish & Ship (1 week)

| Task | Notes |
|------|-------|
| `HerLoading` → avatar morph | The existing loading animation morphs into the idle avatar on warm-up |
| Onboarding | First-run introduces the avatar + converse hotkey |
| Tray micro-states | Listening/thinking/speaking indicator on existing tray icon |
| i18n (20 locales) | English-first; gated by existing `check-translations` CI |
| Accessibility | `prefers-reduced-motion` → no spin/particles/tether; colorblind-safe state cues |
| Perf hardening | Meet Phase 0 budget: hidden=0 frames, idle≤24fps, streaming≤60fps |
| Screen-share privacy | `exclude_from_capture` (Win: `WDA_EXCLUDEFROMCAPTURE`, macOS: `sharingType=.none`) |

---

## 7. File Map — what gets created / modified

### New files (green-field)

```
src-tauri/src/overlay_fx/
├── mod.rs                              [NEW]
├── window.rs                           [NEW]
├── cursor_follow.rs                    [NEW]
├── placement.rs                        [NEW]
├── events.rs                           [NEW]
├── capabilities.rs                     [NEW]
├── shared.rs                           [NEW — extracted refactor in Phase 2]
└── native/
    ├── mod.rs                          [NEW — Track B]
    ├── platform.rs                     [NEW — CursorFX WndProc + NVAPI]
    ├── renderer.rs                     [NEW — CursorFX ribbon + circle]
    ├── shader.wgsl                     [NEW]
    └── tracker.rs                      [NEW]

src-tauri/src/vision/
├── mod.rs                              [NEW — Phase 3]
├── capture.rs                          [NEW]
├── region.rs                           [NEW]
├── encode.rs                           [NEW]
└── platform/win.rs, macos.rs, ...      [NEW]

src-tauri/src/commands/overlay.rs       [NEW]

src/brain-overlay/                      [NEW — standalone webview app]
├── index.html                          [NEW]
├── main.tsx                            [NEW]
├── avatar/
│   └── Avatar.tsx                      [NEW — Three.js 3D avatar]
├── bubble/
│   └── ReplyBubble.tsx                 [NEW — streaming text bubble]
├── actions/
│   └── QuickActions.tsx                [NEW — keyboard + mouse actions]
└── App.tsx                             [NEW]

src/region-select/                      [NEW — Phase 3]
├── index.html                          [NEW]
└── main.tsx                            [NEW]

src/components/settings/overlay-mode/   [NEW]
```

### Modified files (additive touch-points only)

```
src-tauri/src/settings.rs               + OverlayModeConfig, AvatarConfig, VisionConfig
src-tauri/src/lib.rs                     + register overlay_fx, new commands, brain_overlay window
src-tauri/src/overlay.rs                 + one-line mic-level fan-out to brain_overlay
src-tauri/src/tts/player.rs              + RmsSource wrapper (~30 lines)
src-tauri/src/brain/client.rs            + MessageContent enum (back-compat)
src-tauri/src/brain/manager.rs           + optional images arg to ask()
src-tauri/Cargo.toml                     + wgpu, raw-window-handle, xcap, ashpd (feature-gated)
```

### Files NEVER touched

```
src-tauri/src/brain/manager.rs   (logic unchanged — just add optional arg)
src-tauri/src/input.rs           (reused via commands, not edited)
src-tauri/src/clipboard.rs       (unchanged)
src-tauri/src/actions.rs         (unchanged)
src/components/conversation/     (unchanged — Conversation 1.0 lives on)
src-tauri/src/managers/          (unchanged)
src-tauri/src/tts/backends/      (unchanged)
src-tauri/src/audio_toolkit/     (unchanged)
src/stores/                      (unchanged)
src/i18n/                        (only new keys added)
```

---

## 8. Performance Budget

| Metric | Target |
|--------|--------|
| Hidden overlay | 0 frames, 0 timers, native thread parked |
| Visible idle | ≤24 fps avatar breathing; trail idle-sleeps after 2 frames |
| Streaming | ≤60 fps; tokens coalesced per rAF |
| Memory (Track A) | ≤ +80 MB |
| Memory (Track B) | ≤ +20 MB |
| Show latency (hotkey→visible) | <120 ms (window pre-created hidden) |

---

## 9. Risk Register

| Risk | Mitigation |
|------|------------|
| DX12 OOM on transparent window | **Use Vulkan** + NVAPI present fix (proven by CursorFX) |
| WebKit WebGPU immaturity (macOS/Linux) | **Auto WebGL2 fallback** in Three.js |
| Wayland no global cursor | **Anchored placement** + honest labeling |
| Focus theft (cardinal sin) | Non-activating windows everywhere; nothing typed unless Insert pressed |
| Stale CursorFX Cargo.lock | Regenerate to wgpu 29 on vendor-in |
| Vision permissions denied | Degrade to text-only with clear messaging |
| Z-order loss to games | HWND_TOPMOST re-assert + 2s watchdog |

---

## 10. Fallback Ladder (never breaks)

```
1. Native surface fails    → Track A webview overlay (avatar + bubble still work)
2. WebGPU unavailable       → WebGL2 renderer (auto in Three.js)
3. No compositor (Linux)    → Opaque rounded "card" theme
4. No layer-shell (GNOME)   → Normal on-top window, anchored
5. No cursor position       → Anchored placement (Wayland)
6. All fails               → Conversation 1.0 behavior (main window) ← today's user sees nothing different
```

---

## 11. The "One Curve Language" — unifying all three repos

All three projects share the same mathematical language — this is the brand through-line:

| | Curve | Glow | Motion |
|---|-------|------|--------|
| `HerLoading.tsx` (S2B2S) | `CatmullRomCurve3` tube | Additive transparent planes | rAF easing rotation |
| `TD_Web_Trail` | Catmull-Rom / Bézier | 4-pass bloom + taper | Spring-friction chain |
| `CursorFX` | `catmull_rom()` in WGSL | SDF + layered ribbon | Spring-damper + Catmull-Rom |

**The avatar, its thinking orbit, and its cursor tether all use Catmull-Rom curves + additive glow + spring motion** — making the loading screen, the trail, and the avatar read as one visual identity.

---

## Phases at a Glance

```
P0 Groundwork (2d)  →  P1 Avatar v1 + converse loop (1-2w)  →  P2 Polish + refactor (3-5d)
                              │
        P5 Ship (1w)  ←  P4 Native wgpu FX (1-2w)  ←  P3 Vision / eyes (1-2w)

Total: ~6–8 weeks for the full vision, shipping Track A (avatar + bubble) in ~2-3 weeks.
```
