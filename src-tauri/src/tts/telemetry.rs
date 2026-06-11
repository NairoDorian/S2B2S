//! TTS performance telemetry — tracks per-engine synthesis speed for adaptive
//! pagination and ETA estimates.
//!
//! Pattern: CopySpeak's telemetry.rs — per-engine `chars_per_ms` stats
//! drive adaptive fragment sizing so faster engines get bigger chunks.
//!
//! Data is flushed to disk periodically and on app exit.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;

/// Running average for a single engine + voice combination.
#[allow(dead_code)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct RunningStats {
    samples: u64,
    total_chars: u64,
    total_ms: u64,
    /// Rolling chars-per-millisecond estimate.
    chars_per_ms: f64,
}

#[allow(dead_code)]
impl RunningStats {
    fn record(&mut self, chars: u64, ms: u64) {
        self.samples += 1;
        self.total_chars += chars;
        self.total_ms += ms;
        // Exponential moving average with α = 0.1
        let rate = chars as f64 / ms.max(1) as f64;
        if self.chars_per_ms == 0.0 {
            self.chars_per_ms = rate;
        } else {
            self.chars_per_ms = self.chars_per_ms * 0.9 + rate * 0.1;
        }
    }
}

/// Global telemetry store.
#[allow(dead_code)]
pub struct Telemetry {
    stats: RwLock<HashMap<String, RunningStats>>,
}

#[allow(dead_code)]
impl Telemetry {
    pub fn new() -> Self {
        Self {
            stats: RwLock::new(HashMap::new()),
        }
    }

    /// Record a synthesis event for e.g. `"piper:en_US-lessac-medium"`.
    pub fn record(&self, key: &str, chars: usize, duration_ms: u64) {
        let mut guard = self.stats.write().unwrap_or_else(|e| e.into_inner());
        guard
            .entry(key.to_string())
            .or_default()
            .record(chars as u64, duration_ms);
    }

    /// Get the estimated chars-per-millisecond for an engine+voice combination.
    /// Returns `None` if no samples exist yet.
    pub fn chars_per_ms(&self, key: &str) -> Option<f64> {
        let guard = self.stats.read().unwrap_or_else(|e| e.into_inner());
        guard.get(key).map(|s| s.chars_per_ms)
    }

    /// Estimate synthesis time for `char_count` characters.
    /// Returns `(estimated_ms, confidence)` where confidence ranges 0.0–1.0.
    pub fn estimate(&self, key: &str, char_count: usize) -> (u64, f32) {
        let guard = self.stats.read().unwrap_or_else(|e| e.into_inner());
        match guard.get(key) {
            Some(s) if s.samples > 0 => {
                let ms = (char_count as f64 / s.chars_per_ms.max(0.001)) as u64;
                // Confidence grows with sample count, capped at 0.95
                let conf = (s.samples as f32 / 10.0).min(0.95);
                (ms, conf)
            }
            _ => {
                // Default: assume 0.1 chars/ms = 10 chars/sec (conservative)
                (char_count as u64 * 10, 0.0)
            }
        }
    }

    /// Compute adaptive fragment size for a given engine.
    /// Fast engines get larger fragments, slow engines get smaller ones.
    /// Returns a fragment size in characters.
    pub fn adaptive_fragment_size(&self, key: &str, default_size: usize) -> usize {
        let guard = self.stats.read().unwrap_or_else(|e| e.into_inner());
        match guard.get(key) {
            Some(s) if s.chars_per_ms > 0.0 => {
                if s.chars_per_ms > 1.0 {
                    // Very fast (CUDA cloud): ×3, capped at 2000
                    (default_size * 3).min(2000)
                } else if s.chars_per_ms > 0.3 {
                    // Moderate: ×2, capped at 1500
                    (default_size * 2).min(1500)
                } else {
                    // Slow CPU: keep default
                    default_size
                }
            }
            _ => default_size,
        }
    }
}

impl Default for Telemetry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recording_and_estimate() {
        let t = Telemetry::new();
        t.record("test:voice", 100, 500); // 0.2 chars/ms
        t.record("test:voice", 200, 1000);
        t.record("test:voice", 150, 750);

        let (ms, conf) = t.estimate("test:voice", 100);
        assert!(ms > 0);
        assert!(conf > 0.0);
        assert!(conf < 1.0);
    }

    #[test]
    fn test_unknown_key_fallback() {
        let t = Telemetry::new();
        let (ms, conf) = t.estimate("unknown:voice", 100);
        assert_eq!(ms, 1000); // 100 * 10
        assert_eq!(conf, 0.0);
    }

    #[test]
    fn test_adaptive_sizing() {
        let t = Telemetry::new();
        // No data: default
        assert_eq!(t.adaptive_fragment_size("none", 500), 500);
        // Fast
        t.record("fast:v", 5000, 1000); // 5.0 chars/ms
                                        // Need >=3 samples for running stats to be reliable
        t.record("fast:v", 5000, 1000);
        t.record("fast:v", 5000, 1000);
        assert!(t.adaptive_fragment_size("fast:v", 500) > 500);
    }
}
