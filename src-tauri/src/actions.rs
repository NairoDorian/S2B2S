#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use crate::apple_intelligence;
use crate::audio_feedback::{play_feedback_sound, play_feedback_sound_blocking, SoundType};
use crate::audio_toolkit::{is_microphone_access_denied, is_no_input_device_error, VadPolicy};
use crate::managers::audio::AudioRecordingManager;
use crate::managers::history::HistoryManager;
use crate::managers::model::ModelManager;
use crate::managers::transcription::StreamWorkKind;
use crate::managers::transcription::TranscriptionManager;
use crate::settings::{
    get_settings, AppSettings, OverlayStyle, PostProcessAction, APPLE_INTELLIGENCE_PROVIDER_ID,
};
use crate::shortcut;
use crate::stt::multi_stt;
use crate::tray::{change_tray_icon, TrayIconState};
use crate::utils::{
    self, show_processing_overlay, show_recording_overlay, show_transcribing_overlay,
};
use crate::TranscriptionCoordinator;
use ferrous_opencc::{config::BuiltinConfig, OpenCC};
use log::{debug, error, info, warn};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::Manager;
use tauri::{AppHandle, Emitter};

const CANCELLATION_POLL_INTERVAL: Duration = Duration::from_millis(25);

#[derive(Clone, serde::Serialize)]
struct RecordingErrorEvent {
    error_type: String,
    detail: Option<String>,
}

/// Drop guard that notifies the [`TranscriptionCoordinator`] when the
/// transcription pipeline finishes — whether it completes normally or panics.
struct FinishGuard(AppHandle);
impl Drop for FinishGuard {
    fn drop(&mut self) {
        if let Some(c) = self.0.try_state::<TranscriptionCoordinator>() {
            c.notify_processing_finished();
        }
        crate::recording_session::exit_processing(&self.0);
    }
}

// Shortcut Action Trait
pub trait ShortcutAction: Send + Sync {
    fn start(&self, app: &AppHandle, binding_id: &str, shortcut_str: &str);
    fn stop(&self, app: &AppHandle, binding_id: &str, shortcut_str: &str);
}

// Transcribe Action
struct TranscribeAction {
    post_process: bool,
    /// Route the transcription to the Brain (S2B2S conversation loop) instead
    /// of pasting it into the focused application.
    route_to_brain: bool,
}

/// Field name for structured output JSON schema
const TRANSCRIPTION_FIELD: &str = "transcription";

/// Strip invisible Unicode characters that some LLMs may insert
fn strip_invisible_chars(s: &str) -> String {
    s.replace(['\u{200B}', '\u{200C}', '\u{200D}', '\u{FEFF}'], "")
}

/// Build a system prompt from the user's prompt template.
/// Removes `${output}` placeholder since the transcription is sent as the user message.
fn build_system_prompt(prompt_template: &str) -> String {
    prompt_template.replace("${output}", "").trim().to_string()
}

/// Returns `true` when a transcription has no meaningful content to
/// post-process (empty or whitespace-only). Used to skip the post-processing
/// LLM call when nothing was actually transcribed, which would otherwise make
/// the model reply with an error message such as "you need to provide the
/// transcription".
fn is_blank_transcription(transcription: &str) -> bool {
    transcription.trim().is_empty()
}

async fn complete_unless_cancelled<F, C>(operation: F, is_cancelled: C) -> Option<F::Output>
where
    F: Future,
    C: Fn() -> bool,
{
    tokio::pin!(operation);

    loop {
        if is_cancelled() {
            return None;
        }

        if let Ok(result) =
            tokio::time::timeout(CANCELLATION_POLL_INTERVAL, operation.as_mut()).await
        {
            return Some(result);
        }
    }
}

fn should_use_streaming_overlay(style: OverlayStyle, is_streaming: bool) -> bool {
    style == OverlayStyle::Live && is_streaming
}

async fn post_process_transcription(
    app: &AppHandle,
    settings: &AppSettings,
    transcription: &str,
    operation_id: Option<u64>,
) -> Option<String> {
    if is_blank_transcription(transcription) {
        debug!("Post-processing skipped because the transcription is empty");
        return None;
    }

    let check_cancelled = || {
        if let (Some(tracker), Some(op_id)) = (
            app.try_state::<Arc<crate::llm_operation::LlmOperationTracker>>(),
            operation_id,
        ) {
            if tracker.is_cancelled(op_id) {
                debug!(
                    "LLM post-processing operation {} was cancelled, aborting.",
                    op_id
                );
                return true;
            }
        }
        false
    };

    if check_cancelled() {
        return None;
    }

    let provider = match settings.active_post_process_provider().cloned() {
        Some(provider) => provider,
        None => {
            debug!("Post-processing enabled but no provider is selected");
            return None;
        }
    };

    let model = settings
        .post_process_models
        .get(&provider.id)
        .cloned()
        .unwrap_or_default();

    if model.trim().is_empty() {
        debug!(
            "Post-processing skipped because provider '{}' has no model configured",
            provider.id
        );
        return None;
    }

    let selected_prompt_id = match &settings.post_process_selected_prompt_id {
        Some(id) => id.clone(),
        None => {
            debug!("Post-processing skipped because no prompt is selected");
            return None;
        }
    };

    let prompt_raw = match settings
        .post_process_prompts
        .iter()
        .find(|prompt| prompt.id == selected_prompt_id)
    {
        Some(prompt) => prompt.prompt.clone(),
        None => {
            debug!(
                "Post-processing skipped because prompt '{}' was not found",
                selected_prompt_id
            );
            return None;
        }
    };

    let prompt = substitute_context_variables(&prompt_raw);

    if prompt.trim().is_empty() {
        debug!("Post-processing skipped because the selected prompt is empty");
        return None;
    }

    debug!(
        "Starting LLM post-processing with provider '{}' (model: {})",
        provider.id, model
    );

    if provider.id == "llama_cpp" {
        if let Some(llama_manager) =
            app.try_state::<Arc<crate::brain::llama_manager::LlamaManager>>()
        {
            if let Err(e) = llama_manager.ensure_server_running().await {
                error!("Failed to start llama-server for post-processing: {}", e);
                return None;
            }
        }
    }

    let api_key = settings
        .post_process_api_keys
        .get(&provider.id)
        .cloned()
        .unwrap_or_default();

    // Disable reasoning for providers where post-processing rarely benefits from it.
    // - custom: top-level reasoning_effort (works for local OpenAI-compat servers)
    // - openrouter: nested reasoning object; exclude:true also keeps reasoning text
    //   out of the response so it can't pollute structured-output JSON parsing
    let (reasoning_effort, reasoning) = match provider.id.as_str() {
        "custom" | "llama_cpp" => (Some("none".to_string()), None),
        "openrouter" => (
            None,
            Some(crate::llm_client::ReasoningConfig {
                effort: Some("none".to_string()),
                exclude: Some(true),
            }),
        ),
        _ => (None, None),
    };

    if check_cancelled() {
        return None;
    }

    if provider.supports_structured_output {
        debug!("Using structured outputs for provider '{}'", provider.id);

        let system_prompt = build_system_prompt(&prompt);
        let user_content = transcription.to_string();

        // Handle Apple Intelligence separately since it uses native Swift APIs
        if provider.id == APPLE_INTELLIGENCE_PROVIDER_ID {
            #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
            {
                if !apple_intelligence::check_apple_intelligence_availability() {
                    debug!(
                        "Apple Intelligence selected but not currently available on this device"
                    );
                    return None;
                }

                let token_limit = model.trim().parse::<i32>().unwrap_or(0);
                let result = apple_intelligence::process_text_with_system_prompt(
                    &system_prompt,
                    &user_content,
                    token_limit,
                );

                if check_cancelled() {
                    return None;
                }

                return match result {
                    Ok(result) => {
                        if result.trim().is_empty() {
                            debug!("Apple Intelligence returned an empty response");
                            None
                        } else {
                            let result = strip_invisible_chars(&result);
                            debug!(
                                "Apple Intelligence post-processing succeeded. Output length: {} chars",
                                result.len()
                            );
                            Some(result)
                        }
                    }
                    Err(err) => {
                        error!("Apple Intelligence post-processing failed: {}", err);
                        None
                    }
                };
            }

            #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
            {
                debug!("Apple Intelligence provider selected on unsupported platform");
                return None;
            }
        }

        // Define JSON schema for transcription output
        let json_schema = serde_json::json!({
            "type": "object",
            "properties": {
                (TRANSCRIPTION_FIELD): {
                    "type": "string",
                    "description": "The cleaned and processed transcription text"
                }
            },
            "required": [TRANSCRIPTION_FIELD],
            "additionalProperties": false
        });

        let completion_res = crate::llm_client::send_chat_completion_with_schema(
            &provider,
            api_key.clone(),
            &model,
            user_content,
            Some(system_prompt),
            Some(json_schema),
            reasoning_effort.clone(),
            reasoning.clone(),
        )
        .await;

        if check_cancelled() {
            return None;
        }

        match completion_res {
            Ok(Some(content)) => {
                // Parse the JSON response to extract the transcription field
                match serde_json::from_str::<serde_json::Value>(&content) {
                    Ok(json) => {
                        if let Some(transcription_value) =
                            json.get(TRANSCRIPTION_FIELD).and_then(|t| t.as_str())
                        {
                            let result = strip_invisible_chars(transcription_value);
                            debug!(
                                "Structured output post-processing succeeded for provider '{}'. Output length: {} chars",
                                provider.id,
                                result.len()
                            );
                            return Some(result);
                        } else {
                            error!("Structured output response missing 'transcription' field");
                            return Some(strip_invisible_chars(&content));
                        }
                    }
                    Err(e) => {
                        error!(
                            "Failed to parse structured output JSON: {}. Returning raw content.",
                            e
                        );
                        return Some(strip_invisible_chars(&content));
                    }
                }
            }
            Ok(None) => {
                error!("LLM API response has no content");
                return None;
            }
            Err(e) => {
                warn!(
                    "Structured output failed for provider '{}': {}. Falling back to legacy mode.",
                    provider.id, e
                );
                // Fall through to legacy mode below
            }
        }
    }

    // Legacy mode: Replace ${output} variable in the prompt with the actual text
    let processed_prompt = prompt.replace("${output}", transcription);
    debug!("Processed prompt length: {} chars", processed_prompt.len());

    if check_cancelled() {
        return None;
    }

    let completion_res = crate::llm_client::send_chat_completion(
        &provider,
        api_key,
        &model,
        processed_prompt,
        reasoning_effort,
        reasoning,
    )
    .await;

    if check_cancelled() {
        return None;
    }

    match completion_res {
        Ok(Some(content)) => {
            let content = strip_invisible_chars(&content);
            debug!(
                "LLM post-processing succeeded for provider '{}'. Output length: {} chars",
                provider.id,
                content.len()
            );
            Some(content)
        }
        Ok(None) => {
            error!("LLM API response has no content");
            None
        }
        Err(e) => {
            error!(
                "LLM post-processing failed for provider '{}': {}. Falling back to original transcription.",
                provider.id,
                e
            );
            None
        }
    }
}

/// Run a post-process action over the given text, resolving its saved
/// language model (falling back to the first saved model, then to the legacy
/// active provider configuration).
pub(crate) async fn run_post_process_action(
    app: &AppHandle,
    settings: &AppSettings,
    text: &str,
    action: &PostProcessAction,
    operation_id: Option<u64>,
) -> Option<String> {
    let model = action
        .llm_model_id
        .as_deref()
        .and_then(|id| settings.llm_model(id))
        .or_else(|| settings.llm_models.first());

    match model {
        Some(model) => {
            // Reuse the existing process_action logic by building a temporary prompt config
            // This routes through the existing LLM client with the saved model's provider
            if model.provider_id == APPLE_INTELLIGENCE_PROVIDER_ID {
                debug!(
                    "Apple Intelligence provider selected for action, routing through legacy path"
                );
            }
            process_action(
                app,
                settings,
                text,
                &action.prompt,
                Some(&model.model),
                Some(&model.provider_id),
                operation_id,
            )
            .await
        }
        None => {
            // Fallback to legacy post-processing with the active provider/model
            debug!(
                "No saved language model found for action '{}'; falling back to legacy config",
                action.id
            );
            post_process_transcription_with_action(
                app,
                settings,
                text,
                &action.prompt,
                operation_id,
            )
            .await
        }
    }
}

/// Fallback: process text using the legacy active provider/model config.
async fn post_process_transcription_with_action(
    app: &AppHandle,
    settings: &AppSettings,
    transcription: &str,
    action_prompt: &str,
    operation_id: Option<u64>,
) -> Option<String> {
    let check_cancelled = || {
        if let (Some(tracker), Some(op_id)) = (
            app.try_state::<Arc<crate::llm_operation::LlmOperationTracker>>(),
            operation_id,
        ) {
            if tracker.is_cancelled(op_id) {
                debug!(
                    "LLM action post-processing operation {} was cancelled, aborting.",
                    op_id
                );
                return true;
            }
        }
        false
    };

    if check_cancelled() {
        return None;
    }

    let provider = match settings.active_post_process_provider().cloned() {
        Some(provider) => provider,
        None => {
            debug!("Post-processing enabled but no provider is selected");
            return None;
        }
    };

    let model = settings
        .post_process_models
        .get(&provider.id)
        .cloned()
        .unwrap_or_default();

    if model.trim().is_empty() {
        debug!(
            "Post-processing skipped because provider '{}' has no model configured",
            provider.id
        );
        return None;
    }

    let processed_prompt = action_prompt.replace("${output}", transcription);
    let api_key = settings
        .post_process_api_keys
        .get(&provider.id)
        .cloned()
        .unwrap_or_default();

    debug!(
        "Falling back to legacy post-processing with provider '{}' (model: {})",
        provider.id, model
    );

    if check_cancelled() {
        return None;
    }

    let completion_res = crate::llm_client::send_chat_completion(
        &provider,
        api_key,
        &model,
        processed_prompt,
        None,
        None,
    )
    .await;

    if check_cancelled() {
        return None;
    }

    match completion_res {
        Ok(Some(content)) => {
            let content = strip_invisible_chars(&content);
            debug!(
                "Legacy post-processing succeeded. Output length: {} chars",
                content.len()
            );
            Some(content)
        }
        Ok(None) => {
            error!("LLM API response has no content");
            None
        }
        Err(e) => {
            error!("LLM post-processing failed: {}", e);
            None
        }
    }
}

/// Process text through the LLM with a specific prompt, provider, and model.
/// Shared by both the action system and the legacy path.
async fn process_action(
    app: &AppHandle,
    settings: &AppSettings,
    text: &str,
    prompt_template: &str,
    model: Option<&str>,
    provider_id: Option<&str>,
    operation_id: Option<u64>,
) -> Option<String> {
    let check_cancelled = || {
        if let (Some(tracker), Some(op_id)) = (
            app.try_state::<Arc<crate::llm_operation::LlmOperationTracker>>(),
            operation_id,
        ) {
            if tracker.is_cancelled(op_id) {
                debug!("LLM action operation {} was cancelled, aborting.", op_id);
                return true;
            }
        }
        false
    };

    if check_cancelled() {
        return None;
    }

    let provider = match provider_id {
        Some(pid) => settings.post_process_provider(pid).cloned(),
        None => settings.active_post_process_provider().cloned(),
    };
    let provider = match provider {
        Some(p) => p,
        None => {
            debug!("No provider available for action processing");
            return None;
        }
    };

    let model_str = match model {
        Some(m) => m.to_string(),
        None => settings
            .post_process_models
            .get(&provider.id)
            .cloned()
            .unwrap_or_default(),
    };

    if model_str.trim().is_empty() {
        debug!("No model configured for action processing");
        return None;
    }

    if provider.id == "llama_cpp" {
        if let Some(llama_manager) =
            app.try_state::<Arc<crate::brain::llama_manager::LlamaManager>>()
        {
            if let Err(e) = llama_manager.ensure_server_running().await {
                error!("Failed to start llama-server for action processing: {}", e);
                return None;
            }
        }
    }

    let api_key = settings
        .post_process_api_keys
        .get(&provider.id)
        .cloned()
        .unwrap_or_default();

    let processed_prompt = prompt_template.replace("${output}", text);
    let system_prompt = prompt_template.replace("${output}", "").trim().to_string();

    if check_cancelled() {
        return None;
    }

    if provider.supports_structured_output {
        let json_schema = serde_json::json!({
            "type": "object",
            "properties": {
                (TRANSCRIPTION_FIELD): {
                    "type": "string",
                    "description": "The cleaned and processed transcription text"
                }
            },
            "required": [TRANSCRIPTION_FIELD],
            "additionalProperties": false
        });

        let completion_res = crate::llm_client::send_chat_completion_with_schema(
            &provider,
            api_key.clone(),
            &model_str,
            text.to_string(),
            Some(system_prompt),
            Some(json_schema),
            None,
            None,
        )
        .await;

        if check_cancelled() {
            return None;
        }

        match completion_res {
            Ok(Some(content)) => match serde_json::from_str::<serde_json::Value>(&content) {
                Ok(json) => {
                    if let Some(transcription_value) =
                        json.get(TRANSCRIPTION_FIELD).and_then(|t| t.as_str())
                    {
                        return Some(strip_invisible_chars(transcription_value));
                    }
                    return Some(strip_invisible_chars(&content));
                }
                Err(_) => return Some(strip_invisible_chars(&content)),
            },
            Ok(None) => return None,
            Err(_) => {} // Fall through to legacy
        }
    }

    if check_cancelled() {
        return None;
    }

    let completion_res = crate::llm_client::send_chat_completion(
        &provider,
        api_key,
        &model_str,
        processed_prompt,
        None,
        None,
    )
    .await;

    if check_cancelled() {
        return None;
    }

    match completion_res {
        Ok(Some(content)) => Some(strip_invisible_chars(&content)),
        Ok(None) => None,
        Err(e) => {
            error!("Action LLM processing failed: {}", e);
            None
        }
    }
}

async fn maybe_convert_chinese_variant(
    effective_language: &str,
    transcription: &str,
) -> Option<String> {
    // Gate on the language the model actually transcribed in (the effective
    // language), not the persisted intent. A leftover zh-Hans/zh-Hant intent
    // from a previously selected model must not run OpenCC S2T/T2S over output a
    // non-Chinese model produced — that would silently rewrite any shared CJK
    // characters (e.g. Japanese kanji) in the result.
    let is_simplified = effective_language == "zh-Hans";
    let is_traditional = effective_language == "zh-Hant";

    if !is_simplified && !is_traditional {
        debug!("effective language is not Simplified or Traditional Chinese; skipping conversion");
        return None;
    }

    debug!(
        "Starting Chinese variant conversion using OpenCC for language: {}",
        effective_language
    );

    // Use OpenCC to convert based on selected language
    let config = if is_simplified {
        // Convert Traditional Chinese to Simplified Chinese
        BuiltinConfig::Tw2sp
    } else {
        // Convert Simplified Chinese to Traditional Chinese
        BuiltinConfig::S2tw
    };

    match OpenCC::from_config(config) {
        Ok(converter) => {
            let converted = converter.convert(transcription);
            debug!(
                "OpenCC translation completed. Input length: {}, Output length: {}",
                transcription.len(),
                converted.len()
            );
            Some(converted)
        }
        Err(e) => {
            error!("Failed to initialize OpenCC converter: {}. Falling back to original transcription.", e);
            None
        }
    }
}

pub(crate) struct ProcessedTranscription {
    pub final_text: String,
    pub post_processed_text: Option<String>,
    pub post_process_prompt: Option<String>,
}

/// Resolve the persisted language *intent* into the language the currently-loaded
/// model will actually use — the same capability-aware coercion the transcription
/// paths apply (see [`crate::managers::model::effective_language`]). Post-processing
/// resolves it independently so it agrees with the language the transcription ran
/// in, without threading a value through the pipeline.
fn resolve_effective_language(app: &AppHandle, settings: &AppSettings) -> String {
    let tm = app.state::<Arc<TranscriptionManager>>();
    let model_manager = app.state::<Arc<ModelManager>>();
    let active_model = tm
        .get_current_model()
        .unwrap_or_else(|| settings.selected_model.clone());
    match model_manager.get_model_info(&active_model) {
        Some(info) => crate::managers::model::effective_language(
            &settings.selected_language,
            &info.supported_languages,
            info.supports_language_detection,
        ),
        None => settings.selected_language.clone(),
    }
}

/// Resolve the language the Brain should reply in, mirroring the
/// huggingface/speech-to-speech `--language auto` + `--enable_lang_prompt` flow.
///
/// - `brain.reply_language` set to a concrete BCP-47 code forces that reply language.
/// - `"auto"` (default) defers to the effective STT language (the selected language,
///   or the OS input source for `"os_input"`).
/// - Returns `None` when no concrete hint applies (e.g. `en`, `auto`, `os_input`),
///   letting the model infer the language from context.
pub(crate) fn resolve_reply_language(app: &AppHandle, settings: &AppSettings) -> Option<String> {
    let configured = settings.brain.reply_language.trim();
    let lang = if configured.is_empty() || configured == "auto" {
        resolve_effective_language(app, settings)
    } else {
        configured.to_string()
    };
    match lang.trim() {
        "" | "auto" | "os_input" | "en" | "en-US" | "en-GB" => None,
        other => Some(other.to_string()),
    }
}

pub(crate) async fn process_transcription_output(
    app: &AppHandle,
    transcription: &str,
    post_process: bool,
    operation_id: Option<u64>,
) -> ProcessedTranscription {
    let settings = get_settings(app);
    let mut final_text = transcription.to_string();
    let mut post_processed_text: Option<String> = None;
    let mut post_process_prompt: Option<String> = None;

    // Resolve the language the transcription actually ran in (the persisted
    // intent coerced against the loaded model's capabilities) so OpenCC keys off
    // the effective language rather than a possibly-stale intent.
    let effective_language = resolve_effective_language(app, &settings);
    if let Some(converted_text) =
        maybe_convert_chinese_variant(&effective_language, transcription).await
    {
        final_text = converted_text;
    }

    // ITN: spoken → written normalization (e.g., "two hundred" → "200")
    // Uses text-processing-rs — runs before Brain to improve LLM comprehension.
    let itn_text = crate::tts::sanitize::post_stt_normalize(&final_text);
    if itn_text != final_text {
        final_text = itn_text;
    }

    // Check if a specific post-process action was selected via shortcut
    let action_id: Option<String> = app
        .try_state::<ActiveActionState>()
        .and_then(|state| state.0.lock().ok()?.take());

    if let Some(action_id) = action_id {
        let settings = get_settings(app);
        if let Some(action) = settings.post_process_action(&action_id) {
            if let Some(processed_text) =
                run_post_process_action(app, &settings, &final_text, action, operation_id).await
            {
                post_processed_text = Some(processed_text.clone());
                post_process_prompt = Some(action.prompt.clone());
                final_text = processed_text;
            }
        }
    } else if post_process {
        if let Some(processed_text) =
            post_process_transcription(app, &settings, &final_text, operation_id).await
        {
            post_processed_text = Some(processed_text.clone());
            final_text = processed_text;

            if let Some(prompt_id) = &settings.post_process_selected_prompt_id {
                if let Some(prompt) = settings
                    .post_process_prompts
                    .iter()
                    .find(|prompt| &prompt.id == prompt_id)
                {
                    post_process_prompt = Some(prompt.prompt.clone());
                }
            }
        }
    } else if final_text != transcription {
        post_processed_text = Some(final_text.clone());
    }

    ProcessedTranscription {
        final_text,
        post_processed_text,
        post_process_prompt,
    }
}

impl ShortcutAction for TranscribeAction {
    fn start(&self, app: &AppHandle, binding_id: &str, _shortcut_str: &str) {
        let start_time = Instant::now();
        debug!("TranscribeAction::start called for binding: {}", binding_id);

        // Check if there is already an active session
        {
            let state = app.state::<crate::recording_session::ManagedSessionState>();
            let mut session_state = state.lock().unwrap();
            if !matches!(
                &*session_state,
                crate::recording_session::SessionState::Idle
            ) {
                warn!(
                    "TranscribeAction::start called but session is not idle (state is {:?})",
                    *session_state
                );
                return;
            }

            let session = std::sync::Arc::new(
                crate::recording_session::RecordingSession::new_with_resources(
                    app, true, // will register cancel
                    true, // will apply mute
                ),
            );

            let operation_id = crate::session_manager::next_operation_id();
            let captured_settings = get_settings(app);
            *session_state = crate::recording_session::SessionState::Recording {
                session: std::sync::Arc::clone(&session),
                binding_id: binding_id.to_string(),
                operation_id,
                started_at: Instant::now(),
                captured_profile_id: None,
                captured_settings,
            };
        }

        // Load model in the background
        let tm = app.state::<Arc<TranscriptionManager>>();
        let rm = app.state::<Arc<AudioRecordingManager>>();

        // Load ASR model and VAD model in parallel
        let kickoff_started = Instant::now();
        tm.initiate_model_load();
        let rm_clone = Arc::clone(&rm);
        std::thread::spawn(move || {
            if let Err(e) = rm_clone.preload_vad() {
                debug!("VAD pre-load failed: {}", e);
            }
        });
        let kickoff_elapsed = kickoff_started.elapsed();

        let binding_id = binding_id.to_string();
        let tray_started = Instant::now();
        change_tray_icon(app, TrayIconState::Recording);
        let tray_elapsed = tray_started.elapsed();

        // Get the microphone mode to determine audio feedback timing
        let plan_started = Instant::now();
        let settings = get_settings(app);
        let is_always_on = settings.always_on_microphone;

        let selected_model_info = app
            .state::<Arc<ModelManager>>()
            .get_model_info(&settings.selected_model);

        // Use the app-facing model capability as the single pre-recording source
        // for live streaming decisions. Unknown support is represented as false
        // until the model registry is updated by discovery or runtime load.
        let model_supports_streaming = selected_model_info
            .as_ref()
            .map(|m| m.supports_streaming)
            .unwrap_or(false);
        let vad_policy = if !settings.vad_enabled {
            VadPolicy::Disabled
        } else if model_supports_streaming {
            VadPolicy::Streaming
        } else {
            VadPolicy::Offline
        };
        if model_supports_streaming {
            tm.start_stream();
        }
        let plan_elapsed = plan_started.elapsed();

        // Sizing the overlay follows the same advertised capability. A model that
        // doesn't stream (or whose capability is not known yet) gets the compact
        // pill instead of an oversized transparent live window.
        let overlay_started = Instant::now();
        match settings.overlay_style {
            OverlayStyle::Live if model_supports_streaming => utils::show_streaming_overlay(app),
            OverlayStyle::Live | OverlayStyle::Minimal => show_recording_overlay(app),
            OverlayStyle::None => {} // show_overlay_state no-ops on None anyway
        }
        // Everything above runs before capture can begin, so each span here is
        // added keypress->capture latency.
        debug!(
            "start-path pre-recording steps: model_kickoff={:?} tray={:?} settings+stream_plan={:?} overlay={:?}",
            kickoff_elapsed,
            tray_elapsed,
            plan_elapsed,
            overlay_started.elapsed()
        );
        debug!("Microphone mode - always_on: {}", is_always_on);

        let mut recording_error: Option<String> = None;
        if is_always_on {
            // Always-on mode: Play audio feedback immediately, then apply mute after sound finishes
            debug!("Always-on mode: Playing audio feedback immediately");
            let rm_clone = Arc::clone(&rm);
            let app_clone = app.clone();
            std::thread::spawn(move || {
                play_feedback_sound_blocking(&app_clone, SoundType::Start);
                rm_clone.apply_mute();
            });

            if let Err(e) = rm.try_start_recording(&binding_id, vad_policy) {
                debug!("Recording failed: {}", e);
                recording_error = Some(e);
            }
        } else {
            // On-demand mode: Start recording first, then play audio feedback, then apply mute
            debug!("On-demand mode: Starting recording first, then audio feedback");
            let recording_start_time = Instant::now();
            match rm.try_start_recording(&binding_id, vad_policy) {
                Ok(()) => {
                    debug!("Recording started in {:?}", recording_start_time.elapsed());
                    let app_clone = app.clone();
                    let rm_clone = Arc::clone(&rm);
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        debug!("Handling delayed audio feedback/mute sequence");
                        play_feedback_sound_blocking(&app_clone, SoundType::Start);
                        rm_clone.apply_mute();
                    });
                }
                Err(e) => {
                    debug!("Failed to start recording: {}", e);
                    recording_error = Some(e);
                }
            }
        }

        if recording_error.is_none() {
            // Dynamically register the cancel shortcut via the session guard
            if let crate::recording_session::SessionState::Recording { session, .. } = &*app
                .state::<crate::recording_session::ManagedSessionState>()
                .lock()
                .unwrap()
            {
                session.register_cancel_shortcut();
            }

            let settings = get_settings(app);
            if settings.text_replacement_decapitalize_after_edit_key_enabled {
                crate::text_replacement_decapitalize::promote_pending_realtime_trigger_to_standard_output();
            }

            crate::recording_auto_stop::start_auto_stop_timer(app, &binding_id);
        } else {
            // Starting failed (for example due to blocked microphone permissions).
            // Revert UI state and reset the session state to Idle
            let _ = crate::recording_session::take_session(app);
            tm.cancel_stream();
            utils::hide_recording_overlay(app);
            change_tray_icon(app, TrayIconState::Idle);
            if let Some(err) = recording_error {
                let error_type = if is_microphone_access_denied(&err) {
                    "microphone_permission_denied"
                } else if is_no_input_device_error(&err) {
                    "no_input_device"
                } else {
                    "unknown"
                };
                let _ = app.emit(
                    "recording-error",
                    RecordingErrorEvent {
                        error_type: error_type.to_string(),
                        detail: Some(err),
                    },
                );
            }
        }

        debug!(
            "TranscribeAction::start completed in {:?}",
            start_time.elapsed()
        );
    }

    fn stop(&self, app: &AppHandle, binding_id: &str, _shortcut_str: &str) {
        let session = match crate::recording_session::take_session_if_matches(app, binding_id) {
            Some(s) => s,
            None => {
                debug!(
                    "TranscribeAction::stop called but no matching active session found for: {}",
                    binding_id
                );
                return;
            }
        };

        // Transition to Processing state
        {
            let state = app.state::<crate::recording_session::ManagedSessionState>();
            let mut session_state = state.lock().unwrap();
            let operation_id = match &*session_state {
                crate::recording_session::SessionState::Recording { operation_id, .. } => {
                    *operation_id
                }
                _ => crate::session_manager::next_operation_id(),
            };
            *session_state = crate::recording_session::SessionState::Processing {
                binding_id: binding_id.to_string(),
                operation_id,
            };
        }

        let settings = get_settings(app);
        if settings.text_replacement_decapitalize_after_edit_key_enabled {
            crate::text_replacement_decapitalize::begin_standard_post_recording_monitor(
                settings.text_replacement_decapitalize_standard_post_recording_monitor_ms,
            );
        }

        session.finish(); // Releases resources like cancel shortcut and mute

        let stop_time = Instant::now();
        debug!("TranscribeAction::stop called for binding: {}", binding_id);

        let ah = app.clone();
        let rm = Arc::clone(&app.state::<Arc<AudioRecordingManager>>());
        let tm = Arc::clone(&app.state::<Arc<TranscriptionManager>>());
        let hm = Arc::clone(&app.state::<Arc<HistoryManager>>());

        change_tray_icon(app, TrayIconState::Transcribing);
        // Stop should give immediate visual feedback. Live streaming can keep
        // the larger panel, but it still switches from listening to a working
        // spinner while the stream finalizes. Non-streaming paths use the
        // compact transcribing pill (None no-ops in show_*).
        let style = get_settings(app).overlay_style;
        // Capture this before finalizing the stream so every later working state
        // targets the same overlay that was shown for this transcription.
        let use_streaming_overlay = should_use_streaming_overlay(style, tm.is_streaming());
        if use_streaming_overlay {
            tm.emit_stream_working(StreamWorkKind::Transcribing);
        } else {
            show_transcribing_overlay(app);
        }

        // Play audio feedback for recording stop
        play_feedback_sound(app, SoundType::Stop);

        let binding_id = binding_id.to_string(); // Clone binding_id for the async task
        let post_process = self.post_process;
        let route_to_brain = self.route_to_brain;
        let cancel_generation = rm.cancel_generation();

        tauri::async_runtime::spawn(async move {
            let _guard = FinishGuard(ah.clone());
            debug!(
                "Starting async transcription task for binding: {}",
                binding_id
            );

            let stop_recording_time = Instant::now();
            if let Some(samples) = rm.stop_recording(&binding_id, cancel_generation) {
                debug!(
                    "Recording stopped and samples retrieved in {:?}, sample count: {}",
                    stop_recording_time.elapsed(),
                    samples.len()
                );

                if rm.was_cancelled_since(cancel_generation) {
                    debug!("Transcription operation cancelled after recording stop");
                    tm.cancel_stream();
                    utils::hide_recording_overlay(&ah);
                    change_tray_icon(&ah, TrayIconState::Idle);
                    return;
                }

                if samples.is_empty() {
                    debug!("Recording produced no audio samples; skipping persistence");
                    // Tear down any streaming worker so its channel doesn't leak
                    // and block the next start_stream.
                    tm.cancel_stream();
                    utils::hide_recording_overlay(&ah);
                    change_tray_icon(&ah, TrayIconState::Idle);
                } else {
                    // Save WAV concurrently with transcription
                    let sample_count = samples.len();
                    let file_name = format!("s2b2s-{}.wav", chrono::Utc::now().timestamp());
                    let wav_path = hm.recordings_dir().join(&file_name);
                    let wav_path_for_verify = wav_path.clone();
                    let samples_for_wav = samples.clone();
                    let samples_for_brain = samples.clone(); // Pre-clone for multimodal brain
                    let wav_handle = tauri::async_runtime::spawn_blocking(move || {
                        crate::audio_toolkit::save_wav_file(&wav_path, &samples_for_wav)
                    });

                    // Transcribe concurrently with WAV save.
                    // When multi-STT is enabled, run multiple models in parallel
                    // and merge results via LLM post-processing.
                    let transcription_time = Instant::now();
                    let settings = get_settings(&ah);
                    let transcription_result = if route_to_brain
                        && settings.brain.brain_only_transcription
                    {
                        Ok("[STT Bypassed]".to_string())
                    } else if settings.multi_stt_enabled && !settings.multi_stt_models.is_empty() {
                        let mm =
                            Arc::clone(&ah.state::<Arc<crate::managers::model::ModelManager>>());
                        multi_stt::transcribe_parallel(samples.clone(), &settings, &mm, &ah)
                    } else {
                        // Transcribe concurrently with WAV save. If a live stream was
                        // running, finalize it and use its text; otherwise batch-transcribe the samples.
                        match tm.finalize_stream() {
                            Ok(Some(text)) if !text.trim().is_empty() => Ok(text),
                            Ok(_) => tm.transcribe(samples),
                            Err(err) => Err(err),
                        }
                    };

                    // Await WAV save and verify
                    let wav_saved = match wav_handle.await {
                        Ok(Ok(())) => {
                            match crate::audio_toolkit::verify_wav_file(
                                &wav_path_for_verify,
                                sample_count,
                            ) {
                                Ok(()) => true,
                                Err(e) => {
                                    error!("WAV verification failed: {}", e);
                                    false
                                }
                            }
                        }
                        Ok(Err(e)) => {
                            error!("Failed to save WAV file: {}", e);
                            false
                        }
                        Err(e) => {
                            error!("WAV save task panicked: {}", e);
                            false
                        }
                    };

                    if rm.was_cancelled_since(cancel_generation) {
                        debug!("Transcription operation cancelled before output handling");
                        utils::hide_recording_overlay(&ah);
                        change_tray_icon(&ah, TrayIconState::Idle);
                        return;
                    }

                    match transcription_result {
                        Ok(transcription) => {
                            debug!(
                                "Transcription completed in {:?}: '{}'",
                                transcription_time.elapsed(),
                                transcription
                            );

                            let check_cancelled = || {
                                let state =
                                    ah.state::<crate::recording_session::ManagedSessionState>();
                                let session_state = state.lock().unwrap();
                                !matches!(&*session_state, crate::recording_session::SessionState::Processing { binding_id: ref bid, .. } if bid == &binding_id)
                            };

                            if check_cancelled() {
                                debug!("Transcription completed but session is no longer in Processing state. Aborting.");
                                return;
                            }

                            if route_to_brain {
                                // S2B2S conversation loop: persist the raw transcript,
                                // then hand it to the Brain, which streams the reply
                                // (and speaks it when read-aloud is on). No paste.
                                let settings = get_settings(&ah);
                                let use_post_process = settings.post_process_enabled;

                                let processed = if use_post_process {
                                    show_processing_overlay(&ah);
                                    let op_id = ah.try_state::<Arc<crate::llm_operation::LlmOperationTracker>>().map(|t| t.start_operation());
                                    let Some(res) = complete_unless_cancelled(
                                        process_transcription_output(
                                            &ah,
                                            &transcription,
                                            true,
                                            op_id,
                                        ),
                                        || rm.was_cancelled_since(cancel_generation),
                                    )
                                    .await
                                    else {
                                        debug!("Transcription operation cancelled during output handling");
                                        utils::hide_recording_overlay(&ah);
                                        change_tray_icon(&ah, TrayIconState::Idle);
                                        return;
                                    };
                                    if check_cancelled() {
                                        debug!("Post-processing completed but session is no longer in Processing state. Aborting.");
                                        return;
                                    }
                                    res
                                } else {
                                    let mut final_text = transcription.to_string();
                                    let effective_language =
                                        crate::actions::resolve_effective_language(&ah, &settings);
                                    if let Some(converted_text) = maybe_convert_chinese_variant(
                                        &effective_language,
                                        &transcription,
                                    )
                                    .await
                                    {
                                        final_text = converted_text;
                                    }
                                    ProcessedTranscription {
                                        final_text,
                                        post_processed_text: None,
                                        post_process_prompt: None,
                                    }
                                };

                                if wav_saved {
                                    let stt_model = tm.get_current_model();
                                    let stt_duration =
                                        Some(transcription_time.elapsed().as_millis() as i64);
                                    if let Err(err) = hm.save_entry(
                                        file_name,
                                        transcription.clone(),
                                        use_post_process,
                                        processed.post_processed_text.clone(),
                                        processed.post_process_prompt.clone(),
                                        "stt".to_string(),
                                        stt_model,
                                        None,
                                        stt_duration,
                                    ) {
                                        error!("Failed to save history entry: {}", err);
                                    }
                                }
                                utils::hide_recording_overlay(&ah);
                                change_tray_icon(&ah, TrayIconState::Idle);

                                if !processed.final_text.trim().is_empty() {
                                    // Surface the spoken question in the Conversation view with STT timing
                                    let stt_ms = transcription_time.elapsed().as_millis() as u64;
                                    let asked_payload = serde_json::json!({
                                        "text": processed.final_text,
                                        "stt_ms": stt_ms,
                                    });
                                    let _ = ah.emit("brain:asked", &asked_payload);

                                    // Show the brain overlay if enabled in settings
                                    {
                                        let settings = crate::settings::get_settings(&ah);
                                        if settings.overlay_window.reply_bubble {
                                            crate::overlay_fx::window::show_brain_overlay(&ah);
                                            let _ = ah.emit("overlay:state", crate::overlay_fx::events::OverlayState::new(
                                                crate::overlay_fx::events::OverlayPhase::Listening,
                                            ));
                                        }
                                    }

                                    // When brain_only_transcription is on, bypass STT output and
                                    // send the fixed transcription prompt + raw audio to the Brain.
                                    // Gemma 4 handles both transcription and response natively.
                                    let is_brain_only = settings.brain.brain_only_transcription;
                                    let text_to_ask = if is_brain_only {
                                        crate::settings::BRAIN_ONLY_TRANSCRIPTION_PROMPT.to_string()
                                    } else {
                                        processed.final_text.clone()
                                    };
                                    // When brain-only mode is on, multimodal audio is always required
                                    let multimodal_audio =
                                        is_brain_only || settings.brain.multimodal_audio_enabled;

                                    // Forward the (effective) STT language so the Brain replies
                                    // in the language it was spoken to (speech-to-speech lang_prompt).
                                    let reply_language = resolve_reply_language(&ah, &settings);

                                    if let Some(bm) =
                                        ah.try_state::<Arc<crate::brain::manager::BrainManager>>()
                                    {
                                        let bm = bm.inner().clone();
                                        let text_to_ask = text_to_ask;
                                        let reply_language = reply_language.clone();
                                        let sample_count = samples_for_brain.len();
                                        tauri::async_runtime::spawn(async move {
                                            let result = if multimodal_audio {
                                                if is_brain_only {
                                                    info!(
                                                        "[Conversation] Brain-only transcription mode — encoding {} samples ({:.2}s) to WAV, bypassing STT, sending fixed prompt + audio to Gemma 4",
                                                        sample_count,
                                                        sample_count as f64 / 16000.0
                                                    );
                                                } else {
                                                    info!(
                                                        "[Conversation] Multimodal audio enabled — encoding {} samples ({:.2}s) to WAV for Gemma 4",
                                                        sample_count,
                                                        sample_count as f64 / 16000.0
                                                    );
                                                }
                                                let wav_bytes =
                                                    tokio::task::spawn_blocking(move || {
                                                        crate::audio_toolkit::encode_wav_bytes(
                                                            &samples_for_brain,
                                                        )
                                                    })
                                                    .await;
                                                match wav_bytes {
                                                    Ok(Ok(bytes)) => {
                                                        use base64::Engine;
                                                        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                                                        info!(
                                                            "[Conversation] WAV encoded — {} bytes raw, {} base64 — sending to ask_multimodal",
                                                            bytes.len(),
                                                            b64.len()
                                                        );
                                                        bm.ask_multimodal(
                                                            text_to_ask,
                                                            Some(b64),
                                                            None,
                                                            reply_language.clone(),
                                                        )
                                                        .await
                                                    }
                                                    Ok(Err(e)) => {
                                                        error!("Failed to encode WAV for multimodal brain: {e}");
                                                        bm.ask(text_to_ask).await
                                                    }
                                                    Err(e) => {
                                                        error!("spawn_blocking panicked for WAV encoding: {e}");
                                                        bm.ask(text_to_ask).await
                                                    }
                                                }
                                            } else {
                                                info!("[Conversation] Multimodal audio disabled — text-only ask");
                                                bm.ask(text_to_ask).await
                                            };
                                            if let Err(e) = result {
                                                error!("Brain ask failed: {e}");
                                            }
                                        });
                                    } else {
                                        error!("BrainManager not initialized");
                                    }
                                }
                                return;
                            }

                            if post_process {
                                if use_streaming_overlay {
                                    tm.emit_stream_working(StreamWorkKind::Polishing);
                                } else {
                                    show_processing_overlay(&ah);
                                }
                            }
                            let op_id = ah
                                .try_state::<Arc<crate::llm_operation::LlmOperationTracker>>()
                                .map(|t| t.start_operation());
                            let Some(processed) = complete_unless_cancelled(
                                process_transcription_output(
                                    &ah,
                                    &transcription,
                                    post_process,
                                    op_id,
                                ),
                                || rm.was_cancelled_since(cancel_generation),
                            )
                            .await
                            else {
                                debug!("Transcription operation cancelled during output handling");
                                utils::hide_recording_overlay(&ah);
                                change_tray_icon(&ah, TrayIconState::Idle);
                                return;
                            };

                            if check_cancelled() {
                                debug!("Post-processing completed but session is no longer in Processing state. Aborting.");
                                return;
                            }

                            if rm.was_cancelled_since(cancel_generation) {
                                debug!("Transcription operation cancelled before paste");
                                utils::hide_recording_overlay(&ah);
                                change_tray_icon(&ah, TrayIconState::Idle);
                                return;
                            }

                            // Save to history if WAV was saved
                            if wav_saved {
                                let stt_model = tm.get_current_model();
                                let stt_duration =
                                    Some(transcription_time.elapsed().as_millis() as i64);
                                if let Err(err) = hm.save_entry(
                                    file_name,
                                    transcription,
                                    post_process,
                                    processed.post_processed_text.clone(),
                                    processed.post_process_prompt.clone(),
                                    "stt".to_string(),
                                    stt_model,
                                    None,
                                    stt_duration,
                                ) {
                                    error!("Failed to save history entry: {}", err);
                                }
                            }

                            if processed.final_text.is_empty() {
                                utils::hide_recording_overlay(&ah);
                                change_tray_icon(&ah, TrayIconState::Idle);
                            } else {
                                let ah_clone = ah.clone();
                                let paste_time = Instant::now();
                                let final_text = processed.final_text;
                                let rm_for_paste = Arc::clone(&rm);
                                ah.run_on_main_thread(move || {
                                    if rm_for_paste.was_cancelled_since(cancel_generation) {
                                        debug!("Transcription operation cancelled before paste");
                                        utils::hide_recording_overlay(&ah_clone);
                                        change_tray_icon(&ah_clone, TrayIconState::Idle);
                                        return;
                                    }

                                    match utils::paste(final_text, ah_clone.clone()) {
                                        Ok(()) => {
                                            debug!(
                                                "Text pasted successfully in {:?}",
                                                paste_time.elapsed()
                                            );
                                            crate::audio_feedback::play_result_ready_sound(
                                                &ah_clone,
                                            );
                                        }
                                        Err(e) => {
                                            error!("Failed to paste transcription: {}", e);
                                            let _ = ah_clone.emit("paste-error", ());
                                        }
                                    }
                                    utils::hide_recording_overlay(&ah_clone);
                                    change_tray_icon(&ah_clone, TrayIconState::Idle);
                                })
                                .unwrap_or_else(|e| {
                                    error!("Failed to run paste on main thread: {:?}", e);
                                    utils::hide_recording_overlay(&ah);
                                    change_tray_icon(&ah, TrayIconState::Idle);
                                });
                            }
                        }
                        Err(err) => {
                            if rm.was_cancelled_since(cancel_generation) {
                                debug!(
                                    "Transcription operation cancelled after transcription error"
                                );
                                utils::hide_recording_overlay(&ah);
                                change_tray_icon(&ah, TrayIconState::Idle);
                                return;
                            }

                            error!("Transcription failed: {}", err);
                            // Surface the failure to the UI (toast). The full
                            // message is also in handy.log via the line above.
                            let _ = ah.emit("transcription-error", err.to_string());
                            // Save entry with empty text so user can retry
                            if wav_saved {
                                let stt_model = tm.get_current_model();
                                if let Err(save_err) = hm.save_entry(
                                    file_name,
                                    String::new(),
                                    post_process,
                                    None,
                                    None,
                                    "stt".to_string(),
                                    stt_model,
                                    None,
                                    None,
                                ) {
                                    error!("Failed to save failed history entry: {}", save_err);
                                }
                            }
                            utils::hide_recording_overlay(&ah);
                            change_tray_icon(&ah, TrayIconState::Idle);
                        }
                    }
                }
            } else {
                debug!("No samples retrieved from recording stop");
                // Tear down any streaming worker so its channel doesn't leak.
                tm.cancel_stream();
                utils::hide_recording_overlay(&ah);
                change_tray_icon(&ah, TrayIconState::Idle);
            }
        });

        debug!(
            "TranscribeAction::stop completed in {:?}",
            stop_time.elapsed()
        );
    }
}

// Speak Selection Action — CopySpeak "Read Anywhere": capture the selected
// text (falling back to the clipboard) and read it aloud. Pressing the
// shortcut while speech is playing stops playback instead (toggle).
struct SpeakSelectionAction;

impl ShortcutAction for SpeakSelectionAction {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        let app = app.clone();
        // Selection capture simulates a copy keystroke and sleeps; keep it off
        // the shortcut-dispatch thread.
        std::thread::spawn(move || {
            let Some(tts) = app.try_state::<Arc<crate::tts::manager::TtsManager>>() else {
                error!("TtsManager not initialized");
                return;
            };
            if tts.is_playing() {
                tts.stop();
                return;
            }
            match crate::clipboard::capture_selection_text(&app) {
                Ok(text) => tts.speak(text),
                Err(e) => {
                    warn!("Speak selection failed: {e}");
                    let _ = app.emit("tts:error", e);
                }
            }
        });
    }

    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        // Nothing to do on release.
    }
}

// Cancel Action
struct CancelAction;

impl ShortcutAction for CancelAction {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        utils::cancel_current_operation(app);
    }

    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        // Nothing to do on stop for cancel
    }
}

// Test Action
struct TestAction;

impl ShortcutAction for TestAction {
    fn start(&self, app: &AppHandle, binding_id: &str, shortcut_str: &str) {
        log::info!(
            "Shortcut ID '{}': Started - {} (App: {})", // Changed "Pressed" to "Started" for consistency
            binding_id,
            shortcut_str,
            app.package_info().name
        );
    }

    fn stop(&self, app: &AppHandle, binding_id: &str, shortcut_str: &str) {
        log::info!(
            "Shortcut ID '{}': Stopped - {} (App: {})", // Changed "Released" to "Stopped" for consistency
            binding_id,
            shortcut_str,
            app.package_info().name
        );
    }
}

struct TogglePauseAction;

impl ShortcutAction for TogglePauseAction {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        let audio_manager = app.state::<Arc<AudioRecordingManager>>();
        if audio_manager.is_recording() {
            let is_paused = audio_manager.toggle_pause();
            let _ = app.emit("recording_pause_changed", is_paused);
            if is_paused {
                utils::show_paused_overlay(app);
            } else {
                utils::show_recording_overlay(app);
            }
        }
    }

    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        // Nothing to do on release
    }
}

fn substitute_context_variables(prompt: &str) -> String {
    let mut substituted = prompt.to_string();

    // 1. ${current_app}
    if substituted.contains("${current_app}") {
        let app_name = crate::active_app::get_frontmost_app_name()
            .unwrap_or_else(|| "Unknown Application".to_string());
        substituted = substituted.replace("${current_app}", &app_name);
    }

    // 2. ${time_local}
    if substituted.contains("${time_local}") {
        let local_time = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        substituted = substituted.replace("${time_local}", &local_time);
    }

    substituted
}

/// Id of the post-process action selected for the in-flight transcription,
/// set by the coordinator right before the pipeline stops.
pub struct ActiveActionState(pub std::sync::Mutex<Option<String>>);

// Static Action Map
pub static ACTION_MAP: Lazy<HashMap<String, Arc<dyn ShortcutAction>>> = Lazy::new(|| {
    let mut map = HashMap::new();
    map.insert(
        "transcribe".to_string(),
        Arc::new(TranscribeAction {
            post_process: false,
            route_to_brain: false,
        }) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "transcribe_with_post_process".to_string(),
        Arc::new(TranscribeAction {
            post_process: true,
            route_to_brain: false,
        }) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "converse".to_string(),
        Arc::new(TranscribeAction {
            post_process: false,
            route_to_brain: true,
        }) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "speak_selection".to_string(),
        Arc::new(SpeakSelectionAction) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "cancel".to_string(),
        Arc::new(CancelAction) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "toggle_pause".to_string(),
        Arc::new(TogglePauseAction) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "test".to_string(),
        Arc::new(TestAction) as Arc<dyn ShortcutAction>,
    );
    map
});

#[cfg(test)]
mod tests {
    use super::{complete_unless_cancelled, is_blank_transcription, should_use_streaming_overlay};
    use crate::settings::OverlayStyle;
    use std::future;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn blank_transcription_is_detected() {
        assert!(is_blank_transcription(""));
        assert!(is_blank_transcription("   "));
        assert!(is_blank_transcription("\t\n  \r\n"));
    }

    #[test]
    fn non_blank_transcription_is_kept() {
        assert!(!is_blank_transcription("hello"));
        assert!(!is_blank_transcription("  hello  "));
    }

    #[test]
    fn completed_operation_returns_its_output() {
        let result = tauri::async_runtime::block_on(complete_unless_cancelled(
            future::ready("done"),
            || false,
        ));

        assert_eq!(result, Some("done"));
    }

    #[test]
    fn pending_operation_stops_after_cancellation() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancelled_for_thread = Arc::clone(&cancelled);
        let cancel_thread = thread::spawn(move || {
            thread::sleep(Duration::from_millis(10));
            cancelled_for_thread.store(true, Ordering::Release);
        });

        let result = tauri::async_runtime::block_on(complete_unless_cancelled(
            future::pending::<()>(),
            || cancelled.load(Ordering::Acquire),
        ));

        cancel_thread.join().unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn live_overlay_uses_streaming_states_only_for_streaming_models() {
        assert!(should_use_streaming_overlay(OverlayStyle::Live, true));
        assert!(!should_use_streaming_overlay(OverlayStyle::Live, false));
        assert!(!should_use_streaming_overlay(OverlayStyle::Minimal, true));
        assert!(!should_use_streaming_overlay(OverlayStyle::None, true));
    }
}
