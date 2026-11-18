//! Voice activity detection on the capture path.
//!
//! The detector is the engine's capture-side speech gate
//! ([`transcribe_cpp::VadGate`], wrapped by [`SpeechGate`]): the Earshot model
//! with SIMD kernels and bundled weights, the two-threshold hysteresis, the
//! onset debounce, a pre-roll ring that never releases a frame twice and the
//! hangover tail — the chain this module used to assemble from the `earshot`
//! crate and its own `Hysteresis` and `SmoothedVad`, now one C++ implementation
//! shared with the engine. The app keeps the framing, the per-recording policy
//! (offline vs streaming hangover) and its own speech clock (`SpeechClock` in
//! the recorder), which counts the gate's raw per-frame verdict.

use anyhow::Result;

use crate::audio_toolkit::constants;

pub const VAD_PREFILL_MS: u64 = 450;
pub const VAD_OFFLINE_HANGOVER_MS: u64 = 450;
pub const VAD_STREAMING_HANGOVER_MS: u64 = 1650;
pub const VAD_ONSET_MS: u64 = 60;

/// Entry threshold of the History re-analysis, the detector's default.
const HISTORY_THRESHOLD: f32 = 0.5;

/// Convert a VAD timing duration to whole detector frames, rounding up so a
/// change of frame size never shortens the app's onset, pre-roll, or hangover
/// tail. The gate rounds its own millisecond settings the same way; this is
/// for the speech clock's onset, which counts frames.
pub const fn frames_for_duration_ms(duration_ms: u64, frame_samples: usize) -> usize {
    assert!(frame_samples > 0, "VAD frame size must be non-zero");
    let numerator = duration_ms * constants::WHISPER_SAMPLE_RATE as u64;
    let denominator = frame_samples as u64 * 1000;
    numerator.div_ceil(denominator) as usize
}

/// The exit threshold of the two-threshold gate: 0.15 below the entry
/// threshold, floored at 0.01 (silero-vad's reference rule). The engine's gate
/// derives it itself, and re-derives it on every threshold change, because
/// `SpeechGate` gives it no exit edge; this copy is for the History
/// re-analysis (`voiced_ms`) and for checking the gate against the rule.
pub fn exit_threshold(threshold: f32) -> f32 {
    (threshold - 0.15).max(0.01)
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

/// What the recorder needs from a detector. One implementation ships
/// ([`SpeechGate`]); the trait stays so the recorder's tests can drive the
/// capture path with scripted detectors.
pub trait VoiceActivityDetector: Send {
    /// Primary streaming API: feed one backend-sized frame, get a keep/drop decision.
    fn push_frame<'a>(&'a mut self, frame: &'a [f32]) -> Result<VadFrame<'a>>;

    /// Required number of mono 16 kHz samples per prediction.
    fn frame_samples(&self) -> usize;

    /// Set the post-speech hangover tail applied to subsequent frames.
    /// Detectors without a smoothing tail can ignore this.
    fn set_hangover_ms(&mut self, _hangover_ms: u64) {}

    /// Whether the *raw*, pre-smoothing detector classified the most recently
    /// pushed frame as voiced (the two-threshold verdict).
    ///
    /// This is deliberately not the same as `push_frame` returning
    /// [`VadFrame::Speech`]: the gate keeps reporting speech throughout its
    /// hangover tail, so that a streaming decoder keeps receiving audio across
    /// a pause. Speech-time metrics must not count that tail, so they read
    /// this instead.
    fn last_frame_voiced(&self) -> bool {
        false
    }

    /// Raw 0–1 speech score of the most recently pushed frame, before the
    /// hysteresis gate and any smoothing. `None` for detectors that do not
    /// expose one. Read by the live VAD test in Settings → Advanced so the
    /// user can see how far a frame sits from the threshold.
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

/// Milliseconds of voiced audio in a 16 kHz mono clip, by the same Earshot
/// scores and two-threshold rule a recording uses: the History re-analysis of
/// entries saved before the speech duration was recorded. The scores come from
/// the engine's batch scorer (bit-identical to frame-by-frame scoring). `None`
/// when the clip has no voiced frame or the engine refuses it (a non-finite
/// sample), so the caller falls back to the clip's own duration.
pub fn voiced_ms(pcm_16k: &[f32]) -> Option<f64> {
    let scores = transcribe_cpp::score_frames(pcm_16k, 1).ok()?;
    let exit = exit_threshold(HISTORY_THRESHOLD);
    let mut in_speech = false;
    let mut voiced = 0usize;
    for score in scores {
        in_speech = if in_speech {
            score >= exit
        } else {
            score >= HISTORY_THRESHOLD
        };
        voiced += usize::from(in_speech);
    }
    (voiced > 0).then(|| {
        (voiced * transcribe_cpp::EARSHOT_FRAME_SAMPLES) as f64 * 1000.0
            / constants::WHISPER_SAMPLE_RATE as f64
    })
}

mod gate;

pub use gate::SpeechGate;

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

    #[test]
    fn exit_threshold_follows_the_reference_rule() {
        assert!((exit_threshold(0.5) - 0.35).abs() < 1e-6);
        assert_eq!(exit_threshold(0.1), 0.01);
    }

    #[test]
    fn voiced_ms_of_silence_is_none() {
        assert_eq!(voiced_ms(&vec![0.0; 16_000]), None);
        assert_eq!(voiced_ms(&[]), None);
        let mut bad = vec![0.0f32; 512];
        bad[3] = f32::NAN;
        assert_eq!(voiced_ms(&bad), None);
    }
}
