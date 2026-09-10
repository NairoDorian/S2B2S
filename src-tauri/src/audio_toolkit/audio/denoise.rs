//! RNNoise noise suppression on the capture path, ahead of the VAD.
//!
//! `nnnoiseless` is a pure-Rust port of Xiph's RNNoise: a small recurrent
//! network that estimates per-band gains for 10 ms frames of 48 kHz audio.
//! Handy's capture path runs at the microphone's native rate and hands the VAD
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
//! the denoised signal. The raw-audio tap and the overlay level meter are
//! upstream of this and stay untouched.
//!
//! RNNoise wants samples in the `i16` range; the chain scales in and out so
//! the rest of the pipeline keeps its `[-1, 1]` floats.

use std::time::Duration;

use nnnoiseless::DenoiseState;

use super::resampler::FrameResampler;

/// The only sample rate RNNoise is trained for.
pub const RNNOISE_SAMPLE_RATE: usize = 48_000;
/// RNNoise frame: 10 ms at 48 kHz.
pub const RNNOISE_FRAME_SAMPLES: usize = DenoiseState::FRAME_SIZE;
const I16_SCALE: f32 = i16::MAX as f32;

/// Native-rate samples in, denoised VAD-sized 16 kHz frames out.
pub struct DenoiseChain {
    to_48k: FrameResampler,
    denoiser: Box<DenoiseState<'static>>,
    to_out: FrameResampler,
    scaled_in: [f32; RNNOISE_FRAME_SAMPLES],
    scaled_out: [f32; RNNOISE_FRAME_SAMPLES],
    frame_out: [f32; RNNOISE_FRAME_SAMPLES],
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
            frame_out: [0.0; RNNOISE_FRAME_SAMPLES],
        }
    }

    /// Feed native-rate samples; `emit` receives whole output frames.
    pub fn push(&mut self, src: &[f32], mut emit: impl FnMut(&[f32])) {
        let Self {
            to_48k,
            denoiser,
            to_out,
            scaled_in,
            scaled_out,
            frame_out,
        } = self;
        to_48k.push(src, |frame| {
            denoise_frame(denoiser, frame, scaled_in, scaled_out, frame_out);
            to_out.push(frame_out, &mut emit);
        });
    }

    /// Flush both resamplers' delay lines at the end of a recording.
    pub fn finish(&mut self, mut emit: impl FnMut(&[f32])) {
        let Self {
            to_48k,
            denoiser,
            to_out,
            scaled_in,
            scaled_out,
            frame_out,
        } = self;
        to_48k.finish(|frame| {
            denoise_frame(denoiser, frame, scaled_in, scaled_out, frame_out);
            to_out.push(frame_out, &mut emit);
        });
        to_out.finish(&mut emit);
    }

    /// Forget buffered audio and the network's recurrent state, so nothing
    /// from one recording (or from before a toggle) leaks into the next.
    pub fn reset(&mut self) {
        self.to_48k.reset();
        self.to_out.reset();
        self.denoiser = DenoiseState::new();
    }
}

fn denoise_frame(
    denoiser: &mut DenoiseState<'static>,
    frame: &[f32],
    scaled_in: &mut [f32; RNNOISE_FRAME_SAMPLES],
    scaled_out: &mut [f32; RNNOISE_FRAME_SAMPLES],
    frame_out: &mut [f32; RNNOISE_FRAME_SAMPLES],
) {
    debug_assert_eq!(frame.len(), RNNOISE_FRAME_SAMPLES);
    for (dst, src) in scaled_in.iter_mut().zip(frame) {
        *dst = (src * I16_SCALE).clamp(i16::MIN as f32, i16::MAX as f32);
    }
    // The returned value is RNNoise's own speech probability; Handy keeps
    // Earshot as the detector, so it is not used.
    let _ = denoiser.process_frame(scaled_out, scaled_in);
    for (dst, src) in frame_out.iter_mut().zip(scaled_out.iter()) {
        *dst = (src / I16_SCALE).clamp(-1.0, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OUT_HZ: usize = 16_000;
    const OUT_FRAME: Duration = Duration::from_millis(16);
    const OUT_FRAME_SAMPLES: usize = 256;

    fn run(in_hz: usize, input: &[f32]) -> Vec<f32> {
        let mut chain = DenoiseChain::new(in_hz, OUT_HZ, OUT_FRAME);
        let mut out = Vec::new();
        chain.push(input, |f| out.extend_from_slice(f));
        chain.finish(|f| out.extend_from_slice(f));
        out
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
        let mut out = Vec::new();
        chain.push(&vec![0.5f32; 8_000], |f| out.extend_from_slice(f));
        chain.finish(|f| out.extend_from_slice(f));
        assert!(!out.is_empty());

        chain.reset();
        let mut after = Vec::new();
        chain.push(&vec![0.0f32; 16_000], |f| after.extend_from_slice(f));
        assert!(
            after.iter().all(|s| s.abs() < 1e-3),
            "audio from before reset leaked into the next session"
        );
    }
}
