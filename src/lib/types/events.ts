export interface ModelStateEvent {
  event_type: string;
  model_id?: string;
  model_name?: string;
  error?: string;
}

export interface BenchmarkProgressEvent {
  event_type: string;
  quant?: string;
  model_id?: string;
  avg_time_ms?: number | null;
  error?: string | null;
}

export interface RecordingErrorEvent {
  error_type: string;
  detail?: string;
}
