use anyhow::Result;

use crate::audio_toolkit::constants;

pub const VAD_PREFILL_MS: u64 = 450;
pub const VAD_OFFLINE_HANGOVER_MS: u64 = 450;
pub const VAD_STREAMING_HANGOVER_MS: u64 = 1650;
pub const VAD_ONSET_MS: u64 = 60;

/// Convert a VAD timing duration to whole detector frames, rounding up so a
/// change of frame size never shortens the app's onset, pre-roll, or hangover tail.
pub const fn frames_for_duration_ms(duration_ms: u64, frame_samples: usize) -> usize {
    assert!(frame_samples > 0, "VAD frame size must be non-zero");
    let numerator = duration_ms * constants::WHISPER_SAMPLE_RATE as u64;
    let denominator = frame_samples as u64 * 1000;
    numerator.div_ceil(denominator) as usize
}

/// Two-threshold gate: speech is entered at `threshold` and only left once
/// the score falls below a lower exit threshold (the rule silero-vad's
/// reference pipeline uses, applied here to Earshot's 0–1 score).
///
/// A single value for both edges makes a signal hovering near the threshold
/// flap frame to frame — which surfaces directly as a stuttering speech /
/// silence indicator and a speech clock that stalls mid-word.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Hysteresis {
    enter: f32,
    exit: f32,
    in_speech: bool,
}

impl Hysteresis {
    /// The exit threshold follows silero-vad's reference implementation: 0.15
    /// below the entry threshold, floored at 0.01 so it stays above the
    /// near-zero score a detector emits for true silence.
    pub(crate) fn new(threshold: f32) -> Self {
        Self {
            enter: threshold,
            exit: (threshold - 0.15).max(0.01),
            in_speech: false,
        }
    }

    pub(crate) fn update(&mut self, prob: f32) -> bool {
        self.in_speech = if self.in_speech {
            prob >= self.exit
        } else {
            prob >= self.enter
        };
        self.in_speech
    }

    pub(crate) fn reset(&mut self) {
        self.in_speech = false;
    }
}

pub enum VadFrame<'a> {
    /// Speech – may aggregate several frames (prefill + current + hangover)
    Speech(&'a [f32]),
    /// Non-speech (silence, noise). Down-stream code can ignore it.
    Noise,
}

impl<'a> VadFrame<'a> {
    #[inline]
    pub fn is_speech(&self) -> bool {
        matches!(self, VadFrame::Speech(_))
    }
}

pub trait VoiceActivityDetector: Send + Sync {
    /// Primary streaming API: feed one backend-sized frame, get a keep/drop decision.
    fn push_frame<'a>(&'a mut self, frame: &'a [f32]) -> Result<VadFrame<'a>>;

    /// Required number of mono 16 kHz samples per prediction.
    fn frame_samples(&self) -> usize;

    fn is_voice(&mut self, frame: &[f32]) -> Result<bool> {
        Ok(self.push_frame(frame)?.is_speech())
    }

    /// Set the post-speech hangover tail (in backend-sized frames) applied to
    /// subsequent frames. Detectors without a smoothing tail can ignore this.
    fn set_hangover_frames(&mut self, _frames: usize) {}

    /// Whether the *raw*, pre-smoothing detector classified the most recently
    /// pushed frame as voiced.
    ///
    /// This is deliberately not the same as `push_frame` returning
    /// [`VadFrame::Speech`]: the smoothing wrapper keeps reporting speech
    /// throughout its hangover tail, so that a streaming decoder keeps
    /// receiving audio across a pause. Speech-time metrics must not count
    /// that tail, so they read this instead.
    fn last_frame_voiced(&self) -> bool {
        false
    }

    /// Raw 0–1 speech score of the most recently pushed frame, before the
    /// hysteresis gate and any smoothing. `None` for detectors that do not
    /// expose one. Read by the live VAD test in Settings → Advanced so the
    /// user can see how far a frame sits from the threshold. Earshot reports
    /// 0.0 without running its model for a frame below -45 dBFS RMS while it
    /// is not already in speech (its energy pre-gate), so a very quiet
    /// microphone needs gain rather than a lower threshold.
    fn last_frame_score(&self) -> Option<f32> {
        None
    }

    /// Replace the speech threshold in place, without rebuilding the detector
    /// or reopening the microphone. Detectors without a threshold ignore it.
    fn set_threshold(&mut self, _threshold: f32) {}

    /// End-of-recording diagnostic snapshot, taken after the final frame.
    /// Purely observational — implementations must not change what they emit.
    /// Detectors without smoothing state return None.
    fn tail_report(&self) -> Option<VadTailReport> {
        None
    }

    fn reset(&mut self) {}
}

/// End-of-recording snapshot of a smoothing detector's state. Voiced frames
/// in the withheld tail suggest — but don't prove — a final word cut off at
/// the stop; a clean report doesn't rule VAD loss out either (soft trailing
/// speech can be classified as noise).
#[derive(Debug, Clone, Copy)]
pub struct VadTailReport {
    /// Trailing frames buffered but never emitted downstream.
    pub withheld_frames: usize,
    /// How many of those withheld frames the inner VAD classified as voiced.
    pub withheld_voiced_frames: usize,
    pub in_speech: bool,
    /// Voiced frames counted toward an unconfirmed speech onset.
    pub onset_counter: usize,
    pub hangover_counter: usize,
}

pub mod earshot;
mod smoothed;

pub use earshot::EarshotVad;
pub use smoothed::SmoothedVad;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_profiles_for_earshot_frames() {
        assert_eq!(frames_for_duration_ms(VAD_PREFILL_MS, 256), 29);
        assert_eq!(frames_for_duration_ms(VAD_OFFLINE_HANGOVER_MS, 256), 29);
        assert_eq!(frames_for_duration_ms(VAD_STREAMING_HANGOVER_MS, 256), 104);
        assert_eq!(frames_for_duration_ms(VAD_ONSET_MS, 256), 4);
    }
}
