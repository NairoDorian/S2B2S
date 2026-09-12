import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";
import { ENV_PREFIX } from "@/lib/appIdentity";

interface UpdateChecksToggleProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const UpdateChecksToggle: React.FC<UpdateChecksToggleProps> = ({
  descriptionMode = "tooltip",
  grouped = false,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating, updateChecksLocked } =
    useSettings();
  const updateChecksEnabled = getSetting("update_checks_enabled") ?? true;

  return (
    <ToggleSwitch
      checked={updateChecksLocked ? false : updateChecksEnabled}
      onChange={(enabled) => updateSetting("update_checks_enabled", enabled)}
      isUpdating={isUpdating("update_checks_enabled")}
      disabled={updateChecksLocked !== false}
      label={t("settings.debug.updateChecks.label")}
      description={
        updateChecksLocked
          ? // The flag is interpolated rather than spelled inside the locale
            // value: it is the environment prefix, which belongs to the
            // generated identity, and 24 hand-edited copies of it would be 24
            // places a rename has to find.
            t("settings.debug.updateChecks.lockedDescription", {
              envVar: `${ENV_PREFIX}DISABLE_UPDATER`,
            })
          : t("settings.debug.updateChecks.description")
      }
      descriptionMode={descriptionMode}
      grouped={grouped}
    />
  );
};
