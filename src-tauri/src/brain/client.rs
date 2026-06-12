//! Streaming "Brain" LLM client.
//!
//! OpenAI-compatible `/chat/completions` with `stream: true`, parsed as SSE.
//! Adapted from the AgentZero prototype (MIT) and hardened: SSE lines that span
//! chunk boundaries are buffered correctly (the prototype split per-chunk and
//! could drop a token), and the sentence splitter follows the S2B2S 05-spec
//! (≥25-char terminal rule, ≥15-char newline rule, 220-char clause force-split,
//! abbreviation suppression, all char-boundary-safe).

use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Serialize)]
struct ChatCompletionRequest<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    stream: bool,
}

#[derive(Deserialize)]
struct Delta {
    content: Option<String>,
    timings: Option<ChunkTimings>,
}
#[derive(Deserialize)]
struct ChunkChoice {
    delta: Delta,
}
#[derive(Deserialize)]
struct CompletionChunk {
    choices: Vec<ChunkChoice>,
    usage: Option<ChunkUsage>,
}
#[derive(Deserialize)]
struct ChunkUsage {
    #[serde(rename = "predicted_tokens_per_second")]
    predicted_per_second: Option<f64>,
    #[serde(rename = "predicted_ms")]
    predicted_ms: Option<f64>,
}
#[derive(Deserialize)]
struct ChunkTimings {
    predicted_per_second: Option<f64>,
    predicted_ms: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct BrainTiming {
    pub tokens_per_second: Option<f64>,
    pub total_ms: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct BrainResult {
    pub text: String,
    pub timing: Option<BrainTiming>,
}

pub struct BrainClient {
    client: Client,
}

impl Default for BrainClient {
    fn default() -> Self {
        Self::new()
    }
}

impl BrainClient {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    /// Stream a chat completion over the full `messages` context (system + history + user).
    ///
    /// `on_token` fires for each token delta; `on_sentence` fires for each completed
    /// sentence (drives streaming TTS). Returns the full assistant text.
    ///
    /// `abort` is a per-request token: setting it stops the stream between chunks.
    /// Each request gets its own token so aborting one turn can never race with
    /// the next turn starting (barge-in).
    #[allow(clippy::too_many_arguments)]
    pub async fn stream_chat<FT, FS>(
        &self,
        base_url: &str,
        api_key: &str,
        model: &str,
        messages: &[ChatMessage],
        abort: Arc<AtomicBool>,
        mut on_token: FT,
        mut on_sentence: FS,
    ) -> Result<BrainResult, String>
    where
        FT: FnMut(&str) + Send,
        FS: FnMut(String) + Send,
    {
        let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
        let body = ChatCompletionRequest {
            model,
            messages,
            stream: true,
        };

        let mut req = self.client.post(&url).json(&body);
        if !api_key.trim().is_empty() {
            req = req.bearer_auth(api_key);
        }

        log::info!("[Brain] streaming from {url} (model {model})");
        let response = req
            .send()
            .await
            .map_err(|e| format!("Brain request failed: {e}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let err_text = response.text().await.unwrap_or_default();
            return Err(format!("Brain returned status {status}: {err_text}"));
        }

        let mut stream = response.bytes_stream();
        let mut splitter = SentenceSplitter::new(25);
        let mut full = String::new();
        // Buffer partial SSE lines that span chunk boundaries.
        let mut pending = String::new();
        let mut final_timing: Option<BrainTiming> = None;

        while let Some(chunk) = stream.next().await {
            if abort.load(Ordering::SeqCst) {
                log::info!("[Brain] stream aborted by user");
                return Ok(BrainResult { text: full, timing: final_timing });
            }
            let bytes = chunk.map_err(|e| format!("Brain stream error: {e}"))?;
            pending.push_str(&String::from_utf8_lossy(&bytes));

            while let Some(nl) = pending.find('\n') {
                let line: String = pending.drain(..=nl).collect();
                let line = line.trim();
                let Some(payload) = line.strip_prefix("data:") else {
                    continue;
                };
                let payload = payload.trim();
                if payload == "[DONE]" {
                    continue;
                }
                if let Ok(parsed) = serde_json::from_str::<CompletionChunk>(payload) {
                    // Check for timing info in usage or delta.timings
                    if let Some(usage) = &parsed.usage {
                        final_timing = Some(BrainTiming {
                            tokens_per_second: usage.predicted_per_second,
                            total_ms: usage.predicted_ms.map(|ms| ms as i64),
                        });
                    }
                    for choice in &parsed.choices {
                        if let Some(timings) = &choice.delta.timings {
                            final_timing = Some(BrainTiming {
                                tokens_per_second: timings.predicted_per_second,
                                total_ms: timings.predicted_ms.map(|ms| ms as i64),
                            });
                        }
                    }
                    if let Some(content) = parsed
                        .choices
                        .first()
                        .and_then(|c| c.delta.content.as_ref())
                    {
                        if !content.is_empty() {
                            full.push_str(content);
                            on_token(content);
                            for sentence in splitter.push(content) {
                                on_sentence(sentence);
                            }
                        }
                    }
                }
            }
        }

        if let Some(last) = splitter.flush() {
            on_sentence(last);
        }
        Ok(BrainResult { text: full, timing: final_timing })
    }
}

/// Streaming sentence splitter (char-boundary-safe).
pub struct SentenceSplitter {
    buffer: String,
    min_len: usize,
}

fn is_terminal(c: char) -> bool {
    matches!(c, '.' | '!' | '?' | '…' | '。' | '！' | '？')
}

/// Is the `.` at `dot_byte` part of a known abbreviation?
fn is_abbrev_before(buffer: &str, dot_byte: usize) -> bool {
    let word = buffer[..dot_byte]
        .split_whitespace()
        .last()
        .unwrap_or("")
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase();
    matches!(
        word.as_str(),
        "mr" | "mrs"
            | "ms"
            | "dr"
            | "prof"
            | "st"
            | "etc"
            | "eg"
            | "ie"
            | "vs"
            | "no"
            | "approx"
            | "jr"
            | "sr"
    )
}

/// Find a clause boundary (`, ; : —`) within `max_chars`, else a hard char cut.
/// Prefers strong boundaries (`.`) over weak (`,`) with a 10-char bonus window.
fn force_clause_boundary(buffer: &str, max_chars: usize) -> usize {
    let mut last_clause = None;
    let mut last_strong = None;
    let mut count = 0usize;
    let mut hard = buffer.len();
    for (idx, c) in buffer.char_indices() {
        count += 1;
        if matches!(c, ',' | ';' | ':' | '—') {
            last_clause = Some(idx + c.len_utf8());
        }
        if matches!(c, '.' | ')' | ']') {
            last_strong = Some(idx + c.len_utf8());
        }
        if count >= max_chars {
            hard = idx + c.len_utf8();
            break;
        }
    }
    // Prefer strong boundary within max_chars+10, else clause boundary, else hard cut
    if let Some(s) = last_strong {
        if s <= hard + 10 {
            return s;
        }
    }
    last_clause.unwrap_or(hard)
}

/// Split at the first clause boundary after `target_chars`, looking in [target/2 .. target*2].
/// Used for shorten-first-chunk: emit a short first sentence to reduce TTFA.
pub fn split_at_clause_boundary(text: &str, target_chars: usize) -> Option<usize> {
    let half = target_chars / 2;
    let double = target_chars * 2;
    let mut best_clause = None;
    let mut best_strong = None;
    let mut count = 0usize;

    for (idx, c) in text.char_indices() {
        if count < half {
            count += 1;
            continue;
        }
        count += 1;
        if count > double {
            break;
        }

        if matches!(c, ',' | ';' | ':' | '—') {
            best_clause = Some(idx + c.len_utf8());
        }
        if matches!(c, '.' | ')' | ']') {
            // Prefer terminal + ')' / ']' over clause with 10-char bonus
            if best_strong.is_none() {
                best_strong = Some(idx + c.len_utf8());
            }
        }
    }

    if let Some(s) = best_strong {
        if best_clause.is_none_or(|c| s <= c + 10) {
            return Some(s);
        }
    }
    best_clause
}

impl SentenceSplitter {
    pub fn new(min_len: usize) -> Self {
        Self {
            buffer: String::new(),
            min_len,
        }
    }

    pub fn push(&mut self, text: &str) -> Vec<String> {
        self.buffer.push_str(text);
        let mut out = Vec::new();

        loop {
            let chars: Vec<(usize, char)> = self.buffer.char_indices().collect();
            let mut boundary: Option<usize> = None;

            for (k, &(idx, c)) in chars.iter().enumerate() {
                let is_nl = c == '\n';
                if !is_nl && !is_terminal(c) {
                    continue;
                }
                let end = idx + c.len_utf8();
                let cur_len = self.buffer[..end].trim().chars().count();
                let next_ok = chars
                    .get(k + 1)
                    .map(|&(_, n)| n.is_whitespace())
                    .unwrap_or(true);

                if is_nl {
                    if cur_len >= 15 {
                        boundary = Some(end);
                        break;
                    }
                    continue;
                }
                if next_ok
                    && cur_len >= self.min_len
                    && !(c == '.' && is_abbrev_before(&self.buffer, idx))
                {
                    boundary = Some(end);
                    break;
                }
            }

            if boundary.is_none() && self.buffer.chars().count() > 220 {
                boundary = Some(force_clause_boundary(&self.buffer, 220));
            }

            match boundary {
                Some(end) => {
                    let sentence = self.buffer[..end].trim().to_string();
                    self.buffer = self.buffer[end..].to_string();
                    if !sentence.is_empty() {
                        out.push(sentence);
                    }
                }
                None => break,
            }
        }
        out
    }

    pub fn flush(&mut self) -> Option<String> {
        let sentence = self.buffer.trim().to_string();
        self.buffer.clear();
        if sentence.is_empty() {
            None
        } else {
            Some(sentence)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect(parts: &[&str]) -> Vec<String> {
        let mut s = SentenceSplitter::new(25);
        let mut out = Vec::new();
        for p in parts {
            out.extend(s.push(p));
        }
        if let Some(last) = s.flush() {
            out.push(last);
        }
        out
    }

    #[test]
    fn splits_on_terminal_after_min_len() {
        let out = collect(&["This is a reasonably long sentence. ", "Short. end"]);
        assert_eq!(out[0], "This is a reasonably long sentence.");
    }

    #[test]
    fn does_not_split_short_buffer() {
        // "Hi." is under min_len, so it should not split until flush.
        let out = collect(&["Hi.", " ok"]);
        assert_eq!(out, vec!["Hi. ok".to_string()]);
    }

    #[test]
    fn suppresses_abbreviation() {
        let mut s = SentenceSplitter::new(5);
        let parts = s.push("See Dr. Smith now please. ");
        // "See Dr." must not split (abbreviation); the real boundary is after "please."
        assert!(parts.iter().all(|p| !p.ends_with("Dr.")));
    }

    #[test]
    fn handles_cjk_and_emoji_without_panic() {
        let out = collect(&["你好世界。", "Bonjour 😀 le monde ! ", "Café déjà vu… "]);
        assert!(!out.is_empty());
    }

    #[test]
    fn force_splits_runaway_buffer() {
        let long = "word ".repeat(60); // 300 chars, no terminal punctuation
        let mut s = SentenceSplitter::new(25);
        let parts = s.push(&long);
        assert!(!parts.is_empty(), "runaway buffer should force-split");
    }
}
