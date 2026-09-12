//! Experimental Multi-STT streaming mode: the primary streaming model gives an
//! instant rough text, and the Multi-STT pipeline replaces it in place, chunk by
//! chunk, as the speaker pauses.
//!
//! # The unit of work is a chunk
//!
//! A **chunk** is the run of speech between two long breaks — one sentence, or
//! three, or more. It is *one* merge input: the extra models need the whole
//! audio context of what was said, so three sentences are decoded as three
//! sentences, not three times one sentence. That is what makes a break
//! mid-sentence safe: the chunk stays open and accumulates, so nothing is frozen
//! while the sentence is still half-spoken.
//!
//! A chunk is bounded by **sentence ends, not by time or length**. The cut point
//! for the next chunk is a complete sentence end in it, timed into the audio. A
//! long sentence is therefore never cut to fit a limit, and nothing behind the
//! cut is ever merged twice.
//!
//! Two settings bound the window further, both read at each close:
//!
//! - `multi_stt_streaming_max_sentences` (default 3) closes a chunk once it
//!   holds that many sentence ends, so a run the speaker never pauses in is still
//!   merged in pieces of a predictable size instead of growing to the 60 s valve.
//! - `multi_stt_streaming_context_sentences` (default 1) makes the cut land that
//!   many ends *earlier* than the last one, so the merge window is
//!   `[start of the last previous sentence .. now]` rather than everything since
//!   the last cut. The carried sentences are the next window's leading context:
//!   the extras hear the whole joint, so a sentence the stream ended early at a
//!   pause can be joined to what follows it, and a pause inside a sentence
//!   polishes exactly `[previous sentence + what has been spoken]`.
//!
//! # Where the timestamps come from
//!
//! `StreamUpdate::audio_committed_ms` is the family's own statement of how much
//! audio the committed text accounts for. It is documented as a *hint*, and it
//! is captured when a sentence end is **first seen**, not when the chunk closes:
//! by close time the pause is over and the timestamp would sit past the silence,
//! so the cut would swallow the whole break. A sentence end is seen on the
//! worker thread and consumed on this module's own thread, so the capture is a
//! snapshot of the last published update rather than an exact instant — see
//! [`SENTENCE_BOUNDARY_BIAS_MS`] for how that error is made harmless.
//!
//! # Shape
//!
//! One thread per recording, ticked every [`TICK`], the shape of
//! `LiveModeManager::run_session`'s inner loop. Per tick it drains the mid-
//! recording audio tap (the copy of the frames the primary model is being fed),
//! reads the stream's latest text, rebuilds the open chunk's sentence ends,
//! tests for a break, and drives the chunk lifecycle. At most one merge job runs
//! at a time.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, LazyLock, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use log::{debug, error, info, warn};
use serde::Serialize;
use specta::Type;
use tauri::AppHandle;
use tauri_specta::Event;

use crate::actions::{MultiSttHistoryBrain, has_merge_prompt, multi_stt_merge_transcriptions};
use crate::audio_toolkit::audio::{ChunkTap, chunk_tap};
use crate::direct_stream_writer::DirectStreamWriter;
use crate::managers::audio::AudioRecordingManager;
use crate::managers::transcription::TranscriptionManager;
use crate::settings::{AppSettings, PasteMethod, get_settings};

/// The rate the tap and the streaming model both run at (see [`ChunkTap`]).
const SAMPLES_PER_MS: i64 = 16;

/// How far *before* a sentence end's reported timestamp the audio is cut.
///
/// `audio_committed_ms` is a family-reported hint with family-dependent
/// granularity, and it is read from the last published update rather than at the
/// instant the sentence end appeared. The cut is therefore biased backwards and
/// the next chunk starts at the same biased point, so the two regions **overlap**
/// by this much: an over-estimate then costs one duplicated word at the seam,
/// while an under-estimate is recovered because the next chunk still contains
/// the word. Cutting at the unbiased estimate with no forward padding would lose
/// audio from both chunks instead.
const SENTENCE_BOUNDARY_BIAS_MS: i64 = 250;

/// A chunk that has grown this long without a break is closed at its last
/// sentence end. A safety valve for someone who never pauses — deliberately not
/// a setting and deliberately not a sentence cap: it only ever cuts where a
/// sentence already ended, so it can never split one. The one exception is a
/// model that reaches this length without punctuating at all, which has no
/// sentence end to cut at (see [`Step::CloseAll`]).
const MAX_CHUNK_SECONDS: i64 = 60;

/// Below this much closed audio a cut is not worth a merge: the extras would
/// decode near-silence and could return nothing, which would delete text that is
/// already on screen. A cut that lands earlier is treated as a mid-sentence
/// break — the chunk is polished and stays open.
const MIN_CLOSE_MS: i64 = 500;

/// Tick period. 50 ms is Live Mode's cadence: fast enough that a pause is
/// noticed promptly, slow enough that the per-tick string work is irrelevant.
const TICK: Duration = Duration::from_millis(50);

/// How often the cached settings are refreshed. The only value read per tick is
/// the break threshold, and a store read at 20 Hz would be pure waste;
/// everything else is re-read at job dispatch, where it is free.
const SETTINGS_REFRESH_TICKS: u32 = 40;

/// A merge job that has run this long is abandoned: its result would land after
/// the session moved on, and a wedged provider must not stop the mode from
/// closing further chunks. The LLM client has its own request timeout; this is
/// the backstop for everything else (a stuck engine load, a blocked pool).
const MERGE_TIMEOUT: Duration = Duration::from_secs(90);

/// How long the coordinator waits for the in-flight merge at stop, and how long
/// `MultiSttAction::stop` waits for the coordinator, before falling back.
pub const FINISH_TIMEOUT: Duration = Duration::from_secs(120);

/// Emitted when a chunk's merge fails, so the main window can raise a toast.
/// Rate-limited by construction: it fires only when the number of chunks
/// currently showing a failed merge goes *up*, so a retry that fails again does
/// not repeat it.
#[derive(Clone, Serialize, Type, tauri_specta::Event)]
pub struct MultiSttStreamChunkFailedEvent {
    /// 1-based number of the chunk that failed.
    pub chunk: u32,
    /// How many chunks of the session are showing a failed merge right now.
    pub failed_chunks: u32,
}

// ---------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------

/// Byte offsets just past each sentence end in `text`, ascending.
///
/// The offset points directly after the terminator and any closing quote or
/// bracket, but *not* past the whitespace that follows it: the next chunk keeps
/// that space, so concatenating two chunks' texts reproduces the streaming
/// model's own text byte for byte.
///
/// A `.` whose *next character* is a digit is a decimal point, not a sentence
/// end — cutting inside `3.14` would split one value across two merges, and
/// unlike the abbreviation case below the model cannot recover from it. Only an
/// immediately following digit counts: in `Version 1. 2 is old` the space is
/// what separates two sentences, so skipping whitespace before the test would
/// silently merge them.
/// Abbreviations (`Mr.`, `e.g.`) are taken at face value: a cut there costs one
/// extra decode of a few seconds of audio, and the tail carries over verbatim,
/// so the text itself stays intact. That trade is deliberate and not worked
/// around.
///
/// **The offsets are byte offsets**, as everywhere in this module, so a
/// multi-byte terminator (`…`, `。`) advances by its UTF-8 width.
pub fn sentence_ends(text: &str) -> Vec<usize> {
    let mut ends = Vec::new();
    for (idx, ch) in text.char_indices() {
        if !is_terminator(ch) {
            continue;
        }
        let after = idx + ch.len_utf8();
        // Closing quotes/brackets belong to the sentence that just ended.
        let mut end = after;
        for (i, c) in text[after..].char_indices() {
            if is_closer(c) {
                end = after + i + c.len_utf8();
            } else {
                break;
            }
        }
        if ch == '.' && is_decimal_tail(&text[end..]) {
            continue;
        }
        ends.push(end);
    }
    ends
}

fn is_terminator(c: char) -> bool {
    matches!(c, '.' | '!' | '?' | '…' | '。' | '！' | '？')
}

fn is_closer(c: char) -> bool {
    matches!(
        c,
        '"' | '\'' | '”' | '’' | '»' | ')' | ']' | '}' | '）' | '】' | '」' | '』'
    )
}

fn is_decimal_tail(rest: &str) -> bool {
    rest.chars().next().is_some_and(|c| c.is_ascii_digit())
}

/// The largest index `<= at` that is a char boundary of `text`, or `text.len()`
/// when `at` is past its end.
///
/// Every offset this module keeps is a byte offset into a string that only ever
/// grows by appends, so it is always a boundary — except after a re-anchor (a
/// family that rewrote its committed text, contrary to its contract). Clamping
/// costs a byte or two in that case and avoids a panic on the coordinator
/// thread, which would leave the tap held and the overlay's text frozen.
fn clamp_boundary(text: &str, at: usize) -> usize {
    if at >= text.len() {
        return text.len();
    }
    let mut at = at;
    while at > 0 && !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// Append `piece` to `out`, inserting one space if neither side supplies one.
///
/// Merge results are trimmed and the streaming text is not, so the separator
/// cannot be left to either side: without this, `...said.` + `Hello` would run
/// together. When both sides came from the same streaming text the rule is a
/// no-op, because the split point kept the original whitespace.
fn append_join(out: &mut String, piece: &str) {
    if piece.is_empty() {
        return;
    }
    if !out.is_empty()
        && !out.ends_with(char::is_whitespace)
        && !piece.starts_with(char::is_whitespace)
    {
        out.push(' ');
    }
    out.push_str(piece);
}

/// Where a cut at `cut_ms` lands inside a chunk's audio, or `None` when it would
/// leave too little audio behind it (see [`MIN_CLOSE_MS`]) to be worth merging.
fn cut_index(audio_start_sample: i64, audio_len: usize, cut_ms: i64) -> Option<usize> {
    let want = (cut_ms - SENTENCE_BOUNDARY_BIAS_MS) * SAMPLES_PER_MS - audio_start_sample;
    if want < MIN_CLOSE_MS * SAMPLES_PER_MS {
        return None;
    }
    Some(want.min(audio_len as i64) as usize)
}

/// Split a chunk's audio at a cut, leaving the closed part in `audio` and
/// returning the tail with the stream-coordinate index of its first sample.
///
/// The single place this arithmetic lives, so the test that asserts no sample is
/// lost or duplicated across a cut covers the production path.
fn split_audio(
    audio: &mut Vec<f32>,
    audio_start_sample: i64,
    cut_ms: i64,
) -> Option<(Vec<f32>, i64)> {
    let index = cut_index(audio_start_sample, audio.len(), cut_ms)?;
    let tail_start = audio_start_sample + index as i64;
    Some((audio.split_off(index), tail_start))
}

/// Move the sentence ends that survive a cut into the carried text's own
/// coordinates: everything at or before `at` belonged to the text that froze,
/// everything past it travels with the tail.
///
/// The timestamps are kept as they are — each end keeps the stream time it was
/// *first* seen at. Re-deriving them instead (as [`Coordinator::rebuild_ends`]
/// does for new ones) would time them at the revision that re-found them, i.e.
/// after the sentence they end and inside the pause the next cut is measured
/// against, which is the error the first-seen timestamps exist to avoid.
fn recede_ends(ends: &mut Vec<(usize, i64)>, at: usize) {
    ends.retain(|(offset, _)| *offset > at);
    for (offset, _) in ends.iter_mut() {
        *offset -= at;
    }
}

/// What the lifecycle should do this tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Idle,
    /// Close the chunk at this offset into its live text (a byte offset), with
    /// this stream timestamp for the audio.
    Close {
        offset: usize,
        audio_ms: i64,
    },
    /// Merge the chunk as it stands and keep it open: a break arrived before any
    /// sentence ended, so the polish must not freeze a half-spoken sentence.
    Polish,
    /// Close the whole chunk, cut or not: the valve, for a chunk that grew past
    /// [`MAX_CHUNK_SECONDS`] with no sentence end to cut at.
    CloseAll,
}

/// The sentence end a close cuts at: `context` ends *back* from the last one, so
/// the tail carried into the next chunk begins that many complete sentences
/// before the newly spoken text (see [`decide`]). `None` when the chunk does not
/// hold more ends than the context needs — a pause inside the first sentence
/// after a close, where there is nothing to cut at yet.
fn cut_end(ends: &[(usize, i64)], context: usize) -> Option<(usize, i64)> {
    ends.len()
        .checked_sub(context + 1)
        .and_then(|index| ends.get(index).copied())
}

/// The sentence cap to use, from the raw setting: `0` disables it, and anything
/// at or below `context` could never produce a cut — the candidate needs more
/// ends than the context carries — so it would ask for a close on every tick and
/// get none. The store's JSON is hand-editable, so the clamp lives here rather
/// than only in the UI.
fn sentence_cap(raw: u32, context: usize) -> usize {
    if raw == 0 {
        usize::MAX
    } else {
        (raw as usize).max(context + 1)
    }
}

/// The whole lifecycle decision, as a pure function.
///
/// `ends` is the open chunk's sentence ends (offset, first-seen timestamp),
/// `context` how many of those lead the merge window as already-spoken context,
/// `cap` the sentence count that closes a chunk on its own ([`usize::MAX`] when
/// the setting is off), `paused` a long break since the last close, `has_text`
/// whether the open chunk holds any committed text for a merge to replace,
/// `chunk_ms` the open chunk's audio duration.
///
/// The arm order is the design: a cuttable sentence end closes the chunk,
/// whether the trigger was a break or the cap; a break with nothing to cut at
/// polishes and keeps accumulating, so a half-spoken sentence is never frozen;
/// only the valve ever closes a chunk whole, and only when there is no sentence
/// end it could cut at instead — a repeated polish would never bound the chunk.
///
/// **The cut lands `context` ends before the last one**, so the tail that
/// carries into the next chunk begins that many complete sentences early: the
/// next merge window is `[start of the last previous sentence .. now]` rather
/// than everything since the last cut, which is what keeps the audio sent to the
/// extras bounded however long the session runs. With no such end to cut at
/// (fewer sentences than the context needs) a break falls through to
/// [`Step::Polish`] — whose window is then exactly that previous sentence plus
/// what has been spoken, so the extras hear the whole joint and can glue back a
/// sentence the stream ended early. A cap with nothing to cut at waits for the
/// next sentence end instead: a merge with no break behind it would be work
/// thrown away.
// A flat signature on purpose: every case here is a row of values in the tests
// below, and a struct would only add a name to each of them.
#[allow(clippy::too_many_arguments)]
fn decide(
    ends: &[(usize, i64)],
    context: usize,
    cap: usize,
    paused: bool,
    has_text: bool,
    chunk_ms: i64,
    audio_start_sample: i64,
    audio_len: usize,
) -> Step {
    let valves = chunk_ms >= MAX_CHUNK_SECONDS * 1000;
    if !(paused || ends.len() >= cap || valves) {
        return Step::Idle;
    }
    match cut_end(ends, context) {
        Some((offset, audio_ms))
            if cut_index(audio_start_sample, audio_len, audio_ms).is_some() =>
        {
            Step::Close { offset, audio_ms }
        }
        // The valve. A sentence end too early to cut at leaves this as the only
        // way to bound the chunk; splitting a sentence here is the accepted cost
        // of a model that does not punctuate.
        _ if valves => Step::CloseAll,
        // A break with nothing to cut at: polish what is there and keep
        // accumulating, so a half-spoken sentence is never frozen. Under the
        // minimum the work is not worth it and the chunk waits for the next
        // break.
        //
        // Nothing committed yet is the other case to decline: the merge would
        // cover `live[..0]`, and a merge that replaces no text is not a
        // replacement — [`Chunk::display_text`] would append the whole live text
        // to it once the model commits, showing the same words twice. The chunk
        // waits for the next break, by which time the text it is polishing
        // exists.
        _ if paused && has_text && chunk_ms >= MIN_CLOSE_MS => Step::Polish,
        _ => Step::Idle,
    }
}

// ---------------------------------------------------------------------------
// Chunk model
// ---------------------------------------------------------------------------

/// One chunk of the session: an append-only live text and the audio behind it.
struct Chunk {
    /// Monotonic within a session; a merge result is matched back to its chunk
    /// by this, because the open chunk can close while its job is still running.
    id: u64,
    /// The streaming model's own text for this chunk, append-only. It is the
    /// anchor every text offset is computed against.
    live: String,
    /// The merged text, once a merge has landed.
    merged_text: Option<String>,
    /// `live.len()` when `merged_text` was produced. What the streaming model
    /// has added since has not been merged yet and is appended verbatim when
    /// displaying — that is how a mid-sentence polish keeps showing the words
    /// spoken after it.
    merged_live_len: usize,
    /// 16 kHz samples from this chunk's cut point. Taken out when a close job is
    /// dispatched, so a closed chunk holds audio only while its merge is pending
    /// or after one failed (see `Coordinator::retry`).
    audio: Vec<f32>,
    /// Stream-coordinate sample index of `audio[0]`.
    audio_start_sample: i64,
    /// Whether the last merge for this chunk failed, so it is showing the
    /// extras' concatenated text rather than a merged one.
    failed: bool,
}

impl Chunk {
    fn new(id: u64, audio_start_sample: i64) -> Self {
        Self {
            id,
            live: String::new(),
            merged_text: None,
            merged_live_len: 0,
            audio: Vec::new(),
            audio_start_sample,
            failed: false,
        }
    }

    /// What this chunk contributes to the session's text: the merged text plus
    /// whatever the streaming model added after the merge, or the plain live
    /// text while nothing has been merged.
    ///
    /// The tail is appended verbatim, with no separator inserted: it is the
    /// continuation of the very text the merge covered, so a space invented here
    /// would land inside a word. The cut that produced it kept the original
    /// whitespace, and the stream's text after a merge result keeps its own.
    fn display_text(&self) -> String {
        let Some(merged) = &self.merged_text else {
            return self.live.clone();
        };
        let tail = &self.live[clamp_boundary(&self.live, self.merged_live_len)..];
        if tail.is_empty() {
            return merged.clone();
        }
        let mut out = merged.clone();
        out.push_str(tail);
        out
    }

    fn duration_ms(&self) -> i64 {
        self.audio.len() as i64 / SAMPLES_PER_MS
    }

    /// Record a merge result covering `live[..live_len]`.
    fn apply_merge(&mut self, text: String, live_len: usize, failed: bool) {
        self.merged_text = Some(text);
        self.merged_live_len = live_len;
        self.failed = failed;
    }
}

// ---------------------------------------------------------------------------
// Merge job
// ---------------------------------------------------------------------------

/// One chunk's merge input, snapshotted when the job is dispatched — the chunk
/// can keep growing while the job runs.
struct JobInput {
    chunk_id: u64,
    /// The primary model's own text for this chunk, which is what `${output}`
    /// (slot 1) receives.
    live: String,
    /// 16 kHz, from the chunk's cut point.
    audio: Vec<f32>,
    /// Whether this job's per-model outputs belong in the session's history
    /// metadata. True for the merge that closes a chunk, false for a
    /// mid-sentence polish (superseded by the next one) and for a retry (the
    /// outputs are already recorded).
    record_outputs: bool,
}

struct JobResult {
    /// The dispatch generation this job was created under. See [`MergeQueue`].
    generation: u64,
    chunk_id: u64,
    /// `live.len()` when the job was dispatched; the merged text covers exactly
    /// this much of the chunk's live text.
    live_len: usize,
    record_outputs: bool,
    /// `None` when no merge text was produced; `outputs` still holds the extras.
    merged: Option<String>,
    outputs: [String; 4],
    brain: Option<MultiSttHistoryBrain>,
    /// A merge was configured and asked for but did not produce text.
    failed: bool,
    /// The job's audio, handed back only for a failed close so the next break
    /// can retry this chunk instead of losing its polish. Letting the job return
    /// it keeps the alternative — a second copy held by the coordinator — out of
    /// the steady state.
    retry_audio: Option<Vec<f32>>,
    decode_latency_ms: f64,
    merge_latency_ms: f64,
}

/// Decode the extras for one chunk and merge them with the primary's text.
///
/// The extras' decodes run on the blocking pool, one per model, concurrently —
/// the same shape as the batch path in `MultiSttAction::stop`. The untracked
/// `transcribe_with_extra` is used deliberately: a session's statistics record
/// one run, and three extra decode attempts per chunk would inflate that run's
/// numbers by the chunk count.
async fn run_merge_job(
    tm: Arc<TranscriptionManager>,
    settings: AppSettings,
    input: JobInput,
    generation: u64,
) -> JobResult {
    let JobInput {
        chunk_id,
        live,
        audio,
        record_outputs,
    } = input;
    let live_len = live.len();
    let decode_start = Instant::now();

    // Each extra decodes the chunk's audio in full. The clones are ~4 MB at the
    // 60 s valve, against decodes that cost far more.
    let spawn_extra = |slot: usize, model_id: Option<String>| {
        let tm = Arc::clone(&tm);
        let audio = audio.clone();
        model_id.map(move |model_id| {
            tauri::async_runtime::spawn_blocking(move || {
                if !tm.is_extra_model_loaded(&model_id) {
                    warn!(
                        "Multi-STT streaming: extra model '{}' is not loaded, skipping it for this chunk",
                        model_id
                    );
                    return (slot, String::new());
                }
                match tm.transcribe_with_extra(&model_id, audio) {
                    Ok(text) => (slot, text),
                    Err(e) => {
                        warn!(
                            "Multi-STT streaming: extra model '{}' failed on this chunk: {}",
                            model_id, e
                        );
                        (slot, String::new())
                    }
                }
            })
        })
    };

    let handles = [
        spawn_extra(1, settings.multi_stt_model_2.clone()),
        spawn_extra(2, settings.multi_stt_model_3.clone()),
        spawn_extra(3, settings.multi_stt_model_4.clone()),
    ];

    let mut outputs: [String; 4] = Default::default();
    for handle in handles.into_iter().flatten() {
        if let Ok((slot, text)) = handle.await {
            outputs[slot] = text;
        }
    }
    let decode_latency_ms = decode_start.elapsed().as_secs_f64() * 1000.0;

    let merge_start = Instant::now();
    let (merged, brain, failed) = if has_merge_prompt(&settings) {
        let outcome =
            multi_stt_merge_transcriptions(&settings, &live, &outputs[1], &outputs[2], &outputs[3])
                .await;
        match outcome {
            // An empty merge result would delete text that is already on
            // screen; treat it as a failure and keep the extras' output instead.
            Some(outcome) if outcome.cleaned_text.trim().is_empty() => {
                warn!("Multi-STT streaming: the merge returned no text for a chunk");
                (None, None, true)
            }
            Some(outcome) => {
                let brain = MultiSttHistoryBrain {
                    provider_id: outcome.provider_id,
                    provider_label: outcome.provider_label,
                    model_name: outcome.model_name,
                    prompt_name: outcome.prompt_name,
                    latency_ms: Some(merge_start.elapsed().as_secs_f64() * 1000.0),
                    raw_output: outcome.raw_text,
                    cleaned_output: outcome.cleaned_text.clone(),
                };
                (Some(outcome.cleaned_text), Some(brain), false)
            }
            None => (None, None, true),
        }
    } else {
        (None, None, false)
    };
    let merge_latency_ms = merge_start.elapsed().as_secs_f64() * 1000.0;

    JobResult {
        generation,
        chunk_id,
        live_len,
        record_outputs,
        merged,
        outputs,
        brain,
        failed,
        retry_audio: (failed && record_outputs).then_some(audio),
        decode_latency_ms,
        merge_latency_ms,
    }
}

/// The batch path's fallback shape — every model's output on its own line — with
/// the primary's own text standing in for slot 1. Used when a chunk's merge could
/// not run, so a failed chunk reads exactly like a failed Multi-STT run.
fn concatenate(primary: &str, outputs: &[String; 4]) -> String {
    let mut combined = String::new();
    for text in std::iter::once(primary).chain(outputs.iter().skip(1).map(String::as_str)) {
        if text.is_empty() {
            continue;
        }
        if !combined.is_empty() {
            combined.push('\n');
        }
        combined.push_str(text);
    }
    combined
}

/// What a finished session hands back to `MultiSttAction::stop`.
pub struct MultiSttStreamOutcome {
    /// The whole session's text, chunk by chunk, merged where a merge landed.
    pub final_text: String,
    /// Slot 1 is the primary model's own live text for the session; slots 2–4
    /// are the extras' outputs as recorded for each chunk that closed.
    pub model_outputs: [String; 4],
    pub brain: Option<MultiSttHistoryBrain>,
    pub failed_chunks: u32,
    pub chunk_count: u32,
    /// Whether this session typed the text into the app itself, in which case
    /// the action must not paste the final text as well.
    pub owns_typing: bool,
    /// Decode and LLM time summed over every chunk merge of the session.
    pub decode_latency_ms: f64,
    pub merge_latency_ms: f64,
}

// ---------------------------------------------------------------------------
// Session handle
// ---------------------------------------------------------------------------

enum Cmd {
    Finish(Sender<Option<MultiSttStreamOutcome>>),
    Cancel,
}

struct Session {
    tx: Sender<Cmd>,
    handle: JoinHandle<()>,
    /// Cleared by [`Coordinator::shutdown`], the last thing the coordinator
    /// thread does. It exists because a session can end *without* any call into
    /// this module: the user's cancel hotkey stops the recorder and the stream
    /// directly, and only then does the coordinator notice and release itself.
    /// The slot it left behind would otherwise keep [`is_active`] true — and
    /// `MultiSttAction::stop` reads that to choose between asking for a result
    /// and taking the batch path. A flag rather than the thread clearing its own
    /// slot, because the thread can finish before `start` has stored the slot.
    alive: Arc<AtomicBool>,
}

/// The one running session. A process-wide slot, like the tap: only one
/// recording exists at a time, and both `start` and `stop` hold the app handle
/// but no shared state of their own.
static SESSION: LazyLock<Mutex<Option<Session>>> = LazyLock::new(|| Mutex::new(None));

fn take_session() -> Option<Session> {
    SESSION.lock().unwrap().take()
}

/// Whether a coordinator is running. Read by `MultiSttAction::stop` to decide
/// whether to ask it for the result or take the batch path.
pub fn is_active() -> bool {
    SESSION
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|session| session.alive.load(Ordering::Acquire))
}

/// Arm the mode for a recording that is about to start, if every precondition
/// holds. Returns whether a coordinator is now running.
///
/// Call this *before* `try_start_recording`: the tap must be claimed (and the
/// stream's sink installed) while the recording is still off, so the first frame
/// of the recording is the first frame the coordinator sees. Every refusal is
/// logged — the mode is experimental, and silently falling back to plain
/// Multi-STT would look like the setting does nothing.
pub fn start(
    app: &AppHandle,
    tm: &Arc<TranscriptionManager>,
    rm: &Arc<AudioRecordingManager>,
    model_supports_streaming: bool,
) -> bool {
    // A session left over from a recording that never reached `stop` (a stop
    // path that returned early, the mode toggled off mid-recording) must not
    // survive into this one: it would keep draining the tap this recording is
    // filling.
    cancel();

    let settings = get_settings(app);
    if !settings.multi_stt_streaming_first_enabled {
        return false;
    }
    if !model_supports_streaming {
        warn!(
            "Multi-STT streaming: '{}' cannot stream natively; using the normal batch path",
            settings.selected_model
        );
        return false;
    }
    if !has_merge_prompt(&settings) {
        warn!(
            "Multi-STT streaming: no merge prompt configured, so there is nothing to replace the \
             live text with; using the normal batch path"
        );
        return false;
    }

    let tap = chunk_tap();
    let token = tap.begin();

    // The sink owns the overlay's text for the session: the streaming model's
    // rough text never reaches the overlay raw, because what is displayed is
    // composed from the chunks.
    let snapshot = Arc::new(Mutex::new(Arc::new(Snapshot::default())));
    {
        let snapshot = Arc::clone(&snapshot);
        tm.set_stream_text_sink(
            Some(Arc::new(
                move |committed: &str,
                      tentative: &str,
                      audio_committed_ms: i64,
                      input_received_ms: i64| {
                    let mut slot = snapshot.lock().unwrap();
                    let revision = slot.revision.wrapping_add(1);
                    *slot = Arc::new(Snapshot {
                        committed: committed.to_string(),
                        tentative: tentative.to_string(),
                        audio_committed_ms,
                        input_received_ms,
                        revision,
                    });
                },
            )),
            true,
        );
    }

    let (tx, rx) = mpsc::channel();
    let alive = Arc::new(AtomicBool::new(true));
    let writer = (settings.paste_method == PasteMethod::DirectStreaming).then(|| {
        DirectStreamWriter::new(
            app.clone(),
            settings.direct_streaming_speed,
            settings.clone(),
        )
    });
    let owns_typing = writer.is_some();

    let coordinator = Coordinator {
        app: app.clone(),
        tm: Arc::clone(tm),
        rm: Arc::clone(rm),
        tap,
        token,
        start_generation: rm.cancel_generation(),
        snapshot,
        settings,
        settings_ticks: 0,
        seen_revision: u64::MAX,
        primary_text: String::new(),
        primary_tentative: String::new(),
        live_copied: 0,
        alive: Arc::clone(&alive),
        next_chunk_id: 1,
        open: Chunk::new(0, 0),
        closed: Vec::new(),
        ends: Vec::new(),
        last_speech_ms: 0,
        last_speech_change: Instant::now(),
        last_close_speech_ms: 0,
        retry: None,
        merge: MergeQueue::new(),
        failed_chunks: 0,
        outputs: Default::default(),
        brain: None,
        decode_latency_ms: 0.0,
        merge_latency_ms: 0.0,
        published_committed: String::new(),
        published_tentative: String::new(),
        writer,
        owns_typing,
        writer_pushed: String::new(),
        scratch: Vec::new(),
        lead_checked: false,
    };

    let handle = thread::spawn(move || coordinator.run(rx));
    *SESSION.lock().unwrap() = Some(Session { tx, handle, alive });

    info!(
        "Multi-STT streaming: armed (break threshold {} ms, direct typing: {})",
        get_settings(app).multi_stt_streaming_pause_ms,
        owns_typing
    );
    true
}

/// Finish the session and take its result, waiting up to `timeout`.
///
/// Must be called *after* the primary stream has been finalized: the finalize
/// path hands the stream's last words to the sink, so by then the open chunk's
/// live text is complete and its merge covers everything that was said.
///
/// Blocks the calling thread — call it from `spawn_blocking`, never from an
/// async task, because it waits on the coordinator thread, which itself blocks
/// on the last merge.
pub fn finish(timeout: Duration) -> Option<MultiSttStreamOutcome> {
    let tx = {
        let guard = SESSION.lock().unwrap();
        guard.as_ref()?.tx.clone()
    };
    let (reply_tx, reply_rx) = mpsc::channel();
    if tx.send(Cmd::Finish(reply_tx)).is_err() {
        // The coordinator died on its own; its thread is gone with the state.
        take_session();
        return None;
    }
    match reply_rx.recv_timeout(timeout) {
        Ok(outcome) => {
            if let Some(session) = take_session() {
                let _ = session.handle.join();
            }
            outcome
        }
        Err(e) => {
            error!(
                "Multi-STT streaming: the coordinator did not finish in time: {}",
                e
            );
            take_session();
            None
        }
    }
}

/// Stop the session and drop everything it produced: nothing more is typed, no
/// merge is applied, no text is returned. A no-op when no session runs.
pub fn cancel() {
    let Some(session) = take_session() else {
        return;
    };
    let _ = session.tx.send(Cmd::Cancel);
    let _ = session.handle.join();
}

// ---------------------------------------------------------------------------
// The coordinator
// ---------------------------------------------------------------------------

/// The stream worker's latest published text, shared between the sink (which
/// runs on that worker's thread) and this module's own thread.
///
/// Held behind an `Arc` inside the mutex so a per-tick read is one atomic
/// increment: the sink already has to copy the text it is given, and copying it
/// a second time per tick would be pure waste.
#[derive(Default)]
struct Snapshot {
    committed: String,
    tentative: String,
    audio_committed_ms: i64,
    input_received_ms: i64,
    revision: u64,
}

/// The merge jobs the session has handed out, and the results that have come
/// back for them.
///
/// A *queue*, not a single slot. The watchdog abandons a job that runs past
/// [`MERGE_TIMEOUT`] but cannot cancel it — the task is detached and finishes on
/// its own — and the session dispatches a new one meanwhile. With one slot that
/// second dispatch overwrote whatever the first had produced, so a chunk's merge
/// was lost silently and permanently: a close merge's audio had already been
/// taken from the chunk, so there was nothing left to retry from.
///
/// Each job also carries the generation it was dispatched under, and only a
/// result whose generation matches the one the session is waiting for releases
/// it. An abandoned job's result is still *applied* — its chunk has been waiting
/// for it, and it is a real merge of real audio — but retiring the session on it
/// would let a second merge start alongside the one that is genuinely in flight.
///
/// Split out of [`Coordinator`], which cannot be built without an `AppHandle`,
/// so the bookkeeping is reachable from a test.
struct MergeQueue {
    results: Arc<Mutex<VecDeque<Box<JobResult>>>>,
    /// The newest job's generation, which is also the one being waited for.
    generation: u64,
    running: bool,
    started: Instant,
}

impl MergeQueue {
    fn new() -> Self {
        Self {
            results: Arc::new(Mutex::new(VecDeque::new())),
            generation: 0,
            running: false,
            started: Instant::now(),
        }
    }

    /// The generation a job dispatched now runs under, marking the session busy.
    fn next_generation(&mut self) -> u64 {
        debug_assert!(!self.running, "a merge job is already running");
        self.generation += 1;
        self.running = true;
        self.started = Instant::now();
        self.generation
    }

    /// What a spawned job holds to hand its result back: the shared tail only,
    /// so the task cannot reach the session's own bookkeeping.
    fn handle(&self) -> MergeQueueHandle {
        MergeQueueHandle {
            results: Arc::clone(&self.results),
        }
    }

    /// Take the oldest result, and whether it releases the session.
    fn take(&mut self) -> Option<(JobResult, bool)> {
        let result = self.results.lock().unwrap().pop_front()?;
        let releases = result.generation == self.generation;
        if releases {
            self.running = false;
        }
        Some((*result, releases))
    }

    /// Stop waiting for the in-flight job without cancelling it.
    fn abandon(&mut self) {
        self.running = false;
    }

    fn clear(&self) {
        self.results.lock().unwrap().clear();
    }
}

/// The sending half of a [`MergeQueue`], held by a spawned merge job.
struct MergeQueueHandle {
    results: Arc<Mutex<VecDeque<Box<JobResult>>>>,
}

impl MergeQueueHandle {
    fn push(&self, result: JobResult) {
        self.results.lock().unwrap().push_back(Box::new(result));
    }
}

struct Coordinator {
    app: AppHandle,
    tm: Arc<TranscriptionManager>,
    rm: Arc<AudioRecordingManager>,
    tap: Arc<ChunkTap>,
    /// This session's claim on the tap; a different token means another
    /// recording took it and this coordinator must stop touching it.
    token: u64,
    start_generation: u64,
    snapshot: Arc<Mutex<Arc<Snapshot>>>,
    settings: AppSettings,
    settings_ticks: u32,

    seen_revision: u64,
    /// The primary model's own text for the session, refreshed from the
    /// snapshot. Slot 1 of the history metadata.
    primary_text: String,
    primary_tentative: String,
    /// Absolute byte offset in `committed` up to which the open chunk's `live`
    /// has been copied.
    live_copied: usize,

    /// This session's liveness, as [`is_active`] reads it. See [`Session`].
    alive: Arc<AtomicBool>,

    next_chunk_id: u64,
    open: Chunk,
    closed: Vec<Chunk>,
    /// Sentence ends in the *open* chunk's live text with the stream time each
    /// was first seen, rebuilt on every text revision. Rebuilding — rather than
    /// appending — is what lets later text invalidate an earlier end: `3.` looks
    /// like a sentence until `14` arrives.
    ends: Vec<(usize, i64)>,

    last_speech_ms: i64,
    last_speech_change: Instant,
    /// The speech clock's value when the chunk was last closed or polished. A
    /// break only counts again once speech has advanced past it, so one long
    /// pause cannot trigger a merge on every tick.
    last_close_speech_ms: i64,

    /// The last failed chunk's audio and id, kept so the next break can retry its
    /// merge (a transient provider failure should not cost the chunk its polish).
    /// One chunk's worth, bounded by [`MAX_CHUNK_SECONDS`].
    retry: Option<(u64, Vec<f32>)>,

    merge: MergeQueue,
    failed_chunks: u32,
    /// Per-slot extras' outputs, accumulated from the merges that closed a chunk.
    outputs: [String; 4],
    brain: Option<MultiSttHistoryBrain>,
    decode_latency_ms: f64,
    merge_latency_ms: f64,

    published_committed: String,
    published_tentative: String,
    writer: Option<DirectStreamWriter>,
    /// Whether this session is the one typing into the app, in which case the
    /// action must not paste the final text as well.
    owns_typing: bool,
    /// The last text handed to the writer, for the extension/revision test.
    writer_pushed: String,
    scratch: Vec<f32>,
    lead_checked: bool,
}

impl Coordinator {
    fn run(mut self, rx: Receiver<Cmd>) {
        let mut finished = false;
        // This runs on the session's own thread, so a panic anywhere in `step`
        // would unwind straight past `shutdown` — leaving the audio tap claimed
        // by a session that no longer exists and the exclusive text sink
        // installed, which shows up as the *next* recording drawing no overlay
        // text at all. Catch it, say so, and release the session either way.
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            loop {
                match rx.recv_timeout(TICK) {
                    Ok(Cmd::Finish(reply)) => {
                        let outcome = self.finish();
                        let _ = reply.send(outcome);
                        finished = true;
                        break;
                    }
                    Ok(Cmd::Cancel) => break,
                    Err(RecvTimeoutError::Timeout) => {}
                    // The session was dropped without a command: treat as a cancel.
                    Err(RecvTimeoutError::Disconnected) => break,
                }
                if !self.step() {
                    break;
                }
            }
        }))
        .is_err();
        if panicked {
            error!("Multi-STT streaming: the coordinator panicked; releasing the session");
        }
        self.shutdown(finished);
    }

    /// One tick. Returns whether the session should keep running.
    fn step(&mut self) -> bool {
        if !self.tap.is_current(self.token) {
            warn!("Multi-STT streaming: another recording took the audio tap; stopping");
            return false;
        }
        // A cancel never reaches this action (`utils::cancel_current_operation`
        // stops the recorder and the stream, and only then tells the
        // coordinator), so the session has to notice it itself.
        if self.rm.was_cancelled_since(self.start_generation) {
            debug!("Multi-STT streaming: cancelled by the user");
            return false;
        }

        self.settings_ticks = self.settings_ticks.wrapping_add(1);
        if self.settings_ticks.is_multiple_of(SETTINGS_REFRESH_TICKS) {
            self.settings = get_settings(&self.app);
        }

        // Audio first: the tap is drained every tick, so a cut is a local
        // `split_off` and never an index into the past.
        self.tap.take_into(&mut self.scratch);
        if !self.scratch.is_empty() {
            self.open.audio.extend_from_slice(&self.scratch);
            self.scratch.clear();
        }

        self.collect_merge_result();
        self.absorb_stream_text();

        let speech = self.rm.last_speech_ms() as i64;
        if speech != self.last_speech_ms {
            self.last_speech_ms = speech;
            self.last_speech_change = Instant::now();
        }
        let pause =
            Duration::from_millis(u64::from(self.settings.multi_stt_streaming_pause_ms.max(1)));
        let paused = speech > 0
            && speech > self.last_close_speech_ms
            && self.last_speech_change.elapsed() >= pause;

        // One merge at a time. A break during a job is handled on a later tick,
        // not queued: the sentence ends' timestamps were fixed when they were
        // first seen, so a late close cuts in exactly the same place.
        if self.merge.running {
            self.watchdog_merge();
            if self.merge.running {
                self.publish(false);
                return true;
            }
        }

        let context = self.settings.multi_stt_streaming_context_sentences as usize;
        let cap = sentence_cap(self.settings.multi_stt_streaming_max_sentences, context);
        let step = decide(
            &self.ends,
            context,
            cap,
            paused,
            !self.open.live.trim().is_empty(),
            self.open.duration_ms(),
            self.open.audio_start_sample,
            self.open.audio.len(),
        );
        match step {
            Step::Idle => {}
            Step::Close { offset, audio_ms } => {
                self.close_at(offset, audio_ms);
                self.last_close_speech_ms = speech;
            }
            Step::CloseAll => {
                self.close_entire_open_chunk();
                self.last_close_speech_ms = speech;
            }
            Step::Polish => {
                // The chunk stays open and keeps accumulating; the merge only
                // replaces what is on screen while the speaker is paused. A
                // polish never displaces a running job: the close that owns the
                // chunk's fate takes precedence, and a polish into an occupied
                // slot would be a merge whose result is thrown away.
                if !self.merge.running {
                    self.dispatch(self.open_job(false));
                }
                self.last_close_speech_ms = speech;
            }
        }

        // A failed chunk is retried at the next break, so the retry rate is the
        // session's natural pace and a down provider is not hammered.
        if (paused || step != Step::Idle) && !self.merge.running {
            self.retry_failed_chunk();
        }

        self.publish(false);
        true
    }

    /// Read the stream's latest text into the open chunk and rebuild its
    /// sentence ends.
    fn absorb_stream_text(&mut self) {
        let snapshot = Arc::clone(&*self.snapshot.lock().unwrap());
        if snapshot.revision == self.seen_revision {
            return;
        }
        self.seen_revision = snapshot.revision;

        // The tap and the stream are fed the same frames in the same order (see
        // `ChunkTap`), so the tap must have seen exactly the audio the stream was
        // fed. A structural mismatch would mean every timestamp is off by a
        // constant, which is worth one warning — the correction is not applied,
        // because a worker-thread lag of a frame or two is indistinguishable
        // from a real lead and would silently absorb it.
        if !self.lead_checked && snapshot.input_received_ms > 0 {
            self.lead_checked = true;
            let fed = snapshot.input_received_ms as u64 * SAMPLES_PER_MS as u64;
            let tapped = self.tap.pushed_samples();
            if tapped > fed {
                warn!(
                    "Multi-STT streaming: the audio tap is {} samples ahead of the stream; sentence \
                     timestamps may be off by {:.0} ms",
                    tapped - fed,
                    (tapped - fed) as f64 / SAMPLES_PER_MS as f64
                );
            }
        }

        self.primary_text = snapshot.committed.clone();
        self.primary_tentative = snapshot.tentative.clone();

        // `committed` only grows. If a family rewrites it anyway, offsets into it
        // are meaningless: re-anchor on the current length and say so once. The
        // audio is unaffected — it is cut by stream time, not by text offset.
        if snapshot.committed.len() < self.live_copied {
            warn!(
                "Multi-STT streaming: the committed text shrank ({} → {} bytes); re-anchoring the \
                 open chunk",
                self.live_copied,
                snapshot.committed.len()
            );
            self.open.live.clear();
            self.open.merged_text = None;
            self.open.merged_live_len = 0;
            self.live_copied = snapshot.committed.len();
        }
        if snapshot.committed.len() > self.live_copied {
            let at = clamp_boundary(&snapshot.committed, self.live_copied);
            self.open.live.push_str(&snapshot.committed[at..]);
            self.live_copied = snapshot.committed.len();
        }
        self.rebuild_ends(snapshot.audio_committed_ms);
    }

    /// Recompute the open chunk's sentence ends, keeping the timestamp each was
    /// first seen with. A sentence end is timed when it appears, not when the
    /// chunk closes: by close time the pause is under way and the timestamp
    /// would sit past the silence the break is measured on.
    fn rebuild_ends(&mut self, audio_ms: i64) {
        let fresh = sentence_ends(&self.open.live);
        let mut rebuilt = Vec::with_capacity(fresh.len());
        for offset in fresh {
            let seen = self
                .ends
                .iter()
                .find(|(o, _)| *o == offset)
                .map(|(_, ms)| *ms)
                .unwrap_or(audio_ms);
            rebuilt.push((offset, seen));
        }
        self.ends = rebuilt;
    }

    /// The open chunk itself, as a job input. Its audio is cloned because the
    /// chunk keeps growing while the job runs.
    fn open_job(&self, record_outputs: bool) -> JobInput {
        JobInput {
            chunk_id: self.open.id,
            live: self.open.live.clone(),
            audio: self.open.audio.clone(),
            record_outputs,
        }
    }

    /// Close the open chunk at a sentence end: the text before the cut freezes
    /// and is merged, the tail carries into the new open chunk.
    ///
    /// The cut is `context` sentence ends *earlier* than the last one (see
    /// [`decide`]), so the tail is not only the half-spoken sentence — it begins
    /// with that many complete sentences, which the next merge window opens on as
    /// already-spoken context. Text and audio both split at the same offset in
    /// the same step, so the two regions meet exactly: nothing is dropped,
    /// duplicated or shown twice, and no sentence straddles the seam.
    fn close_at(&mut self, offset: usize, audio_ms: i64) {
        let at = clamp_boundary(&self.open.live, offset);
        // `decide` already established that this cut has audio behind it; a
        // second chance here would only hide a bug.
        let Some((tail_audio, tail_start)) =
            split_audio(&mut self.open.audio, self.open.audio_start_sample, audio_ms)
        else {
            debug!("Multi-STT streaming: dropped a cut that no longer has audio behind it");
            return;
        };
        let tail_text = self.open.live.split_off(at);

        // A polish that ran mid-sentence covered text this cut now splits: its
        // merged text belongs to neither side, so it is dropped and the closed
        // chunk falls back to its raw live text until this close's own merge
        // lands. Any earlier merge this cut does not touch is kept.
        let mut merged = self.open.merged_text.take();
        if self.open.merged_live_len > at {
            merged = None;
        }
        let merged_live_len = if merged.is_some() {
            self.open.merged_live_len
        } else {
            0
        };

        let closed_audio = std::mem::take(&mut self.open.audio);
        let closed_start = self.open.audio_start_sample;
        let closed_id = self.open.id;
        let mut closed = Chunk {
            id: closed_id,
            live: std::mem::take(&mut self.open.live),
            merged_text: merged,
            merged_live_len,
            audio: closed_audio,
            audio_start_sample: closed_start,
            failed: false,
        };

        let mut next = Chunk::new(self.next_chunk_id, tail_start);
        self.next_chunk_id += 1;
        next.live = tail_text;
        next.audio = tail_audio;
        self.open = next;
        // The carried text begins `context` sentence ends before the one that was
        // cut at, so those ends move into the new chunk with it — as its leading
        // context sentence, which is what the next merge window opens on.
        recede_ends(&mut self.ends, at);

        // A close merge owns the chunk's history outputs, and it is what decides
        // the chunk's failure state — a failed polish is superseded by it.
        self.dispatch(JobInput {
            chunk_id: closed_id,
            live: closed.live.clone(),
            audio: std::mem::take(&mut closed.audio),
            record_outputs: true,
        });
        self.closed.push(closed);
        self.recount_failed();
    }

    /// Close the whole open chunk, cut or not: the valve, for a chunk that grew
    /// past [`MAX_CHUNK_SECONDS`] with no sentence end to cut at. Nothing carries
    /// over, so the next chunk starts with empty text and audio at this point.
    fn close_entire_open_chunk(&mut self) {
        let start = self.open.audio_start_sample + self.open.audio.len() as i64;
        let mut closed = std::mem::replace(&mut self.open, Chunk::new(self.next_chunk_id, start));
        self.next_chunk_id += 1;
        closed.merged_text = None;
        closed.merged_live_len = 0;
        closed.failed = false;
        self.ends.clear();

        self.dispatch(JobInput {
            chunk_id: closed.id,
            live: closed.live.clone(),
            audio: std::mem::take(&mut closed.audio),
            record_outputs: true,
        });
        self.closed.push(closed);
        self.recount_failed();
    }

    /// Hand a job to the blocking pool. Only one runs at a time; the callers
    /// check [`MergeQueue::running`] first.
    fn dispatch(&mut self, input: JobInput) {
        let tm = Arc::clone(&self.tm);
        // Fresh at dispatch, not the cached copy: a merge is a network round
        // trip, so it reads the provider and prompt the user last saved.
        let settings = get_settings(&self.app);
        let generation = self.merge.next_generation();
        let merge = self.merge.handle();
        tauri::async_runtime::spawn(async move {
            let result = run_merge_job(tm, settings, input, generation).await;
            merge.push(result);
        });
    }

    /// Take the oldest finished job's result and apply it to its chunk.
    ///
    /// Returns whether a result was taken. An abandoned job's result does not
    /// release the session: only the generation the session is waiting for does.
    fn collect_merge_result(&mut self) -> bool {
        let Some((result, releases)) = self.merge.take() else {
            return false;
        };
        if !releases {
            debug!(
                "Multi-STT streaming: applying the result of abandoned merge generation {} \
                 (the session is on {})",
                result.generation, self.merge.generation
            );
        }
        self.apply_job_result(result);
        true
    }

    /// Give up on a job that has run far too long. It is not cancelled — the task
    /// is detached and queues its result when it ends, where it is applied like
    /// any other — but the session carries on closing chunks instead of wedging
    /// on it.
    fn watchdog_merge(&mut self) {
        if self.merge.started.elapsed() < MERGE_TIMEOUT {
            return;
        }
        warn!(
            "Multi-STT streaming: a chunk merge has been running for {:?}; abandoning it",
            self.merge.started.elapsed()
        );
        self.merge.abandon();
    }

    /// Record a finished job's result on the chunk it belongs to, and on the
    /// session (outputs, brain, latencies, failure count).
    fn apply_job_result(&mut self, result: JobResult) {
        let JobResult {
            // Already spent: `collect_merge_result` used it to decide whether
            // this result releases the session's in-flight job.
            generation: _,
            chunk_id,
            live_len,
            record_outputs,
            merged,
            outputs,
            brain,
            failed,
            retry_audio,
            decode_latency_ms,
            merge_latency_ms,
        } = result;

        self.decode_latency_ms += decode_latency_ms;
        self.merge_latency_ms += merge_latency_ms;
        if let Some(brain) = brain {
            self.brain = Some(brain);
        }

        let applied = {
            let target = if self.open.id == chunk_id {
                Some(&mut self.open)
            } else {
                self.closed.iter_mut().find(|c| c.id == chunk_id)
            };
            match target {
                // The chunk's live text is shorter than the result's: a cut moved
                // that text into the next chunk while the job ran, so the result
                // now covers text it does not own. Dropping it leaves the raw
                // live text on screen instead of text from another chunk.
                Some(chunk) if chunk.live.len() < live_len => {
                    debug!(
                        "Multi-STT streaming: dropping a stale merge for chunk {} (its text was cut \
                         while the job ran)",
                        chunk_id
                    );
                    false
                }
                Some(chunk) => {
                    if failed {
                        // The extras' outputs on their own lines, with the
                        // primary's text standing in for slot 1 — the batch
                        // path's fallback shape.
                        let primary = &chunk.live[..clamp_boundary(&chunk.live, live_len)];
                        let fallback = concatenate(primary, &outputs);
                        if fallback.is_empty() {
                            // Nothing at all to show: keep the live text rather
                            // than blanking the chunk.
                            warn!(
                                "Multi-STT streaming: chunk {} has no text at all after a failed \
                                 merge",
                                chunk_id
                            );
                            chunk.merged_text = None;
                            chunk.merged_live_len = 0;
                            chunk.failed = true;
                        } else {
                            chunk.apply_merge(fallback, live_len, true);
                        }
                    } else if let Some(text) = merged {
                        chunk.apply_merge(text, live_len, false);
                    }
                    true
                }
                None => {
                    debug!("Multi-STT streaming: a merge result arrived for a chunk that is gone");
                    false
                }
            }
        };
        if !applied {
            return;
        }

        if record_outputs {
            // The extras' per-model texts, appended chunk by chunk so the
            // history trailer holds the whole session's output per model.
            let extras = outputs.iter().skip(1).zip(self.outputs.iter_mut().skip(1));
            for (text, session) in extras {
                if text.is_empty() {
                    continue;
                }
                if !session.is_empty() {
                    session.push('\n');
                }
                session.push_str(text);
            }
        }
        if let Some(audio) = retry_audio {
            self.retry = Some((chunk_id, audio));
        }

        let previous_failed = self.failed_chunks;
        self.recount_failed();
        if failed && self.failed_chunks > previous_failed {
            // The text itself never carries the failure: with `DirectStreaming`
            // it is typed into the user's document, and a marker would be typed
            // with it. The overlay badge and this event are how it surfaces.
            let _ = MultiSttStreamChunkFailedEvent {
                chunk: (chunk_id + 1) as u32,
                failed_chunks: self.failed_chunks,
            }
            .emit(&self.app);
        }
    }

    /// Recompute the failed-chunk count, which is what the overlay badge and the
    /// history metadata report. A retry that succeeds clears its chunk's flag, so
    /// the count drops on its own.
    fn recount_failed(&mut self) {
        self.failed_chunks =
            self.closed.iter().filter(|c| c.failed).count() as u32 + u32::from(self.open.failed);
    }

    /// Retry the most recent failed chunk's merge, if any is waiting. A retry
    /// records no outputs: that chunk's per-model texts are already part of the
    /// session's metadata.
    fn retry_failed_chunk(&mut self) {
        let Some((chunk_id, audio)) = self.retry.take() else {
            return;
        };
        let Some(live) = self
            .closed
            .iter()
            .find(|c| c.id == chunk_id)
            .map(|c| c.live.clone())
        else {
            // The chunk was dropped with a re-anchor; nothing to retry.
            return;
        };
        debug!(
            "Multi-STT streaming: retrying the merge of failed chunk {}",
            chunk_id
        );
        self.dispatch(JobInput {
            chunk_id,
            live,
            audio,
            record_outputs: false,
        });
    }

    /// The session's text: every closed chunk in order, then the open one.
    fn compose_committed(&self) -> String {
        let mut out = String::new();
        for chunk in &self.closed {
            append_join(&mut out, &chunk.display_text());
        }
        out
    }

    /// Publish the composed text: the closed chunks as `committed`, the open
    /// chunk plus the model's volatile tail as `tentative`. The whole session's
    /// text is therefore on the wire from the first second — the rough text is
    /// never hidden, it is replaced in place as merges land.
    fn publish(&mut self, force: bool) {
        let committed = self.compose_committed();
        let mut tentative = self.open.display_text();
        // The model's volatile tail continues its own committed text, so it is
        // concatenated verbatim: at this point both halves came from the same
        // stream and any separator would land inside a word.
        tentative.push_str(&self.primary_tentative);

        if !force && committed == self.published_committed && tentative == self.published_tentative
        {
            return;
        }
        self.published_committed = committed.clone();
        self.published_tentative = tentative.clone();

        self.tm.emit_composed_stream_text(
            &committed,
            &tentative,
            (self.failed_chunks > 0).then_some(self.failed_chunks),
        );

        if self.owns_typing {
            let mut full = committed;
            full.push_str(&tentative);
            self.push_writer_target(&full);
        }
    }

    /// Hand the writer the text the app should be showing.
    ///
    /// A target that *extends* what was already pushed goes out immediately —
    /// that is ordinary live typing. A target that rewrites it (a merge replaced
    /// text the writer already typed) is pushed only once the writer has caught
    /// up: it reaches a revision by backspacing the divergence and retyping, and
    /// a revision handed to a writer that is still typing would leave it
    /// permanently behind, typing text the user has already watched being
    /// replaced. The flush at the end always applies the latest text.
    fn push_writer_target(&mut self, target: &str) {
        let Some(writer) = &self.writer else {
            return;
        };
        if target == self.writer_pushed {
            return;
        }
        if target.starts_with(&self.writer_pushed) || writer.is_caught_up() {
            writer.update_target(target.to_string());
            self.writer_pushed = target.to_string();
        }
    }

    /// Close the session: apply the last chunk, hand back the session's text and
    /// release the tap and the sink.
    fn finish(&mut self) -> Option<MultiSttStreamOutcome> {
        // The stream has been finalized by the action, so the snapshot is
        // already the session's final text; absorbing it here is what gives the
        // last chunk its last words.
        self.tap.take_into(&mut self.scratch);
        if !self.scratch.is_empty() {
            self.open.audio.extend_from_slice(&self.scratch);
            self.scratch.clear();
        }
        self.absorb_stream_text();
        self.wait_for_merge();

        // Close the open chunk whole — its text and audio as they stand, cut or
        // not — and merge it synchronously, so the session's last words go
        // through the same pipeline as every other chunk.
        let job = self.open_job(true);
        if !job.live.trim().is_empty() || !job.audio.is_empty() {
            let settings = get_settings(&self.app);
            let result = tauri::async_runtime::block_on(run_merge_job(
                Arc::clone(&self.tm),
                settings,
                job,
                self.merge.generation,
            ));
            self.apply_job_result(result);
        }

        let last = std::mem::replace(&mut self.open, Chunk::new(self.next_chunk_id, 0));
        self.next_chunk_id += 1;
        self.closed.push(last);

        let mut final_text = String::new();
        for chunk in &self.closed {
            append_join(&mut final_text, &chunk.display_text());
        }

        // Slot 1 is the streaming model's own live text for the session; the
        // other three are the extras' outputs as they were accumulated.
        let mut model_outputs: [String; 4] = Default::default();
        model_outputs[0] = self.primary_text.clone();
        for (target, source) in model_outputs
            .iter_mut()
            .skip(1)
            .zip(self.outputs.iter_mut().skip(1))
        {
            *target = std::mem::take(source);
        }

        if let Some(writer) = self.writer.take() {
            // The merges revised text the writer has already typed; the flush
            // retypes the divergence and applies the trailing space / newline /
            // auto-submit behaviour.
            writer.flush(Some(final_text.clone()));
        }

        info!(
            "Multi-STT streaming: session finished — {} chunks, {} failed, {} chars",
            self.closed.len(),
            self.failed_chunks,
            final_text.chars().count()
        );

        Some(MultiSttStreamOutcome {
            final_text,
            model_outputs,
            brain: self.brain.take(),
            failed_chunks: self.failed_chunks,
            chunk_count: self.closed.len() as u32,
            owns_typing: self.owns_typing,
            decode_latency_ms: self.decode_latency_ms,
            merge_latency_ms: self.merge_latency_ms,
        })
    }

    /// Wait for the in-flight merge to land, so its result is part of the session
    /// rather than racing the compose.
    fn wait_for_merge(&mut self) {
        let deadline = Instant::now() + FINISH_TIMEOUT;
        while self.merge.running {
            // Drain everything that has landed. An abandoned job's result is
            // applied here like any other, but it does not end the wait: the job
            // the session is actually waiting for is still out.
            while self.collect_merge_result() {}
            if !self.merge.running {
                break;
            }
            if Instant::now() >= deadline {
                warn!("Multi-STT streaming: the last chunk merge did not land in time");
                self.merge.abandon();
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        // A result that landed just after the awaited one still belongs to the
        // session's text, and the compose below has not run yet.
        while self.collect_merge_result() {}
    }

    /// Release everything the session held. `finished` means `finish()` already
    /// took the writer and the text; otherwise nothing more is typed and nothing
    /// is pasted, matching a cancelled recording's behaviour elsewhere.
    fn shutdown(&mut self, finished: bool) {
        // First: the session is over the moment its thread starts releasing, so
        // `is_active` stops reporting it to a `stop` that may be racing it.
        self.alive.store(false, Ordering::Release);
        self.tap.end(self.token);
        // The sink is the session's; clearing it hands the overlay back to the
        // plain path's events (a later recording that is not in this mode).
        self.tm.set_stream_text_sink(None, false);
        self.merge.clear();
        if !finished {
            if let Some(writer) = self.writer.take() {
                writer.cancel();
            }
            debug!(
                "Multi-STT streaming: session stopped after {} chunks",
                self.closed.len()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 16 samples per millisecond, so a chunk's audio length is its duration in
    /// milliseconds times `SAMPLES_PER_MS`.
    fn audio_of_ms(ms: i64) -> Vec<f32> {
        vec![0.0; (ms * SAMPLES_PER_MS) as usize]
    }

    #[test]
    fn plain_sentence_ends_are_found() {
        assert_eq!(sentence_ends("One. Two! Three?"), vec![4, 9, 16]);
        assert_eq!(sentence_ends("no end here"), Vec::<usize>::new());
        // Byte offsets: `…` is 3 bytes, so the end is at 8 + 3. The space that
        // follows stays with the next chunk.
        assert_eq!(sentence_ends("trailing… "), vec![11]);
    }

    #[test]
    fn a_decimal_point_is_not_a_sentence_end() {
        // A cut inside a number would split one value across two merges.
        assert_eq!(
            sentence_ends("The value is 3.14 exactly"),
            Vec::<usize>::new()
        );
        // A digit that follows only after a space is a new sentence, though.
        assert_eq!(sentence_ends("Version 1. 2 is old"), vec![10]);
    }

    #[test]
    fn trailing_closers_belong_to_the_sentence() {
        // The first end is 16 — one past the closing quote, not one past the
        // `.` at 14 — and the final `.` is an end of its own.
        assert_eq!(
            sentence_ends(r#"He said "Hello." Then left."#),
            vec![16, 27]
        );
        assert_eq!(sentence_ends("(Really?) Yes."), vec![9, 14]);
    }

    #[test]
    fn cjk_terminators_are_found() {
        // Three bytes per character, so the offsets are 3 × the character count.
        assert_eq!(sentence_ends("你好。我很好！"), vec![9, 21]);
    }

    #[test]
    fn an_abbreviation_is_taken_as_an_end() {
        // Documented false positive: it costs one extra decode of a few seconds
        // of audio, and the tail carries over verbatim, so no text is lost.
        assert_eq!(sentence_ends("Mr. Smith left."), vec![3, 15]);
    }

    #[test]
    fn the_split_point_keeps_the_following_whitespace() {
        // The point is that concatenating the two halves reproduces the stream's
        // own text byte for byte, so nothing invents or drops a space.
        let text = "One. Two. Three.";
        let at = sentence_ends(text)[1];
        let (head, tail) = text.split_at(at);
        assert_eq!(format!("{head}{tail}"), text);
        assert_eq!(head, "One. Two.");
        assert_eq!(tail, " Three.");
    }

    #[test]
    fn append_join_inserts_one_space_only_when_needed() {
        let mut out = String::from("said.");
        append_join(&mut out, "Hello");
        assert_eq!(out, "said. Hello");

        let mut out = String::from("said. ");
        append_join(&mut out, "Hello");
        assert_eq!(out, "said. Hello");

        let mut out = String::from("said.");
        append_join(&mut out, " Hello");
        assert_eq!(out, "said. Hello");

        // A chunk's tail always starts with the split's own whitespace, so
        // joining chunks never doubles a space.
        let mut out = String::new();
        append_join(&mut out, "你好。");
        append_join(&mut out, " 我很好！");
        assert_eq!(out, "你好。 我很好！");
    }

    #[test]
    fn a_cut_keeps_every_sample_and_biases_backwards() {
        let mut audio = audio_of_ms(10_000);
        let original = audio.len();
        let (tail, tail_start) = split_audio(&mut audio, 0, 5_000).expect("a cut is available");
        assert_eq!(audio.len() + tail.len(), original);
        // Biased back by the bias, so the next chunk re-decodes a little of the
        // seam instead of starting after it.
        assert_eq!(
            tail_start,
            5_000 * SAMPLES_PER_MS - SENTENCE_BOUNDARY_BIAS_MS * SAMPLES_PER_MS
        );
        assert_eq!(audio.len() as i64, tail_start);
    }

    #[test]
    fn a_cut_is_relative_to_the_chunks_own_start() {
        // The second chunk starts 8 s into the stream, so a sentence end at
        // 10 s is 2 s into its own audio — not 10.
        let mut audio = audio_of_ms(4_000);
        let (tail, tail_start) =
            split_audio(&mut audio, 8_000 * SAMPLES_PER_MS, 10_000).expect("a cut is available");
        assert_eq!(
            audio.len() as i64,
            (2_000 - SENTENCE_BOUNDARY_BIAS_MS) * SAMPLES_PER_MS
        );
        assert_eq!(tail_start, 8_000 * SAMPLES_PER_MS + audio.len() as i64);
        assert_eq!(
            tail.len(),
            ((4_000 - 2_000 + SENTENCE_BOUNDARY_BIAS_MS) * SAMPLES_PER_MS) as usize
        );
    }

    #[test]
    fn a_cut_that_leaves_too_little_audio_is_refused() {
        let mut audio = audio_of_ms(4_000);
        assert!(cut_index(0, audio.len(), 600).is_none());
        assert!(split_audio(&mut audio, 0, 600).is_none());
        assert_eq!(audio.len(), (4_000 * SAMPLES_PER_MS) as usize);
    }

    #[test]
    fn a_receding_cut_covers_the_chunk_exactly() {
        // The context sentence's audio leaves with its text, so the closed part
        // and the carried tail still meet at the cut and cover the chunk between
        // them: a receding cut changes where the seam is, not how much audio
        // either side gets.
        let mut audio = audio_of_ms(6_000);
        let total = audio.len();
        let (tail, tail_start) = split_audio(&mut audio, 0, 4_000).expect("a cut is available");
        assert_eq!(audio.len() + tail.len(), total);
        assert_eq!(tail_start, audio.len() as i64);
        assert_eq!(
            audio.len() as i64,
            (4_000 - SENTENCE_BOUNDARY_BIAS_MS) * SAMPLES_PER_MS
        );
    }

    /// `decide` with the settings that reproduce the pre-window behaviour — no
    /// context sentence, no sentence cap — so these cases read as they did.
    fn decide_plain(
        ends: &[(usize, i64)],
        paused: bool,
        chunk_ms: i64,
        audio_start_sample: i64,
        audio_len: usize,
    ) -> Step {
        decide(
            ends,
            0,
            usize::MAX,
            paused,
            // A chunk is only asked about once the streaming model has committed
            // text to it; the cases that need the empty-chunk arm pass it through
            // [`decide`] itself.
            true,
            chunk_ms,
            audio_start_sample,
            audio_len,
        )
    }

    /// A `JobResult` for `generation`, empty of everything else, which is all
    /// the merge bookkeeping looks at.
    fn job_result(generation: u64, chunk_id: u64) -> JobResult {
        JobResult {
            generation,
            chunk_id,
            live_len: 0,
            record_outputs: false,
            merged: None,
            outputs: Default::default(),
            brain: None,
            failed: false,
            retry_audio: None,
            decode_latency_ms: 0.0,
            merge_latency_ms: 0.0,
        }
    }

    #[test]
    fn a_result_from_an_abandoned_merge_does_not_release_the_session() {
        // The watchdog gives up on a job it cannot cancel, and a new one starts.
        // When the abandoned job finally finishes, its result must be applied —
        // its chunk has been waiting for it — but it must not retire the job
        // that is genuinely in flight: that would let a third merge start
        // alongside the second.
        let mut merge = MergeQueue::new();
        let abandoned = merge.next_generation();
        merge.abandon();
        let current = merge.next_generation();

        merge.handle().push(job_result(abandoned, 1));
        let (taken, releases) = merge.take().expect("the abandoned result is applied");
        assert_eq!(taken.chunk_id, 1);
        assert!(!releases, "an abandoned result must not retire the session");
        assert!(merge.running, "the current job is still in flight");

        merge.handle().push(job_result(current, 2));
        let (taken, releases) = merge.take().expect("the current result is applied");
        assert_eq!(taken.chunk_id, 2);
        assert!(releases, "the awaited result retires the session");
        assert!(!merge.running);

        assert!(merge.take().is_none(), "the queue is empty");
    }

    #[test]
    fn a_second_result_never_overwrites_the_first() {
        // One slot held one result: a dispatch that overtook an abandoned job's
        // result lost that chunk's merge for good, with its audio already taken
        // off the chunk so there was nothing left to retry from.
        let mut merge = MergeQueue::new();
        let first = merge.next_generation();
        // The second dispatch happens because the watchdog gave up on the first,
        // which is still running detached.
        merge.abandon();
        let second = merge.next_generation();

        merge.handle().push(job_result(first, 1));
        merge.handle().push(job_result(second, 2));

        let ids: Vec<u64> = std::iter::from_fn(|| merge.take().map(|(r, _)| r.chunk_id)).collect();
        assert_eq!(ids, vec![1, 2], "both results come back, oldest first");
    }

    #[test]
    fn a_break_with_nothing_committed_does_not_polish() {
        // A merge covering no text is not a replacement: `display_text` would
        // append the whole live text to its result once the model commits, so
        // the same words would be shown twice. The chunk waits instead.
        let audio_len = audio_of_ms(4_000).len();
        assert_eq!(
            decide(&[], 1, usize::MAX, true, false, 4_000, 0, audio_len),
            Step::Idle
        );
        // The same state with text to replace is the polish it always was.
        assert_eq!(
            decide(&[], 1, usize::MAX, true, true, 4_000, 0, audio_len),
            Step::Polish
        );
    }

    #[test]
    fn no_break_and_no_valve_is_idle() {
        assert_eq!(
            decide_plain(&[(10, 3_000)], false, 4_000, 0, audio_of_ms(4_000).len()),
            Step::Idle
        );
        assert_eq!(decide_plain(&[], false, 4_000, 0, 16_000), Step::Idle);
    }

    #[test]
    fn a_break_closes_a_multi_sentence_chunk_at_its_last_sentence_end() {
        // Three sentences in one chunk: the run between two long breaks is the
        // merge unit, and with no context sentence the cut is the last end in it.
        let text = "First one. Second one. Third one.";
        let ends = sentence_ends(text);
        assert_eq!(ends.len(), 3);
        let timed: Vec<(usize, i64)> = ends.iter().copied().zip([2_000, 5_000, 8_000]).collect();
        let last = ends[2];
        let audio_len = audio_of_ms(9_000).len();
        assert_eq!(
            decide_plain(&timed, true, 9_000, 0, audio_len),
            Step::Close {
                offset: last,
                audio_ms: 8_000
            }
        );
        // Everything up to the cut is what this chunk merges; the text past it
        // goes to the next chunk.
        assert_eq!(&text[..last], "First one. Second one. Third one.");
    }

    #[test]
    fn a_context_sentence_is_carried_into_the_next_window() {
        // Four sentence ends and one context sentence: the cut lands at the third
        // end, so the text that carries into the next chunk begins one complete
        // sentence back — the "previous sentence" the next merge window leads
        // with, and the reason a wrongly ended sentence can still be joined.
        let text = "One. Two. Three. Four.";
        let ends = sentence_ends(text);
        assert_eq!(ends.len(), 4);
        let timed: Vec<(usize, i64)> = ends
            .iter()
            .copied()
            .zip([2_000, 4_000, 6_000, 8_000])
            .collect();
        let audio_len = audio_of_ms(9_000).len();
        assert_eq!(
            decide(&timed, 1, usize::MAX, true, true, 9_000, 0, audio_len),
            Step::Close {
                offset: ends[2],
                audio_ms: 6_000
            }
        );
        // A second context sentence reaches one sentence further back.
        assert_eq!(
            decide(&timed, 2, usize::MAX, true, true, 9_000, 0, audio_len),
            Step::Close {
                offset: ends[1],
                audio_ms: 4_000
            }
        );
    }

    #[test]
    fn fewer_sentences_than_the_context_polishes_instead_of_cutting() {
        // The chunk leads with its context sentence and the speaker pauses inside
        // the next one: there is no end behind a cut yet, so the break polishes
        // the whole chunk — which is exactly [the previous sentence + what has
        // been spoken], the window the extras need to join the two halves.
        let one = [(sentence_ends("One. Two")[0], 2_000)];
        let audio_len = audio_of_ms(4_000).len();
        assert_eq!(
            decide(&one, 1, usize::MAX, true, true, 4_000, 0, audio_len),
            Step::Polish
        );
        // And with no pause there is nothing to do: a merge mid-speech with no
        // break behind it would be thrown away.
        assert_eq!(
            decide(&one, 1, usize::MAX, false, true, 4_000, 0, audio_len),
            Step::Idle
        );
    }

    #[test]
    fn the_sentence_cap_closes_a_chunk_the_speaker_never_pauses_in() {
        let text = "One. Two. Three. Four.";
        let ends = sentence_ends(text);
        let timed: Vec<(usize, i64)> = ends
            .iter()
            .copied()
            .zip([2_000, 4_000, 6_000, 8_000])
            .collect();
        let audio_len = audio_of_ms(10_000).len();
        // Three sentences in the chunk and no break at all: the cap is what
        // closes it, at the cut the context sentence leaves behind.
        assert_eq!(
            decide(&timed[..3], 1, 3, false, true, 10_000, 0, audio_len),
            Step::Close {
                offset: ends[1],
                audio_ms: 4_000
            }
        );
        // Below the cap, and with no break, nothing happens yet.
        assert_eq!(
            decide(&timed[..2], 1, 3, false, true, 10_000, 0, audio_len),
            Step::Idle
        );
    }

    #[test]
    fn a_cap_with_nothing_to_cut_at_waits_for_the_next_sentence() {
        // The cap is reached but the only ends are too early in the chunk to cut
        // at: under `SENTENCE_BOUNDARY_BIAS_MS` the candidate leaves less than
        // `MIN_CLOSE_MS` of audio behind it. Nothing is closed and nothing is
        // polished mid-speech: the next sentence end is what makes the cut
        // possible.
        let ends = [(5usize, 300i64), (9, 450), (20, 700)];
        let audio_len = audio_of_ms(4_000).len();
        assert_eq!(
            decide(&ends, 0, 3, false, true, 4_000, 0, audio_len),
            Step::Idle
        );
        // Under a break the same state is worth polishing, as it always was.
        assert_eq!(
            decide(&ends, 0, 3, true, true, 4_000, 0, audio_len),
            Step::Polish
        );
    }

    #[test]
    fn a_cap_at_or_below_the_context_can_still_cut() {
        // A cap of one sentence with one context sentence would ask for a close
        // on every tick and never get one; it is clamped to what can cut.
        assert_eq!(sentence_cap(0, 1), usize::MAX);
        assert_eq!(sentence_cap(1, 1), 2);
        assert_eq!(sentence_cap(2, 3), 4);
        assert_eq!(sentence_cap(5, 1), 5);

        let text = "One. Two.";
        let ends = sentence_ends(text);
        let timed: Vec<(usize, i64)> = ends.iter().copied().zip([2_000, 4_000]).collect();
        let cap = sentence_cap(1, 1);
        assert_eq!(
            decide(
                &timed,
                1,
                cap,
                false,
                true,
                6_000,
                0,
                audio_of_ms(6_000).len()
            ),
            Step::Close {
                offset: ends[0],
                audio_ms: 2_000
            }
        );
    }

    #[test]
    fn a_cut_moves_the_carried_sentence_ends_into_the_tail() {
        // The ends past the cut travel with the text that carries over, and keep
        // the time each was first seen with: re-timing them at the next revision
        // would put them after the sentence they end, inside the pause.
        let mut ends = vec![(10, 2_000), (20, 4_000), (30, 6_000)];
        recede_ends(&mut ends, 20);
        assert_eq!(ends, vec![(10, 6_000)]);

        // An end exactly at the cut belongs to the closed text, and a tail with
        // no end of its own carries none.
        let mut none = vec![(10, 2_000)];
        recede_ends(&mut none, 10);
        assert!(none.is_empty());
    }

    #[test]
    fn a_run_with_no_pause_is_merged_in_bounded_windows() {
        // The default settings and a speaker who never pauses: every close takes
        // the cap's worth of ends and leaves `context` of them behind, so each
        // window is `cap - context` sentences and each close moves forward — the
        // window never grows with how long the session has been running.
        let (cap, context) = (3usize, 1usize);
        let audio_len = audio_of_ms(30_000).len();
        // The ends that survived the last close lead the next chunk; the speaker
        // then adds sentence ends until the cap is reached again.
        let mut ends: Vec<(usize, i64)> = Vec::new();
        let mut spoken = 0i64;
        for close in 0..4 {
            while ends.len() < cap {
                spoken += 1;
                ends.push((spoken as usize * 10, 2_000 + spoken * 2_000));
            }
            assert_eq!(ends.len(), cap, "the cap is what triggered close {}", close);

            let Step::Close { offset, .. } =
                decide(&ends, context, cap, false, true, 30_000, 0, audio_len)
            else {
                panic!("close {} did not cut at a sentence end", close);
            };
            let index = ends
                .iter()
                .position(|(o, _)| *o == offset)
                .expect("the cut is an end of this chunk");
            assert_eq!(index + 1, cap - context, "the window's sentence count");

            recede_ends(&mut ends, offset);
            assert_eq!(ends.len(), context, "and the next chunk leads with them");
        }
    }

    #[test]
    fn a_break_without_a_sentence_end_polishes_and_keeps_the_chunk_open() {
        // A pause mid-sentence must not freeze half a sentence: the chunk stays
        // open, accumulating, so its eventual merge has the whole sentence.
        let audio_len = audio_of_ms(4_000).len();
        assert_eq!(decide_plain(&[], true, 4_000, 0, audio_len), Step::Polish);
        // A sentence end too early in the chunk to cut at behaves the same way.
        assert_eq!(
            decide_plain(&[(5, 600)], true, 4_000, 0, audio_len),
            Step::Polish
        );
    }

    #[test]
    fn a_tiny_chunk_is_not_polished() {
        // Under the minimum there is nothing worth three decodes and a merge.
        assert_eq!(
            decide_plain(&[], true, 300, 0, audio_of_ms(300).len()),
            Step::Idle
        );
    }

    #[test]
    fn the_valve_closes_a_chunk_that_never_punctuates() {
        let chunk_ms = MAX_CHUNK_SECONDS * 1000;
        assert_eq!(
            decide_plain(&[], false, chunk_ms, 0, audio_of_ms(chunk_ms).len()),
            Step::CloseAll
        );
        // With a sentence end in the first moments of the chunk — too early to
        // cut at — closing whole is the only way to bound it.
        assert_eq!(
            decide_plain(
                &[(10, 600)],
                false,
                chunk_ms,
                0,
                audio_of_ms(chunk_ms).len()
            ),
            Step::CloseAll
        );
    }

    #[test]
    fn the_valve_cuts_at_a_sentence_end_when_one_is_reachable() {
        let chunk_ms = MAX_CHUNK_SECONDS * 1000;
        let audio_len = audio_of_ms(chunk_ms).len();
        assert_eq!(
            decide_plain(&[(10, 30_000)], false, chunk_ms, 0, audio_len),
            Step::Close {
                offset: 10,
                audio_ms: 30_000
            }
        );
    }

    #[test]
    fn a_chunk_without_a_merge_shows_its_live_text() {
        let mut chunk = Chunk::new(1, 0);
        chunk.live = "hello world and more".to_string();
        assert_eq!(chunk.display_text(), "hello world and more");
    }

    #[test]
    fn a_merge_replaces_only_the_text_it_covered() {
        let mut chunk = Chunk::new(1, 0);
        chunk.live = "hello world. and the next".to_string();
        // A merge of the first sentence only: the rest has not been merged yet
        // and must still be on screen, verbatim.
        chunk.apply_merge("Hello, world.".to_string(), "hello world.".len(), false);
        assert_eq!(chunk.display_text(), "Hello, world. and the next");
    }

    #[test]
    fn a_merge_never_splits_a_word_it_half_covers() {
        // The polish lands mid-word because the model has not committed the rest
        // yet: the tail is the continuation of the text the merge covered, so it
        // is appended verbatim and no space is invented inside the word.
        let mut chunk = Chunk::new(1, 0);
        chunk.live = "hello wor".to_string();
        chunk.apply_merge("Hello, wor".to_string(), "hello wor".len(), false);
        assert_eq!(chunk.display_text(), "Hello, wor");

        let mut chunk = Chunk::new(2, 0);
        chunk.live = "hello world".to_string();
        chunk.apply_merge("Hello, wor".to_string(), "hello wor".len(), false);
        assert_eq!(chunk.display_text(), "Hello, world");
    }

    #[test]
    fn the_fallback_puts_every_model_on_its_own_line() {
        let mut outputs: [String; 4] = Default::default();
        outputs[1] = "second".to_string();
        outputs[2] = String::new();
        outputs[3] = "fourth".to_string();
        assert_eq!(concatenate("first", &outputs), "first\nsecond\nfourth");
        assert_eq!(concatenate("", &Default::default()), "");
    }
}
