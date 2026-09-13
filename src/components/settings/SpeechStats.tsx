import { Show } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import { Slider } from "../ui/Slider";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";
import type { JSX } from "@solidjs/web";

interface SpeechStatsProps {
  descriptionMode?: "tooltip" | "inline";
  grouped?: boolean;
}

/**
 * Live speech statistics in the recording overlay: a speech/silence indicator,
 * a timer that runs only while you are actually talking, and the running
 * average words per minute.
 *
 * Lives on the Overlay page with the rest of the overlay's controls. The
 * pause tolerance is a VAD parameter (it decides how long silence has to
 * last before the speech timer stops counting), which is why the detector's
 * own threshold stays on the Advanced page.
 */
export const SpeechStats = (props: SpeechStatsProps): JSX.Element => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  // Everything reads live: the toggle, the slider's value, and the
  // overlay-hidden guard all follow the setting as it changes.
  return (
    <Show when={getSetting("overlay_style") !== "none"}>
      <>
        <ToggleSwitch
          checked={getSetting("overlay_speech_stats") ?? true}
          onChange={(value) => updateSetting("overlay_speech_stats", value)}
          isUpdating={isUpdating("overlay_speech_stats")}
          label={t("settings.advanced.speechStats.title")}
          description={t("settings.advanced.speechStats.description")}
          descriptionMode={props.descriptionMode}
          grouped={props.grouped}
        />

        <Show when={getSetting("overlay_speech_stats") ?? true}>
          <Slider
            value={getSetting("speech_pause_hold_ms") ?? 500}
            onChange={(value) =>
              updateSetting("speech_pause_hold_ms", Math.round(value))
            }
            min={10}
            max={2000}
            step={10}
            label={t("settings.advanced.speechStats.pauseHold.label")}
            description={t(
              "settings.advanced.speechStats.pauseHold.description",
            )}
            descriptionMode={props.descriptionMode}
            grouped={props.grouped}
            formatValue={(v) => `${Math.round(v)} ms`}
            onReset={() => updateSetting("speech_pause_hold_ms", 500)}
            disabled={isUpdating("speech_pause_hold_ms")}
          />
        </Show>
      </>
    </Show>
  );
};
