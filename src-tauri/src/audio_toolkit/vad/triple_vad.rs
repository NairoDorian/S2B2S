use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::{VadFrame, VoiceActivityDetector};
use crate::audio_toolkit::audio::NoiseSuppressor;
use crate::audio_toolkit::constants;

const SILERO_FRAME_MS: u32 = 30;
const SILERO_FRAME_SAMPLES: usize =
    (constants::WHISPER_SAMPLE_RATE * SILERO_FRAME_MS / 1000) as usize; // 480 samples

pub struct TripleVad {
    silero_vad: Box<dyn VoiceActivityDetector>,
    noise_suppressor: Option<NoiseSuppressor>,
    noise_suppression_enabled: Arc<AtomicBool>,
    voice_prob_threshold: f32,
    rms_threshold: f32,
    temp_buf: Vec<f32>,
}

impl TripleVad {
    pub fn new(
        silero_vad: Box<dyn VoiceActivityDetector>,
        noise_suppression_enabled: Arc<AtomicBool>,
        voice_prob_threshold: f32,
        rms_threshold: f32,
    ) -> Self {
        let noise_suppressor = NoiseSuppressor::new_16khz().ok();
        Self {
            silero_vad,
            noise_suppressor,
            noise_suppression_enabled,
            voice_prob_threshold,
            rms_threshold,
            temp_buf: Vec::new(),
        }
    }
}

impl VoiceActivityDetector for TripleVad {
    fn push_frame<'a>(&'a mut self, frame: &'a [f32]) -> Result<VadFrame<'a>> {
        if frame.len() != SILERO_FRAME_SAMPLES {
            anyhow::bail!(
                "expected {SILERO_FRAME_SAMPLES} samples, got {}",
                frame.len()
            );
        }

        // Stage 1: Fast Screen (Amplitude/RMS VAD)
        let mut sum_sq = 0.0f32;
        for &sample in frame {
            sum_sq += sample * sample;
        }
        let rms = (sum_sq / frame.len() as f32).sqrt();
        if rms < self.rms_threshold {
            return Ok(VadFrame::Noise);
        }

        // Stage 2: RNNoise Voice Probability / Denoising
        let ns_enabled = self.noise_suppression_enabled.load(Ordering::Relaxed);
        let mut processed_frame = frame.to_vec();
        let mut voice_prob = 1.0f32;

        if let Some(ns) = &mut self.noise_suppressor {
            let (denoised, prob) = ns.process_16khz_frame(frame);
            voice_prob = prob;
            if ns_enabled {
                processed_frame = denoised;
            }
        }

        // If RNNoise speech probability is too low, we classify it as noise
        if voice_prob < self.voice_prob_threshold {
            return Ok(VadFrame::Noise);
        }

        // Stage 3: Silero VAD Confirmation
        self.temp_buf = processed_frame;
        let silero_result = self.silero_vad.push_frame(&self.temp_buf)?;
        if silero_result.is_speech() {
            Ok(VadFrame::Speech(&self.temp_buf))
        } else {
            Ok(VadFrame::Noise)
        }
    }

    fn reset(&mut self) {
        self.silero_vad.reset();
        self.temp_buf.clear();
    }
}
