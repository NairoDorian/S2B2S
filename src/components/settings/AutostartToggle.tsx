import { useTranslation } from "@/i18n/useTranslation";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";

interface AutostartToggleProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const AutostartToggle = (props: AutostartToggleProps) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  return (
    <ToggleSwitch
      checked={getSetting("autostart_enabled") ?? false}
      onChange={(enabled) => updateSetting("autostart_enabled", enabled)}
      isUpdating={isUpdating("autostart_enabled")}
      label={t("settings.advanced.autostart.label")}
      description={t("settings.advanced.autostart.description")}
      descriptionMode={props.descriptionMode}
      grouped={props.grouped}
    />
  );
};
