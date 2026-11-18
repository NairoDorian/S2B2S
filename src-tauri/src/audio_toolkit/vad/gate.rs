use anyhow::Result;
use transcribe_cpp::{EARSHOT_FRAME_SAMPLES, VadGate, VadGateParams};

use super::{VAD_ONSET_MS, VAD_PREFILL_MS, VadFrame, VadTailReport, VoiceActivityDetector};
use crate::audio_toolkit::constants;

// The capture pipeline frames itself to the detector; the constant the rest
// of the app reads must be the engine's frame.
const _: () = assert!(EARSHOT_FRAME_SAMPLES == constants::VAD_FRAME_SAMPLES);

/// The engine's speech gate behind the recorder's detector trait.
///
/// One gate per capture source. It is fed exactly one 16 ms frame per call,
/// so each call is one decision: the audio it releases is empty (silence, an
/// onset still being confirmed), the frame itself (speech, hangover), or the
/// pre-roll together with the frame (an onset). The two-threshold verdict and
/// the raw Earshot score of that frame are kept for the speech clock and the
/// live VAD test.
///
/// The engine scores every frame. The `earshot`-crate adapter this replaces
/// skipped the model below −45 dBFS RMS and reported a score of 0 there, so a
/// very quiet microphone could never reach speech whatever the threshold; the
/// gate has no such floor (0.5 % of frames differ on ten minutes of audio).
pub struct SpeechGate {
    gate: VadGate,
    last_voiced: bool,
    last_score: Option<f32>,
}

impl SpeechGate {
    /// `threshold` is clamped by the settings to 0.05–0.95, the range the
    /// engine accepts; `hangover_ms` is the policy's tail, changed per
    /// recording through [`VoiceActivityDetector::set_hangover_ms`].
    pub fn new(threshold: f32, hangover_ms: u64) -> Result<Self> {
        let params = VadGateParams {
            threshold,
            // Derived by the engine (`exit_threshold`'s rule, 0.15 below,
            // floored at 0.01) and re-derived on every `set_threshold`. An
            // explicit value is the caller's fixed exit edge, kept across
            // threshold changes: the slider moved to 0.7 would have left the
            // gate exiting at 0.35, and at 0.3 the two edges would collapse
            // into one (0.3 / 0.3) and the verdict would flap.
            exit_threshold: None,
            onset_ms: VAD_ONSET_MS as u32,
            preroll_ms: VAD_PREFILL_MS as u32,
            hangover_ms: hangover_ms as u32,
            // The gate's own speech clock is not read: the recorder's
            // `SpeechClock` counts `last_frame_voiced` with the per-recording
            // pause tolerance, and keeps counting with the gate bypassed.
            ..VadGateParams::default()
        };
        let gate = VadGate::new(&params)
            .map_err(|e| anyhow::anyhow!("speech gate (threshold {threshold}): {e}"))?;
        Ok(Self {
            gate,
            last_voiced: false,
            last_score: None,
        })
    }
}

impl VoiceActivityDetector for SpeechGate {
    fn push_frame<'a>(&'a mut self, frame: &'a [f32]) -> Result<VadFrame<'a>> {
        if frame.len() != EARSHOT_FRAME_SAMPLES {
            anyhow::bail!(
                "expected {EARSHOT_FRAME_SAMPLES} samples, got {}",
                frame.len()
            );
        }
        // A refused frame (non-finite sample) leaves the gate untouched; the
        // recorder passes it through and logs the first one.
        let update = self
            .gate
            .feed(frame)
            .map_err(|e| anyhow::anyhow!("speech gate: {e}"))?;
        self.last_voiced = update.voiced;
        self.last_score = Some(update.last_score);
        if update.audio.is_empty() {
            Ok(VadFrame::Noise)
        } else {
            Ok(VadFrame::Speech(update.audio))
        }
    }

    fn frame_samples(&self) -> usize {
        EARSHOT_FRAME_SAMPLES
    }

    fn set_hangover_ms(&mut self, hangover_ms: u64) {
        if let Err(e) = self.gate.set_hangover_ms(hangover_ms as u32) {
            log::warn!("VAD hangover {hangover_ms} ms refused: {e}");
        }
    }

    fn last_frame_voiced(&self) -> bool {
        self.last_voiced
    }

    fn last_frame_score(&self) -> Option<f32> {
        self.last_score
    }

    /// From the next frame on, mid-recording included; the exit edge follows
    /// (see `new`). The settings clamp the value into the engine's range, so
    /// a refusal only means an out-of-range caller, and the previous
    /// threshold stays.
    fn set_threshold(&mut self, threshold: f32) {
        if let Err(e) = self.gate.set_threshold(threshold) {
            log::warn!("VAD threshold {threshold} refused: {e}");
        }
    }

    fn tail_report(&self) -> Option<VadTailReport> {
        let tail = self.gate.tail().ok()?;
        let count = |n: i32| usize::try_from(n).unwrap_or(0);
        Some(VadTailReport {
            withheld_frames: count(tail.withheld_frames),
            withheld_voiced_frames: count(tail.withheld_voiced_frames),
            in_speech: tail.speech,
            onset_counter: count(tail.onset_counter),
            hangover_counter: count(tail.hangover_counter),
        })
    }

    fn reset(&mut self) {
        self.gate.reset();
        self.last_voiced = false;
        self.last_score = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_toolkit::vad::{VAD_OFFLINE_HANGOVER_MS, exit_threshold};

    const N: usize = EARSHOT_FRAME_SAMPLES;

    fn gate() -> SpeechGate {
        SpeechGate::new(0.5, VAD_OFFLINE_HANGOVER_MS).expect("gate")
    }

    #[test]
    fn rejects_wrong_frame_size_and_non_finite_audio() {
        let mut vad = gate();
        assert!(vad.push_frame(&[0.0; N - 1]).is_err());
        let mut frame = [0.0; N];
        frame[0] = f32::NAN;
        assert!(vad.push_frame(&frame).is_err());
        // The refusal left nothing behind: silence still reads as silence.
        assert!(!vad.push_frame(&[0.0; N]).unwrap().is_speech());
    }

    #[test]
    fn silence_is_not_voice_and_reports_a_score() {
        let mut vad = gate();
        assert_eq!(vad.last_frame_score(), None);
        assert!(!vad.push_frame(&[0.0; N]).unwrap().is_speech());
        assert!(!vad.last_frame_voiced());
        let score = vad.last_frame_score().expect("score after a frame");
        assert!((0.0..=1.0).contains(&score));
        let report = vad.tail_report().expect("the gate always reports");
        assert!(!report.in_speech);
    }

    #[test]
    fn invalid_thresholds_are_refused_in_place_and_at_construction() {
        assert!(SpeechGate::new(0.0, VAD_OFFLINE_HANGOVER_MS).is_err());
        assert!(SpeechGate::new(1.5, VAD_OFFLINE_HANGOVER_MS).is_err());
        let mut vad = gate();
        // Out of range: logged and ignored, the gate keeps working.
        vad.set_threshold(1.5);
        vad.set_threshold(0.3);
        vad.set_hangover_ms(1650);
        assert!(!vad.push_frame(&[0.0; N]).unwrap().is_speech());
    }

    #[test]
    fn the_exit_edge_follows_every_threshold_change() {
        // The verdict is the two-threshold rule over the gate's own scores,
        // with the exit edge derived from whichever threshold is current. A
        // threshold change re-arms the rule (the gate starts outside speech).
        // An explicit exit would have stayed where construction put it.
        let pcm: Vec<f32> = (0..N * 210)
            .map(|i| {
                let t = i as f32 / 16_000.0;
                let level = [0.0, 0.05, 0.6, 0.2, 0.01, 0.4][(i / (N * 7)) % 6];
                level
                    * ((2.0 * std::f32::consts::PI * 210.0 * t).sin()
                        + 0.4 * (2.0 * std::f32::consts::PI * 1300.0 * t).sin())
                    / 1.4
            })
            .collect();
        let mut vad = gate();
        let mut threshold = 0.5f32;
        let mut in_speech = false;
        for (i, frame) in pcm.as_chunks::<N>().0.iter().enumerate() {
            if i == 70 || i == 140 {
                threshold = if i == 70 { 0.7 } else { 0.3 };
                vad.set_threshold(threshold);
                in_speech = false;
            }
            vad.push_frame(frame).unwrap();
            let score = vad.last_frame_score().unwrap();
            in_speech = if in_speech {
                score >= exit_threshold(threshold)
            } else {
                score >= threshold
            };
            assert_eq!(
                vad.last_frame_voiced(),
                in_speech,
                "frame {i}: score {score}, threshold {threshold}"
            );
        }
    }

    #[test]
    fn an_onset_releases_its_pre_roll_once_and_every_frame_at_most_once() {
        // A loud, speech-band signal, framed exactly as the recorder frames
        // it. Whatever the model decides frame by frame, the gate may not
        // release more audio than went in, and what it releases is the input
        // in order (pre-roll included), never a frame twice.
        let pcm: Vec<f32> = (0..N * 200)
            .map(|i| {
                let t = i as f32 / 16_000.0;
                let envelope = if (i / (N * 40)).is_multiple_of(2) {
                    0.6
                } else {
                    0.0
                };
                envelope
                    * ((2.0 * std::f32::consts::PI * 180.0 * t).sin()
                        + 0.5 * (2.0 * std::f32::consts::PI * 720.0 * t).sin())
                    / 1.5
            })
            .collect();
        let (input, _) = pcm.as_chunks::<N>();
        let mut vad = gate();
        let mut released = Vec::new();
        for frame in input {
            if let VadFrame::Speech(buf) = vad.push_frame(frame).unwrap() {
                assert!(buf.len().is_multiple_of(N));
                released.extend_from_slice(buf);
            }
        }
        assert!(released.len() <= pcm.len());
        // Every released frame is one of the input frames, in increasing
        // position: no duplicate, no reordering.
        let mut next = 0usize;
        for out in released.as_chunks::<N>().0 {
            let index = input[next..]
                .iter()
                .position(|f| f == out)
                .expect("released frame comes from the input, after the previous one");
            next += index + 1;
        }
    }
}
