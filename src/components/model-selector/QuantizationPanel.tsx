import { createMemo, For } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import { Check, Download, LoaderCircle, Play } from "@/components/icons/lucide";
import type { ModelInfo, QuantVariant } from "@/bindings";
import { formatModelSize } from "@/lib/utils/format";
import { getQuantColor } from "./quantColors";
import type { QuantBenchmark } from "./useQuantBenchmark";
import type { JSX } from "@solidjs/web";

interface QuantizationPanelProps {
  variants: QuantVariant[];
  models: ModelInfo[];
  currentModelId: string;
  downloadPercentages: Record<string, number>;
  pendingDownloads: Set<string>;
  benchmark: QuantBenchmark;
  onSelect: (variant: QuantVariant) => void;
  onDownload: (variant: QuantVariant) => void;
}

export const QuantizationPanel = (
  props: QuantizationPanelProps,
): JSX.Element => {
  const {
    variants,
    models,
    currentModelId,
    downloadPercentages,
    pendingDownloads,
    benchmark,
    onSelect,
    onDownload,
  } = props;
  const { t } = useTranslation();

  const downloadedIds = createMemo(
    () =>
      new Set(
        models.filter((model) => model.is_downloaded).map((model) => model.id),
      ),
  );

  const familyResults = createMemo(() =>
    variants
      .map((variant) => ({
        id: variant.model_id,
        sample: benchmark.results()[variant.model_id],
      }))
      .filter((entry) => entry.sample !== undefined),
  );

  const fastestMs = familyResults().length
    ? Math.min(...familyResults().map((entry) => entry.sample!.avgMs))
    : null;
  const fastestId =
    familyResults().length > 1
      ? (familyResults().find((entry) => entry.sample!.avgMs === fastestMs)
          ?.id ?? null)
      : null;

  const formatDuration = (ms: number): string =>
    ms < 1000
      ? t("modelSelector.benchmark.milliseconds", { value: Math.round(ms) })
      : t("modelSelector.benchmark.seconds", { value: (ms / 1000).toFixed(2) });

  return (
    <ul class="max-h-[min(50vh,17rem)] overflow-y-auto py-1">
      <For each={variants}>
        {(variant) => {
          const id = variant.model_id;
          const isCurrent = id === currentModelId;
          const isDownloaded = downloadedIds().has(id);
          const percentage = downloadPercentages[id];
          const isDownloading =
            percentage !== undefined || pendingDownloads.has(id);
          const sample = benchmark.results()[id];
          const failure = benchmark.errors()[id];
          const isMeasuring = benchmark.activeModelId() === id;
          const isFastest = id === fastestId;

          const rowLabel = isCurrent
            ? t("modelSelector.quantPicker.current", { quant: variant.quant })
            : isDownloaded
              ? t("modelSelector.quantPicker.switchTo", {
                  quant: variant.quant,
                })
              : t("modelSelector.quantPicker.download", {
                  quant: variant.quant,
                });

          const speedRatio =
            sample && fastestMs ? fastestMs / Math.max(sample.avgMs, 1) : 0;
          const realTime =
            sample?.audioSecs != null && sample.avgMs > 0
              ? sample.audioSecs / (sample.avgMs / 1000)
              : null;

          return (
            <li>
              <div
                class={`mx-1 rounded-md transition-colors ${isCurrent ? "bg-accent/10" : "hover:bg-mid-gray/10"}`}
              >
                <div class="flex items-center gap-1">
                  <button
                    type="button"
                    onClick={() =>
                      isDownloaded ? onSelect(variant) : onDownload(variant)
                    }
                    disabled={isCurrent || isDownloading}
                    title={rowLabel}
                    aria-label={rowLabel}
                    class="flex min-w-0 flex-1 items-center gap-2 px-2 py-1.5 text-start disabled:cursor-default"
                  >
                    <span
                      class={`h-2 w-2 shrink-0 rounded-full ${getQuantColor(variant.quant)}`}
                    />
                    <span
                      class={`truncate font-medium ${isCurrent ? "text-accent" : "text-text/85"}`}
                    >
                      {variant.quant}
                    </span>
                    {isCurrent && (
                      <Check class="h-3 w-3 shrink-0 text-accent" />
                    )}
                    {isFastest && (
                      <span class="shrink-0 rounded-sm bg-emerald-500/15 px-1 text-[10px] font-medium text-emerald-600 dark:text-emerald-400">
                        {t("modelSelector.benchmark.fastest")}
                      </span>
                    )}
                    {variant.is_default && !isFastest && (
                      <span class="shrink-0 text-[10px] uppercase tracking-wide text-text/35">
                        {t("modelSelector.quantPicker.default")}
                      </span>
                    )}
                    <span class="ms-auto shrink-0 tabular-nums text-text/45">
                      {formatModelSize(variant.size_mb)}
                    </span>
                  </button>
                  {isDownloaded ? (
                    <button
                      type="button"
                      onClick={() => void benchmark.runOne(id)}
                      disabled={
                        benchmark.isBusy || !benchmark.referenceRecording()
                      }
                      title={t("modelSelector.benchmark.runOne", {
                        quant: variant.quant,
                      })}
                      aria-label={t("modelSelector.benchmark.runOne", {
                        quant: variant.quant,
                      })}
                      class="me-1 flex h-6 w-16 shrink-0 items-center justify-end gap-1 rounded px-1 text-[11px] transition-colors hover:bg-mid-gray/20 disabled:cursor-not-allowed disabled:opacity-40"
                    >
                      {isMeasuring ? (
                        <>
                          <LoaderCircle class="h-3 w-3 animate-spin text-text/50" />
                          <span class="tabular-nums text-text/50">
                            {t("modelSelector.benchmark.runProgress", {
                              done: benchmark.activeRun()?.index ?? 0,
                              total: benchmark.activeRun()?.total ?? 0,
                            })}
                          </span>
                        </>
                      ) : failure ? (
                        <span class="truncate text-error" title={failure}>
                          {t("modelSelector.benchmark.failed")}
                        </span>
                      ) : sample ? (
                        <span class="font-mono tabular-nums text-text/75">
                          {formatDuration(sample.avgMs)}
                        </span>
                      ) : (
                        <Play class="h-3 w-3 text-text/40" />
                      )}
                    </button>
                  ) : (
                    <button
                      type="button"
                      onClick={() => onDownload(variant)}
                      disabled={isDownloading}
                      title={rowLabel}
                      aria-label={rowLabel}
                      class="me-1 flex h-6 w-16 shrink-0 items-center justify-end gap-1 rounded px-1 text-[11px] transition-colors hover:bg-mid-gray/20 disabled:cursor-default"
                    >
                      {isDownloading ? (
                        <span class="tabular-nums text-text/50">
                          {t("modelSelector.quantPicker.downloadingPercent", {
                            percentage: Math.round(percentage ?? 0),
                          })}
                        </span>
                      ) : (
                        <Download class="h-3 w-3 text-text/40" />
                      )}
                    </button>
                  )}
                </div>
                {(sample || isDownloading) && (
                  <div class="flex items-center gap-2 px-2 pb-1.5">
                    <div class="h-1 min-w-0 flex-1 overflow-hidden rounded-full bg-mid-gray/20">
                      <div
                        class={`h-full rounded-full transition-[width] duration-300 ${isDownloading ? "bg-accent/60" : isFastest ? "bg-emerald-500" : "bg-accent/60"}`}
                        style={{
                          width: `${Math.max(4, Math.min(100, isDownloading ? (percentage ?? 0) : speedRatio * 100))}%`,
                        }}
                      />
                    </div>
                    {realTime !== null && !isDownloading && (
                      <span class="shrink-0 text-[10px] tabular-nums text-text/45">
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
        }}
      </For>
    </ul>
  );
};

export default QuantizationPanel;
