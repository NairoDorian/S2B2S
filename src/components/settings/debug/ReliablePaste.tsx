import { Show } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import { ToggleSwitch } from "../../ui/ToggleSwitch";
import { useSettings } from "../../../hooks/useSettings";
import { useOsType } from "../../../hooks/useOsType";

interface ReliablePasteToggleProps {
  descriptionMode?: "tooltip" | "inline";
  grouped?: boolean;
}

export const ReliablePasteToggle = (props: ReliablePasteToggleProps) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const osType = useOsType();

  return (
    <Show when={osType === "macos" || osType === "windows"}>
      <ToggleSwitch
        checked={getSetting("reliable_paste") ?? false}
        onChange={(enabled) => updateSetting("reliable_paste", enabled)}
        isUpdating={isUpdating("reliable_paste")}
        label={t("settings.debug.reliablePaste.title")}
        description={t("settings.debug.reliablePaste.description")}
        descriptionMode={props.descriptionMode}
        grouped={props.grouped}
      />
    </Show>
  );
};
