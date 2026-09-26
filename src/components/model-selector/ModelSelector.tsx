import {
  createSignal,
  createEffect,
  createMemo,
  createUniqueId,
  Show,
} from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import { listen } from "@tauri-apps/api/event";
import {
  Cpu,
  Gauge,
  LoaderCircle,
  Radio,
  Zap,
} from "@/components/icons/lucide";
import { commands } from "@/bindings";
import type {
  ModelBackendSetting,
  NativeStreamingLatencyPreset,
  QuantVariant,
} from "@/bindings";
import { getTranslatedModelName } from "../../lib/utils/modelTranslation";
import { useModelStore, selectModel } from "../../stores/modelStore";
import { useSettingsStore } from "@/stores/settingsStore";
import ModelStatusButton, { type ModelStatus } from "./ModelStatusButton";
import ModelDropdown from "./ModelDropdown";
import DownloadProgressDisplay from "./DownloadProgressDisplay";
import StatusBarPopover from "./StatusBarPopover";
import QuantizationPanel from "./QuantizationPanel";
import BenchmarkRunsSelect from "./BenchmarkRunsSelect";
import StreamingLatencyControl from "./StreamingLatencyControl";
import ModelBackendPanel, { backendLabel } from "./ModelBackendPanel";
import { getQuantColor } from "./quantColors";
import {
  MAX_BENCHMARK_RUNS,
  MIN_BENCHMARK_RUNS,
  useQuantBenchmark,
} from "./useQuantBenchmark";
import { resolveModelSetting } from "@/lib/modelId";
import {
  DEFAULT_LATENCY_PRESET,
  R2T2_CHUNK_MS_DEFAULT,
  effectiveLatencyMs,
  isContinuousLatency,
} from "@/lib/streamingLatency";

import { ModelStateEvent } from "@/lib/types/events";
import type { JSX } from "@solidjs/web";

type StatusPanel = "model" | "quant" | "latency" | "backend";

interface ModelSelectorProps {
  onError?: (error: string) => void;
}

const ModelSelector = (props: ModelSelectorProps): JSX.Element => {
  const { t } = useTranslation();
  const modelStore = useModelStore();
  const settingsStore = useSettingsStore();

  const [modelStatus, setModelStatus] = createSignal<ModelStatus>("unloaded");
  const [modelError, setModelError] = createSignal<string | null>(null);
  const [openPanel, setOpenPanel] = createSignal<StatusPanel | null>(null);
  const [pendingModelId, setPendingModelId] = createSignal<string | null>(null);
  let barRef: HTMLDivElement | null = null;
  const [quantVariants, setQuantVariants] = createSignal<QuantVariant[] | null>(
    null,
  );
  const [pendingDownloads, setPendingDownloads] = createSignal<Set<string>>(
    new Set(),
  );

  const benchmark = useQuantBenchmark();

  const displayModelId = createMemo(
    () => pendingModelId() || modelStore.currentModel,
  );

  // This model's pin in `per_model_backends`; absent means `auto`, which is
  // what the panel shows as selected. Inherits across quant variants.
  const currentBackend = (): ModelBackendSetting =>
    resolveModelSetting<ModelBackendSetting>(
      settingsStore.settings?.per_model_backends,
      displayModelId(),
      "auto",
    );

  const togglePanel = (panel: StatusPanel) => {
    setOpenPanel(openPanel() === panel ? null : panel);
  };

  // The compute tracks the store read; the apply only issues the command and
  // writes signals.
  createEffect(
    () => modelStore.currentModel,
    (current) => {
      if (!current) {
        setModelStatus("none");
        return;
      }
      // A reply for the previous model that lands after a switch must not
      // overwrite the status of the new one.
      let cancelled = false;
      commands
        .getTranscriptionModelStatus()
        .then((statusResult) => {
          if (cancelled) return;
          if (statusResult.status === "ok") {
            setModelStatus(
              statusResult.data === current ? "ready" : "unloaded",
            );
          } else {
            console.error("Failed to check model status:", statusResult.error);
            setModelStatus("error");
            setModelError(t("modelSelector.errors.checkStatusFailed"));
          }
        })
        .catch(() => {
          if (cancelled) return;
          setModelStatus("error");
          setModelError(t("modelSelector.errors.checkStatusFailed"));
        });
      return () => {
        cancelled = true;
      };
    },
  );

  createEffect(
    () => undefined,
    () => {
      // The post-download auto-select waits 500 ms; a timer still pending at
      // unmount must not switch the model behind a component that is gone.
      const autoSelectTimers = new Set<ReturnType<typeof setTimeout>>();
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
              setModelError(error || t("modelSelector.errors.loadFailed"));
              setPendingModelId(null);
              break;
            case "unloaded":
              setModelStatus("unloaded");
              setModelError(null);
              break;
          }
        },
      );
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
          const timer = setTimeout(async () => {
            autoSelectTimers.delete(timer);
            try {
              const isRecording = await commands.isRecording();
              if (!isRecording) {
                setPendingModelId(modelId);
                setModelError(null);
                const success = await selectModel(modelId);
                if (!success) setPendingModelId(null);
              }
            } catch {}
          }, 500);
          autoSelectTimers.add(timer);
        },
      );
      return () => {
        for (const timer of autoSelectTimers) clearTimeout(timer);
        autoSelectTimers.clear();
        modelStateUnlisten.then((fn) => fn());
        downloadCompleteUnlisten.then((fn) => fn());
      };
    },
  );

  createEffect(
    () => openPanel(),
    (panel) => {
      if (!panel) return;
      const handlePointerDown = (event: MouseEvent) => {
        if (barRef && !barRef.contains(event.target as Node))
          setOpenPanel(null);
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
    },
  );

  createEffect(
    () => displayModelId(),
    (id) => {
      if (!id || !id.includes("/")) {
        setQuantVariants(null);
        return;
      }
      let cancelled = false;
      commands.getModelQuantVariants(id).then((result) => {
        if (cancelled) return;
        setQuantVariants(result.status === "ok" ? (result.data ?? null) : null);
      });
      return () => {
        cancelled = true;
      };
    },
  );

  // Memos, not body reads: the body runs once, so these would freeze at
  // whatever the store held on mount.
  const currentModelInfo = createMemo(() =>
    modelStore.models.find((m) => m.id === displayModelId()),
  );
  const latencyKind = () => currentModelInfo()?.native_streaming_latency_kind;
  const currentPreset = createMemo(
    (): NativeStreamingLatencyPreset =>
      resolveModelSetting(
        settingsStore.settings?.native_streaming_latency_presets,
        displayModelId(),
        DEFAULT_LATENCY_PRESET,
      ),
  );

  // R2T2 is the one family whose latency is a continuous millisecond value;
  // the others snap to their trained settings. Keyed off the kind string the
  // backend serialises, not off the model id, so the app never has to know
  // the model's name to pick the right control.
  const isChunkMsKind = () => isContinuousLatency(latencyKind());
  const currentChunkMs = createMemo((): number =>
    resolveModelSetting<number>(
      settingsStore.settings?.native_streaming_chunk_ms,
      displayModelId(),
      R2T2_CHUNK_MS_DEFAULT,
    ),
  );

  // What the status bar shows for every family: the latency in ms.
  const currentLatencyMs = () =>
    effectiveLatencyMs(latencyKind(), currentPreset(), currentChunkMs()) ?? 0;

  const downloadPercentages = createMemo(() => {
    const map: Record<string, number> = {};
    for (const progress of Object.values(modelStore.downloadProgress)) {
      map[progress.model_id] = progress.percentage;
    }
    return map;
  });

  const currentVariant = createMemo(() =>
    quantVariants()?.find((variant) => variant.model_id === displayModelId()),
  );

  const downloadedVariantCount = createMemo(
    () =>
      quantVariants()?.filter((variant) =>
        modelStore.models.some(
          (m) => m.id === variant.model_id && m.is_downloaded,
        ),
      ).length ?? 0,
  );

  const measuredVariantCount = createMemo(
    () =>
      quantVariants()?.filter(
        (variant) => benchmark.results()[variant.model_id] !== undefined,
      ).length ?? 0,
  );

  const handleModelSelect = async (modelId: string) => {
    setPendingModelId(modelId);
    setModelError(null);
    setOpenPanel(null);
    const success = await selectModel(modelId);
    if (!success) {
      setPendingModelId(null);
      setModelStatus("error");
      setModelError(t("modelSelector.errors.switchFailed"));
      props.onError?.(t("modelSelector.errors.switchFailed"));
    }
  };

  const handleQuantSelect = (variant: QuantVariant) => {
    if (variant.model_id === displayModelId()) return;
    setPendingModelId(variant.model_id);
    setModelError(null);
    void selectModel(variant.model_id).then((success) => {
      if (!success) {
        setPendingModelId(null);
        setModelStatus("error");
        setModelError(t("modelSelector.errors.switchFailed"));
        props.onError?.(t("modelSelector.errors.switchFailed"));
      }
    });
  };

  const handleQuantDownload = (variant: QuantVariant) => {
    setPendingDownloads((prev) => new Set(prev).add(variant.model_id));
    void commands.downloadModelQuant(variant.model_id).then((result) => {
      if (result.status === "error")
        console.error("Failed to download quant:", result.error);
      setPendingDownloads((prev) => {
        if (!prev.has(variant.model_id)) return prev;
        const next = new Set(prev);
        next.delete(variant.model_id);
        return next;
      });
    });
  };

  // Neither handler closes the popover: a latency is set by dragging and
  // nudging, and closing after every release would fight the user. The value
  // applies to the next stream either way.
  const handleLatencyPresetSelect = (preset: NativeStreamingLatencyPreset) => {
    if (displayModelId())
      void settingsStore.setLatencyPreset(displayModelId(), preset);
  };

  const handleChunkMsSelect = (chunkMs: number) => {
    if (displayModelId())
      void settingsStore.setLatencyChunkMs(displayModelId(), chunkMs);
  };

  const getModelDisplayText = (): string => {
    const verifyingKeys = Object.keys(modelStore.verifyingModels);
    if (verifyingKeys.length > 0) {
      if (verifyingKeys.length === 1) {
        const modelId = verifyingKeys[0];
        const model = modelStore.models.find((m) => m.id === modelId);
        if (!model) return t("modelSelector.verifyingGeneric");
        return t("modelSelector.verifying", {
          modelName: getTranslatedModelName(model, t),
        });
      } else {
        return t("modelSelector.verifyingGeneric");
      }
    }
    const progressValues = Object.values(modelStore.downloadProgress);
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
    switch (modelStatus()) {
      case "ready": {
        const info = currentModelInfo();
        return info
          ? getTranslatedModelName(info, t)
          : t("modelSelector.modelReady");
      }
      case "loading": {
        const info = currentModelInfo();
        return info
          ? t("modelSelector.loading", {
              modelName: getTranslatedModelName(info, t),
            })
          : t("modelSelector.loadingGeneric");
      }
      case "error":
        return modelError() || t("modelSelector.modelError");
      case "unloaded": {
        const info = currentModelInfo();
        return info
          ? getTranslatedModelName(info, t)
          : t("modelSelector.modelUnloaded");
      }
      case "none":
        return t("modelSelector.noModelDownloadRequired");
      default: {
        const info = currentModelInfo();
        return info
          ? getTranslatedModelName(info, t)
          : t("modelSelector.modelUnloaded");
      }
    }
  };

  const getDisplayStatus = (): ModelStatus => {
    if (Object.keys(modelStore.verifyingModels).length > 0) return "verifying";
    if (Object.keys(modelStore.downloadProgress).length > 0)
      return "downloading";
    return modelStatus();
  };

  const referenceRecording = () => benchmark.referenceRecording();
  const canBenchmark = () =>
    !!displayModelId() &&
    !!referenceRecording() &&
    downloadedVariantCount() > 0;

  // The stream benchmark replays the reference recording through the primary
  // model's own stream, so it needs that model resident, the recording to
  // replay, and a family that actually exposes `stream_begin`. It takes the
  // same engine slot as the quantization benchmark, so it reuses `isBusy`.
  const canStreamBenchmark = () => {
    const info = currentModelInfo();
    return (
      !!info &&
      info.is_downloaded &&
      info.supports_streaming &&
      !!referenceRecording()
    );
  };
  const streamBenchmarkTitle = (): string => {
    const info = currentModelInfo();
    if (!canStreamBenchmark()) {
      return info && !info.supports_streaming
        ? t("modelSelector.benchmark.streamNotSupported")
        : t("modelSelector.benchmark.noRecording");
    }
    return t("modelSelector.benchmark.method", { runs: benchmark.runs() });
  };

  // A stored result belongs to the model that produced it: switching models
  // must not leave the previous family's numbers on screen as if they were
  // the new model's.
  const streamResultForModel = () => {
    const result = benchmark.streamResult();
    return result && result.modelId === displayModelId() ? result : null;
  };

  /**
   * The latency the run was measured at, in the same ms the latency slider
   * shows, so a result always says which setting it timed. `null` when the
   * model has no latency control, so there is nothing to name.
   */
  /** Audio horizon of the first text, or a dash when it only came at finalize. */
  const formatFirstText = (ms: number | null): number | string =>
    ms == null ? "—" : Math.round(ms);

  const streamLatencyLabel = (latencyMs: number | null): string | null =>
    latencyMs == null
      ? null
      : t("modelSelector.latencySelector.ms", { ms: latencyMs });

  // Unique per mount: the popover exists once, but the id must not collide if
  // the selector is ever rendered twice.
  const runsSelectId = createUniqueId();
  const runCountOptions = Array.from(
    { length: MAX_BENCHMARK_RUNS - MIN_BENCHMARK_RUNS + 1 },
    (_, index) => MIN_BENCHMARK_RUNS + index,
  );

  const quantSubtitle = () => {
    const recording = referenceRecording();
    return recording
      ? t("modelSelector.benchmark.reference", {
          when: new Date((recording.timestamp ?? 0) * 1000).toLocaleString(
            undefined,
            { dateStyle: "short", timeStyle: "short" },
          ),
        })
      : t("modelSelector.benchmark.noRecording");
  };

  return (
    <div
      ref={(el) => {
        barRef = el;
      }}
      class="flex min-w-0 flex-nowrap items-center gap-2"
    >
      <div class="relative shrink-0">
        <ModelStatusButton
          status={getDisplayStatus()}
          displayText={getModelDisplayText()}
          isDropdownOpen={openPanel() === "model"}
          onClick={() => togglePanel("model")}
          // The box marks the loaded model as R2T2 — selected *and* ready, the
          // two halves of "active". `getDisplayStatus()` is `ready` only when no
          // model is verifying or downloading, which is exactly when
          // `getModelDisplayText` shows the model's own name, so gating on it
          // keeps the box on a name and never on a "downloading 42%" line.
          highlight={isChunkMsKind() && getDisplayStatus() === "ready"}
        />
        <Show when={openPanel() === "model"}>
          <ModelDropdown
            models={modelStore.models}
            currentModelId={displayModelId()}
            onModelSelect={handleModelSelect}
          />
        </Show>
      </div>
      <Show when={quantVariants() && quantVariants()!.length > 0}>
        <StatusBarPopover
          open={openPanel() === "quant"}
          onToggle={() => togglePanel("quant")}
          label={t("modelSelector.quantPicker.pillLabel", {
            quant:
              currentVariant()?.quant ?? t("modelSelector.quantPicker.title"),
          })}
          title={t("modelSelector.quantPicker.title")}
          subtitle={quantSubtitle()}
          // Wide enough for the streaming benchmark's result line at the
          // bottom, which states five numbers and the latency measured.
          widthClass="w-[min(36rem,calc(100vw-2rem))]"
          trigger={
            <>
              {benchmark.isBusy() ? (
                <LoaderCircle class="h-3 w-3 shrink-0 animate-spin text-text/50" />
              ) : (
                <span
                  class={`h-2 w-2 shrink-0 rounded-full ${getQuantColor(currentVariant()?.quant ?? "")}`}
                />
              )}
              <span class="max-w-20 truncate">
                {currentVariant()?.quant ??
                  t("modelSelector.quantPicker.title")}
              </span>
            </>
          }
          headerAction={
            <div class="flex shrink-0 items-center gap-1.5">
              <button
                type="button"
                onClick={() =>
                  displayModelId() && void benchmark.runAll(displayModelId())
                }
                disabled={!canBenchmark() || benchmark.isBusy()}
                title={t("modelSelector.benchmark.method", {
                  runs: benchmark.runs(),
                })}
                class="flex shrink-0 items-center gap-1 rounded-md border border-mid-gray/25 bg-mid-gray/10 px-2 py-1 text-[11px] font-medium text-text/75 transition-colors hover:bg-mid-gray/20 hover:text-text/90 disabled:cursor-not-allowed disabled:opacity-40"
              >
                {benchmark.isRunningAll() ? (
                  <>
                    <LoaderCircle class="h-3 w-3 animate-spin" />
                    <span class="tabular-nums">
                      {t("modelSelector.benchmark.runProgress", {
                        done: measuredVariantCount(),
                        total: downloadedVariantCount(),
                      })}
                    </span>
                  </>
                ) : (
                  <>
                    <Zap class="h-3 w-3" />
                    <span>{t("modelSelector.benchmark.runAll")}</span>
                  </>
                )}
              </button>
              {/* Second measurement, same menu: how fast the model keeps up
                  with speech in its native stream, at the latency setting the
                  panel next door is currently on. */}
              <button
                type="button"
                onClick={() =>
                  displayModelId() && void benchmark.runStream(displayModelId())
                }
                disabled={!canStreamBenchmark() || benchmark.isBusy()}
                title={streamBenchmarkTitle()}
                class="flex shrink-0 items-center gap-1 rounded-md border border-mid-gray/25 bg-mid-gray/10 px-2 py-1 text-[11px] font-medium text-text/75 transition-colors hover:bg-mid-gray/20 hover:text-text/90 disabled:cursor-not-allowed disabled:opacity-40"
              >
                {benchmark.isRunningStream() ? (
                  <>
                    <LoaderCircle class="h-3 w-3 animate-spin" />
                    <span class="tabular-nums">
                      {benchmark.streamProgress()?.warmup
                        ? t("modelSelector.benchmark.warmup")
                        : t("modelSelector.benchmark.runProgress", {
                            done: benchmark.streamProgress()?.index ?? 0,
                            total:
                              benchmark.streamProgress()?.total ??
                              benchmark.runs(),
                          })}
                    </span>
                  </>
                ) : (
                  <>
                    <Radio class="h-3 w-3" />
                    <span>{t("modelSelector.benchmark.runStream")}</span>
                  </>
                )}
              </button>
            </div>
          }
        >
          <QuantizationPanel
            variants={quantVariants() ?? []}
            models={modelStore.models}
            currentModelId={displayModelId()}
            downloadPercentages={downloadPercentages()}
            pendingDownloads={pendingDownloads()}
            benchmark={benchmark}
            onSelect={handleQuantSelect}
            onDownload={handleQuantDownload}
          />
          {/* The method note states the measurement contract in full: the
              first pass is a warmup that is thrown away, and only the runs
              chosen below are averaged. */}
          <div class="border-t border-mid-gray/20 px-3 py-1.5 text-[10px] leading-snug whitespace-normal text-text/40">
            <p>
              {t("modelSelector.benchmark.method", { runs: benchmark.runs() })}
            </p>
            <BenchmarkRunsSelect
              id={runsSelectId}
              value={benchmark.runs()}
              options={runCountOptions}
              disabled={benchmark.isBusy()}
              onChange={benchmark.setRuns}
            />
            {/* The stream benchmark's own numbers: how fast the model keeps
                up with speech (compute xrt), the p95/max cost of a feed, how
                far behind the first word lands, and which latency setting was
                on when it was measured. */}
            <Show when={streamResultForModel()}>
              {(result) => (
                <p
                  class="mt-1.5 border-t border-mid-gray/15 pt-1.5 text-text/55"
                  title={result().streamExtension}
                >
                  {t("modelSelector.benchmark.streamResult", {
                    factor: result().computeXrt.toFixed(1),
                    p95: Math.round(result().feedP95Ms),
                    max: Math.round(result().feedMaxMs),
                    first: formatFirstText(result().firstTextAudioMs),
                  })}
                  <Show when={streamLatencyLabel(result().latencyMs)}>
                    {(label) =>
                      ` ${t("modelSelector.benchmark.streamAt", { latency: label() })}`
                    }
                  </Show>
                </p>
              )}
            </Show>
            <Show when={benchmark.streamError()}>
              {(error) => (
                <p class="mt-1.5 text-error" title={error()}>
                  {t("modelSelector.benchmark.failed")}
                </p>
              )}
            </Show>
          </div>
        </StatusBarPopover>
      </Show>
      <Show when={latencyKind()}>
        <StatusBarPopover
          open={openPanel() === "latency"}
          onToggle={() => togglePanel("latency")}
          label={t("modelSelector.latencySelector.pillLabel", {
            ms: currentLatencyMs(),
          })}
          title={t("modelSelector.latencySelector.title")}
          widthClass="w-[min(17rem,calc(100vw-2rem))]"
          trigger={
            <>
              <Gauge class="h-3 w-3 shrink-0 text-text/50" />
              <span class="max-w-24 truncate">
                {t("modelSelector.latencySelector.ms", {
                  ms: currentLatencyMs(),
                })}
              </span>
            </>
          }
        >
          {/* One control for every family, in ms: continuous for R2T2,
              snapping to the trained settings for the others. */}
          <Show when={latencyKind()}>
            {(kind) => (
              <StreamingLatencyControl
                kind={kind()}
                preset={currentPreset()}
                chunkMs={currentChunkMs()}
                onPreset={handleLatencyPresetSelect}
                onChunkMs={handleChunkMsSelect}
              />
            )}
          </Show>
        </StatusBarPopover>
      </Show>
      {/* The one pinned control: it says which backend this model would load
          on, and is where that is changed. Only the models this build can
          actually load a given backend on appear inside, so a choice the
          engine would refuse cannot be made here. */}
      <StatusBarPopover
        open={openPanel() === "backend"}
        onToggle={() => togglePanel("backend")}
        label={backendLabel(currentBackend(), t)}
        title={t("modelSelector.backend.title")}
        widthClass="w-[min(20rem,calc(100vw-2rem))]"
        trigger={
          <>
            <Cpu class="h-3 w-3 shrink-0 text-text/50" />
            <span class="max-w-24 truncate">
              {backendLabel(currentBackend(), t)}
            </span>
          </>
        }
      >
        <ModelBackendPanel
          modelId={displayModelId()}
          selected={currentBackend()}
          onSelect={(backend) =>
            displayModelId() &&
            void settingsStore.setModelBackend(displayModelId()!, backend)
          }
        />
      </StatusBarPopover>
      <DownloadProgressDisplay
        downloadProgress={modelStore.downloadProgress}
        downloadStats={modelStore.downloadStats}
      />
    </div>
  );
};

export default ModelSelector;
