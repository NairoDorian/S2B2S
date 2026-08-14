//! Batch file transcription: decode an audio file (wav/mp3/m4a/ogg/flac),
//! resample to 16 kHz mono, transcribe with the active STT model, and export
//! as plain text or timed subtitles (SRT/VTT via `crate::subtitle`).

use std::io::BufReader;
use std::sync::Arc;

use rodio::Source;

use crate::audio_toolkit::audio::FrameResampler;
use crate::audio_toolkit::constants::WHISPER_SAMPLE_RATE;
use crate::subtitle::{self, SubtitleFormat};
use tauri::{AppHandle, Manager, State};

/// File extensions accepted by the transcriber.
pub const SUPPORTED_EXTENSIONS: &[&str] = &["wav", "mp3", "m4a", "mp4", "ogg", "oga", "flac"];

/// Decode an audio file to mono 16 kHz f32 samples.
fn decode_audio_file(path: &str) -> Result<Vec<f32>, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("Failed to open file: {e}"))?;
    let decoder = rodio::Decoder::try_from(BufReader::new(file))
        .map_err(|e| format!("Unsupported or corrupt audio file: {e}"))?;

    // Fully decode into an interleaved f32 buffer (rodio handles mp3/flac/
    // vorbis/wav/aac/m4a and their sample formats).
    let buffer = decoder.record();
    let channels = buffer.channels().get() as usize;
    let source_rate = buffer.sample_rate().get();
    let interleaved: Vec<f32> = buffer.collect();

    // Downmix to mono by averaging channels.
    let mut samples: Vec<f32> = Vec::with_capacity(interleaved.len() / channels.max(1));
    if channels == 1 {
        samples = interleaved;
    } else {
        for chunk in interleaved.chunks_exact(channels) {
            samples.push(chunk.iter().sum::<f32>() / channels as f32);
        }
    }

    if source_rate != WHISPER_SAMPLE_RATE {
        let mut resampler = FrameResampler::new(
            source_rate as usize,
            WHISPER_SAMPLE_RATE as usize,
            std::time::Duration::from_millis(30),
        );
        let mut out = Vec::with_capacity(samples.len());
        resampler.push(&samples, &mut |frame: &[f32]| {
            out.extend_from_slice(frame);
        });
        resampler.finish(&mut |frame: &[f32]| {
            out.extend_from_slice(frame);
        });
        samples = out;
    }

    if samples.is_empty() {
        return Err("Audio file contained no decodable samples".to_string());
    }
    Ok(samples)
}

/// Transcribe an audio file with the active STT model and write the result.
/// `format` chooses the output container (None = plain `.txt`). Returns the
/// absolute path of the written transcript.
pub async fn transcribe_audio_file(
    app: &AppHandle,
    path: &str,
    format: Option<SubtitleFormat>,
) -> Result<String, String> {
    // Reject unknown extensions early (before spawning the decoder).
    let ext = std::path::Path::new(path)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if !SUPPORTED_EXTENSIONS.contains(&ext.as_str()) {
        return Err(format!(
            "Unsupported file type '.{ext}'. Supported: {}",
            SUPPORTED_EXTENSIONS.join(", ")
        ));
    }

    // File transcription and mic recording share the STT engine — refuse to
    // run while a recording/processing session is active.
    if let Some(audio_manager) =
        app.try_state::<Arc<crate::managers::audio::AudioRecordingManager>>()
    {
        if audio_manager.is_recording() {
            return Err("Cannot transcribe a file while a recording is in progress".to_string());
        }
    }

    let samples = decode_audio_file(path)?;
    let duration_secs = samples.len() as f32 / WHISPER_SAMPLE_RATE as f32;
    if duration_secs < 0.5 {
        return Err("Audio is too short to transcribe (minimum ~0.5s)".to_string());
    }

    let transcription_manager = app
        .try_state::<Arc<crate::managers::transcription::TranscriptionManager>>()
        .ok_or_else(|| "TranscriptionManager not registered".to_string())?
        .inner()
        .clone();
    let text = transcription_manager
        .transcribe(samples)
        .map_err(|e| format!("Transcription failed: {e}"))?;
    let text = text.trim();
    if text.is_empty() {
        return Err("Transcription produced no text".to_string());
    }

    // Output location: <app data>/transcripts/<stem>-transcript.<ext>
    let input = std::path::Path::new(path);
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "transcript".to_string());
    let app_data = crate::portable::app_data_dir(app)
        .map_err(|e| format!("Failed to resolve app data dir: {e}"))?;
    let out_dir = app_data.join("transcripts");
    std::fs::create_dir_all(&out_dir)
        .map_err(|e| format!("Failed to create transcripts dir: {e}"))?;

    let (content, ext) = match format {
        Some(SubtitleFormat::Srt) => {
            let segments = subtitle::text_to_subtitle_segments(text, duration_secs);
            (subtitle::segments_to_srt(&segments), "srt")
        }
        Some(SubtitleFormat::Vtt) => {
            let segments = subtitle::text_to_subtitle_segments(text, duration_secs);
            (subtitle::segments_to_vtt(&segments), "vtt")
        }
        None => (text.to_string(), "txt"),
    };

    let out_path = out_dir.join(format!("{stem}-transcript.{ext}"));
    std::fs::write(&out_path, content).map_err(|e| format!("Failed to write transcript: {e}"))?;

    Ok(out_path.to_string_lossy().to_string())
}

/// Tauri command wrapper.
#[tauri::command]
#[specta::specta]
pub async fn transcribe_audio_file_command(
    app: AppHandle,
    _transcription_manager: State<'_, Arc<crate::managers::transcription::TranscriptionManager>>,
    path: String,
    format: Option<SubtitleFormat>,
) -> Result<String, String> {
    transcribe_audio_file(&app, &path, format).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsupported_extension_error_is_clean() {
        // decode_audio_file on a bogus path must return a clean error, not panic.
        let err = decode_audio_file("nonexistent-file.xyz");
        assert!(err.is_err());
    }
}
