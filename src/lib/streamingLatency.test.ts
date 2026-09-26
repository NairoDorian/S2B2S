import { describe, expect, it } from "bun:test";
import {
  effectiveLatencyMs,
  isContinuousLatency,
  latencyStops,
  stopForPreset,
} from "./streamingLatency";

// The same numbers `latency_points_match_the_trained_menus` pins on the Rust
// side: a drift between the two would show one latency and stream another.
describe("streamingLatency", () => {
  it("states each Nemotron setting in ms of its trained menu", () => {
    expect(
      latencyStops("nemotron_3_5_cache_aware").map((s) => s.latencyMs),
    ).toEqual([80, 320, 560, 1120]);
    expect(
      latencyStops("nemotron_3_5_cache_aware").map((s) => s.lookaheadMs),
    ).toEqual([0, 240, 480, 1040]);
    expect(
      latencyStops("nemotron_speech_cache_aware").map((s) => s.latencyMs),
    ).toEqual([80, 160, 560, 1120]);
    expect(latencyStops("parakeet_buffered").map((s) => s.latencyMs)).toEqual([
      320, 480, 1120, 2080,
    ]);
  });

  it("orders stops lowest latency first", () => {
    for (const kind of [
      "nemotron_3_5_cache_aware",
      "nemotron_speech_cache_aware",
      "parakeet_buffered",
    ] as const) {
      const ms = latencyStops(kind).map((s) => s.latencyMs);
      expect(ms).toEqual(ms.toSorted((a, b) => a - b));
    }
  });

  it("treats R2T2 as continuous and has no stops for it", () => {
    expect(isContinuousLatency("r2t2_chunk_ms")).toBe(true);
    expect(latencyStops("r2t2_chunk_ms")).toEqual([]);
    expect(effectiveLatencyMs("r2t2_chunk_ms", "fastest", 160)).toBe(160);
    expect(effectiveLatencyMs("r2t2_chunk_ms", "fastest", 9000)).toBe(2000);
  });

  it("resolves a preset to its ms, and nothing without a control", () => {
    expect(stopForPreset("nemotron_3_5_cache_aware", "fast")?.latencyMs).toBe(
      320,
    );
    expect(effectiveLatencyMs("nemotron_speech_cache_aware", "fast", 0)).toBe(
      160,
    );
    expect(effectiveLatencyMs(null, "fast", 320)).toBeNull();
  });
});
