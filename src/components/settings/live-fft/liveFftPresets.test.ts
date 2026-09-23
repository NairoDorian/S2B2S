// Standalone assert check, run by `bun test` (bunfig.toml roots discovery at
// src/). Pins the page's presets to Plugin_FFT's rules: a quality preset
// overrides exactly its four fields and never the output size, a starting
// point keeps the source and threading choices, and the frontend defaults
// mirror the Rust ones.
import { test } from "bun:test";
import assert from "node:assert";
import {
  LIVE_FFT_DEFAULTS,
  LIVE_FFT_PRESETS,
  LIVE_FFT_QUALITY_PRESETS,
  applyPreset,
  applyQualityPreset,
  matchingQualityPreset,
  type ResolvedLiveFft,
} from "./liveFftPresets";

const QUALITY_FIELDS = [
  "fft_size",
  "warp_interpolation",
  "kaiser_beta_mode",
  "warp_aggregation",
] as const;

// Deliberately far from every preset, so each patched field really changes.
const custom: ResolvedLiveFft = {
  ...LIVE_FFT_DEFAULTS,
  source: "processed",
  raw_bins: false,
  fft_size: 65536,
  warp_interpolation: "linear",
  kaiser_beta_mode: "manual",
  warp_aggregation: "off",
  output_bins_mode: "fixed",
  output_bins: 777,
  zero_padding: false,
  async_analysis: false,
  spectral_features: true,
  show_vad: true,
  window_samples: 1234,
};

test("each quality preset changes only its four fields", () => {
  for (const preset of LIVE_FFT_QUALITY_PRESETS) {
    assert.deepStrictEqual(
      Object.keys(preset.patch).toSorted(),
      [...QUALITY_FIELDS].toSorted(),
      preset.id,
    );
    const next = applyQualityPreset(custom, preset.patch);
    for (const key of Object.keys(custom) as (keyof ResolvedLiveFft)[]) {
      if ((QUALITY_FIELDS as readonly string[]).includes(key)) {
        assert.strictEqual(
          next[key],
          preset.patch[key as keyof typeof preset.patch],
          `${preset.id}.${key}`,
        );
      } else {
        assert.strictEqual(
          next[key],
          custom[key],
          `${preset.id} touched ${key}`,
        );
      }
    }
    // The v2.11 rule, spelled out: the output size and its inputs stay.
    assert.strictEqual(next.output_bins, 777);
    assert.strictEqual(next.output_bins_mode, "fixed");
    assert.strictEqual(next.raw_bins, false);
    assert.strictEqual(next.zero_padding, false);
    // Pure: the current settings are not mutated.
    assert.strictEqual(custom.fft_size, 65536);
  }
});

test("the matching-preset helper finds the preset just applied", () => {
  assert.strictEqual(matchingQualityPreset(custom), null);
  for (const preset of LIVE_FFT_QUALITY_PRESETS) {
    const next = applyQualityPreset(custom, preset.patch);
    assert.strictEqual(matchingQualityPreset(next), preset.id);
    // Changing any one of the four fields leaves the preset.
    const other: Pick<ResolvedLiveFft, (typeof QUALITY_FIELDS)[number]> = {
      fft_size: 1024,
      warp_interpolation:
        next.warp_interpolation === "cubic" ? "linear" : "cubic",
      kaiser_beta_mode: "manual",
      warp_aggregation: "off",
    };
    for (const key of QUALITY_FIELDS) {
      assert.notStrictEqual(other[key], next[key], `${preset.id}.${key}`);
      const off = { ...next, [key]: other[key] } as ResolvedLiveFft;
      assert.strictEqual(
        matchingQualityPreset(off),
        null,
        `${preset.id} without ${key}`,
      );
    }
  }
});

test("starting points keep the source and threading choices", () => {
  for (const preset of LIVE_FFT_PRESETS) {
    const next = applyPreset(custom, preset.patch);
    assert.strictEqual(next.source, custom.source, preset.id);
    assert.strictEqual(next.async_analysis, custom.async_analysis, preset.id);
    assert.strictEqual(
      next.spectral_features,
      custom.spectral_features,
      preset.id,
    );
    assert.strictEqual(next.show_vad, custom.show_vad, preset.id);
    // Everything else is the defaults plus the patch.
    assert.strictEqual(
      next.window_samples,
      preset.patch.window_samples ?? LIVE_FFT_DEFAULTS.window_samples,
      preset.id,
    );
    assert.strictEqual(
      next.fft_size,
      preset.patch.fft_size ?? LIVE_FFT_DEFAULTS.fft_size,
      preset.id,
    );
  }
});

// `LiveFftSettings::default()` in src-tauri/src/settings.rs, copied by hand:
// if this fails, one side changed without the other.
const RUST_DEFAULTS: ResolvedLiveFft = {
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
  // Rust's `0.707` (the Butterworth shelf Q), written as a ratio so the
  // approx-constant lint does not ask for Math.SQRT1_2 (not the same float).
  eq_q: 707 / 1000,
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

test("LIVE_FFT_DEFAULTS mirrors the Rust Default", () => {
  assert.deepStrictEqual(LIVE_FFT_DEFAULTS, RUST_DEFAULTS);
});
