import type { OverlayScopeSettings, OverlayScopeStyle } from "@/bindings";

/**
 * `OverlayScopeSettings` with every field present and non-null, the way the
 * overlay window and the Overlay settings page work on it. Defaults and
 * limits mirror `OverlayScopeSettings` in `src-tauri/src/settings.rs`.
 */
export type ResolvedOverlayScope = {
  [K in keyof OverlayScopeSettings]-?: NonNullable<OverlayScopeSettings[K]>;
};

export const OVERLAY_SCOPE_DEFAULTS: ResolvedOverlayScope = {
  show_spectrum: true,
  show_wave: true,
  spectrum_style: "area",
  spectrum_mirror: true,
  peak_hold: false,
  wave_samples: 4096,
  wave_taper_samples: 512,
  wave_gain_floor: 0.02,
  view_width: 48,
  view_height: 22,
};

export const OVERLAY_SCOPE_LIMITS = {
  waveSamples: { min: 256, max: 16384 },
  waveGainFloor: { min: 0.001, max: 0.5 },
  viewWidth: { min: 32, max: 160 },
  viewHeight: { min: 14, max: 48 },
} as const;

export const OVERLAY_SCOPE_STYLES: OverlayScopeStyle[] = [
  "area",
  "line",
  "bars",
];

export function resolveOverlayScope(
  raw: OverlayScopeSettings | undefined | null,
): ResolvedOverlayScope {
  const out: Record<string, unknown> = { ...OVERLAY_SCOPE_DEFAULTS };
  if (raw) {
    for (const [key, value] of Object.entries(raw)) {
      if (value !== null && value !== undefined) out[key] = value;
    }
  }
  return out as ResolvedOverlayScope;
}

/** Gap between the two views and the block's trailing padding, in px. */
const VIEW_GAP = 6;
const BLOCK_PADDING = 8;

/**
 * Width of the scope block inside the pill: the views, the gap between them
 * and the trailing padding. Zero when no view is shown. Mirrors
 * `OverlayScopeSettings::block_width_px` in settings.rs, which sizes the
 * native window the same way.
 */
export function overlayScopeBlockWidth(cfg: ResolvedOverlayScope): number {
  const views = (cfg.show_spectrum ? 1 : 0) + (cfg.show_wave ? 1 : 0);
  if (views === 0) return 0;
  return views * cfg.view_width + VIEW_GAP * (views - 1) + BLOCK_PADDING;
}

/*
 * Pill geometry. The compact pill's widths are "the pill without its scope
 * block" plus the block: 126 = 172 (the original resting pill around the
 * old level bars) - 46 (those bars), likewise 138 for the Live pill and 198
 * for the pill carrying speech stats. The control row is 40 px tall unless
 * the views need more. overlay.rs derives the native window size from the
 * same numbers (OVERLAY_REST_BASE_W / OVERLAY_STATS_BASE_W + slack), so the
 * two must move together.
 */
const REST_BASE_W = 126;
const PILL_BASE_W = 138;
const STATS_BASE_W = 198;
const BASE_ROW_H = 40;
const ROW_PADDING_H = 18;

export function overlayRowHeight(cfg: ResolvedOverlayScope): number {
  return Math.max(BASE_ROW_H, cfg.view_height + ROW_PADDING_H);
}

/**
 * Publish the geometry as the CSS custom properties RecordingOverlay.css
 * reads, so the card and its animations follow the setting.
 */
export function applyOverlayScopeCss(cfg: ResolvedOverlayScope): void {
  const block = overlayScopeBlockWidth(cfg);
  const root = document.documentElement.style;
  root.setProperty("--ov-scope-w", `${cfg.view_width}px`);
  root.setProperty("--ov-scope-h", `${cfg.view_height}px`);
  root.setProperty("--ov-rest-w", `${REST_BASE_W + block}px`);
  root.setProperty("--ov-pill-w", `${PILL_BASE_W + block}px`);
  root.setProperty("--ov-stats-w", `${STATS_BASE_W + block}px`);
  root.setProperty("--ov-base-h", `${overlayRowHeight(cfg)}px`);
}
