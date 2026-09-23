import { useTranslation } from "@/i18n/useTranslation";
import { Button } from "../../ui/Button";
import { SettingContainer } from "../../ui/SettingContainer";

export type OnboardingPreviewStep = "accessibility" | "model";

interface OnboardingPreviewProps {
  onPreview: (step: OnboardingPreviewStep) => void;
  descriptionMode?: "tooltip" | "inline";
  grouped?: boolean;
}

export const OnboardingPreview = (props: OnboardingPreviewProps) => {
  const { t } = useTranslation();

  return (
    <SettingContainer
      title={t("settings.debug.onboardingPreview.title")}
      description={t("settings.debug.onboardingPreview.description")}
      descriptionMode={props.descriptionMode ?? "tooltip"}
      grouped={props.grouped ?? false}
    >
      <div class="flex gap-2">
        <Button
          variant="secondary"
          size="md"
          onClick={() => props.onPreview("accessibility")}
        >
          {t("settings.debug.onboardingPreview.permissionsButton")}
        </Button>
        <Button
          variant="secondary"
          size="md"
          onClick={() => props.onPreview("model")}
        >
          {t("settings.debug.onboardingPreview.modelsButton")}
        </Button>
      </div>
    </SettingContainer>
  );
};
