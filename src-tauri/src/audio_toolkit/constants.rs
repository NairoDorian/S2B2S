pub const WHISPER_SAMPLE_RATE: u32 = 16000;

/// Earshot uses 256 samples at 16 kHz (16 ms per frame). The capture pipeline
/// frames itself to this size so one resampled frame is exactly one VAD
/// decision.
pub const VAD_FRAME_SAMPLES: usize = 256;

/// Length of that window in milliseconds (256 / 16000). The capture path
/// derives its framing and the speech clock's billing from the detector's
/// `frame_samples()` instead; this constant frames the offline resamplers
/// (`read_wav_samples`, file transcription) and the tests.
pub const VAD_FRAME_MS: u64 = 16;
