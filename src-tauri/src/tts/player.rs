//! Streaming TTS audio player.
//!
//! A dedicated audio thread owns a rodio [`OutputStream`] + [`Sink`]. Fragments
//! are decoded and `append`ed to a single sink so they play back gapless and in
//! order — the manager can synthesize fragment *i+1* while *i* is still playing.
//! State changes are surfaced to the UI via Tauri events.

use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player};
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

enum Cmd {
    /// Decode and append WAV/MP3/etc. bytes to the active sink.
    Append(Vec<u8>),
    /// Stop playback and drop the current sink/stream (clears the queue).
    Stop,
    Pause,
    Resume,
    SetVolume(u8),
}

/// Thread-safe, cloneable handle to the audio thread.
#[derive(Clone)]
pub struct TtsPlayer {
    tx: Sender<Cmd>,
    is_playing: Arc<AtomicBool>,
    is_paused: Arc<AtomicBool>,
}

impl TtsPlayer {
    pub fn new(app: AppHandle) -> Self {
        let (tx, rx) = channel::<Cmd>();
        let is_playing = Arc::new(AtomicBool::new(false));
        let is_paused = Arc::new(AtomicBool::new(false));
        let is_playing_t = is_playing.clone();
        let is_paused_t = is_paused.clone();

        thread::spawn(move || {
            // The device sink must stay alive for the duration of playback.
            let mut _stream: Option<MixerDeviceSink> = None;
            let mut sink: Option<Player> = None;
            let mut volume: f32 = 1.0;
            let mut prev_playing = false;
            let mut just_appended = false;
            let mut empty_ticks = 0usize;
            // Whether WE showed the speaking HUD (so we only hide what we showed).
            let mut overlay_shown = false;

            loop {
                let now_playing = sink.as_ref().is_some_and(|s| !s.empty());
                let now_paused = sink.as_ref().is_some_and(|s| s.is_paused());
                is_paused_t.store(now_paused, Ordering::Relaxed);

                if now_playing {
                    empty_ticks = 0;
                    just_appended = false;
                } else if sink.is_some() {
                    empty_ticks += 1;
                }

                // Debounced transition from playing to finished
                let should_stop = sink.is_some() && !just_appended && empty_ticks >= 6; // 6 ticks * 50ms = 300ms
                let reported_playing = sink.is_some() && !should_stop;
                is_playing_t.store(reported_playing, Ordering::Relaxed);

                if should_stop {
                    // Queue drained naturally: release the device.
                    let _ = app.emit("tts:finished", ());
                    let _ = app.emit("tts:playing-changed", false);
                    if overlay_shown {
                        crate::overlay::hide_recording_overlay(&app);
                        overlay_shown = false;
                    }
                    sink = None;
                    _stream = None;
                    empty_ticks = 0;
                    prev_playing = false;
                } else {
                    if reported_playing != prev_playing {
                        let _ = app.emit("tts:playing-changed", reported_playing);
                        prev_playing = reported_playing;
                    }
                }

                match rx.recv_timeout(Duration::from_millis(50)) {
                    Ok(Cmd::Append(bytes)) => {
                        just_appended = true;
                        if bytes.len() < 16 {
                            log::warn!("[TtsPlayer] ignoring tiny audio ({} bytes)", bytes.len());
                            continue;
                        }
                        if sink.is_none() {
                            match DeviceSinkBuilder::from_default_device()
                                .and_then(|b| b.open_stream())
                            {
                                Ok(s) => {
                                    let sk = Player::connect_new(s.mixer());
                                    sk.set_volume(volume);
                                    sink = Some(sk);
                                    _stream = Some(s);
                                }
                                Err(e) => {
                                    log::error!("[TtsPlayer] no output device: {e}");
                                    continue;
                                }
                            }
                        }
                        if let Some(sk) = &sink {
                            match Decoder::new(Cursor::new(bytes)) {
                                Ok(src) => {
                                    sk.append(src);
                                    // Reflect "playing" immediately so the next
                                    // tick doesn't briefly report idle.
                                    prev_playing = true;
                                    is_playing_t.store(true, Ordering::Relaxed);
                                    let _ = app.emit("tts:playing-changed", true);
                                    if !overlay_shown {
                                        crate::overlay::show_speaking_overlay(&app);
                                        overlay_shown = true;
                                    }
                                }
                                Err(e) => log::error!("[TtsPlayer] decode failed: {e}"),
                            }
                        }
                    }
                    Ok(Cmd::Stop) => {
                        if let Some(sk) = sink.take() {
                            sk.stop();
                        }
                        _stream = None;
                        prev_playing = false;
                        is_playing_t.store(false, Ordering::Relaxed);
                        is_paused_t.store(false, Ordering::Relaxed);
                        if overlay_shown {
                            crate::overlay::hide_recording_overlay(&app);
                            overlay_shown = false;
                        }
                    }
                    Ok(Cmd::Pause) => {
                        if let Some(sk) = &sink {
                            sk.pause();
                        }
                    }
                    Ok(Cmd::Resume) => {
                        if let Some(sk) = &sink {
                            sk.play();
                        }
                    }
                    Ok(Cmd::SetVolume(v)) => {
                        volume = (v as f32 / 100.0).clamp(0.0, 2.0);
                        if let Some(sk) = &sink {
                            sk.set_volume(volume);
                        }
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
            log::debug!("[TtsPlayer] audio thread exited");
        });

        Self {
            tx,
            is_playing,
            is_paused,
        }
    }

    /// Append synthesized audio bytes to the playback queue.
    pub fn append(&self, bytes: Vec<u8>) {
        let _ = self.tx.send(Cmd::Append(bytes));
    }

    /// Stop playback and clear the queue.
    pub fn stop(&self) {
        let _ = self.tx.send(Cmd::Stop);
    }

    pub fn pause(&self) {
        let _ = self.tx.send(Cmd::Pause);
    }

    pub fn resume(&self) {
        let _ = self.tx.send(Cmd::Resume);
    }

    pub fn set_volume(&self, volume: u8) {
        let _ = self.tx.send(Cmd::SetVolume(volume));
    }

    pub fn is_playing(&self) -> bool {
        self.is_playing.load(Ordering::Relaxed)
    }

    pub fn is_paused(&self) -> bool {
        self.is_paused.load(Ordering::Relaxed)
    }
}
