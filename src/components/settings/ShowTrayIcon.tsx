import { useTranslation } from "@/i18n/useTranslation";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";
import type { JSX } from "@solidjs/web";

interface ShowTrayIconProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const ShowTrayIcon = (props: ShowTrayIconProps): JSX.Element => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  return (
    <ToggleSwitch
      checked={getSetting("show_tray_icon") ?? true}
      onChange={(enabled) => updateSetting("show_tray_icon", enabled)}
      isUpdating={isUpdating("show_tray_icon")}
      label={t("settings.advanced.showTrayIcon.label")}
      description={t("settings.advanced.showTrayIcon.description")}
      descriptionMode={props.descriptionMode}
      grouped={props.grouped}
      tooltipPosition="bottom"
    />
  );
};
