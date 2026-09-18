import { createSignal, createEffect, createMemo, For, Show } from "solid-js";
import {
  Activity,
  AudioLines,
  Flame,
  Gauge,
  LoaderCircle,
  Mic2,
  RotateCw,
  Type,
  WandSparkles,
  type LucideIcon,
} from "@/components/icons/lucide";
import { useTranslation } from "@/i18n/useTranslation";
import {
  commands,
  events,
  type DurationMetricSummary,
  type StatisticsSummary,
} from "@/bindings";
import { Alert } from "../../ui/Alert";
import { Button } from "../../ui/Button";
import { Dialog } from "../../ui/Dialog";
import { SettingsGroup } from "../../ui/SettingsGroup";
import {
  buildStatisticsRange,
  STATISTICS_RANGE_OPTIONS,
  type StatisticsRangeOption,
} from "./statisticsRange";

type LoadState = "loading" | "error" | "ready";

export const StatisticsSettings = () => {
  const { t, i18n } = useTranslation();
  const [selectedRange, setSelectedRange] =
    createSignal<StatisticsRangeOption>("sevenDays");
  const [statistics, setStatistics] = createSignal<StatisticsSummary | null>(
    null,
  );
  const [loadState, setLoadState] = createSignal<LoadState>("loading");
  const [resetDialogOpen, setResetDialogOpen] = createSignal(false);
  const [resetting, setResetting] = createSignal(false);
  const [resetError, setResetError] = createSignal(false);
  let mountedRef = false;
  let requestIdRef = 0;
  let suppressNextUpdateRef = false;
  const initialFocusRef: { current: HTMLElement | null } = { current: null };

  const loadStatistics = async (
    rangeOption: StatisticsRangeOption,
    showLoading = true,
  ) => {
    const requestId = ++requestIdRef;
    if (showLoading) {
      setLoadState("loading");
      setStatistics(null);
    }

    try {
      const result = await commands.getStatisticsSummary(
        buildStatisticsRange(rangeOption),
      );
      if (result.status === "error") {
        throw new Error(result.error);
      }
      if (mountedRef && requestId === requestIdRef) {
        setStatistics(result.data);
        setLoadState("ready");
      }
    } catch (error) {
      console.error("Failed to load statistics:", error);
      if (mountedRef && requestId === requestIdRef) {
        setStatistics(null);
        setLoadState("error");
      }
    }
  };

  createEffect(
    () => undefined,
    () => {
      mountedRef = true;
      return () => {
        mountedRef = false;
        requestIdRef += 1;
      };
    },
  );

  createEffect(
    () => undefined,
    () => {
      const unlisten = events.statisticsUpdatedEvent.listen(() => {
        if (suppressNextUpdateRef) {
          suppressNextUpdateRef = false;
          return;
        }
        void loadStatistics(selectedRange(), false);
      });
      void unlisten
        .then(() => loadStatistics(selectedRange()))
        .catch((error) => {
          console.error("Failed to listen for statistics updates:", error);
          void loadStatistics(selectedRange());
        });

      return () => {
        unlisten
          .then((removeListener) => removeListener())
          .catch((error) =>
            console.error("Failed to remove statistics listener:", error),
          );
      };
    },
  );

  const numberFormatter = new Intl.NumberFormat(i18n.language, {
    maximumFractionDigits: 1,
  });
  const decimalNumberFormatter = new Intl.NumberFormat(i18n.language, {
    minimumFractionDigits: 1,
    maximumFractionDigits: 1,
  });
  const dateFormatter = new Intl.DateTimeFormat(i18n.language, {
    dateStyle: "medium",
  });
  const formatNumber = (value: number) => numberFormatter.format(value);
  const formatDecimalNumber = (value: number) =>
    decimalNumberFormatter.format(value);
  const formatLatency = (milliseconds: number) =>
    milliseconds < 1_000
      ? t("settings.statistics.units.milliseconds", {
          value: formatNumber(milliseconds),
        })
      : t("settings.statistics.units.seconds", {
          value: formatNumber(milliseconds / 1_000),
        });
  const formatAudioDuration = (milliseconds: number) => {
    const totalSeconds = Math.round(milliseconds / 1_000);
    const hours = Math.floor(totalSeconds / 3_600);
    const minutes = Math.floor((totalSeconds % 3_600) / 60);
    const seconds = totalSeconds % 60;

    if (hours > 0) {
      return t("settings.statistics.units.hoursMinutes", {
        hours: formatNumber(hours),
        minutes: formatNumber(minutes),
      });
    }
    if (minutes > 0) {
      return t("settings.statistics.units.minutesSeconds", {
        minutes: formatNumber(minutes),
        seconds: formatNumber(seconds),
      });
    }
    return t("settings.statistics.units.secondsLong", {
      seconds: formatNumber(seconds),
    });
  };
  const formatReturnedRange = (summary: StatisticsSummary) => {
    if (
      (selectedRange() === "allTime" && summary.range.start_ms === 0) ||
      summary.range.start_ms == null ||
      summary.range.end_ms == null
    ) {
      return t("settings.statistics.range.options.allTime");
    }
    const startMs = summary.range.start_ms;
    const endMs = summary.range.end_ms;
    const start = new Date(startMs);
    const end = new Date(Math.max(startMs, endMs - 1));
    return (
      dateFormatter as Intl.DateTimeFormat & {
        formatRange(startDate: Date, endDate: Date): string;
      }
    ).formatRange(start, end);
  };

  const selectedRangeLabel = createMemo(() => {
    const summary = statistics();
    return summary
      ? formatReturnedRange(summary)
      : t(`settings.statistics.range.options.${selectedRange()}`);
  });

  const handleReset = async () => {
    setResetting(true);
    setResetError(false);
    suppressNextUpdateRef = true;
    try {
      const result = await commands.resetStatistics();
      if (result.status === "error") {
        throw new Error(result.error);
      }
      if (!mountedRef) return;
      setResetDialogOpen(false);
      await loadStatistics(selectedRange());
      suppressNextUpdateRef = false;
    } catch (error) {
      console.error("Failed to reset statistics:", error);
      suppressNextUpdateRef = false;
      if (mountedRef) setResetError(true);
    } finally {
      if (mountedRef) setResetting(false);
    }
  };

  return (
    <div class="max-w-3xl w-full mx-auto space-y-6">
      <header class="space-y-2 px-4">
        <h1 class="text-lg font-semibold">{t("settings.statistics.title")}</h1>
        <p class="max-w-2xl text-sm text-mid-gray">
          {t("settings.statistics.description")}
        </p>
      </header>

      <SettingsGroup title={t("settings.statistics.controls.title")}>
        <fieldset class="space-y-2 p-4">
          <legend class="text-sm font-medium">
            {t("settings.statistics.range.label")}
          </legend>
          <div class="flex flex-wrap gap-2">
            <For each={STATISTICS_RANGE_OPTIONS}>
              {(range) => (
                <button
                  type="button"
                  aria-pressed={selectedRange() === range ? "true" : "false"}
                  onClick={() => setSelectedRange(range)}
                  class={`cursor-pointer rounded-lg border px-3 py-1.5 text-sm font-medium transition-colors focus:outline-none focus-visible:ring-1 focus-visible:ring-accent ${
                    selectedRange() === range
                      ? "border-accent bg-accent/20"
                      : "border-mid-gray/20 bg-mid-gray/10 hover:border-accent hover:bg-accent/10"
                  }`}
                >
                  {t(`settings.statistics.range.options.${range}`)}
                </button>
              )}
            </For>
          </div>
        </fieldset>
      </SettingsGroup>

      <section aria-labelledby="statistics-summary-title" class="space-y-2">
        <div class="px-4">
          <h2
            id="statistics-summary-title"
            class="text-xs font-medium uppercase tracking-wide text-mid-gray"
          >
            {t("settings.statistics.summary.title")}
          </h2>
          <p class="mt-1 text-sm font-medium">
            {t("settings.statistics.range.selected", {
              range: selectedRangeLabel(),
            })}
          </p>
        </div>

        <div
          class="rounded-lg border border-mid-gray/20 bg-background p-3 sm:p-4"
          aria-live="polite"
          aria-busy={loadState() === "loading" ? "true" : "false"}
        >
          {loadState() === "loading" && <LoadingState />}
          {loadState() === "error" && (
            <ErrorState onRetry={() => void loadStatistics(selectedRange())} />
          )}
          <Show when={loadState() === "ready" ? statistics() : null}>
            {(stats) =>
              stats().transcription_count === 0 ? (
                <EmptyState />
              ) : (
                <div class="grid grid-cols-1 gap-3 sm:grid-cols-2">
                  <MetricCard
                    icon={Mic2}
                    title={t(
                      "settings.statistics.metrics.transcriptions.title",
                    )}
                    value={formatNumber(stats().transcription_count)}
                    detail={t(
                      "settings.statistics.metrics.transcriptions.description",
                    )}
                  />
                  <MetricCard
                    icon={Type}
                    title={t("settings.statistics.metrics.words.title")}
                    value={formatNumber(stats().total_words)}
                    unit={t("settings.statistics.units.words")}
                    detail={
                      stats().average_words == null
                        ? t("settings.statistics.latency.unavailable")
                        : t("settings.statistics.metrics.words.average", {
                            value: formatNumber(stats().average_words ?? 0),
                          })
                    }
                  />
                  <MetricCard
                    icon={AudioLines}
                    title={t("settings.statistics.metrics.audio.title")}
                    value={formatAudioDuration(
                      stats().total_audio_duration_ms ?? 0,
                    )}
                    detail={
                      stats().average_audio_duration_ms == null
                        ? t("settings.statistics.latency.unavailable")
                        : t("settings.statistics.metrics.audio.average", {
                            value: formatAudioDuration(
                              stats().average_audio_duration_ms ?? 0,
                            ),
                          })
                    }
                  />
                  <MetricCard
                    icon={Activity}
                    title={t("settings.statistics.metrics.wpm.title")}
                    value={
                      stats().approximate_words_per_minute == null
                        ? t("settings.statistics.latency.unavailable")
                        : formatDecimalNumber(
                            stats().approximate_words_per_minute ?? 0,
                          )
                    }
                    unit={
                      stats().approximate_words_per_minute == null
                        ? undefined
                        : t("settings.statistics.units.wpm")
                    }
                    detail={t("settings.statistics.metrics.wpm.description")}
                  />
                  <MetricCard
                    icon={Flame}
                    title={t("settings.statistics.metrics.streak.title")}
                    value={formatNumber(stats().current_streak_days)}
                    unit={t("settings.statistics.units.days")}
                    detail={t("settings.statistics.metrics.streak.description")}
                  />
                  <LatencyCard
                    icon={Gauge}
                    title={t("settings.statistics.metrics.transcription.title")}
                    description={t(
                      "settings.statistics.metrics.transcription.description",
                    )}
                    summary={stats().transcription_latency}
                    formatLatency={formatLatency}
                    formatNumber={formatNumber}
                    unavailableDescription={t(
                      "settings.statistics.metrics.transcription.description",
                    )}
                  />
                  <LatencyCard
                    icon={WandSparkles}
                    title={t(
                      "settings.statistics.metrics.postProcessing.title",
                    )}
                    description={t(
                      "settings.statistics.metrics.postProcessing.description",
                    )}
                    summary={stats().post_processing_latency}
                    formatLatency={formatLatency}
                    formatNumber={formatNumber}
                    unavailableDescription={t(
                      "settings.statistics.metrics.postProcessing.unavailable",
                    )}
                  />
                </div>
              )
            }
          </Show>
        </div>
      </section>

      <SettingsGroup
        title={t("settings.statistics.reset.title")}
        description={t("settings.statistics.reset.description")}
      >
        <div class="flex flex-col items-start justify-between gap-3 p-4 sm:flex-row sm:items-center">
          <p class="text-sm text-mid-gray">
            {t("settings.statistics.reset.historyUnaffected")}
          </p>
          <Button
            type="button"
            variant="danger-ghost"
            disabled={resetting()}
            onClick={() => {
              setResetError(false);
              setResetDialogOpen(true);
            }}
            aria-label={t("settings.statistics.reset.accessibleAction")}
            class="shrink-0"
          >
            {t("settings.statistics.reset.action")}
          </Button>
        </div>
      </SettingsGroup>

      <Dialog
        open={resetDialogOpen()}
        title={t("settings.statistics.reset.dialog.title")}
        description={t("settings.statistics.reset.dialog.description")}
        closeLabel={t("common.close")}
        dismissible={!resetting()}
        initialFocusRef={initialFocusRef}
        onOpenChange={(open) => {
          if (!resetting()) setResetDialogOpen(open);
        }}
        footer={
          <>
            <Button
              type="button"
              variant="secondary"
              disabled={resetting()}
              onClick={() => setResetDialogOpen(false)}
            >
              {t("settings.statistics.reset.dialog.cancel")}
            </Button>
            <Button
              type="button"
              variant="danger"
              disabled={resetting()}
              onClick={() => void handleReset()}
              class="flex items-center gap-2"
            >
              {resetting() && (
                <LoaderCircle
                  class="h-3.5 w-3.5 animate-spin"
                  aria-hidden="true"
                />
              )}
              {t("settings.statistics.reset.dialog.confirm")}
            </Button>
          </>
        }
      >
        <div class="space-y-3">
          <Alert variant="warning">
            {t("settings.statistics.reset.dialog.warning")}
          </Alert>
          {resetError() && (
            <div role="alert">
              <Alert variant="error">
                {t("settings.statistics.states.error.description")}
              </Alert>
            </div>
          )}
        </div>
      </Dialog>
    </div>
  );
};

interface MetricCardProps {
  icon: LucideIcon;
  title: string;
  value: string;
  unit?: string;
  detail: string;
}

const MetricCard = ({
  icon: Icon,
  title,
  value,
  unit,
  detail,
}: MetricCardProps) => (
  <article class="rounded-lg border border-mid-gray/20 bg-mid-gray/5 p-4">
    <div class="mb-4 flex items-center gap-2 text-mid-gray">
      <Icon class="h-4 w-4 shrink-0" aria-hidden="true" />
      <h3 class="text-xs font-medium uppercase tracking-wide">{title}</h3>
    </div>
    <div class="flex flex-wrap items-baseline gap-1.5">
      <p class="text-2xl font-semibold tabular-nums">{value}</p>
      {unit && <span class="text-sm text-mid-gray">{unit}</span>}
    </div>
    <p class="mt-2 text-xs text-mid-gray">{detail}</p>
  </article>
);

interface LatencyCardProps {
  icon: LucideIcon;
  title: string;
  description: string;
  summary: DurationMetricSummary;
  formatLatency: (milliseconds: number) => string;
  formatNumber: (value: number) => string;
  unavailableDescription: string;
}

const LatencyCard = ({
  icon: Icon,
  title,
  description,
  summary,
  formatLatency,
  formatNumber,
  unavailableDescription,
}: LatencyCardProps) => {
  const { t } = useTranslation();
  const { sample_count, minimum_ms, average_ms, maximum_ms } = summary;
  const available =
    sample_count > 0 &&
    minimum_ms != null &&
    average_ms != null &&
    maximum_ms != null;

  return (
    <article class="rounded-lg border border-mid-gray/20 bg-mid-gray/5 p-4 sm:col-span-2">
      <div class="flex items-start gap-3">
        <div class="rounded-md bg-accent/15 p-2 text-text">
          <Icon class="h-4 w-4" aria-hidden="true" />
        </div>
        <div class="min-w-0 flex-1">
          <h3 class="text-sm font-semibold">{title}</h3>
          <p class="mt-0.5 text-xs text-mid-gray">{description}</p>
        </div>
      </div>

      {available ? (
        <>
          <dl class="mt-4 grid grid-cols-3 gap-2 border-y border-mid-gray/20 py-3">
            <LatencyValue
              label={t("settings.statistics.latency.minimum")}
              value={formatLatency(minimum_ms ?? 0)}
            />
            <LatencyValue
              label={t("settings.statistics.latency.average")}
              value={formatLatency(average_ms ?? 0)}
            />
            <LatencyValue
              label={t("settings.statistics.latency.maximum")}
              value={formatLatency(maximum_ms ?? 0)}
            />
          </dl>
          <p class="mt-2 text-xs text-mid-gray">
            {t("settings.statistics.latency.samples", {
              count: sample_count,
              formattedCount: formatNumber(sample_count),
            })}
          </p>
        </>
      ) : (
        <div class="mt-4 rounded-md border border-mid-gray/20 bg-background p-3">
          <p class="text-sm font-medium">
            {t("settings.statistics.latency.unavailable")}
          </p>
          <p class="mt-1 text-xs text-mid-gray">{unavailableDescription}</p>
        </div>
      )}
    </article>
  );
};

const LatencyValue = ({ label, value }: { label: string; value: string }) => (
  <div class="min-w-0 text-center">
    <dt class="text-xs text-mid-gray">{label}</dt>
    <dd class="mt-1 truncate text-sm font-semibold tabular-nums" title={value}>
      {value}
    </dd>
  </div>
);

const LoadingState = () => {
  const { t } = useTranslation();

  return (
    <output class="flex min-h-64 flex-col items-center justify-center gap-3 text-center block">
      <LoaderCircle
        class="h-7 w-7 animate-spin text-accent"
        aria-hidden="true"
      />
      <div>
        <p class="text-sm font-medium">
          {t("settings.statistics.states.loading.title")}
        </p>
        <p class="mt-1 text-xs text-mid-gray">
          {t("settings.statistics.states.loading.description")}
        </p>
      </div>
    </output>
  );
};

const EmptyState = () => {
  const { t } = useTranslation();

  return (
    <div class="flex min-h-64 flex-col items-center justify-center gap-3 px-4 text-center">
      <div class="rounded-full bg-mid-gray/10 p-3 text-mid-gray">
        <Mic2 class="h-6 w-6" aria-hidden="true" />
      </div>
      <div class="max-w-sm">
        <p class="text-sm font-medium">
          {t("settings.statistics.states.empty.title")}
        </p>
        <p class="mt-1 text-xs text-mid-gray">
          {t("settings.statistics.states.empty.description")}
        </p>
      </div>
    </div>
  );
};

const ErrorState = ({ onRetry }: { onRetry: () => void }) => {
  const { t } = useTranslation();

  return (
    <div
      class="flex min-h-64 flex-col items-center justify-center gap-4"
      role="alert"
    >
      <Alert variant="error" class="w-full max-w-md">
        {t("settings.statistics.states.error.description")}
      </Alert>
      <Button
        type="button"
        variant="secondary"
        onClick={onRetry}
        class="flex items-center gap-2"
      >
        <RotateCw class="h-3.5 w-3.5" aria-hidden="true" />
        {t("settings.statistics.states.error.retry")}
      </Button>
    </div>
  );
};
