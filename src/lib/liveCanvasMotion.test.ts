import { expect, test } from "bun:test";
import {
  LIVE_CANVAS_MOTION_DEFAULTS,
  liveCanvasTiming,
  normalizeLiveCanvasMotion,
} from "./liveCanvasMotion";

test("character starts with exactly 25% of particle opacity remaining", () => {
  for (const speed of [0.25, 1, 4]) {
    const timing = liveCanvasTiming({ ...LIVE_CANVAS_MOTION_DEFAULTS, speed });
    expect(
      1 - (timing.characterStart - timing.travel) / timing.particleFade,
    ).toBeCloseTo(0.25);
    expect(timing.duration).toBeGreaterThanOrEqual(
      timing.travel + timing.particleFade,
    );
    expect(timing.duration).toBeGreaterThanOrEqual(
      timing.characterStart + timing.characterFade,
    );
  }
});

test("overall speed scales the independent parts without changing handoff", () => {
  const normal = liveCanvasTiming(LIVE_CANVAS_MOTION_DEFAULTS);
  const faster = liveCanvasTiming({ ...LIVE_CANVAS_MOTION_DEFAULTS, speed: 2 });
  expect(faster.travel).toBe(normal.travel / 2);
  expect(faster.particleFade).toBe(normal.particleFade / 2);
  expect(faster.characterFade).toBe(normal.characterFade / 2);
  const independent = liveCanvasTiming({
    ...LIVE_CANVAS_MOTION_DEFAULTS,
    character_fade_ms: 200,
  });
  expect(independent.characterStart).toBe(normal.characterStart);
  expect(independent.travel).toBe(normal.travel);
});

test("corrupt or older saved preferences retain usable defaults", () => {
  expect(
    normalizeLiveCanvasMotion({ speed: NaN, travel_ms: Infinity }),
  ).toEqual(LIVE_CANVAS_MOTION_DEFAULTS);
  expect(
    normalizeLiveCanvasMotion({ speed: 0, particle_fade_ms: -20 }).speed,
  ).toBe(0.25);
  expect(
    normalizeLiveCanvasMotion({ speed: 0, particle_fade_ms: -20 })
      .particle_fade_ms,
  ).toBe(2);
});
