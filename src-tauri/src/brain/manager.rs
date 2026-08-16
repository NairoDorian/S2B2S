//! BrainManager: conversation state + streaming orchestration.
//!
//! Owns the multi-turn history, builds the prompt window from settings, streams
//! the reply (emitting `brain:token` / `brain:sentence` / `brain:done` events),
//! and — when read-aloud is enabled — feeds completed sentences straight into
//! the TTS subsystem so speech starts before the reply finishes.

use crate::brain::client::{BrainClient, BrainResult, ChatMessage, ContentPart, MessageContent};
use crate::settings::get_settings;
use crate::tts::manager::TtsManager;
use log::{info, warn};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager};

/// Rough token estimate (~4 chars per token) used for context budgeting.
fn estimate_tokens(text: &str) -> usize {
    text.chars().count() / 4 + 1
}

/// Context window budget. llama.cpp is launched with `-c 16384`; cloud
/// providers get a conservative 8192 budget.
const LLAMA_CPP_CONTEXT_TOKENS: usize = 16384;
const DEFAULT_CONTEXT_TOKENS: usize = 8192;
/// Headroom reserved for the model's reply (and prompt overhead).
const CONTEXT_HEADROOM: usize = 2048;
/// Messages kept unsummarized when compaction runs (≈ last 2 exchanges).
const COMPACTION_KEEP_MESSAGES: usize = 4;
/// How many turns (messages) to summarize in one compaction call.
const COMPACTION_MAX_MESSAGES: usize = 64;

/// Dense-JSON summarization prompt (mirrors speech-to-speech
/// `compaction_prompt.py`): one call compresses the whole old transcript into
/// a single user+assistant summary pair.
const COMPACTION_SYSTEM_PROMPT: &str = "You compress conversation history. Summarize the conversation into dense, third-person notes as valid JSON with exactly two string fields: \"user_summary\" (1-5 sentences: the user's intents, questions, constraints and key facts) and \"assistant_summary\" (1-5 sentences: your answers, decisions and important conclusions). Preserve all facts, names, numbers, and decisions. Output ONLY the JSON object — no markdown fences, no commentary.";

/// Parse the compaction response into `(user_summary, assistant_summary)`.
/// Forgiving: strips markdown fences, tolerates missing fields.
fn parse_compaction_json(raw: &str) -> (String, String) {
    let trimmed = raw
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    let parsed: Option<serde_json::Value> = serde_json::from_str(trimmed).ok();
    match parsed {
        Some(serde_json::Value::Object(map)) => {
            let user = map
                .get("user_summary")
                .and_then(|v| v.as_str())
                .unwrap_or("The user asked for help with the conversation above.");
            let assistant = map
                .get("assistant_summary")
                .and_then(|v| v.as_str())
                .unwrap_or("The assistant answered the user's questions.");
            (user.to_string(), assistant.to_string())
        }
        _ => {
            // Model returned prose instead of JSON: store it as one summary pair.
            let fallback = trimmed.chars().take(2000).collect::<String>();
            (
                format!("Earlier conversation summary: {fallback}"),
                "The assistant continued the conversation after the summarized part.".to_string(),
            )
        }
    }
}

pub struct BrainManager {
    app: AppHandle,
    client: Arc<BrainClient>,
    history: Mutex<Vec<ChatMessage>>,
    /// Abort token of the in-flight turn; replaced on every `ask` so aborting an
    /// old turn can never cancel a new one (barge-in safety).
    current_abort: Mutex<Arc<AtomicBool>>,
    /// Bumped on `clear_history`; a stale compaction result must not splice
    /// into a conversation that was reset while the summary was in flight.
    history_generation: AtomicU64,
    /// Single-flight guard for the background compaction task.
    compaction_in_flight: AtomicBool,
    /// Chain-of-thought captured from the most recent turn (empty when the
    /// provider didn't stream reasoning). Kept out of the returned answer.
    last_reasoning: Mutex<String>,
}

impl BrainManager {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            client: Arc::new(BrainClient::new()),
            history: Mutex::new(Vec::new()),
            current_abort: Mutex::new(Arc::new(AtomicBool::new(false))),
            history_generation: AtomicU64::new(0),
            compaction_in_flight: AtomicBool::new(false),
            last_reasoning: Mutex::new(String::new()),
        }
    }

    /// Chain-of-thought from the most recent completed Brain turn.
    pub fn last_reasoning(&self) -> String {
        self.last_reasoning.lock().unwrap().clone()
    }

    /// Abort the in-flight stream (barge-in) and stop any speech it queued.
    pub fn abort(&self) {
        self.current_abort
            .lock()
            .unwrap()
            .store(true, Ordering::SeqCst);
        if let Some(tts) = self.app.try_state::<Arc<TtsManager>>() {
            tts.stop();
        }
    }

    pub fn clear_history(&self) {
        self.history.lock().unwrap().clear();
        self.history_generation.fetch_add(1, Ordering::SeqCst);
        let _ = self.app.emit("brain:history-cleared", ());
    }

    /// Effective context budget for the active provider.
    fn context_token_limit(&self) -> usize {
        let cfg = get_settings(&self.app).brain;
        let budget = if cfg.provider_id == "llama_cpp" {
            LLAMA_CPP_CONTEXT_TOKENS
        } else {
            DEFAULT_CONTEXT_TOKENS
        };
        budget.saturating_sub(CONTEXT_HEADROOM)
    }

    /// Total estimated tokens across the stored history.
    fn history_tokens(&self) -> usize {
        let history = self.history.lock().unwrap();
        history
            .iter()
            .map(|m| estimate_tokens(&m.content.text_content()))
            .sum()
    }

    /// Build the context window from history with two guards: never exceed the
    /// last `context_turns * 2` messages, and never exceed the token budget
    /// (oldest turns are dropped first). Prevents silent context overflow on
    /// long turns while a background compaction is still in flight.
    fn select_context_messages(&self, cfg: &crate::settings::BrainConfig) -> Vec<ChatMessage> {
        let history = self.history.lock().unwrap();
        let max_messages = if cfg.context_turns > 0 {
            (cfg.context_turns as usize) * 2
        } else {
            0
        };
        let limit = self.context_token_limit();

        let mut selected = Vec::new();
        let mut used_tokens = 0usize;
        for message in history.iter().rev() {
            let tokens = estimate_tokens(&message.content.text_content());
            if max_messages > 0 && selected.len() >= max_messages {
                break;
            }
            if used_tokens + tokens > limit && !selected.is_empty() {
                break;
            }
            used_tokens += tokens;
            selected.push(message.clone());
        }
        selected.reverse();
        selected
    }

    /// After appending a turn: if history exceeds the token budget, either
    /// summarize the old turns (single-flight background task) or drop the
    /// oldest ones when compaction is disabled.
    fn maybe_compact_history(&self) {
        let cfg = get_settings(&self.app).brain;
        if self.history_tokens() <= self.context_token_limit() {
            return;
        }
        if !cfg.compaction_enabled {
            self.drop_oldest_until_fits();
            return;
        }
        if self.compaction_in_flight.swap(true, Ordering::SeqCst) {
            // A compaction is already running; the token-aware context builder
            // keeps requests within budget until it lands.
            return;
        }
        self.spawn_compaction_task();
    }

    /// Hard truncation: remove oldest messages until the token budget holds.
    fn drop_oldest_until_fits(&self) {
        let limit = self.context_token_limit();
        let mut history = self.history.lock().unwrap();
        while !history.is_empty() {
            let total: usize = history
                .iter()
                .map(|m| estimate_tokens(&m.content.text_content()))
                .sum();
            if total <= limit {
                break;
            }
            history.remove(0);
            warn!("[Brain] Dropped oldest turn (context over budget)");
        }
    }

    /// Run the LLM summarization on a background thread and splice the result
    /// back into history — but only if the conversation wasn't cleared while
    /// the summary was in flight (generation counter).
    fn spawn_compaction_task(&self) {
        let app = self.app.clone();
        let client = self.client.clone();
        let generation = self.history_generation.load(Ordering::SeqCst);

        // Snapshot the oldest turns (keep the most recent few intact).
        let transcript = {
            let history = self.history.lock().unwrap();
            let summary_len = history.len().saturating_sub(COMPACTION_KEEP_MESSAGES);
            let summary_len = summary_len.min(COMPACTION_MAX_MESSAGES);
            let turns: Vec<String> = history[..summary_len]
                .iter()
                .map(|m| format!("{}: {}", m.role, m.content.text_content()))
                .collect();
            (turns.join("\n\n"), summary_len)
        };

        std::thread::spawn(move || {
            let (transcript, summary_len) = transcript;
            let result = tauri::async_runtime::block_on(async {
                let settings = get_settings(&app);
                let cfg = &settings.brain;
                if !cfg.enabled {
                    return Err("Brain disabled".to_string());
                }
                if cfg.provider_id == "llama_cpp" {
                    if let Some(llama_manager) =
                        app.try_state::<Arc<crate::brain::llama_manager::LlamaManager>>()
                    {
                        llama_manager.ensure_server_running().await?;
                    }
                }
                let messages = vec![
                    ChatMessage {
                        role: "system".to_string(),
                        content: MessageContent::text(COMPACTION_SYSTEM_PROMPT),
                    },
                    ChatMessage {
                        role: "user".to_string(),
                        content: MessageContent::text(transcript),
                    },
                ];
                let abort = Arc::new(AtomicBool::new(false));
                let mut full = String::new();
                client
                    .stream_chat(
                        &cfg.active_base_url(),
                        &cfg.active_api_key(),
                        &cfg.active_model(),
                        &messages,
                        abort,
                        |token| full.push_str(token),
                        |_| {},
                    )
                    .await?;
                Ok(full)
            });

            // Re-claim the single-flight flag on every exit path.
            let manager = app.state::<Arc<BrainManager>>().inner().clone();
            manager.compaction_in_flight.store(false, Ordering::SeqCst);

            let full = match result {
                Ok(text) => text,
                Err(e) => {
                    warn!("[Brain] Compaction failed ({e}); dropping oldest turns instead");
                    manager.drop_oldest_until_fits();
                    return;
                }
            };

            // Stale splice guard: a clear_history() during the summary invalidates it.
            if manager.history_generation.load(Ordering::SeqCst) != generation {
                return;
            }

            let (user_summary, assistant_summary) = parse_compaction_json(&full);
            let mut history = manager.history.lock().unwrap();
            if history.len() <= summary_len {
                return;
            }
            let tail: Vec<ChatMessage> = history[summary_len..].to_vec();
            *history = vec![
                ChatMessage {
                    role: "user".to_string(),
                    content: MessageContent::text(user_summary),
                },
                ChatMessage {
                    role: "assistant".to_string(),
                    content: MessageContent::text(assistant_summary),
                },
            ];
            history.extend(tail);
            info!(
                "[Brain] Compaction spliced {} turns into a summary pair ({} remaining)",
                summary_len,
                history.len()
            );
            let _ = app.emit("brain:history-compacted", ());
        });
    }

    /// Ask the Brain (text-only). Streams the reply; returns the full assistant text.
    /// Any previous in-flight turn is aborted first (barge-in semantics).
    pub async fn ask(&self, text: String) -> Result<String, String> {
        self.ask_multimodal(text, None, None, None, Vec::new())
            .await
    }

    /// Ask the Brain with optional multimodal inputs.
    /// - `audio_wav_base64`: raw base64-encoded WAV audio (for Gemma 4 native STT)
    /// - `image_png_base64`: raw base64-encoded PNG screenshot (for vision)
    /// - `reply_language`: when `Some`, prepend a "respond in <language>" hint to the
    ///   user turn (mirrors huggingface/speech-to-speech `--enable_lang_prompt`).
    /// - `stt_sources`: individual (model_id, transcript) pairs produced by multi-STT.
    ///   When non-empty, they are listed in the model-facing user message so the
    ///   multimodal Brain can fuse them with the raw audio itself. Conversation
    ///   history always stores only the clean `text` (not the sources block).
    ///
    /// Content parts order follows Gemma 4 best practices:
    /// `image → text → audio`
    pub async fn ask_multimodal(
        &self,
        text: String,
        audio_wav_base64: Option<String>,
        image_png_base64: Option<String>,
        reply_language: Option<String>,
        stt_sources: Vec<(String, String)>,
    ) -> Result<String, String> {
        let has_audio = audio_wav_base64.is_some();
        let has_image = image_png_base64.is_some();
        let audio_size = audio_wav_base64.as_ref().map(|b| b.len()).unwrap_or(0);
        // Gemma 4: ~25 tokens per second of audio at 16kHz, ~640 samples per token
        // base64 ~4/3 expansion, 16-bit PCM = 2 bytes/sample
        let raw_bytes_est = audio_size * 3 / 4;
        let sample_count_est = raw_bytes_est / 2;
        let audio_tokens_est = sample_count_est / 640;
        let audio_seconds = sample_count_est as f64 / 16000.0;
        let text_tokens_est = text.len() / 4; // rough: ~4 chars per token
        info!(
            "[BrainManager::ask_multimodal] has_audio={}, has_image={}, audio_base64_size={}, text_len={}, stt_sources={} — est. {:.1}s audio ≈ {} tokens + {} text tokens = {} total",
            has_audio,
            has_image,
            audio_size,
            text.len(),
            stt_sources.len(),
            audio_seconds,
            audio_tokens_est,
            text_tokens_est,
            audio_tokens_est + text_tokens_est
        );

        let turn_start = Instant::now();
        let abort = Arc::new(AtomicBool::new(false));
        {
            let mut current = self.current_abort.lock().unwrap();
            current.store(true, Ordering::SeqCst);
            *current = abort.clone();
        }

        let cfg = get_settings(&self.app).brain;
        if !cfg.enabled {
            return Err("The Brain is disabled in settings".into());
        }
        if cfg.provider_id == "llama_cpp" {
            if let Some(llama_manager) = self
                .app
                .try_state::<Arc<crate::brain::llama_manager::LlamaManager>>()
            {
                // Audio/image turns require the multimodal projector (mmproj).
                llama_manager
                    .ensure_server_running_with(has_audio || has_image)
                    .await?;
            }
        }
        let text = text.trim().to_string();
        if text.is_empty() {
            return Err("Empty input".into());
        }
        if cfg.active_model().trim().is_empty() {
            return Err("No Brain model configured".into());
        }

        // Build the context window: system + optional speakable-output prompt + last N turns + the new user message.
        let mut messages = Vec::new();
        let mut system = if cfg.read_aloud && !cfg.speakable_output_prompt.trim().is_empty() {
            format!(
                "{}\n\n{}",
                cfg.system_prompt.trim(),
                cfg.speakable_output_prompt.trim()
            )
        } else {
            cfg.system_prompt.clone()
        };
        if cfg.tools_enabled {
            system = format!(
                "{system}\n\n{}",
                crate::brain::tool_calls::tools_prompt_section()
            );
        }
        if !system.trim().is_empty() {
            messages.push(ChatMessage {
                role: "system".into(),
                content: MessageContent::text(system),
            });
        }
        // Token-aware + turn-count-aware context window (oldest dropped first).
        messages.extend(self.select_context_messages(&cfg));
        // Optional reply-language hint (huggingface/speech-to-speech `--enable_lang_prompt`).
        // Prepended only to the model-facing user turn; conversation history keeps the
        // original (unhinted) text so it stays clean.
        let text_with_lang = match reply_language {
            Some(ref lang) if !lang.trim().is_empty() && lang.trim() != "auto" => {
                format!("Please respond in {}.\n\n{}", lang.trim(), text)
            }
            _ => text.clone(),
        };

        let has_multimodal = audio_wav_base64.is_some() || image_png_base64.is_some();

        // Multi-STT sources: list the individual transcripts in the model-facing
        // user turn so the Brain (Gemma 4 multimodal) can fuse them with the raw
        // audio itself. Conversation history keeps only the clean `text`.
        let user_text_for_model = if stt_sources.is_empty() {
            text_with_lang.clone()
        } else {
            let mut block = String::new();
            for (i, (model_id, t)) in stt_sources.iter().enumerate() {
                block.push_str(&format!(
                    "{}. {}: \"{}\"\n",
                    i + 1,
                    crate::stt::multi_stt::short_model_label(model_id),
                    t.trim()
                ));
            }
            format!(
                "{} STT models transcribed the user's speech. Use the raw audio (when present) and these transcripts to resolve the most accurate reading of what the user said:\n\n{}\nUser request:\n{}",
                stt_sources.len(),
                block,
                text_with_lang
            )
        };

        if has_multimodal {
            let mut parts = Vec::new();
            // Image goes before text (Gemma 4 best practice)
            if let Some(ref img_b64) = image_png_base64 {
                parts.push(ContentPart::ImageUrl {
                    image_url: crate::brain::client::ImageUrl {
                        url: format!("data:image/png;base64,{}", img_b64),
                    },
                });
            }
            // Text in the middle
            parts.push(ContentPart::Text {
                text: user_text_for_model.clone(),
            });
            // Audio goes after text (Gemma 4 best practice for ASR)
            if let Some(ref audio_b64) = audio_wav_base64 {
                parts.push(ContentPart::InputAudio {
                    input_audio: crate::brain::client::InputAudio {
                        data: audio_b64.clone(),
                        format: "wav".to_string(),
                    },
                });
            }
            messages.push(ChatMessage {
                role: "user".into(),
                content: MessageContent::parts(parts),
            });
        } else {
            messages.push(ChatMessage {
                role: "user".into(),
                content: MessageContent::text(user_text_for_model.clone()),
            });
        }

        // Read-aloud: start a fresh TTS session for this turn's sentences.
        let tts = if cfg.read_aloud {
            let settings = get_settings(&self.app);
            if settings.tts.enabled {
                self.app
                    .try_state::<Arc<TtsManager>>()
                    .map(|s| s.inner().clone())
            } else {
                None
            }
        } else {
            None
        };
        if let Some(tts) = &tts {
            tts.begin_session();
        }

        let turn_clone = turn_start;
        let app_tokens = self.app.clone();
        let app_sentences = self.app.clone();
        let app_tools = self.app.clone();
        let tts_for_sentences = tts.clone();
        let tools_enabled = cfg.tools_enabled;
        let _ = self.app.emit("brain:thinking", ());

        // Latency: mark time from end of STT to first token
        let ft = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let app_latency = self.app.clone();

        // Tool interception: complete <code> blocks are swallowed from the
        // stream, executed locally, and their results collected for the
        // follow-up turn.
        let tool_intercept = Arc::new(Mutex::new(
            crate::brain::tool_calls::ToolIntercept::default(),
        ));
        let tool_results = Arc::new(Mutex::new(Vec::<String>::new()));

        let result = self
            .client
            .stream_chat(
                &cfg.active_base_url(),
                &cfg.active_api_key(),
                &cfg.active_model(),
                &messages,
                abort.clone(),
                {
                    let tool_intercept = tool_intercept.clone();
                    let tool_results = tool_results.clone();
                    let app_tools = app_tools.clone();
                    let app_tokens = app_tokens.clone();
                    move |token| {
                        if !ft.load(std::sync::atomic::Ordering::SeqCst) {
                            ft.store(true, std::sync::atomic::Ordering::SeqCst);
                            let ms = turn_clone.elapsed().as_millis() as u64;
                            let _ = app_latency.emit(
                                "brain:latency",
                                serde_json::json!({ "stage": "first_token", "ms": ms }),
                            );
                        }
                        let cleaned = if tools_enabled {
                            tool_intercept.lock().unwrap().feed(
                                token,
                                &app_tools,
                                &mut tool_results.lock().unwrap(),
                            )
                        } else {
                            token.to_string()
                        };
                        if !cleaned.is_empty() {
                            let _ = app_tokens.emit("brain:token", cleaned);
                        }
                    }
                },
                {
                    let app_sentences = app_sentences.clone();
                    let tts_for_sentences = tts_for_sentences.clone();
                    move |sentence| {
                        // Defense-in-depth: strip any complete tool blocks that
                        // slipped through the token interceptor (e.g. a block that
                        // arrived whole inside one sentence flush).
                        let cleaned = if tools_enabled {
                            crate::brain::tool_calls::scan_code_blocks(&sentence).0
                        } else {
                            sentence
                        };
                        if cleaned.trim().is_empty() {
                            return;
                        }
                        let _ = app_sentences.emit("brain:sentence", &cleaned);
                        if let Some(tts) = &tts_for_sentences {
                            tts.speak_sentence(cleaned);
                        }
                    }
                },
            )
            .await;

        // Flush any unterminated block back into the stream text.
        let flushed = if tools_enabled {
            tool_intercept.lock().unwrap().flush()
        } else {
            String::new()
        };
        if !flushed.is_empty() {
            let _ = self.app.emit("brain:token", &flushed);
        }

        match result {
            Ok(BrainResult {
                text: full,
                timing,
                reasoning,
            }) => {
                // Capture chain-of-thought for inspection (history, debugging),
                // but never include it in the returned/pasted answer.
                *self.last_reasoning.lock().unwrap() = reasoning;
                // Tool results collected during this stream.
                let tool_lines = if tools_enabled {
                    std::mem::take(&mut *tool_results.lock().unwrap())
                } else {
                    Vec::new()
                };

                // Follow-up turn: feed tool results back so the assistant can
                // answer with them (single round, bounded).
                let (assistant_text, follow_ok) = if !tool_lines.is_empty() {
                    let mut follow_messages = messages.clone();
                    follow_messages.push(ChatMessage {
                        role: "user".into(),
                        content: MessageContent::text(format!(
                            "Tool results:\n{}\n\nContinue your answer to the user using these results. Be concise. Do not call more tools.",
                            tool_lines.join("\n")
                        )),
                    });
                    let follow = self
                        .client
                        .stream_chat(
                            &cfg.active_base_url(),
                            &cfg.active_api_key(),
                            &cfg.active_model(),
                            &follow_messages,
                            abort.clone(),
                            {
                                let app_tokens = app_tokens.clone();
                                move |token| {
                                    let _ = app_tokens.emit("brain:token", token);
                                }
                            },
                            {
                                let app_sentences = app_sentences.clone();
                                let tts_for_sentences = tts_for_sentences.clone();
                                move |sentence| {
                                    let _ = app_sentences.emit("brain:sentence", &sentence);
                                    if let Some(tts) = &tts_for_sentences {
                                        tts.speak_sentence(sentence);
                                    }
                                }
                            },
                        )
                        .await;
                    match follow {
                        Ok(follow_result) => {
                            let cleaned_follow =
                                crate::brain::tool_calls::scan_code_blocks(&follow_result.text).0;
                            (
                                format!("{}\n\n{}", full.trim(), cleaned_follow.trim()),
                                true,
                            )
                        }
                        Err(e) => {
                            warn!("[Brain] Tool follow-up failed: {e}");
                            (full.clone(), false)
                        }
                    }
                } else {
                    (full.clone(), true)
                };
                let _ = follow_ok;

                // Strip tool blocks from the displayed/committed text.
                let cleaned_full = if tools_enabled {
                    crate::brain::tool_calls::scan_code_blocks(&full).0
                } else {
                    full.clone()
                };

                let total_ms = turn_start.elapsed().as_millis() as u64;
                // Use server predicted_per_second from timings block (exact generation speed)
                let server_tps = timing.as_ref().and_then(|t| t.tokens_per_second);
                // Fallback: calculate from completion_tokens / total_ms
                let fallback_tps = timing
                    .as_ref()
                    .and_then(|t| t.completion_tokens)
                    .map(|c| {
                        let token_count = c as f64;
                        if total_ms > 0 {
                            (token_count / total_ms as f64) * 1000.0
                        } else {
                            0.0
                        }
                    })
                    .unwrap_or_else(|| {
                        let token_count = (full.chars().count() / 4).max(1) as f64;
                        if total_ms > 0 {
                            (token_count / total_ms as f64) * 1000.0
                        } else {
                            0.0
                        }
                    });
                let tokens_per_sec = server_tps.unwrap_or(fallback_tps);
                // Use server timing if available (predicted_ms + prompt_ms)
                let predicted_ms = timing.as_ref().and_then(|t| t.predicted_ms);
                let prompt_ms = timing.as_ref().and_then(|t| t.prompt_ms);
                let server_total_ms = predicted_ms.zip(prompt_ms).map(|(p, pp)| p + pp);
                let display_ms = server_total_ms.unwrap_or(total_ms as i64);
                {
                    let mut history = self.history.lock().unwrap();
                    history.push(ChatMessage {
                        role: "user".into(),
                        content: MessageContent::text(text),
                    });
                    history.push(ChatMessage {
                        role: "assistant".into(),
                        content: MessageContent::text(assistant_text.clone()),
                    });
                }
                self.maybe_compact_history();
                let done_payload = serde_json::json!({
                    "text": &assistant_text,
                    "cleaned": &cleaned_full,
                    "tokens_per_sec": tokens_per_sec,
                    "total_ms": display_ms,
                    "predicted_ms": predicted_ms,
                    "prompt_ms": prompt_ms,
                });
                let _ = self.app.emit("brain:done", &done_payload);
                Ok(assistant_text)
            }
            Err(e) => {
                // Remember the user's turn even when the model failed, so the
                // context isn't silently lost on a transient provider error.
                {
                    let mut history = self.history.lock().unwrap();
                    history.push(ChatMessage {
                        role: "user".into(),
                        content: MessageContent::text(text),
                    });
                }
                self.maybe_compact_history();
                let _ = self.app.emit("brain:error", &e);
                Err(e)
            }
        }
    }

    /// Warm up the AI Brain silently. Does not touch conversation history,
    /// does not emit Tauri events, and does not speak the reply.
    pub async fn warmup(&self) -> Result<(), String> {
        let cfg = get_settings(&self.app).brain;
        if !cfg.enabled {
            return Ok(());
        }
        let model = cfg.active_model();
        if model.trim().is_empty() {
            return Ok(());
        }

        // Ensure llama.cpp server is running before warmup.
        // The LlamaManager emits typed `llama-server-status` events itself,
        // so the footer Brain indicator reflects the true server state.
        if cfg.provider_id == "llama_cpp" {
            if let Some(llama_manager) = self
                .app
                .try_state::<Arc<crate::brain::llama_manager::LlamaManager>>()
            {
                llama_manager.ensure_server_running().await?;
            }
        }

        let warmup_text = if cfg.warmup_prompt.trim().is_empty() {
            // No warmup configured — jump straight to ready
            return Ok(());
        } else {
            &cfg.warmup_prompt
        };

        log::info!("[Startup] Warming up AI Brain with: {:?}", warmup_text);
        let messages = vec![ChatMessage {
            role: "user".into(),
            content: MessageContent::text(warmup_text),
        }];

        // Create a standalone abort flag for warmup
        let abort = Arc::new(AtomicBool::new(false));

        log::info!("[Startup] Running silent Brain warm up stream...");
        let result = self
            .client
            .stream_chat(
                &cfg.active_base_url(),
                &cfg.active_api_key(),
                &model,
                &messages,
                abort,
                |_token| {},
                |_sentence| {},
            )
            .await;

        match result {
            Ok(BrainResult { .. }) => {
                log::info!("[Startup] Silent Brain warm up stream completed successfully.");
                Ok(())
            }
            Err(e) => {
                log::error!("[Startup] Brain warm up stream failed: {}", e);
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_estimate_scales_with_length() {
        assert_eq!(estimate_tokens(""), 1);
        assert_eq!(estimate_tokens("hello"), 2);
        assert!(estimate_tokens(&"x".repeat(400)) > 50);
    }

    #[test]
    fn compaction_json_is_parsed() {
        let (user, assistant) = parse_compaction_json(
            r#"```json
{"user_summary": "User asked about weather.", "assistant_summary": "Assistant said it rains."}
```"#,
        );
        assert_eq!(user, "User asked about weather.");
        assert_eq!(assistant, "Assistant said it rains.");
    }

    #[test]
    fn compaction_plain_json_is_parsed() {
        let (user, assistant) =
            parse_compaction_json(r#"{"user_summary": "U.", "assistant_summary": "A."}"#);
        assert_eq!(user, "U.");
        assert_eq!(assistant, "A.");
    }

    #[test]
    fn compaction_prose_falls_back_to_single_summary() {
        let (user, assistant) = parse_compaction_json("The user talked a lot.");
        assert!(user.contains("The user talked a lot."));
        assert!(!assistant.is_empty());
    }

    #[test]
    fn compaction_missing_fields_get_defaults() {
        let (user, assistant) = parse_compaction_json(r#"{"user_summary": "Only user."}"#);
        assert_eq!(user, "Only user.");
        assert!(!assistant.is_empty());
    }
}
