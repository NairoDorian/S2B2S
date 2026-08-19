import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import { commands, events } from "@/bindings";
import type { HistoryEntry } from "@/bindings";
import type { BenchmarkProgressEvent } from "@/lib/types/events";

/** A completed measurement for one quantization variant. */
export interface BenchmarkSample {
  /** Mean wall-clock time of the timed runs, in milliseconds. */
  avgMs: number;
  /** Length of the reference recording, for the real-time factor. */
  audioSecs: number | null;
}

/** Which timed run of the active variant is in flight. */
export interface BenchmarkRunProgress {
  index: number;
  total: number;
}

export interface QuantBenchmark {
  /** Completed measurements, keyed by variant model id. */
  results: Record<string, BenchmarkSample>;
  /** Variants whose run failed, keyed by variant model id. */
  errors: Record<string, string>;
  /** Variant currently being measured, if any. */
  activeModelId: string | null;
  /** Run progress within the active variant. */
  activeRun: BenchmarkRunProgress | null;
  /** A "benchmark every downloaded quant" sweep is in flight. */
  isRunningAll: boolean;
  /** Any benchmark — sweep or single variant — is in flight. */
  isBusy: boolean;
  /** The recording the benchmark transcribes, or `null` if there is none yet. */
  referenceRecording: HistoryEntry | null;
  /** Benchmark every downloaded quant of the given model's family. */
  runAll: (modelId: string) => Promise<void>;
  /** Benchmark a single downloaded quant. */
  runOne: (modelId: string) => Promise<void>;
}

/** Timed runs the backend averages per variant (`BENCHMARK_TIMED_RUNS` in transcription.rs). */
export const DEFAULT_TIMED_RUNS = 3;

/**
 * Owns the quantization benchmark: kicks off runs, tracks live progress from
 * the backend's `benchmark-progress` events, and keeps the reference recording
 * in sync with the history.
 *
 * Results are keyed by variant model id (not by quant name) so switching model
 * families never shows one family's timings against another's variants.
 */
export function useQuantBenchmark(): QuantBenchmark {
  const [results, setResults] = useState<Record<string, BenchmarkSample>>({});
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [activeModelId, setActiveModelId] = useState<string | null>(null);
  const [activeRun, setActiveRun] = useState<BenchmarkRunProgress | null>(null);
  const [isRunningAll, setIsRunningAll] = useState(false);
  const [isRunningOne, setIsRunningOne] = useState(false);
  const [referenceRecording, setReferenceRecording] =
    useState<HistoryEntry | null>(null);

  // Guards against a stale in-flight command resolving after unmount.
  const mountedRef = useRef(true);
  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  const refreshReferenceRecording = useCallback(async () => {
    const result = await commands.getLatestRecordingInfo();
    if (!mountedRef.current) return;
    if (result.status === "ok") {
      setReferenceRecording(result.data ?? null);
    }
  }, []);

  // The reference is always the newest completed recording, so re-read it
  // whenever the history changes rather than only once on mount.
  useEffect(() => {
    void refreshReferenceRecording();
    const unlisten = events.historyUpdatePayload.listen(() => {
      void refreshReferenceRecording();
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [refreshReferenceRecording]);

  useEffect(() => {
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
            if (model_id && error) {
              setErrors((prev) => ({ ...prev, [model_id]: error }));
            }
            setActiveModelId((current) =>
              current === model_id ? null : current,
            );
            setActiveRun(null);
            break;

          case "benchmark_completed":
          case "benchmark_failed":
            // Terminal for the whole run. Deliberately silent: whoever invoked
            // the command (runAll / runOne) surfaces the error, so toasting
            // here as well would report the same failure twice.
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
  }, []);

  const isBusy = isRunningAll || isRunningOne;

  const runAll = useCallback(
    async (modelId: string) => {
      if (isBusy) return;
      setIsRunningAll(true);
      setResults({});
      setErrors({});

      const result = await commands.benchmarkModelQuantizations(modelId);
      if (!mountedRef.current) return;

      if (result.status === "ok") {
        // The per-variant events already filled these in; reconcile anyway so a
        // dropped event can never leave a row stuck on its spinner.
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
    },
    [isBusy],
  );

  const runOne = useCallback(
    async (modelId: string) => {
      if (isBusy) return;
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
      if (!mountedRef.current) return;

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
    },
    [isBusy],
  );

  return useMemo(
    () => ({
      results,
      errors,
      activeModelId,
      activeRun,
      isRunningAll,
      isBusy,
      referenceRecording,
      runAll,
      runOne,
    }),
    [
      results,
      errors,
      activeModelId,
      activeRun,
      isRunningAll,
      isBusy,
      referenceRecording,
      runAll,
      runOne,
    ],
  );
}
