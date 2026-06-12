//! BrainManager: conversation state + streaming orchestration.
//!
//! Owns the multi-turn history, builds the prompt window from settings, streams
//! the reply (emitting `brain:token` / `brain:sentence` / `brain:done` events),
//! and — when read-aloud is enabled — feeds completed sentences straight into
//! the TTS subsystem so speech starts before the reply finishes.

use crate::brain::client::{BrainClient, BrainResult, ChatMessage};
use crate::settings::get_settings;
use crate::tts::manager::TtsManager;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager};

pub struct BrainManager {
    app: AppHandle,
    client: Arc<BrainClient>,
    history: Mutex<Vec<ChatMessage>>,
    /// Abort token of the in-flight turn; replaced on every `ask` so aborting an
    /// old turn can never cancel a new one (barge-in safety).
    current_abort: Mutex<Arc<AtomicBool>>,
}

impl BrainManager {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            client: Arc::new(BrainClient::new()),
            history: Mutex::new(Vec::new()),
            current_abort: Mutex::new(Arc::new(AtomicBool::new(false))),
        }
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
        let _ = self.app.emit("brain:history-cleared", ());
    }

    /// Ask the Brain. Streams the reply; returns the full assistant text.
    /// Any previous in-flight turn is aborted first (barge-in semantics).
    pub async fn ask(&self, text: String) -> Result<String, String> {
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
            if let Some(llama_manager) = self.app.try_state::<Arc<crate::brain::llama_manager::LlamaManager>>() {
                llama_manager.ensure_server_running().await?;
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
        let system = if cfg.read_aloud && !cfg.speakable_output_prompt.trim().is_empty() {
            format!(
                "{}\n\n{}",
                cfg.system_prompt.trim(),
                cfg.speakable_output_prompt.trim()
            )
        } else {
            cfg.system_prompt.clone()
        };
        if !system.trim().is_empty() {
            messages.push(ChatMessage {
                role: "system".into(),
                content: system,
            });
        }
        if cfg.context_turns > 0 {
            let history = self.history.lock().unwrap();
            // 2 messages per turn (user + assistant).
            let keep = (cfg.context_turns as usize) * 2;
            let start = history.len().saturating_sub(keep);
            messages.extend(history[start..].iter().cloned());
        }
        messages.push(ChatMessage {
            role: "user".into(),
            content: text.clone(),
        });

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
        let tts_for_sentences = tts.clone();
        let _ = self.app.emit("brain:thinking", ());

        // Latency: mark time from end of STT to first token
        let ft = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let app_latency = self.app.clone();

        let result = self
            .client
            .stream_chat(
                &cfg.active_base_url(),
                &cfg.active_api_key(),
                &cfg.active_model(),
                &messages,
                abort,
                move |token| {
                    if !ft.load(std::sync::atomic::Ordering::SeqCst) {
                        ft.store(true, std::sync::atomic::Ordering::SeqCst);
                        let ms = turn_clone.elapsed().as_millis() as u64;
                        let _ = app_latency.emit(
                            "brain:latency",
                            serde_json::json!({ "stage": "first_token", "ms": ms }),
                        );
                    }
                    let _ = app_tokens.emit("brain:token", token);
                },
                move |sentence| {
                    let _ = app_sentences.emit("brain:sentence", &sentence);
                    if let Some(tts) = &tts_for_sentences {
                        tts.speak_sentence(sentence);
                    }
                },
            )
            .await;

        match result {
            Ok(BrainResult { text: full, timing }) => {
                let total_ms = turn_start.elapsed().as_millis() as u64;
                // Use server completion_tokens for accurate count, else estimate from chars
                let completion_tokens = timing.as_ref().and_then(|t| t.completion_tokens);
                let token_count = completion_tokens
                    .map(|c| c as f64)
                    .unwrap_or_else(|| (full.chars().count() / 4).max(1) as f64);
                // Calculate t/s from actual token count and elapsed time
                let tokens_per_sec = if total_ms > 0 {
                    (token_count / total_ms as f64) * 1000.0
                } else {
                    0.0
                };
                // Use server timing if available (predicted_ms + prompt_ms)
                let predicted_ms = timing.as_ref().and_then(|t| t.predicted_ms);
                let prompt_ms = timing.as_ref().and_then(|t| t.prompt_ms);
                let server_total_ms = predicted_ms.zip(prompt_ms)
                    .map(|(p, pp)| p + pp);
                let display_ms = server_total_ms.unwrap_or(total_ms as i64);
                {
                    let mut history = self.history.lock().unwrap();
                    history.push(ChatMessage {
                        role: "user".into(),
                        content: text,
                    });
                    history.push(ChatMessage {
                        role: "assistant".into(),
                        content: full.clone(),
                    });
                }
                let done_payload = serde_json::json!({
                    "text": &full,
                    "tokens_per_sec": tokens_per_sec,
                    "total_ms": display_ms,
                    "predicted_ms": predicted_ms,
                    "prompt_ms": prompt_ms,
                });
                let _ = self.app.emit("brain:done", &done_payload);
                Ok(full)
            }
            Err(e) => {
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

        // Ensure llama.cpp server is running before warmup
        if cfg.provider_id == "llama_cpp" {
            let _ = self.app.emit("brain:llama-loading", ());
            if let Some(llama_manager) = self.app.try_state::<Arc<crate::brain::llama_manager::LlamaManager>>() {
                llama_manager.ensure_server_running().await?;
            }
        }

        let messages = vec![ChatMessage {
            role: "user".into(),
            content: "Count from 1 to 3".into(),
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
                if cfg.provider_id == "llama_cpp" {
                    let _ = self.app.emit("brain:llama-ready", ());
                }
                Ok(())
            }
            Err(e) => {
                log::error!("[Startup] Brain warm up stream failed: {}", e);
                if cfg.provider_id == "llama_cpp" {
                    let _ = self.app.emit("brain:llama-error", &e);
                }
                Err(e)
            }
        }
    }
}
