import { Show } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import { Slider } from "../ui/Slider";
import { useSettings } from "../../hooks/useSettings";

interface VadSensitivityProps {
  descriptionMode?: "tooltip" | "inline";
  grouped?: boolean;
  alwaysShow?: boolean;
}

const DEFAULT_THRESHOLD = 0.5;
const MIN_THRESHOLD = 0.05;
const MAX_THRESHOLD = 0.95;

export const VadSensitivity = (props: VadSensitivityProps) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  return (
    <Show
      when={(getSetting("vad_enabled") ?? true) || (props.alwaysShow ?? false)}
    >
      <Slider
        value={getSetting("vad_threshold_earshot") ?? DEFAULT_THRESHOLD}
        onChange={(next) =>
          updateSetting(
            "vad_threshold_earshot",
            Math.min(
              MAX_THRESHOLD,
              Math.max(MIN_THRESHOLD, Math.round(next * 100) / 100),
            ),
          )
        }
        min={MIN_THRESHOLD}
        max={MAX_THRESHOLD}
        step={0.05}
        label={t("settings.advanced.vadSensitivity.label")}
        description={t("settings.advanced.vadSensitivity.description")}
        descriptionMode={props.descriptionMode}
        grouped={props.grouped}
        formatValue={(v) => v.toFixed(2)}
        onReset={() =>
          updateSetting("vad_threshold_earshot", DEFAULT_THRESHOLD)
        }
        disabled={isUpdating("vad_threshold_earshot")}
      />
    </Show>
  );
};
