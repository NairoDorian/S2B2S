import { useTranslation } from "@/i18n/useTranslation";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { useSettings } from "../../hooks/useSettings";
import type { ShortcutActivation } from "@/bindings";
import type { JSX } from "@solidjs/web";

interface ShortcutActivationProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const ShortcutActivationSetting = (
  props: ShortcutActivationProps,
): JSX.Element => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  // Built lazily: each label calls `t`, which reads the language signal — a
  // body-level array would read it untracked and freeze on language change.
  const options = () => [
    {
      value: "hold_or_toggle",
      label: t("settings.general.shortcutActivation.options.holdOrToggle"),
      description: t(
        "settings.general.shortcutActivation.descriptions.hold_or_toggle",
      ),
    },
    {
      value: "push_to_talk",
      label: t("settings.general.shortcutActivation.options.pushToTalk"),
      description: t(
        "settings.general.shortcutActivation.descriptions.push_to_talk",
      ),
    },
    {
      value: "toggle",
      label: t("settings.general.shortcutActivation.options.toggle"),
      description: t("settings.general.shortcutActivation.descriptions.toggle"),
    },
  ];

  return (
    <SettingContainer
      title={t("settings.general.shortcutActivation.title")}
      description={t("settings.general.shortcutActivation.description")}
      descriptionMode={props.descriptionMode}
      grouped={props.grouped}
    >
      <Dropdown
        options={options()}
        menuClassName="right-0 w-80 max-w-[calc(100vw-2rem)]"
        selectedValue={
          (getSetting("shortcut_activation") ||
            "hold_or_toggle") as ShortcutActivation
        }
        onSelect={(value) =>
          updateSetting("shortcut_activation", value as ShortcutActivation)
        }
        disabled={isUpdating("shortcut_activation")}
      />
    </SettingContainer>
  );
};
