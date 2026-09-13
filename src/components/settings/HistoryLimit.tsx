import { useTranslation } from "@/i18n/useTranslation";
import { useSettings } from "../../hooks/useSettings";
import { Input } from "../ui/Input";
import { SettingContainer } from "../ui/SettingContainer";
import type { JSX } from "@solidjs/web";

interface HistoryLimitProps {
  descriptionMode?: "tooltip" | "inline";
  grouped?: boolean;
}

export const HistoryLimit = (props: HistoryLimitProps): JSX.Element => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  const handleChange = async (event: { target: { value: string } }) => {
    const value = parseInt(event.target.value, 10);
    if (!isNaN(value) && value >= 0) {
      updateSetting("history_limit", value);
    }
  };

  return (
    <SettingContainer
      title={t("settings.debug.historyLimit.title")}
      description={t("settings.debug.historyLimit.description")}
      descriptionMode={props.descriptionMode}
      grouped={props.grouped}
      layout="horizontal"
    >
      <div class="flex items-center space-x-2">
        <Input
          type="number"
          min="0"
          max="1000"
          value={getSetting("history_limit") ?? 5}
          onInput={handleChange}
          disabled={isUpdating("history_limit")}
          class="w-20"
        />
        <span class="text-sm text-text">
          {t("settings.debug.historyLimit.entries")}
        </span>
      </div>
    </SettingContainer>
  );
};
