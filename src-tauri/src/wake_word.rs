//! Wake word detection — VAD-based speech activity detection.
//!
//! V1: Energy-threshold activity detection — no model files needed.
//! V2 (planned): sherpa-onnx KeywordSpotter for accurate phrase detection.
//!   The integration code is written (init_kws/feed_kws in git history);
//!   blocked on CRT linking: sherpa-onnx uses /MT static CRT while
//!   transcribe-rs/whisper uses /MD dynamic CRT (Windows only).
//!   To enable: add `sherpa-onnx = "1.13.2"` to Cargo.toml.
//!
//! ─── Mode of operation ───
//! • Always-on microphone captures 16 kHz audio into a 2s ring buffer.
//! • RMS energy threshold (0.03) detects speech vs silence.
//! • 3 consecutive positive checks (~150 ms) = confirmed detection.
//! • On detection: emits `wake-word:detected` event, starts continuous voice.
//!
//! ─── Privacy ───
//! • All processing on-device, ring buffer auto-cleared.
//! • No audio ever saved unless user opts into recording retention.
//! • Feature defaults OFF, requires explicit consent on first enable.

use crate::managers::audio::AudioRecordingManager;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager};

/// VAD-based wake word detector.
pub struct WakeWordDetector {
    /// Whether the detector is running.
    pub active: AtomicBool,
    /// Ring buffer (~2s @ 16kHz).
    ring_buffer: Mutex<Vec<f32>>,
    /// Consecutive positive detections for debounce.
    detection_count: AtomicU32,
}

impl WakeWordDetector {
    pub fn new() -> Self {
        Self {
            active: AtomicBool::new(false),
            ring_buffer: Mutex::new(Vec::with_capacity(32000)),
            detection_count: AtomicU32::new(0),
        }
    }

    /// Reset the ring buffer.
    pub fn reset(&self) {
        self.ring_buffer.lock().unwrap().clear();
        self.detection_count.store(0, Ordering::SeqCst);
    }

    /// Feed audio into the ring buffer for energy analysis.
    #[allow(dead_code)]
    pub fn feed_audio(&self, samples: &[f32]) {
        let mut buf = self.ring_buffer.lock().unwrap();
        buf.extend_from_slice(samples);
        let excess = buf.len().saturating_sub(32000);
        if excess > 0 {
            buf.drain(0..excess);
        }
    }

    /// Check RMS energy threshold (0.03) with 3-frame debounce.
    /// Returns true once per detection event.
    pub fn check_detected(&self) -> bool {
        let buf = self.ring_buffer.lock().unwrap();
        if buf.len() < 1600 {
            return false;
        }

        let rms = (buf.iter().map(|s| s * s).sum::<f32>() / buf.len() as f32).sqrt();
        if rms > 0.03 {
            let count = self.detection_count.fetch_add(1, Ordering::SeqCst) + 1;
            if count >= 3 {
                self.detection_count.store(0, Ordering::SeqCst);
                return true;
            }
        } else {
            self.detection_count.store(0, Ordering::SeqCst);
        }
        false
    }
}

/// Start the wake word detector background thread.
pub fn start_wake_word_detection(app: AppHandle) {
    let detector = app.state::<Arc<WakeWordDetector>>();
    if detector.active.load(Ordering::SeqCst) {
        return;
    }
    detector.reset();
    detector.active.store(true, Ordering::SeqCst);

    let det = detector.inner().clone();
    let app2 = app.clone();

    std::thread::spawn(move || {
        log::info!("[WakeWord] thread started (VAD mode)");
        // TO FINISH: connect audio input stream callback in recorder.rs to call detector.feed_audio()
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

/// Stop the wake word detector.
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
