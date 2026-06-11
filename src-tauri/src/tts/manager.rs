//! TTS manager: orchestrates synthesis + streaming playback.
//!
//! Paginates text, synthesizes each fragment on a worker thread, and appends
//! audio to the [`TtsPlayer`] as it becomes ready — so fragment *i+1* is
//! synthesized while *i* is still playing. A monotonic generation counter makes
//! `stop()` (and any new `speak`) abort in-flight workers promptly.

use crate::settings::{get_settings, TtsConfig, TtsEngine};
use crate::tts::backends::piper::{self, PiperBackend};
use crate::tts::pagination::paginate_text;
use crate::tts::player::TtsPlayer;
use crate::tts::sanitize::sanitize_text;
use crate::tts::{TtsBackend, Voice};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};

pub struct TtsManager {
    app: AppHandle,
    player: TtsPlayer,
    /// Bumped on every `speak`/`stop`; stale workers observe the change and abort.
    generation: Arc<AtomicU64>,
}

impl TtsManager {
    pub fn new(app: AppHandle) -> Self {
        let player = TtsPlayer::new(app.clone());
        Self {
            app,
            player,
            generation: Arc::new(AtomicU64::new(0)),
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
            TtsEngine::Openai => Ok(Box::new(crate::tts::backends::openai::OpenAiTtsBackend::new(
                cfg.openai.clone(),
            ))),
            TtsEngine::Elevenlabs => Ok(Box::new(crate::tts::backends::elevenlabs::ElevenLabsTtsBackend::new(
                cfg.elevenlabs.clone(),
            ))),
            TtsEngine::Cartesia => Ok(Box::new(crate::tts::backends::cartesia::CartesiaTtsBackend::new(
                cfg.cartesia.clone(),
            ))),
        }
    }

    /// Enumerate available voices for the configured engine.
    pub fn list_voices(&self) -> Vec<Voice> {
        self.list_voices_for_engine(None)
    }

    /// Enumerate available voices for a specific engine, or defaults to the configured engine.
    pub fn list_voices_for_engine(&self, engine: Option<TtsEngine>) -> Vec<Voice> {
        let cfg = get_settings(&self.app).tts;
        let engine = engine.unwrap_or(cfg.engine);
        match engine {
            TtsEngine::Piper => piper::list_voices(&self.app),
            TtsEngine::Openai => {
                vec![
                    Voice { id: "alloy".to_string(), name: "Alloy".to_string(), language: Some("en".to_string()) },
                    Voice { id: "echo".to_string(), name: "Echo".to_string(), language: Some("en".to_string()) },
                    Voice { id: "fable".to_string(), name: "Fable".to_string(), language: Some("en".to_string()) },
                    Voice { id: "onyx".to_string(), name: "Onyx".to_string(), language: Some("en".to_string()) },
                    Voice { id: "nova".to_string(), name: "Nova".to_string(), language: Some("en".to_string()) },
                    Voice { id: "shimmer".to_string(), name: "Shimmer".to_string(), language: Some("en".to_string()) },
                ]
            }
            TtsEngine::Elevenlabs => {
                let backend = crate::tts::backends::elevenlabs::ElevenLabsTtsBackend::new(cfg.elevenlabs.clone());
                match backend.list_voices() {
                    Ok(voices) => voices
                        .into_iter()
                        .map(|v| Voice {
                            id: v.voice_id,
                            name: v.name.unwrap_or_else(|| "Unnamed".to_string()),
                            language: v.labels.as_ref()
                                .and_then(|l| l.get("language").and_then(|lang| lang.as_str().map(|s| s.to_string()))),
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
                    Voice { id: "f786b574-daa5-4673-aa0c-cbe3e8534c02".to_string(), name: "Katie".to_string(), language: Some("en".to_string()) },
                    Voice { id: "a5136bf9-224c-4d76-b823-52bd5efcffcc".to_string(), name: "Jameson (Deep Male)".to_string(), language: Some("en".to_string()) },
                    Voice { id: "25a0312d-7437-4b70-9f1e-f3f2d2b512e0".to_string(), name: "Barack Obama".to_string(), language: Some("en".to_string()) },
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
        self.player.stop();
        self.player.set_volume(cfg.volume);

        let sanitized = sanitize_text(&text, &cfg.sanitization);
        if sanitized.trim().is_empty() {
            log::debug!("[TTS] nothing left to speak after sanitization");
            return;
        }
        let fragments = paginate_text(&sanitized, &cfg.pagination);
        let app = self.app.clone();
        let player = self.player.clone();
        let gen_counter = self.generation.clone();
        let voice = cfg.voice.clone();
        let speed = cfg.speed;
        let engine_name = format!("{:?}", cfg.engine).to_lowercase();

        std::thread::spawn(move || {
            let total = fragments.len();
            let _ = app.emit("tts:started", total);
            let mut all_chunks = Vec::new();
            
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
                        all_chunks.push(bytes.clone());
                        player.append(bytes);
                    }
                    Err(e) => {
                        log::error!("[TTS] synthesis failed: {e}");
                        let _ = app.emit("tts:error", e);
                    }
                }
            }
            let _ = app.emit("tts:synth-done", ());

            // Save TTS entry to history with cached audio file
            if !all_chunks.is_empty() {
                if let Some(hm) = app.try_state::<std::sync::Arc<crate::managers::history::HistoryManager>>() {
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
                            None,
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
        let backend = match self.build_backend(&cfg) {
            Ok(b) => b,
            Err(e) => {
                log::error!("[TTS] {e}");
                let _ = self.app.emit("tts:error", e);
                return;
            }
        };
        let generation = self.generation.load(Ordering::SeqCst);
        self.player.set_volume(cfg.volume);
        let app = self.app.clone();
        let player = self.player.clone();
        let gen_counter = self.generation.clone();
        let voice = cfg.voice.clone();
        let speed = cfg.speed;
        std::thread::spawn(move || {
            if gen_counter.load(Ordering::SeqCst) != generation {
                return;
            }
            match backend.synthesize(&sentence, &voice, speed) {
                Ok(bytes) => {
                    if gen_counter.load(Ordering::SeqCst) == generation {
                        player.append(bytes);
                    }
                }
                Err(e) => {
                    log::error!("[TTS] sentence synthesis failed: {e}");
                    let _ = app.emit("tts:error", e);
                }
            }
        });
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

    /// Play raw audio bytes directly through the player.
    pub fn play_raw(&self, bytes: Vec<u8>) {
        let cfg = get_settings(&self.app).tts;
        self.player.set_volume(cfg.volume);
        self.player.append(bytes);
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
