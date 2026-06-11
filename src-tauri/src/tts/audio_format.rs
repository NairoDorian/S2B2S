use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, Type)]
#[serde(rename_all = "lowercase")]
pub enum AudioFormat {
    #[default]
    Wav,
    Mp3,
    Ogg,
    Flac,
}

impl AudioFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            AudioFormat::Wav => "wav",
            AudioFormat::Mp3 => "mp3",
            AudioFormat::Ogg => "ogg",
            AudioFormat::Flac => "flac",
        }
    }

    pub fn mime_type(self) -> &'static str {
        match self {
            AudioFormat::Wav => "audio/wav",
            AudioFormat::Mp3 => "audio/mpeg",
            AudioFormat::Ogg => "audio/ogg",
            AudioFormat::Flac => "audio/flac",
        }
    }
}

/// Convert WAV bytes to the requested format via ffmpeg.
/// Falls back to WAV if ffmpeg is not available or conversion fails.
pub fn convert_audio_bytes(wav_bytes: &[u8], format: AudioFormat) -> Vec<u8> {
    if format == AudioFormat::Wav || wav_bytes.is_empty() || wav_bytes.len() < 44 {
        return wav_bytes.to_vec();
    }

    let ext = format.as_str();
    let temp_dir = std::env::temp_dir();
    let input_path = temp_dir.join("s2b2s_convert_input.wav");
    let output_path = temp_dir.join(format!("s2b2s_convert_output.{}", ext));

    // Write input WAV
    if std::fs::write(&input_path, wav_bytes).is_err() {
        return wav_bytes.to_vec();
    }

    // Build ffmpeg command per format
    let codec_args: &[&str] = match format {
        AudioFormat::Mp3 => &["-c:a", "libmp3lame", "-b:a", "192k"],
        AudioFormat::Ogg => &["-c:a", "libvorbis", "-q:a", "4"],
        AudioFormat::Flac => &["-c:a", "flac", "-compression_level", "5"],
        AudioFormat::Wav => &[],
    };

    let output = std::process::Command::new("ffmpeg")
        .args(["-y", "-i"])
        .arg(&input_path)
        .args(codec_args)
        .arg(&output_path)
        .output();

    // Cleanup input
    let _ = std::fs::remove_file(&input_path);

    match output {
        Ok(out) if out.status.success() => {
            let result = std::fs::read(&output_path).unwrap_or_default();
            let _ = std::fs::remove_file(&output_path);
            if result.is_empty() { wav_bytes.to_vec() } else { result }
        }
        _ => {
            let _ = std::fs::remove_file(&output_path);
            log::warn!("[TTS] ffmpeg not available or conversion failed; falling back to WAV");
            wav_bytes.to_vec()
        }
    }
}

/// Save audio bytes to a file with the appropriate extension.
pub fn save_audio_file(bytes: &[u8], path: &std::path::Path, format: AudioFormat) -> Result<(), String> {
    let data = if format == AudioFormat::Wav {
        bytes.to_vec()
    } else {
        convert_audio_bytes(bytes, format)
    };
    std::fs::write(path, &data).map_err(|e| format!("Failed to write audio file: {e}"))?;
    Ok(())
}
