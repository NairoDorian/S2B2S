//! Windows SAPI/OneCore TTS fallback — zero download, always available.
//!
//! Uses the Windows Speech API for TTS. Acts as a guaranteed fallback when
//! no other engine is configured or when local models fail to load.
//!
//! Platform: Windows only. On macOS/Linux, this backend returns an error.

use crate::tts::{TtsBackend, Voice};

pub struct SapiBackend {
    #[allow(dead_code)]
    voice: String,
    #[allow(dead_code)]
    speed: f32,
}

impl SapiBackend {
    pub fn new(voice: String, speed: f32) -> Self {
        Self { voice, speed }
    }

    pub fn list_voices() -> Vec<Voice> {
        #[cfg(target_os = "windows")]
        {
            use windows::core::BSTR;
            use windows::Win32::Media::Speech::{ISpeechVoice, SpVoice};
            use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};

            unsafe {
                let mut list = vec![Voice {
                    id: "sapi_default".to_string(),
                    name: "System Default (SAPI)".to_string(),
                    language: None,
                }];

                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
                if let Ok(voice) = CoCreateInstance::<_, ISpeechVoice>(&SpVoice, None, CLSCTX_ALL) {
                    if let Ok(tokens) = voice.GetVoices(&BSTR::new(), &BSTR::new()) {
                        if let Ok(count) = tokens.Count() {
                            for i in 0..count {
                                if let Ok(token) = tokens.Item(i) {
                                    if let Ok(id) = token.Id() {
                                        // Locale ID 0 uses the default system language
                                        let name = token.GetDescription(0).map(|b| b.to_string()).unwrap_or_else(|_| id.to_string());
                                        list.push(Voice {
                                            id: id.to_string(),
                                            name,
                                            language: None,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
                list
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            vec![Voice {
                id: "sapi_default".to_string(),
                name: "System Default (SAPI)".to_string(),
                language: None,
            }]
        }
    }
}

impl TtsBackend for SapiBackend {
    fn name(&self) -> &str {
        "SAPI"
    }

    fn synthesize(&self, text: &str, voice: &str, speed: f32) -> Result<Vec<u8>, String> {
        #[cfg(target_os = "windows")]
        {
            use windows::core::{Interface, GUID, IUnknown, PCWSTR};
            use windows::Win32::Media::Audio::{WAVEFORMATEX, WAVE_FORMAT_PCM};
            use windows::Win32::Media::Speech::{ISpStream, ISpVoice, SpStream, SpVoice};
            use windows::Win32::System::Com::StructuredStorage::CreateStreamOnHGlobal;
            use windows::Win32::System::Com::{
                CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED, IStream,
                STREAM_SEEK_END, STREAM_SEEK_SET,
            };
            use std::ffi::c_void;

            // Define SPDFID_WaveFormatEx manually as it's not exported correctly in windows-rs.
            // The correct SAPI GUID is C31ADBAE-527F-4FF5-A230-F62BB61FF70C.
            const SPDFID_WAVEFORMATEX: GUID = GUID::from_values(
                0xC31ADBAE,
                0x527F,
                0x4FF5,
                [0xA2, 0x30, 0xF6, 0x2B, 0xB6, 0x1F, 0xF7, 0x0C],
            );

            unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

                // Create in-memory stream using the standard 2-argument signature returning Result
                let mem_stream: IStream = CreateStreamOnHGlobal(
                    windows::Win32::Foundation::HGLOBAL::default(),
                    true,
                ).map_err(|e| format!("CreateStreamOnHGlobal failed: {e}"))?;

                // Create SAPI stream wrapper
                let sp_stream: ISpStream = CoCreateInstance(&SpStream, None, CLSCTX_ALL)
                    .map_err(|e| format!("Failed to create ISpStream instance: {e}"))?;

                // SAPI format specifier (16kHz, 16-bit, mono PCM)
                let wfx = WAVEFORMATEX {
                    wFormatTag: WAVE_FORMAT_PCM as u16,
                    nChannels: 1,
                    nSamplesPerSec: 16000,
                    nAvgBytesPerSec: 32000,
                    nBlockAlign: 2,
                    wBitsPerSample: 16,
                    cbSize: 0,
                };

                sp_stream.SetBaseStream(&mem_stream, &SPDFID_WAVEFORMATEX, &wfx as *const WAVEFORMATEX)
                    .map_err(|e| format!("Failed to set base stream: {e}"))?;

                // Create Voice
                let speaker: ISpVoice = CoCreateInstance(&SpVoice, None, CLSCTX_ALL)
                    .map_err(|e| format!("Failed to create ISpVoice instance: {e}"))?;

                // If a specific voice was requested, try to select it
                if !voice.is_empty() && voice != "sapi_default" {
                    use windows::Win32::Media::Speech::{ISpObjectToken, SpObjectToken};
                    if let Ok(token) = CoCreateInstance::<_, ISpObjectToken>(&SpObjectToken, None, CLSCTX_ALL) {
                        let voice_w: Vec<u16> = voice.encode_utf16().chain(std::iter::once(0)).collect();
                        if token.SetId(None, PCWSTR(voice_w.as_ptr()), false).is_ok() {
                            let _ = speaker.SetVoice(&token);
                        }
                    }
                }

                // Adjust speed/rate. SAPI rate ranges from -10 to 10 (default 0).
                let rate = ((speed - 1.0) * 10.0).clamp(-10.0, 10.0) as i32;
                let _ = speaker.SetRate(rate);

                // Set speaker output to our stream
                speaker.SetOutput(Some(&sp_stream.cast::<IUnknown>().map_err(|e| format!("Cast failed: {e}"))?), true)
                    .map_err(|e| format!("Failed to set speaker output: {e}"))?;

                // Convert text to wide string
                let text_w: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
                let pcwstr = PCWSTR(text_w.as_ptr());

                // Speak synchronously to write all audio into the stream
                let _ = speaker.Speak(pcwstr, 0, None)
                    .map_err(|e| format!("SAPI Speak failed: {e}"))?;

                // Wait until done (flushes buffer to stream)
                let _ = speaker.WaitUntilDone(u32::MAX);

                // Seek to the end of the memory stream to find the size
                let mut size: u64 = 0;
                mem_stream.Seek(0, STREAM_SEEK_END, Some(&mut size))
                    .map_err(|e| format!("Failed to seek to end of stream: {e}"))?;

                if size == 0 {
                    return Err("Synthesized stream is empty".to_string());
                }

                // Seek back to the beginning to read the bytes
                mem_stream.Seek(0, STREAM_SEEK_SET, None)
                    .map_err(|e| format!("Failed to seek to start of stream: {e}"))?;

                // Read all bytes from the stream
                let mut pcm_buffer = vec![0u8; size as usize];
                let mut bytes_read = 0u32;
                mem_stream.Read(pcm_buffer.as_mut_ptr() as *mut c_void, size as u32, Some(&mut bytes_read)).ok()
                    .map_err(|e| format!("Failed to read from stream: {e}"))?;

                // SAPI writes raw PCM bytes to the memory stream, so we wrap them in a standard WAV container.
                let wav_bytes = pcm_to_wav(&pcm_buffer[0..bytes_read as usize], 16000, 1, 16);
                Ok(wav_bytes)
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = text;
            let _ = voice;
            let _ = speed;
            Err("SAPI is only available on Windows".to_string())
        }
    }

    fn health_check(&self) -> Result<(), String> {
        #[cfg(target_os = "windows")]
        {
            Ok(())
        }
        #[cfg(not(target_os = "windows"))]
        {
            Err("SAPI is only available on Windows".to_string())
        }
    }
}

fn pcm_to_wav(pcm_data: &[u8], sample_rate: u32, channels: u16, bits_per_sample: u16) -> Vec<u8> {
    let mut header = Vec::with_capacity(44 + pcm_data.len());
    header.extend_from_slice(b"RIFF");
    let file_size = (36 + pcm_data.len()) as u32;
    header.extend_from_slice(&file_size.to_le_bytes());
    header.extend_from_slice(b"WAVE");
    header.extend_from_slice(b"fmt ");
    header.extend_from_slice(&16u32.to_le_bytes());
    header.extend_from_slice(&1u16.to_le_bytes());
    header.extend_from_slice(&channels.to_le_bytes());
    header.extend_from_slice(&sample_rate.to_le_bytes());
    let byte_rate = sample_rate * (channels as u32) * (bits_per_sample as u32) / 8;
    header.extend_from_slice(&byte_rate.to_le_bytes());
    let block_align = channels * bits_per_sample / 8;
    header.extend_from_slice(&block_align.to_le_bytes());
    header.extend_from_slice(&bits_per_sample.to_le_bytes());
    header.extend_from_slice(b"data");
    let data_size = pcm_data.len() as u32;
    header.extend_from_slice(&data_size.to_le_bytes());
    header.extend_from_slice(pcm_data);
    header
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_voices() {
        let voices = SapiBackend::list_voices();
        assert!(!voices.is_empty());
        assert_eq!(voices[0].id, "sapi_default");
    }

    #[test]
    fn test_sapi_synthesize() {
        let backend = SapiBackend::new("sapi_default".to_string(), 1.0);
        let result = backend.synthesize("Hello", "sapi_default", 1.0);
        #[cfg(target_os = "windows")]
        {
            if let Err(ref e) = result {
                println!("SAPI Synthesis failed: {}", e);
            }
            assert!(result.is_ok());
            let wav_bytes = result.unwrap();
            println!("Synthesized WAV bytes size: {}", wav_bytes.len());
            let non_zero_count = wav_bytes.iter().filter(|&&b| b != 0).count();
            println!("Non-zero bytes count: {}", non_zero_count);
            if let Some(first_nonzero) = wav_bytes.iter().position(|&b| b != 0) {
                println!("First non-zero byte index: {}", first_nonzero);
                let end_idx = (first_nonzero + 50).min(wav_bytes.len());
                println!("Bytes around first non-zero: {:?}", &wav_bytes[first_nonzero..end_idx]);
            }
            assert!(!wav_bytes.is_empty());
            // A valid WAV file starts with the RIFF header "RIFF"
            assert_eq!(&wav_bytes[0..4], b"RIFF");
        }
        #[cfg(not(target_os = "windows"))]
        {
            assert!(result.is_err());
        }
    }
}
