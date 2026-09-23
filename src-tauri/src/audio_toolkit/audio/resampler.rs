use rubato::audioadapter::Adapter;
use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler};
use std::time::Duration;

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
            Fft::<f32>::new(in_hz, out_hz, RESAMPLER_CHUNK_SIZE, 1, FixedSync::Input)
                .expect("Failed to create resampler")
        });

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
            // Process any remaining input samples (padded internally with zeros).
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
                        self.in_buf.clear();
                        let take = (expected - self.out_count).min(out.len());
                        self.out_count += take;
                        self.emit_frames(&out[..take], &mut emit);
                    } else {
                        self.in_buf.clear();
                        break;
                    }
                }
            }
        }

        // Emit any remaining pending frame (padded with zeros)
        if !self.pending.is_empty() {
            self.pending.resize(self.frame_samples, 0.0);
            emit(&self.pending);
            self.pending.clear();
        }
    }

    pub fn reset(&mut self) {
        self.in_buf.clear();
        self.pending.clear();
        self.in_count = 0;
        self.out_count = 0;
        if let Some(ref mut resampler) = self.resampler {
            resampler.reset();
        }
    }

    fn process_chunk(&mut self) -> Option<Vec<f32>> {
        // Split the borrow so the input is read in place, without a per-chunk
        // copy of `in_buf` on the audio consumer thread.
        let Self {
            resampler, in_buf, ..
        } = self;
        let resampler = resampler.as_mut()?;
        let input = InterleavedSlice::new(in_buf.as_slice(), 1, in_buf.len()).ok()?;
        match resampler.process(&input, None) {
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

        let partial = vec![0.5f32; 500];
        let _ = collect_output(&mut r, &partial);

        r.reset();

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

        let sine = sine_wave(48000, 1000.0, 0.5);
        let _ = collect_output(&mut r, &sine);
        r.finish(|_| {});

        r.reset();

        let silence = vec![0.0f32; 4096];
        let out = collect_output(&mut r, &silence);

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

        let ramp: Vec<f32> = (0..48000).map(|i| i as f32 / 48000.0).collect();
        let out1 = collect_output(&mut r, &ramp);
        r.finish(|_| {});
        assert!(!out1.is_empty(), "Recording 1 should produce output");

        r.reset();

        let dc = vec![-0.5f32; 48000];
        let out2 = collect_output(&mut r, &dc);

        if out2.len() > 480 {
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
        let mut r = FrameResampler::new(16000, 16000, Duration::from_millis(30));

        let partial = vec![1.0f32; 200];
        let _ = collect_output(&mut r, &partial);

        r.reset();

        let silence = vec![0.0f32; 960];
        let out = collect_output(&mut r, &silence);

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
        // delay-line drain. 4096 in (85.3ms) -> 1365 real out + delay -> 1440 framed (90ms).
        assert_tail_burst_flushed(48000, 4 * RESAMPLER_CHUNK_SIZE, 1440);
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
        let mut rs = FrameResampler::new(48000, 16000, Duration::from_millis(30));

        rs.push(&[0.5f32; 100], |_| {});
        rs.finish(|_| {});

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
