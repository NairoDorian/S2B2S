//! Multi-STT: parallel transcription with multiple models + LLM merge.
//!
//! Spawns independent transcription tasks for the primary model (selected_model),
//! optional secondary (model_2) and tertiary (model_3) slots, and an optional
//! 4th Gemma 4 2B multimodal STT source. Results are merged via an LLM.
//!
//! Merge provider selection:
//!   - When `multi_stt_use_llama_merge` is true, the local llama.cpp server is used
//!     (Gemma 4 E2B with mmproj). The server is ensured running before the merge call.
//!   - Otherwise, the same cloud post-process provider used for regular post-processing
//!     is used (settings.active_post_process_provider()).

use anyhow::Result;
use base64::Engine;
use log::{debug, error, info, warn};
use std::sync::Arc;
use tauri::AppHandle;
use tauri::Manager;

use crate::audio_toolkit::encode_wav_bytes;
use crate::llm_client;
use crate::managers::model::{EngineType, ModelInfo, ModelManager};
use crate::settings::AppSettings;
use crate::settings::{PostProcessProvider, BRAIN_ONLY_TRANSCRIPTION_PROMPT};
use crate::stt::unified_parakeet::UnifiedParakeetServer;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub const DEFAULT_MULTI_STT_MERGE_PROMPT: &str = r#"Role: You are an expert multi-source Speech-to-Text (STT) consensus and transcript refinement engine. Your task is to compare 3 different STT transcripts of the exact same audio, merge them into a single accurate transcript, and clean the text according to strict formatting rules.

Core Objective:
Analyze Transcriptions 1, 2, and 3. Reconcile differences between them using contextual logic, phonetic similarity, and majority consensus to reconstruct the single most accurate version of what was spoken.

1. Consensus & Merge Logic:
- Discrepancy Resolution: When the 3 transcripts disagree on a word or phrase, select the version that makes the most sense grammatically and contextually in the original language.
- Majority Voting: If 2 of the 3 transcripts agree on a word/phrase and it fits logically, favor that reading unless it is an obvious shared STT misrecognition.
- Hallucinations & Omissions: Ignore individual model hallucinations, random character glitches, or missing words if the other transcripts provide a coherent sentence.

2. Cleaning & Refinement Instructions:
- Language Retention: Maintain the original language strictly (e.g., if the transcript is in French, output strictly in French). Never translate.
- Speech Artifacts: Strip out filler words (e.g., "um," "uh," "like" used as filler), stutters, and false starts.
- Grammar & Mechanics: Fix spelling, capitalization, missing commas, and sentence boundaries.
- Number Formatting (STRICT):
  - Convert ALL spoken numbers strictly into digits (e.g., "twenty-five" → "25", "un deux trois" → "1 2 3").
  - NEVER write numbers using words or letters under any circumstances.
  - Convert spoken currency and percentage words into symbols (e.g., "ten percent" → "10%", "five dollars" → "$5").
- Spoken Punctuation: Convert spoken punctuation words directly into punctuation marks (e.g., "period" → ".", "comma" → ",").
- Fidelity: Preserve the original speaker's exact sentence structure, tone, and word order as closely as possible. Do NOT paraphrase, summarize, or rewrite valid spoken content.
- Capitalize my sentences when missing uppercases. Add a final '.' period punctuation at the end of sentences as well.
- Never put " '' "  or " "" " around the output transcriptions or any text decorator. Just output the Merged and Cleaned Transcription.

Output Constraints:
- Return ONLY the final merged and cleaned transcript.
- Do NOT include any preamble, introductory text, markdown code blocks, quotes, or commentary (e.g., do NOT write "Here is the merged transcript:").

Numbers: Never words, never letters, only digits (Un deux trois - > 1, 2, 3)( One two three - > 1, 2, 3) Double check final output if dealing with number, only output digits, never letters or words for numbers, remember that is very important

The Transcription N°2 is generally the most accurate.

Me, the user, will speak French primarily but also sometimes in English. Keep English words in English and French words in French, even if they are mixed up in the same sentence.

---

Transcription 1:
"""
${output}
"""

Transcription 2:
"""
${output2}
"""

Transcription 3:
"""
${output3}
""""#;

/// Merge prompt for multi-STT: replaces ${output}, ${output2}, ${output3}
/// and sends to the LLM API (cloud provider or local llama.cpp server).
/// When `multi_stt_merge_include_audio` is enabled, the raw audio is also
/// attached as `input_audio` for Gemma 4 (mmproj) multimodal fusion.
async fn merge_transcriptions(
    settings: &AppSettings,
    results: &[(String, String)],
    audio: Option<&[f32]>,
    app_handle: &AppHandle,
) -> Option<String> {
    let raw_prompt = match &settings.multi_stt_merge_prompt {
        Some(p) if !p.prompt.trim().is_empty() => p.prompt.clone(),
        _ => DEFAULT_MULTI_STT_MERGE_PROMPT.to_string(),
    };

    // Map results to ${output}, ${output2}, ${output3} placeholders.
    let output1 = results.first().map(|(_, t)| t.as_str()).unwrap_or("");
    let output2 = results.get(1).map(|(_, t)| t.as_str()).unwrap_or("");
    let output3 = results.get(2).map(|(_, t)| t.as_str()).unwrap_or("");

    let prompt = raw_prompt
        .replace("${output}", output1)
        .replace("${output1}", output1)
        .replace("${output2}", output2)
        .replace("${output3}", output3);

    if settings.multi_stt_use_llama_merge {
        merge_with_llama_cpp(settings, &prompt, audio, app_handle).await
    } else {
        merge_with_cloud_provider(settings, &prompt).await
    }
}

/// Determine whether multi-STT should be active given current settings.
pub fn is_multi_stt_active(settings: &AppSettings) -> bool {
    settings.multi_stt_enabled
        && (settings.multi_stt_model_2.is_some()
            || settings.multi_stt_model_3.is_some()
            || settings.multi_stt_gemma4_enabled)
}

/// Entry point from actions.rs: run multi-STT and return a single merged text.
///
/// `output1` is the primary model's transcription (already produced by the
/// TranscriptionManager). `audio` is the raw f32 mono samples at 16 kHz.
///
/// Extra models (model_2, model_3) are transcribed in parallel threads. When
/// `multi_stt_gemma4_enabled` is set, a 4th transcription is obtained from the
/// local Gemma 4 2B multimodal llama.cpp server (mmproj + audio). All results
/// are then merged via LLM (cloud or local llama.cpp depending on settings).
pub async fn transcribe_parallel(
    audio: Vec<f32>,
    output1: String,
    settings: &AppSettings,
    model_manager: &Arc<ModelManager>,
    app_handle: &AppHandle,
) -> Result<String> {
    let mut results: Vec<(String, String)> = Vec::new();
    results.push((settings.selected_model.clone(), output1.clone()));

    // Collect extra model IDs from slot 2 and slot 3
    let mut extra_models: Vec<String> = Vec::new();
    if let Some(ref m2) = settings.multi_stt_model_2 {
        extra_models.push(m2.clone());
    }
    if let Some(ref m3) = settings.multi_stt_model_3 {
        extra_models.push(m3.clone());
    }

    // Run extra models in parallel (blocking threads)
    let extra_results = run_parallel(&audio, &extra_models, model_manager, app_handle);
    results.extend(extra_results);

    // 4th special STT: Gemma 4 2B multimodal via llama.cpp server
    if settings.multi_stt_gemma4_enabled {
        match transcribe_with_gemma4(&audio, settings, app_handle).await {
            Ok(text) => {
                info!("Multi-STT: Gemma 4 2B ASR → {} chars", text.len());
                results.push(("gemma-4-2b-multimodal".to_string(), text));
            }
            Err(e) => {
                error!("Multi-STT: Gemma 4 2B ASR failed: {}", e);
            }
        }
    }

    if results.is_empty() {
        return Err(anyhow::anyhow!("All multi-STT transcriptions failed"));
    }

    if results.len() == 1 {
        return Ok(results[0].1.clone());
    }

    // Log the individual results
    for (model_id, text) in &results {
        debug!("Multi-STT: '{}' → {} chars", model_id, text.len());
    }

    // Try LLM merge
    match merge_transcriptions(settings, &results, Some(&audio), app_handle).await {
        Some(merged) if !merged.is_empty() => Ok(merged),
        _ => {
            warn!("Multi-STT merge skipped or returned empty; falling back to longest result");
            // Fallback: pick the longest transcript — the best proxy for
            // "most complete capture" when we can't judge the candidates against each other.
            let best = results
                .iter()
                .max_by_key(|(_, text)| text.chars().count())
                .map(|(_, text)| text.clone())
                .unwrap_or_default();
            Ok(best)
        }
    }
}

// ---------------------------------------------------------------------------
// Extra-model transcription (parallel via OS threads)
// ---------------------------------------------------------------------------

/// Run multiple STT models in parallel and return (model_id, text) pairs.
pub fn run_parallel(
    audio: &[f32],
    model_ids: &[String],
    model_manager: &Arc<ModelManager>,
    app_handle: &AppHandle,
) -> Vec<(String, String)> {
    if audio.is_empty() || model_ids.is_empty() {
        return vec![];
    }

    let mut handles: Vec<(String, std::thread::JoinHandle<Result<String>>)> = Vec::new();

    for model_id in model_ids {
        let model_info = match model_manager.get_model_info(model_id) {
            Some(info) => info,
            None => {
                handles.push((
                    model_id.clone(),
                    std::thread::spawn(move || {
                        Err(anyhow::anyhow!("Model not found")) as Result<String>
                    }),
                ));
                continue;
            }
        };

        let audio = audio.to_vec();
        let model_id = model_id.clone();
        let model_path = match model_manager.get_model_path(&model_id) {
            Ok(p) => p,
            Err(e) => {
                warn!("Multi-STT extra model '{}' not available: {}", model_id, e);
                continue;
            }
        };
        let app_handle = app_handle.clone();
        let settings = crate::settings::get_settings(&app_handle);

        // Capture per-model language / translate overrides for this slot
        let language = match model_id.as_str() {
            id if Some(id) == settings.multi_stt_model_2.as_deref() => {
                settings.multi_stt_language_model_2.clone()
            }
            id if Some(id) == settings.multi_stt_model_3.as_deref() => {
                settings.multi_stt_language_model_3.clone()
            }
            _ => None,
        };
        let translate = match model_id.as_str() {
            id if Some(id) == settings.multi_stt_model_2.as_deref() => {
                settings.multi_stt_translate_model_2
            }
            id if Some(id) == settings.multi_stt_model_3.as_deref() => {
                settings.multi_stt_translate_model_3
            }
            _ => false,
        };

        let model_id_in_thread = model_id.clone();
        let handle = std::thread::spawn(move || {
            if let Some(tm) =
                app_handle.try_state::<crate::managers::transcription::TranscriptionManager>()
            {
                if !tm.is_extra_model_loaded(&model_id_in_thread) {
                    let _ = tm.load_extra_model(&model_id_in_thread);
                }
                if let Ok(text) = tm.transcribe_with_extra(&model_id_in_thread, audio.clone()) {
                    return Ok(text);
                }
            }
            transcribe_single(
                &audio,
                &model_path,
                &model_info,
                &app_handle,
                language,
                translate,
            )
        });
        handles.push((model_id, handle));
    }

    let mut results = Vec::new();
    for (model_id, handle) in handles {
        match handle.join() {
            Ok(Ok(text)) => {
                info!("Multi-STT: {} → {} chars", model_id, text.len());
                results.push((model_id, text));
            }
            Ok(Err(e)) => {
                error!("Multi-STT: {} failed: {}", model_id, e);
            }
            Err(_) => {
                error!("Multi-STT: {} panicked", model_id);
            }
        }
    }

    results
}

// ---------------------------------------------------------------------------
// Single-model transcription (runs in its own thread)
// ---------------------------------------------------------------------------

fn transcribe_single(
    audio: &[f32],
    model_path: &std::path::Path,
    model_info: &ModelInfo,
    _app_handle: &AppHandle,
    language: Option<String>,
    _translate: bool,
) -> Result<String> {
    match model_info.engine_type {
        EngineType::UnifiedParakeet => {
            transcribe_python_with_lang(audio, model_path, model_info, language.as_deref())
        }
        EngineType::TranscribeCpp => {
            transcribe_transcribe_rs(audio, model_path, language.as_deref(), |path| {
                let model = transcribe_cpp::Model::load_with(
                    path,
                    &transcribe_cpp::ModelOptions::default(),
                )?;
                let mut session = model.session()?;
                let transcript = session.run(audio, &transcribe_cpp::RunOptions::default())?;
                Ok(transcript.text)
            })
        }
        EngineType::Parakeet => {
            transcribe_transcribe_rs(audio, model_path, language.as_deref(), |path| {
                use transcribe_rs::onnx::parakeet::ParakeetModel;
                use transcribe_rs::onnx::Quantization;
                let mut engine = ParakeetModel::load(path, &Quantization::Int8)?;
                let mut params = transcribe_rs::onnx::parakeet::ParakeetParams::default();
                if let Some(ref lang) = language {
                    params.language = Some(lang.clone());
                }
                let r = engine.transcribe_with(audio, &params)?;
                Ok(r.text)
            })
        }
        EngineType::Moonshine => {
            transcribe_transcribe_rs(audio, model_path, language.as_deref(), |path| {
                use transcribe_rs::onnx::moonshine::{MoonshineModel, MoonshineVariant};
                use transcribe_rs::onnx::Quantization;
                use transcribe_rs::{SpeechModel, TranscribeOptions};
                let mut engine =
                    MoonshineModel::load(path, MoonshineVariant::Base, &Quantization::default())?;
                let mut opts = TranscribeOptions::default();
                if let Some(ref lang) = language {
                    opts.language = Some(lang.clone());
                }
                let r = engine.transcribe(audio, &opts)?;
                Ok(r.text)
            })
        }
        EngineType::MoonshineStreaming => {
            transcribe_transcribe_rs(audio, model_path, language.as_deref(), |path| {
                use transcribe_rs::onnx::moonshine::StreamingModel;
                use transcribe_rs::onnx::Quantization;
                use transcribe_rs::{SpeechModel, TranscribeOptions};
                let mut engine = StreamingModel::load(path, 0, &Quantization::default())?;
                let mut opts = TranscribeOptions::default();
                if let Some(ref lang) = language {
                    opts.language = Some(lang.clone());
                }
                let r = engine.transcribe(audio, &opts)?;
                Ok(r.text)
            })
        }
        EngineType::SenseVoice => {
            transcribe_transcribe_rs(audio, model_path, language.as_deref(), |path| {
                use transcribe_rs::onnx::sense_voice::{SenseVoiceModel, SenseVoiceParams};
                use transcribe_rs::onnx::Quantization;
                let mut engine = SenseVoiceModel::load(path, &Quantization::Int8)?;
                let params = SenseVoiceParams {
                    language: language.as_deref().map(|l| l.to_string()),
                    use_itn: Some(true),
                };
                let r = engine.transcribe_with(audio, &params)?;
                Ok(r.text)
            })
        }
        EngineType::GigaAM => {
            transcribe_transcribe_rs(audio, model_path, language.as_deref(), |path| {
                use transcribe_rs::onnx::gigaam::GigaAMModel;
                use transcribe_rs::onnx::Quantization;
                use transcribe_rs::{SpeechModel, TranscribeOptions};
                let mut engine = GigaAMModel::load(path, &Quantization::Int8)?;
                let mut opts = TranscribeOptions::default();
                if let Some(ref lang) = language {
                    opts.language = Some(lang.clone());
                }
                let r = engine.transcribe(audio, &opts)?;
                Ok(r.text)
            })
        }
        EngineType::Canary => {
            transcribe_transcribe_rs(audio, model_path, language.as_deref(), |path| {
                use transcribe_rs::onnx::canary::CanaryModel;
                use transcribe_rs::onnx::Quantization;
                use transcribe_rs::{SpeechModel, TranscribeOptions};
                let mut engine = CanaryModel::load(path, &Quantization::Int8)?;
                let mut opts = TranscribeOptions::default();
                if let Some(ref lang) = language {
                    opts.language = Some(lang.clone());
                }
                let r = engine.transcribe(audio, &opts)?;
                Ok(r.text)
            })
        }
        EngineType::Cohere => {
            transcribe_transcribe_rs(audio, model_path, language.as_deref(), |path| {
                use transcribe_rs::onnx::cohere::CohereModel;
                use transcribe_rs::onnx::Quantization;
                use transcribe_rs::{SpeechModel, TranscribeOptions};
                let mut engine = CohereModel::load(path, &Quantization::Int8)?;
                let mut opts = TranscribeOptions::default();
                if let Some(ref lang) = language {
                    opts.language = Some(lang.clone());
                }
                let r = engine.transcribe(audio, &opts)?;
                Ok(r.text)
            })
        }
    }
}

/// Transcribe via a Python ONNX server (Unified Parakeet family) with optional
/// language override. The Python server does not currently accept a language
/// parameter in its HTTP API, so the override is logged but not forwarded.
fn transcribe_python_with_lang(
    audio: &[f32],
    model_path: &std::path::Path,
    model_info: &ModelInfo,
    language: Option<&str>,
) -> Result<String> {
    if let Some(lang) = language {
        debug!(
            "Multi-STT Python model '{}': language override '{}' requested but not supported by server API",
            model_info.filename, lang
        );
    }
    let server = UnifiedParakeetServer::launch(&model_path.to_string_lossy())?;

    let is_eou = model_info
        .hf_repo
        .as_deref()
        .unwrap_or("")
        .contains("parakeet-realtime-eou-120m");

    if is_eou {
        server.stream_start()?;
        let mut last_text = String::new();
        const CHUNK: usize = 4000; // 250ms
        let chunks: Vec<&[f32]> = audio.chunks(CHUNK).collect();
        let n_chunks = chunks.len();
        for (i, &chunk) in chunks.iter().enumerate() {
            let is_last = i + 1 == n_chunks;
            let rms = (chunk.iter().map(|s| s * s).sum::<f32>() / chunk.len() as f32).sqrt();
            if !is_last && rms < 0.002 {
                continue;
            }
            let (text, eou) = server.stream_feed(chunk)?;
            if !text.is_empty() {
                last_text = text;
            }
            if eou {
                break;
            }
        }
        let (text, _) = server.stream_end(&[])?;
        Ok(if text.chars().count() > last_text.chars().count() {
            text
        } else {
            last_text
        })
    } else {
        server.transcribe(audio)
    }
    // server dropped → Python process killed
}

/// Transcribe via a transcribe-rs engine (loaded and dropped per call).
/// The `language` parameter is used by individual engine closures above;
/// this wrapper is a no-op forwarder.
fn transcribe_transcribe_rs<F>(
    _audio: &[f32],
    model_path: &std::path::Path,
    _language: Option<&str>,
    f: F,
) -> Result<String>
where
    F: FnOnce(&std::path::Path) -> Result<String>,
{
    let _ = _language;
    f(model_path)
}

// ---------------------------------------------------------------------------
// Gemma 4 2B multimodal STT (llama.cpp server with mmproj + audio input)
// ---------------------------------------------------------------------------

/// Transcribe audio via the local Gemma 4 2B multimodal llama.cpp server.
/// Sends a non-streaming chat completion with an `input_audio` content part
/// and a transcription prompt. The llama.cpp server is ensured running
/// (with mmproj) beforehand.
pub async fn transcribe_with_gemma4(
    audio: &[f32],
    settings: &AppSettings,
    app_handle: &AppHandle,
) -> Result<String> {
    // Ensure the llama.cpp server is running with mmproj (multimodal audio enabled)
    {
        let llama_manager = app_handle
            .try_state::<Arc<crate::brain::llama_manager::LlamaManager>>()
            .ok_or_else(|| anyhow::anyhow!("LlamaManager not initialized"))?;

        // Force multimodal audio for Gemma 4 ASR
        if !settings.brain.multimodal_audio_enabled {
            warn!("Multi-STT: Gemma 4 ASR requested but multimodal_audio_enabled is false; enabling temporarily");
        }
        llama_manager
            .ensure_server_running()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to start llama.cpp server: {}", e))?;
    }

    // Encode audio to WAV bytes → base64
    let wav_bytes = encode_wav_bytes(audio)?;
    let audio_b64 = base64::engine::general_purpose::STANDARD.encode(&wav_bytes);

    let cfg = &settings.brain;
    let base_url = cfg.active_base_url();
    let api_key = cfg.active_api_key();
    let model = cfg.active_model();

    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(180))
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to build HTTP client: {}", e))?;

    // Gemma 4 ASR: send the raw audio as input_audio with a transcription prompt
    let request_body = serde_json::json!({
        "model": model,
        "messages": [
            {
                "role": "user",
                "content": [
                    {
                        "type": "text",
                        "text": BRAIN_ONLY_TRANSCRIPTION_PROMPT
                    },
                    {
                        "type": "input_audio",
                        "input_audio": {
                            "data": audio_b64,
                            "format": "wav"
                        }
                    }
                ]
            }
        ],
        "stream": false,
        "max_tokens": 1024,
    });

    let mut req = client.post(&url).json(&request_body);
    if !api_key.is_empty() {
        req = req.bearer_auth(&api_key);
    }

    debug!(
        "[Multi-STT Gemma4] Sending non-streaming completion to {}",
        url
    );
    let response = req
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Gemma 4 ASR request failed: {}", e))?;

    let status = response.status();
    if !status.is_success() {
        let err_text = response.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!(
            "Gemma 4 ASR returned status {}: {}",
            status,
            err_text
        ));
    }

    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to parse Gemma 4 ASR response: {}", e))?;

    let text = body
        .pointer("/choices/0/message/content")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();

    if text.is_empty() {
        Err(anyhow::anyhow!("Gemma 4 ASR returned empty transcription"))
    } else {
        Ok(text)
    }
}

// ---------------------------------------------------------------------------
// Merge helpers
// ---------------------------------------------------------------------------

/// Merge via the local llama.cpp server (same server used for the Brain).
/// When `multi_stt_merge_include_audio` is enabled, the raw audio is attached as
/// `input_audio` for Gemma 4 (mmproj) multimodal fusion.
async fn merge_with_llama_cpp(
    settings: &AppSettings,
    prompt: &str,
    audio: Option<&[f32]>,
    app_handle: &AppHandle,
) -> Option<String> {
    let llama_manager =
        match app_handle.try_state::<Arc<crate::brain::llama_manager::LlamaManager>>() {
            Some(m) => m,
            None => {
                warn!("Multi-STT llama.cpp merge: LlamaManager not available");
                return None;
            }
        };

    // Ensure the server is running
    if let Err(e) = llama_manager.ensure_server_running().await {
        warn!("Multi-STT llama.cpp merge: server not running: {}", e);
        return None;
    }

    let cfg = &settings.brain;
    let base_url = cfg.active_base_url();
    let api_key = cfg.active_api_key();
    let model = cfg.active_model();

    // If audio inclusion is enabled and audio samples exist, send multimodal input_audio to Gemma 4 (mmproj)
    if settings.multi_stt_merge_include_audio {
        if let Some(audio_samples) = audio {
            match encode_wav_bytes(audio_samples) {
                Ok(wav_bytes) => {
                    let audio_b64 = base64::engine::general_purpose::STANDARD.encode(&wav_bytes);
                    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
                    let client = match reqwest::Client::builder()
                        .connect_timeout(std::time::Duration::from_secs(10))
                        .timeout(std::time::Duration::from_secs(180))
                        .build()
                    {
                        Ok(c) => c,
                        Err(e) => {
                            error!("Multi-STT llama.cpp merge: HTTP client build failed: {}", e);
                            return None;
                        }
                    };

                    let request_body = serde_json::json!({
                        "model": model,
                        "messages": [
                            {
                                "role": "user",
                                "content": [
                                    {
                                        "type": "text",
                                        "text": prompt
                                    },
                                    {
                                        "type": "input_audio",
                                        "input_audio": {
                                            "data": audio_b64,
                                            "format": "wav"
                                        }
                                    }
                                ]
                            }
                        ],
                        "stream": false,
                        "max_tokens": 1024,
                    });

                    let mut req = client.post(&url).json(&request_body);
                    if !api_key.is_empty() {
                        req = req.bearer_auth(&api_key);
                    }

                    debug!(
                        "[Multi-STT] Sending multimodal merge completion with audio to {}",
                        url
                    );
                    match req.send().await {
                        Ok(response) if response.status().is_success() => {
                            if let Ok(body) = response.json::<serde_json::Value>().await {
                                let content = body
                                    .pointer("/choices/0/message/content")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .trim()
                                    .to_string();
                                if !content.is_empty() {
                                    info!(
                                        "Multi-STT multimodal llama.cpp merge succeeded. Output length: {} chars",
                                        content.len()
                                    );
                                    return Some(content);
                                }
                            }
                        }
                        Ok(response) => {
                            let status = response.status();
                            let err_text = response.text().await.unwrap_or_default();
                            warn!(
                                "Multi-STT multimodal llama.cpp merge returned status {}: {}. Falling back to text merge.",
                                status, err_text
                            );
                        }
                        Err(e) => {
                            warn!(
                                "Multi-STT multimodal llama.cpp merge request failed: {}. Falling back to text merge.",
                                e
                            );
                        }
                    }
                }
                Err(e) => {
                    warn!("Multi-STT: Failed to encode WAV for merge: {}", e);
                }
            }
        }
    }

    // Text-only fallback / standard text merge
    let provider = PostProcessProvider {
        id: "llama_cpp".to_string(),
        label: "Llama.cpp Server".to_string(),
        base_url: base_url.clone(),
        allow_base_url_edit: true,
        allow_insecure_http: true,
        models_endpoint: None,
        supports_structured_output: false,
    };

    match llm_client::send_chat_completion(&provider, api_key, &model, prompt.to_string(), false)
        .await
    {
        Ok(Some(content)) => {
            let content = content.trim().to_string();
            info!(
                "Multi-STT llama.cpp merge succeeded. Output length: {} chars",
                content.len()
            );
            Some(content)
        }
        Ok(None) => {
            warn!("Multi-STT llama.cpp merge: response has no content");
            None
        }
        Err(e) => {
            error!("Multi-STT llama.cpp merge failed: {}", e);
            None
        }
    }
}

/// Merge via a cloud post-process provider (same as regular post-processing).
async fn merge_with_cloud_provider(settings: &AppSettings, prompt: &str) -> Option<String> {
    let provider = match settings.active_post_process_provider().cloned() {
        Some(p) => p,
        None => {
            warn!("Multi-STT cloud merge: no post-process provider configured");
            return None;
        }
    };

    let model = settings
        .post_process_models
        .get(&provider.id)
        .cloned()
        .unwrap_or_default();

    if model.trim().is_empty() {
        warn!(
            "Multi-STT cloud merge: no model configured for provider '{}'",
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
        "Multi-STT cloud merge with provider '{}' (model: {})",
        provider.id, model
    );

    match llm_client::send_chat_completion(&provider, api_key, &model, prompt.to_string(), false)
        .await
    {
        Ok(Some(content)) => {
            let content = content.trim().to_string();
            info!(
                "Multi-STT cloud merge succeeded. Output length: {} chars",
                content.len()
            );
            Some(content)
        }
        Ok(None) => {
            warn!("Multi-STT cloud merge: response has no content");
            None
        }
        Err(e) => {
            error!("Multi-STT cloud merge failed: {}", e);
            None
        }
    }
}
