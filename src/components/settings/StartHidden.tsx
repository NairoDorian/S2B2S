import { useTranslation } from "@/i18n/useTranslation";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";

interface StartHiddenProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const StartHidden = (props: StartHiddenProps) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  return (
    <ToggleSwitch
      checked={getSetting("start_hidden") ?? false}
      onChange={(enabled) => updateSetting("start_hidden", enabled)}
      isUpdating={isUpdating("start_hidden")}
      label={t("settings.advanced.startHidden.label")}
      description={t("settings.advanced.startHidden.description")}
      descriptionMode={props.descriptionMode}
      grouped={props.grouped}
      tooltipPosition="bottom"
    />
  );
};
