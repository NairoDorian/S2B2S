import { useTranslation } from "@/i18n/useTranslation";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";
import type { JSX } from "@solidjs/web";

interface TranslateToEnglishProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const TranslateToEnglish = (
  props: TranslateToEnglishProps,
): JSX.Element => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  return (
    <ToggleSwitch
      checked={getSetting("translate_to_english") || false}
      onChange={(enabled) => updateSetting("translate_to_english", enabled)}
      isUpdating={isUpdating("translate_to_english")}
      label={t("settings.advanced.translateToEnglish.label")}
      description={t("settings.advanced.translateToEnglish.description")}
      descriptionMode={props.descriptionMode}
      grouped={props.grouped}
    />
  );
};
