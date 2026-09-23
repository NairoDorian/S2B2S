use anyhow::Result;

use super::{Hysteresis, VadFrame, VoiceActivityDetector};
use crate::audio_toolkit::constants;

/// Earshot's fixed prediction size: 256 samples, 16 ms at 16 kHz.
pub const EARSHOT_FRAME_SAMPLES: usize = constants::VAD_FRAME_SAMPLES;

/// Pure-Rust Earshot VAD adapter.
///
/// Earshot expects exactly 16 ms of mono 16 kHz audio per prediction. The
/// recorder uses `frame_samples()` to configure its resampler accordingly.
/// Its 0–1 score goes through the [`Hysteresis`] gate, so a signal hovering
/// at the threshold cannot flap the speech indicator or stall the speech clock
/// frame to frame.
pub struct EarshotVad {
    engine: Box<earshot::Detector>,
    gate: Hysteresis,
    clamped_frame: [f32; EARSHOT_FRAME_SAMPLES],
    last_voiced: bool,
    last_score: Option<f32>,
}

impl EarshotVad {
    pub fn new(threshold: f32) -> Result<Self> {
        if !(0.0..=1.0).contains(&threshold) {
            anyhow::bail!("threshold must be between 0.0 and 1.0");
        }

        Ok(Self {
            // Construct directly on the heap: Detector keeps roughly 8 KiB of
            // model state and scratch buffers.
            engine: earshot::Detector::default_boxed(),
            gate: Hysteresis::new(threshold),
            clamped_frame: [0.0; EARSHOT_FRAME_SAMPLES],
            last_voiced: false,
            last_score: None,
        })
    }
}

impl VoiceActivityDetector for EarshotVad {
    fn push_frame<'a>(&'a mut self, frame: &'a [f32]) -> Result<VadFrame<'a>> {
        if frame.len() != EARSHOT_FRAME_SAMPLES {
            anyhow::bail!(
                "expected {EARSHOT_FRAME_SAMPLES} samples, got {}",
                frame.len()
            );
        }
        if frame.iter().any(|sample| !sample.is_finite()) {
            anyhow::bail!("Earshot VAD input contained a non-finite sample");
        }

        // Fast RMS energy pre-gate: skip neural compute for deep silence (< -45 dBFS)
        // when not already in speech.
        let score = if !self.last_voiced && !is_frame_active(frame, -45.0) {
            0.0
        } else {
            // Direct branchless clamp into preallocated scratch buffer.
            // LLVM vectorizes this into 32 iterations of 8-wide AVX2 min/max instructions,
            // eliminating 256 conditional branches per 16 ms frame.
            for (clamped, &sample) in self.clamped_frame.iter_mut().zip(frame.iter()) {
                *clamped = sample.clamp(-1.0, 1.0);
            }
            self.engine.predict_f32(&self.clamped_frame)
        };

        let is_speech = self.gate.update(score);
        self.last_voiced = is_speech;
        self.last_score = Some(score);

        if is_speech {
            Ok(VadFrame::Speech(frame))
        } else {
            Ok(VadFrame::Noise)
        }
    }

    fn frame_samples(&self) -> usize {
        EARSHOT_FRAME_SAMPLES
    }

    fn last_frame_voiced(&self) -> bool {
        self.last_voiced
    }

    fn last_frame_score(&self) -> Option<f32> {
        self.last_score
    }

    /// Swap the hysteresis gate for one built on the new threshold. The
    /// detector state is untouched, so this is safe mid-recording; the gate
    /// re-enters speech on the next frame that clears the new threshold.
    fn set_threshold(&mut self, threshold: f32) {
        if (0.0..=1.0).contains(&threshold) {
            self.gate = Hysteresis::new(threshold);
        }
    }

    fn reset(&mut self) {
        self.engine.reset();
        self.gate.reset();
        self.last_voiced = false;
        self.last_score = None;
    }
}

#[inline(always)]
fn is_frame_active(frame: &[f32], threshold_dbfs: f32) -> bool {
    let sum_sq: f32 = frame.iter().map(|&x| x * x).sum();
    let mean_sq = sum_sq / (frame.len() as f32);
    if mean_sq <= 1e-12 {
        return false;
    }
    10.0 * mean_sq.log10() >= threshold_dbfs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_wrong_frame_size() {
        let mut vad = EarshotVad::new(0.5).unwrap();
        assert!(vad.push_frame(&[0.0; EARSHOT_FRAME_SAMPLES - 1]).is_err());
    }

    #[test]
    fn silence_is_not_voice() {
        let mut vad = EarshotVad::new(0.5).unwrap();
        assert!(
            !vad.push_frame(&[0.0; EARSHOT_FRAME_SAMPLES])
                .unwrap()
                .is_speech()
        );
    }

    #[test]
    fn validates_threshold() {
        assert!(EarshotVad::new(-0.1).is_err());
        assert!(EarshotVad::new(1.1).is_err());
    }

    #[test]
    fn clamps_resampler_overshoot_without_changing_output() {
        let mut vad = EarshotVad::new(0.0).unwrap();
        let frame = [1.01; EARSHOT_FRAME_SAMPLES];
        match vad.push_frame(&frame).unwrap() {
            VadFrame::Speech(output) => assert_eq!(output, frame),
            VadFrame::Noise => panic!("zero threshold should retain the frame"),
        }
    }

    #[test]
    fn reports_the_raw_score_and_takes_a_new_threshold_in_place() {
        let mut vad = EarshotVad::new(0.5).unwrap();
        assert_eq!(vad.last_frame_score(), None);
        vad.push_frame(&[0.0; EARSHOT_FRAME_SAMPLES]).unwrap();
        let score = vad.last_frame_score().expect("score after a frame");
        assert!((0.0..=1.0).contains(&score));

        // A threshold of 0 makes every frame speech, without a rebuild.
        vad.set_threshold(0.0);
        assert!(
            vad.push_frame(&[0.0; EARSHOT_FRAME_SAMPLES])
                .unwrap()
                .is_speech()
        );
        assert!(vad.last_frame_voiced());
        // Out-of-range values are ignored rather than accepted.
        vad.set_threshold(1.5);
        assert!(
            vad.push_frame(&[0.0; EARSHOT_FRAME_SAMPLES])
                .unwrap()
                .is_speech()
        );
    }

    #[test]
    fn rejects_non_finite_audio() {
        let mut vad = EarshotVad::new(0.5).unwrap();
        let mut frame = [0.0; EARSHOT_FRAME_SAMPLES];
        frame[0] = f32::NAN;
        assert!(vad.push_frame(&frame).is_err());
    }
}
