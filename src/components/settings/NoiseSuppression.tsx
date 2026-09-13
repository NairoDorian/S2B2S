import { useTranslation } from "@/i18n/useTranslation";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";
import type { JSX } from "@solidjs/web";

interface NoiseSuppressionProps {
  descriptionMode?: "tooltip" | "inline";
  grouped?: boolean;
}

/**
 * RNNoise suppression on the microphone path, ahead of the VAD and the model.
 * Toggling applies on the next captured chunk, so the live VAD test below
 * shows the difference immediately.
 */
export const NoiseSuppression = (props: NoiseSuppressionProps): JSX.Element => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  return (
    <ToggleSwitch
      checked={getSetting("denoise_enabled") ?? false}
      onChange={(enabled) => updateSetting("denoise_enabled", enabled)}
      isUpdating={isUpdating("denoise_enabled")}
      label={t("settings.advanced.noiseSuppression.title")}
      description={t("settings.advanced.noiseSuppression.description")}
      descriptionMode={props.descriptionMode}
      grouped={props.grouped}
    />
  );
};
