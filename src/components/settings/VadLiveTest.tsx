import React, { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { commands, events, type VadTestEvent } from "@/bindings";
import { Button } from "../ui/Button";
import { SettingContainer } from "../ui/SettingContainer";
import { useSettings } from "../../hooks/useSettings";

interface VadLiveTestProps {
  descriptionMode?: "tooltip" | "inline";
  grouped?: boolean;
}

/** Mirrors `DEFAULT_VAD_THRESHOLD_EARSHOT` in `src-tauri/src/settings.rs`. */
const DEFAULT_THRESHOLD = 0.5;
/** Frames arrive ~31×/s; a gap this long means the backend stopped the test. */
const STALL_MS = 1500;
/** How fast the peak marker on the score meter falls back (per update). */
const PEAK_DECAY = 0.02;

/**
 * Live VAD test next to the threshold slider: opens the microphone, runs the
 * detector only (no model is loaded, nothing is transcribed or saved) and
 * shows the raw speech score against the threshold frame by frame, so the
 * slider can be tuned while talking. The slider stays live during the test —
 * the backend swaps the threshold in place on the next frame.
 */
export const VadLiveTest: React.FC<VadLiveTestProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting } = useSettings();

    const vadEnabled = getSetting("vad_enabled") ?? true;
    const threshold = getSetting("vad_threshold_earshot") ?? DEFAULT_THRESHOLD;

    const [running, setRunning] = useState(false);
    const [starting, setStarting] = useState(false);
    const [frame, setFrame] = useState<VadTestEvent | null>(null);
    const [peak, setPeak] = useState(0);
    const [stalled, setStalled] = useState(false);
    const lastEventAt = useRef(0);
    const runningRef = useRef(false);
    runningRef.current = running;

    const stop = useCallback(async () => {
      setRunning(false);
      setFrame(null);
      setPeak(0);
      setStalled(false);
      const result = await commands.stopVadTest();
      if (result.status === "error") {
        toast.error(result.error);
      }
    }, []);

    const start = useCallback(async () => {
      setStarting(true);
      const result = await commands.startVadTest();
      setStarting(false);
      if (result.status === "error") {
        toast.error(
          t("settings.advanced.vadLiveTest.errorStart", {
            error: result.error,
          }),
        );
        return;
      }
      lastEventAt.current = Date.now();
      setStalled(false);
      setRunning(true);
    }, [t]);

    // Subscribe to per-frame reports while the test runs, and notice when the
    // backend ended it without us (cancel hotkey, safety timeout).
    useEffect(() => {
      if (!running) return;
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
    }, [running]);

    // Never leave the microphone open when the page goes away.
    useEffect(
      () => () => {
        if (runningRef.current) void commands.stopVadTest();
      },
      [],
    );

    if (!vadEnabled) return null;

    const score = frame?.score ?? null;
    const voiced = frame?.voiced ?? false;
    const kept = frame?.kept ?? false;
    const level = frame?.level ?? 0;
    const pct = (v: number) =>
      `${Math.round(Math.min(1, Math.max(0, v)) * 100)}%`;

    return (
      <SettingContainer
        title={t("settings.advanced.vadLiveTest.title")}
        description={t("settings.advanced.vadLiveTest.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
        layout={running ? "stacked" : "horizontal"}
      >
        <div className="w-full flex flex-col gap-3">
          <div className="flex items-center gap-3 justify-end">
            {running && (
              <span
                className={`px-2 py-0.5 rounded-full text-xs font-medium border transition-colors ${
                  stalled
                    ? "bg-amber-500/10 text-amber-400 border-amber-500/20"
                    : voiced
                      ? "bg-emerald-500/15 text-emerald-400 border-emerald-500/30"
                      : "bg-mid-gray/10 text-text/60 border-mid-gray/20"
                }`}
              >
                {stalled
                  ? t("settings.advanced.vadLiveTest.noSignal")
                  : voiced
                    ? t("settings.advanced.vadLiveTest.speech")
                    : t("settings.advanced.vadLiveTest.silence")}
              </span>
            )}
            <Button
              variant={running ? "danger-ghost" : "secondary"}
              size="sm"
              onClick={running ? stop : start}
              disabled={starting}
            >
              {running
                ? t("settings.advanced.vadLiveTest.stop")
                : t("settings.advanced.vadLiveTest.start")}
            </Button>
          </div>

          {running && (
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
                  style={{ left: pct(peak) }}
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
              <p className="text-xs text-text/50">
                {t("settings.advanced.vadLiveTest.hint")}
              </p>
            </div>
          )}
        </div>
      </SettingContainer>
    );
  },
);

VadLiveTest.displayName = "VadLiveTest";
