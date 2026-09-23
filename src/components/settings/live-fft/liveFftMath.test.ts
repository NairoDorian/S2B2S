// Standalone assert check, run by `bun test` (bunfig.toml roots discovery at
// src/). Pins the page's shared units pass and its per-millisecond decays:
// the picture must fall at the same speed whatever the update rate is.
import { test } from "bun:test";
import assert from "node:assert";
import {
  FrameUnitsCache,
  PEAK_HOLD_DECAY_PER_MS,
  REFERENCE_FRAME_MS,
  decayCeiling,
  decayStepMs,
} from "./liveFftMath";

const close = (a: number, b: number, eps = 1e-9) =>
  assert.ok(Math.abs(a - b) <= eps * Math.max(1, Math.abs(b)), `${a} ≉ ${b}`);

test("peak hold falls 0.0045 per 30 Hz frame", () => {
  close(PEAK_HOLD_DECAY_PER_MS * REFERENCE_FRAME_MS, 0.0045);
});

/** One second of silence at `rate` frames per second, from a ceiling of 1. */
const run = (rate: number) => {
  let c = 1;
  const dt = 1000 / rate;
  for (let i = 0; i < rate; i++) c = decayCeiling(c, 0, dt);
  return c;
};

test("ceiling decay is rate independent", () => {
  // One second of silence at 30 Hz vs 60 Hz vs 5 Hz ends at the same ceiling.
  const at30 = run(30);
  close(at30, Math.pow(0.995, 30));
  close(run(60), at30);
  close(run(5), at30);
  // A louder frame lifts it at once.
  assert.strictEqual(decayCeiling(0.5, 2, 33), 2);
});

test("decay step: first frame is one reference frame, stalls are capped", () => {
  close(decayStepMs(1000, 0), REFERENCE_FRAME_MS);
  assert.strictEqual(decayStepMs(1050, 1000), 50);
  assert.strictEqual(decayStepMs(90_000, 1000), 1000);
  assert.strictEqual(decayStepMs(1000, 1000), 0);
});

test("units are computed once per frame and scale", () => {
  const cache = new FrameUnitsCache();
  const frame = { bins: new Float32Array([0, -45, -90, -120]), receivedAt: 1 };
  const a = cache.units(frame, "db", 90);
  assert.deepStrictEqual(Array.from(a), [1, 0.5, 0, 0]);
  // Same frame and scale: the very same buffer, no recompute.
  const snapshot = Array.from(a);
  a[0] = 42;
  assert.strictEqual(cache.units(frame, "db", 90)[0], 42);
  a[0] = snapshot[0];
  // A scale change recomputes.
  assert.deepStrictEqual(Array.from(cache.units(frame, "db", 180)), [
    1,
    0.75,
    0.5,
    Math.fround(1 / 3),
  ]);
});

test("linear ceiling moves once per new frame, not per view", () => {
  const cache = new FrameUnitsCache();
  const f1 = { bins: new Float32Array([2, 1]), receivedAt: 100 };
  cache.units(f1, "off", 90);
  assert.strictEqual(cache.scale.ceiling, 2);
  // Re-reading the same frame under another scale does not decay it.
  cache.units(f1, "db", 90);
  cache.units(f1, "off", 90);
  assert.strictEqual(cache.scale.ceiling, 2);
  const f2 = {
    bins: new Float32Array([0, 0]),
    receivedAt: 100 + REFERENCE_FRAME_MS,
  };
  cache.units(f2, "off", 90);
  close(cache.scale.ceiling, 2 * 0.995);
});
