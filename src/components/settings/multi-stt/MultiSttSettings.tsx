import { createSignal, createEffect, createMemo, Show } from "solid-js";
import { useTranslation, TranslatedMarkup } from "@/i18n/useTranslation";
import { listen } from "@tauri-apps/api/event";
import { sessionToast as toast } from "@/lib/sessionToast";
import { type ModelInfo } from "@/bindings";
import {
  SettingContainer,
  SettingsGroup,
  Slider,
  Textarea,
  ToggleSwitch,
} from "@/components/ui";
import { Alert } from "@/components/ui/Alert";
import { Button } from "@/components/ui/Button";
import { Input } from "@/components/ui/Input";
import { Dropdown } from "@/components/ui/Dropdown";
import { ShortcutInput } from "../ShortcutInput";
import { KeyComboInput } from "../KeyComboInput";
import { useSettings } from "../../../hooks/useSettings";
import { useModelStore } from "../../../stores/modelStore";
import { commands } from "@/bindings";
import { ModelStateEvent } from "@/lib/types/events";
import {
  SELECTABLE_LANGUAGES,
  effectiveLanguage,
  getLanguageLabel,
  supportsLanguageCode,
} from "@/lib/constants/languages";

interface PerModelLanguageSelectorProps {
  slot: 2 | 3 | 4;
  modelId: string | null;
  modelInfo: ModelInfo | undefined;
}

const PerModelLanguageSelector = (props: PerModelLanguageSelectorProps) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting } = useSettings();
  const settingKey =
    props.slot === 2
      ? "multi_stt_language_model_2"
      : props.slot === 3
        ? "multi_stt_language_model_3"
        : "multi_stt_language_model_4";
  const currentLang = () => (getSetting(settingKey) as string | null) ?? null;

  const langOptions = createMemo(() => {
    const modelInfo = props.modelInfo;
    if (!modelInfo || !props.modelId) return [];
    if (
      !modelInfo.supports_language_selection ||
      modelInfo.supported_languages.length === 0
    )
      return [];
    const entries = SELECTABLE_LANGUAGES.filter((lang) =>
      lang.value === "auto"
        ? modelInfo.supports_language_detection
        : supportsLanguageCode(modelInfo.supported_languages, lang.value),
    );
    return entries.map((lang) => ({ value: lang.value, label: lang.label }));
  });

  const effectiveLang = createMemo(() => {
    const modelInfo = props.modelInfo;
    if (!modelInfo) return "auto";
    return effectiveLanguage(
      currentLang() || "auto",
      modelInfo.supported_languages,
      modelInfo.supports_language_detection,
    );
  });

  const label = () =>
    props.slot === 2
      ? t("multiStt.models.model2Language")
      : props.slot === 3
        ? t("multiStt.models.model3Language")
        : t("multiStt.models.model4Language");

  const selectedValue = (modelInfo: ModelInfo) => {
    const lang = currentLang();
    return modelInfo.supports_language_detection
      ? lang
      : lang && lang !== "auto"
        ? lang
        : effectiveLang();
  };

  const placeholder = (modelInfo: ModelInfo) =>
    modelInfo.supports_language_detection
      ? (getLanguageLabel("auto") ?? "Auto")
      : (getLanguageLabel(effectiveLang()) ?? "Select language");

  return (
    <Show when={props.modelId && props.modelInfo ? props.modelInfo : undefined}>
      {(modelInfo) => (
        <Show
          when={modelInfo().supports_language_selection}
          fallback={
            <p class="text-xs text-mid-gray/50 italic mt-1 ml-1">
              {t("multiStt.models.languageNotApplicable")}
            </p>
          }
        >
          <div class="flex items-center gap-2 mt-2 ml-1">
            <label class="text-xs text-mid-gray/70 whitespace-nowrap">
              {label()}
            </label>
            <Dropdown
              selectedValue={selectedValue(modelInfo())}
              options={langOptions()}
              onSelect={(value) => updateSetting(settingKey, value || null)}
              placeholder={placeholder(modelInfo())}
              disabled={langOptions().length === 0}
              class="min-w-[140px]"
            />
          </div>
        </Show>
      )}
    </Show>
  );
};

export const MultiSttSettings = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const store = useModelStore();
  const models = () => store.models;
  const currentModel = () => store.currentModel;

  const multiSttEnabled = () => getSetting("multi_stt_enabled") ?? false;
  const multiSttModel2 = () => getSetting("multi_stt_model_2") ?? null;
  const multiSttModel3 = () => getSetting("multi_stt_model_3") ?? null;
  const multiSttModel4 = () => getSetting("multi_stt_model_4") ?? null;
  const multiSttMergePrompt = () =>
    getSetting("multi_stt_merge_prompt") ?? null;

  const [draftName, setDraftName] = createSignal("");
  const [draftText, setDraftText] = createSignal("");
  const [loadedExtraModels, setLoadedExtraModels] = createSignal(
    new Set<string>(),
  );
  const [loadingModelIds, setLoadingModelIds] = createSignal(new Set<string>());
  const [unloadingModelIds, setUnloadingModelIds] = createSignal(
    new Set<string>(),
  );

  const fetchLoadedExtraModels = async () => {
    const result = await commands.getExtraLoadedModels();
    if (result.status === "ok") {
      setLoadedExtraModels(new Set(result.data));
    }
  };

  createEffect(
    () => undefined,
    () => {
      fetchLoadedExtraModels();
      const unlisten = listen<ModelStateEvent>(
        "model-state-changed",
        (event) => {
          const { event_type, model_id } = event.payload;
          if (event_type === "multi_stt_model_loaded" && model_id) {
            setLoadedExtraModels((prev) => {
              const next = new Set(prev);
              next.add(model_id);
              return next;
            });
          } else if (event_type === "multi_stt_model_unloaded" && model_id) {
            setLoadedExtraModels((prev) => {
              const next = new Set(prev);
              next.delete(model_id);
              return next;
            });
          }
        },
      );
      return () => {
        unlisten.then((fn) => fn());
      };
    },
  );

  const downloadedModels = createMemo(() =>
    models().filter((m) => m.is_downloaded || m.is_custom),
  );
  const primaryModelId = () => currentModel();

  const modelOptionsForSlot2 = createMemo(() =>
    downloadedModels()
      .filter(
        (m) =>
          m.id !== primaryModelId() &&
          m.id !== multiSttModel3() &&
          m.id !== multiSttModel4(),
      )
      .map((m) => ({ value: m.id, label: m.name })),
  );
  const modelOptionsForSlot3 = createMemo(() =>
    downloadedModels()
      .filter(
        (m) =>
          m.id !== primaryModelId() &&
          m.id !== multiSttModel2() &&
          m.id !== multiSttModel4(),
      )
      .map((m) => ({ value: m.id, label: m.name })),
  );
  const modelOptionsForSlot4 = createMemo(() =>
    downloadedModels()
      .filter(
        (m) =>
          m.id !== primaryModelId() &&
          m.id !== multiSttModel2() &&
          m.id !== multiSttModel3(),
      )
      .map((m) => ({ value: m.id, label: m.name })),
  );

  const model2Info = createMemo(() => {
    const id = multiSttModel2();
    return id ? downloadedModels().find((m) => m.id === id) : undefined;
  });
  const model3Info = createMemo(() => {
    const id = multiSttModel3();
    return id ? downloadedModels().find((m) => m.id === id) : undefined;
  });
  const model4Info = createMemo(() => {
    const id = multiSttModel4();
    return id ? downloadedModels().find((m) => m.id === id) : undefined;
  });

  createEffect(
    () => multiSttMergePrompt(),
    (prompt) => {
      if (prompt) {
        setDraftName(prompt.name || "");
        setDraftText(prompt.prompt || "");
      } else {
        setDraftName("");
        setDraftText("");
      }
    },
  );

  const handleModel2Select = (value: string | null) =>
    updateSetting("multi_stt_model_2", value || null);
  const handleModel3Select = (value: string | null) =>
    updateSetting("multi_stt_model_3", value || null);
  const handleModel4Select = (value: string | null) =>
    updateSetting("multi_stt_model_4", value || null);

  const handleSaveMergePrompt = () => {
    if (!draftName().trim() || !draftText().trim()) return;
    updateSetting("multi_stt_merge_prompt", {
      id: "multi_stt_merge_prompt",
      name: draftName().trim(),
      prompt: draftText().trim(),
    });
  };

  const handleClearMergePrompt = () => {
    updateSetting("multi_stt_merge_prompt", null);
    setDraftName("");
    setDraftText("");
  };

  const handleUnloadModel = async (modelId: string | null) => {
    if (!modelId) return;
    setUnloadingModelIds((prev) => new Set([...prev, modelId]));
    try {
      const result = await commands.unloadExtraModel(modelId);
      if (result.status === "ok") {
        toast.success(t("multiStt.models.unloadedSuccessfully"), {
          description: modelId,
        });
        await fetchLoadedExtraModels();
      } else {
        toast.error(t("multiStt.models.unloadFailed"), {
          description: result.error,
        });
      }
    } catch (err) {
      toast.error(t("multiStt.models.unloadFailed"), {
        description: String(err),
      });
    } finally {
      setUnloadingModelIds((prev) => {
        const next = new Set(prev);
        next.delete(modelId);
        return next;
      });
    }
  };

  const handleLoadModel = async (modelId: string | null) => {
    if (!modelId) return;
    setLoadingModelIds((prev) => new Set([...prev, modelId]));
    try {
      const result = await commands.loadExtraModel(modelId);
      if (result.status === "ok") {
        toast.success(t("multiStt.models.loadedSuccessfully"), {
          description: modelId,
        });
        await fetchLoadedExtraModels();
      } else {
        toast.error(t("multiStt.models.loadFailed"), {
          description: result.error,
        });
      }
    } catch (err) {
      toast.error(t("multiStt.models.loadFailed"), {
        description: String(err),
      });
    } finally {
      setLoadingModelIds((prev) => {
        const next = new Set(prev);
        next.delete(modelId);
        return next;
      });
    }
  };

  const isExtraModelLoaded = (modelId: string | null): boolean =>
    modelId != null && loadedExtraModels().has(modelId);
  const isModelLoading = (modelId: string | null): boolean =>
    modelId != null && loadingModelIds().has(modelId);
  const isModelUnloading = (modelId: string | null): boolean =>
    modelId != null && unloadingModelIds().has(modelId);

  const primaryModelName = createMemo(() => {
    const id = primaryModelId();
    return id
      ? downloadedModels().find((m) => m.id === id)?.name || id
      : t("multiStt.models.noPrimaryModel");
  });
  const selectedModel2Name = createMemo(() => {
    const id = multiSttModel2();
    return id ? downloadedModels().find((m) => m.id === id)?.name || id : null;
  });
  const selectedModel3Name = createMemo(() => {
    const id = multiSttModel3();
    return id ? downloadedModels().find((m) => m.id === id)?.name || id : null;
  });
  const selectedModel4Name = createMemo(() => {
    const id = multiSttModel4();
    return id ? downloadedModels().find((m) => m.id === id)?.name || id : null;
  });

  return (
    <div class="max-w-3xl w-full mx-auto space-y-6">
      <SettingsGroup title={t("multiStt.enabled.label")}>
        <ToggleSwitch
          checked={multiSttEnabled()}
          onChange={(enabled) => updateSetting("multi_stt_enabled", enabled)}
          isUpdating={isUpdating("multi_stt_enabled")}
          label={t("multiStt.enabled.label")}
          description={t("multiStt.enabled.description")}
          descriptionMode="tooltip"
          grouped={true}
        />
      </SettingsGroup>

      {multiSttEnabled() && (
        <>
          <SettingsGroup title={t("multiStt.shortcut.title")}>
            <ShortcutInput
              shortcutId="multi_stt_transcribe"
              descriptionMode="tooltip"
              grouped={true}
            />
          </SettingsGroup>

          <SettingsGroup title={t("multiStt.models.title")}>
            <div class="space-y-4">
              <div class="p-3 bg-mid-gray/5 rounded-md border border-mid-gray/20 opacity-70">
                <p class="text-sm font-medium text-text/80">
                  {t("multiStt.status.primary")}: {primaryModelName()}
                </p>
                <p class="text-xs text-mid-gray/60 mt-1">
                  {t("multiStt.models.primaryModel", {
                    model: primaryModelName(),
                  })}
                </p>
              </div>
              <SettingContainer
                title={t("multiStt.models.model2")}
                description={t("multiStt.models.model2Description")}
                descriptionMode="tooltip"
                layout="horizontal"
                grouped={true}
              >
                <div class="flex items-center gap-2 min-w-0">
                  <Dropdown
                    selectedValue={multiSttModel2()}
                    options={modelOptionsForSlot2()}
                    onSelect={(value) => handleModel2Select(value)}
                    placeholder={t("multiStt.models.notSelected")}
                    disabled={modelOptionsForSlot2().length === 0}
                    class="flex-1 min-w-0"
                  />
                  {multiSttModel2() && (
                    <>
                      <span
                        class={`text-xs whitespace-nowrap ${isExtraModelLoaded(multiSttModel2()) ? "text-green-500" : "text-mid-gray/50"}`}
                      >
                        {isExtraModelLoaded(multiSttModel2())
                          ? t("multiStt.models.loaded")
                          : t("multiStt.models.notLoaded")}
                      </span>
                      {isExtraModelLoaded(multiSttModel2()) ? (
                        <Button
                          variant="secondary"
                          size="sm"
                          onClick={() => handleUnloadModel(multiSttModel2())}
                          disabled={
                            isModelUnloading(multiSttModel2()) ||
                            isModelLoading(multiSttModel2())
                          }
                        >
                          {isModelUnloading(multiSttModel2())
                            ? t("multiStt.models.unloadingModel")
                            : t("multiStt.models.unloadModel")}
                        </Button>
                      ) : (
                        <Button
                          variant="secondary"
                          size="sm"
                          onClick={() => handleLoadModel(multiSttModel2())}
                          disabled={
                            isModelLoading(multiSttModel2()) ||
                            isModelUnloading(multiSttModel2())
                          }
                        >
                          {isModelLoading(multiSttModel2())
                            ? t("multiStt.models.loadingModel")
                            : t("multiStt.models.loadModel")}
                        </Button>
                      )}
                      <Button
                        variant="secondary"
                        size="sm"
                        onClick={() => handleModel2Select(null)}
                      >
                        {t("multiStt.models.disableModel")}
                      </Button>
                    </>
                  )}
                </div>
                <PerModelLanguageSelector
                  slot={2}
                  modelId={multiSttModel2()}
                  modelInfo={model2Info()}
                />
                {multiSttModel2() && model2Info()?.supports_translation && (
                  <div class="flex items-center gap-2 mt-1 ml-1">
                    <ToggleSwitch
                      checked={
                        (getSetting(
                          "multi_stt_translate_model_2",
                        ) as boolean) ?? false
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
              {/* Model 3 and 4 selections follow the same pattern as Model 2 */}
              <SettingContainer
                title={t("multiStt.models.model3")}
                description={t("multiStt.models.model3Description")}
                descriptionMode="tooltip"
                layout="horizontal"
                grouped={true}
              >
                <div class="flex items-center gap-2 min-w-0">
                  <Dropdown
                    selectedValue={multiSttModel3()}
                    options={modelOptionsForSlot3()}
                    onSelect={(value) => handleModel3Select(value)}
                    placeholder={t("multiStt.models.notSelected")}
                    disabled={modelOptionsForSlot3().length === 0}
                    class="flex-1 min-w-0"
                  />
                  {multiSttModel3() && (
                    <>
                      <span
                        class={`text-xs whitespace-nowrap ${isExtraModelLoaded(multiSttModel3()) ? "text-green-500" : "text-mid-gray/50"}`}
                      >
                        {isExtraModelLoaded(multiSttModel3())
                          ? t("multiStt.models.loaded")
                          : t("multiStt.models.notLoaded")}
                      </span>
                      {isExtraModelLoaded(multiSttModel3()) ? (
                        <Button
                          variant="secondary"
                          size="sm"
                          onClick={() => handleUnloadModel(multiSttModel3())}
                          disabled={
                            isModelUnloading(multiSttModel3()) ||
                            isModelLoading(multiSttModel3())
                          }
                        >
                          {isModelUnloading(multiSttModel3())
                            ? t("multiStt.models.unloadingModel")
                            : t("multiStt.models.unloadModel")}
                        </Button>
                      ) : (
                        <Button
                          variant="secondary"
                          size="sm"
                          onClick={() => handleLoadModel(multiSttModel3())}
                          disabled={
                            isModelLoading(multiSttModel3()) ||
                            isModelUnloading(multiSttModel3())
                          }
                        >
                          {isModelLoading(multiSttModel3())
                            ? t("multiStt.models.loadingModel")
                            : t("multiStt.models.loadModel")}
                        </Button>
                      )}
                      <Button
                        variant="secondary"
                        size="sm"
                        onClick={() => handleModel3Select(null)}
                      >
                        {t("multiStt.models.disableModel")}
                      </Button>
                    </>
                  )}
                </div>
                <PerModelLanguageSelector
                  slot={3}
                  modelId={multiSttModel3()}
                  modelInfo={model3Info()}
                />
                {multiSttModel3() && model3Info()?.supports_translation && (
                  <div class="flex items-center gap-2 mt-1 ml-1">
                    <ToggleSwitch
                      checked={
                        (getSetting(
                          "multi_stt_translate_model_3",
                        ) as boolean) ?? false
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
              <SettingContainer
                title={t("multiStt.models.model4")}
                description={t("multiStt.models.model4Description")}
                descriptionMode="tooltip"
                layout="horizontal"
                grouped={true}
              >
                <div class="flex items-center gap-2 min-w-0">
                  <Dropdown
                    selectedValue={multiSttModel4()}
                    options={modelOptionsForSlot4()}
                    onSelect={(value) => handleModel4Select(value)}
                    placeholder={t("multiStt.models.notSelected")}
                    disabled={modelOptionsForSlot4().length === 0}
                    class="flex-1 min-w-0"
                  />
                  {multiSttModel4() && (
                    <>
                      <span
                        class={`text-xs whitespace-nowrap ${isExtraModelLoaded(multiSttModel4()) ? "text-green-500" : "text-mid-gray/50"}`}
                      >
                        {isExtraModelLoaded(multiSttModel4())
                          ? t("multiStt.models.loaded")
                          : t("multiStt.models.notLoaded")}
                      </span>
                      {isExtraModelLoaded(multiSttModel4()) ? (
                        <Button
                          variant="secondary"
                          size="sm"
                          onClick={() => handleUnloadModel(multiSttModel4())}
                          disabled={
                            isModelUnloading(multiSttModel4()) ||
                            isModelLoading(multiSttModel4())
                          }
                        >
                          {isModelUnloading(multiSttModel4())
                            ? t("multiStt.models.unloadingModel")
                            : t("multiStt.models.unloadModel")}
                        </Button>
                      ) : (
                        <Button
                          variant="secondary"
                          size="sm"
                          onClick={() => handleLoadModel(multiSttModel4())}
                          disabled={
                            isModelLoading(multiSttModel4()) ||
                            isModelUnloading(multiSttModel4())
                          }
                        >
                          {isModelLoading(multiSttModel4())
                            ? t("multiStt.models.loadingModel")
                            : t("multiStt.models.loadModel")}
                        </Button>
                      )}
                      <Button
                        variant="secondary"
                        size="sm"
                        onClick={() => handleModel4Select(null)}
                      >
                        {t("multiStt.models.disableModel")}
                      </Button>
                    </>
                  )}
                </div>
                <PerModelLanguageSelector
                  slot={4}
                  modelId={multiSttModel4()}
                  modelInfo={model4Info()}
                />
                {multiSttModel4() && model4Info()?.supports_translation && (
                  <div class="flex items-center gap-2 mt-1 ml-1">
                    <ToggleSwitch
                      checked={
                        (getSetting(
                          "multi_stt_translate_model_4",
                        ) as boolean) ?? false
                      }
                      onChange={(enabled) =>
                        updateSetting("multi_stt_translate_model_4", enabled)
                      }
                      isUpdating={isUpdating("multi_stt_translate_model_4")}
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

          <SettingsGroup title={t("multiStt.performanceMode.title")}>
            <ToggleSwitch
              checked={
                (getSetting("multi_stt_performance_mode_enabled") as boolean) ??
                false
              }
              onChange={(enabled) =>
                updateSetting("multi_stt_performance_mode_enabled", enabled)
              }
              isUpdating={isUpdating("multi_stt_performance_mode_enabled")}
              label={t("multiStt.performanceMode.enabled.label")}
              description={t("multiStt.performanceMode.enabled.description")}
              descriptionMode="tooltip"
              grouped={true}
            />
          </SettingsGroup>

          {((getSetting("multi_stt_performance_mode_enabled") as boolean) ??
          false) ? (
            <SettingsGroup title={t("multiStt.performanceMode.title")}>
              <div class="space-y-3">
                <ToggleSwitch
                  checked={
                    (getSetting(
                      "multi_stt_performance_mode_trigger_on_start",
                    ) as boolean) ?? false
                  }
                  onChange={(enabled) =>
                    updateSetting(
                      "multi_stt_performance_mode_trigger_on_start",
                      enabled,
                    )
                  }
                  isUpdating={isUpdating(
                    "multi_stt_performance_mode_trigger_on_start",
                  )}
                  label={t("multiStt.performanceMode.triggerOnStart.label")}
                  description={t(
                    "multiStt.performanceMode.triggerOnStart.description",
                  )}
                  descriptionMode="tooltip"
                  grouped={true}
                />
                <SettingContainer
                  title={t("multiStt.performanceMode.fullPowerLabel")}
                  description={t(
                    "multiStt.performanceMode.fullPowerDescription",
                  )}
                  descriptionMode="tooltip"
                  layout="horizontal"
                  grouped={true}
                >
                  <KeyComboInput settingKey="multi_stt_performance_mode_full_power_shortcut" />
                </SettingContainer>
                <SettingContainer
                  title={t("multiStt.performanceMode.normalModeLabel")}
                  description={t(
                    "multiStt.performanceMode.normalModeDescription",
                  )}
                  descriptionMode="tooltip"
                  layout="horizontal"
                  grouped={true}
                >
                  <KeyComboInput settingKey="multi_stt_performance_mode_normal_shortcut" />
                </SettingContainer>
              </div>
            </SettingsGroup>
          ) : null}

          <SettingsGroup title={t("multiStt.mergePrompt.title")}>
            <SettingContainer
              title={t("multiStt.mergePrompt.description")}
              description={t("multiStt.mergePrompt.promptTip", {
                output: "${output}",
                output2: "${output2}",
                output3: "${output3}",
                output4: "${output4}",
              })}
              descriptionMode="tooltip"
              layout="stacked"
              grouped={true}
            >
              <div class="space-y-3">
                <div class="space-y-2 flex flex-col">
                  <label class="text-sm font-semibold">
                    {t("multiStt.mergePrompt.promptName")}
                  </label>
                  <Input
                    type="text"
                    value={draftName()}
                    onInput={(e) => setDraftName(e.target.value)}
                    placeholder={t(
                      "multiStt.mergePrompt.promptNamePlaceholder",
                    )}
                    variant="compact"
                  />
                </div>
                <div class="space-y-2 flex flex-col">
                  <label class="text-sm font-semibold">
                    {t("multiStt.mergePrompt.promptLabel")}
                  </label>
                  <Textarea
                    value={draftText()}
                    onInput={(e) => setDraftText(e.target.value)}
                    placeholder={t("multiStt.mergePrompt.promptPlaceholder")}
                  />
                  <p class="text-xs text-mid-gray/70">
                    <TranslatedMarkup
                      text={t("multiStt.mergePrompt.promptTip", {
                        output: "${output}",
                        output2: "${output2}",
                        output3: "${output3}",
                        output4: "${output4}",
                      })}
                    />
                  </p>
                </div>
                <div class="flex gap-2 pt-2">
                  <Button
                    onClick={handleSaveMergePrompt}
                    variant="primary"
                    size="md"
                    disabled={!draftName().trim() || !draftText().trim()}
                  >
                    {t("multiStt.mergePrompt.savePrompt")}
                  </Button>
                  {multiSttMergePrompt() && (
                    <Button
                      onClick={handleClearMergePrompt}
                      variant="secondary"
                      size="md"
                    >
                      {t("multiStt.mergePrompt.clearPrompt")}
                    </Button>
                  )}
                </div>
                {!multiSttMergePrompt() && (
                  <Alert variant="info" contained>
                    <p class="text-sm">{t("multiStt.mergePrompt.noPrompt")}</p>
                  </Alert>
                )}
              </div>
            </SettingContainer>
          </SettingsGroup>

          <SettingsGroup title={t("multiStt.streamingFirst.title")}>
            <div class="space-y-3">
              <ToggleSwitch
                checked={
                  (getSetting(
                    "multi_stt_streaming_first_enabled",
                  ) as boolean) ?? false
                }
                onChange={(enabled) =>
                  updateSetting("multi_stt_streaming_first_enabled", enabled)
                }
                isUpdating={isUpdating("multi_stt_streaming_first_enabled")}
                label={t("multiStt.streamingFirst.enabledLabel")}
                description={t("multiStt.streamingFirst.enabledDescription")}
                descriptionMode="tooltip"
                grouped={false}
              />
              {((getSetting("multi_stt_streaming_first_enabled") as boolean) ??
                false) && (
                <>
                  <Slider
                    value={
                      (getSetting("multi_stt_streaming_pause_ms") as number) ??
                      1000
                    }
                    onChange={(value) =>
                      updateSetting(
                        "multi_stt_streaming_pause_ms",
                        Math.round(value),
                      )
                    }
                    min={100}
                    max={10000}
                    step={100}
                    label={t("multiStt.streamingFirst.pauseLabel")}
                    description={t("multiStt.streamingFirst.pauseDescription")}
                    descriptionMode="tooltip"
                    grouped={false}
                    formatValue={(v) => `${Math.round(v)} ms`}
                    onReset={() =>
                      updateSetting("multi_stt_streaming_pause_ms", 1000)
                    }
                    disabled={isUpdating("multi_stt_streaming_pause_ms")}
                  />
                  <Slider
                    value={
                      (getSetting(
                        "multi_stt_streaming_context_chunks",
                      ) as number) ?? 1
                    }
                    onChange={(value) =>
                      updateSetting(
                        "multi_stt_streaming_context_chunks",
                        Math.round(value),
                      )
                    }
                    min={0}
                    max={3}
                    step={1}
                    label={t("multiStt.streamingFirst.contextLabel")}
                    description={t(
                      "multiStt.streamingFirst.contextDescription",
                    )}
                    descriptionMode="tooltip"
                    grouped={false}
                    formatValue={(v) =>
                      v === 0
                        ? t("multiStt.streamingFirst.contextOff")
                        : t("multiStt.streamingFirst.contextValue", {
                            count: Math.round(v),
                          })
                    }
                    onReset={() =>
                      updateSetting("multi_stt_streaming_context_chunks", 1)
                    }
                    disabled={isUpdating("multi_stt_streaming_context_chunks")}
                  />
                  <Alert variant="info" contained>
                    <p class="text-sm">{t("multiStt.streamingFirst.note")}</p>
                  </Alert>
                </>
              )}
            </div>
          </SettingsGroup>

          <SettingsGroup title={t("multiStt.status.title")}>
            <div class="space-y-2">
              <div class="flex items-center gap-2 p-2 bg-mid-gray/5 rounded-md">
                <span class="w-2 h-2 rounded-full bg-green-500" />
                <span class="text-sm">
                  {t("multiStt.status.primary")}: {primaryModelName()}
                </span>
              </div>
              {selectedModel2Name() && (
                <div class="flex items-center gap-2 p-2 bg-mid-gray/5 rounded-md">
                  <span
                    class={`w-2 h-2 rounded-full ${isExtraModelLoaded(multiSttModel2()) ? "bg-green-500" : "bg-mid-gray/50"}`}
                  />
                  <span class="text-sm">
                    {t("multiStt.status.secondary")}: {selectedModel2Name()}
                  </span>
                  <span
                    class={`text-xs ${isExtraModelLoaded(multiSttModel2()) ? "text-green-500" : "text-mid-gray/50"}`}
                  >
                    {isExtraModelLoaded(multiSttModel2())
                      ? t("multiStt.models.loaded")
                      : t("multiStt.models.notLoaded")}
                  </span>
                </div>
              )}
              {selectedModel3Name() && (
                <div class="flex items-center gap-2 p-2 bg-mid-gray/5 rounded-md">
                  <span
                    class={`w-2 h-2 rounded-full ${isExtraModelLoaded(multiSttModel3()) ? "bg-green-500" : "bg-mid-gray/50"}`}
                  />
                  <span class="text-sm">
                    {t("multiStt.status.tertiary")}: {selectedModel3Name()}
                  </span>
                  <span
                    class={`text-xs ${isExtraModelLoaded(multiSttModel3()) ? "text-green-500" : "text-mid-gray/50"}`}
                  >
                    {isExtraModelLoaded(multiSttModel3())
                      ? t("multiStt.models.loaded")
                      : t("multiStt.models.notLoaded")}
                  </span>
                </div>
              )}
              {selectedModel4Name() && (
                <div class="flex items-center gap-2 p-2 bg-mid-gray/5 rounded-md">
                  <span
                    class={`w-2 h-2 rounded-full ${isExtraModelLoaded(multiSttModel4()) ? "bg-green-500" : "bg-mid-gray/50"}`}
                  />
                  <span class="text-sm">
                    {t("multiStt.status.quaternary")}: {selectedModel4Name()}
                  </span>
                  <span
                    class={`text-xs ${isExtraModelLoaded(multiSttModel4()) ? "text-green-500" : "text-mid-gray/50"}`}
                  >
                    {isExtraModelLoaded(multiSttModel4())
                      ? t("multiStt.models.loaded")
                      : t("multiStt.models.notLoaded")}
                  </span>
                </div>
              )}
              {!selectedModel2Name() &&
                !selectedModel3Name() &&
                !selectedModel4Name() && (
                  <p class="text-sm text-mid-gray/60">
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
