# Overlay implementations compared — ZER0 and four sibling projects

**Date:** 2026-09-14 · **Scope:** how each codebase builds its transparent,
always-on-top overlay window; the cross-OS story; the backend that drives it;
and what ZER0 (`Handy_Multi_STT`) should learn from each. Every claim below was
verified against the named files in the named repos — the five projects:

| #   | Project                                 | Path                                           | Purpose of its overlay                                                                            |
| --- | --------------------------------------- | ---------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| 1   | **ZER0** (this repo, `Handy_Multi_STT`) | `PROJECTS\Handy_V2`                            | Live dictation card: status, streaming text, speech stats, spectrum                               |
| 2   | **S2B2S**                               | `PROJECTS\STT_BRAIN_TTS\S2B2S`                 | Same lineage, further evolved: recording overlay + a second "brain overlay" (LLM bubble)          |
| 3   | **CopySpeak**                           | `PROJECTS\STT_BRAIN_TTS\copyspeak`             | TTS "HUD": spoken-text marquee + waveform, permanently click-through                              |
| 4   | **AIVORelay**                           | `PROJECTS\STT_BRAIN_TTS\AIVORelay`             | Five overlays: recording card, live-text preview, TTS window, command confirm, voice button       |
| 5   | **CursorFX**                            | `PROJECTS\Cross_Platform_Rust_WebGPU_CursorFX` | GPU-drawn cursor effects on a full-virtual-desktop transparent surface (wgpu, not a text webview) |

---

## 1. The five implementations, in brief

### ZER0 (`Handy_Multi_STT`)

Tauri 2 webview window, built programmatically at startup and kept hidden
(`overlay.rs:443` `create_recording_overlay`, non-macOS; `:505` macOS).
Flags: `transparent(true)`, `always_on_top(true)`, `decorations(false)`,
`skip_taskbar(true)`, `focused(false)`, `shadow(false)` — one reusable window
resized per state, never recreated. Cross-OS: **Windows** re-asserts
`HWND_TOPMOST` with raw `SetWindowPos` on the main thread after every show
(`force_overlay_topmost`, `overlay.rs:200`) and works around tao's
`WM_DPICHANGED` reflow by re-applying placement; **macOS** converts the window
to an NSPanel via `tauri-nspanel` (`PanelLevel::Status`,
`nonactivating_panel`, `can_become_key_window: false`); **Linux** attaches GTK
Layer Shell (`Layer::Overlay`, `KeyboardMode::None`, anchors + margins instead
of coordinates, `set_size_request` + `resize(1,1)` sizing quirk) with the
`ZER0_NO_GTK_LAYER_SHELL` kill-switch, falling back to a plain always-on-top
window. Data: typed Tauri events (`StreamTextEvent`, `SpeechActivityEvent` —
both gated on cached atomics and suppressed while hidden), plus one raw-byte
polled IPC (`overlay_scope_frame`) for the analyser so per-frame data never
becomes JSON events. Text growth: the webview reports its height in 24 px
steps (`overlay_stream_text_height`), clamped to 70 % of the monitor, with the
geometry math defined once in Rust and mirrored in TypeScript. A generation
counter (`OVERLAY_SHOW_GENERATION`) makes stale hides harmless. Rendering is
HTML/CSS (typewriter reveal, CSS card animations) + 2D canvas for the
spectrum. **No click-through — deliberately**: the overlay carries a cancel
button and scrollable text.

### S2B2S

The same architecture, several steps further along. Notables beyond ZER0:
a user-selectable `OverlayMode` (Tauri vs OS-native, default native);
macOS frontmost-window detection via CoreGraphics `copy_window_info`;
`work_area()` from a Tauri git branch so the overlay tracks the Dock;
Windows accessibility text scaling read from
`HKCU\...\Accessibility\TextScaleFactor` (clamped 1.0–2.25, applied to size
but not to edge offsets); placement + topmost re-asserted _after_ `show()`
because tao's DPI reflow clobbers the first placement (unit-tested for
mixed-DPI and negative origins); emits suppressed while hidden to avoid
WebKit allocation accumulation (issue #1279); all geometry mutations forced
through `run_on_main_thread` because monitor queries on a background thread
corrupt the shared X11 connection (issue #227). The doc comment on
`overlay_fx/window.rs` claims click-through for the brain overlay, but no
click-through mechanism exists anywhere in the repo — and an `opacity`
settings field is parsed but never consumed.

### CopySpeak

The minimalist outlier. The HUD is **declared in `tauri.conf.json`** (no
`WebviewWindowBuilder` anywhere) at a fixed 300×140, and three tricks define
it: (1) **"hidden" means parked off-screen at (-10000,-10000)**, never
`hide()` — an explicit workaround for the WebView2 transparent-window
hidden→visible repaint bug and the white-frame flash at first paint (the
frontend shows the window itself, once, after the transparent CSS is applied);
(2) **permanently click-through** via `set_ignore_cursor_events(true)`,
re-applied on every show — no close button, so even the settings preview
self-destructs after 3 s; (3) **global events instead of window-targeted**
ones, with a 50 ms delayed payload emission after each reposition so the
payload never beats the window move. Zero per-OS code in the overlay path —
everything is portable Tauri API, which also means it relies on Tauri's
topmost being sufficient (it never re-asserts). No macOS/Linux-specific
handling at all; de-facto Windows-first.

### AIVORelay

The most engineered Windows path. Five overlay windows with a policy
distinction: the always-needed card is **pre-created hidden at startup**,
while rarely-used windows are **created lazily on first use** — the comment
says pre-creating the live preview "kept an otherwise idle renderer process
alive for the entire app session". Its key invention is
**`apply_recording_overlay_geometry_native`**: size + position + topmost in
**one** native `SetWindowPos` with `SWP_NOACTIVATE | SWP_NOOWNERZORDER`, so
the overlay "cannot briefly render with stale geometry", followed by a
spawned thread that re-asserts geometry after 50 ms and a double
apply→show→apply dance. It also uses **DWM cosmetics**
(`DWMWA_BORDER_COLOR = DWMWA_COLOR_NONE`, `DWMWCP_DONOTROUND`), reads the
Windows **work area** via `MonitorFromRect` + `GetMonitorInfoW` because
Tauri's own is insufficient, persists user-dragged positions in physical
pixels, reserves transparent padding around the visible frame so glow effects
never clip, keeps a 30 FPS throttle + enable-cache on mic-level IPC ("to
avoid unbounded WebKit event allocations"), and keeps a polled 120 ms state
mirror as a safety net behind its live-text events. The live-text window is
**Windows-only** (explicit stubs elsewhere), and there is no click-through —
overlays are interactive (drag grip, action buttons) and merely refuse to
steal keyboard focus. A patched tao fork fixes `WM_QUERYENDSESSION` vetoing
Windows shutdown.

### CursorFX

The genuinely different one. The overlay is a Tauri webview window like the
others — but the webview page is **empty and transparent**, and a dedicated
`gpu-render-thread` creates a **wgpu surface directly on the native window
handle** via `SurfaceTargetUnsafe::from_display_and_window` +
`create_surface_unsafe` (raw-window-handle), bypassing the webview for all
drawing. Transparency is a four-layer stack: Tauri `transparent(true)` +
transparent page + **surface alpha negotiation** (prefers
`CompositeAlphaMode::PreMultiplied`, falls back through `PostMultiplied`) +
**premultiplied pixels** (WGSL outputs `rgb * alpha, alpha`; blend
`One / OneMinusSrcAlpha`; every frame clears to `wgpu::Color::TRANSPARENT`).
Click-through is Tauri's portable `set_ignore_cursor_events(true)` — the
project's changelog explicitly says it _removed_ dangerous `GWLP_WNDPROC`
subclassing in favor of that API. The window covers the whole virtual desktop
(`GetSystemMetrics(SM_XVIRTUALSCREEN...)` on Windows, monitor union
elsewhere), with logical-size-at-build → physical-rect re-pinning on the
render thread (polling `inner_size` until the WM applies it), display bounds
re-polled every 1 s, `timeBeginPeriod(1)` + above-normal thread priority on
Windows, `Mailbox` present mode, `desired_maximum_frame_latency: 2`, and an
**idle park**: after the last input change it presents 3 "settle" frames so
every swapchain buffer holds the final image, then stops submitting GPU work
entirely and blocks on a condvar until input wakes it. Input arrives from a
`WH_MOUSE_LL` hook (with a UIPI `GetCursorPos` fallback while elevated windows
have foreground), not from window events — the overlay never needs input. A
standalone **winit daemon prototype** (not shipped) is where the raw win32
styles live: `WS_EX_TRANSPARENT | WS_EX_LAYERED | WS_EX_NOREDIRECTIONBITMAP`

- `HWND_TOPMOST`; DirectComposition ideas exist only in its research docs.
  Screen capture cannot see the GPU surface, so it renders PNGs offscreen.

---

## 2. Side-by-side

| Concern             | ZER0                                                                       | S2B2S                                                                                     | CopySpeak                                                       | AIVORelay                                                                                                       | CursorFX                                                                                          |
| ------------------- | -------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------- | --------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| Toolkit             | Tauri 2 webview                                                            | Tauri 2 webview                                                                           | Tauri 2 webview                                                 | Tauri 2 webview                                                                                                 | Tauri 2 shell + **wgpu surface on the native window**                                             |
| Window creation     | Programmatic, startup, hidden, reused                                      | Programmatic, startup, hidden, reused                                                     | **Declarative** in tauri.conf.json, parked off-screen           | Pre-created hidden (card) + lazy (others)                                                                       | Programmatic, startup, one per virtual desktop                                                    |
| Transparency        | Builder + transparent CSS                                                  | Builder + transparent CSS                                                                 | Builder + transparent CSS                                       | Builder + transparent CSS                                                                                       | Builder + transparent page + **swapchain alpha negotiation** + premultiplied WGSL                 |
| Always-on-top       | `always_on_top` + Win32 `HWND_TOPMOST` re-assert after show                | Same + re-assert after DPI reflow                                                         | Declarative only, never re-asserted                             | Builder + **one atomic native `SetWindowPos`** (move+size+topmost) + 50 ms re-assert thread                     | Builder only (per-virtual-desktop window is the topmost surface)                                  |
| Click-through       | **No** (interactive card)                                                  | No (despite a stale doc claim)                                                            | **Yes, permanent** (`set_ignore_cursor_events`)                 | No (interactive: drag, buttons)                                                                                 | **Yes, uniform whole-screen** (portable API; raw `WS_EX_*` only in the unshipped winit daemon)    |
| Focus policy        | `focused(false)`; macOS non-activating panel                               | `focusable(false)`; macOS non-activating panel                                            | Never focused; focus returned to main at startup                | `focused(false)`; macOS non-activating panel; some windows focusable by design                                  | `focused(false)`; input via OS-wide hook                                                          |
| Windows specifics   | `HWND_TOPMOST` re-assert; placement after show (DPI reflow)                | Same + TextScaleFactor accessibility scaling, work-area, CoreGraphics frontmost detection | None                                                            | **Atomic geometry SetWindowPos**, DWM border/corner attributes, `GetMonitorInfoW` work area, tao shutdown patch | Virtual-desktop metrics, refresh-rate probe, `timeBeginPeriod(1)`, thread priority, `WH_MOUSE_LL` |
| macOS specifics     | NSPanel (Status level, non-activating)                                     | NSPanel + CoreGraphics window info + Dock-tracking work area                              | None                                                            | NSPanel (Status, non-activating, all-Spaces)                                                                    | None (daemon declares objc2, unused)                                                              |
| Linux specifics     | **GTK layer shell** (anchors+margins, size-request quirk, env kill-switch) | GTK layer shell (same lineage)                                                            | None                                                            | **None** — plain Tauri path (live-text window Windows-only)                                                     | None (Vulkan backend only)                                                                        |
| "Hidden" strategy   | `hide()` / `show()` on a persistent window                                 | Same                                                                                      | **Parked off-screen forever** (WebView2 repaint-bug workaround) | `hide()`/`show()`; lazy windows destroyed                                                                       | Window always shown; GPU work parked instead                                                      |
| Text/data transport | Typed events + raw-byte polled IPC for analyser                            | Same lineage                                                                              | Global events + 50 ms delayed payload                           | Events + 120 ms polled state mirror + 30 FPS level throttle                                                     | Not text: physics → wgpu frames, condvar-idle                                                     |
| Positioning         | Cursor's monitor, top/bottom anchors, 24 px growth steps, cached geometry  | Same lineage                                                                              | 6 preset anchors, primary monitor only, physical px             | Cursor monitor + manual draggable positions persisted in physical px + glow padding                             | Full virtual desktop, 1 s layout re-poll                                                          |
| Rendering           | HTML/CSS + 2D canvas                                                       | HTML/CSS + 2D canvas                                                                      | HTML/CSS + 2D canvas (marquee synced to audio)                  | HTML/CSS (bars, themes, highlight marks)                                                                        | **wgpu WGSL**, premultiplied, SDF ribbon, Mailbox                                                 |
| Idle behavior       | Events gated while hidden                                                  | Same                                                                                      | Window parked off-screen but webview alive                      | Lazy windows destroyed when unused                                                                              | **GPU fully parked after 3 settle frames**                                                        |

---

## 3. The contrasts that matter

**1. There are only two honest ways to get a transparent overlay in a Tauri
app, and CursorFX shows the second one.** Four of the five are "transparent
webview + CSS": the transparency is the OS compositor seeing an
alpha-capable window whose webview paints nothing behind the card. CursorFX
keeps the alpha-capable window but moves the pixels to a wgpu swapchain
negotiated on the native handle (premultiplied alpha end-to-end, clear to
transparent). That is the only route to GPU-drawn overlays (real particle
systems, SDF ribbons) and the only one where the drawing cost can go to
zero when idle. Its price: screen-capture tools cannot see it, and you own
the whole render loop.

**2. "Hidden" is not one thing.** ZER0/S2B2S/AIVORelay hide the persistent
window; CopySpeak parks it off-screen because WebView2 mis-repaints
transparent windows on the hidden→visible transition and flashes a white
frame at first paint. CursorFX inverts the trade-off: the window never
changes visibility; the _work_ parks instead. If ZER0 ever sees the
WebView2 flash on Windows, the fix is already written down in
`copyspeak/src-tauri/src/hud.rs:159` — park, don't hide, and let the page
show itself after its transparent CSS is in place.

**3. Topmost is a verb, not a flag.** All three STT-lineage apps learned
that Tauri's `always_on_top` alone loses on Windows, and re-assert
`HWND_TOPMOST` natively. AIVORelay's version is the best-engineered: one
atomic `SetWindowPos` doing move+size+topmost with `SWP_NOACTIVATE |
SWP_NOOWNERZORDER`, eliminating the one-frame stale-geometry flash that the
two-step move/then-topmost sequence can show, plus a deferred re-assert for
the DPI reflow. ZER0 currently does topmost and placement as separate
operations with a post-show re-apply — AIVORelay's single-call form is a
direct upgrade.

**4. Click-through is a product decision, not a technique gap.** Only the
two display-only overlays (CopySpeak HUD, CursorFX) use
`set_ignore_cursor_events`; every STT card keeps interactive elements
(cancel, drag, scrolling) and only refuses _keyboard_ focus. If ZER0 ever
adds a minimal passive HUD mode, click-through is one portable call — no
`WS_EX_*` surgery needed (and CursorFX's changelog is the cautionary tale:
they removed WndProc subclassing in favor of the portable API).

**5. Linux is the differentiator.** ZER0 and S2B2S implement GTK layer shell
(anchors + margins, not coordinates — which sidesteps Wayland monitor
queries entirely, plus the `set_size_request` sizing quirk); AIVORelay does
not and ships its live-text window as Windows-only; CopySpeak ignores Linux.
ZER0's layer-shell work is already the best of the four — the env-var
kill-switch for broken compositors is the one piece worth keeping forever.

**6. IPC hygiene converges on the same rules everywhere:** throttle
high-rate emissions (33–150 ms), cache settings in atomics on the audio
callback path, suppress everything while hidden, and take high-rate binary
data out of JSON entirely (ZER0's `overlay_scope_frame` raw-byte polling is
the most advanced form of this in the family; AIVORelay settles for a 120 ms
polled mirror as a correctness net).

**7. macOS is a solved problem only where tauri-nspanel is used.** The
non-activating NSPanel at `PanelLevel::Status` with `can_join_all_spaces` +
`full_screen_auxiliary` is what makes the overlay behave over full-screen
apps and all Spaces; the PanelBuilder trick (create a Tauri window, convert
it, keep using `get_webview_window`) keeps the event plumbing intact. ZER0
already has all of this.

**8. Documentation rot is real.** S2B2S's `overlay_fx/window.rs` claims
click-through that does not exist; both S2B2S and CopySpeak carry parsed-but-
never-consumed `opacity` settings. Any new ZER0 overlay setting must be
wired in the same pass it is added.

---

## 4. What ZER0 should take from each

From **AIVORelay** (highest value, lowest risk):

1. Fold move+size+topmost into one native `SetWindowPos` with
   `SWP_NOACTIVATE | SWP_NOOWNERZORDER` for the Windows streaming overlay —
   kills the stale-geometry flash and simplifies the post-show re-apply.
2. `DwmSetWindowAttribute` border-color `NONE` + `DWMWCP_DONOTROUND` if we
   ever want square-cornered, borderless native frames.
3. The lazy-vs-precreated rule: pre-create only the window every recording
   needs; lazily create anything else so an idle renderer process never
   lingers.

From **S2B2S**: 4. Windows accessibility text scaling (`TextScaleFactor`, clamped, applied
to size but not to edge offsets) — an accessibility win ZER0 lacks. 5. The worked examples of main-thread discipline for monitor queries
(issue #227) — ZER0 already follows it; keep the rationale documented.

From **CopySpeak**: 6. The off-screen parking pattern as the ready-made fix for any WebView2
transparent repaint/white-flash symptom on Windows. 7. The "show once, from the frontend, after transparent CSS" startup order.

From **CursorFX** (the path to a GPU overlay): 8. If ZER0's overlay ever outgrows HTML (GPU waveform, particles), the
recipe is here: empty transparent page + wgpu surface via
`SurfaceTargetUnsafe` on the webview window's native handle +
premultiplied alpha negotiation + idle parking with settle frames. Note
the cost: invisible to screen capture, and we own frame pacing. 9. Its research-doc direction (DirectComposition /
`WS_EX_NOREDIRECTIONBITMAP`) remains unproven in any shipped code here —
treat as future work, not as a fix.

From **all four**: 10. Keep ZER0's overlay interactive and focus-free rather than
click-through; keep the layer-shell kill-switch; and wire every new
overlay setting the day it is added — the sibling repos are the
evidence of what dead settings cost.
