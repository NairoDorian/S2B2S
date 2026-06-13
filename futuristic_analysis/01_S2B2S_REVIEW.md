# 01 — S2B2S Review (verified from source)

Everything below was read from the cloned `NairoDorian/S2B2S` `main` branch (June 2026). Line counts and APIs are real. This grounds the rest of the plan.

---

## 1. Stack & conventions

- **Tauri 2** (`tauri 2.11.2`, `tauri-build 2.6.2`), Rust backend + React 19 / TypeScript frontend, **Vite 8**, **Tailwind 4**, Zustand + Immer stores.
- **Typed IPC** via **`tauri-specta 2.0.0-rc.25`** → `src/bindings.ts`, regenerated with `cargo test export_bindings`. *Every new command/event must go through this.*
- **i18n: 20 languages incl. RTL** (`ar`, `he`); a `getLanguageDirection`/`rtl` helper is already used inside the recording overlay.
- **3D already shipping:** `three ^0.184.0` + `@types/three` (used by `HerLoading.tsx`). Three 0.184 includes **`WebGPURenderer`** and TSL — relevant to the avatar.
- **Cross‑platform mandate** in `AGENTS.md`/`CLAUDE.md`: Windows 11 primary, macOS + Linux first‑class, no single‑OS path without a fallback.
- Reusable crates already present: **`enigo 0.6.1`** (cursor position + synthetic paste), **`rodio`** (audio out), **`rustfft`** (mic visualizer levels), **`tauri-nspanel`** (macOS panels, `v2.1` branch), **`gtk-layer-shell 0.8`** (Linux), **`windows 0.62.2`** (Win32). Tauri features include `image-png`.
- **No `wgpu`, no `winit`, no `device_query`, no screen‑capture crate** in S2B2S today.

---

## 2. The existing overlay — `src-tauri/src/overlay.rs` (415 lines)

**The single most important existing file.** It already solves ~70% of the hard cross‑platform problems, but only for a small **status pill** (172×36 px, anchored top/bottom‑center of the cursor's monitor).

| Capability | How it's done today | Reuse for the Brain Overlay? |
| --- | --- | --- |
| macOS always‑on‑top, all Spaces, over fullscreen | `tauri-nspanel` `PanelBuilder` → `PanelLevel::Status`, `no_activate(true)`, `CollectionBehavior::can_join_all_spaces().full_screen_auxiliary()`, non‑key window | ✅ Verbatim |
| Windows topmost that *stays* topmost | `force_overlay_topmost()` — raw Win32 `SetWindowPos(HWND_TOPMOST, …, SWP_NOACTIVATE)` re‑asserted on every show; bridges tao's older `HWND` type | ✅ Verbatim |
| Linux layer surface | GTK Layer Shell (`Layer::Overlay`, `KeyboardMode::None`, exclusive zone 0), env kill‑switch `S2B2S_NO_GTK_LAYER_SHELL`, graceful fallback to a normal window | ✅ Verbatim |
| Monitor‑of‑cursor detection | `get_monitor_with_cursor()` via `input::get_cursor_position()` (enigo) + per‑monitor `scale_factor()` normalization | ✅ Core of cursor‑follow |
| DPI‑safe positioning | Always `LogicalPosition` (a comment explains tao's cross‑monitor *physical* position bug) | ✅ A rule to keep |
| Transparent, undecorated, no‑taskbar, unfocused webview | `WebviewWindowBuilder` flags: `transparent(true).decorations(false).skip_taskbar(true).focused(false).always_on_top(true).visible(false)` | ✅ Verbatim |
| Show/hide protocol | `show-overlay` / `hide-overlay` events into the webview; **`hide()` after a 300 ms thread sleep** | ✅ Pattern to extend (fix the race, see §7) |
| Mic level fan‑out | `emit_levels()` emits `mic-level` to **both** the main app **and** the recording overlay | ✅ Drives the avatar's "ears" for free |

**What it does *not* do — the gaps the new work fills:** no click‑through (`set_ignore_cursor_events` is *never* called), a fixed screen‑edge anchor (not cursor‑following), no text content (icon + mic bars only), no GPU/shader rendering (DOM/CSS), a single static size.

**Frontend half:** `src/overlay/RecordingOverlay.tsx` + `src/overlay/main.tsx` + `index.html` — a tiny standalone webview app with states `recording | transcribing | processing | speaking`, smoothed mic bars (0.7/0.3), i18n + RTL, calling typed `commands.cancelOperation()` / `commands.ttsStop()`. **The Brain Overlay will be a sibling standalone webview app from this exact template.**

---

## 3. The Brain — `src-tauri/src/brain/`

`manager.rs` (270) · `client.rs` (454) · `llama_manager.rs` (338).

- **`client.rs`** — OpenAI‑compatible **SSE streaming** (`/chat/completions`, `stream: true`); SSE lines that span chunk boundaries are buffered correctly; a sentence splitter (≥25‑char terminal rule, ≥15‑char newline rule, 220‑char clause force‑split, abbreviation suppression, char‑boundary‑safe). Returns `BrainResult { text, timing }` where `timing` parses `tokens_per_second`, `predicted_ms`, `prompt_ms`, `completion_tokens` from `usage`/`timings`.
  - ⚠️ **`ChatMessage { role: String, content: String }`** and `ChatCompletionRequest { model, messages, stream }` — **text‑only.** This is the exact struct the **vision** pillar must upgrade (see `05`).
- **`manager.rs`** — `BrainManager`: owns multi‑turn `history`, builds the prompt window from settings (`system_prompt` + optional `speakable_output_prompt` + last `context_turns × 2` messages + new user msg), has **barge‑in abort** (`current_abort: Mutex<Arc<AtomicBool>>` swapped per turn so aborting an old turn can't cancel a new one), feeds completed sentences straight into TTS (speak‑before‑finish), and emits **the event stream the whole frontend runs on**:

  | Event | Payload | Meaning |
  | --- | --- | --- |
  | `brain:thinking` | — | request sent |
  | `brain:token` | `string` | streaming text delta |
  | `brain:sentence` | `string` | a completed sentence (also fed to TTS) |
  | `brain:latency` | `{ stage: "first_token", ms }` | time‑to‑first‑token |
  | `brain:done` | `{ text, tokens_per_sec, total_ms, predicted_ms, prompt_ms }` | finished |
  | `brain:error` | `string` | failure |
  | `brain:history-cleared` | — | history reset |
  | `brain:llama-loading/ready/error` | — | local server lifecycle |

  All emitted with `app.emit(...)` (**app‑global**), so **any window can listen**. There is also `warmup()` (silent, no history, no events except llama lifecycle).
- **`llama_manager.rs`** — pre‑compiled llama.cpp server lifecycle (CUDA > Vulkan > CPU auto‑select, `-ngl all` VRAM offload, `ensure_server_running()` on demand).

> **Implication for Conversation 2.0:** **zero changes to brain logic.** The overlay simply becomes a second listener of `brain:*`. The only place the Brain must change at all is the **multimodal `ChatMessage` upgrade for vision** (`05`), which is additive.

---

## 4. Conversation Mode 1.0 (what "2.0" builds on)

- **UI:** `src/components/conversation/ConversationView.tsx` — a sidebar tab in the main window: chat transcript, text input, voice‑mode toggle, per‑message metrics (🎤 `stt_ms`, `t/s`, `ms`), read‑aloud toggle, latency HUD.
- **Hands‑free engine:** `managers/continuous_voice.rs` (210) — VAD‑segmented loop: pause listening → STT → save history → `BrainManager::ask` → TTS → re‑arm.
- **Driven by `BrainConfig`** (`settings.rs`):
  - `enabled`, `system_prompt`, `context_turns` (default 20), `read_aloud` (default true), `speakable_output_prompt`
  - `conversation_mode: "push_to_talk" | "toggle" | "hands_free"` (string)
  - `endpoint_preset: "snappy"(300 ms) | "balanced"(600) | "patient"(1200)` (string)
  - `headphone_mode` (barge‑in during TTS)
- **Triggers:** a dedicated **`converse` hotkey** binding; a **`wake_word.rs`** module exists.
- **The limitation that motivates 2.0:** the reply renders **only inside the main window**. In another app you *hear* TTS but *see* nothing — and switching windows defeats the voice‑native promise.

---

## 5. Other directly relevant pieces

- **`input.rs`** (155) — `get_cursor_position()` (enigo, global coords; X11‑reliable, Wayland‑unreliable) and layout‑independent paste: `send_paste_ctrl_v`, `send_copy_ctrl_c`, `send_paste_ctrl_shift_v`, `send_paste_shift_insert`, `paste_text_direct`. → powers **Insert at cursor** and **cursor‑follow**. Wayland caveats are already noted in comments.
- **`actions.rs`** (919) — dictation pipeline + "AI Replace Selection": the established pattern for *typing results into the focused app*. The overlay's Insert action reuses it.
- **TTS** (`tts/`, ~6,000 lines across many backends: kokoro, piper, elevenlabs, cartesia, openai, sapi, kitten, pocket). `tts/player.rs` (205) is a **rodio** audio thread (`Player`/`Sink`, `Append(Vec<u8>)` decoded via `Decoder`), already managing a speaking‑HUD flag and emitting playback state. **Gap:** *no realtime output amplitude event* → needed for the avatar's "mouth." A small RMS tap is the only new audio code in the whole plan (`06 §`).
- **`HerLoading.tsx`** — a polished **Three.js** (`WebGLRenderer`, `alpha: true`) *Her*‑movie loading animation: a `CatmullRomCurve3` **tube** + ring that rotates and transforms, with `rAF` easing. **This is the avatar's design DNA and proves Three.js + transparency is accepted in‑app.**
- **Settings system** — `settings.rs` (~1,400 lines) with the **serde‑default pattern** (`#[serde(default = "...")]`) for safe schema evolution. `OverlayPosition { Top, Bottom, None }` exists; `ShowOverlay.tsx` shows the toggle pattern. Footer status dots (STT/Brain/TTS, green‑orange‑gray) — the avatar's color language should match these.
- **Background residency** — `lib.rs` uses `tauri-plugin-single-instance`, `start_hidden`, tray menu (incl. **unload model**), and **`api.prevent_close()`** (close → hide to tray; the managed model/state stays alive). **So "models stay loaded when minimized to the system tray" is already true** — the overlay only needs to work while the main window is hidden.

---

## 6. Mapping the brief's senses to existing signals

The voice brief describes the avatar gaining "different senses … a mouth, eyes, ears, a brain." Almost every sense already has a backing signal:

| Sense | Backing signal | Status |
| --- | --- | --- |
| **Ears** (listening) | `mic-level` (16 smoothed bands, already fanned out to overlays) | ✅ exists |
| **Brain** (thinking) | `brain:thinking` … `brain:done` | ✅ exists |
| **Mouth** (speaking) | `tts:level` (RMS amplitude) | ❌ tiny new tap (`06`) |
| **Eyes** (seeing) | screen capture + vision turn | ❌ new pillar (`05`) |

---

## 7. Honest gaps / risks observed in the current code

1. **Webview overlay cost:** each Tauri window is a full WebView2/WKWebView/WebKitGTK (~30–80 MB). Fine for one more window; a long‑term argument for the native wgpu track.
2. **Linux transparency variance:** WebKitGTK transparency depends on the compositor; the codebase already hedges (layer‑shell fallback, env flags). The Brain Overlay must hedge identically.
3. **Wayland has no global cursor position** for ordinary clients — `enigo.location()` is X11‑reliable only. Cursor‑follow on Wayland needs a documented anchored fallback (`03 §4.4`).
4. **`hide()` after a fixed 300 ms thread sleep** is a race‑prone pattern; the new overlay should use an explicit `overlay:hidden` ack from the webview (and retrofit the recording pill — fixes an existing bug).
5. **No click‑through anywhere** — `set_ignore_cursor_events` is never called; the new overlay must be click‑through by default.
6. **`ChatMessage.content` is `String`** — blocks vision until upgraded (additive change, `05`).
7. Recording pill and Brain overlay will both want the screen → a simple z/priority coexistence rule is needed (`04 §`).

---

## 8. What "the app stays the same" means concretely

**Untouched:** main window + all tabs (incl. Conversation 1.0), recording‑pill behavior, all default settings *values*, tray, dictation/read‑aloud pipelines, history schema (at most one additive nullable column), existing command bindings.

**New code lands in new files:** `src-tauri/src/overlay_fx/` (window/follow/placement/events + optional `native/` wgpu), `src-tauri/src/vision/` (capture + region selector), `src/brain-overlay/` (the webview app), `src/components/settings/overlay-mode/`. Existing files get only **tiny additive touch‑points**: `settings.rs` (new structs with serde defaults), `lib.rs` (register the new window/commands), `brain/client.rs` (multimodal content enum), `tts/player.rs` (RMS tap), `overlay.rs::emit_levels` (one more fan‑out target). A PR checklist item enforces: *zero diffs outside new modules except registrations.*
