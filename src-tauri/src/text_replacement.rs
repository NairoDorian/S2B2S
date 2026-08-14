//! User-defined text replacement rules, applied after STT (and ITN) to the
//! final transcription before it reaches post-processing or the Brain.
//!
//! Rules support escape sequences (`\n`, `\r\n`, `\r`, `\t`, `\\`, `\u{...}`),
//! case-sensitivity, and regex mode. Non-regex rules are case-insensitive by
//! default (like text expanders: "omw" → "on my way" regardless of casing).

use crate::settings::{get_settings, write_settings, TextReplacement};
use tauri::AppHandle;

/// Expand the user-facing escape sequences in a rule's `from`/`to` strings.
pub fn unescape(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len());
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] != '\\' {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        i += 1;
        if i >= chars.len() {
            break;
        }
        match chars[i] {
            'n' => {
                out.push('\n');
                i += 1;
            }
            'r' => {
                // `\r\n` (backslash-r backslash-n) is a common way to write a
                // newline — collapse it to one.
                if i + 2 < chars.len() && chars[i + 1] == '\\' && chars[i + 2] == 'n' {
                    out.push('\n');
                    i += 3;
                } else {
                    out.push('\r');
                    i += 1;
                }
            }
            't' => {
                out.push('\t');
                i += 1;
            }
            '\\' => {
                out.push('\\');
                i += 1;
            }
            'u' if i + 1 < chars.len() && chars[i + 1] == '{' => {
                i += 2;
                let mut hex = String::new();
                while i < chars.len() && chars[i] != '}' {
                    hex.push(chars[i]);
                    i += 1;
                }
                if i < chars.len() {
                    i += 1; // skip '}'
                }
                if let Ok(codepoint) = u32::from_str_radix(&hex, 16) {
                    if let Some(c) = char::from_u32(codepoint) {
                        out.push(c);
                    }
                }
            }
            other => {
                // Unknown escape: keep both the backslash and the character
                // (regex patterns rely on sequences like \d staying intact).
                out.push('\\');
                out.push(other);
                i += 1;
            }
        }
    }
    out
}

/// Apply all enabled rules to `text`. Replacement strings are always treated
/// literally (no `$1` group expansion).
pub fn apply_replacements(text: &str, rules: &[TextReplacement]) -> String {
    let mut result = text.to_string();
    for rule in rules {
        if !rule.enabled || rule.from.is_empty() {
            continue;
        }
        let from = unescape(&rule.from);
        let to = unescape(&rule.to);

        let pattern = if rule.is_regex {
            if rule.case_sensitive {
                from
            } else {
                format!("(?i){from}")
            }
        } else if rule.case_sensitive {
            regex::escape(&from)
        } else {
            format!("(?i){}", regex::escape(&from))
        };

        match regex::Regex::new(&pattern) {
            Ok(re) => {
                result = re.replace_all(&result, regex::NoExpand(&to)).into_owned();
            }
            Err(e) => {
                log::warn!("[TextReplacement] invalid pattern '{pattern}': {e}");
            }
        }
    }
    result
}

/// Apply the replacement rules from the current settings.
pub fn apply_replacements_from_settings(app: &AppHandle, text: &str) -> String {
    apply_replacements(text, &get_settings(app).text_replacements)
}

/// Upsert a rule (empty id generates one). Returns the saved list.
#[tauri::command]
#[specta::specta]
pub fn save_text_replacement(
    app: AppHandle,
    rule: TextReplacement,
) -> Result<Vec<TextReplacement>, String> {
    let mut settings = get_settings(&app);
    let mut rule = rule;
    if rule.id.trim().is_empty() {
        rule.id = format!(
            "rule-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        );
    }
    if rule.from.trim().is_empty() {
        return Err("The 'from' text is required".to_string());
    }
    if let Some(existing) = settings
        .text_replacements
        .iter_mut()
        .find(|r| r.id == rule.id)
    {
        *existing = rule;
    } else {
        settings.text_replacements.push(rule);
    }
    write_settings(&app, settings.clone());
    Ok(settings.text_replacements)
}

/// Delete a rule. Returns the saved list.
#[tauri::command]
#[specta::specta]
pub fn delete_text_replacement(
    app: AppHandle,
    rule_id: String,
) -> Result<Vec<TextReplacement>, String> {
    let mut settings = get_settings(&app);
    settings.text_replacements.retain(|r| r.id != rule_id);
    write_settings(&app, settings.clone());
    Ok(settings.text_replacements)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(from: &str, to: &str, case_sensitive: bool, is_regex: bool) -> TextReplacement {
        TextReplacement {
            id: "t".to_string(),
            from: from.to_string(),
            to: to.to_string(),
            enabled: true,
            case_sensitive,
            is_regex,
        }
    }

    #[test]
    fn unescape_expands_sequences() {
        assert_eq!(unescape(r"a\nb"), "a\nb");
        assert_eq!(unescape(r"a\r\nb"), "a\nb");
        assert_eq!(unescape(r"a\tb"), "a\tb");
        assert_eq!(unescape(r"a\\b"), "a\\b");
        assert_eq!(unescape(r"\u{20AC}"), "€");
        assert_eq!(unescape("plain"), "plain");
    }

    #[test]
    fn plain_rule_is_case_insensitive_by_default() {
        let text = apply_replacements(
            "I said OMW and omw",
            &[rule("omw", "on my way", false, false)],
        );
        assert_eq!(text, "I said on my way and on my way");
    }

    #[test]
    fn case_sensitive_rule_only_matches_exact_case() {
        let text = apply_replacements("OMW omw", &[rule("omw", "on my way", true, false)]);
        assert_eq!(text, "OMW on my way");
    }

    #[test]
    fn regex_rule_matches_patterns() {
        let text = apply_replacements(
            "call 555-1234 please",
            &[rule(r"(\d{3})-(\d{4})", "redacted", true, true)],
        );
        assert_eq!(text, "call redacted please");
    }

    #[test]
    fn replacement_string_is_literal() {
        let text = apply_replacements("cost $5", &[rule("cost", "$1", true, false)]);
        assert_eq!(text, "$1 $5");
    }

    #[test]
    fn disabled_or_empty_rules_are_skipped() {
        let mut r = rule("a", "b", false, false);
        r.enabled = false;
        assert_eq!(apply_replacements("a", &[r]), "a");
        let r2 = rule("", "b", false, false);
        assert_eq!(apply_replacements("a", &[r2]), "a");
    }
}
