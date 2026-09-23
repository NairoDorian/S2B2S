//! Pure DSP of the Live FFT page on a `realfft` real-to-complex transform.
//!
//! Stage order per frame:
//! FIFO window → window function straight from the FIFO's two slices into
//! the centre of the zero-padded frame (8-float aligned) → R2C FFT →
//! magnitude of only the bins the warp reads → full-scale DC/Nyquist fix →
//! psychoacoustic warp (linear or Catmull-Rom, memcpy when the grid is
//! exactly 1:1) with peak / RMS aggregation wherever an output bin owns two
//! or more FFT bins → equal-loudness weighting (measuring the dB reference
//! peak in the same pass when one is needed) → dB against the selected
//! reference (`log2` by an 8-segment quadratic, or the mantissa LUT without
//! AVX2) → attack/release ballistics (output and state in one pass) → one
//! peak search. Spectral features, when enabled, are read from the linear
//! magnitude. The EQ runs at ingest on new samples only, stateful and in
//! time order: re-filtering the whole window every frame would restart the
//! IIR from a stale state at every window start.
//!
//! A window that is digital silence skips the window, the transform and the
//! warp: its linear magnitude is exactly zero, so the frame is filled with
//! what the full chain would produce for zero (0, the dB floor, or 0 in the
//! normalised mode) and the AGC and the ballistics still run on it.
//!
//! Vectorisation: the loops are written so the compiler vectorises them
//! (zipped iterators, four independent lane arrays for every reduction).
//! Three kernels have an AVX2 + FMA form in [`x86`] — the dB conversion, the
//! warp interpolation and the spectral-feature sums — chosen at run time
//! (`is_x86_feature_detected!`, cached by std) with the portable form kept
//! as the reference and the fallback; each was kept only where an A/B
//! measurement showed it faster than the auto-vectorised Rust (the
//! Plugin_FFT v2.12 performance pass). The only table is the 8 KB
//! `20·log10` mantissa LUT of the portable dB path.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, LazyLock};
use std::time::Instant;

use realfft::num_complex::Complex;
use realfft::{RealFftPlanner, RealToComplex};

use crate::settings::{
    FftBallisticsMode, FftDbReference, FftKaiserBetaMode, FftLoudnessMode, FftMagnitudeNorm,
    FftOutputBinsMode, FftScale, FftWarpAggregation, FftWarpInterp, FftWeighting,
    FftWindowLengthMode, FftWindowType, LiveFftSettings, MAX_FFT_WINDOW_SAMPLES,
    MIN_FFT_WINDOW_SAMPLES,
};

/* ───────────────────────── 1. circular ring buffer ───────────────────────── */

/// Fixed-capacity FIFO of the most recent samples.
pub struct Fifo {
    data: Vec<f32>,
    capacity: usize,
    idx: usize,
    filled: usize,
}

impl Fifo {
    pub fn new(capacity: usize) -> Self {
        let mut fifo = Self {
            data: Vec::new(),
            capacity: 1,
            idx: 0,
            filled: 0,
        };
        fifo.resize(capacity);
        fifo
    }

    pub fn resize(&mut self, capacity: usize) {
        self.capacity = capacity.max(1);
        self.data.clear();
        self.data.resize(self.capacity, 0.0);
        self.idx = 0;
        self.filled = 0;
    }

    pub fn add(&mut self, signal: &[f32]) {
        let count = signal.len();
        if count == 0 {
            return;
        }
        if count >= self.capacity {
            self.data.copy_from_slice(&signal[count - self.capacity..]);
            self.idx = 0;
            self.filled = self.capacity;
            return;
        }
        let end = self.idx + count;
        if end <= self.capacity {
            self.data[self.idx..end].copy_from_slice(signal);
            self.idx = end % self.capacity;
        } else {
            let first = self.capacity - self.idx;
            self.data[self.idx..].copy_from_slice(&signal[..first]);
            self.data[..count - first].copy_from_slice(&signal[first..]);
            self.idx = count - first;
        }
        if self.filled < self.capacity {
            self.filled = (self.filled + count).min(self.capacity);
        }
    }

    /// The window without linearising it: `(leading zeros, older, newer)`.
    /// Read in that order it is exactly [`Fifo::get`]: oldest sample first,
    /// right-aligned behind zeros while the buffer is still filling.
    pub fn segments(&self) -> (usize, &[f32], &[f32]) {
        let zeros = self.capacity - self.filled;
        if self.filled < self.capacity {
            if self.idx >= self.filled {
                (zeros, &self.data[self.idx - self.filled..self.idx], &[])
            } else {
                let part1 = self.filled - self.idx;
                (
                    zeros,
                    &self.data[self.capacity - part1..],
                    &self.data[..self.idx],
                )
            }
        } else {
            (0, &self.data[self.idx..], &self.data[..self.idx])
        }
    }

    /// Linearised copy, oldest sample first; right-aligned and zero-padded
    /// while the buffer is still filling.
    #[cfg(test)]
    pub fn get(&self, out: &mut Vec<f32>) {
        let (zeros, a, b) = self.segments();
        out.clear();
        out.resize(zeros, 0.0);
        out.extend_from_slice(a);
        out.extend_from_slice(b);
    }
}

/* ───────────────────────── 2. RBJ shelving EQ ───────────────────────── */

/// One Direct Form II transposed biquad section.
#[derive(Clone, Copy, Debug)]
struct BiquadSection {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
    active: bool,
}

impl Default for BiquadSection {
    fn default() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            z1: 0.0,
            z2: 0.0,
            active: false,
        }
    }
}

/// Below this the filter state is flushed to zero: an IIR decaying into
/// denormals on silence costs ~100 cycles per operation, and there is no
/// FTZ/DAZ guard in safe Rust.
const DENORMAL_FLUSH: f32 = 1e-15;

impl BiquadSection {
    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        if !self.active {
            return x;
        }
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        if self.z1.abs() < DENORMAL_FLUSH {
            self.z1 = 0.0;
        }
        if self.z2.abs() < DENORMAL_FLUSH {
            self.z2 = 0.0;
        }
        y
    }

    fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }
}

/// RBJ cookbook high / low shelf design. A gain under 0.01 dB deactivates
/// the section (bypass, state cleared).
fn design_shelf(
    sample_rate: f64,
    is_high: bool,
    cutoff_hz: f64,
    gain_db: f64,
    q_factor: f64,
    sec: &mut BiquadSection,
) {
    if gain_db.abs() < 0.01 {
        sec.active = false;
        sec.reset();
        return;
    }
    let cutoff_hz = cutoff_hz.clamp(1.0, sample_rate * 0.49);
    let w0 = 2.0 * std::f64::consts::PI * cutoff_hz / sample_rate;
    let a = 10f64.powf(gain_db / 40.0);
    let alpha = w0.sin() / (2.0 * q_factor.max(0.01));
    let cos_w0 = w0.cos();
    let sqrt_a = a.sqrt();
    let s = if is_high { 1.0 } else { -1.0 };

    let b0 = a * ((a + 1.0) + s * (a - 1.0) * cos_w0 + 2.0 * sqrt_a * alpha);
    let b1 = -s * 2.0 * a * ((a - 1.0) + s * (a + 1.0) * cos_w0);
    let b2 = a * ((a + 1.0) + s * (a - 1.0) * cos_w0 - 2.0 * sqrt_a * alpha);
    let a0 = (a + 1.0) - s * (a - 1.0) * cos_w0 + 2.0 * sqrt_a * alpha;
    let a1 = s * 2.0 * ((a - 1.0) - s * (a + 1.0) * cos_w0);
    let a2 = (a + 1.0) - s * (a - 1.0) * cos_w0 - 2.0 * sqrt_a * alpha;

    sec.b0 = (b0 / a0) as f32;
    sec.b1 = (b1 / a0) as f32;
    sec.b2 = (b2 / a0) as f32;
    sec.a1 = (a1 / a0) as f32;
    sec.a2 = (a2 / a0) as f32;
    sec.active = true;
}

/// High + low shelf pair with a wet/dry blend.
pub struct BiquadEq {
    sample_rate: f64,
    high: BiquadSection,
    low: BiquadSection,
    design_key: Option<[f64; 5]>,
}

impl BiquadEq {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            sample_rate: sample_rate.max(1.0),
            high: BiquadSection::default(),
            low: BiquadSection::default(),
            design_key: None,
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        let sample_rate = sample_rate.max(1.0);
        if self.sample_rate != sample_rate {
            self.sample_rate = sample_rate;
            self.design_key = None;
        }
    }

    /// Re-design the shelves when a parameter changed; returns whether any
    /// section is active so the caller can skip the per-sample loop.
    pub fn update_and_check_active(
        &mut self,
        gain_db: f64,
        cutoff_hz: f64,
        low_gain_db: f64,
        low_cutoff_hz: f64,
        q_factor: f64,
        amount: f64,
    ) -> bool {
        let key = [gain_db, cutoff_hz, low_gain_db, low_cutoff_hz, q_factor];
        if self.design_key != Some(key) {
            design_shelf(
                self.sample_rate,
                true,
                cutoff_hz,
                gain_db,
                q_factor,
                &mut self.high,
            );
            design_shelf(
                self.sample_rate,
                false,
                low_cutoff_hz,
                low_gain_db,
                q_factor,
                &mut self.low,
            );
            self.design_key = Some(key);
        }
        (self.high.active || self.low.active) && amount > 0.0
    }

    /// Streaming (stateful) processing of a block of NEW samples in place:
    /// `x + amount · (eq(x) − x)`.
    pub fn process_block_in_place(&mut self, data: &mut [f32], amount: f64) {
        let amt = amount as f32;
        for x in data.iter_mut() {
            let input = *x;
            let filtered = self.low.process(self.high.process(input));
            *x = input + amt * (filtered - input);
        }
    }

    pub fn reset(&mut self) {
        self.high.reset();
        self.low.reset();
    }
}

/* ───────────────────────── 3. window generator ───────────────────────── */

/// Zeroth-order modified Bessel function (Abramowitz & Stegun 9.8.1/9.8.2).
pub fn bessel_i0(x: f64) -> f64 {
    let ax = x.abs();
    if ax < 3.75 {
        let y = (x / 3.75) * (x / 3.75);
        1.0 + y
            * (3.5156229
                + y * (3.0899424
                    + y * (1.2067492 + y * (0.2659732 + y * (0.0360768 + y * 0.0045813)))))
    } else {
        let y = 3.75 / ax;
        (ax.exp() / ax.sqrt())
            * (0.39894228
                + y * (0.01328592
                    + y * (0.00225319
                        + y * (-0.00157565
                            + y * (0.00916281
                                + y * (-0.02057706
                                    + y * (0.02635537 + y * (-0.01647633 + y * 0.00392377))))))))
    }
}

/// Kaiser β whose highest sidelobe sits `sidelobe_db` below the main lobe
/// (Kaiser & Schafer's fit): 80 dB → β ≈ 10.73, 114.3 dB → β ≈ 15.
pub fn kaiser_beta_for_sidelobe_db(sidelobe_db: f64) -> f64 {
    let a = sidelobe_db;
    let beta = if a > 60.0 {
        0.12438 * (a + 6.3)
    } else if a > 13.26 {
        0.76609 * (a - 13.26).powf(0.4) + 0.09834 * (a - 13.26)
    } else {
        0.0
    };
    beta.clamp(0.0, 100.0)
}

/// The β the Kaiser window is built with: the manual value, or the one that
/// puts the sidelobes at the dB range floor.
pub fn effective_kaiser_beta(p: &LiveFftSettings) -> f64 {
    match p.kaiser_beta_mode {
        FftKaiserBetaMode::Manual => f64::from(p.kaiser_beta),
        FftKaiserBetaMode::Auto => kaiser_beta_for_sidelobe_db(f64::from(p.db_range)),
    }
}

/// Fill `window` with `length` taps of the chosen window, normalised so that
/// `mean == 1` (coherent gain) or `sum == 2` (full scale: a sine of
/// amplitude A reads A in the one-sided spectrum).
pub fn generate_window(
    window_type: FftWindowType,
    kaiser_beta: f64,
    length: usize,
    norm: FftMagnitudeNorm,
    window: &mut Vec<f32>,
) {
    window.clear();
    window.resize(length, 0.0);
    if length == 0 {
        return;
    }
    let denom = if length > 1 { (length - 1) as f64 } else { 1.0 };
    let two_pi = 2.0 * std::f64::consts::PI;
    let kaiser_inv_i0 = 1.0 / bessel_i0(kaiser_beta);
    let mut sum = 0.0;
    for (n, tap) in window.iter_mut().enumerate() {
        let fn_ = n as f64;
        let w = match window_type {
            FftWindowType::Hann => 0.5 - 0.5 * (two_pi * fn_ / denom).cos(),
            FftWindowType::Hamming => 0.54 - 0.46 * (two_pi * fn_ / denom).cos(),
            FftWindowType::Blackman => {
                0.42 - 0.5 * (two_pi * fn_ / denom).cos()
                    + 0.08 * (2.0 * two_pi * fn_ / denom).cos()
            }
            FftWindowType::BlackmanHarris => {
                0.35875 - 0.48829 * (two_pi * fn_ / denom).cos()
                    + 0.14128 * (2.0 * two_pi * fn_ / denom).cos()
                    - 0.01168 * (3.0 * two_pi * fn_ / denom).cos()
            }
            FftWindowType::Rectangular => 1.0,
            FftWindowType::Kaiser => {
                let term = 2.0 * fn_ / denom - 1.0;
                let arg = (1.0 - term * term).max(0.0).sqrt();
                bessel_i0(kaiser_beta * arg) * kaiser_inv_i0
            }
        };
        *tap = w as f32;
        sum += w;
    }
    if sum > 0.0 {
        let target = match norm {
            FftMagnitudeNorm::FullScale => 2.0,
            FftMagnitudeNorm::CoherentGain => length as f64,
        };
        let scale = (target / sum) as f32;
        for tap in window.iter_mut() {
            *tap *= scale;
        }
    }
}

/* ───────────────────────── 4. psychoacoustic warp ───────────────────────── */

pub fn htk_hz_to_mel(hz: f64) -> f64 {
    2595.0 * (1.0 + hz / 700.0).log10()
}
pub fn htk_mel_to_hz(mel: f64) -> f64 {
    700.0 * (10f64.powf(mel / 2595.0) - 1.0)
}
/// The "ERB" warp. Note this is Moore & Glasberg's (1983) ERB *bandwidth*
/// polynomial (`6.23·f² + 93.39·f + 28.52`, Hz for f in kHz) used as a warp
/// with its own `hz / 123` unit, not the ERB-rate (Cam) scale that
/// integrates it: the axis it produces is a quadratic, roughly square-root
/// warp. Kept as is because the page's ERB axis is built on it.
pub fn erb_rate_glasberg(hz: f64) -> f64 {
    let x = hz / 123.0;
    6.230 * (x * x) + 93.390 * x + 28.520
}
/// Inverse of [`erb_rate_glasberg`] (the positive root of the quadratic).
pub fn erb_rate_to_hz(erb: f64) -> f64 {
    let (a, b, c) = (6.230, 93.390, 28.520);
    let x = (-b + (b * b - 4.0 * a * (c - erb)).max(0.0).sqrt()) / (2.0 * a);
    x * 123.0
}
/// Traunmüller (1990) Bark, with his low (< 2 Bark) and high (> 20.1 Bark)
/// corrections.
pub fn hz_to_bark(hz: f64) -> f64 {
    let mut z = (26.81 * hz) / (1960.0 + hz) - 0.53;
    if z < 2.0 {
        z += 0.15 * (2.0 - z);
    } else if z > 20.1 {
        z += 0.22 * (z - 20.1);
    }
    z
}
/// Exact inverse of [`hz_to_bark`]: both corrections are linear
/// (`0.85·z + 0.3` and `1.22·z − 4.422`) and undone before the main formula.
pub fn bark_to_hz(bark: f64) -> f64 {
    let mut z = bark;
    if z < 2.0 {
        z = (z - 0.3) / 0.85;
    } else if z > 20.1 {
        z = (z + 4.422) / 1.22;
    }
    let f = (1960.0 * (z + 0.53)) / (26.81 - (z + 0.53));
    f.max(0.0)
}
pub fn hz_to_chroma(hz: f64) -> f64 {
    12.0 * (hz.max(1e-5) / 440.0).log2() + 69.0
}
pub fn chroma_to_hz(chroma: f64) -> f64 {
    440.0 * 2f64.powf((chroma - 69.0) / 12.0)
}

/// Frequency of every output bin: the perceptual grid blended with a linear
/// one by `warp_blend`, clamped to `[0, fmax]`.
pub fn compute_target_hz_grid(
    scale: FftScale,
    fmax: f64,
    n_out: usize,
    warp_blend: f64,
    log_floor_hz: f64,
    target_hz: &mut Vec<f64>,
) {
    target_hz.clear();
    target_hz.resize(n_out, 0.0);
    if n_out == 0 {
        return;
    }
    let inv_denom = if n_out > 1 {
        1.0 / (n_out - 1) as f64
    } else {
        0.0
    };
    let perceptual = |i: usize| -> f64 {
        let frac = i as f64 * inv_denom;
        match scale {
            FftScale::Log => {
                let log_min = log_floor_hz.max(1.0).ln();
                let log_max = fmax.ln();
                (log_min + frac * (log_max - log_min)).exp()
            }
            FftScale::Mel => {
                let m_min = htk_hz_to_mel(0.0);
                let m_max = htk_hz_to_mel(fmax);
                htk_mel_to_hz(m_min + frac * (m_max - m_min))
            }
            FftScale::Erb => {
                let e_min = erb_rate_glasberg(0.0);
                let e_max = erb_rate_glasberg(fmax);
                erb_rate_to_hz(e_min + frac * (e_max - e_min))
            }
            FftScale::Bark => {
                let b_min = hz_to_bark(0.0);
                let b_max = hz_to_bark(fmax);
                bark_to_hz(b_min + frac * (b_max - b_min))
            }
            FftScale::Chroma => {
                let c_min = hz_to_chroma(20.0);
                let c_max = hz_to_chroma(fmax);
                chroma_to_hz(c_min + frac * (c_max - c_min))
            }
            FftScale::Melog => {
                let m_min = htk_hz_to_mel(0.0);
                let m_max = htk_hz_to_mel(fmax);
                let log_min = log_floor_hz.max(1.0).ln();
                let log_max = fmax.ln();
                0.5 * (htk_mel_to_hz(m_min + frac * (m_max - m_min))
                    + (log_min + frac * (log_max - log_min)).exp())
            }
            FftScale::Linear => frac * fmax,
        }
    };
    for (i, hz) in target_hz.iter_mut().enumerate() {
        let lin = i as f64 * inv_denom * fmax;
        let axis = (1.0 - warp_blend) * lin + warp_blend * perceptual(i);
        *hz = axis.clamp(0.0, fmax);
    }
}

/// `max(a, b)` as a select the compiler maps onto `maxps` (a NaN in `v`
/// never wins).
#[inline(always)]
fn vmax(v: f32, m: f32) -> f32 {
    if v > m { v } else { m }
}

/// Largest of 8 lanes, as a 3-step tree (vectorisable) rather than a
/// serial chain.
#[inline(always)]
fn hmax8(l: [f32; 8]) -> f32 {
    let a = [
        vmax(l[4], l[0]),
        vmax(l[5], l[1]),
        vmax(l[6], l[2]),
        vmax(l[7], l[3]),
    ];
    let b = [vmax(a[2], a[0]), vmax(a[3], a[1])];
    vmax(b[1], b[0])
}

/// Largest value of a slice (−∞ for an empty one; a NaN never wins).
/// Four independent 8-lane accumulators over 32 values per step: with one,
/// every step waited on the previous `maxps` (4-cycle latency) and the loop
/// was latency-bound. They are four separate arrays because one 32-lane
/// array measured slower than v2.11 (0.48–0.83×: the compiler did not keep
/// it in four registers). Short slices (the aggregation ranges) skip
/// straight to one 8-lane accumulator, and the lanes are reduced as a tree.
/// Measured against the v2.11 single chain: 2.8× at 1024 values, 3.8× at
/// 16385, 1.7–2.6× on 5–64-value ranges.
#[inline]
pub(super) fn max_of(values: &[f32]) -> f32 {
    let mut m0 = [f32::NEG_INFINITY; 8];
    let (chunks, rest) = values.as_chunks::<32>();
    if !chunks.is_empty() {
        let (mut m1, mut m2, mut m3) = (m0, m0, m0);
        for chunk in chunks {
            let (a, b, c, d) = (&chunk[0..8], &chunk[8..16], &chunk[16..24], &chunk[24..32]);
            for l in 0..8 {
                m0[l] = vmax(a[l], m0[l]);
                m1[l] = vmax(b[l], m1[l]);
                m2[l] = vmax(c[l], m2[l]);
                m3[l] = vmax(d[l], m3[l]);
            }
        }
        for l in 0..8 {
            m0[l] = vmax(vmax(m1[l], m0[l]), vmax(m3[l], m2[l]));
        }
    }
    let (chunks, remainder) = rest.as_chunks::<8>();
    for chunk in chunks {
        for (lane, &v) in m0.iter_mut().zip(chunk) {
            *lane = vmax(v, *lane);
        }
    }
    let mut max = hmax8(m0);
    for &v in remainder {
        max = vmax(v, max);
    }
    max
}

/// `Σ v²` of a short slice (the aggregation ranges, the 64-bin rolloff
/// blocks), in eight lanes. Up to ~64 values this measured faster than
/// [`sum_of_squares_wide`], whose set-up costs more than its chains save.
#[inline]
pub(super) fn sum_of_squares(values: &[f32]) -> f32 {
    let mut lanes = [0.0f32; 8];
    let (chunks, remainder) = values.as_chunks::<8>();
    for chunk in chunks {
        for (lane, &v) in lanes.iter_mut().zip(chunk) {
            *lane += v * v;
        }
    }
    let tail: f32 = remainder.iter().map(|v| v * v).sum();
    lanes.iter().sum::<f32>() + tail
}

/// `Σ v²` of a long slice (the analysis window): four independent 8-lane
/// accumulators over 32 values per step, then one, as in [`max_of`];
/// 1.7× [`sum_of_squares`] at 1024 values, 2.4× at 16385.
#[inline]
pub(super) fn sum_of_squares_wide(values: &[f32]) -> f32 {
    let mut s0 = [0.0f32; 8];
    let (chunks, rest) = values.as_chunks::<32>();
    if !chunks.is_empty() {
        let (mut s1, mut s2, mut s3) = (s0, s0, s0);
        for chunk in chunks {
            let (a, b, c, d) = (&chunk[0..8], &chunk[8..16], &chunk[16..24], &chunk[24..32]);
            for l in 0..8 {
                s0[l] += a[l] * a[l];
                s1[l] += b[l] * b[l];
                s2[l] += c[l] * c[l];
                s3[l] += d[l] * d[l];
            }
        }
        for l in 0..8 {
            s0[l] = (s0[l] + s1[l]) + (s2[l] + s3[l]);
        }
    }
    let (chunks, remainder) = rest.as_chunks::<8>();
    for chunk in chunks {
        for (lane, &v) in s0.iter_mut().zip(chunk) {
            *lane += v * v;
        }
    }
    let tail: f32 = remainder.iter().map(|v| v * v).sum();
    s0.iter().sum::<f32>() + tail
}

/// Re-maps the linear magnitude spectrum onto the perceptual grid.
pub struct PerceptualWarp {
    target_hz: Vec<f64>,
    i0: Vec<u32>,
    w: Vec<f32>,
    nlin: usize,
    max_index: usize,
    interp: FftWarpInterp,
    is_identity: bool,
    aggregation: FftWarpAggregation,
    /// Output bins that own two or more FFT bins, the first FFT bin each
    /// owns and how many (three parallel tables).
    agg_idx: Vec<u32>,
    agg_lo: Vec<u32>,
    agg_cnt: Vec<u32>,
    /// The interpolation covers `[0, interp_end)`: every output bin from
    /// there up is aggregated, so interpolating it would be overwritten.
    interp_end: usize,
}

impl Default for PerceptualWarp {
    fn default() -> Self {
        Self::new()
    }
}

impl PerceptualWarp {
    pub fn new() -> Self {
        Self {
            target_hz: Vec::new(),
            i0: Vec::new(),
            w: Vec::new(),
            nlin: 0,
            max_index: 0,
            interp: FftWarpInterp::Linear,
            is_identity: false,
            aggregation: FftWarpAggregation::Off,
            agg_idx: Vec::new(),
            agg_lo: Vec::new(),
            agg_cnt: Vec::new(),
            interp_end: 0,
        }
    }

    pub fn set_interpolation(&mut self, interp: FftWarpInterp) {
        self.interp = interp;
    }

    /// Takes effect at the next [`PerceptualWarp::build_tables`].
    pub fn set_aggregation(&mut self, aggregation: FftWarpAggregation) {
        self.aggregation = aggregation;
    }

    /// Highest linear bin the warp reads (cubic look-ahead and aggregated
    /// ranges included). Magnitudes above it need not be computed.
    pub fn max_linear_index(&self) -> usize {
        self.max_index
    }

    pub fn target_hz(&self) -> &[f64] {
        &self.target_hz
    }

    pub fn is_identity(&self) -> bool {
        self.is_identity
    }

    pub fn output_bins(&self) -> usize {
        self.i0.len()
    }

    /// Output bins formed by aggregation rather than interpolation.
    pub fn aggregated_bins(&self) -> usize {
        self.agg_idx.len()
    }

    /// The interpolation tables (`i0`, `w`), for the kernel tests.
    #[cfg(test)]
    pub(super) fn tables(&self) -> (&[u32], &[f32]) {
        (&self.i0, &self.w)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn build_tables(
        &mut self,
        scale: FftScale,
        fmax: f64,
        n_out: usize,
        nyquist: f64,
        warp_blend: f64,
        log_floor_hz: f64,
        nlin: usize,
    ) {
        compute_target_hz_grid(
            scale,
            fmax,
            n_out,
            warp_blend,
            log_floor_hz,
            &mut self.target_hz,
        );
        self.i0.clear();
        self.i0.resize(n_out, 0);
        self.w.clear();
        self.w.resize(n_out, 0.0);
        self.nlin = nlin;
        self.max_index = 0;
        let denom = if nlin > 1 { (nlin - 1) as f64 } else { 1.0 };
        let mut is_id = n_out == nlin;
        let max_i0 = nlin.saturating_sub(2);
        for i in 0..n_out {
            let mut frac = if nyquist > 0.0 {
                (self.target_hz[i] / nyquist) * denom
            } else {
                0.0
            };
            // Snap positions that are an integer up to rounding noise so an
            // exact 1:1 grid is detected as identity (memcpy bypass).
            let rounded = frac.round();
            if (frac - rounded).abs() < 1e-6 {
                frac = rounded;
            }
            let i0 = frac.floor().clamp(0.0, max_i0 as f64) as usize;
            let weight = (frac - i0 as f64).clamp(0.0, 1.0) as f32;
            self.i0[i] = i0 as u32;
            self.w[i] = weight;
            self.max_index = self.max_index.max((i0 + 2).min(nlin.saturating_sub(1)));
            if is_id && ((i0 as f64 + weight as f64) - i as f64).abs() > 1e-5 {
                is_id = false;
            }
        }
        self.is_identity = is_id;

        // Aggregation ranges: output bin i owns the FFT bins between the
        // midpoints to its neighbours (in linear-bin units),
        // `[ceil((p[i-1]+p[i])/2), floor((p[i]+p[i+1])/2)]`. Only bins that
        // own two or more are recorded; everywhere else the interpolated
        // value already reads the nearest FFT bins.
        self.agg_idx.clear();
        self.agg_lo.clear();
        self.agg_cnt.clear();
        if self.aggregation != FftWarpAggregation::Off
            && !is_id
            && n_out >= 2
            && nlin >= 2
            && nyquist > 0.0
        {
            let target = &self.target_hz;
            let pos = |i: usize| (target[i] / nyquist) * denom;
            let last = (nlin - 1) as f64;
            for i in 0..n_out {
                let lo_pos = if i == 0 {
                    pos(0)
                } else {
                    0.5 * (pos(i - 1) + pos(i))
                };
                let hi_pos = if i + 1 == n_out {
                    pos(i)
                } else {
                    0.5 * (pos(i) + pos(i + 1))
                };
                let lo = (lo_pos - 1e-9).ceil().max(0.0);
                let hi = (hi_pos + 1e-9).floor().min(last);
                if hi - lo < 1.0 {
                    continue;
                }
                let (lo, hi) = (lo as usize, hi as usize);
                self.agg_idx.push(i as u32);
                self.agg_lo.push(lo as u32);
                self.agg_cnt.push((hi - lo + 1) as u32);
                self.max_index = self.max_index.max(hi);
            }
        }
        // The trailing run of aggregated bins (on a perceptual axis: every
        // bin from where the output spacing passes two FFT bins to the top)
        // is overwritten by the aggregation, so the interpolation stops
        // where that run starts.
        self.interp_end = n_out;
        for &i in self.agg_idx.iter().rev() {
            if i as usize + 1 != self.interp_end {
                break;
            }
            self.interp_end -= 1;
        }
    }

    /// Fill `out` (resized to the output bin count only when it differs)
    /// with the warped magnitudes.
    pub fn apply(&self, linear: &[f32], out: &mut Vec<f32>) {
        let n_out = self.i0.len();
        if out.len() != n_out {
            out.clear();
            out.resize(n_out, 0.0);
        }
        if n_out == 0 {
            return;
        }
        if self.is_identity && linear.len() == n_out {
            out.copy_from_slice(linear);
            return;
        }
        if linear.len() < self.nlin || self.nlin < 2 {
            out.fill(0.0);
            return;
        }
        // With aggregation on, the bins from `interp_end` up are all
        // written by `apply_aggregation` below.
        let n_interp = if self.aggregation != FftWarpAggregation::Off {
            self.interp_end.min(n_out)
        } else {
            n_out
        };
        let (i0, w, dst) = (
            &self.i0[..n_interp],
            &self.w[..n_interp],
            &mut out[..n_interp],
        );
        match self.interp {
            FftWarpInterp::Cubic if linear.len() >= 4 => {
                #[cfg(target_arch = "x86_64")]
                if avx2_fma() {
                    // SAFETY: AVX2 + FMA are present; every `i0` is at most
                    // `nlin − 2` and `linear.len() >= nlin` (checked above).
                    unsafe { x86::warp_cubic(linear, i0, w, dst) };
                    self.apply_aggregation(linear, out);
                    return;
                }
                warp_cubic_portable(linear, i0, w, dst);
            }
            _ => {
                #[cfg(target_arch = "x86_64")]
                if avx2_fma() {
                    // SAFETY: as above.
                    unsafe { x86::warp_linear(linear, i0, w, dst) };
                    self.apply_aggregation(linear, out);
                    return;
                }
                warp_linear_portable(linear, i0, w, dst);
            }
        }
        self.apply_aggregation(linear, out);
    }

    /// Overwrite the coarse output bins with the peak (or RMS) of the FFT
    /// bins each owns. Runs after the interpolation, so the fine part of the
    /// axis keeps its interpolated values; the ranges do not overlap (except
    /// at a shared midpoint), so this reads every FFT bin at most once.
    fn apply_aggregation(&self, src: &[f32], dst: &mut [f32]) {
        if self.aggregation == FftWarpAggregation::Off || self.agg_idx.is_empty() {
            return;
        }
        let rms = self.aggregation == FftWarpAggregation::Rms;
        for ((&i, &lo), &cnt) in self.agg_idx.iter().zip(&self.agg_lo).zip(&self.agg_cnt) {
            let lo = lo as usize;
            let Some(slot) = dst.get_mut(i as usize) else {
                continue;
            };
            if lo >= src.len() {
                // Fewer bins than the tables expect; the bin may not have
                // been interpolated either (`interp_end`), so it is written.
                *slot = 0.0;
                continue;
            }
            let hi = (lo + cnt as usize).min(src.len());
            let range = &src[lo..hi];
            *slot = if rms {
                (sum_of_squares(range) / range.len() as f32).sqrt()
            } else {
                max_of(range)
            };
        }
    }
}

/// Linear interpolation `a + w·(b − a)` of every output bin: the reference
/// every other form of this kernel must match bit for bit (same operations
/// in the same order, no FMA). Precondition: every `i0 + 1 < src.len()`.
pub(super) fn warp_linear_portable(src: &[f32], i0: &[u32], w: &[f32], dst: &mut [f32]) {
    for ((d, &i0), &w) in dst.iter_mut().zip(i0).zip(w) {
        let i0 = i0 as usize;
        let a = src[i0];
        let b = src[i0 + 1];
        *d = a + w * (b - a);
    }
}

/// Catmull-Rom: `0.5·(2p1 + (−p0+p2)t + (2p0−5p1+4p2−p3)t² + (−p0+3p1−3p2+p3)t³)`,
/// clamped at zero (magnitudes cannot undershoot); the taps are clamped to
/// the spectrum. The bit-exact reference of the AVX2 kernel. Precondition:
/// `src.len() >= 4` and every `i0 + 1 < src.len()`.
pub(super) fn warp_cubic_portable(src: &[f32], i0: &[u32], w: &[f32], dst: &mut [f32]) {
    let last = src.len() - 1;
    for ((d, &i0), &t) in dst.iter_mut().zip(i0).zip(w) {
        let i0 = i0 as usize;
        let p0 = src[i0.saturating_sub(1)];
        let p1 = src[i0];
        let p2 = src[(i0 + 1).min(last)];
        let p3 = src[(i0 + 2).min(last)];
        *d = cubic_tap(p0, p1, p2, p3, t);
    }
}

#[inline(always)]
fn cubic_tap(p0: f32, p1: f32, p2: f32, p3: f32, t: f32) -> f32 {
    let v = 0.5
        * (2.0 * p1
            + (-p0 + p2) * t
            + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t * t
            + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t * t * t);
    v.max(0.0)
}

/* ───────────────────────── 5. equal-loudness weighting ───────────────────────── */

/// ITU-R 468 response `R_ITU(f)` before the standard +18.2 dB offset that
/// puts 1 kHz at 0 dB.
fn itu468_response(f: f64) -> f64 {
    let f2 = f * f;
    let f3 = f2 * f;
    let f4 = f2 * f2;
    let f5 = f4 * f;
    let f6 = f3 * f3;
    let h1 =
        -4.737338981378384e-24 * f6 + 2.043828333266122e-15 * f4 - 1.363894795463638e-7 * f2 + 1.0;
    let h2 = 1.306612257412824e-19 * f5 - 2.118150887518656e-11 * f3 + 5.559488023498642e-4 * f;
    (1.246332637532143e-4 * f) / (h1 * h1 + h2 * h2).max(1e-12).sqrt()
}

/// Linear weighting factor per output bin: A / C (IEC 61672) or ITU-R 468,
/// each normalised to unity at 1 kHz so they share one scale.
pub fn equal_loudness_curve(weighting: FftWeighting, freqs_hz: &[f64], weights: &mut Vec<f32>) {
    weights.clear();
    weights.resize(freqs_hz.len(), 1.0);
    if weighting == FftWeighting::Off {
        return;
    }
    let k2 = 12194.0f64 * 12194.0;
    let inv_ref = match weighting {
        FftWeighting::A => {
            let f1k = 1e6;
            let ra_1k = (k2 * (f1k * f1k))
                / ((f1k + 20.6 * 20.6)
                    * ((f1k + 107.7 * 107.7) * (f1k + 737.9 * 737.9)).sqrt()
                    * (f1k + k2));
            1.0 / ra_1k
        }
        FftWeighting::C => {
            let f1k = 1e6;
            let rc_1k = (k2 * f1k) / ((f1k + 20.6 * 20.6) * (f1k + k2));
            1.0 / rc_1k
        }
        FftWeighting::Itu468 => 1.0 / itu468_response(1000.0),
        FftWeighting::Off => 1.0,
    };
    for (w, &hz) in weights.iter_mut().zip(freqs_hz) {
        let f = hz.max(1e-5);
        let value = match weighting {
            FftWeighting::A => {
                let f2 = f * f;
                let num = k2 * (f2 * f2);
                let den = (f2 + 20.6 * 20.6)
                    * ((f2 + 107.7 * 107.7) * (f2 + 737.9 * 737.9)).sqrt()
                    * (f2 + k2);
                (num / den.max(1e-12)) * inv_ref
            }
            FftWeighting::C => {
                let f2 = f * f;
                let num = k2 * f2;
                let den = (f2 + 20.6 * 20.6) * (f2 + k2);
                (num / den.max(1e-12)) * inv_ref
            }
            FftWeighting::Itu468 => itu468_response(f) * inv_ref,
            FftWeighting::Off => 1.0,
        };
        *w = value as f32;
    }
}

/// `spectrum[i] *= curve[i]`, returning the largest product (−∞ for an
/// empty slice, as [`max_of`]): the weighting pass and the dB reference
/// peak in one read of the spectrum, with `max_of`'s four lane arrays.
/// Only a Frame Peak / AGC reference asks for it; dBFS never reads a peak.
pub(super) fn multiply_in_place_max(spectrum: &mut [f32], curve: &[f32]) -> f32 {
    let n = spectrum.len().min(curve.len());
    let (spectrum, curve) = (&mut spectrum[..n], &curve[..n]);
    let (s_chunks, s_rest) = spectrum.as_chunks_mut::<32>();
    let (c_chunks, c_rest) = curve.as_chunks::<32>();
    let mut m0 = [f32::NEG_INFINITY; 8];
    let (mut m1, mut m2, mut m3) = (m0, m0, m0);
    for (s, c) in s_chunks.iter_mut().zip(c_chunks) {
        for l in 0..8 {
            s[l] *= c[l];
            s[l + 8] *= c[l + 8];
            s[l + 16] *= c[l + 16];
            s[l + 24] *= c[l + 24];
            m0[l] = vmax(s[l], m0[l]);
            m1[l] = vmax(s[l + 8], m1[l]);
            m2[l] = vmax(s[l + 16], m2[l]);
            m3[l] = vmax(s[l + 24], m3[l]);
        }
    }
    for l in 0..8 {
        m0[l] = vmax(vmax(m1[l], m0[l]), vmax(m3[l], m2[l]));
    }
    let mut max = hmax8(m0);
    for (v, &w) in s_rest.iter_mut().zip(c_rest) {
        *v *= w;
        max = vmax(*v, max);
    }
    max
}

/* ───────────────────────── 6. decibels ───────────────────────── */

/// Bits of the mantissa the `20·log10` table is indexed by.
const DB_TABLE_BITS: u32 = 11;
const DB_TABLE_SIZE: usize = 1 << DB_TABLE_BITS;
/// `20·log10(2)`: the dB step of one binary exponent.
const DB_PER_OCTAVE: f32 = 6.020_6;

/// `20·log10(1 + (i + 0.5) / 2048)`: each mantissa bucket evaluated at its
/// centre, so the error is at most half a bucket (≈ 0.0021 dB). 8 KB, stays
/// in L1.
static DB_TABLE: LazyLock<[f32; DB_TABLE_SIZE]> = LazyLock::new(|| {
    let mut table = [0.0f32; DB_TABLE_SIZE];
    for (i, v) in table.iter_mut().enumerate() {
        *v = (20.0 * (1.0 + (i as f64 + 0.5) / DB_TABLE_SIZE as f64).log10()) as f32;
    }
    table
});

/// `20·log10(x)` from the IEEE-754 split: exponent × 6.0206 dB plus the
/// table entry of the top mantissa bits. Precondition: `x` is a positive
/// normal float (the dB stage clamps to 1e-12 first).
#[inline]
fn fast_db(x: f32, table: &[f32; DB_TABLE_SIZE]) -> f32 {
    let bits = x.to_bits();
    let exponent = (((bits >> 23) & 0xFF) as i32).wrapping_sub(127);
    let index = ((bits >> (23 - DB_TABLE_BITS)) as usize) & (DB_TABLE_SIZE - 1);
    exponent as f32 * DB_PER_OCTAVE + table[index]
}

/// `20·log10(x)` for a positive normal `x`, within 0.006 dB.
#[cfg(test)]
pub fn fast_20_log10(x: f32) -> f32 {
    fast_db(x, &DB_TABLE)
}

/// `log2(x)` without a table: the exponent plus an 8-segment quadratic of
/// the mantissa (Plugin_FFT v2.12 `FastLog2Seg`, the same coefficients).
/// Segment `j` is the top 3 mantissa bits; its quadratic in `u ∈ [0, 1/8)`
/// passes through `log2(1 + j/8 + u)` at the three Chebyshev nodes. Max
/// error 2.7e-5 in `log2` = 0.00016 dB (the LUT's is 0.0021 dB).
#[allow(clippy::excessive_precision)] // Plugin_FFT's published digits, kept verbatim
const LOG2_C0: [f32; 8] = [
    2.564_386_887e-05,
    1.699_432_731e-01,
    3.219_415_843e-01,
    4.594_418_406e-01,
    5.849_704_146e-01,
    7.004_460_096e-01,
    8.073_599_935e-01,
    9.068_947_434e-01,
];
#[allow(clippy::excessive_precision)] // Plugin_FFT's published digits, kept verbatim
const LOG2_C1: [f32; 8] = [
    1.438_983_321e+00,
    1.279_752_135e+00,
    1.152_207_255e+00,
    1.047_755_003e+00,
    9.606_496_096e-01,
    8.869_041_204e-01,
    8.236_659_169e-01,
    7.688_398_957e-01,
];
#[allow(clippy::excessive_precision)] // Plugin_FFT's published digits, kept verbatim
const LOG2_C2: [f32; 8] = [
    -6.398_096_681e-01,
    -5.120_694_041e-01,
    -4.190_979_004e-01,
    -3.493_308_127e-01,
    -2.956_413_627e-01,
    -2.534_430_921e-01,
    -2.196_758_091e-01,
    -1.922_342_032e-01,
];
/// `20·log10(2)` in f64: the dB of one octave of magnitude.
const DB_PER_OCTAVE_EXACT: f64 = 6.020_599_913_279_624;
const MIN_MAG: f32 = 1e-12;

/// [`LOG2_C0`]… for a positive normal `x` (the dB stage clamps to 1e-12).
#[inline(always)]
pub(super) fn fast_log2(x: f32) -> f32 {
    let bits = x.to_bits();
    let j = ((bits >> 20) & 7) as usize;
    let e = ((bits >> 23) as i32).wrapping_sub(127) as f32;
    let u = f32::from_bits((bits & 0x000F_FFFF) | 0x3F80_0000) - 1.0;
    (LOG2_C0[j] + u * (LOG2_C1[j] + u * LOG2_C2[j])) + e
}

/// The folded constants of one dB conversion (see [`convert_to_db`]).
#[derive(Clone, Copy)]
pub(super) struct DbParams {
    normalized: bool,
    /// dB (or normalised units) per octave of magnitude (the AVX2 form).
    #[cfg_attr(not(target_arch = "x86_64"), allow(dead_code))]
    k: f32,
    /// The reference offset, in the same units (the AVX2 form).
    #[cfg_attr(not(target_arch = "x86_64"), allow(dead_code))]
    c: f32,
    floor: f32,
    /// dB offset and `1 / top_db` for the table form.
    offset: f32,
    inv_top: f32,
}

impl DbParams {
    pub(super) fn new(mode: FftLoudnessMode, top_db: f64, inv_ref: f32) -> Self {
        let inv_ref = if inv_ref > 0.0 && inv_ref.is_finite() {
            inv_ref
        } else {
            1.0
        };
        // The reference offset exactly, once per call.
        let db_offset = 20.0 * f64::from(inv_ref).log10();
        let tdb = top_db.max(1e-6);
        let normalized = mode == FftLoudnessMode::DbNormalized;
        Self {
            normalized,
            k: if normalized {
                (DB_PER_OCTAVE_EXACT / tdb) as f32
            } else {
                DB_PER_OCTAVE_EXACT as f32
            },
            c: if normalized {
                (db_offset / tdb + 1.0) as f32
            } else {
                db_offset as f32
            },
            floor: -(top_db as f32),
            offset: db_offset as f32,
            inv_top: (1.0 / tdb) as f32,
        }
    }
}

/// In place: `20·log10(x · inv_ref)` floored at `−top_db`, or that mapped
/// onto `[0, 1]` for the normalised mode. `inv_ref` is `1 / reference`.
/// Every constant is folded into one multiply-add per bin:
/// `dB = log2(x)·6.0206 + 20·log10(inv_ref)`, and the normalised value
/// `(dB + top_db) / top_db` is `log2(x)·(6.0206 / top_db) + (offset / top_db + 1)`
/// clamped to `[0, 1]` (`dB >= floor` is exactly `norm >= 0`).
pub fn convert_to_db(mode: FftLoudnessMode, top_db: f64, inv_ref: f32, spectrum: &mut [f32]) {
    if mode == FftLoudnessMode::Off || spectrum.is_empty() {
        return;
    }
    let p = DbParams::new(mode, top_db, inv_ref);
    #[cfg(target_arch = "x86_64")]
    if avx2_fma() {
        // SAFETY: AVX2 + FMA are present.
        unsafe { x86::convert_to_db(p, spectrum) };
        return;
    }
    convert_to_db_portable(p, spectrum);
}

/// [`convert_to_db`] without intrinsics (every non-AVX2 target).
/// The mantissa table: without `vpermps` the three coefficient lookups of
/// [`fast_log2`] cost more than the one table gather (measured 0.44–0.62×).
pub(super) fn convert_to_db_portable(p: DbParams, spectrum: &mut [f32]) {
    let table: &[f32; DB_TABLE_SIZE] = &DB_TABLE;
    if p.normalized {
        for v in spectrum.iter_mut() {
            let db = (fast_db(v.max(MIN_MAG), table) + p.offset).max(p.floor);
            *v = ((db - p.floor) * p.inv_top).clamp(0.0, 1.0);
        }
    } else {
        for v in spectrum.iter_mut() {
            *v = (fast_db(v.max(MIN_MAG), table) + p.offset).max(p.floor);
        }
    }
}

/* ───────────────────────── 7. ballistics ───────────────────────── */

/// Time constant (ms) → per-frame coefficient for a frame delta (ms); tau =
/// time to reach 63 %.
pub fn coef_from_ms(time_ms: f64, dt_ms: f64) -> f32 {
    if time_ms <= 0.0 || dt_ms <= 0.0 {
        return 0.0;
    }
    (-dt_ms / time_ms).exp().clamp(0.0, 0.999) as f32
}

/// Below this a linear-magnitude envelope is flushed to zero: a release
/// decaying towards silence otherwise ends in denormals.
const BALLISTICS_FLUSH: f32 = 1e-20;

/// Asymmetric attack / release envelope, in place: `prev += f · (cur − prev)`
/// with `f` chosen per bin by the sign of the difference, and `out` set to
/// the new envelope in the same pass. `prev` is (re)seeded with `out` when it
/// has the wrong size or both coefficients are zero. `flush_tiny` (linear
/// magnitude only) zeroes values below 1e-20.
pub fn apply_ballistics(
    attack: f32,
    release: f32,
    out: &mut [f32],
    prev: &mut Vec<f32>,
    flush_tiny: bool,
) {
    if prev.len() != out.len() || (attack <= 0.0 && release <= 0.0) {
        prev.clear();
        prev.extend_from_slice(out);
        return;
    }
    let att = 1.0 - attack.clamp(0.0, 0.999);
    let rel = 1.0 - release.clamp(0.0, 0.999);
    if flush_tiny {
        for (dst, state) in out.iter_mut().zip(prev.iter_mut()) {
            let diff = *dst - *state;
            let mut v = *state + if diff > 0.0 { att } else { rel } * diff;
            if v.abs() < BALLISTICS_FLUSH {
                v = 0.0;
            }
            *state = v;
            *dst = v;
        }
    } else {
        for (dst, state) in out.iter_mut().zip(prev.iter_mut()) {
            let diff = *dst - *state;
            let v = *state + if diff > 0.0 { att } else { rel } * diff;
            *state = v;
            *dst = v;
        }
    }
}

/* ───────────────────────── 8. spectral features ───────────────────────── */

/// Eight descriptors of one frame, from the linear magnitude.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpectralFeatures {
    /// Power-weighted mean frequency ("brightness").
    pub centroid_hz: f32,
    /// Frequency below which 85 % of the power lies.
    pub rolloff_hz: f32,
    /// Geometric / arithmetic mean of the power: 0 tonal … 1 noise.
    pub flatness: f32,
    /// Half-wave rectified magnitude increase over the previous frame,
    /// normalised by this frame's total magnitude (onset strength).
    pub flux: f32,
    /// RMS of the analysis window's samples, dBFS.
    pub rms_db: f32,
    /// `10·log10` of the power below 250 Hz, on the spectrum's own scale.
    pub bass_db: f32,
    /// … between 250 Hz and 4 kHz.
    pub mid_db: f32,
    /// … above 4 kHz.
    pub high_db: f32,
}

impl Default for SpectralFeatures {
    /// What a silent window reports.
    fn default() -> Self {
        Self {
            centroid_hz: 0.0,
            rolloff_hz: 0.0,
            flatness: 0.0,
            flux: 0.0,
            rms_db: -120.0,
            bass_db: -120.0,
            mid_db: -120.0,
            high_db: -120.0,
        }
    }
}

impl SpectralFeatures {
    /// The order the frame transport ships them in.
    pub fn to_array(self) -> [f32; 8] {
        [
            self.centroid_hz,
            self.rolloff_hz,
            self.flatness,
            self.flux,
            self.rms_db,
            self.bass_db,
            self.mid_db,
            self.high_db,
        ]
    }
}

/// The per-bin sums of [`compute_spectral_features`] over one band.
#[derive(Clone, Copy, Default)]
pub(super) struct FeatureSums {
    pw: f64,
    weighted: f64,
    mag: f64,
    log_db: f64,
    rise: f64,
}

/// Bins per f32 partial sum before it is flushed into the f64 total: keeps
/// a 32K-bin sum at ~1e-7 relative accuracy.
const FEATURE_FLUSH_BINS: usize = 256;

/// The sums over one band: `mag` (and `prev` when `PREV`) are the band's
/// slices, `k0` the absolute bin index of `mag[0]`. Eight f32 lanes per sum
/// (five independent chains), flushed to f64 every 256 bins; the partial
/// tail of the band is summed in f64. `log_db` is `20·log10(m)` through
/// [`fast_log2`]: here, unlike in the dB loop, it measured 2.0–2.2× faster
/// than the mantissa table (the lanes sum `log2`, scaled to dB per flush).
fn feature_sums<const PREV: bool>(mag: &[f32], prev: &[f32], k0: usize) -> FeatureSums {
    let mut a = FeatureSums::default();
    for (b, block) in mag.chunks(FEATURE_FLUSH_BINS).enumerate() {
        let start = b * FEATURE_FLUSH_BINS;
        let pblock = if PREV {
            &prev[start..start + block.len()]
        } else {
            &[][..]
        };
        let (chunks, rest) = block.as_chunks::<8>();
        // Without a previous frame the rise lanes read `mag` itself and are
        // never used (PREV is a constant), so one zipped loop serves both.
        let pchunks = if PREV {
            pblock.as_chunks::<8>().0
        } else {
            chunks
        };
        let mut kk: [f32; 8] = std::array::from_fn(|l| (k0 + start + l) as f32);
        let (mut spw, mut swk, mut smag, mut slg, mut srise) = (
            [0.0f32; 8],
            [0.0f32; 8],
            [0.0f32; 8],
            [0.0f32; 8],
            [0.0f32; 8],
        );
        for (m, p) in chunks.iter().zip(pchunks) {
            for l in 0..8 {
                let v = m[l];
                let pw = v * v;
                spw[l] += pw;
                swk[l] += pw * kk[l];
                smag[l] += v;
                slg[l] += fast_log2(v.max(1e-12));
                kk[l] += 8.0;
            }
            if PREV {
                for l in 0..8 {
                    srise[l] += (m[l] - p[l]).max(0.0);
                }
            }
        }
        let sum8 = |s: [f32; 8]| f64::from(s.iter().sum::<f32>());
        a.pw += sum8(spw);
        a.weighted += sum8(swk);
        a.mag += sum8(smag);
        a.log_db += sum8(slg) * DB_PER_OCTAVE_EXACT;
        a.rise += sum8(srise);
        let rbase = chunks.len() * 8;
        for (j, &v) in rest.iter().enumerate() {
            let pw = f64::from(v) * f64::from(v);
            a.pw += pw;
            a.weighted += pw * (k0 + start + rbase + j) as f64;
            a.mag += f64::from(v);
            a.log_db += f64::from(fast_log2(v.max(1e-12))) * DB_PER_OCTAVE_EXACT;
            if PREV {
                let d = v - pblock[rbase + j];
                if d > 0.0 {
                    a.rise += f64::from(d);
                }
            }
        }
    }
    a
}

/// Features of `mag` (linear magnitude, bin `k` at `k · bin_hz`). `time` is
/// the analysis window's samples in pieces (for the RMS), `n_time` its
/// length (zero padding while filling counts). `prev` holds the previous
/// frame's magnitude for the flux and is overwritten.
///
/// The loop is split at the two band edges (no per-bin band branch), the
/// sums run in f32 lanes flushed to f64 (`feature_sums`), and the rolloff
/// search skips whole 64-bin blocks below the 85 % point before scanning
/// the block that crosses it bin by bin. With AVX2 + FMA the band sums run
/// in [`x86::feature_sums`] (1.63× the portable form); in all, 13.9 → 3.0 µs
/// at 8193 bins.
pub fn compute_spectral_features(
    mag: &[f32],
    bin_hz: f64,
    time: &[&[f32]],
    n_time: usize,
    prev: &mut Vec<f32>,
) -> SpectralFeatures {
    #[cfg(target_arch = "x86_64")]
    if avx2_fma() {
        return spectral_features_impl::<true>(mag, bin_hz, time, n_time, prev);
    }
    spectral_features_impl::<false>(mag, bin_hz, time, n_time, prev)
}

/// `SIMD`: the band sums through the AVX2 kernel (the caller checked the
/// CPU); otherwise the portable [`feature_sums`].
pub(super) fn spectral_features_impl<const SIMD: bool>(
    mag: &[f32],
    bin_hz: f64,
    time: &[&[f32]],
    n_time: usize,
    prev: &mut Vec<f32>,
) -> SpectralFeatures {
    let mut f = SpectralFeatures::default();
    let n = mag.len();
    if n == 0 || bin_hz <= 0.0 {
        return f;
    }
    let b250 = n.min((250.0 / bin_hz) as usize + 1);
    let b4k = n.min((4000.0 / bin_hz) as usize + 1);
    let have_prev = prev.len() >= n;
    let band = |lo: usize, hi: usize| {
        #[cfg(target_arch = "x86_64")]
        if SIMD {
            // SAFETY: AVX2 + FMA were checked by the caller.
            return unsafe {
                if have_prev {
                    x86::feature_sums::<true>(&mag[lo..hi], &prev[lo..hi], lo)
                } else {
                    x86::feature_sums::<false>(&mag[lo..hi], &[], lo)
                }
            };
        }
        if have_prev {
            feature_sums::<true>(&mag[lo..hi], &prev[lo..hi], lo)
        } else {
            feature_sums::<false>(&mag[lo..hi], &[], lo)
        }
    };
    let (bass, mid, high) = (band(0, b250), band(b250, b4k), band(b4k, n));
    let total = bass.pw + mid.pw + high.pw;
    let sum_mag = bass.mag + mid.mag + high.mag;
    if total > 0.0 {
        f.centroid_hz = ((bass.weighted + mid.weighted + high.weighted) / total * bin_hz) as f32;
        let target = 0.85 * total;
        let mut acc = 0.0f64;
        let mut k = 0;
        while k + 64 <= n {
            let block = f64::from(sum_of_squares(&mag[k..k + 64]));
            if acc + block >= target {
                break;
            }
            acc += block;
            k += 64;
        }
        while k < n {
            acc += f64::from(mag[k]) * f64::from(mag[k]);
            if acc >= target {
                break;
            }
            k += 1;
        }
        f.rolloff_hz = (k.min(n - 1) as f64 * bin_hz) as f32;
        let log_sum = bass.log_db + mid.log_db + high.log_db;
        let geo = 10f64.powf((log_sum / n as f64) / 10.0);
        f.flatness = (geo / (total / n as f64)).clamp(0.0, 1.0) as f32;
    }
    let db = |p: f64| {
        if p > 1e-24 {
            (10.0 * p.log10()) as f32
        } else {
            -240.0
        }
    };
    f.bass_db = db(bass.pw);
    f.mid_db = db(mid.pw);
    f.high_db = db(high.pw);
    f.flux = if have_prev && sum_mag > 0.0 {
        ((bass.rise + mid.rise + high.rise) / sum_mag) as f32
    } else {
        0.0
    };
    if n_time > 0 {
        let s2: f64 = time
            .iter()
            .map(|part| f64::from(sum_of_squares_wide(part)))
            .sum();
        let rms = (s2 / n_time as f64).sqrt();
        f.rms_db = if rms > 1e-6 {
            (20.0 * rms.log10()) as f32
        } else {
            -120.0
        };
    }
    prev.clear();
    prev.extend_from_slice(mag);
    f
}

/* ───────────────────────── helpers ───────────────────────── */

/// True when the AVX2 + FMA kernels may run. `is_x86_feature_detected!`
/// caches its CPUID probe once per process (one relaxed atomic load per
/// call) and folds to a constant when the build already targets AVX2, as
/// the `target-cpu=native` builds do.
#[cfg(target_arch = "x86_64")]
#[inline(always)]
fn avx2_fma() -> bool {
    is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma")
}

/// True when every sample is exactly ±0.0 (digital silence). The bits of
/// 64 samples are OR-ed in lanes and the sign bit masked once per block
/// (`(a & m) | (b & m) == (a | b) & m`), so a silent block costs 14–23×
/// less than v2.11's per-sample `all()`, and audio still leaves at its
/// first sample (as fast as before).
#[inline]
pub fn block_is_silent(x: &[f32]) -> bool {
    // Audio almost always leaves at its first sample.
    if x.first().is_some_and(|s| s.to_bits() & 0x7FFF_FFFF != 0) {
        return false;
    }
    let (blocks, rest) = x.as_chunks::<64>();
    for block in blocks {
        let mut acc = [0u32; 8];
        for part in block.as_chunks::<8>().0 {
            for (a, v) in acc.iter_mut().zip(part) {
                *a |= v.to_bits();
            }
        }
        if acc.iter().fold(0, |m, &a| m | a) & 0x7FFF_FFFF != 0 {
            return false;
        }
    }
    rest.iter().fold(0, |m, s| m | s.to_bits()) & 0x7FFF_FFFF == 0
}

/// Maximum value and its first index (0 / 0.0 for an empty slice; a NaN
/// never wins). Two passes: [`max_of`], then the first index holding it,
/// whole 32-bin blocks tested at once (a vectorisable any-equal) and only
/// the hit block scanned. Same result as the scalar "first maximum".
///
/// Plugin_FFT v2.12's four branch-free (max, index) chains were ported and
/// measured (AVX2 and portable) and not kept: on a real spectrum this form
/// is 1.9–2.6× faster than the AVX2 chains (they pay a blend per vector and
/// a 32-lane reduction), equal with the peak at 3/4 of the axis, and at
/// worst 0.86× on a monotonically rising frame; the portable chains were
/// slower than v2.11 everywhere. Against v2.11 (max, then a scalar
/// `position`): 1.5–5.2×.
pub fn peak_with_index(data: &[f32]) -> (f32, usize) {
    if data.is_empty() {
        return (0.0, 0);
    }
    let max = max_of(data);
    let (chunks, rest) = data.as_chunks::<32>();
    for (c, chunk) in chunks.iter().enumerate() {
        if chunk.iter().fold(false, |hit, &v| hit | (v == max)) {
            let j = chunk.iter().position(|&v| v == max).unwrap_or(0);
            return (max, c * 32 + j);
        }
    }
    match rest.iter().position(|&v| v == max) {
        Some(j) => (max, chunks.len() * 32 + j),
        None => (max, 0),
    }
}

/// Transform length for a window of `capacity` samples: with zero padding
/// the pad length or the next power of two above the window, whichever is
/// larger; without, the window itself rounded up to even.
pub fn fft_size_for(p: &LiveFftSettings, capacity: usize) -> usize {
    if p.zero_padding {
        (p.fft_size as usize)
            .max(capacity.max(1).next_power_of_two())
            .max(2)
    } else {
        let n = capacity.max(2);
        n + (n & 1)
    }
}

/// Process-wide axis versions, so a new session never reuses a number the
/// page has cached for another axis (0 = no axis yet).
static NEXT_AXIS_VERSION: AtomicU32 = AtomicU32::new(1);

fn next_axis_version() -> u32 {
    loop {
        let v = NEXT_AXIS_VERSION.fetch_add(1, Ordering::Relaxed);
        if v != 0 {
            return v;
        }
    }
}

/* ───────────────────────── 9. the pipeline ───────────────────────── */

/// What the page shows about the transform (its telemetry rows).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PipelineStatus {
    pub sample_rate: u32,
    pub fft_size: u32,
    pub window_samples: u32,
    pub linear_bins: u32,
    /// Magnitude bins actually computed (the warp reads no more).
    pub magnitude_bins: u32,
    pub output_bins: u32,
    pub identity_warp: bool,
    /// Magnitude of a full-scale sine under the current normalisation: the
    /// page divides linear values by it.
    pub full_scale_ref: f32,
    /// Top of the axis: `display_max_hz` after the Nyquist clamp (Nyquist
    /// itself with raw bins).
    pub display_max_hz: f32,
    /// Bumped (process-wide) whenever the output axis changes.
    pub axis_version: u32,
    /// `fmax / (output_bins − 1)`; the mean spacing on a perceptual axis.
    pub hz_per_bin: f32,
    /// Output bins formed by peak / RMS aggregation.
    pub aggregated_bins: u32,
    /// β of the Kaiser window in use (0 for any other window).
    pub kaiser_beta: f32,
    pub raw_bins: bool,
}

/// Per-frame telemetry.
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameStats {
    pub peak_hz: f32,
    pub peak_value: f32,
    pub silent: bool,
    pub dsp_us: f32,
    /// Present when `spectral_features` is on.
    pub features: Option<SpectralFeatures>,
}

#[derive(Clone, Copy, PartialEq)]
struct WindowKey {
    window_type: FftWindowType,
    kaiser_beta: f64,
    capacity: usize,
    norm: FftMagnitudeNorm,
}

#[derive(Clone, Copy, PartialEq)]
struct WarpKey {
    scale: FftScale,
    fmax: f64,
    n_out: usize,
    warp: f64,
    log_floor: f32,
    nlin: usize,
    nyquist: f64,
    interp: FftWarpInterp,
    aggregation: FftWarpAggregation,
    raw: bool,
}

/// What the output axis (the Hz of every output bin) depends on.
#[derive(Clone, Copy, PartialEq)]
struct AxisKey {
    scale: FftScale,
    fmax: f64,
    n_out: usize,
    warp: f64,
    log_floor: f32,
}

/// One analysis channel: FIFO, EQ, window, FFT, warp, weighting, dB and
/// ballistics state, rebuilt in place when the settings or the source rate
/// change. Allocation-free per frame once warm.
pub struct SpectrumPipeline {
    planner: RealFftPlanner<f32>,
    sample_rate: u32,
    capacity: usize,
    fft_size: usize,
    pad_start: usize,
    fft: Option<Arc<dyn RealToComplex<f32>>>,
    scratch: Vec<Complex<f32>>,
    spectrum: Vec<Complex<f32>>,
    padded: Vec<f32>,
    magnitude: Vec<f32>,
    magnitude_bins: usize,
    fifo: Fifo,
    block: Vec<f32>,
    silent_run: usize,
    /// Off only in tests, to run the full chain on a silent window.
    silence_shortcut: bool,
    eq: BiquadEq,
    window: Vec<f32>,
    window_key: Option<WindowKey>,
    kaiser_beta: f64,
    warp: PerceptualWarp,
    warp_key: Option<WarpKey>,
    warp_version: u64,
    axis_key: Option<AxisKey>,
    axis_version: u32,
    fmax: f64,
    weighting: Vec<f32>,
    weight_key: Option<(FftWeighting, u64)>,
    prev_spectrum: Vec<f32>,
    prev_linear: Vec<f32>,
    agc_peak: f32,
    prev_loudness: FftLoudnessMode,
    status_version: u64,
    /// Most output bins this pipeline produces, whatever the settings ask
    /// for (see [`Self::with_output_bin_cap`]).
    output_bin_cap: Option<usize>,
}

impl Default for SpectrumPipeline {
    fn default() -> Self {
        Self::new()
    }
}

impl SpectrumPipeline {
    pub fn new() -> Self {
        Self {
            planner: RealFftPlanner::new(),
            sample_rate: 0,
            capacity: 0,
            fft_size: 0,
            pad_start: 0,
            fft: None,
            scratch: Vec::new(),
            spectrum: Vec::new(),
            padded: Vec::new(),
            magnitude: Vec::new(),
            magnitude_bins: 0,
            fifo: Fifo::new(1),
            block: Vec::new(),
            silent_run: 0,
            silence_shortcut: true,
            eq: BiquadEq::new(48_000.0),
            window: Vec::new(),
            window_key: None,
            kaiser_beta: 0.0,
            warp: PerceptualWarp::new(),
            warp_key: None,
            warp_version: 0,
            axis_key: None,
            axis_version: 0,
            fmax: 0.0,
            weighting: Vec::new(),
            weight_key: None,
            prev_spectrum: Vec::new(),
            prev_linear: Vec::new(),
            agc_peak: 0.0,
            prev_loudness: FftLoudnessMode::Off,
            status_version: 0,
            output_bin_cap: None,
        }
    }

    /// Never produce more than `cap` output bins. For a consumer that draws
    /// far fewer pixels than the page can ask for (the recording overlay):
    /// Auto, Raw or a large Fixed count would otherwise be computed and
    /// copied in full on every frame. A capped Raw grid becomes the same
    /// full-band linear axis, peak-aggregated so no partial is dropped.
    pub fn with_output_bin_cap(mut self, cap: usize) -> Self {
        self.output_bin_cap = Some(cap.max(2));
        self
    }

    /// Bumped whenever the transform, the window or the warp tables were
    /// rebuilt.
    pub fn status_version(&self) -> u64 {
        self.status_version
    }

    /// Version of the current output axis (process-wide; 0 before the first
    /// frame).
    pub fn axis_version(&self) -> u32 {
        self.axis_version
    }

    pub fn target_hz(&self) -> &[f64] {
        self.warp.target_hz()
    }

    #[cfg(test)]
    fn set_silence_shortcut(&mut self, on: bool) {
        self.silence_shortcut = on;
    }

    /// Analysis window in samples for these settings at this rate.
    pub fn window_samples_for(p: &LiveFftSettings, sample_rate: u32) -> usize {
        let samples = match p.window_length_mode {
            FftWindowLengthMode::Milliseconds => {
                (f64::from(p.window_ms) * f64::from(sample_rate) / 1000.0).round() as i64
            }
            FftWindowLengthMode::Samples => i64::from(p.window_samples),
        };
        samples.clamp(
            i64::from(MIN_FFT_WINDOW_SAMPLES),
            i64::from(MAX_FFT_WINDOW_SAMPLES),
        ) as usize
    }

    /// Rebuild the transform when the rate, the window or the transform
    /// length changed. Returns true when it did.
    fn prepare(&mut self, p: &LiveFftSettings, sample_rate: u32) -> bool {
        let sample_rate = sample_rate.max(1);
        let capacity = Self::window_samples_for(p, sample_rate);
        let fft_size = fft_size_for(p, capacity);
        if self.fft.is_some()
            && self.sample_rate == sample_rate
            && self.capacity == capacity
            && self.fft_size == fft_size
        {
            return false;
        }
        let rate_changed = self.sample_rate != sample_rate;
        let capacity_changed = self.capacity != capacity;
        self.sample_rate = sample_rate;
        self.capacity = capacity;
        self.fft_size = fft_size;
        // 8-float aligned centre offset: a shift < 8 samples only changes
        // phase, never magnitude.
        let centre = (self.fft_size - capacity) / 2;
        self.pad_start = centre & !7usize;

        let fft = self.planner.plan_fft_forward(self.fft_size);
        self.scratch.clear();
        self.scratch
            .resize(fft.get_scratch_len(), Complex::new(0.0, 0.0));
        self.spectrum = fft.make_output_vec();
        self.padded.clear();
        self.padded.resize(self.fft_size, 0.0);
        self.magnitude.clear();
        self.magnitude.resize(self.fft_size / 2 + 1, 0.0);
        self.magnitude_bins = self.magnitude.len();
        self.fft = Some(fft);

        if capacity_changed || rate_changed {
            self.fifo.resize(capacity);
            self.silent_run = 0;
        }
        self.eq.set_sample_rate(f64::from(sample_rate));
        self.window_key = None;
        self.warp_key = None;
        self.weight_key = None;
        self.prev_spectrum.clear();
        self.prev_linear.clear();
        self.agc_peak = 0.0;
        self.status_version += 1;
        true
    }

    /// Feed new samples: only the tail that still fits the window is kept,
    /// the EQ runs on the new samples in time order, digital silence is
    /// tracked on what enters the FIFO, then they enter it.
    pub fn ingest(&mut self, samples: &[f32], sample_rate: u32, p: &LiveFftSettings) {
        self.prepare(p, sample_rate);
        if samples.is_empty() {
            return;
        }
        let keep = samples.len().min(self.capacity);
        let tail = &samples[samples.len() - keep..];

        let eq_active = p.eq_enabled
            && self.eq.update_and_check_active(
                if p.high_shelf {
                    f64::from(p.high_gain_db)
                } else {
                    0.0
                },
                if p.high_shelf {
                    f64::from(p.high_cutoff_hz)
                } else {
                    1000.0
                },
                if p.low_shelf {
                    f64::from(p.low_gain_db)
                } else {
                    0.0
                },
                if p.low_shelf {
                    f64::from(p.low_cutoff_hz)
                } else {
                    200.0
                },
                f64::from(p.eq_q),
                f64::from(p.eq_amount),
            );
        // Silence is judged on the samples the window will hold (after the
        // EQ, whose decaying tail is not silence), so a window counted
        // silent is exactly zero and the shortcut is exact.
        let silent = if eq_active {
            self.block.clear();
            self.block.extend_from_slice(tail);
            self.eq
                .process_block_in_place(&mut self.block, f64::from(p.eq_amount));
            self.fifo.add(&self.block);
            block_is_silent(&self.block)
        } else {
            self.fifo.add(tail);
            block_is_silent(tail)
        };
        if silent {
            self.silent_run = (self.silent_run + keep).min(self.capacity * 2);
        } else {
            self.silent_run = 0;
        }
    }

    fn update_window(&mut self, p: &LiveFftSettings) {
        let beta = if p.window_type == FftWindowType::Kaiser {
            effective_kaiser_beta(p)
        } else {
            0.0
        };
        self.kaiser_beta = beta;
        let key = WindowKey {
            window_type: p.window_type,
            kaiser_beta: beta,
            capacity: self.capacity,
            norm: p.magnitude_norm,
        };
        if self.window_key == Some(key) && self.window.len() == self.capacity {
            return;
        }
        generate_window(
            p.window_type,
            beta,
            self.capacity,
            p.magnitude_norm,
            &mut self.window,
        );
        self.window_key = Some(key);
        // The status carries the β (Plugin_FFT's `updateWindow` bumps it
        // too), so a β mode or dB range change is republished at once
        // rather than on the next ~1 s heartbeat.
        self.status_version += 1;
    }

    fn update_warp(&mut self, p: &LiveFftSettings) {
        let nlin = self.fft_size / 2 + 1;
        let nyquist = f64::from(self.sample_rate) / 2.0;
        let raw = p.raw_bins;
        // Raw bins: the rfft grid itself, which the warp detects as identity.
        let (scale, fmax, blend, mut n_out, interp, mut aggregation) = if raw {
            (
                FftScale::Linear,
                nyquist,
                0.0,
                nlin,
                FftWarpInterp::Linear,
                FftWarpAggregation::Off,
            )
        } else {
            (
                p.scale,
                f64::from(p.display_max_hz).min(nyquist).max(1.0),
                f64::from(p.warp_blend),
                match p.output_bins_mode {
                    FftOutputBinsMode::Auto => nlin,
                    FftOutputBinsMode::Fixed => p.output_bins as usize,
                },
                p.warp_interpolation,
                p.warp_aggregation,
            )
        };
        if let Some(cap) = self.output_bin_cap.filter(|cap| n_out > *cap) {
            n_out = cap;
            if raw {
                aggregation = FftWarpAggregation::Peak;
            }
        }
        let key = WarpKey {
            scale,
            fmax,
            n_out,
            warp: blend,
            log_floor: p.log_floor_hz,
            nlin,
            nyquist,
            interp,
            aggregation,
            raw,
        };
        if self.warp_key == Some(key) {
            return;
        }
        self.warp.set_interpolation(interp);
        self.warp.set_aggregation(aggregation);
        self.warp.build_tables(
            scale,
            fmax,
            n_out,
            nyquist,
            blend,
            f64::from(p.log_floor_hz),
            nlin,
        );
        self.magnitude_bins = nlin.min(self.warp.max_linear_index() + 1);
        self.fmax = fmax;
        self.warp_key = Some(key);
        self.warp_version += 1;
        self.status_version += 1;
        let axis = AxisKey {
            scale,
            fmax,
            n_out,
            warp: blend,
            log_floor: p.log_floor_hz,
        };
        if self.axis_key != Some(axis) {
            self.axis_key = Some(axis);
            self.axis_version = next_axis_version();
        }
    }

    fn update_weighting(&mut self, p: &LiveFftSettings) {
        let key = (p.weighting, self.warp_version);
        if self.weight_key == Some(key) && self.weighting.len() == self.warp.output_bins() {
            return;
        }
        equal_loudness_curve(p.weighting, self.warp.target_hz(), &mut self.weighting);
        self.weight_key = Some(key);
    }

    /// Clear ballistics, AGC, flux and EQ state (the page's Reset button).
    pub fn reset_state(&mut self) {
        self.prev_spectrum.clear();
        self.prev_linear.clear();
        self.agc_peak = 0.0;
        self.eq.reset();
    }

    pub fn status(&self, p: &LiveFftSettings) -> PipelineStatus {
        let n_out = self.warp.output_bins();
        PipelineStatus {
            sample_rate: self.sample_rate,
            fft_size: self.fft_size as u32,
            window_samples: self.capacity as u32,
            linear_bins: (self.fft_size / 2 + 1) as u32,
            magnitude_bins: self.magnitude_bins as u32,
            output_bins: n_out as u32,
            identity_warp: self.warp.is_identity(),
            full_scale_ref: match p.magnitude_norm {
                FftMagnitudeNorm::FullScale => 1.0,
                FftMagnitudeNorm::CoherentGain => self.capacity as f32 * 0.5,
            },
            display_max_hz: self.fmax as f32,
            axis_version: self.axis_version,
            hz_per_bin: if n_out >= 2 {
                (self.fmax / (n_out - 1) as f64) as f32
            } else {
                0.0
            },
            aggregated_bins: self.warp.aggregated_bins() as u32,
            kaiser_beta: self.kaiser_beta as f32,
            raw_bins: p.raw_bins,
        }
    }

    /// Window the FIFO straight from its two slices into the centre of the
    /// padded frame and re-zero only the pad regions (realfft overwrites
    /// its input, so they do need it every frame).
    fn window_into_frame(&mut self) {
        let cap = self.capacity;
        let start = self.pad_start;
        let (zeros, older, newer) = self.fifo.segments();
        let (head, rest) = self.padded.split_at_mut(start);
        head.fill(0.0);
        let (frame, tail) = rest.split_at_mut(cap);
        tail.fill(0.0);
        let window = &self.window[..cap];
        frame[..zeros].fill(0.0);
        let (frame_a, frame_b) = frame[zeros..].split_at_mut(older.len());
        let (win_a, win_b) = window[zeros..].split_at(older.len());
        for ((d, &s), &w) in frame_a.iter_mut().zip(older).zip(win_a) {
            *d = s * w;
        }
        for ((d, &s), &w) in frame_b.iter_mut().zip(newer).zip(win_b) {
            *d = s * w;
        }
    }

    /// One analysis frame into `out` (the output bin count of the current
    /// axis). `dt_ms` is the time since the previous frame, for the
    /// millisecond ballistics and the AGC follower.
    pub fn process(&mut self, p: &LiveFftSettings, dt_ms: f64, out: &mut Vec<f32>) -> FrameStats {
        let t0 = Instant::now();
        if self.fft.is_none() {
            // Nothing ingested yet: keep the page fed with a flat spectrum.
            self.prepare(p, self.sample_rate.max(48_000));
        } else {
            self.prepare(p, self.sample_rate);
        }
        self.update_window(p);
        self.update_warp(p);
        self.update_weighting(p);

        let dt_ms = if dt_ms.is_finite() && dt_ms > 0.0 {
            dt_ms.min(5000.0)
        } else {
            1000.0 / 30.0
        };
        let (attack_coef, release_coef) = if p.ballistics_enabled {
            match p.ballistics_mode {
                FftBallisticsMode::Milliseconds => (
                    coef_from_ms(f64::from(p.attack_ms), dt_ms),
                    coef_from_ms(f64::from(p.release_ms), dt_ms),
                ),
                FftBallisticsMode::Coefficient => (p.attack, p.release),
            }
        } else {
            (0.0, 0.0)
        };
        let agc_decay = coef_from_ms(1500.0, dt_ms);

        let silent = self.silent_run >= self.capacity;
        let shortcut = silent && self.silence_shortcut;
        let bins = self.warp.output_bins();
        if out.len() != bins {
            out.clear();
            out.resize(bins, 0.0);
        }

        let mut features = None;
        let need_ref_peak =
            p.loudness_mode != FftLoudnessMode::Off && p.db_reference != FftDbReference::Dbfs;
        let mut ref_peak = None;
        if shortcut {
            // Digital silence: the linear magnitude of a zero window is
            // exactly zero through the window, the FFT, the warp and the
            // weighting, so none of them runs.
            out.fill(0.0);
            if p.spectral_features {
                features = Some(SpectralFeatures::default());
            }
            self.prev_linear.clear();
            if p.loudness_mode == FftLoudnessMode::Off && !p.ballistics_enabled {
                self.prev_spectrum.clear();
            }
        } else {
            // 1. window into the aligned centre of the zero-padded frame
            self.window_into_frame();

            // 2. FFT + magnitude (only the bins the warp reads)
            if let Some(fft) = &self.fft
                && fft
                    .process_with_scratch(&mut self.padded, &mut self.spectrum, &mut self.scratch)
                    .is_err()
            {
                self.spectrum.fill(Complex::new(0.0, 0.0));
            }
            let n_mag = self.magnitude_bins.min(self.spectrum.len());
            // (LLVM's auto-vectorised sqrt measured faster than every AVX2
            // form tried: vhaddps / vshufps deinterleave with vsqrtps or
            // rsqrt + Newton-Raphson, 0.74–0.93×.)
            for (m, c) in self.magnitude[..n_mag]
                .iter_mut()
                .zip(&self.spectrum[..n_mag])
            {
                *m = (c.re * c.re + c.im * c.im).sqrt();
            }

            // 2b. full scale: DC / Nyquist have no mirror bin (the Nyquist
            //     bin only when it was computed)
            if p.magnitude_norm == FftMagnitudeNorm::FullScale && n_mag >= 1 {
                self.magnitude[0] *= 0.5;
                let last = self.magnitude.len() - 1;
                if n_mag > last {
                    self.magnitude[last] *= 0.5;
                }
            }

            // 2c. spectral features from the linear magnitude
            if p.spectral_features {
                let bin_hz = f64::from(self.sample_rate) / self.fft_size as f64;
                let (_, older, newer) = self.fifo.segments();
                features = Some(compute_spectral_features(
                    &self.magnitude[..n_mag],
                    bin_hz,
                    &[older, newer],
                    self.capacity,
                    &mut self.prev_linear,
                ));
            }

            // 3. psychoacoustic warp (+ aggregation)
            self.warp.apply(&self.magnitude, out);

            // 4. equal-loudness weighting; when a Frame Peak / AGC dB
            //    reference needs the peak of the weighted spectrum, the same
            //    pass measures it (the dBFS reference needs no peak at all).
            if p.weighting != FftWeighting::Off && self.weighting.len() == out.len() {
                if need_ref_peak {
                    ref_peak = Some(multiply_in_place_max(out, &self.weighting));
                } else {
                    for (v, w) in out.iter_mut().zip(&self.weighting) {
                        *v *= *w;
                    }
                }
            }
        }

        // 5. dB with the selected reference
        if p.loudness_mode != FftLoudnessMode::Off {
            if self.prev_loudness == FftLoudnessMode::Off {
                self.prev_spectrum.clear();
            }
            let frame_peak = || {
                if shortcut {
                    0.0
                } else {
                    ref_peak.unwrap_or_else(|| max_of(out.as_slice()))
                }
            };
            let mut reference = match p.db_reference {
                FftDbReference::Dbfs => match p.magnitude_norm {
                    FftMagnitudeNorm::FullScale => 1.0,
                    FftMagnitudeNorm::CoherentGain => self.capacity as f32 * 0.5,
                },
                FftDbReference::Agc => {
                    self.agc_peak = frame_peak().max(self.agc_peak * agc_decay);
                    self.agc_peak
                }
                FftDbReference::FramePeak => frame_peak(),
            };
            if !reference.is_finite() || reference <= 0.0 {
                reference = 1.0;
            }
            if shortcut {
                // Every bin is the dB value of zero: convert it once.
                let mut zero = [0.0f32];
                convert_to_db(
                    p.loudness_mode,
                    f64::from(p.db_range),
                    1.0 / reference,
                    &mut zero,
                );
                out.fill(zero[0]);
            } else {
                convert_to_db(p.loudness_mode, f64::from(p.db_range), 1.0 / reference, out);
            }
        } else if self.prev_loudness != FftLoudnessMode::Off {
            self.prev_spectrum.clear();
        }
        self.prev_loudness = p.loudness_mode;

        // 6. ballistics in place (bypassed at 0/0)
        if attack_coef > 0.0 || release_coef > 0.0 {
            apply_ballistics(
                attack_coef,
                release_coef,
                out,
                &mut self.prev_spectrum,
                p.loudness_mode == FftLoudnessMode::Off,
            );
        }

        // 7. the one peak search of the frame
        let (peak_value, peak_idx) = peak_with_index(out);
        let peak_hz = self
            .warp
            .target_hz()
            .get(peak_idx)
            .map(|hz| *hz as f32)
            .unwrap_or(0.0);
        FrameStats {
            peak_hz,
            peak_value,
            silent,
            dsp_us: t0.elapsed().as_secs_f32() * 1e6,
            features,
        }
    }
}

/* ───────────────────────── 10. AVX2 + FMA kernels ───────────────────────── */

/// The x86-64 forms of the hot kernels, run when the CPU has AVX2 + FMA
/// (`avx2_fma`). Each one has a portable twin above that is the reference
/// and the fallback (aarch64, Apple silicon, pre-Haswell x86), and each was
/// kept only because an interleaved A/B measurement showed it faster than
/// the auto-vectorised Rust (Plugin_FFT v2.12 port; medians of 3 pinned
/// processes, 10 ABBA rounds each):
///
/// - dB (FastLog2Seg + `vpermps`, one FMA): 1.09–1.17× in dB, 1.45–1.50×
///   normalised against v2.11's table, and 13× more accurate (0.00016 dB);
/// - warp load + permute: linear 1.48–1.54×, Catmull-Rom 2.93×,
///   bit-identical to the portable kernels;
/// - spectral-feature band sums: 1.63–1.65× the portable form.
///
/// Measured and not kept: AVX2 `max_of` (the portable 4-lane-array form is
/// 1.9–2.9× faster), the AVX2 peak + index chains (see [`super::peak_with_index`]),
/// every AVX2 magnitude (`vhaddps` / `vshufps` with `vsqrtps` or rsqrt +
/// Newton-Raphson: 0.74–0.93× of LLVM's own `sqrt` loop) and `fast_log2`
/// without `vpermps` in the dB loop (0.44–0.62× the table).
#[cfg(target_arch = "x86_64")]
pub(super) mod x86 {
    use std::arch::x86_64::*;

    use super::{DbParams, LOG2_C0, LOG2_C1, LOG2_C2, MIN_MAG};

    /// `log2` of eight positive normal floats: [`super::fast_log2`] with the
    /// segment coefficients fetched by `vpermps` from three registers (no
    /// memory access) and the quadratic in two FMAs.
    #[inline]
    #[target_feature(enable = "avx2,fma")]
    fn log2(v: __m256, c0: __m256, c1: __m256, c2: __m256) -> __m256 {
        let bits = _mm256_castps_si256(v);
        // vpermps reads only the low 3 bits of each index: the segment.
        let j = _mm256_srli_epi32::<20>(bits);
        let e = _mm256_cvtepi32_ps(_mm256_sub_epi32(
            _mm256_srli_epi32::<23>(bits),
            _mm256_set1_epi32(127),
        ));
        let u = _mm256_sub_ps(
            _mm256_castsi256_ps(_mm256_or_si256(
                _mm256_and_si256(bits, _mm256_set1_epi32(0x000F_FFFF)),
                _mm256_set1_epi32(0x3F80_0000),
            )),
            _mm256_set1_ps(1.0),
        );
        let q = _mm256_fmadd_ps(
            _mm256_permutevar8x32_ps(c2, j),
            u,
            _mm256_permutevar8x32_ps(c1, j),
        );
        let q = _mm256_fmadd_ps(q, u, _mm256_permutevar8x32_ps(c0, j));
        _mm256_add_ps(q, e)
    }

    struct DbConsts {
        c0: __m256,
        c1: __m256,
        c2: __m256,
        min: __m256,
        k: __m256,
        c: __m256,
        floor: __m256,
        zero: __m256,
        one: __m256,
    }

    #[inline]
    #[target_feature(enable = "avx2,fma")]
    fn db8<const NORM: bool>(v: __m256, k: &DbConsts) -> __m256 {
        let l = log2(_mm256_max_ps(v, k.min), k.c0, k.c1, k.c2);
        let y = _mm256_fmadd_ps(l, k.k, k.c);
        if NORM {
            _mm256_min_ps(_mm256_max_ps(y, k.zero), k.one)
        } else {
            _mm256_max_ps(y, k.floor)
        }
    }

    #[inline]
    #[target_feature(enable = "avx2,fma")]
    fn db_loop<const NORM: bool>(s: &mut [f32], k: &DbConsts) {
        let n = s.len();
        let ptr = s.as_mut_ptr();
        let mut i = 0;
        // SAFETY: every access is within `s` (i + 16 <= n, i + 8 <= n).
        unsafe {
            while i + 16 <= n {
                let a = _mm256_loadu_ps(ptr.add(i));
                let b = _mm256_loadu_ps(ptr.add(i + 8));
                _mm256_storeu_ps(ptr.add(i), db8::<NORM>(a, k));
                _mm256_storeu_ps(ptr.add(i + 8), db8::<NORM>(b, k));
                i += 16;
            }
            while i + 8 <= n {
                _mm256_storeu_ps(ptr.add(i), db8::<NORM>(_mm256_loadu_ps(ptr.add(i)), k));
                i += 8;
            }
        }
        if i < n {
            // The last < 8 bins through the same vector code on a padded
            // copy, so a bin gets the same bits wherever it sits (the
            // silence shortcut converts one bin and must match the frame).
            let mut buf = [1.0f32; 8];
            buf[..n - i].copy_from_slice(&s[i..]);
            // SAFETY: `buf` holds 8 floats.
            unsafe {
                let r = db8::<NORM>(_mm256_loadu_ps(buf.as_ptr()), k);
                _mm256_storeu_ps(buf.as_mut_ptr(), r);
            }
            s[i..].copy_from_slice(&buf[..n - i]);
        }
    }

    /// [`super::convert_to_db`]: one FMA per 8 bins for the folded constants.
    #[target_feature(enable = "avx2,fma")]
    pub(in crate::live_fft) fn convert_to_db(p: DbParams, s: &mut [f32]) {
        // SAFETY: the coefficient tables hold 8 floats each.
        let (c0, c1, c2) = unsafe {
            (
                _mm256_loadu_ps(LOG2_C0.as_ptr()),
                _mm256_loadu_ps(LOG2_C1.as_ptr()),
                _mm256_loadu_ps(LOG2_C2.as_ptr()),
            )
        };
        let k = DbConsts {
            c0,
            c1,
            c2,
            min: _mm256_set1_ps(MIN_MAG),
            k: _mm256_set1_ps(p.k),
            c: _mm256_set1_ps(p.c),
            floor: _mm256_set1_ps(p.floor),
            zero: _mm256_setzero_ps(),
            one: _mm256_set1_ps(1.0),
        };
        if p.normalized {
            db_loop::<true>(s, &k);
        } else {
            db_loop::<false>(s, &k);
        }
    }

    /// Linear warp, bit-identical to [`super::warp_linear_portable`] (same
    /// operations, no FMA). `i0` is non-decreasing along the axis, so where
    /// the 8 output bins of a vector read taps inside one 8-float window
    /// (the fine, upsampled part of the axis) each tap is one unaligned load
    /// plus one `vpermps` instead of an 8-element gather.
    ///
    /// # Safety
    /// Every `i0[k] + 1 < src.len()` and `src.len() < 2^31`.
    #[target_feature(enable = "avx2,fma")]
    pub(in crate::live_fft) unsafe fn warp_linear(
        src: &[f32],
        i0: &[u32],
        w: &[f32],
        dst: &mut [f32],
    ) {
        let n = dst.len().min(i0.len()).min(w.len());
        let src_n = src.len();
        let sp = src.as_ptr();
        let mut i = 0;
        // SAFETY: loads of `i0`, `w` and stores to `dst` stay below `n`;
        // the permute path loads `src[base .. base + 9)` with
        // `base + 9 <= src_n`; the gathers read `src[i0]`, `src[i0 + 1]`,
        // in bounds by the precondition.
        unsafe {
            while i + 8 <= n {
                let vi = _mm256_loadu_si256(i0.as_ptr().add(i).cast());
                let base = *i0.get_unchecked(i) as usize;
                let top = *i0.get_unchecked(i + 7) as usize;
                let (v0, v1) = if top.wrapping_sub(base) <= 7 && base + 9 <= src_n {
                    let rel = _mm256_sub_epi32(vi, _mm256_set1_epi32(base as i32));
                    (
                        _mm256_permutevar8x32_ps(_mm256_loadu_ps(sp.add(base)), rel),
                        _mm256_permutevar8x32_ps(_mm256_loadu_ps(sp.add(base + 1)), rel),
                    )
                } else {
                    (
                        _mm256_i32gather_ps::<4>(sp, vi),
                        _mm256_i32gather_ps::<4>(sp.add(1), vi),
                    )
                };
                let wv = _mm256_loadu_ps(w.as_ptr().add(i));
                let r = _mm256_add_ps(v0, _mm256_mul_ps(wv, _mm256_sub_ps(v1, v0)));
                _mm256_storeu_ps(dst.as_mut_ptr().add(i), r);
                i += 8;
            }
        }
        super::warp_linear_portable(src, &i0[i..n], &w[i..n], &mut dst[i..n]);
    }

    /// Catmull-Rom warp, bit-identical to [`super::warp_cubic_portable`].
    /// Where the 8 output bins' taps fit one window, the four taps
    /// `i0 − 1 … i0 + 2` are four overlapping unaligned loads + `vpermps`
    /// and none of the edge clamps can bind.
    ///
    /// # Safety
    /// `src.len() >= 4`, every `i0[k] + 1 < src.len()`, `src.len() < 2^31`.
    #[target_feature(enable = "avx2,fma")]
    pub(in crate::live_fft) unsafe fn warp_cubic(
        src: &[f32],
        i0: &[u32],
        w: &[f32],
        dst: &mut [f32],
    ) {
        let n = dst.len().min(i0.len()).min(w.len());
        let src_n = src.len();
        let sp = src.as_ptr();
        let one = _mm256_set1_epi32(1);
        let two = _mm256_set1_epi32(2);
        let v_zero = _mm256_setzero_si256();
        let v_last = _mm256_set1_epi32(src_n as i32 - 1);
        let (h, c2, c3, c4, c5) = (
            _mm256_set1_ps(0.5),
            _mm256_set1_ps(2.0),
            _mm256_set1_ps(3.0),
            _mm256_set1_ps(4.0),
            _mm256_set1_ps(5.0),
        );
        let fz = _mm256_setzero_ps();
        let mut i = 0;
        // SAFETY: as in `warp_linear`; the permute path loads
        // `src[base − 1 .. base + 10)` with `base >= 1`, `base + 10 <= src_n`,
        // and the gather indices are clamped to `[0, src_n − 1]`.
        unsafe {
            while i + 8 <= n {
                let vi = _mm256_loadu_si256(i0.as_ptr().add(i).cast());
                let base = *i0.get_unchecked(i) as usize;
                let top = *i0.get_unchecked(i + 7) as usize;
                let (p0, p1, p2, p3) =
                    if base >= 1 && top.wrapping_sub(base) <= 7 && base + 10 <= src_n {
                        let rel = _mm256_sub_epi32(vi, _mm256_set1_epi32(base as i32));
                        (
                            _mm256_permutevar8x32_ps(_mm256_loadu_ps(sp.add(base - 1)), rel),
                            _mm256_permutevar8x32_ps(_mm256_loadu_ps(sp.add(base)), rel),
                            _mm256_permutevar8x32_ps(_mm256_loadu_ps(sp.add(base + 1)), rel),
                            _mm256_permutevar8x32_ps(_mm256_loadu_ps(sp.add(base + 2)), rel),
                        )
                    } else {
                        (
                            _mm256_i32gather_ps::<4>(
                                sp,
                                _mm256_max_epi32(_mm256_sub_epi32(vi, one), v_zero),
                            ),
                            _mm256_i32gather_ps::<4>(sp, vi),
                            _mm256_i32gather_ps::<4>(
                                sp,
                                _mm256_min_epi32(_mm256_add_epi32(vi, one), v_last),
                            ),
                            _mm256_i32gather_ps::<4>(
                                sp,
                                _mm256_min_epi32(_mm256_add_epi32(vi, two), v_last),
                            ),
                        )
                    };
                let t = _mm256_loadu_ps(w.as_ptr().add(i));
                // Exactly the scalar expression's operations, in its order:
                // 0.5 · (((2p1 + (p2−p0)t) + c·t·t) + d·t·t·t) with
                // c = ((2p0 − 5p1) + 4p2) − p3 and d = ((3p1 − p0) − 3p2) + p3.
                let a = _mm256_mul_ps(c2, p1);
                let b = _mm256_mul_ps(_mm256_sub_ps(p2, p0), t);
                let c = _mm256_sub_ps(
                    _mm256_add_ps(
                        _mm256_sub_ps(_mm256_mul_ps(c2, p0), _mm256_mul_ps(c5, p1)),
                        _mm256_mul_ps(c4, p2),
                    ),
                    p3,
                );
                let d = _mm256_add_ps(
                    _mm256_sub_ps(
                        _mm256_sub_ps(_mm256_mul_ps(c3, p1), p0),
                        _mm256_mul_ps(c3, p2),
                    ),
                    p3,
                );
                let ctt = _mm256_mul_ps(_mm256_mul_ps(c, t), t);
                let dttt = _mm256_mul_ps(_mm256_mul_ps(_mm256_mul_ps(d, t), t), t);
                let sum = _mm256_add_ps(_mm256_add_ps(_mm256_add_ps(a, b), ctt), dttt);
                let r = _mm256_max_ps(_mm256_mul_ps(h, sum), fz);
                _mm256_storeu_ps(dst.as_mut_ptr().add(i), r);
                i += 8;
            }
        }
        super::warp_cubic_portable(src, &i0[i..n], &w[i..n], &mut dst[i..n]);
    }

    #[inline]
    #[target_feature(enable = "avx2")]
    fn hsum(v: __m256) -> f32 {
        let s4 = _mm_add_ps(_mm256_castps256_ps128(v), _mm256_extractf128_ps::<1>(v));
        let s4 = _mm_add_ps(s4, _mm_movehl_ps(s4, s4));
        _mm_cvtss_f32(_mm_add_ss(s4, _mm_shuffle_ps::<1>(s4, s4)))
    }

    /// [`super::feature_sums`] with explicit vectors: five f32 lane sums
    /// flushed to f64 every 256 bins, the log through [`log2`].
    ///
    /// # Safety
    /// With `PREV`, `prev.len() >= mag.len()`.
    #[target_feature(enable = "avx2,fma")]
    pub(in crate::live_fft) unsafe fn feature_sums<const PREV: bool>(
        mag: &[f32],
        prev: &[f32],
        k0: usize,
    ) -> super::FeatureSums {
        // SAFETY: the coefficient tables hold 8 floats each.
        let (c0, c1, c2) = unsafe {
            (
                _mm256_loadu_ps(LOG2_C0.as_ptr()),
                _mm256_loadu_ps(LOG2_C1.as_ptr()),
                _mm256_loadu_ps(LOG2_C2.as_ptr()),
            )
        };
        let tiny = _mm256_set1_ps(1e-12);
        let zero = _mm256_setzero_ps();
        let eight = _mm256_set1_ps(8.0);
        let lane = _mm256_setr_ps(0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0);
        let n = mag.len();
        let (mp, pp) = (mag.as_ptr(), prev.as_ptr());
        let mut a = super::FeatureSums::default();
        let mut i = 0;
        while i + 8 <= n {
            let end = (i + super::FEATURE_FLUSH_BINS).min(n);
            let (mut spw, mut swk, mut smag, mut slg, mut srise) = (zero, zero, zero, zero, zero);
            let mut kk = _mm256_add_ps(_mm256_set1_ps((k0 + i) as f32), lane);
            // SAFETY: i + 8 <= end <= n, and prev is at least as long.
            unsafe {
                while i + 8 <= end {
                    let m = _mm256_loadu_ps(mp.add(i));
                    let pw = _mm256_mul_ps(m, m);
                    spw = _mm256_add_ps(spw, pw);
                    swk = _mm256_fmadd_ps(pw, kk, swk);
                    smag = _mm256_add_ps(smag, m);
                    slg = _mm256_add_ps(slg, log2(_mm256_max_ps(m, tiny), c0, c1, c2));
                    if PREV {
                        let d = _mm256_sub_ps(m, _mm256_loadu_ps(pp.add(i)));
                        srise = _mm256_add_ps(srise, _mm256_max_ps(d, zero));
                    }
                    kk = _mm256_add_ps(kk, eight);
                    i += 8;
                }
            }
            a.pw += f64::from(hsum(spw));
            a.weighted += f64::from(hsum(swk));
            a.mag += f64::from(hsum(smag));
            a.log_db += f64::from(hsum(slg)) * super::DB_PER_OCTAVE_EXACT;
            a.rise += f64::from(hsum(srise));
        }
        for k in i..n {
            let v = mag[k];
            let pw = f64::from(v) * f64::from(v);
            a.pw += pw;
            a.weighted += pw * (k0 + k) as f64;
            a.mag += f64::from(v);
            a.log_db += f64::from(super::fast_log2(v.max(1e-12))) * super::DB_PER_OCTAVE_EXACT;
            if PREV {
                let d = v - prev[k];
                if d > 0.0 {
                    a.rise += f64::from(d);
                }
            }
        }
        a
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A counting global allocator for the lib test binary only (this
    /// module is `cfg(test)`, so no other build ever sees it). It counts
    /// `alloc` / `alloc_zeroed` / `realloc` on the current thread while that
    /// thread has armed it, so tests running in parallel on other threads
    /// neither pollute nor see the count; everything else goes straight to
    /// the system allocator.
    mod alloc_gate {
        use std::alloc::{GlobalAlloc, Layout, System};
        use std::cell::Cell;

        thread_local! {
            /// `Some(n)` while armed on this thread.
            static COUNT: Cell<Option<u64>> = const { Cell::new(None) };
        }

        fn bump() {
            // `try_with`: never touch a thread-local being torn down.
            let _ = COUNT.try_with(|c| {
                if let Some(n) = c.get() {
                    c.set(Some(n + 1));
                }
            });
        }

        pub struct Counting;

        // SAFETY: every call is forwarded unchanged to `System`; the count is
        // a const-initialised thread-local `Cell` that never allocates.
        unsafe impl GlobalAlloc for Counting {
            unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
                bump();
                // SAFETY: forwarded with the caller's guarantees.
                unsafe { System.alloc(layout) }
            }
            unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
                bump();
                // SAFETY: as above.
                unsafe { System.alloc_zeroed(layout) }
            }
            unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
                // SAFETY: as above.
                unsafe { System.dealloc(ptr, layout) }
            }
            unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
                bump();
                // SAFETY: as above.
                unsafe { System.realloc(ptr, layout, new_size) }
            }
        }

        #[global_allocator]
        static ALLOCATOR: Counting = Counting;

        /// Start counting this thread's allocations from zero.
        pub fn arm() {
            COUNT.with(|c| c.set(Some(0)));
        }

        /// Stop counting and return how many allocations happened.
        pub fn disarm() -> u64 {
            COUNT.with(|c| c.replace(None)).unwrap_or(0)
        }
    }

    fn settings() -> LiveFftSettings {
        LiveFftSettings::default().normalized()
    }

    fn sine(hz: f32, amp: f32, sr: u32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| amp * (2.0 * std::f32::consts::PI * hz * i as f32 / sr as f32).sin())
            .collect()
    }

    /// Deterministic uniform noise in [-amp, amp].
    fn noise(amp: f32, n: usize, seed: u64) -> Vec<f32> {
        let mut state = seed;
        (0..n)
            .map(|_| {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                let u = (state >> 40) as f32 / (1u64 << 24) as f32;
                amp * (2.0 * u - 1.0)
            })
            .collect()
    }

    #[test]
    fn fifo_keeps_the_newest_samples_in_order() {
        let mut fifo = Fifo::new(4);
        let mut out = Vec::new();
        fifo.add(&[1.0, 2.0]);
        fifo.get(&mut out);
        assert_eq!(out, vec![0.0, 0.0, 1.0, 2.0], "right-aligned while filling");
        fifo.add(&[3.0, 4.0, 5.0]);
        fifo.get(&mut out);
        assert_eq!(out, vec![2.0, 3.0, 4.0, 5.0]);
        fifo.add(&[6.0, 7.0, 8.0, 9.0, 10.0]);
        fifo.get(&mut out);
        assert_eq!(
            out,
            vec![7.0, 8.0, 9.0, 10.0],
            "oversized block keeps its tail"
        );
    }

    /// Plugin_FFT's FIFO test: random block sizes, compared with a plain
    /// reference vector, including the wrapped segments while filling.
    #[test]
    fn fifo_segments_match_a_reference_under_random_blocks() {
        let cap = 10;
        let mut fifo = Fifo::new(cap);
        let mut reference: Vec<f32> = Vec::new();
        let mut counter = 0.0f32;
        let mut state = 7u64;
        for _ in 0..50 {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            let len = 1 + (state >> 60) as usize % 13;
            let block: Vec<f32> = (0..len)
                .map(|_| {
                    counter += 1.0;
                    counter
                })
                .collect();
            fifo.add(&block);
            reference.extend_from_slice(&block);
            let mut expected = vec![0.0; cap.saturating_sub(reference.len())];
            let from = reference.len().saturating_sub(cap);
            expected.extend_from_slice(&reference[from..]);
            let mut got = Vec::new();
            fifo.get(&mut got);
            assert_eq!(got, expected);
            let (zeros, a, b) = fifo.segments();
            assert_eq!(zeros + a.len() + b.len(), cap);
        }
    }

    #[test]
    fn coherent_gain_window_has_unit_mean_and_full_scale_sums_to_two() {
        let mut w = Vec::new();
        for window_type in [
            FftWindowType::Kaiser,
            FftWindowType::Hann,
            FftWindowType::Hamming,
            FftWindowType::Blackman,
            FftWindowType::BlackmanHarris,
            FftWindowType::Rectangular,
        ] {
            generate_window(
                window_type,
                15.0,
                3175,
                FftMagnitudeNorm::CoherentGain,
                &mut w,
            );
            let mean = w.iter().map(|&x| x as f64).sum::<f64>() / w.len() as f64;
            assert!((mean - 1.0).abs() < 1e-4, "{window_type:?} mean {mean}");
            generate_window(window_type, 15.0, 3175, FftMagnitudeNorm::FullScale, &mut w);
            let sum = w.iter().map(|&x| x as f64).sum::<f64>();
            assert!((sum - 2.0).abs() < 1e-4, "{window_type:?} sum {sum}");
        }
        generate_window(
            FftWindowType::Rectangular,
            0.0,
            8,
            FftMagnitudeNorm::CoherentGain,
            &mut w,
        );
        assert!(w.iter().all(|&x| (x - 1.0).abs() < 1e-6));
    }

    #[test]
    fn kaiser_beta_follows_the_sidelobe_level() {
        assert!((kaiser_beta_for_sidelobe_db(80.0) - 0.12438 * 86.3).abs() < 1e-9);
        assert!(
            (kaiser_beta_for_sidelobe_db(114.3) - 15.0).abs() < 0.01,
            "114.3 dB ↔ β 15: {}",
            kaiser_beta_for_sidelobe_db(114.3)
        );
        assert_eq!(kaiser_beta_for_sidelobe_db(10.0), 0.0);
        let mid = kaiser_beta_for_sidelobe_db(40.0);
        assert!(mid > 0.0 && mid < 10.0, "40 dB: {mid}");

        // The pipeline reports the effective β: auto from the dB range, 0
        // for any other window.
        let mut p = settings();
        p.kaiser_beta_mode = FftKaiserBetaMode::Auto;
        p.db_range = 80.0;
        let mut pipe = SpectrumPipeline::new();
        pipe.ingest(&sine(1000.0, 0.5, 48_000, 4096), 48_000, &p);
        let mut out = Vec::new();
        pipe.process(&p, 33.0, &mut out);
        assert!((pipe.status(&p).kaiser_beta - 10.734).abs() < 1e-3);
        p.kaiser_beta_mode = FftKaiserBetaMode::Manual;
        pipe.process(&p, 33.0, &mut out);
        assert_eq!(pipe.status(&p).kaiser_beta, 15.0);
        p.window_type = FftWindowType::Hann;
        pipe.process(&p, 33.0, &mut out);
        assert_eq!(pipe.status(&p).kaiser_beta, 0.0);
    }

    #[test]
    fn perceptual_scales_round_trip() {
        for hz in [50.0, 440.0, 1000.0, 8000.0, 15000.0] {
            assert!((htk_mel_to_hz(htk_hz_to_mel(hz)) - hz).abs() < 1e-6);
            assert!((erb_rate_to_hz(erb_rate_glasberg(hz)) - hz).abs() < 1e-6);
            assert!((chroma_to_hz(hz_to_chroma(hz)) - hz).abs() < 1e-6);
            // Low (50 Hz), middle and high (8 k / 15 k: above 20.1 Bark)
            // corrections all invert exactly.
            assert!(
                (bark_to_hz(hz_to_bark(hz)) - hz).abs() < 1e-6,
                "Bark round trip at {hz} Hz: {}",
                bark_to_hz(hz_to_bark(hz))
            );
        }
        // The top of a 48 kHz display must not collapse to 0 Hz.
        assert!((bark_to_hz(hz_to_bark(24_000.0)) - 24_000.0).abs() < 1e-6);
    }

    #[test]
    fn linear_grid_with_matching_bins_is_identity() {
        let mut warp = PerceptualWarp::new();
        warp.set_aggregation(FftWarpAggregation::Peak);
        let nlin = 513;
        warp.build_tables(FftScale::Linear, 24_000.0, nlin, 24_000.0, 0.0, 20.0, nlin);
        assert!(warp.is_identity());
        assert_eq!(
            warp.aggregated_bins(),
            0,
            "an identity grid never aggregates"
        );
        let src: Vec<f32> = (0..nlin).map(|i| i as f32).collect();
        let mut out = Vec::new();
        warp.apply(&src, &mut out);
        assert_eq!(out, src);
        // Every bin sits exactly on i · sr / N.
        for (i, hz) in warp.target_hz().iter().enumerate() {
            assert!((hz - i as f64 * 48_000.0 / 1024.0).abs() < 1e-9);
        }
    }

    #[test]
    fn log_grid_is_monotonic_and_spans_floor_to_fmax() {
        let mut grid = Vec::new();
        compute_target_hz_grid(FftScale::Log, 20_000.0, 256, 1.0, 20.0, &mut grid);
        assert!((grid[0] - 20.0).abs() < 1e-6);
        assert!((grid[255] - 20_000.0).abs() < 1e-6);
        assert!(grid.windows(2).all(|w| w[1] > w[0]));
    }

    #[test]
    fn cubic_warp_never_undershoots_zero() {
        let mut warp = PerceptualWarp::new();
        warp.set_interpolation(FftWarpInterp::Cubic);
        warp.build_tables(FftScale::Log, 20_000.0, 300, 24_000.0, 1.0, 20.0, 1025);
        let src: Vec<f32> = (0..1025)
            .map(|i| if i % 7 == 0 { 10.0 } else { 0.0 })
            .collect();
        let mut out = Vec::new();
        warp.apply(&src, &mut out);
        assert_eq!(out.len(), 300);
        assert!(out.iter().all(|&v| v >= 0.0));
    }

    /// Plugin_FFT's aggregation test: a 1024-bin log axis over a 16384-point
    /// transform at 44.1 kHz.
    #[test]
    fn warp_aggregation_never_drops_a_narrow_peak() {
        let nlin = 8193;
        let nyq = 22_050.0;
        let build = |agg: FftWarpAggregation, interp: FftWarpInterp| {
            let mut w = PerceptualWarp::new();
            w.set_aggregation(agg);
            w.set_interpolation(interp);
            w.build_tables(FftScale::Log, nyq, 1024, nyq, 0.963, 20.0, nlin);
            w
        };
        let off = build(FftWarpAggregation::Off, FftWarpInterp::Linear);
        let peak = build(FftWarpAggregation::Peak, FftWarpInterp::Linear);
        let rms = build(FftWarpAggregation::Rms, FftWarpInterp::Linear);
        assert_eq!(off.aggregated_bins(), 0);
        assert!(
            peak.aggregated_bins() > 300,
            "coarse top half aggregates: {}",
            peak.aggregated_bins()
        );
        assert!(peak.max_linear_index() >= nlin - 2);

        // A single-bin partial swept across the upper half: peak mode always
        // reports it at full height, plain interpolation loses it.
        let mut mag = vec![0.0f32; nlin];
        let (mut o1, mut o2) = (Vec::new(), Vec::new());
        let (mut worst_peak, mut worst_off) = (1.0f32, 1.0f32);
        for k in (nlin / 2..nlin - 1).step_by(7) {
            mag.fill(0.0);
            mag[k] = 1.0;
            peak.apply(&mag, &mut o1);
            off.apply(&mag, &mut o2);
            worst_peak = worst_peak.min(max_of(&o1));
            worst_off = worst_off.min(max_of(&o2));
        }
        assert_eq!(worst_peak, 1.0);
        assert!(worst_off < 0.5, "interpolation keeps {worst_off}");

        // RMS of a constant range is the constant.
        mag.fill(2.0);
        rms.apply(&mag, &mut o1);
        assert!(o1.iter().all(|v| (v - 2.0).abs() < 1e-5));

        // The cubic kernel aggregates too.
        let cubic = build(FftWarpAggregation::Peak, FftWarpInterp::Cubic);
        mag.fill(0.0);
        mag[nlin - 100] = 3.0;
        cubic.apply(&mag, &mut o1);
        assert_eq!(max_of(&o1), 3.0);
    }

    #[test]
    fn a_and_c_weighting_are_unity_at_one_khz() {
        let mut w = Vec::new();
        equal_loudness_curve(FftWeighting::A, &[1000.0, 100.0, 10_000.0], &mut w);
        assert!((w[0] - 1.0).abs() < 1e-4, "A @1k = {}", w[0]);
        assert!(w[1] < 0.2, "A rolls off at 100 Hz: {}", w[1]);
        equal_loudness_curve(FftWeighting::C, &[1000.0, 100.0], &mut w);
        assert!((w[0] - 1.0).abs() < 1e-4);
        assert!(w[1] > 0.9, "C is flat at 100 Hz: {}", w[1]);
        equal_loudness_curve(FftWeighting::Itu468, &[1000.0, 6300.0], &mut w);
        assert!((w[0] - 1.0).abs() < 1e-4, "ITU-R 468 @1k = {}", w[0]);
        let peak_db = 20.0 * w[1].log10();
        assert!(
            (peak_db - 12.2).abs() < 0.2,
            "ITU-R 468 @6.3k = {peak_db} dB"
        );
        equal_loudness_curve(FftWeighting::Off, &[1000.0], &mut w);
        assert_eq!(w, vec![1.0]);
    }

    #[test]
    fn fast_log10_is_within_six_thousandths_of_a_db() {
        let mut x = 1e-9f64;
        let mut worst = 0.0f64;
        while x < 1e6 {
            let exact = 20.0 * x.log10();
            let fast = f64::from(fast_20_log10(x as f32));
            // compare against the f32 input actually converted
            let exact_f32 = 20.0 * f64::from(x as f32).log10();
            worst = worst.max((fast - exact_f32).abs());
            assert!((fast - exact).abs() < 0.01);
            x *= 1.037;
        }
        assert!(worst < 0.006, "max error {worst} dB");
    }

    #[test]
    fn db_conversion_floors_and_normalises() {
        // The LUT is within 0.0021 dB per conversion, value and reference.
        let mut v = vec![1.0, 0.1, 1e-9];
        convert_to_db(FftLoudnessMode::Db, 80.0, 1.0, &mut v);
        assert!((v[0]).abs() < 6e-3);
        assert!((v[1] + 20.0).abs() < 6e-3);
        assert!((v[2] + 80.0).abs() < 1e-4, "floored at -80: {}", v[2]);
        let mut n = vec![1.0, 0.1, 1e-9];
        convert_to_db(FftLoudnessMode::DbNormalized, 80.0, 1.0, &mut n);
        assert!((n[0] - 1.0).abs() < 1e-3);
        assert!((n[1] - 0.75).abs() < 1e-3);
        assert!(n[2].abs() < 1e-4);
        // A ramp against its own peak: the top is 0 dB, the middle −6 dB.
        let mut ramp: Vec<f32> = (1..=64).map(|i| i as f32).collect();
        convert_to_db(FftLoudnessMode::Db, 80.0, 1.0 / 64.0, &mut ramp);
        assert!(ramp[63].abs() < 6e-3);
        assert!((ramp[31] - 20.0 * 0.5f32.log10()).abs() < 6e-3);
        let mut zeros = vec![0.0f32; 4];
        convert_to_db(FftLoudnessMode::Db, 80.0, 1.0, &mut zeros);
        assert!(zeros.iter().all(|&v| v == -80.0));
    }

    #[test]
    fn ballistics_attack_and_release_are_asymmetric() {
        let mut prev = vec![0.0, 1.0];
        let mut out = vec![1.0, 0.0];
        apply_ballistics(0.0, 0.0, &mut out, &mut prev, false);
        assert_eq!(prev, vec![1.0, 0.0], "0/0 follows instantly");
        assert_eq!(out, vec![1.0, 0.0]);
        let mut prev = vec![0.0, 1.0];
        let mut out = vec![1.0, 0.0];
        apply_ballistics(0.5, 0.9, &mut out, &mut prev, false);
        assert!((prev[0] - 0.5).abs() < 1e-6, "attack half way: {}", prev[0]);
        assert!((prev[1] - 0.9).abs() < 1e-6, "release slow: {}", prev[1]);
        assert_eq!(out, prev, "the output is the envelope");
        // No history yet, or history of another output size (the axis
        // changed): restart at the current frame, no blend from stale state
        // (Plugin_FFT v2.12 "no history yet: restart at the frame").
        for stale in [vec![], vec![9.0f32; 3], vec![9.0f32; 5]] {
            let frame = vec![0.25f32, 1.0, -3.0, 0.5];
            let mut prev = stale.clone();
            let mut out = frame.clone();
            apply_ballistics(0.3, 0.8, &mut out, &mut prev, false);
            assert_eq!(out, frame, "reseed leaves the frame as it is ({stale:?})");
            assert_eq!(prev, frame, "reseed restarts the history at the frame");
            // The next frame blends from that restart, not from `stale`.
            let mut out = vec![1.25f32, 1.0, -3.0, 0.5];
            apply_ballistics(0.5, 0.8, &mut out, &mut prev, false);
            assert!(
                (out[0] - 0.75).abs() < 1e-6,
                "blend from the restart: {}",
                out[0]
            );
            assert_eq!(&out[1..], &frame[1..]);
        }
        assert!(coef_from_ms(0.0, 33.0) == 0.0);
        let c = coef_from_ms(100.0, 100.0);
        assert!((f64::from(c) - (-1f64).exp()).abs() < 1e-6);
        let c = coef_from_ms(100.0, 16.6667);
        assert!((f64::from(c) - (-0.166667f64).exp()).abs() < 1e-6);
        // Linear mode flushes a decayed envelope instead of going denormal.
        let mut prev = vec![1.5e-20f32];
        let mut out = vec![0.0f32];
        apply_ballistics(0.0, 0.5, &mut out, &mut prev, true);
        assert_eq!(prev[0], 0.0);
        let mut prev = vec![1.5e-20f32];
        let mut out = vec![0.0f32];
        apply_ballistics(0.0, 0.5, &mut out, &mut prev, false);
        assert_eq!(prev[0], 7.5e-21, "dB modes are never flushed");
    }

    #[test]
    fn shelf_eq_boosts_highs_and_bypasses_at_zero_gain() {
        let sr = 48_000.0;
        let mut eq = BiquadEq::new(sr);
        assert!(!eq.update_and_check_active(0.0, 1000.0, 0.0, 200.0, 0.707, 1.0));
        assert!(eq.update_and_check_active(12.0, 1000.0, 0.0, 200.0, 0.707, 1.0));
        // Steady-state gain of a 10 kHz sine after the high shelf ≈ +12 dB.
        let n = 48_000;
        let mut hi = sine(10_000.0, 1.0, 48_000, n);
        eq.process_block_in_place(&mut hi, 1.0);
        let peak = hi[n / 2..].iter().fold(0f32, |m, v| m.max(v.abs()));
        let db = 20.0 * peak.log10();
        assert!((db - 12.0).abs() < 1.0, "high shelf gain {db} dB");
        // A 50 Hz sine is left alone by a 1 kHz high shelf.
        eq.reset();
        let mut lo = sine(50.0, 1.0, 48_000, n);
        eq.process_block_in_place(&mut lo, 1.0);
        let peak = lo[n / 2..].iter().fold(0f32, |m, v| m.max(v.abs()));
        assert!((20.0 * peak.log10()).abs() < 0.5);
    }

    #[test]
    fn pipeline_finds_a_sine_at_its_frequency_in_dbfs() {
        let mut p = settings();
        p.scale = FftScale::Linear;
        p.warp_blend = 0.0;
        p.output_bins = 2048;
        p.fft_size = 8192;
        p.window_samples = 4096;
        p.magnitude_norm = FftMagnitudeNorm::FullScale;
        p.loudness_mode = FftLoudnessMode::Db;
        p.db_reference = FftDbReference::Dbfs;
        p.ballistics_enabled = false;
        p.window_type = FftWindowType::Hann;
        let sr = 48_000u32;
        let hz = 3000.0f32;
        let mut pipe = SpectrumPipeline::new();
        pipe.ingest(&sine(hz, 0.5, sr, 8192), sr, &p);
        let mut out = Vec::new();
        let stats = pipe.process(&p, 33.0, &mut out);
        assert_eq!(out.len(), 2048);
        assert!(
            (stats.peak_hz - hz).abs() < 30.0,
            "peak at {} Hz",
            stats.peak_hz
        );
        // amplitude 0.5 → −6.02 dBFS (Hann coherent gain is normalised away)
        assert!(
            (stats.peak_value + 6.02).abs() < 0.5,
            "peak {} dB",
            stats.peak_value
        );
        let status = pipe.status(&p);
        assert_eq!(status.fft_size, 8192);
        assert_eq!(status.window_samples, 4096);
        assert_eq!(status.output_bins, 2048);
        assert!(!status.identity_warp);
        assert!((status.hz_per_bin - 24_000.0 / 2047.0).abs() < 1e-3);
        assert!(status.axis_version > 0);
    }

    #[test]
    fn pipeline_short_circuits_digital_silence_in_linear_mode() {
        let mut p = settings();
        p.loudness_mode = FftLoudnessMode::Off;
        p.ballistics_enabled = false;
        p.fft_size = 1024;
        p.window_samples = 512;
        let mut pipe = SpectrumPipeline::new();
        pipe.ingest(&vec![0.0; 2048], 16_000, &p);
        let mut out = Vec::new();
        let stats = pipe.process(&p, 33.0, &mut out);
        assert!(stats.silent);
        assert!(out.iter().all(|&v| v == 0.0));
        assert!(stats.dsp_us < 5_000.0);
    }

    /// The silence shortcut must be invisible: in every loudness mode and
    /// reference, with ballistics, weighting, aggregation and the cubic
    /// kernel on, a silent window yields bit for bit what the full chain
    /// computes, frame after frame (AGC decaying, ballistics releasing).
    #[test]
    fn silence_shortcut_equals_the_full_chain_bit_for_bit() {
        let sr = 48_000u32;
        for loudness in [
            FftLoudnessMode::Off,
            FftLoudnessMode::Db,
            FftLoudnessMode::DbNormalized,
        ] {
            for reference in [
                FftDbReference::Dbfs,
                FftDbReference::FramePeak,
                FftDbReference::Agc,
            ] {
                for ballistics in [false, true] {
                    let mut p = settings();
                    p.fft_size = 4096;
                    p.window_samples = 1024;
                    p.output_bins = 300;
                    p.loudness_mode = loudness;
                    p.db_reference = reference;
                    p.ballistics_enabled = ballistics;
                    p.weighting = FftWeighting::A;
                    p.warp_interpolation = FftWarpInterp::Cubic;
                    p.eq_enabled = true;
                    p.high_gain_db = 6.0;
                    let mut fast = SpectrumPipeline::new();
                    let mut full = SpectrumPipeline::new();
                    full.set_silence_shortcut(false);
                    let loud = sine(1000.0, 0.5, sr, 2048);
                    fast.ingest(&loud, sr, &p);
                    full.ingest(&loud, sr, &p);
                    let (mut a, mut b) = (Vec::new(), Vec::new());
                    fast.process(&p, 33.0, &mut a);
                    full.process(&p, 33.0, &mut b);
                    // Long enough for the EQ tail to flush: then the window
                    // is exactly zero and counted silent.
                    let zeros = vec![0.0f32; 480];
                    let mut saw_silent = false;
                    for _ in 0..40 {
                        fast.ingest(&zeros, sr, &p);
                        full.ingest(&zeros, sr, &p);
                        let sa = fast.process(&p, 33.0, &mut a);
                        let sb = full.process(&p, 33.0, &mut b);
                        saw_silent |= sa.silent;
                        assert_eq!(sa.silent, sb.silent);
                        let bits_a: Vec<u32> = a.iter().map(|v| v.to_bits()).collect();
                        let bits_b: Vec<u32> = b.iter().map(|v| v.to_bits()).collect();
                        assert_eq!(
                            bits_a, bits_b,
                            "{loudness:?} / {reference:?} / ballistics {ballistics}"
                        );
                        assert_eq!(sa.peak_value.to_bits(), sb.peak_value.to_bits());
                        assert_eq!(sa.peak_hz.to_bits(), sb.peak_hz.to_bits());
                    }
                    assert!(saw_silent, "the shortcut never engaged");
                }
            }
        }
    }

    #[test]
    fn pipeline_rebuilds_when_the_source_rate_changes() {
        let mut p = settings();
        p.fft_size = 4096;
        p.window_length_mode = FftWindowLengthMode::Milliseconds;
        p.window_ms = 50.0;
        let mut pipe = SpectrumPipeline::new();
        pipe.ingest(&[0.1; 4800], 48_000, &p);
        let mut out = Vec::new();
        pipe.process(&p, 33.0, &mut out);
        assert_eq!(pipe.status(&p).window_samples, 2400);
        let v1 = pipe.status_version();
        let axis1 = pipe.axis_version();
        pipe.ingest(&[0.1; 1600], 16_000, &p);
        pipe.process(&p, 33.0, &mut out);
        assert_eq!(pipe.status(&p).window_samples, 800);
        assert!(pipe.status_version() > v1);
        assert_ne!(
            pipe.axis_version(),
            axis1,
            "a new top frequency is a new axis"
        );
        assert!(
            (pipe.status(&p).display_max_hz - 8000.0).abs() < 1e-3,
            "clamped to Nyquist"
        );
        // An interpolation change rebuilds the tables but keeps the axis.
        let axis2 = pipe.axis_version();
        p.warp_interpolation = FftWarpInterp::Cubic;
        pipe.process(&p, 33.0, &mut out);
        assert_eq!(pipe.axis_version(), axis2);
    }

    #[test]
    fn output_bin_cap_limits_auto_and_raw_but_keeps_the_peak() {
        let sr = 48_000u32;
        let mut p = settings();
        p.fft_size = 65536;
        let tone = sine(3000.0, 0.5, sr, 65536);
        for raw in [false, true] {
            p.raw_bins = raw;
            p.output_bins_mode = FftOutputBinsMode::Auto;
            let mut capped = SpectrumPipeline::new().with_output_bin_cap(1024);
            capped.ingest(&tone, sr, &p);
            let mut out = Vec::new();
            let stats = capped.process(&p, 33.0, &mut out);
            assert_eq!(out.len(), 1024, "raw = {raw}");
            assert_eq!(capped.status(&p).output_bins, 1024);
            assert!(
                (stats.peak_hz - 3000.0).abs() < 60.0,
                "peak {} Hz",
                stats.peak_hz
            );
            // Uncapped, the same settings give the full N/2 + 1 grid.
            let mut full = SpectrumPipeline::new();
            full.ingest(&tone, sr, &p);
            full.process(&p, 33.0, &mut out);
            assert_eq!(out.len(), 32769);
        }
        // Below the cap nothing changes.
        p.raw_bins = false;
        p.output_bins_mode = FftOutputBinsMode::Fixed;
        p.output_bins = 512;
        let mut capped = SpectrumPipeline::new().with_output_bin_cap(1024);
        capped.ingest(&tone, sr, &p);
        let mut out = Vec::new();
        capped.process(&p, 33.0, &mut out);
        assert_eq!(out.len(), 512);
    }

    #[test]
    fn output_bin_modes_and_transform_sizes() {
        let sr = 48_000u32;
        let mut p = settings();
        let run = |p: &LiveFftSettings| {
            let mut pipe = SpectrumPipeline::new();
            pipe.ingest(&sine(1000.0, 0.5, sr, 8192), sr, p);
            let mut out = Vec::new();
            let stats = pipe.process(p, 33.0, &mut out);
            (pipe.status(p), out, stats)
        };
        // Auto: N/2 + 1 of the transform actually run, whatever output_bins says.
        p.output_bins_mode = FftOutputBinsMode::Auto;
        p.fft_size = 4096;
        p.window_samples = 3175;
        let (st, out, _) = run(&p);
        assert_eq!((st.fft_size, st.output_bins), (4096, 2049));
        assert_eq!(out.len(), 2049);
        // Auto ignores output_bins (Plugin_FFT test_v210_rate_helpers: 777 → N/2 + 1).
        p.fft_size = 16384;
        p.output_bins = 777;
        let (st, out, _) = run(&p);
        assert_eq!((st.fft_size, st.output_bins), (16384, 8193));
        assert_eq!(out.len(), 8193);
        p.fft_size = 4096;
        // A 100 ms window at 48 kHz without padding: N = 4800, 2401 bins.
        p.window_length_mode = FftWindowLengthMode::Milliseconds;
        p.window_ms = 100.0;
        p.zero_padding = false;
        let (st, out, _) = run(&p);
        assert_eq!(
            (st.window_samples, st.fft_size, st.output_bins),
            (4800, 4800, 2401)
        );
        assert_eq!(out.len(), 2401);
        p.window_length_mode = FftWindowLengthMode::Samples;
        p.zero_padding = true;
        // A window larger than the pad grows the transform to its next power of two.
        p.window_samples = 5000;
        let (st, _, _) = run(&p);
        assert_eq!((st.fft_size, st.output_bins), (8192, 4097));
        // Without zero padding the transform is the window, rounded up to even.
        p.zero_padding = false;
        p.window_samples = 3175;
        let (st, _, _) = run(&p);
        assert_eq!(
            (st.fft_size, st.linear_bins, st.output_bins),
            (3176, 1589, 1589)
        );
        p.window_samples = 3000;
        let (st, _, _) = run(&p);
        assert_eq!((st.fft_size, st.output_bins), (3000, 1501));
        p.raw_bins = true;
        let (st, _, _) = run(&p);
        assert_eq!((st.fft_size, st.output_bins), (3000, 1501));
        assert!(st.identity_warp);
        // Fixed: exactly output_bins, even above N/2 + 1 (interpolated).
        p.raw_bins = false;
        p.zero_padding = true;
        p.output_bins_mode = FftOutputBinsMode::Fixed;
        p.fft_size = 16384;
        p.window_samples = 4096;
        p.output_bins = 32768;
        p.scale = FftScale::Linear;
        p.warp_blend = 0.0;
        let (st, out, stats) = run(&p);
        assert_eq!((st.linear_bins, st.output_bins), (8193, 32768));
        assert_eq!(out.len(), 32768);
        assert!(
            (stats.peak_hz - 1000.0).abs() < 2.0 * sr as f32 / 16384.0 + 1.0,
            "upsampled peak at {} Hz",
            stats.peak_hz
        );
        p.output_bins = 65536;
        let (st, _, _) = run(&p);
        assert_eq!(st.output_bins, 65536);
        // Fixed keeps its count with zero padding off too (Plugin_FFT
        // test_output_bin_modes: "Fixed no-pad still 32768").
        p.zero_padding = false;
        p.window_samples = 3000;
        p.output_bins = 32768;
        let (st, out, stats) = run(&p);
        assert_eq!(
            (st.fft_size, st.linear_bins, st.output_bins),
            (3000, 1501, 32768)
        );
        assert_eq!(out.len(), 32768);
        assert!(
            (stats.peak_hz - 1000.0).abs() < 2.0 * sr as f32 / 3000.0 + 1.0,
            "unpadded upsampled peak at {} Hz",
            stats.peak_hz
        );
    }

    /// Raw bins are the rfft magnitudes of the windowed, centred frame,
    /// checked against an independent f64 DFT. Scale, display max and
    /// output size are ignored.
    #[test]
    fn raw_bins_are_the_plain_dft_of_the_windowed_frame() {
        let sr = 48_000u32;
        let mut p = settings();
        p.raw_bins = true;
        p.scale = FftScale::Mel;
        p.display_max_hz = 5000.0;
        p.output_bins = 64;
        p.fft_size = 1024;
        p.window_samples = 300;
        p.window_type = FftWindowType::Hann;
        p.loudness_mode = FftLoudnessMode::Off;
        p.ballistics_enabled = false;
        let signal: Vec<f32> = sine(1500.0, 0.5, sr, 700)
            .iter()
            .zip(noise(0.05, 700, 3))
            .map(|(a, b)| a + b)
            .collect();
        let mut pipe = SpectrumPipeline::new();
        pipe.ingest(&signal, sr, &p);
        let mut out = Vec::new();
        let stats = pipe.process(&p, 33.0, &mut out);
        let st = pipe.status(&p);
        assert_eq!(out.len(), 513);
        assert!(st.identity_warp);
        assert!(st.raw_bins);
        assert_eq!(st.display_max_hz, 24_000.0, "the axis is DC..Nyquist");
        assert!((st.hz_per_bin - 24_000.0 / 512.0).abs() < 1e-3);
        assert!((stats.peak_hz - 1500.0).abs() <= 48_000.0 / 1024.0);

        // Independent reference: the frame as the pipeline lays it out.
        let n = 1024usize;
        let win_len = 300usize;
        let mut window = Vec::new();
        generate_window(
            FftWindowType::Hann,
            0.0,
            win_len,
            FftMagnitudeNorm::CoherentGain,
            &mut window,
        );
        let start = ((n - win_len) / 2) & !7;
        let mut frame = vec![0.0f64; n];
        let tail = &signal[signal.len() - win_len..];
        for i in 0..win_len {
            frame[start + i] = f64::from(tail[i] * window[i]);
        }
        let mut reference = vec![0.0f64; n / 2 + 1];
        for (k, r) in reference.iter_mut().enumerate() {
            let (mut re, mut im) = (0.0f64, 0.0f64);
            for (t, &x) in frame.iter().enumerate() {
                let ph = -2.0 * std::f64::consts::PI * (k * t) as f64 / n as f64;
                re += x * ph.cos();
                im += x * ph.sin();
            }
            *r = (re * re + im * im).sqrt();
        }
        let top = reference.iter().cloned().fold(0.0, f64::max);
        for (k, (&got, &want)) in out.iter().zip(&reference).enumerate() {
            assert!(
                (f64::from(got) - want).abs() <= 1e-4 * top,
                "bin {k}: {got} vs {want}"
            );
        }
    }

    #[test]
    fn spectral_features_describe_a_sine_and_white_noise() {
        let sr = 48_000u32;
        let mut p = settings();
        p.spectral_features = true;
        p.scale = FftScale::Linear;
        p.warp_blend = 0.0;
        p.fft_size = 8192;
        p.window_samples = 4096;
        p.window_type = FftWindowType::Hann;
        p.magnitude_norm = FftMagnitudeNorm::FullScale;
        let mut pipe = SpectrumPipeline::new();
        let tone = sine(1000.0, 0.5, sr, 4096);
        pipe.ingest(&tone, sr, &p);
        let mut out = Vec::new();
        let f = pipe.process(&p, 33.0, &mut out).features.expect("features");
        assert!(
            (f.centroid_hz - 1000.0).abs() < 20.0,
            "centroid {}",
            f.centroid_hz
        );
        assert!(
            (f.rolloff_hz - 1000.0).abs() < 20.0,
            "rolloff {}",
            f.rolloff_hz
        );
        assert!(f.flatness < 0.01, "flatness {}", f.flatness);
        let rms = 20.0 * (0.5f32 / 2f32.sqrt()).log10();
        assert!((f.rms_db - rms).abs() < 0.05, "rms {}", f.rms_db);
        assert!(f.mid_db > f.bass_db + 30.0 && f.mid_db > f.high_db + 30.0);
        assert_eq!(f.flux, 0.0, "no previous frame yet");
        // The same frame again has no flux; a level ×4 is an onset.
        let f = pipe.process(&p, 33.0, &mut out).features.unwrap();
        assert!(f.flux < 1e-6, "flux {}", f.flux);
        let louder: Vec<f32> = tone.iter().map(|v| v * 4.0).collect();
        pipe.ingest(&louder, sr, &p);
        let f = pipe.process(&p, 33.0, &mut out).features.unwrap();
        assert!(f.flux > 0.5, "flux {}", f.flux);

        pipe.ingest(&noise(0.2, 4096, 11), sr, &p);
        let f = pipe.process(&p, 33.0, &mut out).features.unwrap();
        assert!(f.flatness > 0.3, "noise flatness {}", f.flatness);
        assert!(
            (f.centroid_hz - sr as f32 / 4.0).abs() < 1500.0,
            "noise centroid {}",
            f.centroid_hz
        );

        // Silence reports the defaults and forgets the flux history.
        pipe.ingest(&vec![0.0; 4096], sr, &p);
        let stats = pipe.process(&p, 33.0, &mut out);
        assert!(stats.silent);
        assert_eq!(stats.features, Some(SpectralFeatures::default()));
        p.spectral_features = false;
        assert!(pipe.process(&p, 33.0, &mut out).features.is_none());
    }

    /// The ballistics history restarts at the current frame when the output
    /// size changes, inside the pipeline: the frame after an axis change
    /// equals the same frame without ballistics, bit for bit.
    #[test]
    fn ballistics_reseed_after_an_output_size_change() {
        let sr = 48_000u32;
        let mut p = settings();
        p.fft_size = 4096;
        p.window_samples = 2048;
        p.output_bins = 512;
        p.ballistics_mode = FftBallisticsMode::Coefficient;
        p.attack = 0.9;
        p.release = 0.9;
        let mut plain = p.clone();
        plain.ballistics_enabled = false;
        let mut slow = SpectrumPipeline::new();
        let mut fast = SpectrumPipeline::new();
        let (mut a, mut b) = (Vec::new(), Vec::new());
        let mut frame = |slow: &mut SpectrumPipeline,
                         fast: &mut SpectrumPipeline,
                         p: &LiveFftSettings,
                         plain: &LiveFftSettings,
                         hz: f32| {
            let x = sine(hz, 0.5, sr, 2048);
            slow.ingest(&x, sr, p);
            fast.ingest(&x, sr, plain);
            slow.process(p, 33.0, &mut a);
            fast.process(plain, 33.0, &mut b);
            let bits = |v: &[f32]| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
            (bits(&a) == bits(&b), a.len())
        };
        assert_eq!(
            frame(&mut slow, &mut fast, &p, &plain, 1000.0),
            (true, 512),
            "empty history"
        );
        assert_eq!(
            frame(&mut slow, &mut fast, &p, &plain, 5000.0),
            (false, 512),
            "blended"
        );
        p.output_bins = 300;
        plain.output_bins = 300;
        assert_eq!(
            frame(&mut slow, &mut fast, &p, &plain, 9000.0),
            (true, 300),
            "a new output size restarts at the frame"
        );
        assert_eq!(
            frame(&mut slow, &mut fast, &p, &plain, 2000.0),
            (false, 300),
            "blends again"
        );
    }

    /// A window rebuild (here: a new auto β from a new dB range) bumps the
    /// status version, so the new β is published at once; a change that
    /// rebuilds nothing does not.
    #[test]
    fn window_rebuild_bumps_the_status_version() {
        let sr = 48_000u32;
        let mut p = settings();
        p.fft_size = 4096;
        p.window_samples = 1024;
        p.window_type = FftWindowType::Kaiser;
        p.kaiser_beta_mode = FftKaiserBetaMode::Auto;
        p.db_range = 80.0;
        let mut pipe = SpectrumPipeline::new();
        let mut out = Vec::new();
        pipe.ingest(&sine(1000.0, 0.5, sr, 1024), sr, &p);
        pipe.process(&p, 33.0, &mut out);
        let beta1 = pipe.status(&p).kaiser_beta;
        let v1 = pipe.status_version();
        pipe.process(&p, 33.0, &mut out);
        assert_eq!(pipe.status_version(), v1, "nothing rebuilt");
        p.attack_ms = 40.0;
        pipe.process(&p, 33.0, &mut out);
        assert_eq!(
            pipe.status_version(),
            v1,
            "a ballistics change rebuilds nothing"
        );
        p.db_range = 120.0;
        pipe.process(&p, 33.0, &mut out);
        assert!(pipe.status_version() > v1, "a β change is a rebuild");
        assert!(pipe.status(&p).kaiser_beta > beta1);
        let v2 = pipe.status_version();
        p.kaiser_beta_mode = FftKaiserBetaMode::Manual;
        p.kaiser_beta = 3.0;
        pipe.process(&p, 33.0, &mut out);
        assert!(pipe.status_version() > v2, "manual β is a rebuild too");
        assert_eq!(pipe.status(&p).kaiser_beta, 3.0);
    }

    /// Plugin_FFT's window limits reach down to a 1-sample window and a
    /// 0.1 ms one: every window type, both zero-padding modes, the EQ, the
    /// weighting, the cubic warp, aggregation, ballistics and the features
    /// stay finite and the sizes follow `fft_size_for`.
    #[test]
    fn one_sample_and_tenth_of_a_millisecond_windows_are_safe() {
        let sr = 48_000u32;
        let windows = [
            FftWindowType::Kaiser,
            FftWindowType::Hann,
            FftWindowType::Hamming,
            FftWindowType::Blackman,
            FftWindowType::BlackmanHarris,
            FftWindowType::Rectangular,
        ];
        // (mode, samples, ms, expected window length at 48 kHz)
        let lengths = [
            (FftWindowLengthMode::Samples, 1u32, 72.0f32, 1usize),
            (FftWindowLengthMode::Milliseconds, 3175, 0.1, 5),
        ];
        for zero_padding in [false, true] {
            for window_type in windows {
                for (mode, samples, ms, cap) in lengths {
                    for bins_mode in [FftOutputBinsMode::Fixed, FftOutputBinsMode::Auto] {
                        let mut p = settings();
                        p.window_length_mode = mode;
                        p.window_samples = samples;
                        p.window_ms = ms;
                        p.zero_padding = zero_padding;
                        p.window_type = window_type;
                        p.fft_size = 16_384;
                        p.output_bins_mode = bins_mode;
                        p.eq_enabled = true;
                        p.low_gain_db = -6.0;
                        p.weighting = FftWeighting::A;
                        p.warp_interpolation = FftWarpInterp::Cubic;
                        p.db_reference = FftDbReference::Agc;
                        p.spectral_features = true;
                        let p = p.normalized();
                        assert_eq!((p.window_samples, p.window_ms), (samples, ms));
                        let n = if zero_padding {
                            16_384
                        } else {
                            cap + (cap & 1)
                        };
                        let what =
                            format!("{window_type:?} L={cap} pad={zero_padding} {bins_mode:?}");
                        let mut pipe = SpectrumPipeline::new();
                        let mut out = Vec::new();
                        let x: Vec<f32> = sine(1000.0, 0.5, sr, 960)
                            .iter()
                            .zip(noise(0.1, 960, 7))
                            .map(|(a, b)| a + b)
                            .collect();
                        for chunk in x.chunks(480).chain(x.chunks(1)) {
                            pipe.ingest(chunk, sr, &p);
                        }
                        for _ in 0..3 {
                            pipe.ingest(&x, sr, &p);
                            let stats = pipe.process(&p, 33.0, &mut out);
                            let st = pipe.status(&p);
                            assert_eq!(st.window_samples as usize, cap, "{what}");
                            assert_eq!(st.fft_size as usize, n, "{what}");
                            assert_eq!(st.linear_bins as usize, n / 2 + 1, "{what}");
                            let bins = match bins_mode {
                                FftOutputBinsMode::Fixed => p.output_bins as usize,
                                FftOutputBinsMode::Auto => n / 2 + 1,
                            };
                            assert_eq!(out.len(), bins, "{what}");
                            assert!(out.iter().all(|v| v.is_finite()), "{what}: {out:?}");
                            assert!(stats.peak_value.is_finite() && stats.peak_hz.is_finite());
                            let f = stats.features.expect("features").to_array();
                            assert!(f.iter().all(|v| v.is_finite()), "{what}: {f:?}");
                        }
                    }
                }
            }
        }
    }

    /* ─────────── allocation gate (Plugin_FFT test_allocation_gate) ─────────── */

    /// Zero heap allocations over 500 steady-state `ingest` + `process`
    /// calls. The counting allocator (`alloc_gate`) counts only on this
    /// thread and only while armed, so tests running in parallel do not
    /// pollute the count.
    #[test]
    fn steady_state_ingest_and_process_allocate_nothing() {
        // The gate itself sees allocations (so a 0 below is not vacuous),
        // and a disarmed thread counts nothing.
        alloc_gate::arm();
        let probe = std::hint::black_box(vec![1u8; 64]);
        let mut grown = std::hint::black_box(Vec::<u32>::with_capacity(1));
        grown.extend_from_slice(&[1, 2, 3, 4]);
        assert!(
            alloc_gate::disarm() >= 2,
            "the counting allocator is installed"
        );
        drop((probe, grown));
        assert_eq!(alloc_gate::disarm(), 0);

        let sr = 48_000u32;
        let signal: Vec<f32> = sine(1000.0, 0.3, sr, 48_000)
            .iter()
            .zip(noise(0.05, 48_000, 5))
            .map(|(a, b)| a + b)
            .collect();
        let full_chain = LiveFftSettings {
            eq_enabled: true,
            low_gain_db: -4.0,
            weighting: FftWeighting::A,
            db_reference: FftDbReference::Agc,
            ballistics_enabled: true,
            ballistics_mode: FftBallisticsMode::Milliseconds,
            warp_interpolation: FftWarpInterp::Cubic,
            spectral_features: true,
            ..settings()
        };
        let cases: [(&str, LiveFftSettings, bool); 5] = [
            ("defaults", settings(), false),
            ("full chain", full_chain, true),
            (
                "raw bins",
                LiveFftSettings {
                    raw_bins: true,
                    ..settings()
                },
                false,
            ),
            (
                "aggregation peak",
                LiveFftSettings {
                    warp_aggregation: FftWarpAggregation::Peak,
                    output_bins: 256,
                    ..settings()
                },
                false,
            ),
            (
                "aggregation rms",
                LiveFftSettings {
                    warp_aggregation: FftWarpAggregation::Rms,
                    output_bins: 256,
                    ..settings()
                },
                false,
            ),
        ];
        for (name, p, encode) in cases {
            let mut pipe = SpectrumPipeline::new();
            let mut out = Vec::new();
            let mut encoded = Vec::new();
            let mut chunks = signal.chunks(480).cycle();
            let mut step = |pipe: &mut SpectrumPipeline, out: &mut Vec<f32>, enc: &mut Vec<u8>| {
                pipe.ingest(chunks.next().unwrap_or(&[]), sr, &p);
                let stats = pipe.process(&p, 10.0, out);
                if encode {
                    let features = stats.features.map(|f| f.to_array());
                    super::super::encode_frame(
                        enc,
                        &super::super::FrameHeader::default(),
                        out,
                        features.as_ref(),
                    );
                }
            };
            // Warm-up: plans, tables, buffers.
            for _ in 0..20 {
                step(&mut pipe, &mut out, &mut encoded);
            }
            if name.starts_with("aggregation") {
                assert!(pipe.status(&p).aggregated_bins > 0, "{name} aggregates");
            }
            alloc_gate::arm();
            for _ in 0..500 {
                step(&mut pipe, &mut out, &mut encoded);
            }
            let allocations = alloc_gate::disarm();
            assert_eq!(
                allocations, 0,
                "{name}: {allocations} allocations over 500 steady-state frames"
            );
        }
    }

    /* ─────────── v2.12 kernels against scalar references ─────────── */

    /// Deterministic uniform values in [0, 1).
    fn uniform(n: usize, seed: u64) -> Vec<f32> {
        noise(0.5, n, seed).iter().map(|v| v + 0.5).collect()
    }

    /// Plugin_FFT v2.12's `FastLog2Seg` bound: < 0.0002 dB from 1e-12 to
    /// 1e6, scalar and through the AVX2 dB kernel; the table fallback stays
    /// within its own 0.0021 dB.
    #[test]
    fn fast_log2_is_within_two_ten_thousandths_of_a_db() {
        let n = 400_000;
        let xs: Vec<f32> = (0..n)
            .map(|i| (-27.6 + 41.4 * (i as f64 / n as f64)).exp() as f32)
            .collect();
        let db = |x: f32| 20.0 * f64::from(x).log10();
        let worst = xs
            .iter()
            .map(|&x| (f64::from(fast_log2(x)) - f64::from(x).log2()).abs())
            .fold(0.0, f64::max);
        assert!(
            worst * DB_PER_OCTAVE_EXACT < 2e-4,
            "scalar {} dB",
            worst * DB_PER_OCTAVE_EXACT
        );
        // floor −400 dB: nothing in the sweep is clamped
        let p = DbParams::new(FftLoudnessMode::Db, 400.0, 1.0);
        #[cfg(target_arch = "x86_64")]
        if avx2_fma() {
            let mut v = xs.clone();
            // SAFETY: AVX2 + FMA are present.
            unsafe { x86::convert_to_db(p, &mut v) };
            let worst = v
                .iter()
                .zip(&xs)
                .map(|(&d, &x)| (f64::from(d) - db(x)).abs())
                .fold(0.0, f64::max);
            assert!(worst < 2e-4, "AVX2 {worst} dB");
        }
        let mut v = xs.clone();
        convert_to_db_portable(p, &mut v);
        let worst = v
            .iter()
            .zip(&xs)
            .map(|(&d, &x)| (f64::from(d) - db(x)).abs())
            .fold(0.0, f64::max);
        assert!(worst < 2.5e-3, "table {worst} dB");
    }

    /// The folded normalised form against the exact formula, with an
    /// exact reference offset.
    #[test]
    fn db_normalised_matches_the_exact_formula() {
        let m: Vec<f32> = uniform(4099, 21)
            .iter()
            .map(|u| ((u - 0.7) * 20.0).exp())
            .collect();
        let exact = |x: f32| {
            ((20.0 * f64::from(x).max(1e-12).log10() + 20.0 * (1.0f64 / 3.0).log10() + 80.0) / 80.0)
                .clamp(0.0, 1.0)
        };
        let worst = |d: &[f32]| {
            d.iter()
                .zip(&m)
                .map(|(&d, &x)| (f64::from(d) - exact(x)).abs() * 80.0)
                .fold(0.0, f64::max)
        };
        let mut d = m.clone();
        convert_to_db(FftLoudnessMode::DbNormalized, 80.0, 1.0 / 3.0, &mut d);
        let bound = if cfg!(target_arch = "x86_64") && dispatches_avx2() {
            4e-4
        } else {
            5e-3
        };
        assert!(worst(&d) < bound, "dispatched: {} dB", worst(&d));
        let mut d = m.clone();
        convert_to_db_portable(
            DbParams::new(FftLoudnessMode::DbNormalized, 80.0, 1.0 / 3.0),
            &mut d,
        );
        assert!(worst(&d) < 5e-3, "table: {} dB", worst(&d));
        // A lone bin (the silence shortcut) gets the same bits as inside a frame.
        let mut frame = m[..37].to_vec();
        convert_to_db(FftLoudnessMode::Db, 90.0, 0.25, &mut frame);
        for (i, &x) in m[..37].iter().enumerate() {
            let mut one = [x];
            convert_to_db(FftLoudnessMode::Db, 90.0, 0.25, &mut one);
            assert_eq!(one[0].to_bits(), frame[i].to_bits(), "bin {i}");
        }
    }

    #[cfg(target_arch = "x86_64")]
    fn dispatches_avx2() -> bool {
        avx2_fma()
    }
    #[cfg(not(target_arch = "x86_64"))]
    fn dispatches_avx2() -> bool {
        false
    }

    /// The first maximum, for every tail length and for random, rising,
    /// tied, constant and NaN-strewn data (a NaN never wins).
    #[test]
    fn peak_index_is_the_first_maximum() {
        let reference = |d: &[f32]| {
            let (mut max, mut idx) = (f32::NEG_INFINITY, 0usize);
            for (i, &v) in d.iter().enumerate() {
                if v > max {
                    max = v;
                    idx = i;
                }
            }
            if d.is_empty() { (0.0, 0) } else { (max, idx) }
        };
        for n in [0usize, 1, 7, 8, 31, 32, 33, 63, 64, 100, 1000, 16384, 16385] {
            let u = uniform(n, n as u64 + 1);
            for pattern in 0..5 {
                let d: Vec<f32> = (0..n)
                    .map(|i| match pattern {
                        0 => u[i],
                        1 => i as f32,
                        2 => ((i * 7) % 5) as f32,
                        3 => 1.0,
                        _ => {
                            if i % 3 == 0 {
                                f32::NAN
                            } else {
                                u[i]
                            }
                        }
                    })
                    .collect();
                let (v, i) = peak_with_index(&d);
                let (rv, ri) = reference(&d);
                assert_eq!(
                    (v.to_bits(), i),
                    (rv.to_bits(), ri),
                    "n {n} pattern {pattern}"
                );
            }
        }
    }

    /// Reductions against plain scalar folds, every length 0…300.
    #[test]
    fn reductions_match_scalar_folds() {
        let u = uniform(300, 5);
        for n in 0..=300 {
            let d = &u[..n];
            let max = d.iter().fold(f32::NEG_INFINITY, |m, &v| m.max(v));
            assert_eq!(max_of(d).to_bits(), max.to_bits(), "max_of {n}");
            let ss: f64 = d.iter().map(|&v| f64::from(v) * f64::from(v)).sum();
            for got in [sum_of_squares(d), sum_of_squares_wide(d)] {
                assert!(
                    (f64::from(got) - ss).abs() <= 1e-5 * ss.max(1e-30),
                    "Σv² {n}: {got} vs {ss}"
                );
            }
        }
    }

    /// −0.0 is silence; a denormal in the tail or a tiny negative sample is
    /// not; the first-sample exit and every block boundary are covered.
    #[test]
    fn silence_check_masks_the_sign_bit() {
        for len in [1usize, 63, 64, 65, 3175] {
            let mut z = vec![0.0f32; len];
            assert!(block_is_silent(&z), "{len}");
            z[len / 2] = -0.0;
            z[0] = -0.0;
            assert!(block_is_silent(&z), "{len} with -0.0");
            z[len - 1] = 1e-40;
            assert!(!block_is_silent(&z), "{len} denormal at the end");
            z[len - 1] = 0.0;
            z[len / 3] = -1e-30;
            assert!(!block_is_silent(&z), "{len} tiny negative");
            z[len / 3] = 0.0;
            z[0] = 0.5;
            assert!(!block_is_silent(&z), "{len} audio");
        }
        assert!(block_is_silent(&[]));
    }

    /// Both warp kernels against an f64 reference on grids that switch
    /// between the permute and the gather path (Log: mixed; Linear
    /// upsampled: all permute; Linear downsampled: all gather), and the
    /// AVX2 kernels against the portable ones bit for bit.
    #[test]
    fn warp_kernels_match_the_scalar_reference() {
        let grids = [
            (FftScale::Log, 16384usize, 8193usize, 0.963),
            (FftScale::Linear, 20000, 4097, 0.0),
            (FftScale::Linear, 1000, 8193, 0.0),
            (FftScale::Mel, 777, 2049, 1.0),
            (FftScale::Log, 37, 513, 0.963),
        ];
        for (gi, &(scale, n_out, nlin, blend)) in grids.iter().enumerate() {
            let src = uniform(nlin, gi as u64 + 40);
            for interp in [FftWarpInterp::Linear, FftWarpInterp::Cubic] {
                let mut w = PerceptualWarp::new();
                w.set_interpolation(interp);
                w.build_tables(scale, 22_050.0, n_out, 22_050.0, blend, 20.0, nlin);
                let mut out = Vec::new();
                w.apply(&src, &mut out);
                assert_eq!(out.len(), n_out);
                let last = nlin - 1;
                let mut worst = 0.0f64;
                for (i, &hz) in w.target_hz().iter().enumerate() {
                    let mut frac = hz / 22_050.0 * (nlin - 1) as f64;
                    if (frac - frac.round()).abs() < 1e-6 {
                        frac = frac.round();
                    }
                    let i0 = frac.floor().clamp(0.0, (nlin - 2) as f64) as usize;
                    let t = (frac - i0 as f64).clamp(0.0, 1.0);
                    let s = |k: usize| f64::from(src[k]);
                    let r = match interp {
                        FftWarpInterp::Linear => s(i0) + t * (s(i0 + 1) - s(i0)),
                        FftWarpInterp::Cubic => {
                            let (p0, p1) = (s(i0.saturating_sub(1)), s(i0));
                            let (p2, p3) = (s((i0 + 1).min(last)), s((i0 + 2).min(last)));
                            (0.5 * (2.0 * p1
                                + (-p0 + p2) * t
                                + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t * t
                                + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t * t * t))
                                .max(0.0)
                        }
                    };
                    worst = worst.max((r - f64::from(out[i])).abs());
                }
                assert!(worst < 1e-5, "grid {gi} {interp:?}: {worst}");

                let (i0, wt) = w.tables();
                let mut portable = vec![0.0f32; n_out];
                match interp {
                    FftWarpInterp::Linear => warp_linear_portable(&src, i0, wt, &mut portable),
                    FftWarpInterp::Cubic => warp_cubic_portable(&src, i0, wt, &mut portable),
                }
                let bits = |v: &[f32]| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
                assert_eq!(bits(&out), bits(&portable), "grid {gi} {interp:?}");
                #[cfg(target_arch = "x86_64")]
                if avx2_fma() {
                    let mut simd = vec![0.0f32; n_out];
                    // SAFETY: AVX2 + FMA are present; the tables come from
                    // `build_tables` for `nlin` bins.
                    unsafe {
                        match interp {
                            FftWarpInterp::Linear => x86::warp_linear(&src, i0, wt, &mut simd),
                            FftWarpInterp::Cubic => x86::warp_cubic(&src, i0, wt, &mut simd),
                        }
                    }
                    assert_eq!(bits(&simd), bits(&portable), "grid {gi} {interp:?} AVX2");
                }
            }
        }
    }

    /// With Peak / RMS aggregation the interpolation stops where the
    /// trailing aggregated run starts: every output bin is still written
    /// (NaN sentinel), the fine part equals the plain interpolation, and a
    /// Peak bin holds an actual FFT-bin value.
    #[test]
    fn aggregation_skip_still_writes_every_bin() {
        for interp in [FftWarpInterp::Linear, FftWarpInterp::Cubic] {
            for agg in [FftWarpAggregation::Peak, FftWarpAggregation::Rms] {
                let src = uniform(8193, 77);
                let build = |a: FftWarpAggregation| {
                    let mut w = PerceptualWarp::new();
                    w.set_interpolation(interp);
                    w.set_aggregation(a);
                    w.build_tables(FftScale::Log, 22_050.0, 8193, 22_050.0, 0.963, 20.0, 8193);
                    w
                };
                let (plain, aggd) = (build(FftWarpAggregation::Off), build(agg));
                assert!(aggd.interp_end < 8193, "a trailing aggregated run exists");
                let mut o1 = Vec::new();
                let mut o2 = vec![f32::NAN; 8193];
                plain.apply(&src, &mut o1);
                aggd.apply(&src, &mut o2);
                let (mut same, mut from_src) = (0usize, 0usize);
                for (i, (&a, &b)) in o1.iter().zip(&o2).enumerate() {
                    assert!(!b.is_nan(), "{interp:?} {agg:?}: bin {i} never written");
                    if a.to_bits() == b.to_bits() {
                        same += 1;
                    } else if agg == FftWarpAggregation::Peak && src.contains(&b) {
                        from_src += 1;
                    }
                }
                assert!(aggd.aggregated_bins() > 100);
                assert!(same + aggd.aggregated_bins() >= o2.len());
                if agg == FftWarpAggregation::Peak {
                    assert_eq!(same + from_src, o2.len(), "{interp:?}");
                }
            }
        }
    }

    /// The fused weighting + reference-peak pass equals the two passes.
    #[test]
    fn weighting_and_reference_peak_in_one_pass() {
        for n in [0usize, 5, 31, 32, 33, 1003] {
            let a = uniform(n, 3);
            let curve: Vec<f32> = uniform(n, 4).iter().map(|v| 0.5 + v).collect();
            let (mut a1, mut a2) = (a.clone(), a.clone());
            let m1 = multiply_in_place_max(&mut a1, &curve);
            for (v, w) in a2.iter_mut().zip(&curve) {
                *v *= *w;
            }
            assert_eq!(a1, a2, "{n}");
            assert_eq!(m1.to_bits(), max_of(&a2).to_bits(), "{n}");
        }
    }

    /// The split-band, lane-summed features against the pre-2.12 f64 loop
    /// (with an exact log), on every code path this CPU can run.
    #[test]
    fn spectral_features_match_the_previous_loop() {
        let n = 8193;
        let bin_hz = 44_100.0 / 16_384.0;
        let mag: Vec<f32> = uniform(n, 9)
            .iter()
            .enumerate()
            .map(|(i, u)| u * (-(i as f64) / 1500.0).exp() as f32)
            .collect();
        let time: Vec<f32> = uniform(3175, 10).iter().map(|v| v - 0.5).collect();
        let prev0: Vec<f32> = mag.iter().map(|v| v * 0.7).collect();

        let b250 = n.min((250.0 / bin_hz) as usize + 1);
        let b4k = n.min((4000.0 / bin_hz) as usize + 1);
        let (mut total, mut weighted, mut sum_mag, mut log_sum) = (0.0f64, 0.0, 0.0, 0.0);
        let (mut bass, mut mid, mut high, mut rise) = (0.0f64, 0.0, 0.0, 0.0);
        for (k, &m) in mag.iter().enumerate() {
            let pw = f64::from(m) * f64::from(m);
            total += pw;
            weighted += pw * k as f64;
            sum_mag += f64::from(m);
            log_sum += 20.0 * f64::from(m.max(1e-12)).log10();
            if k < b250 {
                bass += pw;
            } else if k < b4k {
                mid += pw;
            } else {
                high += pw;
            }
            let d = m - prev0[k];
            if d > 0.0 {
                rise += f64::from(d);
            }
        }
        let (mut acc, mut kr) = (0.0f64, 0usize);
        while kr < n {
            acc += f64::from(mag[kr]) * f64::from(mag[kr]);
            if acc >= 0.85 * total {
                break;
            }
            kr += 1;
        }
        let geo = 10f64.powf((log_sum / n as f64) / 10.0);
        let flatness = (geo / (total / n as f64)).clamp(0.0, 1.0);
        let s2: f64 = time.iter().map(|&x| f64::from(x) * f64::from(x)).sum();
        let rms_db = 20.0 * (s2 / time.len() as f64).sqrt().log10();

        let check = |f: SpectralFeatures, path: &str| {
            let near = |got: f32, want: f64, tol: f64, what: &str| {
                assert!(
                    (f64::from(got) - want).abs() <= tol,
                    "{path} {what}: {got} vs {want}"
                );
            };
            let centroid = weighted / total * bin_hz;
            near(f.centroid_hz, centroid, 1e-5 * centroid, "centroid");
            near(f.rolloff_hz, kr as f64 * bin_hz, bin_hz * 1.01, "rolloff");
            near(f.flatness, flatness, 1e-4 * flatness.max(1e-3), "flatness");
            near(f.flux, rise / sum_mag, 1e-5, "flux");
            near(f.bass_db, 10.0 * bass.log10(), 1e-4, "bass");
            near(f.mid_db, 10.0 * mid.log10(), 1e-4, "mid");
            near(f.high_db, 10.0 * high.log10(), 1e-4, "high");
            near(f.rms_db, rms_db, 1e-4, "rms");
        };
        let (ta, tb) = time.split_at(1000);
        let mut prev = prev0.clone();
        check(
            compute_spectral_features(&mag, bin_hz, &[ta, tb], time.len(), &mut prev),
            "dispatched",
        );
        assert_eq!(prev, mag, "prev holds this frame for the next flux");
        let mut prev = prev0.clone();
        check(
            spectral_features_impl::<false>(&mag, bin_hz, &[ta, tb], time.len(), &mut prev),
            "portable",
        );
    }

    /// Opt-in timing of `process()` (run with
    /// `cargo test --release --lib bench_process -- --ignored --nocapture`).
    #[test]
    #[ignore]
    fn bench_process() {
        let sr = 48_000u32;
        let signal: Vec<f32> = sine(1000.0, 0.3, sr, 48_000)
            .iter()
            .zip(noise(0.05, 48_000, 5))
            .map(|(a, b)| a + b)
            .collect();
        let configs: [(&str, LiveFftSettings); 3] = [
            ("defaults", settings()),
            (
                "raw 32768",
                LiveFftSettings {
                    raw_bins: true,
                    fft_size: 32_768,
                    ..settings()
                },
            ),
            (
                "features + A + eq",
                LiveFftSettings {
                    spectral_features: true,
                    weighting: FftWeighting::A,
                    eq_enabled: true,
                    ..settings()
                },
            ),
        ];
        for (name, p) in configs {
            let mut pipe = SpectrumPipeline::new();
            let mut out = Vec::new();
            let mut times = Vec::new();
            for (i, chunk) in signal.chunks(1600).cycle().take(600).enumerate() {
                pipe.ingest(chunk, sr, &p);
                let t = Instant::now();
                pipe.process(&p, 33.3, &mut out);
                if i >= 100 {
                    times.push(t.elapsed().as_secs_f64() * 1e6);
                }
            }
            times.sort_by(|a, b| a.partial_cmp(b).unwrap());
            println!(
                "{name}: p50 {:.1} µs, p99 {:.1} µs",
                times[times.len() / 2],
                times[times.len() * 99 / 100]
            );
        }
    }
}
