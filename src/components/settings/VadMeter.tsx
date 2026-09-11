import React, { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { events, type VadTestEvent } from "@/bindings";

/** Frames arrive ~31×/s; a gap this long means the backend stopped sending. */
const STALL_MS = 1500;
/** How fast the peak marker on the score meter falls back (per update). */
const PEAK_DECAY = 0.02;

export interface VadFrames {
  frame: VadTestEvent | null;
  /** Slowly falling peak of the speech score, for the marker. */
  peak: number;
  /** No frame for a while: the recording ended elsewhere. */
  stalled: boolean;
}

/**
 * Subscribe to the detector's per-frame reports (`VadTestEvent`) while
 * `active`, and notice when they stop. Shared by the Advanced page's live
 * test and the Live FFT page's voice-detection view; the backend decides
 * which session emits.
 */
export function useVadFrames(active: boolean): VadFrames {
  const [frame, setFrame] = useState<VadTestEvent | null>(null);
  const [peak, setPeak] = useState(0);
  const [stalled, setStalled] = useState(false);
  const lastEventAt = useRef(0);

  useEffect(() => {
    if (!active) {
      setFrame(null);
      setPeak(0);
      setStalled(false);
      return;
    }
    lastEventAt.current = Date.now();
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    events.vadTestEvent
      .listen((event) => {
        lastEventAt.current = Date.now();
        setStalled(false);
        setFrame(event.payload);
        const score = event.payload.score ?? 0;
        setPeak((prev) => Math.max(score, prev - PEAK_DECAY));
      })
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      });
    const watchdog = setInterval(() => {
      if (Date.now() - lastEventAt.current > STALL_MS) setStalled(true);
    }, 500);
    return () => {
      cancelled = true;
      unlisten?.();
      clearInterval(watchdog);
    };
  }, [active]);

  return { frame, peak, stalled };
}

const pct = (v: number) => `${Math.round(Math.min(1, Math.max(0, v)) * 100)}%`;

interface VadStatusChipProps {
  frames: VadFrames;
}

/** Speech / Silence / No signal, coloured. */
export const VadStatusChip: React.FC<VadStatusChipProps> = ({ frames }) => {
  const { t } = useTranslation();
  const voiced = frames.frame?.voiced ?? false;
  return (
    <span
      className={`px-2 py-0.5 rounded-full text-xs font-medium border transition-colors ${
        frames.stalled
          ? "bg-amber-500/10 text-amber-400 border-amber-500/20"
          : voiced
            ? "bg-emerald-500/15 text-emerald-400 border-emerald-500/30"
            : "bg-mid-gray/10 text-text/60 border-mid-gray/20"
      }`}
    >
      {frames.stalled
        ? t("settings.advanced.vadLiveTest.noSignal")
        : voiced
          ? t("settings.advanced.vadLiveTest.speech")
          : t("settings.advanced.vadLiveTest.silence")}
    </span>
  );
};

interface VadMeterProps {
  frames: VadFrames;
  /** The hysteresis threshold, drawn as the marker on the score bar. */
  threshold: number;
  /**
   * RNNoise's gate threshold, drawn on its probability bar (shown only while
   * suppression runs on the analysed audio). 0 = the gate is off.
   */
  denoiseThreshold?: number;
  children?: React.ReactNode;
}

/**
 * The detector's verdict, frame by frame: speech score against the threshold
 * marker (with a falling peak), input level, whether the frame would have
 * reached a model, and RNNoise's own speech probability when the suppressor
 * ran. `children` render below the bars (page-specific controls such as the
 * noise-suppression toggle).
 */
export const VadMeter: React.FC<VadMeterProps> = ({
  frames,
  threshold,
  denoiseThreshold = 0,
  children,
}) => {
  const { t } = useTranslation();
  const score = frames.frame?.score ?? null;
  const voiced = frames.frame?.voiced ?? false;
  const kept = frames.frame?.kept ?? false;
  const level = frames.frame?.level ?? 0;
  const denoiseProb = frames.frame?.denoise_prob ?? null;

  return (
    <div className="flex flex-col gap-2 rounded-md p-3 bg-card/40 border border-mid-gray/15">
      <div className="flex items-center justify-between text-xs text-text/70">
        <span>
          {t("settings.advanced.vadLiveTest.score", {
            score: score === null ? "—" : score.toFixed(2),
          })}
        </span>
        <span>
          {t("settings.advanced.vadLiveTest.threshold", {
            threshold: threshold.toFixed(2),
          })}
        </span>
      </div>
      {/* Speech score against the threshold marker */}
      <div className="relative h-3 rounded-full bg-mid-gray/20 overflow-hidden">
        <div
          className={`h-full rounded-full transition-[width] duration-75 ${
            voiced ? "bg-emerald-500" : "bg-mid-gray/60"
          }`}
          style={{ width: pct(score ?? 0) }}
        />
        <div
          className="absolute top-0 h-full w-0.5 bg-text/40"
          style={{ left: pct(frames.peak) }}
        />
        <div
          className="absolute top-0 h-full w-0.5 bg-logo-primary"
          style={{ left: pct(threshold) }}
        />
      </div>
      {/* Input level */}
      <div className="flex items-center gap-2 text-xs text-text/60">
        <span className="w-20 shrink-0">
          {t("settings.advanced.vadLiveTest.level")}
        </span>
        <div className="relative flex-grow h-1.5 rounded-full bg-mid-gray/20 overflow-hidden">
          <div
            className="h-full rounded-full bg-blue-400/70 transition-[width] duration-75"
            style={{ width: pct(level) }}
          />
        </div>
        <span
          className={`shrink-0 px-1.5 py-0.5 rounded text-[10px] font-medium border ${
            kept
              ? "bg-logo-primary/15 text-text border-logo-primary/30"
              : "bg-mid-gray/10 text-text/50 border-mid-gray/20"
          }`}
        >
          {kept
            ? t("settings.advanced.vadLiveTest.kept")
            : t("settings.advanced.vadLiveTest.dropped")}
        </span>
      </div>
      {/* RNNoise's own speech probability, with its gate threshold */}
      {denoiseProb !== null && (
        <div className="flex items-center gap-2 text-xs text-text/60">
          <span className="w-20 shrink-0">
            {t("settings.advanced.vadLiveTest.denoiseProb")}
          </span>
          <div className="relative flex-grow h-1.5 rounded-full bg-mid-gray/20 overflow-hidden">
            <div
              className="h-full rounded-full bg-violet-400/70 transition-[width] duration-75"
              style={{ width: pct(denoiseProb) }}
            />
            {denoiseThreshold > 0 && (
              <div
                className="absolute top-0 h-full w-0.5 bg-logo-primary"
                style={{ left: pct(denoiseThreshold) }}
              />
            )}
          </div>
          <span className="shrink-0 w-10 text-right tabular-nums">
            {denoiseProb.toFixed(2)}
          </span>
        </div>
      )}
      {children}
    </div>
  );
};
