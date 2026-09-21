import { createSignal, createEffect, createMemo, Show } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import { listen } from "@tauri-apps/api/event";
import { Gauge, LoaderCircle, Zap } from "@/components/icons/lucide";
import { commands } from "@/bindings";
import type { NativeStreamingLatencyPreset, QuantVariant } from "@/bindings";
import { getTranslatedModelName } from "../../lib/utils/modelTranslation";
import { useModelStore, selectModel } from "../../stores/modelStore";
import { useSettingsStore } from "@/stores/settingsStore";
import ModelStatusButton from "./ModelStatusButton";
import ModelDropdown from "./ModelDropdown";
import DownloadProgressDisplay from "./DownloadProgressDisplay";
import StatusBarPopover from "./StatusBarPopover";
import QuantizationPanel from "./QuantizationPanel";
import LatencyPanel, {
  DEFAULT_LATENCY_PRESET,
  latencyPresetLabelKey,
} from "./LatencyPanel";
import ChunkSizePanel, { R2T2_CHUNK_MS_DEFAULT } from "./ChunkSizePanel";
import { getQuantColor } from "./quantColors";
import { DEFAULT_TIMED_RUNS, useQuantBenchmark } from "./useQuantBenchmark";

import { ModelStateEvent } from "@/lib/types/events";
import type { JSX } from "@solidjs/web";

type ModelStatus =
  | "ready"
  | "loading"
  | "downloading"
  | "verifying"
  | "error"
  | "unloaded"
  | "none";

type StatusPanel = "model" | "quant" | "latency";

interface ModelSelectorProps {
  onError?: (error: string) => void;
}

const ModelSelector = (props: ModelSelectorProps): JSX.Element => {
  const { t } = useTranslation();
  const { onError } = props;
  const modelStore = useModelStore();
  const settingsStore = useSettingsStore();

  const [modelStatus, setModelStatus] = createSignal<ModelStatus>("unloaded");
  const [modelError, setModelError] = createSignal<string | null>(null);
  const [openPanel, setOpenPanel] = createSignal<StatusPanel | null>(null);
  const [pendingModelId, setPendingModelId] = createSignal<string | null>(null);
  let barRef: HTMLDivElement | null = null;
  const [quantVariants, setQuantVariants] = createSignal<QuantVariant[] | null>(
    null,
  );
  const [pendingDownloads, setPendingDownloads] = createSignal<Set<string>>(
    new Set(),
  );

  const benchmark = useQuantBenchmark();

  const displayModelId = createMemo(
    () => pendingModelId() || modelStore.currentModel,
  );

  const togglePanel = (panel: StatusPanel) => {
    setOpenPanel(openPanel() === panel ? null : panel);
  };

  // The compute tracks the store read; the apply only issues the command and
  // writes signals.
  createEffect(
    () => modelStore.currentModel,
    (current) => {
      if (!current) {
        setModelStatus("none");
        return;
      }
      const checkStatus = async () => {
        try {
          const statusResult = await commands.getTranscriptionModelStatus();
          if (statusResult.status === "ok") {
            setModelStatus(
              statusResult.data === current ? "ready" : "unloaded",
            );
          }
        } catch {
          setModelStatus("error");
          setModelError("Failed to check model status");
        }
      };
      checkStatus();
    },
  );

  createEffect(
    () => undefined,
    () => {
      const modelStateUnlisten = listen<ModelStateEvent>(
        "model-state-changed",
        (event) => {
          const { event_type, error } = event.payload;
          switch (event_type) {
            case "loading_started":
              setModelStatus("loading");
              setModelError(null);
              break;
            case "loading_completed":
              setModelStatus("ready");
              setModelError(null);
              setPendingModelId(null);
              break;
            case "loading_failed":
              setModelStatus("error");
              setModelError(error || "Failed to load model");
              setPendingModelId(null);
              break;
            case "unloaded":
              setModelStatus("unloaded");
              setModelError(null);
              break;
          }
        },
      );
      const downloadCompleteUnlisten = listen<string>(
        "model-download-complete",
        (event) => {
          const modelId = event.payload;
          setPendingDownloads((prev) => {
            if (!prev.has(modelId)) return prev;
            const next = new Set(prev);
            next.delete(modelId);
            return next;
          });
          setTimeout(async () => {
            try {
              const isRecording = await commands.isRecording();
              if (!isRecording) {
                setPendingModelId(modelId);
                setModelError(null);
                const success = await selectModel(modelId);
                if (!success) setPendingModelId(null);
              }
            } catch {}
          }, 500);
        },
      );
      return () => {
        modelStateUnlisten.then((fn) => fn());
        downloadCompleteUnlisten.then((fn) => fn());
      };
    },
  );

  createEffect(
    () => openPanel(),
    (panel) => {
      if (!panel) return;
      const handlePointerDown = (event: MouseEvent) => {
        if (barRef && !barRef.contains(event.target as Node))
          setOpenPanel(null);
      };
      const handleKeyDown = (event: KeyboardEvent) => {
        if (event.key === "Escape") setOpenPanel(null);
      };
      document.addEventListener("mousedown", handlePointerDown);
      document.addEventListener("keydown", handleKeyDown);
      return () => {
        document.removeEventListener("mousedown", handlePointerDown);
        document.removeEventListener("keydown", handleKeyDown);
      };
    },
  );

  createEffect(
    () => displayModelId(),
    (id) => {
      if (!id || !id.includes("/")) {
        setQuantVariants(null);
        return;
      }
      let cancelled = false;
      commands.getModelQuantVariants(id).then((result) => {
        if (cancelled) return;
        setQuantVariants(result.status === "ok" ? (result.data ?? null) : null);
      });
      return () => {
        cancelled = true;
      };
    },
  );

  // Memos, not body reads: the body runs once, so these would freeze at
  // whatever the store held on mount.
  const currentModelInfo = createMemo(() =>
    modelStore.models.find((m) => m.id === displayModelId()),
  );
  const latencyKind = () => currentModelInfo()?.native_streaming_latency_kind;
  const currentPreset = createMemo(
    (): NativeStreamingLatencyPreset =>
      settingsStore.settings?.native_streaming_latency_presets?.[
        displayModelId() ?? ""
      ] ?? DEFAULT_LATENCY_PRESET,
  );

  // R2T2 is the one family whose control is a continuous millisecond value
  // rather than a four-way preset, so it gets a numeric panel instead of the
  // radio group. Keyed off the kind string the backend serialises
  // (`NativeStreamingLatencyKind::R2T2ChunkMs`), not off the model id, so the
  // app never has to know the model's name to pick the right control.
  const isChunkMsKind = () => latencyKind() === "r2t2_chunk_ms";
  const currentChunkMs = createMemo(
    (): number =>
      settingsStore.settings?.native_streaming_chunk_ms?.[
        displayModelId() ?? ""
      ] ?? R2T2_CHUNK_MS_DEFAULT,
  );

  const downloadPercentages = createMemo(() => {
    const map: Record<string, number> = {};
    for (const progress of Object.values(modelStore.downloadProgress)) {
      map[progress.model_id] = progress.percentage;
    }
    return map;
  });

  const currentVariant = createMemo(() =>
    quantVariants()?.find((variant) => variant.model_id === displayModelId()),
  );

  const downloadedVariantCount = createMemo(
    () =>
      quantVariants()?.filter((variant) =>
        modelStore.models.some(
          (m) => m.id === variant.model_id && m.is_downloaded,
        ),
      ).length ?? 0,
  );

  const measuredVariantCount = createMemo(
    () =>
      quantVariants()?.filter(
        (variant) => benchmark.results()[variant.model_id] !== undefined,
      ).length ?? 0,
  );

  const handleModelSelect = async (modelId: string) => {
    setPendingModelId(modelId);
    setModelError(null);
    setOpenPanel(null);
    const success = await selectModel(modelId);
    if (!success) {
      setPendingModelId(null);
      setModelStatus("error");
      setModelError("Failed to switch model");
      onError?.("Failed to switch model");
    }
  };

  const handleQuantSelect = (variant: QuantVariant) => {
    if (variant.model_id === displayModelId()) return;
    setPendingModelId(variant.model_id);
    setModelError(null);
    void selectModel(variant.model_id).then((success) => {
      if (!success) {
        setPendingModelId(null);
        setModelStatus("error");
        setModelError("Failed to switch model");
        onError?.("Failed to switch model");
      }
    });
  };

  const handleQuantDownload = (variant: QuantVariant) => {
    setPendingDownloads((prev) => new Set(prev).add(variant.model_id));
    void commands.downloadModelQuant(variant.model_id).then((result) => {
      if (result.status === "error")
        console.error("Failed to download quant:", result.error);
      setPendingDownloads((prev) => {
        if (!prev.has(variant.model_id)) return prev;
        const next = new Set(prev);
        next.delete(variant.model_id);
        return next;
      });
    });
  };

  const handleLatencyPresetSelect = (preset: NativeStreamingLatencyPreset) => {
    if (displayModelId())
      void settingsStore.setLatencyPreset(displayModelId(), preset);
    setOpenPanel(null);
  };

  // Deliberately does NOT close the popover, unlike the preset handler: picking
  // a preset is a one-shot decision, whereas setting a chunk size is an
  // exploratory drag-and-nudge where closing the panel after every release
  // would fight the user. The value applies to the next stream either way.
  const handleChunkMsSelect = (chunkMs: number) => {
    if (displayModelId())
      void settingsStore.setLatencyChunkMs(displayModelId(), chunkMs);
  };

  const getModelDisplayText = (): string => {
    const verifyingKeys = Object.keys(modelStore.verifyingModels);
    if (verifyingKeys.length > 0) {
      if (verifyingKeys.length === 1) {
        const modelId = verifyingKeys[0];
        const model = modelStore.models.find((m) => m.id === modelId);
        const modelName = model
          ? getTranslatedModelName(model, t)
          : t("modelSelector.verifyingGeneric").replace("...", "");
        return t("modelSelector.verifying", { modelName });
      } else {
        return t("modelSelector.verifyingGeneric");
      }
    }
    const progressValues = Object.values(modelStore.downloadProgress);
    if (progressValues.length > 0) {
      if (progressValues.length === 1) {
        const progress = progressValues[0];
        const percentage = Math.max(
          0,
          Math.min(100, Math.round(progress.percentage)),
        );
        return t("modelSelector.downloading", { percentage });
      } else {
        return t("modelSelector.downloadingMultiple", {
          count: progressValues.length,
        });
      }
    }
    switch (modelStatus()) {
      case "ready": {
        const info = currentModelInfo();
        return info
          ? getTranslatedModelName(info, t)
          : t("modelSelector.modelReady");
      }
      case "loading": {
        const info = currentModelInfo();
        return info
          ? t("modelSelector.loading", {
              modelName: getTranslatedModelName(info, t),
            })
          : t("modelSelector.loadingGeneric");
      }
      case "error":
        return modelError() || t("modelSelector.modelError");
      case "unloaded": {
        const info = currentModelInfo();
        return info
          ? getTranslatedModelName(info, t)
          : t("modelSelector.modelUnloaded");
      }
      case "none":
        return t("modelSelector.noModelDownloadRequired");
      default: {
        const info = currentModelInfo();
        return info
          ? getTranslatedModelName(info, t)
          : t("modelSelector.modelUnloaded");
      }
    }
  };

  const getDisplayStatus = (): ModelStatus => {
    if (Object.keys(modelStore.verifyingModels).length > 0) return "verifying";
    if (Object.keys(modelStore.downloadProgress).length > 0)
      return "downloading";
    return modelStatus();
  };

  const referenceRecording = () => benchmark.referenceRecording();
  const canBenchmark = () =>
    !!displayModelId() &&
    !!referenceRecording() &&
    downloadedVariantCount() > 0;

  const quantSubtitle = () => {
    const recording = referenceRecording();
    return recording
      ? t("modelSelector.benchmark.reference", {
          when: new Date((recording.timestamp ?? 0) * 1000).toLocaleString(
            undefined,
            { dateStyle: "short", timeStyle: "short" },
          ),
        })
      : t("modelSelector.benchmark.noRecording");
  };

  return (
    <div
      ref={(el) => {
        barRef = el;
      }}
      class="flex min-w-0 flex-nowrap items-center gap-2"
    >
      <div class="relative shrink-0">
        <ModelStatusButton
          status={getDisplayStatus()}
          displayText={getModelDisplayText()}
          isDropdownOpen={openPanel() === "model"}
          onClick={() => togglePanel("model")}
        />
        <Show when={openPanel() === "model"}>
          <ModelDropdown
            models={modelStore.models}
            currentModelId={displayModelId()}
            onModelSelect={handleModelSelect}
          />
        </Show>
      </div>
      <Show when={quantVariants() && quantVariants()!.length > 0}>
        <StatusBarPopover
          open={openPanel() === "quant"}
          onToggle={() => togglePanel("quant")}
          label={t("modelSelector.quantPicker.pillLabel", {
            quant:
              currentVariant()?.quant ?? t("modelSelector.quantPicker.title"),
          })}
          title={t("modelSelector.quantPicker.title")}
          subtitle={quantSubtitle()}
          trigger={
            <>
              {benchmark.isBusy() ? (
                <LoaderCircle class="h-3 w-3 shrink-0 animate-spin text-text/50" />
              ) : (
                <span
                  class={`h-2 w-2 shrink-0 rounded-full ${getQuantColor(currentVariant()?.quant ?? "")}`}
                />
              )}
              <span class="max-w-20 truncate">
                {currentVariant()?.quant ??
                  t("modelSelector.quantPicker.title")}
              </span>
            </>
          }
          headerAction={
            <button
              type="button"
              onClick={() =>
                displayModelId() && void benchmark.runAll(displayModelId())
              }
              disabled={!canBenchmark() || benchmark.isBusy()}
              title={t("modelSelector.benchmark.method", {
                runs: DEFAULT_TIMED_RUNS,
              })}
              class="flex shrink-0 items-center gap-1 rounded-md border border-mid-gray/25 bg-mid-gray/10 px-2 py-1 text-[11px] font-medium text-text/75 transition-colors hover:bg-mid-gray/20 hover:text-text/90 disabled:cursor-not-allowed disabled:opacity-40"
            >
              {benchmark.isRunningAll() ? (
                <>
                  <LoaderCircle class="h-3 w-3 animate-spin" />
                  <span class="tabular-nums">
                    {t("modelSelector.benchmark.runProgress", {
                      done: measuredVariantCount(),
                      total: downloadedVariantCount(),
                    })}
                  </span>
                </>
              ) : (
                <>
                  <Zap class="h-3 w-3" />
                  <span>{t("modelSelector.benchmark.runAll")}</span>
                </>
              )}
            </button>
          }
        >
          <QuantizationPanel
            variants={quantVariants() ?? []}
            models={modelStore.models}
            currentModelId={displayModelId()}
            downloadPercentages={downloadPercentages()}
            pendingDownloads={pendingDownloads()}
            benchmark={benchmark}
            onSelect={handleQuantSelect}
            onDownload={handleQuantDownload}
          />
          <div class="border-t border-mid-gray/20 px-3 py-1.5 text-[10px] leading-snug text-text/40">
            {t("modelSelector.benchmark.method", { runs: DEFAULT_TIMED_RUNS })}
          </div>
        </StatusBarPopover>
      </Show>
      <Show when={latencyKind}>
        <StatusBarPopover
          open={openPanel() === "latency"}
          onToggle={() => togglePanel("latency")}
          label={
            isChunkMsKind()
              ? t("modelSelector.latencySelector.chunkSize.pillLabel", {
                  ms: currentChunkMs(),
                })
              : t("modelSelector.latencySelector.pillLabel", {
                  preset: t(latencyPresetLabelKey(currentPreset())),
                })
          }
          title={
            isChunkMsKind()
              ? t("modelSelector.latencySelector.chunkSize.label")
              : t("modelSelector.latencySelector.title")
          }
          widthClass="w-[min(17rem,calc(100vw-2rem))]"
          trigger={
            <>
              <Gauge class="h-3 w-3 shrink-0 text-text/50" />
              <span class="max-w-24 truncate">
                {isChunkMsKind()
                  ? t("modelSelector.latencySelector.chunkSize.trigger", {
                      ms: currentChunkMs(),
                    })
                  : t(latencyPresetLabelKey(currentPreset()))}
              </span>
            </>
          }
        >
          {/* Two controls, one popover: R2T2 takes the numeric chunk size, every
              other streaming family keeps the preset radio group untouched. */}
          <Show
            when={isChunkMsKind()}
            fallback={
              <LatencyPanel
                selected={currentPreset()}
                onSelect={handleLatencyPresetSelect}
              />
            }
          >
            <ChunkSizePanel
              selected={currentChunkMs()}
              onSelect={handleChunkMsSelect}
            />
          </Show>
        </StatusBarPopover>
      </Show>
      <DownloadProgressDisplay
        downloadProgress={modelStore.downloadProgress}
        downloadStats={modelStore.downloadStats}
      />
    </div>
  );
};

export default ModelSelector;
