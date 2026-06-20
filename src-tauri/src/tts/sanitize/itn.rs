// ITN (Inverse Text Normalization): spoken → written form.
// Uses text-processing-rs (Apache 2.0) — Rust port of NVIDIA NeMo Text Processing.
// 98.6% compatible with NeMo test suite (1200/1217 tests).
//
// Converts ASR output like "two hundred thirty two" → "232".
// Applied post-STT, before sending text to the Brain.

use text_processing_rs::{normalize, normalize_sentence};

/// Normalize spoken-form text to written form (ITN).
/// Handles numbers, dates, money, measurements, time, ordinals, etc.
pub fn itn_normalize(text: &str) -> String {
    // Use sentence-level mode which scans for normalizable spans
    // within larger text (doesn't blindly normalize the whole string).
    normalize_sentence(text)
}

/// Normalize a single token (e.g., "two hundred") — for short phrases.
#[allow(dead_code)]
pub fn itn_normalize_token(text: &str) -> String {
    normalize(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cardinal_numbers() {
        assert_eq!(itn_normalize("two hundred thirty two"), "232");
        assert_eq!(itn_normalize_token("two hundred"), "200");
    }

    #[test]
    fn test_money() {
        assert_eq!(itn_normalize_token("five dollars and fifty cents"), "$5.50");
    }

    #[test]
    fn test_dates() {
        let result = itn_normalize_token("january fifth twenty twenty five");
        println!("test_dates result: '{}'", result);
        let normalized_lower = result.to_lowercase();
        assert!(
            normalized_lower.contains("january") && result.contains("5") && result.contains("2025")
        );
    }

    #[test]
    fn test_time() {
        let result = itn_normalize_token("quarter past two pm");
        assert!(result.contains("02:15"));
    }

    #[test]
    fn test_sentence_mode() {
        let result = itn_normalize("I have twenty one apples");
        assert_eq!(result, "I have 21 apples");
    }

    #[test]
    fn test_sentence_mode_no_op() {
        // Plain text with no normalizable tokens should pass through
        let input = "Hello world this is a test";
        assert_eq!(itn_normalize(input), input);
    }
}
