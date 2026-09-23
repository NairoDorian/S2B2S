import { useTranslation } from "@/i18n/useTranslation";
import { getVersion } from "@tauri-apps/api/app";
import { openUrl } from "@tauri-apps/plugin-opener";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { SettingContainer } from "../../ui/SettingContainer";
import { Button } from "../../ui/Button";
import { AppDataDirectory } from "../AppDataDirectory";
import { AppLanguageSelector } from "../AppLanguageSelector";
import { ShowWhatsNewOnUpdate } from "../ShowWhatsNewOnUpdate";
import { ThemeSelector } from "../ThemeSelector";
import { AccentColorSelector } from "../AccentColorSelector";
import { LogDirectory } from "../debug";
import { REPO_URL } from "@/lib/appIdentity";
import { createSignal, createEffect } from "solid-js";

export const AboutSettings = () => {
  const { t } = useTranslation();
  const [version, setVersion] = createSignal("");

  createEffect(
    () => undefined,
    () => {
      getVersion()
        .then(setVersion)
        .catch((error) => {
          console.error("Failed to get app version:", error);
          setVersion("");
        });
    },
  );

  return (
    <div class="max-w-3xl w-full mx-auto space-y-6">
      <SettingsGroup title={t("settings.about.title")}>
        <AppLanguageSelector descriptionMode="tooltip" grouped={true} />
        <ThemeSelector descriptionMode="tooltip" grouped={true} />
        <AccentColorSelector descriptionMode="tooltip" grouped={true} />
        <SettingContainer
          title={t("settings.about.version.title")}
          description={t("settings.about.version.description")}
          grouped={true}
        >
          <span class="text-sm font-mono">
            {version() ? `v${version()}` : null}
          </span>
        </SettingContainer>

        <ShowWhatsNewOnUpdate descriptionMode="tooltip" grouped={true} />

        <SettingContainer
          title={t("settings.about.sourceCode.title")}
          description={t("settings.about.sourceCode.description")}
          grouped={true}
        >
          <Button
            variant="secondary"
            size="md"
            onClick={() => openUrl(REPO_URL)}
          >
            {t("settings.about.sourceCode.button")}
          </Button>
        </SettingContainer>

        <AppDataDirectory descriptionMode="tooltip" grouped={true} />
        <LogDirectory grouped={true} />
      </SettingsGroup>

      <SettingsGroup title={t("settings.about.acknowledgments.title")}>
        <SettingContainer
          title={t("settings.about.acknowledgments.handy.title")}
          description={t("settings.about.acknowledgments.handy.description")}
          grouped={true}
          layout="stacked"
        >
          <div class="text-sm text-mid-gray">
            {t("settings.about.acknowledgments.handy.details")}
          </div>
        </SettingContainer>
        <SettingContainer
          title={t("settings.about.acknowledgments.ggml.title")}
          description={t("settings.about.acknowledgments.ggml.description")}
          grouped={true}
          layout="stacked"
        >
          <div class="text-sm text-mid-gray">
            {t("settings.about.acknowledgments.ggml.details")}
          </div>
        </SettingContainer>
        <SettingContainer
          title={t("settings.about.acknowledgments.rnnoise.title")}
          description={t("settings.about.acknowledgments.rnnoise.description")}
          grouped={true}
          layout="stacked"
        >
          <div class="text-sm text-mid-gray">
            {t("settings.about.acknowledgments.rnnoise.details")}
          </div>
        </SettingContainer>
      </SettingsGroup>
    </div>
  );
};
