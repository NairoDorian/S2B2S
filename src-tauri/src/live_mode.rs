//! Live Mode (fork feature): keep the microphone open, save the raw signal in
//! chunked WAV files and mirror the live transcription into a text file.
//!
//! One session = one folder `<output dir>/live_<timestamp>/` holding
//! `transcript.txt` (or `.md`) and `chunk_0001.wav`, `chunk_0002.wav`, …
//! Each chunk is a normal recording: `AudioRecordingManager` captures (the
//! raw tap on exactly when `save_audio` is, whatever `save_raw_audio` says)
//! while `TranscriptionManager::start_stream` runs the model's native live
//! stream. When a chunk reaches its target length — on
//! the first pause after 80 % of it, if `prefer_silence_boundary` is on — the
//! stream is finalized, the recording stopped, the chunk WAV written on a
//! blocking thread, the finalized text committed to the transcript, and the
//! next chunk starts immediately. The gap between chunks is the stream
//! finalize plus the recorder restart (plus a batch decode when the stream
//! produced no text); the WAV is written off-thread so it adds nothing.
//!
//! The transcript file has two regions: the committed prefix (finalized
//! chunks) and the live tail, which is rewritten on every stream update —
//! character by character, or word by word (`LiveTranscriptGranularity`).
//! The UI mirrors the same two regions through [`LiveModeTranscriptEvent`].

use std::fs::{File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};
use tauri_specta::Event;

use crate::audio_toolkit::{VadPolicy, save_raw_wav_file, save_wav_file};
use crate::managers::audio::{AudioRecordingManager, StopRecordingResult};
use crate::managers::statistics::{StatisticsManager, StatisticsRunStatus};
use crate::managers::transcription::{StreamFinalization, TranscriptionManager};
use crate::settings::{
    LiveModeSettings, LiveTranscriptGranularity, TranscriptOutputFormat, get_settings,
};

/// Binding id the live session records under (shows up in logs / statistics).
pub const LIVE_MODE_BINDING_ID: &str = "live_mode";
/// Sub-folder of the app data dir used when no output folder is configured.
pub const DEFAULT_OUTPUT_SUBDIR: &str = "live_mode";
/// Poll interval of the chunk loop.
const TICK: Duration = Duration::from_millis(100);
/// Status heartbeat interval while listening.
const HEARTBEAT: Duration = Duration::from_millis(1000);
/// A pause this long counts as a chunk boundary once the soft target is reached.
const SILENCE_BOUNDARY: Duration = Duration::from_millis(1200);
/// Minimum interval between two rewrites of the live tail on disk.
const LIVE_WRITE_INTERVAL: Duration = Duration::from_millis(60);

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum LiveModePhase {
    #[default]
    Idle,
    Starting,
    Listening,
    Rotating,
    Stopping,
    Error,
}

#[derive(Serialize, Deserialize, Clone, Debug, Type)]
pub struct LiveChunkInfo {
    pub index: u32,
    pub path: Option<String>,
    pub duration_ms: f64,
    pub bytes: f64,
    pub text_chars: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, Type)]
pub struct LiveModeStatus {
    pub phase: LiveModePhase,
    pub session_dir: Option<String>,
    pub transcript_path: Option<String>,
    pub model_id: Option<String>,
    /// Number of the chunk currently being recorded (1-based).
    pub chunk_index: u32,
    pub chunks: Vec<LiveChunkInfo>,
    pub started_at_ms: Option<f64>,
    pub elapsed_ms: f64,
    pub current_chunk_ms: f64,
    /// Speech measured by the VAD in the current chunk.
    pub current_chunk_speech_ms: f64,
    /// Bytes of committed transcript text.
    pub transcript_bytes: f64,
    pub error: Option<String>,
}

/// Emitted whenever the session status changes and roughly once a second
/// while listening.
#[derive(Serialize, Deserialize, Clone, Debug, Type, tauri_specta::Event)]
pub struct LiveModeStateEvent {
    pub status: LiveModeStatus,
}

/// Incremental transcript update. `reset` replaces the UI's committed text
/// with `stable_appended`; otherwise `stable_appended` is appended to it.
/// `live` always replaces the volatile tail.
#[derive(Serialize, Deserialize, Clone, Debug, Type, tauri_specta::Event)]
pub struct LiveModeTranscriptEvent {
    pub reset: bool,
    pub stable_appended: String,
    pub live: String,
}

/// A past (or current) session folder, for the page's session list.
#[derive(Serialize, Deserialize, Clone, Debug, Type)]
pub struct LiveSessionInfo {
    pub dir: String,
    pub name: String,
    pub transcript_path: Option<String>,
    pub transcript_bytes: f64,
    pub chunk_count: u32,
    pub modified_ms: f64,
}

/* ───────────────────────── transcript writer ───────────────────────── */

/// Owns the transcript file: an append-only committed prefix plus a live tail
/// that is rewritten in place.
struct TranscriptWriter {
    file: File,
    stable_len: u64,
    live_len: u64,
}

impl TranscriptWriter {
    fn create(path: &Path, header: &str) -> Result<Self> {
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)
            .with_context(|| format!("Cannot create {}", path.display()))?;
        file.write_all(header.as_bytes())?;
        file.flush()?;
        Ok(Self {
            file,
            stable_len: header.len() as u64,
            live_len: 0,
        })
    }

    fn stable_len(&self) -> u64 {
        self.stable_len
    }

    /// Replace the live tail with `live`.
    fn set_live(&mut self, live: &str) -> Result<()> {
        self.file.seek(SeekFrom::Start(self.stable_len))?;
        self.file.write_all(live.as_bytes())?;
        let new_len = self.stable_len + live.len() as u64;
        if new_len < self.stable_len + self.live_len {
            self.file.set_len(new_len)?;
        }
        self.live_len = live.len() as u64;
        self.file.flush()?;
        Ok(())
    }

    /// Append `text` to the committed prefix, dropping any live tail.
    fn commit(&mut self, text: &str) -> Result<()> {
        self.file.seek(SeekFrom::Start(self.stable_len))?;
        self.file.write_all(text.as_bytes())?;
        self.stable_len += text.len() as u64;
        self.file.set_len(self.stable_len)?;
        self.live_len = 0;
        self.file.flush()?;
        Ok(())
    }
}

/// Text that should be on disk for the current live state under `granularity`.
pub fn live_tail_text(
    committed: &str,
    tentative: &str,
    granularity: LiveTranscriptGranularity,
) -> String {
    match granularity {
        // Same join as transcribe-cpp's `StreamText::display()`: the tentative
        // tail continues the committed text byte for byte.
        LiveTranscriptGranularity::Character => format!("{committed}{tentative}"),
        LiveTranscriptGranularity::Word => {
            // Only completed words: with more text still tentative, the last
            // committed token may still be extended, so stop at the last
            // whitespace boundary of the committed prefix.
            if tentative.trim().is_empty() {
                return committed.trim_end().to_string();
            }
            match committed.rfind(char::is_whitespace) {
                Some(pos) => committed[..pos].trim_end().to_string(),
                None => String::new(),
            }
        }
    }
}

/* ───────────────────────── manager ───────────────────────── */

pub struct LiveModeManager {
    app: AppHandle,
    active: Arc<AtomicBool>,
    stop_requested: Arc<AtomicBool>,
    status: Arc<Mutex<LiveModeStatus>>,
    worker: Mutex<Option<thread::JoinHandle<()>>>,
}

impl LiveModeManager {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            active: Arc::new(AtomicBool::new(false)),
            stop_requested: Arc::new(AtomicBool::new(false)),
            status: Arc::new(Mutex::new(LiveModeStatus::default())),
            worker: Mutex::new(None),
        }
    }

    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }

    pub fn status(&self) -> LiveModeStatus {
        self.status.lock().unwrap().clone()
    }

    /// Resolve the base output folder from settings (or the app-data default).
    pub fn output_root(app: &AppHandle, settings: &LiveModeSettings) -> Result<PathBuf> {
        match &settings.output_dir {
            Some(dir) => Ok(PathBuf::from(dir)),
            None => Ok(crate::portable::app_data_dir(app)
                .map_err(|e| anyhow!("Cannot resolve app data dir: {e}"))?
                .join(DEFAULT_OUTPUT_SUBDIR)),
        }
    }

    pub fn start(self: &Arc<Self>) -> Result<(), String> {
        if self
            .active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err("Live Mode is already running".to_string());
        }
        // Reap a finished worker so a stale handle never blocks a new session.
        if let Some(handle) = self.worker.lock().unwrap().take() {
            let _ = handle.join();
        }

        let app = &self.app;
        let settings = get_settings(app);
        let live = settings.live_mode.clone().normalized();
        let model_id = settings.selected_model.clone();

        let fail = |msg: String| -> Result<(), String> {
            self.active.store(false, Ordering::Release);
            let mut status = self.status.lock().unwrap();
            status.phase = LiveModePhase::Error;
            status.error = Some(msg.clone());
            let _ = LiveModeStateEvent {
                status: status.clone(),
            }
            .emit(&self.app);
            Err(msg)
        };

        if model_id.trim().is_empty() {
            return fail("No transcription model is selected".to_string());
        }
        let model_manager = app.state::<Arc<crate::managers::model::ModelManager>>();
        match model_manager.get_model_info(&model_id) {
            Some(info) if !info.supports_streaming => {
                return fail(format!(
                    "'{}' does not support live streaming; Live Mode needs a streaming-capable model",
                    info.name
                ));
            }
            Some(_) => {}
            None => return fail(format!("Model '{model_id}' is not available")),
        }
        let rm = app.state::<Arc<AudioRecordingManager>>();
        if rm.is_recording() {
            return fail("A recording is in progress; wait for it to finish".to_string());
        }
        if app
            .try_state::<Arc<crate::file_transcription::FileTranscriptionManager>>()
            .is_some_and(|ft| ft.is_running())
        {
            return fail("Wait for the file transcription job to finish".to_string());
        }

        let root = match Self::output_root(app, &live) {
            Ok(root) => root,
            Err(e) => return fail(e.to_string()),
        };
        let session_dir = root.join(format!(
            "live_{}",
            chrono::Local::now().format("%Y-%m-%d_%H-%M-%S")
        ));
        if let Err(e) = std::fs::create_dir_all(&session_dir) {
            return fail(format!(
                "Cannot create session folder {}: {e}",
                session_dir.display()
            ));
        }

        self.stop_requested.store(false, Ordering::Release);
        {
            let mut status = self.status.lock().unwrap();
            *status = LiveModeStatus {
                phase: LiveModePhase::Starting,
                session_dir: Some(session_dir.to_string_lossy().to_string()),
                transcript_path: None,
                model_id: Some(model_id.clone()),
                chunk_index: 0,
                chunks: Vec::new(),
                started_at_ms: Some(chrono::Utc::now().timestamp_millis() as f64),
                elapsed_ms: 0.0,
                current_chunk_ms: 0.0,
                current_chunk_speech_ms: 0.0,
                transcript_bytes: 0.0,
                error: None,
            };
            let _ = LiveModeStateEvent {
                status: status.clone(),
            }
            .emit(app);
        }

        let manager = Arc::clone(self);
        let spawned = thread::Builder::new()
            .name("live-mode".into())
            .spawn(move || {
                let outcome = manager.run_session(live, model_id, session_dir);
                manager.finish_session(outcome.err());
            });
        // Through `fail` so a spawn failure clears `active` rather than leaving
        // Live Mode "running" with no worker.
        let handle = match spawned {
            Ok(handle) => handle,
            Err(e) => return fail(format!("Failed to start Live Mode thread: {e}")),
        };
        *self.worker.lock().unwrap() = Some(handle);
        Ok(())
    }

    pub fn stop(&self) -> Result<(), String> {
        if !self.is_active() {
            return Err("Live Mode is not running".to_string());
        }
        info!("Live Mode: stop requested");
        self.stop_requested.store(true, Ordering::Release);
        self.update_status(|s| s.phase = LiveModePhase::Stopping);
        Ok(())
    }

    fn update_status(&self, f: impl FnOnce(&mut LiveModeStatus)) {
        let snapshot = {
            let mut status = self.status.lock().unwrap();
            f(&mut status);
            status.clone()
        };
        let _ = LiveModeStateEvent { status: snapshot }.emit(&self.app);
    }

    fn finish_session(&self, error: Option<anyhow::Error>) {
        let tm = self.app.state::<Arc<TranscriptionManager>>();
        tm.set_stream_text_sink(None, false);
        tm.maybe_unload_immediately("live mode");
        self.active.store(false, Ordering::Release);
        self.update_status(|s| match &error {
            Some(err) => {
                error!("Live Mode ended with an error: {err}");
                s.phase = LiveModePhase::Error;
                s.error = Some(err.to_string());
            }
            None => {
                s.phase = LiveModePhase::Idle;
            }
        });
        info!("Live Mode: session finished");
    }

    /// The whole session, on the dedicated worker thread.
    fn run_session(
        &self,
        live: LiveModeSettings,
        model_id: String,
        session_dir: PathBuf,
    ) -> Result<()> {
        let app = &self.app;
        let tm = app.state::<Arc<TranscriptionManager>>().inner().clone();
        let rm = app.state::<Arc<AudioRecordingManager>>().inner().clone();
        let sm = app.state::<Arc<StatisticsManager>>().inner().clone();
        let settings = get_settings(app);

        if !tm.is_model_loaded() || tm.get_current_model().as_deref() != Some(model_id.as_str()) {
            info!("Live Mode: loading model '{model_id}'");
            tm.load_model(&model_id)
                .map_err(|e| anyhow!("Failed to load model '{model_id}': {e}"))?;
        }
        if self.stop_requested.load(Ordering::Acquire) {
            return Ok(());
        }

        let transcript_path =
            session_dir.join(format!("transcript.{}", live.transcript_format.extension()));
        let header = match live.transcript_format {
            TranscriptOutputFormat::Txt => String::new(),
            TranscriptOutputFormat::Md => format!(
                "# Live transcript {}\n\n_{} Live Mode · model `{model_id}`_\n\n",
                chrono::Local::now().format("%Y-%m-%d %H:%M"),
                crate::app_identity::NAME
            ),
        };
        let writer = Arc::new(Mutex::new(TranscriptWriter::create(
            &transcript_path,
            &header,
        )?));
        let _ = LiveModeTranscriptEvent {
            reset: true,
            stable_appended: header.clone(),
            live: String::new(),
        }
        .emit(app);
        self.update_status(|s| {
            s.transcript_path = Some(transcript_path.to_string_lossy().to_string());
            s.transcript_bytes = header.len() as f64;
        });

        // Mirror every live update into the file tail (throttled) and the UI.
        {
            let writer = Arc::clone(&writer);
            let app = app.clone();
            let granularity = live.granularity;
            let last_write = Mutex::new(Instant::now() - LIVE_WRITE_INTERVAL);
            tm.set_stream_text_sink(
                // Live Mode is one model and one transcript; a second live stream
                // is the experimental Multi Streaming STT mode's, never this
                // session's, so the slot is ignored rather than branched on.
                Some(Arc::new(
                    move |_slot: u8, committed: &str, tentative: &str, _, _| {
                        let tail = live_tail_text(committed, tentative, granularity);
                        let _ = LiveModeTranscriptEvent {
                            reset: false,
                            stable_appended: String::new(),
                            live: tail.clone(),
                        }
                        .emit(&app);
                        let mut last = last_write.lock().unwrap();
                        if last.elapsed() < LIVE_WRITE_INTERVAL {
                            return;
                        }
                        *last = Instant::now();
                        if let Err(e) = writer.lock().unwrap().set_live(&tail) {
                            warn!("Live Mode: failed to write live text: {e}");
                        }
                    },
                )),
                false,
            );
        }

        let vad_policy = if settings.vad_enabled {
            VadPolicy::Streaming
        } else {
            VadPolicy::Disabled
        };
        let chunk_target = Duration::from_secs(u64::from(live.chunk_minutes) * 60);
        let soft_target = chunk_target.mul_f64(0.8);
        let separator = match live.transcript_format {
            TranscriptOutputFormat::Txt => "\n",
            TranscriptOutputFormat::Md => "\n\n",
        };

        let mut chunk_index: u32 = 0;
        while !self.stop_requested.load(Ordering::Acquire) {
            // `finalize_stream()` and `transcribe()` unload the engine when the
            // unload timeout is "Immediately"; reload so every chunk has a model.
            if !tm.is_model_loaded() {
                info!("Live Mode: reloading model '{model_id}'");
                tm.load_model(&model_id)
                    .map_err(|e| anyhow!("Failed to reload model '{model_id}': {e}"))?;
            }
            chunk_index += 1;
            let chunk_started = Instant::now();
            self.update_status(|s| {
                s.phase = LiveModePhase::Listening;
                s.chunk_index = chunk_index;
                s.current_chunk_ms = 0.0;
                s.current_chunk_speech_ms = 0.0;
            });

            let cancel_generation = rm.cancel_generation();
            rm.try_start_recording_with_raw(
                LIVE_MODE_BINDING_ID,
                vad_policy,
                Some(live.save_audio),
            )
            .map_err(|e| anyhow!("Failed to start recording: {e}"))?;
            let statistics = sm.begin_normal_run(LIVE_MODE_BINDING_ID);
            tm.start_stream(false, statistics.clone());

            // ── listen until rotation or stop ──
            let mut last_heartbeat = Instant::now();
            let mut last_speech_ms = rm.last_speech_ms();
            let mut last_speech_change = Instant::now();
            loop {
                thread::sleep(TICK);
                let elapsed = chunk_started.elapsed();
                let speech_ms = rm.last_speech_ms();
                if speech_ms != last_speech_ms {
                    last_speech_ms = speech_ms;
                    last_speech_change = Instant::now();
                }
                if self.stop_requested.load(Ordering::Acquire) {
                    break;
                }
                if !rm.is_recording() {
                    // Cancelled from the hotkey / tray: the recorder stopped
                    // under us. Finish this chunk and start a fresh one.
                    warn!("Live Mode: recording stopped externally; rotating chunk");
                    break;
                }
                let paused_long_enough =
                    last_speech_ms > 0 && last_speech_change.elapsed() >= SILENCE_BOUNDARY;
                if elapsed >= chunk_target
                    || (live.prefer_silence_boundary
                        && elapsed >= soft_target
                        && paused_long_enough)
                {
                    break;
                }
                if last_heartbeat.elapsed() >= HEARTBEAT {
                    last_heartbeat = Instant::now();
                    let started_at = self.status.lock().unwrap().started_at_ms.unwrap_or(0.0);
                    self.update_status(|s| {
                        s.current_chunk_ms = elapsed.as_secs_f64() * 1000.0;
                        s.current_chunk_speech_ms = speech_ms as f64;
                        s.elapsed_ms =
                            (chrono::Utc::now().timestamp_millis() as f64 - started_at).max(0.0);
                    });
                }
            }

            let stopping = self.stop_requested.load(Ordering::Acquire);
            self.update_status(|s| {
                s.phase = if stopping {
                    LiveModePhase::Stopping
                } else {
                    LiveModePhase::Rotating
                };
            });

            // ── finalize the stream, stop the recorder ──
            // The chunk's speech time, as the dictation paths record it: a
            // Success row without an audio duration fails validation.
            statistics.set_speech_audio_duration_ms(rm.last_speech_ms() as i64, 16_000);
            let mut stream_failed = false;
            let mut chunk_text = match tm.finalize_stream() {
                StreamFinalization::Completed(tracked) => {
                    tracked.attempt.finish(StatisticsRunStatus::Success);
                    statistics.finish(StatisticsRunStatus::Success);
                    Some(tracked.text)
                }
                StreamFinalization::NeverStarted => None,
                // The stream's own attempt was already finished as Failed; the
                // run stays open so a batch fallback below can still succeed.
                StreamFinalization::Failed(err) | StreamFinalization::Timeout(err) => {
                    warn!("Live Mode: stream finalize failed: {err}");
                    stream_failed = true;
                    None
                }
            };

            let stop_result = rm.stop_recording(LIVE_MODE_BINDING_ID, cancel_generation);
            let mut chunk_info = LiveChunkInfo {
                index: chunk_index,
                path: None,
                duration_ms: chunk_started.elapsed().as_secs_f64() * 1000.0,
                bytes: 0.0,
                text_chars: 0,
            };
            match stop_result {
                StopRecordingResult::Captured { recorded, .. } => {
                    if chunk_text.is_none() && !recorded.stt_samples.is_empty() {
                        // No stream text (never started, failed or timed out):
                        // fall back to a batch decode of the captured chunk. A
                        // tracked decode, so a rescued chunk counts as a success
                        // (an untracked one leaves the run without an attempt,
                        // which records it as Empty); on error
                        // `transcribe_tracked` finishes the run as Failed.
                        match tm
                            .transcribe_tracked(recorded.stt_samples.clone(), statistics.clone())
                        {
                            Ok(tracked) => {
                                tracked.attempt.finish(StatisticsRunStatus::Success);
                                statistics.finish(StatisticsRunStatus::Success);
                                chunk_text = Some(tracked.text);
                            }
                            Err(e) => warn!("Live Mode: batch fallback failed: {e}"),
                        }
                    } else if !statistics.is_terminal() {
                        statistics.finish(if stream_failed {
                            StatisticsRunStatus::Failed
                        } else {
                            StatisticsRunStatus::Empty
                        });
                    }

                    if live.save_audio {
                        let path = session_dir.join(format!("chunk_{chunk_index:04}.wav"));
                        let (samples, rate, format) = if recorded.raw_samples.is_empty() {
                            (recorded.stt_samples, 16_000u32, None)
                        } else {
                            (
                                recorded.raw_samples,
                                recorded.native_sample_rate,
                                Some(recorded.native_sample_format),
                            )
                        };
                        chunk_info.duration_ms = samples.len() as f64 / rate.max(1) as f64 * 1000.0;
                        chunk_info.path = Some(path.to_string_lossy().to_string());
                        // List the chunk before the save task can finish, so its
                        // byte count always has an entry to land in. The next
                        // status event carries it.
                        self.status.lock().unwrap().chunks.push(chunk_info.clone());
                        // WAV serialization off the session thread so the next
                        // chunk starts recording right away.
                        let save_path = path.clone();
                        let status = Arc::clone(&self.status);
                        let app_for_event = app.clone();
                        tauri::async_runtime::spawn_blocking(move || {
                            let result = match format {
                                Some(format) => {
                                    save_raw_wav_file(&save_path, &samples, rate, format)
                                }
                                None => save_wav_file(&save_path, &samples),
                            };
                            match result {
                                Ok(()) => {
                                    let bytes = std::fs::metadata(&save_path)
                                        .map(|m| m.len() as f64)
                                        .unwrap_or(0.0);
                                    let snapshot = {
                                        let mut s = status.lock().unwrap();
                                        if let Some(c) =
                                            s.chunks.iter_mut().find(|c| c.index == chunk_index)
                                        {
                                            c.bytes = bytes;
                                        }
                                        s.clone()
                                    };
                                    let _ = LiveModeStateEvent { status: snapshot }
                                        .emit(&app_for_event);
                                }
                                Err(e) => error!(
                                    "Live Mode: failed to write {}: {e}",
                                    save_path.display()
                                ),
                            }
                        });
                    }
                }
                StopRecordingResult::Cancelled | StopRecordingResult::NotActive => {
                    warn!(
                        "Live Mode: chunk {chunk_index} audio was discarded (recording cancelled)"
                    );
                    if !statistics.is_terminal() {
                        statistics.finish(StatisticsRunStatus::Cancelled);
                    }
                }
                StopRecordingResult::Failed(err) => {
                    if !statistics.is_terminal() {
                        statistics.finish(StatisticsRunStatus::Failed);
                    }
                    return Err(anyhow!("Recorder failed: {err}"));
                }
            }

            // ── commit the chunk's text ──
            let text = chunk_text.unwrap_or_default();
            let text = text.trim();
            let committed = if text.is_empty() {
                String::new()
            } else {
                format!("{text}{separator}")
            };
            chunk_info.text_chars = text.chars().count() as u32;
            {
                let mut w = writer.lock().unwrap();
                if let Err(e) = w.commit(&committed) {
                    return Err(anyhow!("Cannot write transcript: {e}"));
                }
                let stable_len = w.stable_len();
                self.update_status(|s| {
                    match s.chunks.iter_mut().find(|c| c.index == chunk_index) {
                        // Listed before its WAV was written: keep the byte count.
                        Some(listed) => listed.text_chars = chunk_info.text_chars,
                        None => s.chunks.push(chunk_info),
                    }
                    s.transcript_bytes = stable_len as f64;
                });
            }
            let _ = LiveModeTranscriptEvent {
                reset: false,
                stable_appended: committed,
                live: String::new(),
            }
            .emit(app);
        }

        Ok(())
    }
}

/// Scan `root` for session folders (those containing a `transcript.*`).
pub fn list_sessions(root: &Path) -> Vec<LiveSessionInfo> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut sessions: Vec<LiveSessionInfo> = entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|entry| {
            let dir = entry.path();
            let mut transcript: Option<PathBuf> = None;
            let mut chunk_count = 0u32;
            for child in std::fs::read_dir(&dir).ok()?.flatten() {
                let path = child.path();
                let name = path.file_name()?.to_string_lossy().to_string();
                if name.starts_with("transcript.") {
                    transcript = Some(path);
                } else if name.starts_with("chunk_") && name.ends_with(".wav") {
                    chunk_count += 1;
                }
            }
            transcript.as_ref()?;
            let modified_ms = std::fs::metadata(&dir)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as f64)
                .unwrap_or(0.0);
            let transcript_bytes = transcript
                .as_ref()
                .and_then(|p| std::fs::metadata(p).ok())
                .map(|m| m.len() as f64)
                .unwrap_or(0.0);
            Some(LiveSessionInfo {
                name: dir.file_name()?.to_string_lossy().to_string(),
                dir: dir.to_string_lossy().to_string(),
                transcript_path: transcript.map(|p| p.to_string_lossy().to_string()),
                transcript_bytes,
                chunk_count,
                modified_ms,
            })
        })
        .collect();
    sessions.sort_by(|a, b| b.name.cmp(&a.name));
    sessions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn character_mode_mirrors_the_whole_stream() {
        assert_eq!(
            live_tail_text("hello wor", "ld here", LiveTranscriptGranularity::Character),
            "hello world here"
        );
        assert_eq!(
            live_tail_text("hello ", "world", LiveTranscriptGranularity::Character),
            "hello world"
        );
        assert_eq!(
            live_tail_text("hello", "", LiveTranscriptGranularity::Character),
            "hello"
        );
    }

    #[test]
    fn word_mode_only_writes_completed_words() {
        assert_eq!(
            live_tail_text("hello wor", "ld", LiveTranscriptGranularity::Word),
            "hello"
        );
        assert_eq!(
            live_tail_text("hello", "world", LiveTranscriptGranularity::Word),
            ""
        );
        // Nothing tentative left: the committed text is final.
        assert_eq!(
            live_tail_text("hello world", "", LiveTranscriptGranularity::Word),
            "hello world"
        );
    }

    #[test]
    fn writer_rewrites_live_tail_and_commits() {
        let dir = crate::utils::temp_test_dir("live");
        let path = dir.join("t.txt");
        let mut w = TranscriptWriter::create(&path, "H\n").unwrap();
        w.set_live("abc def").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "H\nabc def");
        w.set_live("abc").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "H\nabc");
        w.commit("first\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "H\nfirst\n");
        w.set_live("x").unwrap();
        w.commit("").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "H\nfirst\n");
        assert_eq!(w.stable_len(), 8);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_sessions_finds_transcripts() {
        let root = crate::utils::temp_test_dir("live-list");
        let s1 = root.join("live_2026-01-01_10-00-00");
        std::fs::create_dir_all(&s1).unwrap();
        std::fs::write(s1.join("transcript.txt"), "hi").unwrap();
        std::fs::write(s1.join("chunk_0001.wav"), "").unwrap();
        std::fs::create_dir_all(root.join("not_a_session")).unwrap();
        let sessions = list_sessions(&root);
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].chunk_count, 1);
        assert_eq!(sessions[0].transcript_bytes, 2.0);
        let _ = std::fs::remove_dir_all(&root);
    }
}
