---
name: ZER0
description: Precision real-time speech interface and tactile acoustic terminal
colors:
  dark-background: "#0b0e11"
  dark-text: "#e6f1f3"
  dark-accent: "#1fe0ff"
  light-background: "#f2f4f5"
  light-text: "#101418"
  light-accent: "#0c8fa3"
  background-ui: "#0e94a8"
  mid-gray: "#7c8a90"
  warning-dark: "#fbbf24"
  error-dark: "#f87171"
  hairline: "rgba(230, 241, 243, 0.14)"
typography:
  display:
    fontFamily: "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif"
    fontSize: "1.75rem"
    fontWeight: 600
    lineHeight: 1.2
    letterSpacing: "-0.01em"
  headline:
    fontFamily: "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif"
    fontSize: "1.25rem"
    fontWeight: 600
    lineHeight: 1.3
  title:
    fontFamily: "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif"
    fontSize: "0.9375rem"
    fontWeight: 500
    lineHeight: 1.4
  body:
    fontFamily: "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif"
    fontSize: "0.9375rem"
    fontWeight: 400
    lineHeight: 1.5
  label:
    fontFamily: "'Cascadia Mono', 'JetBrains Mono', 'Fira Code', Consolas, monospace"
    fontSize: "0.75rem"
    fontWeight: 500
    lineHeight: 1.4
    letterSpacing: "0.05em"
rounded:
  default: "0px"
  sm: "0px"
  md: "0px"
  lg: "0px"
  pill: "9999px"
spacing:
  xs: "4px"
  sm: "8px"
  md: "12px"
  lg: "16px"
  xl: "24px"
components:
  button-primary:
    backgroundColor: "{colors.background-ui}"
    textColor: "#ffffff"
    rounded: "{rounded.default}"
    padding: "5px 16px"
  button-secondary:
    backgroundColor: "rgba(124, 138, 144, 0.1)"
    textColor: "{colors.dark-text}"
    rounded: "{rounded.default}"
    padding: "5px 16px"
---

# Design System: ZER0

## Overview

### Creative North Star: The Precision Terminal

ZER0 is an acoustic instrument and real-time transcription engine built with the aesthetic discipline of high-end laboratory equipment and retro-futuristic terminal interfaces.

It rejects contemporary decorative SaaS fluff—bubbly cards, oversized corner radii, diffuse box shadows, and gratuitous gradient text. Instead, it commits to:

- **Pure Dark Void**: High-contrast, near-black backgrounds (`#0b0e11`) paired with cold off-white text (`#e6f1f3`).
- **Laser-Sharp Edges**: Zero corner radii on containers, modals, inputs, and cards.
- **Electric Cyan Telemetry**: Live spectral feedback (FFT audio visualizers, peak meters, circular sweeps) rendered in electric cyan (`#1fe0ff`).
- **Tactile & Restrained Density**: Clear structural hair-lines (1px crisp borders) separating dense information layers without visual noise.
- **Multi-Layered Presence**: Deep configuration tools when tuning hardware models; an ultra-receded, minimal "épuré" dark window displaying only white transcription and circular audio geometry during dictation.

---

## Colors

The application operates on an explicit dual-palette system (Dark / Light) driven by `data-theme` on `<html>`. The default and primary identity is Dark.

### Primary Palette (Dark - Default)

- **Background (`--dark-color-background`)**: `#0b0e11` — Deep obsidian base.
- **Text / Foreground (`--dark-color-text`)**: `#e6f1f3` — Crisp, cool off-white.
- **Accent (`--dark-color-accent`)**: `#1fe0ff` — High-energy electric cyan for active state indicators, telemetry, and live frequency bins.
- **UI Surface (`--color-background-ui`)**: `#0e94a8` — Deep cyan teal for primary action buttons.
- **Mid Gray (`--color-mid-gray`)**: `#7c8a90` — Secondary descriptive copy, muted icons, and border bases.
- **Hairline Border (`--color-hairline`)**: `color-mix(in srgb, var(--color-text) 14%, transparent)` — Subtle 1px structural dividing lines.
- **Log / Scope Surface (`--color-log-surface`)**: `color-mix(in srgb, var(--color-background), black 6%)` — Receded terminal console backgrounds.

### Light Mode Complement

- **Background (`--light-color-background`)**: `#f2f4f5` — Off-white technical paper ground.
- **Text (`--light-color-text`)**: `#101418` — Stark, dark ink.
- **Accent (`--light-color-accent`)**: `#0c8fa3` — Deep precision teal.

### Semantic Alerts

- **Warning**: Dark `#fbbf24` | Light `#d97706` — VAD boundary triggers, security warnings.
- **Error / Danger**: Dark `#f87171` | Light `#dc2626` — Hardware capture faults, model load failures.

---

## Typography

Typographic hierarchy enforces the separation between natural human speech and machine telemetry:

1. **Interface & Prose (`font-sans`)**:
   - Stack: `-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif`
   - Weight: Regular (`400`) for body/settings labels; Medium (`500`) for section titles; Semibold (`600`) for top-level headers.
   - Size Scale:
     - `15px` (`--font-size` base) with `24px` line height.
     - Section Headings: `14px` (uppercase, letter-spaced).
     - Body Text: `14px` / `15px`.
     - Descriptions: `12px` / `13px` in `--color-mid-gray`.

2. **Machine Telemetry & Tokens (`font-mono`)**:
   - Stack: `"Cascadia Mono", "JetBrains Mono", "Fira Code", Consolas, "SF Mono", Menlo, monospace`
   - Role: Numerical metrics (FFT dB bins, latency ms, memory usage %, VAD confidence, shortcut chips).
   - Character: High legibility, tabular numbers, uppercase tags.

---

## Layout

1. **Density**: Compact and information-rich without clutter. Settings pages run on structured grids with clear vertical flow.
2. **Settings Groups**: Structured into modular `<SettingsGroup>` boxes framed by 1px hairline borders.
3. **Live Surface (Épuré Mode)**: Ultra-minimal canvas. All framing chrome, menus, and sidebars hide completely, leaving only:
   - Centered live transcription text in high-contrast white.
   - Real-time circular FFT audio visualizer and waveform scope below or adjacent.
4. **Recording Overlay**: Floating desktop HUD anchored to screen edges, constrained to tight dimensions (`--ov-rest-w: 236px`), housing twin 48px scope canvases and recording state indicator.

---

## Elevation & Depth

- **Strictly Flat & Layered**: ZER0 does not use blurred drop shadows (`box-shadow: 0 4px 20px ...` is prohibited).
- **Hairline Separation**: All elevation tokens resolve to a 1px boundary:
  ```css
  --shadow-sm: 0 0 0 1px var(--color-hairline);
  --shadow-md: 0 0 0 1px var(--color-hairline);
  ```
- **Tonal Layering**: Depth is achieved solely through subtle surface luminance shifts:
  - Base window ground: `#0b0e11`
  - Group container fill: `mid-gray/10` or `color-mix(in srgb, var(--color-background), black 6%)`
  - Active interactive hover: `background-ui/20`

---

## Shapes

- **Absolute Sharp Corners (`0px`)**:
  - Tailwind v4 radius tokens are globally overridden to `0`:
    `--radius-xs: 0; --radius-sm: 0; --radius-md: 0; --radius-lg: 0; --radius-xl: 0;`
  - `.rounded-full` is overridden to `border-radius: 0`.
  - Cards, modals, buttons, text inputs, dropdowns, and progress bars **must remain razor-sharp rectangles**.
- **The Sole Exception**:
  - Toggle switches and push-to-talk pill indicators use `@utility rounded-pill { border-radius: 9999px; }`. This exception is permitted only because toggle tracks and knobs require pill geometry to be immediately recognizable as switches.

---

## Components

1. **Buttons**:
   - **Primary**: Solid teal `bg-background-ui`, white text, sharp edges, 1px border. Hover shifts brightness subtly.
   - **Primary-Soft**: Cyan tint `bg-accent/20`, border-transparent, text `var(--color-text)`.
   - **Secondary / Ghost**: Hairline border `border-mid-gray/20`, background transparent or `mid-gray/10`.
   - Padding: `px-4 py-[5px]` (medium), compact line-height.
2. **Inputs & Textareas**:
   - Background: Dark tonal fill `bg-background` or `bg-mid-gray/10`.
   - Border: 1px hairline `border-mid-gray/20`.
   - Focus: 1px crisp ring in electric cyan (`focus:ring-1 focus:ring-accent`), no blur.
3. **Meters & Progress Bars**:
   - Normalized `0…100%` range enforced by `clampPercent`.
   - Track: Flat dark channel with 1px border.
   - Fill: Electric cyan (`#1fe0ff`) or dynamic gradient without rounded caps.
4. **Scopes & Visualizers**:
   - Circular FFT scope, waveform scope, and bar spectrum.
   - Direct HTML5 Canvas 2D/WebGL rendering with zero layout thrash.

---

## Do's and Don'ts

### Do:

- **DO** keep every new card, dialog, dropdown, and button at `border-radius: 0`.
- **DO** use `font-mono` for all numeric metrics, timestamps, percentages, shortcut hotkeys, and audio technical values.
- **DO** use 1px hairlines (`var(--color-hairline)`) for visual boundaries instead of drop shadows.
- **DO** maintain high contrast between text (`#e6f1f3`) and backgrounds (`#0b0e11`).
- **DO** use the single source of truth for app branding via `appIdentity` rather than hardcoding names.

### Don't:

- **DON'T** introduce soft drop shadows, blur glares, or floating pill containers (outside toggle switches).
- **DON'T** use rounded buttons (`rounded-lg`, `rounded-full`) that bypass the 0-radius theme.
- **DON'T** use colorful saturated rainbows or pastel palettes; stick to obsidian ground + electric cyan telemetry.
- **DON'T** clutter the live transcription canvas with unnecessary chrome when in focused dictation mode.
