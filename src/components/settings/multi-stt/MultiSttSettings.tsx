import React, { useEffect, useMemo, useState } from "react";
import { Trans, useTranslation } from "react-i18next";
import { type ModelInfo } from "@/bindings";

import {
  SettingContainer,
  SettingsGroup,
  Textarea,
  ToggleSwitch,
} from "@/components/ui";
import { Alert } from "@/components/ui/Alert";
import { Button } from "@/components/ui/Button";
import { Input } from "@/components/ui/Input";
import { Dropdown } from "@/components/ui/Dropdown";
import {
  getLanguageLabel,
  SELECTABLE_LANGUAGES,
  supportsLanguageCode,
} from "@/lib/constants/languages";

import { ShortcutInput } from "../ShortcutInput";
import { useSettings } from "../../../hooks/useSettings";
import { useModelStore } from "../../../stores/modelStore";
import { commands } from "@/bindings";

/** Inline language selector for an extra multi-STT model slot. */
interface PerModelLanguageSelectorProps {
  slot: 2 | 3;
  modelId: string | null;
  modelInfo: ModelInfo | undefined;
}

const PerModelLanguageSelector: React.FC<PerModelLanguageSelectorProps> = ({
  slot,
  modelId,
  modelInfo,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting } = useSettings();
  const settingKey =
    slot === 2 ? "multi_stt_language_model_2" : "multi_stt_language_model_3";
  const currentLang = (getSetting(settingKey) as string | null) ?? null;

  // Build lang options from the model's supported languages + auto
  // NOTE: useMemo must be called BEFORE any early return (React hooks rule).
  const langOptions = useMemo(() => {
    if (!modelInfo || !modelId) return [];
    if (
      !modelInfo.supports_language_selection ||
      modelInfo.supported_languages.length === 0
    )
      return [];
    const entries = SELECTABLE_LANGUAGES.filter(
      (lang) =>
        lang.value === "auto" ||
        supportsLanguageCode(modelInfo.supported_languages, lang.value),
    );
    return entries.map((lang) => ({ value: lang.value, label: lang.label }));
  }, [modelId, modelInfo]);

  if (!modelId || !modelInfo) return null;

  const supportsSelection = modelInfo.supports_language_selection;

  // If the model does not support selection, show a disabled note
  if (!supportsSelection) {
    return (
      <p className="text-xs text-mid-gray/50 italic mt-1 ml-1">
        {t("multiStt.models.languageNotApplicable")}
      </p>
    );
  }

  // Show a language dropdown
  return (
    <div className="flex items-center gap-2 mt-2 ml-1">
      <label className="text-xs text-mid-gray/70 whitespace-nowrap">
        {slot === 2
          ? t("multiStt.models.model2Language")
          : t("multiStt.models.model3Language")}
      </label>
      <Dropdown
        selectedValue={currentLang}
        options={langOptions}
        onSelect={(value) => updateSetting(settingKey, value || null)}
        placeholder={getLanguageLabel("auto") ?? "Auto"}
        disabled={langOptions.length === 0}
        className="min-w-[140px]"
      />
    </div>
  );
};

export const MultiSttSettings: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const { models, currentModel, loadModels } = useModelStore();

  const multiSttEnabled = getSetting("multi_stt_enabled") ?? false;
  const multiSttModel2 = getSetting("multi_stt_model_2") ?? null;
  const multiSttModel3 = getSetting("multi_stt_model_3") ?? null;
  const multiSttMergePrompt = getSetting("multi_stt_merge_prompt") ?? null;

  const [draftName, setDraftName] = useState("");
  const [draftText, setDraftText] = useState("");

  // Filter out primary model + already-selected models for model 2/3 dropdowns
  const downloadedModels = models.filter(
    (m: ModelInfo) => m.is_downloaded || m.is_custom,
  );
  const primaryModelId = currentModel;

  const modelOptionsForSlot2 = downloadedModels
    .filter(
      (m: ModelInfo) => m.id !== primaryModelId && m.id !== multiSttModel3,
    )
    .map((m: ModelInfo) => ({ value: m.id, label: m.name }));

  const modelOptionsForSlot3 = downloadedModels
    .filter(
      (m: ModelInfo) => m.id !== primaryModelId && m.id !== multiSttModel2,
    )
    .map((m: ModelInfo) => ({ value: m.id, label: m.name }));

  const model2Info = multiSttModel2
    ? downloadedModels.find((m: ModelInfo) => m.id === multiSttModel2)
    : undefined;

  const model3Info = multiSttModel3
    ? downloadedModels.find((m: ModelInfo) => m.id === multiSttModel3)
    : undefined;

  // Initialize draft from existing merge prompt. Keyed on the saved values
  // (not the object identity) so an externally-changed prompt syncs in without
  // clobbering an in-progress draft.
  useEffect(() => {
    if (multiSttMergePrompt) {
      setDraftName(multiSttMergePrompt.name || "");
      setDraftText(multiSttMergePrompt.prompt || "");
    } else {
      setDraftName("");
      setDraftText("");
    }
  }, [multiSttMergePrompt?.name, multiSttMergePrompt?.prompt]);

  const handleModel2Select = (value: string | null) => {
    updateSetting("multi_stt_model_2", value || null);
  };

  const handleModel3Select = (value: string | null) => {
    updateSetting("multi_stt_model_3", value || null);
  };

  const handleSaveMergePrompt = () => {
    if (!draftName.trim() || !draftText.trim()) {
      return;
    }

    updateSetting("multi_stt_merge_prompt", {
      id: "multi_stt_merge_prompt",
      name: draftName.trim(),
      prompt: draftText.trim(),
    });
  };

  const handleClearMergePrompt = () => {
    updateSetting("multi_stt_merge_prompt", null);
    setDraftName("");
    setDraftText("");
  };

  const handleUnloadModel = async (modelId: string) => {
    try {
      await commands.unloadExtraModel(modelId);
      loadModels();
    } catch (err) {
      console.error("Failed to unload model:", err);
    }
  };

  const primaryModelName = primaryModelId
    ? downloadedModels.find((m: ModelInfo) => m.id === primaryModelId)?.name ||
      primaryModelId
    : t("multiStt.models.noPrimaryModel");

  const selectedModel2Name = multiSttModel2
    ? downloadedModels.find((m: ModelInfo) => m.id === multiSttModel2)?.name ||
      multiSttModel2
    : null;

  const selectedModel3Name = multiSttModel3
    ? downloadedModels.find((m: ModelInfo) => m.id === multiSttModel3)?.name ||
      multiSttModel3
    : null;

  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      {/* Toggle */}
      <SettingsGroup title={t("multiStt.enabled.label")}>
        <ToggleSwitch
          checked={multiSttEnabled}
          onChange={(enabled) => updateSetting("multi_stt_enabled", enabled)}
          isUpdating={isUpdating("multi_stt_enabled")}
          label={t("multiStt.enabled.label")}
          description={t("multiStt.enabled.description")}
          descriptionMode="tooltip"
          grouped={true}
        />
      </SettingsGroup>

      {multiSttEnabled && (
        <>
          {/* Hotkey */}
          <SettingsGroup title={t("multiStt.shortcut.title")}>
            <ShortcutInput
              shortcutId="multi_stt_transcribe"
              descriptionMode="tooltip"
              grouped={true}
            />
          </SettingsGroup>

          {/* Model Selection */}
          <SettingsGroup title={t("multiStt.models.title")}>
            <div className="space-y-4">
              {/* Primary Model Info */}
              <div className="p-3 bg-mid-gray/5 rounded-md border border-mid-gray/20 opacity-70">
                <p className="text-sm font-medium text-text/80">
                  {t("multiStt.status.primary")}: {primaryModelName}
                </p>
                <p className="text-xs text-mid-gray/60 mt-1">
                  {t("multiStt.models.primaryModel", {
                    model: primaryModelName,
                  })}
                </p>
              </div>

              {/* Model 2 Selection */}
              <SettingContainer
                title={t("multiStt.models.model2")}
                description={t("multiStt.models.model2Description")}
                descriptionMode="tooltip"
                layout="horizontal"
                grouped={true}
              >
                <div className="flex items-center gap-2 min-w-0">
                  <Dropdown
                    selectedValue={multiSttModel2}
                    options={modelOptionsForSlot2}
                    onSelect={(value) => handleModel2Select(value)}
                    placeholder={t("multiStt.models.notSelected")}
                    disabled={modelOptionsForSlot2.length === 0}
                    className="flex-1 min-w-0"
                  />
                  {multiSttModel2 && (
                    <>
                      <Button
                        variant="secondary"
                        size="sm"
                        onClick={() => handleModel2Select(null)}
                      >
                        {t("multiStt.models.disableModel")}
                      </Button>
                      <Button
                        variant="secondary"
                        size="sm"
                        onClick={() => handleUnloadModel(multiSttModel2)}
                      >
                        {t("multiStt.models.unloadModel")}
                      </Button>
                    </>
                  )}
                </div>
                <PerModelLanguageSelector
                  slot={2}
                  modelId={multiSttModel2}
                  modelInfo={model2Info}
                />
                {multiSttModel2 && model2Info?.supports_translation && (
                  <div className="flex items-center gap-2 mt-1 ml-1">
                    <ToggleSwitch
                      checked={
                        (getSetting(
                          "multi_stt_translate_model_2",
                        ) as boolean) ?? true
                      }
                      onChange={(enabled) =>
                        updateSetting("multi_stt_translate_model_2", enabled)
                      }
                      isUpdating={isUpdating("multi_stt_translate_model_2")}
                      label={t("multiStt.models.translateToEnglish")}
                      description=""
                      descriptionMode="tooltip"
                      grouped={false}
                    />
                  </div>
                )}
              </SettingContainer>

              {/* Model 3 Selection */}
              <SettingContainer
                title={t("multiStt.models.model3")}
                description={t("multiStt.models.model3Description")}
                descriptionMode="tooltip"
                layout="horizontal"
                grouped={true}
              >
                <div className="flex items-center gap-2 min-w-0">
                  <Dropdown
                    selectedValue={multiSttModel3}
                    options={modelOptionsForSlot3}
                    onSelect={(value) => handleModel3Select(value)}
                    placeholder={t("multiStt.models.notSelected")}
                    disabled={modelOptionsForSlot3.length === 0}
                    className="flex-1 min-w-0"
                  />
                  {multiSttModel3 && (
                    <>
                      <Button
                        variant="secondary"
                        size="sm"
                        onClick={() => handleModel3Select(null)}
                      >
                        {t("multiStt.models.disableModel")}
                      </Button>
                      <Button
                        variant="secondary"
                        size="sm"
                        onClick={() => handleUnloadModel(multiSttModel3)}
                      >
                        {t("multiStt.models.unloadModel")}
                      </Button>
                    </>
                  )}
                </div>
                <PerModelLanguageSelector
                  slot={3}
                  modelId={multiSttModel3}
                  modelInfo={model3Info}
                />
                {multiSttModel3 && model3Info?.supports_translation && (
                  <div className="flex items-center gap-2 mt-1 ml-1">
                    <ToggleSwitch
                      checked={
                        (getSetting(
                          "multi_stt_translate_model_3",
                        ) as boolean) ?? true
                      }
                      onChange={(enabled) =>
                        updateSetting("multi_stt_translate_model_3", enabled)
                      }
                      isUpdating={isUpdating("multi_stt_translate_model_3")}
                      label={t("multiStt.models.translateToEnglish")}
                      description=""
                      descriptionMode="tooltip"
                      grouped={false}
                    />
                  </div>
                )}
              </SettingContainer>
            </div>
          </SettingsGroup>

          {/* Keep Extra Models Loaded */}
          <SettingsGroup title={t("multiStt.keepModelsLoaded.label")}>
            <ToggleSwitch
              checked={
                (getSetting("multi_stt_keep_extra_models_loaded") as boolean) ??
                true
              }
              onChange={(enabled) =>
                updateSetting("multi_stt_keep_extra_models_loaded", enabled)
              }
              isUpdating={isUpdating("multi_stt_keep_extra_models_loaded")}
              label={t("multiStt.keepModelsLoaded.label")}
              description={t("multiStt.keepModelsLoaded.description")}
              descriptionMode="tooltip"
              grouped={true}
            />
          </SettingsGroup>

          {/* Merge Prompt */}
          <SettingsGroup title={t("multiStt.mergePrompt.title")}>
            <SettingContainer
              title={t("multiStt.mergePrompt.description")}
              description={t("multiStt.mergePrompt.promptTip", {
                output: "${output}",
                output2: "${output2}",
                output3: "${output3}",
              })}
              descriptionMode="tooltip"
              layout="stacked"
              grouped={true}
            >
              <div className="space-y-3">
                <div className="space-y-2 flex flex-col">
                  <label className="text-sm font-semibold">
                    {t("multiStt.mergePrompt.promptName")}
                  </label>
                  <Input
                    type="text"
                    value={draftName}
                    onChange={(e) => setDraftName(e.target.value)}
                    placeholder={t(
                      "multiStt.mergePrompt.promptNamePlaceholder",
                    )}
                    variant="compact"
                  />
                </div>

                <div className="space-y-2 flex flex-col">
                  <label className="text-sm font-semibold">
                    {t("multiStt.mergePrompt.promptLabel")}
                  </label>
                  <Textarea
                    value={draftText}
                    onChange={(e) => setDraftText(e.target.value)}
                    placeholder={t("multiStt.mergePrompt.promptPlaceholder")}
                  />
                  <p className="text-xs text-mid-gray/70">
                    <Trans
                      i18nKey="multiStt.mergePrompt.promptTip"
                      components={{ code: <code /> }}
                      values={{
                        output: "${output}",
                        output2: "${output2}",
                        output3: "${output3}",
                      }}
                    />
                  </p>
                </div>

                <div className="flex gap-2 pt-2">
                  <Button
                    onClick={handleSaveMergePrompt}
                    variant="primary"
                    size="md"
                    disabled={!draftName.trim() || !draftText.trim()}
                  >
                    {t("multiStt.mergePrompt.savePrompt")}
                  </Button>
                  {multiSttMergePrompt && (
                    <Button
                      onClick={handleClearMergePrompt}
                      variant="secondary"
                      size="md"
                    >
                      {t("multiStt.mergePrompt.clearPrompt")}
                    </Button>
                  )}
                </div>

                {!multiSttMergePrompt && (
                  <Alert variant="info" contained>
                    <p className="text-sm">
                      {t("multiStt.mergePrompt.noPrompt")}
                    </p>
                  </Alert>
                )}
              </div>
            </SettingContainer>
          </SettingsGroup>

          {/* Status Summary */}
          <SettingsGroup title={t("multiStt.status.title")}>
            <div className="space-y-2">
              <div className="flex items-center gap-2 p-2 bg-mid-gray/5 rounded-md">
                <span className="w-2 h-2 rounded-full bg-green-500" />
                <span className="text-sm">
                  {t("multiStt.status.primary")}: {primaryModelName}
                </span>
              </div>
              {selectedModel2Name && (
                <div className="flex items-center gap-2 p-2 bg-mid-gray/5 rounded-md">
                  <span className="w-2 h-2 rounded-full bg-blue-500" />
                  <span className="text-sm">
                    {t("multiStt.status.secondary")}: {selectedModel2Name}
                  </span>
                </div>
              )}
              {selectedModel3Name && (
                <div className="flex items-center gap-2 p-2 bg-mid-gray/5 rounded-md">
                  <span className="w-2 h-2 rounded-full bg-purple-500" />
                  <span className="text-sm">
                    {t("multiStt.status.tertiary")}: {selectedModel3Name}
                  </span>
                </div>
              )}
              {!selectedModel2Name && !selectedModel3Name && (
                <p className="text-sm text-mid-gray/60">
                  {t("multiStt.status.noModels")}
                </p>
              )}
            </div>
          </SettingsGroup>
        </>
      )}
    </div>
  );
};
