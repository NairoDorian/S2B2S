import type { FftLoudnessMode } from "@/bindings";
import { parseHex } from "@/lib/utils/color";

export const NOTE_NAMES = [
  "C",
  "C#",
  "D",
  "D#",
  "E",
  "F",
  "F#",
  "G",
  "G#",
  "A",
  "A#",
  "B",
];

/** "20.0 Hz", "1.20 kHz", "12.3 kHz". */
export function formatHz(hz: number): string {
  if (!Number.isFinite(hz)) return "—";
  if (hz >= 10_000) return `${(hz / 1000).toFixed(1)} kHz`;
  if (hz >= 1000) return `${(hz / 1000).toFixed(2)} kHz`;
  if (hz >= 100) return `${hz.toFixed(0)} Hz`;
  return `${hz.toFixed(1)} Hz`;
}

/** Axis label: "20", "500", "1k", "20k". */
export function shortHz(hz: number): string {
  return hz >= 1000 ? `${hz / 1000}k` : `${hz}`;
}

/** Nearest equal-tempered note with its cents offset, e.g. "A4 +12¢". */
export function noteName(hz: number): string | null {
  if (!(hz > 0) || !Number.isFinite(hz)) return null;
  const midi = 69 + 12 * Math.log2(hz / 440);
  const rounded = Math.round(midi);
  const cents = Math.round((midi - rounded) * 100);
  const name = NOTE_NAMES[((rounded % 12) + 12) % 12];
  const octave = Math.floor(rounded / 12) - 1;
  const sign = cents >= 0 ? "+" : "−";
  return `${name}${octave} ${sign}${Math.abs(cents)}¢`;
}

const TICK_HZ = [
  10, 20, 30, 50, 100, 200, 300, 500, 1000, 2000, 3000, 5000, 10000, 20000,
  40000, 80000,
];

/**
 * Position of `hz` across an axis given as the frequency of every bin
 * (monotonic, as the warp tables produce it), 0…1, or null when outside.
 */
export function axisPosition(
  axis: ArrayLike<number>,
  hz: number,
): number | null {
  const n = axis.length;
  if (n < 2) return null;
  if (hz < axis[0] || hz > axis[n - 1]) return null;
  let lo = 0;
  let hi = n - 1;
  while (hi - lo > 1) {
    const mid = (lo + hi) >> 1;
    if (axis[mid] <= hz) lo = mid;
    else hi = mid;
  }
  const a = axis[lo];
  const b = axis[hi];
  const frac = b > a ? (hz - a) / (b - a) : 0;
  return (lo + frac) / (n - 1);
}

export interface AxisTick {
  x: number;
  hz: number;
  label: string;
}

/** Frequency ticks that fit the axis without crowding. */
export function frequencyTicks(
  axis: ArrayLike<number>,
  minSpacing = 0.055,
): AxisTick[] {
  const ticks: AxisTick[] = [];
  let lastX = -1;
  for (const hz of TICK_HZ) {
    const x = axisPosition(axis, hz);
    if (x === null || x - lastX < minSpacing) continue;
    ticks.push({ x, hz, label: shortHz(hz) });
    lastX = x;
  }
  return ticks;
}

export const clamp01 = (v: number): number => (v < 0 ? 0 : v > 1 ? 1 : v);

/** How bin values map onto the 0…1 drawing range. */
export interface ValueScale {
  mode: FftLoudnessMode;
  /** dB below the reference shown as the floor (dB modes). */
  dbRange: number;
  /** Auto-ranged ceiling of the linear mode. */
  ceiling: number;
}

export function valueToUnit(v: number, s: ValueScale): number {
  switch (s.mode) {
    case "db":
      return clamp01((v + s.dbRange) / s.dbRange);
    case "db_normalized":
      return clamp01(v);
    default:
      return s.ceiling > 0 ? clamp01(v / s.ceiling) : 0;
  }
}

export interface ValueTick {
  y: number;
  label: string;
}

export function valueTicks(s: ValueScale): ValueTick[] {
  if (s.mode === "db") {
    const step = s.dbRange > 100 ? 20 : 10;
    const ticks: ValueTick[] = [];
    for (let db = 0; db >= -s.dbRange + 1e-6; db -= step) {
      ticks.push({ y: (db + s.dbRange) / s.dbRange, label: `${db}` });
    }
    return ticks;
  }
  if (s.mode === "db_normalized") {
    return [0.25, 0.5, 0.75, 1].map((y) => ({ y, label: y.toFixed(2) }));
  }
  return [0.25, 0.5, 0.75, 1].map((y) => ({
    y,
    label: (y * s.ceiling).toPrecision(2),
  }));
}

/** Text of a bin value in the unit of the loudness mode. */
export function formatValue(v: number, mode: FftLoudnessMode): string {
  if (!Number.isFinite(v)) return "—";
  if (mode === "db") return `${v.toFixed(1)} dB`;
  if (mode === "db_normalized") return v.toFixed(3);
  return v < 0.01 ? v.toExponential(2) : v.toFixed(3);
}

/** Reduce `bins` to `columns` values, keeping the maximum per column. */
export function maxPerColumn(
  bins: Float32Array,
  columns: number,
  out: Float32Array,
): Float32Array {
  const n = bins.length;
  if (n === 0 || columns <= 0) return out;
  for (let x = 0; x < columns; x++) {
    const start = Math.floor((x * n) / columns);
    let end = Math.floor(((x + 1) * n) / columns);
    if (end <= start) end = start + 1;
    let max = -Infinity;
    for (let i = start; i < end && i < n; i++) {
      if (bins[i] > max) max = bins[i];
    }
    out[x] = max === -Infinity ? 0 : max;
  }
  return out;
}

export type ColormapKind = "inferno" | "accent" | "ice";

type Stop = [number, number, number, number];

const INFERNO: Stop[] = [
  [0, 0, 0, 4],
  [0.13, 31, 12, 72],
  [0.25, 85, 15, 109],
  [0.38, 136, 34, 106],
  [0.5, 186, 54, 85],
  [0.63, 227, 89, 51],
  [0.75, 249, 140, 10],
  [0.88, 249, 201, 50],
  [1, 252, 255, 164],
];

const ICE: Stop[] = [
  [0, 2, 4, 12],
  [0.25, 10, 30, 90],
  [0.55, 30, 120, 200],
  [0.8, 120, 220, 255],
  [1, 255, 255, 255],
];

function stopsToLut(stops: Stop[]): Uint8ClampedArray {
  const lut = new Uint8ClampedArray(256 * 3);
  for (let i = 0; i < 256; i++) {
    const t = i / 255;
    let k = 0;
    while (k < stops.length - 2 && stops[k + 1][0] < t) k++;
    const [t0, r0, g0, b0] = stops[k];
    const [t1, r1, g1, b1] = stops[k + 1];
    const f = t1 > t0 ? clamp01((t - t0) / (t1 - t0)) : 0;
    lut[i * 3] = r0 + (r1 - r0) * f;
    lut[i * 3 + 1] = g0 + (g1 - g0) * f;
    lut[i * 3 + 2] = b0 + (b1 - b0) * f;
  }
  return lut;
}

/** 256-entry RGB lookup for the waterfall. */
export function buildColormap(
  kind: ColormapKind,
  accentHex: string,
): Uint8ClampedArray {
  if (kind === "inferno") return stopsToLut(INFERNO);
  if (kind === "ice") return stopsToLut(ICE);
  const rgb = parseHex(accentHex) ?? { r: 31, g: 224, b: 255 };
  return stopsToLut([
    [0, 0, 0, 0],
    [0.35, rgb.r * 0.35, rgb.g * 0.35, rgb.b * 0.35],
    [0.7, rgb.r, rgb.g, rgb.b],
    [1, 255, 255, 255],
  ]);
}

/** Resolve a CSS custom property to a colour string usable on a canvas. */
export function cssColor(name: string, fallback: string): string {
  try {
    const value = getComputedStyle(document.documentElement)
      .getPropertyValue(name)
      .trim();
    return value || fallback;
  } catch {
    return fallback;
  }
}

/** `#rrggbb` → `rgba(r, g, b, a)`; other inputs get an approximate fallback. */
export function withAlpha(color: string, alpha: number): string {
  const rgb = parseHex(color);
  if (rgb) return `rgba(${rgb.r}, ${rgb.g}, ${rgb.b}, ${alpha})`;
  return color;
}
