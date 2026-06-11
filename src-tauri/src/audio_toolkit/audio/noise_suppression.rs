use nnnoiseless::DenoiseState;
use rubato::{Fft, FixedSync, Resampler};
use audioadapter::Adapter;
use audioadapter_buffers::direct::InterleavedSlice;

const RNNOISE_SAMPLE_RATE: usize = 48_000;
const RNNOISE_FRAME_SIZE: usize = DenoiseState::FRAME_SIZE; // 480 samples
const I16_SCALE: f32 = i16::MAX as f32;

pub struct NoiseSuppressor {
    upsampler: Fft<f32>,
    downsampler: Fft<f32>,
    denoise: Box<DenoiseState<'static>>,
    input_frame_size: usize,
}

impl NoiseSuppressor {
    pub fn new_16khz() -> Result<Self, String> {
        let input_frame_size = 480; // 30ms of 16kHz
        let output_frame_size = RNNOISE_FRAME_SIZE * 3; // 1440 samples (30ms of 48kHz)

        // rubato 3.0 Fft: new(in_rate, out_rate, max_resample_chunk_size, channels, sub_chunks, sync)
        let upsampler = Fft::<f32>::new(
            16000,
            RNNOISE_SAMPLE_RATE,
            input_frame_size,
            1,
            1,
            FixedSync::Input,
        )
        .map_err(|e| format!("Failed to create RNNoise upsampler: {e}"))?;

        let downsampler = Fft::<f32>::new(
            RNNOISE_SAMPLE_RATE,
            16000,
            output_frame_size,
            1,
            1,
            FixedSync::Input,
        )
        .map_err(|e| format!("Failed to create RNNoise downsampler: {e}"))?;

        Ok(Self {
            upsampler,
            downsampler,
            denoise: DenoiseState::new(),
            input_frame_size,
        })
    }

    pub fn process_16khz_frame(&mut self, samples: &[f32]) -> (Vec<f32>, f32) {
        if samples.len() != self.input_frame_size {
            return (samples.to_vec(), 0.0);
        }

        // Upsample to 48kHz
        let input = match InterleavedSlice::new(samples, 1, samples.len()) {
            Ok(input) => input,
            Err(e) => {
                log::warn!("RNNoise InterleavedSlice failed: {e}");
                return (samples.to_vec(), 0.0);
            }
        };

        let upsampled_buf = match self.upsampler.process(&input, 0, None) {
            Ok(out) => {
                let n = out.frames();
                let mut s = Vec::with_capacity(n);
                for f in 0..n {
                    s.push(out.read_sample(0, f).unwrap_or(0.0));
                }
                s
            }
            Err(err) => {
                log::warn!("RNNoise upsample failed: {err}");
                return (samples.to_vec(), 0.0);
            }
        };

        if upsampled_buf.len() < RNNOISE_FRAME_SIZE {
            return (samples.to_vec(), 0.0);
        }

        let mut denoised_48khz = Vec::with_capacity(upsampled_buf.len());
        let mut input_frame = [0.0f32; RNNOISE_FRAME_SIZE];
        let mut output_frame = [0.0f32; RNNOISE_FRAME_SIZE];
        let mut max_voice_prob = 0.0f32;

        for chunk in upsampled_buf.chunks_exact(RNNOISE_FRAME_SIZE) {
            for (dst, src) in input_frame.iter_mut().zip(chunk.iter()) {
                *dst = (*src * I16_SCALE).clamp(i16::MIN as f32, i16::MAX as f32);
            }

            let prob = self.denoise.process_frame(&mut output_frame, &input_frame);
            if prob > max_voice_prob {
                max_voice_prob = prob;
            }

            denoised_48khz.extend(
                output_frame
                    .iter()
                    .map(|sample| (*sample / I16_SCALE).clamp(-1.0, 1.0)),
            );
        }

        if denoised_48khz.is_empty() {
            return (samples.to_vec(), 0.0);
        }

        // Downsample back to 16kHz
        let output_input = match InterleavedSlice::new(&denoised_48khz, 1, denoised_48khz.len()) {
            Ok(input) => input,
            Err(e) => {
                log::warn!("RNNoise InterleavedSlice downsample failed: {e}");
                return (samples.to_vec(), max_voice_prob);
            }
        };

        match self.downsampler.process(&output_input, 0, None) {
            Ok(out) => {
                let n = out.frames();
                let mut output = Vec::with_capacity(n);
                for f in 0..n {
                    output.push(out.read_sample(0, f).unwrap_or(0.0));
                }
                if output.len() > samples.len() {
                    output.truncate(samples.len());
                } else if output.len() < samples.len() {
                    output.resize(samples.len(), 0.0);
                }
                (output, max_voice_prob)
            }
            Err(err) => {
                log::warn!("RNNoise downsample failed: {err}");
                (samples.to_vec(), max_voice_prob)
            }
        }
    }
}
