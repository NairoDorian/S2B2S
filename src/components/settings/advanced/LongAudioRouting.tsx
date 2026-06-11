import React from "react";
import { useTranslation } from "react-i18next";
import { useSettings } from "../../../hooks/useSettings";
import { useModelStore } from "../../../stores/modelStore";
import { SettingContainer } from "../../ui/SettingContainer";
import { Dropdown } from "../../ui/Dropdown";
import { Slider } from "../../ui/Slider";

interface LongAudioRoutingProps {
  grouped?: boolean;
}

export const LongAudioRouting: React.FC<LongAudioRoutingProps> = ({
  grouped = true,
}) => {
  const { t } = useTranslation();
  const { settings, updateSetting, isUpdating } = useSettings();
  const { models } = useModelStore();

  const longAudioModel = settings?.long_audio_model ?? null;
  const longAudioThreshold = settings?.long_audio_threshold_seconds ?? 10;

  // Filter models that are downloaded
  const downloadedModels = models.filter((m) => m.is_downloaded);

  const modelOptions = [
    { value: "", label: t("settings.advanced.longAudio.disabledOption", { defaultValue: "Disabled (Use current model)" }) },
    ...downloadedModels.map((m) => ({
      value: m.id,
      label: m.name,
    })),
  ];

  return (
    <>
      <SettingContainer
        title={t("settings.advanced.longAudio.model.label", { defaultValue: "Long Audio Model Routing" })}
        description={t("settings.advanced.longAudio.model.description", { defaultValue: "Specify a fallback model to route transcriptions to when the recording exceeds the threshold duration. Useful for routing long recordings to a larger model." })}
        grouped={grouped}
      >
        <Dropdown
          options={modelOptions}
          selectedValue={longAudioModel || ""}
          onSelect={(value) => void updateSetting("long_audio_model", value === "" ? null : value)}
          disabled={isUpdating("long_audio_model")}
        />
      </SettingContainer>

      {longAudioModel && (
        <Slider
          value={longAudioThreshold}
          onChange={(val) => void updateSetting("long_audio_threshold_seconds", val)}
          min={1}
          max={60}
          step={0.5}
          label={t("settings.advanced.longAudio.threshold.label", { defaultValue: "Long Audio Threshold" })}
          description={t("settings.advanced.longAudio.threshold.description", { defaultValue: "The recording duration in seconds before routing the audio to the long audio model." })}
          grouped={grouped}
          showValue
          formatValue={(val) => `${val.toFixed(1)}s`}
          disabled={isUpdating("long_audio_threshold_seconds")}
        />
      )}
    </>
  );
};
