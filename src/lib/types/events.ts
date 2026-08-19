export interface ModelStateEvent {
  event_type: string;
  model_id?: string;
  model_name?: string;
  error?: string;
}

/**
 * Live progress from a quantization benchmark run.
 *
 * `event_type` is one of `benchmark_started`, `variant_started`,
 * `run_completed`, `variant_completed`, `variant_error` or
 * `benchmark_completed`. Every other field is populated only for the events
 * that carry it.
 */
export interface BenchmarkProgressEvent {
  event_type: string;
  quant?: string;
  model_id?: string;
  /** Elapsed time of a single run (`run_completed`), or the average across all runs (`variant_completed`). */
  avg_time_ms?: number | null;
  /** Duration of the reference recording, for the real-time factor. */
  audio_secs?: number | null;
  /** 1-based index of the run that just finished (`run_completed` only). */
  run_index?: number | null;
  /** Total timed runs per variant, so the UI can render "2 / 3". */
  total_runs?: number | null;
  error?: string | null;
}

export interface RecordingErrorEvent {
  error_type: string;
  detail?: string;
}
