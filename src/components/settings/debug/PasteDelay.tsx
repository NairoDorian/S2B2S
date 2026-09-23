import { useTranslation } from "@/i18n/useTranslation";
import { Slider } from "../../ui/Slider";
import { useSettings } from "../../../hooks/useSettings";

interface PasteDelayProps {
  descriptionMode?: "tooltip" | "inline";
  grouped?: boolean;
  settingKey?: "paste_delay_ms" | "paste_delay_after_ms";
  labelKey?: string;
  descriptionKey?: string;
}

export const PasteDelay = (props: PasteDelayProps) => {
  const { t } = useTranslation();
  const { settings, updateSetting, resetSetting, isUpdating } = useSettings();

  const settingKey = () => props.settingKey ?? "paste_delay_ms";

  const handleDelayChange = (value: number) => {
    updateSetting(settingKey(), value);
  };

  return (
    <Slider
      value={settings()?.[settingKey()] ?? 60}
      onChange={handleDelayChange}
      onReset={() => resetSetting(settingKey())}
      isResetting={isUpdating(settingKey())}
      min={1}
      max={5000}
      step={1}
      label={t(props.labelKey ?? "settings.debug.pasteDelay.title")}
      description={t(
        props.descriptionKey ?? "settings.debug.pasteDelay.description",
      )}
      descriptionMode={props.descriptionMode}
      grouped={props.grouped}
      formatValue={(v) =>
        t("settings.statistics.units.milliseconds", { value: v })
      }
    />
  );
};
