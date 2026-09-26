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
 * `warmup_completed`, `run_completed`, `variant_completed`, `variant_error` or
 * `benchmark_completed`. Every other field is populated only for the events
 * that carry it.
 *
 * `warmup_completed` marks the end of the first pass, which is always
 * discarded and never averaged; `run_index` is 0 for it.
 *
 * The native-streaming benchmark shares this channel and payload with
 * distinct `event_type`s so a listener can tell the two measurements apart:
 * `stream_started`, `stream_warmup_completed`, `stream_run_completed`,
 * `stream_completed` and `stream_failed`. They follow the same rule — the
 * warmup replay is discarded, `run_index` is 0 for
 * `stream_warmup_completed`, and `stream_failed` carries `error`.
 */
export interface BenchmarkProgressEvent {
  event_type: string;
  quant?: string;
  model_id?: string;
  /** Elapsed time of a single run (`run_completed`), or the average across all runs (`variant_completed`). */
  avg_time_ms?: number | null;
  /** Duration of the reference recording, for the real-time factor. */
  audio_secs?: number | null;
  /** 1-based index of the run that just finished (`run_completed` only); 0 for `warmup_completed`. */
  run_index?: number | null;
  /** Number of timed runs averaged per variant — the discarded warmup is not part of this count. */
  total_runs?: number | null;
  error?: string | null;
}

export interface RecordingErrorEvent {
  error_type: string;
  detail?: string;
}
