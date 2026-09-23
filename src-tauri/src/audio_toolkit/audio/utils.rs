use anyhow::Result;
use hound::{WavReader, WavSpec, WavWriter};
use log::debug;
use std::path::Path;

/// Read a WAV file and return normalised f32 samples at 16 kHz.
/// Handles 16/24/32-bit integer and 32-bit IEEE float WAV files at any sample rate,
/// automatically downsampling to 16 kHz if needed (for Whisper / benchmarking).
pub fn read_wav_samples<P: AsRef<Path>>(file_path: P) -> Result<Vec<f32>> {
    let reader = WavReader::open(file_path.as_ref())?;
    let spec = reader.spec();

    let raw_samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .into_samples::<f32>()
            .collect::<Result<Vec<f32>, _>>()?,
        hound::SampleFormat::Int => match spec.bits_per_sample {
            16 => reader
                .into_samples::<i16>()
                .map(|s| s.map(|v| v as f32 / i16::MAX as f32))
                .collect::<Result<Vec<f32>, _>>()?,
            24 => reader
                .into_samples::<i32>()
                .map(|s| s.map(|v| v as f32 / 8_388_607.0))
                .collect::<Result<Vec<f32>, _>>()?,
            32 => reader
                .into_samples::<i32>()
                .map(|s| s.map(|v| v as f32 / i32::MAX as f32))
                .collect::<Result<Vec<f32>, _>>()?,
            bits => {
                anyhow::bail!("Unsupported bits per sample in WAV: {bits}");
            }
        },
    };

    if spec.sample_rate == 16000 {
        Ok(raw_samples)
    } else {
        // Resample to 16 kHz so acoustic models and benchmarks always receive valid 16kHz audio
        let mut resampler = crate::audio_toolkit::audio::FrameResampler::new(
            spec.sample_rate as usize,
            16000,
            std::time::Duration::from_millis(crate::audio_toolkit::constants::VAD_FRAME_MS),
        );
        let mut out = Vec::new();
        resampler.push(&raw_samples, &mut |frame: &[f32]| {
            out.extend_from_slice(frame);
        });
        resampler.finish(&mut |frame: &[f32]| {
            out.extend_from_slice(frame);
        });
        Ok(out)
    }
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

/// Save audio samples as a standard 16 kHz 16-bit integer WAV file
pub fn save_wav_file<P: AsRef<Path>>(file_path: P, samples: &[f32]) -> Result<()> {
    let spec = WavSpec {
        channels: 1,
        sample_rate: 16000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = WavWriter::create(file_path.as_ref(), spec)?;

    // Convert f32 samples to i16 for standard WAV
    for sample in samples {
        let sample_i16 = (sample * i16::MAX as f32).clamp(i16::MIN as f32, i16::MAX as f32) as i16;
        writer.write_sample(sample_i16)?;
    }

    writer.finalize()?;
    debug!("Saved standard 16kHz WAV file: {:?}", file_path.as_ref());
    Ok(())
}

/// Save raw uncompressed audio samples matching the hardware microphone's native
/// sample rate and sample format (16-bit int, 24-bit int, or 32-bit float).
pub fn save_raw_wav_file<P: AsRef<Path>>(
    file_path: P,
    samples: &[f32],
    sample_rate: u32,
    sample_format: cpal::SampleFormat,
) -> Result<()> {
    match sample_format {
        cpal::SampleFormat::F32 => {
            let spec = WavSpec {
                channels: 1,
                sample_rate,
                bits_per_sample: 32,
                sample_format: hound::SampleFormat::Float,
            };
            let mut writer = WavWriter::create(file_path.as_ref(), spec)?;
            for &sample in samples {
                writer.write_sample(sample)?;
            }
            writer.finalize()?;
            debug!(
                "Saved raw uncompressed WAV file ({} Hz, 32-bit float): {:?}",
                sample_rate,
                file_path.as_ref()
            );
        }
        cpal::SampleFormat::I32 => {
            // Native 24-bit / 32-bit integer PCM
            let spec = WavSpec {
                channels: 1,
                sample_rate,
                bits_per_sample: 24,
                sample_format: hound::SampleFormat::Int,
            };
            let mut writer = WavWriter::create(file_path.as_ref(), spec)?;
            for &sample in samples {
                let sample_i24 = (sample * 8_388_607.0).clamp(-8_388_608.0, 8_388_607.0) as i32;
                writer.write_sample(sample_i24)?;
            }
            writer.finalize()?;
            debug!(
                "Saved raw uncompressed WAV file ({} Hz, 24-bit integer): {:?}",
                sample_rate,
                file_path.as_ref()
            );
        }
        cpal::SampleFormat::I16 => {
            let spec = WavSpec {
                channels: 1,
                sample_rate,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            };
            let mut writer = WavWriter::create(file_path.as_ref(), spec)?;
            for &sample in samples {
                let sample_i16 =
                    (sample * i16::MAX as f32).clamp(i16::MIN as f32, i16::MAX as f32) as i16;
                writer.write_sample(sample_i16)?;
            }
            writer.finalize()?;
            debug!(
                "Saved raw uncompressed WAV file ({} Hz, 16-bit integer): {:?}",
                sample_rate,
                file_path.as_ref()
            );
        }
        _ => {
            // Default fallback to 32-bit float for any other format
            let spec = WavSpec {
                channels: 1,
                sample_rate,
                bits_per_sample: 32,
                sample_format: hound::SampleFormat::Float,
            };
            let mut writer = WavWriter::create(file_path.as_ref(), spec)?;
            for &sample in samples {
                writer.write_sample(sample)?;
            }
            writer.finalize()?;
            debug!(
                "Saved raw uncompressed WAV file ({} Hz, fallback 32-bit float): {:?}",
                sample_rate,
                file_path.as_ref()
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_raw_wav_float_48khz_roundtrip() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path();

        // 48000 Hz tone (1 second of samples)
        let sample_rate = 48000;
        let original: Vec<f32> = (0..sample_rate)
            .map(|i| {
                ((i as f32 * 440.0 * 2.0 * std::f32::consts::PI) / sample_rate as f32).sin() * 0.5
            })
            .collect();

        save_raw_wav_file(path, &original, sample_rate, cpal::SampleFormat::F32).unwrap();
        verify_wav_file(path, original.len()).unwrap();

        // Reading back decodes and automatically downsamples to 16 kHz in
        // whole VAD frames (256 samples).
        let decoded = read_wav_samples(path).unwrap();
        assert_eq!(
            decoded.len() % crate::audio_toolkit::constants::VAD_FRAME_SAMPLES,
            0
        );
        assert_eq!(
            decoded.len(),
            63 * crate::audio_toolkit::constants::VAD_FRAME_SAMPLES
        ); // ≈ 1 s at 16 kHz
        // Verify audio content is non-empty and non-zero
        assert!(decoded.iter().any(|&s| s.abs() > 0.1));
    }

    #[test]
    fn test_raw_wav_i32_24bit_roundtrip() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path();

        let sample_rate = 48000;
        let original: Vec<f32> = (0..480).map(|i| (i as f32 / 480.0) * 0.8 - 0.4).collect();

        save_raw_wav_file(path, &original, sample_rate, cpal::SampleFormat::I32).unwrap();
        verify_wav_file(path, original.len()).unwrap();

        let decoded = read_wav_samples(path).unwrap();
        // 160 samples at 16 kHz + the resampler delay, padded to two
        // 256-sample VAD frames.
        assert_eq!(decoded.len(), 512);
    }

    #[test]
    fn test_raw_wav_i16_16khz_roundtrip() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path();

        let original = vec![0.0f32, 0.25, -0.25, 0.5, -0.5];
        save_raw_wav_file(path, &original, 16000, cpal::SampleFormat::I16).unwrap();
        verify_wav_file(path, original.len()).unwrap();

        let decoded = read_wav_samples(path).unwrap();
        assert_eq!(decoded.len(), original.len());
        for (orig, dec) in original.iter().zip(decoded.iter()) {
            assert!((orig - dec).abs() < 0.001);
        }
    }
}
