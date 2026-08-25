use anyhow::Result;

pub const VAD_PREFILL_FRAMES: usize = 15;
pub const VAD_OFFLINE_HANGOVER_FRAMES: usize = 15;
pub const VAD_STREAMING_HANGOVER_FRAMES: usize = 55;
pub const VAD_ONSET_FRAMES: usize = 2;

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
    /// Primary streaming API: feed one frame (see `constants::VAD_FRAME_SAMPLES`),
    /// get a keep/drop decision.
    fn push_frame<'a>(&'a mut self, frame: &'a [f32]) -> Result<VadFrame<'a>>;

    fn is_voice(&mut self, frame: &[f32]) -> Result<bool> {
        Ok(self.push_frame(frame)?.is_speech())
    }

    /// Set the post-speech hangover tail (in frames) applied to
    /// subsequent frames. Detectors without a smoothing tail can ignore this.
    fn set_hangover_frames(&mut self, _frames: usize) {}

    /// Whether the *raw*, pre-smoothing detector classified the most recently
    /// pushed frame as voiced.
    ///
    /// This is deliberately not the same as `push_frame` returning
    /// [`VadFrame::Speech`]: the smoothing wrapper keeps reporting speech
    /// throughout its hangover tail (up to [`VAD_STREAMING_HANGOVER_FRAMES`],
    /// i.e. 1.76 s, so that a streaming decoder keeps receiving audio across a
    /// pause). Speech-time metrics must not count that tail, so they read this
    /// instead.
    fn last_frame_voiced(&self) -> bool {
        false
    }

    fn reset(&mut self) {}
}

mod silero;
mod smoothed;

pub use silero::SileroVad;
pub use smoothed::SmoothedVad;
