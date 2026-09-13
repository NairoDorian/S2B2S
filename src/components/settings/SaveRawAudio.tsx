import { useTranslation } from "@/i18n/useTranslation";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";
import type { JSX } from "@solidjs/web";

interface SaveRawAudioProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const SaveRawAudio = (props: SaveRawAudioProps): JSX.Element => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  return (
    <ToggleSwitch
      checked={getSetting("save_raw_audio") ?? false}
      onChange={(enabled) => updateSetting("save_raw_audio", enabled)}
      isUpdating={isUpdating("save_raw_audio")}
      label={t("settings.advanced.saveRawAudio.label")}
      description={t("settings.advanced.saveRawAudio.description")}
      descriptionMode={props.descriptionMode}
      grouped={props.grouped}
    />
  );
};
