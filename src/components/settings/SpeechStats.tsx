import React from "react";
import { useTranslation } from "react-i18next";
import { Slider } from "../ui/Slider";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";

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
export const SpeechStats: React.FC<SpeechStatsProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    const enabled = getSetting("overlay_speech_stats") ?? true;
    const pauseHold = getSetting("speech_pause_hold_ms") ?? 500;
    // Nothing to measure into when the overlay is hidden.
    const overlayHidden = getSetting("overlay_style") === "none";

    if (overlayHidden) return null;

    return (
      <>
        <ToggleSwitch
          checked={enabled}
          onChange={(value) => updateSetting("overlay_speech_stats", value)}
          isUpdating={isUpdating("overlay_speech_stats")}
          label={t("settings.advanced.speechStats.title")}
          description={t("settings.advanced.speechStats.description")}
          descriptionMode={descriptionMode}
          grouped={grouped}
        />

        {enabled && (
          <Slider
            value={pauseHold}
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
            descriptionMode={descriptionMode}
            grouped={grouped}
            formatValue={(v) => `${Math.round(v)} ms`}
            onReset={() => updateSetting("speech_pause_hold_ms", 500)}
            disabled={isUpdating("speech_pause_hold_ms")}
          />
        )}
      </>
    );
  },
);
