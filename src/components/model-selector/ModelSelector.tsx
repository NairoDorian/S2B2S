import React, { useState, useRef, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { commands } from "@/bindings";
import type { QuantVariant, NativeStreamingLatencyPreset } from "@/bindings";
import { getTranslatedModelName } from "../../lib/utils/modelTranslation";
import { useModelStore } from "../../stores/modelStore";
import { useSettingsStore } from "@/stores/settingsStore";
import ModelStatusButton from "./ModelStatusButton";
import ModelDropdown from "./ModelDropdown";
import DownloadProgressDisplay from "./DownloadProgressDisplay";

import { ModelStateEvent } from "@/lib/types/events";

const quantColorMap: Record<string, string> = {
  Q2_K: "bg-red-500",
  Q3_K_S: "bg-red-500",
  Q3_K_M: "bg-red-400",
  Q3_K_L: "bg-red-400",
  Q4_0: "bg-green-500",
  Q4_K_M: "bg-green-500",
  Q4_K_S: "bg-green-600",
  Q5_0: "bg-blue-500",
  Q5_K_M: "bg-blue-500",
  Q5_K_S: "bg-blue-600",
  Q6_K: "bg-purple-500",
  Q8_0: "bg-yellow-500",
  F16: "bg-orange-400",
  F32: "bg-teal-400",
  BF16: "bg-cyan-400",
};

function getQuantColor(quant: string): string {
  return quantColorMap[quant] || "bg-gray-500";
}

const LATENCY_PRESET_ORDER: NativeStreamingLatencyPreset[] = [
  "fastest",
  "fast",
  "balanced",
  "accurate",
];

const LATENCY_PRESET_LABELS: Record<NativeStreamingLatencyPreset, string> = {
  fastest: "modelSelector.latencySelector.fastest",
  fast: "modelSelector.latencySelector.fast",
  balanced: "modelSelector.latencySelector.balanced",
  accurate: "modelSelector.latencySelector.accurate",
};

const LATENCY_PRESET_DESCRIPTIONS: Record<
  NativeStreamingLatencyPreset,
  string
> = {
  fastest: "modelSelector.latencySelector.descriptions.fastest",
  fast: "modelSelector.latencySelector.descriptions.fast",
  balanced: "modelSelector.latencySelector.descriptions.balanced",
  accurate: "modelSelector.latencySelector.descriptions.accurate",
};

type ModelStatus =
  | "ready"
  | "loading"
  | "downloading"
  | "verifying"
  | "extracting"
  | "error"
  | "unloaded"
  | "none";

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
  const [showModelDropdown, setShowModelDropdown] = useState(false);
  // Track pending model switch for optimistic display
  const [pendingModelId, setPendingModelId] = useState<string | null>(null);

  const dropdownRef = useRef<HTMLDivElement>(null);

  const [quantVariants, setQuantVariants] = useState<QuantVariant[] | null>(
    null,
  );
  const [quantDownloading, setQuantDownloading] = useState<Set<string>>(
    new Set(),
  );

  const settings = useSettingsStore((s) => s.settings);
  const setLatencyPreset = useSettingsStore((s) => s.setLatencyPreset);

  const displayModelId = pendingModelId || currentModel;

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
        setTimeout(async () => {
          try {
            const isRecording = await commands.isRecording();
            if (!isRecording) {
              setPendingModelId(modelId);
              setModelError(null);
              setShowModelDropdown(false);
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

    // Click outside to close dropdown
    const handleClickOutside = (event: MouseEvent) => {
      if (
        dropdownRef.current &&
        !dropdownRef.current.contains(event.target as Node)
      ) {
        setShowModelDropdown(false);
      }
    };

    document.addEventListener("mousedown", handleClickOutside);

    return () => {
      document.removeEventListener("mousedown", handleClickOutside);
      modelStateUnlisten.then((fn) => fn());
      downloadCompleteUnlisten.then((fn) => fn());
    };
  }, [selectModel]);

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
  const currentPreset =
    settings?.native_streaming_latency_presets?.[displayModelId ?? ""] ??
    ("accurate" as NativeStreamingLatencyPreset);

  const handleQuantClick = (variant: QuantVariant) => {
    if (variant.model_id === displayModelId) return;
    const downloaded = models.some(
      (m) => m.id === variant.model_id && m.is_downloaded,
    );
    if (downloaded) {
      void handleModelSelect(variant.model_id);
    } else {
      setQuantDownloading((prev) => new Set(prev).add(variant.model_id));
      void commands.downloadModelQuant(variant.model_id).then((result) => {
        setQuantDownloading((prev) => {
          const next = new Set(prev);
          next.delete(variant.model_id);
          return next;
        });
        if (result.status === "error") {
          console.error("Failed to download quant:", result.error);
        }
      });
    }
  };

  const handleLatencyPresetClick = (preset: NativeStreamingLatencyPreset) => {
    if (displayModelId) {
      void setLatencyPreset(displayModelId, preset);
    }
  };

  const handleModelSelect = async (modelId: string) => {
    setPendingModelId(modelId);
    setModelError(null);
    setShowModelDropdown(false);
    const success = await selectModel(modelId);
    if (!success) {
      setPendingModelId(null);
      setModelStatus("error");
      setModelError("Failed to switch model");
      onError?.("Failed to switch model");
    }
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

    const currentModelInfo = models.find((m) => m.id === displayModelId);

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

  return (
    <>
      {/* Model Status and Switcher */}
      <div className="relative" ref={dropdownRef}>
        <ModelStatusButton
          status={getDisplayStatus()}
          displayText={getModelDisplayText()}
          isDropdownOpen={showModelDropdown}
          onClick={() => setShowModelDropdown(!showModelDropdown)}
        />

        {/* Model Dropdown */}
        {showModelDropdown && (
          <ModelDropdown
            models={models}
            currentModelId={displayModelId}
            onModelSelect={handleModelSelect}
          />
        )}
      </div>

      {/* Quantization Picker — shows variants for the current model */}
      {quantVariants && quantVariants.length > 0 && (
        <div className="mt-1">
          <div className="text-xs font-medium text-text/60 mb-1">
            {t("modelSelector.quantPicker.title")}
          </div>
          <div className="flex flex-wrap gap-1">
            {quantVariants.map((variant) => {
              const isCurrent = variant.model_id === displayModelId;
              const isDownloaded = models.some(
                (m) => m.id === variant.model_id && m.is_downloaded,
              );
              const isDownloading = quantDownloading.has(variant.model_id);
              const isSelected = isCurrent;
              const chipBase =
                "inline-flex items-center gap-1 px-2 py-1 rounded text-xs";
              const chipClass = isSelected
                ? "bg-logo-primary/10 border border-logo-primary/30 text-logo-primary font-medium"
                : isDownloading
                  ? "bg-mid-gray/5 border border-mid-gray/20 text-text/50 cursor-wait"
                  : isDownloaded
                    ? "bg-mid-gray/5 border border-mid-gray/20 text-text/80 hover:bg-mid-gray/10 cursor-pointer"
                    : "bg-mid-gray/5 border border-mid-gray/20 text-text/80 hover:bg-mid-gray/10 cursor-pointer";
              return (
                <button
                  key={variant.model_id}
                  onClick={() => handleQuantClick(variant)}
                  disabled={isSelected || isDownloading}
                  className={chipBase + " " + chipClass}
                >
                  <span
                    className={`w-2 h-2 rounded-full flex-shrink-0 ${getQuantColor(variant.quant)}`}
                  />
                  <span>{variant.quant}</span>
                  {variant.is_default && (
                    <span className="text-text/40">·</span>
                  )}
                  <span className="text-text/40">{variant.size_mb} MB</span>
                  {isCurrent && (
                    <svg
                      className="w-3 h-3 text-logo-primary"
                      fill="currentColor"
                      viewBox="0 0 20 20"
                    >
                      <path
                        fillRule="evenodd"
                        d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L7 11.586l7.293-7.293a1 1 0 011.414 0z"
                        clipRule="evenodd"
                      />
                    </svg>
                  )}
                  {!isCurrent && !isDownloaded && !isDownloading && (
                    <span>↓</span>
                  )}
                </button>
              );
            })}
          </div>
        </div>
      )}

      {/* Native Streaming Latency Selector — shown for models that support
          configurable streaming latency (e.g. Nemotron, Parakeet Unified) */}
      {latencyKind && (
        <div className="mt-2">
          <div className="text-xs font-medium text-text/60 mb-1">
            {t("modelSelector.latencySelector.title")}
          </div>
          <div className="flex gap-1 text-xs">
            {LATENCY_PRESET_ORDER.map((preset) => {
              const isSelected = currentPreset === preset;
              return (
                <button
                  key={preset}
                  onClick={() => handleLatencyPresetClick(preset)}
                  className={
                    isSelected
                      ? "flex-1 px-2 py-1 bg-logo-primary/10 border border-logo-primary/30 text-logo-primary font-medium rounded"
                      : "flex-1 px-2 py-1 bg-mid-gray/5 border border-mid-gray/20 text-text/80 hover:bg-mid-gray/10 rounded"
                  }
                >
                  {t(LATENCY_PRESET_LABELS[preset])}
                </button>
              );
            })}
          </div>
          {currentPreset !== "accurate" && (
            <div className="text-xs text-text/40 mt-1">
              {t(LATENCY_PRESET_DESCRIPTIONS[currentPreset])}
            </div>
          )}
        </div>
      )}

      {/* Download Progress Bar for Models */}
      <DownloadProgressDisplay
        downloadProgress={downloadProgress}
        downloadStats={downloadStats}
      />
    </>
  );
};

export default ModelSelector;
