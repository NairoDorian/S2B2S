import React from "react";
import { useTranslation } from "react-i18next";
import { Slider } from "../ui/Slider";
import { useSettings } from "../../hooks/useSettings";

interface VadSensitivityProps {
  descriptionMode?: "tooltip" | "inline";
  grouped?: boolean;
}

/** Defaults mirror `DEFAULT_VAD_THRESHOLD_*` in `src-tauri/src/settings.rs`. */
const DEFAULT_THRESHOLD = { silero: 0.3, earshot: 0.5 } as const;
const MIN_THRESHOLD = 0.05;
const MAX_THRESHOLD = 0.95;

/**
 * Speech-probability threshold of the active VAD backend. One slider per
 * backend (only the active one is shown) so switching between Silero and
 * Earshot never carries a threshold tuned for the other's score range.
 *
 * Lower threshold = more sensitive (more borderline audio kept); higher =
 * more aggressive silence removal.
 */
export const VadSensitivity: React.FC<VadSensitivityProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    const vadEnabled = getSetting("vad_enabled") ?? true;
    const backend = getSetting("vad_backend") ?? "silero";
    const key =
      backend === "earshot" ? "vad_threshold_earshot" : "vad_threshold_silero";
    const value = getSetting(key) ?? DEFAULT_THRESHOLD[backend];
    const backendLabel = t(`settings.advanced.vadBackend.options.${backend}`);

    if (!vadEnabled) return null;

    return (
      <Slider
        value={value}
        onChange={(next) =>
          updateSetting(
            key,
            Math.min(
              MAX_THRESHOLD,
              Math.max(MIN_THRESHOLD, Math.round(next * 100) / 100),
            ),
          )
        }
        min={MIN_THRESHOLD}
        max={MAX_THRESHOLD}
        step={0.05}
        label={t("settings.advanced.vadSensitivity.label", {
          backend: backendLabel,
        })}
        description={t("settings.advanced.vadSensitivity.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
        formatValue={(v) => v.toFixed(2)}
        onReset={() => updateSetting(key, DEFAULT_THRESHOLD[backend])}
        disabled={isUpdating(key)}
      />
    );
  },
);

VadSensitivity.displayName = "VadSensitivity";
