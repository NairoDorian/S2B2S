import React from "react";
import { useTranslation } from "react-i18next";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { SettingsGroup } from "../ui/SettingsGroup";
import { useSettings } from "../../hooks/useSettings";
import type { OverlayMode } from "@/bindings";

interface OverlayWindowSettingsProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const OverlayModeSelector: React.FC<OverlayWindowSettingsProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    const currentMode = (getSetting("overlay_window")?.mode || "osNative") as OverlayMode;

    const options = [
      { value: "tauri", label: t("settings.advanced.overlayWindow.mode.options.tauri") },
      { value: "osNative", label: t("settings.advanced.overlayWindow.mode.options.osNative") },
    ];

    return (
      <SettingContainer
        title={t("settings.advanced.overlayWindow.mode.title")}
        description={t("settings.advanced.overlayWindow.mode.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      >
        <Dropdown
          options={options}
          selectedValue={currentMode}
          onSelect={async (value) => {
            const cfg = getSetting("overlay_window") || {};
            await updateSetting("overlay_window", { ...cfg, mode: value as OverlayMode });
          }}
          disabled={isUpdating("overlay_window")}
        />
      </SettingContainer>
    );
  },
);

export const OverlayReplyBubbleToggle: React.FC<OverlayWindowSettingsProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    const cfg = getSetting("overlay_window") || {};
    const checked = cfg.reply_bubble ?? false;

    return (
      <ToggleSwitch
        checked={checked}
        onChange={async (enabled) => {
          await updateSetting("overlay_window", { ...cfg, reply_bubble: enabled });
        }}
        isUpdating={isUpdating("overlay_window")}
        label={t("settings.advanced.overlayWindow.replyBubble.title")}
        description={t("settings.advanced.overlayWindow.replyBubble.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      />
    );
  },
);

const OverlayWindowSettings: React.FC = () => {
  const { t } = useTranslation();

  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      <SettingsGroup title={t("settings.advanced.overlayWindow.groups.behavior")}>
        <OverlayModeSelector descriptionMode="tooltip" grouped={true} />
        <OverlayReplyBubbleToggle descriptionMode="tooltip" grouped={true} />
      </SettingsGroup>
    </div>
  );
};

export default OverlayWindowSettings;
