import { createMemo } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { useSettings } from "../../hooks/useSettings";
import { commands, type ModelUnloadTimeout } from "@/bindings";
import type { JSX } from "@solidjs/web";

interface ModelUnloadTimeoutProps {
  descriptionMode?: "tooltip" | "inline";
  grouped?: boolean;
}

export const ModelUnloadTimeoutSetting = (
  props: ModelUnloadTimeoutProps,
): JSX.Element => {
  const { t } = useTranslation();
  const { settings, getSetting, updateSetting } = useSettings();

  // Built lazily for the same reason as everywhere else: `t` reads the
  // language signal, and a body-level array would freeze on language change.
  const timeoutOptions = () => [
    {
      value: "never",
      label: t("settings.advanced.modelUnload.options.never"),
    },
    {
      value: "immediately",
      label: t("settings.advanced.modelUnload.options.immediately"),
    },
    {
      value: "min2",
      label: t("settings.advanced.modelUnload.options.min2"),
    },
    {
      value: "min5",
      label: t("settings.advanced.modelUnload.options.min5"),
    },
    {
      value: "min10",
      label: t("settings.advanced.modelUnload.options.min10"),
    },
    {
      value: "min15",
      label: t("settings.advanced.modelUnload.options.min15"),
    },
    {
      value: "hour1",
      label: t("settings.advanced.modelUnload.options.hour1"),
    },
  ];

  const debugTimeoutOptions = () => [
    ...timeoutOptions(),
    {
      value: "sec15",
      label: t("settings.advanced.modelUnload.options.sec15"),
    },
  ];

  const handleChange = async (event: { target: { value: string } }) => {
    const newTimeout = event.target.value as ModelUnloadTimeout;

    try {
      await commands.setModelUnloadTimeout(newTimeout);
      updateSetting("model_unload_timeout", newTimeout);
    } catch (error) {
      console.error("Failed to update model unload timeout:", error);
    }
  };

  const options = createMemo(() => {
    return settings()?.debug_mode === true
      ? debugTimeoutOptions()
      : timeoutOptions();
  });

  return (
    <SettingContainer
      title={t("settings.advanced.modelUnload.title")}
      description={t("settings.advanced.modelUnload.description")}
      descriptionMode={props.descriptionMode}
      grouped={props.grouped}
    >
      <Dropdown
        options={options()}
        selectedValue={getSetting("model_unload_timeout") ?? "never"}
        onSelect={(value) =>
          handleChange({
            target: { value },
          })
        }
        disabled={false}
      />
    </SettingContainer>
  );
};
