# 04 — Conversation Mode 2.0

The user‑facing behavior of the overlay: a streaming reply bubble anchored at the cursor, fronted by the 3D avatar, driven by the **existing** Brain event stream. **Zero changes to `BrainManager` logic** — Conversation 2.0 is plumbing + rendering + one new event sink.

---

## 1. Core user journey

```
1. You're in any app (IDE, browser, game, doc).
2. Press the converse hotkey  (or say the wake word).            → avatar fades in near cursor, EARS open
3. You speak: "what's wrong with this function?"                  → ears react to your voice (mic-level)
   (optionally: tap the region-grab key first to show it your    → eyes light up; screenshot attached (05)
    screen, or drag a rectangle)
4. You stop. VAD ends the turn → STT → Brain.                     → avatar THINKS (brain-core spins, orbit)
5. The reply streams into the glass bubble at your cursor,        → MOUTH waveform pulses with TTS
   spoken aloud at the same time (read_aloud).
6. You press Insert → the answer is typed at your cursor.         → (or Copy / Open in tab / Regenerate)
   Or press Esc → it fades away. Or just keep talking → barge-in. → avatar settles / dismisses
```

The whole loop **never steals focus**: the caret stays in your editor the entire time; nothing is inserted unless you press Insert.

---

## 2. Overlay conversation state machine

This **is** the avatar's state machine (`06 §2`); the two are one.

```
            converse hotkey / wake word
   HIDDEN ───────────────────────────────► LISTENING ──(VAD end)──► TRANSCRIBING ──► THINKING
     ▲                                        │  ▲                                      │
     │ Esc / dismiss / auto-hide              │  │ barge-in (speak/hotkey)              │ brain:token…
     │                                        │  │                                      ▼
     └───────────────── DONE ◄──── SPEAKING ◄─┴──┴──────────────── STREAMING ◄──────────┘
                          ▲           │  (tts:level)                 (bubble fills)
                          │           │
                       (no TTS)   ERROR (brain:error) ──(2s)──► DONE/HIDDEN
```

- `LISTENING` shows the avatar's ears reacting to `mic-level`.
- Optional `SEEING` sub‑state (eyes lit) overlaps when a screenshot is attached (`05`).
- `barge-in` from any state with audio → abort + back to `LISTENING` (same semantics as 1.0's `current_abort`).

---

## 3. IPC contract (all typed via `tauri-specta`)

### 3.1 Events consumed by the overlay (existing — fan‑out only, NO backend change)

| Event | Today | Overlay use |
| --- | --- | --- |
| `mic-level` | already emitted to recording overlay via `overlay::emit_levels` | avatar **ears**; **extend the fan‑out** to the `brain_overlay` label (one line) |
| `brain:thinking` | exists | THINKING state |
| `brain:token` | exists | append to bubble (coalesce per `rAF`) |
| `brain:sentence` | exists | optional sentence highlight synced with TTS |
| `brain:latency` | exists (`first_token` ms) | "⚡ 280 ms" chip |
| `brain:done` | exists (`tokens_per_sec`, `total_ms`, …) | metrics chip (`42 t/s · 1.3 s`) — same data the Conversation tab shows |
| `brain:error`, `brain:llama-loading/ready` | exist | ERROR state / "warming up GPU…" micro‑status |
| `brain:history-cleared` | exists | clear bubble context indicator |

### 3.2 New events (small, additive)

| Event | Payload | Emitter |
| --- | --- | --- |
| `overlay:show-conversation` | `{ anchor: {x,y,monitor}, mode: "follow"\|"pinned"\|"anchored" }` | shortcut / wake‑word handler |
| `overlay:state` | `OverlayConvState` enum (drives the avatar) | `overlay_fx` |
| `overlay:hidden` | ack from the webview after fade‑out (**replaces the 300 ms sleep‑then‑hide race**) | overlay webview |
| `tts:level` | `f32` RMS @ ~30 Hz during playback + a final `0.0` on stop | tiny tap in `tts/player.rs` (`06`) |
| `vision:capture-started` / `vision:attached` | `{ kind: "full"\|"region", w, h }` | `vision` module (`05`) |

### 3.3 New commands (Rust, `commands/overlay.rs`)

```
overlay_converse_trigger(mode)        // start a converse turn (hotkey/wake word/tray)
overlay_dismiss()                     // fade + hide (+ abort if streaming)
overlay_insert_at_cursor(text)        // reuse input.rs paste pipeline
overlay_copy(text)                    // clipboard
overlay_open_in_conversation(turn)    // hand off to the main window's Conversation tab
overlay_regenerate()                  // re-ask last user turn
overlay_set_pinned(bool)              // follow ⇄ pinned
overlay_probe_capabilities()          // OverlayCapabilities (03 §4.5)
```

> The trigger handler is the **only** new orchestration: on hotkey, decide `mode`, compute the anchor from `get_monitor_with_cursor()`, `emit("overlay:show-conversation", …)`, show the pre‑created window, then drive the *existing* converse pipeline (`continuous_voice` / `BrainManager::ask`). The recording pill is suppressed while the overlay converses (`§6`).

---

## 4. The reply bubble

- **Glass / frosted** panel anchored to the cursor (quadrant‑aware, `03 §3`), cyberpunk‑tinted to match the avatar accent. A small **tail** points from the bubble toward the avatar.
- **Streaming text:** append‑only; tokens coalesced per `rAF` (never re‑layout prior lines). A blinking caret while streaming.
- **Markdown‑lite:** bold / inline code / fenced code blocks / links, rendered with the app's existing markdown styling (free in the webview). Code blocks get a copy button.
- **Header line:** "🎤 you said: …" with `stt_ms`, when available (from the transcript/`brain:asked`).
- **Footer chips:** `42 t/s · 1.3 s · ⚡280 ms` (reuse the Conversation tab's metric formatting). If a screenshot was attached, a small 🖼 thumbnail chip.
- **Long replies:** grow to `max_height` (S/M/L), then inner‑scroll (`↑/↓`); an optional "collapse" affordance.
- **RTL:** `dir` from `getLanguageDirection` (the recording overlay already does this) — bubble, tail side, and action bar all mirror.

---

## 5. Keyboard‑first quick actions

Registered as a chord layer **only while the overlay is shown** (deregistered on hide), rebindable in Settings → Bindings:

| Key (default) | Action |
| --- | --- |
| `Enter` (with overlay‑modifier) | **Insert at cursor** (reuse `input.rs` paste; honors Wayland caveats) |
| `C` | Copy reply |
| `O` | Open in Conversation tab (hand off the turn) |
| `R` | Regenerate |
| `P` | Pin / unpin (stops following, disables auto‑hide) |
| `S` | **Show it my screen** (region‑grab; full‑screen with a modifier — `05`) |
| `Esc` or converse‑hotkey tap | Dismiss / barge‑in |
| `↑ / ↓` | Scroll the bubble |

Mouse users get the same actions in the interactive action bar (`03 §5`).

---

## 6. Coexistence rules (recording pill, speaking HUD, vision)

- **One overlay at a time visually.** While the Brain overlay is conversing, **suppress the recording pill** (the avatar's own ears/states replace it). Keep a single source of truth for "am I listening/speaking" so the pill and avatar never both show.
- **Speaking HUD:** the pill's `speaking` state is also subsumed by the avatar's mouth; if the user has the overlay **off**, the pill behaves exactly as today (unchanged).
- **Vision capture:** while the region selector is up (`05`), the avatar pauses follow and the bubble waits; on attach, flow resumes.
- **Z‑order:** if both ever coexist (edge cases), the recording pill wins the top band; the avatar yields placement to avoid overlap.

---

## 7. New settings — `OverlayModeConfig` (additive, serde‑defaulted)

```rust
pub struct OverlayModeConfig {
    pub enabled: bool,                 // default false → app is byte-identical to today
    pub trigger: String,               // "converse_hotkey" | "wake_word" | "both"
    pub placement: String,             // "follow" | "pinned" | "anchored"
    pub anchor_corner: String,         // for anchored/Wayland: "br" | "bl" | "tr" | "tl"
    pub size: String,                  // "S" | "M" | "L"  (bubble width + avatar size)
    pub auto_hide_secs: u32,           // 0 = never; else fade after N s of inactivity
    pub exclude_from_capture: bool,    // Win/macOS screen-share privacy (default false)
    pub reduced_motion: bool,          // also honors OS prefers-reduced-motion
    pub show_trail: bool,              // the cursor→avatar tether / ambient trail (Track B)
    pub renderer: String,              // "auto" | "webgpu" | "webgl" | "native"
    pub avatar: AvatarConfig,          // see 06 §6
    pub vision: VisionConfig,          // see 05 §6
}
```

Settings → **Overlay Mode** group with a **live preview** canvas cycling the avatar states; every option capability‑gated (`03 §4.5`); i18n keys added across the 20 locales (English fallback acceptable for beta, gated by the existing `check-translations` CI).

---

## 8. Edge cases & answers

| Case | Behavior |
| --- | --- |
| Trigger while a previous reply is streaming | barge‑in (abort + new LISTENING) — same as 1.0 |
| Cursor on a different monitor than the trigger | anchor = the cursor's monitor at trigger time (`get_monitor_with_cursor`) |
| Fullscreen **exclusive** game | overlay may be invisible (OS limit) — TTS still speaks; documented; **borderless‑fullscreen works** |
| Screen sharing | honor `exclude_from_capture` (Win/macOS); default visible |
| Wake word fires while a password field is focused | overlay never takes focus; **nothing is typed anywhere** unless the user presses Insert |
| Brain disabled / no model | avatar shows ERROR for ~2 s with "Brain is disabled — open Settings" (reuse `brain.enabled` message) |
| Very fast dismissal mid‑stream | abort + hide; the stream task is already abort‑safe (`current_abort`) |
| Vision attached but model isn't multimodal | warn once; send text‑only; offer to pick a vision model (`05 §5`) |

---

## 9. What Conversation 2.0 does **not** change

- `BrainManager` semantics, the event names/payloads, the sentence splitter, barge‑in logic, `continuous_voice` loop, `conversation_mode` / `endpoint_preset` / `headphone_mode` / wake word, per‑message metrics, history schema (at most one additive `source: "overlay"|"window"` column).
- The Conversation **tab** keeps working exactly as today; "Open in Conversation tab" simply hands the overlay's turn to it.

Conversation 2.0 is the **same brain, same voice, new stage.**
