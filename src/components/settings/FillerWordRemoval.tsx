import { useTranslation } from "@/i18n/useTranslation";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";

interface FillerWordRemovalProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const FillerWordRemoval = (props: FillerWordRemovalProps) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  return (
    <ToggleSwitch
      checked={getSetting("filler_word_removal_enabled") ?? true}
      onChange={(nextEnabled) =>
        updateSetting("filler_word_removal_enabled", nextEnabled)
      }
      isUpdating={isUpdating("filler_word_removal_enabled")}
      label={t("settings.advanced.fillerWordRemoval.title")}
      description={t("settings.advanced.fillerWordRemoval.description")}
      descriptionMode={props.descriptionMode}
      grouped={props.grouped}
    />
  );
};
