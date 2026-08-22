import React from "react";
import { useTranslation } from "react-i18next";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { Slider } from "../ui/Slider";
import { useSettings } from "../../hooks/useSettings";
import type { OverlayPosition, OverlayStyle } from "@/bindings";

interface ShowOverlayProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const ShowOverlay: React.FC<ShowOverlayProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    const styleOptions = [
      {
        value: "none",
        label: t("settings.advanced.overlay.style.options.none"),
      },
      {
        value: "minimal",
        label: t("settings.advanced.overlay.style.options.minimal"),
      },
      {
        value: "live",
        label: t("settings.advanced.overlay.style.options.live"),
      },
    ];

    const positionOptions = [
      {
        value: "bottom",
        label: t("settings.advanced.overlay.position.options.bottom"),
      },
      {
        value: "top",
        label: t("settings.advanced.overlay.position.options.top"),
      },
    ];

    const selectedStyle = (getSetting("overlay_style") ||
      "live") as OverlayStyle;
    // Only "top" and "bottom" are selectable; anything else (empty, or a legacy
    // "none" from before the position was retired) falls back to "bottom".
    const selectedPosition: OverlayPosition =
      getSetting("overlay_position") === "top" ? "top" : "bottom";
    const directMode = getSetting("overlay_direct_mode") ?? false;
    const directSpeed = getSetting("overlay_direct_speed") ?? 30;

    return (
      <>
        <SettingContainer
          title={t("settings.advanced.overlay.style.title")}
          description={t("settings.advanced.overlay.style.description")}
          descriptionMode={descriptionMode}
          grouped={grouped}
        >
          <Dropdown
            options={styleOptions}
            selectedValue={selectedStyle}
            onSelect={(value) =>
              updateSetting("overlay_style", value as OverlayStyle)
            }
            disabled={isUpdating("overlay_style")}
          />
        </SettingContainer>

        {selectedStyle !== "none" && (
          <SettingContainer
            title={t("settings.advanced.overlay.position.title")}
            description={t("settings.advanced.overlay.position.description")}
            descriptionMode={descriptionMode}
            grouped={grouped}
          >
            <Dropdown
              options={positionOptions}
              selectedValue={selectedPosition}
              onSelect={(value) =>
                updateSetting("overlay_position", value as OverlayPosition)
              }
              disabled={isUpdating("overlay_position")}
            />
          </SettingContainer>
        )}

        {selectedStyle === "live" && (
          <ToggleSwitch
            checked={directMode}
            onChange={(enabled) =>
              updateSetting("overlay_direct_mode", enabled)
            }
            isUpdating={isUpdating("overlay_direct_mode")}
            label={t("settings.advanced.overlay.directMode.label")}
            description={t("settings.advanced.overlay.directMode.description")}
            descriptionMode={descriptionMode}
            grouped={grouped}
          />
        )}

        {selectedStyle === "live" && directMode && (
          <Slider
            value={directSpeed}
            onChange={(value) =>
              updateSetting("overlay_direct_speed", Math.round(value))
            }
            min={10}
            max={60}
            step={5}
            label={t("settings.advanced.overlay.directSpeed.label")}
            description={t("settings.advanced.overlay.directSpeed.description")}
            descriptionMode={descriptionMode}
            grouped={grouped}
            formatValue={(v) => `${Math.round(v)} chars/s`}
            onReset={() => updateSetting("overlay_direct_speed", 30)}
            disabled={isUpdating("overlay_direct_speed")}
          />
        )}
      </>
    );
  },
);
