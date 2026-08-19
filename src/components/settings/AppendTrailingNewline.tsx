import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";

interface AppendTrailingNewlineProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const AppendTrailingNewline: React.FC<AppendTrailingNewlineProps> =
  React.memo(({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    const enabled = getSetting("append_trailing_newline") ?? false;

    return (
      <ToggleSwitch
        checked={enabled}
        onChange={(enabled) => updateSetting("append_trailing_newline", enabled)}
        isUpdating={isUpdating("append_trailing_newline")}
        label={t("settings.debug.appendTrailingNewline.label")}
        description={t("settings.debug.appendTrailingNewline.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      />
    );
  });
