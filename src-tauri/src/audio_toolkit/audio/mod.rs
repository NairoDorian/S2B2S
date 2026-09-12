// Re-export all audio components
mod chunk_tap;
mod denoise;
mod device;
mod recorder;
mod resampler;
mod utils;
mod visualizer;

pub use chunk_tap::{ChunkTap, tap as chunk_tap};
pub use denoise::{
    DenoiseChain, DenoiseControls, DenoiseParams, MAX_DENOISE_VAD_GRACE_MS, RNNOISE_FRAME_SAMPLES,
    RNNOISE_SAMPLE_RATE,
};
pub use device::{
    CpalDeviceInfo, default_input_endpoint, default_output_endpoint, list_input_devices,
    list_output_devices,
};
pub use recorder::{
    AnalysisSink, AudioRecorder, DEFAULT_SPEECH_PAUSE_HOLD_MS, RecordedAudio, SpeechActivity,
    VadPolicy, is_microphone_access_denied, is_no_input_device_error,
};
pub use resampler::FrameResampler;
pub use utils::{read_wav_samples, save_raw_wav_file, save_wav_file, verify_wav_file};
pub use visualizer::AudioVisualiser;
