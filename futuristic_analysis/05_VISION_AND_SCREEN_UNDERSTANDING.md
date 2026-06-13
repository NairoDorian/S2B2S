# 05 — Screen Vision & Screen Understanding (the "eyes")

**This pillar is new.** The voice brief asks for it directly: *"the model could have an understanding of my screen … it can take screenshots, and it can take zone‑specific screenshots with a shortcut to select a rectangle … those images are sent to the brain model … vision models can take images as parameters."* The current code has **no capture path** and the Brain is **text‑only** (`ChatMessage { content: String }`). This document specifies the whole feature.

---

## 1. User experience

```
While the avatar is up (or via a global hotkey):

  • Tap  [Show‑full‑screen]   → grabs the current monitor, avatar EYES light up, 🖼 chip appears in the bubble
  • Tap  [Region‑grab]        → a dim overlay + crosshair; DRAG a rectangle; release → that region is captured
  • Then just speak/type:     "what does this error mean?" / "summarize this page" / "is this layout balanced?"
  • The screenshot rides along with your next Brain turn → a vision model answers about what it saw
  • The image is shown as a small thumbnail in the bubble; click to expand; it's dropped after the turn (unless pinned)
```

Two capture modes, one shortcut family:

| Mode | Default key | What it captures |
| --- | --- | --- |
| **Full monitor** | `converse‑modifier + S` | the monitor under the cursor (or all monitors → tiled, optional) |
| **Region rectangle** | `converse‑modifier + Shift + S` (or the `S` quick‑action while the overlay is up) | a user‑dragged rectangle |

The avatar's **eyes are the consent signal**: they are dark until a capture happens, then they brighten and a "👁 seeing" micro‑status shows. **Capture is never silent.**

---

## 2. The region selector

A dedicated, full‑screen, transparent, **input‑capturing** overlay window (distinct from the click‑through bubble — this one *must* receive the mouse):

- Per‑OS creation reuses the `overlay.rs` machinery, but **without** click‑through and **with** keyboard focus for `Esc` to cancel.
- Dims the screen ~30%, shows a crosshair and a live `WxH @ x,y` readout; drag draws the bright "hole"; release commits; `Esc` cancels.
- Snapping helpers (optional, later): snap to window bounds under the cursor (via the OS window list) for "capture this window."
- On commit → hand the rect (in **physical** pixels, monitor‑aware) to the capture backend (`§3`), then close.

This is a small, self‑contained webview app `src/region-select/` (sibling of `src/overlay/`), or a native layer in Track B — either works; the webview version is simplest and cross‑platform.

---

## 3. Capture backend — `src-tauri/src/vision/`

```
src-tauri/src/vision/
├── mod.rs        // capture(full|region|window) → RgbaImage; resize; encode
├── capture.rs    // per-OS capture (xcap-style abstraction)
├── region.rs     // bridges the region-select overlay → a physical-pixel rect
├── encode.rs     // downscale + PNG/JPEG + base64 data URI + token-budget guard
└── platform/     // win.rs / macos.rs / x11.rs / wayland.rs
```

### 3.1 Cross‑platform capture matrix

| OS | Mechanism | Notes |
| --- | --- | --- |
| **Windows 11** | Windows.Graphics.Capture (preferred) or BitBlt/DXGI Desktop Duplication | DXGI Duplication is fastest for full‑monitor; Graphics.Capture respects per‑window capture & HDR. |
| **macOS** | ScreenCaptureKit (12.3+) / `CGWindowListCreateImage` | **Requires the Screen Recording permission** — request via the existing macOS‑permissions plugin; add copy. |
| **Linux X11** | XGetImage / XShm | works directly. |
| **Linux Wayland** | **XDG Desktop Portal** `org.freedesktop.portal.Screenshot` / `ScreenCast` (PipeWire) via `ashpd` | Wayland forbids raw grabs; the portal shows a system picker the first time. Region selection may be portal‑driven on some compositors. Document this. |

**Recommendation:** use a cross‑platform crate to avoid hand‑writing four backends — **`xcap`** covers Windows/macOS/X11 (monitor + window capture) in one API; add an **`ashpd`** path for Wayland portals. Both are mature Rust crates and keep the surface small. (If you prefer zero new heavy deps, the per‑OS `platform/` files above are the fallback.)

### 3.2 Encoding & token‑budget guard (`encode.rs`)

Vision tokens are expensive and models cap image size. Before sending:

1. **Downscale** to a max long edge (default 1568 px, configurable) preserving aspect — matches common vision‑model tiling sweet spots.
2. Encode **PNG** for crisp UI/text screenshots (lossless, sharper for code), or **JPEG q≈85** for photos/large scenes (smaller). Default: PNG for region grabs, JPEG for full‑screen.
3. Wrap as a **data URI**: `data:image/png;base64,…`.
4. **Guard:** cap total attached bytes (default ~4 MB) and image count per turn (default 1–2); warn if exceeded; never silently send a 4K multi‑monitor PNG.

---

## 4. The multimodal Brain upgrade (`brain/client.rs`) — additive

This is the one Brain change in the whole plan. Make `ChatMessage.content` accept **either** a plain string (today) **or** an array of content parts (OpenAI‑compatible multimodal):

```rust
// today:  pub struct ChatMessage { pub role: String, pub content: String }

#[derive(Serialize, Deserialize, Clone)]
#[serde(untagged)]                       // serializes back-compatibly
pub enum MessageContent {
    Text(String),                        // existing wire shape — unchanged
    Parts(Vec<ContentPart>),             // new multimodal shape
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    Text { text: String },
    ImageUrl { image_url: ImageUrl },    // { "url": "data:image/png;base64,..." }
}

pub struct ChatMessage { pub role: String, pub content: MessageContent }
```

- **Back‑compatible:** with `#[serde(untagged)]`, a `MessageContent::Text` serializes to exactly today's `"content": "..."`, so **text‑only providers see no change**. Only when images are attached do we emit the `parts` array (the OpenAI/`llama.cpp`/Ollama multimodal shape: `content: [{type:text,...},{type:image_url,...}]`).
- `BrainManager::ask` gains an optional `images: Vec<DataUri>` argument (defaulting to empty); when non‑empty it builds the user message as `Parts([Text, ImageUrl…])`. **History stores the text** (and optionally a small thumbnail ref), not full images, to keep the context window sane.
- The **SSE streaming, sentence splitter, timing, barge‑in — all unchanged.** Images only affect how the *request* user‑message is built.

---

## 5. Vision settings & model selection — `VisionConfig`

```rust
pub struct VisionConfig {
    pub enabled: bool,               // default false
    pub model_id: Option<String>,   // a vision-capable Brain model (may differ from the text model)
    pub default_mode: String,        // "region" | "full"
    pub max_long_edge_px: u32,       // default 1568
    pub format: String,              // "auto" | "png" | "jpeg"
    pub max_images_per_turn: u8,     // default 1
    pub keep_after_turn: bool,       // default false (drop image once answered)
    pub redact_prompt: bool,         // default true: never log image bytes
}
```

- **Model awareness:** the Brain settings already enumerate providers/models; add a **"vision‑capable"** flag (known multimodal model names, or a capability probe) and let the user pick a vision model (which may be a *second* endpoint, e.g. a local LLaVA/Qwen‑VL via llama.cpp, while the text model stays elsewhere). If vision is on but the active model isn't multimodal → warn once and offer to switch (`04 §8`).
- **Local‑first:** vision works with local multimodal GGUFs through the existing `llama.cpp` path — no new network dependency required.

---

## 6. Privacy & safety (explicit, by design)

1. **Opt‑in:** `vision.enabled` defaults **off**; the feature is invisible until enabled.
2. **Never silent:** every capture lights the avatar's eyes + shows a "👁 seeing" status + a 🖼 chip. There is no background screen watching.
3. **No persistence by default:** images are dropped after the turn (`keep_after_turn = false`); never written to history/disk unless the user pins/saves.
4. **No logging of bytes:** `redact_prompt` ensures image data never hits logs/telemetry.
5. **Screen‑share exclusion:** honors `exclude_from_capture` (`03 §4.1/4.2`) so the avatar/bubble don't leak into a shared screen; and the region selector itself is excluded from its own capture.
6. **Permission honesty:** macOS Screen Recording / Wayland portal prompts are surfaced with clear copy explaining *why* (the avatar wants to see what you point it at).

---

## 7. The "eyes" tie‑in (to the avatar)

The vision feature is the avatar's **sense of sight** (`06 §2`):

- **Idle:** eyes dark / half‑lidded.
- **Capturing:** a quick "shutter" flash; eyes **brighten** and a faint scanline sweeps the avatar (it's "looking").
- **Reasoning about an image:** during THINKING after a capture, the eyes stay lit and occasionally **saccade** (tiny darts) — a cheap, charming "it's studying the picture" cue.
- The 🖼 thumbnail chip in the bubble is literally "what the eyes saw."

This makes the privacy model *legible*: you can always tell, at a glance, whether S2B2S is looking.

---

## 8. Future extensions (out of scope for v1, noted)

- **Auto‑context (opt‑in, explicit):** a "look before you answer" toggle that grabs the focused window on each turn — powerful but privacy‑sensitive; gated behind a separate, loud setting.
- **OCR‑assist:** run local OCR (e.g. for non‑vision text models) to feed text instead of pixels when a vision model isn't available.
- **Annotate‑then‑ask:** draw an arrow/box on the region before sending ("what's *this*?").
- **Multi‑monitor tiling:** capture all monitors as one tiled image for "describe my whole desktop."

---

## 9. Acceptance criteria (vision)

1. Full‑screen and region capture both work on Windows, macOS (with permission), and Linux/X11; Wayland works via portal with the documented first‑run picker.
2. Region rectangle is pixel‑accurate and monitor/DPI‑correct.
3. A captured image reaches a local multimodal model and produces an answer that streams into the bubble + speaks — end‑to‑end, no main window.
4. Text‑only providers are **byte‑identical on the wire** to today when no image is attached (verified by a serialization test).
5. The avatar's eyes light up on every capture and never light up otherwise (the privacy invariant).
6. With `vision.enabled = false`, the feature and its hotkeys are fully inert.
