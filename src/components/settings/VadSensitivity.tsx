import React from "react";
import { useTranslation } from "react-i18next";
import { Slider } from "../ui/Slider";
import { useSettings } from "../../hooks/useSettings";

interface VadSensitivityProps {
  descriptionMode?: "tooltip" | "inline";
  grouped?: boolean;
}

/** Mirrors `DEFAULT_VAD_THRESHOLD_EARSHOT` in `src-tauri/src/settings.rs`. */
const DEFAULT_THRESHOLD = 0.5;
const MIN_THRESHOLD = 0.05;
const MAX_THRESHOLD = 0.95;

/**
 * Speech-probability threshold of the Earshot VAD. Lower threshold = more
 * sensitive (more borderline audio kept); higher = more aggressive silence
 * removal.
 */
export const VadSensitivity: React.FC<VadSensitivityProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    const vadEnabled = getSetting("vad_enabled") ?? true;
    const value = getSetting("vad_threshold_earshot") ?? DEFAULT_THRESHOLD;

    if (!vadEnabled) return null;

    return (
      <Slider
        value={value}
        onChange={(next) =>
          updateSetting(
            "vad_threshold_earshot",
            Math.min(
              MAX_THRESHOLD,
              Math.max(MIN_THRESHOLD, Math.round(next * 100) / 100),
            ),
          )
        }
        min={MIN_THRESHOLD}
        max={MAX_THRESHOLD}
        step={0.05}
        label={t("settings.advanced.vadSensitivity.label")}
        description={t("settings.advanced.vadSensitivity.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
        formatValue={(v) => v.toFixed(2)}
        onReset={() =>
          updateSetting("vad_threshold_earshot", DEFAULT_THRESHOLD)
        }
        disabled={isUpdating("vad_threshold_earshot")}
      />
    );
  },
);

VadSensitivity.displayName = "VadSensitivity";
