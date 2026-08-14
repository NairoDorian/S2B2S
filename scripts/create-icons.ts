import fs from "node:fs";
import path from "node:path";
import zlib from "node:zlib";

/**
 * Generates a cross-platform Tauri v2 icon set entirely from code.
 *
 * Produces:
 *  - `32x32.png`, `128x128.png`, `128x128@2x.png` (256px), `icon.png` (512px)
 *  - `icon.ico`  — multi-entry PNG-compressed ICO for Windows (16/32/48/64/128/256)
 *  - `icon.icns` — a VALID Apple Icon Image container for macOS with PNG-encoded chunks.
 */

const BRAND: readonly [number, number, number] = [242, 140, 187]; // S2B2S Rose Pink

function createValidPngBuffer(width: number, height: number): Buffer {
  const scanlineLength = 1 + width * 4;
  const rawData = Buffer.alloc(scanlineLength * height);

  for (let y = 0; y < height; y++) {
    const offset = y * scanlineLength;
    rawData[offset] = 0; // Filter 0 (None)
    for (let x = 0; x < width; x++) {
      const px = offset + 1 + x * 4;
      rawData[px] = BRAND[0]; // R
      rawData[px + 1] = BRAND[1]; // G
      rawData[px + 2] = BRAND[2]; // B
      rawData[px + 3] = 255; // A
    }
  }

  const idatData = zlib.deflateSync(rawData);

  function makeChunk(type: string, data: Buffer): Buffer {
    const lenBuf = Buffer.alloc(4);
    lenBuf.writeUInt32BE(data.length, 0);
    const typeBuf = Buffer.from(type, "ascii");
    const crcBuf = Buffer.alloc(4);
    const crc = zlib.crc32(Buffer.concat([typeBuf, data]));
    crcBuf.writeUInt32BE(crc, 0);
    return Buffer.concat([lenBuf, typeBuf, data, crcBuf]);
  }

  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr.writeUInt8(8, 8); // bit depth 8
  ihdr.writeUInt8(6, 9); // color type 6 (RGBA)
  ihdr.writeUInt8(0, 10);
  ihdr.writeUInt8(0, 11);
  ihdr.writeUInt8(0, 12);

  const signature = Buffer.from([
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a,
  ]);
  const ihdrChunk = makeChunk("IHDR", ihdr);
  const idatChunk = makeChunk("IDAT", idatData);
  const iendChunk = makeChunk("IEND", Buffer.alloc(0));

  return Buffer.concat([signature, ihdrChunk, idatChunk, iendChunk]);
}

const pngCache = new Map<number, Buffer>();
function getPng(size: number): Buffer {
  let buf = pngCache.get(size);
  if (!buf) {
    buf = createValidPngBuffer(size, size);
    pngCache.set(size, buf);
  }
  return buf;
}

function createIco(entries: readonly { size: number; png: Buffer }[]): Buffer {
  const headerSize = 6 + entries.length * 16;
  const header = Buffer.alloc(headerSize);

  header.writeUInt16LE(0, 0); // Reserved
  header.writeUInt16LE(1, 2); // Type 1 = ICO
  header.writeUInt16LE(entries.length, 4); // Image count

  let offset = headerSize;
  entries.forEach((entry, i) => {
    const base = 6 + i * 16;
    header.writeUInt8(entry.size >= 256 ? 0 : entry.size, base + 0); // Width
    header.writeUInt8(entry.size >= 256 ? 0 : entry.size, base + 1); // Height
    header.writeUInt8(0, base + 2); // Color count
    header.writeUInt8(0, base + 3); // Reserved
    header.writeUInt16LE(1, base + 4); // Planes
    header.writeUInt16LE(32, base + 6); // Bits per pixel
    header.writeUInt32LE(entry.png.length, base + 8); // Size of PNG data
    header.writeUInt32LE(offset, base + 12); // Offset to PNG data
    offset += entry.png.length;
  });

  return Buffer.concat([header, ...entries.map((e) => e.png)]);
}

function createIcns(entries: readonly { type: string; png: Buffer }[]): Buffer {
  const chunks: Buffer[] = [];

  for (const { type, png } of entries) {
    const lengthBuf = Buffer.alloc(4);
    lengthBuf.writeUInt32BE(8 + png.length, 0);
    chunks.push(Buffer.concat([Buffer.from(type, "ascii"), lengthBuf, png]));
  }

  const totalLength = 8 + chunks.reduce((sum, c) => sum + c.length, 0);
  const lengthBuf = Buffer.alloc(4);
  lengthBuf.writeUInt32BE(totalLength, 0);

  return Buffer.concat([Buffer.from("icns", "ascii"), lengthBuf, ...chunks]);
}

const sizes = [16, 32, 48, 64, 128, 256, 512, 1024] as const;
const pngs = new Map<number, Buffer>(sizes.map((s) => [s, getPng(s)]));

const ico = createIco(
  [16, 32, 48, 64, 128, 256].map((size) => ({ size, png: pngs.get(size)! })),
);

const icns = createIcns([
  { type: "icp4", png: pngs.get(16)! },
  { type: "icp5", png: pngs.get(32)! },
  { type: "icp6", png: pngs.get(64)! },
  { type: "ic07", png: pngs.get(128)! },
  { type: "ic08", png: pngs.get(256)! },
  { type: "ic09", png: pngs.get(512)! },
  { type: "ic10", png: pngs.get(1024)! },
  { type: "ic11", png: pngs.get(32)! },
  { type: "ic12", png: pngs.get(64)! },
  { type: "ic13", png: pngs.get(256)! },
  { type: "ic14", png: pngs.get(512)! },
  { type: "ic15", png: pngs.get(1024)! },
]);

const iconsDir = path.resolve("src-tauri/icons");
if (!fs.existsSync(iconsDir)) {
  fs.mkdirSync(iconsDir, { recursive: true });
}

fs.writeFileSync(path.join(iconsDir, "32x32.png"), pngs.get(32)!);
fs.writeFileSync(path.join(iconsDir, "128x128.png"), pngs.get(128)!);
fs.writeFileSync(path.join(iconsDir, "128x128@2x.png"), pngs.get(256)!);
fs.writeFileSync(path.join(iconsDir, "icon.png"), pngs.get(512)!);
fs.writeFileSync(path.join(iconsDir, "icon.ico"), ico);
fs.writeFileSync(path.join(iconsDir, "icon.icns"), icns);

console.log(
  `✅ Valid cross-platform icon set generated (${sizes.length} PNG sizes, multi-res ICO + ICNS) at ${iconsDir}`,
);
