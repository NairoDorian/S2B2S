import { useTranslation } from "@/i18n/useTranslation";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { Slider } from "../ui/Slider";
import { Input } from "../ui/Input";
import { useSettings } from "../../hooks/useSettings";
import { useOsType } from "../../hooks/useOsType";
import type { PasteMethod } from "@/bindings";
import type { JSX } from "@solidjs/web";

interface PasteMethodProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const PasteMethodSetting = (props: PasteMethodProps): JSX.Element => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const osType = useOsType();

  // Built in the JSX bindings: the option list depends on the live selected
  // method (an out-of-range selection stays visible) and the platform.
  const getPasteMethodOptions = () => {
    const method = (getSetting("paste_method") || "ctrl_v") as PasteMethod;
    const mod = osType === "macos" ? "Cmd" : "Ctrl";

    const options: { value: string; label: string; disabled?: boolean }[] = [
      {
        value: "ctrl_v",
        label: t("settings.advanced.pasteMethod.options.clipboard", {
          modifier: mod,
        }),
      },
    ];

    // Direct input is not offered on macOS, but keep an existing/manual
    // selection visible so the UI accurately represents the saved setting.
    if (osType !== "macos" || method === "direct") {
      options.push({
        value: "direct",
        label: t("settings.advanced.pasteMethod.options.direct"),
        disabled: osType === "macos",
      });
    }

    // Direct streaming option for live streaming models
    if (osType !== "macos" || method === "direct_streaming") {
      options.push({
        value: "direct_streaming",
        label: t("settings.advanced.pasteMethod.options.directStreaming"),
        disabled: osType === "macos",
      });
    }

    options.push({
      value: "none",
      label: t("settings.advanced.pasteMethod.options.none"),
    });

    // Add Shift+Insert and Ctrl+Shift+V options for Windows and Linux only
    if (osType === "windows" || osType === "linux") {
      options.push(
        {
          value: "ctrl_shift_v",
          label: t("settings.advanced.pasteMethod.options.clipboardCtrlShiftV"),
        },
        {
          value: "shift_insert",
          label: t(
            "settings.advanced.pasteMethod.options.clipboardShiftInsert",
          ),
        },
      );
    }

    // External script is only available on Linux
    if (osType === "linux") {
      options.push({
        value: "external_script",
        label: t("settings.advanced.pasteMethod.options.externalScript"),
      });
    }

    return options;
  };

  return (
    <>
      <SettingContainer
        title={t("settings.advanced.pasteMethod.title")}
        description={t("settings.advanced.pasteMethod.description")}
        descriptionMode={props.descriptionMode}
        grouped={props.grouped}
        tooltipPosition="bottom"
      >
        <div class="flex flex-col gap-2">
          <Dropdown
            options={getPasteMethodOptions()}
            selectedValue={
              (getSetting("paste_method") || "ctrl_v") as PasteMethod
            }
            onSelect={(value) =>
              updateSetting("paste_method", value as PasteMethod)
            }
            disabled={isUpdating("paste_method")}
          />
          {getSetting("paste_method") === "external_script" && (
            <Input
              type="text"
              value={getSetting("external_script_path") || ""}
              onInput={(e) =>
                updateSetting("external_script_path", e.target.value)
              }
              placeholder={t(
                "settings.advanced.pasteMethod.externalScriptPlaceholder",
              )}
              disabled={isUpdating("external_script_path")}
            />
          )}
        </div>
      </SettingContainer>

      {getSetting("paste_method") === "direct_streaming" && (
        <Slider
          value={getSetting("direct_streaming_speed") ?? 30}
          onChange={(value) =>
            updateSetting("direct_streaming_speed", Math.round(value))
          }
          min={10}
          max={60}
          step={5}
          label={t("settings.advanced.pasteMethod.directStreamingSpeed.label")}
          description={t(
            "settings.advanced.pasteMethod.directStreamingSpeed.description",
          )}
          descriptionMode={props.descriptionMode}
          grouped={props.grouped}
          formatValue={(v) =>
            t("common.charsPerSecond", { value: Math.round(v) })
          }
          onReset={() => updateSetting("direct_streaming_speed", 30)}
          disabled={isUpdating("direct_streaming_speed")}
        />
      )}
    </>
  );
};
