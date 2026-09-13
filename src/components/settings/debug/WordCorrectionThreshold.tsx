import { useTranslation } from "@/i18n/useTranslation";
import { Slider } from "../../ui/Slider";
import { useSettings } from "../../../hooks/useSettings";

interface WordCorrectionThresholdProps {
  descriptionMode?: "tooltip" | "inline";
  grouped?: boolean;
}

export const WordCorrectionThreshold = (
  props: WordCorrectionThresholdProps,
) => {
  const { t } = useTranslation();
  const { settings, updateSetting, resetSetting, isUpdating } = useSettings();

  const handleThresholdChange = (value: number) => {
    updateSetting("word_correction_threshold", value);
  };

  return (
    <Slider
      value={settings()?.word_correction_threshold ?? 0.18}
      onChange={handleThresholdChange}
      onReset={() => resetSetting("word_correction_threshold")}
      isResetting={isUpdating("word_correction_threshold")}
      min={0.0}
      max={1.0}
      label={t("settings.debug.wordCorrectionThreshold.title")}
      description={t("settings.debug.wordCorrectionThreshold.description")}
      descriptionMode={props.descriptionMode}
      grouped={props.grouped}
    />
  );
};
