use anyhow::Result;
use log::info;
use ndarray::{Array1, Array2, Array3, ArrayBase, Ix1, Ix3, OwnedRepr};
use ort::session::{Session, builder::GraphOptimizationLevel};
use ort::value::Value;
use std::path::Path;

use super::{VadFrame, VoiceActivityDetector};
use crate::audio_toolkit::constants::{
    VAD_CONTEXT_SAMPLES, VAD_FRAME_SAMPLES, WHISPER_SAMPLE_RATE,
};

/// Dual-threshold gate, mirroring silero-vad's own reference pipeline: speech
/// is *entered* at `threshold`, but only *left* once the probability drops below
/// a lower exit threshold.
///
/// A single value for both edges makes a signal hovering near the threshold flap
/// frame to frame — which surfaces directly as a stuttering speech/silence
/// indicator and a speech clock that stalls mid-word.
#[derive(Debug, Clone, Copy)]
struct Hysteresis {
    enter: f32,
    exit: f32,
    in_speech: bool,
}

impl Hysteresis {
    /// The exit threshold follows the reference implementation: 0.15 below the
    /// entry threshold, floored at 0.01 so it stays above the ~0.0005 the model
    /// emits for true silence.
    fn new(threshold: f32) -> Self {
        Self {
            enter: threshold,
            exit: (threshold - 0.15).max(0.01),
            in_speech: false,
        }
    }

    fn update(&mut self, prob: f32) -> bool {
        self.in_speech = if self.in_speech {
            prob >= self.exit
        } else {
            prob >= self.enter
        };
        self.in_speech
    }

    fn reset(&mut self) {
        self.in_speech = false;
    }
}

/// Recurrent state carried between frames. Silero v5/v6 replaced v4's separate
/// `h`/`c` LSTM tensors with a single packed `state` of shape (2, batch, 128).
const STATE_SHAPE: (usize, usize, usize) = (2, 1, 128);

/// Silero voice activity detection, driven directly against the ONNX session.
///
/// This deliberately does not use `vad-rs`: that crate speaks Silero **v4**'s
/// tensor interface (`input`/`sr`/`h`/`c` in, `hn`/`cn` out), while the model
/// shipped here is v6, which takes a single packed `state` and returns `stateN`.
/// Against a v6 model every `vad-rs` call fails with "Invalid input name: h" —
/// and because callers treat a VAD error as "keep this audio", that failure is
/// invisible: VAD silently passes everything through.
///
/// Two details of the v5/v6 contract are easy to miss, and getting either wrong
/// yields a model that runs without error but reports ~0.0005 for speech and
/// silence alike:
///
/// * the window must be exactly [`VAD_FRAME_SAMPLES`] (512 at 16 kHz), and
/// * the last [`VAD_CONTEXT_SAMPLES`] samples of the previous window must be
///   prepended to each chunk, so the tensor actually fed is 576 long.
pub struct SileroVad {
    session: Session,
    state: ArrayBase<OwnedRepr<f32>, Ix3>,
    sample_rate: ArrayBase<OwnedRepr<i64>, Ix1>,
    /// Tail of the previous window, prepended to the next one.
    context: Vec<f32>,
    gate: Hysteresis,
    /// Raw voiced/not decision for the most recent frame, before any smoothing
    /// wrapper widens it into a speech segment. Read by speech-time metrics.
    last_voiced: bool,
}

impl SileroVad {
    pub fn new<P: AsRef<Path>>(model_path: P, threshold: f32) -> Result<Self> {
        if !(0.0..=1.0).contains(&threshold) {
            anyhow::bail!("threshold must be between 0.0 and 1.0");
        }

        let path = model_path.as_ref();
        info!(
            "SileroVad: loading model from '{}' (threshold={})",
            path.display(),
            threshold
        );

        let metadata = std::fs::metadata(path)
            .map_err(|e| anyhow::anyhow!("Failed to stat VAD model '{}': {}", path.display(), e))?;
        let file_size_mb = metadata.len() as f64 / 1_048_576.0;
        info!("SileroVad: model file size = {:.2} MB", file_size_mb);

        // One thread each way: the VAD runs per frame on the audio consumer
        // thread, where scheduling overhead costs more than this small graph
        // gains from parallelism.
        let session = (|| -> ort::Result<Session> {
            Session::builder()?
                .with_optimization_level(GraphOptimizationLevel::Level3)
                .map_err(|e| -> ort::Error { e.into() })?
                .with_intra_threads(1)
                .map_err(|e| -> ort::Error { e.into() })?
                .with_inter_threads(1)
                .map_err(|e| -> ort::Error { e.into() })?
                .commit_from_file(path)
        })()
        .map_err(|e| anyhow::anyhow!("Failed to create VAD session: {e}"))?;

        // Fail loudly at load time rather than once per frame, so a mismatched
        // model can never degrade into a silently-disabled VAD.
        let inputs: Vec<&str> = session.inputs().iter().map(|i| i.name()).collect();
        for required in ["input", "state", "sr"] {
            if !inputs.contains(&required) {
                anyhow::bail!(
                    "VAD model '{}' is missing the '{}' input (has: {:?}); \
                     expected a Silero v5/v6 model",
                    path.display(),
                    required,
                    inputs
                );
            }
        }

        info!("SileroVad: model loaded and ready (inputs: {:?})", inputs);

        Ok(Self {
            session,
            state: Array3::<f32>::zeros(STATE_SHAPE),
            sample_rate: Array1::from_vec(vec![i64::from(WHISPER_SAMPLE_RATE)]),
            context: vec![0.0; VAD_CONTEXT_SAMPLES],
            gate: Hysteresis::new(threshold),
            last_voiced: false,
        })
    }

    /// Run one frame through the network, returning the speech probability.
    fn compute(&mut self, frame: &[f32]) -> Result<f32> {
        let mut input = Vec::with_capacity(VAD_CONTEXT_SAMPLES + frame.len());
        input.extend_from_slice(&self.context);
        input.extend_from_slice(frame);
        // Carry this window's tail forward as the next window's context.
        self.context
            .copy_from_slice(&input[input.len() - VAD_CONTEXT_SAMPLES..]);

        let samples = Array2::from_shape_vec((1, input.len()), input)?;

        let outputs = self
            .session
            .run(ort::inputs![
                "input" => Value::from_array(samples)?,
                "state" => Value::from_array(self.state.clone())?,
                "sr" => Value::from_array(self.sample_rate.clone())?,
            ])
            .map_err(|e| anyhow::anyhow!("Silero VAD inference failed: {e}"))?;

        // Carry the recurrent state forward, so the network sees the utterance
        // rather than one window at a time.
        let next_state = outputs
            .get("stateN")
            .ok_or_else(|| anyhow::anyhow!("VAD output 'stateN' missing"))?
            .try_extract_tensor::<f32>()
            .map_err(|e| anyhow::anyhow!("Failed to read VAD state: {e}"))?;
        self.state = Array3::from_shape_vec(STATE_SHAPE, next_state.1.to_vec())?;

        let output = outputs
            .get("output")
            .ok_or_else(|| anyhow::anyhow!("VAD output 'output' missing"))?
            .try_extract_tensor::<f32>()
            .map_err(|e| anyhow::anyhow!("Failed to read VAD output: {e}"))?;

        output
            .1
            .first()
            .copied()
            .ok_or_else(|| anyhow::anyhow!("VAD produced an empty probability"))
    }
}

impl VoiceActivityDetector for SileroVad {
    fn push_frame<'a>(&'a mut self, frame: &'a [f32]) -> Result<VadFrame<'a>> {
        if frame.len() != VAD_FRAME_SAMPLES {
            anyhow::bail!("expected {VAD_FRAME_SAMPLES} samples, got {}", frame.len());
        }

        let prob = self.compute(frame)?;

        self.last_voiced = self.gate.update(prob);
        if self.last_voiced {
            Ok(VadFrame::Speech(frame))
        } else {
            Ok(VadFrame::Noise)
        }
    }

    fn last_frame_voiced(&self) -> bool {
        self.last_voiced
    }

    fn reset(&mut self) {
        self.last_voiced = false;
        self.gate.reset();
        // Clear the recurrent state and the carried context so a new session
        // doesn't inherit anything from the previous recording.
        self.state.fill(0.0);
        self.context.fill(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::Hysteresis;

    #[test]
    fn entry_uses_the_high_threshold_and_exit_the_low_one() {
        let mut gate = Hysteresis::new(0.3);
        assert_eq!(gate.exit, 0.15);

        // Below the entry threshold, silence stays silence...
        assert!(!gate.update(0.25));
        // ...but at it, speech starts.
        assert!(gate.update(0.30));
        // Once speaking, the same 0.25 now counts as continuing speech, because
        // it is still above the exit threshold. This is the whole point.
        assert!(gate.update(0.25));
        assert!(gate.update(0.16));
        // Only below the exit threshold does it end.
        assert!(!gate.update(0.14));
        // And re-entry needs the high threshold again.
        assert!(!gate.update(0.25));
    }

    #[test]
    fn does_not_flap_on_a_signal_sitting_between_the_thresholds() {
        let mut gate = Hysteresis::new(0.3);
        gate.update(0.9); // enter speech

        // A signal oscillating in the band between exit and enter would toggle
        // on every frame with a single threshold; here it holds steady.
        for prob in [0.29f32, 0.16, 0.28, 0.17, 0.25] {
            assert!(gate.update(prob), "flapped at {prob}");
        }
    }

    #[test]
    fn exit_threshold_is_floored_above_the_models_silence_output() {
        // A low entry threshold must not drive the exit threshold to zero or
        // negative, where the model's ~0.0005 silence output would read as
        // speech forever.
        let gate = Hysteresis::new(0.1);
        assert_eq!(gate.exit, 0.01);

        let mut gate = Hysteresis::new(0.1);
        gate.update(0.5);
        assert!(!gate.update(0.0005), "silence read as speech");
    }

    #[test]
    fn reset_returns_to_requiring_the_entry_threshold() {
        let mut gate = Hysteresis::new(0.3);
        gate.update(0.9);
        gate.reset();
        // Mid-band, which would have continued speech before the reset.
        assert!(!gate.update(0.2));
    }
}
