use audioadapter::Adapter;
use audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler};
use std::time::Duration;

// Make this a constant you can tweak
const RESAMPLER_CHUNK_SIZE: usize = 1024;

pub struct FrameResampler {
    resampler: Option<Fft<f32>>,
    chunk_in: usize,
    in_buf: Vec<f32>,
    frame_samples: usize,
    pending: Vec<f32>,
}

impl FrameResampler {
    pub fn new(in_hz: usize, out_hz: usize, frame_dur: Duration) -> Self {
        let frame_samples = ((out_hz as f64 * frame_dur.as_secs_f64()).round()) as usize;
        assert!(frame_samples > 0, "frame duration too short");

        let resampler = (in_hz != out_hz).then(|| {
            // rubato 3.0: FftFixedIn -> Fft with an explicit fixed-size mode.
            // FixedSync::Input keeps the input chunk size fixed (the old FftFixedIn behavior).
            Fft::<f32>::new(in_hz, out_hz, RESAMPLER_CHUNK_SIZE, 1, 1, FixedSync::Input)
                .expect("Failed to create resampler")
        });

        // The FFT resampler rounds the fixed input size up to a multiple of the
        // rate-ratio GCD block, so feed exactly what it asks for on each call.
        let chunk_in = resampler
            .as_ref()
            .map(|r| r.input_frames_next())
            .unwrap_or(RESAMPLER_CHUNK_SIZE);

        Self {
            resampler,
            chunk_in,
            in_buf: Vec::with_capacity(chunk_in),
            frame_samples,
            pending: Vec::with_capacity(frame_samples),
        }
    }

    pub fn push(&mut self, mut src: &[f32], mut emit: impl FnMut(&[f32])) {
        if self.resampler.is_none() {
            self.emit_frames(src, &mut emit);
            return;
        }

        while !src.is_empty() {
            let space = self.chunk_in - self.in_buf.len();
            let take = space.min(src.len());
            self.in_buf.extend_from_slice(&src[..take]);
            src = &src[take..];

            if self.in_buf.len() == self.chunk_in {
                if let Some(out) = self.process_chunk() {
                    self.emit_frames(&out, &mut emit);
                }
                self.in_buf.clear();
            }
        }
    }

    pub fn finish(&mut self, mut emit: impl FnMut(&[f32])) {
        // Process any remaining input samples, padded with zeros to a full chunk.
        if self.resampler.is_some() && !self.in_buf.is_empty() {
            self.in_buf.resize(self.chunk_in, 0.0);
            if let Some(out) = self.process_chunk() {
                self.emit_frames(&out, &mut emit);
            }
            self.in_buf.clear();
        }

        // Emit any remaining pending frame (padded with zeros)
        if !self.pending.is_empty() {
            self.pending.resize(self.frame_samples, 0.0);
            emit(&self.pending);
            self.pending.clear();
        }
    }

    /// Resample the full `chunk_in` frames currently buffered in `in_buf`.
    /// rubato 3.0 takes an input `Adapter` and returns an interleaved output buffer.
    fn process_chunk(&mut self) -> Option<Vec<f32>> {
        let frames = self.in_buf.len();
        // Clone the chunk so the input adapter borrows a local, not `self` (which is
        // also mutably borrowed via the resampler).
        let input_data = self.in_buf.clone();
        let resampler = self.resampler.as_mut()?;
        let input = InterleavedSlice::new(&input_data, 1, frames).ok()?;
        match resampler.process(&input, 0, None) {
            Ok(out) => {
                let n = out.frames();
                let mut samples = Vec::with_capacity(n);
                for f in 0..n {
                    samples.push(out.read_sample(0, f).unwrap_or(0.0));
                }
                Some(samples)
            }
            Err(e) => {
                log::warn!("[Resampler] process failed: {e}");
                None
            }
        }
    }

    fn emit_frames(&mut self, mut data: &[f32], emit: &mut impl FnMut(&[f32])) {
        while !data.is_empty() {
            let space = self.frame_samples - self.pending.len();
            let take = space.min(data.len());
            self.pending.extend_from_slice(&data[..take]);
            data = &data[take..];

            if self.pending.len() == self.frame_samples {
                emit(&self.pending);
                self.pending.clear();
            }
        }
    }
}
