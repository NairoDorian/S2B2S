import { createSignal, onSettled, type Accessor } from "solid-js";
import { listen } from "@tauri-apps/api/event";
import { sessionToast as toast } from "@/lib/sessionToast";
import { commands, events } from "@/bindings";
import type { HistoryEntry } from "@/bindings";
import type { BenchmarkProgressEvent } from "@/lib/types/events";

export interface BenchmarkSample {
  avgMs: number;
  audioSecs: number | null;
}
export interface BenchmarkRunProgress {
  index: number;
  total: number;
}

export interface QuantBenchmark {
  results: Accessor<Record<string, BenchmarkSample>>;
  errors: Accessor<Record<string, string>>;
  activeModelId: Accessor<string | null>;
  activeRun: Accessor<BenchmarkRunProgress | null>;
  isRunningAll: Accessor<boolean>;
  isBusy: Accessor<boolean>;
  referenceRecording: Accessor<HistoryEntry | null>;
  runAll: (modelId: string) => Promise<void>;
  runOne: (modelId: string) => Promise<void>;
}

export const DEFAULT_TIMED_RUNS = 3;

export function useQuantBenchmark(): QuantBenchmark {
  const [results, setResults] = createSignal<Record<string, BenchmarkSample>>(
    {},
  );
  const [errors, setErrors] = createSignal<Record<string, string>>({});
  const [activeModelId, setActiveModelId] = createSignal<string | null>(null);
  const [activeRun, setActiveRun] = createSignal<BenchmarkRunProgress | null>(
    null,
  );
  const [isRunningAll, setIsRunningAll] = createSignal(false);
  const [isRunningOne, setIsRunningOne] = createSignal(false);
  const [referenceRecording, setReferenceRecording] =
    createSignal<HistoryEntry | null>(null);

  let mounted = true;
  onSettled(() => {
    return () => {
      mounted = false;
    };
  });

  const refreshReferenceRecording = async () => {
    const result = await commands.getLatestRecordingInfo();
    if (!mounted) return;
    if (result.status === "ok") setReferenceRecording(result.data ?? null);
  };

  onSettled(() => {
    void refreshReferenceRecording();
    const unlisten = events.historyUpdatePayload.listen(() => {
      void refreshReferenceRecording();
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  });

  onSettled(() => {
    const unlisten = listen<BenchmarkProgressEvent>(
      "benchmark-progress",
      (event) => {
        const {
          event_type,
          model_id,
          avg_time_ms,
          audio_secs,
          run_index,
          total_runs,
          error,
        } = event.payload;
        switch (event_type) {
          case "benchmark_started":
            setIsRunningAll(true);
            setResults({});
            setErrors({});
            setActiveModelId(null);
            setActiveRun(null);
            break;
          case "variant_started":
            if (model_id) setActiveModelId(model_id);
            setActiveRun({
              index: 0,
              total: total_runs ?? DEFAULT_TIMED_RUNS,
            });
            break;
          case "run_completed":
            setActiveRun({
              index: run_index ?? 0,
              total: total_runs ?? DEFAULT_TIMED_RUNS,
            });
            break;
          case "variant_completed":
            if (model_id && avg_time_ms != null) {
              setResults((prev) => ({
                ...prev,
                [model_id]: {
                  avgMs: avg_time_ms,
                  audioSecs: audio_secs ?? null,
                },
              }));
              setErrors((prev) => {
                if (!(model_id in prev)) return prev;
                const next = { ...prev };
                delete next[model_id];
                return next;
              });
            }
            setActiveModelId((current) =>
              current === model_id ? null : current,
            );
            setActiveRun(null);
            break;
          case "variant_error":
            if (model_id && error)
              setErrors((prev) => ({ ...prev, [model_id]: error }));
            setActiveModelId((current) =>
              current === model_id ? null : current,
            );
            setActiveRun(null);
            break;
          case "benchmark_completed":
          case "benchmark_failed":
            setIsRunningAll(false);
            setActiveModelId(null);
            setActiveRun(null);
            break;
        }
      },
    );
    return () => {
      void unlisten.then((fn) => fn());
    };
  });

  // An accessor, not a body-level boolean: the component body runs once, so
  // a plain `isRunningAll() || isRunningOne()` there would freeze at the
  // mount-time `false` and the double-run guards could never fire.
  const isBusy = () => isRunningAll() || isRunningOne();

  const runAll = async (modelId: string) => {
    if (isBusy()) return;
    setIsRunningAll(true);
    setResults({});
    setErrors({});
    const result = await commands.benchmarkModelQuantizations(modelId);
    if (!mounted) return;
    if (result.status === "ok") {
      setResults(
        Object.fromEntries(
          result.data.map((entry) => [
            entry.model_id,
            {
              avgMs: entry.avg_time_ms ?? 0,
              audioSecs: entry.audio_secs ?? null,
            },
          ]),
        ),
      );
    } else {
      toast.error(result.error);
    }
    setIsRunningAll(false);
    setActiveModelId(null);
    setActiveRun(null);
  };

  const runOne = async (modelId: string) => {
    if (isBusy()) return;
    setIsRunningOne(true);
    setActiveModelId(modelId);
    setActiveRun({ index: 0, total: DEFAULT_TIMED_RUNS });
    setErrors((prev) => {
      if (!(modelId in prev)) return prev;
      const next = { ...prev };
      delete next[modelId];
      return next;
    });
    const result = await commands.benchmarkSingleQuantization(modelId);
    if (!mounted) return;
    if (result.status === "ok") {
      setResults((prev) => ({
        ...prev,
        [modelId]: {
          avgMs: result.data.avg_time_ms ?? 0,
          audioSecs: result.data.audio_secs ?? null,
        },
      }));
    } else {
      setErrors((prev) => ({ ...prev, [modelId]: result.error }));
      toast.error(result.error);
    }
    setIsRunningOne(false);
    setActiveModelId(null);
    setActiveRun(null);
  };

  return {
    results,
    errors,
    activeModelId,
    activeRun,
    isRunningAll,
    isBusy,
    referenceRecording,
    runAll,
    runOne,
  };
}
