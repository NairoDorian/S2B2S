import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";

export const ParakeetStreamingToggle: React.FC = React.memo(() => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  const enabled = getSetting("parakeet_streaming_enabled") as boolean ?? true;

  return (
    <ToggleSwitch
      checked={enabled}
      onChange={(enabled) => updateSetting("parakeet_streaming_enabled", enabled)}
      isUpdating={isUpdating("parakeet_streaming_enabled")}
      label={t("settings.modelSettings.parakeetStreamingToggle.label")}
      description={t("settings.modelSettings.parakeetStreamingToggle.description")}
    />
  );
});
