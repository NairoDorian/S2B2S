import { useTranslation } from "@/i18n/useTranslation";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";

interface AlwaysOnMicrophoneProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const AlwaysOnMicrophone = (props: AlwaysOnMicrophoneProps) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  return (
    <ToggleSwitch
      checked={getSetting("always_on_microphone") || false}
      onChange={(enabled) => updateSetting("always_on_microphone", enabled)}
      isUpdating={isUpdating("always_on_microphone")}
      label={t("settings.debug.alwaysOnMicrophone.label")}
      description={t("settings.debug.alwaysOnMicrophone.description")}
      descriptionMode={props.descriptionMode}
      grouped={props.grouped}
    />
  );
};
