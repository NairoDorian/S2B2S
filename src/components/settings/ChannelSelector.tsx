import { createSignal, createEffect, Show } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { commands } from "@/bindings";
import { useSettings } from "../../hooks/useSettings";

interface ChannelSelectorProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const ChannelSelector = (props: ChannelSelectorProps) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating, isLoading } = useSettings();
  const [channelCount, setChannelCount] = createSignal(1);

  createEffect(
    // Re-fetch the channel count whenever the selected microphone changes.
    () => getSetting("selected_microphone") || "default",
    (selectedMicrophone) => {
      let cancelled = false;
      setChannelCount(1);

      const fetchChannels = async () => {
        try {
          const deviceName =
            selectedMicrophone === "Default" ? "default" : selectedMicrophone;
          const result = await commands.getMicrophoneChannels(deviceName);
          if (!cancelled && result.status === "ok") {
            setChannelCount(result.data);
          }
        } catch (error) {
          console.error("Failed to get microphone channel count:", error);
        }
      };

      void fetchChannels();
      return () => {
        cancelled = true;
      };
    },
  );

  const handleChannelSelect = async (value: string) => {
    const channel = value === "average" ? null : parseInt(value, 10);
    await updateSetting("selected_channel", channel);
  };

  // Built live: the option list follows the fetched channel count and the
  // selected value follows the setting.
  const options = () => [
    { value: "average", label: t("settings.sound.channel.average") },
    ...Array.from({ length: channelCount() }, (_, index) => ({
      value: index.toString(),
      label: t("settings.sound.channel.channel", { n: index + 1 }),
    })),
  ];

  const currentValue = () => {
    const selectedChannel = getSetting("selected_channel");
    return selectedChannel == null || selectedChannel >= channelCount()
      ? "average"
      : selectedChannel.toString();
  };

  return (
    <Show when={channelCount() > 1}>
      <SettingContainer
        title={t("settings.sound.channel.title")}
        description={t("settings.sound.channel.description")}
        descriptionMode={props.descriptionMode}
        grouped={props.grouped}
      >
        <Dropdown
          options={options()}
          selectedValue={currentValue()}
          onSelect={handleChannelSelect}
          disabled={isUpdating("selected_channel") || isLoading()}
        />
      </SettingContainer>
    </Show>
  );
};
