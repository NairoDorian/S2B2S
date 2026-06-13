# 02 — Reference Projects: TD_Web_Trail & CursorFX

What the two cursor‑FX repos *actually are* (read from source), and the **exact techniques to lift** from each. This corrects the old plan's CursorFX guesses.

---

## A. `Cross_Platform_Rust_WebGPU_CursorFX` — the overlay machinery

### A.1 What it actually is (corrects the old plan)

> The old plan: *"⚠️ Not accessible … assume Rust + winit + wgpu … reserve a from‑scratch Track B."*
> **Reality:** it is **Tauri V2 + Bun + Vite + React 19 + TailwindCSS 4** for the config UI, with a **Rust + `wgpu`** backend rendering a **transparent, fullscreen, always‑on‑top, click‑through overlay**. It is the **same application shell as S2B2S.** Its own CHANGELOG documents a deliberate **V2 → V3** rewrite that *replaced* `winit 0.29` + `egui 0.26` **with Tauri V2 + React**, keeping the wgpu rendering engine.

This is the most consequential correction in the whole plan: **Track B is not a from‑scratch winit project. It is "render `wgpu` into a transparent Tauri overlay window's surface" — and S2B2S is already a Tauri app.**

### A.2 Architecture (from `project_cursor/src-tauri/src/`)

```
overlay/mod.rs       window styling (WndProc subclass), NVAPI fix, the wgpu render loop
overlay/renderer.rs  wgpu pipelines + CPU‑side physics  (1,234 lines)
overlay/shader.wgsl  vertex/fragment shaders            (121 lines)
tracker.rs           global mouse position via device_query
config.rs            AppConfig + RON serialization
lib.rs               Tauri commands + state; creates the overlay window
```

- **Window:** a Tauri `WebviewWindow` labeled `"overlay"`, made transparent + topmost + click‑through, whose **raw handle** feeds a `wgpu::Surface`. `set_ignore_cursor_events(true)` is set on the Tauri side.
- **Rendering:** the render loop runs `wgpu` against that surface; the webview itself shows nothing. **Two pipelines**: a **ribbon** pipeline (the trail) and an **instanced circle** pipeline (satellites, ripples, particles, the squishy head) drawn with **SDF** math in the fragment shader.
- **Tracking:** `device_query 4` polls the global mouse at the monitor refresh rate; an idle path drops to 60 Hz when nothing moves.
- **Config:** `RON` at the per‑OS config dir; a React panel edits physics/layers/colors live over Tauri IPC.

### A.3 The exact techniques to lift (with the gotchas)

These are real, debugged solutions to problems pillar 1 will hit:

1. **Click‑through on Windows = WndProc subclassing + extended styles, re‑applied every frame.**
   - Styles: `WS_EX_TRANSPARENT | WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE`, and *remove* `WS_EX_APPWINDOW`.
   - Subclass the WndProc so `WM_NCHITTEST → HTTRANSPARENT` (click‑through), `WM_SETCURSOR → 1`, `WM_ERASEBKGND → 1`.
   - **Re‑assert the styles + topmost on every frame** — games/installers/other apps steal Z‑order and reset styles. (CursorFX's CHANGELOG even logs a regression where applying styles once‑only broke click‑through.)
2. **🔴 The big one — Windows transparent overlay: use Vulkan, NOT DX12.** CursorFX's *Known Issues*: **"DX12 backend fails with `OutOfMemory` on RTX 4070 for transparent overlay (Vulkan is default, works perfectly)."** The old plan recommended *"wgpu DX12 swapchain via DirectComposition for premultiplied alpha"* on Windows — **that path OOMs**. The proven path is **Vulkan** + the NVAPI fix below.
3. **The NVAPI "Prefer Native" present fix (Windows + NVIDIA).** NVIDIA's driver wraps Vulkan presentation in a DXGI swapchain that **breaks transparency**. CursorFX fixes it by setting the driver profile setting `OGL_CPL_PREFER_DXPRESENT`‑style "Vulkan/OpenGL present method = Prefer Native" via **NVAPI** (`nvapi64.dll`, DRS session → base profile → set setting `0x20324987 = 0` → save). It logs `ACCESS_DENIED` and asks to run once as admin if the profile write is blocked. **Vendor this almost verbatim.**
4. **Surface from a Tauri window handle:** `instance.create_surface_unsafe(SurfaceTargetUnsafe::RawHandle { raw_display_handle, raw_window_handle })` using `raw-window-handle 0.6` (`HasWindowHandle`/`HasDisplayHandle`). Pick the surface format that `is_srgb()`, and pick an **alpha mode** of `PostMultiplied` or `PreMultiplied` from `caps.alpha_modes` (this is what actually makes the overlay transparent). `present_mode: Fifo` (vsync), `desired_maximum_frame_latency: 2`.
5. **On‑demand render loop:** redraw only on mouse move / button change / active animation / `enabled`; sleep to the next frame otherwise. Recreate the surface on `Outdated/Lost`. This is the power‑budget model pillar 1 wants.
6. **The renderer's reusable building blocks** (`renderer.rs` / `shader.wgsl`): `catmull_rom()` interpolation, `hsl_to_rgba()`, fade‑curve functions, a ribbon vertex builder with per‑layer width/blur, and an SDF circle fragment shader (`thickness < 0` = filled, `≥ 0` = outline). **These map directly onto the avatar's orbiting "thinking" particles, the listening rim‑ripples, and the cursor trail.**

### A.4 ⚠️ Two real caveats in the CursorFX repo to fix on vendor‑in

- **`Cargo.lock` is stale.** `Cargo.toml` requests **`wgpu = "29"`** and the CHANGELOG says **29.0.3**, but the committed **`Cargo.lock` pins `wgpu 0.19.4`** while the *code* is written against wgpu‑29 APIs (`CurrentSurfaceTexture` enum, `experimental_features`, `wgpu::Trace`, `MemoryHints`). A fresh build from the lock would not compile against that source. **On vendor‑in, regenerate the lock to wgpu 29** and pin it. (The README header still says "wgpu 24" in places — also stale; trust the source + CHANGELOG: **29**.)
- **`device_query` for global mouse** works on Windows/macOS (needs Accessibility on macOS) and **X11**, but **not Wayland** — same constraint S2B2S already documents for `enigo`. S2B2S should **keep using `enigo`** for cursor position (it's already a dependency) rather than add `device_query`, unless the native track needs sub‑frame polling.

### A.5 Bottom line

CursorFX gives pillar 1 a **proven, cross‑platform, transparent, click‑through wgpu overlay** with the Windows transparency minefield already mapped. Vendor it as a workspace crate (`crates/cursorfx/`) or fork‑merge it into `overlay_fx/native/`, behind a feature flag, with Track A (webview) as the always‑available fallback. See `03 §6`.

---

## B. `TD_Web_Trail` — the trail physics & rendering recipe

### B.1 What it is

A **zero‑latency, physics‑based web cursor trail** (HTML5 Canvas 2D) that also **streams cursor coordinates to TouchDesigner** over a custom binary WebSocket protocol via a zero‑dependency **Bun.js** relay. It is pure web (no Rust), so it is **not** the overlay engine — it is the **aesthetic + physics + low‑latency‑streaming reference** for the cursor→avatar **tether** and any ambient trail.

### B.2 The techniques worth lifting

1. **Spring‑friction chain physics** — the trail is `N` points `{x, y, dx, dy}`; the head chases the cursor with a damped spring, each subsequent point chases its predecessor, velocity is scaled by friction and integrated. This is the exact model for a **trail that lags elastically behind the cursor and connects to the avatar** — organic, cheap, frame‑rate‑independent‑ish.
2. **Distance‑constraint solver** (clamped/rigid) — keeps the chain from stretching unboundedly; gives the tether a "rope/skeleton" feel if desired.
3. **Multi‑pass tapered glow rendering** — four draws per frame: a downscaled (25%) blurred **bloom** layer + a tapered body + a dark inner mask + a bright core filament, with a `(1−p)^1.5` width taper. This is the **neon/cyberpunk glow recipe** the brief asks for, achievable in Canvas, WebGL, or WGSL.
4. **Catmull‑Rom / cubic‑Bézier smoothing** — upsample the physics points into a smooth spline (Bézier control points derived from velocity → the trail bends in the direction of motion). Mirrors what `HerLoading.tsx` already does in 3D and what CursorFX's `catmull_rom()` does in Rust — **one consistent curve language across all three projects.**
5. **Performance discipline worth copying wholesale:** desynchronized canvas context, **idle sleep after 2 still frames (0% CPU)**, color caching, and **style/progress quantization to ≤24 steps so the whole trail strokes in a handful of draw calls.** These are the same "calm, near‑zero idle" goals as pillar 1's power budget.
6. **The binary streaming idea** — packing coordinates into raw little‑endian byte frames instead of JSON (8‑byte client→server) for sub‑millisecond serialization. **Reusable for any high‑rate IPC** the avatar needs (e.g. streaming `mic-level`/`tts:level`/cursor deltas to a native renderer) if the JSON `emit` path ever becomes a bottleneck — almost certainly not needed at first, but a known lever.

### B.3 Bottom line

TD_Web_Trail is the **look** (neon tapered glow), the **motion** (spring‑friction + Catmull‑Rom), and the **latency discipline**. Its physics power the **cursor→avatar tether** in `06`, and its glow recipe informs both the webview avatar and the native wgpu trail. It is a design reference, not a dependency.

---

## C. One curve language, three projects

A nice unifying observation that makes the whole thing feel coherent rather than bolted together:

| | Curve | Glow | Motion |
| --- | --- | --- | --- |
| `HerLoading.tsx` (S2B2S) | `CatmullRomCurve3` tube | additive transparent planes | rAF easing rotation |
| `TD_Web_Trail` | Catmull‑Rom / Bézier | 4‑pass bloom + taper | spring‑friction chain |
| `CursorFX` | `catmull_rom()` in WGSL | SDF + layered ribbon | spring‑damper + Catmull‑Rom |

The avatar, the thinking‑orbit, and the cursor tether can **all** use Catmull‑Rom curves + additive glow + spring motion — so the loading screen, the trail, and the avatar read as **one visual identity**, exactly the "the Her lineage is the brand" idea the old plan had, now extended across all three repos.
