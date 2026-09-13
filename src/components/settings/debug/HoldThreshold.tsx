import { useTranslation } from "@/i18n/useTranslation";
import { Slider } from "../../ui/Slider";
import { useSettings } from "../../../hooks/useSettings";

interface HoldThresholdProps {
  descriptionMode?: "tooltip" | "inline";
  grouped?: boolean;
}

export const HoldThreshold = (props: HoldThresholdProps) => {
  const { t } = useTranslation();
  const { settings, updateSetting, resetSetting, isUpdating } = useSettings();

  return (
    <Slider
      value={settings()?.hold_threshold_ms ?? 300}
      onChange={(value) => updateSetting("hold_threshold_ms", value)}
      onReset={() => resetSetting("hold_threshold_ms")}
      isResetting={isUpdating("hold_threshold_ms")}
      min={100}
      max={1000}
      step={50}
      label={t("settings.debug.holdThreshold.title")}
      description={t("settings.debug.holdThreshold.description")}
      descriptionMode={props.descriptionMode}
      grouped={props.grouped}
      formatValue={(v) => `${v}ms`}
    />
  );
};
