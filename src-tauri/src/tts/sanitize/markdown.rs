// Pass 1: Markdown syntax stripping via regex.

use once_cell::sync::Lazy;
use regex::Regex;

/// Strip all markdown syntax from text.
pub(super) fn strip_markdown(text: &str) -> String {
    let mut result = text.to_string();
    result = strip_code_blocks(&result);
    result = strip_inline_code(&result);
    result = strip_tables(&result);
    result = strip_links(&result);
    result = strip_headers(&result);
    result = strip_bold_italic(&result);
    result = strip_lists(&result);
    result = strip_blockquotes(&result);
    result
}

fn strip_code_blocks(text: &str) -> String {
    static CODE_BLOCK_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"```[\s\S]*?```").unwrap());
    CODE_BLOCK_REGEX.replace_all(text, "").to_string()
}

fn strip_inline_code(text: &str) -> String {
    static INLINE_CODE_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"`[^`]+`").unwrap());
    INLINE_CODE_REGEX.replace_all(text, "").to_string()
}

fn strip_links(text: &str) -> String {
    static LINK_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"\[([^\]]+)\]\([^)]+\)").unwrap());
    LINK_REGEX.replace_all(text, "$1").to_string()
}

fn strip_headers(text: &str) -> String {
    static HEADER_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^(#{1,6})\s+(.*)$").unwrap());
    HEADER_REGEX
        .replace_all(text, |caps: &regex::Captures| {
            let content = caps.get(2).map_or("", |m| m.as_str());
            if content.ends_with(['.', '?', '!', ':', ';']) {
                content.to_string()
            } else {
                format!("{}.", content)
            }
        })
        .to_string()
}

fn strip_bold_italic(text: &str) -> String {
    // Strip emphasis markers but PRESERVE intra-word '*' and '_' so identifiers like
    // `snake_case`, `__dunder__`, and expressions like `a * b` are not mangled.
    // (The previous version did a blunt `replace(['*','_'], "")`, which corrupted them.)
    static BOLD_STAR: Lazy<Regex> = Lazy::new(|| Regex::new(r"\*\*([^\n*]+)\*\*").unwrap());
    // Italic asterisks must hug non-space content (CommonMark: `* x *` is not emphasis),
    // which also leaves bare math like `2 * 3` untouched.
    static ITALIC_STAR: Lazy<Regex> = Lazy::new(|| Regex::new(r"\*(\S(?:[^\n*]*\S)?)\*").unwrap());
    // Underscore emphasis only at word boundaries — '\b' never fires inside snake_case
    // because '_' is itself a word character.
    static BOLD_UNDER: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b__([^\n_]+)__\b").unwrap());
    static ITALIC_UNDER: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b_([^\n_]+)_\b").unwrap());

    let result = BOLD_STAR.replace_all(text, "$1").to_string();
    let result = ITALIC_STAR.replace_all(&result, "$1").to_string();
    let result = BOLD_UNDER.replace_all(&result, "$1").to_string();
    ITALIC_UNDER.replace_all(&result, "$1").to_string()
}

/// Convert GitHub-flavored markdown tables into comma-separated speech.
/// Each row's cells are joined with ", " and the `|---|---|` separator/alignment
/// rows are dropped. Lines that aren't table rows pass through unchanged.
fn strip_tables(text: &str) -> String {
    text.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            // A table row starts/ends with '|' or has at least two cell separators.
            let is_row = trimmed.starts_with('|') || trimmed.matches('|').count() >= 2;
            if !is_row {
                return Some(line.to_string());
            }
            // Drop alignment rows like |---|:--:| (only pipes, dashes, colons, spaces).
            if trimmed.contains('-') && trimmed.chars().all(|c| matches!(c, '|' | '-' | ':' | ' '))
            {
                return None;
            }
            let cells: Vec<&str> = trimmed
                .trim_matches('|')
                .split('|')
                .map(str::trim)
                .filter(|c| !c.is_empty())
                .collect();
            if cells.is_empty() {
                return Some(String::new());
            }
            let mut row = cells.join(", ");
            if !row.ends_with(['.', '?', '!', ':', ';', ',']) {
                row.push('.');
            }
            Some(row)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn strip_lists(text: &str) -> String {
    static LIST_REGEX: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?m)^[ \t]*[-*+]\s+(.+)$|^[ \t]*\d+\.\s+(.+)$").unwrap());
    LIST_REGEX
        .replace_all(text, |caps: &regex::Captures| {
            let content = caps
                .get(1)
                .or_else(|| caps.get(2))
                .map_or("", |m| m.as_str())
                .trim();

            if content.is_empty() {
                return String::new();
            }

            if content.ends_with(['.', '?', '!', ':', ';']) {
                content.to_string()
            } else {
                format!("{}.", content)
            }
        })
        .to_string()
}

fn strip_blockquotes(text: &str) -> String {
    static BLOCKQUOTE_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^[ \t]*>\s+").unwrap());
    BLOCKQUOTE_REGEX.replace_all(text, "").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_headers() {
        assert_eq!(strip_headers("# Header"), "Header.");
        assert_eq!(strip_headers("## Header"), "Header.");
        assert_eq!(strip_headers("Normal text"), "Normal text");
        // Should not double-up existing sentence-ending punctuation
        assert_eq!(strip_headers("# Already done!"), "Already done!");
        assert_eq!(strip_headers("# Section:"), "Section:");
        assert_eq!(strip_headers("# Question?"), "Question?");
    }

    #[test]
    fn test_strip_code_blocks() {
        assert_eq!(strip_code_blocks("Text\n```code```\nMore"), "Text\n\nMore");
        assert_eq!(strip_code_blocks("```print()```"), "");
    }

    #[test]
    fn test_strip_inline_code() {
        assert_eq!(strip_inline_code("Use `sudo` command"), "Use  command");
        assert_eq!(strip_inline_code("No code here"), "No code here");
    }

    #[test]
    fn test_strip_links() {
        assert_eq!(
            strip_links("Visit [Google](https://google.com)"),
            "Visit Google"
        );
        assert_eq!(strip_links("[Link](url)"), "Link");
    }

    #[test]
    fn test_strip_bold_italic() {
        assert_eq!(strip_bold_italic("**bold**"), "bold");
        assert_eq!(strip_bold_italic("*italic*"), "italic");
        assert_eq!(strip_bold_italic("__bold__"), "bold");
        assert_eq!(strip_bold_italic("_italic_"), "italic");
    }

    #[test]
    fn test_strip_bold_italic_preserves_identifiers() {
        // snake_case and intra-word markers must survive emphasis stripping.
        assert_eq!(
            strip_bold_italic("call get_user_name() now"),
            "call get_user_name() now"
        );
        assert_eq!(
            strip_bold_italic("snake_case_id stays"),
            "snake_case_id stays"
        );
        assert_eq!(strip_bold_italic("a * b * c"), "a * b * c");
        assert_eq!(
            strip_bold_italic("**bold** and *italic* text"),
            "bold and italic text"
        );
        assert_eq!(strip_bold_italic("__dunder__ word"), "dunder word");
    }

    #[test]
    fn test_strip_tables() {
        let input = "| Name | Age |\n|------|-----|\n| Alice | 30 |\n| Bob | 25 |";
        assert_eq!(strip_tables(input), "Name, Age.\nAlice, 30.\nBob, 25.");
        // Non-table text is untouched.
        assert_eq!(strip_tables("just a sentence"), "just a sentence");
    }

    #[test]
    fn test_strip_lists_legend_example() {
        let input = "## Legend\n\n- **Added**: New features\n- **Changed**: Changes in existing functionality.\n- **Deprecated**: Soon-to-be removed features.\n- **Removed**: Removed features";
        let result = strip_markdown(input);
        assert_eq!(result, "Legend.\n\nAdded: New features.\nChanged: Changes in existing functionality.\nDeprecated: Soon-to-be removed features.\nRemoved: Removed features.");
    }

    #[test]
    fn test_strip_blockquotes() {
        assert_eq!(strip_blockquotes("> Quote"), "Quote");
        assert_eq!(strip_blockquotes("> Quote text"), "Quote text");
    }

    #[test]
    fn test_strip_markdown_all() {
        let input = "# Title\n**bold** and *italic*\n- item\n> quote\n`code`";
        let result = strip_markdown(input);
        assert!(!result.contains('#'));
        assert!(!result.contains("**"));
        assert!(!result.contains('`'));
        assert!(!result.contains('>'));
    }
}
