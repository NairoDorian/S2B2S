use crate::TranscriptionCoordinator;
use crate::app_identity;
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use crate::apple_intelligence;
use crate::audio_feedback::{SoundType, play_feedback_sound, play_feedback_sound_blocking};
use crate::audio_toolkit::{VadPolicy, is_microphone_access_denied, is_no_input_device_error};
use crate::managers::audio::{AudioRecordingManager, RecordingReadiness, StopRecordingResult};
use crate::managers::history::HistoryManager;
use crate::managers::model::ModelManager;
use crate::managers::statistics::{StatisticsManager, StatisticsRunStatus};
use crate::managers::transcription::{
    StreamFinalization, StreamWorkKind, TrackedTranscription, TranscriptionManager,
};
use crate::settings::{
    APPLE_INTELLIGENCE_PROVIDER_ID, AppSettings, OverlayStyle, PasteMethod, get_settings,
};
use crate::shortcut;
use crate::tray::{TrayIconState, set_tray_state};
use crate::utils::{
    self, show_processing_overlay, show_recording_overlay, show_transcribing_overlay,
};
use ferrous_opencc::{OpenCC, config::BuiltinConfig};
use log::{debug, error, info, warn};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
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
        // The pipeline just freed its large transient buffers (captured PCM,
        // WAV copy, engine scratch); hand the cached pages back to the OS so
        // they don't sit in malloc arenas until they get swapped out (#1792).
        crate::memory::trim_freed_memory();
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
}

/// Field name for structured output JSON schema
const TRANSCRIPTION_FIELD: &str = "transcription";

/// Strip invisible Unicode characters that some LLMs may insert
fn strip_invisible_chars(s: &str) -> String {
    s.replace(['\u{200B}', '\u{200C}', '\u{200D}', '\u{FEFF}'], "")
}

/// Strip a leading `<think>...</think>` block. Some endpoints can't disable
/// reasoning, and some local servers put the reasoning text into `content`
/// instead of a separate field — without this the user would get the model's
/// chain of thought pasted along with the cleaned transcription.
fn strip_think_block(s: &str) -> &str {
    if let Some(rest) = s.trim_start().strip_prefix("<think>")
        && let Some(end) = rest.find("</think>")
    {
        return rest[end + "</think>".len()..].trim_start();
    }
    s
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

/// Direct streaming types the live text into the foreground app only for
/// plain transcription. When the final text will be post-processed or merged
/// (Multi-STT), typing the raw stream first would leave two versions in the
/// app, so the stream becomes a *preview only*: it is shown in the Live
/// overlay and the final text is pasted through the default clipboard paste.
fn live_stream_is_preview_only(settings: &AppSettings, final_text_is_processed: bool) -> bool {
    final_text_is_processed && settings.paste_method == PasteMethod::DirectStreaming
}

/// The overlay to show: the user's choice, except that a preview-only live
/// stream forces the Live overlay (when the model can stream) so the user
/// still sees the transcript as it forms.
fn effective_overlay_style(
    settings: &AppSettings,
    preview_only: bool,
    model_supports_streaming: bool,
) -> OverlayStyle {
    if preview_only && model_supports_streaming {
        OverlayStyle::Live
    } else {
        settings.overlay_style
    }
}

/// Paste method override for the final text: a preview-only live stream
/// pastes its processed result with Ctrl+V instead of typing it.
fn final_paste_method(preview_only: bool) -> Option<PasteMethod> {
    preview_only.then_some(PasteMethod::CtrlV)
}

/// Multi-STT performance mode, on an exit path after the "full power"
/// shortcut was sent at recording start (`trigger_on_start`): send the
/// "normal" one so the external power profile is not left at full power.
fn restore_power_if_triggered_on_start(app: &AppHandle) {
    let settings = get_settings(app);
    if settings.multi_stt_performance_mode_enabled
        && settings.multi_stt_performance_mode_trigger_on_start
    {
        restore_normal_power(app, settings.multi_stt_performance_mode_normal_shortcut);
    }
}

/// Multi-STT performance mode: simulate the "normal power" shortcut off the
/// current thread. Used on exit paths that otherwise would leave the external
/// power profile stuck in "full power".
fn restore_normal_power(app: &AppHandle, normal_shortcut: String) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::clipboard::simulate_key_combination(&app, &normal_shortcut);
    });
}

/// Wait (off-thread) for the first real microphone samples, then emit the
/// overlay's `recording-ready` cue, play the start chime and apply mute — all
/// guarded by the readiness generation so a recording that was stopped or
/// cancelled in the meantime never gets a stale cue.
///
/// `TranscribeAction::start` carries an inline copy of this sequence (kept
/// inline there to stay diff-compatible with upstream); `MultiSttAction` and
/// the overlay preview use this helper. The two differ in one place only: the
/// inline copy also has the debug-only `DEBUG_MIC_READY_DELAY_MS` preview hook,
/// which this helper does not.
pub(crate) fn spawn_recording_ready_cue(
    app: &AppHandle,
    rm: &Arc<AudioRecordingManager>,
    readiness: RecordingReadiness,
) {
    let generation = readiness.generation();
    let app_clone = app.clone();
    let rm_clone = Arc::clone(rm);
    std::thread::spawn(move || {
        if !readiness.wait() {
            debug!("Microphone readiness wait ended without receiving samples");
            return;
        }
        if !rm_clone.is_recording_readiness_current(generation) {
            debug!("Microphone became ready for an inactive recording");
            return;
        }
        debug!("Microphone is receiving samples; recording is ready");
        utils::emit_recording_ready(&app_clone);
        if rm_clone.is_recording_readiness_current(generation) {
            play_feedback_sound_blocking(&app_clone, SoundType::Start);
        }
        if rm_clone.is_recording_readiness_current(generation) {
            rm_clone.apply_mute();
        }
    });
}

async fn post_process_transcription(settings: &AppSettings, transcription: &str) -> Option<String> {
    if is_blank_transcription(transcription) {
        debug!("Post-processing skipped because the transcription is empty");
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

    let prompt = match settings
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

    if prompt.trim().is_empty() {
        debug!("Post-processing skipped because the selected prompt is empty");
        return None;
    }

    debug!(
        "Starting LLM post-processing with provider '{}' (model: {})",
        provider.id, model
    );

    let api_key = settings
        .post_process_api_keys
        .get(&provider.id)
        .cloned()
        .unwrap_or_default();

    // Ask these providers to skip reasoning/thinking — post-processing rarely
    // benefits from it and it adds seconds of latency. llm_client picks the
    // field the endpoint understands and retries without it if rejected.
    let disable_reasoning = matches!(provider.id.as_str(), "custom" | "openrouter");

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
                return match apple_intelligence::process_text_with_system_prompt(
                    &system_prompt,
                    &user_content,
                    token_limit,
                ) {
                    Ok(result) => {
                        if result.trim().is_empty() {
                            debug!("Apple Intelligence returned an empty response");
                            None
                        } else {
                            let result = strip_invisible_chars(&result);
                            debug!(
                                "Apple Intelligence post-processing succeeded. Output length: {} chars",
                                result.chars().count()
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

        match crate::llm_client::send_chat_completion_with_schema(
            &provider,
            api_key.clone(),
            &model,
            user_content,
            Some(system_prompt),
            Some(json_schema),
            disable_reasoning,
        )
        .await
        {
            Ok(Some(content)) => {
                // Parse the JSON response to extract the transcription field
                let content = strip_think_block(&content);
                match serde_json::from_str::<serde_json::Value>(content) {
                    Ok(json) => {
                        if let Some(transcription_value) =
                            json.get(TRANSCRIPTION_FIELD).and_then(|t| t.as_str())
                        {
                            let result = strip_invisible_chars(transcription_value);
                            debug!(
                                "Structured output post-processing succeeded for provider '{}'. Output length: {} chars",
                                provider.id,
                                result.chars().count()
                            );
                            return Some(result);
                        } else {
                            error!("Structured output response missing 'transcription' field");
                            return Some(strip_invisible_chars(content));
                        }
                    }
                    Err(e) => {
                        error!(
                            "Failed to parse structured output JSON: {}. Returning raw content.",
                            e
                        );
                        return Some(strip_invisible_chars(content));
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
    debug!(
        "Processed prompt length: {} chars",
        processed_prompt.chars().count()
    );

    match crate::llm_client::send_chat_completion(
        &provider,
        api_key,
        &model,
        processed_prompt,
        disable_reasoning,
    )
    .await
    {
        Ok(Some(content)) => {
            let content = strip_invisible_chars(strip_think_block(&content));
            debug!(
                "LLM post-processing succeeded for provider '{}'. Output length: {} chars",
                provider.id,
                content.chars().count()
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
                provider.id, e
            );
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
            error!(
                "Failed to initialize OpenCC converter: {}. Falling back to original transcription.",
                e
            );
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

pub(crate) async fn process_transcription_output(
    app: &AppHandle,
    transcription: &str,
    post_process: bool,
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

    if post_process {
        if let Some(processed_text) = post_process_transcription(&settings, &final_text).await {
            post_processed_text = Some(processed_text.clone());
            final_text = processed_text;

            if let Some(prompt_id) = &settings.post_process_selected_prompt_id
                && let Some(prompt) = settings
                    .post_process_prompts
                    .iter()
                    .find(|prompt| &prompt.id == prompt_id)
            {
                post_process_prompt = Some(prompt.prompt.clone());
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

/// Minimum measured speech in a recording for it to be worth decoding.
///
/// Below this, running a model costs a GPU decode and risks something worse
/// than nothing: Whisper-family models hallucinate confidently on silence, and
/// that invented text goes straight into whatever the user was typing in.
/// Silence measures 0 ms exactly, so this only has to clear stray onset frames
/// while staying under the shortest real word — silero-vad's reference
/// `min_speech_duration_ms` is 250 ms, and this sits deliberately below it so
/// a clipped "yes" still transcribes.
///
/// With VAD disabled every frame counts as speech, so this can never suppress a
/// recording the user made with filtering turned off.
pub(crate) const MIN_SPEECH_MS_TO_TRANSCRIBE: u64 = 200;

impl ShortcutAction for TranscribeAction {
    fn start(&self, app: &AppHandle, binding_id: &str, _shortcut_str: &str) {
        let start_time = Instant::now();
        debug!("TranscribeAction::start called for binding: {}", binding_id);

        // Load model in the background
        let tm = app.state::<Arc<TranscriptionManager>>();
        let rm = app.state::<Arc<AudioRecordingManager>>();

        // Load the ASR model and build the recorder (Earshot VAD) in parallel
        let kickoff_started = Instant::now();
        tm.initiate_model_load();
        let rm_clone = Arc::clone(&rm);
        std::thread::spawn(move || {
            if let Err(e) = rm_clone.preload_vad() {
                warn!("VAD pre-load failed: {}", e);
            }
        });
        let kickoff_elapsed = kickoff_started.elapsed();

        let binding_id = binding_id.to_string();
        let tray_started = Instant::now();
        set_tray_state(app, TrayIconState::Recording);
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
        // With post-processing the live stream is a preview only (see
        // `live_stream_is_preview_only`): no live typing, Live overlay forced.
        // Recall insertion mode is preview-only too — the final text is
        // inserted at the note's caret, so typing the stream into the
        // focused app would put it there twice.
        let preview_only = live_stream_is_preview_only(&settings, self.post_process)
            || crate::recall::insertion::is_armed();
        let statistics_manager = app.state::<Arc<StatisticsManager>>();
        let statistics = statistics_manager.begin_normal_run(&binding_id);
        if model_supports_streaming {
            tm.start_stream(!preview_only, statistics.clone());
        }
        let plan_elapsed = plan_started.elapsed();

        let overlay_started = Instant::now();
        match effective_overlay_style(&settings, preview_only, model_supports_streaming) {
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
        let recording_start_time = Instant::now();
        match rm.try_start_recording(&binding_id, vad_policy) {
            Ok(readiness) => {
                debug!(
                    "Recording request accepted in {:?}; waiting for first microphone samples",
                    recording_start_time.elapsed()
                );
                let generation = readiness.generation();
                let app_clone = app.clone();
                let rm_clone = Arc::clone(&rm);
                std::thread::spawn(move || {
                    if !readiness.wait() {
                        debug!("Microphone readiness wait ended without receiving samples");
                        return;
                    }

                    // Development-only preview hook for evaluating the brief
                    // arming animation on hardware that normally starts too fast
                    // to make it visible.
                    #[cfg(debug_assertions)]
                    if let Ok(delay_ms) = utils::app_env_var("DEBUG_MIC_READY_DELAY_MS")
                        .unwrap_or_default()
                        .parse::<u64>()
                    {
                        let delay_ms = delay_ms.min(10_000);
                        if delay_ms > 0 {
                            debug!("Delaying microphone-ready cue by {delay_ms}ms for UI preview");
                            std::thread::sleep(Duration::from_millis(delay_ms));
                        }
                    }

                    if !rm_clone.is_recording_readiness_current(generation) {
                        debug!("Microphone became ready for an inactive recording");
                        return;
                    }

                    debug!("Microphone is receiving samples; recording is ready");
                    utils::emit_recording_ready(&app_clone);

                    // The start chime is a readiness cue, so it must follow the
                    // first real input callback rather than Stream::play() or a
                    // fixed delay. The helper returns immediately when feedback
                    // is disabled; mute still follows the same readiness point.
                    if rm_clone.is_recording_readiness_current(generation) {
                        play_feedback_sound_blocking(&app_clone, SoundType::Start);
                    }
                    if rm_clone.is_recording_readiness_current(generation) {
                        rm_clone.apply_mute();
                    }
                });
            }
            Err(e) => {
                debug!("Failed to start recording: {}", e);
                recording_error = Some(e);
            }
        }

        if recording_error.is_none() {
            // Dynamically register the cancel shortcut in a separate task to avoid deadlock
            shortcut::register_cancel_shortcut(app);
        } else {
            statistics.finish(StatisticsRunStatus::Failed);
            // Starting failed (for example due to blocked microphone permissions).
            // Revert UI state so we don't stay stuck in the recording overlay.
            tm.cancel_stream();
            utils::hide_recording_overlay(app);
            set_tray_state(app, TrayIconState::Idle);
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
        // Prevent a slow microphone from emitting a ready event or start chime
        // after the user has already requested stop.
        app.state::<Arc<AudioRecordingManager>>()
            .invalidate_recording_readiness();

        // Unregister the cancel shortcut when transcription stops
        shortcut::unregister_cancel_shortcut(app);

        let stop_time = Instant::now();
        debug!("TranscribeAction::stop called for binding: {}", binding_id);

        let ah = app.clone();
        let rm = Arc::clone(&app.state::<Arc<AudioRecordingManager>>());
        let tm = Arc::clone(&app.state::<Arc<TranscriptionManager>>());
        let hm = Arc::clone(&app.state::<Arc<HistoryManager>>());
        let sm = Arc::clone(&app.state::<Arc<StatisticsManager>>());
        let statistics = sm
            .get_normal_run(binding_id)
            .unwrap_or_else(|| sm.begin_normal_run(binding_id));
        statistics.mark_input_stopped(stop_time, self.post_process);

        set_tray_state(app, TrayIconState::Transcribing);
        // Stop should give immediate visual feedback. Live streaming can keep
        // the larger panel, but it still switches from listening to a working
        // spinner while the stream finalizes. Non-streaming paths use the
        // compact transcribing pill (None no-ops in show_*).
        let stop_settings = get_settings(app);
        // The same test `start` used, Recall insertion included, so the stop
        // path targets the overlay that was actually shown.
        let preview_only = live_stream_is_preview_only(&stop_settings, self.post_process)
            || crate::recall::insertion::is_armed();
        let style = effective_overlay_style(&stop_settings, preview_only, tm.is_streaming());
        // Capture this before finalizing the stream so every later working state
        // targets the same overlay that was shown for this transcription.
        let use_streaming_overlay = should_use_streaming_overlay(style, tm.is_streaming());
        if use_streaming_overlay {
            tm.emit_stream_working(StreamWorkKind::Transcribing);
        } else {
            show_transcribing_overlay(app);
        }

        // Unmute before playing audio feedback so the stop sound is audible
        rm.remove_mute();

        // Play audio feedback for recording stop
        play_feedback_sound(app, SoundType::Stop);

        let binding_id = binding_id.to_string(); // Clone binding_id for the async task
        let post_process = self.post_process;
        let cancel_generation = rm.cancel_generation();

        tauri::async_runtime::spawn(async move {
            let _guard = FinishGuard(ah.clone());
            debug!(
                "Starting async transcription task for binding: {}",
                binding_id
            );

            let stop_recording_time = Instant::now();
            let recorded_res = rm.stop_recording(&binding_id, cancel_generation);
            let recorded = match recorded_res {
                StopRecordingResult::Captured {
                    recorded,
                    captured_sample_count,
                    sample_rate,
                } => {
                    statistics.set_audio(captured_sample_count, sample_rate);
                    recorded
                }
                StopRecordingResult::Cancelled => {
                    statistics.finish(StatisticsRunStatus::Cancelled);
                    tm.cancel_stream();
                    utils::hide_recording_overlay(&ah);
                    set_tray_state(&ah, TrayIconState::Idle);
                    return;
                }
                StopRecordingResult::NotActive => {
                    statistics.finish(StatisticsRunStatus::Failed);
                    // Nothing of ours was recording: a cancel got there first.
                    // Undo the "transcribing" state `stop()` has just shown,
                    // unless another recording owns the overlay by now.
                    if !rm.is_recording() {
                        tm.cancel_stream();
                        utils::hide_recording_overlay(&ah);
                        set_tray_state(&ah, TrayIconState::Idle);
                    }
                    return;
                }
                StopRecordingResult::Failed(err) => {
                    statistics.finish(StatisticsRunStatus::Failed);
                    tm.cancel_stream();
                    utils::hide_recording_overlay(&ah);
                    set_tray_state(&ah, TrayIconState::Idle);
                    error!("Recording failed: {err}");
                    return;
                }
            };

            let samples = recorded.stt_samples;
            debug!(
                "Recording stopped and samples retrieved in {:?}, STT sample count: {}, raw sample count: {}",
                stop_recording_time.elapsed(),
                samples.len(),
                recorded.raw_samples.len()
            );

            if rm.was_cancelled_since(cancel_generation) {
                debug!("Transcription operation cancelled after recording stop");
                statistics.finish(StatisticsRunStatus::Cancelled);
                tm.cancel_stream();
                utils::hide_recording_overlay(&ah);
                set_tray_state(&ah, TrayIconState::Idle);
                return;
            }

            let speech_ms = rm.last_speech_ms();
            statistics.set_speech_audio_duration_ms(speech_ms as i64, 16000);
            if samples.is_empty() || speech_ms < MIN_SPEECH_MS_TO_TRANSCRIBE {
                debug!(
                    "Recording has no usable speech ({} samples, {}ms voiced); \
                     skipping transcription",
                    samples.len(),
                    speech_ms
                );
                statistics.finish(StatisticsRunStatus::Empty);
                // Tear down any streaming worker so its channel doesn't leak
                // and block the next start_stream.
                tm.cancel_stream();
                utils::hide_recording_overlay(&ah);
                set_tray_state(&ah, TrayIconState::Idle);
            } else {
                let settings = get_settings(&ah);
                let raw_count = recorded.raw_samples.len();
                let (samples_for_wav, sample_count, save_is_raw, raw_rate, raw_format) =
                    if settings.save_raw_audio && raw_count > 0 {
                        (
                            recorded.raw_samples,
                            raw_count,
                            true,
                            recorded.native_sample_rate,
                            recorded.native_sample_format,
                        )
                    } else {
                        (
                            samples.clone(),
                            samples.len(),
                            false,
                            16000,
                            cpal::SampleFormat::I16,
                        )
                    };

                // Save WAV concurrently with transcription
                let file_name = app_identity::recording_file_name(chrono::Utc::now().timestamp());
                let wav_path = hm.recordings_dir().join(&file_name);
                let wav_path_for_verify = wav_path.clone();
                let wav_handle = tauri::async_runtime::spawn_blocking(move || {
                    if save_is_raw {
                        crate::audio_toolkit::save_raw_wav_file(
                            &wav_path,
                            &samples_for_wav,
                            raw_rate,
                            raw_format,
                        )
                    } else {
                        crate::audio_toolkit::save_wav_file(&wav_path, &samples_for_wav)
                    }
                });

                // Transcribe concurrently with WAV save. If a live stream was
                // running, finalize it and use its text (all audio was already
                // fed to the stream); otherwise batch-transcribe the samples.
                let transcription_time = Instant::now();
                let stream_finalized = tm.finalize_stream();
                // Plain transcription with direct streaming: the writer
                // already typed the text, so no final paste follows. With
                // post-processing the stream was preview-only (nothing was
                // typed) and the polished text is pasted below.
                let was_direct_stream_written = matches!(&stream_finalized, StreamFinalization::Completed(t) if !t.text.trim().is_empty())
                    && settings.paste_method == PasteMethod::DirectStreaming
                    && !preview_only;

                let transcription_result = match stream_finalized {
                    // A finalized stream with usable text wins. An empty result
                    // (no active stream, produced nothing, or a finalize error
                    // after the engine was returned) falls back to a full batch
                    // transcription of the same audio. A finalize timeout is
                    // surfaced instead — the worker may still hold the engine,
                    // so a batch fallback would contend with it.
                    StreamFinalization::Completed(tracked) if !tracked.text.trim().is_empty() => {
                        Ok(tracked)
                    }
                    StreamFinalization::Completed(_) | StreamFinalization::NeverStarted => {
                        tm.transcribe_tracked(samples, statistics.clone())
                    }
                    StreamFinalization::Failed(err) => {
                        warn!("Stream failed: {err}; falling back to batch transcription");
                        tm.transcribe_tracked(samples, statistics.clone())
                    }
                    StreamFinalization::Timeout(err) => Err(anyhow::anyhow!(err)),
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
                    if let Ok(ref tracked) = transcription_result {
                        tracked.attempt.finish(StatisticsRunStatus::Cancelled);
                    }
                    statistics.finish(StatisticsRunStatus::Cancelled);
                    utils::hide_recording_overlay(&ah);
                    set_tray_state(&ah, TrayIconState::Idle);
                    return;
                }

                match transcription_result {
                    Ok(tracked) => {
                        let transcription = tracked.text;
                        let attempt = tracked.attempt;
                        debug!(
                            "Transcription completed in {:?}: '{}'",
                            transcription_time.elapsed(),
                            utils::redact_text(&transcription)
                        );

                        let stt_latency_ms = transcription_time.elapsed().as_secs_f64() * 1000.0;
                        let post_process_start = Instant::now();

                        if post_process {
                            if use_streaming_overlay {
                                tm.emit_stream_working(StreamWorkKind::Polishing);
                            } else {
                                show_processing_overlay(&ah);
                            }
                        }
                        let Some(processed) = complete_unless_cancelled(
                            process_transcription_output(&ah, &transcription, post_process),
                            || rm.was_cancelled_since(cancel_generation),
                        )
                        .await
                        else {
                            debug!("Transcription operation cancelled during output handling");
                            attempt.finish(StatisticsRunStatus::Cancelled);
                            statistics.finish(StatisticsRunStatus::Cancelled);
                            utils::hide_recording_overlay(&ah);
                            set_tray_state(&ah, TrayIconState::Idle);
                            return;
                        };

                        let post_processing_latency_ms = if post_process {
                            Some(post_process_start.elapsed().as_secs_f64() * 1000.0)
                        } else {
                            None
                        };

                        if rm.was_cancelled_since(cancel_generation) {
                            debug!("Transcription operation cancelled before paste");
                            attempt.finish(StatisticsRunStatus::Cancelled);
                            statistics.finish(StatisticsRunStatus::Cancelled);
                            utils::hide_recording_overlay(&ah);
                            set_tray_state(&ah, TrayIconState::Idle);
                            return;
                        }

                        // Complete post processing for statistics attempt
                        attempt.complete_post_processing();
                        attempt.finish(StatisticsRunStatus::Success);
                        statistics.finish(StatisticsRunStatus::Success);

                        // Recall insertion mode: the Recall editor is armed,
                        // so this transcription belongs to the note at the
                        // caret — the text goes to the webview, nothing is
                        // pasted, no history row, no recording file. The
                        // vault, not History, is the destination.
                        if crate::recall::insertion::is_armed() {
                            let recording_path = hm.recordings_dir().join(&file_name);
                            let absorb_ah = ah.clone();
                            tauri::async_runtime::spawn_blocking(move || {
                                crate::recall::insertion::absorb_recording(
                                    &absorb_ah,
                                    &recording_path,
                                    wav_saved,
                                );
                            });
                            if let Err(e) = crate::recall::insertion::emit_insert(
                                &ah,
                                processed.final_text.clone(),
                            ) {
                                error!("Failed to emit recall insert event: {e}");
                            }
                            utils::hide_recording_overlay(&ah);
                            set_tray_state(&ah, TrayIconState::Idle);
                            return;
                        }

                        // Save to history if WAV was saved
                        if wav_saved {
                            let audio_duration_ms =
                                (sample_count as f64 * 1000.0) / (raw_rate as f64);
                            let speech_duration_ms = speech_ms as f64;
                            let word_count = (!transcription.trim().is_empty())
                                .then(|| transcription.split_whitespace().count() as i32);
                            let mode = if use_streaming_overlay {
                                "streaming"
                            } else {
                                "single"
                            }
                            .to_string();

                            if let Err(err) =
                                hm.save_entry_full(crate::managers::history::NewHistoryEntry {
                                    file_name,
                                    transcription_text: transcription,
                                    post_process_requested: post_process,
                                    post_processed_text: processed.post_processed_text.clone(),
                                    post_process_prompt: processed.post_process_prompt.clone(),
                                    model_id: Some(settings.selected_model.clone()),
                                    engine: None,
                                    audio_duration_ms: Some(audio_duration_ms),
                                    speech_duration_ms: Some(speech_duration_ms),
                                    sample_rate_hz: Some(raw_rate as i32),
                                    word_count,
                                    transcription_latency_ms: Some(stt_latency_ms),
                                    post_processing_latency_ms,
                                    language: Some(settings.selected_language.clone()),
                                    mode: Some(mode),
                                    extra_models: None,
                                })
                            {
                                error!("Failed to save history entry: {}", err);
                            }
                        }

                        if processed.final_text.is_empty() || was_direct_stream_written {
                            debug!(
                                "Direct streaming or empty output - skipping final paste action (direct_streamed: {})",
                                was_direct_stream_written
                            );
                            utils::hide_recording_overlay(&ah);
                            set_tray_state(&ah, TrayIconState::Idle);
                        } else {
                            let ah_clone = ah.clone();
                            let paste_time = Instant::now();
                            let final_text = processed.final_text;
                            let rm_for_paste = Arc::clone(&rm);
                            // Preview-only live stream: the polished text goes
                            // in through the default clipboard paste.
                            let paste_override = final_paste_method(preview_only);
                            ah.run_on_main_thread(move || {
                                if rm_for_paste.was_cancelled_since(cancel_generation) {
                                    debug!("Transcription operation cancelled before paste");
                                    utils::hide_recording_overlay(&ah_clone);
                                    set_tray_state(&ah_clone, TrayIconState::Idle);
                                    return;
                                }

                                match utils::paste_with_method(
                                    final_text,
                                    ah_clone.clone(),
                                    paste_override,
                                ) {
                                    Ok(()) => debug!(
                                        "Text pasted successfully in {:?}",
                                        paste_time.elapsed()
                                    ),
                                    Err(e) => {
                                        error!("Failed to paste transcription: {}", e);
                                        let _ = ah_clone.emit("paste-error", ());
                                    }
                                }
                                utils::hide_recording_overlay(&ah_clone);
                                set_tray_state(&ah_clone, TrayIconState::Idle);
                            })
                            .unwrap_or_else(|e| {
                                error!("Failed to run paste on main thread: {:?}", e);
                                utils::hide_recording_overlay(&ah);
                                set_tray_state(&ah, TrayIconState::Idle);
                            });
                        }
                    }
                    Err(err) => {
                        if rm.was_cancelled_since(cancel_generation) {
                            debug!("Transcription operation cancelled after transcription error");
                            statistics.finish(StatisticsRunStatus::Cancelled);
                            utils::hide_recording_overlay(&ah);
                            set_tray_state(&ah, TrayIconState::Idle);
                            return;
                        }

                        statistics.finish(StatisticsRunStatus::Failed);
                        error!("Transcription failed: {}", err);
                        // Surface the failure to the UI (toast). The full
                        // message is also in the app's log file via the line above.
                        let _ = ah.emit("transcription-error", err.to_string());
                        // Save entry with empty text so user can retry
                        if wav_saved
                            && let Err(save_err) =
                                hm.save_entry(file_name, String::new(), post_process, None, None)
                        {
                            error!("Failed to save failed history entry: {}", save_err);
                        }
                        utils::hide_recording_overlay(&ah);
                        set_tray_state(&ah, TrayIconState::Idle);
                    }
                }
            }
        });

        debug!(
            "TranscribeAction::stop completed in {:?}",
            stop_time.elapsed()
        );
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
            "Shortcut ID '{}': Started - {} (App: {})",
            binding_id,
            shortcut_str,
            app.package_info().name
        );
    }

    fn stop(&self, app: &AppHandle, binding_id: &str, shortcut_str: &str) {
        log::info!(
            "Shortcut ID '{}': Stopped - {} (App: {})",
            binding_id,
            shortcut_str,
            app.package_info().name
        );
    }
}

// ============================================================================
// Multi STT Action
// ============================================================================

struct MultiSttAction;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MultiSttMergeOutcome {
    pub cleaned_text: String,
    pub raw_text: String,
    pub provider_id: String,
    pub provider_label: String,
    pub model_name: String,
    pub prompt_name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MultiSttHistoryModel {
    pub slot: usize,
    pub model_id: String,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MultiSttHistoryBrain {
    pub provider_id: String,
    pub provider_label: String,
    pub model_name: String,
    pub prompt_name: Option<String>,
    pub latency_ms: Option<f64>,
    pub raw_output: String,
    pub cleaned_output: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MultiSttHistoryMetadata {
    pub version: u32,
    pub models: Vec<MultiSttHistoryModel>,
    pub brain: Option<MultiSttHistoryBrain>,
    pub final_merged_text: String,
}

/// The history row's per-model dump plus its machine-readable metadata.
/// `models` is `(model id, output)` per Multi-STT slot, Model 1 (the primary)
/// first, one entry per configured slot.
pub(crate) fn format_multi_stt_history_transcript(
    models: &[(&str, &str)],
    brain: Option<MultiSttHistoryBrain>,
    final_merged: &str,
) -> String {
    let mut text = String::new();
    text.push_str("=== Multi-STT Results ===\n");
    for (index, (model, output)) in models.iter().enumerate() {
        text.push_str(&format!("Model {}: {}\n{}\n", index + 1, model, output));
    }

    if let Some(ref b) = brain {
        text.push_str(&format!(
            "\n=== Brain Model ===\nProvider: {} ({})\nModel: {}\n",
            b.provider_label, b.provider_id, b.model_name
        ));
        if let Some(ref p) = b.prompt_name {
            text.push_str(&format!("Prompt: {}\n", p));
        }
        if let Some(lat) = b.latency_ms {
            text.push_str(&format!("Latency: {:.0} ms\n", lat));
        }
        text.push_str(&format!(
            "\n--- Brain Cleaned Output ---\n{}\n",
            b.cleaned_output
        ));
        text.push_str(&format!("\n--- Brain Raw Output ---\n{}\n", b.raw_output));
    }

    text.push_str(&format!("\n=== Merged ===\n{}\n", final_merged));

    let metadata = MultiSttHistoryMetadata {
        version: 1,
        models: models
            .iter()
            .enumerate()
            .map(|(index, (model, output))| MultiSttHistoryModel {
                slot: index + 1,
                model_id: model.to_string(),
                text: output.to_string(),
            })
            .collect(),
        brain,
        final_merged_text: final_merged.to_string(),
    };

    if let Ok(json_str) = serde_json::to_string(&metadata) {
        text.push_str(&format!("\n<!--MULTI_STT_METADATA:{}-->", json_str));
    }

    text
}

/// Whether any of the merge's slots carries a word, as opposed to whitespace,
/// punctuation, or nothing at all.
///
/// The merge prompt hands a model several transcripts of the same audio and asks
/// for one reconciled transcript. A slot set with no words in it gives it
/// nothing to reconcile, and a small model answers *the request* instead: the
/// reply is a sentence asking for the transcripts ("Please provide the four raw
/// audio transcripts so I can perform the multi-source merge and consensus
/// according to your strict rules."), which every caller would otherwise accept
/// as merged text and type into the user's document. `.`, `…`, `   ` are not
/// text to merge — they are a decoder emitting a fragment of punctuation.
///
/// Shared with the streaming coordinator, which asks this before dispatching a
/// merge at all: there, "no merge" has to be told apart from "the merge failed",
/// because a failure is retried after the next close.
pub(crate) fn slots_carry_words(slots: &[&str]) -> bool {
    slots
        .iter()
        .any(|slot| slot.chars().any(char::is_alphanumeric))
}

/// Phrases that ask the reader for the input rather than delivering a
/// transcript. Matched against the reply lowercased with whitespace collapsed,
/// and only counted when the reply also names the thing it is missing, so an
/// ordinary sentence containing "provide" is not caught by itself.
const MERGE_INPUT_REQUESTS: &[&str] = &[
    "please provide",
    "please share",
    "please send",
    "please paste",
    "provide the four",
    "provide the raw",
    "provide the audio",
    "provide the transcript",
    "provide the transcripts",
    "provide me with",
    "i need the four",
    "i need the transcript",
    "i need the audio",
    "i will need the",
    "i'll need the",
    "you have not provided",
    "you did not provide",
    "you haven't provided",
    "no transcripts",
    "no audio transcripts",
    "transcripts were provided",
    "transcripts are empty",
    "transcripts appear to be",
    "transcripts are missing",
    "transcript is empty",
    "without the transcripts",
    "waiting for the transcripts",
    "once you provide",
    "if you provide",
];

const MERGE_INPUT_NOUNS: &[&str] = &["transcript", "audio", "input", "dictation"];

/// Why a merge reply must not be pasted, if it must not be.
///
/// The failure this guards is not a wrong transcript, it is a model answering
/// the *prompt* instead of the audio — text the user never said, typed into
/// their document. Two independent tests, because either alone can be fooled:
///
/// * it asks for its own input. A merge never asks for the transcripts it was
///   given.
/// * it is implausibly long. A merge reconciles transcripts of the same audio,
///   so it cannot be several times the longest one it was handed; a few
///   disjoint partials are the most it can legitimately amount to.
///
/// `None` means "looks like a transcript". A rejection is not a lost session:
/// every caller already has a path for "no merge" that keeps the raw outputs,
/// so what the user said still reaches them — uncleaned, rather than replaced
/// by a sentence the model addressed to us.
fn merge_response_rejection(response: &str, slots: &[&str]) -> Option<String> {
    let trimmed = response.trim();
    if trimmed.is_empty() {
        // The callers' own case: an empty merge is a failed merge, never text.
        return None;
    }

    let flat = trimmed
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let dictated = |phrase: &str| {
        // A phrase the inputs themselves contain is not the model asking for
        // them: a speaker who dictated "please provide the transcripts to the
        // court" gets that sentence back from a faithful merge. Only a phrase
        // the model brought on its own is evidence against the reply. Matched
        // within one slot, since that is how the prompt delimits them.
        slots.iter().any(|slot| {
            let flat_slot = slot
                .to_lowercase()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            flat_slot.contains(phrase)
        })
    };
    // Both halves are needed: the phrase says the model is asking, the noun
    // says what it is asking for.
    let asks_for_input = MERGE_INPUT_REQUESTS
        .iter()
        .find(|p| flat.contains(**p) && !dictated(p));
    if let (Some(phrase), true) = (
        asks_for_input,
        MERGE_INPUT_NOUNS.iter().any(|noun| flat.contains(noun)),
    ) {
        return Some(format!("the reply asks for its own input ({phrase:?})"));
    }

    let longest = slots
        .iter()
        .map(|slot| slot.trim().chars().count())
        .max()
        .unwrap_or(0);
    let response_chars = trimmed.chars().count();
    if response_chars > longest * 4 + 64 {
        return Some(format!(
            "the reply is {response_chars} chars against a longest input of {longest}, which is \
             not a merge of what it was given"
        ));
    }

    None
}

/// Merge prompt for multi-STT: fills `${output}` / `${output1}` with the
/// primary's text and `${outputN}` with slot N's, then sends it to the LLM API
/// (same provider as post-processing). `outputs` is one text per Multi-STT
/// slot, Model 1 first; a placeholder for a slot past the configured ones is
/// filled with an empty string, as an empty slot's is.
pub(crate) async fn multi_stt_merge_transcriptions(
    settings: &AppSettings,
    outputs: &[&str],
) -> Option<MultiSttMergeOutcome> {
    let merge_prompt = match &settings.multi_stt_merge_prompt {
        Some(p) => p.clone(),
        None => {
            debug!("Multi-STT merge skipped: no merge prompt configured");
            return None;
        }
    };

    if merge_prompt.prompt.trim().is_empty() {
        debug!("Multi-STT merge skipped: merge prompt is empty");
        return None;
    }

    // Replace placeholders. `${output1}` and `${output10}` differ by their
    // closing brace, so the ascending order cannot clip one into the other.
    let slot_text = |index: usize| outputs.get(index).copied().unwrap_or("");
    let mut prompt = merge_prompt.prompt.replace("${output}", slot_text(0));
    for index in 0..=crate::settings::MULTI_STT_MAX_EXTRA_MODELS {
        prompt = prompt.replace(&format!("${{output{}}}", index + 1), slot_text(index));
    }

    debug!(
        "Multi-STT merge prompt prepared, length: {} chars",
        prompt.chars().count()
    );
    // One line per model's answer for the same audio. These are what the merge
    // reconciles, so they are the first thing to read when the merged text comes
    // back wrong, short, or repeating — and an empty slot is invisible in a
    // prompt length, which is exactly how a chunk merged against nothing.
    for (index, text) in outputs.iter().enumerate() {
        crate::utils::log_multiline(
            &format!("Multi-STT slot {} (model {})", index + 1, index + 1),
            text,
        );
    }

    // Nothing to reconcile: the slots hold no word between them, so the
    // prompt would reach the model as empty quoted blocks and the model
    // would answer the request instead of the audio — see
    // [`slots_carry_words`]. The call is skipped rather than made and then
    // rejected, which also removes a round-trip that can only return what is
    // already here.
    if !slots_carry_words(outputs) {
        debug!(
            "Multi-STT merge skipped: no slot carries a word ({} chars across the {} slots)",
            outputs.iter().map(|s| s.chars().count()).sum::<usize>(),
            outputs.len()
        );
        return None;
    }

    let provider = match settings.active_post_process_provider().cloned() {
        Some(provider) => provider,
        None => {
            debug!("Multi-STT merge skipped: no post-process provider configured");
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
            "Multi-STT merge skipped: no model configured for provider '{}'",
            provider.id
        );
        return None;
    }

    let api_key = settings
        .post_process_api_keys
        .get(&provider.id)
        .cloned()
        .unwrap_or_default();

    debug!(
        "Multi-STT merging with provider '{}' (model: {})",
        provider.id, model
    );

    // Use legacy chat completion for merging
    let merge_result = match crate::llm_client::send_chat_completion(
        &provider, api_key, &model, prompt, false,
    )
    .await
    {
        Ok(Some(raw_content)) => {
            // Same sanitising as post-processing: a reasoning model on the
            // same provider must not paste its <think> block, and invisible
            // characters must not leak into the pasted text.
            let cleaned_text = strip_invisible_chars(strip_think_block(&raw_content))
                .trim()
                .to_string();
            match merge_response_rejection(&cleaned_text, outputs) {
                // The reply is not a transcript — the model answered the
                // prompt. Returning "no merge" hands the caller its
                // concatenation fallback, so the user's own words are kept
                // and the model's sentence never reaches their document.
                Some(reason) => {
                    warn!(
                        "Multi-STT merge rejected: {}. Keeping the raw outputs instead; the \
                         model's reply was not a transcript:",
                        reason
                    );
                    crate::utils::log_multiline("Multi-STT merge reply (rejected)", &cleaned_text);
                    None
                }
                None => {
                    debug!(
                        "Multi-STT merge succeeded. Output length: {} chars, raw length: {} chars",
                        cleaned_text.chars().count(),
                        raw_content.chars().count()
                    );
                    // The text the app will actually paste, after the <think>
                    // strip and the trim. It is logged separately from the raw
                    // response because the two differ exactly when sanitising
                    // changed something, which is otherwise invisible.
                    crate::utils::log_multiline("Multi-STT merged text (as pasted)", &cleaned_text);
                    Some(MultiSttMergeOutcome {
                        cleaned_text,
                        raw_text: raw_content,
                        provider_id: provider.id.clone(),
                        provider_label: provider.label.clone(),
                        model_name: model.clone(),
                        prompt_name: Some(merge_prompt.name.clone()),
                    })
                }
            }
        }
        Ok(None) => {
            error!("Multi-STT merge: LLM API response has no content");
            None
        }
        Err(e) => {
            error!(
                "Multi-STT merge failed for provider '{}': {}",
                provider.id, e
            );
            None
        }
    };

    // Clean up server-side conversation state on llama.cpp servers to prevent
    // memory accumulation across many merge round-trips. Only the "custom"
    // provider can point at a llama.cpp server; hosted providers (OpenAI,
    // Anthropic, OpenRouter, ...) have no `/chat/erase_all` and would just
    // receive a pointless unauthenticated POST after every merge.
    if provider.id == "custom" {
        crate::llm_client::erase_llama_server_conversations(&provider.base_url).await;
    }

    merge_result
}

/// Pre-load extra STT models in parallel on the blocking thread pool.
/// Called from `start()` so models are ready before `stop()` runs,
/// eliminating loading latency from the transcription path.
async fn preload_extra_models_parallel(tm: &Arc<TranscriptionManager>, extra_models: Vec<String>) {
    if extra_models.is_empty() {
        return;
    }

    let load_start = Instant::now();
    info!(
        "Multi-STT: pre-loading {} extra model(s) in parallel",
        extra_models.len()
    );

    let mut handles = Vec::new();
    for model_id in &extra_models {
        if tm.is_extra_model_loaded(model_id) {
            continue;
        }
        let tm_clone = Arc::clone(tm);
        let model_id = model_id.clone();
        handles.push(tauri::async_runtime::spawn_blocking(move || {
            if let Err(e) = tm_clone.load_extra_model(&model_id) {
                warn!("Multi-STT: pre-load failed for '{}': {}", model_id, e);
            }
        }));
    }

    for h in handles {
        let _ = h.await;
    }

    info!(
        "Multi-STT: pre-loading completed in {:?}",
        load_start.elapsed()
    );
}

impl ShortcutAction for MultiSttAction {
    fn start(&self, app: &AppHandle, binding_id: &str, _shortcut_str: &str) {
        let start_time = Instant::now();
        debug!("MultiSttAction::start called for binding: {}", binding_id);

        // Load primary model
        let tm = app.state::<Arc<TranscriptionManager>>();
        let rm = app.state::<Arc<AudioRecordingManager>>();

        tm.initiate_model_load();
        let rm_clone = Arc::clone(&rm);
        std::thread::spawn(move || {
            if let Err(e) = rm_clone.preload_vad() {
                warn!("VAD pre-load failed: {}", e);
            }
        });

        let binding_id = binding_id.to_string();
        set_tray_state(app, TrayIconState::Recording);
        let settings = get_settings(app);
        let is_always_on = settings.always_on_microphone;

        // Pre-load extra STT models in parallel while the primary model loads
        // and the user records, so they're ready before transcription starts.
        // Extra models follow the same idle-timeout lifecycle as the primary
        // model — they stay loaded until the primary unloads (which may be
        // infinite depending on the user's ModelUnloadTimeout setting).
        // The nested Multi Streaming STT mode streams from every Multi-STT slot
        // that can stream, and that set is also what is preloaded: a slot the
        // mode cannot stream from is a model that must not be loaded at all
        // ("not even loaded", as the design note puts it). Picked here, once, so
        // the preload and the session below cannot disagree about which models
        // run — the session is handed this very list.
        let multi_streaming_slots = if settings.multi_stt_streaming_first_enabled
            && settings.multi_stt_streaming_multi_enabled
        {
            crate::multi_streaming::streaming_slots(app, &settings)
        } else {
            Vec::new()
        };
        if settings.multi_stt_enabled {
            let tm_pre = Arc::clone(&tm);
            // With the nested mode on, only the slots it streams from are
            // preloaded (the choice above), so the others stay unloaded.
            let to_preload: Vec<String> = if multi_streaming_slots.is_empty() {
                settings
                    .multi_stt_extra_model_ids()
                    .into_iter()
                    .flatten()
                    .collect()
            } else {
                multi_streaming_slots
                    .iter()
                    .map(|(_, model_id)| model_id.clone())
                    .collect()
            };
            tauri::async_runtime::spawn(async move {
                preload_extra_models_parallel(&tm_pre, to_preload).await;
            });
        }

        // === PERFORMANCE MODE: FULL POWER (Trigger at 1st keybind / recording start) ===
        if settings.multi_stt_performance_mode_enabled
            && settings.multi_stt_performance_mode_trigger_on_start
        {
            let full_power_shortcut = settings
                .multi_stt_performance_mode_full_power_shortcut
                .clone();
            let app_clone = app.clone();
            tauri::async_runtime::spawn_blocking(move || {
                crate::clipboard::simulate_key_combination(&app_clone, &full_power_shortcut);
            });
        }

        // Start recording
        let selected_model_info = app
            .state::<Arc<ModelManager>>()
            .get_model_info(&settings.selected_model);

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
        // Multi-STT always merges/concatenates, so the primary model's live
        // stream is a preview only: never typed into the app, Live overlay
        // forced when the model can stream.
        let preview_only = live_stream_is_preview_only(&settings, true);
        let statistics_manager = app.state::<Arc<StatisticsManager>>();
        let statistics = statistics_manager.begin_normal_run(&binding_id);
        // Experimental streaming mode: the primary model's live text becomes the
        // overlay's text and is replaced in place, chunk by chunk, as the extras
        // and the merge land. Arming happens before the recorder starts, so the
        // coordinator sees the recording's very first frame.
        // The overlay is handed to the coordinator for the whole session, so it
        // must own the text before the stream worker can emit its first raw
        // update and win the race.
        //
        // The nested Multi Streaming STT mode takes the session over when it can
        // arm: the two modes never run together, and the earlier return means the
        // parent's coordinator is not armed at all. When the nested mode refuses
        // — no streaming-capable slot, or a primary that cannot stream — the
        // parent's own toggle still stands, so its coordinator arms as usual.
        let multi_streaming = crate::multi_streaming::start(
            app,
            &tm,
            &rm,
            &multi_streaming_slots,
            model_supports_streaming,
            statistics.clone(),
        );
        let streaming_mode = multi_streaming
            || crate::multi_stt_stream::start(
                app,
                &tm,
                &rm,
                model_supports_streaming,
                // The parent mode's own session: its chunk texts are re-decodes of
                // the recorded audio, and it streams from no extra slot — the
                // nested mode above is the one that brings live extras.
                crate::multi_stt_stream::TextSource::ReDecode,
                &[],
            );
        if model_supports_streaming {
            // `false`: with the experimental mode on, the coordinator does the
            // live typing from its own thread (it owns the only
            // `DirectStreamWriter`); the worker must never create a second one.
            // Without the mode, Multi-STT's stream is preview-only anyway, so
            // this stays false on both paths.
            tm.start_stream(false, statistics.clone());
        }
        let overlay_style = if streaming_mode {
            // The mode is the session's text on screen, and the Minimal overlay
            // never renders the transcript — so it forces Live whatever the
            // user's choice is. (A preview-only stream forces it too, but only
            // when DirectStreaming is the paste method; this mode needs it on
            // every path.)
            debug!("Multi-STT streaming-first mode: the session coordinator owns the overlay text");
            OverlayStyle::Live
        } else {
            effective_overlay_style(&settings, preview_only, model_supports_streaming)
        };

        match overlay_style {
            OverlayStyle::Live if model_supports_streaming => utils::show_streaming_overlay(app),
            OverlayStyle::Live | OverlayStyle::Minimal => show_recording_overlay(app),
            OverlayStyle::None => {}
        }

        debug!("Multi-STT microphone mode - always_on: {}", is_always_on);

        // Mirrors TranscribeAction::start: the start chime, mute and the
        // overlay's `recording-ready` cue all follow the first real microphone
        // callback rather than a fixed delay, so slow Bluetooth/USB devices
        // don't chime before they are actually capturing.
        let mut recording_error: Option<String> = None;
        match rm.try_start_recording(&binding_id, vad_policy) {
            Ok(readiness) => {
                spawn_recording_ready_cue(app, &rm, readiness);
            }
            Err(e) => {
                debug!("Multi-STT: failed to start recording: {}", e);
                recording_error = Some(e);
            }
        }

        if recording_error.is_none() {
            shortcut::register_cancel_shortcut(app);
        } else {
            statistics.finish(StatisticsRunStatus::Failed);
            // The mode is armed but no recording will ever feed it: drop the
            // coordinator (and its tap claim) — or the multi-streaming session,
            // whose waiter would otherwise open a second stream on a recording
            // that never began — rather than leaving either to drain the next
            // recording's audio.
            crate::multi_stt_stream::cancel();
            crate::multi_streaming::cancel(&tm);
            tm.cancel_stream();
            utils::hide_recording_overlay(app);
            set_tray_state(app, TrayIconState::Idle);
            if settings.multi_stt_performance_mode_enabled
                && settings.multi_stt_performance_mode_trigger_on_start
            {
                restore_normal_power(
                    app,
                    settings.multi_stt_performance_mode_normal_shortcut.clone(),
                );
            }
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
            "MultiSttAction::start completed in {:?}",
            start_time.elapsed()
        );
    }

    fn stop(&self, app: &AppHandle, binding_id: &str, _shortcut_str: &str) {
        // Prevent a slow microphone from emitting a ready event or start chime
        // after the user has already requested stop (as `TranscribeAction`).
        app.state::<Arc<AudioRecordingManager>>()
            .invalidate_recording_readiness();

        // Unregister cancel shortcut
        shortcut::unregister_cancel_shortcut(app);

        let stop_time = Instant::now();
        debug!("MultiSttAction::stop called for binding: {}", binding_id);

        let ah = app.clone();
        let rm = Arc::clone(&app.state::<Arc<AudioRecordingManager>>());
        let tm = Arc::clone(&app.state::<Arc<TranscriptionManager>>());
        let hm = Arc::clone(&app.state::<Arc<HistoryManager>>());
        let sm = Arc::clone(&app.state::<Arc<StatisticsManager>>());

        set_tray_state(app, TrayIconState::Transcribing);
        let stop_settings = get_settings(app);
        let merge_requested = has_merge_prompt(&stop_settings);
        let statistics = sm
            .get_normal_run(binding_id)
            .unwrap_or_else(|| sm.begin_normal_run(binding_id));
        statistics.mark_input_stopped(stop_time, merge_requested);

        // In the experimental streaming mode the overlay belongs to the
        // coordinator: it is showing the session's text, and a working phase
        // event would replace it with a spinner while the last chunk merges.
        let coordinator_active = crate::multi_stt_stream::is_active();
        // The nested multi-streaming mode runs this same coordinator, so
        // `coordinator_active` covers it; `multi_streaming_active` is read below
        // to decide which decodes run.
        let multi_streaming_active = crate::multi_streaming::is_active();
        let preview_only = live_stream_is_preview_only(&stop_settings, true);
        let style = effective_overlay_style(&stop_settings, preview_only, tm.is_streaming());
        let use_streaming_overlay = should_use_streaming_overlay(style, tm.is_streaming());
        if coordinator_active {
            // nothing: the coordinator owns the overlay
        } else if use_streaming_overlay {
            tm.emit_stream_working(StreamWorkKind::Transcribing);
        } else {
            show_transcribing_overlay(app);
        }

        rm.remove_mute();
        play_feedback_sound(app, SoundType::Stop);

        let binding_id = binding_id.to_string();
        let cancel_generation = rm.cancel_generation();

        tauri::async_runtime::spawn(async move {
            let _guard = FinishGuard(ah.clone());
            debug!(
                "Multi-STT: Starting async transcription task for binding: {}",
                binding_id
            );

            let recorded_res = rm.stop_recording(&binding_id, cancel_generation);
            let recorded = match recorded_res {
                StopRecordingResult::Captured {
                    recorded,
                    captured_sample_count,
                    sample_rate,
                } => {
                    statistics.set_audio(captured_sample_count, sample_rate);
                    recorded
                }
                StopRecordingResult::Cancelled => {
                    statistics.finish(StatisticsRunStatus::Cancelled);
                    crate::multi_streaming::cancel(&tm);
                    tm.cancel_stream();
                    utils::hide_recording_overlay(&ah);
                    set_tray_state(&ah, TrayIconState::Idle);
                    restore_power_if_triggered_on_start(&ah);
                    return;
                }
                StopRecordingResult::NotActive => {
                    statistics.finish(StatisticsRunStatus::Failed);
                    // As in TranscribeAction: a cancel got there first. Release
                    // the session state and undo the UI `stop()` has shown,
                    // unless another recording owns it by now.
                    if !rm.is_recording() {
                        crate::multi_stt_stream::cancel();
                        crate::multi_streaming::cancel(&tm);
                        tm.cancel_stream();
                        utils::hide_recording_overlay(&ah);
                        set_tray_state(&ah, TrayIconState::Idle);
                        restore_power_if_triggered_on_start(&ah);
                    }
                    return;
                }
                StopRecordingResult::Failed(err) => {
                    statistics.finish(StatisticsRunStatus::Failed);
                    // An armed coordinator would keep the tap and the exclusive
                    // text sink, and the next plain recording would draw no
                    // overlay text.
                    crate::multi_stt_stream::cancel();
                    crate::multi_streaming::cancel(&tm);
                    tm.cancel_stream();
                    utils::hide_recording_overlay(&ah);
                    set_tray_state(&ah, TrayIconState::Idle);
                    error!("Multi-STT: Recording failed: {err}");
                    return;
                }
            };

            let samples = recorded.stt_samples;
            debug!(
                "Multi-STT: Recording stopped, STT sample count: {}, raw sample count: {}",
                samples.len(),
                recorded.raw_samples.len()
            );

            // === EXPERIMENTAL STREAMING MODE ===
            // The coordinator has been doing the work while the user spoke: its
            // chunks hold the merged text, and the primary stream's final words
            // reach it through the sink inside `finalize_stream`. So the stream
            // is finalized here and the session is asked to close its last chunk
            // — one blocking handshake, on the blocking pool because `finish`
            // waits on a merge. Any path that cannot produce a session outcome
            // clears the coordinator and falls through to the batch pipeline:
            // extra coordinator state left behind would claim the next
            // recording's audio tap.
            // `coordinator_active` implies the primary model streams natively:
            // the coordinator refuses to arm otherwise, and this is the same
            // recording it armed for. The nested Multi Streaming STT mode runs
            // *this* coordinator — it is the parent's own session, told where its
            // chunk texts come from — so a nested session is simply a coordinator
            // session, and its primary stream is finalized on these same terms.
            let stream_tracked = if coordinator_active || multi_streaming_active {
                match tm.finalize_stream() {
                    StreamFinalization::Completed(tracked) => Some(tracked),
                    // Nothing was decoded live: the batch path is a better answer
                    // than a session built on no text. The reason is not logged —
                    // `Failed`/`Timeout` carry model text.
                    _ => {
                        debug!(
                            "Multi-STT streaming: the primary stream did not complete; falling \
                             back to the batch path"
                        );
                        None
                    }
                }
            } else {
                None
            };

            // === THE NESTED MODE'S EXTRA STREAMS ===
            // Finalized here, between the primary's finalize above and the
            // session's finish below, because that order is the whole of this
            // mode's ending: the session's last chunk is built from the extras'
            // live text, and a finalize is what delivers a model's last words to
            // it through the sink. Held back until after the session had closed,
            // those words would arrive for a coordinator with no chunk left to put
            // them in, and be dropped without a trace.
            //
            // Each result is that model's *own* session text — the second column,
            // in the history row so the two live texts stay comparable beside the
            // merged one. It is kept on the same terms `task1` keeps the primary's,
            // so every stream this session opened finishes its statistics attempt.
            let mut multi_extras: [Option<TrackedTranscription>;
                crate::multi_streaming::EXTRA_MODELS] =
                if multi_streaming_active && stream_tracked.is_some() {
                    tauri::async_runtime::spawn_blocking({
                        let tm = Arc::clone(&tm);
                        move || crate::multi_streaming::finish_extras(&tm)
                    })
                    .await
                    .unwrap_or_default()
                } else {
                    // No session outcome is coming — the primary's stream did not
                    // complete. The extras are released rather than finalized with
                    // it, below: their attempts belong to a session that is not
                    // what gets reported. In the nested mode nothing batch-decodes
                    // them either, so the result degrades to the primary model
                    // alone.
                    Default::default()
                };

            // The session result is the coordinator's, and in this mode that is
            // the point of it: it holds the merged-and-cleaned text of every chunk
            // that closed while the user spoke, merged from the *live* texts of
            // every streaming model — the primary's and each extra's — with no
            // re-decode anywhere in the recording. That text is what gets pasted.
            // The nested mode takes this path on exactly the parent's terms,
            // because it is the parent's session; the only difference is where the
            // merged-from texts came from.
            let stream_outcome = if stream_tracked.is_some() {
                // A session that retired itself mid-recording — the primary's
                // committed text was not tracking its audio, so its chunks could
                // not be merged against their own text — is gone by the time the
                // recording stops, and the warning it logged when it stopped is
                // the reason. Asking the empty slot for a result would come back
                // with the same `None` and a misleading timeout message.
                if !crate::multi_stt_stream::is_active() {
                    warn!(
                        "Multi-STT streaming: the coordinator stopped before the recording did; \
                         falling back to the batch path"
                    );
                    None
                } else {
                    let stream_finish_start = Instant::now();
                    let outcome = tauri::async_runtime::spawn_blocking(|| {
                        crate::multi_stt_stream::finish(crate::multi_stt_stream::FINISH_TIMEOUT)
                    })
                    .await
                    .ok()
                    .flatten();
                    if outcome.is_none() {
                        warn!(
                            "Multi-STT streaming: the session did not return a result after {:?}; \
                             falling back to the batch path",
                            stream_finish_start.elapsed()
                        );
                    }
                    outcome
                }
            } else {
                None
            };

            // A finished session with no text at all is treated like an empty
            // recording: the WAV and the history row belong to the batch path.
            let stream_outcome = stream_outcome.filter(|o| !o.final_text.trim().is_empty());
            if coordinator_active && stream_outcome.is_none() {
                // Either the stream never completed or the session came back
                // empty. Both leave a coordinator alive: release it, or it would
                // claim the next recording's audio tap.
                crate::multi_stt_stream::cancel();
            }
            if multi_streaming_active && stream_tracked.is_none() {
                // The nested mode's result *is* its session's merged text, which is
                // built on the primary's live text, so a primary stream that did
                // not complete leaves it with nothing to report. The extras are not
                // batch-decoded in this mode (see the load below), so the result
                // degrades to the primary model alone, decoded from the recording.
                // The session is released either way: an armed waiter would open
                // a second stream on the next recording, and the extra's engine
                // would stay leased.
                crate::multi_streaming::cancel(&tm);
            }

            if rm.was_cancelled_since(cancel_generation) {
                debug!("Multi-STT: Cancelled after recording stop");
                statistics.finish(StatisticsRunStatus::Cancelled);
                tm.cancel_stream();
                utils::hide_recording_overlay(&ah);
                set_tray_state(&ah, TrayIconState::Idle);
                restore_power_if_triggered_on_start(&ah);
                return;
            }

            let speech_ms = rm.last_speech_ms();
            statistics.set_speech_audio_duration_ms(speech_ms as i64, 16000);
            if samples.is_empty() || speech_ms < MIN_SPEECH_MS_TO_TRANSCRIBE {
                debug!(
                    "Multi-STT: Recording has no usable speech ({} samples, {}ms voiced)",
                    samples.len(),
                    speech_ms
                );
                statistics.finish(StatisticsRunStatus::Empty);
                tm.cancel_stream();
                // An armed coordinator whose recording had nothing to say is
                // released with everything else that this recording owned.
                crate::multi_stt_stream::cancel();
                // Likewise the nested mode's session, which would otherwise keep
                // its waiter polling and the extra's engine leased.
                crate::multi_streaming::cancel(&tm);
                utils::hide_recording_overlay(&ah);
                set_tray_state(&ah, TrayIconState::Idle);
                restore_power_if_triggered_on_start(&ah);
                return;
            }

            // === PERFORMANCE MODE: FULL POWER ===
            // Signal the user's performance-mode shortcut (e.g. Ctrl+Space)
            // before the heavy transcription workload starts, giving the OS
            // a chance to ramp up CPU clocks ahead of the 4-way inference.
            // If trigger_on_start is enabled, it was already triggered in start().
            let perf_settings = get_settings(&ah);
            if perf_settings.multi_stt_performance_mode_enabled
                && !perf_settings.multi_stt_performance_mode_trigger_on_start
            {
                let full_power_shortcut = perf_settings
                    .multi_stt_performance_mode_full_power_shortcut
                    .clone();
                let ah_clone = ah.clone();
                tauri::async_runtime::spawn_blocking(move || {
                    crate::clipboard::simulate_key_combination(&ah_clone, &full_power_shortcut);
                });
            }

            let settings = get_settings(&ah);
            let (samples_for_wav, save_is_raw, raw_rate, raw_format) =
                if settings.save_raw_audio && !recorded.raw_samples.is_empty() {
                    (
                        recorded.raw_samples,
                        true,
                        recorded.native_sample_rate,
                        recorded.native_sample_format,
                    )
                } else {
                    (samples.clone(), false, 16000, cpal::SampleFormat::I16)
                };

            // Save WAV concurrently. The timestamp is shared with the
            // history entry below so the recorded file name always matches.
            let recording_timestamp = chrono::Utc::now().timestamp();
            let wav_path = hm
                .recordings_dir()
                .join(app_identity::multi_recording_file_name(recording_timestamp));
            let wav_for_save = wav_path.clone();
            let wav_sample_count = samples_for_wav.len();
            let wav_handle = tauri::async_runtime::spawn_blocking(move || {
                if save_is_raw {
                    crate::audio_toolkit::save_raw_wav_file(
                        &wav_for_save,
                        &samples_for_wav,
                        raw_rate,
                        raw_format,
                    )
                } else {
                    crate::audio_toolkit::save_wav_file(&wav_for_save, &samples_for_wav)
                }
            });

            // === LOAD EXTRA MODELS IN PARALLEL (fastest first-run) ===
            // The extra models load concurrently on the blocking pool so the
            // async worker stays free for events/UI. Models already loaded (e.g.
            // by the pre-load in start()) are skipped immediately.
            //
            // The nested multi-streaming mode runs no batch decode at all: its
            // second column is a live stream, and the engine behind it is leased
            // by that stream's worker for the whole session. The extras are
            // therefore taken out of the list entirely — the slots the mode does
            // not stream from were never loaded (see the preload in `start`), and
            // loading them here would be exactly the work this mode exists to
            // skip.
            let settings = get_settings(&ah);
            // One entry per configured extra slot ("Model 2" first), `None` for
            // an empty one, so every output keeps its slot's placeholder.
            let configured_extras = settings.multi_stt_extra_model_ids();
            let extra_models: Vec<Option<String>> = if multi_streaming_active {
                debug!(
                    "Multi streaming STT: no batch decode — the extra columns are the extras' live \
                     streams, not re-decodes of the recording"
                );
                vec![None; configured_extras.len()]
            } else {
                configured_extras.clone()
            };

            // Every load is spawned before any is awaited — they run concurrently
            // in the blocking pool, so the total time is the slowest load's.
            // Model N is Multi-STT slot N, the numbering the settings page and
            // the history row use.
            let load_start = Instant::now();
            let mut load_handles = Vec::new();
            for (model_number, model_id) in (2_usize..).zip(extra_models.iter()) {
                let Some(model_id) = model_id else {
                    continue;
                };
                if tm.is_extra_model_loaded(model_id) {
                    info!(
                        "Multi-STT: extra model {} '{}' already loaded, skipping",
                        model_number, model_id
                    );
                    continue;
                }
                let tm = Arc::clone(&tm);
                let model_id = model_id.clone();
                load_handles.push(tauri::async_runtime::spawn_blocking(move || {
                    info!(
                        "Multi-STT: loading extra model {}: {}",
                        model_number, model_id
                    );
                    match tm.load_extra_model(&model_id) {
                        Ok(name) => info!(
                            "Multi-STT: extra model {} '{}' loaded successfully",
                            model_number, name
                        ),
                        Err(e) => error!(
                            "Multi-STT: failed to load extra model {} '{}': {}",
                            model_number, model_id, e
                        ),
                    }
                }));
            }
            for handle in load_handles {
                let _ = handle.await;
            }

            info!(
                "Multi-STT: extra model loading complete in {:?}",
                load_start.elapsed()
            );

            // === TRANSCRIBE WITH ALL MODELS IN PARALLEL ===
            // Inference is CPU/GPU-bound blocking work: run it on the
            // blocking pool so tokio workers stay free for events/UI.
            let transcribe_start = Instant::now();

            let tm1 = Arc::clone(&tm);
            let s1 = samples.clone();
            let stats1 = statistics.clone();

            // In the streaming mode the stream was already finalized (the
            // coordinator needed its last words), so the tracked result is used
            // as it stands instead of finalizing a second time — a stream can be
            // consumed exactly once, and the second call would come back
            // `NeverStarted` and quietly re-decode the whole recording.
            let task1 = tauri::async_runtime::spawn_blocking(move || match stream_tracked {
                Some(tracked) if !tracked.text.trim().is_empty() => {
                    info!(
                        "Multi-STT: Model 1 (primary) transcription: '{}'",
                        utils::redact_text(&tracked.text)
                    );
                    Some(tracked)
                }
                Some(_) => tm1.transcribe_tracked(s1, stats1).ok(),
                None => match tm1.finalize_stream() {
                    StreamFinalization::Completed(tracked) if !tracked.text.trim().is_empty() => {
                        info!(
                            "Multi-STT: Model 1 (primary) transcription: '{}'",
                            utils::redact_text(&tracked.text)
                        );
                        Some(tracked)
                    }
                    StreamFinalization::Completed(_) | StreamFinalization::NeverStarted => {
                        tm1.transcribe_tracked(s1, stats1).ok()
                    }
                    StreamFinalization::Failed(err) => {
                        error!("Multi-STT: Model 1 finalize failed: {}", err);
                        tm1.transcribe_tracked(s1, stats1).ok()
                    }
                    StreamFinalization::Timeout(err) => {
                        error!("Multi-STT: Model 1 finalize timeout: {}", err);
                        None
                    }
                },
            });

            // One batch decode per configured extra slot. The nested mode has
            // none: `extra_models` is empty there (see above), because the
            // extras' engines were leased by their own live stream workers and
            // went home with them in `finish_extras`, and each model's text is
            // already in hand (`multi_extras`).
            let extra_tasks: Vec<_> = extra_models
                .iter()
                .enumerate()
                .map(|(index, model_id)| {
                    let model_id = model_id.clone()?;
                    let model_number = index + 2;
                    let tm = Arc::clone(&tm);
                    let samples = samples.clone();
                    let stats = statistics.clone();
                    Some(tauri::async_runtime::spawn_blocking(move || {
                        if tm.is_extra_model_loaded(&model_id) {
                            tm.transcribe_with_extra_tracked(&model_id, samples, stats)
                                .ok()
                        } else {
                            warn!(
                                "Multi-STT: Model {} '{}' not loaded, skipping",
                                model_number, model_id
                            );
                            None
                        }
                    }))
                })
                .collect();

            let mut tracked1 = task1.await.unwrap_or(None);
            let mut tracked_extras: Vec<Option<TrackedTranscription>> =
                Vec::with_capacity(extra_tasks.len());
            for (index, task) in extra_tasks.into_iter().enumerate() {
                tracked_extras.push(match task {
                    Some(t) => t.await.unwrap_or(None),
                    // The nested mode: the extra's own live text, taken beside the
                    // primary's finalize so the history row holds every column.
                    None => multi_extras.get_mut(index).and_then(Option::take),
                });
            }
            let source = if multi_streaming_active {
                "live"
            } else {
                "batch"
            };
            for (model_number, tracked) in (2_usize..).zip(tracked_extras.iter()) {
                if let Some(t) = tracked {
                    info!(
                        "Multi-STT: Model {} ({}) transcription: '{}'",
                        model_number,
                        source,
                        utils::redact_text(&t.text)
                    );
                }
            }

            // === EXPERIMENTAL STREAMING MODE: THE SESSION'S OWN RESULT ===
            // The chunks were decoded and merged while the user spoke, so this
            // replaces the batch decode → merge pipeline wholesale. Slot 1 is the
            // primary model's own session text and the slots after it are the extras'
            // outputs as recorded for each chunk that closed; both are already
            // assembled by the coordinator, which is why the batch merge below is
            // skipped. A successful merge this session made is `brain`; a chunk
            // that fell back to the concatenation has `failed_chunks` > 0.
            let streaming_final = stream_outcome.map(|outcome| {
                info!(
                    "Multi-STT streaming: using the session result — {} chunks, {} failed, \
                     decode {:.0} ms, merge {:.0} ms",
                    outcome.chunk_count,
                    outcome.failed_chunks,
                    outcome.decode_latency_ms,
                    outcome.merge_latency_ms
                );
                (
                    outcome.final_text,
                    outcome.model_outputs,
                    outcome.brain,
                    outcome.failed_chunks,
                    outcome.owns_typing,
                )
            });

            // One text per Multi-STT slot, Model 1 first: the session's own
            // outputs in the streaming mode, the decodes otherwise.
            let tracked_text = |t: &Option<TrackedTranscription>| {
                t.as_ref().map(|t| t.text.clone()).unwrap_or_default()
            };
            let outputs: Vec<String> = match &streaming_final {
                Some((_, session_outputs, ..)) => session_outputs
                    .iter()
                    .take(1 + configured_extras.len())
                    .cloned()
                    .collect(),
                None => std::iter::once(tracked_text(&tracked1))
                    .chain(tracked_extras.iter().map(tracked_text))
                    .collect(),
            };
            let output_refs: Vec<&str> = outputs.iter().map(String::as_str).collect();
            // Whether the session typed the text into the app itself, in which
            // case it has already delivered the result and there is nothing left
            // to paste.
            let typed_by_session = streaming_final
                .as_ref()
                .is_some_and(|(_, _, _, _, owns_typing)| *owns_typing);

            info!(
                "Multi-STT: All transcriptions complete. Chars per model: [{}]",
                outputs
                    .iter()
                    .enumerate()
                    .map(|(index, text)| format!("{}: {}", index + 1, text.chars().count()))
                    .collect::<Vec<_>>()
                    .join(", ")
            );

            let multi_transcription_latency_ms = transcribe_start.elapsed().as_secs_f64() * 1000.0;

            // A cancel that landed during the (multi-second) parallel
            // decode must not re-show the overlay, call the LLM, save a
            // history row or paste. Same gate TranscribeAction applies
            // before its output handling.
            if rm.was_cancelled_since(cancel_generation) {
                debug!("Multi-STT: Cancelled during transcription");
                finish_tracked_attempts(
                    &mut tracked1,
                    &mut tracked_extras,
                    StatisticsRunStatus::Cancelled,
                );
                statistics.finish(StatisticsRunStatus::Cancelled);
                utils::hide_recording_overlay(&ah);
                set_tray_state(&ah, TrayIconState::Idle);
                return;
            }

            // === MERGE TRANSCRIPTIONS ===
            // In the streaming mode there is nothing left to merge here: every
            // chunk already went through the extras and the LLM, and the session's
            // assembled text is the final one. `llm_merge_succeeded` means the
            // whole session merged. A chunk whose merge failed shows the
            // concatenation fallback and is retried after the next break; one that
            // still shows it at stop (the retry failed too, or no break came after
            // the failure) counts the session as not merged, which moves the power
            // restore to after the paste, as a failed batch merge does.
            let merge_start = Instant::now();
            let settings_for_merge = get_settings(&ah);
            let (merged, brain_details, llm_merge_succeeded, merge_latency_ms) = if let Some((
                final_text,
                _,
                brain,
                failed_chunks,
                _,
            )) =
                streaming_final.clone()
            {
                let succeeded = brain.is_some() && failed_chunks == 0;
                if failed_chunks > 0 {
                    warn!(
                        "Multi-STT streaming: the session ended with {} failed chunk(s)",
                        failed_chunks
                    );
                } else if brain.is_none() {
                    debug!("Multi-STT streaming: no chunk of the session was merged");
                }
                (final_text, brain, succeeded, None)
            } else if merge_requested {
                if use_streaming_overlay {
                    tm.emit_stream_working(StreamWorkKind::Polishing);
                } else {
                    show_processing_overlay(&ah);
                }

                // Poll for cancellation while the LLM round-trip is in
                // flight so Escape aborts the merge instead of waiting on it.
                let Some(merge_outcome) = complete_unless_cancelled(
                    multi_stt_merge_transcriptions(&settings_for_merge, &output_refs),
                    || rm.was_cancelled_since(cancel_generation),
                )
                .await
                else {
                    debug!("Multi-STT: Cancelled during LLM merge");
                    finish_tracked_attempts(
                        &mut tracked1,
                        &mut tracked_extras,
                        StatisticsRunStatus::Cancelled,
                    );
                    statistics.finish(StatisticsRunStatus::Cancelled);
                    utils::hide_recording_overlay(&ah);
                    set_tray_state(&ah, TrayIconState::Idle);
                    return;
                };

                let latency = merge_start.elapsed().as_secs_f64() * 1000.0;

                match merge_outcome {
                    Some(outcome) => {
                        let brain = MultiSttHistoryBrain {
                            provider_id: outcome.provider_id,
                            provider_label: outcome.provider_label,
                            model_name: outcome.model_name,
                            prompt_name: outcome.prompt_name,
                            latency_ms: Some(latency),
                            raw_output: outcome.raw_text,
                            cleaned_output: outcome.cleaned_text.clone(),
                        };
                        (outcome.cleaned_text, Some(brain), true, Some(latency))
                    }
                    None => {
                        // Fallback: concatenate with newlines
                        warn!(
                            "Multi-STT: Merge prompt failed or not configured, concatenating outputs"
                        );
                        let combined = join_nonempty_lines(&output_refs);
                        (combined, None, false, Some(latency))
                    }
                }
            } else {
                // No merge prompt: concatenate
                let combined = join_nonempty_lines(&output_refs);
                (combined, None, false, None)
            };

            // Finish attempts for statistics
            finish_tracked_attempts(
                &mut tracked1,
                &mut tracked_extras,
                StatisticsRunStatus::Success,
            );
            statistics.finish(StatisticsRunStatus::Success);

            // === PERFORMANCE MODE: NORMAL after LLM merge succeeds ===
            // If the Brain LLM merged the outputs, signal normal power mode
            // immediately after merge completes.
            let normal_settings = get_settings(&ah);
            if normal_settings.multi_stt_performance_mode_enabled && llm_merge_succeeded {
                restore_normal_power(
                    &ah,
                    normal_settings
                        .multi_stt_performance_mode_normal_shortcut
                        .clone(),
                );
            }

            // The WAV was written concurrently with the decode; only record
            // a history row that points at a file that actually exists and
            // holds every sample (mirrors TranscribeAction).
            let wav_saved = match wav_handle.await {
                Ok(Ok(())) => {
                    match crate::audio_toolkit::verify_wav_file(&wav_path, wav_sample_count) {
                        Ok(()) => true,
                        Err(e) => {
                            error!("Multi-STT: WAV verification failed: {}", e);
                            false
                        }
                    }
                }
                Ok(Err(e)) => {
                    error!("Multi-STT: Failed to save WAV file: {}", e);
                    false
                }
                Err(e) => {
                    error!("Multi-STT: WAV save task panicked: {}", e);
                    false
                }
            };

            if rm.was_cancelled_since(cancel_generation) {
                debug!("Multi-STT: Cancelled before history save");
                utils::hide_recording_overlay(&ah);
                set_tray_state(&ah, TrayIconState::Idle);
                return;
            }

            // Save to history in background (parallel with paste for speed)
            let model_ids: Vec<String> = std::iter::once(settings.selected_model.clone())
                .chain(
                    configured_extras
                        .iter()
                        .map(|id| id.clone().unwrap_or_else(|| "none".to_string())),
                )
                .collect();

            // Recall insertion mode: the merged text belongs to the note at
            // the caret, not to History. No paste, no history row, no
            // recording file — and the performance-mode power restore still
            // fires, exactly as the ordinary paths do.
            if crate::recall::insertion::is_armed() {
                let (absorb_ah, absorb_path) = (ah.clone(), wav_path.clone());
                tauri::async_runtime::spawn_blocking(move || {
                    crate::recall::insertion::absorb_recording(&absorb_ah, &absorb_path, wav_saved);
                });
                if let Err(e) = crate::recall::insertion::emit_insert(&ah, merged) {
                    error!("Failed to emit recall insert event: {e}");
                }
                utils::hide_recording_overlay(&ah);
                set_tray_state(&ah, TrayIconState::Idle);
                if normal_settings.multi_stt_performance_mode_enabled && !llm_merge_succeeded {
                    restore_normal_power(
                        &ah,
                        normal_settings
                            .multi_stt_performance_mode_normal_shortcut
                            .clone(),
                    );
                }
                return;
            }

            let history_models: Vec<(&str, &str)> = model_ids
                .iter()
                .map(String::as_str)
                .zip(output_refs.iter().copied())
                .collect();
            let multi_transcript =
                format_multi_stt_history_transcript(&history_models, brain_details, &merged);
            let hm_clone = Arc::clone(&hm);
            let file_name = app_identity::multi_recording_file_name(recording_timestamp);
            let merge_prompt_text = settings
                .multi_stt_merge_prompt
                .as_ref()
                .map(|p| p.prompt.clone());
            let merged_for_history = merged.clone();
            let extra_models_list: Vec<String> =
                configured_extras.iter().flatten().cloned().collect();

            let word_count =
                (!merged.trim().is_empty()).then(|| merged.split_whitespace().count() as i32);
            let audio_duration_ms = (wav_sample_count as f64 * 1000.0) / (raw_rate as f64);
            let speech_duration_ms = speech_ms as f64;
            let selected_model = settings.selected_model.clone();
            let selected_language = settings.selected_language.clone();

            if wav_saved {
                tauri::async_runtime::spawn_blocking(move || {
                    if let Err(err) =
                        hm_clone.save_entry_full(crate::managers::history::NewHistoryEntry {
                            file_name,
                            transcription_text: multi_transcript,
                            post_process_requested: merge_requested,
                            post_processed_text: Some(merged_for_history),
                            post_process_prompt: merge_prompt_text,
                            model_id: Some(selected_model),
                            engine: None,
                            audio_duration_ms: Some(audio_duration_ms),
                            speech_duration_ms: Some(speech_duration_ms),
                            sample_rate_hz: Some(raw_rate as i32),
                            word_count,
                            transcription_latency_ms: Some(multi_transcription_latency_ms),
                            post_processing_latency_ms: merge_latency_ms,
                            language: Some(selected_language),
                            mode: Some("multi_stt".to_string()),
                            extra_models: Some(extra_models_list),
                        })
                    {
                        error!("Failed to save multi-STT history entry: {}", err);
                    }
                });
            } else {
                warn!("Multi-STT: Skipping history entry because the WAV could not be saved");
            }

            // A session that typed its text into the app itself has already
            // delivered the result: pasting it again would leave two copies. This
            // is the one case the experimental mode changes about the paste
            // contract (see `Direct Streaming` in AGENTS.md), and it is why the
            // final text is *not* routed through `final_paste_method`.
            if typed_by_session {
                debug!("Multi-STT streaming: the session typed the text; no paste");
                utils::hide_recording_overlay(&ah);
                set_tray_state(&ah, TrayIconState::Idle);
                if normal_settings.multi_stt_performance_mode_enabled && !llm_merge_succeeded {
                    restore_normal_power(
                        &ah,
                        normal_settings
                            .multi_stt_performance_mode_normal_shortcut
                            .clone(),
                    );
                }
            } else if merged.is_empty() {
                utils::hide_recording_overlay(&ah);
                set_tray_state(&ah, TrayIconState::Idle);
                // LLM didn't respond and nothing to paste — still restore power.
                if normal_settings.multi_stt_performance_mode_enabled && !llm_merge_succeeded {
                    restore_normal_power(
                        &ah,
                        normal_settings
                            .multi_stt_performance_mode_normal_shortcut
                            .clone(),
                    );
                }
            } else {
                let ah_clone = ah.clone();
                let final_text = merged;
                let rm_for_paste = Arc::clone(&rm);
                let need_normal_mode =
                    normal_settings.multi_stt_performance_mode_enabled && !llm_merge_succeeded;
                let normal_shortcut = normal_settings
                    .multi_stt_performance_mode_normal_shortcut
                    .clone();
                // The merged text is always a processed result: with direct
                // streaming configured the live stream was preview-only and
                // the result goes in through the default clipboard paste.
                let paste_override =
                    final_paste_method(live_stream_is_preview_only(&normal_settings, true));
                // If the main-thread dispatch itself fails the closure never
                // runs, so keep what the fallback needs to restore power.
                let fallback_normal_shortcut = need_normal_mode.then(|| normal_shortcut.clone());
                ah.run_on_main_thread(move || {
                    if rm_for_paste.was_cancelled_since(cancel_generation) {
                        debug!("Multi-STT: Cancelled before paste");
                        utils::hide_recording_overlay(&ah_clone);
                        set_tray_state(&ah_clone, TrayIconState::Idle);
                        return;
                    }
                    match utils::paste_with_method(final_text, ah_clone.clone(), paste_override) {
                        Ok(()) => debug!("Multi-STT: Text pasted successfully"),
                        Err(e) => {
                            error!("Multi-STT: Failed to paste transcription: {}", e);
                            let _ = ah_clone.emit("paste-error", ());
                        }
                    }
                    // LLM didn't respond — restore normal power after paste.
                    if need_normal_mode {
                        restore_normal_power(&ah_clone, normal_shortcut.clone());
                    }
                    utils::hide_recording_overlay(&ah_clone);
                    set_tray_state(&ah_clone, TrayIconState::Idle);
                })
                .unwrap_or_else(|e| {
                    error!("Multi-STT: Failed to run paste on main thread: {:?}", e);
                    if let Some(shortcut) = fallback_normal_shortcut {
                        restore_normal_power(&ah, shortcut);
                    }
                    utils::hide_recording_overlay(&ah);
                    set_tray_state(&ah, TrayIconState::Idle);
                });
            }
        });

        debug!(
            "MultiSttAction::stop completed in {:?}",
            stop_time.elapsed()
        );
    }
}

/// Finish every Multi-STT decode attempt of a run with `status` (a successful
/// run also completes each attempt's post-processing step, the merge).
fn finish_tracked_attempts(
    primary: &mut Option<TrackedTranscription>,
    extras: &mut [Option<TrackedTranscription>],
    status: StatisticsRunStatus,
) {
    for tracked in std::iter::once(primary).chain(extras.iter_mut()) {
        if let Some(t) = tracked.take() {
            if status == StatisticsRunStatus::Success {
                t.attempt.complete_post_processing();
            }
            t.attempt.finish(status);
        }
    }
}

/// The Multi-STT fallback when there is no merged text: every non-empty output
/// on its own line, in slot order. Shared with `multi_stt_stream`, whose failed
/// chunks read exactly like a failed Multi-STT run.
pub(crate) fn join_nonempty_lines(parts: &[&str]) -> String {
    let mut combined = String::new();
    for part in parts.iter().filter(|part| !part.is_empty()) {
        if !combined.is_empty() {
            combined.push('\n');
        }
        combined.push_str(part);
    }
    combined
}

/// Whether a merge prompt is configured at all. Shared with
/// `multi_stt_stream`, which refuses to arm the experimental streaming mode
/// without one: that mode's whole contract is "the merged text replaces the
/// rough text", and with nothing to merge with it would only quadruple the
/// live transcript on screen.
pub(crate) fn has_merge_prompt(settings: &AppSettings) -> bool {
    settings
        .multi_stt_merge_prompt
        .as_ref()
        .is_some_and(|p| !p.prompt.trim().is_empty())
}

// Static Action Map
pub static ACTION_MAP: Lazy<HashMap<String, Arc<dyn ShortcutAction>>> = Lazy::new(|| {
    let mut map = HashMap::new();
    map.insert(
        "transcribe".to_string(),
        Arc::new(TranscribeAction {
            post_process: false,
        }) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "transcribe_with_post_process".to_string(),
        Arc::new(TranscribeAction { post_process: true }) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "cancel".to_string(),
        Arc::new(CancelAction) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "test".to_string(),
        Arc::new(TestAction) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "multi_stt_transcribe".to_string(),
        Arc::new(MultiSttAction) as Arc<dyn ShortcutAction>,
    );
    map
});

#[cfg(test)]
mod tests {
    use super::{
        complete_unless_cancelled, effective_overlay_style, final_paste_method,
        is_blank_transcription, live_stream_is_preview_only, merge_response_rejection,
        should_use_streaming_overlay, slots_carry_words, strip_think_block,
    };
    use crate::settings::{AppSettings, OverlayStyle, PasteMethod};
    use std::future;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
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
    fn leading_think_block_is_stripped() {
        assert_eq!(
            strip_think_block("<think>pondering...</think>Cleaned text."),
            "Cleaned text."
        );
        assert_eq!(
            strip_think_block("  \n<think>multi\nline</think>\n  Cleaned text."),
            "Cleaned text."
        );
    }

    #[test]
    fn content_without_think_block_is_unchanged() {
        assert_eq!(strip_think_block("Cleaned text."), "Cleaned text.");
        assert_eq!(
            strip_think_block("Mentions <think> mid-sentence."),
            "Mentions <think> mid-sentence."
        );
        // Unclosed block: leave untouched rather than guess
        assert_eq!(
            strip_think_block("<think>never closed"),
            "<think>never closed"
        );
    }

    // A merge slot set with no words in it is skipped, not merged. The four
    // slots of a chunk that decoded nothing but a punctuation fragment are
    // exactly how a small merge model is invited to answer the *prompt* — the
    // "Please provide the four raw audio transcripts..." reply that gets pasted
    // as a transcript. `.` is the observed case: a slot of one period.
    #[test]
    fn slots_without_a_word_are_not_worth_merging() {
        assert!(!slots_carry_words(&["", "", "", ""]));
        assert!(!slots_carry_words(&["   ", "\t\n", "", ""]));
        assert!(!slots_carry_words(&[".", "", "", ""]));
        assert!(!slots_carry_words(&["...", "…", "-", "?"]));
        assert!(slots_carry_words(&[".", "Sentence one.", "", ""]));
        // Scripts that are not Latin are words too, or Chinese and Arabic audio
        // would have its merges skipped.
        assert!(slots_carry_words(&["", "", "这是一句话。", ""]));
        assert!(slots_carry_words(&["", "", "", "مرحبا"]));
        assert!(slots_carry_words(&["", "", "", "42"]));
    }

    #[test]
    fn a_merge_that_asks_for_its_input_is_rejected() {
        let slots = [".", "", "", ""];
        let refusal = "Please provide the four raw audio transcripts so I can perform the \
                       multi-source merge and consensus according to your strict rules.";
        let reason = merge_response_rejection(refusal, &slots).expect("rejected");
        assert!(reason.contains("asks for its own input"), "{reason}");

        // The same sentence with a longer input cannot be caught by the length
        // test, which is what the phrase list is for.
        let long_slot = "x".repeat(400);
        let long_slots = [long_slot.as_str(), "", "", ""];
        assert!(merge_response_rejection(refusal, &long_slots).is_some());

        for reply in [
            "I need the transcripts to continue.",
            "No transcripts were provided, so there is nothing to merge.",
            "Once you provide the audio transcripts I will proceed.",
            "There are no transcripts in your message.",
        ] {
            assert!(
                merge_response_rejection(reply, &["Sentence one.", "", "", ""]).is_some(),
                "{reply}"
            );
        }
    }

    #[test]
    fn a_real_merge_is_not_rejected() {
        let slots = [
            "This is now sentence two.",
            "This is sentence one. This is now sentence two.",
            "",
            "",
        ];
        assert_eq!(
            merge_response_rejection("This is sentence one. This is now sentence two.", &slots),
            None
        );
        // A single-slot merge is legitimate: the cleaning rules apply to one
        // transcript as much as to four.
        assert_eq!(
            merge_response_rejection(
                "le budget de 150000 EUR",
                &["le budget de 120000 euros, mais correction, 150000 euros"],
            ),
            None
        );
        // Complementary partials: the output may be nearly the sum of its
        // inputs, which is the largest a merge can honestly be.
        let parts = [
            "alpha bravo charlie",
            "delta echo foxtrot",
            "golf hotel",
            "india",
        ];
        let joined = parts.join(" ");
        assert_eq!(merge_response_rejection(&joined, &parts), None);
        // Dictated speech may even contain a phrase from the request list, as
        // long as it is not addressed at the reader with the missing noun.
        assert_eq!(
            merge_response_rejection(
                "Please provide the report to accounting by Friday.",
                &["Please provide the report to accounting by Friday.", ""],
            ),
            None
        );
        // And a dictated phrase that *is* on the list survives, because the
        // model did not bring it: it is what the speaker said. Cleaning rules
        // still apply to the rest of the sentence around it.
        assert_eq!(
            merge_response_rejection(
                "Please provide the transcripts to the court by 5:00 PM.",
                &[
                    "Please provide the transcripts to the court by five PM.",
                    "Please provide the transcripts to the courthouse by 5 PM.",
                ],
            ),
            None
        );
        // The same words with no such sentence behind them are the model
        // asking for its input, and are rejected however short they are.
        assert!(
            merge_response_rejection(
                "Please provide the transcripts to continue.",
                &["Sentence two.", "Sentence two", "", ""],
            )
            .is_some()
        );
    }

    #[test]
    fn an_implausibly_long_reply_is_rejected() {
        let slots = ["Sentence one.", "Test sentence one", "", ""];
        // 145 chars against a 17-char longest input (limit 17 * 4 + 64 = 132):
        // not a merge of what it was given, whatever it says.
        let hallucination = "This is a transcript that was never in any of the inputs, padded out to a length \
             no reconciliation of two short sentences could reach on its own.";
        let reason = merge_response_rejection(hallucination, &slots).expect("rejected");
        assert!(reason.contains("chars against"), "{reason}");
        // The boundary itself is allowed: four disjoint inputs plus the slack.
        let twenty = "a".repeat(20);
        let slots = [twenty.as_str()];
        assert_eq!(merge_response_rejection(&"b".repeat(144), &slots), None);
        assert!(merge_response_rejection(&"b".repeat(145), &slots).is_some());
        // An empty reply is the callers' own case, not a rejection here.
        assert_eq!(merge_response_rejection("", &slots), None);
    }

    #[test]
    fn live_overlay_uses_streaming_states_only_for_streaming_models() {
        assert!(should_use_streaming_overlay(OverlayStyle::Live, true));
        assert!(!should_use_streaming_overlay(OverlayStyle::Live, false));
        assert!(!should_use_streaming_overlay(OverlayStyle::Minimal, true));
        assert!(!should_use_streaming_overlay(OverlayStyle::None, true));
    }

    fn settings_with(paste_method: PasteMethod, overlay_style: OverlayStyle) -> AppSettings {
        let mut settings = crate::settings::get_default_settings();
        settings.paste_method = paste_method;
        settings.overlay_style = overlay_style;
        settings
    }

    #[test]
    fn direct_streaming_types_live_only_for_plain_transcription() {
        let direct = settings_with(PasteMethod::DirectStreaming, OverlayStyle::Minimal);
        assert!(!live_stream_is_preview_only(&direct, false));
        assert!(live_stream_is_preview_only(&direct, true));

        let clipboard = settings_with(PasteMethod::CtrlV, OverlayStyle::Minimal);
        assert!(!live_stream_is_preview_only(&clipboard, true));
    }

    #[test]
    fn preview_only_stream_forces_live_overlay_when_model_streams() {
        let settings = settings_with(PasteMethod::DirectStreaming, OverlayStyle::Minimal);
        assert_eq!(
            effective_overlay_style(&settings, true, true),
            OverlayStyle::Live
        );
        // No stream to preview: keep the user's choice.
        assert_eq!(
            effective_overlay_style(&settings, true, false),
            OverlayStyle::Minimal
        );
        // Plain transcription: keep the user's choice.
        assert_eq!(
            effective_overlay_style(&settings, false, true),
            OverlayStyle::Minimal
        );
        let none = settings_with(PasteMethod::DirectStreaming, OverlayStyle::None);
        assert_eq!(
            effective_overlay_style(&none, true, true),
            OverlayStyle::Live
        );
    }

    #[test]
    fn processed_results_paste_with_ctrl_v_when_stream_was_preview_only() {
        assert_eq!(final_paste_method(true), Some(PasteMethod::CtrlV));
        assert_eq!(final_paste_method(false), None);
    }
}
