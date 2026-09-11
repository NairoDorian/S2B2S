//! Pure DSP of the Live FFT page on a `realfft` real-to-complex transform.
//!
//! Stage order per frame:
//! FIFO window → window function into the centre of the zero-padded frame
//! (8-float aligned) → R2C FFT → magnitude of only the bins
//! the warp reads → full-scale DC/Nyquist fix → psychoacoustic warp (linear
//! or Catmull-Rom, memcpy when the grid is exactly 1:1) → equal-loudness
//! weighting → dB against the selected reference → attack/release
//! ballistics. The EQ runs at ingest on new samples only, stateful and in
//! time order: re-filtering the whole window every
//! frame would restart the IIR from a stale state at every window start.
//!
//! No SIMD intrinsics: at the ≤ 8192 output bins a webview can draw, the
//! auto-vectorised scalar loops cost far less than the JSON emit that
//! follows them, and the code stays portable. `20·log10` uses libm `log10`
//! instead of a mantissa lookup table for the same reason.

use std::sync::Arc;
use std::time::Instant;

use realfft::num_complex::Complex;
use realfft::{RealFftPlanner, RealToComplex};

use crate::settings::{
    FftBallisticsMode, FftDbReference, FftLoudnessMode, FftMagnitudeNorm, FftScale, FftWarpInterp,
    FftWeighting, FftWindowLengthMode, FftWindowType, LiveFftSettings, MAX_FFT_WINDOW_SAMPLES,
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

    /// Linearised copy, oldest sample first; right-aligned and zero-padded
    /// while the buffer is still filling.
    pub fn get(&self, out: &mut Vec<f32>) {
        out.clear();
        out.resize(self.capacity, 0.0);
        if self.filled < self.capacity {
            if self.filled > 0 {
                let start_dest = self.capacity - self.filled;
                if self.idx >= self.filled {
                    out[start_dest..].copy_from_slice(&self.data[self.idx - self.filled..self.idx]);
                } else {
                    let part1 = self.filled - self.idx;
                    out[start_dest..start_dest + part1]
                        .copy_from_slice(&self.data[self.capacity - part1..]);
                    out[start_dest + part1..].copy_from_slice(&self.data[..self.idx]);
                }
            }
            return;
        }
        if self.idx == 0 {
            out.copy_from_slice(&self.data);
        } else {
            let n = self.capacity - self.idx;
            out[..n].copy_from_slice(&self.data[self.idx..]);
            out[n..].copy_from_slice(&self.data[..self.idx]);
        }
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
pub fn erb_rate_glasberg(hz: f64) -> f64 {
    let x = hz / 123.0;
    6.230 * (x * x) + 93.390 * x + 28.520
}
pub fn erb_rate_to_hz(erb: f64) -> f64 {
    let (a, b, c) = (6.230, 93.390, 28.520);
    let x = (-b + (b * b - 4.0 * a * (c - erb)).max(0.0).sqrt()) / (2.0 * a);
    x * 123.0
}
pub fn hz_to_bark(hz: f64) -> f64 {
    let mut z = (26.81 * hz) / (1960.0 + hz) - 0.53;
    if z < 2.0 {
        z += 0.15 * (2.0 - z);
    } else if z > 20.1 {
        z += 0.22 * (z - 20.1);
    }
    z
}
pub fn bark_to_hz(bark: f64) -> f64 {
    let mut z = bark;
    if z < 2.0 {
        z = (z - 0.3) / 0.85;
    } else if z > 20.1 {
        z = (z - 4.422) / 0.78;
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

/// Re-maps the linear magnitude spectrum onto the perceptual grid.
pub struct PerceptualWarp {
    target_hz: Vec<f64>,
    i0: Vec<u32>,
    w: Vec<f32>,
    nlin: usize,
    max_index: usize,
    interp: FftWarpInterp,
    is_identity: bool,
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
        }
    }

    pub fn set_interpolation(&mut self, interp: FftWarpInterp) {
        self.interp = interp;
    }

    /// Highest linear bin the warp reads (cubic look-ahead included).
    /// Magnitudes above it need not be computed.
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
    }

    pub fn apply(&self, linear: &[f32], out: &mut Vec<f32>) {
        let n_out = self.i0.len();
        out.clear();
        out.resize(n_out, 0.0);
        if n_out == 0 {
            return;
        }
        if self.is_identity && linear.len() == n_out {
            out.copy_from_slice(linear);
            return;
        }
        if linear.len() < self.nlin || self.nlin < 2 {
            return;
        }
        match self.interp {
            FftWarpInterp::Cubic => self.apply_cubic(linear, out),
            FftWarpInterp::Linear => {
                for ((dst, &i0), &w) in out.iter_mut().zip(&self.i0).zip(&self.w) {
                    let i0 = i0 as usize;
                    let a = linear[i0];
                    let b = linear[i0 + 1];
                    *dst = a + w * (b - a);
                }
            }
        }
    }

    /// Catmull-Rom: `0.5·(2p1 + (−p0+p2)t + (2p0−5p1+4p2−p3)t² + (−p0+3p1−3p2+p3)t³)`,
    /// clamped at zero (magnitudes cannot undershoot).
    fn apply_cubic(&self, src: &[f32], dst: &mut [f32]) {
        let last = src.len() - 1;
        if src.len() < 4 {
            for ((d, &i0), &w) in dst.iter_mut().zip(&self.i0).zip(&self.w) {
                let i0 = i0 as usize;
                *d = src[i0] + w * (src[i0 + 1] - src[i0]);
            }
            return;
        }
        for ((d, &i0), &t) in dst.iter_mut().zip(&self.i0).zip(&self.w) {
            let i0 = i0 as usize;
            let p0 = src[i0.saturating_sub(1)];
            let p1 = src[i0];
            let p2 = src[(i0 + 1).min(last)];
            let p3 = src[(i0 + 2).min(last)];
            let v = 0.5
                * (2.0 * p1
                    + (-p0 + p2) * t
                    + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t * t
                    + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t * t * t);
            *d = v.max(0.0);
        }
    }
}

/* ───────────────────────── 5. equal-loudness weighting ───────────────────────── */

/// Linear weighting factor per output bin: A / C (IEC 61672, unity at
/// 1 kHz) or ITU-R 468.
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
        _ => 1.0,
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
            FftWeighting::Itu468 => {
                let f2 = f * f;
                let f3 = f2 * f;
                let f4 = f2 * f2;
                let f5 = f4 * f;
                let f6 = f3 * f3;
                let h1 = -4.737338981378384e-24 * f6 + 2.043828333266122e-15 * f4
                    - 1.363894795463638e-7 * f2
                    + 1.0;
                let h2 = 1.306612257412824e-19 * f5 - 2.118150887518656e-11 * f3
                    + 5.559488023498642e-4 * f;
                (1.246332637532143e-4 * f) / (h1 * h1 + h2 * h2).max(1e-12).sqrt()
            }
            FftWeighting::Off => 1.0,
        };
        *w = value as f32;
    }
}

/* ───────────────────────── 6. decibels ───────────────────────── */

/// In place: `20·log10(x · inv_ref)` floored at `−top_db`, or that mapped
/// onto `[0, 1]` for the normalised mode. `inv_ref` is `1 / reference`.
pub fn convert_to_db(mode: FftLoudnessMode, top_db: f64, inv_ref: f32, spectrum: &mut [f32]) {
    if mode == FftLoudnessMode::Off || spectrum.is_empty() {
        return;
    }
    let inv_ref = if inv_ref > 0.0 && inv_ref.is_finite() {
        inv_ref
    } else {
        1.0
    };
    let db_offset = 20.0 * inv_ref.log10();
    let floor = -(top_db as f32);
    let inv_top = (1.0 / top_db.max(1e-6)) as f32;
    const MIN_MAG: f32 = 1e-12;
    let normalise = mode == FftLoudnessMode::DbNormalized;
    for v in spectrum.iter_mut() {
        let raw = v.max(MIN_MAG);
        let db = (20.0 * raw.log10() + db_offset).max(floor);
        *v = if normalise {
            ((db - floor) * inv_top).clamp(0.0, 1.0)
        } else {
            db
        };
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

/// Asymmetric attack / release envelope: `prev += f · (cur − prev)` with `f`
/// chosen per bin by the sign of the difference. `prev_out` is (re)seeded
/// with `current` when it has the wrong size or both coefficients are zero.
pub fn apply_ballistics(attack: f32, release: f32, current: &[f32], prev_out: &mut Vec<f32>) {
    if prev_out.len() != current.len() || (attack <= 0.0 && release <= 0.0) {
        prev_out.clear();
        prev_out.extend_from_slice(current);
        return;
    }
    let att = 1.0 - attack.clamp(0.0, 0.999);
    let rel = 1.0 - release.clamp(0.0, 0.999);
    for (dst, &src) in prev_out.iter_mut().zip(current) {
        let diff = src - *dst;
        *dst += if diff > 0.0 { att } else { rel } * diff;
    }
}

/* ───────────────────────── helpers ───────────────────────── */

/// True when every sample is exactly ±0.0 (digital silence).
pub fn block_is_silent(x: &[f32]) -> bool {
    x.iter().all(|s| s.to_bits() & 0x7FFF_FFFF == 0)
}

/// Maximum value and its index (0 / 0.0 for an empty slice).
pub fn peak_with_index(data: &[f32]) -> (f32, usize) {
    let mut max = f32::NEG_INFINITY;
    let mut idx = 0;
    for (i, &v) in data.iter().enumerate() {
        if v > max {
            max = v;
            idx = i;
        }
    }
    if data.is_empty() {
        (0.0, 0)
    } else {
        (max, idx)
    }
}

/* ───────────────────────── 8. the pipeline ───────────────────────── */

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
    /// `display_max_hz` after the Nyquist clamp.
    pub display_max_hz: f32,
}

/// Per-frame telemetry.
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameStats {
    pub peak_hz: f32,
    pub peak_value: f32,
    pub silent: bool,
    pub dsp_us: f32,
}

#[derive(Clone, Copy, PartialEq)]
struct WindowKey {
    window_type: FftWindowType,
    kaiser_beta: f32,
    capacity: usize,
    norm: FftMagnitudeNorm,
}

#[derive(Clone, Copy, PartialEq)]
struct WarpKey {
    scale: FftScale,
    fmax: f64,
    bins: u32,
    warp: f32,
    log_floor: f32,
    nlin: usize,
    nyquist: f64,
    interp: FftWarpInterp,
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
    pad_choice: u32,
    fft: Option<Arc<dyn RealToComplex<f32>>>,
    scratch: Vec<Complex<f32>>,
    spectrum: Vec<Complex<f32>>,
    padded: Vec<f32>,
    magnitude: Vec<f32>,
    magnitude_bins: usize,
    fifo: Fifo,
    window_in: Vec<f32>,
    block: Vec<f32>,
    silent_run: usize,
    eq: BiquadEq,
    window: Vec<f32>,
    window_key: Option<WindowKey>,
    warp: PerceptualWarp,
    warp_key: Option<WarpKey>,
    warp_version: u64,
    weighting: Vec<f32>,
    weight_key: Option<(FftWeighting, u64)>,
    prev_spectrum: Vec<f32>,
    agc_peak: f32,
    prev_loudness: FftLoudnessMode,
    status_version: u64,
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
            pad_choice: 0,
            fft: None,
            scratch: Vec::new(),
            spectrum: Vec::new(),
            padded: Vec::new(),
            magnitude: Vec::new(),
            magnitude_bins: 0,
            fifo: Fifo::new(1),
            window_in: Vec::new(),
            block: Vec::new(),
            silent_run: 0,
            eq: BiquadEq::new(48_000.0),
            window: Vec::new(),
            window_key: None,
            warp: PerceptualWarp::new(),
            warp_key: None,
            warp_version: 0,
            weighting: Vec::new(),
            weight_key: None,
            prev_spectrum: Vec::new(),
            agc_peak: 0.0,
            prev_loudness: FftLoudnessMode::Off,
            status_version: 0,
        }
    }

    /// Bumped whenever the transform or the warp tables were rebuilt.
    pub fn status_version(&self) -> u64 {
        self.status_version
    }

    pub fn target_hz(&self) -> &[f64] {
        self.warp.target_hz()
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

    /// Rebuild the transform when the rate, window or pad length changed.
    /// Returns true when it did.
    fn prepare(&mut self, p: &LiveFftSettings, sample_rate: u32) -> bool {
        let sample_rate = sample_rate.max(1);
        let capacity = Self::window_samples_for(p, sample_rate);
        if self.fft.is_some()
            && self.sample_rate == sample_rate
            && self.capacity == capacity
            && self.pad_choice == p.fft_size
        {
            return false;
        }
        let rate_changed = self.sample_rate != sample_rate;
        let capacity_changed = self.capacity != capacity;
        self.sample_rate = sample_rate;
        self.capacity = capacity;
        self.pad_choice = p.fft_size;

        // FFT size ≥ zero-pad length and ≥ next power of two of the window,
        // so the window never overflows the frame.
        self.fft_size = (p.fft_size as usize)
            .max(capacity.next_power_of_two())
            .max(2);
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
        self.agc_peak = 0.0;
        self.status_version += 1;
        true
    }

    /// Feed new samples: only the tail
    /// that still fits the window is kept, digital silence is tracked, the
    /// EQ runs on the new samples in time order, then they enter the FIFO.
    pub fn ingest(&mut self, samples: &[f32], sample_rate: u32, p: &LiveFftSettings) {
        self.prepare(p, sample_rate);
        if samples.is_empty() {
            return;
        }
        let keep = samples.len().min(self.capacity);
        let tail = &samples[samples.len() - keep..];

        if block_is_silent(tail) {
            self.silent_run = (self.silent_run + keep).min(self.capacity * 2);
        } else {
            self.silent_run = 0;
        }

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
        if eq_active {
            self.block.clear();
            self.block.extend_from_slice(tail);
            self.eq
                .process_block_in_place(&mut self.block, f64::from(p.eq_amount));
            self.fifo.add(&self.block);
        } else {
            self.fifo.add(tail);
        }
    }

    fn update_window(&mut self, p: &LiveFftSettings) {
        let key = WindowKey {
            window_type: p.window_type,
            kaiser_beta: p.kaiser_beta,
            capacity: self.capacity,
            norm: p.magnitude_norm,
        };
        if self.window_key == Some(key) && self.window.len() == self.capacity {
            return;
        }
        generate_window(
            p.window_type,
            f64::from(p.kaiser_beta),
            self.capacity,
            p.magnitude_norm,
            &mut self.window,
        );
        self.window_key = Some(key);
    }

    fn update_warp(&mut self, p: &LiveFftSettings) {
        let nlin = self.fft_size / 2 + 1;
        let nyquist = f64::from(self.sample_rate) / 2.0;
        let fmax = f64::from(p.display_max_hz).min(nyquist).max(1.0);
        let key = WarpKey {
            scale: p.scale,
            fmax,
            bins: p.output_bins,
            warp: p.warp_blend,
            log_floor: p.log_floor_hz,
            nlin,
            nyquist,
            interp: p.warp_interpolation,
        };
        if self.warp_key == Some(key) {
            return;
        }
        self.warp.set_interpolation(p.warp_interpolation);
        self.warp.build_tables(
            p.scale,
            fmax,
            p.output_bins as usize,
            nyquist,
            f64::from(p.warp_blend),
            f64::from(p.log_floor_hz),
            nlin,
        );
        self.magnitude_bins = nlin.min(self.warp.max_linear_index() + 1);
        self.warp_key = Some(key);
        self.warp_version += 1;
        self.status_version += 1;
    }

    fn update_weighting(&mut self, p: &LiveFftSettings) {
        let key = (p.weighting, self.warp_version);
        if self.weight_key == Some(key) && self.weighting.len() == self.warp.output_bins() {
            return;
        }
        equal_loudness_curve(p.weighting, self.warp.target_hz(), &mut self.weighting);
        self.weight_key = Some(key);
    }

    /// Clear ballistics, AGC and EQ state (the page's Reset button).
    pub fn reset_state(&mut self) {
        self.prev_spectrum.clear();
        self.agc_peak = 0.0;
        self.eq.reset();
    }

    pub fn status(&self, p: &LiveFftSettings) -> PipelineStatus {
        let nyquist = self.sample_rate as f32 / 2.0;
        PipelineStatus {
            sample_rate: self.sample_rate,
            fft_size: self.fft_size as u32,
            window_samples: self.capacity as u32,
            linear_bins: (self.fft_size / 2 + 1) as u32,
            magnitude_bins: self.magnitude_bins as u32,
            output_bins: self.warp.output_bins() as u32,
            identity_warp: self.warp.is_identity(),
            full_scale_ref: match p.magnitude_norm {
                FftMagnitudeNorm::FullScale => 1.0,
                FftMagnitudeNorm::CoherentGain => self.capacity as f32 * 0.5,
            },
            display_max_hz: p.display_max_hz.min(nyquist).max(1.0),
        }
    }

    /// One analysis frame into `out` (`p.output_bins` values). `dt_ms` is
    /// the time since the previous frame, for the millisecond ballistics and
    /// the AGC follower.
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

        self.fifo.get(&mut self.window_in);
        let silent = self.silent_run >= self.capacity;
        let bins = self.warp.output_bins();
        out.clear();
        out.resize(bins, 0.0);

        // Digital silence: nothing to analyse. dB modes need the floor value
        // and ballistics need to decay, so this only applies to the plain
        // linear path.
        if silent && p.loudness_mode == FftLoudnessMode::Off && !p.ballistics_enabled {
            self.prev_spectrum.clear();
            return FrameStats {
                peak_hz: 0.0,
                peak_value: 0.0,
                silent,
                dsp_us: t0.elapsed().as_secs_f32() * 1e6,
            };
        }

        // 1. window into the aligned centre of the zero-padded frame. realfft
        //    uses its input as scratch, so the pad region is re-zeroed each time.
        self.padded.fill(0.0);
        let win_len = self
            .capacity
            .min(self.window.len())
            .min(self.window_in.len());
        for i in 0..win_len {
            self.padded[self.pad_start + i] = self.window_in[i] * self.window[i];
        }

        // 2. FFT + magnitude (only the bins the warp reads)
        if let Some(fft) = &self.fft
            && fft
                .process_with_scratch(&mut self.padded, &mut self.spectrum, &mut self.scratch)
                .is_err()
        {
            self.spectrum
                .iter_mut()
                .for_each(|c| *c = Complex::new(0.0, 0.0));
        }
        let n_mag = self.magnitude_bins.min(self.spectrum.len());
        for (m, c) in self.magnitude[..n_mag]
            .iter_mut()
            .zip(&self.spectrum[..n_mag])
        {
            *m = c.norm();
        }

        // 2b. full scale: DC / Nyquist have no mirror bin
        if p.magnitude_norm == FftMagnitudeNorm::FullScale && self.magnitude.len() >= 2 {
            self.magnitude[0] *= 0.5;
            let last = self.magnitude.len() - 1;
            self.magnitude[last] *= 0.5;
        }

        // 3. psychoacoustic warp
        self.warp.apply(&self.magnitude, out);

        // 4. equal-loudness weighting
        if p.weighting != FftWeighting::Off && self.weighting.len() == out.len() {
            for (v, w) in out.iter_mut().zip(&self.weighting) {
                *v *= *w;
            }
        }

        // 5. dB with the selected reference
        if p.loudness_mode != FftLoudnessMode::Off {
            if self.prev_loudness == FftLoudnessMode::Off {
                self.prev_spectrum.clear();
            }
            let (peak, _) = peak_with_index(out);
            let mut reference = match p.db_reference {
                FftDbReference::Dbfs => match p.magnitude_norm {
                    FftMagnitudeNorm::FullScale => 1.0,
                    FftMagnitudeNorm::CoherentGain => self.capacity as f32 * 0.5,
                },
                FftDbReference::Agc => {
                    self.agc_peak = peak.max(self.agc_peak * agc_decay);
                    self.agc_peak
                }
                FftDbReference::FramePeak => peak,
            };
            if !reference.is_finite() || reference <= 0.0 {
                reference = 1.0;
            }
            convert_to_db(p.loudness_mode, f64::from(p.db_range), 1.0 / reference, out);
        } else if self.prev_loudness != FftLoudnessMode::Off {
            self.prev_spectrum.clear();
        }
        self.prev_loudness = p.loudness_mode;

        // 6. ballistics (bypassed at 0/0)
        if attack_coef > 0.0 || release_coef > 0.0 {
            apply_ballistics(attack_coef, release_coef, out, &mut self.prev_spectrum);
            out.copy_from_slice(&self.prev_spectrum);
        }

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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> LiveFftSettings {
        LiveFftSettings::default().normalized()
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

    #[test]
    fn coherent_gain_window_has_unit_mean_and_full_scale_sums_to_two() {
        let mut w = Vec::new();
        generate_window(
            FftWindowType::Kaiser,
            15.0,
            1000,
            FftMagnitudeNorm::CoherentGain,
            &mut w,
        );
        let mean = w.iter().map(|&x| x as f64).sum::<f64>() / w.len() as f64;
        assert!((mean - 1.0).abs() < 1e-4, "mean {mean}");
        generate_window(
            FftWindowType::Hann,
            0.0,
            1000,
            FftMagnitudeNorm::FullScale,
            &mut w,
        );
        let sum = w.iter().map(|&x| x as f64).sum::<f64>();
        assert!((sum - 2.0).abs() < 1e-4, "sum {sum}");
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
    fn perceptual_scales_round_trip() {
        for hz in [50.0, 440.0, 1000.0, 8000.0, 15000.0] {
            assert!((htk_mel_to_hz(htk_hz_to_mel(hz)) - hz).abs() < 1e-6);
            assert!((erb_rate_to_hz(erb_rate_glasberg(hz)) - hz).abs() < 1e-6);
            assert!((chroma_to_hz(hz_to_chroma(hz)) - hz).abs() < 1e-6);
        }
        // Bark is only invertible in its middle range.
        let hz = 1000.0;
        assert!((bark_to_hz(hz_to_bark(hz)) - hz).abs() < 1.0);
    }

    #[test]
    fn linear_grid_with_matching_bins_is_identity() {
        let mut warp = PerceptualWarp::new();
        let nlin = 513;
        warp.build_tables(FftScale::Linear, 24_000.0, nlin, 24_000.0, 0.0, 20.0, nlin);
        assert!(warp.is_identity());
        let src: Vec<f32> = (0..nlin).map(|i| i as f32).collect();
        let mut out = Vec::new();
        warp.apply(&src, &mut out);
        assert_eq!(out, src);
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

    #[test]
    fn a_and_c_weighting_are_unity_at_one_khz() {
        let mut w = Vec::new();
        equal_loudness_curve(FftWeighting::A, &[1000.0, 100.0, 10_000.0], &mut w);
        assert!((w[0] - 1.0).abs() < 1e-4, "A @1k = {}", w[0]);
        assert!(w[1] < 0.2, "A rolls off at 100 Hz: {}", w[1]);
        equal_loudness_curve(FftWeighting::C, &[1000.0, 100.0], &mut w);
        assert!((w[0] - 1.0).abs() < 1e-4);
        assert!(w[1] > 0.9, "C is flat at 100 Hz: {}", w[1]);
        equal_loudness_curve(FftWeighting::Off, &[1000.0], &mut w);
        assert_eq!(w, vec![1.0]);
    }

    #[test]
    fn db_conversion_floors_and_normalises() {
        let mut v = vec![1.0, 0.1, 1e-9];
        convert_to_db(FftLoudnessMode::Db, 80.0, 1.0, &mut v);
        assert!((v[0]).abs() < 1e-4);
        assert!((v[1] + 20.0).abs() < 1e-3);
        assert!((v[2] + 80.0).abs() < 1e-4, "floored at -80: {}", v[2]);
        let mut n = vec![1.0, 0.1, 1e-9];
        convert_to_db(FftLoudnessMode::DbNormalized, 80.0, 1.0, &mut n);
        assert!((n[0] - 1.0).abs() < 1e-4);
        assert!((n[1] - 0.75).abs() < 1e-3);
        assert!(n[2].abs() < 1e-4);
    }

    #[test]
    fn ballistics_attack_and_release_are_asymmetric() {
        let mut prev = vec![0.0, 1.0];
        apply_ballistics(0.0, 0.0, &[1.0, 0.0], &mut prev);
        assert_eq!(prev, vec![1.0, 0.0], "0/0 follows instantly");
        let mut prev = vec![0.0, 1.0];
        apply_ballistics(0.5, 0.9, &[1.0, 0.0], &mut prev);
        assert!((prev[0] - 0.5).abs() < 1e-6, "attack half way: {}", prev[0]);
        assert!((prev[1] - 0.9).abs() < 1e-6, "release slow: {}", prev[1]);
        assert!(coef_from_ms(0.0, 33.0) == 0.0);
        let c = coef_from_ms(100.0, 100.0);
        assert!((f64::from(c) - (-1f64).exp()).abs() < 1e-6);
    }

    #[test]
    fn shelf_eq_boosts_highs_and_bypasses_at_zero_gain() {
        let sr = 48_000.0;
        let mut eq = BiquadEq::new(sr);
        assert!(!eq.update_and_check_active(0.0, 1000.0, 0.0, 200.0, 0.707, 1.0));
        assert!(eq.update_and_check_active(12.0, 1000.0, 0.0, 200.0, 0.707, 1.0));
        // Steady-state gain of a 10 kHz sine after the high shelf ≈ +12 dB.
        let n = 48_000;
        let mut hi: Vec<f32> = (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * 10_000.0 * i as f32 / sr as f32).sin())
            .collect();
        eq.process_block_in_place(&mut hi, 1.0);
        let peak = hi[n / 2..].iter().fold(0f32, |m, v| m.max(v.abs()));
        let db = 20.0 * peak.log10();
        assert!((db - 12.0).abs() < 1.0, "high shelf gain {db} dB");
        // A 50 Hz sine is left alone by a 1 kHz high shelf.
        eq.reset();
        let mut lo: Vec<f32> = (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * 50.0 * i as f32 / sr as f32).sin())
            .collect();
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
        let amp = 0.5f32;
        let sine: Vec<f32> = (0..8192)
            .map(|i| amp * (2.0 * std::f32::consts::PI * hz * i as f32 / sr as f32).sin())
            .collect();
        let mut pipe = SpectrumPipeline::new();
        pipe.ingest(&sine, sr, &p);
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
        pipe.ingest(&[0.1; 1600], 16_000, &p);
        pipe.process(&p, 33.0, &mut out);
        assert_eq!(pipe.status(&p).window_samples, 800);
        assert!(pipe.status_version() > v1);
        assert!(
            (pipe.status(&p).display_max_hz - 8000.0).abs() < 1e-3,
            "clamped to Nyquist"
        );
    }
}
