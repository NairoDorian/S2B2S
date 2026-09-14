import { Show } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { LanguageSelector } from "../LanguageSelector";
import { TranslateToEnglish } from "../TranslateToEnglish";
import { useModelStore } from "../../../stores/modelStore";
import type { ModelInfo } from "@/bindings";
import {
  CHINESE_LANGUAGE_CODE,
  getUniqueCapabilityLanguages,
} from "@/lib/constants/languages";

export const ModelSettingsCard = () => {
  const { t } = useTranslation();
  const modelStore = useModelStore();

  // All derived from the store inside functions and read in the JSX below:
  // a body-level read would snapshot the empty mount-time store and the
  // card could never appear once a model loads.
  const currentModelInfo = () =>
    modelStore.models.find((m: ModelInfo) => m.id === modelStore.currentModel);

  const supportsLanguageSelection = () =>
    currentModelInfo()?.supports_language_selection ?? false;
  const capabilityLanguages = () =>
    getUniqueCapabilityLanguages(currentModelInfo()?.supported_languages ?? []);
  const supportsChineseOnlyScriptSelection = () =>
    capabilityLanguages().length === 1 &&
    capabilityLanguages()[0] === CHINESE_LANGUAGE_CODE;
  const showLanguageSelector = () =>
    supportsLanguageSelection() || supportsChineseOnlyScriptSelection();
  const supportsTranslation = () =>
    currentModelInfo()?.supports_translation ?? false;
  const hasAnySettings = () => showLanguageSelector() || supportsTranslation();

  return (
    <Show
      when={
        !!modelStore.currentModel && !!currentModelInfo() && hasAnySettings()
      }
    >
      <SettingsGroup
        title={t("settings.modelSettings.title", {
          model: currentModelInfo()!.name,
        })}
      >
        {showLanguageSelector() && (
          <LanguageSelector
            descriptionMode="tooltip"
            grouped={true}
            supportedLanguages={currentModelInfo()!.supported_languages}
            supportsLanguageDetection={
              currentModelInfo()!.supports_language_detection
            }
          />
        )}
        {supportsTranslation() && (
          <TranslateToEnglish descriptionMode="tooltip" grouped={true} />
        )}
      </SettingsGroup>
    </Show>
  );
};
