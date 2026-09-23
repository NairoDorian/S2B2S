use super::{VadFrame, VadTailReport, VoiceActivityDetector};
use anyhow::Result;

#[derive(Clone, Copy, Default)]
struct BufferedSlot {
    emitted: bool,
    voiced: bool,
}

pub struct SmoothedVad {
    inner_vad: Box<dyn VoiceActivityDetector>,
    prefill_frames: usize,
    hangover_frames: usize,
    onset_frames: usize,

    // Zero-allocation circular ring buffer for pre-roll
    capacity_frames: usize,
    slot_samples: usize,
    buffer_samples: Vec<f32>,
    buffer_slots: Vec<BufferedSlot>,
    head_idx: usize,
    buffered_count: usize,

    hangover_counter: usize,
    onset_counter: usize,
    in_speech: bool,

    temp_out: Vec<f32>,
}

impl SmoothedVad {
    pub fn new(
        inner_vad: Box<dyn VoiceActivityDetector>,
        prefill_frames: usize,
        hangover_frames: usize,
        onset_frames: usize,
    ) -> Self {
        let slot_samples = inner_vad.frame_samples();
        let capacity_frames = prefill_frames + 2;
        let buffer_samples = vec![0.0f32; capacity_frames * slot_samples];
        let buffer_slots = vec![BufferedSlot::default(); capacity_frames];
        let temp_out = Vec::with_capacity((prefill_frames + 1) * slot_samples);

        Self {
            inner_vad,
            prefill_frames,
            hangover_frames,
            onset_frames,
            capacity_frames,
            slot_samples,
            buffer_samples,
            buffer_slots,
            head_idx: 0,
            buffered_count: 0,
            hangover_counter: 0,
            onset_counter: 0,
            in_speech: false,
            temp_out,
        }
    }

    fn ensure_capacity(&mut self, samples_len: usize) {
        if self.slot_samples != samples_len
            || self.buffer_samples.len() < self.capacity_frames * samples_len
        {
            self.slot_samples = samples_len;
            self.buffer_samples = vec![0.0f32; self.capacity_frames * samples_len];
            self.temp_out
                .reserve((self.prefill_frames + 1) * samples_len);
            self.head_idx = 0;
            self.buffered_count = 0;
        }
    }

    fn mark_last_emitted(&mut self) {
        if self.buffered_count > 0 {
            let last_idx = (self.head_idx + self.buffered_count - 1) % self.capacity_frames;
            self.buffer_slots[last_idx].emitted = true;
        }
    }
}

impl VoiceActivityDetector for SmoothedVad {
    fn push_frame<'a>(&'a mut self, frame: &'a [f32]) -> Result<VadFrame<'a>> {
        self.ensure_capacity(frame.len());

        // 1. Buffer every incoming frame for possible pre-roll into circular buffer
        let write_idx = (self.head_idx + self.buffered_count) % self.capacity_frames;
        let start = write_idx * self.slot_samples;
        self.buffer_samples[start..start + frame.len()].copy_from_slice(frame);
        self.buffer_slots[write_idx] = BufferedSlot {
            emitted: false,
            voiced: false,
        };
        self.buffered_count += 1;

        while self.buffered_count > self.prefill_frames + 1 {
            self.head_idx = (self.head_idx + 1) % self.capacity_frames;
            self.buffered_count -= 1;
        }

        // 2. Delegate to the wrapped boolean VAD
        let is_voice = self.inner_vad.is_voice(frame)?;
        if self.buffered_count > 0 {
            let last_idx = (self.head_idx + self.buffered_count - 1) % self.capacity_frames;
            self.buffer_slots[last_idx].voiced = is_voice;
        }

        match (self.in_speech, is_voice) {
            // Potential start of speech - need to accumulate onset frames
            (false, true) => {
                self.onset_counter += 1;
                if self.onset_counter >= self.onset_frames {
                    // We have enough consecutive voice frames to trigger speech
                    self.in_speech = true;
                    self.hangover_counter = self.hangover_frames;
                    self.onset_counter = 0; // Reset for next time

                    // Collect prefill + current frame. Slots already sent
                    // (the previous utterance's hangover tail, when the pause
                    // was shorter than the pre-roll) are skipped so no audio
                    // is emitted twice; the current frame is never marked, so
                    // the output is never empty.
                    self.temp_out.clear();
                    for i in 0..self.buffered_count {
                        let idx = (self.head_idx + i) % self.capacity_frames;
                        if self.buffer_slots[idx].emitted {
                            continue;
                        }
                        let s = idx * self.slot_samples;
                        self.temp_out
                            .extend_from_slice(&self.buffer_samples[s..s + self.slot_samples]);
                        self.buffer_slots[idx].emitted = true;
                    }
                    Ok(VadFrame::Speech(&self.temp_out))
                } else {
                    // Not enough frames yet, still silence
                    Ok(VadFrame::Noise)
                }
            }

            // Ongoing Speech
            (true, true) => {
                self.hangover_counter = self.hangover_frames;
                self.mark_last_emitted();
                Ok(VadFrame::Speech(frame))
            }

            // End of Speech or interruption during onset phase
            (true, false) => {
                if self.hangover_counter > 0 {
                    self.hangover_counter -= 1;
                    self.mark_last_emitted();
                    Ok(VadFrame::Speech(frame))
                } else {
                    self.in_speech = false;
                    Ok(VadFrame::Noise)
                }
            }

            // Silence or broken onset sequence
            (false, false) => {
                self.onset_counter = 0; // Reset onset counter on silence
                Ok(VadFrame::Noise)
            }
        }
    }

    fn frame_samples(&self) -> usize {
        self.inner_vad.frame_samples()
    }

    fn set_hangover_frames(&mut self, frames: usize) {
        self.hangover_frames = frames;
    }

    /// Trailing run of withheld frames plus smoothing state. Interior
    /// withheld frames (before already-emitted speech) are not counted.
    fn tail_report(&self) -> Option<VadTailReport> {
        let mut withheld_frames = 0;
        let mut withheld_voiced_frames = 0;
        for i in (0..self.buffered_count).rev() {
            let idx = (self.head_idx + i) % self.capacity_frames;
            let slot = self.buffer_slots[idx];
            if slot.emitted {
                break;
            }
            withheld_frames += 1;
            if slot.voiced {
                withheld_voiced_frames += 1;
            }
        }

        Some(VadTailReport {
            withheld_frames,
            withheld_voiced_frames,
            in_speech: self.in_speech,
            onset_counter: self.onset_counter,
            hangover_counter: self.hangover_counter,
        })
    }

    /// Delegate to the wrapped detector: the whole point of this method is to
    /// see the decision *before* this wrapper's onset debounce and hangover tail
    /// widen it into a speech segment.
    fn last_frame_voiced(&self) -> bool {
        self.inner_vad.last_frame_voiced()
    }
    fn last_frame_score(&self) -> Option<f32> {
        self.inner_vad.last_frame_score()
    }
    fn set_threshold(&mut self, threshold: f32) {
        self.inner_vad.set_threshold(threshold);
    }
    fn reset(&mut self) {
        self.inner_vad.reset();
        self.head_idx = 0;
        self.buffered_count = 0;
        self.hangover_counter = 0;
        self.onset_counter = 0;
        self.in_speech = false;
        self.temp_out.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    /// Inner VAD that replays a scripted voice/no-voice sequence.
    struct ScriptedVad {
        script: VecDeque<bool>,
    }

    impl ScriptedVad {
        fn new(script: &[bool]) -> Self {
            Self {
                script: script.iter().copied().collect(),
            }
        }
    }

    impl VoiceActivityDetector for ScriptedVad {
        fn push_frame<'a>(&'a mut self, frame: &'a [f32]) -> Result<VadFrame<'a>> {
            if self.script.pop_front().unwrap_or(false) {
                Ok(VadFrame::Speech(frame))
            } else {
                Ok(VadFrame::Noise)
            }
        }

        fn frame_samples(&self) -> usize {
            4
        }
    }

    fn frame(value: f32) -> Vec<f32> {
        vec![value; 4]
    }

    fn smoothed(script: &[bool], onset_frames: usize) -> SmoothedVad {
        SmoothedVad::new(Box::new(ScriptedVad::new(script)), 3, 2, onset_frames)
    }

    #[test]
    fn tail_report_counts_withheld_onset_tail() {
        // One voiced frame at the end: onset (2 frames) never confirms, so
        // both trailing frames are withheld and one of them is voiced —
        // consistent with a final word cut off at the stop boundary.
        let mut vad = smoothed(&[false, true], 2);
        assert!(!vad.push_frame(&frame(0.1)).unwrap().is_speech());
        assert!(!vad.push_frame(&frame(0.9)).unwrap().is_speech());

        let report = vad.tail_report().expect("smoothed VAD always reports");
        assert_eq!(report.withheld_frames, 2);
        assert_eq!(report.withheld_voiced_frames, 1);
        assert_eq!(report.onset_counter, 1);
        assert!(!report.in_speech);
    }

    #[test]
    fn onset_prefill_does_not_re_emit_the_previous_hangover_tail() {
        // prefill 3, hangover 2, onset 2. Speech, a pause one frame past the
        // hangover, then speech again: the new onset's pre-roll still holds
        // hangover frames that were already sent and must not repeat them.
        let hangover = 2;
        let onset = 2;
        let mut script = vec![true; 2];
        script.extend(std::iter::repeat_n(false, hangover + 1));
        script.extend(std::iter::repeat_n(true, onset));
        let mut vad = smoothed(&script, onset);

        let mut emitted = Vec::new();
        for i in 0..script.len() {
            let input = frame(i as f32);
            if let VadFrame::Speech(buf) = vad.push_frame(&input).unwrap() {
                emitted.extend_from_slice(buf);
            }
        }

        // Every frame is emitted exactly once: the first onset, the hangover,
        // the one withheld silent frame (as pre-roll) and the new onset.
        let expected: Vec<f32> = (0..script.len()).flat_map(|i| frame(i as f32)).collect();
        assert_eq!(emitted, expected);
    }

    #[test]
    fn tail_report_counts_only_trailing_run() {
        // Speech emitted through hangover, then silence past the hangover is
        // withheld. Only the trailing run counts — frames older than an
        // emitted frame are excluded.
        let mut vad = smoothed(&[true, true, false, false, false, false], 2);
        for v in [0.1, 0.2, 0.3, 0.4] {
            let _ = vad.push_frame(&frame(v)).unwrap();
        }
        assert!(!vad.push_frame(&frame(0.5)).unwrap().is_speech()); // past hangover
        assert!(!vad.push_frame(&frame(0.6)).unwrap().is_speech());

        let report = vad.tail_report().unwrap();
        assert_eq!(report.withheld_frames, 2);
        assert_eq!(report.withheld_voiced_frames, 0);
    }
}
