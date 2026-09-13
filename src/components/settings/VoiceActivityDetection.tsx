import { useTranslation } from "@/i18n/useTranslation";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";

interface VoiceActivityDetectionProps {
  descriptionMode?: "tooltip" | "inline";
  grouped?: boolean;
}

export const VoiceActivityDetection = (props: VoiceActivityDetectionProps) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  return (
    <ToggleSwitch
      checked={getSetting("vad_enabled") ?? true}
      onChange={(enabled) => updateSetting("vad_enabled", enabled)}
      isUpdating={isUpdating("vad_enabled")}
      label={t("settings.advanced.voiceActivityDetection.title")}
      description={t("settings.advanced.voiceActivityDetection.description")}
      descriptionMode={props.descriptionMode}
      grouped={props.grouped}
    />
  );
};
