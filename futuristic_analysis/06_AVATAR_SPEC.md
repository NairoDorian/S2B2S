# 06 — The 3D Avatar ("the face & the senses")

The face of S2B2S: a small, friendly, **3D cybernetic entity** that lives in the overlay and whose four senses map **1:1 to real pipeline signals**. The brief is explicit: *"it should be a 3D entity with rotating effects where we can see it has a back and sides … the brain should be 3D, and the ears and the eyes and the mouth … futuristic, cyberpunk, cybernetic."* This is the upgrade from the old plan's flat **"Orbi the 2D orb."**

**Crucially, this is almost all rendering.** No new model, no new ML. Three of the four senses are driven by signals that **already exist** in the codebase; only the **mouth** needs new code (one tiny audio tap), and only the **eyes** need the new vision pillar (`05`). The avatar is seeded by code already in the app: **`src/components/HerLoading.tsx`** (a real Three.js `WebGLRenderer({alpha:true})` "Her"‑style `CatmullRomCurve3` tube+ring animation) — proof that Three.js + transparency + the curve aesthetic **already work inside S2B2S today**.

---

## 1. Character brief (the face)

A translucent **holographic cybernetic head** — "head" loosely: a glassy faceted **visor‑shell** with a living **brain‑core** suspended inside it, plus discrete sense‑organs. It **rotates idly** so you see its back and sides (the 3D‑ness the brief asks for). Small and friendly — a desk‑buddy, not a HUD.

```
            ╭─────────────╮        ◀ slow idle rotation (showcase spin)
        (((  ◜ ◝  )))               ears  = side arcs / audio-grilles  → pulse to mic-level
            │ ◉◉ │                  eyes  = emissive pair / visor band → dark until "seeing"
           ╱│ ░░ │╲                 brain = lobed neural core inside    → spins + Her-curve orbit
          ╱ │▓▓▓▓│ ╲                 mouth = waveform aperture at base   → animates to tts:level
            ╰─◡◡◡─╯                  glass = fresnel rim + scanlines + holo-flicker (cyberpunk)
              │ │
           ╲╲ tether ╱╱  ───────────► to your cursor (the cord that ties you to S2B2S, §7)
```

| Element | Form | Bound to | Aesthetic |
| --- | --- | --- | --- |
| **Brain‑core** | a lobed / neural‑filament core (procedural mesh or GLTF) suspended in the shell | `brain:thinking`, `brain:token` | inner glow, filaments pulse per token |
| **Eyes** | an emissive pair (or a visor band) | vision (`05`) | **dark by default**; brighten + saccade when seeing |
| **Ears** | two side arcs / rings | `mic-level` | bloom outward with your voice; 16‑band ring EQ |
| **Mouth** | a waveform / iris aperture at the base of the visor | `tts:level` | opens to speech amplitude |
| **Shell** | faceted translucent glass visor | — | fresnel neon rim, refraction, chromatic scanlines, subtle holo‑flicker |

- **Aesthetic:** cyberpunk / cyberdelic — dark translucent body, a single **neon accent** (config), glass refraction, fresnel rim‑light, faint scanlines, gentle holographic flicker, selective **bloom** on the neon edges. Rendered **premultiplied‑alpha** against the transparent overlay.
- **Scale:** S/M/L = **56 / 72 / 96 logical px** (`§6`). Small enough to never block your work; it leads the bubble slightly and yields to the recording pill's band (`04 §6`).
- **Personality:** slow breathing rotation, occasional blink, a subtle **look‑toward your cursor**, a little settle after answering. **Friendly, never uncanny** — round, abstract forms; warm‑leaning neon; no humanoid face.

---

## 2. State machine (7 states)

This **is** the conversation state machine of `04 §2`, **projected onto the avatar's body.** Same machine, one source of truth — the avatar holds no logic; it renders `overlay:state` (`04 §3.2`).

```
                 converse hotkey / wake word
   ┌── IDLE ──────────────────────────────► LISTENING ───(VAD end)───► THINKING
   │    ▲   (visible rest / pinned)             │  ▲                       │
   │    │ Esc / auto-hide                       │  │ barge-in              │ brain:token…
   │    │                                       │  │                       ▼
   │    └────── DONE ◄──── SPEAKING ◄───────────┴──┴────────────────── (stream fills bubble)
   │                          ▲   (tts:level)
   │                          │
   │   SEEING ─(overlaps LISTENING/THINKING when a screenshot is attached, 05)─┐
   │     ▲ vision:capture                                                       │
   │     └───────────────────────────────────────────────────────────────────┘
   └── ERROR ◄── (brain:error) ──(2 s)──► DONE / IDLE
```

| State | Source signal | Brain‑core | Eyes | Ears | Mouth | Tether |
| --- | --- | --- | --- | --- | --- | --- |
| **IDLE** | none (rest / pinned) | slow breathe + showcase spin | half‑lidded | quiet | closed | calm, follows cursor |
| **LISTENING** | `mic-level` | gentle | open, attentive | **bloom to your voice** | closed | taut, tracks cursor |
| **THINKING** | `brain:thinking` / `brain:token` | **spin‑up + Her‑curve orbit**; filaments pulse per token | lit if image attached | settle | closed | frozen with bubble (`03 §3.4`) |
| **SEEING** | `vision:*` (`05`) | normal | **brighten + scanline sweep + saccade** | settle | closed | paused during region drag |
| **SPEAKING** | `tts:level` | calm glow | normal | settle | **waveform to TTS amplitude** | linked, frozen |
| **DONE** | `brain:done` | settle; show `42 t/s · 1.3 s` chip | normal | quiet | closes | relaxes |
| **ERROR** | `brain:error` | dim red flicker (~2 s) | normal | quiet | closed | retracts |

`HIDDEN` = the window isn't shown (no avatar, zero frames — `03 §7`). The avatar is the **single visual source of truth for "listening vs speaking,"** which is why the recording pill is suppressed while it's up (`04 §6`).

---

## 3. Senses ↔ signals map

The whole point: each sense is wired to a **concrete, verified** signal from the pipeline.

| Sense | Bound signal | Origin (verified in source) | Visual |
| --- | --- | --- | --- |
| 👂 **Ears** | `mic-level` | **already emitted** to the recording overlay via `overlay::emit_levels`; we **extend the fan‑out one line** to the `brain_overlay` label (`04 §3.1`) | ear arcs bloom; 16‑band ring EQ |
| 🧠 **Brain** | `brain:thinking`, `brain:token` | `brain/manager.rs` — `app.emit` (app‑global, any window can listen) | core spin‑up + `CatmullRomCurve3` orbit; per‑token filament pulse |
| 👁 **Eyes** | `vision:capture-started` / `vision:attached` | the **new** `vision` module (`05`) | brighten + scanline sweep + saccade |
| 👄 **Mouth** | `tts:level` | the **one new piece of audio code** — an RMS tap in `tts/player.rs` (`§5`) | waveform / aperture to amplitude |

Plus the metric signals already on the bus feed micro‑status, not motion: `brain:latency` → "⚡280 ms", `brain:done` → "42 t/s · 1.3 s", `brain:llama-loading` → "warming up GPU…" (same data the Conversation tab shows). **Three of four senses cost zero backend work** — they're already broadcasting.

---

## 4. Rendering pipeline

**Primary: Three.js in the Track A webview (R2 → R3 fallback, `03 §1`).** Three.js is **already a dependency** (`three ^0.184.0`), and that version ships **`WebGPURenderer` + TSL** (node materials). This is the fast path to the face you want, and it reuses `HerLoading`.

| Layer | Choice | Why |
| --- | --- | --- |
| Renderer | `WebGPURenderer` where available (**WebView2 on Windows = your #1 target**), **automatic WebGL2 fallback** elsewhere (macOS/Linux WebKit) | true WebGPU on the priority OS; never breaks (`03 §8` rung 2) |
| Seed | extend `HerLoading.tsx`'s setup: `alpha:true`, transparent clear, `CatmullRomCurve3` | transparency + the curve aesthetic are **already proven in‑app** (`§8`) |
| Materials | **TSL node materials** (run on both WebGPU & WebGL2 backends in three ^0.184): fresnel rim, glass/transmission, scanline, holo‑flicker; `MeshPhysicalMaterial` transmission as the lite fallback | one material graph, two backends |
| Post | **selective bloom** on the neon edges (off in `lite`/reduced‑motion) | the cyberpunk glow |
| Geometry | low‑poly brain‑core (procedural lobes or a small GLTF), glass visor shell, ear arcs, emissive eye/mouth planes | desk‑buddy size — keep it cheap |
| Transparency | renderer alpha, **premultiplied**, clear‑alpha 0 | matches the transparent overlay webview (`overlay.rs` + `HerLoading`) |

**Optional Track B (all‑native, R1 — `03 §1.3`, endgame only):** a **WGSL raymarched SDF** head + `glyphon` text. More work for the 3D character + 20‑locale text, so it's not the starting point — but because the avatar is a **dumb renderer driven only by events** (`03 §1.4`), R2→R1 is a swap, not a rewrite.

---

## 5. Audio reactivity (the ears & the mouth)

Two amplitude sources, **one shared visual language** (a peak‑follower with fast attack / slow release, used by both ears and mouth so the face reads coherently).

### 5.1 Ears ← `mic-level` (already exists)
`mic-level` is already produced and fanned to the recording overlay (`overlay::emit_levels`). We add **one line** to also fan it to `brain_overlay` (`04 §3.1`). The ears bloom to it; for the **16‑band ring EQ** look, derive a small client‑side band split if `mic-level` is a scalar (or consume its bands directly if it already carries a spectrum). Envelope‑smoothed to avoid jitter.

### 5.2 Mouth ← `tts:level` (the only new audio code in the whole plan)
`tts/player.rs` today is a rodio audio thread: **`Player → Sink → Append(Vec<u8>)`** of a decoded source. It exposes **no realtime amplitude** (verified gap). Add a thin, non‑invasive tap:

```
  decoded audio ──► [ RmsTap(Source) ] ──► Sink ──► output device
                          │  accumulate RMS over ~33 ms (≈30 Hz)
                          └──► app.emit("tts:level", f32)        // app-global, like brain:*
                               + a final emit(0.0) on stop/drain // mouth closes cleanly
```

- A **rodio `Source` wrapper** that passes samples straight through while accumulating RMS over a short window (~33 ms → ~30 Hz), emitting `tts:level(f32)` via `app.emit` (same broadcast pattern as `brain:*`). A trailing `0.0` on stop so the mouth shuts.
- **Non‑invasive:** wraps the existing source; does **not** touch decoding, the `Sink`, volume, or **barge‑in**. ~30–40 lines, isolated to `tts/player.rs`.
- The mouth maps amplitude → aperture/waveform height with the **same envelope** as the ears.

> This single tap is what gives the avatar a **mouth**. Everything else it needs to "speak" (the streamed text, the sentence splitter, the metrics) is already on the bus.

---

## 6. `AvatarConfig` & skins (additive, serde‑defaulted)

```rust
pub struct AvatarConfig {
    pub style: String,            // "cyber" (default) | "orb" (minimal, a nod to old "Orbi") | "glyph" | <skin id>
    pub size: String,             // "S" | "M" | "L"  →  56 | 72 | 96 logical px
    pub accent: String,           // hex neon accent — drives fresnel/scanline + bubble tint (04 §4)
    pub show_eyes: bool,          // default true; defers to vision.enabled (05) — no eyes, no eye-glow ever
    pub show_tether: bool,        // cursor→avatar trail (§7); defers to OverlayModeConfig.show_trail (04 §7)
    pub face_toward_cursor: bool, // subtle look-at; default true
    pub idle_rotation: bool,      // slow showcase spin; default true
    pub reduced_motion: bool,     // also honors OS prefers-reduced-motion
    pub quality: String,          // "auto" | "high" | "lite"  (bloom/particles/tether on/off)
}
```

- **Relationships:** `accent` tints the reply bubble (`04 §4`); `show_eyes` pairs with `VisionConfig.enabled` (`05`) — keeping the **privacy invariant legible** (`05 §9`); `show_tether` is the avatar‑anchored segment of the overlay trail, so it **defers to** `OverlayModeConfig.show_trail` (`04 §7`). Lives **inside** `OverlayModeConfig` (`04 §7`).
- **Live preview:** Settings → **Overlay Mode** shows a small canvas cycling the **7 states** so you can tune `accent`/`size`/`quality` and watch the senses react.

**Skins = declarative JSON param packs (no code).** A skin names a geometry preset + material params (accent, rim power, glass IOR, scanline density, flicker, bloom strength) + motion params (idle rpm, look‑at gain, attack/release) + an optional GLTF URL. Ship a few built‑ins (`cyber` default, `orb` minimal, `glyph` wireframe); power users drop a JSON in a skins dir. Versioned schema. The **engine stays fixed; the look is data** — the same serde‑default philosophy as `settings.rs`.

---

## 7. The cursor → avatar tether (TD_Web_Trail physics)

The cord that ties **you** (cursor) to **S2B2S** (avatar) — and the literal payoff of vendoring your two trail projects.

| Aspect | Recipe (lifted) |
| --- | --- |
| **Physics** | `TD_Web_Trail`'s **spring‑friction chain** from cursor → avatar anchor; **distance‑constraint solver** holds segment lengths; head node chases the cursor with **critical damping** (the "lazy brush"); **idle‑sleep after 2 still frames** → zero cost at rest |
| **Geometry** | a **Catmull‑Rom** spline through the chain nodes → tessellated ribbon (the shared curve language, `§8`) |
| **Look** | the **4‑pass tapered neon glow** from `TD_Web_Trail` (downscaled bloom + body + dark mask + bright core), taper `(1−p)^1.5` so it **tapers into the avatar**; accent‑tinted; optional HSL drift (`CursorFX hsl_to_rgba()`) |
| **Renders in** | Track A / single‑webview → a WebGPU/WebGL canvas **behind** the avatar+bubble (port the JS physics); Track B → the **CursorFX ribbon pipeline** on the native overlay (`03 §6`), the tether being a trail whose far endpoint is the avatar window |

**Behavior:** when the bubble **freezes for reading** (`03 §3.4`), the tether **still links cursor ↔ avatar** so the bond persists without the bubble chasing the mouse. On dismiss, the trail **retracts into the avatar**, then fades. Governed by `show_tether` / `show_trail`; off entirely under reduced‑motion (`§9`).

---

## 8. The "Her" lineage = brand continuity

`HerLoading.tsx` is **already shipping** in S2B2S: a Three.js `WebGLRenderer({alpha:true})` **"Her"‑style `CatmullRomCurve3` tube + ring** loading animation. The avatar is **HerLoading grown up**:

- The loading **ring/tube becomes the brain‑core's orbiting Her‑curve** — the THINKING animation literally **reuses the `CatmullRomCurve3` motif** (`§4`, `§2`).
- **Morph on warm‑up:** on first launch / model load, the existing `HerLoading` **morphs into the idle avatar** (the ring contracts into the orbit, the tube coalesces into the core) — one continuous visual language from *loading* → *living avatar*.
- **One curve language across all three repos:** `HerLoading` `CatmullRomCurve3`, `TD_Web_Trail` Catmull‑Rom/Bézier, `CursorFX` `catmull_rom()` → the avatar, its thinking orbit, **and** its tether all speak **Catmull‑Rom**. That through‑line is what makes the whole thing feel **designed, not assembled** — and it's why these three projects belong together.

---

## 9. Reduced motion, accessibility & performance

- **`prefers-reduced-motion` (OS) or `reduced_motion` (config)** → no idle spin, no particles, no tether motion (or a single slow gentle loop); **state changes via color/opacity + micro‑status text, not motion**; mouth → a simple level bar; ears → a simple level. (Honors `03 §7` battery rule + `04` `OverlayModeConfig.reduced_motion`.)
- **`quality = "lite"`** (auto‑selected on low‑power / integrated GPU / on battery) → drop bloom/post, fewer core polys, **no tether**, WebGL2 path. Pairs with `03 §7`.
- **Within the `03 §7` budget:** ≤24 fps idle breathing, ≤60 fps while speaking; **audio‑level updates coalesced per `rAF`**; small, low‑poly, single canvas; **hidden → zero frames.**
- **Color‑independent:** state is also carried by **shape/motion/micro‑status**, never hue alone (colorblind‑safe).
- **Never creepy:** abstract, round, friendly motion; no uncanny humanoid face — a companion, not a person.

---

## 10. Acceptance criteria (avatar)

1. Renders with **true transparency** over arbitrary apps on Windows (WebView2 / `WebGPURenderer`), with **automatic WebGL2 fallback** on macOS/Linux webviews.
2. All **7 states** are visually distinct and driven **only by events** (no avatar‑side logic) — provable by replaying a canned event sequence with **no backend** (`03 §1.4`).
3. **Ears** react to `mic-level` and **mouth** reacts to `tts:level` in real time; with audio muted the mouth rests closed; the `tts:level` tap adds **no audible artifact** and does not alter playback or barge‑in.
4. The THINKING orbit is the **`HerLoading` `CatmullRomCurve3` motif** (shared code path), and `HerLoading` **morphs into** the idle avatar on warm‑up.
5. The **tether** tracks the cursor with `TD_Web_Trail` spring physics and **idle‑sleeps after 2 still frames** (zero cost at rest).
6. With `reduced_motion` → no spin/particles/tether motion; with `quality = "lite"` → no bloom, WebGL2; both stay within the `03 §7` budget.
7. `show_eyes = false` (or vision disabled) → **no eye‑glow ever** (the `05 §9` privacy invariant holds visually).
8. Swapping a **skin JSON** changes the look with **no code**; switching renderer R2→R3 needs **no orchestration change** (the dumb‑renderer contract, `03 §1.4`).

The avatar is the same brain, the same voice, given a **face, four senses, and a body** — the Clippy 2.0 the brief asks for, built almost entirely from pixels and signals you already have.
