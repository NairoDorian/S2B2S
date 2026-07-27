import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { SettingContainer, SettingsGroup } from "../ui";
import { Input } from "../ui/Input";
import { Dropdown } from "../ui/Dropdown";
import { useSettings } from "../../hooks/useSettings";

type TimeoutUnit = "seconds" | "minutes";

export const MicIdleTimeout: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  const lazyClose = getSetting("lazy_stream_close") ?? false;
  const value = getSetting("mic_idle_timeout_value") ?? 30;
  const unit = (getSetting("mic_idle_timeout_unit") ?? "seconds") as TimeoutUnit;
  const infinite = getSetting("mic_idle_infinite") ?? false;

  if (!lazyClose) return null;

  const unitOptions: { value: string; label: string }[] = [
    { value: "seconds", label: t("settings.micIdleTimeout.seconds") },
    { value: "minutes", label: t("settings.micIdleTimeout.minutes") },
  ];

  return (
    <SettingsGroup title={t("settings.micIdleTimeout.title")}>
      <div className="space-y-3 px-4 p-2">
        <SettingContainer
          title={t("settings.micIdleTimeout.infiniteLabel")}
          description={t("settings.micIdleTimeout.infiniteDescription")}
          descriptionMode="tooltip"
          grouped
        >
          <ToggleSwitch
            checked={infinite}
            onChange={(enabled) => updateSetting("mic_idle_infinite", enabled)}
            isUpdating={isUpdating("mic_idle_infinite")}
            label={t("settings.micIdleTimeout.infiniteLabel")}
            description={t("settings.micIdleTimeout.infiniteDescription")}
            descriptionMode="tooltip"
            grouped
          />
        </SettingContainer>

        {!infinite && (
          <div className="flex items-end gap-2">
            <div className="flex-1">
              <SettingContainer
                title={t("settings.micIdleTimeout.timeoutLabel")}
                description={t("settings.micIdleTimeout.timeoutDescription")}
                descriptionMode="tooltip"
                layout="stacked"
                grouped
              >
                <Input
                  type="number"
                  min={1}
                  value={value}
                  onChange={(e) => {
                    const v = parseInt(e.target.value, 10);
                    if (!isNaN(v) && v >= 1) {
                      updateSetting("mic_idle_timeout_value", v);
                    }
                  }}
                  variant="compact"
                />
              </SettingContainer>
            </div>
            <div className="w-32">
              <SettingContainer
                title={t("settings.micIdleTimeout.unitLabel")}
                description={t("settings.micIdleTimeout.unitDescription")}
                descriptionMode="tooltip"
                grouped
              >
                <Dropdown
                  selectedValue={unit}
                  options={unitOptions}
                  onSelect={(val) => updateSetting("mic_idle_timeout_unit", val as TimeoutUnit)}
                />
              </SettingContainer>
            </div>
          </div>
        )}
      </div>
    </SettingsGroup>
  );
};