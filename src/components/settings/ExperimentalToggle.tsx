import { useTranslation } from "@/i18n/useTranslation";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";

interface ExperimentalToggleProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const ExperimentalToggle = (props: ExperimentalToggleProps) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  return (
    <ToggleSwitch
      checked={getSetting("experimental_enabled") || false}
      onChange={(enabled) => updateSetting("experimental_enabled", enabled)}
      isUpdating={isUpdating("experimental_enabled")}
      label={t("settings.advanced.experimentalToggle.label")}
      description={t("settings.advanced.experimentalToggle.description")}
      descriptionMode={props.descriptionMode}
      grouped={props.grouped}
    />
  );
};
