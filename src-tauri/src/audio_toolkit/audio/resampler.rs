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
    in_hz: usize,
    out_hz: usize,
    /// Samples in/out of the inner resampler; `finish()` uses the pair to
    /// know how much real audio (~10-30ms) its delay line still holds.
    in_count: usize,
    out_count: usize,
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
            in_hz,
            out_hz,
            in_count: 0,
            out_count: 0,
        }
    }

    pub fn push(&mut self, mut src: &[f32], mut emit: impl FnMut(&[f32])) {
        if self.resampler.is_none() {
            self.emit_frames(src, &mut emit);
            return;
        }
        self.in_count += src.len();

        while !src.is_empty() {
            let space = self.chunk_in - self.in_buf.len();
            let take = space.min(src.len());
            self.in_buf.extend_from_slice(&src[..take]);
            src = &src[take..];

            if self.in_buf.len() == self.chunk_in {
                if let Some(out) = self.process_chunk() {
                    self.out_count += out.len();
                    self.emit_frames(&out, &mut emit);
                }
                self.in_buf.clear();
            }
        }
    }

    pub fn finish(&mut self, mut emit: impl FnMut(&[f32])) {
        if self.resampler.is_some() {
            // Process any remaining input samples (padded to a full chunk).
            if !self.in_buf.is_empty() {
                self.in_buf.resize(self.chunk_in, 0.0);
                if let Some(out) = self.process_chunk() {
                    self.out_count += out.len();
                    self.emit_frames(&out, &mut emit);
                }
                self.in_buf.clear();
            }

            // Output lags input by output_delay() samples, so all real audio
            // has emerged only once in*ratio + delay samples are out. Feed
            // zero chunks until then, trimming the synthetic remainder.
            if self.in_count > 0 {
                let delay = self.resampler.as_ref().unwrap().output_delay();
                let expected = self.in_count * self.out_hz / self.in_hz + delay;
                let mut rounds = 0;
                while self.out_count < expected && rounds < 8 {
                    rounds += 1;
                    self.in_buf.resize(self.chunk_in, 0.0);
                    if let Some(out) = self.process_chunk() {
                        let take = (expected - self.out_count).min(out.len());
                        self.out_count += take;
                        self.emit_frames(&out[..take], &mut emit);
                    } else {
                        break;
                    }
                    self.in_buf.clear();
                }
            }
        }
        self.in_buf.clear();

        // Emit any remaining pending frame (padded with zeros)
        if !self.pending.is_empty() {
            self.pending.resize(self.frame_samples, 0.0);
            emit(&self.pending);
            self.pending.clear();
        }
    }

    /// Clear all internal buffers so the next `push()` starts from a clean state.
    ///
    /// Call this between recordings to prevent stale audio from the previous
    /// session leaking into the start of the next one via the following recording
    /// via the FFT overlap buffers.
    pub fn reset(&mut self) {
        self.in_buf.clear();
        self.pending.clear();
        self.in_count = 0;
        self.out_count = 0;
        if let Some(ref mut resampler) = self.resampler {
            resampler.reset();
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Generate a 1kHz sine wave at the given sample rate and duration.
    fn sine_wave(sample_rate: usize, freq: f64, duration_secs: f64) -> Vec<f32> {
        let n = (sample_rate as f64 * duration_secs) as usize;
        (0..n)
            .map(|i| {
                (2.0 * std::f64::consts::PI * freq * i as f64 / sample_rate as f64).sin() as f32
            })
            .collect()
    }

    fn collect_output(resampler: &mut FrameResampler, input: &[f32]) -> Vec<f32> {
        let mut out = Vec::new();
        resampler.push(input, |frame| out.extend_from_slice(frame));
        out
    }

    #[test]
    fn reset_clears_in_buf_and_pending() {
        let mut r = FrameResampler::new(48000, 16000, Duration::from_millis(30));

        // Push less than one chunk (1024 samples) to leave data in in_buf
        let partial = vec![0.5f32; 500];
        let _ = collect_output(&mut r, &partial);

        r.reset();

        // Now push silence — should get only silence out, no remnants of 0.5
        let silence = vec![0.0f32; 4096];
        let out = collect_output(&mut r, &silence);

        let max_abs = out.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
        assert!(
            max_abs < 0.01,
            "After reset, silence input should produce near-silence output, got max_abs={}",
            max_abs
        );
    }

    #[test]
    fn reset_clears_fft_overlap_buffers() {
        let mut r = FrameResampler::new(48000, 16000, Duration::from_millis(30));

        // Push a loud 1kHz sine wave through the resampler (simulates recording 1)
        let sine = sine_wave(48000, 1000.0, 0.5); // 500ms of audio
        let _ = collect_output(&mut r, &sine);
        r.finish(|_| {});

        // Reset (simulates new recording starting)
        r.reset();

        // Push silence (simulates recording 2 starting with no speech)
        let silence = vec![0.0f32; 4096];
        let out = collect_output(&mut r, &silence);

        // The output should be near-zero. If the FFT overlap buffers weren't
        // cleared, the sine wave's tail would leak into this output.
        let max_abs = out.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
        assert!(
            max_abs < 0.01,
            "FFT overlap should not leak after reset; got max_abs={} (expected near-zero)",
            max_abs
        );
    }

    #[test]
    fn reset_between_recordings_no_crosstalk() {
        let mut r = FrameResampler::new(48000, 16000, Duration::from_millis(30));

        // Recording 1: ascending ramp (distinctive pattern)
        let ramp: Vec<f32> = (0..48000).map(|i| i as f32 / 48000.0).collect(); // 1 second
        let out1 = collect_output(&mut r, &ramp);
        r.finish(|_| {});
        assert!(!out1.is_empty(), "Recording 1 should produce output");

        // Reset between recordings
        r.reset();

        // Recording 2: constant DC signal of -0.5
        let dc = vec![-0.5f32; 48000]; // 1 second
        let out2 = collect_output(&mut r, &dc);

        // After the resampler settles (skip first frame which may have transient),
        // all samples should be near -0.5, not contaminated by the ascending ramp.
        if out2.len() > 480 {
            // Skip first frame (480 samples at 16kHz/30ms), check the rest
            let tail = &out2[480..];
            for (i, &s) in tail.iter().enumerate() {
                assert!(
                    (s - (-0.5)).abs() < 0.05,
                    "Recording 2 sample {} = {} (expected ~-0.5); ramp leaked through",
                    i + 480,
                    s
                );
            }
        }
    }

    #[test]
    fn reset_passthrough_mode_clears_pending() {
        // When in_hz == out_hz, no rubato resampler is created (passthrough mode).
        // Reset should still clear the pending frame buffer.
        let mut r = FrameResampler::new(16000, 16000, Duration::from_millis(30));

        // Push partial frame (less than 480 samples) to leave data in pending
        let partial = vec![1.0f32; 200];
        let _ = collect_output(&mut r, &partial);

        r.reset();

        // Push silence
        let silence = vec![0.0f32; 960];
        let out = collect_output(&mut r, &silence);

        // First complete frame should be all zeros, not contain the 1.0 values
        if !out.is_empty() {
            let max_abs = out.iter().take(480).map(|s| s.abs()).fold(0.0f32, f32::max);
            assert!(
                max_abs < 0.001,
                "Passthrough mode: pending buffer should be cleared after reset, got max_abs={}",
                max_abs
            );
        }
    }

    /// Push silence ending in a 200-sample 0.5 burst, then assert finish()
    /// recovers the burst and emits floor(input*ratio) + output_delay samples,
    /// padded to whole 480-sample frames.
    fn assert_tail_burst_flushed(in_hz: usize, input_len: usize, expected_out: usize) {
        let mut rs = FrameResampler::new(in_hz, 16000, Duration::from_millis(30));
        let mut input = vec![0.0f32; input_len];
        input[input_len - 200..].fill(0.5);

        let mut out = Vec::new();
        rs.push(&input, |frame| out.extend_from_slice(frame));
        rs.finish(|frame| out.extend_from_slice(frame));

        let max_abs = out.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
        assert!(
            max_abs > 0.3,
            "tail burst was lost in the resampler, max_abs={max_abs}"
        );
        assert_eq!(out.len(), expected_out);
    }

    #[test]
    fn finish_flushes_resampler_delay() {
        // Exact chunks keep in_buf empty, so the burst survives only via the
        // delay-line drain. 4096 in -> 1365 real out + 171 delay -> 1920 framed.
        assert_tail_burst_flushed(48000, 4 * RESAMPLER_CHUNK_SIZE, 1920);
    }

    #[test]
    fn finish_flushes_resampler_delay_44100() {
        // fft_size_in (1323) exceeds the 1024 chunk, so the drain must survive
        // a zero-output round. 4096 in -> 1486 real out + 240 delay -> 1920.
        assert_tail_burst_flushed(44100, 4 * RESAMPLER_CHUNK_SIZE, 1920);
    }

    #[test]
    fn finish_flushes_unaligned_tail() {
        // Ends mid-chunk: partial-chunk path plus delay drain together.
        // 4396 in -> 1465 real out + 171 delay -> 1920 framed.
        assert_tail_burst_flushed(48000, 4 * RESAMPLER_CHUNK_SIZE + 300, 1920);
    }

    #[test]
    fn finish_does_not_leak_tail_into_next_session() {
        // 48kHz -> 16kHz, 30ms frames (480 output samples per frame).
        let mut rs = FrameResampler::new(48000, 16000, Duration::from_millis(30));

        // Leave a partial chunk buffered, then end the session.
        rs.push(&[0.5f32; 100], |_| {});
        rs.finish(|_| {});

        // One fresh chunk yields ~341 output samples — below one frame, so
        // nothing should be emitted yet. If finish() left its padded tail in
        // in_buf, that tail is re-processed first, the output crosses the
        // 480-sample frame boundary, and a stale frame is emitted here.
        let mut emitted = 0usize;
        rs.push(&[0.25f32; RESAMPLER_CHUNK_SIZE], |frame| {
            emitted += frame.len()
        });
        assert_eq!(
            emitted, 0,
            "stale resampler tail from finish() leaked into the next session"
        );
    }
}
