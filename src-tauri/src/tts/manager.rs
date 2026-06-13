//! TTS manager: orchestrates synthesis + streaming playback.
//!
//! Paginates text, synthesizes each fragment on a worker thread, and appends
//! audio to the [`TtsPlayer`] as it becomes ready — so fragment *i+1* is
//! synthesized while *i* is still playing. A monotonic generation counter makes
//! `stop()` (and any new `speak`) abort in-flight workers promptly.

use crate::audio_toolkit::extract_envelope;
use crate::settings::{get_settings, TtsConfig, TtsEngine};
use crate::tts::backends::kitten::KittenBackend;
use crate::tts::backends::kokoro::KokoroBackend;
use crate::tts::backends::piper::{self, PiperBackend};
use crate::tts::backends::pocket::PocketBackend;
use crate::tts::backends::sapi::SapiBackend;
use crate::tts::pagination::paginate_text;
use crate::tts::player::TtsPlayer;
use crate::tts::sanitize::sanitize_text;
use crate::tts::{TtsBackend, Voice};
use std::sync::mpsc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager};

pub struct TtsManager {
    app: AppHandle,
    player: TtsPlayer,
    /// Bumped on every `speak`/`stop`; stale workers observe the change and abort.
    generation: Arc<AtomicU64>,
    /// Sentence queue sender for ordered FIFO synthesis
    sentence_tx: Mutex<Option<mpsc::Sender<(String, u64)>>>,
}

impl TtsManager {
    pub fn new(app: AppHandle) -> Self {
        let player = TtsPlayer::new(app.clone());
        Self {
            app,
            player,
            generation: Arc::new(AtomicU64::new(0)),
            sentence_tx: Mutex::new(None),
        }
    }

    fn build_backend(&self, cfg: &TtsConfig) -> Result<Box<dyn TtsBackend>, String> {
        self.build_backend_with_noise(cfg, 0.667, 0.8)
    }

    fn build_backend_with_noise(
        &self,
        cfg: &TtsConfig,
        noise_scale: f32,
        noise_w_scale: f32,
    ) -> Result<Box<dyn TtsBackend>, String> {
        match cfg.engine {
            TtsEngine::Piper => Ok(Box::new(
                PiperBackend::new(self.app.clone(), cfg.piper.cuda)
                    .with_noise(noise_scale, noise_w_scale),
            )),
            TtsEngine::Kokoro => Ok(Box::new(KokoroBackend::new(cfg.voice.clone(), cfg.speed))),
            TtsEngine::Kitten => Ok(Box::new(KittenBackend::new(cfg.voice.clone(), cfg.speed))),
            TtsEngine::Pocket => Ok(Box::new(PocketBackend::new(cfg.voice.clone(), cfg.speed))),
            TtsEngine::Sapi => Ok(Box::new(SapiBackend::new(cfg.voice.clone(), cfg.speed))),
            TtsEngine::Openai => Ok(Box::new(
                crate::tts::backends::openai::OpenAiTtsBackend::new(cfg.openai.clone()),
            )),
            TtsEngine::Elevenlabs => Ok(Box::new(
                crate::tts::backends::elevenlabs::ElevenLabsTtsBackend::new(cfg.elevenlabs.clone()),
            )),
            TtsEngine::Cartesia => Ok(Box::new(
                crate::tts::backends::cartesia::CartesiaTtsBackend::new(cfg.cartesia.clone()),
            )),
        }
    }

    /// Enumerate available voices for a specific engine, or defaults to the configured engine.
    pub fn list_voices_for_engine(&self, engine: Option<TtsEngine>) -> Vec<Voice> {
        let cfg = get_settings(&self.app).tts;
        let engine = engine.unwrap_or(cfg.engine);
        match engine {
            TtsEngine::Piper => piper::list_voices(&self.app),
            TtsEngine::Kokoro => KokoroBackend::list_voices(),
            TtsEngine::Kitten => KittenBackend::list_voices(),
            TtsEngine::Pocket => PocketBackend::list_voices(),
            TtsEngine::Sapi => SapiBackend::list_voices(),
            TtsEngine::Openai => {
                vec![
                    Voice {
                        id: "alloy".to_string(),
                        name: "Alloy".to_string(),
                        language: Some("en".to_string()),
                    },
                    Voice {
                        id: "echo".to_string(),
                        name: "Echo".to_string(),
                        language: Some("en".to_string()),
                    },
                    Voice {
                        id: "fable".to_string(),
                        name: "Fable".to_string(),
                        language: Some("en".to_string()),
                    },
                    Voice {
                        id: "onyx".to_string(),
                        name: "Onyx".to_string(),
                        language: Some("en".to_string()),
                    },
                    Voice {
                        id: "nova".to_string(),
                        name: "Nova".to_string(),
                        language: Some("en".to_string()),
                    },
                    Voice {
                        id: "shimmer".to_string(),
                        name: "Shimmer".to_string(),
                        language: Some("en".to_string()),
                    },
                ]
            }
            TtsEngine::Elevenlabs => {
                let backend = crate::tts::backends::elevenlabs::ElevenLabsTtsBackend::new(
                    cfg.elevenlabs.clone(),
                );
                match backend.list_voices() {
                    Ok(voices) => voices
                        .into_iter()
                        .map(|v| Voice {
                            id: v.voice_id,
                            name: v.name.unwrap_or_else(|| "Unnamed".to_string()),
                            language: v.labels.as_ref().and_then(|l| {
                                l.get("language")
                                    .and_then(|lang| lang.as_str().map(|s| s.to_string()))
                            }),
                        })
                        .collect(),
                    Err(e) => {
                        log::error!("Failed to list ElevenLabs voices: {e}");
                        Vec::new()
                    }
                }
            }
            TtsEngine::Cartesia => {
                vec![
                    Voice {
                        id: "f786b574-daa5-4673-aa0c-cbe3e8534c02".to_string(),
                        name: "Katie".to_string(),
                        language: Some("en".to_string()),
                    },
                    Voice {
                        id: "a5136bf9-224c-4d76-b823-52bd5efcffcc".to_string(),
                        name: "Jameson (Deep Male)".to_string(),
                        language: Some("en".to_string()),
                    },
                    Voice {
                        id: "25a0312d-7437-4b70-9f1e-f3f2d2b512e0".to_string(),
                        name: "Barack Obama".to_string(),
                        language: Some("en".to_string()),
                    },
                ]
            }
        }
    }

    pub fn set_volume(&self, volume: u8) {
        self.player.set_volume(volume);
    }

    pub fn pause(&self) {
        self.player.pause();
    }

    pub fn resume(&self) {
        self.player.resume();
    }

    pub fn is_playing(&self) -> bool {
        self.player.is_playing()
    }

    /// Stop playback and abort any in-flight synthesis.
    pub fn stop(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        *self.sentence_tx.lock().unwrap() = None; // drop old channel, kills consumer thread
        self.player.stop();
        let _ = self.app.emit("tts:stopped", ());
    }

    /// Speak arbitrary text aloud (paginated, streaming).
    pub fn speak(&self, text: String) {
        let text = text.trim().to_string();
        if text.is_empty() {
            return;
        }
        let cfg = get_settings(&self.app).tts;
        let backend = match self.build_backend(&cfg) {
            Ok(b) => b,
            Err(e) => {
                log::error!("[TTS] {e}");
                let _ = self.app.emit("tts:error", e);
                return;
            }
        };

        // New generation; stop anything currently playing.
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        *self.sentence_tx.lock().unwrap() = None; // drop old channel
        self.player.stop();
        self.player.set_volume(cfg.volume);

        let sanitized = sanitize_text(&text, &cfg.sanitization);
        if sanitized.trim().is_empty() {
            log::debug!("[TTS] nothing left to speak after sanitization");
            return;
        }
        let shorten_first = cfg.tts_shorten_first_chunk;
        let fragments = if shorten_first {
            // 3-fragment streaming pattern for fast TTFA:
            //   1st sentence split at first period/!-/? → play immediately
            //   2nd sentence split at next period → synthesized while 1st plays
            //   3rd fragment: rest of text in one go → synthesized while 2nd plays
            let mut frags = Vec::new();
            let mut remaining = sanitized.as_str();

            // Helper: find first sentence boundary (., !, ?, \n)
            let find_sentence_end = |t: &str| -> Option<usize> {
                t.char_indices()
                    .find(|(_, c)| matches!(c, '.' | '!' | '?' | '\n'))
                    .map(|(idx, c)| idx + c.len_utf8())
            };

            // Fragment 1: up to first sentence-ending punctuation
            if let Some(split) = find_sentence_end(remaining) {
                let first = remaining[..split].trim().to_string();
                if !first.is_empty() {
                    frags.push(crate::tts::pagination::TextFragment {
                        text: first,
                        index: 0,
                        total: 0,
                    });
                }
                remaining = remaining[split..].trim();
            }

            // Fragment 2: next sentence
            if !remaining.is_empty() {
                if let Some(split) = find_sentence_end(remaining) {
                    let second = remaining[..split].trim().to_string();
                    if !second.is_empty() {
                        frags.push(crate::tts::pagination::TextFragment {
                            text: second,
                            index: frags.len(),
                            total: 0,
                        });
                    }
                    remaining = remaining[split..].trim();
                }
            }

            // Fragment 3: rest in one go
            if !remaining.is_empty() {
                frags.push(crate::tts::pagination::TextFragment {
                    text: remaining.to_string(),
                    index: frags.len(),
                    total: 0,
                });
            }

            // Fix total
            let total = frags.len();
            for f in &mut frags {
                f.total = total;
            }
            frags
        } else {
            paginate_text(&sanitized, &cfg.pagination)
        };
        let app = self.app.clone();
        let player = self.player.clone();
        let gen_counter = self.generation.clone();
        let voice = cfg.voice.clone();
        let speed = cfg.speed;
        let engine_name = format!("{:?}", cfg.engine).to_lowercase();

        std::thread::spawn(move || {
            let total = fragments.len();
            let synth_start = std::time::Instant::now();
            let _ = app.emit("tts:started", total);
            let mut all_chunks = Vec::new();
            let mut first_audio_emitted = false;

            for frag in fragments {
                if gen_counter.load(Ordering::SeqCst) != generation {
                    log::debug!("[TTS] speak aborted (superseded)");
                    return;
                }
                match backend.synthesize(&frag.text, &voice, speed) {
                    Ok(bytes) => {
                        if gen_counter.load(Ordering::SeqCst) != generation {
                            return;
                        }
                        let _ = app.emit(
                            "tts:fragment",
                            serde_json::json!({ "index": frag.index, "total": frag.total }),
                        );
                        // Emit waveform envelope for HUD visualization
                        if let Some(envelope) = extract_envelope(&bytes, 32) {
                            let _ = app.emit(
                                "tts:waveform",
                                serde_json::json!({
                                    "fragment_index": frag.index,
                                    "values": envelope.values,
                                    "duration_ms": envelope.duration_ms,
                                }),
                            );
                        }
                        all_chunks.push(bytes.clone());
                        player.append(bytes);
                        if !first_audio_emitted {
                            first_audio_emitted = true;
                            let ttfa_ms = synth_start.elapsed().as_millis() as u64;
                            let _ = app.emit("tts:first-audio", serde_json::json!({ "ms": ttfa_ms }));
                        }
                    }
                    Err(e) => {
                        log::error!("[TTS] synthesis failed: {e}");
                        let _ = app.emit("tts:error", e);
                    }
                }
            }
            let synth_total_ms = synth_start.elapsed().as_millis() as u64;
            let _ = app.emit("tts:synth-done", serde_json::json!({ "ms": synth_total_ms }));

            // Save TTS entry to history with cached audio file
            if !all_chunks.is_empty() {
                if let Some(hm) =
                    app.try_state::<std::sync::Arc<crate::managers::history::HistoryManager>>()
                {
                    let is_wav = all_chunks[0].len() >= 4 && &all_chunks[0][0..4] == b"RIFF";
                    let combined_bytes = if is_wav {
                        concatenate_wavs(&all_chunks)
                    } else {
                        let mut direct = Vec::new();
                        for c in &all_chunks {
                            direct.extend_from_slice(c);
                        }
                        direct
                    };

                    let ext = if is_wav { "wav" } else { "mp3" };
                    let file_name = format!("tts-{}.{}", chrono::Utc::now().timestamp(), ext);
                    let cache_path = hm.recordings_dir().join(&file_name);

                    if let Err(e) = std::fs::write(&cache_path, &combined_bytes) {
                        log::error!("[TTS] failed to write cached TTS audio: {e}");
                    } else {
                        let _ = hm.save_entry(
                            file_name,
                            text,
                            false,
                            None,
                            None,
                            "tts".to_string(),
                            Some(engine_name),
                            None,
                            Some(synth_total_ms as i64),
                        );
                    }
                }
            }
        });
    }

    /// Speak a single already-segmented sentence as part of an ongoing session
    /// (used by the Brain → TTS bridge). Returns immediately; synthesis happens
    /// on a worker and the result is appended to the active playback queue.
    pub fn speak_sentence(&self, sentence: String) {
        let cfg = get_settings(&self.app).tts;
        let sentence = sanitize_text(sentence.trim(), &cfg.sanitization);
        if sentence.trim().is_empty() {
            return;
        }
        let generation = self.generation.load(Ordering::SeqCst);
        self.player.set_volume(cfg.volume);

        // Create sentence queue consumer thread if not already running
        let mut tx_guard = self.sentence_tx.lock().unwrap();
        if tx_guard.is_none() {
            let (tx, rx) = mpsc::channel::<(String, u64)>();
            let backend = match self.build_backend(&cfg) {
                Ok(b) => b,
                Err(e) => {
                    log::error!("[TTS] {e}");
                    let _ = self.app.emit("tts:error", e);
                    return;
                }
            };
            let voice = cfg.voice.clone();
            let speed = cfg.speed;
            let player = self.player.clone();
            let gen_counter = self.generation.clone();
            let app = self.app.clone();
            std::thread::spawn(move || {
                while let Ok((text, gen)) = rx.recv() {
                    if gen_counter.load(Ordering::SeqCst) != gen {
                        continue;
                    }
                    let synth_start = std::time::Instant::now();
                    match backend.synthesize(&text, &voice, speed) {
                        Ok(bytes) => {
                            if gen_counter.load(Ordering::SeqCst) == gen {
                                player.append(bytes);
                                let synth_ms = synth_start.elapsed().as_millis() as u64;
                                let _ = app.emit("tts:synth-done", serde_json::json!({ "ms": synth_ms }));
                                let _ = app.emit("tts:playing-changed", true);
                            }
                        }
                        Err(e) => {
                            log::error!("[TTS] sentence synthesis failed: {e}");
                            let _ = app.emit("tts:error", e);
                        }
                    }
                }
            });
            *tx_guard = Some(tx);
        }
        let tx = tx_guard.as_ref().unwrap();
        let _ = tx.send((sentence, generation));
    }

    /// Begin a fresh TTS session for streamed sentences (e.g. a new Brain turn).
    pub fn begin_session(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.player.stop();
    }

    /// Play the customized startup greeting message.
    pub fn play_greeting(&self) {
        let settings = get_settings(&self.app);
        let greeting_cfg = settings.tts.greeting.clone();
        if greeting_cfg.text.trim().is_empty() {
            return;
        }

        // Build temporary TtsConfig to build the backend
        let mut temp_tts_cfg = settings.tts.clone();
        temp_tts_cfg.engine = greeting_cfg.engine;
        temp_tts_cfg.voice = greeting_cfg.voice.clone();
        temp_tts_cfg.speed = greeting_cfg.speed;

        let backend = match self.build_backend_with_noise(
            &temp_tts_cfg,
            greeting_cfg.noise_scale,
            greeting_cfg.noise_w_scale,
        ) {
            Ok(b) => b,
            Err(e) => {
                log::error!("[Greeting] Failed to build backend for greeting: {e}");
                return;
            }
        };

        let player = self.player.clone();
        let text = greeting_cfg.text.clone();
        let voice = greeting_cfg.voice.clone();
        let speed = greeting_cfg.speed;
        let volume = settings.tts.volume;

        std::thread::spawn(move || {
            log::info!("[Greeting] Synthesizing custom greeting...");
            player.set_volume(volume);
            match backend.synthesize(&text, &voice, speed) {
                Ok(bytes) => {
                    log::info!("[Greeting] Playing custom greeting audio out loud...");
                    player.append(bytes);
                }
                Err(e) => {
                    log::error!("[Greeting] Synthesis failed for custom greeting: {e}");
                }
            }
        });
    }
}

fn concatenate_wavs(chunks: &[Vec<u8>]) -> Vec<u8> {
    if chunks.is_empty() {
        return Vec::new();
    }
    if chunks.len() == 1 {
        return chunks[0].clone();
    }

    // Find the first valid WAV chunk to serve as our base
    let mut base_chunk = None;
    let mut start_idx = 0;
    for (i, chunk) in chunks.iter().enumerate() {
        if chunk.len() >= 44 && &chunk[0..4] == b"RIFF" {
            base_chunk = Some(chunk.clone());
            start_idx = i + 1;
            break;
        }
    }

    let mut base = match base_chunk {
        Some(b) => b,
        None => {
            // None of the chunks are WAV, fallback to direct concatenation
            let mut all = Vec::new();
            for c in chunks {
                all.extend_from_slice(c);
            }
            return all;
        }
    };

    let mut total_data_bytes = if base.len() >= 44 {
        u32::from_le_bytes(base[40..44].try_into().unwrap_or([0; 4])) as usize
    } else {
        base.len().saturating_sub(44)
    };

    for chunk in &chunks[start_idx..] {
        if chunk.len() >= 44 && &chunk[0..4] == b"RIFF" {
            let data_part = &chunk[44..];
            base.extend_from_slice(data_part);
            total_data_bytes += data_part.len();
        } else {
            base.extend_from_slice(chunk);
            total_data_bytes += chunk.len();
        }
    }

    if base.len() >= 8 {
        let riff_size = (base.len() as u32).saturating_sub(8);
        base[4..8].copy_from_slice(&riff_size.to_le_bytes());
    }

    if base.len() >= 44 {
        let data_size = total_data_bytes as u32;
        base[40..44].copy_from_slice(&data_size.to_le_bytes());
    }

    base
}
