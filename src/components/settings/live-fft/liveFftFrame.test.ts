// Standalone assert check, run by `bun test` (bunfig.toml roots discovery at
// src/). Pins the page's half of the `live_fft_frame` byte layout
// (live_fft/mod.rs encodes it; FFT_SPEC §3.1).
import { test } from "bun:test";
import assert from "node:assert";
import {
  FFT_FLAG_ACTIVE,
  FFT_FLAG_HAS_FEATURES,
  FFT_FLAG_SILENT,
  decodeFftFrame,
  framePollPeriodMs,
} from "./liveFftFrame";

interface Encode {
  seq: number;
  flags: number;
  axisVersion: number;
  peakHz: number;
  peakValue: number;
  dspUs: number;
  bins: number[];
  features: number[];
}

/** Little-endian, as the backend writes it. */
const encode = (f: Encode): ArrayBuffer => {
  const words = 8 + f.bins.length + f.features.length;
  const buffer = new ArrayBuffer(words * 4);
  const view = new DataView(buffer);
  view.setUint32(0, f.seq, true);
  view.setUint32(4, f.flags, true);
  view.setUint32(8, f.bins.length, true);
  view.setUint32(12, f.axisVersion, true);
  view.setFloat32(16, f.peakHz, true);
  view.setFloat32(20, f.peakValue, true);
  view.setFloat32(24, f.dspUs, true);
  view.setUint32(28, f.features.length, true);
  let at = 32;
  for (const v of [...f.bins, ...f.features]) {
    view.setFloat32(at, v, true);
    at += 4;
  }
  return buffer;
};

test("decodes header, zero-copy bins and features", () => {
  const buffer = encode({
    seq: 42,
    flags: FFT_FLAG_ACTIVE | FFT_FLAG_HAS_FEATURES,
    axisVersion: 7,
    peakHz: 1000,
    peakValue: -6,
    dspUs: 123.5,
    bins: [0.5, -10, -90.25],
    features: [1500, 4000, 0.25, 0.125, -20, -30, -40, -50],
  });
  const frame = decodeFftFrame(buffer);
  assert.ok(frame);
  assert.strictEqual(frame.seq, 42);
  assert.strictEqual(frame.active, true);
  assert.strictEqual(frame.silent, false);
  assert.strictEqual(frame.axisVersion, 7);
  assert.strictEqual(frame.peakHz, 1000);
  assert.strictEqual(frame.peakValue, -6);
  assert.strictEqual(frame.dspUs, 123.5);
  assert.deepStrictEqual(Array.from(frame.bins), [0.5, -10, -90.25]);
  // A view on the reply, not a copy.
  assert.strictEqual(frame.bins.buffer, buffer);
  assert.strictEqual(frame.bins.byteOffset, 32);
  assert.deepStrictEqual(frame.features, {
    centroidHz: 1500,
    rolloffHz: 4000,
    flatness: 0.25,
    flux: 0.125,
    rmsDb: -20,
    bassDb: -30,
    midDb: -40,
    highDb: -50,
  });
});

test("header-only reply decodes with empty bins and no features", () => {
  const frame = decodeFftFrame(
    encode({
      seq: 9,
      flags: FFT_FLAG_ACTIVE | FFT_FLAG_SILENT,
      axisVersion: 1,
      peakHz: 0,
      peakValue: 0,
      dspUs: 0,
      bins: [],
      features: [],
    }),
  );
  assert.ok(frame);
  assert.strictEqual(frame.seq, 9);
  assert.strictEqual(frame.silent, true);
  assert.strictEqual(frame.bins.length, 0);
  assert.strictEqual(frame.features, null);
});

test("features are ignored without the flag", () => {
  const frame = decodeFftFrame(
    encode({
      seq: 1,
      flags: FFT_FLAG_ACTIVE,
      axisVersion: 1,
      peakHz: 0,
      peakValue: 0,
      dspUs: 0,
      bins: [1],
      features: [1, 2, 3, 4, 5, 6, 7, 8],
    }),
  );
  assert.ok(frame);
  assert.strictEqual(frame.features, null);
});

test("truncated replies are rejected", () => {
  assert.strictEqual(decodeFftFrame(new ArrayBuffer(16)), null);
  const full = encode({
    seq: 1,
    flags: FFT_FLAG_ACTIVE,
    axisVersion: 1,
    peakHz: 0,
    peakValue: 0,
    dspUs: 0,
    bins: [1, 2, 3, 4],
    features: [],
  });
  assert.strictEqual(decodeFftFrame(full.slice(0, full.byteLength - 4)), null);
});

test("poll period is half the frame period, floored at 8 ms", () => {
  assert.strictEqual(framePollPeriodMs(30), 17);
  assert.strictEqual(framePollPeriodMs(5), 100);
  assert.strictEqual(framePollPeriodMs(60), 8);
  assert.strictEqual(framePollPeriodMs(0), 500);
});
