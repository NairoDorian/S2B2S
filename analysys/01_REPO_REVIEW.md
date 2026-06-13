# 01 — Repository Review

What exists today, file by file, and what it means for the overlay/avatar work. Everything in §1–2 was verified against the cloned `NairoDorian/S2B2S` source (branch `main`, June 2026).

---

## 1. S2B2S — Current State (verified)

### 1.1 Stack & conventions

- **Tauri 2** (`tauri 2.11.x`, `tauri-build 2.6.x`), Rust backend + React/TypeScript frontend, Vite, Tailwind, Zustand stores, i18n with **20 languages incl. RTL (ar, he)** and a `getLanguageDirection` helper already used inside the recording overlay.
- **Typed IPC** through `tauri-specta` (`src/bindings.ts`, regenerated via `cargo test export_bindings`). All new commands/events must go through this.
- **Cross-platform mandate** in `AGENTS.md`/`CLAUDE.md`: Windows 11 primary, macOS and Linux first-class, no single-OS code paths without fallback.
- Key crates already present and reusable: `enigo` (cursor position + synthetic paste), `rodio`/`cpal` (audio out), `rustfft` (already used for mic visualizer levels), `tauri-nspanel` (macOS panels), `gtk-layer-shell` (Linux), `windows` crate (Win32 calls).

### 1.2 The existing overlay — `src-tauri/src/overlay.rs` (415 lines)

This is the single most important file for the new work, because **it already solves 70% of the hard cross-platform problems**, just for a small status pill (172×36 px, top/bottom-center of the cursor's monitor):

| Capability | How it's done today | Reusable for Brain Overlay? |
| --- | --- | --- |
| macOS always-on-top, all Spaces, fullscreen apps | `tauri-nspanel` `PanelBuilder` — `PanelLevel::Status`, `no_activate`, `CollectionBehavior::can_join_all_spaces().full_screen_auxiliary()`, non-key window | ✅ Verbatim |
| Windows topmost that *stays* topmost | `force_overlay_topmost()` — raw Win32 `SetWindowPos(HWND_TOPMOST, SWP_NOACTIVATE…)` re-asserted on every show, bridging tao's older `HWND` type | ✅ Verbatim |
| Linux layer surface | GTK Layer Shell (`Layer::Overlay`, `KeyboardMode::None`, exclusive zone 0), env kill-switch `S2B2S_NO_GTK_LAYER_SHELL`, graceful fallback to a normal window | ✅ Verbatim |
| Monitor-of-cursor detection | `get_monitor_with_cursor()` using `input::get_cursor_position()` (enigo) + per-monitor `scale_factor()` normalization, with documented logical/physical pitfalls | ✅ Core of cursor-follow |
| DPI-safe positioning | Always `LogicalPosition` (comment explains tao's cross-monitor physical-position bug) | ✅ Rule to keep |
| Transparent, undecorated, no-taskbar, unfocused webview | `WebviewWindowBuilder` flags: `transparent(true) .decorations(false) .skip_taskbar(true) .focused(false) .always_on_top(true)` | ✅ Verbatim |
| Show/hide with fade + event protocol | `show-overlay` / `hide-overlay` events into the webview; 300 ms delayed `hide()` | ✅ Pattern to extend |
| Mic level fan-out | `emit_levels()` already emits `mic-level` to both main app **and** the overlay window | ✅ Drives avatar Listening state for free |

**What it does *not* do (the gaps the new work fills):** no click-through (`set_ignore_cursor_events` never called), fixed screen-edge anchor (not cursor-following), no text content (status icon + mic bars only), no GPU/shader rendering (DOM/CSS), single static size.

Frontend half: `src/overlay/RecordingOverlay.tsx` — a tiny standalone webview app (own `index.html`/`main.tsx`) with states `recording | transcribing | processing | speaking`, smoothed mic bars, i18n + RTL aware. The Brain Overlay will be a **sibling** standalone webview app following this exact template.

### 1.3 The Brain — `src-tauri/src/brain/`

- `client.rs` — OpenAI-compatible **SSE streaming** client; returns `BrainResult` with timing (`tokens_per_sec`, `total_ms`, parsed from `usage` / `delta.timings`).
- `manager.rs` — `BrainManager`: owns multi-turn history (`context_turns` window), **barge-in abort** (`current_abort: Mutex<Arc<AtomicBool>>` swapped per turn), feeds completed sentences straight into TTS for speak-before-finish, and emits the event stream the whole frontend runs on:
  - `brain:thinking` → request sent
  - `brain:token` (string delta) → streaming text
  - `brain:sentence` → fed to streaming TTS
  - `brain:done` (`{ text, tokens_per_sec, total_ms, predicted_ms }`)
  - `brain:asked` (carries `stt_ms`), plus llama lifecycle events `brain:llama-loading/ready/error`
- `llama_manager.rs` + `llama_server/` — pre-compiled llama.cpp server lifecycle (CUDA > Vulkan > CPU auto-select, `-ngl all` VRAM offload, `ensure_server_running()` on demand).

**Implication:** Conversation 2.0 needs **zero changes to brain logic**. The overlay window simply becomes a second listener of `brain:*` (Tauri `emit` is app-global; window-targeted `emit_to` optional for efficiency).

### 1.4 Conversation Mode 1.0 (what "2.0" builds on)

- **UI:** `src/components/conversation/ConversationView.tsx` — a sidebar tab inside the main window. Chat transcript, text input, voice-mode toggle, per-message metrics (🎤 `stt_ms`, `t/s`, `ms`), read-aloud toggle, latency HUD.
- **Hands-free engine:** `managers/continuous_voice.rs` — VAD-segmented loop: pause listening → STT → history save → `BrainManager::ask` → TTS → re-arm. Driven by `BrainConfig`:
  - `conversation_mode: push_to_talk | toggle | hands_free`
  - `endpoint_preset: snappy(300ms) | balanced(600) | patient(1200)`
  - `headphone_mode` (barge-in during TTS), `auto_listen`, `read_aloud`, `speakable_output_prompt`, `context_turns`, `system_prompt`
- **Triggers:** dedicated `converse` hotkey binding; **wake word** module (`wake_word.rs`) is complete per roadmap.
- **Limitation that motivates 2.0:** the reply renders **only inside the main window**. If you're in another app, you hear TTS but see nothing — and switching windows defeats the voice-native promise.

### 1.5 Other directly relevant pieces

- `input.rs` — `get_cursor_position()` (enigo, global coords) and the layout-independent paste machinery (`send_paste_ctrl_v` etc.) → powers "Insert at cursor" and cursor-follow.
- `actions.rs` — dictation pipeline + "AI Replace Selection"; shows the established pattern for *typing results into the focused app*, which the overlay's Insert action reuses.
- `tts/player.rs` (rodio) + `tts/status.rs`/`telemetry.rs` — playback; currently emits `tts:synth-done`, `tts:first-audio`. **Gap:** no realtime output amplitude event → needed for the avatar's speaking mouth (tiny tap, see 04 §5).
- `HerLoading.tsx` — a polished Three.js *Her*-movie-style animation (Catmull-Rom tube curve, ring) already shipping. **This is the avatar's design DNA and proves Three.js/WebGL is acceptable in-app.**
- Settings system — `settings.rs` (~1,400 lines) with serde defaults pattern (`#[serde(default = "...")]`) for safe schema evolution; `OverlayPosition { Top, Bottom, None }` exists; UI components `ShowOverlay.tsx` etc. show the pattern for new toggles.
- Footer status dots (STT/Brain/TTS green-orange-gray) — the avatar's color language should match these.

### 1.6 Honest gaps / risks observed in the current code

1. **Webview overlay cost:** each Tauri window is a full WebView2/WKWebView/WebKitGTK instance (~30–80 MB). Fine for one more window; an argument for the native wgpu track long-term.
2. **Linux transparency variance:** WebKitGTK transparency depends on compositor; the codebase already hedges (layer-shell fallback, env flags) — the Brain Overlay must hedge identically.
3. **Wayland has no global cursor position** for ordinary clients — `enigo.location()` is X11-reliable only. Cursor-follow on Wayland needs a degraded mode (documented in 02 §4.4).
4. **`hide()` after fixed 300 ms** (thread sleep) is a race-prone pattern; the new overlay should use an explicit `overlay:hidden` ack from the webview instead.
5. Recording overlay and Brain overlay will both want the screen → simple z/priority rule needed (Brain overlay yields to recording pill, or they merge — see 03 §6).

---

## 2. What "the app stays the same" means concretely

Untouched: main window + all tabs (incl. Conversation 1.0), recording overlay behavior, all default settings values, tray, dictation/read-aloud pipelines, history schema (one additive column at most), bindings of existing commands. New code lands in **new files** (`src-tauri/src/overlay_fx/`, `src/brain-overlay/`, `src/components/settings/overlay-mode/`) plus minimal registration touch-points (`lib.rs` builder, settings struct additive fields with serde defaults, sidebar settings entry).

---

## 3. `Cross_Platform_Rust_WebGPU_CursorFX` — status & assumptions

**Status:** `git clone` over HTTPS prompts for credentials and the repo is not discoverable publicly → **private or renamed**; it could not be reviewed.

**Assumptions used by this plan** (from the name and the stated goal — correct these once the code is shared):

1. Rust binary using **winit** (or raw platform APIs) to create a transparent, undecorated, always-on-top, click-through window.
2. **wgpu** rendering (WebGPU API → Vulkan/Metal/DX12 backends) with an alpha-composited surface, drawing **effects that follow the OS cursor** (trails/particles/glow).
3. Per-frame global cursor polling or platform mouse hooks; multi-monitor aware; some per-OS conditional code for click-through and compositing.

**What the plan will want to extract from it** (the "integration seam", detailed in 02 §6):

- Window creation flags per OS (esp. Windows `WS_EX_LAYERED|WS_EX_TRANSPARENT|WS_EX_NOACTIVATE` handling and any DirectComposition setup; macOS `NSWindow`/`CAMetalLayer` config; X11 ARGB visual + input-shape code).
- `wgpu` surface configuration: chosen `CompositeAlphaMode` per backend, present mode, premultiplied-alpha conventions in shaders.
- The render loop structure (event-driven vs continuous), cursor-tracking method, and any DPI handling.
- Its FX shaders (trail/particles) — reusable as the avatar's "thinking" particles and an optional cursor-trail cosmetic.

**Unblock options (pick one):** make the repo public · add as a git submodule with a machine deploy key · publish it as a private crate in the workspace · or paste the ~3 relevant files into an issue. Until then, Track B in `02` specifies a from-scratch reference design that the CursorFX code can replace or accelerate.
