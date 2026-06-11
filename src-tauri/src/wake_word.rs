//! Wake word detection — hybrid approach.
//!
//! Two detection modes that can run simultaneously:
//!
//! **KWS mode** (sherpa-onnx KeywordSpotter): accurate phrase detection
//! ("Hey S2B2S"). Requires downloading a small ONNX model (~1-5 MB).
//! Ideal as the primary wake word engine.
//!
//! **VAD mode** (energy-based): detects any speech activity via RMS energy
//! threshold. Works with zero model files, higher false-positive rate.
//! Useful as fallback or "voice activity trigger" alongside KWS.
//!
//! ─── Privacy ───
//! • Audio processed entirely on-device
//! • ~2s ring buffer, auto-cleared
//! • No audio saved unless user opts in
//! • Feature defaults OFF, requires consent

use crate::managers::audio::AudioRecordingManager;
use crate::settings::WakeWordConfig;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager};

/// Detection backend in use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WakeWordBackend {
    /// sherpa-onnx KeywordSpotter (requires model files).
    Kws,
    /// Energy/VAD-based activity detection (no models needed).
    Vad,
}

/// Hybrid wake word detector.
pub struct WakeWordDetector {
    /// Whether the detector is currently running.
    pub active: AtomicBool,
    /// Selected backend.
    backend: Mutex<WakeWordBackend>,
    /// Ring buffer of audio samples (~2s @ 16kHz).
    ring_buffer: Mutex<Vec<f32>>,
    /// Current keyword phrase.
    current_keyword: Mutex<String>,
    /// Consecutive detection counter (debounce).
    detection_count: AtomicU32,
    /// RMS energy threshold for VAD mode.
    energy_threshold: f32,
    // ── sherpa-onnx KWS state ──
    kws_spotter: Mutex<Option<sherpa_onnx::KeywordSpotter>>,
    kws_stream: Mutex<Option<sherpa_onnx::OnlineStream>>,
}

impl WakeWordDetector {
    pub fn new() -> Self {
        Self {
            active: AtomicBool::new(false),
            backend: Mutex::new(WakeWordBackend::Vad),
            ring_buffer: Mutex::new(Vec::with_capacity(32000)),
            current_keyword: Mutex::new(String::new()),
            detection_count: AtomicU32::new(0),
            energy_threshold: 0.03,
            kws_spotter: Mutex::new(None),
            kws_stream: Mutex::new(None),
        }
    }

    /// Initialize the detector. Tries KWS mode first if model files exist;
    /// falls back to VAD mode.
    pub fn init(&self, keyword: &str) -> Result<(), String> {
        let keyword = keyword.trim().to_lowercase();
        if keyword.is_empty() {
            return Err("Wake word cannot be empty".to_string());
        }
        *self.current_keyword.lock().unwrap() = keyword.clone();
        self.detection_count.store(0, Ordering::SeqCst);

        // Try KWS initialisation if model files are present
        if let Ok(()) = self.init_kws(&keyword) {
            *self.backend.lock().unwrap() = WakeWordBackend::Kws;
            log::info!("[WakeWord] KWS backend active (keyword: '{keyword}')");
            return Ok(());
        }

        // Fallback: VAD mode
        *self.backend.lock().unwrap() = WakeWordBackend::Vad;
        log::info!("[WakeWord] VAD backend active (keyword: '{keyword}', threshold: {})", self.energy_threshold);
        Ok(())
    }

    /// Try to initialise sherpa-onnx KWS.
    fn init_kws(&self, keyword: &str) -> Result<(), String> {
        let model_dir = Self::resolve_model_dir()?;
        let encoder = model_dir.join("encoder.onnx");
        let decoder = model_dir.join("decoder.onnx");
        let joiner = model_dir.join("joiner.onnx");
        let tokens = model_dir.join("tokens.txt");

        if !encoder.exists() || !decoder.exists() || !joiner.exists() || !tokens.exists() {
            return Err("KWS model files not found — download to models/wake_word/".to_string());
        }

        // KeywordSpotterConfig supports Default; set fields explicitly
        let mut config = sherpa_onnx::KeywordSpotterConfig::default();
        config.model_config = sherpa_onnx::OnlineModelConfig {
            transducer: sherpa_onnx::OnlineTransducerModelConfig {
                encoder: Some(encoder.to_string_lossy().to_string()),
                decoder: Some(decoder.to_string_lossy().to_string()),
                joiner: Some(joiner.to_string_lossy().to_string()),
            },
            tokens: Some(tokens.to_string_lossy().to_string()),
            num_threads: 1,
            provider: Some("cpu".to_string()),
            ..Default::default()
        };
        config.keywords_buf = Some(keyword.to_string());
        config.keywords_threshold = 0.6;
        config.max_active_paths = 4;

        let spotter = sherpa_onnx::KeywordSpotter::create(&config)
            .ok_or("Failed to create KeywordSpotter")?;
        let stream = spotter.create_stream();

        *self.kws_spotter.lock().unwrap() = Some(spotter);
        *self.kws_stream.lock().unwrap() = Some(stream);
        Ok(())
    }

    /// Feed audio into the detector.
    pub fn feed_audio(&self, samples: &[f32], sample_rate: u32) {
        let backend = *self.backend.lock().unwrap();

        // Always update ring buffer (for VAD mode + debug)
        {
            let mut buf = self.ring_buffer.lock().unwrap();
            buf.extend_from_slice(samples);
            let excess = buf.len().saturating_sub(32000);
            if excess > 0 { buf.drain(0..excess); }
        }

        // KWS path
        if backend == WakeWordBackend::Kws {
            self.feed_kws(samples, sample_rate);
            return;
        }

        // VAD path (fallback)
        self.check_vad();
    }

    fn feed_kws(&self, samples: &[f32], sample_rate: u32) {
        // 1. Feed audio to stream
        {
            let mut stream = self.kws_stream.lock().unwrap();
            if let Some(s) = stream.as_mut() {
                s.accept_waveform(sample_rate as i32, samples);
            }
        }
        // 2. Decode and check result
        let result_keyword = {
            let spotter = self.kws_spotter.lock().unwrap();
            let spotter = match spotter.as_ref() { Some(s) => s, None => return };
            let mut stream_guard = self.kws_stream.lock().unwrap();
            let stream = match stream_guard.as_mut() { Some(s) => s, None => return };
            if spotter.is_ready(stream) {
                spotter.decode(stream);
                spotter.get_result(stream).map(|r| r.keyword.clone())
            } else { None }
        };
        if let Some(ref kw) = result_keyword {
            if kw.len() > 2 {
                let count = self.detection_count.fetch_add(1, Ordering::SeqCst) + 1;
                if count >= 3 {
                    log::info!("[WakeWord/KWS] Detected: '{kw}'");
                    self.detection_count.store(0, Ordering::SeqCst);
                }
            } else {
                self.detection_count.store(0, Ordering::SeqCst);
            }
        }
    }

    /// VAD fallback: RMS energy check on ring buffer.
    fn check_vad(&self) {
        let buf = self.ring_buffer.lock().unwrap();
        if buf.len() < 1600 { return; }
        let rms = (buf.iter().map(|s| s * s).sum::<f32>() / buf.len() as f32).sqrt();
        if rms > self.energy_threshold {
            let count = self.detection_count.fetch_add(1, Ordering::SeqCst) + 1;
            if count >= 3 {
                self.detection_count.store(0, Ordering::SeqCst);
            }
        } else {
            self.detection_count.store(0, Ordering::SeqCst);
        }
    }

    /// Check if wake word was recently detected.
    pub fn check_detected(&self) -> bool {
        let backend = *self.backend.lock().unwrap();
        match backend {
            WakeWordBackend::Kws => self.detection_count.load(Ordering::Acquire) >= 3,
            WakeWordBackend::Vad => {
                // Re-check energy every call
                self.check_vad();
                self.detection_count.load(Ordering::Acquire) >= 3
            }
        }
    }

    /// Reset after detection.
    pub fn reset(&self) {
        self.ring_buffer.lock().unwrap().clear();
        self.detection_count.store(0, Ordering::SeqCst);
        {
            if let Some(spotter) = self.kws_spotter.lock().unwrap().as_ref() {
                if let Some(stream) = self.kws_stream.lock().unwrap().as_mut() {
                    stream.input_finished();
                }
                *self.kws_stream.lock().unwrap() = Some(spotter.create_stream());
            }
        }
    }

    fn resolve_model_dir() -> Result<std::path::PathBuf, String> {
        let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
        Ok(cwd.join("models").join("wake_word"))
    }
}

/// Start detection.
pub fn start_wake_word_detection(app: AppHandle) {
    let detector = app.state::<Arc<WakeWordDetector>>();
    if detector.active.load(Ordering::SeqCst) { return; }

    let keyword = crate::settings::get_settings(&app).tts.wake_word.keyword.clone();

    if let Err(e) = detector.init(&keyword) {
        log::error!("[WakeWord] init failed: {e}");
        let _ = app.emit("wake-word:error", e);
        return;
    }

    detector.active.store(true, Ordering::SeqCst);
    let det = detector.inner().clone();
    let app2 = app.clone();

    std::thread::spawn(move || {
        log::info!("[WakeWord] thread started");
        if let Some(mgr) = app2.try_state::<Arc<AudioRecordingManager>>() {
            mgr.enable_wake_word(true);
        }
        loop {
            if !det.active.load(Ordering::SeqCst) {
                if let Some(mgr) = app2.try_state::<Arc<AudioRecordingManager>>() {
                    mgr.enable_wake_word(false);
                }
                break;
            }
            if det.check_detected() {
                det.reset();
                let _ = app2.emit("wake-word:detected", "activity");
                if let Some(mgr) = app2.try_state::<Arc<AudioRecordingManager>>() {
                    let _ = mgr.set_continuous_mode(true);
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    });
}

/// Stop detection.
pub fn stop_wake_word_detection(app: AppHandle) {
    if let Some(detector) = app.try_state::<Arc<WakeWordDetector>>() {
        detector.active.store(false, Ordering::SeqCst);
        detector.reset();
    }
    if let Some(mgr) = app.try_state::<Arc<AudioRecordingManager>>() {
        mgr.enable_wake_word(false);
    }
    log::info!("[WakeWord] stopped");
}
