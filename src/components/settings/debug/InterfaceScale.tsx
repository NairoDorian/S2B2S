import React from "react";
import { useTranslation } from "react-i18next";
import { Slider } from "../../ui/Slider";
import { useSettings } from "../../../hooks/useSettings";

interface InterfaceScaleProps {
  descriptionMode?: "tooltip" | "inline";
  grouped?: boolean;
}

const MIN_SCALE = 0.7;
const MAX_SCALE = 1.6;

/**
 * Zoom for the whole settings window, for screens whose OS scaling makes
 * Handy too small or too large. Applied as CSS zoom on the document root
 * (see `applyUiScale`), so layout scales with it.
 */
export const InterfaceScale: React.FC<InterfaceScaleProps> = ({
  descriptionMode = "tooltip",
  grouped = false,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const value = getSetting("ui_scale") ?? 1;

  return (
    <Slider
      value={value}
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
      descriptionMode={descriptionMode}
      grouped={grouped}
      formatValue={(v) => `${Math.round(v * 100)}%`}
      onReset={() => updateSetting("ui_scale", 1)}
      disabled={isUpdating("ui_scale")}
    />
  );
};
