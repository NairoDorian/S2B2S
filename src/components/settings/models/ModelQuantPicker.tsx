import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type ModelInfo, type QuantVariant } from "@/bindings";
import { useModelStore } from "@/stores/modelStore";

const formatSize = (mb: number): string =>
  mb >= 1024 ? `${(mb / 1024).toFixed(1)} GB` : `${mb} MB`;

/**
 * All quantizations the handy-computer catalog offers for a model family,
 * shown as one-click chips in the download menu: size per quant, download
 * state per quant, and a streaming-latency note when the family supports
 * configurable latency (Nemotron / Parakeet Unified).
 */
export const ModelQuantPicker: React.FC<{ model: ModelInfo }> = ({ model }) => {
  const { t } = useTranslation();
  const {
    models,
    downloadingModels,
    downloadProgress,
    downloadModelQuant,
    selectModel,
  } = useModelStore();
  const [variants, setVariants] = useState<QuantVariant[] | null>(null);

  useEffect(() => {
    let cancelled = false;
    commands
      .getModelQuantVariants(model.id)
      .then((res) => {
        if (!cancelled && res.status === "ok") {
          setVariants(res.data);
        }
      })
      .catch(() => {
        // Non-catalog models simply don't get the picker.
      });
    return () => {
      cancelled = true;
    };
  }, [model.id]);

  if (!variants || variants.length === 0) return null;

  const hasConfigurableLatency = !!model.native_streaming_latency_kind;

  return (
    <div className="mt-2 space-y-1.5">
      <div className="text-[11px] font-medium text-text/50">
        {t("settings.models.quants")}
      </div>
      <div className="flex flex-wrap gap-1.5">
        {variants.map((v) => {
          const info = models.find((m: ModelInfo) => m.id === v.modelId);
          const downloaded = info?.is_downloaded ?? false;
          const downloading = v.modelId in downloadingModels;
          const pct = downloadProgress[v.modelId]?.percentage;
          return (
            <button
              key={v.modelId}
              type="button"
              disabled={downloading}
              onClick={() => {
                if (downloaded) {
                  void selectModel(v.modelId);
                } else {
                  void downloadModelQuant(v.modelId);
                }
              }}
              title={
                downloaded
                  ? t("settings.models.quantSelect", { quant: v.quant })
                  : t("settings.models.quantDownload", {
                      quant: v.quant,
                      size: formatSize(v.sizeMb),
                    })
              }
              className={`px-2 py-1 rounded-md text-[11px] font-semibold border transition-colors ${
                downloaded
                  ? "bg-logo-primary/10 border-logo-primary/40 text-logo-primary"
                  : downloading
                    ? "bg-mid-gray/10 border-mid-gray/30 text-text/50 cursor-wait"
                    : "bg-mid-gray/5 border-mid-gray/30 text-text/70 hover:border-logo-primary/50 hover:text-logo-primary"
              }`}
            >
              {v.quant}
              {downloading && pct != null
                ? ` ${Math.round(pct)}%`
                : ` · ${formatSize(v.sizeMb)}`}
            </button>
          );
        })}
      </div>
      {hasConfigurableLatency && (
        <p className="text-[11px] text-amber-300/90">
          {t("settings.models.latencyConfigurable")}
        </p>
      )}
    </div>
  );
};
