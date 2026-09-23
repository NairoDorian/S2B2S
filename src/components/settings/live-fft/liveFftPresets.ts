/* oxlint-disable oxc/approx-constant */
import type { LiveFftSettings } from "@/bindings";

/**
 * `LiveFftSettings` with every field present and non-null. specta exports
 * the Rust f32 fields as `number | null` and the `serde(default)` struct
 * with optional fields; the page works on this resolved form.
 */
export type ResolvedLiveFft = {
  [K in keyof LiveFftSettings]-?: NonNullable<LiveFftSettings[K]>;
};

/** Mirrors `LiveFftSettings::default()` in `src-tauri/src/settings.rs`. */
export const LIVE_FFT_DEFAULTS: ResolvedLiveFft = {
  source: "microphone",
  raw_bins: false,
  scale: "log",
  warp_interpolation: "linear",
  warp_aggregation: "peak",
  display_max_hz: 24000,
  output_bins_mode: "fixed",
  output_bins: 1024,
  warp_blend: 0.963,
  log_floor_hz: 20,
  window_length_mode: "samples",
  window_samples: 3175,
  window_ms: 72,
  zero_padding: true,
  fft_size: 16384,
  eq_enabled: false,
  high_shelf: true,
  low_shelf: true,
  high_gain_db: 6,
  high_cutoff_hz: 1000,
  low_gain_db: 0,
  low_cutoff_hz: 200,
  eq_q: 0.707,
  eq_amount: 1,
  window_type: "kaiser",
  kaiser_beta_mode: "manual",
  kaiser_beta: 15,
  weighting: "off",
  magnitude_norm: "coherent_gain",
  loudness_mode: "db",
  db_reference: "dbfs",
  db_range: 90,
  ballistics_enabled: true,
  ballistics_mode: "milliseconds",
  attack: 0,
  release: 0,
  attack_ms: 15,
  release_ms: 250,
  async_analysis: true,
  update_rate_hz: 30,
  spectral_features: false,
  show_vad: false,
};

export function resolveLiveFft(
  raw: LiveFftSettings | undefined | null,
): ResolvedLiveFft {
  const out: Record<string, unknown> = { ...LIVE_FFT_DEFAULTS };
  if (raw) {
    for (const [key, value] of Object.entries(raw)) {
      if (value !== null && value !== undefined) out[key] = value;
    }
  }
  return out as ResolvedLiveFft;
}

/** The FFT lengths the backend accepts (`FFT_SIZES` in settings.rs). */
export const FFT_SIZES = [1024, 2048, 4096, 8192, 16384, 32768, 65536];
/**
 * Output bin counts offered by the page in Fixed mode (backend range
 * 8…65536; a count above the transform's N/2+1 is interpolated).
 */
export const OUTPUT_BIN_CHOICES = [
  8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096, 8192, 16384, 32768, 65536,
];

export interface LiveFftPreset {
  id: "analyzer" | "voice" | "music" | "meter" | "chroma";
  patch: Partial<ResolvedLiveFft>;
}

/**
 * Starting points. Each preset is applied on top of the defaults (not the
 * current values), keeping only the source and threading choices.
 */
export const LIVE_FFT_PRESETS: LiveFftPreset[] = [
  { id: "analyzer", patch: {} },
  {
    id: "voice",
    patch: {
      scale: "mel",
      display_max_hz: 8000,
      log_floor_hz: 60,
      output_bins: 512,
      fft_size: 8192,
      window_length_mode: "milliseconds",
      window_ms: 40,
      weighting: "a",
      attack_ms: 10,
      release_ms: 300,
    },
  },
  {
    id: "music",
    patch: {
      scale: "log",
      warp_interpolation: "cubic",
      fft_size: 16384,
      output_bins: 2048,
      window_type: "blackman_harris",
      db_range: 100,
      release_ms: 400,
    },
  },
  {
    id: "meter",
    patch: {
      scale: "linear",
      warp_blend: 0,
      output_bins: 256,
      fft_size: 2048,
      window_length_mode: "milliseconds",
      window_ms: 20,
      update_rate_hz: 60,
      loudness_mode: "db_normalized",
      attack_ms: 5,
      release_ms: 120,
    },
  },
  {
    id: "chroma",
    patch: {
      scale: "chroma",
      warp_interpolation: "cubic",
      display_max_hz: 5000,
      log_floor_hz: 30,
      output_bins: 1024,
      fft_size: 32768,
      window_length_mode: "milliseconds",
      window_ms: 120,
    },
  },
];

export function applyPreset(
  current: ResolvedLiveFft,
  patch: Partial<ResolvedLiveFft>,
): ResolvedLiveFft {
  return {
    ...LIVE_FFT_DEFAULTS,
    ...patch,
    source: current.source,
    async_analysis: current.async_analysis,
    spectral_features: current.spectral_features,
    show_vad: current.show_vad,
  };
}

/** The fields a quality preset sets; everything else is left as it is. */
export type QualityPatch = Pick<
  ResolvedLiveFft,
  "fft_size" | "warp_interpolation" | "kaiser_beta_mode" | "warp_aggregation"
>;

export interface LiveFftQualityPreset {
  id: "visual60" | "visual120" | "analysis";
  patch: QualityPatch;
}

/**
 * Plugin_FFT's quality presets (catalog §1.6): transform length, warp
 * interpolation, Kaiser β mode and warp aggregation, tuned for a frame
 * budget. Unlike the starting points above they apply on top of the
 * current settings and never change the output bin count or its mode.
 */
export const LIVE_FFT_QUALITY_PRESETS: LiveFftQualityPreset[] = [
  {
    id: "visual60",
    patch: {
      fft_size: 8192,
      warp_interpolation: "cubic",
      kaiser_beta_mode: "auto",
      warp_aggregation: "peak",
    },
  },
  {
    id: "visual120",
    patch: {
      fft_size: 4096,
      warp_interpolation: "cubic",
      kaiser_beta_mode: "auto",
      warp_aggregation: "peak",
    },
  },
  {
    id: "analysis",
    patch: {
      fft_size: 32768,
      warp_interpolation: "linear",
      kaiser_beta_mode: "auto",
      warp_aggregation: "rms",
    },
  },
];

export function applyQualityPreset(
  current: ResolvedLiveFft,
  patch: QualityPatch,
): ResolvedLiveFft {
  return { ...current, ...patch };
}

/** Which quality preset the settings currently match, if any. */
export function matchingQualityPreset(
  current: ResolvedLiveFft,
): LiveFftQualityPreset["id"] | null {
  for (const preset of LIVE_FFT_QUALITY_PRESETS) {
    const p = preset.patch;
    if (
      current.fft_size === p.fft_size &&
      current.warp_interpolation === p.warp_interpolation &&
      current.kaiser_beta_mode === p.kaiser_beta_mode &&
      current.warp_aggregation === p.warp_aggregation
    ) {
      return preset.id;
    }
  }
  return null;
}
