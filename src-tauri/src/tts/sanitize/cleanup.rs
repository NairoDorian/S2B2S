// Pass 3: Final cleanup — spacing, punctuation artifacts, and trailing whitespace.
// Ported from the AgentZero prototype (MIT); lazy_static -> once_cell.

use once_cell::sync::Lazy;
use regex::Regex;

/// Clean up spacing and punctuation artifacts introduced by prior normalization passes.
pub(crate) fn cleanup_artifacts(text: &str) -> String {
    static MULTI_SPACE: Lazy<Regex> = Lazy::new(|| Regex::new(r" {2,}").unwrap());
    static SPACE_BEFORE_PUNCT: Lazy<Regex> = Lazy::new(|| Regex::new(r" +([,.:;!?])").unwrap());
    static REPEATED_COMMA: Lazy<Regex> = Lazy::new(|| Regex::new(r",(\s*,)+").unwrap());
    static COMMA_BEFORE_PERIOD: Lazy<Regex> = Lazy::new(|| Regex::new(r",\s*\.").unwrap());
    static PUNCT_NO_SPACE: Lazy<Regex> = Lazy::new(|| Regex::new(r"([,;:])([A-Za-z])").unwrap());
    static TRAILING_COMMA: Lazy<Regex> = Lazy::new(|| Regex::new(r",\s*$").unwrap());
    static MULTI_NEWLINE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\n{3,}").unwrap());

    let mut result = text.to_string();

    // Run core cleanup twice to catch cascading artifacts
    for _ in 0..2 {
        result = MULTI_SPACE.replace_all(&result, " ").to_string();
        result = SPACE_BEFORE_PUNCT.replace_all(&result, "$1").to_string();
        result = REPEATED_COMMA.replace_all(&result, ",").to_string();
        result = COMMA_BEFORE_PERIOD.replace_all(&result, ".").to_string();
        result = PUNCT_NO_SPACE.replace_all(&result, "$1 $2").to_string();
    }

    // Remove trailing comma
    result = TRAILING_COMMA.replace_all(&result, "").to_string();

    // Collapse excessive blank lines
    result = MULTI_NEWLINE.replace_all(&result, "\n\n").to_string();

    // Trim each line and the whole string
    result
        .lines()
        .map(|line| line.trim())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tts::sanitize::tts_normalize::sanitize_tts;

    #[test]
    fn test_cleanup_double_spaces() {
        assert_eq!(cleanup_artifacts("word  more"), "word more");
        assert_eq!(cleanup_artifacts("word   more"), "word more");
    }

    #[test]
    fn test_cleanup_space_before_comma() {
        assert_eq!(cleanup_artifacts("word , more"), "word, more");
        assert_eq!(cleanup_artifacts("word  , more"), "word, more");
    }

    #[test]
    fn test_cleanup_double_commas() {
        assert_eq!(cleanup_artifacts("word,, more"), "word, more");
        assert_eq!(cleanup_artifacts("word, , more"), "word, more");
        assert_eq!(cleanup_artifacts("word,,, more"), "word, more");
    }

    #[test]
    fn test_cleanup_comma_before_period() {
        assert_eq!(cleanup_artifacts("word,."), "word.");
        assert_eq!(cleanup_artifacts("word, ."), "word.");
    }

    #[test]
    fn test_cleanup_missing_space_after_punct() {
        assert_eq!(cleanup_artifacts("word,next"), "word, next");
        assert_eq!(cleanup_artifacts("word;next"), "word; next");
    }

    #[test]
    fn test_cleanup_trailing_comma() {
        assert_eq!(cleanup_artifacts("word,"), "word");
        assert_eq!(cleanup_artifacts("word, "), "word");
    }

    #[test]
    fn test_cleanup_preserves_ellipsis() {
        assert_eq!(cleanup_artifacts("word..."), "word...");
        assert_eq!(cleanup_artifacts("wait... more"), "wait... more");
    }

    #[test]
    fn test_sanitize_tts_parentheses_no_artifacts() {
        let result = sanitize_tts("text (aside) more");
        assert_eq!(result, "text, aside, more");
    }

    #[test]
    fn test_sanitize_tts_no_double_commas_or_space_before_comma() {
        let input = "integrity (wholeness), grace (composure) and kindness under pressure (and a balanced proportion), seeing things";
        let result = sanitize_tts(input);
        assert!(!result.contains(",,"), "Double commas in: {}", result);
        assert!(!result.contains(" ,"), "Space before comma in: {}", result);
        assert!(!result.ends_with(','), "Trailing comma in: {}", result);
    }

    #[test]
    fn test_sanitize_tts_em_dash_spacing() {
        let result = sanitize_tts("word\u{2014}another");
        assert_eq!(result, "word, another");
    }
}
