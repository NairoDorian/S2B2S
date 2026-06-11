// TN (Text Normalization): written → spoken form.
// Uses text-processing-rs (Apache 2.0) — Rust port of NVIDIA NeMo Text Processing.
//
// Converts text like "123" → "one hundred twenty three", "$5.50" → "five dollars and fifty cents".
// Applied pre-TTS, after LLM output and markdown stripping.

use text_processing_rs::{tn_normalize, tn_normalize_sentence};

/// Normalize written-form text to spoken form (TN).
/// Handles numbers, dates, money, measurements, time, ordinals, etc.
pub fn tn_normalize_text(text: &str) -> String {
    // Sentence-level mode scans for normalizable spans within larger text.
    tn_normalize_sentence(text)
}

/// Normalize a single token — for targeted use.
#[allow(dead_code)]
pub fn tn_normalize_token(text: &str) -> String {
    tn_normalize(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cardinal_numbers() {
        assert_eq!(tn_normalize_token("123"), "one hundred twenty three");
    }

    #[test]
    fn test_money() {
        assert_eq!(tn_normalize_token("$5.50"), "five dollars and fifty cents");
    }

    #[test]
    fn test_dates() {
        let result = tn_normalize_token("January 5, 2025");
        assert!(
            result.contains("january")
                && result.contains("fifth")
                && result.contains("twenty five")
        );
    }

    #[test]
    fn test_time() {
        let result = tn_normalize_token("2:30 PM");
        assert!(result.contains("two") && result.contains("thirty"));
    }

    #[test]
    fn test_ordinal() {
        assert_eq!(tn_normalize_token("1st"), "first");
        assert_eq!(tn_normalize_token("21st"), "twenty first");
    }

    #[test]
    fn test_measurements() {
        let result = tn_normalize_token("200 km/h");
        assert!(result.contains("two hundred") && result.contains("kilometers per hour"));
    }

    #[test]
    fn test_sentence_mode() {
        let result = tn_normalize_text("I paid $5 for 23 items");
        assert_eq!(result, "I paid five dollars for twenty three items");
    }

    #[test]
    fn test_sentence_mode_no_op() {
        let input = "Hello world this is a test";
        assert_eq!(tn_normalize_text(input), input);
    }
}
