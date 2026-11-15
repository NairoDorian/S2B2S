import { For } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import { useSettings } from "@/hooks/useSettings";
import { SettingsGroup } from "@/components/ui/SettingsGroup";
import { SettingContainer } from "@/components/ui/SettingContainer";
import { Select } from "@/components/ui/Select";
import { Button } from "@/components/ui/Button";
import {
  DEFAULT_TYPOGRAPHY,
  FONT_ROLES,
  fontStack,
  normalizeFont,
  normalizeTypography,
  type FontRole,
} from "@/lib/typography";

const NAMED_FONTS = [
  "Arial",
  "Georgia",
  "Verdana",
  "Segoe UI",
  "Consolas",
  "Menlo",
];

export const FontSettings = () => {
  const { t } = useTranslation();
  const { settings, updateSetting, isUpdating } = useSettings();
  const fonts = () => normalizeTypography(settings()?.typography);
  const change = (role: FontRole, value: string) =>
    updateSetting("typography", {
      ...fonts(),
      [role]: normalizeFont(value, role),
    });
  const options = (role: FontRole) => [
    ...(role === "ui"
      ? []
      : [{ value: "inherit", label: t("fonts.options.inherit") }]),
    { value: "Libron", label: t("fonts.options.libron") },
    { value: "system-sans", label: t("fonts.options.sans") },
    { value: "system-serif", label: t("fonts.options.serif") },
    { value: "system-mono", label: t("fonts.options.mono") },
    ...NAMED_FONTS.map((value) => ({ value, label: value })),
  ];

  return (
    <SettingsGroup
      title={t("fonts.title")}
      description={t("fonts.description")}
    >
      <For each={FONT_ROLES}>
        {(role) => (
          <SettingContainer
            title={t(`fonts.roles.${role}.title`)}
            description={t(`fonts.roles.${role}.description`)}
            grouped
            layout="stacked"
          >
            <div class="grid gap-3 sm:grid-cols-2 sm:items-center">
              <Select
                value={fonts()[role]}
                options={options(role)}
                onChange={(value) =>
                  change(role, value ?? DEFAULT_TYPOGRAPHY[role])
                }
                ariaLabel={t(`fonts.roles.${role}.title`)}
                disabled={isUpdating("typography")}
                isClearable={false}
                isCreatable
                onCreateOption={(value) => change(role, value)}
                formatCreateLabel={(name) => t("fonts.useFont", { name })}
              />
              <p
                class="min-w-0 break-words text-sm leading-relaxed text-text/80"
                data-font-preview={role}
                style={{ "font-family": fontStack(fonts()[role], fonts().ui) }}
              >
                {t("fonts.preview")}
              </p>
            </div>
          </SettingContainer>
        )}
      </For>
      <div class="px-4 py-3 space-y-3">
        <p class="text-xs leading-relaxed text-mid-gray">{t("fonts.note")}</p>
        <Button
          variant="secondary"
          size="sm"
          disabled={isUpdating("typography")}
          onClick={() => updateSetting("typography", { ...DEFAULT_TYPOGRAPHY })}
        >
          {t("fonts.reset")}
        </Button>
      </div>
    </SettingsGroup>
  );
};
