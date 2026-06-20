//! Audio processing toolkit: capture, resampling, VAD, noise suppression,
//! and spectral visualization. The foundation of the dictation pipeline.

pub mod audio;
pub mod constants;
pub mod text;
pub mod utils;
pub mod vad;

pub use audio::{
    encode_wav_bytes, extract_envelope, is_microphone_access_denied,
    is_no_input_device_error, list_input_devices, list_output_devices, read_wav_samples,
    save_wav_file, verify_wav_file, AmplitudeEnvelope, AudioRecorder, CpalDeviceInfo,
};
pub use text::{apply_custom_words, filter_transcription_output};
pub use utils::get_cpal_host;
pub use vad::{SileroVad, VoiceActivityDetector};
