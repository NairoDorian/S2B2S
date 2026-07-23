import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";
import { VolumeSlider } from "./VolumeSlider";
import { SoundPicker } from "./SoundPicker";

interface AudioFeedbackProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const AudioFeedback: React.FC<AudioFeedbackProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();
    const audioFeedbackEnabled = getSetting("audio_feedback") || false;
    const resultReadyFeedbackEnabled =
      getSetting("result_ready_audio_feedback") || false;

    return (
      <div className="flex flex-col">
        <ToggleSwitch
          checked={audioFeedbackEnabled}
          onChange={(enabled) => updateSetting("audio_feedback", enabled)}
          isUpdating={isUpdating("audio_feedback")}
          label={t("settings.sound.audioFeedback.label")}
          description={t("settings.sound.audioFeedback.description")}
          descriptionMode={descriptionMode}
          grouped={grouped}
        />
        <ToggleSwitch
          checked={resultReadyFeedbackEnabled}
          onChange={(enabled) =>
            updateSetting("result_ready_audio_feedback", enabled)
          }
          isUpdating={isUpdating("result_ready_audio_feedback")}
          label={t(
            "settings.sound.audioFeedback.resultReady.label",
            "Result-ready sound",
          )}
          description={t(
            "settings.sound.audioFeedback.resultReady.description",
            "Play sound when dictation transcription finishes and text is delivered",
          )}
          descriptionMode={descriptionMode}
          grouped={grouped}
        />
      </div>
    );
  },
);
