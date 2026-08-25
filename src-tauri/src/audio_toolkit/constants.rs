pub const WHISPER_SAMPLE_RATE: u32 = 16000;

/// Silero v5/v6 accepts exactly one window size per sample rate — 512 samples
/// at 16 kHz — and silently returns near-zero probabilities for anything else
/// rather than failing. The capture pipeline is framed to match, so one
/// resampled frame is exactly one VAD decision.
pub const VAD_FRAME_SAMPLES: usize = 512;

/// Length of that window in milliseconds (512 / 16000). Used to frame the
/// resampler and to bill time on the speech clock.
pub const VAD_FRAME_MS: u64 = 32;

/// Samples of the preceding window that Silero v5/v6 expects prepended to each
/// chunk, matching the reference implementation's `_context`. Without it the
/// model cannot tell speech from silence at all.
pub const VAD_CONTEXT_SAMPLES: usize = 64;
