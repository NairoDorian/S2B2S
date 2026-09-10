import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";

interface NoiseSuppressionProps {
  descriptionMode?: "tooltip" | "inline";
  grouped?: boolean;
}

/**
 * RNNoise suppression on the microphone path, ahead of the VAD and the model.
 * Toggling applies on the next captured chunk, so the live VAD test below
 * shows the difference immediately.
 */
export const NoiseSuppression: React.FC<NoiseSuppressionProps> = ({
  descriptionMode = "tooltip",
  grouped = false,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const enabled = getSetting("denoise_enabled") ?? false;

  return (
    <ToggleSwitch
      checked={enabled}
      onChange={(enabled) => updateSetting("denoise_enabled", enabled)}
      isUpdating={isUpdating("denoise_enabled")}
      label={t("settings.advanced.noiseSuppression.title")}
      description={t("settings.advanced.noiseSuppression.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
    />
  );
};
