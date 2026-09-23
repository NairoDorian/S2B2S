//! RNNoise noise suppression on the capture path, ahead of the VAD.
//!
//! `nnnoiseless` is a pure-Rust port of Xiph's RNNoise: a small recurrent
//! network that estimates per-band gains for 10 ms frames of 48 kHz audio.
//! The capture path runs at the microphone's native rate and hands the VAD
//! 16 kHz frames, so the chain here is
//!
//! ```text
//! native rate ──▶ 48 kHz, 480-sample frames ──▶ RNNoise ──▶ 16 kHz VAD frames
//! ```
//!
//! With a 48 kHz microphone (the WASAPI shared-mode default) the first stage
//! is pure framing, so the denoiser sees the full-band signal and only one
//! real resample happens, exactly as without suppression. The output frames
//! are the same size the VAD consumes, so everything downstream — hysteresis,
//! smoothing, the speech clock, the live VAD test's score and level — sees
//! the denoised signal. The raw-audio tap and the native analysis tap are
//! upstream of this and stay untouched; the "after noise suppression" tap
//! sees every 48 kHz frame as it leaves the suppressor.
//!
//! RNNoise's model has no knobs, but three things shape its output per frame,
//! all read from atomics the settings commands update ([`DenoiseControls`])
//! so a slider applies on the next frame, mid-recording included:
//!
//! - `strength`: wet/dry mix of the suppressed and the untouched frame;
//! - `vad_threshold`: RNNoise's own per-frame speech probability below which
//!   the frame is muted (0 turns the gate off), and
//! - `vad_grace_ms`: how long audio keeps passing after the last frame above
//!   that threshold, so word endings survive.
//!
//! The gate slews its gain over a few frames instead of cutting, to avoid
//! clicks. RNNoise wants samples in the `i16` range; the chain scales in and
//! out so the rest of the pipeline keeps its `[-1, 1]` floats.

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use nnnoiseless::DenoiseState;

use super::resampler::FrameResampler;

/// The only sample rate RNNoise is trained for.
pub const RNNOISE_SAMPLE_RATE: usize = 48_000;
/// RNNoise frame: 10 ms at 48 kHz.
pub const RNNOISE_FRAME_SAMPLES: usize = DenoiseState::FRAME_SIZE;
/// Duration of one RNNoise frame.
pub const RNNOISE_FRAME_MS: u32 = 10;
const I16_SCALE: f32 = i16::MAX as f32;
/// Fraction of the remaining distance the gate's gain covers per 10 ms
/// frame: about three frames from open to closed and back.
const GATE_SLEW: f32 = 0.5;
/// Longest grace period the settings accept.
pub const MAX_DENOISE_VAD_GRACE_MS: u32 = 5000;

/// The suppressor's tunables (see the module docs).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DenoiseParams {
    /// Wet/dry mix, 0–1: 1 = RNNoise's output, 0 = the input untouched.
    pub strength: f32,
    /// RNNoise's own speech probability below which frames are muted, 0–1;
    /// 0 turns the gate off.
    pub vad_threshold: f32,
    /// How long audio keeps passing after the last frame above the
    /// threshold, in milliseconds.
    pub vad_grace_ms: u32,
}

impl Default for DenoiseParams {
    fn default() -> Self {
        Self {
            strength: 1.0,
            vad_threshold: 0.0,
            vad_grace_ms: 200,
        }
    }
}

impl DenoiseParams {
    /// Clamp into the ranges the chain expects; non-finite floats fall back
    /// to the default.
    pub fn normalized(self) -> Self {
        let d = Self::default();
        let finite = |v: f32, fallback: f32| if v.is_finite() { v } else { fallback };
        Self {
            strength: finite(self.strength, d.strength).clamp(0.0, 1.0),
            vad_threshold: finite(self.vad_threshold, d.vad_threshold).clamp(0.0, 1.0),
            vad_grace_ms: self.vad_grace_ms.min(MAX_DENOISE_VAD_GRACE_MS),
        }
    }
}

/// Lock-free home of the params: written by the settings commands, read by
/// the audio consumer thread once per drained chunk.
pub struct DenoiseControls {
    strength: AtomicU32,
    vad_threshold: AtomicU32,
    vad_grace_ms: AtomicU32,
}

impl DenoiseControls {
    pub fn new(params: DenoiseParams) -> Self {
        let p = params.normalized();
        Self {
            strength: AtomicU32::new(p.strength.to_bits()),
            vad_threshold: AtomicU32::new(p.vad_threshold.to_bits()),
            vad_grace_ms: AtomicU32::new(p.vad_grace_ms),
        }
    }

    pub fn set(&self, params: DenoiseParams) {
        let p = params.normalized();
        self.strength.store(p.strength.to_bits(), Ordering::Relaxed);
        self.vad_threshold
            .store(p.vad_threshold.to_bits(), Ordering::Relaxed);
        self.vad_grace_ms.store(p.vad_grace_ms, Ordering::Relaxed);
    }

    pub fn get(&self) -> DenoiseParams {
        DenoiseParams {
            strength: f32::from_bits(self.strength.load(Ordering::Relaxed)),
            vad_threshold: f32::from_bits(self.vad_threshold.load(Ordering::Relaxed)),
            vad_grace_ms: self.vad_grace_ms.load(Ordering::Relaxed),
        }
    }
}

/// The probability gate: open while RNNoise's speech probability is at or
/// above the threshold and for the grace period after, closed otherwise,
/// with the gain slewed between frames.
#[derive(Debug)]
struct Gate {
    gain: f32,
    grace_left: u32,
}

impl Gate {
    fn new() -> Self {
        Self {
            gain: 1.0,
            grace_left: 0,
        }
    }

    /// The gain to apply to the frame with this speech probability.
    fn update(&mut self, prob: f32, params: &DenoiseParams) -> f32 {
        let target = if params.vad_threshold <= 0.0 {
            1.0
        } else if prob >= params.vad_threshold {
            self.grace_left = params.vad_grace_ms / RNNOISE_FRAME_MS;
            1.0
        } else if self.grace_left > 0 {
            self.grace_left -= 1;
            1.0
        } else {
            0.0
        };
        self.gain += (target - self.gain) * GATE_SLEW;
        if (self.gain - target).abs() < 1e-3 {
            self.gain = target;
        }
        self.gain
    }

    fn reset(&mut self) {
        self.gain = 1.0;
        self.grace_left = 0;
    }
}

/// Native-rate samples in, denoised VAD-sized 16 kHz frames out.
pub struct DenoiseChain {
    to_48k: FrameResampler,
    denoiser: Box<DenoiseState<'static>>,
    to_out: FrameResampler,
    scaled_in: [f32; RNNOISE_FRAME_SAMPLES],
    scaled_out: [f32; RNNOISE_FRAME_SAMPLES],
    /// The previous frame's scaled input: RNNoise's output lags its input by
    /// one frame (overlap-add synthesis), so the dry side of the mix is
    /// delayed to match or a partial strength would comb-filter.
    dry_delay: [f32; RNNOISE_FRAME_SAMPLES],
    frame_out: [f32; RNNOISE_FRAME_SAMPLES],
    gate: Gate,
}

impl DenoiseChain {
    /// `in_hz` is the microphone's native rate, `out_hz` / `out_frame_dur`
    /// the frames the VAD consumes (16 kHz, one detector frame).
    pub fn new(in_hz: usize, out_hz: usize, out_frame_dur: Duration) -> Self {
        Self {
            to_48k: FrameResampler::new(in_hz, RNNOISE_SAMPLE_RATE, Duration::from_millis(10)),
            denoiser: DenoiseState::new(),
            to_out: FrameResampler::new(RNNOISE_SAMPLE_RATE, out_hz, out_frame_dur),
            scaled_in: [0.0; RNNOISE_FRAME_SAMPLES],
            scaled_out: [0.0; RNNOISE_FRAME_SAMPLES],
            dry_delay: [0.0; RNNOISE_FRAME_SAMPLES],
            frame_out: [0.0; RNNOISE_FRAME_SAMPLES],
            gate: Gate::new(),
        }
    }

    /// Feed native-rate samples. `on_denoised` sees every 48 kHz frame as it
    /// leaves the suppressor (the Live FFT "after noise suppression" tap);
    /// `emit` receives whole output frames together with RNNoise's speech
    /// probability of the most recent 48 kHz frame.
    pub fn push(
        &mut self,
        src: &[f32],
        params: DenoiseParams,
        mut on_denoised: impl FnMut(&[f32]),
        mut emit: impl FnMut(&[f32], f32),
    ) {
        let Self {
            to_48k,
            denoiser,
            to_out,
            scaled_in,
            scaled_out,
            dry_delay,
            frame_out,
            gate,
        } = self;
        to_48k.push(src, |frame| {
            let prob = denoise_frame(
                denoiser, gate, &params, frame, scaled_in, scaled_out, dry_delay, frame_out,
            );
            on_denoised(frame_out);
            to_out.push(frame_out, |out| emit(out, prob));
        });
    }

    /// Flush both resamplers' delay lines at the end of a recording.
    pub fn finish(
        &mut self,
        params: DenoiseParams,
        mut on_denoised: impl FnMut(&[f32]),
        mut emit: impl FnMut(&[f32], f32),
    ) {
        let Self {
            to_48k,
            denoiser,
            to_out,
            scaled_in,
            scaled_out,
            dry_delay,
            frame_out,
            gate,
        } = self;
        let mut last_prob = 0.0;
        to_48k.finish(|frame| {
            let prob = denoise_frame(
                denoiser, gate, &params, frame, scaled_in, scaled_out, dry_delay, frame_out,
            );
            last_prob = prob;
            on_denoised(frame_out);
            to_out.push(frame_out, |out| emit(out, prob));
        });
        to_out.finish(|out| emit(out, last_prob));
    }

    /// Forget buffered audio, the gate and the network's recurrent state, so
    /// nothing from one recording (or from before a toggle) leaks into the
    /// next.
    pub fn reset(&mut self) {
        self.to_48k.reset();
        self.to_out.reset();
        self.denoiser = DenoiseState::new();
        self.dry_delay = [0.0; RNNOISE_FRAME_SAMPLES];
        self.gate.reset();
    }
}

/// One 48 kHz frame through RNNoise, the wet/dry mix and the gate. Returns
/// RNNoise's speech probability for the frame. The output is one frame behind
/// the input on both sides of the mix (see `DenoiseChain::dry_delay`).
#[allow(clippy::too_many_arguments)]
fn denoise_frame(
    denoiser: &mut DenoiseState<'static>,
    gate: &mut Gate,
    params: &DenoiseParams,
    frame: &[f32],
    scaled_in: &mut [f32; RNNOISE_FRAME_SAMPLES],
    scaled_out: &mut [f32; RNNOISE_FRAME_SAMPLES],
    dry_delay: &mut [f32; RNNOISE_FRAME_SAMPLES],
    frame_out: &mut [f32; RNNOISE_FRAME_SAMPLES],
) -> f32 {
    debug_assert_eq!(frame.len(), RNNOISE_FRAME_SAMPLES);
    for (dst, src) in scaled_in.iter_mut().zip(frame) {
        *dst = (src * I16_SCALE).clamp(i16::MIN as f32, i16::MAX as f32);
    }
    let prob = denoiser.process_frame(scaled_out, scaled_in);
    let gain = gate.update(prob, params);
    let wet = params.strength;
    let dry = 1.0 - wet;
    for ((dst, dry_sample), wet_sample) in frame_out
        .iter_mut()
        .zip(dry_delay.iter())
        .zip(scaled_out.iter())
    {
        *dst = (((dry_sample * dry + wet_sample * wet) / I16_SCALE) * gain).clamp(-1.0, 1.0);
    }
    dry_delay.copy_from_slice(scaled_in);
    prob
}

#[cfg(test)]
mod tests {
    use super::*;

    const OUT_HZ: usize = 16_000;
    const OUT_FRAME: Duration = Duration::from_millis(16);
    const OUT_FRAME_SAMPLES: usize = 256;

    fn run(in_hz: usize, input: &[f32]) -> Vec<f32> {
        let mut chain = DenoiseChain::new(in_hz, OUT_HZ, OUT_FRAME);
        let params = DenoiseParams::default();
        let mut out = Vec::new();
        chain.push(input, params, |_| {}, |f, _| out.extend_from_slice(f));
        chain.finish(params, |_| {}, |f, _| out.extend_from_slice(f));
        out
    }

    fn sine(len: usize, hz: f32, rate: f32, amp: f32) -> Vec<f32> {
        (0..len)
            .map(|i| (2.0 * std::f32::consts::PI * hz * i as f32 / rate).sin() * amp)
            .collect()
    }

    #[test]
    fn silence_stays_silent_and_keeps_its_length() {
        let out = run(48_000, &vec![0.0f32; 48_000]);
        assert_eq!(out.len() % OUT_FRAME_SAMPLES, 0);
        // One second in → about one second out, give or take resampler
        // padding and a frame of delay.
        assert!((15_000..=17_000).contains(&out.len()), "got {}", out.len());
        assert!(out.iter().all(|s| s.abs() < 1e-3));
    }

    #[test]
    fn output_is_bounded_and_finite_for_loud_input() {
        // A noisy, full-scale signal must come out clamped to [-1, 1] with no
        // NaN, whatever the network decides to keep.
        let mut x = 0.123_456_78f32;
        let input: Vec<f32> = (0..44_100)
            .map(|_| {
                x = (x * 9_301.0 + 49_297.0) % 233_280.0;
                (x / 233_280.0) * 2.0 - 1.0
            })
            .collect();
        let out = run(44_100, &input);
        assert!((15_000..=17_000).contains(&out.len()), "got {}", out.len());
        assert!(
            out.iter()
                .all(|s| s.is_finite() && (-1.0..=1.0).contains(s))
        );
    }

    #[test]
    fn works_at_native_16k_and_reset_clears_state() {
        let mut chain = DenoiseChain::new(16_000, OUT_HZ, OUT_FRAME);
        let params = DenoiseParams::default();
        let mut out = Vec::new();
        chain.push(
            &vec![0.5f32; 8_000],
            params,
            |_| {},
            |f, _| out.extend_from_slice(f),
        );
        chain.finish(params, |_| {}, |f, _| out.extend_from_slice(f));
        assert!(!out.is_empty());

        chain.reset();
        let mut after = Vec::new();
        chain.push(
            &vec![0.0f32; 16_000],
            params,
            |_| {},
            |f, _| after.extend_from_slice(f),
        );
        assert!(
            after.iter().all(|s| s.abs() < 1e-3),
            "audio from before reset leaked into the next session"
        );
    }

    #[test]
    fn the_denoised_observer_sees_every_48k_frame() {
        let mut chain = DenoiseChain::new(48_000, OUT_HZ, OUT_FRAME);
        let input = sine(48_000, 440.0, 48_000.0, 0.3);
        let mut frames = 0usize;
        let mut lengths_ok = true;
        chain.push(
            &input,
            DenoiseParams::default(),
            |f| {
                frames += 1;
                lengths_ok &= f.len() == RNNOISE_FRAME_SAMPLES;
            },
            |_, _| {},
        );
        assert!(lengths_ok);
        assert_eq!(frames, 48_000 / RNNOISE_FRAME_SAMPLES);
    }

    #[test]
    fn strength_zero_passes_the_input_through_one_frame_late() {
        // The dry side is delayed by the frame RNNoise's output lags, so at
        // strength 0 the second output is the first input, untouched.
        let mut denoiser = DenoiseState::new();
        let mut gate = Gate::new();
        let params = DenoiseParams {
            strength: 0.0,
            ..DenoiseParams::default()
        };
        let first = sine(RNNOISE_FRAME_SAMPLES, 1000.0, 48_000.0, 0.5);
        let second = sine(RNNOISE_FRAME_SAMPLES, 250.0, 48_000.0, 0.25);
        let (mut a, mut b, mut delay, mut out) = (
            [0.0; RNNOISE_FRAME_SAMPLES],
            [0.0; RNNOISE_FRAME_SAMPLES],
            [0.0; RNNOISE_FRAME_SAMPLES],
            [0.0; RNNOISE_FRAME_SAMPLES],
        );
        for frame in [&first, &second] {
            denoise_frame(
                &mut denoiser,
                &mut gate,
                &params,
                frame,
                &mut a,
                &mut b,
                &mut delay,
                &mut out,
            );
        }
        for (x, y) in first.iter().zip(out.iter()) {
            assert!((x - y).abs() < 1e-4, "{x} vs {y}");
        }
    }

    #[test]
    fn gate_is_transparent_when_off_and_holds_for_the_grace_period() {
        let off = DenoiseParams::default();
        let mut gate = Gate::new();
        for prob in [0.0, 0.2, 0.9, 0.0] {
            assert_eq!(gate.update(prob, &off), 1.0);
        }

        let on = DenoiseParams {
            vad_threshold: 0.5,
            vad_grace_ms: 30,
            ..DenoiseParams::default()
        };
        let mut gate = Gate::new();
        assert_eq!(gate.update(0.9, &on), 1.0);
        // Three 10 ms frames of grace stay fully open…
        for _ in 0..3 {
            assert_eq!(gate.update(0.1, &on), 1.0);
        }
        // …then the gain slews shut instead of cutting.
        let first = gate.update(0.1, &on);
        assert!(first < 1.0 && first > 0.0, "slewing, got {first}");
        let mut last = first;
        for _ in 0..12 {
            last = gate.update(0.1, &on);
        }
        assert_eq!(last, 0.0);
        // Speech reopens it.
        assert!(gate.update(0.8, &on) > 0.0);
    }

    #[test]
    fn params_normalise_and_round_trip_through_the_controls() {
        let controls = DenoiseControls::new(DenoiseParams {
            strength: 7.0,
            vad_threshold: f32::NAN,
            vad_grace_ms: 999_999,
        });
        let p = controls.get();
        assert_eq!(p.strength, 1.0);
        assert_eq!(p.vad_threshold, 0.0);
        assert_eq!(p.vad_grace_ms, MAX_DENOISE_VAD_GRACE_MS);
        controls.set(DenoiseParams {
            strength: 0.25,
            vad_threshold: 0.6,
            vad_grace_ms: 150,
        });
        assert_eq!(
            controls.get(),
            DenoiseParams {
                strength: 0.25,
                vad_threshold: 0.6,
                vad_grace_ms: 150
            }
        );
    }
}
