//! Batch transcription of audio files — the backend of the "Transcribe Files"
//! page (fork feature).
//!
//! A job is a list of file paths. Each file is decoded to mono 16 kHz
//! (`symphonia`: WAV / MP3 / M4A / AAC / FLAC / OGG-Vorbis), cut into segments
//! of at most `max_segment_minutes` at the quietest point near each boundary,
//! run through the primary model (and, in the Multi-STT modes, the configured
//! extra models), optionally merged / post-processed through the LLM, and
//! written next to the source or into the chosen output folder. Progress is
//! streamed to the UI as [`FileTranscriptionEvent`]s; the job runs on the
//! async runtime with every blocking step on the blocking pool so the settings
//! window stays responsive.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::{Context, Result, anyhow};
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};
use tauri_specta::Event;

use crate::audio_toolkit::audio::FrameResampler;
use crate::audio_toolkit::constants::{VAD_FRAME_MS, WHISPER_SAMPLE_RATE};
use crate::managers::audio::AudioRecordingManager;
use crate::managers::transcription::TranscriptionManager;
use crate::settings::{
    AppSettings, FileTranscriptionMode, FileTranscriptionSettings, TranscriptOutputFormat,
    get_settings,
};

/// 16 kHz as `usize`, the unit every sample-count computation below uses.
const SAMPLE_RATE: usize = WHISPER_SAMPLE_RATE as usize;

/// Extensions the decoder accepts. Everything here is covered by the
/// `symphonia` features enabled in `Cargo.toml`; Opus and WebM are not.
pub const SUPPORTED_EXTENSIONS: &[&str] =
    &["wav", "mp3", "m4a", "mp4", "aac", "flac", "ogg", "oga"];

/// When looking for a quiet point to cut a long file, search this far back
/// from the hard boundary.
const SEGMENT_SEARCH_SECS: usize = 20;
/// RMS is measured over windows of this length while searching for a cut.
const SEGMENT_QUIET_WINDOW_MS: usize = 100;

pub fn is_supported_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .is_some_and(|e| SUPPORTED_EXTENSIONS.contains(&e.as_str()))
}

/* ───────────────────────── decoding ───────────────────────── */

/// Decode any supported audio file to mono f32 at 16 kHz.
pub fn decode_audio_file(path: &Path) -> Result<Vec<f32>> {
    if !path.is_file() {
        return Err(anyhow!("File not found: {}", path.display()));
    }
    if !is_supported_audio_file(path) {
        return Err(anyhow!(
            "Unsupported audio format: .{}. Supported: {}",
            path.extension()
                .and_then(|e| e.to_str())
                .unwrap_or_default(),
            SUPPORTED_EXTENSIONS.join(", ")
        ));
    }

    let (mono, sample_rate) = match decode_with_symphonia(path) {
        Ok(decoded) => decoded,
        // Some WAV flavours (e.g. WAVE_FORMAT_EXTENSIBLE float) confuse the
        // probe; hound reads those fine.
        Err(err)
            if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("wav")) =>
        {
            debug!(
                "symphonia failed on {} ({err}); falling back to hound",
                path.display()
            );
            decode_wav_with_hound(path)?
        }
        Err(err) => return Err(err),
    };

    if mono.is_empty() {
        return Err(anyhow!("The file contains no audio samples"));
    }
    Ok(resample_to_16k(&mono, sample_rate))
}

fn decode_with_symphonia(path: &Path) -> Result<(Vec<f32>, u32)> {
    use symphonia::core::codecs::audio::AudioDecoderOptions;
    use symphonia::core::errors::Error as SymphoniaError;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::formats::probe::Hint;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;

    let file =
        std::fs::File::open(path).with_context(|| format!("Failed to open {}", path.display()))?;
    let stream = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            stream,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|e| anyhow!("Unrecognised audio container: {e}"))?;

    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.as_ref().and_then(|p| p.audio()).is_some())
        .ok_or_else(|| anyhow!("No decodable audio track found"))?;
    let track_id = track.id;
    let audio_params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .ok_or_else(|| anyhow!("No decodable audio track found"))?;
    let sample_rate = audio_params
        .sample_rate
        .ok_or_else(|| anyhow!("Audio track does not declare a sample rate"))?;
    if !(1_000..=384_000).contains(&sample_rate) {
        return Err(anyhow!("Unsupported sample rate: {sample_rate} Hz"));
    }

    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(audio_params, &AudioDecoderOptions::default())
        .map_err(|e| anyhow!("No decoder for this codec: {e}"))?;

    let mut mono: Vec<f32> = Vec::new();
    let mut interleaved_buf: Vec<f32> = Vec::new();
    let mut decode_errors = 0usize;

    loop {
        let packet = match format.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => break,
            Err(SymphoniaError::ResetRequired) => {
                // Chained streams (e.g. concatenated OGG) — keep the audio we
                // have rather than failing the whole file.
                warn!(
                    "Decoder reset required in {}; stopping early",
                    path.display()
                );
                break;
            }
            Err(e) => return Err(anyhow!("Failed to read audio packet: {e}")),
        };
        if packet.track_id != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            Err(SymphoniaError::DecodeError(e)) => {
                decode_errors += 1;
                if decode_errors <= 3 {
                    warn!("Skipping undecodable packet in {}: {e}", path.display());
                }
                continue;
            }
            Err(e) => return Err(anyhow!("Failed to decode audio: {e}")),
        };

        let spec = decoded.spec();
        let channels = spec.channels().count().max(1);
        let num_samples = decoded.samples_interleaved();
        interleaved_buf.resize(num_samples, 0.0f32);
        decoded.copy_to_slice_interleaved(&mut interleaved_buf);
        if channels == 1 {
            mono.extend_from_slice(&interleaved_buf);
        } else {
            mono.extend(
                interleaved_buf
                    .chunks_exact(channels)
                    .map(|frame| frame.iter().sum::<f32>() / channels as f32),
            );
        }
    }

    Ok((mono, sample_rate))
}

fn decode_wav_with_hound(path: &Path) -> Result<(Vec<f32>, u32)> {
    let reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    let channels = spec.channels.max(1) as usize;
    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .into_samples::<f32>()
            .collect::<Result<Vec<f32>, _>>()?,
        hound::SampleFormat::Int => {
            let max = (1i64 << (spec.bits_per_sample.max(1) - 1)) as f32;
            reader
                .into_samples::<i32>()
                .map(|s| s.map(|v| v as f32 / max))
                .collect::<Result<Vec<f32>, _>>()?
        }
    };
    let mono = if channels == 1 {
        interleaved
    } else {
        interleaved
            .chunks_exact(channels)
            .map(|frame| frame.iter().sum::<f32>() / channels as f32)
            .collect()
    };
    Ok((mono, spec.sample_rate))
}

fn resample_to_16k(samples: &[f32], sample_rate: u32) -> Vec<f32> {
    if sample_rate as usize == SAMPLE_RATE {
        return samples.to_vec();
    }
    let mut resampler = FrameResampler::new(
        sample_rate as usize,
        SAMPLE_RATE,
        std::time::Duration::from_millis(VAD_FRAME_MS),
    );
    let mut out = Vec::with_capacity(
        (samples.len() as f64 * SAMPLE_RATE as f64 / sample_rate as f64) as usize + 1024,
    );
    resampler.push(samples, |frame: &[f32]| out.extend_from_slice(frame));
    resampler.finish(|frame: &[f32]| out.extend_from_slice(frame));
    out
}

/* ───────────────────────── segmentation ───────────────────────── */

/// Split `len` samples into `(start, end)` ranges of at most `max_len`, moving
/// each cut back to the quietest `quiet_window` inside the last `search`
/// samples before the hard boundary. Segments never exceed `max_len`, so a
/// very long file still bounds memory and engine time per decode call.
pub fn segment_boundaries(
    samples: &[f32],
    max_len: usize,
    search: usize,
    quiet_window: usize,
) -> Vec<(usize, usize)> {
    let len = samples.len();
    let mut ranges = Vec::new();
    if len == 0 {
        return ranges;
    }
    let max_len = max_len.max(1);
    let quiet_window = quiet_window.max(1);
    let mut start = 0usize;
    while start < len {
        let hard_end = (start + max_len).min(len);
        if hard_end == len {
            ranges.push((start, len));
            break;
        }
        let search_from = hard_end.saturating_sub(search).max(start + 1);
        let mut best_end = hard_end;
        if hard_end > search_from + quiet_window {
            let mut best_rms = f32::INFINITY;
            let mut pos = search_from;
            while pos + quiet_window <= hard_end {
                let window = &samples[pos..pos + quiet_window];
                let rms = (window.iter().map(|s| s * s).sum::<f32>() / quiet_window as f32).sqrt();
                // `<=` prefers the latest quiet window, keeping segments long.
                if rms <= best_rms {
                    best_rms = rms;
                    best_end = pos + quiet_window / 2;
                }
                pos += quiet_window;
            }
        }
        ranges.push((start, best_end));
        start = best_end;
    }
    ranges
}

/* ───────────────────────── job state & events ───────────────────────── */

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum FileJobStatus {
    Queued,
    Decoding,
    Transcribing,
    Merging,
    PostProcessing,
    Saving,
    Done,
    Failed,
    Cancelled,
}

/// Progress of one file inside a job. The last event of a job carries
/// `batch_finished = true` (with `index == total`) so the UI can leave its
/// "running" state even if an individual event was missed.
#[derive(Serialize, Deserialize, Clone, Debug, Type, tauri_specta::Event)]
pub struct FileTranscriptionEvent {
    pub job_id: u32,
    /// 0-based position in the job (== `total` on the batch-finished event).
    pub index: u32,
    pub total: u32,
    pub path: String,
    pub status: FileJobStatus,
    pub segment: Option<u32>,
    pub segments: Option<u32>,
    pub text: Option<String>,
    pub output_path: Option<String>,
    pub error: Option<String>,
    pub audio_seconds: Option<f64>,
    pub elapsed_ms: Option<f64>,
    pub batch_finished: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, Type)]
pub struct FileTranscriptionStatus {
    pub running: bool,
    pub job_id: u32,
    pub total: u32,
    pub completed: u32,
    pub current_path: Option<String>,
}

pub struct FileTranscriptionManager {
    running: AtomicBool,
    cancel_requested: AtomicBool,
    next_job_id: AtomicU32,
    status: Mutex<FileTranscriptionStatus>,
}

impl Default for FileTranscriptionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl FileTranscriptionManager {
    pub fn new() -> Self {
        Self {
            running: AtomicBool::new(false),
            cancel_requested: AtomicBool::new(false),
            next_job_id: AtomicU32::new(1),
            status: Mutex::new(FileTranscriptionStatus::default()),
        }
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    pub fn status(&self) -> FileTranscriptionStatus {
        self.status.lock().unwrap().clone()
    }

    pub fn cancel(&self) {
        if self.is_running() {
            info!("File transcription: cancellation requested");
            self.cancel_requested.store(true, Ordering::Release);
        }
    }

    fn cancelled(&self) -> bool {
        self.cancel_requested.load(Ordering::Acquire)
    }

    /// Queue `paths` and start the job. Returns the job id the events carry.
    pub fn start(self: &Arc<Self>, app: &AppHandle, paths: Vec<String>) -> Result<u32, String> {
        let paths: Vec<PathBuf> = paths
            .into_iter()
            .map(PathBuf::from)
            .filter(|p| is_supported_audio_file(p))
            .collect();
        if paths.is_empty() {
            return Err("No supported audio files were selected".to_string());
        }
        if self
            .running
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err("A file transcription job is already running".to_string());
        }
        if app
            .try_state::<Arc<AudioRecordingManager>>()
            .is_some_and(|rm| rm.is_recording())
        {
            self.running.store(false, Ordering::Release);
            return Err("A recording is in progress; wait for it to finish".to_string());
        }
        if app
            .try_state::<Arc<crate::live_mode::LiveModeManager>>()
            .is_some_and(|live| live.is_active())
        {
            self.running.store(false, Ordering::Release);
            return Err("Stop Live Mode before transcribing files".to_string());
        }

        let settings = get_settings(app);
        if settings.selected_model.trim().is_empty() {
            self.running.store(false, Ordering::Release);
            return Err("No transcription model is selected".to_string());
        }

        self.cancel_requested.store(false, Ordering::Release);
        let job_id = self.next_job_id.fetch_add(1, Ordering::Relaxed);
        *self.status.lock().unwrap() = FileTranscriptionStatus {
            running: true,
            job_id,
            total: paths.len() as u32,
            completed: 0,
            current_path: None,
        };

        let manager = Arc::clone(self);
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            manager.run_job(app, job_id, paths, settings).await;
        });
        Ok(job_id)
    }

    async fn run_job(
        self: Arc<Self>,
        app: AppHandle,
        job_id: u32,
        paths: Vec<PathBuf>,
        settings: AppSettings,
    ) {
        let total = paths.len() as u32;
        let options = settings.file_transcription.clone().normalized();
        let tm = app.state::<Arc<TranscriptionManager>>().inner().clone();
        info!(
            "File transcription job {job_id}: {total} file(s), mode {:?}, format {:?}",
            options.mode, options.output_format
        );

        let emit = |event: FileTranscriptionEvent| {
            let _ = event.emit(&app);
        };

        // Load the primary (and, for Multi-STT, the extra) engines once up front.
        let primary_model = settings.selected_model.clone();
        let preload_error = self
            .preload_models(&tm, &settings, &options.mode, &primary_model)
            .await
            .err();

        for (index, path) in paths.iter().enumerate() {
            let index = index as u32;
            let path_str = path.to_string_lossy().to_string();
            {
                let mut status = self.status.lock().unwrap();
                status.current_path = Some(path_str.clone());
            }
            let base = |status: FileJobStatus| FileTranscriptionEvent {
                job_id,
                index,
                total,
                path: path_str.clone(),
                status,
                segment: None,
                segments: None,
                text: None,
                output_path: None,
                error: None,
                audio_seconds: None,
                elapsed_ms: None,
                batch_finished: false,
            };

            if self.cancelled() {
                emit(base(FileJobStatus::Cancelled));
                continue;
            }
            if let Some(err) = &preload_error {
                emit(FileTranscriptionEvent {
                    error: Some(err.clone()),
                    ..base(FileJobStatus::Failed)
                });
                continue;
            }

            let started = Instant::now();
            match self
                .transcribe_one(
                    &app,
                    &tm,
                    &settings,
                    &options,
                    &primary_model,
                    path,
                    |status, seg, segs| {
                        emit(FileTranscriptionEvent {
                            segment: seg,
                            segments: segs,
                            ..base(status)
                        })
                    },
                )
                .await
            {
                Ok(outcome) => {
                    self.status.lock().unwrap().completed += 1;
                    emit(FileTranscriptionEvent {
                        text: Some(outcome.text),
                        output_path: outcome.output_path.map(|p| p.to_string_lossy().to_string()),
                        audio_seconds: Some(outcome.audio_seconds),
                        elapsed_ms: Some(started.elapsed().as_secs_f64() * 1000.0),
                        ..base(FileJobStatus::Done)
                    });
                }
                Err(JobError::Cancelled) => {
                    emit(base(FileJobStatus::Cancelled));
                }
                Err(JobError::Failed(err)) => {
                    error!("File transcription failed for {}: {err}", path.display());
                    emit(FileTranscriptionEvent {
                        error: Some(err),
                        elapsed_ms: Some(started.elapsed().as_secs_f64() * 1000.0),
                        ..base(FileJobStatus::Failed)
                    });
                }
            }
        }

        tm.maybe_unload_immediately("file transcription");
        {
            let mut status = self.status.lock().unwrap();
            status.running = false;
            status.current_path = None;
        }
        self.running.store(false, Ordering::Release);
        emit(FileTranscriptionEvent {
            job_id,
            index: total,
            total,
            path: String::new(),
            status: if self.cancelled() {
                FileJobStatus::Cancelled
            } else {
                FileJobStatus::Done
            },
            segment: None,
            segments: None,
            text: None,
            output_path: None,
            error: None,
            audio_seconds: None,
            elapsed_ms: None,
            batch_finished: true,
        });
        info!("File transcription job {job_id} finished");
    }

    async fn preload_models(
        &self,
        tm: &Arc<TranscriptionManager>,
        settings: &AppSettings,
        mode: &FileTranscriptionMode,
        primary_model: &str,
    ) -> Result<(), String> {
        if !tm.is_model_loaded() || tm.get_current_model().as_deref() != Some(primary_model) {
            let tm = Arc::clone(tm);
            let model = primary_model.to_string();
            tauri::async_runtime::spawn_blocking(move || tm.load_model(&model))
                .await
                .map_err(|e| format!("Model load task panicked: {e}"))?
                .map_err(|e| format!("Failed to load model '{primary_model}': {e}"))?;
        }
        if uses_extra_models(mode) {
            let mut handles = Vec::new();
            for model_id in extra_model_ids(settings) {
                let tm = Arc::clone(tm);
                handles.push((
                    model_id.clone(),
                    tauri::async_runtime::spawn_blocking(move || tm.load_extra_model(&model_id)),
                ));
            }
            for (model_id, handle) in handles {
                match handle.await {
                    Ok(Ok(_)) => {}
                    Ok(Err(e)) => warn!("Extra model '{model_id}' failed to load: {e}"),
                    Err(e) => warn!("Extra model '{model_id}' load task panicked: {e}"),
                }
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn transcribe_one(
        &self,
        app: &AppHandle,
        tm: &Arc<TranscriptionManager>,
        settings: &AppSettings,
        options: &FileTranscriptionSettings,
        primary_model: &str,
        path: &Path,
        progress: impl Fn(FileJobStatus, Option<u32>, Option<u32>),
    ) -> Result<FileOutcome, JobError> {
        progress(FileJobStatus::Decoding, None, None);
        let decode_path = path.to_path_buf();
        let samples = tauri::async_runtime::spawn_blocking(move || decode_audio_file(&decode_path))
            .await
            .map_err(|e| JobError::Failed(format!("Decode task panicked: {e}")))?
            .map_err(|e| JobError::Failed(e.to_string()))?;
        let audio_seconds = samples.len() as f64 / SAMPLE_RATE as f64;
        if self.cancelled() {
            return Err(JobError::Cancelled);
        }

        let max_len = options.max_segment_minutes as usize * 60 * SAMPLE_RATE;
        let ranges = segment_boundaries(
            &samples,
            max_len,
            SEGMENT_SEARCH_SECS * SAMPLE_RATE,
            SEGMENT_QUIET_WINDOW_MS * SAMPLE_RATE / 1000,
        );
        let segment_count = ranges.len() as u32;
        let samples = Arc::new(samples);

        let extra_ids: Vec<String> = if uses_extra_models(&options.mode) {
            extra_model_ids(settings)
                .into_iter()
                .filter(|id| tm.is_extra_model_loaded(id))
                .collect()
        } else {
            Vec::new()
        };

        let mut primary_parts: Vec<String> = Vec::with_capacity(ranges.len());
        let mut extra_parts: Vec<Vec<String>> =
            vec![Vec::with_capacity(ranges.len()); extra_ids.len()];

        for (seg_index, (start, end)) in ranges.iter().enumerate() {
            if self.cancelled() {
                return Err(JobError::Cancelled);
            }
            progress(
                FileJobStatus::Transcribing,
                Some(seg_index as u32 + 1),
                Some(segment_count),
            );

            // `transcribe()` unloads the engine afterwards when the unload
            // timeout is "Immediately"; reload so every segment has a model.
            if !tm.is_model_loaded() {
                let tm2 = Arc::clone(tm);
                let model = primary_model.to_string();
                tauri::async_runtime::spawn_blocking(move || tm2.load_model(&model))
                    .await
                    .map_err(|e| JobError::Failed(format!("Model load task panicked: {e}")))?
                    .map_err(|e| JobError::Failed(format!("Failed to reload model: {e}")))?;
            }

            let (start, end) = (*start, *end);
            let primary = {
                let tm = Arc::clone(tm);
                let samples = Arc::clone(&samples);
                tauri::async_runtime::spawn_blocking(move || {
                    tm.transcribe(samples[start..end].to_vec())
                })
            };
            let extras: Vec<_> = extra_ids
                .iter()
                .map(|model_id| {
                    let tm = Arc::clone(tm);
                    let samples = Arc::clone(&samples);
                    let model_id = model_id.clone();
                    tauri::async_runtime::spawn_blocking(move || {
                        tm.transcribe_with_extra(&model_id, samples[start..end].to_vec())
                    })
                })
                .collect();

            let primary_text = primary
                .await
                .map_err(|e| JobError::Failed(format!("Transcription task panicked: {e}")))?
                .map_err(|e| JobError::Failed(format!("Transcription failed: {e}")))?;
            primary_parts.push(primary_text);
            for (slot, handle) in extras.into_iter().enumerate() {
                match handle.await {
                    Ok(Ok(text)) => extra_parts[slot].push(text),
                    Ok(Err(e)) => {
                        warn!("Extra model '{}' failed on a segment: {e}", extra_ids[slot]);
                        extra_parts[slot].push(String::new());
                    }
                    Err(e) => {
                        warn!("Extra model '{}' task panicked: {e}", extra_ids[slot]);
                        extra_parts[slot].push(String::new());
                    }
                }
            }
        }

        let primary_text = join_segments(&primary_parts);
        let mut text = primary_text.clone();

        if uses_extra_models(&options.mode) {
            let outputs: Vec<String> = extra_parts
                .iter()
                .map(|parts| join_segments(parts))
                .collect();
            let output = |slot: usize| outputs.get(slot).map(String::as_str).unwrap_or("");
            if self.cancelled() {
                return Err(JobError::Cancelled);
            }
            progress(FileJobStatus::Merging, None, None);
            let merged = crate::actions::multi_stt_merge_transcriptions(
                settings,
                &primary_text,
                output(0),
                output(1),
                output(2),
            )
            .await;
            text = match merged {
                Some(merged) => merged,
                None => {
                    let mut combined = primary_text.clone();
                    for extra in outputs.iter().filter(|o| !o.trim().is_empty()) {
                        if !combined.is_empty() {
                            combined.push('\n');
                        }
                        combined.push_str(extra);
                    }
                    combined
                }
            };
        }

        if self.cancelled() {
            return Err(JobError::Cancelled);
        }
        let post_process = matches!(
            options.mode,
            FileTranscriptionMode::PostProcess | FileTranscriptionMode::MultiSttPostProcess
        );
        if post_process {
            progress(FileJobStatus::PostProcessing, None, None);
        }
        let processed =
            crate::actions::process_transcription_output(app, &text, post_process).await;
        text = processed.final_text;

        if self.cancelled() {
            return Err(JobError::Cancelled);
        }
        progress(FileJobStatus::Saving, None, None);
        let output_path = write_transcript(path, &text, options, settings)
            .map_err(|e| JobError::Failed(format!("Failed to save transcript: {e}")))?;
        info!(
            "Transcribed {} ({:.1}s audio, {} segment(s)) → {}",
            path.display(),
            audio_seconds,
            segment_count,
            output_path.display()
        );

        Ok(FileOutcome {
            text,
            output_path: Some(output_path),
            audio_seconds,
        })
    }
}

struct FileOutcome {
    text: String,
    output_path: Option<PathBuf>,
    audio_seconds: f64,
}

enum JobError {
    Cancelled,
    Failed(String),
}

fn uses_extra_models(mode: &FileTranscriptionMode) -> bool {
    matches!(
        mode,
        FileTranscriptionMode::MultiStt | FileTranscriptionMode::MultiSttPostProcess
    )
}

fn extra_model_ids(settings: &AppSettings) -> Vec<String> {
    [
        &settings.multi_stt_model_2,
        &settings.multi_stt_model_3,
        &settings.multi_stt_model_4,
    ]
    .into_iter()
    .flatten()
    .filter(|id| !id.trim().is_empty())
    .cloned()
    .collect()
}

/// Join per-segment transcripts with single spaces, skipping empty segments.
fn join_segments(parts: &[String]) -> String {
    let mut out = String::new();
    for part in parts {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(part);
    }
    out
}

/* ───────────────────────── output ───────────────────────── */

fn mode_label(mode: FileTranscriptionMode) -> &'static str {
    match mode {
        FileTranscriptionMode::Simple => "transcription",
        FileTranscriptionMode::PostProcess => "transcription + post-processing",
        FileTranscriptionMode::MultiStt => "Multi-STT",
        FileTranscriptionMode::MultiSttPostProcess => "Multi-STT + post-processing",
    }
}

/// Render the saved file: plain text for `.txt`, a small header + body for `.md`.
pub fn render_transcript(
    source: &Path,
    text: &str,
    format: TranscriptOutputFormat,
    mode: FileTranscriptionMode,
    model_id: &str,
) -> String {
    match format {
        TranscriptOutputFormat::Txt => format!("{}\n", text.trim_end()),
        TranscriptOutputFormat::Md => {
            let name = source
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "transcript".to_string());
            format!(
                "# {name}\n\n_Handy {} · model `{model_id}` · {}_\n\n{}\n",
                mode_label(mode),
                chrono::Local::now().format("%Y-%m-%d %H:%M"),
                text.trim_end()
            )
        }
    }
}

fn write_transcript(
    source: &Path,
    text: &str,
    options: &FileTranscriptionSettings,
    settings: &AppSettings,
) -> Result<PathBuf> {
    let dir = match &options.output_dir {
        Some(dir) => {
            let dir = PathBuf::from(dir);
            std::fs::create_dir_all(&dir)
                .with_context(|| format!("Cannot create output folder {}", dir.display()))?;
            dir
        }
        None => source
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| anyhow!("Source file has no parent folder"))?,
    };
    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("transcript");
    let preferred = dir.join(format!("{stem}.{}", options.output_format.extension()));
    let body = render_transcript(
        source,
        text,
        options.output_format,
        options.mode,
        &settings.selected_model,
    );
    if options.overwrite_existing {
        std::fs::write(&preferred, body.as_bytes())
            .with_context(|| format!("Cannot write {}", preferred.display()))?;
        return Ok(preferred);
    }
    write_without_overwrite(&preferred, body.as_bytes())
}

/// Create `preferred`, or `<stem>-2.<ext>`, `<stem>-3.<ext>`, … if it exists.
/// `create_new` makes the existence check and the create one atomic step.
pub fn write_without_overwrite(preferred: &Path, contents: &[u8]) -> Result<PathBuf> {
    let parent = preferred
        .parent()
        .ok_or_else(|| anyhow!("Output path has no parent folder"))?;
    let stem = preferred
        .file_stem()
        .ok_or_else(|| anyhow!("Output path has no file name"))?;
    let extension = preferred.extension();
    for index in 1..=10_000u32 {
        let candidate = if index == 1 {
            preferred.to_path_buf()
        } else {
            let mut name = stem.to_os_string();
            name.push(format!("-{index}"));
            if let Some(ext) = extension {
                name.push(".");
                name.push(ext);
            }
            parent.join(name)
        };
        let mut file = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(anyhow!("Cannot create {}: {e}", candidate.display())),
        };
        if let Err(e) = file.write_all(contents).and_then(|_| file.flush()) {
            drop(file);
            let _ = std::fs::remove_file(&candidate);
            return Err(anyhow!("Cannot write {}: {e}", candidate.display()));
        }
        return Ok(candidate);
    }
    Err(anyhow!(
        "Could not find a free file name for {}",
        preferred.display()
    ))
}

/// Recursively (optionally) list the supported audio files below `folder`,
/// sorted by path. Hidden entries (dot-prefixed) are skipped.
pub fn list_audio_files(folder: &Path, include_subfolders: bool) -> Result<Vec<PathBuf>> {
    if !folder.is_dir() {
        return Err(anyhow!("Not a folder: {}", folder.display()));
    }
    let mut out = Vec::new();
    let mut stack = vec![folder.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir)
            .with_context(|| format!("Cannot read folder {}", dir.display()))?;
        for entry in entries.flatten() {
            let path = entry.path();
            let hidden = path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with('.'));
            if hidden {
                continue;
            }
            if path.is_dir() {
                if include_subfolders {
                    stack.push(path);
                }
            } else if is_supported_audio_file(&path) {
                out.push(path);
            }
        }
    }
    out.sort();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_input_is_one_segment() {
        let samples = vec![0.5f32; 1000];
        assert_eq!(segment_boundaries(&samples, 4000, 500, 10), vec![(0, 1000)]);
    }

    #[test]
    fn empty_input_has_no_segments() {
        assert!(segment_boundaries(&[], 4000, 500, 10).is_empty());
    }

    #[test]
    fn cuts_land_on_the_quiet_window_and_never_exceed_max() {
        // Loud everywhere except a silent gap at 700..800 inside the search
        // window (600..1000) before the first hard boundary at 1000.
        let mut samples = vec![0.8f32; 2500];
        for s in &mut samples[700..800] {
            *s = 0.0;
        }
        let ranges = segment_boundaries(&samples, 1000, 400, 20);
        assert_eq!(ranges[0].0, 0);
        assert!(
            (700..=800).contains(&ranges[0].1),
            "first cut {} should fall in the silent gap",
            ranges[0].1
        );
        let mut expected_start = 0;
        for (start, end) in &ranges {
            assert_eq!(*start, expected_start);
            assert!(end - start <= 1000);
            expected_start = *end;
        }
        assert_eq!(expected_start, samples.len());
    }

    #[test]
    fn join_skips_empty_parts() {
        assert_eq!(
            join_segments(&["a ".into(), String::new(), " b".into()]),
            "a b"
        );
    }

    #[test]
    fn supported_extensions_are_case_insensitive() {
        assert!(is_supported_audio_file(Path::new("x/Meeting.MP3")));
        assert!(!is_supported_audio_file(Path::new("x/notes.txt")));
    }

    #[test]
    fn write_without_overwrite_appends_suffix() {
        let dir = std::env::temp_dir().join(format!("handy-ft-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let preferred = dir.join("a.txt");
        let first = write_without_overwrite(&preferred, b"1").unwrap();
        let second = write_without_overwrite(&preferred, b"2").unwrap();
        assert_eq!(first, preferred);
        assert_eq!(second.file_name().unwrap(), "a-2.txt");
        assert_eq!(std::fs::read_to_string(&first).unwrap(), "1");
        assert_eq!(std::fs::read_to_string(&second).unwrap(), "2");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn md_render_has_header_and_body() {
        let out = render_transcript(
            Path::new("talk.mp3"),
            "hello",
            TranscriptOutputFormat::Md,
            FileTranscriptionMode::Simple,
            "model-x",
        );
        assert!(out.starts_with("# talk.mp3\n"));
        assert!(out.ends_with("hello\n"));
        assert_eq!(
            render_transcript(
                Path::new("talk.mp3"),
                "hello  ",
                TranscriptOutputFormat::Txt,
                FileTranscriptionMode::Simple,
                "m"
            ),
            "hello\n"
        );
    }
}
