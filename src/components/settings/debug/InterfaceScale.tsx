import { useTranslation } from "@/i18n/useTranslation";
import { Slider } from "../../ui/Slider";
import { useSettings } from "../../../hooks/useSettings";

interface InterfaceScaleProps {
  descriptionMode?: "tooltip" | "inline";
  grouped?: boolean;
}

const MIN_SCALE = 0.7;
const MAX_SCALE = 1.6;

export const InterfaceScale = (props: InterfaceScaleProps) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const value = () => getSetting("ui_scale") ?? 1;

  return (
    <Slider
      value={value()}
      onChange={(next) =>
        updateSetting(
          "ui_scale",
          Math.min(MAX_SCALE, Math.max(MIN_SCALE, Math.round(next * 20) / 20)),
        )
      }
      min={MIN_SCALE}
      max={MAX_SCALE}
      step={0.05}
      label={t("settings.debug.interfaceScale.label")}
      description={t("settings.debug.interfaceScale.description")}
      descriptionMode={props.descriptionMode}
      grouped={props.grouped}
      formatValue={(v) => `${Math.round(v * 100)}%`}
      onReset={() => updateSetting("ui_scale", 1)}
      disabled={isUpdating("ui_scale")}
    />
  );
};
