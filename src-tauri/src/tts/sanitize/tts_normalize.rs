// Pass 2: TTS text normalization — expands abbreviations, symbols, and units for TTS readability.
// Ported from the AgentZero prototype (MIT); lazy_static -> once_cell.

use once_cell::sync::Lazy;
use regex::Regex;

use super::cleanup::cleanup_artifacts;

/// Normalize text for TTS readability.
/// Applies replacements in priority order: emojis → URLs → citations → slashes →
/// Latin abbreviations → metric units → symbols → punctuation.
/// Newlines are stripped at the end — they don't affect speech and produce
/// cleaner single-line text for history preview.
pub fn sanitize_tts(text: &str) -> String {
    let mut result = text.to_string();

    // Order matters — run in the specified priority sequence
    result = remove_emojis(&result);
    result = remove_urls(&result);
    result = remove_citations(&result);
    result = expand_slash_lookups(&result);
    result = expand_slash_options(&result);
    result = expand_slash_ratios(&result);
    result = expand_latin_abbreviations(&result);
    result = expand_title_abbreviations(&result);
    result = expand_number_suffixes(&result); // Run before metric units (5m = 5 million, not 5 meters)
    result = expand_metric_units(&result);
    result = expand_symbols(&result);
    result = normalize_punctuation(&result);
    result = cleanup_artifacts(&result);

    // Strip newlines — they have no effect on speech and produce cleaner
    // single-line output for history preview.
    result = result.replace('\r', "").replace('\n', " ");
    result.trim().to_string()
}

// ── 0. Emoji Removal ─────────────────────────────────────────────────────────

fn remove_emojis(text: &str) -> String {
    // Matches common emoji Unicode ranges:
    // 1F300–1F9FF: misc symbols, pictographs, emoticons, transport, etc.
    // 1FA00–1FAFF: newer emoji additions
    // 2600–27BF:   miscellaneous symbols and dingbats
    // FE00–FE0F:   variation selectors (emoji vs text presentation)
    // 200D:        zero-width joiner (used in multi-part emoji sequences)
    // 20E3:        combining enclosing keycap (e.g. 1️⃣)
    // 1F1E0–1F1FF: regional indicator symbols (flag pairs)
    static EMOJI_REGEX: Lazy<Regex> = Lazy::new(|| {
        Regex::new(
            r"[\u{1F300}-\u{1F9FF}\u{1FA00}-\u{1FAFF}\u{2600}-\u{27BF}\u{FE00}-\u{FE0F}\u{200D}\u{20E3}\u{1F1E0}-\u{1F1FF}]+",
        )
        .unwrap()
    });
    EMOJI_REGEX.replace_all(text, "").to_string()
}

// ── 1. Web Artifacts ────────────────────────────────────────────────────────

fn remove_urls(text: &str) -> String {
    static URL_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"https?://[^\s)\]]+").unwrap());
    URL_REGEX.replace_all(text, "").to_string()
}

// ── 2. Citations ────────────────────────────────────────────────────────────

fn remove_citations(text: &str) -> String {
    static CITATION_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"\[[a-zA-Z0-9]+\]").unwrap());
    CITATION_REGEX.replace_all(text, "").to_string()
}

// ── 3. Slash Lookups (Priority 1 — specific abbreviations) ──────────────────

fn expand_slash_lookups(text: &str) -> String {
    // Order matters: match longer patterns first; w/ must come after w/o.
    static WO_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\bw/o\b").unwrap());
    static NA_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\bn/a\b").unwrap());
    static AND_OR_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\band/or\b").unwrap());
    static W_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\bw/\s*").unwrap());

    let result = WO_REGEX.replace_all(text, "without").to_string();
    let result = NA_REGEX.replace_all(&result, "not applicable").to_string();
    let result = AND_OR_REGEX.replace_all(&result, "and or").to_string();
    W_REGEX.replace_all(&result, "with ").to_string()
}

// ── 4. Slash Options (Priority 2 — wordA/wordB → wordA or wordB) ───────────

fn expand_slash_options(text: &str) -> String {
    static SLASH_OPTION_REGEX: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"\b([a-zA-Z]+)/([a-zA-Z]+)\b").unwrap());
    SLASH_OPTION_REGEX.replace_all(text, "$1 or $2").to_string()
}

// ── 5. Slash Ratios (Priority 3 — unit/unit → unit per unit) ────────────────

fn expand_slash_ratios(text: &str) -> String {
    // Match patterns like km/h, m/s, miles/hour — unit/unit after a digit
    static SLASH_RATIO_REGEX: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(\d\s*)([a-zA-Z]+)/([a-zA-Z]+)").unwrap());
    SLASH_RATIO_REGEX
        .replace_all(text, "$1$2 per $3")
        .to_string()
}

// ── 6. Latin Abbreviations ──────────────────────────────────────────────────

fn expand_latin_abbreviations(text: &str) -> String {
    static EG_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\be\.g\.\s*").unwrap());
    static IE_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\bi\.e\.\s*").unwrap());
    static ETC_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\betc\.\s*").unwrap());
    static VS_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\bvs\.\s*").unwrap());

    let result = EG_REGEX.replace_all(text, "for example ").to_string();
    let result = IE_REGEX.replace_all(&result, "that is ").to_string();
    let result = ETC_REGEX.replace_all(&result, "et cetera ").to_string();
    VS_REGEX.replace_all(&result, "versus ").to_string()
}

// ── 7. Title Abbreviations ────────────────────────────────────────────────────

fn expand_title_abbreviations(text: &str) -> String {
    static DR_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\bDr\.\s*").unwrap());
    static MR_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\bMr\.\s*").unwrap());
    static MRS_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\bMrs\.\s*").unwrap());
    static MS_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\bMs\.\s*").unwrap());
    static PROF_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\bProf\.\s*").unwrap());
    static REV_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\bRev\.\s*").unwrap());
    static SR_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\bSr\.\s*").unwrap());
    static JR_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\bJr\.\s*").unwrap());
    static HON_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\bHon\.\s*").unwrap());

    let result = DR_REGEX.replace_all(text, "Doctor ").to_string();
    let result = MR_REGEX.replace_all(&result, "Mister ").to_string();
    let result = MRS_REGEX.replace_all(&result, "Misses ").to_string();
    let result = MS_REGEX.replace_all(&result, "Miss ").to_string();
    let result = PROF_REGEX.replace_all(&result, "Professor ").to_string();
    let result = REV_REGEX.replace_all(&result, "Reverend ").to_string();
    let result = SR_REGEX.replace_all(&result, "Senior ").to_string();
    let result = JR_REGEX.replace_all(&result, "Junior ").to_string();
    HON_REGEX.replace_all(&result, "Honorable ").to_string()
}

// ── 8. Number Suffixes (Magnitude) ──────────────────────────────────────────

fn expand_number_suffixes(text: &str) -> String {
    static BN_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)(\d+(?:\.\d+)?)\s*bn\b").unwrap());
    static BILLION_B_REGEX: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(\d+(?:\.\d+)?)\s*B\b").unwrap());
    static MILLION_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(\d+(?:\.\d+)?)\s*m\b").unwrap());
    static MILLION_M_REGEX: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(\d+(?:\.\d+)?)\s*M\b").unwrap());
    static K_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)(\d+(?:\.\d+)?)\s*k\b").unwrap());
    static TR_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)(\d+(?:\.\d+)?)\s*tr\b").unwrap());

    // Order matters: longer patterns first
    let result = BN_REGEX.replace_all(text, "$1 billion").to_string();
    let result = BILLION_B_REGEX
        .replace_all(&result, "$1 billion")
        .to_string();
    let result = TR_REGEX.replace_all(&result, "$1 trillion").to_string();
    let result = MILLION_REGEX.replace_all(&result, "$1 million").to_string();
    let result = MILLION_M_REGEX
        .replace_all(&result, "$1 million")
        .to_string();
    K_REGEX.replace_all(&result, "$1 thousand").to_string()
}

// ── 9. Metric Units ────────────────────────────────────────────────────────

fn expand_metric_units(text: &str) -> String {
    static MM_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(\d)\s*mm\b").unwrap());
    static CM_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(\d)\s*cm\b").unwrap());
    static KM_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(\d)\s*km\b").unwrap());
    static KG_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(\d)\s*kg\b").unwrap());
    static G_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(\d)\s*g\b").unwrap());
    // Note: 'm' for meters is handled after expand_number_suffixes (which claims
    // 'm' for million); only a leftover standalone 'm' becomes meters.
    static M_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(\d)\s*m\b").unwrap());

    // Order: longer unit abbreviations first to avoid partial matches
    let result = MM_REGEX.replace_all(text, "$1 millimeters").to_string();
    let result = CM_REGEX.replace_all(&result, "$1 centimeters").to_string();
    let result = KM_REGEX.replace_all(&result, "$1 kilometers").to_string();
    let result = KG_REGEX.replace_all(&result, "$1 kilograms").to_string();
    let result = G_REGEX.replace_all(&result, "$1 grams").to_string();
    M_REGEX.replace_all(&result, "$1 meters").to_string()
}

// ── 10. Symbols ──────────────────────────────────────────────────────────────

fn expand_symbols(text: &str) -> String {
    // @ only inside email addresses or @handles
    static AT_EMAIL_REGEX: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"([a-zA-Z0-9._%+-])@([a-zA-Z0-9.-]+\.[a-zA-Z]{2,})").unwrap());
    static AT_HANDLE_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"@([a-zA-Z0-9_]+)").unwrap());
    // Currency symbols: $50 → 50 dollars, €20 → 20 euros, £15 → 15 pounds, ¥1000 → 1000 yen
    static DOLLAR_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"\$(\d+(?:\.\d+)?)\b").unwrap());
    static EURO_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"€(\d+(?:\.\d+)?)\b").unwrap());
    static POUND_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"£(\d+(?:\.\d+)?)\b").unwrap());
    static YEN_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"¥(\d+(?:\.\d+)?)\b").unwrap());

    let mut result = text.to_string();

    // Handle @ in emails and handles first (before generic symbol replacement)
    result = AT_EMAIL_REGEX.replace_all(&result, "$1 at $2").to_string();
    result = AT_HANDLE_REGEX.replace_all(&result, "at $1").to_string();

    // Handle currency symbols before other symbol replacements
    result = DOLLAR_REGEX.replace_all(&result, "$1 dollars").to_string();
    result = EURO_REGEX.replace_all(&result, "$1 euros").to_string();
    result = POUND_REGEX.replace_all(&result, "$1 pounds").to_string();
    result = YEN_REGEX.replace_all(&result, "$1 yen").to_string();

    // Simple character replacements
    result = result.replace('&', " and ");
    result = result.replace('%', " percent");
    result = result.replace('~', "approximately ");
    result = result.replace('+', " plus ");
    result = result.replace('=', " equals ");
    result = result.replace('°', " degrees");

    result
}

// ── 11. Punctuation Normalization ───────────────────────────────────────────

fn normalize_punctuation(text: &str) -> String {
    // Normalize various ellipsis forms to standard "..."
    static ELLIPSIS_UNICODE_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"\u{2026}").unwrap());
    static ELLIPSIS_SPACED_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"\.\s*\.\s*\.").unwrap());
    // Em-dash → comma
    static EM_DASH_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"\u{2014}").unwrap());
    // Parenthesized text → comma-delimited
    static PAREN_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"\(([^)]+)\)").unwrap());

    let result = ELLIPSIS_UNICODE_REGEX.replace_all(text, "...").to_string();
    let result = ELLIPSIS_SPACED_REGEX
        .replace_all(&result, "...")
        .to_string();
    let result = EM_DASH_REGEX.replace_all(&result, ", ").to_string();
    PAREN_REGEX.replace_all(&result, ", $1,").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remove_emojis() {
        assert_eq!(remove_emojis("Hello 😀 world"), "Hello  world");
        assert_eq!(remove_emojis("🎉🎊 Party time"), " Party time");
        assert_eq!(remove_emojis("No emojis here"), "No emojis here");
    }

    #[test]
    fn test_sanitize_tts_strips_newlines() {
        assert_eq!(sanitize_tts("line one\nline two"), "line one line two");
        assert_eq!(sanitize_tts("a\r\nb"), "a b");
    }

    #[test]
    fn test_sanitize_tts_urls() {
        assert_eq!(
            remove_urls("Visit https://example.com for info"),
            "Visit  for info"
        );
    }

    #[test]
    fn test_sanitize_tts_citations() {
        assert_eq!(
            remove_citations("According to [1] and [12]"),
            "According to  and "
        );
    }

    #[test]
    fn test_sanitize_tts_slash_lookup() {
        assert_eq!(expand_slash_lookups("w/o any issue"), "without any issue");
        assert_eq!(expand_slash_lookups("w/ sugar"), "with sugar");
        assert_eq!(expand_slash_lookups("n/a"), "not applicable");
        assert_eq!(expand_slash_lookups("and/or both"), "and or both");
    }

    #[test]
    fn test_sanitize_tts_slash_option() {
        assert_eq!(
            expand_slash_options("true/false value"),
            "true or false value"
        );
    }

    #[test]
    fn test_sanitize_tts_slash_ratio() {
        assert_eq!(expand_slash_ratios("100 km/h speed"), "100 km per h speed");
    }

    #[test]
    fn test_sanitize_tts_latin_abbreviations() {
        assert_eq!(
            expand_latin_abbreviations("e.g. this one"),
            "for example this one"
        );
        assert_eq!(expand_latin_abbreviations("A vs. B"), "A versus B");
    }

    #[test]
    fn test_sanitize_tts_title_abbreviations() {
        assert_eq!(expand_title_abbreviations("Dr. Smith"), "Doctor Smith");
        assert_eq!(expand_title_abbreviations("Mr. Johnson"), "Mister Johnson");
        assert_eq!(expand_title_abbreviations("Prof. Brown"), "Professor Brown");
    }

    #[test]
    fn test_sanitize_tts_metric_units() {
        assert_eq!(expand_metric_units("10mm gap"), "10 millimeters gap");
        assert_eq!(expand_metric_units("5 cm wide"), "5 centimeters wide");
        assert_eq!(expand_metric_units("3km away"), "3 kilometers away");
        // Should NOT replace 'm' in regular words
        assert_eq!(expand_metric_units("maximum"), "maximum");
    }

    #[test]
    fn test_sanitize_tts_number_suffixes() {
        assert_eq!(expand_number_suffixes("2bn revenue"), "2 billion revenue");
        assert_eq!(expand_number_suffixes("5m users"), "5 million users");
        assert_eq!(
            expand_number_suffixes("3k subscribers"),
            "3 thousand subscribers"
        );
        assert_eq!(
            expand_number_suffixes("1tr market cap"),
            "1 trillion market cap"
        );
        assert_eq!(expand_number_suffixes("maximum"), "maximum");
    }

    #[test]
    fn test_number_suffixes_before_metric_units() {
        let result = sanitize_tts("The company has 5m users");
        assert!(
            result.contains("million"),
            "Expected 'million' in: {}",
            result
        );

        let result = sanitize_tts("The distance is 5km");
        assert!(
            result.contains("kilometers"),
            "Expected 'kilometers' in: {}",
            result
        );
    }

    #[test]
    fn test_sanitize_tts_symbols() {
        assert!(expand_symbols("A & B").contains("and"));
        assert!(expand_symbols("50%").contains("percent"));
        assert!(expand_symbols("~100").contains("approximately"));
        assert!(expand_symbols("90°").contains("degrees"));
    }

    #[test]
    fn test_sanitize_tts_currency_symbols() {
        assert_eq!(expand_symbols("$50"), "50 dollars");
        assert_eq!(expand_symbols("€20"), "20 euros");
        assert_eq!(expand_symbols("£15"), "15 pounds");
        assert_eq!(expand_symbols("¥1000"), "1000 yen");
        assert_eq!(expand_symbols("$19.99"), "19.99 dollars");
    }

    #[test]
    fn test_sanitize_tts_symbols_at() {
        assert_eq!(
            expand_symbols("email user@example.com here"),
            "email user at example.com here"
        );
        assert_eq!(expand_symbols("@username"), "at username");
    }

    #[test]
    fn test_sanitize_tts_punctuation() {
        assert_eq!(normalize_punctuation("wait\u{2026}"), "wait...");
        assert_eq!(
            normalize_punctuation("word\u{2014}another"),
            "word, another"
        );
        assert_eq!(
            normalize_punctuation("text (aside) more"),
            "text , aside, more"
        );
    }

    #[test]
    fn test_sanitize_tts_combined() {
        let input = "According to [1], the speed is ~100 km/h (e.g. on highways) & drivers should check https://traffic.info for updates etc.";
        let result = sanitize_tts(input);
        assert!(!result.contains("https://"));
        assert!(!result.contains("[1]"));
        assert!(result.contains("and"));
        assert!(result.contains("approximately"));
        assert!(result.contains("for example"));
        assert!(result.contains("et cetera"));
    }
}
