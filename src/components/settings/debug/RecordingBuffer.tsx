import { useTranslation } from "@/i18n/useTranslation";
import { Slider } from "../../ui/Slider";
import { useSettings } from "../../../hooks/useSettings";

interface RecordingBufferProps {
  descriptionMode?: "tooltip" | "inline";
  grouped?: boolean;
}

export const RecordingBuffer = (props: RecordingBufferProps) => {
  const { t } = useTranslation();
  const { settings, updateSetting, resetSetting, isUpdating } = useSettings();

  const handleBufferChange = (value: number) => {
    updateSetting("extra_recording_buffer_ms", value);
  };

  return (
    <Slider
      value={settings()?.extra_recording_buffer_ms ?? 0}
      onChange={handleBufferChange}
      onReset={() => resetSetting("extra_recording_buffer_ms")}
      isResetting={isUpdating("extra_recording_buffer_ms")}
      min={0}
      max={1500}
      step={50}
      label={t("settings.debug.recordingBuffer.title")}
      description={t("settings.debug.recordingBuffer.description")}
      descriptionMode={props.descriptionMode}
      grouped={props.grouped}
      formatValue={(v) => `${v}ms`}
    />
  );
};
