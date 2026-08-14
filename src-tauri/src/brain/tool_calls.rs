//! Built-in tool calling for the Brain (MCP-lite, fully offline).
//!
//! Local models are prompted to emit tool invocations inside `<code>…</code>`
//! blocks (speech-to-speech's `tool_prompt.py` pattern). Blocks are parsed
//! from streamed sentences, validated against a fixed registry, executed
//! locally, and their results are fed back to the model in a follow-up turn
//! so the assistant can speak the answer.

use serde_json::{Map, Value};
use tauri::Emitter;
use tauri_plugin_clipboard_manager::ClipboardExt;

/// One parsed, schema-validated tool invocation.
#[derive(Debug, Clone)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: Map<String, Value>,
}

/// Description of one built-in tool for prompt rendering.
struct ToolSpec {
    name: &'static str,
    description: &'static str,
    params: &'static [(&'static str, &'static str)], // (name, description)
}

const TOOLS: &[ToolSpec] = &[
    ToolSpec {
        name: "get_current_time",
        description: "Return the current local date and time.",
        params: &[],
    },
    ToolSpec {
        name: "read_clipboard",
        description: "Return the text currently on the system clipboard.",
        params: &[],
    },
    ToolSpec {
        name: "copy_to_clipboard",
        description: "Write text to the system clipboard.",
        params: &[("text", "The text to copy to the clipboard.")],
    },
];

fn find_tool(name: &str) -> Option<&'static ToolSpec> {
    TOOLS.iter().find(|t| t.name == name)
}

/// Render the tools section appended to the system prompt when enabled.
pub fn tools_prompt_section() -> String {
    let mut section = String::from(
        "You can use these tools. To call a tool, output ONLY a code block with the tool name and a JSON object of arguments, e.g.:\n<code>get_current_time {}</code>\nDo not put anything else in the block, do not invent results — after emitting a tool call, stop and wait for the tool result.\n\nAvailable tools:\n",
    );
    for tool in TOOLS {
        section.push_str(&format!("- {}: {}", tool.name, tool.description));
        if !tool.params.is_empty() {
            section.push_str(" Arguments: ");
            let args: Vec<String> = tool
                .params
                .iter()
                .map(|(name, desc)| format!("{} ({desc})", *name))
                .collect();
            section.push_str(&args.join(", "));
        }
        section.push('\n');
    }
    section
}

/// Parse one `<code>` block body into a validated call.
/// Accepts `name {"json": ...}` (preferred) and bare `name` (no arguments).
pub fn parse_call(body: &str) -> Option<FunctionCall> {
    let body = body.trim();
    let split = body
        .char_indices()
        .find(|(_, c)| c.is_whitespace() || *c == '{' || *c == '(')
        .map(|(i, _)| i)
        .unwrap_or(body.len());
    let name = &body[..split];
    let spec = find_tool(name)?;

    let mut arguments = Map::new();
    let rest = body[split..].trim();
    if !rest.is_empty() {
        let json = rest
            .strip_prefix('(')
            .map(|r| r.strip_suffix(')').unwrap_or(r))
            .unwrap_or(rest);
        let parsed: Value = serde_json::from_str(json).ok()?;
        if let Value::Object(map) = parsed {
            for (key, value) in map {
                if spec.params.iter().any(|(p, _)| p == &key) {
                    arguments.insert(key, value);
                }
                // Unknown arguments are dropped (schema validation).
            }
        }
    }
    // Required params must be present.
    for (param, _) in spec.params {
        if !arguments.contains_key(*param) {
            return None;
        }
    }
    Some(FunctionCall {
        name: name.to_string(),
        arguments,
    })
}

/// Scan a sentence for complete `<code>…</code>` tool blocks.
/// Returns the sentence with complete blocks removed and the parsed calls.
/// Incomplete blocks (no closing tag yet) are left in the text so a block
/// split across sentences survives until its closing tag arrives.
pub fn scan_code_blocks(text: &str) -> (String, Vec<FunctionCall>) {
    let mut calls = Vec::new();
    let mut cleaned = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("<code>") {
        cleaned.push_str(&rest[..start]);
        let after_open = &rest[start + "<code>".len()..];
        match after_open.find("</code>") {
            Some(end) => {
                let body = &after_open[..end];
                if let Some(call) = parse_call(body) {
                    calls.push(call);
                } else {
                    // Not a tool call — keep the original block in the text.
                    cleaned.push_str(&rest[start..start + "<code>".len() + end + "</code>".len()]);
                }
                rest = &after_open[end + "</code>".len()..];
            }
            None => {
                // Incomplete block: keep it as-is for the next sentence.
                cleaned.push_str(&rest[start..]);
                rest = "";
                break;
            }
        }
    }
    cleaned.push_str(rest);
    let cleaned = cleaned.trim().to_string();
    // Collapse doubled spaces left by removed blocks ("word <code>…</code> next"
    // → "word  next" → "word next").
    let mut cleaned = cleaned.replace("  ", " ");
    while cleaned.contains("  ") {
        cleaned = cleaned.replace("  ", " ");
    }
    (cleaned, calls)
}

/// Execute a validated call locally and return a human-readable result.
pub fn execute(app: &tauri::AppHandle, call: &FunctionCall) -> String {
    match call.name.as_str() {
        "get_current_time" => chrono::Local::now()
            .format("%A, %B %d, %Y at %H:%M:%S")
            .to_string(),
        "read_clipboard" => app
            .clipboard()
            .read_text()
            .unwrap_or_default()
            .trim()
            .to_string(),
        "copy_to_clipboard" => {
            let text = call
                .arguments
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            match app.clipboard().write_text(text.to_string()) {
                Ok(()) => format!(
                    "Copied to clipboard ({len} characters)",
                    len = text.chars().count()
                ),
                Err(e) => format!("Clipboard write failed: {e}"),
            }
        }
        other => format!("Unknown tool: {other}"),
    }
}

/// Streaming interceptor: swallows complete `<code>…</code>` tool blocks from
/// the token stream (so they never reach the UI or TTS), executes them as they
/// close, and passes everything else through unchanged. Handles blocks and
/// even the `<code>` opener split across arbitrary token boundaries.
#[derive(Default)]
pub struct ToolIntercept {
    inside: bool,
    buf: String,
    /// Suffix of the previous token that may be a partial `<code>` opener.
    open_prefix: String,
    /// Suffix of the previous token that may be a partial `</code>` closer.
    close_prefix: String,
}

const OPEN_TAG: &str = "<code>";
const CLOSE_TAG: &str = "</code>";

impl ToolIntercept {
    /// Pure token-level interception: returns the text to emit and the bodies
    /// of any blocks completed by this token (for parsing/execution).
    pub fn feed_text(&mut self, token: &str) -> (String, Vec<String>) {
        let mut out = String::with_capacity(token.len() + self.open_prefix.len());
        let mut completed = Vec::new();
        // Re-assemble any held partial tag with the new token.
        let work = if self.inside {
            let mut w = std::mem::take(&mut self.close_prefix);
            w.push_str(token);
            w
        } else {
            let mut w = std::mem::take(&mut self.open_prefix);
            w.push_str(token);
            w
        };
        let mut idx = 0usize;
        while idx < work.len() {
            if !self.inside {
                match work[idx..].find(OPEN_TAG) {
                    Some(pos) => {
                        out.push_str(&work[idx..idx + pos]);
                        self.inside = true;
                        self.buf.clear();
                        idx += pos + OPEN_TAG.len();
                    }
                    None => {
                        // The token may end with a partial "<code>" opener —
                        // hold the longest matching suffix for the next token.
                        let rest = &work[idx..];
                        let mut held = 0usize;
                        for len in (1..=OPEN_TAG.len()).rev() {
                            if rest.ends_with(&OPEN_TAG[..len]) {
                                held = len;
                                break;
                            }
                        }
                        if held > 0 {
                            out.push_str(&rest[..rest.len() - held]);
                            self.open_prefix = rest[rest.len() - held..].to_string();
                        } else {
                            out.push_str(rest);
                        }
                        break;
                    }
                }
            } else {
                match work[idx..].find(CLOSE_TAG) {
                    Some(pos) => {
                        self.buf.push_str(&work[idx..idx + pos]);
                        completed.push(std::mem::take(&mut self.buf));
                        self.inside = false;
                        idx += pos + CLOSE_TAG.len();
                    }
                    None => {
                        self.buf.push_str(&work[idx..]);
                        // Hold back a partial "</code>" closer for the next
                        // token so the block can still complete.
                        let mut held = 0usize;
                        for len in (1..=CLOSE_TAG.len()).rev() {
                            if self.buf.ends_with(&CLOSE_TAG[..len]) {
                                held = len;
                                break;
                            }
                        }
                        if held > 0 {
                            let split = self.buf.len() - held;
                            self.close_prefix = self.buf[split..].to_string();
                            self.buf.truncate(split);
                        }
                        break;
                    }
                }
            }
        }
        (out, completed)
    }

    /// Feed one streamed token: swallow complete tool blocks (executing them
    /// and recording `"tool → result"` strings) and emit the rest.
    pub fn feed(
        &mut self,
        token: &str,
        app: &tauri::AppHandle,
        results: &mut Vec<String>,
    ) -> String {
        let (out, completed) = self.feed_text(token);
        for body in completed {
            if let Some(call) = parse_call(&body) {
                let result = execute(app, &call);
                let _ = app.emit(
                    "brain:tool-call",
                    serde_json::json!({ "name": call.name, "result": result }),
                );
                results.push(format!("{} → {}", call.name, result));
            }
        }
        out
    }

    /// Flush any unterminated block as plain text (stream ended mid-block).
    pub fn flush(&mut self) -> String {
        if self.inside {
            self.inside = false;
            return format!(
                "<code>{}{}",
                std::mem::take(&mut self.buf),
                std::mem::take(&mut self.close_prefix)
            );
        }
        if !self.open_prefix.is_empty() {
            return std::mem::take(&mut self.open_prefix);
        }
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_and_parses_complete_blocks() {
        let (cleaned, calls) =
            scan_code_blocks("Sure, checking. <code>get_current_time {}</code> Done.");
        assert_eq!(cleaned, "Sure, checking. Done.");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "get_current_time");
    }

    #[test]
    fn incomplete_block_survives_in_text() {
        let (cleaned, calls) = scan_code_blocks("Wait <code>get_current_time");
        assert_eq!(cleaned, "Wait <code>get_current_time");
        assert!(calls.is_empty());
    }

    #[test]
    fn unknown_tool_blocks_are_kept_verbatim() {
        let (cleaned, calls) = scan_code_blocks("<code>not_a_tool {}</code> tail");
        assert!(cleaned.contains("<code>not_a_tool {}</code>"));
        assert!(calls.is_empty());
    }

    #[test]
    fn validates_required_arguments() {
        assert!(parse_call("copy_to_clipboard {}").is_none());
        let call = parse_call(r#"copy_to_clipboard {"text": "hi"}"#).unwrap();
        assert_eq!(
            call.arguments.get("text").and_then(|v| v.as_str()),
            Some("hi")
        );
    }

    #[test]
    fn bare_call_without_arguments_is_valid_for_parameterless_tools() {
        assert!(parse_call("get_current_time").is_some());
        assert!(parse_call("get_current_time {}").is_some());
    }

    #[test]
    fn intercept_handles_block_split_across_tokens() {
        let mut intercept = ToolIntercept::default();
        let (out1, done1) = intercept.feed_text("Here is the answer: <cod");
        assert_eq!(out1, "Here is the answer: ");
        assert!(done1.is_empty());
        // The partial opener is held back, not passed through.
        assert_eq!(intercept.open_prefix, "<cod");

        let (out2, done2) = intercept.feed_text("e>get_current_time {}</code> Done.");
        assert_eq!(out2, " Done.");
        assert_eq!(done2, vec!["get_current_time {}".to_string()]);
        assert!(!intercept.inside);
    }

    #[test]
    fn intercept_handles_opener_split_into_many_tokens() {
        let mut intercept = ToolIntercept::default();
        let (out1, _) = intercept.feed_text("<co");
        assert_eq!(out1, "");
        let (out2, _) = intercept.feed_text("de>get_current_time {}</cod");
        assert_eq!(out2, "");
        assert!(intercept.inside);
        let (out3, done3) = intercept.feed_text("e> done");
        assert_eq!(out3, " done");
        assert_eq!(done3.len(), 1);
    }

    #[test]
    fn intercept_passes_plain_text_through() {
        let mut intercept = ToolIntercept::default();
        let (out, done) = intercept.feed_text("No tools here.");
        assert_eq!(out, "No tools here.");
        assert!(done.is_empty());
    }

    #[test]
    fn flush_recovers_unterminated_block() {
        let mut intercept = ToolIntercept::default();
        let (out, _) = intercept.feed_text("<code>get_current");
        assert_eq!(out, "");
        assert_eq!(intercept.flush(), "<code>get_current");
    }
}
