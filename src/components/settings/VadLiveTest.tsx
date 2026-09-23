import { createSignal, createEffect, Show } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import { sessionToast as toast } from "@/lib/sessionToast";
import { commands } from "@/bindings";
import { Button } from "../ui/Button";
import { SettingContainer } from "../ui/SettingContainer";
import { useSettings } from "../../hooks/useSettings";
import { VadMeter, VadStatusChip, useVadFrames } from "./VadMeter";
import { DEFAULT_THRESHOLD } from "./VadSensitivity";
import type { JSX } from "@solidjs/web";

interface VadLiveTestProps {
  descriptionMode?: "tooltip" | "inline";
  grouped?: boolean;
}

/**
 * Live VAD test next to the threshold slider: opens the microphone, runs the
 * detector only (no model is loaded, nothing is transcribed or saved) and
 * shows the raw speech score against the threshold frame by frame, so the
 * slider can be tuned while talking. The slider stays live during the test —
 * the backend swaps the threshold in place on the next frame. The meter
 * itself (`VadMeter`) is shared with the Live FFT page.
 */
export const VadLiveTest = (props: VadLiveTestProps): JSX.Element => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  const vadEnabled = () => getSetting("vad_enabled") ?? true;
  const threshold = () =>
    getSetting("vad_threshold_earshot") ?? DEFAULT_THRESHOLD;
  const denoise = () => getSetting("denoise_enabled") ?? false;

  const [running, setRunning] = createSignal(false);
  const [starting, setStarting] = createSignal(false);
  const frames = useVadFrames(running);

  const stop = async () => {
    setRunning(false);
    const result = await commands.stopVadTest();
    if (result.status === "error") {
      toast.error(result.error);
    }
  };

  const start = async () => {
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
  };

  createEffect(
    () => undefined,
    () => {
      return () => {
        if (running()) void commands.stopVadTest();
      };
    },
  );

  return (
    <Show when={vadEnabled()}>
      <SettingContainer
        title={t("settings.advanced.vadLiveTest.title")}
        description={t("settings.advanced.vadLiveTest.description")}
        descriptionMode={props.descriptionMode ?? "tooltip"}
        grouped={props.grouped ?? false}
        layout={running() ? "stacked" : "horizontal"}
      >
        <div class="w-full flex flex-col gap-3">
          <div class="flex items-center gap-3 justify-end">
            {running() && <VadStatusChip frames={frames} />}
            <Button
              variant={running() ? "danger-ghost" : "secondary"}
              size="sm"
              onClick={running() ? stop : start}
              disabled={starting()}
            >
              {running()
                ? t("settings.advanced.vadLiveTest.stop")
                : t("settings.advanced.vadLiveTest.start")}
            </Button>
          </div>

          {running() && (
            <VadMeter frames={frames} threshold={threshold()}>
              {/* Noise suppression applies on the next chunk, so flipping it
                  here changes the score and level bars immediately. */}
              <div class="flex items-center justify-between gap-3 text-xs text-text/70">
                <span>
                  {t("settings.advanced.vadLiveTest.noiseSuppression")}
                </span>
                <button
                  type="button"
                  role="switch"
                  aria-checked={denoise() ? "true" : "false"}
                  disabled={isUpdating("denoise_enabled")}
                  onClick={() => updateSetting("denoise_enabled", !denoise())}
                  class={`px-2 py-0.5 rounded-full text-xs font-medium border transition-colors cursor-pointer disabled:opacity-50 ${
                    denoise()
                      ? "bg-accent/20 text-text border-accent/40"
                      : "bg-mid-gray/10 text-text/60 border-mid-gray/20"
                  }`}
                >
                  {denoise()
                    ? t("settings.advanced.vadLiveTest.noiseSuppressionOn")
                    : t("settings.advanced.vadLiveTest.noiseSuppressionOff")}
                </button>
              </div>
              <p class="text-xs text-text/50">
                {t("settings.advanced.vadLiveTest.hint")}
              </p>
            </VadMeter>
          )}
        </div>
      </SettingContainer>
    </Show>
  );
};
