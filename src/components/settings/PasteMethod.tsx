import React from "react";
import { useTranslation } from "react-i18next";
import { Dropdown, type DropdownOption } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { Slider } from "../ui/Slider";
import { Input } from "../ui/Input";
import { useSettings } from "../../hooks/useSettings";
import { useOsType } from "../../hooks/useOsType";
import type { PasteMethod } from "@/bindings";

interface PasteMethodProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const PasteMethodSetting: React.FC<PasteMethodProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();
    const osType = useOsType();

    const selectedMethod = (getSetting("paste_method") ||
      "ctrl_v") as PasteMethod;
    const directStreamingSpeed = getSetting("direct_streaming_speed") ?? 30;

    const getPasteMethodOptions = (osType: string) => {
      const mod = osType === "macos" ? "Cmd" : "Ctrl";

      const options: DropdownOption[] = [
        {
          value: "ctrl_v",
          label: t("settings.advanced.pasteMethod.options.clipboard", {
            modifier: mod,
          }),
        },
      ];

      // Direct input is not offered on macOS, but keep an existing/manual
      // selection visible so the UI accurately represents the saved setting.
      if (osType !== "macos" || selectedMethod === "direct") {
        options.push({
          value: "direct",
          label: t("settings.advanced.pasteMethod.options.direct"),
          disabled: osType === "macos",
        });
      }

      // Direct streaming option for live streaming models
      if (osType !== "macos" || selectedMethod === "direct_streaming") {
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
            label: t(
              "settings.advanced.pasteMethod.options.clipboardCtrlShiftV",
            ),
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

    const externalScriptPath = getSetting("external_script_path") || "";

    const pasteMethodOptions = getPasteMethodOptions(osType);

    return (
      <>
        <SettingContainer
          title={t("settings.advanced.pasteMethod.title")}
          description={t("settings.advanced.pasteMethod.description")}
          descriptionMode={descriptionMode}
          grouped={grouped}
          tooltipPosition="bottom"
        >
          <div className="flex flex-col gap-2">
            <Dropdown
              options={pasteMethodOptions}
              selectedValue={selectedMethod}
              onSelect={(value) =>
                updateSetting("paste_method", value as PasteMethod)
              }
              disabled={isUpdating("paste_method")}
            />
            {selectedMethod === "external_script" && (
              <Input
                type="text"
                value={externalScriptPath}
                onChange={(e) =>
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

        {selectedMethod === "direct_streaming" && (
          <Slider
            value={directStreamingSpeed}
            onChange={(value) =>
              updateSetting("direct_streaming_speed", Math.round(value))
            }
            min={10}
            max={60}
            step={5}
            label={t(
              "settings.advanced.pasteMethod.directStreamingSpeed.label",
            )}
            description={t(
              "settings.advanced.pasteMethod.directStreamingSpeed.description",
            )}
            descriptionMode={descriptionMode}
            grouped={grouped}
            formatValue={(v) => `${Math.round(v)} chars/s`}
            onReset={() => updateSetting("direct_streaming_speed", 30)}
            disabled={isUpdating("direct_streaming_speed")}
          />
        )}
      </>
    );
  },
);
