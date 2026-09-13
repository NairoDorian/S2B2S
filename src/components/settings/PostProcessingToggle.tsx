import { useTranslation } from "@/i18n/useTranslation";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";

interface PostProcessingToggleProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const PostProcessingToggle = (props: PostProcessingToggleProps) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  return (
    <ToggleSwitch
      checked={getSetting("post_process_enabled") || false}
      onChange={(enabled) => updateSetting("post_process_enabled", enabled)}
      isUpdating={isUpdating("post_process_enabled")}
      label={t("settings.debug.postProcessingToggle.label")}
      description={t("settings.debug.postProcessingToggle.description")}
      descriptionMode={props.descriptionMode}
      grouped={props.grouped}
    />
  );
};
