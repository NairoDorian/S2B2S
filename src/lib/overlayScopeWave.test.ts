// Standalone assert check, run by `bun test` (bunfig.toml roots discovery at
// src/). Pins the overlay waveform's min/max envelope: every sample lands in
// exactly one column, each column reports its true min and max, and the
// returned peak is the largest |sample| (what the per-sample loop computed).
import { test } from "bun:test";
import assert from "node:assert";
import { waveEnvelope, waveEnvelopeApplies } from "./overlayScope";

test("envelope reports each column's min and max and the overall peak", () => {
  const samples = new Float32Array([
    0.1, -0.2, 0.3, 0, -0.5, 0.25, 0.9, -0.1, 0, 0, -0.75, 0.05,
  ]);
  const min = new Float32Array(3);
  const max = new Float32Array(3);
  const peak = waveEnvelope(samples, 3, min, max);
  // 12 samples over 3 columns: [0,4) [4,8) [8,12).
  assert.deepStrictEqual(Array.from(min), [
    Math.fround(-0.2),
    Math.fround(-0.5),
    Math.fround(-0.75),
  ]);
  assert.deepStrictEqual(Array.from(max), [
    Math.fround(0.3),
    Math.fround(0.9),
    Math.fround(0.05),
  ]);
  assert.strictEqual(peak, Math.fround(0.9));
});

test("envelope of a sine covers the swing in every column and matches the per-sample peak", () => {
  const n = 4096;
  const samples = new Float32Array(n);
  for (let i = 0; i < n; i++)
    samples[i] = 0.6 * Math.sin((i / n) * 40 * Math.PI);
  const cols = 48;
  const min = new Float32Array(cols);
  const max = new Float32Array(cols);
  const peak = waveEnvelope(samples, cols, min, max);
  let expected = 0;
  for (let i = 0; i < n; i++)
    expected = Math.max(expected, Math.abs(samples[i]));
  assert.strictEqual(peak, expected);
  // Each column holds ~85 samples, more than one 205-sample period's half:
  // the envelope spans most of the swing, and min ≤ max everywhere.
  for (let x = 0; x < cols; x++) {
    assert.ok(min[x] <= max[x]);
    assert.ok(max[x] - min[x] > 0.5, `column ${x} too narrow`);
  }
});

test("every sample belongs to exactly one column", () => {
  // A single spike moved across the buffer shows up in exactly one column.
  const n = 100;
  const cols = 7;
  const min = new Float32Array(cols);
  const max = new Float32Array(cols);
  for (let spike = 0; spike < n; spike++) {
    const samples = new Float32Array(n);
    samples[spike] = 1;
    waveEnvelope(samples, cols, min, max);
    const hits = Array.from(max).filter((v) => v === 1).length;
    assert.strictEqual(hits, 1, `spike at ${spike}`);
  }
});

test("empty input and the polyline fallback threshold", () => {
  const min = new Float32Array(4);
  const max = new Float32Array(4);
  assert.strictEqual(waveEnvelope(new Float32Array(0), 4, min, max), 0);
  assert.strictEqual(waveEnvelopeApplies(4096, 48), true);
  assert.strictEqual(waveEnvelopeApplies(95, 48), false);
  assert.strictEqual(waveEnvelopeApplies(96, 48), true);
  assert.strictEqual(waveEnvelopeApplies(100, 0), false);
});
