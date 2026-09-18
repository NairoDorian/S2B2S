import { createSolidStore } from "@/lib/solidStore";
import {
  commands,
  events,
  type LiveFftFrameEvent,
  type LiveFftStatus,
} from "@/bindings";

/**
 * One analysed spectrum frame, as the canvases consume it. Frames bypass
 * React state entirely: at up to 60 Hz a store update per frame would
 * re-render the whole page. The canvases read `getLatestFrame()` from
 * their animation loops and subscribe with `subscribeFrames` to know when
 * a new one arrived.
 */
export interface SpectrumFrame {
  seq: number;
  bins: Float32Array;
  peakHz: number;
  peakValue: number;
  silent: boolean;
  dspUs: number;
  receivedAt: number;
}

export const IDLE_FFT_STATUS: LiveFftStatus = {
  phase: "idle",
  error: null,
  stop_reason: null,
  sample_rate: 0,
  fft_size: 0,
  window_samples: 0,
  linear_bins: 0,
  magnitude_bins: 0,
  output_bins: 0,
  identity_warp: false,
  full_scale_ref: 1,
  display_max_hz: 0,
  nyquist_hz: 0,
  update_rate_hz: 0,
  async_analysis: true,
  frames: 0,
  dropped_samples: 0,
  dsp_us_last: 0,
  dsp_us_avg: 0,
  dsp_us_max: 0,
  started_at_ms: null,
  axis_hz: [],
};

export const isFftActive = (status: LiveFftStatus): boolean =>
  status.phase !== "idle" && status.phase !== "error";

type FrameListener = (frame: SpectrumFrame) => void;

let latestFrame: SpectrumFrame | null = null;
let lastFrameAt = 0;
let frozen = false;
const frameListeners = new Set<FrameListener>();

export const getLatestFrame = (): SpectrumFrame | null => latestFrame;
/** Wall-clock time of the last frame received (even while frozen). */
export const getLastFrameAt = (): number => lastFrameAt;

export const subscribeFrames = (listener: FrameListener): (() => void) => {
  frameListeners.add(listener);
  return () => {
    frameListeners.delete(listener);
  };
};

const acceptFrame = (payload: LiveFftFrameEvent) => {
  lastFrameAt = Date.now();
  if (frozen) return;
  const bins = new Float32Array(payload.bins.length);
  for (let i = 0; i < bins.length; i++) bins[i] = payload.bins[i] ?? 0;
  latestFrame = {
    seq: payload.seq,
    bins,
    peakHz: payload.peak_hz ?? 0,
    peakValue: payload.peak_value ?? 0,
    silent: payload.silent,
    dspUs: payload.dsp_us ?? 0,
    receivedAt: lastFrameAt,
  };
  for (const listener of frameListeners) listener(latestFrame);
};

interface LiveFftStore {
  status: LiveFftStatus;
  initialized: boolean;
  /** Hold the last frame on screen while the backend keeps analysing. */
  frozen: boolean;

  initialize: () => Promise<void>;
  start: () => Promise<string | null>;
  stop: () => Promise<string | null>;
  reset: () => Promise<void>;
  setFrozen: (frozen: boolean) => void;
  clearFrame: () => void;
}

const liveFftState = createSolidStore<LiveFftStore>((set, get) => ({
  status: IDLE_FFT_STATUS,
  initialized: false,
  frozen: false,

  initialize: async () => {
    if (get().initialized) return;
    set({ initialized: true });
    await events.liveFftStateEvent.listen((event) => {
      set({ status: event.payload.status });
    });
    await events.liveFftFrameEvent.listen((event) => {
      acceptFrame(event.payload);
    });
    try {
      const status = await commands.liveFftStatus();
      set({ status });
    } catch (error) {
      console.error("Failed to read Live FFT status:", error);
    }
  },

  start: async () => {
    latestFrame = null;
    lastFrameAt = Date.now();
    frozen = false;
    set({ frozen: false });
    const result = await commands.liveFftStart();
    return result.status === "error" ? result.error : null;
  },

  stop: async () => {
    const result = await commands.liveFftStop();
    return result.status === "error" ? result.error : null;
  },

  reset: async () => {
    await commands.liveFftReset();
  },

  setFrozen: (next) => {
    frozen = next;
    set({ frozen: next });
  },

  clearFrame: () => {
    latestFrame = null;
  },
}));

export function useLiveFftStore() {
  return liveFftState;
}

export const initialize = () => useLiveFftStore().initialize();
export const start = () => useLiveFftStore().start();
export const stop = () => useLiveFftStore().stop();
export const reset = () => useLiveFftStore().reset();
export const setFrozen = (nextFrozen: boolean) =>
  useLiveFftStore().setFrozen(nextFrozen);
export const clearFrame = () => useLiveFftStore().clearFrame();
