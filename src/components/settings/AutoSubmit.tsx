import { useTranslation } from "@/i18n/useTranslation";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { useSettings } from "../../hooks/useSettings";
import { useOsType } from "../../hooks/useOsType";
import type { AutoSubmitKey } from "@/bindings";

interface AutoSubmitProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

type AutoSubmitOptionValue = AutoSubmitKey | "off";

export const AutoSubmit = (props: AutoSubmitProps) => {
  const { t } = useTranslation();
  const osType = useOsType();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  const autoSubmitOptions = () => [
    {
      value: "off",
      label: t("settings.advanced.autoSubmit.options.off"),
    },
    {
      value: "enter",
      label: t("settings.advanced.autoSubmit.options.enter"),
    },
    {
      value: "ctrl_enter",
      label: t("settings.advanced.autoSubmit.options.ctrlEnter"),
    },
    {
      value: "cmd_enter",
      label:
        osType === "macos"
          ? t("settings.advanced.autoSubmit.options.cmdEnter")
          : t("settings.advanced.autoSubmit.options.superEnter"),
    },
  ];

  const handleAutoSubmitSelect = async (value: string) => {
    const selected = value as AutoSubmitOptionValue;

    if (selected === "off") {
      await updateSetting("auto_submit", false);
      return;
    }

    await updateSetting("auto_submit_key", selected as AutoSubmitKey);
    // Read at call time, not at mount: the toggle may have flipped since.
    if (!(getSetting("auto_submit") ?? false)) {
      await updateSetting("auto_submit", true);
    }
  };

  return (
    <SettingContainer
      title={t("settings.advanced.autoSubmit.title")}
      description={t("settings.advanced.autoSubmit.description")}
      descriptionMode={props.descriptionMode}
      grouped={props.grouped}
    >
      <Dropdown
        options={autoSubmitOptions()}
        selectedValue={
          (getSetting("auto_submit") ?? false)
            ? ((getSetting("auto_submit_key") || "enter") as AutoSubmitKey)
            : ("off" as AutoSubmitOptionValue)
        }
        onSelect={handleAutoSubmitSelect}
        disabled={isUpdating("auto_submit") || isUpdating("auto_submit_key")}
      />
    </SettingContainer>
  );
};
