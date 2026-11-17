import type { JSX } from "@solidjs/web";
import { useTranslation } from "@/i18n/useTranslation";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { useSettings } from "../../hooks/useSettings";
import type { ChineseScript } from "@/bindings";

interface ChineseScriptProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

/**
 * How Mandarin and Cantonese output is written (`chinese_script`): Simplified,
 * Traditional with Taiwan's or Hong Kong's forms, or as the model wrote it.
 * The backend decides from the text whether it is Chinese, so English and
 * Japanese in the same dictation are left alone.
 */
export const ChineseScriptSetting = (
  props: ChineseScriptProps,
): JSX.Element => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  // The script's own name leads, so it reads the same in every language.
  const options = () => [
    {
      value: "simplified",
      label: `简体 ${t("settings.advanced.chineseScript.options.simplified")}`,
    },
    {
      value: "traditional_tw",
      label: `繁體 ${t("settings.advanced.chineseScript.options.traditionalTw")}`,
    },
    {
      value: "traditional_hk",
      label: `繁體 ${t("settings.advanced.chineseScript.options.traditionalHk")}`,
    },
    {
      value: "as_transcribed",
      label: t("settings.advanced.chineseScript.options.asTranscribed"),
    },
  ];

  return (
    <SettingContainer
      title={t("settings.advanced.chineseScript.title")}
      description={t("settings.advanced.chineseScript.description")}
      descriptionMode={props.descriptionMode ?? "tooltip"}
      grouped={props.grouped ?? false}
    >
      <Dropdown
        options={options()}
        selectedValue={getSetting("chinese_script") ?? "as_transcribed"}
        onSelect={(value) =>
          void updateSetting("chinese_script", value as ChineseScript)
        }
        disabled={isUpdating("chinese_script")}
      />
    </SettingContainer>
  );
};
