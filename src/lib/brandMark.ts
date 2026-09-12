/**
 * The application mark, as geometry.
 *
 * This module is the **only** place the mark's shape is written down. The React
 * component (`components/icons/BrandMark.tsx`) and the icon generator
 * (`scripts/gen-icons.ts`, which rasterises every taskbar / tray / installer
 * PNG, ICO and ICNS) both draw from here, so the sidebar icon, the tray icon
 * and the installer icon cannot drift apart — a change made once lands in all
 * of them.
 *
 * ## The shape
 *
 * A rounded-square badge with a slashed zero inside it. The badge reads as the
 * app's frame (the same rounded-square line weight as the sidebar's line
 * icons), and the zero is the product's initial — a letterform rather than a
 * picture, so it survives every future rename without redrawing anything.
 *
 * Everything is stroked, never filled, so the mark is a line drawing in
 * `currentColor` and inherits the accent colour wherever it is placed.
 *
 * ## Sizing
 *
 * The grid is {@link GRID} units square and the paths are resolution-free, so
 * the mark renders correctly at any size. Two optical adjustments kick in
 * below {@link OPTICAL_MIN_SIZE}, where a
 * literal 1:1 scale would turn the drawing into a smudge:
 *
 * - {@link strokeScaleFor} thickens every stroke as the size shrinks, so the
 *   line weight stays roughly constant *in pixels* instead of vanishing.
 * - {@link slashFor} drops the slash, whose diagonal and the zero's ring merge
 *   into a single blob at favicon-sized renderings.
 *
 * Both are functions of the rendered size, so every caller gets the variant
 * that suits the size it is drawing at.
 */

/** The coordinate space every constant below is expressed in. */
export const GRID = 96;

const CENTER = GRID / 2;

/** Rounded-square badge: the outer frame around the zero. */
const BADGE = {
  /** Distance from the grid edge to the *outer* edge of the stroke. */
  inset: 6,
  /** Corner radius along the stroke's centreline. */
  radius: 22,
  stroke: 8,
} as const;

/** The zero itself — an ellipse, not a circle: a circle reads as an "O". */
const ZERO = {
  /** Horizontal radius along the stroke's centreline. */
  rx: 13,
  /** Vertical radius. Taller than wide, so the glyph reads as a digit. */
  ry: 17,
  stroke: 8,
} as const;

/** The diagonal that makes the zero unambiguous. */
const SLASH = {
  /** Degrees from +x at each end, y pointing down: bottom-left to top-right. */
  from: 125,
  to: -55,
  /** How far the slash runs past the ring at each end. */
  overshoot: 5,
  stroke: 7,
} as const;

/** Sizes at or above this use the mark exactly as drawn. */
export const OPTICAL_MIN_SIZE = 48;

/** Below this the slash is dropped: its diagonal fills the zero's counter. */
export const SLASH_MIN_SIZE = 24;

/**
 * Stroke widths are in grid units, so a 1:1 scale at a small render size would
 * leave sub-pixel lines. This scales them up as the size falls, keeping the
 * drawn weight roughly constant in pixels down to ~16 px.
 */
export function strokeScaleFor(size: number): number {
  if (size >= OPTICAL_MIN_SIZE) return 1;
  if (size >= 32) return 1.12;
  if (size >= SLASH_MIN_SIZE) return 1.28;
  return 1.45;
}

/** Whether a rendering at `size` pixels includes the slash. */
export function slashFor(size: number): boolean {
  return size >= SLASH_MIN_SIZE;
}

/** A point on the zero's outline: `deg` from +x, y pointing down. */
function zeroPoint(deg: number): readonly [number, number] {
  const t = (deg * Math.PI) / 180;
  return [CENTER + ZERO.rx * Math.cos(t), CENTER + ZERO.ry * Math.sin(t)];
}

/**
 * The slash's endpoints, each pushed `overshoot` units further outward along
 * the slash's own direction, so it crosses the ring and reads as a strike
 * rather than a chord.
 */
function slashEnds(): readonly [
  readonly [number, number],
  readonly [number, number],
] {
  const a = zeroPoint(SLASH.from);
  const b = zeroPoint(SLASH.to);
  const len = Math.hypot(b[0] - a[0], b[1] - a[1]);
  const ux = (b[0] - a[0]) / len;
  const uy = (b[1] - a[1]) / len;
  const k = SLASH.overshoot;
  return [
    [a[0] - ux * k, a[1] - uy * k],
    [b[0] + ux * k, b[1] + uy * k],
  ];
}

const n = (value: number): string => Number(value.toFixed(3)).toString();

/** The badge outline, as one closed path. */
export const BADGE_PATH: string = (() => {
  const r = BADGE.radius;
  const lo = BADGE.inset + BADGE.stroke / 2; // centreline, top/left
  const hi = GRID - lo; // centreline, bottom/right
  // Each corner is a single 90° arc: out along one edge, turn, down the next.
  return [
    `M ${n(lo + r)} ${n(lo)}`,
    `H ${n(hi - r)}`,
    `A ${r} ${r} 0 0 1 ${n(hi)} ${n(lo + r)}`,
    `V ${n(hi - r)}`,
    `A ${r} ${r} 0 0 1 ${n(hi - r)} ${n(hi)}`,
    `H ${n(lo + r)}`,
    `A ${r} ${r} 0 0 1 ${n(lo)} ${n(hi - r)}`,
    `V ${n(lo + r)}`,
    `A ${r} ${r} 0 0 1 ${n(lo + r)} ${n(lo)}`,
    "Z",
  ].join(" ");
})();

/**
 * The zero's ring, as one closed path.
 *
 * Two half-arcs sweeping the same way, meeting at the ellipse's top and bottom
 * extremes: one arc would have to be the long way round, and its endpoints
 * (left and right, the same height) cannot express that — so the ring is drawn
 * as two 180° halves and is still a single closed contour.
 */
export const ZERO_PATH: string = (() => {
  const left = CENTER - ZERO.rx;
  const right = CENTER + ZERO.rx;
  return [
    `M ${n(left)} ${n(CENTER)}`,
    `A ${n(ZERO.rx)} ${n(ZERO.ry)} 0 0 1 ${n(right)} ${n(CENTER)}`,
    `A ${n(ZERO.rx)} ${n(ZERO.ry)} 0 0 1 ${n(left)} ${n(CENTER)}`,
    "Z",
  ].join(" ");
})();

/** The slash, as one open path. */
export const SLASH_PATH: string = (() => {
  const [[x0, y0], [x1, y1]] = slashEnds();
  return `M ${n(x0)} ${n(y0)} L ${n(x1)} ${n(y1)}`;
})();

/** One stroked stroke of the mark. */
export interface BrandStroke {
  /** Stable id — also the React key. */
  id: "badge" | "zero" | "slash";
  d: string;
  /** Stroke width in grid units, before {@link strokeScaleFor}. */
  width: number;
}

/**
 * Every stroke of the mark, in draw order, for a rendering at `size` pixels.
 *
 * `size` only selects the optical variant; the geometry is identical at every
 * size above {@link SLASH_MIN_SIZE}.
 */
export function brandStrokes(size: number): BrandStroke[] {
  const scale = strokeScaleFor(size);
  const strokes: BrandStroke[] = [
    { id: "badge", d: BADGE_PATH, width: BADGE.stroke * scale },
    { id: "zero", d: ZERO_PATH, width: ZERO.stroke * scale },
  ];
  if (slashFor(size)) {
    strokes.push({ id: "slash", d: SLASH_PATH, width: SLASH.stroke * scale });
  }
  return strokes;
}

export interface BrandMarkOptions {
  /** Rendered size in pixels; selects the optical variant. */
  size: number;
  /** Any CSS colour. Defaults to `currentColor`, so it inherits. */
  color?: string;
  /** Override the variant's slash choice (used by the generator's variants). */
  withSlash?: boolean;
}

/**
 * The mark as a standalone SVG document.
 *
 * Used by `scripts/gen-icons.ts`, which hands each output to a headless
 * browser to rasterise. Kept here rather than in the script so the rasterised
 * files and the on-screen component are the same drawing.
 */
export function brandMarkSvg({
  size,
  color = "currentColor",
  withSlash,
}: BrandMarkOptions): string {
  const strokes = brandStrokes(size).filter(
    (s) => withSlash || s.id !== "slash",
  );
  const body = strokes
    .map(
      (s) =>
        `<path d="${s.d}" fill="none" stroke="${color}" ` +
        `stroke-width="${s.width.toFixed(3)}" stroke-linecap="round" ` +
        `stroke-linejoin="round"/>`,
    )
    .join("");
  return (
    `<svg xmlns="http://www.w3.org/2000/svg" width="${size}" height="${size}" ` +
    `viewBox="0 0 ${GRID} ${GRID}">${body}</svg>`
  );
}
