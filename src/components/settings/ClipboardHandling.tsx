import { useTranslation } from "@/i18n/useTranslation";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { useSettings } from "../../hooks/useSettings";
import type { ClipboardHandling } from "@/bindings";

interface ClipboardHandlingProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const ClipboardHandlingSetting = (props: ClipboardHandlingProps) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  const clipboardHandlingOptions = [
    {
      value: "dont_modify",
      label: t("settings.advanced.clipboardHandling.options.dontModify"),
    },
    {
      value: "copy_to_clipboard",
      label: t("settings.advanced.clipboardHandling.options.copyToClipboard"),
    },
  ];

  return (
    <SettingContainer
      title={t("settings.advanced.clipboardHandling.title")}
      description={t("settings.advanced.clipboardHandling.description")}
      descriptionMode={props.descriptionMode}
      grouped={props.grouped}
    >
      <Dropdown
        options={clipboardHandlingOptions}
        selectedValue={
          (getSetting("clipboard_handling") ||
            "dont_modify") as ClipboardHandling
        }
        onSelect={(value) =>
          updateSetting("clipboard_handling", value as ClipboardHandling)
        }
        disabled={isUpdating("clipboard_handling")}
      />
    </SettingContainer>
  );
};
