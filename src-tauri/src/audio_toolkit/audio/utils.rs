use anyhow::Result;
use hound::{WavReader, WavSpec, WavWriter};
use log::debug;
use std::path::Path;
use serde::Serialize;
use specta::Type;

/// Read a WAV file and return normalised f32 samples.
pub fn read_wav_samples<P: AsRef<Path>>(file_path: P) -> Result<Vec<f32>> {
    let reader = WavReader::open(file_path.as_ref())?;
    let samples = reader
        .into_samples::<i16>()
        .map(|s| s.map(|v| v as f32 / i16::MAX as f32))
        .collect::<Result<Vec<f32>, _>>()?;
    Ok(samples)
}

/// Verify a WAV file by reading it back and checking the sample count.
pub fn verify_wav_file<P: AsRef<Path>>(file_path: P, expected_samples: usize) -> Result<()> {
    let reader = WavReader::open(file_path.as_ref())?;
    let actual_samples = reader.len() as usize;
    if actual_samples != expected_samples {
        anyhow::bail!(
            "WAV sample count mismatch: expected {}, got {}",
            expected_samples,
            actual_samples
        );
    }
    Ok(())
}

/// Save audio samples as a WAV file
pub fn save_wav_file<P: AsRef<Path>>(file_path: P, samples: &[f32]) -> Result<()> {
    let spec = WavSpec {
        channels: 1,
        sample_rate: 16000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = WavWriter::create(file_path.as_ref(), spec)?;

    // Convert f32 samples to i16 for WAV
    for sample in samples {
        let sample_i16 = (sample * i16::MAX as f32) as i16;
        writer.write_sample(sample_i16)?;
    }

    writer.finalize()?;
    debug!("Saved WAV file: {:?}", file_path.as_ref());
    Ok(())
}

/// RMS amplitude envelope — used for the waveform HUD during TTS playback.
/// Divides audio into `num_bars` segments and returns the RMS of each.
#[derive(Debug, Clone, Serialize, Type)]
pub struct AmplitudeEnvelope {
    pub values: Vec<f32>,
    #[specta(type = specta_typescript::Number)]
    pub duration_ms: u64,
}

/// Extract an RMS amplitude envelope from WAV bytes for waveform visualization.
pub fn extract_envelope(wav_bytes: &[u8], num_bars: usize) -> Option<AmplitudeEnvelope> {
    use std::io::Cursor;
    let reader = WavReader::new(Cursor::new(wav_bytes)).ok()?;
    let spec = reader.spec();
    let samples: Vec<f32> = reader
        .into_samples::<i16>()
        .filter_map(|s| s.ok())
        .map(|s| s as f32 / i16::MAX as f32)
        .collect();

    if samples.is_empty() {
        return None;
    }

    let total_samples = samples.len();
    let sample_rate = spec.sample_rate as u64;
    let duration_ms = (total_samples as u64 * 1000) / sample_rate;
    let chunk_size = (total_samples / num_bars).max(1);

    let values: Vec<f32> = (0..num_bars)
        .map(|i| {
            let start = i * chunk_size;
            let end = ((i + 1) * chunk_size).min(total_samples);
            let chunk = &samples[start..end];
            if chunk.is_empty() {
                return 0.0;
            }
            let rms = (chunk.iter().map(|s| s * s).sum::<f32>() / chunk.len() as f32).sqrt();
            (rms * 4.0).clamp(0.0, 1.0) // scale up for visibility
        })
        .collect();

    Some(AmplitudeEnvelope { values, duration_ms })
}
