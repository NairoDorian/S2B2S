import { createSignal, createEffect, Show } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import { commands } from "@/bindings";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { ResetButton } from "../ui/ResetButton";
import { useSettings } from "../../hooks/useSettings";

interface ClamshellMicrophoneSelectorProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const ClamshellMicrophoneSelector = (
  props: ClamshellMicrophoneSelectorProps,
) => {
  const { t } = useTranslation();
  const {
    getSetting,
    updateSetting,
    resetSetting,
    isUpdating,
    isLoading,
    audioDevices,
    refreshAudioDevices,
  } = useSettings();

  const [isLaptop, setIsLaptop] = createSignal<boolean>(false);

  createEffect(
    () => undefined,
    () => {
      commands
        .isLaptop()
        .then((result) => {
          setIsLaptop(result.status === "ok" ? result.data : false);
        })
        .catch((error) => {
          console.error("Failed to check if device is laptop:", error);
          setIsLaptop(false);
        });
    },
  );

  // An accessor, and the laptop check lives in the JSX: the body runs once,
  // before the `isLaptop` IPC has answered.
  const selectedClamshellMicrophone = () =>
    getSetting("clamshell_microphone") === "default"
      ? "Default"
      : getSetting("clamshell_microphone") || "Default";

  const handleClamshellMicrophoneSelect = async (deviceName: string) => {
    await updateSetting("clamshell_microphone", deviceName);
  };

  const handleReset = async () => {
    await resetSetting("clamshell_microphone");
  };

  const microphoneOptions = () =>
    audioDevices().map((device) => ({
      value: device.name,
      label: device.name,
    }));

  return (
    <Show when={isLaptop()}>
      <SettingContainer
        title={t("settings.debug.clamshellMicrophone.title")}
        description={t("settings.debug.clamshellMicrophone.description")}
        descriptionMode={props.descriptionMode ?? "tooltip"}
        grouped={props.grouped ?? false}
      >
        <div class="flex items-center space-x-1">
          <Dropdown
            options={microphoneOptions()}
            selectedValue={selectedClamshellMicrophone()}
            onSelect={handleClamshellMicrophoneSelect}
            placeholder={
              isLoading() || audioDevices().length === 0
                ? t("common.loading")
                : t("settings.sound.microphone.placeholder")
            }
            disabled={
              isUpdating("clamshell_microphone") ||
              isLoading() ||
              audioDevices().length === 0
            }
            onRefresh={refreshAudioDevices}
          />
          <ResetButton
            onClick={handleReset}
            disabled={isUpdating("clamshell_microphone") || isLoading()}
          />
        </div>
      </SettingContainer>
    </Show>
  );
};
