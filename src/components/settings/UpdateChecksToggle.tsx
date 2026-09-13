import { useTranslation } from "@/i18n/useTranslation";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";
import { ENV_PREFIX } from "@/lib/appIdentity";

interface UpdateChecksToggleProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const UpdateChecksToggle = (props: UpdateChecksToggleProps) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating, updateChecksLocked } =
    useSettings();

  return (
    <ToggleSwitch
      checked={
        updateChecksLocked()
          ? false
          : (getSetting("update_checks_enabled") ?? true)
      }
      onChange={(enabled) => updateSetting("update_checks_enabled", enabled)}
      isUpdating={isUpdating("update_checks_enabled")}
      disabled={updateChecksLocked() !== false}
      label={t("settings.debug.updateChecks.label")}
      description={
        updateChecksLocked()
          ? t("settings.debug.updateChecks.lockedDescription", {
              envVar: `${ENV_PREFIX}DISABLE_UPDATER`,
            })
          : t("settings.debug.updateChecks.description")
      }
      descriptionMode={props.descriptionMode}
      grouped={props.grouped}
    />
  );
};
