// Text pagination: split long texts into fragments at sentence boundaries.
// Char-boundary-safe throughout (CopySpeak C2). Ported from the AgentZero
// prototype (MIT); only the config import path changed for S2B2S.

use crate::settings::PaginationConfig;

/// A text fragment created from splitting a larger text.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct TextFragment {
    /// The fragment text content
    pub text: String,
    /// Zero-based index of this fragment in sequence
    pub index: usize,
    /// Total number of fragments
    pub total: usize,
}

impl TextFragment {
    /// Create a new text fragment.
    pub fn new(text: String, index: usize, total: usize) -> Self {
        Self { text, index, total }
    }

    /// Returns true if this is the first fragment.
    #[allow(dead_code)]
    pub fn is_first(&self) -> bool {
        self.index == 0
    }

    /// Returns true if this is the last fragment.
    #[allow(dead_code)]
    pub fn is_last(&self) -> bool {
        self.index == self.total - 1
    }

    /// Returns a formatted label like "Part 1 of 3".
    #[allow(dead_code)]
    pub fn label(&self) -> String {
        format!("Part {} of {}", self.index + 1, self.total)
    }
}

/// Sentence boundary position in text.
#[derive(Debug, Clone, PartialEq)]
struct SentenceBoundary {
    /// Character position of the sentence end (inclusive)
    position: usize,
    /// The character that marks the sentence end
    delimiter: char,
}

/// Detect sentence boundary positions in the text.
/// Returns a list of positions where sentences end.
fn detect_sentence_boundaries(text: &str) -> Vec<SentenceBoundary> {
    let mut boundaries = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        // Check for sentence-ending punctuation
        if is_sentence_end(c) {
            // Skip if this looks like an abbreviation (e.g., "Mr.", "Dr.", "etc.")
            if is_abbreviation(&chars, i) {
                i += 1;
                continue;
            }

            // This is a sentence boundary
            boundaries.push(SentenceBoundary {
                position: i,
                delimiter: c,
            });
        }

        i += 1;
    }

    // If no boundaries found, treat the entire text as one sentence
    if boundaries.is_empty() && !text.is_empty() {
        boundaries.push(SentenceBoundary {
            position: text.chars().count().saturating_sub(1),
            delimiter: if let Some(&last_char) = chars.last() {
                last_char
            } else {
                '.'
            },
        });
    }

    boundaries
}

/// Check if a character is a sentence-ending punctuation.
fn is_sentence_end(c: char) -> bool {
    matches!(c, '.' | '!' | '?' | '。' | '！' | '？')
}

/// Check if the punctuation at position is likely part of an abbreviation.
fn is_abbreviation(chars: &[char], pos: usize) -> bool {
    // Need at least 1 character before the period
    if pos < 1 {
        return false;
    }

    // Check for special multi-period abbreviations (these take priority).
    // "e.g.", "i.e.", "n.b." are 4 chars including both periods.
    if pos >= 3 {
        let four_char: String = chars[pos - 3..=pos]
            .iter()
            .collect::<String>()
            .to_lowercase();
        if four_char == "e.g." || four_char == "i.e." || four_char == "n.b." {
            return true;
        }
    }

    // "vs." is only 3 chars (v, s, .) — checked separately so it isn't missed by
    // the 4-char window above (which could never match it).
    if pos >= 2 {
        let three_char: String = chars[pos - 2..=pos]
            .iter()
            .collect::<String>()
            .to_lowercase();
        if three_char == "vs." {
            return true;
        }
    }

    // Check for "etc." (5 chars including the leading space/boundary)
    if pos >= 4 {
        let five_char: String = chars[pos - 4..=pos]
            .iter()
            .collect::<String>()
            .to_lowercase();
        if five_char == " etc." || five_char == ".etc." {
            return true;
        }
    }

    // Extract the word before the period by scanning backwards
    let mut word_start = pos - 1;
    while word_start > 0 && chars[word_start - 1].is_alphabetic() {
        word_start -= 1;
    }

    // The word must be preceded by whitespace or start of text (not part of a larger word)
    if word_start > 0 && !chars[word_start - 1].is_whitespace() && chars[word_start - 1] != '.' {
        return false;
    }

    let word: String = chars[word_start..pos]
        .iter()
        .collect::<String>()
        .to_lowercase();

    // Known title/honorific abbreviations (case-insensitive)
    matches!(
        word.as_str(),
        "mr" | "mrs"
            | "ms"
            | "dr"
            | "sr"
            | "jr"
            | "prof"
            | "rev"
            | "gen"
            | "gov"
            | "sgt"
            | "cpl"
            | "pvt"
            | "lt"
            | "capt"
            | "col"
            | "maj"
            | "cmdr"
            | "st"
            | "ave"
            | "blvd"
            | "dept"
            | "est"
            | "approx"
            | "inc"
            | "ltd"
            | "corp"
            | "no"
            | "vol"
            | "fig"
            | "ed"
            | "al"
    )
}

/// Split text into fragments at sentence boundaries, respecting the maximum fragment size.
/// Each fragment will be as close to the target size as possible without splitting sentences.
///
/// # Arguments
/// * `text` - The text to split into fragments
/// * `config` - Pagination configuration with fragment size setting
///
/// # Returns
/// A vector of text fragments. Returns a single fragment if pagination is disabled or text is short.
pub fn paginate_text(text: &str, config: &PaginationConfig) -> Vec<TextFragment> {
    // If pagination is disabled or text is empty, return as single fragment
    if !config.enabled || text.is_empty() {
        return vec![TextFragment::new(text.to_string(), 0, 1)];
    }

    let max_size = config.fragment_size as usize;
    let text_len = text.chars().count();

    // If text fits in one fragment, return as single fragment
    if text_len <= max_size {
        return vec![TextFragment::new(text.to_string(), 0, 1)];
    }

    // Detect sentence boundaries
    let boundaries = detect_sentence_boundaries(text);

    // If no boundaries found, force split at max_size (fallback)
    if boundaries.is_empty() {
        log::warn!(
            "[Pagination] No sentence boundaries found, forcing split at {} chars",
            max_size
        );
        return force_split(text, max_size);
    }

    // Build fragments by grouping sentences within max_size.
    //
    // We iterate over sentence boundaries and accumulate sentences into the
    // current fragment until adding the next sentence would exceed max_size,
    // then we cut.
    let mut fragments = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut fragment_start = 0; // start of the current accumulating fragment

    for (idx, boundary) in boundaries.iter().enumerate() {
        let sentence_end = boundary.position + 1; // position after the delimiter
        let current_fragment_len = sentence_end - fragment_start;

        // Check if including this sentence would exceed the limit
        if current_fragment_len > max_size {
            // If we already have accumulated sentences before this one, flush them
            if idx > 0 {
                let prev_end = boundaries[idx - 1].position + 1;
                if prev_end > fragment_start {
                    let fragment: String = chars[fragment_start..prev_end].iter().collect();
                    if !fragment.trim().is_empty() {
                        fragments.push(fragment);
                    }
                    fragment_start = prev_end;
                }
            }

            // Now check if this single sentence is itself longer than max_size.
            // If so, force-split it, then continue from after this sentence.
            let lone_len = sentence_end - fragment_start;
            if lone_len > max_size {
                let lone_text: String = chars[fragment_start..sentence_end].iter().collect();
                let sub_fragments = force_split(&lone_text, max_size);
                for sf in sub_fragments {
                    fragments.push(sf.text);
                }
                fragment_start = sentence_end;
            }
            // Otherwise this sentence starts a new fragment, continue accumulating
        }
        // else: sentence fits within max_size from fragment_start, keep accumulating
    }

    // Add the final fragment (text from fragment_start to end)
    if fragment_start < chars.len() {
        let final_text: String = chars[fragment_start..].iter().collect();
        if !final_text.trim().is_empty() {
            fragments.push(final_text);
        }
    }

    // If we only have one fragment, return it
    if fragments.len() <= 1 {
        return vec![TextFragment::new(text.to_string(), 0, 1)];
    }

    // Create TextFragment objects
    let total = fragments.len();
    fragments
        .into_iter()
        .enumerate()
        .map(|(index, fragment_text)| {
            TextFragment::new(fragment_text.trim().to_string(), index, total)
        })
        .collect()
}

/// Fallback: split text at exact character positions when no sentence boundaries exist.
fn force_split(text: &str, max_size: usize) -> Vec<TextFragment> {
    let mut fragments = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut start = 0;
    let mut index = 0;

    while start < chars.len() {
        let end = (start + max_size).min(chars.len());
        let fragment: String = chars[start..end].iter().collect();
        fragments.push(TextFragment::new(fragment.trim().to_string(), index, 0));
        start = end;
        index += 1;
    }

    // Update total counts
    let total = fragments.len();
    for fragment in &mut fragments {
        fragment.total = total;
    }

    fragments
}

/// Get the number of fragments that would be created for the given text.
#[allow(dead_code)]
pub fn estimate_fragment_count(text: &str, config: &PaginationConfig) -> usize {
    if !config.enabled || text.is_empty() {
        return 1;
    }

    let max_size = config.fragment_size as usize;
    let text_len = text.chars().count();

    if text_len <= max_size {
        return 1;
    }

    // Estimate based on sentence boundaries
    let boundaries = detect_sentence_boundaries(text);
    if boundaries.is_empty() {
        return text_len.div_ceil(max_size); // Ceiling division
    }

    // Count how many fragments would be created (mirrors paginate_text logic)
    let mut count = 1;
    let mut fragment_start = 0;

    for (idx, boundary) in boundaries.iter().enumerate() {
        let sentence_end = boundary.position + 1;
        let current_fragment_len = sentence_end - fragment_start;

        if current_fragment_len > max_size {
            if idx > 0 {
                let prev_end = boundaries[idx - 1].position + 1;
                if prev_end > fragment_start {
                    count += 1;
                    fragment_start = prev_end;
                }
            }

            let lone_len = sentence_end - fragment_start;
            if lone_len > max_size {
                count += lone_len.div_ceil(max_size) - 1;
                fragment_start = sentence_end;
            }
        }
    }

    count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> PaginationConfig {
        PaginationConfig::default()
    }

    #[test]
    fn test_paginate_disabled_returns_single() {
        let config = PaginationConfig {
            enabled: false,
            ..default_config()
        };
        let text = "This is a very long text that should normally be split into multiple fragments for proper TTS processing.";
        let fragments = paginate_text(text, &config);
        assert_eq!(fragments.len(), 1);
        assert_eq!(fragments[0].text, text);
    }

    #[test]
    fn test_short_text_returns_single() {
        let config = PaginationConfig {
            enabled: true,
            fragment_size: 2000,
        };
        let text = "Short text.";
        let fragments = paginate_text(text, &config);
        assert_eq!(fragments.len(), 1);
        assert_eq!(fragments[0].text, text);
    }

    #[test]
    fn test_empty_text_returns_single() {
        let config = default_config();
        let fragments = paginate_text("", &config);
        assert_eq!(fragments.len(), 1);
        assert_eq!(fragments[0].text, "");
    }

    #[test]
    fn test_sentence_boundary_detection_periods() {
        let text = "First sentence. Second sentence. Third sentence.";
        let boundaries = detect_sentence_boundaries(text);
        assert_eq!(boundaries.len(), 3);
        assert_eq!(boundaries[0].delimiter, '.');
        assert_eq!(boundaries[1].delimiter, '.');
        assert_eq!(boundaries[2].delimiter, '.');
    }

    #[test]
    fn test_abbreviation_handling() {
        let text = "Mr. Smith went to the store. Dr. Jones was there.";
        let boundaries = detect_sentence_boundaries(text);
        // Only "store." and "there." are real boundaries.
        assert_eq!(boundaries.len(), 2);
    }

    #[test]
    fn test_abbreviation_handling_with_etc() {
        let text = "We have apples, oranges, etc. Then we have bananas.";
        let boundaries = detect_sentence_boundaries(text);
        assert_eq!(boundaries.len(), 1);
    }

    #[test]
    fn test_chinese_sentence_boundaries() {
        let text = "这是第一句。这是第二句！这是第三句？";
        let boundaries = detect_sentence_boundaries(text);
        assert_eq!(boundaries.len(), 3);
        assert_eq!(boundaries[0].delimiter, '。');
        assert_eq!(boundaries[1].delimiter, '！');
        assert_eq!(boundaries[2].delimiter, '？');
    }

    #[test]
    fn test_emoji_and_combining_marks_do_not_panic() {
        // UTF-8 torture: emoji, combining accents, CJK, RTL.
        let config = PaginationConfig {
            enabled: true,
            fragment_size: 10,
        };
        let text = "Café ☕ déjà vu 😀😀😀. 你好世界！مرحبا بالعالم. Ñoño e\u{0301}.";
        let fragments = paginate_text(text, &config);
        // The invariant we care about: no panic, content preserved (ignoring spaces).
        let combined: String = fragments.iter().map(|f| f.text.as_str()).collect();
        assert_eq!(combined.replace(' ', ""), text.replace(' ', ""));
    }

    #[test]
    fn test_text_fragment_properties() {
        let fragment = TextFragment::new("Test text".to_string(), 1, 3);
        assert!(!fragment.is_first());
        assert!(!fragment.is_last());
        assert_eq!(fragment.label(), "Part 2 of 3");
    }

    #[test]
    fn test_paginate_long_text_at_sentence_boundaries() {
        let config = PaginationConfig {
            enabled: true,
            fragment_size: 50,
        };
        let text = "This is sentence one. This is sentence two. This is sentence three. This is sentence four. This is sentence five.";
        let fragments = paginate_text(text, &config);

        assert!(fragments.len() > 1);

        for fragment in &fragments {
            if !fragment.text.is_empty() && !fragment.is_last() {
                let last_char = fragment.text.chars().last().unwrap();
                assert!(
                    is_sentence_end(last_char),
                    "Fragment should end at sentence boundary: {}",
                    fragment.text
                );
            }
        }

        let combined: String = fragments.iter().map(|f| f.text.as_str()).collect();
        assert_eq!(combined.replace(' ', ""), text.replace(' ', ""));
    }

    #[test]
    fn test_force_split_no_sentence_boundaries() {
        let text = "word1 word2 word3 word4 word5 word6 word7 word8 word9 word10";
        let fragments = force_split(text, 20);
        assert!(fragments.len() > 1);
        for fragment in &fragments {
            assert!(fragment.text.chars().count() <= 20);
        }
    }

    #[test]
    fn test_paginate_preserves_content_various_lengths() {
        let test_lengths = vec![100usize, 500, 1000, 5000, 10000];
        for length in test_lengths {
            let config = PaginationConfig {
                enabled: true,
                fragment_size: 1000,
            };
            let text = "A sentence. ".repeat(length / 15);
            let fragments = paginate_text(&text, &config);
            let reconstructed: String = fragments.iter().map(|f| f.text.as_str()).collect();
            assert_eq!(
                reconstructed.replace(' ', ""),
                text.replace(' ', ""),
                "Content mismatch for text length {}",
                length
            );
        }
    }
}
