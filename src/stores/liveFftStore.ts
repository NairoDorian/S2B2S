import { invoke } from "@tauri-apps/api/core";
import { createSolidStore } from "@/lib/solidStore";
import { commands, events, type LiveFftStatus } from "@/bindings";
import {
  decodeFftFrame,
  framePollPeriodMs,
  type SpectralFeatures,
} from "@/components/settings/live-fft/liveFftFrame";

/**
 * One analysed spectrum frame, as the canvases consume it. Frames bypass
 * the reactive store entirely: at up to 60 Hz a store update per frame would
 * re-render the whole page. The canvases subscribe with `subscribeFrames`
 * and read `getLatestFrame()` when they paint.
 */
export interface SpectrumFrame {
  seq: number;
  /** Zero-copy view on the `live_fft_frame` reply. */
  bins: Float32Array;
  peakHz: number;
  peakValue: number;
  silent: boolean;
  dspUs: number;
  /** performance.now() when the frame arrived. */
  receivedAt: number;
  /** `LiveFftAxis.version` the bins are laid out on. */
  axisVersion: number;
  features: SpectralFeatures | null;
}

/** Every output bin's centre frequency, as `live_fft_axis` returns it. */
export interface SpectrumAxis {
  version: number;
  hz: Float32Array;
}

const EMPTY_AXIS: SpectrumAxis = { version: 0, hz: new Float32Array(0) };

const IDLE_FFT_STATUS: LiveFftStatus = {
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
  axis_version: 0,
  hz_per_bin: 0,
  aggregated_bins: 0,
  kaiser_beta: 0,
  analysis_latency_ms: 0,
  raw_bins: false,
};

export const isFftActive = (status: LiveFftStatus): boolean =>
  status.phase !== "idle" && status.phase !== "error";

// ---- Frame poll ---------------------------------------------------------------
//
// Frames are pulled, not pushed: `live_fft_frame` is a raw-bytes command (no
// JSON, no copy into a JS array), polled at twice the analysis rate with one
// request in flight. `knownSeq` makes the backend answer an unchanged frame
// with its 32-byte header alone. The poll runs only while a session is
// active, the view is not frozen and the page is visible.

let latestFrame: SpectrumFrame | null = null;
let lastFrameAt = 0;
let frozen = false;
let knownSeq: number | null = null;
let pollTimer: ReturnType<typeof setTimeout> | null = null;
let pollInFlight = false;
let axisInFlight: number | null = null;
// Mirrors of what the poll decides on. The Solid store stages writes until
// the next microtask flush, so `get()` right after `set()` still reads the
// previous value; the poll must not start or stop on a stale status.
let currentStatus: LiveFftStatus | null = null;
let currentAxisVersion = 0;
let initialized = false;
const frameListeners = new Set<() => void>();

export const getLatestFrame = (): SpectrumFrame | null => latestFrame;
/** Wall-clock time of the last new frame (or of the last unfreeze). */
export const getLastFrameAt = (): number => lastFrameAt;

/** Called after each new frame; returns the unsubscribe. */
export const subscribeFrames = (listener: () => void): (() => void) => {
  frameListeners.add(listener);
  return () => {
    frameListeners.delete(listener);
  };
};

interface LiveFftStore {
  status: LiveFftStatus;
  /** Frequency of every output bin; replaced only when its version moves. */
  axis: SpectrumAxis;
  initialized: boolean;
  /** Hold the last frame on screen; the poll pauses meanwhile. */
  frozen: boolean;

  initialize: () => Promise<void>;
  start: () => Promise<string | null>;
  stop: () => Promise<string | null>;
  reset: () => Promise<void>;
  setFrozen: (frozen: boolean) => void;
}

const pageVisible = () =>
  typeof document === "undefined" || document.visibilityState !== "hidden";

const stopPoll = () => {
  if (pollTimer !== null) clearTimeout(pollTimer);
  pollTimer = null;
};

const shouldPoll = () =>
  currentStatus !== null &&
  isFftActive(currentStatus) &&
  !frozen &&
  pageVisible();

const liveFftState = createSolidStore<LiveFftStore>((set) => {
  const fetchAxis = async (version: number) => {
    if (axisInFlight === version) return;
    axisInFlight = version;
    try {
      const result = await commands.liveFftAxis();
      if (result.status === "ok") {
        currentAxisVersion = result.data.version;
        set({
          axis: {
            version: result.data.version,
            hz: Float32Array.from(result.data.hz, (v) => v ?? 0),
          },
        });
      } else {
        console.error("Failed to read the Live FFT axis:", result.error);
      }
    } catch (error) {
      console.error("Failed to read the Live FFT axis:", error);
    } finally {
      if (axisInFlight === version) axisInFlight = null;
    }
  };

  const accept = (buffer: ArrayBuffer) => {
    const frame = decodeFftFrame(buffer);
    // No frame yet, an unchanged one (header-only reply) or a leftover of a
    // stopped session: nothing new to draw.
    if (!frame || frame.seq === 0 || frame.seq === knownSeq) return;
    if (!frame.active) return;
    knownSeq = frame.seq;
    lastFrameAt = Date.now();
    latestFrame = {
      seq: frame.seq,
      bins: frame.bins,
      peakHz: frame.peakHz,
      peakValue: frame.peakValue,
      silent: frame.silent,
      dspUs: frame.dspUs,
      receivedAt: performance.now(),
      axisVersion: frame.axisVersion,
      features: frame.features,
    };
    if (frame.axisVersion !== currentAxisVersion) {
      void fetchAxis(frame.axisVersion);
    }
    for (const listener of frameListeners) listener();
  };

  const poll = async () => {
    pollTimer = null;
    if (pollInFlight || !shouldPoll()) return;
    pollInFlight = true;
    try {
      const buffer = await invoke<ArrayBuffer>("live_fft_frame", {
        knownSeq,
      });
      if (!frozen) accept(buffer);
    } catch (error) {
      // Only fails while the app is shutting down; keep polling quietly.
      console.debug("live_fft_frame failed:", error);
    } finally {
      pollInFlight = false;
    }
    schedulePoll();
  };

  function schedulePoll() {
    if (pollTimer !== null || pollInFlight) return;
    if (!shouldPoll()) return;
    const rate = currentStatus?.update_rate_hz || 30;
    pollTimer = setTimeout(poll, framePollPeriodMs(rate));
  }

  /** Start or stop the poll after anything it depends on changed. */
  const syncPoll = () => {
    if (shouldPoll()) schedulePoll();
    else stopPoll();
  };

  const setStatus = (status: LiveFftStatus) => {
    const wasActive = currentStatus !== null && isFftActive(currentStatus);
    currentStatus = status;
    set({ status });
    if (!wasActive && isFftActive(status)) {
      // A new session numbers its frames from scratch.
      knownSeq = null;
    }
    syncPoll();
  };

  return {
    status: IDLE_FFT_STATUS,
    axis: EMPTY_AXIS,
    initialized: false,
    frozen: false,

    initialize: async () => {
      if (initialized) return;
      initialized = true;
      set({ initialized: true });
      if (typeof document !== "undefined") {
        document.addEventListener("visibilitychange", syncPoll);
      }
      await events.liveFftStateEvent.listen((event) => {
        setStatus(event.payload.status);
      });
      try {
        setStatus(await commands.liveFftStatus());
      } catch (error) {
        console.error("Failed to read Live FFT status:", error);
      }
    },

    start: async () => {
      latestFrame = null;
      knownSeq = null;
      lastFrameAt = Date.now();
      frozen = false;
      set({ frozen: false });
      for (const listener of frameListeners) listener();
      const result = await commands.liveFftStart();
      if (result.status === "error") return result.error;
      syncPoll();
      return null;
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
      // The stall detector measures frames against the last one received;
      // a thawed view has not been waiting for any.
      if (!next) lastFrameAt = Date.now();
      set({ frozen: next });
      syncPoll();
    },
  };
});

export function useLiveFftStore() {
  return liveFftState;
}

export const initialize = () => useLiveFftStore().initialize();
export const start = () => useLiveFftStore().start();
export const stop = () => useLiveFftStore().stop();
export const reset = () => useLiveFftStore().reset();
export const setFrozen = (nextFrozen: boolean) =>
  useLiveFftStore().setFrozen(nextFrozen);
