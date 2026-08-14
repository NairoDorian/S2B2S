import React, { useCallback, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { Slider } from "@/components/ui/Slider";
import { useModelStore } from "@/stores/modelStore";
import { useSettingsStore } from "@/stores/settingsStore";
import type { ModelInfo, NativeStreamingLatencyPreset } from "@/bindings";
import { commands } from "@/bindings";
import { toast } from "sonner";

const PRESETS: NativeStreamingLatencyPreset[] = [
  "fastest",
  "fast",
  "balanced",
  "accurate",
];

interface NativeStreamingLatencySelectorProps {
  modelId?: string;
  modelInfo?: ModelInfo;
  grouped?: boolean;
  descriptionMode?: "inline" | "tooltip";
}

export const NativeStreamingLatencySelector: React.FC<
  NativeStreamingLatencySelectorProps
> = ({
  modelId: propModelId,
  modelInfo: propModelInfo,
  grouped = true,
  descriptionMode = "tooltip",
}) => {
  const { t } = useTranslation();
  const { currentModel, models, downloadModel, isModelDownloading } =
    useModelStore();
  const { settings, refreshSettings } = useSettingsStore();

  const activeModelId = propModelId || currentModel;
  const activeModelInfo =
    propModelInfo || models.find((m: ModelInfo) => m.id === activeModelId);

  const supportsLatency = !!activeModelInfo?.native_streaming_latency_kind;

  const currentPreset: NativeStreamingLatencyPreset = useMemo(() => {
    if (!activeModelId || !settings?.native_streaming_latency_presets) {
      return "accurate";
    }
    return (
      (settings.native_streaming_latency_presets[
        activeModelId
      ] as NativeStreamingLatencyPreset) || "accurate"
    );
  }, [activeModelId, settings?.native_streaming_latency_presets]);

  const sliderIndex = useMemo(() => {
    const idx = PRESETS.indexOf(currentPreset);
    return idx === -1 ? 3 : idx;
  }, [currentPreset]);

  const formatPresetLabel = useCallback(
    (index: number) => {
      const preset = PRESETS[index] || "accurate";
      switch (preset) {
        case "fastest":
          return t(
            "modelSelector.nativeStreamingLatency.presets.fastest",
            "Fastest (~80–160ms)",
          );
        case "fast":
          return t(
            "modelSelector.nativeStreamingLatency.presets.fast",
            "Fast (~160–320ms)",
          );
        case "balanced":
          return t(
            "modelSelector.nativeStreamingLatency.presets.balanced",
            "Balanced (~560ms)",
          );
        case "accurate":
        default:
          return t(
            "modelSelector.nativeStreamingLatency.presets.accurate",
            "Accurate (Full)",
          );
      }
    },
    [t],
  );

  const handleLatencyChange = useCallback(
    async (index: number) => {
      if (!activeModelId) return;
      const targetPreset = PRESETS[index] || "accurate";
      await commands.changeNativeStreamingLatencyPresetSetting(
        activeModelId,
        targetPreset,
      );
      await refreshSettings();

      // Make sure the right model is actually available: if the selected
      // streaming model (e.g. Nemotron) isn't downloaded yet, download it now
      // so the new latency preset can take effect. It auto-selects on completion.
      if (
        activeModelInfo &&
        !activeModelInfo.is_downloaded &&
        !activeModelInfo.is_downloading &&
        !isModelDownloading(activeModelId)
      ) {
        toast.info(
          t(
            "modelSelector.nativeStreamingLatency.downloadToApply",
            "The model needs to be downloaded — downloading {{model}} now so the new latency preset can take effect.",
            { model: activeModelInfo.name },
          ),
        );
        await downloadModel(activeModelId);
      }
    },
    [
      activeModelId,
      activeModelInfo,
      refreshSettings,
      downloadModel,
      isModelDownloading,
      t,
    ],
  );

  if (!activeModelId || !activeModelInfo || !supportsLatency) {
    return null;
  }

  return (
    <div className="space-y-1">
      <Slider
        label={t(
          "modelSelector.nativeStreamingLatency.title",
          "Streaming Latency",
        )}
        description={t(
          "modelSelector.nativeStreamingLatency.description",
          "Choose how quickly this model responds. Faster modes reduce latency, while balanced and accurate modes give higher accuracy.",
        )}
        descriptionMode={descriptionMode}
        grouped={grouped}
        min={0}
        max={3}
        step={1}
        value={sliderIndex}
        onChange={(val) => {
          void handleLatencyChange(val);
        }}
        formatValue={formatPresetLabel}
      />
      <div className="px-3 text-[11px] leading-snug text-mid-gray/70">
        {t(
          "modelSelector.nativeStreamingLatency.appliesNextRecording",
          "Applies from the next recording.",
        )}
        {activeModelInfo.native_streaming_latency_kind ===
          "parakeet_buffered" &&
          sliderIndex <= 1 && (
            <span className="ml-1 text-amber-400">
              {t(
                "modelSelector.nativeStreamingLatency.cpuWarning",
                "This mode can lag on CPU.",
              )}
            </span>
          )}
      </div>
    </div>
  );
};
