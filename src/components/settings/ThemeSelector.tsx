import { useTranslation } from "@/i18n/useTranslation";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { useSettings } from "@/hooks/useSettings";
import { applyTheme, THEME_OPTIONS } from "@/lib/utils/theme";
import type { Theme } from "@/bindings";
import type { JSX } from "@solidjs/web";

interface ThemeSelectorProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const ThemeSelector = (props: ThemeSelectorProps): JSX.Element => {
  const { t } = useTranslation();
  const { settings, updateSetting } = useSettings();

  const currentTheme = (): Theme => settings()?.theme ?? "system";

  const handleThemeChange = (value: string) => {
    const theme = value as Theme;
    applyTheme(theme);
    updateSetting("theme", theme);
  };

  return (
    <SettingContainer
      title={t("theme.title")}
      description={t("theme.description")}
      descriptionMode={props.descriptionMode}
      grouped={props.grouped}
    >
      <Dropdown
        options={THEME_OPTIONS.map((value) => ({
          value,
          label: t(`theme.options.${value}`),
        }))}
        selectedValue={currentTheme()}
        onSelect={handleThemeChange}
      />
    </SettingContainer>
  );
};
