import { createSignal, onSettled, type Accessor } from "solid-js";
import { listen } from "@tauri-apps/api/event";
import { sessionToast as toast } from "@/lib/sessionToast";
import { commands, events } from "@/bindings";
import type { HistoryEntry } from "@/bindings";
import type { BenchmarkProgressEvent } from "@/lib/types/events";
import { readPref, writePref } from "@/lib/appIdentity";

export interface BenchmarkSample {
  avgMs: number;
  audioSecs: number | null;
}
export interface BenchmarkRunProgress {
  index: number;
  total: number;
  /** True while the discarded warmup pass is running (`index` is 0 then). */
  warmup: boolean;
}

/**
 * One completed native-streaming benchmark, flattened from
 * `StreamingBenchmarkResult`. Everything is already averaged over the timed
 * runs — the warmup never reaches it.
 */
export interface StreamBenchmarkSample {
  modelId: string;
  /** Audio seconds per compute second — the headline "keeps up with speech" number. */
  computeXrt: number;
  avgComputeMs: number;
  feedP95Ms: number;
  feedMaxMs: number;
  avgFinalizeMs: number;
  /** Audio horizon of the first text, in ms of audio; null when it only appeared at finalize. */
  firstTextAudioMs: number | null;
  audioSecs: number;
  runs: number;
  feedChunkMs: number;
  /** Debug of the resolved stream extension — shown as a tooltip. */
  streamExtension: string;
  /**
   * Streaming latency the run measured, in ms of audio (the value the latency
   * slider shows); null for a model with no latency control.
   */
  latencyMs: number | null;
  /** Lookahead part of `latencyMs`; 0 for R2T2. */
  lookaheadMs: number | null;
}

export interface QuantBenchmark {
  results: Accessor<Record<string, BenchmarkSample>>;
  errors: Accessor<Record<string, string>>;
  activeModelId: Accessor<string | null>;
  activeRun: Accessor<BenchmarkRunProgress | null>;
  isRunningAll: Accessor<boolean>;
  isBusy: Accessor<boolean>;
  referenceRecording: Accessor<HistoryEntry | null>;
  /** Timed runs to average, after the one discarded warmup run. */
  runs: Accessor<number>;
  setRuns: (runs: number) => void;
  runAll: (modelId: string) => Promise<void>;
  runOne: (modelId: string) => Promise<void>;
  /** Last completed streaming benchmark (null until one finishes). */
  streamResult: Accessor<StreamBenchmarkSample | null>;
  /** Failure of the last streaming attempt, cleared by the next one. */
  streamError: Accessor<string | null>;
  /** Warmup / run counter of an in-flight streaming benchmark. */
  streamProgress: Accessor<BenchmarkRunProgress | null>;
  isRunningStream: Accessor<boolean>;
  runStream: (modelId: string) => Promise<void>;
}

/**
 * Defaults of the timed-run count — the runs averaged *after* the warmup pass,
 * which is always run first and never counted. Mirrors
 * `BENCHMARK_DEFAULT_RUNS` / `BENCHMARK_MIN_RUNS` / `BENCHMARK_MAX_RUNS` in
 * `managers/transcription.rs`, which clamp again: the UI is not a trust
 * boundary.
 */
export const DEFAULT_BENCHMARK_RUNS = 5;
export const MIN_BENCHMARK_RUNS = 2;
export const MAX_BENCHMARK_RUNS = 10;

/** Preference suffix for the chosen run count (see `readPref` in `appIdentity`). */
const RUNS_PREF = "benchmark.runs";

const clampRuns = (value: number): number =>
  Math.min(MAX_BENCHMARK_RUNS, Math.max(MIN_BENCHMARK_RUNS, Math.round(value)));

const readRunsPref = (): number => {
  const raw = Number.parseInt(readPref(RUNS_PREF) ?? "", 10);
  return Number.isFinite(raw) ? clampRuns(raw) : DEFAULT_BENCHMARK_RUNS;
};

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
  const [runs, setRunsSignal] = createSignal<number>(readRunsPref());
  const [referenceRecording, setReferenceRecording] =
    createSignal<HistoryEntry | null>(null);
  const [streamResult, setStreamResult] =
    createSignal<StreamBenchmarkSample | null>(null);
  const [streamError, setStreamError] = createSignal<string | null>(null);
  const [streamProgress, setStreamProgress] =
    createSignal<BenchmarkRunProgress | null>(null);
  const [isRunningStream, setIsRunningStream] = createSignal(false);

  const setRuns = (value: number) => {
    const next = clampRuns(value);
    setRunsSignal(next);
    writePref(RUNS_PREF, String(next));
  };

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
              total: total_runs ?? runs(),
              warmup: true,
            });
            break;
          case "warmup_completed":
            // The discarded first pass is over; every run from here is timed
            // and counted, so the label can switch to "n / total".
            setActiveRun((current) => ({
              index: current?.index ?? 0,
              total: total_runs ?? current?.total ?? runs(),
              warmup: false,
            }));
            break;
          case "run_completed":
            setActiveRun({
              index: run_index ?? 0,
              total: total_runs ?? runs(),
              warmup: false,
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
          // The streaming benchmark shares this channel but is a different
          // measurement, so its events never touch the quantization result
          // map above — and the quantization events leave the stream state
          // alone. The terminal events only clear the progress counter here;
          // `runStream` owns the awaiting side.
          case "stream_started":
            setIsRunningStream(true);
            setStreamProgress({
              index: 0,
              total: total_runs ?? runs(),
              warmup: true,
            });
            break;
          case "stream_warmup_completed":
            setStreamProgress({
              index: 0,
              total: total_runs ?? runs(),
              warmup: false,
            });
            break;
          case "stream_run_completed":
            setStreamProgress({
              index: run_index ?? 0,
              total: total_runs ?? runs(),
              warmup: false,
            });
            break;
          case "stream_completed":
          case "stream_failed":
            setStreamProgress(null);
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
  // mount-time `false` and the double-run guards could never fire. The
  // streaming benchmark is in the same guard: it takes the same engine slot,
  // so the two kinds must never overlap.
  const isBusy = () => isRunningAll() || isRunningOne() || isRunningStream();

  const runAll = async (modelId: string) => {
    if (isBusy()) return;
    setIsRunningAll(true);
    setResults({});
    setErrors({});
    const result = await commands.benchmarkModelQuantizations(modelId, runs());
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
    setActiveRun({ index: 0, total: runs(), warmup: true });
    setErrors((prev) => {
      if (!(modelId in prev)) return prev;
      const next = { ...prev };
      delete next[modelId];
      return next;
    });
    const result = await commands.benchmarkSingleQuantization(modelId, runs());
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

  const runStream = async (modelId: string) => {
    if (isBusy()) return;
    setIsRunningStream(true);
    setStreamError(null);
    setStreamResult(null);
    setStreamProgress({ index: 0, total: runs(), warmup: true });
    // The backend reads the latency settings when the run starts, so whatever
    // the status-bar latency panel was last set to is what gets measured.
    const result = await commands.benchmarkModelStreaming(modelId, runs());
    if (!mounted) return;
    if (result.status === "ok") {
      const data = result.data;
      setStreamResult({
        modelId: data.model_id,
        computeXrt: data.compute_xrt ?? 0,
        avgComputeMs: data.avg_compute_ms ?? 0,
        feedP95Ms: data.feed_p95_ms ?? 0,
        feedMaxMs: data.feed_max_ms ?? 0,
        avgFinalizeMs: data.avg_finalize_ms ?? 0,
        firstTextAudioMs: data.avg_first_text_audio_ms,
        audioSecs: data.audio_secs ?? 0,
        runs: data.runs,
        feedChunkMs: data.feed_chunk_ms,
        streamExtension: data.stream_extension,
        latencyMs: data.latency_ms,
        lookaheadMs: data.lookahead_ms,
      });
    } else {
      setStreamError(result.error);
      toast.error(result.error);
    }
    setIsRunningStream(false);
    setStreamProgress(null);
  };

  return {
    results,
    errors,
    activeModelId,
    activeRun,
    isRunningAll,
    isBusy,
    referenceRecording,
    runs,
    setRuns,
    runAll,
    runOne,
    streamResult,
    streamError,
    streamProgress,
    isRunningStream,
    runStream,
  };
}
