//! Multi-STT: parallel transcription with multiple models + LLM merge.
//!
//! Spawns independent transcription tasks for each selected model, runs them
//! concurrently in background threads, then optionally merges results via LLM.
//!
//! Model types supported in parallel:
//!   - Python-based (UnifiedParakeet): each spawns its own server on a unique port
//!   - transcribe-rs (Whisper, Parakeet V2/V3, Moonshine, etc.): loaded independently

use anyhow::Result;
use log::{error, info};
use std::path::Path;
use std::sync::Arc;
use tauri::AppHandle;

use crate::managers::model::{EngineType, ModelInfo, ModelManager};
use crate::settings::AppSettings;
use crate::stt::unified_parakeet::UnifiedParakeetServer;
use transcribe_cpp::{Model, ModelOptions, RunOptions};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Entry point from actions.rs: run multi-STT and return a single merged text.
/// Formats individual model outputs into a markdown-like block for LLM post-processing,
/// or falls back to the best single result if only one model succeeds.
pub fn transcribe_parallel(
    audio: Vec<f32>,
    settings: &AppSettings,
    model_manager: &Arc<ModelManager>,
    app_handle: &AppHandle,
) -> Result<String> {
    let results = run_parallel(audio, &settings.multi_stt_models, model_manager, app_handle);

    if results.is_empty() {
        return Err(anyhow::anyhow!("All multi-STT models failed"));
    }

    if results.len() == 1 {
        return Ok(results[0].1.clone());
    }

    // Build a formatted transcript block for LLM post-processing
    let transcriptions_block: String = results
        .iter()
        .enumerate()
        .map(|(i, (model_id, text))| format!("Transcription {} ({}):\n{}\n", i + 1, model_id, text))
        .collect::<Vec<_>>()
        .join("\n");

    let prompt = settings
        .multi_stt_prompt
        .replace("{transcriptions}", &transcriptions_block);

    // If post-process is enabled, append the merge prompt for the LLM.
    // The caller (actions.rs) will feed this through the existing post_process pipeline.
    if settings.post_process_enabled && !settings.post_process_provider_id.is_empty() {
        Ok(format!(
            "{}\n\n--- MULTI_STT_MERGE ---\n{}",
            transcriptions_block, prompt
        ))
    } else {
        // No LLM merge available: pick the longest transcript — the best proxy for
        // "most complete capture" when we can't judge the candidates against each
        // other — instead of blindly returning the first model's output.
        let best = results
            .iter()
            .max_by_key(|(_, text)| text.chars().count())
            .map(|(_, text)| text.clone())
            .unwrap_or_default();
        Ok(best)
    }
}

/// Run multiple STT models in parallel and return (model_id, text) pairs.
pub fn run_parallel(
    audio: Vec<f32>,
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

        let audio = audio.clone();
        let model_id = model_id.clone();
        let model_path = model_manager.models_dir().join(&model_info.filename);
        let app_handle = app_handle.clone();

        let handle = std::thread::spawn(move || {
            transcribe_single(&audio, &model_path, &model_info, &app_handle)
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
    model_path: &Path,
    model_info: &ModelInfo,
    _app_handle: &AppHandle,
) -> Result<String> {
    match model_info.engine_type {
        EngineType::UnifiedParakeet => transcribe_python(audio, model_path, model_info),
        EngineType::TranscribeCpp => transcribe_transcribe_rs(audio, model_path, |path| {
            let model =
                transcribe_cpp::Model::load_with(path, &transcribe_cpp::ModelOptions::default())?;
            let mut session = model.session()?;
            let transcript = session.run(audio, &transcribe_cpp::RunOptions::default())?;
            Ok(transcript.text)
        }),
        EngineType::Parakeet => transcribe_transcribe_rs(audio, model_path, |path| {
            use transcribe_rs::onnx::parakeet::ParakeetModel;
            use transcribe_rs::onnx::Quantization;
            let mut engine = ParakeetModel::load(path, &Quantization::Int8)?;
            let params = transcribe_rs::onnx::parakeet::ParakeetParams::default();
            let r = engine.transcribe_with(audio, &params)?;
            Ok(r.text)
        }),
        EngineType::Moonshine => transcribe_transcribe_rs(audio, model_path, |path| {
            use transcribe_rs::onnx::moonshine::{MoonshineModel, MoonshineVariant};
            use transcribe_rs::onnx::Quantization;
            use transcribe_rs::{SpeechModel, TranscribeOptions};
            let mut engine =
                MoonshineModel::load(path, MoonshineVariant::Base, &Quantization::default())?;
            let r = engine.transcribe(audio, &TranscribeOptions::default())?;
            Ok(r.text)
        }),
        EngineType::MoonshineStreaming => transcribe_transcribe_rs(audio, model_path, |path| {
            use transcribe_rs::onnx::moonshine::StreamingModel;
            use transcribe_rs::onnx::Quantization;
            use transcribe_rs::{SpeechModel, TranscribeOptions};
            let mut engine = StreamingModel::load(path, 0, &Quantization::default())?;
            let r = engine.transcribe(audio, &TranscribeOptions::default())?;
            Ok(r.text)
        }),
        EngineType::SenseVoice => transcribe_transcribe_rs(audio, model_path, |path| {
            use transcribe_rs::onnx::sense_voice::{SenseVoiceModel, SenseVoiceParams};
            use transcribe_rs::onnx::Quantization;
            let mut engine = SenseVoiceModel::load(path, &Quantization::Int8)?;
            let params = SenseVoiceParams {
                language: None,
                use_itn: Some(true),
            };
            let r = engine.transcribe_with(audio, &params)?;
            Ok(r.text)
        }),
        EngineType::GigaAM => transcribe_transcribe_rs(audio, model_path, |path| {
            use transcribe_rs::onnx::gigaam::GigaAMModel;
            use transcribe_rs::onnx::Quantization;
            use transcribe_rs::{SpeechModel, TranscribeOptions};
            let mut engine = GigaAMModel::load(path, &Quantization::Int8)?;
            let r = engine.transcribe(audio, &TranscribeOptions::default())?;
            Ok(r.text)
        }),
        EngineType::Canary => transcribe_transcribe_rs(audio, model_path, |path| {
            use transcribe_rs::onnx::canary::CanaryModel;
            use transcribe_rs::onnx::Quantization;
            use transcribe_rs::{SpeechModel, TranscribeOptions};
            let mut engine = CanaryModel::load(path, &Quantization::Int8)?;
            let r = engine.transcribe(audio, &TranscribeOptions::default())?;
            Ok(r.text)
        }),
        EngineType::Cohere => transcribe_transcribe_rs(audio, model_path, |path| {
            use transcribe_rs::onnx::cohere::CohereModel;
            use transcribe_rs::onnx::Quantization;
            use transcribe_rs::{SpeechModel, TranscribeOptions};
            let mut engine = CohereModel::load(path, &Quantization::Int8)?;
            let r = engine.transcribe(audio, &TranscribeOptions::default())?;
            Ok(r.text)
        }),
    }
}

// ---------------------------------------------------------------------------
// Backend helpers
// ---------------------------------------------------------------------------

/// Transcribe via a Python ONNX server (Unified Parakeet family).
/// Each call spawns a fresh server on a random port, transcribes, then kills it.
fn transcribe_python(audio: &[f32], model_path: &Path, model_info: &ModelInfo) -> Result<String> {
    let server = UnifiedParakeetServer::launch(&model_path.to_string_lossy())?;

    // EOU models use the streaming path for progressive results; Unified uses offline.
    let is_eou = model_info
        .hf_repo
        .as_deref()
        .unwrap_or("")
        .contains("parakeet-realtime-eou-120m");

    if is_eou {
        server.stream_start()?;
        let mut last_text = String::new();
        const CHUNK: usize = 4000; // 250ms
                                   // Skip near-silent MIDDLE chunks only — never the final chunk (the tail of
                                   // already-VAD-gated speech), which was being dropped and truncating results.
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
        // Prefer whichever is longer — the final flush can come back shorter/empty.
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
fn transcribe_transcribe_rs<F>(_audio: &[f32], model_path: &Path, f: F) -> Result<String>
where
    F: FnOnce(&Path) -> Result<String>,
{
    f(model_path)
}
