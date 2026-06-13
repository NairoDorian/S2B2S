# 07 — Implementation Roadmap

A phased plan to get from today's S2B2S to the overlay vision **without ever breaking the app that exists.** The guardrail across every phase: **all new config defaults to `enabled: false`**, so until the user opts in, S2B2S is **byte‑identical to today**. Each phase ships something usable on its own; the order front‑loads the visible win (the face) and defers the heaviest GPU work (native wgpu) to the end.

**Sequencing rationale**
- **Track A (webview avatar) before Track B (native wgpu):** the face, the bubble, and the converse loop are the user‑visible product; they reuse `HerLoading` + the recording‑overlay template and ship in weeks (`03 §1.2`).
- **Refactor in Phase 2, not Phase 1:** `overlay.rs` stays **untouched in Phase 1** (`03 §2`); only once `brain_overlay` exists and proves which helpers are shared do we extract them — a pure refactor with a regression‑tested pill.
- **Vision in Phase 3:** the eyes need the bubble + avatar to land first.
- **Native wgpu last:** the ambient trail/tether is the "more powerful GPU" layer; CursorFX already solved its OS quirks, so it's lower‑risk *and* additive once the rest works.

```
P0 Groundwork ─► P1 Avatar v1 + converse loop ─► P2 Conversation 2.0 + refactor
      (invisible)        (Track A, the win)             (complete + share helpers)
                                                              │
                         P5 Polish & brand ◄── P4 Native wgpu FX ◄── P3 Vision (eyes)
                            (ship)              (Track B, tether)        (NEW pillar)
```

---

## Phase 0 — Groundwork (no user‑visible change)

Lay the contracts and the skeleton; touch **nothing** the user sees.

| Task | File(s) | Notes |
| --- | --- | --- |
| Config structs (serde‑defaulted, `enabled:false`) | `settings.rs` | `OverlayModeConfig` (`04 §7`), `AvatarConfig` (`06 §6`), `VisionConfig` (`05 §5`) — same default pattern as `BrainConfig` |
| Typed bindings | `cargo test export_bindings` (tauri‑specta) | front‑end gets the new types for free |
| `overlay_fx/` skeleton | `src-tauri/src/overlay_fx/{mod,window,cursor_follow,placement,events,capabilities}.rs` | `native/` behind feature `overlay-native` (compiles as a stub) — `03 §2` |
| Capability probe | `overlay_fx/capabilities.rs` | `OverlayCapabilities` (`03 §4.5`) → typed binding so Settings greys out unsupported options per machine |
| **`tts:level` RMS tap** | `tts/player.rs` | the **only new audio code** (`06 §5`); a pass‑through `Source` wrapper emitting `f32` @ ~30 Hz + a trailing `0.0` |
| CI feature stub | build matrix | add `overlay-native` to the matrix so it compiles on Win/macOS/Linux from day 1 |

**Do not** restructure `overlay.rs` here (`03 §2`: untouched in Phase 1).
**Exit:** app behaves exactly as today; new structs serialize with defaults; bindings export; `probe()` returns sane per‑OS values; `tts:level` emits during playback with no audible change.

---

## Phase 1 — Avatar v1 + the converse loop (Track A — the visible win)

The whole user‑facing experience, in a transparent webview.

| Task | File(s) | Notes |
| --- | --- | --- |
| `brain_overlay` window, created **hidden at startup** | `overlay_fx/window.rs` | per‑OS flags **lifted from `overlay.rs`**: NSPanel (`PanelLevel::Status`, `no_activate`) / `HWND_TOPMOST` / GTK layer‑shell — `03 §4` |
| Overlay webview app | `src/overlay/` (sibling template of `RecordingOverlay.tsx` + `main.tsx`) | reuses i18n/RTL + typed `commands.*` |
| Avatar v1 (Three.js) | `src/overlay/avatar/` | `WebGPURenderer` → WebGL2 fallback (`06 §4`); **7 states** (`06 §2`) wired to events |
| Ears + mouth | `overlay::emit_levels` (one‑line fan‑out to `brain_overlay`) + reuse `tts:level` | `04 §3.1`, `06 §5` |
| Cursor‑follow | `overlay_fx/cursor_follow.rs` + `placement.rs` | 30 Hz follow, quadrant flip, **freeze‑on‑speak** — `03 §3` |
| Converse trigger | `commands/overlay.rs` → `overlay_converse_trigger` | hotkey → `get_monitor_with_cursor()` → show → drive the **existing** `continuous_voice` / `BrainManager::ask` (**no brain changes**) — `04 §3.3` |
| Reply bubble | `src/overlay/bubble/` | streaming append (coalesce per `rAF`), markdown‑lite, metric chips — `04 §4` |
| Quick actions | `commands/overlay.rs` (`insert_at_cursor`, `copy`, `regenerate`, …) | registered **only while shown**; Insert reuses `input.rs` paste — `04 §5` |
| Settings group | `src/.../settings/OverlayMode*` | **Overlay Mode** with live‑preview canvas; capability‑gated — `04 §7` |

**Exit:** in any app, the converse hotkey makes the avatar appear at the cursor, hear you, think, and **stream + speak** the Brain's reply into the bubble; **Insert** types at the cursor, **Esc** dismisses; the **main window can stay hidden in the tray** the whole time (residency already works — `lib.rs`); the recording pill is suppressed while the overlay converses (`04 §6`).

---

## Phase 2 — Conversation 2.0 complete + the shared‑helper refactor

Round out the UX and unify the window plumbing.

| Task | File(s) | Notes |
| --- | --- | --- |
| **Extract shared helpers** (pure refactor) | `overlay.rs` → `overlay_fx/shared.rs` | move `get_monitor_with_cursor`, `calculate_*`, `force_overlay_topmost`; **both** pill and `brain_overlay` import them — `03 §2`. Regression‑test the pill for **byte‑identical** behavior |
| Replace the **300 ms sleep‑then‑hide** | shared hide path | migrate to the `overlay:hidden` **ack** (`04 §3.2`) for both overlays |
| Barge‑in in the overlay | reuse `current_abort` | same semantics as 1.0 (`04 §8`) |
| Wake‑word trigger | `wake_word.rs` + trigger handler | `trigger = "wake_word"\|"both"` |
| Pinned / anchored modes + Wayland anchor | `placement.rs` | `03 §4.4` degraded‑but‑honest |
| Open‑in‑Conversation‑tab handoff | `overlay_open_in_conversation` | `04 §3.3` |
| RTL + auto‑hide polish | overlay webview | mirror bubble/tail/action‑bar; `auto_hide_secs` |
| Coexistence finalization | pill ⇄ avatar single source of truth | `04 §6` |

**Exit:** the full `04` behavior on Windows / macOS / Linux‑X11, Wayland degraded‑but‑honest; pill and avatar share **one** window‑helper core; no hide race.

---

## Phase 3 — Vision pillar (the eyes) [NEW]

The sense the old plan missed (`05`).

| Task | File(s) | Notes |
| --- | --- | --- |
| Capture backend | `src-tauri/src/vision/{mod,capture,region,encode}.rs` + `platform/` | **`xcap`** (Win/macOS/X11) + **`ashpd`** Wayland portal — `05 §3` |
| Encode + token guard | `vision/encode.rs` | downscale (≤1568 px long edge), PNG/JPEG, data‑URI, byte/count caps — `05 §3.2` |
| Region selector | `src/region-select/` | input‑**capturing** transparent overlay, drag rect, `Esc` cancel, physical‑pixel monitor‑aware — `05 §2` |
| **Multimodal `ChatMessage`** | `brain/client.rs` | `MessageContent` untagged enum (**back‑compatible**); `BrainManager::ask` gains optional `images` — `05 §4` |
| Eyes wired | overlay avatar | brighten + scanline + saccade on `vision:*`; `show_eyes`/`vision.enabled` invariant — `06 §3`, `05 §9` |
| Vision model selection | Brain settings | "vision‑capable" flag; macOS Screen‑Recording permission copy; privacy defaults — `05 §5/§6` |

**Exit:** full‑screen **and** region capture → a **local multimodal model** → answer streams into the bubble + speaks, **with no main window**; text‑only providers remain **byte‑identical on the wire** (serialization test) — `05` acceptance.

---

## Phase 4 — Native wgpu FX + the tether (Track B — the "powerful GPU" layer)

Vendor CursorFX for the ambient trail/glow and the cursor→avatar tether. **The old "CursorFX inaccessible" risk is resolved** — the source is read and corrected (`02`).

| Task | File(s) | Notes |
| --- | --- | --- |
| Vendor CursorFX | `crates/cursorfx/` or `overlay_fx/native/` (feature `overlay-native`) | **regen `Cargo.lock` to wgpu 29** (its lock is stale at 0.19.4) — `02 §A.4`, `03 §6` |
| **Windows: Vulkan, not DX12** | `native/platform.rs` | DX12 **OOMs** on the transparent surface (RTX 4070); apply the **NVAPI "Prefer Native" present fix** (`nvapi64.dll`, `0x20324987=0`) so NVIDIA doesn't DXGI‑wrap Vulkan and kill transparency — `03 §4.1`, `02 §A.3.3` |
| Click‑through every frame | `native/platform.rs` | WndProc subclass `WM_NCHITTEST→HTTRANSPARENT` + `WS_EX_TRANSPARENT\|LAYERED\|TOPMOST\|TOOLWINDOW\|NOACTIVATE` re‑applied per frame (verbatim CursorFX) |
| Surface | `native/mod.rs` | `SurfaceTargetUnsafe::RawHandle` on the transparent overlay window; premultiplied alpha from `caps`; on‑demand render, **zero frames hidden**, recreate on `Outdated/Lost` — `03 §6` |
| Trail + particles + **tether** | `native/renderer.rs` + `shader.wgsl` | CursorFX ribbon + circle pipelines; the cursor→avatar **tether** uses `TD_Web_Trail` spring physics + Catmull‑Rom + 4‑pass glow — `06 §7` |
| Per‑pixel hit‑test + runtime switch | `overlay_fx` | alpha‑mask interactive islands (`03 §5`); `renderer = auto\|webgpu\|webgl\|native` with the fallback ladder (`03 §8`) |

**Exit:** ambient GPU trail + tether on Windows (Vulkan) **with transparency intact**; if native surface creation fails, it **falls back to Track A** automatically (`03 §8` rung 1).

---

## Phase 5 — Polish, brand & ship

| Task | Notes |
| --- | --- |
| `HerLoading` → avatar **morph** | one continuous visual language loading → living avatar — `06 §8` |
| Onboarding | a first‑run that introduces the avatar and the converse hotkey |
| Tray micro‑states | listening/thinking/speaking on the **existing** tray (`lib.rs`) |
| **Naming vote** | retire the old "Orbi"; pick the Clippy‑2.0 successor name |
| i18n fill (20 locales) | gate via the existing `check-translations` CI |
| Accessibility pass | reduced‑motion, colorblind‑safe state cues — `06 §9` |
| Perf hardening | meet `03 §7`; verify screen‑share exclusion; docs |

---

## Risk register

| Risk | Sev | Mitigation |
| --- | --- | --- |
| ~~CursorFX internals unknown ("inaccessible")~~ | — | **RESOLVED** — source read & corrected (`02`); plan no longer guesses |
| Windows DX12 OOM on transparent surface | High | **Use Vulkan** + NVAPI present fix (`03 §4.1`); proven by CursorFX |
| WebKit (macOS/Linux) WebGPU immaturity | Med | **automatic WebGL2 fallback** in Three.js (`03 §8` rung 2); WebGPU is true on Windows/WebView2 (the #1 target) |
| Wayland: no global cursor / no self‑positioning | Med | **anchored** placement + layer‑shell; labeled honestly in Settings (`03 §4.4`) |
| Focus theft (caret leaves the user's editor) | High | non‑activating windows everywhere; **nothing typed unless Insert** (`04 §8`); covered by the focus test below |
| Vision permissions (macOS Screen Recording / Wayland portal) | Med | request with clear copy; degrade to text‑only if denied (`05 §6`) |
| Multimodal wire compatibility | Med | `#[serde(untagged)]` → text‑only **byte‑identical**; serialization test (`05` #4) |
| Perf / RAM regression | Med | hidden = zero frames; budgets in `03 §7`; `lite`/reduced‑motion auto‑select (`06 §9`) |
| Z‑order loss to games/installers | Low | `HWND_TOPMOST` re‑assert + ~2 s watchdog while visible (`03 §4.1`) |
| wgpu 29 vs stale lock | Low | regenerate `Cargo.lock` on vendor‑in (`02 §A.4`) |

---

## Test matrix

| Test | What it proves |
| --- | --- |
| **Caret‑stays test**: open Notepad (Win) / TextEdit (macOS) / gedit (Linux), trigger the overlay, converse, **type into the editor throughout** | the overlay **never steals focus**; the caret stays put (`04 §8`) |
| **Insert test**: press Insert → text lands at the caret | the paste pipeline (`input.rs`) works through the overlay |
| **DPI screenshot‑diff harness**: region‑capture at **100 / 150 / 200 %** scaling, multi‑monitor | region rect is **pixel‑accurate & monitor/DPI‑correct** (`05` #2) |
| **Transparency over capture**: screen‑share with `exclude_from_capture` on/off | avatar/bubble leak only when allowed (`03 §4.1/4.2`) |
| **Wire‑identical test**: serialize a text‑only turn old vs new `ChatMessage` | byte‑identical (`05` #4) |
| **Replay test**: feed a canned `overlay:state` / `brain:*` / `mic-level` / `tts:level` sequence with **no backend** | avatar's 7 states are event‑driven only (`06` #2) |
| **Idle‑sleep test**: leave cursor still | trail/tether **zero frames** after 2 still frames (`06` #5, `03 §7`) |
| **Fallback‑ladder test**: force native‑surface failure, then WebGPU‑unavailable, then no‑compositor | each rung degrades to something usable; bottom rung = today (`03 §8`) |
| **Per‑OS smoke**: Win11 (Vulkan), macOS (NSPanel), Linux‑X11 (layer‑shell), Wayland (anchored) | the overlay shows, follows/anchors, click‑through holds |

---

## Performance targets (from `03 §7`)

- **Hidden:** 0 frames, 0 timers, native thread parked.
- **Visible idle:** ≤24 fps avatar breathing; trail idle‑sleeps; <3 % iGPU, <1 % CPU.
- **Streaming:** ≤60 fps; token events **coalesced per `rAF`**.
- **Memory:** Track A ≤ +80 MB; Track B ≤ +20 MB.
- **Show latency:** <120 ms (window pre‑created hidden, like `recording_overlay`).
- **Battery / reduced‑motion:** static states, no particles, no trail (`06 §9`).

---

## Definition of done

1. With everything **disabled**, S2B2S is **byte‑identical** to today (no behavior, perf, or wire change).
2. Enabling Overlay Mode gives the **full `04` experience** on Windows (priority #1), macOS, and Linux‑X11; Wayland is **degraded‑but‑honest**.
3. The **3D avatar** (`06`) renders with true transparency, shows all 7 states, and binds its four senses to real signals (3 existing + the one `tts:level` tap).
4. **Vision** (`05`) works end‑to‑end into a local multimodal model with the privacy invariants intact.
5. **Native wgpu** trail + tether (`04`/`06 §7`) runs on Windows via **Vulkan** with transparency, and **falls back** cleanly when unavailable.
6. **Zero changes** to `BrainManager` logic, event names/payloads, the recording pill's behavior (post‑refactor regression‑verified), and the Conversation tab.
7. The test matrix passes; performance meets `03 §7`.

The plan adds **four pillars** — a GPU transparent overlay, Conversation 2.0, screen vision, and a 3D avatar — as a **single optional mode** layered on top of an app that, until you flip the switch, **stays exactly as it is.**
