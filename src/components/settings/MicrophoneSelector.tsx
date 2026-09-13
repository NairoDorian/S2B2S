import { useTranslation } from "@/i18n/useTranslation";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { ResetButton } from "../ui/ResetButton";
import { useSettings } from "../../hooks/useSettings";
import type { JSX } from "@solidjs/web";

interface MicrophoneSelectorProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const MicrophoneSelector = (
  props: MicrophoneSelectorProps,
): JSX.Element => {
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

  const selectedMicrophone = () =>
    getSetting("selected_microphone") === "default"
      ? "Default"
      : getSetting("selected_microphone") || "Default";

  const handleMicrophoneSelect = async (deviceName: string) => {
    await updateSetting("selected_microphone", deviceName);
  };

  const handleReset = async () => {
    await resetSetting("selected_microphone");
  };

  const microphoneOptions = () =>
    audioDevices().map((device) => ({
      value: device.name,
      label: device.name,
    }));

  return (
    <SettingContainer
      title={t("settings.sound.microphone.title")}
      description={t("settings.sound.microphone.description")}
      descriptionMode={props.descriptionMode}
      grouped={props.grouped}
    >
      <div class="flex items-center space-x-1">
        <Dropdown
          options={microphoneOptions()}
          selectedValue={selectedMicrophone()}
          onSelect={handleMicrophoneSelect}
          placeholder={
            isLoading() || audioDevices().length === 0
              ? t("settings.sound.microphone.loading")
              : t("settings.sound.microphone.placeholder")
          }
          disabled={
            isUpdating("selected_microphone") ||
            isLoading() ||
            audioDevices().length === 0
          }
          onRefresh={refreshAudioDevices}
        />
        <ResetButton
          onClick={handleReset}
          disabled={isUpdating("selected_microphone") || isLoading()}
        />
      </div>
    </SettingContainer>
  );
};
