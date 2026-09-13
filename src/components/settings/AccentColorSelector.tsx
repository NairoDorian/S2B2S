import { For } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import { Check, Pipette } from "@/components/icons/lucide";
import { SettingContainer } from "../ui/SettingContainer";
import { ResetButton } from "../ui/ResetButton";
import { useSettings } from "@/hooks/useSettings";
import {
  ACCENT_PRESETS,
  DEFAULT_ACCENT_COLOR,
  parseHex,
} from "@/lib/utils/color";
import { applyAccentColor } from "@/lib/utils/theme";

interface AccentColorSelectorProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const AccentColorSelector = (props: AccentColorSelectorProps) => {
  const { descriptionMode = "tooltip", grouped = false } = props;
  const { t } = useTranslation();
  const { settings, updateSetting, isUpdating } = useSettings();
  let colorInputRef: HTMLInputElement | undefined;

  const currentColor = () =>
    (settings()?.custom_accent_color || DEFAULT_ACCENT_COLOR).toUpperCase();

  const isPreset = () =>
    ACCENT_PRESETS.some(
      (preset) => preset.hex.toUpperCase() === currentColor(),
    );

  const handleSelectColor = async (hex: string) => {
    if (!parseHex(hex)) return;
    applyAccentColor(hex, true);
    await updateSetting("custom_accent_color", hex);
  };

  const handleReset = async () => {
    applyAccentColor(DEFAULT_ACCENT_COLOR, true);
    await updateSetting("custom_accent_color", null);
  };

  return (
    <SettingContainer
      title={t("accentColor.title")}
      description={t("accentColor.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
      layout="horizontal"
    >
      <div class="flex items-center gap-2 flex-wrap">
        <For each={ACCENT_PRESETS} keyed={(preset) => preset.id}>
          {(preset) => {
            const isSelected = () =>
              preset().hex.toUpperCase() === currentColor();
            return (
              <button
                type="button"
                title={t(`accentColor.presets.${preset().id}`, {
                  defaultValue: preset().name,
                })}
                aria-label={t(`accentColor.presets.${preset().id}`, {
                  defaultValue: preset().name,
                })}
                onClick={() => handleSelectColor(preset().hex)}
                class="w-6 h-6 rounded-full flex items-center justify-center transition-transform hover:scale-110 active:scale-95 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-offset-background cursor-pointer"
                style={{
                  "background-color": preset().hex,
                }}
              >
                {isSelected() && (
                  <Check
                    class="w-3.5 h-3.5 text-white drop-shadow-md stroke-[2.5]"
                    aria-hidden="true"
                  />
                )}
              </button>
            );
          }}
        </For>

        <div class="relative flex items-center">
          <input
            ref={(el) => (colorInputRef = el)}
            type="color"
            value={
              currentColor().startsWith("#")
                ? currentColor()
                : DEFAULT_ACCENT_COLOR
            }
            onInput={(e) => handleSelectColor(e.target.value)}
            class="absolute inset-0 opacity-0 w-full h-full cursor-pointer z-10"
            aria-label={t("accentColor.custom")}
          />
          <button
            type="button"
            title={t("accentColor.custom")}
            aria-label={t("accentColor.custom")}
            onClick={() => colorInputRef?.click()}
            class={`w-6 h-6 rounded-full flex items-center justify-center transition-transform hover:scale-110 active:scale-95 border border-mid-gray/40 cursor-pointer ${
              !isPreset()
                ? "ring-2 ring-offset-2 ring-offset-background ring-accent"
                : ""
            }`}
            style={{
              background: !isPreset()
                ? currentColor()
                : "conic-gradient(from 180deg at 50% 50%, #f43f5e 0deg, #f59e0b 60deg, #10b981 120deg, #0ea5e9 180deg, #6366f1 240deg, #a855f7 300deg, #f43f5e 360deg)",
            }}
          >
            {!isPreset() ? (
              <Check
                class="w-3.5 h-3.5 text-white drop-shadow-md stroke-[2.5]"
                aria-hidden="true"
              />
            ) : (
              <Pipette
                class="w-3 h-3 text-white drop-shadow stroke-[2]"
                aria-hidden="true"
              />
            )}
          </button>
        </div>

        <ResetButton
          onClick={handleReset}
          disabled={
            isUpdating("custom_accent_color") ||
            currentColor() === DEFAULT_ACCENT_COLOR.toUpperCase()
          }
          ariaLabel={t("accentColor.reset")}
        />
      </div>
    </SettingContainer>
  );
};
