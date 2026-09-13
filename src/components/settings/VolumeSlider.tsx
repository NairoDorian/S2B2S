import { useTranslation } from "@/i18n/useTranslation";
import { Slider } from "../ui/Slider";
import { useSettings } from "../../hooks/useSettings";

export const VolumeSlider = (props: { disabled?: boolean }) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting } = useSettings();

  return (
    <Slider
      value={getSetting("audio_feedback_volume") ?? 0.5}
      onChange={(value: number) =>
        updateSetting("audio_feedback_volume", value)
      }
      min={0}
      max={1}
      label={t("settings.sound.volume.title")}
      description={t("settings.sound.volume.description")}
      descriptionMode="tooltip"
      grouped
      formatValue={(value) => `${Math.round(value * 100)}%`}
      disabled={props.disabled ?? false}
    />
  );
};
