import {
  createSignal,
  createEffect,
  createMemo,
  For,
  Show,
  untrack,
} from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import { ask } from "@tauri-apps/plugin-dialog";
import {
  AudioLines,
  Blocks,
  ChevronDown,
  FolderOpen,
  Globe,
  Languages,
  RefreshCw,
  Search,
} from "@/components/icons/lucide";
import type { ModelCardStatus } from "@/components/onboarding";
import { ModelCard } from "@/components/onboarding";
import { isLegacySource } from "@/components/onboarding/ModelCard";
import { useModelStore } from "@/stores/modelStore";
import {
  getLanguageLabel,
  MODEL_CAPABILITY_LANGUAGES,
  supportsLanguageCode,
} from "@/lib/constants/languages.ts";
import { commands, type ArchPluginInfo, type ModelInfo } from "@/bindings";
import { sessionToast as toast } from "@/lib/sessionToast";

// check if model supports a language based on its supported_languages list
const modelSupportsLanguage = (model: ModelInfo, langCode: string): boolean => {
  return supportsLanguageCode(model.supported_languages, langCode);
};

const openPluginsFolder = async () => {
  try {
    const result = await commands.openPluginsFolder();
    if (result.status !== "ok") {
      throw new Error(String(result.error));
    }
  } catch (error) {
    console.error("Failed to open plugins folder:", error);
  }
};

const openModelsFolder = async () => {
  try {
    const result = await commands.openModelsFolder();
    if (result.status !== "ok") {
      throw new Error(String(result.error));
    }
  } catch (error) {
    console.error("Failed to open models folder:", error);
  }
};

export const ModelsSettings = () => {
  const { t } = useTranslation();
  const [switchingModelId, setSwitchingModelId] = createSignal<string | null>(
    null,
  );
  const [searchQuery, setSearchQuery] = createSignal("");
  const [filterStreaming, setFilterStreaming] = createSignal(false);
  const [filterTranslation, setFilterTranslation] = createSignal(false);
  const [languageFilter, setLanguageFilter] = createSignal("all");
  const [languageDropdownOpen, setLanguageDropdownOpen] = createSignal(false);
  const [languageSearch, setLanguageSearch] = createSignal("");
  let languageDropdownRef: HTMLDivElement | undefined;
  let languageSearchInputRef: HTMLInputElement | undefined;
  const store = useModelStore();
  const models = () => store.models;
  const currentModel = () => store.currentModel;
  const downloadingModels = () => store.downloadingModels;
  const downloadProgress = () => store.downloadProgress;
  const downloadStats = () => store.downloadStats;
  const verifyingModels = () => store.verifyingModels;
  const loading = () => store.loading;
  const isRescanning = () => store.isRescanning;
  // Stable action references, read once (see hooks/useSettings.ts).
  const {
    downloadModel,
    cancelDownload,
    selectModel,
    deleteModel,
    rescanLocalModels,
  } = untrack(() => ({
    downloadModel: store.downloadModel,
    cancelDownload: store.cancelDownload,
    selectModel: store.selectModel,
    deleteModel: store.deleteModel,
    rescanLocalModels: store.rescanLocalModels,
  }));

  // click outside handler for language dropdown
  createEffect(
    () => undefined,
    () => {
      const handleClickOutside = (event: MouseEvent) => {
        if (
          languageDropdownRef &&
          !languageDropdownRef.contains(event.target as Node)
        ) {
          setLanguageDropdownOpen(false);
          setLanguageSearch("");
        }
      };
      document.addEventListener("mousedown", handleClickOutside);
      return () =>
        document.removeEventListener("mousedown", handleClickOutside);
    },
  );

  // focus search input when dropdown opens
  createEffect(
    () => languageDropdownOpen(),
    (open) => {
      if (open && languageSearchInputRef) {
        languageSearchInputRef.focus();
      }
    },
  );

  // filtered languages for dropdown (exclude "auto")
  const filteredLanguages = createMemo(() => {
    return MODEL_CAPABILITY_LANGUAGES.filter((lang) =>
      lang.label.toLowerCase().includes(languageSearch().toLowerCase()),
    );
  });

  // Get selected language label
  const selectedLanguageLabel = createMemo(() => {
    if (languageFilter() === "all") {
      return t("settings.models.filters.allLanguages");
    }
    return getLanguageLabel(languageFilter()) || "";
  });

  const getModelStatus = (modelId: string): ModelCardStatus => {
    if (modelId in verifyingModels()) {
      return "verifying";
    }
    if (modelId in downloadingModels()) {
      return "downloading";
    }
    if (switchingModelId() === modelId) {
      return "switching";
    }
    const model = models().find((m: ModelInfo) => m.id === modelId);
    // A stale persisted selection must never make a missing model look Active.
    // Catalog models without files should offer their recovery action instead.
    if (!model?.is_downloaded) {
      return "downloadable";
    }
    if (modelId === currentModel()) {
      return "active";
    }
    return "available";
  };

  const getDownloadProgress = (modelId: string): number | undefined => {
    const progress = downloadProgress()[modelId];
    return progress?.percentage;
  };

  const getDownloadSpeed = (modelId: string): number | undefined => {
    const stats = downloadStats()[modelId];
    return stats?.speed;
  };

  // The store's actions never throw: a failure returns false and leaves its
  // message in `store.error`, which this page otherwise never shows.
  const reportStoreError = (context: string) => {
    const message = untrack(() => store.error);
    console.error(`${context}:`, message);
    if (message) toast.error(message);
  };

  const handleModelSelect = async (modelId: string) => {
    setSwitchingModelId(modelId);
    const ok = await selectModel(modelId);
    setSwitchingModelId(null);
    if (!ok) reportStoreError(`Failed to switch to model ${modelId}`);
  };

  const handleModelDownload = async (modelId: string) => {
    await downloadModel(modelId);
  };

  const handleModelDelete = async (modelId: string) => {
    const model = models().find((m: ModelInfo) => m.id === modelId);
    const modelName = model?.name || modelId;
    const isActive = modelId === currentModel();

    const confirmed = await ask(
      isActive
        ? t("settings.models.deleteActiveConfirm", { modelName })
        : t("settings.models.deleteConfirm", { modelName }),
      {
        title: t("settings.models.deleteTitle"),
        kind: "warning",
      },
    );

    if (confirmed && !(await deleteModel(modelId))) {
      reportStoreError(`Failed to delete model ${modelId}`);
    }
  };

  const handleModelCancel = async (modelId: string) => {
    if (!(await cancelDownload(modelId))) {
      reportStoreError(`Failed to cancel download for ${modelId}`);
    }
  };

  // Filter models by search query (name + description), language filter, and toggles
  const filteredModels = createMemo(() => {
    const q = searchQuery().trim().toLowerCase();
    return models().filter((model: ModelInfo) => {
      // Hide deprecated legacy (Url-sourced .bin/ONNX) downloads, superseded
      // by the catalog GGUFs, unless already on disk: they stay runnable, but
      // we no longer advertise the download.
      if (isLegacySource(model) && !model.is_downloaded) return false;
      if (languageFilter() !== "all") {
        if (!modelSupportsLanguage(model, languageFilter())) return false;
      }
      if (filterStreaming() && !model.supports_streaming) return false;
      if (filterTranslation() && !model.supports_translation) return false;

      if (q) {
        const haystack = `${model.name} ${model.description}`.toLowerCase();
        if (!haystack.includes(q)) return false;
      }
      return true;
    });
  });

  // Split filtered models into downloaded (including custom) and available sections
  const splitModels = createMemo(() => {
    const downloaded: ModelInfo[] = [];
    const available: ModelInfo[] = [];

    for (const model of filteredModels()) {
      if (
        model.is_custom ||
        model.is_downloaded ||
        model.id in downloadingModels()
      ) {
        downloaded.push(model);
      } else {
        available.push(model);
      }
    }

    // Sort: active model first, then non-custom, then custom at the bottom
    downloaded.sort((a, b) => {
      if (a.id === currentModel()) return -1;
      if (b.id === currentModel()) return 1;
      if (a.is_custom !== b.is_custom) return a.is_custom ? 1 : -1;
      return 0;
    });

    return {
      downloadedModels: downloaded,
      availableModels: available,
    };
  });

  const [archPlugins, setArchPlugins] = createSignal<ArchPluginInfo[]>([]);

  const fetchArchPlugins = () => {
    commands.getArchPlugins().then((result) => {
      if (result.status === "ok") {
        setArchPlugins(result.data);
      }
    });
  };

  createEffect(
    () => undefined,
    () => {
      fetchArchPlugins();
    },
  );

  const externalPluginCount = () => archPlugins().length;

  // Complete sentences per state rather than prose assembled from fragments:
  // a translator can reorder "Open ... (N loaded)" freely, which a template
  // literal of concatenated pieces would not allow.
  const pluginsFolderLabel = () => t("settings.models.openPluginsFolder");
  const pluginsFolderTooltip = () =>
    externalPluginCount() > 0
      ? t("settings.models.openPluginsFolderLoaded", {
          count: externalPluginCount(),
        })
      : t("settings.models.openPluginsFolderEmpty");

  return (
    <Show
      when={!loading()}
      fallback={
        <div class="max-w-3xl w-full mx-auto">
          <div class="flex items-center justify-center py-16">
            <div class="w-8 h-8 border-2 border-accent border-t-transparent rounded-full animate-spin" />
          </div>
        </div>
      }
    >
      <div class="max-w-3xl w-full mx-auto space-y-4">
        <div class="mb-4">
          <h1 class="text-xl font-semibold mb-2">
            {t("settings.models.title")}
          </h1>
          <p class="text-sm text-text/60">{t("settings.models.description")}</p>
        </div>

        {/* Search bar -- filter the catalog by name or description */}
        <div class="relative">
          <Search class="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-text/40 pointer-events-none" />
          <input
            type="text"
            value={searchQuery()}
            onInput={(e) => setSearchQuery(e.currentTarget.value)}
            placeholder={t("settings.models.searchPlaceholder")}
            class="w-full pl-9 pr-3 py-2 text-sm bg-mid-gray/10 border border-mid-gray/40 rounded-lg focus:outline-none focus:ring-1 focus:ring-accent placeholder:text-text/40"
          />
        </div>

        <div class="space-y-6">
          {/* Downloaded Models Section -- header always visible so filter stays accessible */}
          <div class="space-y-3">
            <div class="flex items-center justify-between">
              <h2 class="text-sm font-medium text-text/60">
                {t("settings.models.yourModels")}
              </h2>
              <div class="flex items-center gap-2">
                {/* Open models directory in native file explorer */}
                <button
                  type="button"
                  onClick={openModelsFolder}
                  title={t("settings.models.openFolder")}
                  aria-label={t("settings.models.openFolder")}
                  class="flex items-center justify-center w-8 h-8 text-sm font-medium rounded-lg bg-mid-gray/10 text-text/60 hover:bg-mid-gray/20 transition-colors"
                >
                  <FolderOpen class="w-3.5 h-3.5" />
                </button>

                {/* Open architecture plugins directory (transcribe-arch-*.dll) */}
                <button
                  type="button"
                  onClick={openPluginsFolder}
                  title={pluginsFolderTooltip()}
                  aria-label={pluginsFolderLabel()}
                  class="relative flex items-center justify-center w-8 h-8 text-sm font-medium rounded-lg bg-mid-gray/10 text-text/60 hover:bg-mid-gray/20 transition-colors"
                >
                  <Blocks class="w-3.5 h-3.5" />
                  {externalPluginCount() > 0 && (
                    <span class="absolute -top-1 -right-1 w-2 h-2 rounded-full bg-accent" />
                  )}
                </button>

                {/* Rescan local sources for models added outside the app */}
                <button
                  type="button"
                  onClick={() => {
                    rescanLocalModels();
                    fetchArchPlugins();
                  }}
                  disabled={isRescanning()}
                  title={t("settings.models.rescan.tooltip")}
                  aria-label={t("settings.models.rescan.tooltip")}
                  class="flex items-center justify-center w-8 h-8 text-sm font-medium rounded-lg bg-mid-gray/10 text-text/60 hover:bg-mid-gray/20 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                >
                  <RefreshCw
                    class={`w-3.5 h-3.5 ${isRescanning() ? "animate-spin" : ""}`}
                  />
                </button>

                {/* Vertical divider separating action from filters */}
                <div class="h-4 w-px bg-mid-gray/30 mx-0.5" />
                <button
                  type="button"
                  onClick={() => setFilterStreaming((enabled) => !enabled)}
                  title={t("settings.models.filters.streaming")}
                  aria-label={t("settings.models.filters.streaming")}
                  aria-pressed={filterStreaming() ? "true" : "false"}
                  class={`flex items-center justify-center w-8 h-8 text-sm font-medium rounded-lg transition-colors ${
                    filterStreaming()
                      ? "bg-accent/20 text-accent hover:bg-accent/30"
                      : "bg-mid-gray/10 text-text/60 hover:bg-mid-gray/20"
                  }`}
                >
                  <AudioLines class="w-3.5 h-3.5" />
                </button>
                <button
                  type="button"
                  onClick={() => setFilterTranslation((enabled) => !enabled)}
                  title={t("settings.models.filters.translation")}
                  aria-label={t("settings.models.filters.translation")}
                  aria-pressed={filterTranslation() ? "true" : "false"}
                  class={`flex items-center justify-center w-8 h-8 text-sm font-medium rounded-lg transition-colors ${
                    filterTranslation()
                      ? "bg-accent/20 text-accent hover:bg-accent/30"
                      : "bg-mid-gray/10 text-text/60 hover:bg-mid-gray/20"
                  }`}
                >
                  <Languages class="w-3.5 h-3.5" />
                </button>
                {/* Language filter dropdown */}
                <div
                  class="relative"
                  ref={(el) => {
                    languageDropdownRef = el;
                  }}
                >
                  <button
                    type="button"
                    onClick={() =>
                      setLanguageDropdownOpen(!languageDropdownOpen())
                    }
                    class={`flex items-center gap-1.5 h-8 px-3 text-sm font-medium rounded-lg transition-colors ${
                      languageFilter() !== "all"
                        ? "bg-accent/20 text-accent"
                        : "bg-mid-gray/10 text-text/60 hover:bg-mid-gray/20"
                    }`}
                  >
                    <Globe class="w-3.5 h-3.5" />
                    <span class="max-w-[120px] truncate">
                      {selectedLanguageLabel()}
                    </span>
                    <ChevronDown
                      class={`w-3.5 h-3.5 transition-transform ${
                        languageDropdownOpen() ? "rotate-180" : ""
                      }`}
                    />
                  </button>

                  <Show when={languageDropdownOpen()}>
                    <div class="absolute top-full right-0 mt-1 w-56 bg-background border border-mid-gray/80 rounded-lg shadow-lg z-50 overflow-hidden">
                      <div class="p-2 border-b border-mid-gray/40">
                        <input
                          ref={(el) => {
                            languageSearchInputRef = el;
                          }}
                          type="text"
                          value={languageSearch()}
                          onInput={(e) =>
                            setLanguageSearch(e.currentTarget.value)
                          }
                          onKeyDown={(e) => {
                            if (
                              e.key === "Enter" &&
                              filteredLanguages().length > 0
                            ) {
                              setLanguageFilter(filteredLanguages()[0].value);
                              setLanguageDropdownOpen(false);
                              setLanguageSearch("");
                            } else if (e.key === "Escape") {
                              setLanguageDropdownOpen(false);
                              setLanguageSearch("");
                            }
                          }}
                          placeholder={t(
                            "settings.general.language.searchPlaceholder",
                          )}
                          class="w-full px-2 py-1 text-sm bg-mid-gray/10 border border-mid-gray/40 rounded-md focus:outline-none focus:ring-1 focus:ring-accent"
                        />
                      </div>
                      <div class="max-h-48 overflow-y-auto">
                        <button
                          type="button"
                          onClick={() => {
                            setLanguageFilter("all");
                            setLanguageDropdownOpen(false);
                            setLanguageSearch("");
                          }}
                          class={`w-full px-3 py-1.5 text-sm text-left transition-colors ${
                            languageFilter() === "all"
                              ? "bg-accent/20 text-accent font-semibold"
                              : "hover:bg-mid-gray/10"
                          }`}
                        >
                          {t("settings.models.filters.allLanguages")}
                        </button>
                        <For each={filteredLanguages()}>
                          {(lang) => (
                            <button
                              type="button"
                              onClick={() => {
                                setLanguageFilter(lang.value);
                                setLanguageDropdownOpen(false);
                                setLanguageSearch("");
                              }}
                              class={`w-full px-3 py-1.5 text-sm text-left transition-colors ${
                                languageFilter() === lang.value
                                  ? "bg-accent/20 text-accent font-semibold"
                                  : "hover:bg-mid-gray/10"
                              }`}
                            >
                              {lang.label}
                            </button>
                          )}
                        </For>
                        <Show when={filteredLanguages().length === 0}>
                          <div class="px-3 py-2 text-sm text-text/50 text-center">
                            {t("settings.general.language.noResults")}
                          </div>
                        </Show>
                      </div>
                    </div>
                  </Show>
                </div>
              </div>
            </div>
            <For
              each={splitModels().downloadedModels}
              keyed={(model) => model.id}
            >
              {(model) => (
                <ModelCard
                  model={model()}
                  status={getModelStatus(model().id)}
                  onSelect={handleModelSelect}
                  onDownload={handleModelDownload}
                  onDelete={handleModelDelete}
                  onCancel={handleModelCancel}
                  downloadProgress={getDownloadProgress(model().id)}
                  downloadSpeed={getDownloadSpeed(model().id)}
                  showRecommended={false}
                />
              )}
            </For>
          </div>

          {/* Available Models Section */}
          <Show when={splitModels().availableModels.length > 0}>
            <div class="space-y-3">
              <h2 class="text-sm font-medium text-text/60">
                {t("settings.models.availableModels")}
              </h2>
              <For
                each={splitModels().availableModels}
                keyed={(model) => model.id}
              >
                {(model) => (
                  <ModelCard
                    model={model()}
                    status={getModelStatus(model().id)}
                    onSelect={handleModelSelect}
                    onDownload={handleModelDownload}
                    onDelete={handleModelDelete}
                    onCancel={handleModelCancel}
                    downloadProgress={getDownloadProgress(model().id)}
                    downloadSpeed={getDownloadSpeed(model().id)}
                    showRecommended={true}
                  />
                )}
              </For>
            </div>
          </Show>
          <Show when={filteredModels().length === 0}>
            <div class="text-center py-8 text-text/50">
              {t("settings.models.noModelsMatch")}
            </div>
          </Show>
        </div>
      </div>
    </Show>
  );
};
