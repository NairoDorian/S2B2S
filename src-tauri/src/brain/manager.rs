//! BrainManager: conversation state + streaming orchestration.
//!
//! Owns the multi-turn history, builds the prompt window from settings, streams
//! the reply (emitting `brain:token` / `brain:sentence` / `brain:done` events),
//! and — when read-aloud is enabled — feeds completed sentences straight into
//! the TTS subsystem so speech starts before the reply finishes.

use crate::brain::client::{BrainClient, ChatMessage};
use crate::settings::get_settings;
use crate::tts::manager::TtsManager;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
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
        // Cancel the previous turn and install a fresh abort token for this one.
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
        let text = text.trim().to_string();
        if text.is_empty() {
            return Err("Empty input".into());
        }
        if cfg.active_model().trim().is_empty() {
            return Err("No Brain model configured".into());
        }

        // Build the context window: system + last N turns + the new user message.
        let mut messages = Vec::new();
        if !cfg.system_prompt.trim().is_empty() {
            messages.push(ChatMessage {
                role: "system".into(),
                content: cfg.system_prompt.clone(),
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

        let app_tokens = self.app.clone();
        let app_sentences = self.app.clone();
        let tts_for_sentences = tts.clone();
        let _ = self.app.emit("brain:thinking", ());

        let result = self
            .client
            .stream_chat(
                &cfg.active_base_url(),
                &cfg.active_api_key(),
                &cfg.active_model(),
                &messages,
                abort,
                move |token| {
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
            Ok(full) => {
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
                let _ = self.app.emit("brain:done", &full);
                Ok(full)
            }
            Err(e) => {
                let _ = self.app.emit("brain:error", &e);
                Err(e)
            }
        }
    }
}
