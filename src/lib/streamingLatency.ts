import type {
  NativeStreamingLatencyKind,
  NativeStreamingLatencyPreset,
} from "@/bindings";

/**
 * Streaming latency, stated in milliseconds for every family.
 *
 * R2T2's latency is a free chunk size, so its control is a continuous range.
 * The FastConformer families (Nemotron 3.5, Nemotron Speech, Parakeet Unified)
 * were trained on a fixed menu of settings; the persisted value stays the
 * preset that selects one, and this table says what each one means in ms so
 * the control can show the number instead of a word.
 *
 * Mirrors `latency_point` in `managers/native_streaming_latency.rs`, whose
 * test pins the same numbers against the fork's model cards. The backend is
 * the authority: this copy only draws the control.
 */

/** One trained setting of a preset family, in ms of audio. */
export interface LatencyStop {
  preset: NativeStreamingLatencyPreset;
  /** Audio the model needs before it can emit a chunk's text. */
  latencyMs: number;
  /** The future-context part of `latencyMs`. */
  lookaheadMs: number;
}

/**
 * Inclusive bounds of the R2T2 streaming chunk size, in whole milliseconds.
 * They mirror `R2T2_CHUNK_MS_MIN` / `R2T2_CHUNK_MS_MAX` in
 * `native_streaming_latency.rs`, which mirror `k_r2t2_chunk_ms_*` in the fork.
 * The settings commands and the native `stream_begin` reject an out-of-range
 * value again: the UI is not a trust boundary.
 */
export const R2T2_CHUNK_MS_MIN = 80;
export const R2T2_CHUNK_MS_MAX = 2000;
export const R2T2_CHUNK_MS_DEFAULT = 320;

/** The preset an absent settings entry resolves to (the model default). */
export const DEFAULT_LATENCY_PRESET: NativeStreamingLatencyPreset = "accurate";

/** Clamp to the native R2T2 range; typed input can hold anything. */
export const clampChunkMs = (ms: number): number =>
  Math.min(R2T2_CHUNK_MS_MAX, Math.max(R2T2_CHUNK_MS_MIN, Math.round(ms)));

/** Encoder frame of the FastConformer streaming variants. */
const FRAME_MS = 80;

/** A Nemotron setting from its trained right context, in encoder frames. */
const nemotronStop = (
  preset: NativeStreamingLatencyPreset,
  right: number,
): LatencyStop => ({
  preset,
  latencyMs: (right + 1) * FRAME_MS,
  lookaheadMs: right * FRAME_MS,
});

/** A Parakeet Unified setting from its `(chunk, right)` in ms. */
const bufferedStop = (
  preset: NativeStreamingLatencyPreset,
  chunkMs: number,
  rightMs: number,
): LatencyStop => ({
  preset,
  latencyMs: chunkMs + rightMs,
  lookaheadMs: rightMs,
});

const STOPS: Record<
  Exclude<NativeStreamingLatencyKind, "r2t2_chunk_ms">,
  LatencyStop[]
> = {
  // Right context {0, 3, 6, 13} frames.
  nemotron_3_5_cache_aware: [
    nemotronStop("fastest", 0),
    nemotronStop("fast", 3),
    nemotronStop("balanced", 6),
    nemotronStop("accurate", 13),
  ],
  // Right context {0, 1, 6, 13} frames.
  nemotron_speech_cache_aware: [
    nemotronStop("fastest", 0),
    nemotronStop("fast", 1),
    nemotronStop("balanced", 6),
    nemotronStop("accurate", 13),
  ],
  parakeet_buffered: [
    bufferedStop("fastest", 160, 160),
    bufferedStop("fast", 160, 320),
    bufferedStop("balanced", 560, 560),
    bufferedStop("accurate", 1040, 1040),
  ],
};

/** Whether the family's latency is R2T2's continuous chunk size. */
export const isContinuousLatency = (
  kind: NativeStreamingLatencyKind | null | undefined,
): kind is "r2t2_chunk_ms" => kind === "r2t2_chunk_ms";

/**
 * The trained settings of a preset family, lowest latency first; empty for
 * R2T2 (continuous) and for a model without a latency control.
 */
export const latencyStops = (
  kind: NativeStreamingLatencyKind | null | undefined,
): LatencyStop[] => (!kind || isContinuousLatency(kind) ? [] : STOPS[kind]);

/** The stop a preset selects, falling back to the model default. */
export const stopForPreset = (
  kind: NativeStreamingLatencyKind | null | undefined,
  preset: NativeStreamingLatencyPreset,
): LatencyStop | undefined => {
  const stops = latencyStops(kind);
  return (
    stops.find((stop) => stop.preset === preset) ??
    stops.find((stop) => stop.preset === DEFAULT_LATENCY_PRESET)
  );
};

/**
 * The latency in ms a model streams at under the given settings, or `null`
 * when it has no latency control.
 */
export const effectiveLatencyMs = (
  kind: NativeStreamingLatencyKind | null | undefined,
  preset: NativeStreamingLatencyPreset,
  chunkMs: number,
): number | null => {
  if (!kind) return null;
  if (isContinuousLatency(kind)) return clampChunkMs(chunkMs);
  return stopForPreset(kind, preset)?.latencyMs ?? null;
};
