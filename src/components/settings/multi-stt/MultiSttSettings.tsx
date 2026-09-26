import { createSignal, createEffect, createMemo, For, Show } from "solid-js";
import { useTranslation, TranslatedMarkup } from "@/i18n/useTranslation";
import { listen } from "@tauri-apps/api/event";
import { sessionToast as toast } from "@/lib/sessionToast";
import {
  type ModelInfo,
  type MultiSttExtraModel_Serialize as ExtraSlotConfig,
} from "@/bindings";
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
import { ModelBackendDropdown } from "./ModelBackendDropdown";
import StreamingLatencyControl from "../../model-selector/StreamingLatencyControl";
import { resolveModelSetting } from "@/lib/modelId";
import {
  DEFAULT_LATENCY_PRESET,
  R2T2_CHUNK_MS_DEFAULT,
} from "@/lib/streamingLatency";
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

/**
 * Most extra models Multi-STT runs beside the primary: `MULTI_STT_MAX_EXTRA_MODELS`
 * in `src-tauri/src/settings.rs`, which clamps whatever count is sent.
 */
const MAX_EXTRA_MODELS = 8;

/** The merge-prompt placeholder a Multi-STT slot ("Model N") fills. */
const placeholderFor = (slot: number) => "${output" + slot + "}";

/** Whether `modelId` is in one of the page's model-id sets. */
const hasId = (ids: Set<string>, modelId: string | null | undefined) =>
  modelId != null && ids.has(modelId);

interface DropdownOption {
  value: string;
  label: string;
}

interface PerModelLanguageSelectorProps {
  /** The slot's "Model N" number (2 is the first extra). */
  slot: number;
  modelId: string | null;
  modelInfo: ModelInfo | undefined;
  language: string | null;
  onSelect: (language: string | null) => void;
}

const PerModelLanguageSelector = (props: PerModelLanguageSelectorProps) => {
  const { t } = useTranslation();

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
      props.language || "auto",
      modelInfo.supported_languages,
      modelInfo.supports_language_detection,
    );
  });

  const selectedValue = (modelInfo: ModelInfo) => {
    const lang = props.language;
    return modelInfo.supports_language_detection
      ? lang
      : lang && lang !== "auto"
        ? lang
        : effectiveLang();
  };

  const placeholder = (modelInfo: ModelInfo) =>
    modelInfo.supports_language_detection
      ? (getLanguageLabel("auto") ?? t("settings.general.language.auto"))
      : (getLanguageLabel(effectiveLang()) ?? t("multiStt.models.notSelected"));

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
              {t("multiStt.models.slotLanguage", { number: props.slot })}
            </label>
            <Dropdown
              selectedValue={selectedValue(modelInfo())}
              options={langOptions()}
              onSelect={(value) => props.onSelect(value || null)}
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

interface ExtraModelSlotProps {
  /** The slot's "Model N" number (2 is the first extra). */
  slot: number;
  config: ExtraSlotConfig;
  /** Downloaded models this slot may pick: none that another slot runs. */
  options: DropdownOption[];
  modelInfo: ModelInfo | undefined;
  isLoaded: boolean;
  isLoading: boolean;
  isUnloading: boolean;
  isUpdating: boolean;
  onChange: (patch: Partial<ExtraSlotConfig>) => void;
  onLoad: () => void;
  onUnload: () => void;
}

/**
 * Streaming latency of one extra slot, shown only for a model with a native
 * streaming latency control. The value is the slot's own override when it has
 * one, else the model's per-model setting (the status-bar value, shared with
 * its quant siblings); moving the slider writes the slot override, which the
 * extra's live stream in Multi Streaming STT resolves before the per-model
 * entry.
 */
const ExtraSlotLatency = (props: {
  slot: number;
  modelId: string;
  modelInfo: ModelInfo;
  config: ExtraSlotConfig;
  onChange: (patch: Partial<ExtraSlotConfig>) => void;
}) => {
  const { t } = useTranslation();
  const { getSetting } = useSettings();
  const hasOverride = () =>
    props.config.latency_preset != null || props.config.chunk_ms != null;
  const preset = () =>
    props.config.latency_preset ??
    resolveModelSetting(
      getSetting("native_streaming_latency_presets"),
      props.modelId,
      DEFAULT_LATENCY_PRESET,
    );
  const chunkMs = () =>
    props.config.chunk_ms ??
    resolveModelSetting<number>(
      getSetting("native_streaming_chunk_ms"),
      props.modelId,
      R2T2_CHUNK_MS_DEFAULT,
    );

  return (
    <Show when={props.modelInfo.native_streaming_latency_kind}>
      {(kind) => (
        <div class="mt-2 ml-1 max-w-sm rounded-md border border-mid-gray/20 text-xs">
          <StreamingLatencyControl
            kind={kind()}
            preset={preset()}
            chunkMs={chunkMs()}
            label={t("multiStt.models.latency", { number: props.slot })}
            hideHint={true}
            onPreset={(latency_preset) =>
              props.onChange({ latency_preset, chunk_ms: null })
            }
            onChunkMs={(chunk_ms) =>
              props.onChange({ chunk_ms, latency_preset: null })
            }
          />
          <div class="flex items-start justify-between gap-2 px-2 pb-1.5 text-[11px] leading-snug text-text/45">
            <span>{t("multiStt.models.latencyHint")}</span>
            <Show when={hasOverride()}>
              <button
                type="button"
                onClick={() =>
                  props.onChange({ latency_preset: null, chunk_ms: null })
                }
                class="shrink-0 rounded px-1 text-accent hover:bg-mid-gray/10"
              >
                {t("multiStt.models.latencyFollowModel")}
              </button>
            </Show>
          </div>
        </div>
      )}
    </Show>
  );
};

/** One extra Multi-STT slot: its model, load state, language, latency, backend and translation. */
const ExtraModelSlot = (props: ExtraModelSlotProps) => {
  const { t } = useTranslation();
  const modelId = () => props.config.model_id ?? null;

  return (
    <SettingContainer
      title={t("multiStt.models.slotTitle", { number: props.slot })}
      description={t("multiStt.models.slotDescription", {
        number: props.slot,
        placeholder: placeholderFor(props.slot),
      })}
      descriptionMode="tooltip"
      layout="horizontal"
      grouped={true}
    >
      <div class="flex items-center gap-2 min-w-0">
        <Dropdown
          selectedValue={modelId()}
          options={props.options}
          onSelect={(value) => props.onChange({ model_id: value || null })}
          placeholder={t("multiStt.models.notSelected")}
          disabled={props.options.length === 0}
          class="flex-1 min-w-0"
        />
        <Show when={modelId()}>
          <span
            class={`text-xs whitespace-nowrap ${props.isLoaded ? "text-green-500" : "text-mid-gray/50"}`}
          >
            {props.isLoaded
              ? t("multiStt.models.loaded")
              : t("multiStt.models.notLoaded")}
          </span>
          <Show
            when={props.isLoaded}
            fallback={
              <Button
                variant="secondary"
                size="sm"
                onClick={() => props.onLoad()}
                disabled={props.isLoading || props.isUnloading}
              >
                {props.isLoading
                  ? t("multiStt.models.loadingModel")
                  : t("multiStt.models.loadModel")}
              </Button>
            }
          >
            <Button
              variant="secondary"
              size="sm"
              onClick={() => props.onUnload()}
              disabled={props.isUnloading || props.isLoading}
            >
              {props.isUnloading
                ? t("multiStt.models.unloadingModel")
                : t("multiStt.models.unloadModel")}
            </Button>
          </Show>
          <Button
            variant="secondary"
            size="sm"
            onClick={() => props.onChange({ model_id: null })}
          >
            {t("multiStt.models.disableModel")}
          </Button>
        </Show>
      </div>
      <PerModelLanguageSelector
        slot={props.slot}
        modelId={modelId()}
        modelInfo={props.modelInfo}
        language={props.config.language ?? null}
        onSelect={(language) => props.onChange({ language })}
      />
      <Show
        when={
          modelId() && props.modelInfo?.supports_streaming
            ? props.modelInfo
            : undefined
        }
      >
        {(info) => (
          <ExtraSlotLatency
            slot={props.slot}
            modelId={modelId()!}
            modelInfo={info()}
            config={props.config}
            onChange={props.onChange}
          />
        )}
      </Show>
      <ModelBackendDropdown
        modelId={modelId()}
        label={t("multiStt.models.backend")}
        class="mt-2"
      />
      <Show when={modelId() && props.modelInfo?.supports_translation}>
        <div class="flex items-center gap-2 mt-1 ml-1">
          <ToggleSwitch
            checked={props.config.translate ?? false}
            onChange={(enabled) => props.onChange({ translate: enabled })}
            isUpdating={props.isUpdating}
            label={t("multiStt.models.translateToEnglish")}
            description={t("settings.advanced.translateToEnglish.description")}
            descriptionMode="tooltip"
            grouped={false}
          />
        </div>
      </Show>
    </SettingContainer>
  );
};

export const MultiSttSettings = () => {
  const { t } = useTranslation();
  const {
    getSetting,
    updateSetting,
    isUpdating,
    updateMultiSttExtraModel,
    setMultiSttExtraModelCount,
  } = useSettings();
  const store = useModelStore();
  const models = () => store.models;
  const currentModel = () => store.currentModel;

  const multiSttEnabled = () => getSetting("multi_stt_enabled") ?? false;
  const extraSlots = (): ExtraSlotConfig[] =>
    (getSetting("multi_stt_extra_models") ?? []).map((slot) => ({
      model_id: slot.model_id ?? null,
      language: slot.language ?? null,
      translate: slot.translate ?? false,
      latency_preset: slot.latency_preset ?? null,
      chunk_ms: slot.chunk_ms ?? null,
    }));
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

  /** The models slot `index` may pick: not the primary, not another slot's. */
  const modelOptionsFor = (index: number): DropdownOption[] =>
    downloadedModels()
      .filter(
        (m) =>
          m.id !== primaryModelId() &&
          !extraSlots().some(
            (slot, other) => other !== index && slot.model_id === m.id,
          ),
      )
      .map((m) => ({ value: m.id, label: m.name }));

  const modelInfoFor = (id: string | null | undefined) =>
    id ? downloadedModels().find((m) => m.id === id) : undefined;

  const countOptions: DropdownOption[] = Array.from(
    { length: MAX_EXTRA_MODELS },
    (_, i) => ({ value: String(i + 1), label: String(i + 1) }),
  );

  /** The merge-prompt placeholders of every slot, `${output}` first. */
  const placeholders = createMemo(() =>
    ["${output}"].concat(extraSlots().map((_, i) => placeholderFor(i + 2))),
  );
  /** Slots with a model whose placeholder the saved prompt never uses. */
  const unusedPlaceholders = createMemo(() => {
    const prompt = multiSttMergePrompt()?.prompt;
    if (!prompt) return [];
    return extraSlots()
      .map((slot, i) => ({ slot, placeholder: placeholderFor(i + 2) }))
      .filter(
        ({ slot, placeholder }) =>
          slot.model_id && !prompt.includes(placeholder),
      )
      .map(({ placeholder }) => placeholder);
  });

  /** The merge-prompt tip, listing the placeholders of the current slots. */
  const placeholderTip = () =>
    t("multiStt.mergePrompt.placeholderTip", {
      first: placeholders()[0],
      list: placeholders()
        .slice(1)
        .map((p) => "<code>" + p + "</code>")
        .join(", "),
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

  /** The slots that run a model, as "Model N" and model id. */
  const configuredSlots = createMemo(() =>
    extraSlots().flatMap((config, index) =>
      config.model_id ? [{ slot: index + 2, modelId: config.model_id }] : [],
    ),
  );

  const primaryModelName = createMemo(() => {
    const id = primaryModelId();
    return id
      ? downloadedModels().find((m) => m.id === id)?.name || id
      : t("multiStt.models.noPrimaryModel");
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
                title={t("multiStt.models.count.label")}
                description={t("multiStt.models.count.description", {
                  max: MAX_EXTRA_MODELS,
                })}
                descriptionMode="tooltip"
                layout="horizontal"
                grouped={true}
              >
                <Dropdown
                  selectedValue={String(extraSlots().length)}
                  options={countOptions}
                  onSelect={(value) => {
                    const count = Number(value);
                    if (count >= 1 && count !== extraSlots().length) {
                      void setMultiSttExtraModelCount(count);
                    }
                  }}
                  disabled={isUpdating("multi_stt_extra_models")}
                  class="min-w-[80px]"
                />
              </SettingContainer>
              <For each={extraSlots()} keyed={false}>
                {(config, index) => (
                  <ExtraModelSlot
                    slot={index + 2}
                    config={config()}
                    options={modelOptionsFor(index)}
                    modelInfo={modelInfoFor(config().model_id)}
                    isLoaded={hasId(loadedExtraModels(), config().model_id)}
                    isLoading={hasId(loadingModelIds(), config().model_id)}
                    isUnloading={hasId(unloadingModelIds(), config().model_id)}
                    isUpdating={isUpdating(
                      `multi_stt_extra_models_${index + 2}`,
                    )}
                    onChange={(patch) =>
                      void updateMultiSttExtraModel(index + 2, patch)
                    }
                    onLoad={() => void handleLoadModel(config().model_id)}
                    onUnload={() => void handleUnloadModel(config().model_id)}
                  />
                )}
              </For>
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
            {((getSetting("multi_stt_performance_mode_enabled") as boolean) ??
            false) ? (
              <>
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
              </>
            ) : null}
          </SettingsGroup>

          <SettingsGroup title={t("multiStt.mergePrompt.title")}>
            <SettingContainer
              title={t("multiStt.mergePrompt.description")}
              // The tooltip renders plain text; the tip's <code> markup is
              // only rendered by the TranslatedMarkup copy below.
              description={placeholderTip().replace(/<\/?code>/g, "")}
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
                    <TranslatedMarkup text={placeholderTip()} />
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
                <Show when={unusedPlaceholders().length > 0}>
                  <Alert variant="warning" contained>
                    <p class="text-sm">
                      {t("multiStt.mergePrompt.missingPlaceholders", {
                        placeholders: unusedPlaceholders().join(", "),
                      })}
                    </p>
                  </Alert>
                </Show>
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
                  <ToggleSwitch
                    checked={
                      (getSetting(
                        "multi_stt_streaming_multi_enabled",
                      ) as boolean) ?? false
                    }
                    onChange={(enabled) =>
                      updateSetting(
                        "multi_stt_streaming_multi_enabled",
                        enabled,
                      )
                    }
                    isUpdating={isUpdating("multi_stt_streaming_multi_enabled")}
                    label={t("multiStt.streamingFirst.multi.label")}
                    description={t("multiStt.streamingFirst.multi.description")}
                    descriptionMode="tooltip"
                    grouped={false}
                  />
                  {((getSetting(
                    "multi_stt_streaming_multi_enabled",
                  ) as boolean) ?? false) ? (
                    <>
                      <Alert variant="info" contained>
                        <p class="text-sm">
                          {t("multiStt.streamingFirst.multi.note")}
                        </p>
                      </Alert>
                      <ToggleSwitch
                        checked={
                          (getSetting(
                            "multi_stt_streaming_multi_debug_view",
                          ) as boolean) ?? false
                        }
                        onChange={(enabled) =>
                          updateSetting(
                            "multi_stt_streaming_multi_debug_view",
                            enabled,
                          )
                        }
                        isUpdating={isUpdating(
                          "multi_stt_streaming_multi_debug_view",
                        )}
                        label={t("multiStt.streamingFirst.debugView.label")}
                        description={t(
                          "multiStt.streamingFirst.debugView.description",
                        )}
                        descriptionMode="tooltip"
                        grouped={false}
                      />
                    </>
                  ) : null}
                  {/* The pause is what ends a chunk in *both* of this mode's
                      forms — in this one it is what triggers the merge over the
                      models' live texts, so it is the setting that matters most
                      here and it stays on screen. */}
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
              <For each={configuredSlots()}>
                {({ slot, modelId }) => (
                  <div class="flex items-center gap-2 p-2 bg-mid-gray/5 rounded-md">
                    <span
                      class={`w-2 h-2 rounded-full ${hasId(loadedExtraModels(), modelId) ? "bg-green-500" : "bg-mid-gray/50"}`}
                    />
                    <span class="text-sm">
                      {t("multiStt.models.slotTitle", { number: slot })}:{" "}
                      {modelInfoFor(modelId)?.name || modelId}
                    </span>
                    <span
                      class={`text-xs ${hasId(loadedExtraModels(), modelId) ? "text-green-500" : "text-mid-gray/50"}`}
                    >
                      {hasId(loadedExtraModels(), modelId)
                        ? t("multiStt.models.loaded")
                        : t("multiStt.models.notLoaded")}
                    </span>
                  </div>
                )}
              </For>
              <Show when={configuredSlots().length === 0}>
                <p class="text-sm text-mid-gray/60">
                  {t("multiStt.status.noModels")}
                </p>
              </Show>
            </div>
          </SettingsGroup>
        </>
      )}
    </div>
  );
};
