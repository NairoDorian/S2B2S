import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { SettingContainer, SettingsGroup } from "../ui";
import { Input } from "../ui/Input";
import { Dropdown } from "../ui/Dropdown";
import { useSettings } from "../../hooks/useSettings";

type TimeoutUnit = "seconds" | "minutes";

// Keep the idle timeout within a sane band: at least one unit, at most a day.
const MIN_TIMEOUT = 1;
const MAX_TIMEOUT: Record<TimeoutUnit, number> = {
  seconds: 86_400,
  minutes: 1_440,
};

export const MicIdleTimeout: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  const lazyClose = getSetting("lazy_stream_close") ?? false;
  const value = getSetting("mic_idle_timeout_value") ?? 30;
  const unit = (getSetting("mic_idle_timeout_unit") ??
    "seconds") as TimeoutUnit;
  const infinite = getSetting("mic_idle_infinite") ?? false;

  // Draft the number locally and commit on blur/Enter: a controlled input
  // that writes the setting on every keystroke can't be cleared (backspacing
  // to "" is rejected and the next digit gets appended → 30 becomes 305) and
  // hits the settings store once per digit.
  const [draft, setDraft] = React.useState(String(value));
  React.useEffect(() => {
    setDraft(String(value));
  }, [value]);

  const commitDraft = () => {
    const parsed = parseInt(draft, 10);
    if (isNaN(parsed)) {
      setDraft(String(value));
      return;
    }
    const clamped = Math.min(Math.max(parsed, MIN_TIMEOUT), MAX_TIMEOUT[unit]);
    setDraft(String(clamped));
    if (clamped !== value) {
      updateSetting("mic_idle_timeout_value", clamped);
    }
  };

  if (!lazyClose) return null;

  const unitOptions: { value: string; label: string }[] = [
    { value: "seconds", label: t("settings.advanced.micIdleTimeout.seconds") },
    { value: "minutes", label: t("settings.advanced.micIdleTimeout.minutes") },
  ];

  return (
    <SettingsGroup title={t("settings.advanced.micIdleTimeout.title")}>
      <div className="space-y-3 px-4 p-2">
        <SettingContainer
          title={t("settings.advanced.micIdleTimeout.infiniteLabel")}
          description={t(
            "settings.advanced.micIdleTimeout.infiniteDescription",
          )}
          descriptionMode="tooltip"
          grouped
        >
          <ToggleSwitch
            checked={infinite}
            onChange={(enabled) => updateSetting("mic_idle_infinite", enabled)}
            isUpdating={isUpdating("mic_idle_infinite")}
            label={t("settings.advanced.micIdleTimeout.infiniteLabel")}
            description={t(
              "settings.advanced.micIdleTimeout.infiniteDescription",
            )}
            descriptionMode="tooltip"
            grouped
          />
        </SettingContainer>

        {!infinite && (
          <div className="flex items-end gap-2">
            <div className="flex-1">
              <SettingContainer
                title={t("settings.advanced.micIdleTimeout.timeoutLabel")}
                description={t(
                  "settings.advanced.micIdleTimeout.timeoutDescription",
                )}
                descriptionMode="tooltip"
                layout="stacked"
                grouped
              >
                <Input
                  type="number"
                  min={MIN_TIMEOUT}
                  max={MAX_TIMEOUT[unit]}
                  value={draft}
                  onChange={(e) => setDraft(e.target.value)}
                  onBlur={commitDraft}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      e.currentTarget.blur();
                    }
                  }}
                  disabled={isUpdating("mic_idle_timeout_value")}
                  variant="compact"
                />
              </SettingContainer>
            </div>
            <div className="w-32">
              <SettingContainer
                title={t("settings.advanced.micIdleTimeout.unitLabel")}
                description={t(
                  "settings.advanced.micIdleTimeout.unitDescription",
                )}
                descriptionMode="tooltip"
                grouped
              >
                <Dropdown
                  selectedValue={unit}
                  options={unitOptions}
                  onSelect={(val) =>
                    updateSetting("mic_idle_timeout_unit", val as TimeoutUnit)
                  }
                />
              </SettingContainer>
            </div>
          </div>
        )}
      </div>
    </SettingsGroup>
  );
};
