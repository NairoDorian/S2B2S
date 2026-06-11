import React from "react";
import { useTranslation } from "react-i18next";
import { useSettings } from "../../../hooks/useSettings";
import { ToggleSwitch } from "../../ui/ToggleSwitch";
import { SettingContainer } from "../../ui/SettingContainer";
import { Dropdown } from "../../ui/Dropdown";
import { Slider } from "../../ui/Slider";

interface AudioEnhancementsProps {
  grouped?: boolean;
}

export const AudioEnhancements: React.FC<AudioEnhancementsProps> = ({
  grouped = true,
}) => {
  const { t } = useTranslation();
  const { settings, updateSetting, isUpdating } = useSettings();

  const noiseSuppressionEnabled = settings?.noise_suppression_enabled ?? false;
  const vadMode = settings?.vad_mode ?? "triple";
  const rnnoiseThreshold = settings?.rnnoise_voice_threshold ?? 0.2;

  return (
    <>
      <ToggleSwitch
        checked={noiseSuppressionEnabled}
        onChange={(checked) => void updateSetting("noise_suppression_enabled", checked)}
        isUpdating={isUpdating("noise_suppression_enabled")}
        label={t("settings.advanced.noiseSuppression.label", { defaultValue: "Noise Suppression (RNNoise)" })}
        description={t("settings.advanced.noiseSuppression.description", { defaultValue: "Use a recurrent neural network to suppress noise and clean microphone input audio in real-time." })}
        grouped={grouped}
      />
      <SettingContainer
        title={t("settings.advanced.vadMode.label", { defaultValue: "Voice Activity Detection Mode" })}
        description={t("settings.advanced.vadMode.description", { defaultValue: "Switch VAD engine. 'Silero VAD' uses neural network to verify speech. 'Triple VAD' runs amplitude VAD, RNNoise, and Silero VAD for maximum accuracy." })}
        grouped={grouped}
      >
        <Dropdown
          options={[
            { value: "silero", label: t("settings.advanced.vadMode.options.silero", { defaultValue: "Silero VAD" }) },
            { value: "triple", label: t("settings.advanced.vadMode.options.triple", { defaultValue: "Triple VAD (Amplitude + RNNoise + Silero)" }) },
          ]}
          selectedValue={vadMode}
          onSelect={(value) => void updateSetting("vad_mode", value)}
          disabled={isUpdating("vad_mode")}
        />
      </SettingContainer>
      {vadMode === "triple" && (
        <Slider
          label={t("settings.advanced.rnnoiseThreshold.label", { defaultValue: "RNNoise Voice Threshold" })}
          description={t("settings.advanced.rnnoiseThreshold.description", { defaultValue: "Minimum voice probability from RNNoise to consider audio as speech. Lower values are more sensitive, higher values filter more aggressively." })}
          value={rnnoiseThreshold}
          onChange={(value) => void updateSetting("rnnoise_voice_threshold", value)}
          min={0.05}
          max={0.9}
          step={0.05}
          grouped={grouped}
          showValue={true}
          formatValue={(v) => v.toFixed(2)}
        />
      )}
    </>
  );
};
