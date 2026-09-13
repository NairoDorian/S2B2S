import { createSignal, createEffect, Show } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { SettingContainer, SettingsGroup } from "../ui";
import { Input } from "../ui/Input";
import { Dropdown } from "../ui/Dropdown";
import { useSettings } from "../../hooks/useSettings";

type TimeoutUnit = "seconds" | "minutes";

const MIN_TIMEOUT = 1;
const MAX_TIMEOUT: Record<TimeoutUnit, number> = {
  seconds: 86_400,
  minutes: 1_440,
};

export const MicIdleTimeout = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  // Accessors, not snapshots. `getSetting` reads the settings store, and a
  // Solid component body runs once — read eagerly and these would freeze at
  // whatever the store held before the settings IPC resolved.
  const lazyClose = () => getSetting("lazy_stream_close") ?? false;
  const value = () => getSetting("mic_idle_timeout_value") ?? 30;
  const unit = () =>
    (getSetting("mic_idle_timeout_unit") ?? "seconds") as TimeoutUnit;
  const infinite = () => getSetting("mic_idle_infinite") ?? false;

  const [draft, setDraft] = createSignal(String(value()));

  // Follow the stored value: it is the source of truth, so a change made
  // elsewhere — or a rejected write rolling back — has to land in the field.
  createEffect(
    () => value(),
    (current) => {
      setDraft(String(current));
    },
  );

  const commitDraft = () => {
    const parsed = parseInt(draft(), 10);
    if (isNaN(parsed)) {
      setDraft(String(value()));
      return;
    }
    const clamped = Math.min(
      Math.max(parsed, MIN_TIMEOUT),
      MAX_TIMEOUT[unit()],
    );
    setDraft(String(clamped));
    if (clamped !== value()) {
      updateSetting("mic_idle_timeout_value", clamped);
    }
  };

  const unitOptions = () => [
    { value: "seconds", label: t("settings.advanced.micIdleTimeout.seconds") },
    { value: "minutes", label: t("settings.advanced.micIdleTimeout.minutes") },
  ];

  return (
    <Show when={lazyClose()}>
      <SettingsGroup title={t("settings.advanced.micIdleTimeout.title")}>
        <div class="space-y-3 px-4 p-2">
          <SettingContainer
            title={t("settings.advanced.micIdleTimeout.infiniteLabel")}
            description={t(
              "settings.advanced.micIdleTimeout.infiniteDescription",
            )}
            descriptionMode="tooltip"
            grouped
          >
            <ToggleSwitch
              checked={infinite()}
              onChange={(enabled) =>
                updateSetting("mic_idle_infinite", enabled)
              }
              isUpdating={isUpdating("mic_idle_infinite")}
              label={t("settings.advanced.micIdleTimeout.infiniteLabel")}
              description={t(
                "settings.advanced.micIdleTimeout.infiniteDescription",
              )}
              descriptionMode="tooltip"
              grouped
            />
          </SettingContainer>

          <Show when={!infinite()}>
            <div class="flex items-end gap-2">
              <div class="flex-1">
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
                    max={MAX_TIMEOUT[unit()]}
                    value={draft()}
                    onInput={(e) => setDraft(e.target.value)}
                    onBlur={commitDraft}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") {
                        (e.currentTarget as HTMLInputElement).blur();
                      }
                    }}
                    disabled={isUpdating("mic_idle_timeout_value")}
                    variant="compact"
                  />
                </SettingContainer>
              </div>
              <div class="w-32">
                <SettingContainer
                  title={t("settings.advanced.micIdleTimeout.unitLabel")}
                  description={t(
                    "settings.advanced.micIdleTimeout.unitDescription",
                  )}
                  descriptionMode="tooltip"
                  grouped
                >
                  <Dropdown
                    selectedValue={unit()}
                    options={unitOptions()}
                    onSelect={(val) =>
                      updateSetting("mic_idle_timeout_unit", val as TimeoutUnit)
                    }
                  />
                </SettingContainer>
              </div>
            </div>
          </Show>
        </div>
      </SettingsGroup>
    </Show>
  );
};
