# 04 — The S2B2S Avatar: Giving the App a Face

S2B2S already has a soul — the *Her*-style loading animation (`HerLoading.tsx`, the Catmull-Rom tube curve and ring). The avatar grows that DNA into a persistent, friendly character that **is** the visible agent of Conversation Mode 2.0.

---

## 1. Character brief — "Orbi" (working name; final name = community vote)

- **Form:** a soft, luminous **orb** — a blob of light with subtle surface wobble, two simple oval eyes, and a *waveform* for a mouth. No limbs, no face beyond eyes+wave: keeps it genderless, culture-neutral, cheap to render, impossible to hit the uncanny valley.
- **Personality through motion, not features:** curiosity = slight lean toward the cursor; attention = eyes track the bubble text as it streams; effort = the *Her* curve orbits while thinking. Calm by default — breathing at rest, never bouncing for attention.
- **The Her lineage is the brand:** the thinking animation literally reuses the `HerLoading` curve math (ported to a shader), shrunk into an orbital around the orb. Loading screen and avatar become one visual language.
- **Size presets:** 56 / 72 / 96 logical px (S/M/L), DPI-crisp because it's procedural.

```
      idle            listening           thinking            speaking            error
                      ∿ ripples            ──╮ her-curve        ▂▅▇▅▂ mouth =
     ( ◠ ◠ )         (( ◠ ◠ ))           ( ◉ ◉ )╭──           ( ◠ ◠ )            ( ◡̦ ◡̦ )
      ‿                 ‿﹏‿               ╰──orbit             ▂▃▅▃▂              ⌒  (dim, amber)
   slow breath      reacts to YOUR       particles swirl    pulses with TTS     brief, apologetic
   2 % scale sine   mic-level bars       (CursorFX particles)  output level
```

---

## 2. State machine (visual layer of 03 §2)

| State | Trigger signal | Eyes | Mouth/wave | Body | Glow color* |
| --- | --- | --- | --- | --- | --- |
| Idle | overlay visible, no activity | slow blink (every 4–7 s) | flat line | breathing scale ±2 % | neutral accent |
| Listening | `mic-level` stream active | wide, fixed on user | none — **rim ripples** driven by the same smoothed levels the pill uses | leans 4° toward cursor | green (matches footer STT dot) |
| Thinking | `brain:thinking` | look up-left | none | Her-curve orbit + particles | orange pulse (matches Brain loading dot) |
| Speaking | `tts:level` > ε | relaxed, occasional blink | **waveform mouth** = tts amplitude envelope | gentle sway | accent / green |
| Done (silent) | `brain:done`, no TTS | soft "smile" ease | settles | settle bounce (180 ms, ≤4 px) | green flash 300 ms |
| Error | `brain:error` | droop | flat | brief shrink | amber, never red-alarm |
| Dismiss | hide | — | — | scale-out + fade 200 ms | — |

\* Colors bind to the existing footer status-dot palette and both app themes; user-themable accent in `AvatarConfig`.

Transitions: every state change tweens ≤ 250 ms with ease-out; `prefers-reduced-motion` / `reduced_motion` setting ⇒ crossfade only, no particles, no breathing.

---

## 3. Implementation options compared

| Option | Pros | Cons | Verdict |
| --- | --- | --- | --- |
| **A. Procedural (SDF shader / Canvas-WebGL)** | tiny (~5 KB logic), infinitely themable, resolution-independent, identical math reusable in Track A (GLSL/Three) and Track B (WGSL), audio-reactive trivially, no asset pipeline, no licenses | needs a few days of shader craft | ✅ **Chosen for v1** |
| B. Rive (`.riv` state machine) | designer-friendly, rich character acting, built-in state machines | new runtime dep (per window), asset pipeline + design skill, harder to make audio-reactive per-sample, WGSL port impossible (locks Track B to webview) | 🔁 Optional "Character skin" later |
| C. Lottie | huge ecosystem | playback-only (poor reactivity), perf on big comps | ❌ |
| D. Sprite sheets | simplest | not reactive, blurry on DPI mix, big | ❌ |
| E. 3D model (glTF) | wow factor | overkill, perf, uncanny risk | ❌ |

**Architecture for A:** one pure function `avatarFrame(state, t, micLevels[16], ttsLevel, params) → drawcalls`. In Track A it renders to a Three.js fullscreen-quad shader (we already ship Three) or 2D canvas fallback; in Track B the same math is a WGSL fragment shader (SDF circle + ripple displacement + eye/wave SDFs + Her-curve polyline pass + simple particle buffer from CursorFX). Skins = parameter presets (`classic`, `minimal-dot`, `flame`, `mono`), enabling community skins as JSON param packs — no code.

---

## 4. Audio reactivity

- **Listening:** consume the existing `mic-level` event (16 smoothed bands, already produced by `AudioToolkit` visualizer + `overlay::emit_levels`; the recording pill's 0.7/0.3 smoothing is reused). Bands map to radial rim-ripple amplitudes — the avatar visibly "hears" you, which doubles as live mic-works feedback (a real support pain-killer).
- **Speaking:** requires the one genuinely new backend primitive: **`tts:level`**.

## 5. The `tts:level` tap (only new audio code in the whole plan)

In `tts/player.rs` (rodio): wrap the playing source with a lightweight RMS-metering adapter (`Source` wrapper computing a running RMS over ~33 ms windows), pushing `app.emit("tts:level", rms)` at ≤ 30 Hz, plus a final `tts:level 0.0` on stop/abort. Cost: O(samples) adds, one atomic, no allocation — negligible next to synthesis. Also benefits 1.0: the Conversation tab can show a speaking indicator, and the pill's `speaking` state can pulse too. (Phase 1 interim fallback if needed: synthesize a fake envelope from `brain:sentence` timing — acceptable for a week, not for ship.)

Optional later: 2–3 band split (rustfft is already a dep) for slightly "phonetic" mouth movement — explicitly **not** real visemes; the waveform-mouth design makes accurate lip-sync unnecessary by construction.

---

## 6. `AvatarConfig` (inside `OverlayModeConfig`)

```rust
pub struct AvatarConfig {
    pub style: String,        // "classic" | "minimal-dot" | "mono" | skin id
    pub size: SizeS_M_L,      // 56 | 72 | 96 px
    pub accent: Option<String>, // hex override; None = theme accent
    pub eyes: bool,           // some users will want a pure orb
    pub reduced_motion: bool, // also auto-honors OS preference
    pub show_in_tray_flair: bool, // mini state in tray icon (Phase 4, optional)
}
```

Settings UI ships with a live preview canvas cycling the five states.

---

## 7. Beyond the overlay (cheap brand wins, later phases)

- Onboarding: the avatar replaces the static logo on first-run and *introduces itself* (TTS speaks the welcome — eating our own dog food).
- `HerLoading` end-state morphs into the avatar instead of fading out — one continuous identity from launch.
- README/landing hero, tray icon micro-states, error dialogs fronted by the Error pose.
- Naming: keep "Orbi" as placeholder; run a community issue to name him at first public beta.

---

## 8. Acceptance criteria (avatar)

1. All five states reachable and visually distinct in a scripted demo (`overlay preview` dev command).
2. Listening ripples track real mic input within one smoothing window; Speaking mouth tracks `tts:level` with < 50 ms perceived lag.
3. Identical look Windows/macOS/Linux-X11 at 100 %/150 %/200 % DPI (screenshot diff harness).
4. Reduced-motion mode: zero continuous animation; states still distinguishable by static pose + color.
5. GPU/CPU within the budget of 02 §7 on an iGPU laptop on battery.
