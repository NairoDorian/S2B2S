// Sanitize: multi-pass text cleaning pipeline for TTS.
// Removes markdown formatting, normalizes text for speech, and cleans up artifacts.
// Ported from the AgentZero prototype (MIT); lazy_static -> once_cell, config flattened.

pub(crate) mod cleanup;
mod markdown;
pub(crate) mod tts_normalize;

use crate::settings::SanitizationConfig;

// Re-export the public API
pub use tts_normalize::sanitize_tts;

/// Sanitize text by removing markdown formatting and normalizing for TTS.
/// Returns the sanitized text with all enabled sanitization passes applied.
///
/// # Pipeline Order
/// 1. **Markdown Stripping** (if enabled)
/// 2. **TTS Normalization** (if enabled)
/// 3. **Artifact Cleanup** (always runs)
pub fn sanitize_text(text: &str, config: &SanitizationConfig) -> String {
    if !config.enabled {
        return text.to_string();
    }

    let mut result = text.to_string();

    // Pass 1: Strip markdown syntax
    if config.markdown {
        result = markdown::strip_markdown(&result);
    }

    // Pass 2: TTS text normalization
    if config.tts_normalization {
        result = sanitize_tts(&result);
    }

    // Final pass: clean up spacing and punctuation artifacts from all prior passes
    cleanup::cleanup_artifacts(&result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(enabled: bool, markdown: bool, tts_normalization: bool) -> SanitizationConfig {
        SanitizationConfig {
            enabled,
            markdown,
            tts_normalization,
        }
    }

    #[test]
    fn test_full_sanitize_enabled() {
        let input = "# Title\n**bold** w/o issue & 10km etc.";
        let result = sanitize_text(input, &cfg(true, true, true));
        assert!(!result.contains('#'));
        assert!(!result.contains("**"));
        assert!(result.contains("without"));
        assert!(result.contains("and"));
        assert!(result.contains("kilometers"));
        assert!(result.contains("et cetera"));
    }

    #[test]
    fn test_full_sanitize_disabled() {
        let input = "# Title w/o **bold**";
        let result = sanitize_text(input, &cfg(false, true, true));
        assert_eq!(result, input);
    }

    #[test]
    fn test_full_sanitize_markdown_only() {
        let input = "# Title\n**Bold** text & ~100 km/h.";
        let result = sanitize_text(input, &cfg(true, true, false));
        assert!(!result.contains('#'));
        assert!(!result.contains("**"));
        assert!(
            result.contains('&'),
            "Should not expand '&' without TTS normalization"
        );
        assert!(
            result.contains('~'),
            "Should not expand '~' without TTS normalization"
        );
    }

    #[test]
    fn test_full_sanitize_tts_normalization_only() {
        let input = "# Title\n**Bold** text & ~100 km/h.";
        let result = sanitize_text(input, &cfg(true, false, true));
        assert!(result.contains('#'), "Should keep markdown headers");
        assert!(result.contains("and"), "Should expand '&'");
        assert!(result.contains("approximately"), "Should expand '~'");
        assert!(result.contains("kilometers"), "Should expand 'km'");
    }

    #[test]
    fn test_full_sanitize_clean_output() {
        let input = "According to [1], the result (see appendix), was **significant** & ~100km/h.";
        let result = sanitize_text(input, &cfg(true, true, true));
        assert!(!result.contains(",,"), "Double commas in: {}", result);
        assert!(!result.contains(" ,"), "Space before comma in: {}", result);
        assert!(!result.contains("  "), "Double spaces in: {}", result);
    }

    #[test]
    fn test_full_sanitize_realistic_text() {
        let input = r#"# Introduction

According to **recent studies** [1], the speed limit is ~100 km/h (e.g. on highways). Dr. Smith & Prof. Johnson reported that the company's revenue grew from $2bn to $5bn.

Visit https://example.com for more info & contact user@example.com.

- Item 1
- Item 2

> This is an important quote from the research.

See [documentation](https://docs.example.com) for details."#;

        let result = sanitize_text(input, &cfg(true, true, true));

        assert!(!result.contains('#'), "Should not contain markdown headers");
        assert!(!result.contains("**"), "Should not contain bold markers");
        assert!(!result.contains("[1]"), "Should not contain citations");
        assert!(!result.contains("- "), "Should not contain list markers");
        assert!(
            !result.contains("> "),
            "Should not contain blockquote markers"
        );
        assert!(!result.contains("https://"), "Should not contain URLs");
        assert!(result.contains("for example"), "Should expand 'e.g.'");
        assert!(result.contains("approximately"), "Should expand '~'");
        assert!(result.contains("kilometers"), "Should expand 'km'");
        assert!(result.contains("Doctor"), "Should expand 'Dr.'");
        assert!(result.contains("Professor"), "Should expand 'Prof.'");
        assert!(result.contains("billion"), "Should expand 'bn'");
        assert!(result.contains("dollars"), "Should expand '$'");
        assert!(
            result.contains("at example.com"),
            "Should expand '@' in email"
        );
        assert!(!result.contains("  "), "Should not have double spaces");
        assert!(!result.contains(",,"), "Should not have double commas");
    }
}
