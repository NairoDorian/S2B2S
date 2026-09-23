import { untrack } from "solid-js";
import { useSettingsStore } from "../../stores/settingsStore";
import { useSettings } from "../../hooks/useSettings";
import { useTranslation } from "@/i18n/useTranslation";
import { Button } from "../ui/Button";
import { Dropdown, DropdownOption } from "../ui/Dropdown";
import { PlayIcon } from "@/components/icons/lucide";
import { SettingContainer } from "../ui/SettingContainer";
import type { JSX } from "@solidjs/web";

interface SoundPickerProps {
  label: string;
  description: string;
}

export const SoundPicker = (props: SoundPickerProps): JSX.Element => {
  const { t } = useTranslation();
  const { getSetting, updateSetting } = useSettings();
  const store = useSettingsStore();
  // The action ref is a stable function; snapshot it explicitly. The custom
  // sounds read stays live inside the JSX so a newly detected custom theme
  // appears without a remount.
  const playTestSound = untrack(() => store.playTestSound);

  const options = (): DropdownOption[] => {
    const list: DropdownOption[] = [
      {
        value: "marimba",
        label: t("settings.debug.soundTheme.options.marimba"),
      },
      { value: "pop", label: t("settings.debug.soundTheme.options.pop") },
    ];
    if (store.customSounds.start && store.customSounds.stop) {
      list.push({
        value: "custom",
        label: t("settings.debug.soundTheme.options.custom"),
      });
    }
    return list;
  };

  const handlePlayBothSounds = async () => {
    await playTestSound("start");
    await playTestSound("stop");
  };

  return (
    <SettingContainer
      title={props.label}
      description={props.description}
      grouped
      layout="horizontal"
    >
      <div class="flex items-center gap-2">
        <Dropdown
          selectedValue={getSetting("sound_theme") ?? "marimba"}
          onSelect={(value) =>
            updateSetting("sound_theme", value as "marimba" | "pop" | "custom")
          }
          options={options()}
        />
        <Button
          variant="ghost"
          size="sm"
          onClick={handlePlayBothSounds}
          title={t("settings.debug.soundTheme.preview")}
          aria-label={t("settings.debug.soundTheme.preview")}
        >
          <PlayIcon class="h-4 w-4" />
        </Button>
      </div>
    </SettingContainer>
  );
};
