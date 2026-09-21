import { useTranslation } from "@/i18n/useTranslation";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { Slider } from "../ui/Slider";
import { useSettings } from "../../hooks/useSettings";
import type { OverlayPosition, OverlayStyle } from "@/bindings";
import type { JSX } from "@solidjs/web";

interface ShowOverlayProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const ShowOverlay = (props: ShowOverlayProps): JSX.Element => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  // All setting reads live in the JSX bindings: the selected style drives the
  // conditional sections below it, so a body-level snapshot would freeze the
  // whole group at whatever was set on mount.
  const styleOptions = () => [
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

  const positionOptions = () => [
    {
      value: "bottom",
      label: t("settings.advanced.overlay.position.options.bottom"),
    },
    {
      value: "top",
      label: t("settings.advanced.overlay.position.options.top"),
    },
  ];

  return (
    <>
      <SettingContainer
        title={t("settings.advanced.overlay.style.title")}
        description={t("settings.advanced.overlay.style.description")}
        descriptionMode={props.descriptionMode}
        grouped={props.grouped}
      >
        <Dropdown
          options={styleOptions()}
          selectedValue={
            (getSetting("overlay_style") || "live") as OverlayStyle
          }
          onSelect={(value) =>
            updateSetting("overlay_style", value as OverlayStyle)
          }
          disabled={isUpdating("overlay_style")}
        />
      </SettingContainer>

      {getSetting("overlay_style") !== "none" && (
        <SettingContainer
          title={t("settings.advanced.overlay.position.title")}
          description={t("settings.advanced.overlay.position.description")}
          descriptionMode={props.descriptionMode}
          grouped={props.grouped}
        >
          <Dropdown
            options={positionOptions()}
            selectedValue={
              (getSetting("overlay_position") === "top"
                ? "top"
                : "bottom") as OverlayPosition
            }
            onSelect={(value) =>
              updateSetting("overlay_position", value as OverlayPosition)
            }
            disabled={isUpdating("overlay_position")}
          />
        </SettingContainer>
      )}

      {getSetting("overlay_style") === "live" && (
        <ToggleSwitch
          checked={getSetting("overlay_direct_mode") ?? false}
          onChange={(enabled) => updateSetting("overlay_direct_mode", enabled)}
          isUpdating={isUpdating("overlay_direct_mode")}
          label={t("settings.advanced.overlay.directMode.label")}
          description={t("settings.advanced.overlay.directMode.description")}
          descriptionMode={props.descriptionMode}
          grouped={props.grouped}
        />
      )}

      {getSetting("overlay_style") === "live" &&
        (getSetting("overlay_direct_mode") ?? false) && (
          <Slider
            value={getSetting("overlay_direct_speed") ?? 30}
            onChange={(value) =>
              updateSetting("overlay_direct_speed", Math.round(value))
            }
            min={10}
            max={60}
            step={5}
            label={t("settings.advanced.overlay.directSpeed.label")}
            description={t("settings.advanced.overlay.directSpeed.description")}
            descriptionMode={props.descriptionMode}
            grouped={props.grouped}
            formatValue={(v) => `${Math.round(v)} chars/s`}
            onReset={() => updateSetting("overlay_direct_speed", 30)}
            disabled={isUpdating("overlay_direct_speed")}
          />
        )}

      {/* Sits under the typewriter speed because it only means anything while
          the typewriter is running: with direct mode off the preview already
          applies every update as a block, so there is nothing to allow. */}
      {getSetting("overlay_style") === "live" &&
        (getSetting("overlay_direct_mode") ?? false) && (
          <ToggleSwitch
            checked={getSetting("overlay_back_correction") ?? false}
            onChange={(enabled) =>
              updateSetting("overlay_back_correction", enabled)
            }
            isUpdating={isUpdating("overlay_back_correction")}
            label={t("settings.advanced.overlay.backCorrection.label")}
            description={t(
              "settings.advanced.overlay.backCorrection.description",
            )}
            descriptionMode={props.descriptionMode}
            grouped={props.grouped}
          />
        )}
    </>
  );
};
