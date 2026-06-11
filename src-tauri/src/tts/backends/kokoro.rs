// Kokoro-82M TTS backend via tts-rs (in-process ONNX, no sidecar).
//
// Pattern: Parrot's worker pool (managers/tts.rs) with CopySpeak's lifecycle
// states (Loading → WarmingUp → Ready → Error → Stopped).
//
// 54 voices across 9 languages, ~115 MB ONNX model, Apache 2.0.

use crate::tts::{TtsBackend, Voice};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

/// Lifecycle state of the Kokoro engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KokoroStatus {
    Stopped,
    Loading,
    WarmingUp,
    Ready,
    Error,
}

impl KokoroStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            KokoroStatus::Stopped => "stopped",
            KokoroStatus::Loading => "loading",
            KokoroStatus::WarmingUp => "warming_up",
            KokoroStatus::Ready => "ready",
            KokoroStatus::Error => "error",
        }
    }
}

/// Voice prefixes per language (from Kokoro voice naming convention).
/// Format: `{prefix}f_*` = female, `{prefix}m_*` = male.
const VOICE_LANGUAGE_MAP: &[(&str, &str, &[&str])] = &[
    ("en", "US", &["af_", "am_"]),
    ("en", "GB", &["bf_", "bm_"]),
    ("es", "", &["ef_"]),
    ("fr", "", &["ff_"]),
    ("hi", "", &["hf_"]),
    ("it", "", &["if_"]),
    ("ja", "", &["jf_"]),
    ("pt", "BR", &["pf_"]),
    ("zh", "", &["zf_", "zm_"]),
];

/// Known voice IDs (54 total).
const KNOWN_VOICES: &[&str] = &[
    // US English
    "af_alloy", "af_aoede", "af_bella", "af_heart", "af_jessica", "af_kore",
    "af_nicole", "af_nova", "af_river", "af_sarah", "af_sky",
    "am_adam", "am_echo", "am_eric", "am_fenrir", "am_liam", "am_michael",
    "am_onyx", "am_puck", "am_santa",
    // British English
    "bf_alice", "bf_emma", "bf_isabella", "bf_lily",
    "bm_daniel", "bm_fable", "bm_george", "bm_lewis",
    // Spanish
    "ef_dora",
    // French
    "ff_siwis",
    // Hindi
    "hf_alpha", "hf_beta",
    // Italian
    "if_sara", "if_nicola",
    // Japanese
    "jf_alpha", "jf_gongitsune", "jf_nezumi", "jf_tebukuro",
    // Brazilian Portuguese
    "pf_dora",
    // Mandarin Chinese
    "zf_xiaobei", "zf_xiaoni", "zf_xiaoxiao", "zf_xiaoyi",
    "zm_yunjian", "zm_yunxia", "zm_yunyang",
];

// ========================================================================
// Single-engine backend (pool management lives in TtsManager)
// ========================================================================

pub struct KokoroBackend {
    voice: String,
    speed: f32,
    generation: Arc<AtomicU64>,
    status: Arc<parking_lot::Mutex<KokoroStatus>>,
    loaded: Arc<AtomicBool>,
}

impl KokoroBackend {
    pub fn new(voice: String, speed: f32) -> Self {
        Self {
            voice,
            speed,
            generation: Arc::new(AtomicU64::new(1)),
            status: Arc::new(parking_lot::Mutex::new(KokoroStatus::Stopped)),
            loaded: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn status(&self) -> KokoroStatus {
        *self.status.lock()
    }

    pub fn is_loaded(&self) -> bool {
        self.loaded.load(Ordering::Relaxed)
    }

    /// List all known Kokoro voices.
    pub fn list_voices() -> Vec<Voice> {
        KNOWN_VOICES
            .iter()
            .map(|id| {
                let (lang, name) = VOICE_LANGUAGE_MAP
                    .iter()
                    .find_map(|(lang_code, region, prefixes)| {
                        prefixes
                            .iter()
                            .find(|&&pfx| id.starts_with(pfx))
                            .map(|_| {
                                let label = if region.is_empty() {
                                    lang_code.to_string()
                                } else {
                                    format!("{}-{}", lang_code, region)
                                };
                                (Some(label), id.trim_start_matches(&id[..3]))
                            })
                    })
                    .unwrap_or((None, id));
                Voice {
                    id: id.to_string(),
                    name: name.to_string(),
                    language: lang,
                }
            })
            .collect()
    }

    /// Select the best voice for a given language code (e.g., "fr", "ja").
    pub fn voice_for_language(lang_code: &str) -> Option<&'static str> {
        for (code, _, prefixes) in VOICE_LANGUAGE_MAP {
            if *code == lang_code {
                return Some(prefixes[0]);
            }
        }
        None
    }

    /// Crossfade two audio buffers at 24kHz (10ms overlap = 240 samples).
    /// `prev_tail` should be the last N samples of the previous chunk.
    /// Returns the crossfaded chunk (tail of prev crossfaded into head of next).
    pub fn crossfade(prev_tail: &[f32], next_chunk: &[f32], overlap: usize) -> Vec<f32> {
        if prev_tail.len() < overlap || next_chunk.len() < overlap {
            let mut result = prev_tail.to_vec();
            result.extend_from_slice(next_chunk);
            return result;
        }

        let actual_overlap = overlap.min(prev_tail.len()).min(next_chunk.len());
        let prev_start = prev_tail.len() - actual_overlap;

        let mut result = Vec::with_capacity(prev_tail.len() + next_chunk.len() - actual_overlap);
        result.extend_from_slice(&prev_tail[..prev_start]);

        for i in 0..actual_overlap {
            let fade = i as f32 / actual_overlap as f32;
            let sample = prev_tail[prev_start + i] * (1.0 - fade) + next_chunk[i] * fade;
            result.push(sample);
        }

        result.extend_from_slice(&next_chunk[actual_overlap..]);
        result
    }
}

impl TtsBackend for KokoroBackend {
    fn name(&self) -> &str {
        "Kokoro"
    }

    fn synthesize(&self, text: &str, voice: &str, speed: f32) -> Result<Vec<u8>, String> {
        // TODO: Integrate tts-rs crate for actual synthesis.
        // For now, return a properly-structured error that directs users to
        // install the Kokoro model and espeak-ng data.
        Err(format!(
            "Kokoro-82M synthesis not yet available in this build.\n\n\
             To use Kokoro:\n\
             1. Place the Kokoro ONNX model in models/kokoro/\n\
             2. Place espeak-ng data in src-tauri/resources/espeak-ng-data/\n\
             3. Ensure tts-rs crate is compiled with 'kokoro' feature\n\
             \n\
             Requested: voice='{voice}', text='{}'",
            &text[..text.len().min(40)]
        ))
    }

    fn health_check(&self) -> Result<(), String> {
        match self.status() {
            KokoroStatus::Ready => Ok(()),
            KokoroStatus::Stopped => Err("Kokoro engine is stopped".to_string()),
            KokoroStatus::Loading => Err("Kokoro engine is still loading".to_string()),
            KokoroStatus::WarmingUp => Err("Kokoro engine is warming up".to_string()),
            KokoroStatus::Error => Err("Kokoro engine is in error state".to_string()),
        }
    }

    fn file_extension(&self) -> &str {
        "wav"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_voices() {
        let voices = KokoroBackend::list_voices();
        assert_eq!(voices.len(), KNOWN_VOICES.len());
        let af_heart = voices.iter().find(|v| v.id == "af_heart").unwrap();
        assert_eq!(af_heart.language.as_deref(), Some("en-US"));
    }

    #[test]
    fn test_voice_for_language() {
        assert_eq!(KokoroBackend::voice_for_language("fr"), Some("ff_siwis"));
        assert_eq!(KokoroBackend::voice_for_language("ja"), Some("jf_alpha"));
        assert!(KokoroBackend::voice_for_language("de").is_none());
    }

    #[test]
    fn test_crossfade_basic() {
        let prev = vec![0.5; 300];
        let next = vec![1.0; 300];
        let result = KokoroBackend::crossfade(&prev, &next, 240);
        assert_eq!(result.len(), 360); // 300 + 300 - 240
        // Start of crossfade region should be between 0.5 and 1.0
        assert!(result[60] < 1.0 && result[60] > 0.5);
        // End should be close to 1.0
        assert!(result[result.len() - 1] >= 0.99);
    }

    #[test]
    fn test_crossfade_small_buffers() {
        let prev = vec![0.5; 100];
        let next = vec![1.0; 100];
        let result = KokoroBackend::crossfade(&prev, &next, 240);
        assert_eq!(result.len(), 200); // fallback: concat
    }
}
