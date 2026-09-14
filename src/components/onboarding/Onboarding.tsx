import { createEffect, createMemo, createSignal, For } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import { sessionToast as toast } from "@/lib/sessionToast";
import { ChevronDown } from "@/components/icons/lucide";
import type { ModelInfo } from "@/bindings";
import type { ModelCardStatus } from "./ModelCard";
import ModelCard, { isLegacySource } from "./ModelCard";
import BrandLockup from "../icons/BrandLockup";
import { useModelStore } from "../../stores/modelStore";

interface OnboardingProps {
  onModelSelected: () => void;
  preview?: boolean;
}

const Onboarding = (props: OnboardingProps) => {
  const { t } = useTranslation();
  const modelStore = useModelStore();
  // Actions are stable functions and safe to take once; every reactive list
  // (models, downloadingModels, …) is read through the store proxy inside
  // memos and JSX below — a body-level read would snapshot the initial
  // empty state and the picker would never populate.
  const downloadModel = modelStore.downloadModel;
  const selectModel = modelStore.selectModel;
  const cancelDownload = modelStore.cancelDownload;
  const [selectedModelId, setSelectedModelId] = createSignal<string | null>(
    null,
  );
  const [showAll, setShowAll] = createSignal(false);
  let hasStartedSelection = false;

  const isBusy = () => selectedModelId() !== null;

  const curated = createMemo(() => {
    const downloadable = modelStore.models.filter(
      (m: ModelInfo) => !m.is_downloaded && isLegacySource(m),
    );
    const recommended = downloadable.filter((m: ModelInfo) => m.is_recommended);
    const rest = downloadable.filter((m: ModelInfo) => !m.is_recommended);
    return {
      downloadable,
      topPicks: recommended.slice(0, 2),
      otherRecommended: recommended.slice(2),
      rest,
    };
  });

  const hasRecommended = () =>
    curated().topPicks.length > 0 || curated().otherRecommended.length > 0;
  const showRest = () => showAll() || !hasRecommended();

  // Auto-select a model the moment its download lands. Everything the
  // decision reads is in the compute phase — it has to re-run when the
  // selection changes and when the model's download/verify state flips,
  // not once at mount when every store list is still empty.
  createEffect(
    () =>
      ({
        preview: props.preview ?? false,
        id: selectedModelId(),
        models: modelStore.models,
        downloading: modelStore.downloadingModels,
        verifying: modelStore.verifyingModels,
      }) as const,
    ({ preview, id, models, downloading, verifying }) => {
      if (preview) return;

      if (!id) {
        hasStartedSelection = false;
        return;
      }

      const model = models.find((m) => m.id === id);
      const stillDownloading = id in downloading;
      const stillVerifying = id in verifying;

      if (
        model?.is_downloaded &&
        !stillDownloading &&
        !stillVerifying &&
        !hasStartedSelection
      ) {
        hasStartedSelection = true;

        selectModel(id).then((success) => {
          if (success) {
            props.onModelSelected();
          } else {
            toast.error(t("onboarding.errors.selectModel"));
            hasStartedSelection = false;
            setSelectedModelId(null);
          }
        });
      }
    },
  );

  const handleDownloadModel = async (modelId: string) => {
    if (props.preview) return;

    setSelectedModelId(modelId);

    const success = await downloadModel(modelId);
    if (!success) {
      setSelectedModelId(null);
    }
  };

  const handleCancelDownload = async (modelId: string) => {
    if (props.preview) return;

    const success = await cancelDownload(modelId);
    if (success) {
      setSelectedModelId(null);
    }
  };

  const handleSelectExistingModel = (modelId: string) => {
    if (props.preview) return;

    setSelectedModelId(modelId);
  };

  const getModelStatus = (modelId: string): ModelCardStatus => {
    if (modelId in modelStore.verifyingModels) return "verifying";
    if (modelId in modelStore.downloadingModels) return "downloading";
    return "downloadable";
  };

  const getExistingModelStatus = (modelId: string): ModelCardStatus => {
    if (selectedModelId() === modelId) return "switching";
    return "available";
  };

  const getModelDownloadProgress = (modelId: string): number | undefined => {
    return modelStore.downloadProgress[modelId]?.percentage;
  };

  const getModelDownloadSpeed = (modelId: string): number | undefined => {
    return modelStore.downloadStats[modelId]?.speed;
  };

  return (
    <div class="h-screen w-full flex flex-col p-6 gap-4">
      <div class="flex flex-col items-center gap-2 shrink-0">
        <BrandLockup size={44} />
        <p class="text-text/70 max-w-md font-medium mx-auto">
          {t("onboarding.subtitle")}
        </p>
      </div>

      <div class="max-w-[600px] w-full mx-auto text-center flex-1 flex flex-col min-h-0">
        <div class="space-y-6 pb-6">
          {modelStore.models.some((m: ModelInfo) => m.is_downloaded) && (
            <div class="space-y-3">
              <div class="text-left">
                <h2 class="text-sm font-medium text-text/60">
                  {t("onboarding.existingModelsTitle")}
                </h2>
              </div>
              <For
                each={modelStore.models.filter(
                  (m: ModelInfo) => m.is_downloaded,
                )}
                keyed={(model) => model.id}
              >
                {(model) => (
                  <ModelCard
                    model={model()}
                    status={getExistingModelStatus(model().id)}
                    disabled={isBusy()}
                    onSelect={handleSelectExistingModel}
                    showRecommended={false}
                  />
                )}
              </For>
            </div>
          )}

          {curated().downloadable.length > 0 && (
            <div class="space-y-3">
              <div class="text-left">
                <h2 class="text-sm font-medium text-text/60">
                  {t("onboarding.downloadModelsTitle")}
                </h2>
              </div>

              <For each={curated().topPicks} keyed={(model) => model.id}>
                {(model) => (
                  <ModelCard
                    model={model()}
                    variant="featured"
                    status={getModelStatus(model().id)}
                    disabled={isBusy()}
                    onSelect={handleDownloadModel}
                    onDownload={handleDownloadModel}
                    onCancel={handleCancelDownload}
                    downloadProgress={getModelDownloadProgress(model().id)}
                    downloadSpeed={getModelDownloadSpeed(model().id)}
                    showRecommended={false}
                  />
                )}
              </For>

              <For
                each={curated().otherRecommended}
                keyed={(model) => model.id}
              >
                {(model) => (
                  <ModelCard
                    model={model()}
                    status={getModelStatus(model().id)}
                    disabled={isBusy()}
                    onSelect={handleDownloadModel}
                    onDownload={handleDownloadModel}
                    onCancel={handleCancelDownload}
                    downloadProgress={getModelDownloadProgress(model().id)}
                    downloadSpeed={getModelDownloadSpeed(model().id)}
                    showRecommended={false}
                  />
                )}
              </For>

              {hasRecommended() && curated().rest.length > 0 && (
                <button
                  type="button"
                  onClick={() => setShowAll((v) => !v)}
                  class="flex items-center justify-center gap-1.5 mx-auto py-1 text-sm font-medium text-text/60 hover:text-text transition-colors"
                >
                  {showAll()
                    ? t("onboarding.showFewerModels")
                    : t("onboarding.showAllModels", {
                        total: curated().downloadable.length,
                      })}
                  <ChevronDown
                    class={`w-4 h-4 transition-transform duration-200 ${
                      showAll() ? "rotate-180" : ""
                    }`}
                  />
                </button>
              )}

              {showRest() && (
                <For each={curated().rest} keyed={(model) => model.id}>
                  {(model) => (
                    <ModelCard
                      model={model()}
                      status={getModelStatus(model().id)}
                      disabled={isBusy()}
                      onSelect={handleDownloadModel}
                      onDownload={handleDownloadModel}
                      onCancel={handleCancelDownload}
                      downloadProgress={getModelDownloadProgress(model().id)}
                      downloadSpeed={getModelDownloadSpeed(model().id)}
                      showRecommended={false}
                    />
                  )}
                </For>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
};

export default Onboarding;
