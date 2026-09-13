import { useTranslation } from "@/i18n/useTranslation";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";

interface AppendTrailingSpaceProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const AppendTrailingSpace = (props: AppendTrailingSpaceProps) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  return (
    <ToggleSwitch
      checked={getSetting("append_trailing_space") ?? false}
      onChange={(enabled) => updateSetting("append_trailing_space", enabled)}
      isUpdating={isUpdating("append_trailing_space")}
      label={t("settings.debug.appendTrailingSpace.label")}
      description={t("settings.debug.appendTrailingSpace.description")}
      descriptionMode={props.descriptionMode}
      grouped={props.grouped}
    />
  );
};
