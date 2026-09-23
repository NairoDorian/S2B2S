use crate::audio_toolkit::VoiceActivityDetector;
use anyhow::{Result, anyhow};
use chrono::{DateTime, Local, Utc};
use log::{debug, error, info, warn};
use rusqlite::{Connection, OptionalExtension, params};
use rusqlite_migration::{M, Migrations};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use tauri::AppHandle;
use tauri_specta::Event;

/// Database migrations for transcription history.
/// Each migration is applied in order. The library tracks which migrations
/// have been applied using SQLite's user_version pragma.
///
/// Note: For users upgrading from tauri-plugin-sql, migrate_from_tauri_plugin_sql()
/// converts the old _sqlx_migrations table tracking to the user_version pragma,
/// ensuring migrations don't re-run on existing databases.
static MIGRATIONS: &[M] = &[
    M::up(
        "CREATE TABLE IF NOT EXISTS transcription_history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            file_name TEXT NOT NULL,
            timestamp INTEGER NOT NULL,
            saved BOOLEAN NOT NULL DEFAULT 0,
            title TEXT NOT NULL,
            transcription_text TEXT NOT NULL
        );",
    ),
    M::up("ALTER TABLE transcription_history ADD COLUMN post_processed_text TEXT;"),
    M::up("ALTER TABLE transcription_history ADD COLUMN post_process_prompt TEXT;"),
    M::up(
        "ALTER TABLE transcription_history ADD COLUMN post_process_requested BOOLEAN NOT NULL DEFAULT 0;",
    ),
    M::up(
        "CREATE TABLE transcription_statistics (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            started_at_ms INTEGER NOT NULL CHECK (started_at_ms >= 0),
            completed_at_ms INTEGER NOT NULL CHECK (completed_at_ms >= started_at_ms),
            status TEXT NOT NULL CHECK (status IN ('success', 'empty', 'failed', 'cancelled')),
            origin TEXT NOT NULL CHECK (origin IN ('normal', 'history_retry', 'headless')),
            model_id TEXT CHECK (model_id IS NULL OR length(model_id) > 0),
            engine TEXT CHECK (engine IS NULL OR length(engine) > 0),
            backend TEXT CHECK (backend IS NULL OR length(backend) > 0),
            audio_duration_ms INTEGER CHECK (audio_duration_ms IS NULL OR audio_duration_ms >= 0),
            sample_rate_hz INTEGER CHECK (sample_rate_hz IS NULL OR sample_rate_hz > 0),
            word_count INTEGER CHECK (word_count IS NULL OR word_count >= 0),
            transcription_latency_ms INTEGER CHECK (transcription_latency_ms IS NULL OR transcription_latency_ms >= 0),
            post_processing_latency_ms INTEGER CHECK (post_processing_latency_ms IS NULL OR post_processing_latency_ms >= 0),
            post_processing_requested INTEGER NOT NULL CHECK (post_processing_requested IN (0, 1)),
            measurement_version INTEGER NOT NULL CHECK (measurement_version > 0),
            word_count_version INTEGER NOT NULL CHECK (word_count_version > 0),
            source_history_id INTEGER CHECK (source_history_id IS NULL OR source_history_id > 0),
            CHECK ((audio_duration_ms IS NULL) = (sample_rate_hz IS NULL)),
            CHECK (transcription_latency_ms IS NULL OR status = 'success'),
            CHECK (
                post_processing_latency_ms IS NULL OR
                (status = 'success' AND post_processing_requested = 1 AND
                 transcription_latency_ms IS NOT NULL AND
                 post_processing_latency_ms >= transcription_latency_ms)
            ),
            CHECK (
                status != 'success' OR
                (word_count IS NOT NULL AND word_count > 0 AND audio_duration_ms IS NOT NULL AND
                 sample_rate_hz IS NOT NULL)
            )
        );
        CREATE INDEX transcription_statistics_status_completed_idx
            ON transcription_statistics(status, completed_at_ms);",
    ),
    M::up(
        "ALTER TABLE transcription_history ADD COLUMN model_id TEXT;
         ALTER TABLE transcription_history ADD COLUMN engine TEXT;
         ALTER TABLE transcription_history ADD COLUMN audio_duration_ms REAL;
         ALTER TABLE transcription_history ADD COLUMN speech_duration_ms REAL;
         ALTER TABLE transcription_history ADD COLUMN sample_rate_hz INTEGER;
         ALTER TABLE transcription_history ADD COLUMN word_count INTEGER;
         ALTER TABLE transcription_history ADD COLUMN transcription_latency_ms REAL;
         ALTER TABLE transcription_history ADD COLUMN post_processing_latency_ms REAL;
         ALTER TABLE transcription_history ADD COLUMN language TEXT;
         ALTER TABLE transcription_history ADD COLUMN mode TEXT;
         ALTER TABLE transcription_history ADD COLUMN extra_models TEXT;",
    ),
];

fn migrations() -> Migrations<'static> {
    Migrations::new(MIGRATIONS.to_vec())
}

pub(super) fn apply_migrations(conn: &mut Connection) -> Result<()> {
    migrate_from_tauri_plugin_sql(conn)?;

    let migrations = migrations();
    #[cfg(debug_assertions)]
    migrations.validate().expect("Invalid migrations");
    migrations.to_latest(conn)?;
    Ok(())
}

fn migrate_from_tauri_plugin_sql(conn: &Connection) -> Result<()> {
    // Check if the old _sqlx_migrations table exists
    let has_sqlx_migrations: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='_sqlx_migrations'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(false);

    if !has_sqlx_migrations {
        return Ok(());
    }

    // Check current user_version
    let current_version: i32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;

    if current_version > 0 {
        // Already migrated to rusqlite_migration system
        return Ok(());
    }

    // Get the highest version from the old migrations table
    let old_version: i32 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM _sqlx_migrations WHERE success = 1",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    if old_version > 0 {
        info!(
            "Migrating from tauri-plugin-sql (version {}) to rusqlite_migration",
            old_version
        );

        // Set user_version to match the old migration state
        conn.pragma_update(None, "user_version", old_version)?;

        // Optionally drop the old migrations table (keeping it doesn't hurt)
        // conn.execute("DROP TABLE IF EXISTS _sqlx_migrations", [])?;

        info!(
            "Migration tracking converted: user_version set to {}",
            old_version
        );
    }

    Ok(())
}

const HISTORY_COLUMNS: &str = "id, file_name, timestamp, saved, title, transcription_text, post_processed_text, post_process_prompt, post_process_requested, model_id, engine, audio_duration_ms, speech_duration_ms, sample_rate_hz, word_count, transcription_latency_ms, post_processing_latency_ms, language, mode, extra_models";

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct PaginatedHistory {
    pub entries: Vec<HistoryEntry>,
    pub has_more: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type, tauri_specta::Event)]
#[serde(tag = "action")]
pub enum HistoryUpdatePayload {
    #[serde(rename = "added")]
    Added { entry: HistoryEntry },
    #[serde(rename = "updated")]
    Updated { entry: HistoryEntry },
    #[serde(rename = "deleted")]
    Deleted { id: i32 },
    #[serde(rename = "toggled")]
    Toggled { id: i32 },
    #[serde(rename = "cleared")]
    Cleared,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct HistoryEntry {
    pub id: i32,
    pub file_name: String,
    pub timestamp: f64,
    pub saved: bool,
    pub title: String,
    pub transcription_text: String,
    pub post_processed_text: Option<String>,
    pub post_process_prompt: Option<String>,
    pub post_process_requested: bool,
    pub model_id: Option<String>,
    pub engine: Option<String>,
    pub audio_duration_ms: Option<f64>,
    pub speech_duration_ms: Option<f64>,
    pub sample_rate_hz: Option<i32>,
    pub word_count: Option<i32>,
    pub transcription_latency_ms: Option<f64>,
    pub post_processing_latency_ms: Option<f64>,
    pub language: Option<String>,
    pub mode: Option<String>,
    pub extra_models: Option<Vec<String>>,
}

#[derive(Clone, Debug, Default)]
pub struct NewHistoryEntry {
    pub file_name: String,
    pub transcription_text: String,
    pub post_process_requested: bool,
    pub post_processed_text: Option<String>,
    pub post_process_prompt: Option<String>,
    pub model_id: Option<String>,
    pub engine: Option<String>,
    pub audio_duration_ms: Option<f64>,
    pub speech_duration_ms: Option<f64>,
    pub sample_rate_hz: Option<i32>,
    pub word_count: Option<i32>,
    pub transcription_latency_ms: Option<f64>,
    pub post_processing_latency_ms: Option<f64>,
    pub language: Option<String>,
    pub mode: Option<String>,
    pub extra_models: Option<Vec<String>>,
}

pub struct HistoryManager {
    app_handle: AppHandle,
    recordings_dir: PathBuf,
    db_path: PathBuf,
}

impl HistoryManager {
    pub fn new(app_handle: &AppHandle) -> Result<Self> {
        // Create recordings directory in app data dir
        let app_data_dir = crate::portable::app_data_dir(app_handle)?;
        let recordings_dir = app_data_dir.join("recordings");
        let db_path = app_data_dir.join("history.db");

        // Ensure recordings directory exists
        if !recordings_dir.exists() {
            fs::create_dir_all(&recordings_dir)?;
            debug!("Created recordings directory: {:?}", recordings_dir);
        }

        let manager = Self {
            app_handle: app_handle.clone(),
            recordings_dir,
            db_path,
        };

        // Initialize database and run migrations synchronously
        manager.init_database()?;

        Ok(manager)
    }

    fn init_database(&self) -> Result<()> {
        info!("Initializing database at {:?}", self.db_path);

        let mut conn = Connection::open(&self.db_path)?;
        conn.busy_timeout(Duration::from_secs(5))?;

        // Get current version before migration
        let version_before: i32 =
            conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
        debug!("Database version before migration: {}", version_before);

        // Apply any pending migrations
        apply_migrations(&mut conn)?;

        // Get version after migration
        let version_after: i32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;

        if version_after > version_before {
            info!(
                "Database migrated from version {} to {}",
                version_before, version_after
            );
        } else {
            debug!("Database already at latest version {}", version_after);
        }

        // Backfill metadata for legacy entries if any are missing audio duration or word count
        if let Err(e) = self.backfill_legacy_entries(&mut conn) {
            warn!("Failed to backfill legacy history entries: {}", e);
        }

        if let Err(e) = Self::repair_multi_stt_word_counts(&conn) {
            warn!("Failed to repair Multi-STT history word counts: {}", e);
        }

        Ok(())
    }

    /// Multi-STT entries re-transcribed from the History page before the
    /// `counted_word_count` fix stored the word count of the per-model dump
    /// (every model's output plus the merged text), several times the real
    /// figure, which the page then turned into an impossible WPM. Recount them
    /// from the merged text once; rows that already match are left alone.
    fn repair_multi_stt_word_counts(conn: &Connection) -> Result<()> {
        let mut stmt = conn.prepare(
            "SELECT id, transcription_text, post_processed_text, word_count FROM transcription_history WHERE mode = 'multi_stt' AND post_processed_text IS NOT NULL",
        )?;
        let rows: Vec<(i32, String, Option<String>, Option<i32>)> = stmt
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })?
            .filter_map(|res| res.ok())
            .collect();
        drop(stmt);

        let mut repaired = 0usize;
        for (id, transcription_text, post_processed_text, stored) in rows {
            let expected = counted_word_count(
                &transcription_text,
                post_processed_text.as_deref(),
                Some("multi_stt"),
            );
            if expected.is_some() && expected != stored {
                conn.execute(
                    "UPDATE transcription_history SET word_count = ?1 WHERE id = ?2",
                    params![expected, id],
                )?;
                repaired += 1;
            }
        }
        if repaired > 0 {
            info!("Repaired the word count of {repaired} Multi-STT history entries");
        }
        Ok(())
    }

    fn backfill_legacy_entries(&self, conn: &mut Connection) -> Result<()> {
        // Only rows the backfill can still complete. A failed transcription is
        // saved with empty text, so its word count stays NULL for good; once
        // its durations are filled it must not be selected (and its WAV
        // re-decoded) on every launch, on the startup path.
        let mut stmt = conn.prepare(
            "SELECT id, file_name, transcription_text, post_processed_text,
                    audio_duration_ms IS NULL OR speech_duration_ms IS NULL AS needs_audio
             FROM transcription_history
             WHERE audio_duration_ms IS NULL OR speech_duration_ms IS NULL
                OR (word_count IS NULL
                    AND trim(COALESCE(post_processed_text, transcription_text)) != '')",
        )?;

        let entries_to_update: Vec<(i32, String, String, Option<String>, bool)> = stmt
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })?
            .filter_map(|res| res.ok())
            .collect();

        if !entries_to_update.is_empty() {
            debug!(
                "Backfilling metadata for {} legacy history entries",
                entries_to_update.len()
            );

            for (id, file_name, transcription_text, post_processed_text, needs_audio) in
                entries_to_update
            {
                let wav_path = self.recordings_dir.join(&file_name);
                let (audio_duration_ms, speech_duration_ms, sample_rate_hz) =
                    if needs_audio && wav_path.exists() {
                        if let Ok(samples_16k) = crate::audio_toolkit::read_wav_samples(&wav_path) {
                            let dur = (samples_16k.len() as f64 * 1000.0) / 16000.0;
                            let speech_dur =
                                if let Ok(mut vad) = crate::audio_toolkit::EarshotVad::new(0.5) {
                                    let mut voiced = 0;
                                    let total = samples_16k.len()
                                        / crate::audio_toolkit::vad::earshot::EARSHOT_FRAME_SAMPLES;
                                    #[allow(clippy::chunks_exact_to_as_chunks)]
                                    for chunk in samples_16k.chunks_exact(
                                        crate::audio_toolkit::vad::earshot::EARSHOT_FRAME_SAMPLES,
                                    ) {
                                        if let Ok(frame) = vad.push_frame(chunk)
                                            && frame.is_speech()
                                        {
                                            voiced += 1;
                                        }
                                    }
                                    if total > 0 && voiced > 0 {
                                        (voiced as f64) * 16.0
                                    } else {
                                        dur
                                    }
                                } else {
                                    dur
                                };
                            (Some(dur), Some(speech_dur), Some(16000))
                        } else {
                            (None, None, None)
                        }
                    } else {
                        (None, None, None)
                    };

                let text_for_words = post_processed_text.unwrap_or(transcription_text);
                let word_count = if !text_for_words.trim().is_empty() {
                    Some(text_for_words.split_whitespace().count() as i32)
                } else {
                    None
                };

                let _ = conn.execute(
                    "UPDATE transcription_history SET audio_duration_ms = COALESCE(audio_duration_ms, ?1), speech_duration_ms = COALESCE(speech_duration_ms, ?2), sample_rate_hz = COALESCE(sample_rate_hz, ?3), word_count = COALESCE(word_count, ?4) WHERE id = ?5",
                    rusqlite::params![audio_duration_ms, speech_duration_ms, sample_rate_hz, word_count, id],
                );
            }
        }

        // Also backfill transcription_statistics to ensure audio_duration_ms stores silence-removed speech duration
        let _ = conn.execute(
            "UPDATE transcription_statistics
             SET audio_duration_ms = (
                 SELECT CAST(ROUND(h.speech_duration_ms) AS INTEGER)
                 FROM transcription_history h
                 WHERE h.id = transcription_statistics.source_history_id AND h.speech_duration_ms IS NOT NULL AND h.speech_duration_ms > 0
             )
             WHERE source_history_id IS NOT NULL
               AND EXISTS (
                 SELECT 1 FROM transcription_history h
                 WHERE h.id = transcription_statistics.source_history_id AND h.speech_duration_ms IS NOT NULL AND h.speech_duration_ms > 0
               )",
            [],
        );

        Ok(())
    }

    fn get_connection(&self) -> Result<Connection> {
        let conn = Connection::open(&self.db_path)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        Ok(conn)
    }

    fn map_history_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<HistoryEntry> {
        let extra_models_str: Option<String> = row.get("extra_models").unwrap_or(None);
        let extra_models: Option<Vec<String>> =
            extra_models_str.and_then(|s| serde_json::from_str(&s).ok());

        let transcription_text: String = row.get("transcription_text")?;
        let post_processed_text: Option<String> = row.get("post_processed_text")?;
        let word_count: Option<i32> = row.get("word_count").unwrap_or(None).or_else(|| {
            let active_text = post_processed_text.as_ref().unwrap_or(&transcription_text);
            if !active_text.trim().is_empty() {
                Some(active_text.split_whitespace().count() as i32)
            } else {
                None
            }
        });

        Ok(HistoryEntry {
            id: row.get("id")?,
            file_name: row.get("file_name")?,
            timestamp: row.get("timestamp")?,
            saved: row.get("saved")?,
            title: row.get("title")?,
            transcription_text,
            post_processed_text,
            post_process_prompt: row.get("post_process_prompt")?,
            post_process_requested: row.get("post_process_requested")?,
            model_id: row.get("model_id").unwrap_or(None),
            engine: row.get("engine").unwrap_or(None),
            audio_duration_ms: row.get("audio_duration_ms").unwrap_or(None),
            speech_duration_ms: row.get("speech_duration_ms").unwrap_or(None),
            sample_rate_hz: row.get("sample_rate_hz").unwrap_or(None),
            word_count,
            transcription_latency_ms: row.get("transcription_latency_ms").unwrap_or(None),
            post_processing_latency_ms: row.get("post_processing_latency_ms").unwrap_or(None),
            language: row.get("language").unwrap_or(None),
            mode: row.get("mode").unwrap_or(None),
            extra_models,
        })
    }

    pub fn recordings_dir(&self) -> &std::path::Path {
        &self.recordings_dir
    }

    pub fn database_path(&self) -> &std::path::Path {
        &self.db_path
    }

    /// Save a new history entry to the database.
    /// The WAV file should already have been written to the recordings directory.
    pub fn save_entry(
        &self,
        file_name: String,
        transcription_text: String,
        post_process_requested: bool,
        post_processed_text: Option<String>,
        post_process_prompt: Option<String>,
    ) -> Result<HistoryEntry> {
        let word_count = (!transcription_text.trim().is_empty())
            .then(|| transcription_text.split_whitespace().count() as i32);
        self.save_entry_full(NewHistoryEntry {
            file_name,
            transcription_text,
            post_process_requested,
            post_processed_text,
            post_process_prompt,
            word_count,
            ..Default::default()
        })
    }

    /// Save a complete history entry with all recording, audio, latency, and model metadata.
    pub fn save_entry_full(&self, new_entry: NewHistoryEntry) -> Result<HistoryEntry> {
        let timestamp = Utc::now().timestamp() as f64;
        let title = self.format_timestamp_title(timestamp);
        let extra_models_json = new_entry
            .extra_models
            .as_ref()
            .and_then(|m| serde_json::to_string(m).ok());
        let word_count = new_entry.word_count.or_else(|| {
            (!new_entry.transcription_text.trim().is_empty())
                .then(|| new_entry.transcription_text.split_whitespace().count() as i32)
        });

        let conn = self.get_connection()?;
        conn.execute(
            "INSERT INTO transcription_history (
                file_name,
                timestamp,
                saved,
                title,
                transcription_text,
                post_processed_text,
                post_process_prompt,
                post_process_requested,
                model_id,
                engine,
                audio_duration_ms,
                speech_duration_ms,
                sample_rate_hz,
                word_count,
                transcription_latency_ms,
                post_processing_latency_ms,
                language,
                mode,
                extra_models
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
            params![
                &new_entry.file_name,
                timestamp as i64,
                false,
                &title,
                &new_entry.transcription_text,
                &new_entry.post_processed_text,
                &new_entry.post_process_prompt,
                new_entry.post_process_requested,
                &new_entry.model_id,
                &new_entry.engine,
                new_entry.audio_duration_ms,
                new_entry.speech_duration_ms,
                new_entry.sample_rate_hz,
                word_count,
                new_entry.transcription_latency_ms,
                new_entry.post_processing_latency_ms,
                &new_entry.language,
                &new_entry.mode,
                &extra_models_json,
            ],
        )?;

        let entry = HistoryEntry {
            id: conn.last_insert_rowid() as i32,
            file_name: new_entry.file_name,
            timestamp,
            saved: false,
            title,
            transcription_text: new_entry.transcription_text,
            post_processed_text: new_entry.post_processed_text,
            post_process_prompt: new_entry.post_process_prompt,
            post_process_requested: new_entry.post_process_requested,
            model_id: new_entry.model_id,
            engine: new_entry.engine,
            audio_duration_ms: new_entry.audio_duration_ms,
            speech_duration_ms: new_entry.speech_duration_ms,
            sample_rate_hz: new_entry.sample_rate_hz,
            word_count,
            transcription_latency_ms: new_entry.transcription_latency_ms,
            post_processing_latency_ms: new_entry.post_processing_latency_ms,
            language: new_entry.language,
            mode: new_entry.mode,
            extra_models: new_entry.extra_models,
        };

        debug!("Saved history entry with id {}", entry.id);

        // The row is already stored: a failed retention pass must not turn the
        // save into an error or keep the History page from hearing about it.
        if let Err(e) = self.cleanup_old_entries() {
            error!("History retention cleanup failed: {e}");
        }

        // Emit typed event for real-time frontend updates
        if let Err(e) = (HistoryUpdatePayload::Added {
            entry: entry.clone(),
        })
        .emit(&self.app_handle)
        {
            error!("Failed to emit history-updated event: {}", e);
        }

        Ok(entry)
    }

    /// Update an existing history entry with complete updated details.
    #[allow(clippy::too_many_arguments)]
    pub fn update_entry_full(
        &self,
        id: i32,
        transcription_text: String,
        post_processed_text: Option<String>,
        post_process_prompt: Option<String>,
        post_process_requested: Option<bool>,
        model_id: Option<String>,
        engine: Option<String>,
        transcription_latency_ms: Option<f64>,
        post_processing_latency_ms: Option<f64>,
        mode: Option<String>,
        extra_models: Option<Vec<String>>,
    ) -> Result<HistoryEntry> {
        let extra_models_json = extra_models
            .as_ref()
            .and_then(|m| serde_json::to_string(m).ok());
        let conn = self.get_connection()?;
        // Callers that don't change the mode pass None; the word count must
        // still follow the entry's real mode, or a post-process pass over a
        // Multi-STT entry would count the per-model dump again.
        let effective_mode: Option<String> = match &mode {
            Some(m) => Some(m.clone()),
            None => conn
                .query_row(
                    "SELECT mode FROM transcription_history WHERE id = ?1",
                    params![id],
                    |row| row.get::<_, Option<String>>(0),
                )
                .ok()
                .flatten(),
        };
        let word_count = counted_word_count(
            &transcription_text,
            post_processed_text.as_deref(),
            effective_mode.as_deref(),
        );

        let updated = conn.execute(
            "UPDATE transcription_history
             SET transcription_text = ?1,
                 post_processed_text = ?2,
                 post_process_prompt = ?3,
                 post_process_requested = COALESCE(?4, post_process_requested),
                 model_id = COALESCE(?5, model_id),
                 engine = COALESCE(?6, engine),
                 word_count = COALESCE(?7, word_count),
                 transcription_latency_ms = COALESCE(?8, transcription_latency_ms),
                 post_processing_latency_ms = COALESCE(?9, post_processing_latency_ms),
                 mode = COALESCE(?10, mode),
                 extra_models = COALESCE(?11, extra_models)
             WHERE id = ?12",
            params![
                transcription_text,
                post_processed_text,
                post_process_prompt,
                post_process_requested,
                model_id,
                engine,
                word_count,
                transcription_latency_ms,
                post_processing_latency_ms,
                mode,
                extra_models_json,
                id
            ],
        )?;

        if updated == 0 {
            return Err(anyhow!("History entry {} not found", id));
        }

        let query = format!("SELECT {HISTORY_COLUMNS} FROM transcription_history WHERE id = ?1");
        let entry: HistoryEntry = conn.query_row(&query, params![id], Self::map_history_entry)?;

        debug!("Updated transcription for history entry {}", id);

        if let Err(e) = (HistoryUpdatePayload::Updated {
            entry: entry.clone(),
        })
        .emit(&self.app_handle)
        {
            error!("Failed to emit history-updated event: {}", e);
        }

        Ok(entry)
    }

    pub fn cleanup_old_entries(&self) -> Result<()> {
        let retention_period = crate::settings::get_recording_retention_period(&self.app_handle);

        match retention_period {
            crate::settings::RecordingRetentionPeriod::Never => {
                // Don't delete anything
                Ok(())
            }
            crate::settings::RecordingRetentionPeriod::PreserveLimit => {
                // Use the old count-based logic with history_limit
                let limit = crate::settings::get_history_limit(&self.app_handle);
                self.cleanup_by_count(limit)
            }
            _ => {
                // Use time-based logic
                self.cleanup_by_time(retention_period)
            }
        }
    }

    /// Delete the given rows and their WAV files. Returns the number of rows
    /// deleted (a row whose recording is already gone still counts).
    fn delete_entries_and_files(&self, entries: &[(i32, String)]) -> Result<u32> {
        if entries.is_empty() {
            return Ok(0);
        }

        let conn = self.get_connection()?;
        let mut deleted_count = 0;
        let mut files_removed = 0;

        for (id, file_name) in entries {
            // Delete database entry
            conn.execute(
                "DELETE FROM transcription_history WHERE id = ?1",
                params![id],
            )?;
            deleted_count += 1;

            // Delete WAV file
            let file_path = self.recordings_dir.join(file_name);
            if file_path.exists() {
                if let Err(e) = fs::remove_file(&file_path) {
                    error!("Failed to delete WAV file {}: {}", file_name, e);
                } else {
                    debug!("Deleted old WAV file: {}", file_name);
                    files_removed += 1;
                }
            }
        }

        if let Err(e) = conn.execute("VACUUM", []) {
            error!("Failed to VACUUM database after deleting entries: {}", e);
        }
        debug!("Deleted {deleted_count} history entries and {files_removed} recording files");

        Ok(deleted_count)
    }

    fn cleanup_by_count(&self, limit: u32) -> Result<()> {
        let conn = self.get_connection()?;

        // Get all entries that are not saved, ordered by timestamp desc
        let mut stmt = conn.prepare(
            "SELECT id, file_name FROM transcription_history WHERE saved = 0 ORDER BY timestamp DESC"
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, i32>("id")?, row.get::<_, String>("file_name")?))
        })?;

        let mut entries: Vec<(i32, String)> = Vec::new();
        for row in rows {
            entries.push(row?);
        }

        if entries.len() > limit as usize {
            let entries_to_delete = &entries[limit as usize..];
            let deleted_count = self.delete_entries_and_files(entries_to_delete)?;
            if deleted_count > 0 {
                debug!(
                    "Cleaned up {} old history entries exceeding limit of {}",
                    deleted_count, limit
                );
            }
        }

        Ok(())
    }

    fn cleanup_by_time(&self, period: crate::settings::RecordingRetentionPeriod) -> Result<()> {
        let conn = self.get_connection()?;
        let now = Utc::now();

        let cutoff_date = match period {
            crate::settings::RecordingRetentionPeriod::Days3 => now - chrono::Duration::days(3),
            crate::settings::RecordingRetentionPeriod::Weeks2 => now - chrono::Duration::weeks(2),
            crate::settings::RecordingRetentionPeriod::Months3 => {
                now - chrono::Duration::days(90) // Approximate 3 months
            }
            _ => return Ok(()), // Never or PreserveLimit handled elsewhere
        };

        let cutoff_timestamp = cutoff_date.timestamp() as f64;

        // Get all entries older than cutoff that are not saved
        let mut stmt = conn.prepare(
            "SELECT id, file_name FROM transcription_history WHERE timestamp < ?1 AND saved = 0",
        )?;

        let rows = stmt.query_map(params![cutoff_timestamp as i64], |row| {
            Ok((row.get::<_, i32>("id")?, row.get::<_, String>("file_name")?))
        })?;

        let mut entries_to_delete = Vec::new();
        for row in rows {
            entries_to_delete.push(row?);
        }

        let deleted_count = self.delete_entries_and_files(&entries_to_delete)?;

        if deleted_count > 0 {
            debug!(
                "Cleaned up {} old history entries based on retention period",
                deleted_count
            );
        }

        Ok(())
    }

    pub async fn get_history_entries(
        &self,
        cursor: Option<i32>,
        limit: Option<u32>,
    ) -> Result<PaginatedHistory> {
        let conn = self.get_connection()?;
        let limit = limit.map(|l| l.min(100));

        let mut entries: Vec<HistoryEntry> = match (cursor, limit) {
            (Some(cursor_id), Some(lim)) => {
                let fetch_count = (lim + 1) as i32;
                let query = format!(
                    "SELECT {HISTORY_COLUMNS}
                     FROM transcription_history
                     WHERE id < ?1
                     ORDER BY id DESC
                     LIMIT ?2"
                );
                let mut stmt = conn.prepare(&query)?;
                stmt.query_map(params![cursor_id, fetch_count], Self::map_history_entry)?
                    .collect::<std::result::Result<Vec<_>, _>>()?
            }
            (None, Some(lim)) => {
                let fetch_count = (lim + 1) as i32;
                let query = format!(
                    "SELECT {HISTORY_COLUMNS}
                     FROM transcription_history
                     ORDER BY id DESC
                     LIMIT ?1"
                );
                let mut stmt = conn.prepare(&query)?;
                stmt.query_map(params![fetch_count], Self::map_history_entry)?
                    .collect::<std::result::Result<Vec<_>, _>>()?
            }
            (_, None) => {
                let query = format!(
                    "SELECT {HISTORY_COLUMNS}
                     FROM transcription_history
                     ORDER BY id DESC"
                );
                let mut stmt = conn.prepare(&query)?;
                stmt.query_map([], Self::map_history_entry)?
                    .collect::<std::result::Result<Vec<_>, _>>()?
            }
        };

        let has_more = limit.is_some_and(|lim| entries.len() > lim as usize);
        if has_more {
            entries.pop();
        }

        Ok(PaginatedHistory { entries, has_more })
    }

    #[cfg(test)]
    fn get_latest_entry_with_conn(conn: &Connection) -> Result<Option<HistoryEntry>> {
        let query = format!(
            "SELECT {HISTORY_COLUMNS}
             FROM transcription_history
             ORDER BY timestamp DESC
             LIMIT 1"
        );
        let mut stmt = conn.prepare(&query)?;

        let entry = stmt.query_row([], Self::map_history_entry).optional()?;
        Ok(entry)
    }

    /// Get the latest entry with non-empty transcription text.
    pub fn get_latest_completed_entry(&self) -> Result<Option<HistoryEntry>> {
        let conn = self.get_connection()?;
        Self::get_latest_completed_entry_with_conn(&conn)
    }

    fn get_latest_completed_entry_with_conn(conn: &Connection) -> Result<Option<HistoryEntry>> {
        let query = format!(
            "SELECT {HISTORY_COLUMNS}
             FROM transcription_history
             WHERE transcription_text != ''
             ORDER BY timestamp DESC
             LIMIT 1"
        );
        let mut stmt = conn.prepare(&query)?;

        let entry = stmt.query_row([], Self::map_history_entry).optional()?;
        Ok(entry)
    }

    pub async fn toggle_saved_status(&self, id: i32) -> Result<()> {
        let conn = self.get_connection()?;

        // Get current saved status
        let current_saved: bool = conn.query_row(
            "SELECT saved FROM transcription_history WHERE id = ?1",
            params![id],
            |row| row.get("saved"),
        )?;

        let new_saved = !current_saved;

        conn.execute(
            "UPDATE transcription_history SET saved = ?1 WHERE id = ?2",
            params![new_saved, id],
        )?;

        debug!("Toggled saved status for entry {}: {}", id, new_saved);

        // Emit history updated event
        if let Err(e) = (HistoryUpdatePayload::Toggled { id }).emit(&self.app_handle) {
            error!("Failed to emit history-updated event: {}", e);
        }

        Ok(())
    }

    pub fn get_audio_file_path(&self, file_name: &str) -> PathBuf {
        self.recordings_dir.join(file_name)
    }

    pub async fn get_entry_by_id(&self, id: i32) -> Result<Option<HistoryEntry>> {
        self.entry_by_id(id)
    }

    fn entry_by_id(&self, id: i32) -> Result<Option<HistoryEntry>> {
        let conn = self.get_connection()?;
        let query = format!("SELECT {HISTORY_COLUMNS} FROM transcription_history WHERE id = ?1");
        let mut stmt = conn.prepare(&query)?;

        let entry = stmt.query_row([id], Self::map_history_entry).optional()?;

        Ok(entry)
    }

    /// Blocking (file delete + VACUUM): callers run it on the blocking pool.
    pub fn delete_entry(&self, id: i32) -> Result<()> {
        let conn = self.get_connection()?;

        // Get the entry to find the file name
        if let Some(entry) = self.entry_by_id(id)? {
            // Delete the audio file first
            let file_path = self.get_audio_file_path(&entry.file_name);
            if file_path.exists()
                && let Err(e) = fs::remove_file(&file_path)
            {
                error!("Failed to delete audio file {}: {}", entry.file_name, e);
                // Continue with database deletion even if file deletion fails
            }
        }

        // Delete from database
        conn.execute(
            "DELETE FROM transcription_history WHERE id = ?1",
            params![id],
        )?;

        if let Err(e) = conn.execute("VACUUM", []) {
            error!(
                "Failed to VACUUM database after deleting entry {}: {}",
                id, e
            );
        }

        debug!("Deleted history entry with id: {}", id);

        // Emit history updated event
        if let Err(e) = (HistoryUpdatePayload::Deleted { id }).emit(&self.app_handle) {
            error!("Failed to emit history-updated event: {}", e);
        }

        Ok(())
    }

    /// Blocking (every recording deleted + VACUUM): callers run it on the
    /// blocking pool.
    pub fn delete_all_recordings(&self) -> Result<()> {
        // Clear all files and subdirectories in the recordings directory, keeping the folder itself
        if self.recordings_dir.exists() {
            match fs::read_dir(&self.recordings_dir) {
                Ok(entries) => {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_dir() {
                            if let Err(e) = fs::remove_dir_all(&path) {
                                error!("Failed to remove directory {:?}: {}", path, e);
                            }
                        } else if let Err(e) = fs::remove_file(&path) {
                            error!("Failed to remove file {:?}: {}", path, e);
                        }
                    }
                }
                Err(e) => {
                    error!(
                        "Failed to read recordings directory {:?}: {}",
                        self.recordings_dir, e
                    );
                }
            }
        } else {
            fs::create_dir_all(&self.recordings_dir)?;
        }

        // Clear the table. The AUTOINCREMENT sequence is deliberately kept:
        // statistics rows keep their `source_history_id`, and a reused id
        // would join an old retry run to an unrelated new recording.
        let conn = self.get_connection()?;
        conn.execute("DELETE FROM transcription_history", [])?;

        // VACUUM to shrink the database file on disk down to minimal schema size
        if let Err(e) = conn.execute("VACUUM", []) {
            error!(
                "Failed to VACUUM database after delete_all_recordings: {}",
                e
            );
        }

        debug!("Deleted all recordings and cleared transcription history database");

        // Emit history cleared event
        if let Err(e) = HistoryUpdatePayload::Cleared.emit(&self.app_handle) {
            error!("Failed to emit history cleared event: {}", e);
        }

        Ok(())
    }

    fn format_timestamp_title(&self, timestamp: f64) -> String {
        if let Some(utc_datetime) = DateTime::from_timestamp(timestamp as i64, 0) {
            // Convert UTC to local timezone
            let local_datetime = utc_datetime.with_timezone(&Local);
            local_datetime.format("%B %e, %Y - %l:%M%p").to_string()
        } else {
            format!("Recording {}", timestamp)
        }
    }
}

/// Words in the text the user actually reads. For a Multi-STT entry
/// `transcription_text` is the per-model dump (every model's output plus the
/// merged text, i.e. several times the spoken words); the merged result in
/// `post_processed_text` is the transcript. Counting the dump inflated the
/// history word count and WPM by roughly the number of models.
fn counted_word_count(
    transcription_text: &str,
    post_processed_text: Option<&str>,
    mode: Option<&str>,
) -> Option<i32> {
    let text = match (mode, post_processed_text) {
        (Some("multi_stt"), Some(merged)) => merged,
        _ => transcription_text,
    };
    (!text.trim().is_empty()).then(|| text.split_whitespace().count() as i32)
}

#[cfg(test)]
mod tests {
    #[test]
    fn multi_stt_word_count_uses_the_merged_text() {
        let dump = "=== Multi-STT Results ===\nModel 1: a\nhello world\nModel 2: b\nhello world\n=== Merged ===\nhello world";
        assert_eq!(
            super::counted_word_count(dump, Some("hello world"), Some("multi_stt")),
            Some(2)
        );
        // Other modes keep counting the raw transcription.
        assert_eq!(
            super::counted_word_count("one two three", Some("one, two, three!"), Some("single")),
            Some(3)
        );
        assert_eq!(
            super::counted_word_count("one two", Some("x"), None),
            Some(2)
        );
        assert_eq!(
            super::counted_word_count("   ", None, Some("multi_stt")),
            None
        );
    }

    use super::*;
    use rusqlite::{Connection, params};

    fn setup_conn() -> Connection {
        let mut conn = Connection::open_in_memory().expect("open in-memory db");
        apply_migrations(&mut conn).expect("apply migrations");
        conn
    }

    fn insert_entry(conn: &Connection, timestamp: f64, text: &str, post_processed: Option<&str>) {
        conn.execute(
            "INSERT INTO transcription_history (
                file_name,
                timestamp,
                saved,
                title,
                transcription_text,
                post_processed_text,
                post_process_prompt,
                post_process_requested
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                crate::app_identity::recording_file_name(timestamp as i64),
                timestamp as i64,
                false,
                format!("Recording {}", timestamp),
                text,
                post_processed,
                Option::<String>::None,
                false,
            ],
        )
        .expect("insert history entry");
    }

    #[test]
    fn get_latest_entry_returns_none_when_empty() {
        let conn = setup_conn();
        let entry = HistoryManager::get_latest_entry_with_conn(&conn).expect("fetch latest entry");
        assert!(entry.is_none());
    }

    #[test]
    fn get_latest_entry_returns_newest_entry() {
        let conn = setup_conn();
        insert_entry(&conn, 100.0, "first", None);
        insert_entry(&conn, 200.0, "second", Some("processed"));

        let entry = HistoryManager::get_latest_entry_with_conn(&conn)
            .expect("fetch latest entry")
            .expect("entry exists");

        assert!((entry.timestamp - 200.0).abs() < f64::EPSILON);
        assert_eq!(entry.transcription_text, "second");
        assert_eq!(entry.post_processed_text.as_deref(), Some("processed"));
    }

    #[test]
    fn get_latest_completed_entry_skips_empty_entries() {
        let conn = setup_conn();
        insert_entry(&conn, 100.0, "completed", None);
        insert_entry(&conn, 200.0, "", None);

        let entry = HistoryManager::get_latest_completed_entry_with_conn(&conn)
            .expect("fetch latest completed entry")
            .expect("completed entry exists");

        assert!((entry.timestamp - 100.0).abs() < f64::EPSILON);
        assert_eq!(entry.transcription_text, "completed");
    }

    #[test]
    fn clear_database_removes_all_entries_and_vacuums() {
        let conn = setup_conn();
        insert_entry(&conn, 100.0, "entry 1", None);
        insert_entry(&conn, 200.0, "entry 2", None);

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM transcription_history", [], |row| {
                row.get(0)
            })
            .expect("count entries before clear");
        assert_eq!(count, 2);

        conn.execute("DELETE FROM transcription_history", [])
            .expect("delete all entries");
        conn.execute("VACUUM", []).expect("vacuum database");

        let count_after: i64 = conn
            .query_row("SELECT COUNT(*) FROM transcription_history", [], |row| {
                row.get(0)
            })
            .expect("count entries after clear");
        assert_eq!(count_after, 0);

        let latest = HistoryManager::get_latest_entry_with_conn(&conn).expect("fetch latest entry");
        assert!(latest.is_none());

        // Ids are never reused after a clear: statistics rows still refer to
        // the old ones through `source_history_id`.
        insert_entry(&conn, 300.0, "entry 3", None);
        assert_eq!(conn.last_insert_rowid(), 3);
    }
}
