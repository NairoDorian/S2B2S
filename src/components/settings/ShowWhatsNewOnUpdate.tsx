import { useTranslation } from "@/i18n/useTranslation";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";
import type { JSX } from "@solidjs/web";

interface ShowWhatsNewOnUpdateProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const ShowWhatsNewOnUpdate = (
  props: ShowWhatsNewOnUpdateProps,
): JSX.Element => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  return (
    <ToggleSwitch
      checked={getSetting("show_whats_new_on_update") ?? true}
      onChange={(nextEnabled) =>
        updateSetting("show_whats_new_on_update", nextEnabled)
      }
      isUpdating={isUpdating("show_whats_new_on_update")}
      label={t("settings.about.whatsNewUpdates.label")}
      description={t("settings.about.whatsNewUpdates.description")}
      descriptionMode={props.descriptionMode}
      grouped={props.grouped}
    />
  );
};
