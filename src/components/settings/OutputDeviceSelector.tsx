import { useTranslation } from "@/i18n/useTranslation";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { ResetButton } from "../ui/ResetButton";
import { useSettings } from "../../hooks/useSettings";
import type { AudioDevice } from "@/bindings";
import type { JSX } from "@solidjs/web";

interface OutputDeviceSelectorProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
  disabled?: boolean;
}

export const OutputDeviceSelector = (
  props: OutputDeviceSelectorProps,
): JSX.Element => {
  const { t } = useTranslation();
  const {
    getSetting,
    updateSetting,
    resetSetting,
    isUpdating,
    isLoading,
    outputDevices,
    refreshOutputDevices,
  } = useSettings();

  const selectedOutputDevice = () =>
    getSetting("selected_output_device") === "default"
      ? "Default"
      : getSetting("selected_output_device") || "Default";

  const handleOutputDeviceSelect = async (deviceName: string) => {
    await updateSetting("selected_output_device", deviceName);
  };

  const handleReset = async () => {
    await resetSetting("selected_output_device");
  };

  const outputDeviceOptions = () =>
    outputDevices().map((device: AudioDevice) => ({
      value: device.name,
      label: device.name,
    }));

  return (
    <SettingContainer
      title={t("settings.sound.outputDevice.title")}
      description={t("settings.sound.outputDevice.description")}
      descriptionMode={props.descriptionMode}
      grouped={props.grouped}
      disabled={props.disabled ?? false}
    >
      <div class="flex items-center space-x-1">
        <Dropdown
          options={outputDeviceOptions()}
          selectedValue={selectedOutputDevice()}
          onSelect={handleOutputDeviceSelect}
          placeholder={
            isLoading() || outputDevices().length === 0
              ? t("settings.sound.outputDevice.loading")
              : t("settings.sound.outputDevice.placeholder")
          }
          disabled={
            (props.disabled ?? false) ||
            isUpdating("selected_output_device") ||
            isLoading() ||
            outputDevices().length === 0
          }
          onRefresh={refreshOutputDevices}
        />
        <ResetButton
          onClick={handleReset}
          disabled={
            (props.disabled ?? false) ||
            isUpdating("selected_output_device") ||
            isLoading()
          }
        />
      </div>
    </SettingContainer>
  );
};
