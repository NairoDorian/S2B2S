import { useTranslation } from "@/i18n/useTranslation";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";
import type { JSX } from "@solidjs/web";

interface MuteWhileRecordingToggleProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const MuteWhileRecording = (
  props: MuteWhileRecordingToggleProps,
): JSX.Element => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  // `getSetting` is called inside the JSX binding, not in this body: a body
  // read is a mount-time snapshot, and the switch would never follow a
  // settings change (or the initial async load).
  return (
    <ToggleSwitch
      checked={getSetting("mute_while_recording") ?? false}
      onChange={(enabled) => updateSetting("mute_while_recording", enabled)}
      isUpdating={isUpdating("mute_while_recording")}
      label={t("settings.debug.muteWhileRecording.label")}
      description={t("settings.debug.muteWhileRecording.description")}
      descriptionMode={props.descriptionMode}
      grouped={props.grouped}
    />
  );
};
