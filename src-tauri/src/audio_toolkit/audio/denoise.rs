//! RNNoise noise suppression on the capture path, ahead of the VAD.
//!
//! The suppressor itself is the engine's ([`transcribe_cpp::Denoiser`]): classic
//! RNNoise with its bundled model, the wet/dry mix and the speech-probability
//! gate in one SIMD C++ implementation — the algorithm of the `nnnoiseless`
//! crate this module used to wrap, output within one int16 step of it and
//! about 1.4× faster. What stays here is the framing around it. The capture
//! path runs at the microphone's native rate and hands the VAD 16 kHz frames,
//! so the chain is
//!
//! ```text
//! native rate ──▶ 48 kHz, 480-sample frames ──▶ Denoiser ──▶ 16 kHz VAD frames
//! ```
//!
//! With a 48 kHz microphone (the WASAPI shared-mode default) the first stage
//! is pure framing, so the denoiser sees the full-band signal and only one
//! real resample happens, exactly as without suppression. The output frames
//! are the same size the VAD consumes, so everything downstream — the speech
//! gate, the speech clock, the live VAD test's score and level — sees the
//! denoised signal. The raw-audio tap and the native analysis tap are upstream
//! of this and stay untouched; the "after noise suppression" tap sees every
//! 48 kHz frame as it leaves the suppressor.
//!
//! Three tunables shape the output per frame ([`DenoiseParams`]), read from
//! atomics the settings commands update ([`DenoiseControls`]) so a slider
//! applies on the next frame, mid-recording included:
//!
//! - `strength`: wet/dry mix of the suppressed and the untouched frame;
//! - `vad_threshold`: RNNoise's own per-frame speech probability below which
//!   the frame is muted (0 turns the gate off), and
//! - `vad_grace_ms`: how long audio keeps passing after the last frame above
//!   that threshold, so word endings survive.

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

pub use transcribe_cpp::DenoiseParams;
use transcribe_cpp::{DENOISE_FRAME_SAMPLES, DENOISE_MAX_GRACE_MS, DENOISE_SAMPLE_RATE, Denoiser};

use super::resampler::FrameResampler;

/// The only sample rate RNNoise is trained for.
pub const RNNOISE_SAMPLE_RATE: usize = DENOISE_SAMPLE_RATE as usize;
/// RNNoise frame: 10 ms at 48 kHz.
pub const RNNOISE_FRAME_SAMPLES: usize = DENOISE_FRAME_SAMPLES;
/// Longest grace period the settings accept.
pub const MAX_DENOISE_VAD_GRACE_MS: u32 = DENOISE_MAX_GRACE_MS;

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

/// Native-rate samples in, denoised VAD-sized 16 kHz frames out.
pub struct DenoiseChain {
    to_48k: FrameResampler,
    denoiser: Denoiser,
    /// The params the denoiser holds, so `set_params` crosses the FFI only
    /// when a slider actually moved.
    applied: DenoiseParams,
    to_out: FrameResampler,
    frame_out: [f32; RNNOISE_FRAME_SAMPLES],
    /// A params update or a frame the engine refused, reported once per chain
    /// rather than once per 10 ms.
    error_logged: bool,
}

impl DenoiseChain {
    /// `in_hz` is the microphone's native rate, `out_hz` / `out_frame_dur`
    /// the frames the VAD consumes (16 kHz, one detector frame). Fails only
    /// when the engine cannot allocate a suppressor.
    pub fn new(
        in_hz: usize,
        out_hz: usize,
        out_frame_dur: Duration,
    ) -> transcribe_cpp::Result<Self> {
        let applied = DenoiseParams::default();
        Ok(Self {
            to_48k: FrameResampler::new(in_hz, RNNOISE_SAMPLE_RATE, Duration::from_millis(10)),
            denoiser: Denoiser::new(&applied)?,
            applied,
            to_out: FrameResampler::new(RNNOISE_SAMPLE_RATE, out_hz, out_frame_dur),
            frame_out: [0.0; RNNOISE_FRAME_SAMPLES],
            error_logged: false,
        })
    }

    /// Hand the denoiser new params when they changed. `DenoiseControls`
    /// only ever holds normalized values, so a refusal is a bug; the previous
    /// params stay in force and it is logged once.
    fn apply(&mut self, params: DenoiseParams) {
        if params == self.applied {
            return;
        }
        if let Err(e) = self.denoiser.set_params(&params)
            && !self.error_logged
        {
            self.error_logged = true;
            log::warn!("Noise suppression kept its previous settings: {e}");
        }
        self.applied = params;
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
        self.apply(params);
        let Self {
            to_48k,
            denoiser,
            to_out,
            frame_out,
            error_logged,
            ..
        } = self;
        to_48k.push(src, |frame| {
            let prob = denoise_frame(denoiser, frame, frame_out, error_logged);
            on_denoised(frame_out);
            to_out.push(frame_out, |out| emit(out, prob));
        });
    }

    /// Flush both resamplers' delay lines at the end of a recording, and the
    /// suppressor's own one-frame lag (overlap-add synthesis): the engine's
    /// `flush` pushes the last real frame out, or the final ~10 ms would be
    /// lost.
    pub fn finish(
        &mut self,
        params: DenoiseParams,
        mut on_denoised: impl FnMut(&[f32]),
        mut emit: impl FnMut(&[f32], f32),
    ) {
        self.apply(params);
        let Self {
            to_48k,
            denoiser,
            to_out,
            frame_out,
            error_logged,
            ..
        } = self;
        to_48k.finish(|frame| {
            let prob = denoise_frame(denoiser, frame, frame_out, error_logged);
            on_denoised(frame_out);
            to_out.push(frame_out, |out| emit(out, prob));
        });
        // The flush frame is always the last one through the suppressor, so
        // its probability is also the one the final partial frame carries.
        let last_prob = match denoiser.flush(frame_out) {
            Ok(prob) => prob,
            Err(_) => {
                frame_out.fill(0.0);
                0.0
            }
        };
        on_denoised(frame_out);
        to_out.push(frame_out, |out| emit(out, last_prob));
        to_out.finish(|out| emit(out, last_prob));
    }

    /// Forget buffered audio, the gate and the network's recurrent state, so
    /// nothing from one recording (or from before a toggle) leaks into the
    /// next. The params stay.
    pub fn reset(&mut self) {
        self.to_48k.reset();
        self.to_out.reset();
        self.denoiser.reset();
    }
}

/// One 48 kHz frame through the suppressor. Returns RNNoise's speech
/// probability for it; the output is one frame behind the input.
///
/// The engine refuses a frame with a non-finite sample (a broken driver can
/// produce one). That frame comes out silent with probability 0 instead of
/// poisoning the VAD and the model downstream, and the first refusal of the
/// chain is logged.
fn denoise_frame(
    denoiser: &mut Denoiser,
    frame: &[f32],
    out: &mut [f32; RNNOISE_FRAME_SAMPLES],
    error_logged: &mut bool,
) -> f32 {
    let Ok(input) = <&[f32; RNNOISE_FRAME_SAMPLES]>::try_from(frame) else {
        // `FrameResampler` only ever emits whole 10 ms frames.
        debug_assert!(false, "denoise frame of {} samples", frame.len());
        out.fill(0.0);
        return 0.0;
    };
    match denoiser.process_frame(input, out) {
        Ok(prob) => prob,
        Err(e) => {
            if !*error_logged {
                *error_logged = true;
                log::warn!("Noise suppression dropped a frame: {e}");
            }
            out.fill(0.0);
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OUT_HZ: usize = 16_000;
    const OUT_FRAME: Duration = Duration::from_millis(16);
    const OUT_FRAME_SAMPLES: usize = 256;

    fn chain(in_hz: usize) -> DenoiseChain {
        DenoiseChain::new(in_hz, OUT_HZ, OUT_FRAME).expect("denoiser")
    }

    fn run(in_hz: usize, input: &[f32]) -> Vec<f32> {
        let mut chain = chain(in_hz);
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
        let mut chain = chain(16_000);
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
        let mut chain = chain(48_000);
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
        // The wet/dry mix now lives in the engine; this pins that the chain
        // hands it the params: at strength 0 the 48 kHz frames leaving the
        // suppressor are the input, one frame late.
        let mut chain = chain(48_000);
        let params = DenoiseParams {
            strength: 0.0,
            ..DenoiseParams::default()
        };
        let input = sine(RNNOISE_FRAME_SAMPLES * 3, 1000.0, 48_000.0, 0.5);
        let mut seen: Vec<f32> = Vec::new();
        chain.push(&input, params, |f| seen.extend_from_slice(f), |_, _| {});
        assert_eq!(seen.len(), input.len());
        let (first, rest) = seen.split_at(RNNOISE_FRAME_SAMPLES);
        assert!(first.iter().all(|&s| s == 0.0));
        for (x, y) in input.iter().zip(rest.iter()) {
            assert!((x - y).abs() < 1e-4, "{x} vs {y}");
        }
    }

    #[test]
    fn a_non_finite_sample_silences_its_frame_only() {
        let mut chain = chain(48_000);
        let params = DenoiseParams {
            strength: 0.0,
            ..DenoiseParams::default()
        };
        let mut input = sine(RNNOISE_FRAME_SAMPLES * 3, 300.0, 48_000.0, 0.3);
        input[RNNOISE_FRAME_SAMPLES + 7] = f32::NAN;
        let mut seen: Vec<f32> = Vec::new();
        chain.push(&input, params, |f| seen.extend_from_slice(f), |_, _| {});
        assert!(seen.iter().all(|s| s.is_finite()));
        // The refused frame comes out silent; the frames around it still flow.
        let frames: Vec<&[f32]> = seen.chunks(RNNOISE_FRAME_SAMPLES).collect();
        assert!(frames[1].iter().all(|&s| s == 0.0));
        assert!(frames[2].iter().any(|&s| s != 0.0));
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
