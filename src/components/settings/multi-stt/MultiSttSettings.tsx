import React, { useEffect, useMemo, useState } from "react";
import { Trans, useTranslation } from "react-i18next";
import { type ModelInfo, commands } from "@/bindings";

import {
  SettingContainer,
  SettingsGroup,
  Textarea,
  ToggleSwitch,
} from "@/components/ui";
import { Alert } from "@/components/ui/Alert";
import { Button } from "@/components/ui/Button";
import { Dropdown } from "@/components/ui/Dropdown";
import {
  getLanguageLabel,
  SELECTABLE_LANGUAGES,
  supportsLanguageCode,
} from "@/lib/constants/languages";

import { ShortcutInput } from "../ShortcutInput";
import { useSettings } from "../../../hooks/useSettings";
import { useModelStore } from "../../../stores/modelStore";

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

  if (!supportsSelection) {
    return (
      <p className="text-xs text-mid-gray/50 italic mt-1 ml-1">
        {t("multiStt.models.languageNotApplicable")}
      </p>
    );
  }

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

export const DEFAULT_MERGE_PROMPT_PROFILE = {
  id: "merge_and_clean",
  name: "Merge and Clean",
  prompt: `Role: You are an expert multi-source Speech-to-Text (STT) consensus and transcript refinement engine. Your task is to compare 3 different STT transcripts of the exact same audio, merge them into a single accurate transcript, and clean the text according to strict formatting rules.

Core Objective:
Analyze Transcriptions 1, 2, and 3. Reconcile differences between them using contextual logic, phonetic similarity, and majority consensus to reconstruct the single most accurate version of what was spoken.

1. Consensus & Merge Logic:
- Discrepancy Resolution: When the 3 transcripts disagree on a word or phrase, select the version that makes the most sense grammatically and contextually in the original language.
- Majority Voting: If 2 of the 3 transcripts agree on a word/phrase and it fits logically, favor that reading unless it is an obvious shared STT misrecognition.
- Hallucinations & Omissions: Ignore individual model hallucinations, random character glitches, or missing words if the other transcripts provide a coherent sentence.

2. Cleaning & Refinement Instructions:
- Language Retention: Maintain the original language strictly (e.g., if the transcript is in French, output strictly in French). Never translate.
- Speech Artifacts: Strip out filler words (e.g., "um," "uh," "like" used as filler), stutters, and false starts.
- Grammar & Mechanics: Fix spelling, capitalization, missing commas, and sentence boundaries.
- Number Formatting (STRICT):
  - Convert ALL spoken numbers strictly into digits (e.g., "twenty-five" → "25", "un deux trois" → "1 2 3").
  - NEVER write numbers using words or letters under any circumstances.
  - Convert spoken currency and percentage words into symbols (e.g., "ten percent" → "10%", "five dollars" → "$5").
- Spoken Punctuation: Convert spoken punctuation words directly into punctuation marks (e.g., "period" → ".", "comma" → ",").
- Fidelity: Preserve the original speaker's exact sentence structure, tone, and word order as closely as possible. Do NOT paraphrase, summarize, or rewrite valid spoken content.
- Capitalize my sentences when missing uppercases. Add a final '.' period punctuation at the end of sentences as well.
- Never put " '' "  or " "" " around the output transcriptions or any text decorator. Just output the Merged and Cleaned Transcription.

Output Constraints:
- Return ONLY the final merged and cleaned transcript.
- Do NOT include any preamble, introductory text, markdown code blocks, quotes, or commentary (e.g., do NOT write "Here is the merged transcript:").

Numbers: Never words, never letters, only digits (Un deux trois - > 1, 2, 3)( One two three - > 1, 2, 3) Double check final output if dealing with number, only output digits, never letters or words for numbers, remember that is very important

The Transcription N°2 is generally the most accurate.

Me, the user, will speak French primarily but also sometimes in English. Keep English words in English and French words in French, even if they are mixed up in the same sentence.

---

Transcription 1:
"""
\${output}
"""

Transcription 2:
"""
\${output2}
"""

Transcription 3:
"""
\${output3}
"""`,
};

export const MultiSttSettings: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const { models, currentModel } = useModelStore();

  const multiSttEnabled = getSetting("multi_stt_enabled") ?? false;
  const multiSttModel2 = getSetting("multi_stt_model_2") ?? null;
  const multiSttModel3 = getSetting("multi_stt_model_3") ?? null;
  const multiSttMergePrompt = getSetting("multi_stt_merge_prompt") ?? null;
  const useLlamaMerge = getSetting("multi_stt_use_llama_merge") ?? false;
  const brainMode = getSetting("multi_stt_brain_mode") ?? "text_only";

  const [draftName, setDraftName] = useState(
    multiSttMergePrompt?.name || DEFAULT_MERGE_PROMPT_PROFILE.name,
  );
  const [draftText, setDraftText] = useState(
    multiSttMergePrompt?.prompt || DEFAULT_MERGE_PROMPT_PROFILE.prompt,
  );

  const downloadedModels = models.filter(
    (m: ModelInfo) => m.is_downloaded || m.is_custom,
  );
  const primaryModelId = currentModel;

  // Each slot's options keep its own current selection (so the Dropdown can
  // display the selected model's name) and exclude the primary model and the
  // model chosen in the other slot (no duplicate selections).
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

  // Resolve model info/names against the full catalog so names stay correct
  // even if a selected model is no longer downloaded.
  const model2Info = multiSttModel2
    ? models.find((m: ModelInfo) => m.id === multiSttModel2)
    : undefined;

  const model3Info = multiSttModel3
    ? models.find((m: ModelInfo) => m.id === multiSttModel3)
    : undefined;

  useEffect(() => {
    if (multiSttMergePrompt) {
      setDraftName(multiSttMergePrompt.name || "");
      setDraftText(multiSttMergePrompt.prompt || "");
    } else {
      setDraftName(DEFAULT_MERGE_PROMPT_PROFILE.name);
      setDraftText(DEFAULT_MERGE_PROMPT_PROFILE.prompt);
    }
  }, [multiSttMergePrompt]);

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
      id: "merge_and_clean",
      name: draftName.trim(),
      prompt: draftText.trim(),
    });
  };

  const handleResetDefaultPrompt = () => {
    updateSetting("multi_stt_merge_prompt", DEFAULT_MERGE_PROMPT_PROFILE);
    setDraftName(DEFAULT_MERGE_PROMPT_PROFILE.name);
    setDraftText(DEFAULT_MERGE_PROMPT_PROFILE.prompt);
  };

  const handleClearMergePrompt = () => {
    updateSetting("multi_stt_merge_prompt", null);
    setDraftName("");
    setDraftText("");
  };

  const primaryModelName = primaryModelId
    ? models.find((m: ModelInfo) => m.id === primaryModelId)?.name ||
      primaryModelId
    : t("multiStt.models.noPrimaryModel");

  const selectedModel2Name = multiSttModel2
    ? models.find((m: ModelInfo) => m.id === multiSttModel2)?.name ||
      multiSttModel2
    : null;

  const selectedModel3Name = multiSttModel3
    ? models.find((m: ModelInfo) => m.id === multiSttModel3)?.name ||
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

          {/* LLM Merge Provider */}
          <SettingsGroup title={t("multiStt.mergeProvider.title")}>
            <SettingContainer
              title={t("multiStt.mergeProvider.useLlama.label")}
              description={t("multiStt.mergeProvider.useLlama.description")}
              descriptionMode="tooltip"
              layout="horizontal"
              grouped={true}
            >
              <ToggleSwitch
                checked={useLlamaMerge}
                onChange={(enabled) =>
                  updateSetting("multi_stt_use_llama_merge", enabled)
                }
                isUpdating={isUpdating("multi_stt_use_llama_merge")}
                label={t("multiStt.mergeProvider.useLlama.label")}
                description=""
                descriptionMode="tooltip"
                grouped={false}
              />
            </SettingContainer>
          </SettingsGroup>

          {/* Brain Model Participation */}
          <SettingsGroup title={t("multiStt.brainMode.title")}>
            <SettingContainer
              title={t("multiStt.brainMode.label")}
              description={t("multiStt.brainMode.description")}
              descriptionMode="tooltip"
              layout="horizontal"
              grouped={true}
            >
              <Dropdown
                selectedValue={brainMode}
                options={[
                  {
                    value: "text_only",
                    label: t("multiStt.brainMode.options.textOnly"),
                  },
                  {
                    value: "separate_asr",
                    label: t("multiStt.brainMode.options.separateAsr"),
                  },
                  {
                    value: "audio_in_merge",
                    label: t("multiStt.brainMode.options.audioInMerge"),
                  },
                ]}
                onSelect={(value) => {
                  updateSetting(
                    "multi_stt_brain_mode",
                    value as "text_only" | "separate_asr" | "audio_in_merge",
                  );
                  if (value !== "text_only") {
                    // Both multimodal modes need the mmproj-loaded Brain
                    // server. Warm it immediately so the first multi-STT turn
                    // doesn't fail with "audio input is not supported".
                    void commands
                      .warmBrainServer(true)
                      .catch((err) =>
                        console.error("Failed to warm Brain server:", err),
                      );
                  }
                }}
                className="min-w-[200px]"
              />
            </SettingContainer>
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
                    <Button
                      variant="secondary"
                      size="sm"
                      onClick={() => handleModel2Select(null)}
                    >
                      {t("multiStt.models.disableModel")}
                    </Button>
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
                    <Button
                      variant="secondary"
                      size="sm"
                      onClick={() => handleModel3Select(null)}
                    >
                      {t("multiStt.models.disableModel")}
                    </Button>
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
                  <input
                    type="text"
                    value={draftName}
                    onChange={(e) => setDraftName(e.target.value)}
                    placeholder={t(
                      "multiStt.mergePrompt.promptNamePlaceholder",
                    )}
                    className="px-3 py-2 bg-mid-gray/10 rounded-md text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
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
                  <Button
                    variant="secondary"
                    size="md"
                    onClick={handleResetDefaultPrompt}
                  >
                    {t("multiStt.mergePrompt.resetDefault", "Reset to Default")}
                  </Button>
                  {multiSttMergePrompt && (
                    <Button
                      variant="secondary"
                      size="md"
                      onClick={handleClearMergePrompt}
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
              {brainMode === "separate_asr" && (
                <div className="flex items-center gap-2 p-2 bg-mid-gray/5 rounded-md">
                  <span className="w-2 h-2 rounded-full bg-orange-500" />
                  <span className="text-sm">
                    {t("multiStt.status.gemma4")}:{" "}
                    {t("multiStt.status.gemma4Model")}
                  </span>
                </div>
              )}
              {brainMode === "audio_in_merge" && (
                <div className="flex items-center gap-2 p-2 bg-mid-gray/5 rounded-md">
                  <span className="w-2 h-2 rounded-full bg-orange-500" />
                  <span className="text-sm">
                    {t("multiStt.status.audioInMerge")}
                  </span>
                </div>
              )}
              {!selectedModel2Name &&
                !selectedModel3Name &&
                brainMode === "text_only" && (
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
