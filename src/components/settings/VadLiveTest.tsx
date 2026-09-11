import React, { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { sessionToast as toast } from "@/lib/sessionToast";
import { commands } from "@/bindings";
import { Button } from "../ui/Button";
import { SettingContainer } from "../ui/SettingContainer";
import { useSettings } from "../../hooks/useSettings";
import { VadMeter, VadStatusChip, useVadFrames } from "./VadMeter";

interface VadLiveTestProps {
  descriptionMode?: "tooltip" | "inline";
  grouped?: boolean;
}

/** Mirrors `DEFAULT_VAD_THRESHOLD_EARSHOT` in `src-tauri/src/settings.rs`. */
const DEFAULT_THRESHOLD = 0.5;

/**
 * Live VAD test next to the threshold slider: opens the microphone, runs the
 * detector only (no model is loaded, nothing is transcribed or saved) and
 * shows the raw speech score against the threshold frame by frame, so the
 * slider can be tuned while talking. The slider stays live during the test —
 * the backend swaps the threshold in place on the next frame. The meter
 * itself (`VadMeter`) is shared with the Live FFT page.
 */
export const VadLiveTest: React.FC<VadLiveTestProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    const vadEnabled = getSetting("vad_enabled") ?? true;
    const threshold = getSetting("vad_threshold_earshot") ?? DEFAULT_THRESHOLD;
    const denoise = getSetting("denoise_enabled") ?? false;

    const [running, setRunning] = useState(false);
    const [starting, setStarting] = useState(false);
    const runningRef = useRef(false);
    runningRef.current = running;
    // Per-frame reports while the test runs; `stalled` flags a backend that
    // ended it without us (cancel hotkey, safety timeout).
    const frames = useVadFrames(running);

    const stop = useCallback(async () => {
      setRunning(false);
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
      setRunning(true);
    }, [t]);

    // Never leave the microphone open when the page goes away.
    useEffect(
      () => () => {
        if (runningRef.current) void commands.stopVadTest();
      },
      [],
    );

    if (!vadEnabled) return null;

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
            {running && <VadStatusChip frames={frames} />}
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
            <VadMeter frames={frames} threshold={threshold}>
              {/* Noise suppression applies on the next chunk, so flipping it
                  here changes the score and level bars immediately. */}
              <div className="flex items-center justify-between gap-3 text-xs text-text/70">
                <span>
                  {t("settings.advanced.vadLiveTest.noiseSuppression")}
                </span>
                <button
                  type="button"
                  role="switch"
                  aria-checked={denoise}
                  disabled={isUpdating("denoise_enabled")}
                  onClick={() => updateSetting("denoise_enabled", !denoise)}
                  className={`px-2 py-0.5 rounded-full text-xs font-medium border transition-colors cursor-pointer disabled:opacity-50 ${
                    denoise
                      ? "bg-logo-primary/20 text-text border-logo-primary/40"
                      : "bg-mid-gray/10 text-text/60 border-mid-gray/20"
                  }`}
                >
                  {denoise
                    ? t("settings.advanced.vadLiveTest.noiseSuppressionOn")
                    : t("settings.advanced.vadLiveTest.noiseSuppressionOff")}
                </button>
              </div>
              <p className="text-xs text-text/50">
                {t("settings.advanced.vadLiveTest.hint")}
              </p>
            </VadMeter>
          )}
        </div>
      </SettingContainer>
    );
  },
);

VadLiveTest.displayName = "VadLiveTest";
