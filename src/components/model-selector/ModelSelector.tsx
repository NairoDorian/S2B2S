import React, {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { Gauge, LoaderCircle, Zap } from "lucide-react";
import { commands } from "@/bindings";
import type { NativeStreamingLatencyPreset, QuantVariant } from "@/bindings";
import { getTranslatedModelName } from "../../lib/utils/modelTranslation";
import { useModelStore } from "../../stores/modelStore";
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
import { getQuantColor } from "./quantColors";
import { DEFAULT_TIMED_RUNS, useQuantBenchmark } from "./useQuantBenchmark";

import { ModelStateEvent } from "@/lib/types/events";

type ModelStatus =
  | "ready"
  | "loading"
  | "downloading"
  | "verifying"
  | "extracting"
  | "error"
  | "unloaded"
  | "none";

/** Which status-bar panel is expanded. Only ever one at a time. */
type StatusPanel = "model" | "quant" | "latency";

interface ModelSelectorProps {
  onError?: (error: string) => void;
}

const ModelSelector: React.FC<ModelSelectorProps> = ({ onError }) => {
  const { t } = useTranslation();
  const {
    models,
    currentModel,
    downloadProgress,
    downloadStats,
    verifyingModels,
    extractingModels,
    selectModel,
  } = useModelStore();

  const [modelStatus, setModelStatus] = useState<ModelStatus>("unloaded");
  const [modelError, setModelError] = useState<string | null>(null);
  const [openPanel, setOpenPanel] = useState<StatusPanel | null>(null);
  // Track pending model switch for optimistic display
  const [pendingModelId, setPendingModelId] = useState<string | null>(null);

  const barRef = useRef<HTMLDivElement>(null);

  const [quantVariants, setQuantVariants] = useState<QuantVariant[] | null>(
    null,
  );
  const [pendingDownloads, setPendingDownloads] = useState<Set<string>>(
    new Set(),
  );

  const benchmark = useQuantBenchmark();

  const settings = useSettingsStore((s) => s.settings);
  const setLatencyPreset = useSettingsStore((s) => s.setLatencyPreset);

  const displayModelId = pendingModelId || currentModel;

  const togglePanel = useCallback((panel: StatusPanel) => {
    setOpenPanel((current) => (current === panel ? null : panel));
  }, []);

  // Check model status when currentModel changes
  useEffect(() => {
    const checkStatus = async () => {
      if (currentModel) {
        try {
          const statusResult = await commands.getTranscriptionModelStatus();
          if (statusResult.status === "ok") {
            setModelStatus(
              statusResult.data === currentModel ? "ready" : "unloaded",
            );
          }
        } catch {
          setModelStatus("error");
          setModelError("Failed to check model status");
        }
      } else {
        setModelStatus("none");
      }
    };
    checkStatus();
  }, [currentModel]);

  useEffect(() => {
    // Listen for model loading lifecycle events
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

    // Auto-select model when download completes (fires after extraction too)
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
              if (!success) {
                setPendingModelId(null);
              }
            }
          } catch {
            // Ignore errors in auto-select
          }
        }, 500);
      },
    );

    return () => {
      modelStateUnlisten.then((fn) => fn());
      downloadCompleteUnlisten.then((fn) => fn());
    };
  }, [selectModel]);

  // Dismissal for every status-bar panel lives here, so the pills behave as one
  // mutually-exclusive group instead of three independent dropdowns.
  useEffect(() => {
    if (!openPanel) return;

    const handlePointerDown = (event: MouseEvent) => {
      if (barRef.current && !barRef.current.contains(event.target as Node)) {
        setOpenPanel(null);
      }
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
  }, [openPanel]);

  // Fetch quant variants for the current model when it changes
  useEffect(() => {
    const id = displayModelId;
    if (!id || !id.includes("/")) {
      setQuantVariants(null);
      return;
    }
    let cancelled = false;
    commands.getModelQuantVariants(id).then((result) => {
      if (!cancelled && result.status === "ok") {
        setQuantVariants(result.data ?? null);
      }
    });
    return () => {
      cancelled = true;
    };
  }, [displayModelId]);

  const currentModelInfo = models.find((m) => m.id === displayModelId);
  const latencyKind = currentModelInfo?.native_streaming_latency_kind;
  const currentPreset: NativeStreamingLatencyPreset =
    settings?.native_streaming_latency_presets?.[displayModelId ?? ""] ??
    DEFAULT_LATENCY_PRESET;

  const downloadPercentages = useMemo(() => {
    const map: Record<string, number> = {};
    for (const progress of Object.values(downloadProgress)) {
      map[progress.model_id] = progress.percentage;
    }
    return map;
  }, [downloadProgress]);

  const currentVariant = quantVariants?.find(
    (variant) => variant.model_id === displayModelId,
  );

  const downloadedVariantCount = useMemo(
    () =>
      quantVariants?.filter((variant) =>
        models.some((m) => m.id === variant.model_id && m.is_downloaded),
      ).length ?? 0,
    [quantVariants, models],
  );

  const measuredVariantCount = useMemo(
    () =>
      quantVariants?.filter(
        (variant) => benchmark.results[variant.model_id] !== undefined,
      ).length ?? 0,
    [quantVariants, benchmark.results],
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
    if (variant.model_id === displayModelId) return;
    // Keep the panel open: the check mark moving to the new row is the
    // confirmation, and comparing quants usually means several switches.
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
      if (result.status === "error") {
        console.error("Failed to download quant:", result.error);
        setPendingDownloads((prev) => {
          const next = new Set(prev);
          next.delete(variant.model_id);
          return next;
        });
      }
    });
  };

  const handleLatencyPresetSelect = (preset: NativeStreamingLatencyPreset) => {
    if (displayModelId) {
      void setLatencyPreset(displayModelId, preset);
    }
    setOpenPanel(null);
  };

  const getModelDisplayText = (): string => {
    const verifyingKeys = Object.keys(verifyingModels);
    if (verifyingKeys.length > 0) {
      if (verifyingKeys.length === 1) {
        const modelId = verifyingKeys[0];
        const model = models.find((m) => m.id === modelId);
        const modelName = model
          ? getTranslatedModelName(model, t)
          : t("modelSelector.verifyingGeneric").replace("...", "");
        return t("modelSelector.verifying", { modelName });
      } else {
        return t("modelSelector.verifyingGeneric");
      }
    }

    const extractingKeys = Object.keys(extractingModels);
    if (extractingKeys.length > 0) {
      if (extractingKeys.length === 1) {
        const modelId = extractingKeys[0];
        const model = models.find((m) => m.id === modelId);
        const modelName = model
          ? getTranslatedModelName(model, t)
          : t("modelSelector.extractingGeneric").replace("...", "");
        return t("modelSelector.extracting", { modelName });
      } else {
        return t("modelSelector.extractingMultiple", {
          count: extractingKeys.length,
        });
      }
    }

    const progressValues = Object.values(downloadProgress);
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

    switch (modelStatus) {
      case "ready":
        return currentModelInfo
          ? getTranslatedModelName(currentModelInfo, t)
          : t("modelSelector.modelReady");
      case "loading":
        return currentModelInfo
          ? t("modelSelector.loading", {
              modelName: getTranslatedModelName(currentModelInfo, t),
            })
          : t("modelSelector.loadingGeneric");
      case "extracting":
        return currentModelInfo
          ? t("modelSelector.extracting", {
              modelName: getTranslatedModelName(currentModelInfo, t),
            })
          : t("modelSelector.extractingGeneric");
      case "error":
        return modelError || t("modelSelector.modelError");
      case "unloaded":
        return currentModelInfo
          ? getTranslatedModelName(currentModelInfo, t)
          : t("modelSelector.modelUnloaded");
      case "none":
        return t("modelSelector.noModelDownloadRequired");
      default:
        return currentModelInfo
          ? getTranslatedModelName(currentModelInfo, t)
          : t("modelSelector.modelUnloaded");
    }
  };

  // Derive display status from model status + store state
  const getDisplayStatus = (): ModelStatus => {
    if (Object.keys(verifyingModels).length > 0) return "verifying";
    if (Object.keys(extractingModels).length > 0) return "extracting";
    if (Object.keys(downloadProgress).length > 0) return "downloading";
    return modelStatus;
  };

  const referenceRecording = benchmark.referenceRecording;
  const canBenchmark =
    !!displayModelId && !!referenceRecording && downloadedVariantCount > 0;

  const quantSubtitle = referenceRecording
    ? t("modelSelector.benchmark.reference", {
        when: new Date(
          (referenceRecording.timestamp ?? 0) * 1000,
        ).toLocaleString(undefined, {
          dateStyle: "short",
          timeStyle: "short",
        }),
      })
    : t("modelSelector.benchmark.noRecording");

  return (
    <div ref={barRef} className="flex min-w-0 flex-wrap items-center gap-2">
      {/* Model Status and Switcher */}
      <div className="relative shrink-0">
        <ModelStatusButton
          status={getDisplayStatus()}
          displayText={getModelDisplayText()}
          isDropdownOpen={openPanel === "model"}
          onClick={() => togglePanel("model")}
        />

        {openPanel === "model" && (
          <ModelDropdown
            models={models}
            currentModelId={displayModelId}
            onModelSelect={handleModelSelect}
          />
        )}
      </div>

      {/* Quantization picker — the variants of the current model, with their
          benchmark timings attached to the rows they describe. */}
      {quantVariants && quantVariants.length > 0 && (
        <StatusBarPopover
          open={openPanel === "quant"}
          onToggle={() => togglePanel("quant")}
          label={t("modelSelector.quantPicker.pillLabel", {
            quant:
              currentVariant?.quant ?? t("modelSelector.quantPicker.title"),
          })}
          title={t("modelSelector.quantPicker.title")}
          subtitle={quantSubtitle}
          trigger={
            <>
              {benchmark.isBusy ? (
                <LoaderCircle className="h-3 w-3 shrink-0 animate-spin text-text/50" />
              ) : (
                <span
                  className={`h-2 w-2 shrink-0 rounded-full ${getQuantColor(currentVariant?.quant ?? "")}`}
                />
              )}
              <span className="max-w-24 truncate">
                {currentVariant?.quant ?? t("modelSelector.quantPicker.title")}
              </span>
            </>
          }
          headerAction={
            <button
              type="button"
              onClick={() =>
                displayModelId && void benchmark.runAll(displayModelId)
              }
              disabled={!canBenchmark || benchmark.isBusy}
              title={t("modelSelector.benchmark.method", {
                runs: DEFAULT_TIMED_RUNS,
              })}
              className="flex shrink-0 items-center gap-1 rounded-md border border-mid-gray/25 bg-mid-gray/10 px-2 py-1 text-[11px] font-medium text-text/75 transition-colors hover:bg-mid-gray/20 hover:text-text/90 disabled:cursor-not-allowed disabled:opacity-40"
            >
              {benchmark.isRunningAll ? (
                <>
                  <LoaderCircle className="h-3 w-3 animate-spin" />
                  <span className="tabular-nums">
                    {t("modelSelector.benchmark.runProgress", {
                      done: measuredVariantCount,
                      total: downloadedVariantCount,
                    })}
                  </span>
                </>
              ) : (
                <>
                  <Zap className="h-3 w-3" />
                  <span>{t("modelSelector.benchmark.runAll")}</span>
                </>
              )}
            </button>
          }
        >
          <QuantizationPanel
            variants={quantVariants}
            models={models}
            currentModelId={displayModelId}
            downloadPercentages={downloadPercentages}
            pendingDownloads={pendingDownloads}
            benchmark={benchmark}
            onSelect={handleQuantSelect}
            onDownload={handleQuantDownload}
          />
          <div className="border-t border-mid-gray/20 px-3 py-1.5 text-[10px] leading-snug text-text/40">
            {t("modelSelector.benchmark.method", { runs: DEFAULT_TIMED_RUNS })}
          </div>
        </StatusBarPopover>
      )}

      {/* Native streaming latency — only for models that expose a configurable
          streaming latency extension (e.g. Nemotron, Parakeet Unified). */}
      {latencyKind && (
        <StatusBarPopover
          open={openPanel === "latency"}
          onToggle={() => togglePanel("latency")}
          label={t("modelSelector.latencySelector.pillLabel", {
            preset: t(latencyPresetLabelKey(currentPreset)),
          })}
          title={t("modelSelector.latencySelector.title")}
          widthClass="w-[min(17rem,calc(100vw-2rem))]"
          trigger={
            <>
              <Gauge className="h-3 w-3 shrink-0 text-text/50" />
              <span className="truncate">
                {t(latencyPresetLabelKey(currentPreset))}
              </span>
            </>
          }
        >
          <LatencyPanel
            selected={currentPreset}
            onSelect={handleLatencyPresetSelect}
          />
        </StatusBarPopover>
      )}

      {/* Download Progress Bar for Models */}
      <DownloadProgressDisplay
        downloadProgress={downloadProgress}
        downloadStats={downloadStats}
      />
    </div>
  );
};

export default ModelSelector;
