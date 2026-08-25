import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";

interface SaveRawAudioProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const SaveRawAudio: React.FC<SaveRawAudioProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    const enabled = getSetting("save_raw_audio") ?? false;

    return (
      <ToggleSwitch
        checked={enabled}
        onChange={(enabled) => updateSetting("save_raw_audio", enabled)}
        isUpdating={isUpdating("save_raw_audio")}
        label={t("settings.advanced.saveRawAudio.label")}
        description={t("settings.advanced.saveRawAudio.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      />
    );
  },
);

SaveRawAudio.displayName = "SaveRawAudio";
