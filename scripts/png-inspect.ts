// Render a PNG as ASCII art + a colour histogram, so an icon can be reviewed
// without opening an image viewer. Chromium does the decoding.
//
//   bun scripts/png-inspect.ts <file.png> [columns]
import { existsSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join, resolve } from "node:path";
import { chromium } from "@playwright/test";

const file = process.argv[2];
if (!file) {
  console.error("usage: bun scripts/png-inspect.ts <file.png> [columns]");
  process.exit(2);
}
const cols = Number(process.argv[3]) || 48;

const dataUrl =
  "data:image/png;base64," + readFileSync(resolve(file)).toString("base64");

// The pinned Playwright alpha expects a browser build newer than the one in
// the maintainer's local cache, so that Windows build is named explicitly when
// it exists; anywhere else Playwright's own resolution is used. Chromium is
// only a rasteriser/decoder here, so the exact revision does not matter.
const CHROME = join(
  homedir(),
  "AppData",
  "Local",
  "ms-playwright",
  "chromium-1234",
  "chrome-win64",
  "chrome.exe",
);
const browser = await chromium.launch(
  existsSync(CHROME) ? { executablePath: CHROME } : {},
);
const page = await browser.newPage();
const out = await page.evaluate(async (src) => {
  const img = new Image();
  img.src = src;
  await img.decode();
  const canvas = document.createElement("canvas");
  canvas.width = img.naturalWidth;
  canvas.height = img.naturalHeight;
  const ctx = canvas.getContext("2d")!;
  ctx.drawImage(img, 0, 0);
  const { data, width, height } = ctx.getImageData(
    0,
    0,
    canvas.width,
    canvas.height,
  );
  const px: { r: number; g: number; b: number; a: number }[] = [];
  for (let i = 0; i < data.length; i += 4) {
    px.push({ r: data[i], g: data[i + 1], b: data[i + 2], a: data[i + 3] });
  }
  return { width, height, px };
}, dataUrl);
await browser.close();

console.log(`${file}: ${out.width}x${out.height}`);

// Colour histogram. Alpha is bucketed (it is a coverage ramp); RGB is exact,
// so the dominant ink colour prints as the colour it actually is.
const hist = new Map<string, number>();
for (const p of out.px) {
  const key = [p.r, p.g, p.b, Math.round(p.a / 32) * 32].join(",");
  hist.set(key, (hist.get(key) ?? 0) + 1);
}
const top = [...hist.entries()].sort((a, b) => b[1] - a[1]).slice(0, 6);
console.log("colours (rgba → pixel count):");
for (const [key, count] of top) {
  const [r, g, b, a] = key.split(",").map(Number);
  const hex =
    "#" + [r, g, b].map((v) => v.toString(16).padStart(2, "0")).join("");
  console.log(`  ${hex} a=${a}  ${count}`);
}

// Two maps, because one is not enough to read an icon: a dark glyph on a light
// tile and a light glyph on a dark tile have the same *shape* and opposite
// *colours*, so a coverage map alone cannot tell you which one you drew.
function map(
  value: (p: { r: number; g: number; b: number; a: number }) => number | null,
  ramp: string,
): string[] {
  const rows = Math.max(1, Math.round((cols * out.height) / out.width / 2));
  const lines: string[] = [];
  for (let ry = 0; ry < rows; ry++) {
    let line = "";
    for (let rx = 0; rx < cols; rx++) {
      const x0 = Math.floor((rx * out.width) / cols);
      const x1 = Math.max(x0 + 1, Math.floor(((rx + 1) * out.width) / cols));
      const y0 = Math.floor((ry * out.height) / rows);
      const y1 = Math.max(y0 + 1, Math.floor(((ry + 1) * out.height) / rows));
      let sum = 0;
      let n = 0;
      for (let y = y0; y < y1; y++) {
        for (let x = x0; x < x1; x++) {
          const v = value(out.px[y * out.width + x]);
          if (v !== null) sum += v;
          n++;
        }
      }
      const v = n ? sum / n : 0;
      line +=
        ramp[
          Math.min(
            ramp.length - 1,
            Math.max(0, Math.round(v * (ramp.length - 1))),
          )
        ];
    }
    lines.push(line);
  }
  return lines;
}

const luminance = (p: { r: number; g: number; b: number; a: number }): number =>
  (0.299 * p.r + 0.587 * p.g + 0.114 * p.b) / 255;

console.log("\ncoverage (alpha — what is drawn):");
console.log(map((p) => p.a / 255, " .:-=+*#%@").join("\n"));

console.log("\nluminance (what colour it is; blank = transparent):");
console.log(
  map((p) => (p.a < 40 ? null : luminance(p)), "@%#*+=-:. ").join("\n"),
);
