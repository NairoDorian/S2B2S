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
use crate::tts::backends::qwen3::Qwen3Backend;
use crate::tts::backends::sapi::SapiBackend;
use crate::tts::pagination::paginate_text;
use crate::tts::player::TtsPlayer;
use crate::tts::sanitize::sanitize_text;
use crate::tts::{TtsBackend, Voice};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
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
            TtsEngine::Pocket => Ok(Box::new(PocketBackend::new(
                self.app.clone(),
                cfg.voice.clone(),
                cfg.speed,
            ))),
            TtsEngine::Qwen3 => Ok(Box::new(Qwen3Backend::new(
                self.app.clone(),
                cfg.voice.clone(),
                cfg.speed,
            ))),
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
            TtsEngine::Pocket => PocketBackend::list_voices(&self.app),
            TtsEngine::Qwen3 => Qwen3Backend::list_voices(&self.app),
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
            //   Fallback: if no punctuation found, split at 12-word boundary
            let mut frags = Vec::new();
            let mut remaining = sanitized.as_str();

            // Helper: find sentence boundary (., !, ?, \n) or word-count fallback
            let find_sentence_end = |t: &str| -> Option<usize> {
                // First try punctuation
                if let Some(idx) = t
                    .char_indices()
                    .find(|(_, c)| matches!(c, '.' | '!' | '?' | '\n'))
                    .map(|(idx, c)| idx + c.len_utf8())
                {
                    return Some(idx);
                }
                // Fallback: find 12th word boundary
                let mut word_count = 0;
                let mut last_space = None;
                for (idx, c) in t.char_indices() {
                    if c == ' ' || c == '\t' {
                        if word_count > 0 {
                            last_space = Some(idx);
                        }
                    } else if last_space.map_or(true, |s| idx > s) {
                        if word_count == 0
                            || t[..idx]
                                .chars()
                                .last()
                                .map_or(true, |prev| prev.is_whitespace())
                        {
                            word_count += 1;
                            if word_count >= 12 {
                                return Some(idx + c.len_utf8());
                            }
                        }
                    }
                }
                None
            };

            // Fragment 1: up to first sentence-ending punctuation or 12-word boundary
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

            // Fragment 2: next sentence or next 12-word boundary
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
            let mut pagination_cfg = cfg.pagination.clone();
            if let Some(telemetry) = self
                .app
                .try_state::<Arc<crate::tts::telemetry::Telemetry>>()
            {
                let current_engine_name = format!("{:?}", cfg.engine).to_lowercase();
                let key = format!("{}:{}", current_engine_name, cfg.voice);
                let adaptive_size =
                    telemetry.adaptive_fragment_size(&key, pagination_cfg.fragment_size as usize);
                pagination_cfg.fragment_size = adaptive_size as u32;
            }
            paginate_text(&sanitized, &pagination_cfg)
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
                let frag_synth_start = std::time::Instant::now();
                match backend.synthesize(&frag.text, &voice, speed) {
                    Ok(bytes) => {
                        if gen_counter.load(Ordering::SeqCst) != generation {
                            return;
                        }
                        let frag_synth_ms = frag_synth_start.elapsed().as_millis() as u64;
                        if let Some(telemetry) =
                            app.try_state::<Arc<crate::tts::telemetry::Telemetry>>()
                        {
                            let key = format!("{}:{}", engine_name, voice);
                            telemetry.record(&key, frag.text.len(), frag_synth_ms);
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
                            let _ =
                                app.emit("tts:first-audio", serde_json::json!({ "ms": ttfa_ms }));
                        }
                    }
                    Err(e) => {
                        log::error!("[TTS] synthesis failed: {e}");
                        let _ = app.emit("tts:error", e);
                    }
                }
            }
            let synth_total_ms = synth_start.elapsed().as_millis() as u64;
            let _ = app.emit(
                "tts:synth-done",
                serde_json::json!({ "ms": synth_total_ms }),
            );

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
            let engine_name = format!("{:?}", cfg.engine).to_lowercase();
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
                                if let Some(telemetry) =
                                    app.try_state::<Arc<crate::tts::telemetry::Telemetry>>()
                                {
                                    let key = format!("{}:{}", engine_name, voice);
                                    telemetry.record(&key, text.len(), synth_ms);
                                }
                                let _ = app
                                    .emit("tts:synth-done", serde_json::json!({ "ms": synth_ms }));
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
        // Drop the lazy sentence consumer so the next turn rebuilds it with the
        // current backend/voice/speed. stop()/speak() already do this; without it,
        // a TTS engine/voice/speed change between Brain turns would be ignored.
        *self.sentence_tx.lock().unwrap() = None;
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

/// Walk a WAV's RIFF chunk list and return the byte range `[start, end)` of the
/// named chunk's payload, or `None` if it isn't a parseable WAV / lacks the chunk.
/// Handles non-canonical files where `fmt `/`data` aren't at the fixed 44-byte
/// offset (e.g. libsndfile's `fact`/`PEAK` chunks for float WAVs).
fn find_wav_chunk(wav: &[u8], id: &[u8; 4]) -> Option<(usize, usize)> {
    if wav.len() < 12 || &wav[0..4] != b"RIFF" || &wav[8..12] != b"WAVE" {
        return None;
    }
    let mut pos = 12;
    while pos + 8 <= wav.len() {
        let size = u32::from_le_bytes(wav[pos + 4..pos + 8].try_into().ok()?) as usize;
        let data_start = pos + 8;
        if &wav[pos..pos + 4] == id {
            let end = data_start.saturating_add(size).min(wav.len());
            return Some((data_start, end));
        }
        // Chunks are word-aligned: advance past the data plus an optional pad byte.
        pos = data_start.saturating_add(size).saturating_add(size & 1);
    }
    None
}

/// Concatenate the PCM payloads of several same-format WAVs into one canonical WAV.
/// Parses each file's `data` chunk (rather than assuming a fixed 44-byte header) so
/// non-canonical inputs aren't spliced with their metadata chunks as if it were audio.
fn concatenate_wavs(chunks: &[Vec<u8>]) -> Vec<u8> {
    if chunks.is_empty() {
        return Vec::new();
    }
    if chunks.len() == 1 {
        return chunks[0].clone();
    }

    // The first parseable WAV provides the fmt template; concatenate every payload from it on.
    let Some(base_idx) = chunks
        .iter()
        .position(|c| find_wav_chunk(c, b"data").is_some())
    else {
        return chunks.concat(); // no parseable WAV — best-effort raw concat
    };

    let Some((fmt_start, fmt_end)) = find_wav_chunk(&chunks[base_idx], b"fmt ") else {
        return chunks.concat();
    };
    let fmt_bytes = chunks[base_idx][fmt_start..fmt_end].to_vec();

    let mut pcm = Vec::new();
    for chunk in &chunks[base_idx..] {
        if let Some((start, end)) = find_wav_chunk(chunk, b"data") {
            pcm.extend_from_slice(&chunk[start..end]);
        } else {
            pcm.extend_from_slice(chunk); // not a WAV — append raw (matches old fallback)
        }
    }

    let fmt_len = fmt_bytes.len() as u32;
    let data_len = pcm.len() as u32;
    // RIFF size = "WAVE"(4) + fmt header(8) + fmt body + data header(8) + data body.
    let riff_size = 4u32
        .saturating_add(8 + fmt_len)
        .saturating_add(8u32.saturating_add(data_len));

    let mut out = Vec::with_capacity(12 + 8 + fmt_bytes.len() + 8 + pcm.len());
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&riff_size.to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&fmt_len.to_le_bytes());
    out.extend_from_slice(&fmt_bytes);
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    out.extend_from_slice(&pcm);
    out
}
