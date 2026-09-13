import { useTranslation } from "@/i18n/useTranslation";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";

interface AudioFeedbackProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const AudioFeedback = (props: AudioFeedbackProps) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  return (
    <div class="flex flex-col">
      <ToggleSwitch
        checked={getSetting("audio_feedback") || false}
        onChange={(enabled) => updateSetting("audio_feedback", enabled)}
        isUpdating={isUpdating("audio_feedback")}
        label={t("settings.sound.audioFeedback.label")}
        description={t("settings.sound.audioFeedback.description")}
        descriptionMode={props.descriptionMode}
        grouped={props.grouped}
      />
    </div>
  );
};
