//! Isolated persistence contract for per-inference statistics.
//!
//! Each actual inference, including a history retry, gets a separate row and integer run ID.
//! Success, empty, failure, and cancellation outcomes are retained, while summaries include only
//! successes with non-empty raw ASR output. Headless runs have a reserved origin so later lifecycle
//! integration can explicitly include or omit them without changing the schema. Times are UTC Unix
//! milliseconds and ranges are half-open `[start_ms, end_ms)`. Audio duration means captured audio
//! before short-input padding. Transcription latency starts at the user's stop action and ends when
//! canonical raw ASR output is available; post-processing latency uses the same baseline and ends
//! when final text is ready to paste. Word-count version 1 uses Unicode-aware
//! `split_whitespace`: punctuation stays attached, unspaced CJK generally counts as one word, and
//! whitespace-separated emoji count as words. Rows have no history foreign key or recording
//! dependency and persist until a separate manual reset.

use anyhow::{Result, anyhow};
use log::error;
use rusqlite::{Connection, OptionalExtension, Row, params, types::Type as SqlType};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::{
    collections::HashMap,
    path::PathBuf,
    str::FromStr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tauri::AppHandle;
use tauri_specta::Event;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum StatisticsRunStatus {
    Success,
    Empty,
    Failed,
    Cancelled,
}

impl StatisticsRunStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Empty => "empty",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

impl FromStr for StatisticsRunStatus {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value {
            "success" => Ok(Self::Success),
            "empty" => Ok(Self::Empty),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(format!("unknown statistics status: {value}")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum StatisticsRunOrigin {
    Normal,
    HistoryRetry,
    Headless,
}

impl StatisticsRunOrigin {
    fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::HistoryRetry => "history_retry",
            Self::Headless => "headless",
        }
    }
}

impl FromStr for StatisticsRunOrigin {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value {
            "normal" => Ok(Self::Normal),
            "history_retry" => Ok(Self::HistoryRetry),
            "headless" => Ok(Self::Headless),
            _ => Err(format!("unknown statistics origin: {value}")),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewStatisticsRun {
    pub started_at_ms: i64,
    pub completed_at_ms: i64,
    pub status: StatisticsRunStatus,
    pub origin: StatisticsRunOrigin,
    pub model_id: Option<String>,
    pub engine: Option<String>,
    pub backend: Option<String>,
    pub audio_duration_ms: Option<i64>,
    pub sample_rate_hz: Option<i64>,
    pub word_count: Option<i64>,
    pub transcription_latency_ms: Option<i64>,
    pub post_processing_latency_ms: Option<i64>,
    pub post_processing_requested: bool,
    pub measurement_version: i64,
    pub word_count_version: i64,
    pub source_history_id: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredStatisticsRun {
    pub id: i64,
    pub started_at_ms: i64,
    pub completed_at_ms: i64,
    pub status: StatisticsRunStatus,
    pub origin: StatisticsRunOrigin,
    pub model_id: Option<String>,
    pub engine: Option<String>,
    pub backend: Option<String>,
    pub audio_duration_ms: Option<i64>,
    pub sample_rate_hz: Option<i64>,
    pub word_count: Option<i64>,
    pub transcription_latency_ms: Option<i64>,
    pub post_processing_latency_ms: Option<i64>,
    pub post_processing_requested: bool,
    pub measurement_version: i64,
    pub word_count_version: i64,
    pub source_history_id: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct StatisticsRange {
    pub start_ms: f64,
    pub end_ms: f64,
}

impl StatisticsRange {
    pub fn validate(self) -> Result<()> {
        if self.start_ms < 0.0 {
            return Err(anyhow!("statistics range start must be non-negative"));
        }
        if self.end_ms <= self.start_ms {
            return Err(anyhow!("statistics range end must be after its start"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct StatisticsUpdatedEvent;

pub fn audio_duration_ms(sample_count: usize, sample_rate_hz: u32) -> Option<i64> {
    (sample_rate_hz > 0).then(|| (sample_count as i64 * 1_000) / sample_rate_hz as i64)
}

fn canonical_word_count(text: &str) -> i64 {
    text.split_whitespace().count() as i64
}

#[derive(Clone, Debug)]
struct AttemptState {
    finalized: bool,
    model_id: Option<String>,
    engine: Option<String>,
    backend: Option<String>,
    inference_started_at_ms: i64,
    transcription_latency_ms: Option<i64>,
    post_processing_latency_ms: Option<i64>,
    word_count: Option<i64>,
}

#[derive(Debug)]
struct RunState {
    started_at_ms: i64,
    origin: StatisticsRunOrigin,
    source_history_id: Option<i64>,
    audio_duration_ms: Option<i64>,
    sample_rate_hz: Option<i64>,
    input_stopped: Option<Instant>,
    post_processing_requested: bool,
    terminal: bool,
    attempt_count: usize,
    attempts: Vec<Arc<Mutex<AttemptState>>>,
}

fn new_run_state(
    origin: StatisticsRunOrigin,
    source_history_id: Option<i64>,
) -> Arc<Mutex<RunState>> {
    Arc::new(Mutex::new(RunState {
        started_at_ms: chrono::Utc::now().timestamp_millis(),
        origin,
        source_history_id,
        audio_duration_ms: None,
        sample_rate_hz: None,
        input_stopped: None,
        post_processing_requested: false,
        terminal: false,
        attempt_count: 0,
        attempts: Vec::new(),
    }))
}

fn register_attempt(run: &mut RunState, attempt: Arc<Mutex<AttemptState>>) -> bool {
    if run.terminal {
        return false;
    }
    run.attempt_count += 1;
    run.attempts.push(attempt);
    true
}

fn complete_canonical(attempt: &mut AttemptState, text: &str, input_stopped: Option<Instant>) {
    if attempt.finalized || attempt.word_count.is_some() {
        return;
    }
    attempt.word_count = Some(canonical_word_count(text));
    if !text.trim().is_empty() {
        attempt.transcription_latency_ms =
            input_stopped.map(|stopped| stopped.elapsed().as_millis() as i64);
    }
}

fn complete_post_processing(
    attempt: &mut AttemptState,
    input_stopped: Option<Instant>,
    requested: bool,
) {
    if !attempt.finalized && requested && attempt.post_processing_latency_ms.is_none() {
        attempt.post_processing_latency_ms =
            input_stopped.map(|stopped| stopped.elapsed().as_millis() as i64);
    }
}

fn resolved_terminal_status(
    requested: StatisticsRunStatus,
    word_count: Option<i64>,
) -> StatisticsRunStatus {
    if requested == StatisticsRunStatus::Success && word_count.is_none_or(|words| words == 0) {
        StatisticsRunStatus::Empty
    } else {
        requested
    }
}

fn mark_terminal(run: &mut RunState) -> bool {
    if run.terminal {
        false
    } else {
        run.terminal = true;
        true
    }
}

fn persist_best_effort(repository: &StatisticsRepository, run: &NewStatisticsRun) -> bool {
    match repository.insert(run) {
        Ok(_) => true,
        Err(err) => {
            error!("Failed to persist transcription statistics: {err}");
            false
        }
    }
}

pub struct StatisticsManager {
    repository: StatisticsRepository,
    app_handle: AppHandle,
    active_normal_runs: Mutex<NormalRunRegistry>,
}

#[derive(Default)]
struct NormalRunRegistry {
    runs: HashMap<String, Arc<Mutex<RunState>>>,
}

impl NormalRunRegistry {
    fn get(&self, binding_id: &str) -> Option<Arc<Mutex<RunState>>> {
        self.runs.get(binding_id).cloned()
    }

    fn insert_or_get(
        &mut self,
        binding_id: &str,
        state: Arc<Mutex<RunState>>,
    ) -> Arc<Mutex<RunState>> {
        Arc::clone(self.runs.entry(binding_id.to_string()).or_insert(state))
    }

    fn remove_state(&mut self, state: &Arc<Mutex<RunState>>) {
        self.runs.retain(|_, active| !Arc::ptr_eq(active, state));
    }
}

#[derive(Clone)]
pub struct StatisticsRunContext {
    manager: Arc<StatisticsManager>,
    state: Arc<Mutex<RunState>>,
}

#[derive(Clone)]
pub struct PendingStatisticsAttempt {
    manager: Arc<StatisticsManager>,
    run: Arc<Mutex<RunState>>,
    state: Arc<Mutex<AttemptState>>,
}

impl StatisticsManager {
    pub fn new(app_handle: &AppHandle, db_path: PathBuf) -> Self {
        Self {
            repository: StatisticsRepository::new(db_path),
            app_handle: app_handle.clone(),
            active_normal_runs: Mutex::new(NormalRunRegistry::default()),
        }
    }

    fn new_run(
        self: &Arc<Self>,
        origin: StatisticsRunOrigin,
        source_history_id: Option<i64>,
    ) -> StatisticsRunContext {
        StatisticsRunContext {
            manager: Arc::clone(self),
            state: new_run_state(origin, source_history_id),
        }
    }

    pub fn begin_normal_run(self: &Arc<Self>, binding_id: &str) -> StatisticsRunContext {
        let candidate = self.new_run(StatisticsRunOrigin::Normal, None);
        let state = self
            .active_normal_runs
            .lock()
            .unwrap()
            .insert_or_get(binding_id, candidate.state);
        StatisticsRunContext {
            manager: Arc::clone(self),
            state,
        }
    }

    pub fn get_normal_run(self: &Arc<Self>, binding_id: &str) -> Option<StatisticsRunContext> {
        self.active_normal_runs
            .lock()
            .unwrap()
            .get(binding_id)
            .map(|state| StatisticsRunContext {
                manager: Arc::clone(self),
                state,
            })
    }

    pub fn begin_history_retry(self: &Arc<Self>, source_history_id: i64) -> StatisticsRunContext {
        self.new_run(StatisticsRunOrigin::HistoryRetry, Some(source_history_id))
    }

    pub fn cancel_active_normal_runs(&self) {
        let active = self
            .active_normal_runs
            .lock()
            .unwrap()
            .runs
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for state in active {
            self.finish_run(&state, StatisticsRunStatus::Cancelled);
        }
    }

    pub fn summarize(&self, range: StatisticsRange) -> Result<StatisticsSummary> {
        range.validate()?;
        self.repository.summarize(range)
    }

    pub fn reset(&self) -> Result<usize> {
        let count = self.repository.reset()?;
        let _ = StatisticsUpdatedEvent.emit(&self.app_handle);
        Ok(count)
    }

    fn finish_run(&self, state: &Arc<Mutex<RunState>>, status: StatisticsRunStatus) {
        let (attempts, operational) = {
            let mut run = state.lock().unwrap();
            if !mark_terminal(&mut run) {
                return;
            }
            let attempts = run.attempts.clone();
            let operational = (run.attempt_count == 0).then(|| NewStatisticsRun {
                started_at_ms: run.started_at_ms,
                completed_at_ms: chrono::Utc::now().timestamp_millis(),
                status,
                origin: run.origin,
                model_id: None,
                engine: None,
                backend: None,
                audio_duration_ms: run.audio_duration_ms,
                sample_rate_hz: run.sample_rate_hz,
                word_count: None,
                transcription_latency_ms: None,
                post_processing_latency_ms: None,
                post_processing_requested: run.post_processing_requested,
                measurement_version: 1,
                word_count_version: 1,
                source_history_id: run.source_history_id,
            });
            (attempts, operational)
        };

        for attempt in attempts {
            self.finish_attempt(state, &attempt, status);
        }
        if let Some(run) = operational {
            self.insert_best_effort(run);
        }

        self.active_normal_runs.lock().unwrap().remove_state(state);
    }

    fn finish_attempt(
        &self,
        run_state: &Arc<Mutex<RunState>>,
        attempt_state: &Arc<Mutex<AttemptState>>,
        status: StatisticsRunStatus,
    ) {
        let attempt = {
            let mut attempt = attempt_state.lock().unwrap();
            if attempt.finalized {
                return;
            }
            attempt.finalized = true;
            attempt.clone()
        };
        let run = run_state.lock().unwrap();
        let status = resolved_terminal_status(status, attempt.word_count);
        let (transcription_latency_ms, post_processing_latency_ms) =
            if status == StatisticsRunStatus::Success {
                (
                    attempt.transcription_latency_ms,
                    attempt.post_processing_latency_ms,
                )
            } else {
                (None, None)
            };
        self.insert_best_effort(NewStatisticsRun {
            started_at_ms: attempt.inference_started_at_ms,
            completed_at_ms: chrono::Utc::now().timestamp_millis(),
            status,
            origin: run.origin,
            model_id: attempt.model_id,
            engine: attempt.engine,
            backend: attempt.backend,
            audio_duration_ms: run.audio_duration_ms,
            sample_rate_hz: run.sample_rate_hz,
            word_count: attempt.word_count,
            transcription_latency_ms,
            post_processing_latency_ms,
            post_processing_requested: run.post_processing_requested,
            measurement_version: 1,
            word_count_version: 1,
            source_history_id: run.source_history_id,
        });
    }

    fn insert_best_effort(&self, run: NewStatisticsRun) {
        if persist_best_effort(&self.repository, &run) {
            let _ = StatisticsUpdatedEvent.emit(&self.app_handle);
        }
    }
}

impl StatisticsRunContext {
    pub fn set_audio(&self, captured_sample_count: usize, sample_rate_hz: u32) {
        let mut run = self.state.lock().unwrap();
        run.audio_duration_ms = audio_duration_ms(captured_sample_count, sample_rate_hz);
        run.sample_rate_hz = (sample_rate_hz > 0).then_some(sample_rate_hz as i64);
    }

    pub fn set_speech_audio_duration_ms(&self, speech_duration_ms: i64, sample_rate_hz: u32) {
        let mut run = self.state.lock().unwrap();
        run.audio_duration_ms = Some(speech_duration_ms);
        run.sample_rate_hz = (sample_rate_hz > 0).then_some(sample_rate_hz as i64);
    }

    pub fn mark_input_stopped(&self, stopped: Instant, post_processing_requested: bool) {
        let mut run = self.state.lock().unwrap();
        if run.input_stopped.is_none() {
            run.input_stopped = Some(stopped);
            run.post_processing_requested = post_processing_requested;
        }
    }

    #[allow(dead_code)]
    pub fn set_post_processing_requested(&self, requested: bool) {
        self.state.lock().unwrap().post_processing_requested = requested;
    }

    pub fn begin_attempt(
        &self,
        model_id: Option<String>,
        engine: Option<String>,
        backend: Option<String>,
        inference_started_at_ms: i64,
    ) -> Option<PendingStatisticsAttempt> {
        let state = Arc::new(Mutex::new(AttemptState {
            finalized: false,
            model_id,
            engine,
            backend,
            inference_started_at_ms,
            transcription_latency_ms: None,
            post_processing_latency_ms: None,
            word_count: None,
        }));
        let mut run = self.state.lock().unwrap();
        if !register_attempt(&mut run, Arc::clone(&state)) {
            return None;
        }
        drop(run);
        Some(PendingStatisticsAttempt {
            manager: Arc::clone(&self.manager),
            run: Arc::clone(&self.state),
            state,
        })
    }

    pub fn finish(&self, status: StatisticsRunStatus) {
        self.manager.finish_run(&self.state, status);
    }

    pub fn is_terminal(&self) -> bool {
        self.state.lock().unwrap().terminal
    }
}

impl PendingStatisticsAttempt {
    pub fn complete_canonical(&self, text: &str) {
        let input_stopped = self.run.lock().unwrap().input_stopped;
        let mut attempt = self.state.lock().unwrap();
        complete_canonical(&mut attempt, text, input_stopped);
    }

    pub fn complete_post_processing(&self) {
        let (input_stopped, requested) = {
            let run = self.run.lock().unwrap();
            (run.input_stopped, run.post_processing_requested)
        };
        let mut attempt = self.state.lock().unwrap();
        complete_post_processing(&mut attempt, input_stopped, requested);
    }

    pub fn finish(&self, status: StatisticsRunStatus) {
        self.manager.finish_attempt(&self.run, &self.state, status);
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct DurationMetricSummary {
    pub sample_count: i32,
    pub minimum_ms: Option<f64>,
    pub average_ms: Option<f64>,
    pub maximum_ms: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct StatisticsSummary {
    pub range: StatisticsRange,
    pub transcription_count: i32,
    pub total_words: i32,
    pub average_words: Option<f64>,
    pub total_audio_duration_ms: f64,
    pub average_audio_duration_ms: Option<f64>,
    pub approximate_words_per_minute: Option<f64>,
    pub current_streak_days: i32,
    pub transcription_latency: DurationMetricSummary,
    pub post_processing_latency: DurationMetricSummary,
}

pub struct StatisticsRepository {
    db_path: PathBuf,
}

impl StatisticsRepository {
    pub fn new(db_path: PathBuf) -> Self {
        Self { db_path }
    }

    fn get_connection(&self) -> Result<Connection> {
        let conn = Connection::open(&self.db_path)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        Ok(conn)
    }

    pub fn insert(&self, run: &NewStatisticsRun) -> Result<StoredStatisticsRun> {
        let conn = self.get_connection()?;
        Self::insert_with_conn(&conn, run)
    }

    #[allow(dead_code)]
    pub fn get_by_id(&self, id: i64) -> Result<Option<StoredStatisticsRun>> {
        let conn = self.get_connection()?;
        Self::get_by_id_with_conn(&conn, id)
    }

    pub fn summarize(&self, range: StatisticsRange) -> Result<StatisticsSummary> {
        let conn = self.get_connection()?;
        Self::summarize_with_conn(&conn, range)
    }

    pub fn reset(&self) -> Result<usize> {
        let conn = self.get_connection()?;
        Ok(conn.execute("DELETE FROM transcription_statistics", [])?)
    }

    fn validate_run(run: &NewStatisticsRun) -> Result<()> {
        if run.started_at_ms < 0 || run.completed_at_ms < run.started_at_ms {
            return Err(anyhow!("invalid statistics run timestamps"));
        }
        if run.measurement_version <= 0 || run.word_count_version <= 0 {
            return Err(anyhow!("statistics versions must be positive"));
        }
        for (name, value) in [
            ("model_id", run.model_id.as_deref()),
            ("engine", run.engine.as_deref()),
            ("backend", run.backend.as_deref()),
        ] {
            if value.is_some_and(str::is_empty) {
                return Err(anyhow!("{name} must not be empty when provided"));
            }
        }
        for (name, value) in [
            ("audio duration", run.audio_duration_ms),
            ("word count", run.word_count),
            ("transcription latency", run.transcription_latency_ms),
            ("post-processing latency", run.post_processing_latency_ms),
        ] {
            if value.is_some_and(|value| value < 0) {
                return Err(anyhow!("statistics {name} must be non-negative"));
            }
        }
        if run.sample_rate_hz.is_some_and(|value| value <= 0) {
            return Err(anyhow!("statistics sample rate must be positive"));
        }
        if run.audio_duration_ms.is_some() != run.sample_rate_hz.is_some() {
            return Err(anyhow!(
                "statistics audio duration and sample rate must be provided together"
            ));
        }
        if run.source_history_id.is_some_and(|value| value <= 0) {
            return Err(anyhow!("statistics source history ID must be positive"));
        }
        if run.status != StatisticsRunStatus::Success
            && (run.transcription_latency_ms.is_some() || run.post_processing_latency_ms.is_some())
        {
            return Err(anyhow!(
                "latency samples require a successful statistics run"
            ));
        }
        if run.post_processing_latency_ms.is_some() && !run.post_processing_requested {
            return Err(anyhow!(
                "post-processing latency requires post-processing to be requested"
            ));
        }
        if run.post_processing_latency_ms.is_some()
            && (run.transcription_latency_ms.is_none()
                || run.post_processing_latency_ms < run.transcription_latency_ms)
        {
            return Err(anyhow!(
                "post-processing latency cannot precede transcription latency"
            ));
        }
        if run.status == StatisticsRunStatus::Success
            && (run.word_count.is_none_or(|value| value <= 0)
                || run.audio_duration_ms.is_none()
                || run.sample_rate_hz.is_none())
        {
            return Err(anyhow!(
                "successful statistics runs require words, audio, and sample rate"
            ));
        }
        Ok(())
    }

    fn insert_with_conn(conn: &Connection, run: &NewStatisticsRun) -> Result<StoredStatisticsRun> {
        Self::validate_run(run)?;
        conn.execute(
            "INSERT INTO transcription_statistics (
                started_at_ms, completed_at_ms, status, origin, model_id, engine, backend,
                audio_duration_ms, sample_rate_hz, word_count, transcription_latency_ms,
                post_processing_latency_ms, post_processing_requested, measurement_version,
                word_count_version, source_history_id
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                run.started_at_ms,
                run.completed_at_ms,
                run.status.as_str(),
                run.origin.as_str(),
                run.model_id,
                run.engine,
                run.backend,
                run.audio_duration_ms,
                run.sample_rate_hz,
                run.word_count,
                run.transcription_latency_ms,
                run.post_processing_latency_ms,
                run.post_processing_requested,
                run.measurement_version,
                run.word_count_version,
                run.source_history_id,
            ],
        )?;
        Self::get_by_id_with_conn(conn, conn.last_insert_rowid())?
            .ok_or_else(|| anyhow!("inserted statistics run was not found"))
    }

    fn get_by_id_with_conn(conn: &Connection, id: i64) -> Result<Option<StoredStatisticsRun>> {
        let mut statement = conn.prepare(
            "SELECT id, started_at_ms, completed_at_ms, status, origin, model_id, engine,
                    backend, audio_duration_ms, sample_rate_hz, word_count,
                    transcription_latency_ms, post_processing_latency_ms,
                    post_processing_requested, measurement_version,
                    word_count_version, source_history_id
             FROM transcription_statistics WHERE id = ?1",
        )?;
        Ok(statement
            .query_row([id], Self::map_statistics_run)
            .optional()?)
    }

    fn map_statistics_run(row: &Row<'_>) -> rusqlite::Result<StoredStatisticsRun> {
        let status_value: String = row.get("status")?;
        let origin_value: String = row.get("origin")?;
        let post_processing_requested_value: i64 = row.get("post_processing_requested")?;
        let status = status_value.parse().map_err(|error: String| {
            rusqlite::Error::FromSqlConversionFailure(3, SqlType::Text, error.into())
        })?;
        let origin = origin_value.parse().map_err(|error: String| {
            rusqlite::Error::FromSqlConversionFailure(4, SqlType::Text, error.into())
        })?;
        let post_processing_requested = match post_processing_requested_value {
            0 => false,
            1 => true,
            value => {
                return Err(rusqlite::Error::FromSqlConversionFailure(
                    13,
                    SqlType::Integer,
                    format!("invalid post-processing requested flag: {value}").into(),
                ));
            }
        };
        let stored = StoredStatisticsRun {
            id: row.get("id")?,
            started_at_ms: row.get("started_at_ms")?,
            completed_at_ms: row.get("completed_at_ms")?,
            status,
            origin,
            model_id: row.get("model_id")?,
            engine: row.get("engine")?,
            backend: row.get("backend")?,
            audio_duration_ms: row.get("audio_duration_ms")?,
            sample_rate_hz: row.get("sample_rate_hz")?,
            word_count: row.get("word_count")?,
            transcription_latency_ms: row.get("transcription_latency_ms")?,
            post_processing_latency_ms: row.get("post_processing_latency_ms")?,
            post_processing_requested,
            measurement_version: row.get("measurement_version")?,
            word_count_version: row.get("word_count_version")?,
            source_history_id: row.get("source_history_id")?,
        };
        let validation_copy = NewStatisticsRun {
            started_at_ms: stored.started_at_ms,
            completed_at_ms: stored.completed_at_ms,
            status: stored.status,
            origin: stored.origin,
            model_id: stored.model_id.clone(),
            engine: stored.engine.clone(),
            backend: stored.backend.clone(),
            audio_duration_ms: stored.audio_duration_ms,
            sample_rate_hz: stored.sample_rate_hz,
            word_count: stored.word_count,
            transcription_latency_ms: stored.transcription_latency_ms,
            post_processing_latency_ms: stored.post_processing_latency_ms,
            post_processing_requested: stored.post_processing_requested,
            measurement_version: stored.measurement_version,
            word_count_version: stored.word_count_version,
            source_history_id: stored.source_history_id,
        };
        Self::validate_run(&validation_copy).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(0, SqlType::Integer, error.into())
        })?;
        Ok(stored)
    }

    fn calculate_streak_with_conn(conn: &Connection) -> Result<i32> {
        let mut stmt = conn.prepare(
            "SELECT DISTINCT date(completed_at_ms / 1000, 'unixepoch', 'localtime') as day
             FROM transcription_statistics
             WHERE status = 'success' AND completed_at_ms > 0
             ORDER BY day DESC",
        )?;
        let days: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();

        if days.is_empty() {
            return Ok(0);
        }

        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let yesterday = (chrono::Local::now() - chrono::Duration::days(1))
            .format("%Y-%m-%d")
            .to_string();

        if days.first() != Some(&today) && days.first() != Some(&yesterday) {
            return Ok(0);
        }

        let mut current_streak: i32 = 1;
        for i in 1..days.len() {
            if let (Ok(prev), Ok(curr)) = (
                chrono::NaiveDate::parse_from_str(&days[i - 1], "%Y-%m-%d"),
                chrono::NaiveDate::parse_from_str(&days[i], "%Y-%m-%d"),
            ) {
                if prev.signed_duration_since(curr).num_days() == 1 {
                    current_streak += 1;
                } else {
                    break;
                }
            }
        }

        Ok(current_streak)
    }

    fn summarize_with_conn(conn: &Connection, range: StatisticsRange) -> Result<StatisticsSummary> {
        range.validate()?;
        let streak = Self::calculate_streak_with_conn(conn).unwrap_or(0);
        let summary = conn.query_row(
            "SELECT
                COUNT(*) AS transcription_count,
                COALESCE(SUM(s.word_count), 0) AS total_words,
                AVG(s.word_count) AS average_words,
                COALESCE(SUM(COALESCE(h.speech_duration_ms, s.audio_duration_ms)), 0) AS total_audio_duration_ms,
                AVG(COALESCE(h.speech_duration_ms, s.audio_duration_ms)) AS average_audio_duration_ms,
                CASE WHEN SUM(COALESCE(h.speech_duration_ms, s.audio_duration_ms)) > 0
                    THEN 60_000.0 * SUM(s.word_count) / SUM(COALESCE(h.speech_duration_ms, s.audio_duration_ms))
                END AS approximate_words_per_minute,
                COUNT(s.transcription_latency_ms) AS transcription_sample_count,
                MIN(s.transcription_latency_ms) AS transcription_minimum_ms,
                AVG(s.transcription_latency_ms) AS transcription_average_ms,
                MAX(s.transcription_latency_ms) AS transcription_maximum_ms,
                COUNT(s.post_processing_latency_ms) AS post_processing_sample_count,
                MIN(s.post_processing_latency_ms) AS post_processing_minimum_ms,
                AVG(s.post_processing_latency_ms) AS post_processing_average_ms,
                MAX(s.post_processing_latency_ms) AS post_processing_maximum_ms
             FROM transcription_statistics s
             LEFT JOIN transcription_history h ON s.source_history_id = h.id
             WHERE s.status = 'success' AND s.completed_at_ms >= ?1 AND s.completed_at_ms < ?2",
            params![range.start_ms, range.end_ms],
            |row| {
                Ok(StatisticsSummary {
                    range,
                    transcription_count: row.get("transcription_count")?,
                    total_words: row.get("total_words")?,
                    average_words: row.get("average_words")?,
                    total_audio_duration_ms: row.get("total_audio_duration_ms")?,
                    average_audio_duration_ms: row.get("average_audio_duration_ms")?,
                    approximate_words_per_minute: row.get("approximate_words_per_minute")?,
                    current_streak_days: streak,
                    transcription_latency: DurationMetricSummary {
                        sample_count: row.get("transcription_sample_count")?,
                        minimum_ms: row.get("transcription_minimum_ms")?,
                        average_ms: row.get("transcription_average_ms")?,
                        maximum_ms: row.get("transcription_maximum_ms")?,
                    },
                    post_processing_latency: DurationMetricSummary {
                        sample_count: row.get("post_processing_sample_count")?,
                        minimum_ms: row.get("post_processing_minimum_ms")?,
                        average_ms: row.get("post_processing_average_ms")?,
                        maximum_ms: row.get("post_processing_maximum_ms")?,
                    },
                })
            },
        )?;
        Ok(summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::managers::history::apply_migrations;

    fn setup_conn() -> Connection {
        let mut conn = Connection::open_in_memory().expect("open in-memory database");
        apply_migrations(&mut conn).expect("apply database migrations");
        conn
    }

    fn successful_run(completed_at_ms: i64) -> NewStatisticsRun {
        NewStatisticsRun {
            started_at_ms: completed_at_ms - 500,
            completed_at_ms,
            status: StatisticsRunStatus::Success,
            origin: StatisticsRunOrigin::Normal,
            model_id: Some("whisper-small".into()),
            engine: Some("whisper".into()),
            backend: Some("cpu".into()),
            audio_duration_ms: Some(2_000),
            sample_rate_hz: Some(16_000),
            word_count: Some(10),
            transcription_latency_ms: Some(500),
            post_processing_latency_ms: None,
            post_processing_requested: false,
            measurement_version: 1,
            word_count_version: 1,
            source_history_id: None,
        }
    }

    fn pending_run_state() -> RunState {
        RunState {
            started_at_ms: 0,
            origin: StatisticsRunOrigin::Normal,
            source_history_id: None,
            audio_duration_ms: None,
            sample_rate_hz: None,
            input_stopped: None,
            post_processing_requested: false,
            terminal: false,
            attempt_count: 0,
            attempts: Vec::new(),
        }
    }

    #[test]
    fn word_count_uses_canonical_whitespace_semantics() {
        assert_eq!(canonical_word_count("  one\ttwo\nthree  "), 3);
        assert_eq!(canonical_word_count("????"), 1);
        assert_eq!(canonical_word_count("   "), 0);
    }

    #[test]
    fn audio_duration_uses_pre_padding_sample_count() {
        assert_eq!(audio_duration_ms(8_000, 16_000), Some(500));
        assert_eq!(audio_duration_ms(8_000, 0), None);
    }

    #[test]
    fn terminal_transition_is_idempotent() {
        let mut run = pending_run_state();
        assert!(mark_terminal(&mut run));
        assert!(!mark_terminal(&mut run));
    }

    #[test]
    fn normal_run_registry_is_strong_keyed_and_retry_independent() {
        let mut registry = NormalRunRegistry::default();
        let first = new_run_state(StatisticsRunOrigin::Normal, None);
        let registered = registry.insert_or_get("primary", Arc::clone(&first));
        drop(first);
        assert!(Arc::ptr_eq(
            &registered,
            &registry.get("primary").expect("registered normal run")
        ));

        let second = new_run_state(StatisticsRunOrigin::Normal, None);
        registry.insert_or_get("secondary", Arc::clone(&second));
        assert!(!Arc::ptr_eq(
            &registered,
            &registry.get("secondary").expect("second normal run")
        ));
        let retry = new_run_state(StatisticsRunOrigin::HistoryRetry, Some(42));
        assert_eq!(
            retry.lock().unwrap().origin,
            StatisticsRunOrigin::HistoryRetry
        );
        assert_eq!(retry.lock().unwrap().source_history_id, Some(42));
        assert!(Arc::ptr_eq(
            &registered,
            &registry
                .get("primary")
                .expect("normal run remains registered")
        ));
    }

    #[test]
    fn summarizes_counts_words_audio_and_latencies() {
        let conn = setup_conn();
        let run1 = successful_run(1_000);
        let mut run2 = successful_run(2_000);
        run2.word_count = Some(20);
        run2.audio_duration_ms = Some(4_000);
        run2.transcription_latency_ms = Some(1_000);
        run2.post_processing_requested = true;
        run2.post_processing_latency_ms = Some(1_500);

        StatisticsRepository::insert_with_conn(&conn, &run1).expect("insert run 1");
        StatisticsRepository::insert_with_conn(&conn, &run2).expect("insert run 2");

        let summary = StatisticsRepository::summarize_with_conn(
            &conn,
            StatisticsRange {
                start_ms: 0.0,
                end_ms: 3_000.0,
            },
        )
        .expect("summarize");

        assert_eq!(summary.transcription_count, 2);
        assert_eq!(summary.total_words, 30);
        assert_eq!(summary.average_words, Some(15.0));
        assert_eq!(summary.total_audio_duration_ms, 6_000.0);
        assert_eq!(summary.average_audio_duration_ms, Some(3_000.0));
        assert_eq!(summary.approximate_words_per_minute, Some(300.0));
        assert_eq!(summary.transcription_latency.sample_count, 2);
        assert_eq!(summary.transcription_latency.minimum_ms, Some(500.0));
        assert_eq!(summary.transcription_latency.average_ms, Some(750.0));
        assert_eq!(summary.transcription_latency.maximum_ms, Some(1_000.0));
        assert_eq!(summary.post_processing_latency.sample_count, 1);
        assert_eq!(summary.post_processing_latency.minimum_ms, Some(1_500.0));
        assert_eq!(summary.post_processing_latency.average_ms, Some(1_500.0));
        assert_eq!(summary.post_processing_latency.maximum_ms, Some(1_500.0));
    }
}
