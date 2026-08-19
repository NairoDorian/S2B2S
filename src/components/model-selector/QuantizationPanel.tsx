import React, { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { Check, Download, LoaderCircle, Play } from "lucide-react";
import type { ModelInfo, QuantVariant } from "@/bindings";
import { formatModelSize } from "@/lib/utils/format";
import { getQuantColor } from "./quantColors";
import type { QuantBenchmark } from "./useQuantBenchmark";

interface QuantizationPanelProps {
  variants: QuantVariant[];
  models: ModelInfo[];
  currentModelId: string;
  /** Live download percentage per model id, from the model store. */
  downloadPercentages: Record<string, number>;
  /** Downloads clicked here that have not reported progress yet. */
  pendingDownloads: Set<string>;
  benchmark: QuantBenchmark;
  onSelect: (variant: QuantVariant) => void;
  onDownload: (variant: QuantVariant) => void;
}

/**
 * The quantization picker, shown from the status bar.
 *
 * One row per catalog variant, in catalog order — rows never reorder while you
 * are aiming at one. Benchmark timings attach to the row they belong to so
 * "which quant should I run?" is answered in the same place the choice is made.
 */
export const QuantizationPanel: React.FC<QuantizationPanelProps> = ({
  variants,
  models,
  currentModelId,
  downloadPercentages,
  pendingDownloads,
  benchmark,
  onSelect,
  onDownload,
}) => {
  const { t } = useTranslation();

  const downloadedIds = useMemo(
    () =>
      new Set(
        models.filter((model) => model.is_downloaded).map((model) => model.id),
      ),
    [models],
  );

  // Timings for this family only — results are keyed by variant id, so another
  // family's numbers can never leak into this list.
  const familyResults = useMemo(
    () =>
      variants
        .map((variant) => ({
          id: variant.model_id,
          sample: benchmark.results[variant.model_id],
        }))
        .filter((entry) => entry.sample !== undefined),
    [variants, benchmark.results],
  );

  const fastestMs = familyResults.length
    ? Math.min(...familyResults.map((entry) => entry.sample!.avgMs))
    : null;
  const fastestId =
    familyResults.length > 1
      ? (familyResults.find((entry) => entry.sample!.avgMs === fastestMs)?.id ??
        null)
      : null;

  const formatDuration = (ms: number): string =>
    ms < 1000
      ? t("modelSelector.benchmark.milliseconds", { value: Math.round(ms) })
      : t("modelSelector.benchmark.seconds", {
          value: (ms / 1000).toFixed(2),
        });

  return (
    <ul className="max-h-[min(50vh,17rem)] overflow-y-auto py-1">
      {variants.map((variant) => {
        const id = variant.model_id;
        const isCurrent = id === currentModelId;
        const isDownloaded = downloadedIds.has(id);
        const percentage = downloadPercentages[id];
        const isDownloading =
          percentage !== undefined || pendingDownloads.has(id);
        const sample = benchmark.results[id];
        const failure = benchmark.errors[id];
        const isMeasuring = benchmark.activeModelId === id;
        const isFastest = id === fastestId;

        const rowLabel = isCurrent
          ? t("modelSelector.quantPicker.current", { quant: variant.quant })
          : isDownloaded
            ? t("modelSelector.quantPicker.switchTo", { quant: variant.quant })
            : t("modelSelector.quantPicker.download", { quant: variant.quant });

        // Speed bar: relative to the fastest measured variant, so a longer bar
        // always means "better" rather than "took longer".
        const speedRatio =
          sample && fastestMs ? fastestMs / Math.max(sample.avgMs, 1) : 0;
        const realTime =
          sample?.audioSecs != null && sample.avgMs > 0
            ? sample.audioSecs / (sample.avgMs / 1000)
            : null;

        return (
          <li key={id}>
            <div
              className={`mx-1 rounded-md transition-colors ${
                isCurrent ? "bg-logo-primary/10" : "hover:bg-mid-gray/10"
              }`}
            >
              <div className="flex items-center gap-1">
                <button
                  type="button"
                  onClick={() =>
                    isDownloaded ? onSelect(variant) : onDownload(variant)
                  }
                  disabled={isCurrent || isDownloading}
                  title={rowLabel}
                  aria-label={rowLabel}
                  className="flex min-w-0 flex-1 items-center gap-2 px-2 py-1.5 text-start disabled:cursor-default"
                >
                  <span
                    className={`h-2 w-2 shrink-0 rounded-full ${getQuantColor(variant.quant)}`}
                  />
                  <span
                    className={`truncate font-medium ${isCurrent ? "text-logo-primary" : "text-text/85"}`}
                  >
                    {variant.quant}
                  </span>
                  {isCurrent && (
                    <Check className="h-3 w-3 shrink-0 text-logo-primary" />
                  )}
                  {isFastest && (
                    <span className="shrink-0 rounded-sm bg-emerald-500/15 px-1 text-[10px] font-medium text-emerald-600 dark:text-emerald-400">
                      {t("modelSelector.benchmark.fastest")}
                    </span>
                  )}
                  {variant.is_default && !isFastest && (
                    <span className="shrink-0 text-[10px] uppercase tracking-wide text-text/35">
                      {t("modelSelector.quantPicker.default")}
                    </span>
                  )}
                  <span className="ms-auto shrink-0 tabular-nums text-text/45">
                    {formatModelSize(variant.size_mb)}
                  </span>
                </button>

                {/* One trailing slot for every row — always the same width, so
                    the size column lines up whatever state a row is in. */}
                {isDownloaded ? (
                  <button
                    type="button"
                    onClick={() => void benchmark.runOne(id)}
                    disabled={benchmark.isBusy || !benchmark.referenceRecording}
                    title={t("modelSelector.benchmark.runOne", {
                      quant: variant.quant,
                    })}
                    aria-label={t("modelSelector.benchmark.runOne", {
                      quant: variant.quant,
                    })}
                    className="me-1 flex h-6 w-16 shrink-0 items-center justify-end gap-1 rounded px-1 text-[11px] transition-colors hover:bg-mid-gray/20 disabled:cursor-not-allowed disabled:opacity-40"
                  >
                    {isMeasuring ? (
                      <>
                        <LoaderCircle className="h-3 w-3 animate-spin text-text/50" />
                        <span className="tabular-nums text-text/50">
                          {t("modelSelector.benchmark.runProgress", {
                            done: benchmark.activeRun?.index ?? 0,
                            total: benchmark.activeRun?.total ?? 0,
                          })}
                        </span>
                      </>
                    ) : failure ? (
                      <span className="truncate text-error" title={failure}>
                        {t("modelSelector.benchmark.failed")}
                      </span>
                    ) : sample ? (
                      <span className="font-mono tabular-nums text-text/75">
                        {formatDuration(sample.avgMs)}
                      </span>
                    ) : (
                      <Play className="h-3 w-3 text-text/40" />
                    )}
                  </button>
                ) : (
                  <button
                    type="button"
                    onClick={() => onDownload(variant)}
                    disabled={isDownloading}
                    title={rowLabel}
                    aria-label={rowLabel}
                    className="me-1 flex h-6 w-16 shrink-0 items-center justify-end gap-1 rounded px-1 text-[11px] transition-colors hover:bg-mid-gray/20 disabled:cursor-default"
                  >
                    {isDownloading ? (
                      <span className="tabular-nums text-text/50">
                        {t("modelSelector.quantPicker.downloadingPercent", {
                          percentage: Math.round(percentage ?? 0),
                        })}
                      </span>
                    ) : (
                      <Download className="h-3 w-3 text-text/40" />
                    )}
                  </button>
                )}
              </div>

              {(sample || isDownloading) && (
                <div className="flex items-center gap-2 px-2 pb-1.5">
                  <div className="h-1 min-w-0 flex-1 overflow-hidden rounded-full bg-mid-gray/20">
                    <div
                      className={`h-full rounded-full transition-[width] duration-300 ${
                        isDownloading
                          ? "bg-logo-primary/60"
                          : isFastest
                            ? "bg-emerald-500"
                            : "bg-logo-primary/60"
                      }`}
                      style={{
                        width: `${Math.max(4, Math.min(100, isDownloading ? (percentage ?? 0) : speedRatio * 100))}%`,
                      }}
                    />
                  </div>
                  {realTime !== null && !isDownloading && (
                    <span className="shrink-0 text-[10px] tabular-nums text-text/45">
                      {t("modelSelector.benchmark.realTime", {
                        factor: realTime.toFixed(1),
                      })}
                    </span>
                  )}
                </div>
              )}
            </div>
          </li>
        );
      })}
    </ul>
  );
};

export default QuantizationPanel;
