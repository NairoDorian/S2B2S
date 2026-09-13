import { useTranslation } from "@/i18n/useTranslation";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";

interface AppendTrailingNewlineProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const AppendTrailingNewline = (props: AppendTrailingNewlineProps) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  return (
    <ToggleSwitch
      checked={getSetting("append_trailing_newline") ?? false}
      onChange={(enabled) => updateSetting("append_trailing_newline", enabled)}
      isUpdating={isUpdating("append_trailing_newline")}
      label={t("settings.debug.appendTrailingNewline.label")}
      description={t("settings.debug.appendTrailingNewline.description")}
      descriptionMode={props.descriptionMode}
      grouped={props.grouped}
    />
  );
};
