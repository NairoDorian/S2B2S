# 03 — Conversation Mode 2.0

Conversation 1.0 (the in-window tab + hands-free engine) stays untouched. **Conversation 2.0 = the same pipeline, rendered at your cursor**, with the avatar as the visible agent. This doc specifies UX, state machine, IPC contract, settings, and edge cases.

---

## 1. Core user journey

```
 You're in any app (IDE, browser, game)…
 ─────────────────────────────────────────────────────────────────────────
 1. TRIGGER     converse hotkey ▸ or wake word ▸ or double-tap modifier
 2. APPEAR      avatar fades in next to cursor (<120 ms), Listening state,
                ripples to your voice (mic-level events)        — focus NEVER leaves your app
 3. ENDPOINT    VAD endpoint (snappy/balanced/patient preset) → avatar Thinking
 4. STREAM      reply bubble grows next to avatar, tokens stream in
                (brain:token), TTS speaks in parallel (read_aloud),
                avatar mouth pulses with audio (tts:level)
 5. ACT         ⏎ Insert at cursor · C Copy · O Open in Conversation tab ·
                R Regenerate · Esc Dismiss · just speak again → barge-in
 6. VANISH      auto-hide after N s of inactivity (configurable), or Esc
```

The "brain prompt result next to the cursor" requirement is step 4: the **streaming reply bubble** is anchored to the cursor position captured at trigger time (frozen while you read — see 02 §3.4).

---

## 2. Overlay conversation state machine

States map 1:1 onto **existing** backend signals (no new pipeline states invented):

```
                 hotkey/wake word
        ┌──────────────┐
 HIDDEN ─────────────► LISTENING ──VAD endpoint──► THINKING ──first brain:token──► STREAMING
   ▲                    │  ▲  (mic-level drives avatar)（brain:thinking)              │
   │     Esc/timeout    │  └──── barge-in (speak / hotkey) ◄────────────────────────┤
   ├────────────────────┤                 (BrainManager::abort — already exists)    │
   │                    │                                                            ▼
   │                 ERROR ◄──brain:error────────────────────────────  DONE+SPEAKING (tts:level)
   │                    │                                                            │
   └────────────────────┴──────────────auto-hide countdown──────────────────────────┘
                                        (paused while pointer over bubble / pinned)
```

- **Barge-in:** identical semantics to today — `BrainManager::abort()` kills the in-flight stream and stops TTS; the overlay clears the bubble and returns to LISTENING. `headphone_mode` gates barge-in during playback exactly as it does now.
- **Hands-free:** when `conversation_mode = hands_free` + `auto_listen`, DONE → LISTENING automatically; the overlay stays up as a persistent companion until dismissed.
- **Push-to-talk / toggle:** existing binding behavior unchanged; the overlay just visualizes it.

---

## 3. IPC contract (all typed via tauri-specta)

### 3.1 Events consumed by the overlay window (existing — fan-out only)

| Event | Today | Overlay use |
| --- | --- | --- |
| `mic-level` | already emitted to recording overlay via `overlay::emit_levels` | avatar Listening ripples — extend fan-out to `brain_overlay` |
| `brain:thinking` | exists | Thinking state |
| `brain:token` | exists | append to bubble (coalesce per frame) |
| `brain:sentence` | exists | optional sentence-highlight sync with TTS |
| `brain:done` | exists (`tokens_per_sec`, `total_ms`) | metrics chip (`42 t/s · 1.3 s`) — same data Conversation 1.0 shows |
| `brain:asked` | exists (`stt_ms`, transcript) | show "you said: …" header line with 🎤 latency |
| `brain:error`, `brain:llama-loading/ready` | exist | Error state / "warming up GPU…" micro-status |

### 3.2 New events

| Event | Payload | Emitter |
| --- | --- | --- |
| `overlay:show-conversation` | `{ anchor: {x,y,monitor}, mode: "follow"\|"pinned"\|"anchored" }` | shortcut/wake-word handler |
| `overlay:state` | `OverlayConvState` enum (drives avatar) | `overlay_fx` |
| `overlay:hidden` | ack from webview after fade-out (replaces the 300 ms sleep-then-hide race) | overlay webview |
| `tts:level` | `f32` RMS @ ~30 Hz during playback | small tap in `tts/player.rs` (see 04 §5) |

### 3.3 New commands (Rust, in `commands/overlay.rs`)

- `overlay_converse_trigger()` — capture cursor anchor, ensure window, enter LISTENING (shared by hotkey, wake word, tray menu item).
- `overlay_insert_result(text)` — reuse the dictation paste pipeline (`actions.rs` / `send_paste_ctrl_v`, clipboard-restore behavior and `paste_delay` respected). Focus is still on the user's app because the overlay never took it — that is the whole trick.
- `overlay_dismiss()`, `overlay_pin(bool)`, `overlay_set_interactive(bool)` (click-through island toggle), `overlay_open_in_main()` — opens main window on the Conversation tab with this turn preloaded (turns are already in `BrainManager` history + SQLite history manager).
- `overlay_regenerate()` — re-ask last user turn (drop last assistant turn from history, call `ask` again).

---

## 4. The reply bubble

- **Layout:** avatar (left, 56–96 px per size setting) + glass bubble (blurred translucent bg, 1 px hairline, 14 px radius — consistent with existing pill styling). Width S/M/L = 280/360/460 logical px; height grows to `max_height` (default 40 % of monitor height) then scrolls inside, autoscroll pinned to bottom while streaming.
- **Content:** "you said …" header (from `brain:asked`) → streaming reply with **markdown-lite** (bold, inline code, fenced code with copy button, lists; links shown but open via opener plugin only from interactive mode) → footer: metrics chip + action bar.
- **Long replies:** after `max_lines` (default 14) collapse with "↓ 23 more lines — O to open in app".
- **Code-heavy replies:** fenced blocks get monospace + per-block copy; Insert action inserts raw text (no markdown), reusing the existing TTS markdown-strip rationale in reverse.
- **i18n/RTL:** the overlay webview reuses `i18n` + `syncLanguageFromSettings()` + `getLanguageDirection` exactly like `RecordingOverlay.tsx`; bubble mirrors in RTL; all new strings added to the 20 locale files (`bun run check-translations` gate).

---

## 5. Keyboard-first quick actions

Because the overlay is click-through and non-activating, the primary interaction is a **chord layer** active only while the overlay is visible (registered/unregistered with show/hide so we never squat global keys):

| Key (default, rebindable in Settings → Bindings) | Action |
| --- | --- |
| `Enter` (with overlay-modifier) | Insert at cursor |
| `C` | Copy reply |
| `O` | Open in Conversation tab |
| `R` | Regenerate |
| `P` | Pin / unpin (stops following, disables auto-hide) |
| `Esc` or converse-hotkey-tap | Dismiss / barge-in |
| `↑/↓` | scroll bubble |

Mouse remains available via the interactive island (02 §5) for users who prefer it.

---

## 6. Coexistence rules

- **Recording pill vs Brain overlay:** when a 2.0 session is active, the avatar's Listening state *replaces* the recording pill (suppress `show_recording_overlay` for converse-initiated recordings); plain dictation continues to use the pill untouched.
- **TTS speaking HUD:** the existing `speaking` pill state remains for read-aloud-of-selection; avatar speaking visuals apply only to Brain replies.
- **Conversation tab simultaneously open:** both render the same events — they stay in sync for free; the tab remains the place for long-session reading/history.
- **History:** 2.0 turns flow through the same `BrainManager` history + `HistoryManager` persistence as 1.0; one additive SQLite column `source: "window"|"overlay"` for analytics/filtering.

---

## 7. New settings — `OverlayModeConfig` (additive, serde-defaulted)

```rust
pub struct OverlayModeConfig {
    pub enabled: bool,                  // default false — opt-in, app unchanged by default
    pub placement: OverlayPlacement,    // FollowCursor | Pinned | Anchored(corner)  (Wayland → Anchored)
    pub avatar: AvatarConfig,           // see 04 §6 (style, size, reduced_motion, theme)
    pub bubble_size: SizeS_M_L,
    pub max_height_pct: u8,             // default 40
    pub opacity: f32,                   // default 0.97
    pub auto_hide_secs: u16,            // default 12, 0 = never
    pub click_through: bool,            // default true
    pub show_metrics_chip: bool,        // default true (matches 1.0 metrics)
    pub exclude_from_capture: bool,     // default false (Win/macOS only, per probe)
    pub suppress_recording_pill_in_converse: bool, // default true
}
```

Settings UI: new **"Overlay Mode"** group in Settings (components under `src/components/settings/overlay-mode/`), following the existing `SettingsGroup`/`SettingContainer`/`ToggleSwitch` patterns, with capability-based disabling from `OverlayCapabilities` (02 §4.5) and a **Live Preview** button that shows the overlay with a canned streamed reply.

---

## 8. Edge cases & answers

| Case | Behavior |
| --- | --- |
| Trigger while previous reply streaming | barge-in (abort + new LISTENING) — same as 1.0 |
| Cursor on a different monitor than trigger | anchor = cursor's monitor at trigger time (existing `get_monitor_with_cursor`) |
| Fullscreen game (exclusive mode) | overlay may be invisible (OS limit) — TTS still speaks; document; borderless-fullscreen works |
| Screen sharing | respect `exclude_from_capture`; default visible |
| Wake word triggers while typing password field | overlay never takes focus; nothing is typed anywhere unless user presses Insert |
| Brain disabled / no model | avatar appears in Error state for 2 s with "Brain is disabled — open Settings" (existing `brain.enabled` check message) |
| Very fast dismissal mid-stream | abort + hide; stream task already abort-safe |
