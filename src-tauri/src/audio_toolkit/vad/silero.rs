//! Silero VAD wrappers — supports both v4 (via `vad-rs`) and v6.x (native ort).
//!
//! # Model API differences
//!
//! | Feature           | v4 (vad-rs)                        | v6.x (native ort)                        |
//! |-------------------|------------------------------------|------------------------------------------|
//! | Input names       | `input`, `sr`, `h`, `c`            | `input`, `state`, `sr`                   |
//! | State shape       | `h:(2,1,64)` + `c:(2,1,64)` split  | `state:(2,1,128)` single tensor           |
//! | Context prepend   | ❌                                  | ✅ 64-sample context prepended per frame  |
//! | Audio chunk size  | 480 samples (30 ms @ 16 kHz)       | 512 samples (32 ms @ 16 kHz)             |
//! | Input tensor size | 480 samples                        | 576 = 64 context + 512 audio             |
//! | Output names      | `output`, `hn`, `cn`               | `output`, `stateN`                       |
//!
//! The `SileroVad` struct auto-detects the model version by reading ONNX input names at
//! construction time.  For v6.x the struct also buffers incoming 480-sample frames (30 ms)
//! into 512-sample (32 ms) windows before calling inference, so the rest of the pipeline
//! does not need to be aware of the frame-size difference.

use anyhow::Result;
use log::{debug, info};
use ndarray::{Array1, Array2, ArrayD};
use ort::{session::Session, value::Value};
use std::path::Path;

use super::{VadFrame, VoiceActivityDetector};
use crate::audio_toolkit::constants;

// ── Frame sizing ─────────────────────────────────────────────────────────────

/// Frame size the *pipeline* passes us (30 ms at 16 kHz).
const PIPELINE_FRAME_SAMPLES: usize = (constants::WHISPER_SAMPLE_RATE * 30 / 1000) as usize; // 480

/// Frame size v4 expects (same as the pipeline frame).
const V4_FRAME_SAMPLES: usize = PIPELINE_FRAME_SAMPLES; // 480

/// Frame size v6.x expects (32 ms at 16 kHz).
const V6_AUDIO_SAMPLES: usize = 512;

/// Context prepended to every v6.x call.
const V6_CONTEXT_SIZE: usize = 64;

// ── Inner engine ─────────────────────────────────────────────────────────────

enum Engine {
    /// `vad-rs` wrapper — only works with Silero v4.
    V4(vad_rs::Vad),

    /// Native `ort` wrapper — works with Silero v6.x.
    V6 {
        session: Session,
        /// Combined RNN state tensor, shape (2, 1, 128).
        state: ArrayD<f32>,
        /// Tail of the previous audio window used as leading context (64 samples).
        context: Array1<f32>,
        sample_rate: Array1<i64>,
        /// Internal accumulator: buffers pipeline frames until we have 512 samples.
        audio_buf: Vec<f32>,
        /// Last probability returned (used when not enough samples yet).
        last_prob: f32,
    },
}

// ── Public struct ─────────────────────────────────────────────────────────────

pub struct SileroVad {
    engine: Engine,
    threshold: f32,
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

        let engine = Self::build_engine(path, constants::WHISPER_SAMPLE_RATE as usize)?;
        info!("SileroVad: model loaded and ready");
        Ok(Self { engine, threshold })
    }

    /// Open the model and decide which engine to use based on ONNX input names.
    fn build_engine(path: &Path, sample_rate: usize) -> Result<Engine> {
        let probe = Session::builder()
            .map_err(|e| anyhow::anyhow!("ort Session builder error: {e}"))?
            .with_intra_threads(1)
            .map_err(|e| anyhow::anyhow!("ort intra_threads error: {e}"))?
            .with_inter_threads(1)
            .map_err(|e| anyhow::anyhow!("ort inter_threads error: {e}"))?
            .commit_from_file(path)
            .map_err(|e| anyhow::anyhow!("ort failed to open model '{}': {e}", path.display()))?;

        let input_names: Vec<String> = probe
            .inputs()
            .iter()
            .map(|i| i.name().to_string())
            .collect();

        info!("SileroVad: ONNX input names = {:?}", input_names);

        let has_h = input_names.iter().any(|n| n == "h");
        let has_c = input_names.iter().any(|n| n == "c");
        let has_state = input_names.iter().any(|n| n == "state");

        if has_h && has_c && !has_state {
            drop(probe);
            info!("SileroVad: detected v4 model → using vad-rs engine (480-sample frames)");
            let vad = vad_rs::Vad::new(path, sample_rate)
                .map_err(|e| anyhow::anyhow!("Failed to create VAD: {e}"))?;
            Ok(Engine::V4(vad))
        } else if has_state {
            info!("SileroVad: detected v6.x model → using native ort engine (512-sample frames, 64-sample context)");
            Ok(Engine::V6 {
                session: probe,
                state: ArrayD::<f32>::zeros([2, 1, 128].as_slice()),
                context: Array1::<f32>::zeros(V6_CONTEXT_SIZE),
                sample_rate: Array1::from_vec(vec![sample_rate as i64]),
                audio_buf: Vec::with_capacity(V6_AUDIO_SAMPLES * 2),
                last_prob: 0.0,
            })
        } else {
            anyhow::bail!(
                "Unrecognised Silero VAD inputs: {:?}. Expected v4 (h/c) or v6.x (state).",
                input_names
            );
        }
    }

    /// Run one 512-sample inference step for the v6.x model.
    fn infer_v6(
        session: &mut Session,
        state: &mut ArrayD<f32>,
        context: &mut Array1<f32>,
        sample_rate: &Array1<i64>,
        audio_512: &[f32], // exactly V6_AUDIO_SAMPLES samples
    ) -> Result<f32> {
        debug_assert_eq!(audio_512.len(), V6_AUDIO_SAMPLES);

        // Build [context(64) | audio(512)] → shape (1, 576)
        let mut input_data = Vec::with_capacity(V6_CONTEXT_SIZE + V6_AUDIO_SAMPLES);
        input_data.extend_from_slice(context.as_slice().unwrap());
        input_data.extend_from_slice(audio_512);

        let frame_arr = Array2::<f32>::from_shape_vec((1, input_data.len()), input_data)
            .map_err(|e| anyhow::anyhow!("ndarray shape error: {e}"))?;

        let frame_val = Value::from_array(frame_arr)
            .map_err(|e| anyhow::anyhow!("ort Value error (input): {e}"))?;

        let prev_state = std::mem::replace(state, ArrayD::zeros([2usize, 1, 128].as_slice()));
        let state_val = Value::from_array(prev_state)
            .map_err(|e| anyhow::anyhow!("ort Value error (state): {e}"))?;

        let sr_val = Value::from_array(sample_rate.clone())
            .map_err(|e| anyhow::anyhow!("ort Value error (sr): {e}"))?;

        let outputs = session
            .run(ort::inputs![
                "input" => frame_val,
                "state" => state_val,
                "sr"    => sr_val
            ])
            .map_err(|e| anyhow::anyhow!("Silero v6.x inference error: {e}"))?;

        // Restore state
        let (shape, state_data) = outputs["stateN"]
            .try_extract_tensor::<f32>()
            .map_err(|e| anyhow::anyhow!("stateN extraction: {e}"))?;
        let shape_us: Vec<usize> = shape.as_ref().iter().map(|&d| d as usize).collect();
        *state = ArrayD::from_shape_vec(shape_us.as_slice(), state_data.to_vec())
            .map_err(|e| anyhow::anyhow!("state reshape: {e}"))?;

        // Update context: last 64 samples of the audio window
        let ctx_start = audio_512.len().saturating_sub(V6_CONTEXT_SIZE);
        *context = Array1::from_vec(audio_512[ctx_start..].to_vec());

        // Extract probability
        let prob = *outputs["output"]
            .try_extract_tensor::<f32>()
            .map_err(|e| anyhow::anyhow!("output extraction: {e}"))?
            .1
            .first()
            .ok_or_else(|| anyhow::anyhow!("empty output tensor"))?;

        debug!("SileroVad v6.x prob = {:.4}", prob);
        Ok(prob)
    }
}

// ── VoiceActivityDetector impl ────────────────────────────────────────────────

impl VoiceActivityDetector for SileroVad {
    fn push_frame<'a>(&'a mut self, frame: &'a [f32]) -> Result<VadFrame<'a>> {
        // We accept any frame length for v4; for v6.x we buffer.
        match &mut self.engine {
            Engine::V4(vad) => {
                if frame.len() != V4_FRAME_SAMPLES {
                    anyhow::bail!(
                        "v4: expected {V4_FRAME_SAMPLES} samples, got {}",
                        frame.len()
                    );
                }
                let result = vad
                    .compute(frame)
                    .map_err(|e| anyhow::anyhow!("Silero VAD error: {e}"))?;
                if result.prob > self.threshold {
                    Ok(VadFrame::Speech(frame))
                } else {
                    Ok(VadFrame::Noise)
                }
            }

            Engine::V6 {
                session,
                state,
                context,
                sample_rate,
                audio_buf,
                last_prob,
            } => {
                // Accumulate incoming samples
                audio_buf.extend_from_slice(frame);

                // Run inference every time we have >= 512 samples
                let mut prob = *last_prob;
                while audio_buf.len() >= V6_AUDIO_SAMPLES {
                    let chunk: Vec<f32> = audio_buf.drain(..V6_AUDIO_SAMPLES).collect();
                    prob = Self::infer_v6(session, state, context, sample_rate, &chunk)?;
                }
                *last_prob = prob;

                if prob > self.threshold {
                    Ok(VadFrame::Speech(frame))
                } else {
                    Ok(VadFrame::Noise)
                }
            }
        }
    }

    /// Reset RNN state and buffers so each new recording starts fresh.
    fn reset(&mut self) {
        match &mut self.engine {
            Engine::V4(vad) => vad.reset(),
            Engine::V6 {
                state,
                context,
                audio_buf,
                last_prob,
                ..
            } => {
                state.fill(0.0);
                context.fill(0.0);
                audio_buf.clear();
                *last_prob = 0.0;
            }
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn run_silence_test(path: &str, label: &str) {
        if !Path::new(path).exists() {
            println!("Skipping {label} test: file not found at {path}");
            return;
        }
        let mut vad = SileroVad::new(path, 0.3).expect("failed to create SileroVad");
        // 30-ms frames (pipeline size) for both models
        let silence_frame = vec![0.0f32; PIPELINE_FRAME_SAMPLES];
        for i in 0..20 {
            let res = vad.push_frame(&silence_frame).expect("push_frame failed");
            println!("{label} frame {i}: is_speech={}", res.is_speech());
        }
    }

    #[test]
    fn test_silero_vad_v4_silence() {
        run_silence_test("resources/models/silero_vad_v4.onnx", "v4");
    }

    #[test]
    fn test_silero_vad_v6_2_silence() {
        run_silence_test("resources/models/silero_vad_v6.2.onnx", "v6.2");
    }
}
