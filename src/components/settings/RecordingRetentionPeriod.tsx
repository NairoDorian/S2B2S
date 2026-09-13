import { useTranslation } from "@/i18n/useTranslation";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { useSettings } from "../../hooks/useSettings";
import { RecordingRetentionPeriod } from "@/bindings";
import type { JSX } from "@solidjs/web";

interface RecordingRetentionPeriodProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const RecordingRetentionPeriodSelector = (
  props: RecordingRetentionPeriodProps,
): JSX.Element => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  const handleRetentionPeriodSelect = async (period: string) => {
    await updateSetting(
      "recording_retention_period",
      period as RecordingRetentionPeriod,
    );
  };

  // Built inside the JSX binding: the "preserve limit" label quotes the live
  // history limit, and the selected value follows the setting.
  const retentionOptions = () => [
    { value: "never", label: t("settings.debug.recordingRetention.never") },
    {
      value: "preserve_limit",
      label: t("settings.debug.recordingRetention.preserveLimit", {
        count: Number(getSetting("history_limit") || 5),
      }),
    },
    { value: "days3", label: t("settings.debug.recordingRetention.days3") },
    { value: "weeks2", label: t("settings.debug.recordingRetention.weeks2") },
    {
      value: "months3",
      label: t("settings.debug.recordingRetention.months3"),
    },
  ];

  return (
    <SettingContainer
      title={t("settings.debug.recordingRetention.title")}
      description={t("settings.debug.recordingRetention.description")}
      descriptionMode={props.descriptionMode}
      grouped={props.grouped}
    >
      <Dropdown
        options={retentionOptions()}
        selectedValue={getSetting("recording_retention_period") || "never"}
        onSelect={handleRetentionPeriodSelect}
        placeholder={t("settings.debug.recordingRetention.placeholder")}
        disabled={isUpdating("recording_retention_period")}
      />
    </SettingContainer>
  );
};
