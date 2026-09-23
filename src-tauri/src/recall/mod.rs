//! Recall — the note vault (fork feature).
//!
//! A plain folder of Markdown files: one note per `.md` file under
//! `notes/`, with the recordings behind them copied into `audio/`
//! (encrypted with the notes when the vault is). There is deliberately no
//! database: the folder *is* the
//! format, greppable, syncable and readable with any editor, and the
//! derived `index.json` is a cache that can be deleted at any time.
//!
//! Frontmatter is a hand-rolled `key: value` block between `---` fences —
//! small enough that a YAML dependency would out-weigh it, and every key
//! this module writes is one it also parses.

pub mod crypto;
pub mod dictate;
pub mod insertion;

use chrono::{Local, Utc};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::AppHandle;

use crate::settings::RecallSettings;

/// Frontmatter keys, in the order they are written.
const KEY_TITLE: &str = "title";
const KEY_CREATED: &str = "created";
const KEY_UPDATED: &str = "updated";
const KEY_TAGS: &str = "tags";
const KEY_SOURCE: &str = "source";
const KEY_AUDIO: &str = "audio";

/// Sub-folder of the vault the note files live in.
pub const NOTES_SUBDIR: &str = "notes";
/// Sub-folder holding (or linking to) the recordings behind the notes.
pub const AUDIO_SUBDIR: &str = "audio";
/// The derived, rebuildable index file. Plaintext metadata by definition,
/// so enabling encryption removes it and disabling rebuilds it.
pub(crate) const INDEX_FILE: &str = "index.json";

/// One note's metadata as the UI lists it.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type)]
pub struct RecallNoteMeta {
    /// Stable file identity: the `.md` file's stem under `notes/`.
    pub id: String,
    pub title: String,
    /// Epoch milliseconds.
    pub created_ms: i64,
    /// Epoch milliseconds.
    pub updated_ms: i64,
    pub tags: Vec<String>,
    pub word_count: u32,
    /// What produced the note: a model id, `"dictation"`, `"manual"`, …
    pub source: Option<String>,
    /// Recording file name in `audio/` (or a history file name) the note
    /// was spoken from, if any.
    pub audio_file: Option<String>,
}

/// A note opened for reading or editing: metadata plus the Markdown body.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type)]
pub struct RecallNoteContent {
    pub meta: RecallNoteMeta,
    pub body: String,
}

/// What the vault looks like on disk right now.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type)]
pub struct RecallVaultInfo {
    pub root: String,
    pub note_count: u32,
    pub audio_count: u32,
}

// ---------------------------------------------------------------------------
// paths
// ---------------------------------------------------------------------------

/// The vault root: the configured folder, else `<app data>/recall`.
pub fn vault_root(app: &AppHandle, settings: &RecallSettings) -> Result<PathBuf, String> {
    match &settings.output_dir {
        Some(dir) => Ok(PathBuf::from(dir)),
        None => crate::portable::app_data_dir(app)
            .map(|base| base.join("recall"))
            .map_err(|e| format!("Cannot resolve app data dir: {e}")),
    }
}

/// The folder used when none is configured — for display in the UI.
pub fn default_vault_root(app: &AppHandle) -> Result<PathBuf, String> {
    vault_root(app, &RecallSettings::default())
}

pub(crate) fn notes_dir(root: &Path) -> PathBuf {
    root.join(NOTES_SUBDIR)
}

fn note_path(root: &Path, id: &str) -> PathBuf {
    notes_dir(root).join(format!("{id}.md"))
}

/// Reject ids that could escape `notes/`: a plain file stem only.
fn sanitize_id(id: &str) -> Result<&str, String> {
    let id = id.trim();
    let ok = !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && !id.starts_with('.')
        && !is_windows_device_name(id);
    if ok {
        Ok(id)
    } else {
        Err(format!("Invalid note id: {id:?}"))
    }
}

fn ensure_dirs(root: &Path) -> Result<(), String> {
    fs::create_dir_all(notes_dir(root))
        .map_err(|e| format!("Failed to create notes folder: {e}"))?;
    fs::create_dir_all(root.join(AUDIO_SUBDIR))
        .map_err(|e| format!("Failed to create audio folder: {e}"))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// frontmatter
// ---------------------------------------------------------------------------

/// Minimal slug: lowercase, non-alphanumerics folded to `-`, trimmed.
fn slugify(text: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = true;
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        "note".to_string()
    } else {
        slug
    }
}

/// Windows refuses file names whose base is a device, extension or not —
/// an id of `aux` would create an unwritable `aux.md`.
fn is_windows_device_name(id: &str) -> bool {
    let stem = id
        .split('.')
        .next()
        .unwrap_or(id)
        .trim()
        .to_ascii_lowercase();
    matches!(
        stem.as_str(),
        "con"
            | "prn"
            | "aux"
            | "nul"
            | "com1"
            | "com2"
            | "com3"
            | "com4"
            | "com5"
            | "com6"
            | "com7"
            | "com8"
            | "com9"
            | "lpt1"
            | "lpt2"
            | "lpt3"
            | "lpt4"
            | "lpt5"
            | "lpt6"
            | "lpt7"
            | "lpt8"
            | "lpt9"
    )
}

fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

/// `YYYY-MM-DD` in the user's local date — the prefix every new note gets,
/// so the folder reads like a journal in any file manager.
fn local_date_prefix() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

/// A unique id for a new note: `YYYY-MM-DD-slug`, `-2`, `-3`, … on
/// collision — in either state of the vault, so an encrypted vault never
/// reuses a `.rcl` stem either.
fn fresh_note_id(root: &Path, title: &str) -> String {
    let base = format!("{}-{}", local_date_prefix(), slugify(title));
    let mut candidate = base.clone();
    let mut n = 2;
    while note_path(root, &candidate).exists()
        || crypto::encrypted_note_path(root, &candidate).exists()
    {
        candidate = format!("{base}-{n}");
        n += 1;
    }
    candidate
}

/// Frontmatter values are single lines: a newline inside a title would
/// otherwise end the key and start a bogus one.
fn escape_value(value: &str) -> String {
    value.replace('\n', " ")
}

fn serialize_note(meta: &RecallNoteMeta, body: &str) -> String {
    let mut out = String::from("---\n");
    out.push_str(&format!("{KEY_TITLE}: {}\n", escape_value(&meta.title)));
    out.push_str(&format!("{KEY_CREATED}: {}\n", meta.created_ms));
    out.push_str(&format!("{KEY_UPDATED}: {}\n", meta.updated_ms));
    if !meta.tags.is_empty() {
        out.push_str(&format!("{KEY_TAGS}: {}\n", meta.tags.join(", ")));
    }
    if let Some(source) = &meta.source {
        out.push_str(&format!("{KEY_SOURCE}: {}\n", escape_value(source)));
    }
    if let Some(audio) = &meta.audio_file {
        out.push_str(&format!("{KEY_AUDIO}: {}\n", escape_value(audio)));
    }
    out.push_str("---\n\n");
    out.push_str(body.trim_end());
    out.push('\n');
    out
}

/// Parse `key: value` frontmatter plus the body. Anything missing falls back
/// to sane defaults derived from the file itself, so a hand-written or
/// externally edited note still opens.
fn parse_note(id: &str, raw: &str, file_updated_ms: i64) -> RecallNoteContent {
    let mut title = String::new();
    let mut created_ms: Option<i64> = None;
    let mut updated_ms: Option<i64> = None;
    let mut tags: Vec<String> = Vec::new();
    let mut source: Option<String> = None;
    let mut audio_file: Option<String> = None;
    let mut body = raw;

    if let Some(rest) = raw.strip_prefix("---\n")
        && let Some(end) = rest.find("\n---")
    {
        let block = &rest[..end];
        body = rest[end + 4..].trim_start_matches('\n');
        for line in block.lines() {
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            // Strip one surrounding pair of quotes only: a value that merely
            // starts or ends with a quote (`He said "hi"`) must round-trip.
            let value = value.trim();
            let value = value
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .unwrap_or(value);
            match key.trim() {
                KEY_TITLE => title = value.to_string(),
                KEY_CREATED => created_ms = value.parse().ok(),
                KEY_UPDATED => updated_ms = value.parse().ok(),
                KEY_TAGS => {
                    tags = value
                        .split(',')
                        .map(|t| t.trim().to_string())
                        .filter(|t| !t.is_empty())
                        .collect();
                }
                KEY_SOURCE => source = Some(value.to_string()),
                KEY_AUDIO => audio_file = Some(value.to_string()),
                _ => {}
            }
        }
    }

    if title.is_empty() {
        title = id.to_string();
    }

    RecallNoteContent {
        meta: RecallNoteMeta {
            id: id.to_string(),
            title,
            created_ms: created_ms.unwrap_or(file_updated_ms),
            updated_ms: updated_ms.unwrap_or(file_updated_ms),
            tags,
            word_count: count_words(body),
            source,
            audio_file,
        },
        body: body.to_string(),
    }
}

fn count_words(text: &str) -> u32 {
    text.split_whitespace().count() as u32
}

fn file_updated_ms(path: &Path) -> i64 {
    path.metadata()
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// CRUD
// ---------------------------------------------------------------------------

/// Every note in the vault, newest update first. A file that fails to parse
/// is skipped rather than failing the whole listing — one bad note must not
/// hide the rest.
pub fn list_notes(root: &Path) -> Result<Vec<RecallNoteMeta>, String> {
    let dir = notes_dir(root);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut notes = Vec::new();
    let entries = fs::read_dir(&dir).map_err(|e| format!("Failed to read vault: {e}"))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let Some(id) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Ok(raw) = fs::read_to_string(&path) else {
            continue;
        };
        notes.push(parse_note(id, &raw, file_updated_ms(&path)).meta);
    }
    notes.sort_by(|a, b| b.updated_ms.cmp(&a.updated_ms).then(a.id.cmp(&b.id)));
    Ok(notes)
}

pub fn read_note(root: &Path, id: &str) -> Result<RecallNoteContent, String> {
    let id = sanitize_id(id)?;
    let path = note_path(root, id);
    let raw = fs::read_to_string(&path).map_err(|e| format!("Failed to read note {id}: {e}"))?;
    Ok(parse_note(id, &raw, file_updated_ms(&path)))
}

/// Metadata of a new, empty note (`create_note` / `create_note_any`).
fn created_meta(root: &Path, title: &str, tags: &[String]) -> RecallNoteMeta {
    let title = title.trim();
    let title = if title.is_empty() { "Untitled" } else { title };
    let now = now_ms();
    RecallNoteMeta {
        id: fresh_note_id(root, title),
        title: title.to_string(),
        created_ms: now,
        updated_ms: now,
        tags: tags.to_vec(),
        word_count: 0,
        source: Some("manual".to_string()),
        audio_file: None,
    }
}

/// Metadata of an edited note: an empty title keeps the existing one, and
/// creation time, source and recording carry over.
fn updated_meta(
    id: &str,
    existing: RecallNoteMeta,
    title: &str,
    tags: &[String],
    body: &str,
) -> RecallNoteMeta {
    let title = title.trim();
    RecallNoteMeta {
        id: id.to_string(),
        title: if title.is_empty() {
            existing.title
        } else {
            title.to_string()
        },
        created_ms: existing.created_ms,
        updated_ms: now_ms(),
        tags: tags.to_vec(),
        word_count: count_words(body),
        source: existing.source,
        audio_file: existing.audio_file,
    }
}

/// A saved transcription's title: the one given, else the text's first line
/// (up to 60 characters), else `Transcription`.
fn guess_title(title: Option<String>, text: &str) -> String {
    title
        .filter(|t| !t.trim().is_empty())
        .map(|t| t.trim().to_string())
        .unwrap_or_else(|| {
            let first_line = text.lines().next().unwrap_or("").trim();
            let guessed: String = first_line.chars().take(60).collect();
            if guessed.is_empty() {
                "Transcription".to_string()
            } else {
                guessed
            }
        })
}

/// Metadata of a saved transcription (`save_transcription_any`).
fn transcription_meta(
    root: &Path,
    text: &str,
    title: String,
    source: Option<String>,
    audio_file: Option<String>,
    tags: &[String],
) -> RecallNoteMeta {
    let now = now_ms();
    RecallNoteMeta {
        id: fresh_note_id(root, &title),
        title,
        created_ms: now,
        updated_ms: now,
        tags: tags.to_vec(),
        word_count: count_words(text),
        source: source.or_else(|| Some("transcription".to_string())),
        audio_file,
    }
}

/// Write a note to a plain vault and refresh the derived index.
fn persist_plain(root: &Path, meta: &RecallNoteMeta, body: &str) -> Result<(), String> {
    write_note_file(root, meta, body)?;
    rebuild_index(root)
}

/// Write a note in whichever form the vault holds: plain Markdown (index
/// refreshed), or ciphertext under the session key (no index to refresh).
fn persist(root: &Path, meta: &RecallNoteMeta, body: &str) -> Result<(), String> {
    if !crypto::is_encrypted(root) {
        return persist_plain(root, meta, body);
    }
    let key = crypto::session_key()?;
    let raw = serialize_note(meta, body);
    crypto::write_note_encrypted(root, &meta.id, &key, raw.as_bytes())
}

pub fn create_note(root: &Path, title: &str, tags: &[String]) -> Result<RecallNoteMeta, String> {
    ensure_dirs(root)?;
    let meta = created_meta(root, title, tags);
    persist_plain(root, &meta, "")?;
    Ok(meta)
}

pub fn write_note(
    root: &Path,
    id: &str,
    title: &str,
    tags: &[String],
    body: &str,
) -> Result<RecallNoteMeta, String> {
    let id = sanitize_id(id)?;
    let existing = read_note(root, id)?;
    let meta = updated_meta(id, existing.meta, title, tags, body);
    persist_plain(root, &meta, body)?;
    Ok(meta)
}

pub fn delete_note(root: &Path, id: &str) -> Result<(), String> {
    let id = sanitize_id(id)?;
    let path = note_path(root, id);
    fs::remove_file(path).map_err(|e| format!("Failed to delete note {id}: {e}"))?;
    rebuild_index(root)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// encryption-aware CRUD
//
// The `*_any` layer is what the commands call: it dispatches on the vault's
// state — plain Markdown, encrypted-and-unlocked (session key held), or
// encrypted-and-locked (reads collapse to opaque metadata, writes refuse).
// The plaintext functions above stay the single implementation of the file
// format; encryption wraps it at the byte level in `crypto`.
// ---------------------------------------------------------------------------

/// A note the session cannot decrypt. The id is all a locked vault can
/// honestly show: everything else lives inside the ciphertext.
fn locked_meta(id: &str) -> RecallNoteMeta {
    RecallNoteMeta {
        id: id.to_string(),
        title: id.to_string(),
        created_ms: 0,
        updated_ms: 0,
        tags: Vec::new(),
        word_count: 0,
        source: None,
        audio_file: None,
    }
}

fn note_bytes_to_content(id: &str, bytes: &[u8], root: &Path) -> Result<RecallNoteContent, String> {
    let raw = String::from_utf8(bytes.to_vec())
        .map_err(|_| format!("Corrupt note {id}: not valid UTF-8"))?;
    // Only encrypted notes come through here: their file is the `.rcl`.
    Ok(parse_note(
        id,
        &raw,
        file_updated_ms(&crypto::encrypted_note_path(root, id)),
    ))
}

/// `(metas, locked)`: when locked, the metas are opaque placeholders and
/// the UI shows the unlock screen instead of the editor.
pub fn list_notes_any(root: &Path) -> Result<(Vec<RecallNoteMeta>, bool), String> {
    if crypto::is_encrypted(root) {
        match crypto::session_key() {
            Ok(key) => {
                // Like `list_notes`: one unreadable note is skipped, never
                // allowed to hide the rest of the vault.
                let mut notes = Vec::new();
                for id in crypto::note_ids_encrypted(root)? {
                    match crypto::read_note_encrypted(root, &id, &key)
                        .and_then(|bytes| note_bytes_to_content(&id, &bytes, root))
                    {
                        Ok(content) => notes.push(content.meta),
                        Err(e) => log::warn!("Recall: skipping unreadable note {id}: {e}"),
                    }
                }
                notes.sort_by(|a, b| b.updated_ms.cmp(&a.updated_ms).then(a.id.cmp(&b.id)));
                Ok((notes, false))
            }
            Err(_) => {
                let mut notes: Vec<RecallNoteMeta> = crypto::note_ids_encrypted(root)?
                    .iter()
                    .map(|id| locked_meta(id))
                    .collect();
                notes.sort_by(|a, b| a.id.cmp(&b.id));
                Ok((notes, true))
            }
        }
    } else {
        Ok((list_notes(root)?, false))
    }
}

pub fn read_note_any(root: &Path, id: &str) -> Result<RecallNoteContent, String> {
    let id = sanitize_id(id)?;
    if crypto::is_encrypted(root) {
        let key = crypto::session_key()?;
        let bytes = crypto::read_note_encrypted(root, id, &key)?;
        note_bytes_to_content(id, &bytes, root)
    } else {
        read_note(root, id)
    }
}

pub fn write_note_any(
    root: &Path,
    id: &str,
    title: &str,
    tags: &[String],
    body: &str,
) -> Result<RecallNoteMeta, String> {
    let id = sanitize_id(id)?;
    let existing = read_note_any(root, id)?;
    let meta = updated_meta(id, existing.meta, title, tags, body);
    persist(root, &meta, body)?;
    Ok(meta)
}

pub fn create_note_any(
    root: &Path,
    title: &str,
    tags: &[String],
) -> Result<RecallNoteMeta, String> {
    ensure_dirs(root)?;
    let meta = created_meta(root, title, tags);
    persist(root, &meta, "")?;
    Ok(meta)
}

/// Copy a recording into the vault's `audio/` folder, so the vault really
/// is the one place the note, its text and the voice behind it live. In an
/// encrypted vault the copy is ciphertext (`.rcl`); the note's `audio_file`
/// keeps the original name either way.
pub fn attach_audio_file(root: &Path, source: &Path) -> Result<Option<String>, String> {
    if !source.exists() {
        return Ok(None);
    }
    fs::create_dir_all(root.join(AUDIO_SUBDIR))
        .map_err(|e| format!("Failed to create audio folder: {e}"))?;
    let name = source
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .ok_or_else(|| format!("Bad recording path {}", source.display()))?;
    if crypto::is_encrypted(root) {
        let key = crypto::session_key()?;
        let bytes = fs::read(source).map_err(|e| format!("Failed to read recording: {e}"))?;
        let dest = root.join(AUDIO_SUBDIR).join(format!("{name}.rcl"));
        crypto::write_encrypted_file(&dest, &key, &bytes)?;
    } else {
        let dest = root.join(AUDIO_SUBDIR).join(&name);
        fs::copy(source, &dest).map_err(|e| format!("Failed to copy recording: {e}"))?;
    }
    Ok(Some(name))
}

/// Append transcription text as a new note — the "Save to Recall" path,
/// encryption-aware like every writer. `audio_source` is the recording the
/// text came from; it is *copied* into the vault's `audio/` folder, so the
/// vault is self-contained (and its audio encrypted with the notes).
/// `audio_file` is the frontmatter reference used when there is no copy.
pub fn save_transcription_any(
    root: &Path,
    text: &str,
    title: Option<String>,
    model_source: Option<String>,
    audio_file: Option<String>,
    tags: &[String],
    audio_source: Option<&Path>,
) -> Result<RecallNoteMeta, String> {
    ensure_dirs(root)?;
    let title = guess_title(title, text);
    let vault_audio = match audio_source {
        Some(path) => attach_audio_file(root, path)?,
        None => None,
    };
    let audio_ref = vault_audio.or(audio_file);
    let meta = transcription_meta(root, text, title, model_source, audio_ref, tags);
    persist(root, &meta, text)?;
    Ok(meta)
}

pub fn delete_note_any(root: &Path, id: &str) -> Result<(), String> {
    let id = sanitize_id(id)?;
    if crypto::is_encrypted(root) {
        // The existence check is the session check: deleting an opaque file
        // while locked is still gated on unlock, deliberately.
        let key = crypto::session_key()?;
        let _ = crypto::read_note_encrypted(root, id, &key)?;
        fs::remove_file(crypto::encrypted_note_path(root, id))
            .map_err(|e| format!("Failed to delete note {id}: {e}"))?;
        Ok(())
    } else {
        delete_note(root, id)
    }
}

fn write_note_file(root: &Path, meta: &RecallNoteMeta, body: &str) -> Result<(), String> {
    let path = note_path(root, &meta.id);
    // Temp file + rename: a crash mid-write leaves the previous note intact.
    let tmp = path.with_extension("md.tmp");
    fs::write(&tmp, serialize_note(meta, body))
        .map_err(|e| format!("Failed to write note {}: {e}", meta.id))?;
    fs::rename(&tmp, &path).map_err(|e| format!("Failed to finalize note {}: {e}", meta.id))?;
    Ok(())
}

pub fn vault_info(root: &Path) -> Result<RecallVaultInfo, String> {
    // Both extensions count: an encrypted vault's notes are `.rcl`, and the
    // counts must not silently drop to zero the moment encryption is on.
    let note_count = fs::read_dir(notes_dir(root))
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| {
                    matches!(
                        e.path().extension().and_then(|e| e.to_str()),
                        Some("md") | Some("rcl")
                    )
                })
                .count() as u32
        })
        .unwrap_or(0);
    let audio_count = fs::read_dir(root.join(AUDIO_SUBDIR))
        .map(|entries| entries.flatten().count() as u32)
        .unwrap_or(0);
    Ok(RecallVaultInfo {
        root: root.to_string_lossy().to_string(),
        note_count,
        audio_count,
    })
}

// ---------------------------------------------------------------------------
// derived index
// ---------------------------------------------------------------------------

/// Tag → note count, plus the listing itself. Rebuilt after every mutation;
/// deleting it costs nothing.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type)]
pub struct RecallIndex {
    pub generated_ms: i64,
    pub note_count: u32,
    pub tags: Vec<RecallTagCount>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type)]
pub struct RecallTagCount {
    pub tag: String,
    pub count: u32,
}

pub fn rebuild_index(root: &Path) -> Result<(), String> {
    let notes = list_notes(root)?;
    let mut counts: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    for note in &notes {
        for tag in &note.tags {
            *counts.entry(tag.clone()).or_insert(0) += 1;
        }
    }
    let mut tags: Vec<RecallTagCount> = counts
        .into_iter()
        .map(|(tag, count)| RecallTagCount { tag, count })
        .collect();
    tags.sort_by(|a, b| b.count.cmp(&a.count).then(a.tag.cmp(&b.tag)));
    let index = RecallIndex {
        generated_ms: now_ms(),
        note_count: notes.len() as u32,
        tags,
    };
    let tmp = root.join(format!("{INDEX_FILE}.tmp"));
    fs::create_dir_all(root).map_err(|e| format!("Failed to create vault: {e}"))?;
    fs::write(
        &tmp,
        serde_json::to_string_pretty(&index).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("Failed to write index: {e}"))?;
    fs::rename(&tmp, root.join(INDEX_FILE))
        .map_err(|e| format!("Failed to finalize index: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("zer0-recall-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        root
    }

    #[test]
    fn create_write_read_roundtrip() {
        let root = temp_root("roundtrip");
        let meta = create_note(&root, "Standup Notes", &["work".into(), "voice".into()]).unwrap();
        assert!(meta.id.starts_with(&format!(
            "{}-standup-notes",
            Local::now().format("%Y-%m-%d")
        )));

        write_note(
            &root,
            &meta.id,
            "Standup Notes",
            &["work".into()],
            "Hello **world**.",
        )
        .unwrap();
        let note = read_note(&root, &meta.id).unwrap();
        assert_eq!(note.meta.title, "Standup Notes");
        assert_eq!(note.meta.tags, vec!["work".to_string()]);
        assert_eq!(note.meta.word_count, 2);
        assert_eq!(note.meta.source.as_deref(), Some("manual"));
        assert_eq!(note.body, "Hello **world**.\n");

        delete_note(&root, &meta.id).unwrap();
        assert!(list_notes(&root).unwrap().is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn note_ids_are_unique_and_safe() {
        let root = temp_root("unique");
        let a = create_note(&root, "Same Title", &[]).unwrap();
        let b = create_note(&root, "Same Title", &[]).unwrap();
        assert_ne!(a.id, b.id);
        assert!(sanitize_id("../escape").is_err());
        assert!(sanitize_id(".hidden").is_err());
        assert!(sanitize_id("").is_err());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn externally_edited_note_parses_with_fallbacks() {
        let root = temp_root("freeform");
        ensure_dirs(&root).unwrap();
        fs::write(
            note_path(&root, "2026-09-14-freeform"),
            "Just a body, no frontmatter at all.\n",
        )
        .unwrap();
        let note = read_note(&root, "2026-09-14-freeform").unwrap();
        assert_eq!(note.meta.title, "2026-09-14-freeform");
        assert_eq!(note.body, "Just a body, no frontmatter at all.\n");

        fs::write(
            note_path(&root, "2026-09-14-full"),
            "---\ntitle: \"Quoted\"\ncreated: 1000\nupdated: 2000\ntags: a, b\nsource: whisper-large-turbo\naudio: zer0-1.wav\n---\n\nBody.\n",
        )
        .unwrap();
        let note = read_note(&root, "2026-09-14-full").unwrap();
        assert_eq!(note.meta.title, "Quoted");
        assert_eq!(note.meta.created_ms, 1000);
        assert_eq!(note.meta.tags, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(note.meta.source.as_deref(), Some("whisper-large-turbo"));
        assert_eq!(note.meta.audio_file.as_deref(), Some("zer0-1.wav"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn titles_with_edge_quotes_round_trip() {
        let root = temp_root("quotes");
        for title in [r#"He said "hi""#, r#""Quoted" start"#] {
            let meta = create_note(&root, title, &[]).unwrap();
            assert_eq!(read_note(&root, &meta.id).unwrap().meta.title, title);
        }
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn index_counts_tags() {
        let root = temp_root("index");
        create_note(&root, "One", &["work".into(), "voice".into()]).unwrap();
        create_note(&root, "Two", &["work".into()]).unwrap();
        let index: RecallIndex =
            serde_json::from_str(&fs::read_to_string(root.join(INDEX_FILE)).unwrap()).unwrap();
        assert_eq!(index.note_count, 2);
        assert_eq!(index.tags[0].tag, "work");
        assert_eq!(index.tags[0].count, 2);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn slugify_folds_and_falls_back() {
        assert_eq!(slugify("Héllo, Wörld!"), "h-llo-w-rld");
        assert_eq!(slugify("   "), "note");
        assert_eq!(slugify("---***---"), "note");
    }
}
