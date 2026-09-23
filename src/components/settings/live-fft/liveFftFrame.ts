// Decoder of the Live FFT page's binary frame (`live_fft_frame`, a raw
// `tauri::ipc::Response` outside specta). Framework-free, so the layout
// contract with `live_fft/mod.rs` is pinned by liveFftFrame.test.ts.
//
// Little-endian, 8 header words, then `bins_len` f32, then `features_len`
// f32:
//   [0] seq u32 (0 = no frame yet)
//   [1] flags u32: bit0 SILENT, bit1 ACTIVE, bit2 HAS_FEATURES
//   [2] bins_len u32 (0 in a header-only reply)
//   [3] axis_version u32 (matches `LiveFftAxis.version`)
//   [4] peak_hz f32
//   [5] peak_value f32 (output units)
//   [6] dsp_us f32
//   [7] features_len u32 (0 or 8)
// A reply to `knownSeq == seq` is the header alone: nothing new.

export const FFT_FRAME_HEADER_WORDS = 8;
export const FFT_FLAG_SILENT = 1;
export const FFT_FLAG_ACTIVE = 2;
export const FFT_FLAG_HAS_FEATURES = 4;
/** Number of spectral features a frame carries when they are on. */
export const FFT_FEATURE_COUNT = 8;

/** The eight spectral features (Plugin_FFT's Info CHOP channels). */
export interface SpectralFeatures {
  centroidHz: number;
  rolloffHz: number;
  flatness: number;
  flux: number;
  rmsDb: number;
  bassDb: number;
  midDb: number;
  highDb: number;
}

export interface DecodedFftFrame {
  seq: number;
  active: boolean;
  silent: boolean;
  axisVersion: number;
  peakHz: number;
  peakValue: number;
  dspUs: number;
  /** Zero-copy view on the reply; empty in a header-only reply. */
  bins: Float32Array;
  features: SpectralFeatures | null;
}

const finite = (v: number): number => (Number.isFinite(v) ? v : 0);

/**
 * Map a reply onto typed views without copying the bins. Returns null for a
 * reply too short for the lengths its header announces.
 */
export function decodeFftFrame(buffer: ArrayBuffer): DecodedFftFrame | null {
  const headerBytes = FFT_FRAME_HEADER_WORDS * 4;
  if (buffer.byteLength < headerBytes) return null;
  const words = new Uint32Array(buffer, 0, FFT_FRAME_HEADER_WORDS);
  const floats = new Float32Array(buffer, 0, FFT_FRAME_HEADER_WORDS);
  const binsLength = words[2];
  const featuresLength = words[7];
  const featuresOffset = headerBytes + binsLength * 4;
  if (buffer.byteLength < featuresOffset + featuresLength * 4) return null;
  const flags = words[1];
  let features: SpectralFeatures | null = null;
  if (
    (flags & FFT_FLAG_HAS_FEATURES) !== 0 &&
    featuresLength >= FFT_FEATURE_COUNT
  ) {
    const f = new Float32Array(buffer, featuresOffset, FFT_FEATURE_COUNT);
    features = {
      centroidHz: f[0],
      rolloffHz: f[1],
      flatness: f[2],
      flux: f[3],
      rmsDb: f[4],
      bassDb: f[5],
      midDb: f[6],
      highDb: f[7],
    };
  }
  return {
    seq: words[0],
    active: (flags & FFT_FLAG_ACTIVE) !== 0,
    silent: (flags & FFT_FLAG_SILENT) !== 0,
    axisVersion: words[3],
    peakHz: finite(floats[4]),
    peakValue: finite(floats[5]),
    dspUs: finite(floats[6]),
    bins: new Float32Array(buffer, headerBytes, binsLength),
    features,
  };
}

/** Poll period for an analysis rate: twice the frame rate, never below 8 ms. */
export function framePollPeriodMs(updateRateHz: number): number {
  return Math.max(8, Math.round(500 / Math.max(1, updateRateHz)));
}
