/**
 * A tiny signed-distance-field rasteriser.
 *
 * The brand mark is made of four primitives — a rounded box, an ellipse, a
 * segment and a circle — so it does not need an SVG engine to become pixels.
 * Each primitive is expressed as a signed distance to its *centreline*; a
 * stroke is then simply "everything within half a stroke width of that
 * centreline", and a fill is "everything the distance says is inside".
 *
 * Two properties make this better than rasterising an SVG here:
 *
 * - **Anti-aliasing is analytic.** Coverage is derived from the distance in
 *   units of one pixel rather than from supersampling, so edges are smooth at
 *   every size without the memory or the time of a 4× supersample.
 * - **No dependency.** No browser, no native image library, no download — the
 *   same input produces the same bytes on every machine, so a regenerated
 *   icon is never a spurious diff.
 *
 * Distances are in the caller's own units; `pixel` converts one output pixel
 * into those units, which is what the coverage falloff is measured against.
 */

export interface Point {
  x: number;
  y: number;
}

/** A shape's distance to a point: negative inside, positive outside. */
export type Distance = (x: number, y: number) => number;

/** Distance to the centreline of a stroked shape. */
export interface Stroke {
  /** Distance to the shape's outline, in grid units. */
  distance: Distance;
  /** Half the stroke width, in grid units. */
  half: number;
}

const clamp = (v: number, lo: number, hi: number): number =>
  v < lo ? lo : v > hi ? hi : v;

/**
 * How much of the pixel at (x, y) the shape covers, treating the shape's
 * boundary as a half-pixel-wide band. One sample per pixel, but the value is
 * continuous, so curves stay smooth instead of banding into the 17 levels a
 * 4×4 supersample would give.
 *
 * `signed` is the difference between the two kinds of shape: a **stroke** is a
 * ribbon around a centreline, so the distance that matters is `|d| - half`; a
 * **fill** is the region the distance already calls inside, so it is `d`.
 */
function coverage(
  distance: Distance,
  half: number,
  x: number,
  y: number,
  pixel: number,
  signed: boolean,
): number {
  const d = distance(x, y);
  const edge = signed ? d : Math.abs(d) - half;
  return clamp((pixel * 0.5 - edge) / pixel, 0, 1);
}

/** Coverage of a filled shape. */
function fillCoverage(
  distance: Distance,
  x: number,
  y: number,
  pixel: number,
): number {
  return coverage(distance, 0, x, y, pixel, true);
}

// --- Primitives ------------------------------------------------------------

/** Rounded box centred on the origin, with the given half-extents. */
export function roundedBox(
  halfWidth: number,
  halfHeight: number,
  radius: number,
): Distance {
  return (x, y) => {
    const qx = Math.abs(x) - halfWidth + radius;
    const qy = Math.abs(y) - halfHeight + radius;
    return (
      Math.hypot(Math.max(qx, 0), Math.max(qy, 0)) +
      Math.min(Math.max(qx, qy), 0) -
      radius
    );
  };
}

/**
 * Ellipse centred on the origin.
 *
 * The exact distance needs an iterative solve; this is the standard
 * gradient-normalised approximation, which is smooth, cheap, and visual
 * perfection at every stroke width this project draws.
 */
export function ellipse(rx: number, ry: number): Distance {
  return (x, y) => {
    const k1 = Math.hypot(x / rx, y / ry);
    const k2 = Math.hypot(x / (rx * rx), y / (ry * ry));
    return k2 === 0 ? -Math.min(rx, ry) : (k1 * (k1 - 1)) / k2;
  };
}

/** Line segment from (ax, ay) to (bx, by). */
export function segment(
  ax: number,
  ay: number,
  bx: number,
  by: number,
): Distance {
  const dx = bx - ax;
  const dy = by - ay;
  const lenSq = dx * dx + dy * dy || 1;
  return (x, y) => {
    const h = clamp(((x - ax) * dx + (y - ay) * dy) / lenSq, 0, 1);
    return Math.hypot(x - ax - dx * h, y - ay - dy * h);
  };
}

/** Circle centred on (cx, cy). */
export function circle(cx: number, cy: number, r: number): Distance {
  return (x, y) => Math.hypot(x - cx, y - cy) - r;
}

/**
 * Arc of a circle from `fromDeg` to `toDeg` (degrees, 0 = +x, increasing
 * clockwise because y points down). Inside the sweep the distance is to the
 * circle; outside it, to the nearer end cap.
 */
export function arc(
  cx: number,
  cy: number,
  r: number,
  fromDeg: number,
  toDeg: number,
): Distance {
  const from = (fromDeg * Math.PI) / 180;
  let sweep = ((toDeg - fromDeg) * Math.PI) / 180;
  if (sweep < 0) sweep += 2 * Math.PI;
  const at = (a: number): Point => ({
    x: cx + r * Math.cos(a),
    y: cy + r * Math.sin(a),
  });
  const start = at(from);
  const end = at(from + sweep);
  return (x, y) => {
    let angle = Math.atan2(y - cy, x - cx) - from;
    while (angle < 0) angle += 2 * Math.PI;
    while (angle >= 2 * Math.PI) angle -= 2 * Math.PI;
    if (angle <= sweep) return Math.abs(Math.hypot(x - cx, y - cy) - r);
    return Math.min(
      Math.hypot(x - start.x, y - start.y),
      Math.hypot(x - end.x, y - end.y),
    );
  };
}

// --- Composition -----------------------------------------------------------

/**
 * The mark, placed in the output.
 *
 * Shape distances are expressed in the mark's own coordinates; placing the
 * mark at a scale or an offset is applied here rather than baked into the
 * primitives, so one set of shapes serves every variant and a change to the
 * drawing reaches all of them.
 */
export interface Placement {
  /** 1 draws the mark at its native size. */
  scale: number;
  /** Translation in output units, applied after the scale. */
  dx?: number;
  dy?: number;
}

const IDENTITY: Placement = { scale: 1 };

/** Coverage of one stroke, honouring its placement. */
function placedCoverage(
  stroke: Stroke,
  placement: Placement,
  x: number,
  y: number,
  pixel: number,
): number {
  const s = placement.scale;
  const lx = (x - (placement.dx ?? 0)) / s;
  const ly = (y - (placement.dy ?? 0)) / s;
  // The half-width and the distance are both in the mark's units, so they
  // compare directly; only the pixel's own size has to be converted.
  return coverage(stroke.distance, stroke.half, lx, ly, pixel / s, false);
}

/** One layer: a stroked or filled group of shapes, in one colour. */
export interface Layer {
  /** `#rrggbb`. */
  color: string;
  /** Strokes, unioned. */
  strokes?: Stroke[];
  /** Filled shapes, unioned. */
  fills?: Distance[];
  placement?: Placement;
  /**
   * Region erased from everything drawn *before* this layer, in output units.
   *
   * This is how a badge keeps a gap between itself and the mark it sits on:
   * the gap has to be taken out of the layer underneath, not out of the badge,
   * which is drawn on top of it either way.
   */
  clearsBelow?: { distance: Distance; radius: number };
  /** Global multiplier on this layer's coverage, 0–1. */
  opacity?: number;
}

export interface Canvas {
  width: number;
  height: number;
  rgba: Uint8Array;
}

/** A transparent canvas. */
export function createCanvas(width: number, height: number): Canvas {
  return { width, height, rgba: new Uint8Array(width * height * 4) };
}

/**
 * Draw `layers` onto `canvas`, back to front.
 *
 * `gridSize` is the coordinate space the shapes are expressed in, so a 64 px
 * icon of a 96-unit drawing reports `pixel = 1.5`.
 */
export function draw(
  canvas: Canvas,
  layers: Layer[],
  gridSize: number,
  hex: (color: string) => [number, number, number],
): void {
  const { width, height, rgba } = canvas;
  const pixel = gridSize / width;

  for (const layer of layers) {
    const [r, g, b] = hex(layer.color);
    const opacity = layer.opacity ?? 1;

    if (layer.clearsBelow) {
      const { distance, radius } = layer.clearsBelow;
      for (let py = 0; py < height; py++) {
        const y = (py + 0.5) * pixel;
        for (let pxi = 0; pxi < width; pxi++) {
          if (distance((pxi + 0.5) * pixel, y) >= radius) continue;
          const i = (py * width + pxi) * 4;
          rgba[i] = rgba[i + 1] = rgba[i + 2] = rgba[i + 3] = 0;
        }
      }
    }

    for (let py = 0; py < height; py++) {
      const y = (py + 0.5) * pixel;
      for (let pxi = 0; pxi < width; pxi++) {
        const x = (pxi + 0.5) * pixel;

        const placement = layer.placement ?? IDENTITY;
        let cov = 0;
        for (const stroke of layer.strokes ?? []) {
          const c = placedCoverage(stroke, placement, x, y, pixel);
          if (c > cov) cov = c;
          if (cov >= 1) break;
        }
        if (cov < 1) {
          for (const fill of layer.fills ?? []) {
            const c = fillCoverage(
              fill,
              (x - (placement.dx ?? 0)) / placement.scale,
              (y - (placement.dy ?? 0)) / placement.scale,
              pixel / placement.scale,
            );
            if (c > cov) cov = c;
            if (cov >= 1) break;
          }
        }
        if (cov <= 0) continue;

        cov *= opacity;
        if (cov <= 0) continue;

        const i = (py * width + pxi) * 4;
        // Source-over: the layer is opaque inside, so only alpha blends.
        const dstA = rgba[i + 3] / 255;
        const outA = cov + dstA * (1 - cov);
        if (outA <= 0) continue;
        rgba[i] = Math.round((r * cov + rgba[i] * dstA * (1 - cov)) / outA);
        rgba[i + 1] = Math.round(
          (g * cov + rgba[i + 1] * dstA * (1 - cov)) / outA,
        );
        rgba[i + 2] = Math.round(
          (b * cov + rgba[i + 2] * dstA * (1 - cov)) / outA,
        );
        rgba[i + 3] = Math.round(outA * 255);
      }
    }
  }
}
