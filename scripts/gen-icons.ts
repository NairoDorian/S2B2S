/**
 * Regenerate every icon the application ships.
 *
 * The mark's geometry lives in `src/lib/brandMark.ts`, which is also what the
 * sidebar and the onboarding screens render. This script is the *other* half of
 * that contract: it draws the same geometry into the files the OS and the tray
 * read, so a change to the mark reaches the taskbar, the tray, the window
 * chrome and the installer without anyone opening an image editor.
 *
 *   bun run icons:generate
 *
 * Two outputs, two techniques, both from the one geometry:
 *
 * | Output | How | Why that way |
 * | --- | --- | --- |
 * | `src-tauri/icons/**` | an SVG handed to `tauri icon` | the bundler's own tool knows every size, and writes the ICO and ICNS containers itself |
 * | `src-tauri/resources/tray_*.png` | the SDF rasteriser in `scripts/lib/sdf.ts` | the tray needs exact pixel control: which ink for which OS theme, and a state badge punched out of the mark |
 *
 * The app icon is the *badge*: a white tile with an ink outline and the zero
 * struck through it, reading as one black-and-white glyph at any size. The
 * tray icons are the *line drawing* alone, matching the sidebar's icons, with
 * a state badge in the corner while recording or transcribing.
 *
 * No image library and no browser are involved, so this runs anywhere Bun does
 * and produces byte-identical output every time.
 */

import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { $ } from "bun";

import {
  BADGE_PATH,
  brandStrokes,
  GRID,
  SLASH_PATH,
  slashFor,
  ZERO_PATH,
  type BrandStroke,
} from "../src/lib/brandMark";
import { encodePng, parseHex } from "./lib/png";
import {
  arc,
  circle,
  createCanvas,
  draw,
  ellipse,
  roundedBox,
  segment,
  type Distance,
  type Layer,
  type Stroke,
} from "./lib/sdf";

const ROOT = join(import.meta.dir, "..");
const ICONS_DIR = join(ROOT, "src-tauri", "icons");
const RESOURCES_DIR = join(ROOT, "src-tauri", "resources");

/** Tray icons are read by the OS at this size; the source art is a 64 px PNG. */
const TRAY_SIZE = 64;

/**
 * The accent, mirroring `--dark-color-accent` in `src/styles/theme.css`.
 *
 * The tray's colour theme cannot follow the user's custom accent — these are
 * files on disk, written once — so it uses the shipped default. A custom
 * accent reaches the window and the overlay, which are drawn live.
 */
const ACCENT = "#1fe0ff";

const INK_ON_DARK = "#ffffff";
const INK_ON_LIGHT = "#000000";

/** The app tile: a near-black ground with a white glyph, so it reads as one
 *  black-and-white character on any desktop. */
const TILE_INK = "#0b0e11";
const TILE_GLYPH = "#ffffff";

// --- Geometry, mirrored from `src/lib/brandMark.ts` -------------------------

/**
 * The mark's constants, re-derived here from the same exported paths.
 *
 * `brandMark.ts` exports the path *strings* (what the renderers need); the
 * rasteriser needs the same shapes as distances. Rather than duplicate the
 * numbers, the path strings are the source and the numbers are parsed back out
 * of them — a drift between the two becomes a thrown error at generation time
 * instead of a silently different icon.
 */
function parseRoundedBox(path: string): { half: number; radius: number } {
  // "M {lo+r} {lo} H {hi-r} A r r 0 0 1 …" — the move's second coordinate is
  // the top edge, i.e. the centreline inset; the corner radius follows it.
  const m = path.match(/^M\s+([\d.]+)\s+([\d.]+)/);
  const a = path.match(/A\s+([\d.]+)\s+([\d.]+)/);
  if (!m || !a) throw new Error(`unparseable badge path: ${path}`);
  const lo = Number(m[2]); // the top edge's y is the centreline inset
  const radius = Number(a[1]);
  const half = GRID / 2 - lo;
  return { half, radius };
}

function parseRadii(path: string): { rx: number; ry: number } {
  const a = path.match(/A\s+([\d.]+)\s+([\d.]+)/);
  if (!a) throw new Error(`unparseable zero path: ${path}`);
  return { rx: Number(a[1]), ry: Number(a[2]) };
}

function parseSegment(path: string): [number, number, number, number] {
  const m = path.match(
    /^M\s+(-?[\d.]+)\s+(-?[\d.]+)\s+L\s+(-?[\d.]+)\s+(-?[\d.]+)/,
  );
  if (!m) throw new Error(`unparseable slash path: ${path}`);
  return [Number(m[1]), Number(m[2]), Number(m[3]), Number(m[4])];
}

/**
 * Parse the drawn paths back into distances.
 *
 * `size` selects the optical variant, exactly as the renderers do: the stroke
 * widths must be the ones `brandStrokes` chose for that size, or the raster
 * would be a different drawing than the screen.
 */
function markStrokes(size: number): Stroke[] {
  const { half: boxHalf, radius } = parseRoundedBox(BADGE_PATH);
  const { rx, ry } = parseRadii(ZERO_PATH);
  const [sx0, sy0, sx1, sy1] = parseSegment(SLASH_PATH);

  // Each drawn stroke's width is recovered from `brandStrokes` so the optical
  // scaling is applied in one place.
  const widths = new Map<string, number>(
    brandStrokes(size).map((s: BrandStroke) => [s.id, s.width]),
  );

  const strokes: Stroke[] = [
    {
      distance: translate(
        roundedBox(boxHalf, boxHalf, radius),
        GRID / 2,
        GRID / 2,
      ),
      half: (widths.get("badge") ?? 0) / 2,
    },
    {
      distance: translate(ellipse(rx, ry), GRID / 2, GRID / 2),
      half: (widths.get("zero") ?? 0) / 2,
    },
  ];

  if (slashFor(size)) {
    strokes.push({
      distance: segment(sx0, sy0, sx1, sy1),
      half: (widths.get("slash") ?? 0) / 2,
    });
  }
  return strokes;
}

/** Move a distance field by (dx, dy). */
function translate(d: Distance, dx: number, dy: number): Distance {
  return (x, y) => d(x - dx, y - dy);
}

// --- Tray layout ------------------------------------------------------------

/**
 * The mark shrinks to make room for a state badge. Without this the badge would
 * sit on top of the mark's corner and the two would read as one blob.
 */
const STATE_MARK_SCALE = 0.7;

/** Centre and radius of the state badge, in grid units. */
const BADGE_CENTRE = { x: 75, y: 75 };
const STATE_DISC_RADIUS = 14;
/** Clear space around the badge, punched out of the mark beneath it. */
const STATE_CLEAR_RADIUS = 20;

type TrayState = "idle" | "recording" | "transcribing";

/** The clear ring punched out of the mark so the badge reads as separate. */
const STATE_CLEARING = {
  distance: circle(BADGE_CENTRE.x, BADGE_CENTRE.y, 0),
  radius: STATE_CLEAR_RADIUS,
};

/**
 * A layer that draws nothing and erases what is beneath it.
 *
 * `distance` is negative inside the region to erase, so a union of shapes is
 * its `min` — the same convention the primitives already use.
 */
function eraser(distance: Distance): Layer {
  return { color: "#000000", clearsBelow: { distance, radius: 0 } };
}

/** What the corner badge draws for each state, in grid units. */
function stateLayers(state: TrayState, ink: string): Layer[] {
  if (state === "recording") {
    // A solid dot: the universal "recording" mark, and the one shape that
    // still reads when the icon is small in a crowded tray.
    return [
      {
        color: ink,
        fills: [circle(BADGE_CENTRE.x, BADGE_CENTRE.y, STATE_DISC_RADIUS)],
        clearsBelow: STATE_CLEARING,
      },
    ];
  }

  if (state === "transcribing") {
    // A three-quarter ring, the shape of a spinner mid-turn. A second dot
    // would be indistinguishable from the recording badge at tray size; an
    // open ring reads as "busy" at a glance.
    return [
      {
        color: ink,
        strokes: [
          {
            // 230° of sweep, leaving the gap at the top — the 12-to-2 o'clock
            // opening that makes a ring read as a spinner rather than a circle.
            distance: arc(BADGE_CENTRE.x, BADGE_CENTRE.y, 10.5, -25, 205),
            half: 3.5,
          },
        ],
        clearsBelow: STATE_CLEARING,
      },
    ];
  }

  return [];
}

/**
 * The idle-with-warning badge: a disc in the theme's ink with an exclamation
 * erased out of it.
 *
 * The exclamation is a *hole*, so what shows through is the taskbar itself —
 * which is what makes it legible in both themes: light ink on a dark taskbar
 * leaves a dark exclamation in a white disc, and the reverse on a light one.
 * The same trick the app icon's zero uses on its tile.
 */
function warningLayers(ink: string): Layer[] {
  const cx = BADGE_CENTRE.x;
  const cy = BADGE_CENTRE.y - 2;
  const bar = segment(cx, cy - 8, cx, cy + 2);
  const dot = circle(cx, cy + 8.5, 2.6);
  return [
    {
      color: ink,
      fills: [circle(cx, cy, STATE_DISC_RADIUS)],
      clearsBelow: STATE_CLEARING,
    },
    eraser((x, y) => Math.min(bar(x, y) - 2.6, dot(x, y))),
  ];
}

// --- Tray rendering ---------------------------------------------------------

function trayIcon(state: TrayState, ink: string, warning = false): Buffer {
  const canvas = createCanvas(TRAY_SIZE, TRAY_SIZE);

  const placement =
    state === "idle" && !warning ? undefined : { scale: STATE_MARK_SCALE };

  // The mark's own strokes are chosen for the size they are drawn at, which is
  // the *scaled* size once a badge has taken part of the frame.
  const markSize = (placement?.scale ?? 1) * TRAY_SIZE;
  const strokes = markStrokes(markSize);

  const layers: Layer[] = [{ color: ink, strokes, placement }];
  layers.push(...(warning ? warningLayers(ink) : stateLayers(state, ink)));

  draw(canvas, layers, GRID, parseHex);
  return encodePng(TRAY_SIZE, TRAY_SIZE, canvas.rgba);
}

// --- App icon ---------------------------------------------------------------

/**
 * The app icon as an SVG, in the same grid as everything else.
 *
 * `tauri icon` rasterises this into every size the bundler and the two mobile
 * platforms want, and writes the ICO and ICNS containers itself — reusing the
 * tool that *consumes* the icons is what keeps them correct without a bespoke
 * container writer here.
 */
function appIconSvg(): string {
  const strokes = brandStrokes(1024);
  const badge = strokes.find((s) => s.id === "badge")!;
  const glyph = strokes.filter((s) => s.id !== "badge");

  const glyphPaths = glyph
    .map(
      (s) =>
        `<path d="${s.d}" fill="none" stroke="${TILE_GLYPH}" ` +
        `stroke-width="${s.width.toFixed(3)}" stroke-linecap="round" ` +
        `stroke-linejoin="round"/>`,
    )
    .join("");

  return (
    `<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" ` +
    `viewBox="0 0 ${GRID} ${GRID}">` +
    // The tile: an ink ground with the badge's own outline drawn over it, so
    // the shape is the mark's shape at any size rather than a second guess.
    `<path d="${badge.d}" fill="${TILE_INK}" stroke="${TILE_INK}" ` +
    `stroke-width="${badge.width.toFixed(3)}" stroke-linejoin="round"/>` +
    glyphPaths +
    `</svg>`
  );
}

// --- Entry point ------------------------------------------------------------

async function main(): Promise<void> {
  const only = process.argv[2];

  if (only !== "tray") {
    const staging = mkdtempSync(join(tmpdir(), "brand-icon-"));
    try {
      const source = join(staging, "icon.svg");
      writeFileSync(source, appIconSvg());
      console.log("src-tauri/icons  ← tauri icon");
      await $`bunx tauri icon ${source} -o ${ICONS_DIR}`.quiet();
      // `tauri icon` does not write the 1024 master the previous set carried;
      // nothing reads it, so it is not resurrected here.
      rmSync(join(ICONS_DIR, "logo.png"), { force: true });
    } finally {
      rmSync(staging, { recursive: true, force: true });
    }
  }

  if (only !== "app") {
    mkdirSync(RESOURCES_DIR, { recursive: true });
    // One file per (OS theme, state). The theme names the *taskbar*, matching
    // the `AppTheme` variants the tray selects on, so `tray_dark_*` is the set
    // for a dark taskbar and therefore carries light ink.
    const set: [string, TrayState, string, boolean?][] = [
      ["tray_dark_idle.png", "idle", INK_ON_DARK],
      ["tray_light_idle.png", "idle", INK_ON_LIGHT],
      ["tray_color_idle.png", "idle", ACCENT],
      ["tray_dark_recording.png", "recording", INK_ON_DARK],
      ["tray_light_recording.png", "recording", INK_ON_LIGHT],
      ["tray_color_recording.png", "recording", ACCENT],
      ["tray_dark_transcribing.png", "transcribing", INK_ON_DARK],
      ["tray_light_transcribing.png", "transcribing", INK_ON_LIGHT],
      ["tray_color_transcribing.png", "transcribing", ACCENT],
      // Secure Input is macOS-only, so only the two macOS themes need a
      // warning badge; the colour theme keeps its plain idle icon.
      ["tray_dark_idle_warning.png", "idle", INK_ON_DARK, true],
      ["tray_light_idle_warning.png", "idle", INK_ON_LIGHT, true],
    ];

    console.log("src-tauri/resources  ← SDF rasteriser");
    for (const [file, state, ink, warning] of set) {
      writeFileSync(join(RESOURCES_DIR, file), trayIcon(state, ink, warning));
      console.log(`  ${file}`);
    }
  }
}

await main();
