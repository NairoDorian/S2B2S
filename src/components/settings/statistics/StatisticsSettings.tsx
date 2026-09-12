import React, { useCallback, useEffect, useRef, useState } from "react";
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
} from "lucide-react";
import { useTranslation } from "react-i18next";
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

export const StatisticsSettings: React.FC = () => {
  const { t, i18n } = useTranslation();
  const [selectedRange, setSelectedRange] =
    useState<StatisticsRangeOption>("sevenDays");
  const [statistics, setStatistics] = useState<StatisticsSummary | null>(null);
  const [loadState, setLoadState] = useState<LoadState>("loading");
  const [resetDialogOpen, setResetDialogOpen] = useState(false);
  const [resetting, setResetting] = useState(false);
  const [resetError, setResetError] = useState(false);
  const mountedRef = useRef(false);
  const requestIdRef = useRef(0);
  const suppressNextUpdateRef = useRef(false);

  const loadStatistics = useCallback(
    async (rangeOption: StatisticsRangeOption, showLoading = true) => {
      const requestId = ++requestIdRef.current;
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
        if (mountedRef.current && requestId === requestIdRef.current) {
          setStatistics(result.data);
          setLoadState("ready");
        }
      } catch (error) {
        console.error("Failed to load statistics:", error);
        if (mountedRef.current && requestId === requestIdRef.current) {
          setStatistics(null);
          setLoadState("error");
        }
      }
    },
    [],
  );

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      requestIdRef.current += 1;
    };
  }, []);

  useEffect(() => {
    const unlisten = events.statisticsUpdatedEvent.listen(() => {
      if (suppressNextUpdateRef.current) {
        suppressNextUpdateRef.current = false;
        return;
      }
      void loadStatistics(selectedRange, false);
    });
    void unlisten
      .then(() => loadStatistics(selectedRange))
      .catch((error) => {
        console.error("Failed to listen for statistics updates:", error);
        void loadStatistics(selectedRange);
      });

    return () => {
      unlisten
        .then((removeListener) => removeListener())
        .catch((error) =>
          console.error("Failed to remove statistics listener:", error),
        );
    };
  }, [loadStatistics, selectedRange]);

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
      (selectedRange === "allTime" && summary.range.start_ms === 0) ||
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

  const handleReset = async () => {
    setResetting(true);
    setResetError(false);
    suppressNextUpdateRef.current = true;
    try {
      const result = await commands.resetStatistics();
      if (result.status === "error") {
        throw new Error(result.error);
      }
      if (!mountedRef.current) return;
      setResetDialogOpen(false);
      await loadStatistics(selectedRange);
      suppressNextUpdateRef.current = false;
    } catch (error) {
      console.error("Failed to reset statistics:", error);
      suppressNextUpdateRef.current = false;
      if (mountedRef.current) setResetError(true);
    } finally {
      if (mountedRef.current) setResetting(false);
    }
  };

  const averageWords = statistics?.average_words;
  const averageAudioDuration = statistics?.average_audio_duration_ms;

  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      <header className="space-y-2 px-4">
        <h1 className="text-lg font-semibold">
          {t("settings.statistics.title")}
        </h1>
        <p className="max-w-2xl text-sm text-mid-gray">
          {t("settings.statistics.description")}
        </p>
      </header>

      <SettingsGroup title={t("settings.statistics.controls.title")}>
        <fieldset className="space-y-2 p-4">
          <legend className="text-sm font-medium">
            {t("settings.statistics.range.label")}
          </legend>
          <div className="flex flex-wrap gap-2">
            {STATISTICS_RANGE_OPTIONS.map((range) => (
              <button
                key={range}
                type="button"
                aria-pressed={selectedRange === range}
                onClick={() => setSelectedRange(range)}
                className={`cursor-pointer rounded-lg border px-3 py-1.5 text-sm font-medium transition-colors focus:outline-none focus-visible:ring-1 focus-visible:ring-accent ${
                  selectedRange === range
                    ? "border-accent bg-accent/20"
                    : "border-mid-gray/20 bg-mid-gray/10 hover:border-accent hover:bg-accent/10"
                }`}
              >
                {t(`settings.statistics.range.options.${range}`)}
              </button>
            ))}
          </div>
        </fieldset>
      </SettingsGroup>

      <section aria-labelledby="statistics-summary-title" className="space-y-2">
        <div className="px-4">
          <h2
            id="statistics-summary-title"
            className="text-xs font-medium uppercase tracking-wide text-mid-gray"
          >
            {t("settings.statistics.summary.title")}
          </h2>
          <p className="mt-1 text-sm font-medium">
            {t("settings.statistics.range.selected", {
              range: statistics
                ? formatReturnedRange(statistics)
                : t(`settings.statistics.range.options.${selectedRange}`),
            })}
          </p>
        </div>

        <div
          className="rounded-lg border border-mid-gray/20 bg-background p-3 sm:p-4"
          aria-live="polite"
          aria-busy={loadState === "loading"}
        >
          {loadState === "loading" && <LoadingState />}
          {loadState === "error" && (
            <ErrorState onRetry={() => void loadStatistics(selectedRange)} />
          )}
          {loadState === "ready" && statistics?.transcription_count === 0 && (
            <EmptyState />
          )}
          {loadState === "ready" &&
            statistics &&
            statistics.transcription_count > 0 && (
              <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                <MetricCard
                  icon={Mic2}
                  title={t("settings.statistics.metrics.transcriptions.title")}
                  value={formatNumber(statistics.transcription_count)}
                  detail={t(
                    "settings.statistics.metrics.transcriptions.description",
                  )}
                />
                <MetricCard
                  icon={Type}
                  title={t("settings.statistics.metrics.words.title")}
                  value={formatNumber(statistics.total_words)}
                  unit={t("settings.statistics.units.words")}
                  detail={
                    averageWords == null
                      ? t("settings.statistics.latency.unavailable")
                      : t("settings.statistics.metrics.words.average", {
                          value: formatNumber(averageWords),
                        })
                  }
                />
                <MetricCard
                  icon={AudioLines}
                  title={t("settings.statistics.metrics.audio.title")}
                  value={formatAudioDuration(
                    statistics.total_audio_duration_ms ?? 0,
                  )}
                  detail={
                    averageAudioDuration == null
                      ? t("settings.statistics.latency.unavailable")
                      : t("settings.statistics.metrics.audio.average", {
                          value: formatAudioDuration(averageAudioDuration),
                        })
                  }
                />
                <MetricCard
                  icon={Activity}
                  title={t("settings.statistics.metrics.wpm.title")}
                  value={
                    statistics.approximate_words_per_minute == null
                      ? t("settings.statistics.latency.unavailable")
                      : formatDecimalNumber(
                          statistics.approximate_words_per_minute,
                        )
                  }
                  unit={
                    statistics.approximate_words_per_minute == null
                      ? undefined
                      : t("settings.statistics.units.wpm")
                  }
                  detail={t("settings.statistics.metrics.wpm.description")}
                />
                <MetricCard
                  icon={Flame}
                  title={t("settings.statistics.metrics.streak.title")}
                  value={formatNumber(statistics.current_streak_days)}
                  unit={t("settings.statistics.units.days")}
                  detail={t("settings.statistics.metrics.streak.description")}
                />
                <LatencyCard
                  icon={Gauge}
                  title={t("settings.statistics.metrics.transcription.title")}
                  description={t(
                    "settings.statistics.metrics.transcription.description",
                  )}
                  summary={statistics.transcription_latency}
                  formatLatency={formatLatency}
                  formatNumber={formatNumber}
                  unavailableDescription={t(
                    "settings.statistics.metrics.transcription.description",
                  )}
                />
                <LatencyCard
                  icon={WandSparkles}
                  title={t("settings.statistics.metrics.postProcessing.title")}
                  description={t(
                    "settings.statistics.metrics.postProcessing.description",
                  )}
                  summary={statistics.post_processing_latency}
                  formatLatency={formatLatency}
                  formatNumber={formatNumber}
                  unavailableDescription={t(
                    "settings.statistics.metrics.postProcessing.unavailable",
                  )}
                />
              </div>
            )}
        </div>
      </section>

      <SettingsGroup
        title={t("settings.statistics.reset.title")}
        description={t("settings.statistics.reset.description")}
      >
        <div className="flex flex-col items-start justify-between gap-3 p-4 sm:flex-row sm:items-center">
          <p className="text-sm text-mid-gray">
            {t("settings.statistics.reset.historyUnaffected")}
          </p>
          <Button
            type="button"
            variant="danger-ghost"
            disabled={resetting}
            onClick={() => {
              setResetError(false);
              setResetDialogOpen(true);
            }}
            aria-label={t("settings.statistics.reset.accessibleAction")}
            className="shrink-0"
          >
            {t("settings.statistics.reset.action")}
          </Button>
        </div>
      </SettingsGroup>

      <Dialog
        open={resetDialogOpen}
        title={t("settings.statistics.reset.dialog.title")}
        description={t("settings.statistics.reset.dialog.description")}
        closeLabel={t("common.close")}
        dismissible={!resetting}
        onOpenChange={(open) => {
          if (!resetting) setResetDialogOpen(open);
        }}
        footer={
          <>
            <Button
              type="button"
              variant="secondary"
              disabled={resetting}
              onClick={() => setResetDialogOpen(false)}
            >
              {t("settings.statistics.reset.dialog.cancel")}
            </Button>
            <Button
              type="button"
              variant="danger"
              disabled={resetting}
              onClick={() => void handleReset()}
              className="flex items-center gap-2"
            >
              {resetting && (
                <LoaderCircle
                  className="h-3.5 w-3.5 animate-spin"
                  aria-hidden="true"
                />
              )}
              {t("settings.statistics.reset.dialog.confirm")}
            </Button>
          </>
        }
      >
        <div className="space-y-3">
          <Alert variant="warning">
            {t("settings.statistics.reset.dialog.warning")}
          </Alert>
          {resetError && (
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

const MetricCard: React.FC<MetricCardProps> = ({
  icon: Icon,
  title,
  value,
  unit,
  detail,
}) => (
  <article className="rounded-lg border border-mid-gray/20 bg-mid-gray/5 p-4">
    <div className="mb-4 flex items-center gap-2 text-mid-gray">
      <Icon className="h-4 w-4 shrink-0" aria-hidden="true" />
      <h3 className="text-xs font-medium uppercase tracking-wide">{title}</h3>
    </div>
    <div className="flex flex-wrap items-baseline gap-1.5">
      <p className="text-2xl font-semibold tabular-nums">{value}</p>
      {unit && <span className="text-sm text-mid-gray">{unit}</span>}
    </div>
    <p className="mt-2 text-xs text-mid-gray">{detail}</p>
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

const LatencyCard: React.FC<LatencyCardProps> = ({
  icon: Icon,
  title,
  description,
  summary,
  formatLatency,
  formatNumber,
  unavailableDescription,
}) => {
  const { t } = useTranslation();
  const { sample_count, minimum_ms, average_ms, maximum_ms } = summary;
  const available =
    sample_count > 0 &&
    minimum_ms != null &&
    average_ms != null &&
    maximum_ms != null;

  return (
    <article className="rounded-lg border border-mid-gray/20 bg-mid-gray/5 p-4 sm:col-span-2">
      <div className="flex items-start gap-3">
        <div className="rounded-md bg-accent/15 p-2 text-text">
          <Icon className="h-4 w-4" aria-hidden="true" />
        </div>
        <div className="min-w-0 flex-1">
          <h3 className="text-sm font-semibold">{title}</h3>
          <p className="mt-0.5 text-xs text-mid-gray">{description}</p>
        </div>
      </div>

      {available ? (
        <>
          <dl className="mt-4 grid grid-cols-3 gap-2 border-y border-mid-gray/20 py-3">
            <LatencyValue
              label={t("settings.statistics.latency.minimum")}
              value={formatLatency(minimum_ms)}
            />
            <LatencyValue
              label={t("settings.statistics.latency.average")}
              value={formatLatency(average_ms)}
            />
            <LatencyValue
              label={t("settings.statistics.latency.maximum")}
              value={formatLatency(maximum_ms)}
            />
          </dl>
          <p className="mt-2 text-xs text-mid-gray">
            {t("settings.statistics.latency.samples", {
              count: sample_count,
              formattedCount: formatNumber(sample_count),
            })}
          </p>
        </>
      ) : (
        <div className="mt-4 rounded-md border border-mid-gray/20 bg-background p-3">
          <p className="text-sm font-medium">
            {t("settings.statistics.latency.unavailable")}
          </p>
          <p className="mt-1 text-xs text-mid-gray">{unavailableDescription}</p>
        </div>
      )}
    </article>
  );
};

const LatencyValue: React.FC<{ label: string; value: string }> = ({
  label,
  value,
}) => (
  <div className="min-w-0 text-center">
    <dt className="text-xs text-mid-gray">{label}</dt>
    <dd
      className="mt-1 truncate text-sm font-semibold tabular-nums"
      title={value}
    >
      {value}
    </dd>
  </div>
);

const LoadingState: React.FC = () => {
  const { t } = useTranslation();

  return (
    <div
      className="flex min-h-64 flex-col items-center justify-center gap-3 text-center"
      role="status"
      aria-live="polite"
    >
      <LoaderCircle
        className="h-7 w-7 animate-spin text-accent"
        aria-hidden="true"
      />
      <div>
        <p className="text-sm font-medium">
          {t("settings.statistics.states.loading.title")}
        </p>
        <p className="mt-1 text-xs text-mid-gray">
          {t("settings.statistics.states.loading.description")}
        </p>
      </div>
    </div>
  );
};

const EmptyState: React.FC = () => {
  const { t } = useTranslation();

  return (
    <div className="flex min-h-64 flex-col items-center justify-center gap-3 px-4 text-center">
      <div className="rounded-full bg-mid-gray/10 p-3 text-mid-gray">
        <Mic2 className="h-6 w-6" aria-hidden="true" />
      </div>
      <div className="max-w-sm">
        <p className="text-sm font-medium">
          {t("settings.statistics.states.empty.title")}
        </p>
        <p className="mt-1 text-xs text-mid-gray">
          {t("settings.statistics.states.empty.description")}
        </p>
      </div>
    </div>
  );
};

const ErrorState: React.FC<{ onRetry: () => void }> = ({ onRetry }) => {
  const { t } = useTranslation();

  return (
    <div
      className="flex min-h-64 flex-col items-center justify-center gap-4"
      role="alert"
    >
      <Alert variant="error" className="w-full max-w-md">
        {t("settings.statistics.states.error.description")}
      </Alert>
      <Button
        type="button"
        variant="secondary"
        onClick={onRetry}
        className="flex items-center gap-2"
      >
        <RotateCw className="h-3.5 w-3.5" aria-hidden="true" />
        {t("settings.statistics.states.error.retry")}
      </Button>
    </div>
  );
};
