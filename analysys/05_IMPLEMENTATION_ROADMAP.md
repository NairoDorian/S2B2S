# 05 — Implementation Roadmap

Phased so each phase ships something usable, keeps `main` releasable, and honors the cross-platform mandate. Effort assumes one experienced dev; sizes are S (≤2 d), M (≤1 wk), L (≤2 wk).

---

## Phase 0 — Groundwork (no user-visible change) — ~1 week

| # | Task | Files | Size |
| --- | --- | --- | --- |
| 0.1 | `OverlayModeConfig` + `AvatarConfig` settings structs, serde defaults, export bindings | `settings.rs`, `bindings.ts` | S |
| 0.2 | `overlay_fx/` module skeleton + `OverlayCapabilities` probe command | `src-tauri/src/overlay_fx/*`, `commands/overlay.rs` | S |
| 0.3 | Extract shared helpers from `overlay.rs` (`get_monitor_with_cursor`, topmost forcing) into `overlay_fx/shared.rs` — pure refactor, recording pill behavior byte-identical | `overlay.rs`, `overlay_fx/shared.rs` | S |
| 0.4 | **`tts:level` tap** in rodio player + final-zero on stop/abort (04 §5) | `tts/player.rs` | S |
| 0.5 | Fan `mic-level` out to `brain_overlay` window label (no-op until window exists) | `overlay.rs::emit_levels` | S |
| 0.6 | Replace sleep-then-hide with `overlay:hidden` ack pattern (fixes existing race too) | `overlay.rs`, `RecordingOverlay.tsx` | S |
| 0.7 | CI: add `--features overlay-native` cargo check matrix entry (compiles empty stub) | workflows | S |

**Exit:** all tests green, recording pill unchanged, new events visible in log viewer.

## Phase 1 — Track A MVP: Brain Overlay + Avatar v1 (Windows + macOS; Linux-X11 best effort) — ~2–3 weeks

| # | Task | Size |
| --- | --- | --- |
| 1.1 | `brain_overlay` window creation per OS (NSPanel recipe / layered+topmost / layer-shell), pre-created hidden at startup, click-through on | M |
| 1.2 | New webview app `src/brain-overlay/` (sibling of `src/overlay/`): event wiring (`brain:*`, `mic-level`, `tts:level`, `overlay:*`), bubble with streaming text (token coalescing per rAF), metrics chip, markdown-lite | M |
| 1.3 | Avatar v1 procedural renderer (states idle/listening/thinking/speaking/error; shader or canvas; reduced-motion path) | M |
| 1.4 | Cursor-follow service + placement/quadrant-flip/clamping + freeze-on-stream (02 §3) | M |
| 1.5 | `overlay_converse_trigger` wired to the converse hotkey behind `overlay.enabled` setting; suppress recording pill during converse | S |
| 1.6 | Quick actions: Insert at cursor (reuse paste pipeline), Copy, Dismiss + chord layer registration on show/hide | M |
| 1.7 | Settings → Overlay Mode group + Live Preview; i18n keys ×20 locales; capability-based disabling | M |

**Exit demo:** in VS Code, hit hotkey → avatar listens → ask "write a haiku about Rust" → reply streams beside cursor, spoken aloud → `Enter` inserts it into the editor → `Esc`. Focus never moved.

## Phase 2 — Conversation 2.0 complete — ~2 weeks

| # | Task | Size |
| --- | --- | --- |
| 2.1 | Barge-in from overlay (speak / hotkey) incl. `headphone_mode` parity; hands-free auto-listen loop in overlay | M |
| 2.2 | Wake-word → `overlay_converse_trigger` | S |
| 2.3 | Pin mode, anchored mode, Wayland anchored fallback + interactive action-bar input-region | M |
| 2.4 | Open-in-Conversation-tab handoff (preload turn), Regenerate, history `source` column | M |
| 2.5 | Long-reply collapse, code-block copy, scroll keys, RTL polish | M |
| 2.6 | Coexistence rules with pill + speaking HUD; exclude-from-capture setting (Win/macOS) | S |

**Exit:** full 03 spec; Linux X11 at parity; Wayland degraded-mode documented in README.

## Phase 3 — Track B: native wgpu overlay (the CursorFX phase) — ~3–4 weeks, parallelizable

| # | Task | Size |
| --- | --- | --- |
| 3.1 | **Unblock & vendor `Cross_Platform_Rust_WebGPU_CursorFX`** as `crates/cursorfx` (or fork-merge); gap analysis vs 02 §6 checklist | S–M |
| 3.2 | winit window + per-OS flags + wgpu surface with alpha (per-backend `CompositeAlphaMode` selection, fallback ladder) behind `overlay-native` feature | L |
| 3.3 | Avatar WGSL port (same `avatarFrame` math) + CursorFX particle pass as Thinking particles + optional cursor-trail cosmetic setting | M |
| 3.4 | glyphon text pass: streaming layout, RTL, code styling | L |
| 3.5 | Per-pixel/region hit testing; runtime renderer switch (native ⇄ webview) with auto-fallback on surface failure | M |
| 3.6 | Perf pass against 02 §7 budget on iGPU laptop, battery test | M |

**Exit:** native renderer default-on where probe says safe (Windows first), webview fallback always available.

## Phase 4 — Polish & brand — ~1–2 weeks

Onboarding intro by the avatar · `HerLoading` morph into avatar · avatar skins (param packs) + community naming vote · tray flair (optional) · docs (README pipelines diagram gains the Overlay branch; new `OVERLAY.md`) · demo GIFs for the repo.

---

## Risk register

| Risk | Likelihood | Impact | Mitigation |
| --- | --- | --- | --- |
| CursorFX repo stays inaccessible | M | M (Phase 3 slower, not blocked) | Track B reference design is self-sufficient; CursorFX is an accelerator, not a dependency |
| Wayland limitations frustrate users | H | M | honest anchored mode + clear settings copy + README matrix; revisit when portals improve |
| WebKitGTK transparency broken on some distros | M | M | compositor probe → opaque card theme fallback (02 §8) |
| Webview RAM cost complaints | M | L | window pre-created but webview lazy-loaded on first trigger; Track B endgame |
| Click-through island toggling feels flaky over remote desktop / odd cursors | M | M | keyboard-first design means mouse is optional; island logic disabled ⇒ pure click-through still fully usable |
| Focus theft regressions (the cardinal sin) | L | H | dedicated test: caret stays in Notepad/TextEdit/gedit across full converse cycle, asserted in QA checklist every release |
| Topmost lost over games/UAC | M | L | 2 s topmost watchdog (Windows), document exclusive-fullscreen limit |
| Per-token rerender jank on long replies | M | M | rAF coalescing + append-only DOM/text-buffer, perf test with 4k-token reply |
| i18n debt (20 locales × new strings) | H | L | `check-translations` CI gate; English fallback acceptable for beta |
| Scope creep into the existing app | M | H | "app stays the same" principle; PR checklist item: zero diffs outside new modules except registrations |

## Test matrix (minimum)

- **OS:** Win 11 (100 %/150 %, 2 monitors mixed DPI) · macOS (notch laptop + external, Spaces, fullscreen app) · Ubuntu X11 · Fedora GNOME Wayland (fallback path) · Hyprland/KDE Wayland (layer-shell path).
- **Scenarios:** trigger in IDE/browser/terminal · barge-in mid-stream · hands-free 5-turn session · monitor hop mid-follow · insert into password field forbidden-by-design check (no auto-insert ever) · screen-share with exclusion on/off · reduced motion · RTL locale (he) · Brain disabled · llama.cpp cold start (loading state) · 4k-token reply · abort within 200 ms of trigger.
- **Automation:** Playwright already in repo — add overlay-webview component tests (state machine snapshots driven by synthetic events); Rust unit tests for placement math (quadrant flip/clamping, table-driven); screenshot-diff harness for avatar states.

## Definition of done (whole initiative)

1. `overlay.enabled=false` ⇒ app is bit-for-bit behaviorally identical to today.
2. The full demo of Phase 1 exit works on all three OSes (Wayland in anchored mode).
3. Performance within 02 §7 budget on an iGPU laptop.
4. No focus theft, ever, in the QA matrix.
5. Docs: README section + `OVERLAY.md` + settings tooltips + this plan archived under `docs/plans/`.
6. CHANGELOG entry under "S2B2S v0.11 (Overlay & Avatar)" following Keep-a-Changelog style.
