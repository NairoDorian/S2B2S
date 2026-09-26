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

// Every prop is read through `props.*` at its use site: this panel mounts
// only while open, and its props are live values (download progress, pending
// downloads, benchmark results) that change while the user watches it. The
// Solid 2 component body runs once, so a body-level destructure would be a
// mount-time snapshot and the panel would never update.
export const QuantizationPanel = (
  props: QuantizationPanelProps,
): JSX.Element => {
  const { t } = useTranslation();

  const downloadedIds = createMemo(
    () =>
      new Set(
        props.models
          .filter((model) => model.is_downloaded)
          .map((model) => model.id),
      ),
  );

  const familyResults = createMemo(() =>
    props.variants
      .map((variant) => ({
        id: variant.model_id,
        sample: props.benchmark.results()[variant.model_id],
      }))
      .filter((entry) => entry.sample !== undefined),
  );

  const fastestMs = createMemo(() => {
    const entries = familyResults();
    if (entries.length === 0) return null;
    return Math.min(...entries.map((entry) => entry.sample!.avgMs));
  });

  const fastestId = createMemo(() => {
    const entries = familyResults();
    if (entries.length <= 1) return null;
    const ms = fastestMs();
    return entries.find((entry) => entry.sample!.avgMs === ms)?.id ?? null;
  });

  const formatDuration = (ms: number): string =>
    ms < 1000
      ? t("modelSelector.benchmark.milliseconds", { value: Math.round(ms) })
      : t("modelSelector.benchmark.seconds", { value: (ms / 1000).toFixed(2) });

  return (
    <ul class="max-h-[min(50vh,17rem)] overflow-y-auto py-1">
      <For each={props.variants}>
        {(variant) => {
          const id = variant.model_id;
          const isCurrent = () => id === props.currentModelId;
          const isDownloaded = () => downloadedIds().has(id);
          const percentage = () => props.downloadPercentages[id];
          const isDownloading = () =>
            percentage() !== undefined || props.pendingDownloads.has(id);
          const sample = () => props.benchmark.results()[id];
          const failure = () => props.benchmark.errors()[id];
          const isMeasuring = () => props.benchmark.activeModelId() === id;
          const isFastest = () => id === fastestId();

          // "warmup" while the discarded first pass runs (index 0), "n / total"
          // once the runs that actually get averaged start landing.
          const progressLabel = () => {
            const run = props.benchmark.activeRun();
            if (!run) return "";
            return run.warmup
              ? t("modelSelector.benchmark.warmup")
              : t("modelSelector.benchmark.runProgress", {
                  done: run.index,
                  total: run.total,
                });
          };

          const rowLabel = () =>
            isCurrent()
              ? t("modelSelector.quantPicker.current", { quant: variant.quant })
              : isDownloaded()
                ? t("modelSelector.quantPicker.switchTo", {
                    quant: variant.quant,
                  })
                : t("modelSelector.quantPicker.download", {
                    quant: variant.quant,
                  });

          const speedRatio = () => {
            const ms = fastestMs();
            const s = sample();
            return s && ms ? ms / Math.max(s.avgMs, 1) : 0;
          };
          const realTime = () => {
            const s = sample();
            return s?.audioSecs != null && s.avgMs > 0
              ? s.audioSecs / (s.avgMs / 1000)
              : null;
          };

          return (
            <li>
              <div
                class={`mx-1 rounded-md transition-colors ${isCurrent() ? "bg-accent/10" : "hover:bg-mid-gray/10"}`}
              >
                <div class="flex items-center gap-1">
                  <button
                    type="button"
                    onClick={() =>
                      isDownloaded()
                        ? props.onSelect(variant)
                        : props.onDownload(variant)
                    }
                    disabled={isCurrent() || isDownloading()}
                    title={rowLabel()}
                    aria-label={rowLabel()}
                    class="flex min-w-0 flex-1 items-center gap-2 px-2 py-1.5 text-start disabled:cursor-default"
                  >
                    <span
                      class={`h-2 w-2 shrink-0 rounded-full ${getQuantColor(variant.quant)}`}
                    />
                    <span
                      class={`truncate font-medium ${isCurrent() ? "text-accent" : "text-text/85"}`}
                    >
                      {variant.quant}
                    </span>
                    {isCurrent() && (
                      <Check class="h-3 w-3 shrink-0 text-accent" />
                    )}
                    {isFastest() && (
                      <span class="shrink-0 rounded-sm bg-emerald-500/15 px-1 text-[10px] font-medium text-emerald-600 dark:text-emerald-400">
                        {t("modelSelector.benchmark.fastest")}
                      </span>
                    )}
                    {variant.is_default && !isFastest() && (
                      <span class="shrink-0 text-[10px] uppercase tracking-wide text-text/35">
                        {t("modelSelector.quantPicker.default")}
                      </span>
                    )}
                    <span class="ms-auto shrink-0 tabular-nums text-text/45">
                      {formatModelSize(variant.size_mb)}
                    </span>
                  </button>
                  {isDownloaded() ? (
                    <button
                      type="button"
                      onClick={() => void props.benchmark.runOne(id)}
                      disabled={
                        props.benchmark.isBusy() ||
                        !props.benchmark.referenceRecording()
                      }
                      title={t("modelSelector.benchmark.runOne", {
                        quant: variant.quant,
                      })}
                      aria-label={t("modelSelector.benchmark.runOne", {
                        quant: variant.quant,
                      })}
                      class="me-1 flex h-6 w-16 shrink-0 items-center justify-end gap-1 rounded px-1 text-[11px] transition-colors hover:bg-mid-gray/20 disabled:cursor-not-allowed disabled:opacity-40"
                    >
                      {isMeasuring() ? (
                        <>
                          <LoaderCircle class="h-3 w-3 shrink-0 animate-spin text-text/50" />
                          <span class="truncate text-text/50">
                            {progressLabel()}
                          </span>
                        </>
                      ) : failure() ? (
                        <span class="truncate text-error" title={failure()}>
                          {t("modelSelector.benchmark.failed")}
                        </span>
                      ) : sample() ? (
                        <span class="font-mono tabular-nums text-text/75">
                          {formatDuration(sample()!.avgMs)}
                        </span>
                      ) : (
                        <Play class="h-3 w-3 text-text/40" />
                      )}
                    </button>
                  ) : (
                    <button
                      type="button"
                      onClick={() => props.onDownload(variant)}
                      disabled={isDownloading()}
                      title={rowLabel()}
                      aria-label={rowLabel()}
                      class="me-1 flex h-6 w-16 shrink-0 items-center justify-end gap-1 rounded px-1 text-[11px] transition-colors hover:bg-mid-gray/20 disabled:cursor-default"
                    >
                      {isDownloading() ? (
                        <span class="tabular-nums text-text/50">
                          {t("modelSelector.quantPicker.downloadingPercent", {
                            percentage: Math.round(percentage() ?? 0),
                          })}
                        </span>
                      ) : (
                        <Download class="h-3 w-3 text-text/40" />
                      )}
                    </button>
                  )}
                </div>
                {(sample() || isDownloading()) && (
                  <div class="flex items-center gap-2 px-2 pb-1.5">
                    <div class="h-1 min-w-0 flex-1 overflow-hidden rounded-full bg-mid-gray/20">
                      <div
                        class={`h-full rounded-full transition-[width] duration-300 ${isDownloading() ? "bg-accent/60" : isFastest() ? "bg-emerald-500" : "bg-accent/60"}`}
                        style={{
                          width: `${Math.max(4, Math.min(100, isDownloading() ? (percentage() ?? 0) : speedRatio() * 100))}%`,
                        }}
                      />
                    </div>
                    {realTime() !== null && !isDownloading() && (
                      <span class="shrink-0 text-[10px] tabular-nums text-text/45">
                        {t("modelSelector.benchmark.realTime", {
                          factor: realTime()!.toFixed(1),
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
