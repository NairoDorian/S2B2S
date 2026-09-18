// Standalone assert check, run by `bun test` (bunfig.toml roots discovery at
// src/). Originally run as a bare script before the runner existed.
//
// Pins the circular spectrum's construction: the input signal is the
// original pooled spectrum with its inversion appended, plus the original
// with its inversion prepended, added point-wise (the cross-sum of each
// bin with its mirror partner, halved into display units); that signal is
// laid ONCE around the full circle (a 2π sweep — no quarter-and-rotate-4),
// the whole figure is rotated 90° so the seam straddles the RIGHT of the
// ring, and the figure is exactly mirror-symmetric about the horizontal
// axis. The functions under test are the ones the overlay's painter
// actually calls (src/overlay/OverlayScope.tsx), so "the spec holds" is
// about the shipped code, not a copy of it.
import { test } from "bun:test";
import assert from "node:assert";
import {
  circularAngleAt,
  circularPointCount,
  circularSignalAt,
} from "./overlayScope";

// A representative display-bin count (the default) and a degenerate one.
const BINS = 48;
const TINY = 12;
const TAU = Math.PI * 2;

const norm = (th: number) => Math.atan2(Math.sin(th), Math.cos(th));

test("circular spectrum geometry: A+B cross-sum input, full 2π sweep, 90° rotation, exact horizontal mirror symmetry", () => {
  // 1. The ring has exactly 2×displayBins points — each summed signal (A and
  //    B) is twice the original spectrum's length.
  assert.strictEqual(circularPointCount(BINS), 96);
  assert.strictEqual(circularPointCount(TINY), 24);

  // 2. The sweep is the full circle: one step is 2π/points, and the points
  //    cover every angle exactly once. The 90° rotation puts the seam at the
  //    RIGHT of the ring: the first point half a step below 3 o'clock, the
  //    last half a step above it, with the closing chord spanning it.
  for (const bins of [BINS, TINY]) {
    const points = circularPointCount(bins);
    const step = TAU / points;
    assert.ok(Math.abs(circularAngleAt(0, points) - step / 2) < 1e-9);
    assert.ok(
      Math.abs(circularAngleAt(points - 1, points) - (TAU - step / 2)) < 1e-9,
    );
    // Monotone, strictly clockwise, one step at a time (float-tolerant).
    for (let k = 1; k < points; k++) {
      const prev = circularAngleAt(k - 1, points);
      const curr = circularAngleAt(k, points);
      assert.ok(curr > prev && Math.abs(curr - prev - step) < 1e-9);
    }
  }

  // 3. The input signal is the cross-sum: A + B where A = [s, rev(s)] and
  //    B = [rev(s), s]. At every point k that is (bin + its mirror partner)
  //    / 2, which is why the signal is a palindrome that repeats twice.
  const pooled = Array.from({ length: BINS }, (_, i) => (i % 7) / 8);
  for (let k = 0; k < 2 * BINS; k++) {
    const j = k < BINS ? k : k - BINS;
    const expected = (pooled[j] + pooled[BINS - 1 - j]) / 2;
    assert.ok(Math.abs(circularSignalAt(k, pooled) - expected) < 1e-12);
  }
  // A + B spelled out literally, per the spec, for a few probe points.
  const A = (i: number) => (i < BINS ? pooled[i] : pooled[2 * BINS - 1 - i]);
  const B = (i: number) => (i < BINS ? pooled[BINS - 1 - i] : pooled[i - BINS]);
  for (const k of [0, 5, BINS - 1, BINS, BINS + 13, 2 * BINS - 1]) {
    assert.ok(
      Math.abs(circularSignalAt(k, pooled) - (A(k) + B(k)) / 2) < 1e-12,
      `point ${k}: the signal is (A + B) / 2`,
    );
  }
  // Palindrome: the value at k equals the value at its ring mirror.
  for (let k = 0; k < 2 * BINS; k++) {
    assert.strictEqual(
      circularSignalAt(k, pooled),
      circularSignalAt(2 * BINS - 1 - k, pooled),
    );
  }
  // Period: the summed signal repeats twice around the ring.
  for (let k = 0; k < BINS; k++) {
    assert.strictEqual(
      circularSignalAt(k, pooled),
      circularSignalAt(k + BINS, pooled),
    );
  }

  // 4. Deterministic single-peak check: with a synthetic spectrum whose only
  //    non-zero bin is `peak`, the cross-sum lights exactly the four points
  //    where that bin (or its mirror partner) is paired, and the lit angles
  //    are symmetric about the horizontal axis through the seam.
  const peak = 7;
  const single = Array.from<number>({ length: BINS }).fill(0);
  single[peak] = 1;
  const lit = new Map<number, number>();
  for (let k = 0; k < 2 * BINS; k++) {
    const v = circularSignalAt(k, single);
    if (v > 0) lit.set(k, circularAngleAt(k, 2 * BINS));
  }
  assert.strictEqual(lit.size, 4);
  const expected = new Set([
    peak,
    BINS - 1 - peak,
    BINS + peak,
    2 * BINS - 1 - peak,
  ]);
  for (const k of lit.keys())
    assert.ok(expected.has(k), `unexpected lit point ${k}`);
  // Mirror symmetry about the horizontal axis: θ → -θ (mod 2π).
  for (const th of lit.values()) {
    const reflected = norm(-norm(th));
    assert.ok(
      [...lit.values()].some(
        (other) => Math.abs(reflected - norm(other)) < 1e-9,
      ),
      `angle ${th} has no mirror partner across the horizontal axis`,
    );
  }
});
