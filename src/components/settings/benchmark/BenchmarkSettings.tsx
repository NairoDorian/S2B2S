import React, { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { useModelStore } from "@/stores/modelStore";
import { commands } from "@/bindings";
import type { BenchmarkResult, HistoryEntry, ModelInfo } from "@/bindings";
import { BenchmarkProgressEvent } from "@/lib/types/events";
import { getTranslatedModelName } from "@/lib/utils/modelTranslation";
import { Clock, Play } from "lucide-react";

export const BenchmarkSettings: React.FC = () => {
  const { t } = useTranslation();
  const { models, currentModel } = useModelStore();
  const [isBenchmarking, setIsBenchmarking] = useState(false);
  const [benchmarkResults, setBenchmarkResults] = useState<
    BenchmarkResult[] | null
  >(null);
  const [latestRecording, setLatestRecording] = useState<HistoryEntry | null>(
    null,
  );
  const [downloadedQuants, setDownloadedQuants] = useState(
    getDownloadedQuantCount(currentModel, models),
  );

  // Fetch latest recording info on mount
  useEffect(() => {
    let cancelled = false;
    commands.getLatestRecordingInfo().then((result) => {
      if (!cancelled && result.status === "ok") {
        setLatestRecording(result.data ?? null);
      }
    });
    return () => {
      cancelled = true;
    };
  }, []);

  // Update downloaded quant count when current model or models change
  useEffect(() => {
    setDownloadedQuants(getDownloadedQuantCount(currentModel, models));
  }, [currentModel, models]);

  const currentModelInfo: ModelInfo | undefined = models.find(
    (m) => m.id === currentModel,
  );

  const handleBenchmarkClick = async () => {
    if (!currentModel) return;

    setIsBenchmarking(true);
    setBenchmarkResults(null);

    const result = await commands.benchmarkModelQuantizations(currentModel);
    if (result.status === "ok") {
      setBenchmarkResults(result.data);
    } else {
      toast.error(result.error);
    }
    setIsBenchmarking(false);
  };

  // Listen for progress events from the backend
  useEffect(() => {
    const unlisten = listen<BenchmarkProgressEvent>(
      "benchmark-progress",
      (event) => {
        const { event_type } = event.payload;
        if (event_type === "benchmark_started") {
          setIsBenchmarking(true);
          setBenchmarkResults(null);
        } else if (
          event_type === "benchmark_completed" ||
          event_type === "benchmark_failed"
        ) {
          setIsBenchmarking(false);
        }
      },
    );
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const formatTime = (ms: number): string => {
    if (ms < 1) return "<1ms";
    if (ms < 1000) return `${Math.round(ms)}ms`;
    return `${(ms / 1000).toFixed(2)}s`;
  };

  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      <div className="mb-4">
        <h1 className="text-xl font-semibold mb-2">
          {t("settings.benchmark.title")}
        </h1>
        <p className="text-sm text-text/60">
          {t("settings.benchmark.description")}
        </p>
      </div>

      <SettingsGroup
        title={t("settings.benchmark.title")}
        description={t("settings.benchmark.description")}
      >
        <div className="p-4 space-y-4">
          {/* Current Model & Benchmark Button */}
          <div className="flex items-center justify-between">
            <div>
              <span className="text-xs font-medium text-text/60">
                {t("settings.models.title")}
              </span>
              <div className="mt-1">
                {currentModelInfo
                  ? getTranslatedModelName(currentModelInfo, t)
                  : t("settings.benchmark.noCurrentModel")}
              </div>
            </div>
            <button
              type="button"
              onClick={handleBenchmarkClick}
              disabled={
                isBenchmarking ||
                !currentModel ||
                downloadedQuants === 0 ||
                !latestRecording
              }
              className="flex items-center gap-2 px-4 py-2 text-sm font-medium rounded-lg border border-mid-gray/30 bg-mid-gray/10 hover:bg-mid-gray/20 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
            >
              {isBenchmarking ? (
                <>
                  <Clock className="w-4 h-4 animate-spin" />
                  {t("settings.benchmark.buttonRunning")}
                </>
              ) : (
                <>
                  <Play className="w-4 h-4" />
                  {t("settings.benchmark.button")}
                </>
              )}
            </button>
          </div>

          {/* Reference Recording */}
          <div className="flex items-center gap-2 text-sm">
            <Clock className="w-4 h-4 text-text/40" />
            <span className="text-text/60">
              {t("settings.benchmark.usingRecording")}
            </span>
            {latestRecording ? (
              <span className="font-medium">
                {new Date(
                  (latestRecording.timestamp ?? 0) * 1000,
                ).toLocaleString()}
              </span>
            ) : (
              <span className="text-text/40">
                {t("settings.benchmark.noRecording")}
              </span>
            )}
          </div>

          {/* Downloaded Quantizations Count */}
          <div className="text-sm text-text/60">
            {downloadedQuants > 0
              ? t("settings.benchmark.downloadedQuants", {
                  count: downloadedQuants,
                })
              : t("settings.benchmark.noDownloadedQuants")}
          </div>

          {/* Results Table */}
          {benchmarkResults && benchmarkResults.length > 0 && (
            <div className="space-y-2 pt-2 border-t border-mid-gray/20">
              <h3 className="text-sm font-medium text-text/60">
                {t("settings.benchmark.results.title")}
              </h3>
              <div className="space-y-1.5">
                {benchmarkResults
                  .slice()
                  .sort((a, b) => (a.avg_time_ms ?? 0) - (b.avg_time_ms ?? 0))
                  .map((result) => (
                    <div
                      key={result.quant}
                      className="flex items-center justify-between px-3 py-2 bg-mid-gray/5 border border-mid-gray/20 rounded-lg"
                    >
                      <div className="flex items-center gap-2">
                        <span
                          className={`w-2 h-2 rounded-full ${getQuantColor(result.quant)}`}
                        />
                        <span className="font-medium">{result.quant}</span>
                        {result.is_default && (
                          <span className="text-xs text-text/40">
                            {t("settings.benchmark.results.default")}
                          </span>
                        )}
                        <span className="text-xs text-text/40">
                          {result.size_mb} MB
                        </span>
                      </div>
                      <div className="text-right">
                        <div className="font-mono font-medium text-text/80">
                          {formatTime(result.avg_time_ms ?? 0)}
                        </div>
                        <div className="text-xs text-text/40">
                          {t("settings.benchmark.results.ranIn", {
                            ms: Math.round(result.avg_time_ms ?? 0),
                          })}
                        </div>
                      </div>
                    </div>
                  ))}
              </div>
            </div>
          )}

          {!benchmarkResults && !isBenchmarking && (
            <div className="text-center py-8 text-text/50">
              <Clock className="w-8 h-8 mx-auto mb-2 opacity-30" />
              <p>{t("settings.benchmark.noResults")}</p>
            </div>
          )}
        </div>
      </SettingsGroup>
    </div>
  );
};

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

function getDownloadedQuantCount(
  currentModel: string | null,
  models: ModelInfo[],
): number {
  if (!currentModel) return 0;
  // Quant variants share the same repo prefix (everything before the last /)
  const repoPrefix = currentModel.split("/").slice(0, -1).join("/");
  return models.filter(
    (m) => m.is_downloaded && m.id.startsWith(repoPrefix + "/"),
  ).length;
}
