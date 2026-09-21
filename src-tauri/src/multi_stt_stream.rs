//! Experimental Multi-STT streaming mode: the primary streaming model gives an
//! instant rough text, and the Multi-STT pipeline replaces it in place, chunk by
//! chunk, as the speaker pauses.
//!
//! # A chunk is the audio between two breaks
//!
//! A **chunk** is the run of audio between two breaks. The speaker stops for
//! `multi_stt_streaming_pause_ms` — settable from 100 ms to 10 s — and the chunk
//! that was open closes there. *Every* break closes a chunk: there is no second
//! condition to satisfy and nothing to detect in the text, so a session of any
//! length is just a list of slices with an obvious start and end. The close is
//! decided on the audio alone.
//!
//! [`MAX_CHUNK_SECONDS`] (60 s) is the one other trigger, and it is a valve
//! rather than a policy: someone who talks for a minute without pausing still
//! gets merged, and no chunk can grow without bound.
//!
//! # The audio sent to the extras is a window, never the session
//!
//! Feeding the extra models everything since the recording began is what makes
//! the mode useless on a long session: every break would re-decode the whole
//! dictation, so the cost would grow with the session instead of with the chunk.
//! A merge is therefore fed `multi_stt_streaming_context_chunks` (0–3, default 1)
//! already-closed chunks followed by the chunk that just closed: a window of at
//! most four chunks, flat in the length of the session.
//!
//! The window is not free. The extras' decode covers the context's words as well
//! as the chunk's, while the text the merge replaces is the chunk's alone. Each
//! extra's decode is therefore **cropped** back to the chunk by
//! [`strip_context_prefix`], against the context chunks' own displayed text —
//! which is the text already on screen, so the ruler is exactly the words the
//! user is looking at. Every slot of the merge then covers the same span (the
//! chunk), which is what lets a merge be applied to one chunk and to nothing
//! else. A decode the crop cannot align is dropped for that chunk rather than
//! guessed at.
//!
//! That per-chunk ownership is the whole design. A merge that covered
//! `[context chunks + chunk]` and replaced all of it would overlap the previous
//! merge's window by `context` chunks, and folding two overlapping windows
//! either drops the chunk that left the window or shows the context's words
//! twice. Here the context is only ever an *input* to the merge: the text it
//! produces belongs to the chunk, earlier chunks keep their own, and nothing is
//! dropped, duplicated or shown twice.
//!
//! # What the tap holds
//!
//! The tap receives the same VAD-filtered frames the primary stream is fed, in
//! the same order, so a chunk's audio is the recording's own speech without the
//! silence the VAD dropped, and `audio_committed_ms` is a coordinate in that same
//! stream. It is read at each close to *report* how far behind the stream's
//! committed text is (the `chunk n closed` log line) and never to cut anything:
//! audio and text are both taken whole, so a mis-timed hint cannot shift a seam.
//!
//! # Shape
//!
//! One thread per recording, ticked every [`TICK`], the shape of
//! `LiveModeManager::run_session`'s inner loop. Per tick it drains the mid-
//! recording audio tap (the copy of the frames the primary model is being fed),
//! absorbs the stream's latest text, tests for a break and drives the chunk
//! lifecycle. At most one merge job runs at a time: a break that arrives while
//! one is in flight is acted on as soon as it lands, and if the speaker resumes
//! before that the chunk simply keeps the speech that followed.

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
use crate::managers::transcription::{StreamTiming, TranscriptionManager, real_time_factor};
use crate::settings::{AppSettings, PasteMethod, get_settings};

/// The rate the tap and the streaming model both run at (see [`ChunkTap`]).
const SAMPLES_PER_MS: i64 = 16;

/// A chunk that has grown this long without a break is closed anyway. A valve
/// for someone who never pauses — deliberately not a setting: it only exists so
/// that no chunk and no merge window can grow without bound.
const MAX_CHUNK_SECONDS: i64 = 60;

/// The most already-closed chunks a merge window may carry. Matches the range the
/// settings page offers and caps what a hand-edited store can ask for: each
/// context chunk is up to [`MAX_CHUNK_SECONDS`] of audio held in memory and
/// re-decoded by three models.
const MAX_CONTEXT_CHUNKS: usize = 3;

/// How much of the context's tail the crop tries to find in an extra's decode,
/// longest first. The tail is what abuts the chunk, so it is the boundary being
/// sought; a longer run is a more certain one, and the shorter lengths are only
/// reached when a model disagrees with the primary over the last words.
const CONTEXT_TAIL_TOKENS: usize = 12;

/// The shortest run of context words a crop will accept as a boundary. Below
/// this a "match" is as likely to be a coincidence somewhere else in the decode
/// as the real seam, and cropping there would cut the chunk's own words off.
const CONTEXT_TAIL_MIN_TOKENS: usize = 3;

/// How many of the context's last words a decode may disagree about and still be
/// cropped. The word at the seam is the one every model is least sure of — it is
/// the word the break fell after, so it is the one with speech running up to the
/// chunk's first sample — and without this slack a single mis-heard word there
/// would defeat *every* anchor length at once, because every one of them ends at
/// that word.
const CONTEXT_SEAM_DRIFT: usize = 2;

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

/// How much audio the stream may have been fed and not yet drained and still
/// count as having reached the end of the chunk's audio.
///
/// Not zero, and that is the whole of this constant. `audio_committed_ms` is the
/// family's *drain hint* — `transcribe.h` says so in as many words ("Family-
/// reported audio progress / drain hint. It is not a byte boundary into
/// committed_text."), and Parakeet derives it from `mel_frames_consumed`, i.e.
/// from how much audio the model has decoded, not from how much text it has
/// published. Every streaming family keeps audio in flight while it runs: a
/// right-context window it will not emit a word without. The `buffered` figure
/// `Live preview perf` logs is this same difference, and on the model this was
/// measured against it sat between 22 and 86 ms while decoding at 2.4x real
/// time. Zero is therefore not "late", it is unreachable — and a break makes it
/// more so, not less: the VAD feeds the stream nothing while the pause lasts, so
/// the drain hint has no new input to advance on and the residual *freezes*. The
/// log still reported the same 22 ms 3.5 s after a break, at which point the
/// grace ran out and the session retired. With the old exact-zero test, every
/// break waited out the grace, every session retired with zero chunks merged,
/// and the mode corrected nothing on any recording.
///
/// A tolerance is safe for the same reason the wait for it is cheap: a break is
/// `multi_stt_streaming_pause_ms` of *silence*, so the un-drained tail of a
/// closed chunk is the end of the audio the stream was fed — a suffix, because a
/// decoder consumes in order — and a suffix this short lies inside that silence
/// and cannot hold a word that has not been written yet. Keep it below the
/// smallest break for that to hold. It is also an order of magnitude below the
/// backlog this must still catch: the model that motivated [`Break::Retire`]
/// measured 4918 ms.
const STREAM_DRAIN_TOLERANCE_MS: i64 = 500;

/// How long a break waits for the chunk's own text to arrive before the chunk is
/// closed with whatever has arrived.
///
/// A break is decided on the audio alone (`closes`), but the primary's text for
/// that audio arrives on the model's own schedule, and the mode's invariant —
/// *a chunk is merged against its own text, and its merged text replaces that
/// text and nothing else* — is only true if the text is there. Two things can be
/// missing, and they are independent:
///
/// - **The audio is not decoded yet** ([`STREAM_DRAIN_TOLERANCE_MS`]). Then the
///   chunk's last words have not been read at all, and they will be read while
///   the *next* chunk is open: they land in its `live` text, whose merged text
///   already contains them. The same speech, twice.
/// - **The audio is decoded but the text is not published** (`Chunk::live` is
///   empty or short). Then slot 1 is missing text the extras' decode of the same
///   audio does contain, and the merge — which is a rewrite, not an append —
///   replaces the chunk's text with a version that cannot include words the
///   primary never gave it. The same speech, twice, in the other direction.
///
/// A drain hint cannot stand in for the second: a family that decodes eagerly and
/// commits late has drained completely while its text is still owed, so the
/// closeness test passes while slot 1 is empty. That case is what
/// [`break_outcome`] reads `has_text` for.
///
/// Waiting is cheap because a pause is silence by definition: the VAD feeds the
/// stream nothing while it lasts, so the only work left is the model's backlog
/// and it collapses on its own.
///
/// A chunk that is *still* owed its text when the grace runs out is the end of
/// the mode for that session, because the wait is the mode's premise. The
/// session retires itself instead of closing the chunk, and the batch path
/// produces the text (see [`Coordinator::step`]): a model that publishes its
/// committed text only at finalize, or one that cannot decode faster than the
/// user speaks, has a lag that never collapses, and no routing change can
/// recover text that does not exist yet.
const TEXT_CATCHUP_GRACE: Duration = Duration::from_millis(2500);

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

/// Whether `c` belongs to a script that does not put spaces between words.
///
/// The ranges are the overlay's own CJK set (`CJK_CHARS` in
/// `RecordingOverlay.tsx`), widened to the whole fullwidth-forms block and to
/// CJK punctuation, so `。` and `，` count as well. Chinese and Japanese supply
/// no inter-word whitespace to preserve, and inventing one splits a word — see
/// [`append_join`].
fn is_unspaced_script(c: char) -> bool {
    matches!(c as u32,
        0x3000..=0x303F      // CJK punctuation: 。、「」・
        | 0x3040..=0x30FF    // Hiragana, Katakana (with their extensions)
        | 0x3400..=0x4DBF    // CJK Unified Ideographs Extension A
        | 0x4E00..=0x9FFF    // CJK Unified Ideographs
        | 0xF900..=0xFAFF    // CJK Compatibility Ideographs
        | 0xFF00..=0xFFEF    // Halfwidth and Fullwidth Forms
    )
}

/// Append `piece` to `out`, inserting one space if neither side supplies one.
///
/// Merge results are trimmed and the streaming text is not, so the separator
/// cannot be left to either side: without this, `...said.` + `Hello` would run
/// together. When both sides came from the same streaming text the rule is a
/// no-op, because the split kept the original whitespace — and that is exactly
/// what does not hold for a script that puts no space between its words. Two
/// consecutive Chinese chunks are two halves of a word (`放射性` + `物质碘`), so
/// a space invented there is a defect rather than a separator. The space is
/// skipped only when *both* sides are of such a script, which leaves every
/// other boundary on the old rule.
fn append_join(out: &mut String, piece: &str) {
    if piece.is_empty() {
        return;
    }
    if !out.is_empty()
        && !out.ends_with(char::is_whitespace)
        && !piece.starts_with(char::is_whitespace)
        && !(out.ends_with(is_unspaced_script) && piece.starts_with(is_unspaced_script))
    {
        out.push(' ');
    }
    out.push_str(piece);
}

/// How many already-closed chunks a merge window carries in front of the one it
/// merges, from the setting. Clamped to [`MAX_CONTEXT_CHUNKS`] because the
/// store's JSON is hand-editable.
fn context_depth(raw: u32) -> usize {
    (raw as usize).min(MAX_CONTEXT_CHUNKS)
}

/// Whether the open chunk closes on this tick: a break, or the valve.
///
/// `has_audio` is the only structural guard there is, and it is not a policy:
/// a chunk with no audio behind it has nothing to merge, and dispatching a job
/// over an empty window would only cost three decodes of silence. Every break
/// that has audio behind it closes its chunk.
fn closes(paused: bool, has_audio: bool, chunk_ms: i64) -> bool {
    has_audio && (paused || chunk_ms >= MAX_CHUNK_SECONDS * 1000)
}

/// What a break in the audio does with the chunk that was open.
#[derive(Debug, PartialEq, Eq)]
enum Break {
    /// Nothing yet. Either the pause has not lasted long enough to be a break,
    /// or it is one and the chunk's own text is still owed: either the audio is
    /// not decoded ([`STREAM_DRAIN_TOLERANCE_MS`]) or the text is not published
    /// (`has_text`). The pause persists until a close consumes it, so waiting
    /// costs nothing but the silence it is already made of.
    Wait,
    /// Close the chunk and merge it.
    Close,
    /// Close nothing. The session's premise has failed — see
    /// [`TEXT_CATCHUP_GRACE`] — and the session retires itself.
    Retire,
}

/// Decide what a break does, from what the chunk holds and how long the pause
/// has lasted.
///
/// The chunk can be closed only when it owns the text its merge will replace,
/// which is two independent things and needs both to be true:
///
/// - `has_text`: the chunk's `live` text — the primary's own words for this
///   chunk's audio — is what a merge's slot 1 (`${output}`) is, and a merge is a
///   *replacement* of the chunk's text, not an append to it. A chunk with none
///   has nothing to replace, and the merge would be handed the extras' decode of
///   the audio with no primary reading to reconcile it against. Its own text
///   arrives during the next chunk's lifetime and is read into it, so the speech
///   ends up on screen twice.
/// - `stream_drain_ms` within [`STREAM_DRAIN_TOLERANCE_MS`]: the family has
///   decoded the audio up to the end of the chunk. Not the same test: a family
///   can drain its audio completely and publish the text for it later, which is
///   exactly the case `has_text` covers and a drain hint cannot.
///
/// A break that has waited past [`TEXT_CATCHUP_GRACE`] with either still false
/// is therefore not a chunk to close but a session to end: waiting longer cannot
/// produce text the model has not written, and closing anyway is the duplication
/// the wait exists to prevent.
fn break_outcome(
    closes: bool,
    has_text: bool,
    stream_drain_ms: i64,
    paused_for: Duration,
    pause: Duration,
) -> Break {
    if !closes {
        return Break::Wait;
    }
    if has_text && stream_drain_ms <= STREAM_DRAIN_TOLERANCE_MS {
        return Break::Close;
    }
    if paused_for >= pause + TEXT_CATCHUP_GRACE {
        return Break::Retire;
    }
    Break::Wait
}

/// A word of a text, folded for comparison, with the byte offset it starts at.
type Token = (usize, String);

/// Split a text into comparable words, each with its starting byte offset.
///
/// Case is folded so two models' decodes of the same speech compare equal, and
/// punctuation and whitespace are dropped: they are exactly what the models
/// disagree about. A run of ASCII letters and digits is one word (`don't` is
/// two, which is fine — both sides split it the same way), while any other
/// alphanumeric character is a word of its own, so a CJK clause becomes one word
/// per character instead of one word for the whole clause.
fn tokens(text: &str) -> Vec<Token> {
    let mut out: Vec<Token> = Vec::new();
    let mut open = false;
    for (index, ch) in text.char_indices() {
        let ascii = ch.is_ascii_alphanumeric();
        let own = !ascii && ch.is_alphanumeric();
        if !ascii && !own {
            open = false;
            continue;
        }
        // Mid-word: grow the run rather than starting a new one.
        if ascii
            && open
            && let Some(last) = out.last_mut()
        {
            last.1.extend(ch.to_lowercase());
            continue;
        }
        let mut folded = String::new();
        folded.extend(ch.to_lowercase());
        out.push((index, folded));
        open = ascii;
    }
    out
}

/// The byte offset at which `want` first occurs in `haystack`, token by token.
fn find_run(haystack: &[Token], want: &[Token]) -> Option<usize> {
    if want.is_empty() || want.len() > haystack.len() {
        return None;
    }
    (0..=haystack.len() - want.len()).find(|start| {
        want.iter()
            .zip(&haystack[*start..])
            .all(|(a, b)| a.1 == b.1)
    })
}

/// Crop one extra's decode of a merge window back to the window's last chunk.
///
/// The extras are fed `[the context chunks' audio] + [this chunk's]`, so their
/// text starts with the context's words. The merge needs the chunk's words only:
/// slot 1 is the primary's text for the chunk, and a merge whose slots cover
/// different spans has no single span to replace.
///
/// The anchor is a run of the context's words ending near the **end** of the
/// context, because that end is the boundary, and it is looked for longest run
/// first — the more words a run covers, the more certain it is the seam. Each
/// length is tried with no drift before any drift, and `drift` is how many of the
/// context's last words the decode is allowed to disagree about
/// ([`CONTEXT_SEAM_DRIFT`]).
///
/// `None` means no run of at least [`CONTEXT_TAIL_MIN_TOKENS`] words aligned:
/// the caller drops that model's output for this chunk, which costs one model's
/// opinion and never text that is already on screen. A context shorter than that
/// minimum is anchored on whatever it has, because a shorter anchor is a better
/// risk than dropping every extra whenever the previous chunk was a single word —
/// and the cost of being wrong is bounded in the safe direction: a run that
/// matched too late cuts words off this chunk's own text, it never repeats the
/// context's.
///
/// An empty context is not a failure but the setting's own 0 case — the extras
/// heard the chunk alone, so there is nothing to crop.
fn strip_context_prefix(decoded: &str, context: &str) -> Option<String> {
    let context = tokens(context);
    if context.is_empty() {
        return Some(decoded.trim().to_string());
    }
    let decoded_tokens = tokens(decoded);

    let longest = context.len().min(CONTEXT_TAIL_TOKENS);
    let shortest = CONTEXT_TAIL_MIN_TOKENS.min(context.len());
    for tail in (shortest..=longest).rev() {
        for drift in 0..=CONTEXT_SEAM_DRIFT {
            if context.len() < tail + drift {
                continue;
            }
            let want = &context[context.len() - tail - drift..context.len() - drift];
            let Some(start) = find_run(&decoded_tokens, want) else {
                continue;
            };
            // Past the run come the words this decode got wrong (the context's
            // own text already shows those, and it is not this chunk's to
            // replace), and only then this chunk's own words.
            let cut = decoded_tokens
                .get(start + tail + drift)
                .map_or(decoded.len(), |token| token.0);
            return Some(decoded[cut..].trim().to_string());
        }
    }
    None
}

/// Crop a decode, saying so when it cannot be done. The model's output is
/// dropped for this chunk rather than risk repeating text that is already on
/// screen or cutting the chunk's own words off.
fn crop_decode(model_id: &str, decoded: &str, context: &str) -> String {
    if decoded.trim().is_empty() {
        return String::new();
    }
    match strip_context_prefix(decoded, context) {
        Some(cropped) => cropped,
        None => {
            warn!(
                "Multi-STT streaming: '{}' decoded {} chars that could not be aligned with the \
                 {} chars of context text; dropping its output for this chunk",
                model_id,
                decoded.chars().count(),
                context.chars().count()
            );
            String::new()
        }
    }
}

// ---------------------------------------------------------------------------
// Chunk model
// ---------------------------------------------------------------------------

/// One chunk of the session: an append-only live text and the audio behind it.
struct Chunk {
    /// Monotonic within a session; a merge result is matched back to its chunk
    /// by this, because later chunks exist by the time a job lands.
    id: u64,
    /// The streaming model's own text for this chunk, append-only. It is a
    /// contiguous slice of the stream's committed text: each close hands the
    /// open chunk's whole text over and starts the next one empty.
    live: String,
    /// The merged text, once a merge has landed.
    merged_text: Option<String>,
    /// 16 kHz samples of this chunk's own audio. Kept after the chunk closes
    /// while it can still be a merge window's context, and freed after that (see
    /// `Coordinator::retain_context_audio`).
    audio: Vec<f32>,
    /// Whether the last merge for this chunk failed, so it is showing the
    /// extras' concatenated text rather than a merged one.
    failed: bool,
}

impl Chunk {
    fn new(id: u64) -> Self {
        Self {
            id,
            live: String::new(),
            merged_text: None,
            audio: Vec::new(),
            failed: false,
        }
    }

    /// What this chunk contributes to the session's text: the merged text once a
    /// merge has landed, the streaming model's own text until then.
    fn display_text(&self) -> String {
        self.merged_text
            .clone()
            .unwrap_or_else(|| self.live.clone())
    }

    fn duration_ms(&self) -> i64 {
        self.audio.len() as i64 / SAMPLES_PER_MS
    }

    /// Record a merge result for this chunk. A chunk's text is complete before
    /// its merge is dispatched — it closes first and is never appended to again
    /// — so there is no tail to keep outside the merged text.
    fn apply_merge(&mut self, text: String, failed: bool) {
        self.merged_text = Some(text);
        self.failed = failed;
    }
}

// ---------------------------------------------------------------------------
// Merge job
// ---------------------------------------------------------------------------

/// One chunk's merge input, snapshotted when the job is dispatched.
struct JobInput {
    chunk_id: u64,
    /// The primary model's own text for this chunk, which is what `${output}`
    /// (slot 1) receives.
    live: String,
    /// The window: the context chunks' audio, then this chunk's.
    audio: Vec<f32>,
    /// How many samples of `audio` belong to the context chunks, i.e. where this
    /// chunk's own audio starts. 0 when the window carries no context.
    context_samples: usize,
    /// The context chunks' displayed text, the ruler the extras' decodes are
    /// cropped against.
    context_text: String,
    /// Whether this job's per-model outputs belong in the session's history
    /// metadata. True for the merge that closes a chunk, false for a retry (the
    /// outputs are already recorded).
    record_outputs: bool,
}

struct JobResult {
    /// The dispatch generation this job was created under. See [`MergeQueue`].
    generation: u64,
    chunk_id: u64,
    /// `live.len()` when the job was dispatched.
    live_len: usize,
    record_outputs: bool,
    /// `None` when no merge text was produced; `outputs` still holds the extras.
    merged: Option<String>,
    outputs: [String; 4],
    brain: Option<MultiSttHistoryBrain>,
    /// A merge was configured and asked for but did not produce text.
    failed: bool,
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
        context_samples,
        context_text,
        record_outputs,
    } = input;
    let live_len = live.len();
    let decode_start = Instant::now();

    info!(
        "Multi-STT streaming: chunk {} merge started — {} ms of window audio ({} ms of it \
         context), {} chars to replace",
        chunk_id + 1,
        audio.len() as i64 / SAMPLES_PER_MS,
        context_samples as i64 / SAMPLES_PER_MS,
        live.chars().count()
    );

    // Each extra decodes the whole window. The clones are the window's samples —
    // at most four chunks, ~7.7 MB at the valve — against decodes that cost far
    // more.
    let spawn_extra = |slot: usize, model_id: Option<String>| {
        let tm = Arc::clone(&tm);
        let audio = audio.clone();
        let context = context_text.clone();
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
                    Ok(text) => (slot, crop_decode(&model_id, &text, &context)),
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

    info!(
        "Multi-STT streaming: chunk {} merge finished — {} in {} ms decode, {} in {} ms merge",
        chunk_id + 1,
        if failed { "failed" } else { "ok" },
        decode_latency_ms.round() as i64,
        if merged.is_some() { "text" } else { "no text" },
        merge_latency_ms.round() as i64
    );

    JobResult {
        generation,
        chunk_id,
        live_len,
        record_outputs,
        merged,
        outputs,
        brain,
        failed,
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

/// The context a merge window carries in front of `closed[index]`: the `depth`
/// chunks before it, oldest first, as audio and as the text they already show.
///
/// The walk stops at the first chunk whose audio has been freed: the text and
/// the audio must describe the same span — the text is the crop's ruler — so a
/// chunk that can no longer be heard cannot contribute its words.
fn window_context(closed: &[Chunk], index: usize, depth: usize) -> (Vec<f32>, String) {
    let mut chunks: Vec<&Chunk> = Vec::new();
    for candidate in closed[..index].iter().rev().take(depth) {
        if candidate.audio.is_empty() {
            break;
        }
        chunks.push(candidate);
    }
    chunks.reverse();

    let mut audio = Vec::new();
    let mut text = String::new();
    for chunk in chunks {
        audio.extend_from_slice(&chunk.audio);
        append_join(&mut text, &chunk.display_text());
    }
    (audio, text)
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

    // The primary model's rate is reported per chunk as a difference of these
    // totals, so the baseline has to be taken before the stream worker can feed
    // anything. `start` runs strictly before the action calls `start_stream`
    // (`actions.rs`), so reading them here is the session's zero point.
    let primary_timing = tm.stream_timing();
    let primary_seen = primary_timing.totals();

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
        stream_drained_ms: 0,
        stream_input_ms: 0,
        primary_timing,
        primary_seen,
        alive: Arc::clone(&alive),
        next_chunk_id: 1,
        open: Chunk::new(0),
        closed: Vec::new(),
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
        "Multi-STT streaming: armed (break threshold {} ms, up to {} context chunk(s), direct \
         typing: {})",
        get_settings(app).multi_stt_streaming_pause_ms,
        context_depth(get_settings(app).multi_stt_streaming_context_chunks),
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
    /// How much audio the committed text accounts for. Reported at each close,
    /// never used to cut (see the module docs).
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
/// was lost silently and permanently.
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
    /// The family's own drain cursor (`StreamText::audio_committed_ms`): how far
    /// into the audio the stream has decoded. Not text — `transcribe.h` is
    /// explicit that it is a hint and "not a byte boundary into
    /// committed_text", and Parakeet sets it from `mel_frames_consumed`. Kept
    /// for the `chunk n closed` log line and for [`STREAM_DRAIN_TOLERANCE_MS`],
    /// which is the only thing that may read it.
    stream_drained_ms: i64,
    /// How much audio the stream has been fed, in the same accounting. The
    /// difference to `stream_drained_ms` is the audio the family has taken in
    /// and not decoded — the un-drained suffix of the session, which is what
    /// `Live preview perf` reports as `buffered`.
    ///
    /// Both figures ride on the stream's text revision, so they are the values
    /// as of the last text update rather than as of this tick. That makes the
    /// difference an *upper* bound on the live backlog while the model decodes
    /// faster than real time — its backlog shrinks between updates — which is
    /// the direction a close test needs. The audio tap's own position is
    /// deliberately not used: it measures what was pushed, not what the worker
    /// has taken, and the two differ by the feed channel's backlog.
    stream_input_ms: i64,

    /// The primary model's process-lifetime stream totals
    /// ([`StreamTiming`]), read to report its rate per chunk.
    primary_timing: Arc<StreamTiming>,
    /// The totals [`Self::primary_timing`] held when the last rate was reported.
    /// A chunk's rate is the difference between two reads, because the primary
    /// streams continuously and has no per-chunk decode to time on its own.
    primary_seen: (u64, u64),

    /// This session's liveness, as [`is_active`] reads it. See [`Session`].
    alive: Arc<AtomicBool>,

    next_chunk_id: u64,
    open: Chunk,
    /// Every chunk that has closed, in order. Their audio is what a merge
    /// window's context is built from, so it is kept for a while after the
    /// close (see [`Coordinator::retain_context_audio`]).
    closed: Vec<Chunk>,

    last_speech_ms: i64,
    last_speech_change: Instant,
    /// The speech clock's value when the chunk was last closed. A break only
    /// counts again once speech has advanced past it, so one long pause cannot
    /// trigger a merge on every tick.
    last_close_speech_ms: i64,

    /// The last failed chunk, kept so a later close can retry its merge (a
    /// transient provider failure should not cost the chunk its polish). One
    /// chunk's worth: a second failure replaces the first, which keeps its
    /// concatenated fallback text.
    retry: Option<u64>,

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
            // The flag that armed this session is read once, in `start`, so a
            // toggle switched off while recording would otherwise leave the
            // coordinator running: it would keep closing a chunk at every pause
            // and keep replacing the preview with its composed text, which is
            // precisely the behaviour the user just turned off. The periodic
            // refresh is where the change is seen — up to `SETTINGS_REFRESH_TICKS`
            // late, the same latency the pause and context sliders already have —
            // and the retire below is the right way out: the recording keeps
            // running, the overlay goes back to the primary's own live text, and
            // the batch path transcribes and merges the session at stop.
            if !self.settings.multi_stt_streaming_first_enabled {
                info!(
                    "Multi-STT streaming: the mode was switched off while recording; releasing \
                     the session. The overlay returns to the primary's own live text and the \
                     batch path covers the whole session at stop"
                );
                self.publish_primary_text();
                return false;
            }
        }

        // Audio first: the tap is drained every tick, so a chunk's audio is
        // whatever accumulated since the last break and nothing has to be
        // indexed out of the past.
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

        // One merge at a time. A break during a job is not lost: the pause
        // persists until a close consumes it, so it is acted on the moment the
        // job lands — unless the speaker resumed meanwhile, in which case the
        // chunk keeps the speech that followed, as it should.
        if self.merge.running {
            self.watchdog_merge();
            if self.merge.running {
                self.publish(false);
                return true;
            }
        }

        // A chunk is closed when its audio paused *and* it owns the text its
        // merge will replace. Both halves of that are read here, and both come
        // from the stream's own accounting, so nothing the tap does can bias
        // them; see `break_outcome`, `STREAM_DRAIN_TOLERANCE_MS` and
        // `TEXT_CATCHUP_GRACE` for why it takes two. The two ways this can end
        // are a close and, past the grace, the session itself.
        //
        // A pause is silent by definition, so waiting is normally cheap: no new
        // audio is fed and the model's un-drained backlog is all that is left to
        // work through. `paused_for` is the age of the current speech state, so
        // the grace is measured from the break itself rather than from whenever
        // the speaker happened to go quiet.
        let stream_drain_ms = (self.stream_input_ms - self.stream_drained_ms).max(0);
        let has_text = !self.open.live.trim().is_empty();
        let paused_for = self.last_speech_change.elapsed();

        match break_outcome(
            closes(paused, !self.open.audio.is_empty(), self.open.duration_ms()),
            has_text,
            stream_drain_ms,
            paused_for,
            pause,
        ) {
            Break::Wait => {}
            Break::Close => {
                self.close_open_chunk();
                self.last_close_speech_ms = speech;
                // A failed chunk is retried at the next close, so the retry rate
                // is the session's natural pace and a down provider is not
                // hammered.
                if !self.merge.running {
                    self.retry_failed_chunk();
                }
            }
            Break::Retire => {
                // The grace has run out with the chunk's own text still owed,
                // and there is nothing left to wait for: the mode's premise is
                // dead for this session, and closing the chunk now is exactly the
                // duplication the grace exists to prevent.
                //
                // So the session retires itself rather than publish a preview it
                // knows is wrong. Nothing is lost: the recording keeps running —
                // the action's own sample buffer is what the batch path decodes —
                // the overlay goes back to the primary's own live text, and the
                // whole session is transcribed and merged at stop by the batch
                // path, which is the mode's fallback everywhere else. One chunk's
                // merge is the most this can waste, and the return is before the
                // dispatch of another.
                warn!(
                    "Multi-STT streaming: chunk {} was still owed its own text {:?} after the \
                     break (grace {:?}) — {} chars of it, and {} ms of audio the stream decodes \
                     but has not drained. A chunk cannot be merged against text it does not have: \
                     closing this one would leave slot 1 empty or short, and its words would \
                     arrive during the next chunk and be read into it, showing the same speech \
                     twice. Retiring the session: the recording keeps running, the overlay goes \
                     back to the primary's own live text, and the batch path transcribes and \
                     merges the whole session at stop",
                    self.closed.len() + 1,
                    paused_for,
                    TEXT_CATCHUP_GRACE,
                    self.open.live.chars().count(),
                    stream_drain_ms,
                );
                self.publish_primary_text();
                return false;
            }
        }

        self.publish(false);
        true
    }

    /// Read the stream's latest text into the open chunk.
    fn absorb_stream_text(&mut self) {
        let snapshot = Arc::clone(&*self.snapshot.lock().unwrap());
        if snapshot.revision == self.seen_revision {
            return;
        }
        self.seen_revision = snapshot.revision;

        // The tap and the stream are fed the same frames in the same order (see
        // `ChunkTap`), so the tap must have seen exactly the audio the stream was
        // fed. A structural mismatch would mean every reported lag is off by a
        // constant, which is worth one warning — the correction is not applied,
        // because a worker-thread lag of a frame or two is indistinguishable
        // from a real lead and would silently absorb it.
        if !self.lead_checked && snapshot.input_received_ms > 0 {
            self.lead_checked = true;
            let fed = snapshot.input_received_ms as u64 * SAMPLES_PER_MS as u64;
            let tapped = self.tap.pushed_samples();
            if tapped > fed {
                warn!(
                    "Multi-STT streaming: the audio tap is {} samples ahead of the stream; the \
                     stream lag reported at each close may be off by {:.0} ms",
                    tapped - fed,
                    (tapped - fed) as f64 / SAMPLES_PER_MS as f64
                );
            }
        }

        self.primary_text = snapshot.committed.clone();
        self.primary_tentative = snapshot.tentative.clone();
        self.stream_drained_ms = snapshot.audio_committed_ms;
        self.stream_input_ms = snapshot.input_received_ms;

        // `committed` only grows. If a family rewrites it anyway, offsets into it
        // are meaningless: re-anchor on the current length and say so once. Only
        // the open chunk is re-anchored — a closed chunk's text is frozen, and it
        // is the text already on screen. The open chunk never carries a merge:
        // every merge is dispatched at a close.
        if snapshot.committed.len() < self.live_copied {
            warn!(
                "Multi-STT streaming: the committed text shrank ({} → {} bytes); re-anchoring the \
                 open chunk",
                self.live_copied,
                snapshot.committed.len()
            );
            self.open.live.clear();
            self.live_copied = snapshot.committed.len();
        }
        if snapshot.committed.len() > self.live_copied {
            let at = clamp_boundary(&snapshot.committed, self.live_copied);
            self.open.live.push_str(&snapshot.committed[at..]);
            self.live_copied = snapshot.committed.len();
        }
    }

    /// Report the primary streaming model's text and rate for the chunk that
    /// just closed.
    ///
    /// The primary is not decoded per chunk — it streams continuously, which is
    /// the whole point of the mode — so there is no per-chunk decode call to
    /// time. What is knowable is how much audio it was fed and how much compute
    /// it spent between two chunk boundaries, which is the same rate the
    /// `Live preview perf` line reports, scoped to one chunk. The text is the
    /// primary's own live text for that chunk (`Chunk::live`), never the merged
    /// text a merge may put on screen afterwards: the point of the line is to
    /// show what the streaming model produced before anything corrected it.
    ///
    /// Silent when no audio was fed since the last report — a chunk that closed
    /// with nothing new behind it has no rate to state.
    fn log_primary_rate(&mut self) {
        let now = self.primary_timing.totals();
        let delta = (
            now.0.saturating_sub(self.primary_seen.0),
            now.1.saturating_sub(self.primary_seen.1),
        );
        self.primary_seen = now;

        let (audio_secs, compute_secs) = StreamTiming::secs(delta);
        if audio_secs <= 0.0 {
            return;
        }
        let text = self
            .closed
            .last()
            .map(|chunk| chunk.live.trim())
            .unwrap_or_default();
        if compute_secs > 0.0 {
            info!(
                "Multi-STT streaming: chunk {} model 1 (primary stream) transcribed {:.2}s of \
                 audio in {:.2}s ({:.2}x real-time): '{}'",
                self.closed.len(),
                audio_secs,
                compute_secs,
                real_time_factor(audio_secs, compute_secs),
                crate::utils::redact_text(text)
            );
        } else {
            info!(
                "Multi-STT streaming: chunk {} model 1 (primary stream) transcribed {:.2}s of \
                 audio: '{}'",
                self.closed.len(),
                audio_secs,
                crate::utils::redact_text(text)
            );
        }
    }

    /// Close the open chunk and merge it.
    ///
    /// The chunk is taken whole — all of the audio since the last break and all
    /// of the streaming text it owns — so there is no seam to keep aligned,
    /// nothing dropped and nothing shown twice. The next chunk starts empty here.
    fn close_open_chunk(&mut self) {
        let mut closed = std::mem::replace(&mut self.open, Chunk::new(self.next_chunk_id));
        self.next_chunk_id += 1;
        closed.failed = false;
        self.closed.push(closed);
        self.retain_context_audio();

        let index = self.closed.len() - 1;
        let context_samples = {
            let depth = context_depth(self.settings.multi_stt_streaming_context_chunks);
            window_context(&self.closed, index, depth).0.len()
        };
        // The stream's own audio position, which the tap has just drained up to.
        // The difference to the family's drain cursor is the audio the close had
        // to merge across without a decode: silence by construction, which is
        // what makes the close legal at [`STREAM_DRAIN_TOLERANCE_MS`], so it is
        // worth a line — a figure crawling towards the tolerance is the mode
        // working near its edge, and one above it is the feed or the model
        // falling behind rather than a bug in the seam.
        let stream_position_ms = self.tap.pushed_samples() as i64 / SAMPLES_PER_MS;
        info!(
            "Multi-STT streaming: chunk {} closed — {} ms of audio, {} chars, {} ms of context, \
             {} ms of it left un-drained (tolerance {} ms)",
            self.closed.len(),
            self.closed[index].duration_ms(),
            self.closed[index].live.chars().count(),
            context_samples as i64 / SAMPLES_PER_MS,
            (stream_position_ms - self.stream_drained_ms).max(0),
            STREAM_DRAIN_TOLERANCE_MS
        );
        self.log_primary_rate();

        let input = self.window_for(index, true);
        self.dispatch(input);
        self.recount_failed();
    }

    /// The merge input for the closed chunk at `index`: the context chunks the
    /// setting asks for, then the chunk itself. The window is what bounds the
    /// mode's cost — it never grows with the length of the session.
    fn window_for(&self, index: usize, record_outputs: bool) -> JobInput {
        let chunk = &self.closed[index];
        let depth = context_depth(self.settings.multi_stt_streaming_context_chunks);
        let (mut audio, context_text) = window_context(&self.closed, index, depth);
        let context_samples = audio.len();
        audio.extend_from_slice(&chunk.audio);
        JobInput {
            chunk_id: chunk.id,
            live: chunk.live.clone(),
            audio,
            context_samples,
            context_text,
            record_outputs,
        }
    }

    /// Free the audio of the closed chunks that have fallen out of the context
    /// window.
    ///
    /// A closed chunk keeps its audio while it can still be a merge's context —
    /// the next `context_chunks` closes — because the window is served from these
    /// buffers. That retention is what bounds the mode's memory: the window plus
    /// the open chunk, `context_chunks + 1` chunks at most, and a chunk at the
    /// 60 s valve is ~3.8 MB.
    fn retain_context_audio(&mut self) {
        let depth = context_depth(self.settings.multi_stt_streaming_context_chunks);
        let keep_from = self.closed.len().saturating_sub(depth);
        for (index, chunk) in self.closed.iter_mut().enumerate() {
            // A chunk waiting to retry its own merge still needs its audio,
            // however far back it is: a retry is offered at a close, and that
            // close may be several chunks later.
            if index < keep_from && Some(chunk.id) != self.retry && !chunk.audio.is_empty() {
                chunk.audio = Vec::new();
            }
        }
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
                // The chunk's live text is shorter than the result's: nothing in
                // the mode grows a closed chunk's text, so this can only be a
                // re-anchored open chunk. Dropping the result leaves its raw live
                // text on screen instead of text from another chunk.
                Some(chunk) if chunk.live.len() < live_len => {
                    debug!(
                        "Multi-STT streaming: dropping a stale merge for chunk {} (its text was \
                         re-anchored while the job ran)",
                        chunk_id + 1
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
                                chunk_id + 1
                            );
                            chunk.merged_text = None;
                            chunk.failed = true;
                        } else {
                            chunk.apply_merge(fallback, true);
                        }
                    } else if let Some(text) = merged {
                        chunk.apply_merge(text, false);
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
        // A failed chunk waits for the next close to be retried, whether or not
        // this job was a retry: a provider that is down for a minute should not
        // cost the chunk its polish for the rest of the session.
        if failed {
            self.retry = Some(chunk_id);
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

    /// Retry the last failed chunk's merge, if any is waiting.
    ///
    /// A retry records no outputs: that chunk's per-model texts are already part
    /// of the session's metadata. Its audio is read back off the chunk — a closed
    /// chunk keeps it for the context window anyway — so nothing has to be held
    /// on the side for a retry to be possible.
    fn retry_failed_chunk(&mut self) {
        let Some(chunk_id) = self.retry.take() else {
            return;
        };
        let Some(index) = self.closed.iter().position(|c| c.id == chunk_id) else {
            // The chunk was re-anchored away; nothing to retry.
            return;
        };
        if self.closed[index].audio.is_empty() {
            return;
        }
        debug!(
            "Multi-STT streaming: retrying the merge of failed chunk {}",
            chunk_id + 1
        );
        let input = self.window_for(index, false);
        self.dispatch(input);
    }

    /// The session's text: every closed chunk in order, then the open one.
    fn compose_committed(&self) -> String {
        let mut out = String::new();
        for chunk in &self.closed {
            append_join(&mut out, &chunk.display_text());
        }
        out
    }

    /// Give the overlay the primary stream's own text and drop the composed
    /// preview.
    ///
    /// Called when the session retires itself. `shutdown` clears the exclusive
    /// sink, which hands the overlay back to the plain stream's own events — but
    /// those only arrive when the model produces text, so a model that has gone
    /// quiet (or one that does not commit text until finalize) would leave the
    /// abandoned composition on screen for the rest of the recording. This one
    /// event replaces it with exactly what the plain path shows: the stream's
    /// committed text and its volatile tail, nothing composed.
    fn publish_primary_text(&mut self) {
        self.tm
            .emit_composed_stream_text(&self.primary_text, &self.primary_tentative, None);
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

        // The overlay event is deduped: an unchanged text is not re-sent, which
        // is what keeps a silent pause from emitting 20 times a second.
        let changed =
            force || committed != self.published_committed || tentative != self.published_tentative;
        if changed {
            self.published_committed = committed.clone();
            self.published_tentative = tentative.clone();

            self.tm.emit_composed_stream_text(
                &committed,
                &tentative,
                (self.failed_chunks > 0).then_some(self.failed_chunks),
            );
        }

        if self.owns_typing {
            let mut full = committed;
            full.push_str(&tentative);
            // Outside the dedupe on purpose. The writer holds a revision that
            // arrived while it was still behind, and it is *this* call that
            // retries it: an unchanged text is what the writer catching up
            // looks like from here, so skipping the call would leave a merge
            // unpushed for as long as the speaker stays quiet.
            self.push_writer_target(&full);
        }
    }

    /// Hand the writer the text the app should be showing.
    ///
    /// A target that *extends* what was already pushed goes out immediately —
    /// that is ordinary live typing. A target that rewrites it is a merge that
    /// replaced a closed chunk, and the writer reaches it exactly the way it
    /// reaches any revision: by backspacing the divergence and typing the rest,
    /// which is what the typewriter is for and what the user watches work. It is
    /// held until the writer has caught up so that a revision never lands on top
    /// of one still being typed, and [`Self::publish`] calls this on every tick
    /// — including the ticks that emit nothing — so the hold is released the
    /// moment the writer is free rather than at the next word. The flush at the
    /// end always applies the latest text.
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

        // The last chunk is a chunk like any other: it closes here — the
        // recording ended, which is a break of sorts — and is merged
        // synchronously, so the session's last words go through the same
        // pipeline as every other chunk's.
        let mut last = std::mem::replace(&mut self.open, Chunk::new(self.next_chunk_id));
        self.next_chunk_id += 1;
        last.failed = false;
        let has_content = !last.live.trim().is_empty() || !last.audio.is_empty();
        self.closed.push(last);
        self.retain_context_audio();

        let index = self.closed.len() - 1;
        self.log_primary_rate();
        if has_content {
            let settings = get_settings(&self.app);
            let input = self.window_for(index, true);
            let result = tauri::async_runtime::block_on(run_merge_job(
                Arc::clone(&self.tm),
                settings,
                input,
                self.merge.generation,
            ));
            self.apply_job_result(result);
        }

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

    /// A closed chunk with `live` text and `ms` of audio.
    fn closed_chunk(id: u64, live: &str, ms: i64) -> Chunk {
        let mut chunk = Chunk::new(id);
        chunk.live = live.to_string();
        chunk.audio = audio_of_ms(ms);
        chunk
    }

    #[test]
    fn a_break_closes_the_chunk_and_nothing_else_does() {
        // The break is the delimiter, on its own: no text condition, no minimum
        // length, no punctuation to wait for.
        assert!(closes(true, true, 200));
        assert!(closes(true, true, 30_000));
        // A running speaker's chunk stays open...
        assert!(!closes(false, true, 200));
        assert!(!closes(false, true, MAX_CHUNK_SECONDS * 1000 - 1));
        // ...until the valve, which is the only other thing that closes one.
        assert!(closes(false, true, MAX_CHUNK_SECONDS * 1000));
    }

    #[test]
    fn a_break_with_no_audio_closes_nothing() {
        // The one structural guard: a chunk with no audio behind it has nothing
        // to merge, so a job over it would be three decodes of silence.
        assert!(!closes(true, false, 0));
        assert!(!closes(true, false, MAX_CHUNK_SECONDS * 1000));
    }

    #[test]
    fn a_break_waits_for_the_chunks_own_text_then_retires_the_session() {
        let pause = Duration::from_millis(1000);
        let broken = pause + Duration::from_millis(200);
        let overdue = pause + TEXT_CATCHUP_GRACE;

        // No break, or nothing to close: nothing happens, whatever the chunk
        // holds and however long the pause has lasted.
        assert_eq!(break_outcome(false, true, 0, overdue, pause), Break::Wait);
        assert_eq!(
            break_outcome(false, true, 4000, overdue, pause),
            Break::Wait
        );
        assert_eq!(
            break_outcome(false, false, 4000, overdue, pause),
            Break::Wait
        );

        // A break whose chunk owns its text closes at once. "Drained" is the
        // family's residual, not an exact zero: a running stream always has
        // audio in flight, and the pause this is asked during feeds it none, so
        // an exact zero never arrives at all.
        assert_eq!(break_outcome(true, true, 0, broken, pause), Break::Close);
        assert_eq!(
            break_outcome(true, true, STREAM_DRAIN_TOLERANCE_MS, broken, pause),
            Break::Close
        );

        // Text still out is worth waiting for — up to the grace, and no further.
        assert_eq!(
            break_outcome(true, true, STREAM_DRAIN_TOLERANCE_MS + 1, broken, pause),
            Break::Wait
        );
        assert_eq!(break_outcome(true, true, 1200, broken, pause), Break::Wait);
        assert_eq!(
            break_outcome(true, true, 1200, overdue, pause),
            Break::Retire
        );

        // A chunk with no text of its own waits however well drained the stream
        // is. This is the case a drain hint cannot see: a family that decodes
        // eagerly and publishes late has `audio_committed_ms` at the end of the
        // audio while slot 1 is still empty, and closing there replaces the
        // chunk's text with the extras' decode alone — then the primary's words
        // arrive during the next chunk and are read into it. Both halves have to
        // hold.
        assert_eq!(break_outcome(true, false, 0, broken, pause), Break::Wait);
        assert_eq!(
            break_outcome(true, false, STREAM_DRAIN_TOLERANCE_MS, broken, pause),
            Break::Wait
        );
        assert_eq!(break_outcome(true, false, 0, overdue, pause), Break::Retire);

        // Text that arrived while the wait ran out closes the chunk rather than
        // retiring: it is the chunk's own text, which is all the wait was for.
        assert_eq!(
            break_outcome(true, true, STREAM_DRAIN_TOLERANCE_MS, overdue, pause),
            Break::Close
        );
    }

    #[test]
    fn the_context_depth_is_capped() {
        assert_eq!(context_depth(0), 0);
        assert_eq!(context_depth(1), 1);
        assert_eq!(context_depth(MAX_CONTEXT_CHUNKS as u32), MAX_CONTEXT_CHUNKS);
        // A hand-edited store cannot make the mode hold more audio than the
        // settings page can ask for.
        assert_eq!(context_depth(99), MAX_CONTEXT_CHUNKS);
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

        // A chunk's text keeps the stream's own leading whitespace, so joining
        // chunks never doubles a space.
        let mut out = String::new();
        append_join(&mut out, "你好。");
        append_join(&mut out, " 我很好！");
        assert_eq!(out, "你好。 我很好！");

        // Chunks of a script that puts no space between its words are joined
        // with none: the cut between them is inside a word, and a space there
        // would split it. R2T2 commits at character granularity, so this is the
        // ordinary case for its Chinese output, not an edge one.
        let mut out = String::new();
        append_join(&mut out, "日本原子能机构表示，核电站检测出了放射性");
        append_join(&mut out, "物质碘和碘。");
        assert_eq!(out, "日本原子能机构表示，核电站检测出了放射性物质碘和碘。");

        // Only a boundary that is unspaced on *both* sides is left alone. A
        // Latin neighbour keeps the space, so the merge case above still works
        // and so does a sentence ending in CJK punctuation followed by English.
        let mut out = String::from("很好");
        append_join(&mut out, "Hello");
        assert_eq!(out, "很好 Hello");
    }

    #[test]
    fn words_are_folded_and_punctuation_is_dropped() {
        let words: Vec<String> = tokens("Hello, World. 3.14!")
            .into_iter()
            .map(|t| t.1)
            .collect();
        assert_eq!(words, vec!["hello", "world", "3", "14"]);

        // A non-ASCII letter is a word of its own, so a CJK clause is many
        // words rather than one — the crop needs a boundary inside it.
        let cjk: Vec<String> = tokens("你好世界").into_iter().map(|t| t.1).collect();
        assert_eq!(cjk, vec!["你", "好", "世", "界"]);

        // Offsets are byte offsets: `你` is 3 bytes, so the second word starts
        // at 3.
        let offsets: Vec<usize> = tokens("你好").into_iter().map(|t| t.0).collect();
        assert_eq!(offsets, vec![0, 3]);
    }

    #[test]
    fn a_decode_is_cropped_back_to_the_chunks_own_words() {
        let context = "First one. Second one.";
        let decoded = "First one. Second one. And then this is new.";
        assert_eq!(
            strip_context_prefix(decoded, context).as_deref(),
            Some("And then this is new.")
        );
    }

    #[test]
    fn the_crop_follows_the_longest_shared_run() {
        // The extras punctuate and capitalise differently, and mis-hear a word
        // here and there: the crop only needs the context's *tail* to be
        // recognised, and it takes the longest run it can.
        let context = "the quick brown fox jumps over the lazy dog";
        let decoded = "The quick brown fox jumps over the lazy log and then he sleeps";
        assert_eq!(
            strip_context_prefix(decoded, context).as_deref(),
            Some("and then he sleeps")
        );
    }

    #[test]
    fn a_misheard_word_at_the_seam_does_not_defeat_the_crop() {
        // The seam word is the one every model is least sure of, and every anchor
        // length ends at it — so without drift slack, one wrong word there would
        // drop the model's whole output for the chunk.
        let context = "one two three four five";
        let decoded = "One two three four hive and the rest is new";
        assert_eq!(
            strip_context_prefix(decoded, context).as_deref(),
            Some("and the rest is new")
        );
    }

    #[test]
    fn a_decode_that_disagrees_with_the_context_is_refused() {
        // Nothing of the context's tail is in the decode, so there is no seam to
        // find: the caller drops this model's output rather than cutting the
        // chunk's own words off.
        assert!(strip_context_prefix("unrelated words entirely", "One. Two. Three.").is_none());
        // Too little of the tail to trust: one shared word is a coincidence.
        assert!(strip_context_prefix("two", "One. Two. Three. Four.").is_none());
    }

    #[test]
    fn no_context_means_no_crop() {
        // The setting's 0 case: the extras heard the chunk alone.
        assert_eq!(
            strip_context_prefix("  the chunk's own words ", "").as_deref(),
            Some("the chunk's own words")
        );
        // A context with no words in it — punctuation only — is the same case.
        assert_eq!(
            strip_context_prefix("the chunk's own words", "!!! ... ?").as_deref(),
            Some("the chunk's own words")
        );
        // A context whose words are not in the decode at all is a refusal, not a
        // no-op: there is no seam to find.
        assert!(strip_context_prefix("the chunk's own words", "hello there friend").is_none());
    }

    #[test]
    fn a_crop_that_ends_at_the_decodes_end_leaves_nothing() {
        // The model decoded the whole window and stopped: there are no words of
        // the chunk's own, which is an empty contribution and not a failure.
        assert_eq!(
            strip_context_prefix("One. Two. Three.", "One. Two. Three.").as_deref(),
            Some("")
        );
    }

    #[test]
    fn the_window_carries_the_chunks_before_it() {
        let closed = vec![
            closed_chunk(1, "one", 2_000),
            closed_chunk(2, "two", 3_000),
            closed_chunk(3, "three", 4_000),
        ];
        // No context: the window is the chunk alone.
        let (audio, text) = window_context(&closed, 2, 0);
        assert!(audio.is_empty());
        assert_eq!(text, "");

        // One chunk of context: the one just before.
        let (audio, text) = window_context(&closed, 2, 1);
        assert_eq!(audio.len(), audio_of_ms(3_000).len());
        assert_eq!(text, "two");

        // Two: oldest first, so the text reads in the order it was spoken.
        let (audio, text) = window_context(&closed, 2, 2);
        assert_eq!(audio.len(), audio_of_ms(5_000).len());
        assert_eq!(text, "one two");
    }

    #[test]
    fn a_windows_text_uses_what_the_chunk_is_already_showing() {
        // The context is the crop's ruler, so it has to be the text on screen —
        // the merged one where a merge has landed, not the streaming rough draft.
        let mut merged = closed_chunk(1, "hello world", 2_000);
        merged.apply_merge("Hello, world.".to_string(), false);
        let closed = vec![merged, closed_chunk(2, "next", 1_000)];
        let (_, text) = window_context(&closed, 1, 1);
        assert_eq!(text, "Hello, world.");
    }

    #[test]
    fn the_walk_stops_where_the_audio_was_freed() {
        // Text and audio must describe the same span, so a chunk whose audio has
        // been freed cannot contribute its words.
        let mut freed = closed_chunk(1, "one", 2_000);
        freed.audio = Vec::new();
        let closed = vec![
            freed,
            closed_chunk(2, "two", 3_000),
            closed_chunk(3, "three", 4_000),
        ];

        let (audio, text) = window_context(&closed, 2, 2);
        assert_eq!(audio.len(), audio_of_ms(3_000).len());
        assert_eq!(text, "two");
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
        // result lost that chunk's merge for good.
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
            decode_latency_ms: 0.0,
            merge_latency_ms: 0.0,
        }
    }

    #[test]
    fn a_chunk_without_a_merge_shows_its_live_text() {
        let mut chunk = Chunk::new(1);
        chunk.live = "hello world and more".to_string();
        assert_eq!(chunk.display_text(), "hello world and more");
    }

    #[test]
    fn a_merge_replaces_the_chunks_whole_text() {
        // A merge is dispatched only once the chunk has closed, so its text is
        // complete: what it replaces is everything the chunk had.
        let mut chunk = Chunk::new(1);
        chunk.live = "hello world. and the next".to_string();
        chunk.apply_merge("Hello, world. And the next.".to_string(), false);
        assert_eq!(chunk.display_text(), "Hello, world. And the next.");
    }

    /// The session's text, composed the way `compose_committed` does: every
    /// closed chunk's current text, joined.
    fn session_text(chunks: &[Chunk]) -> String {
        let mut out = String::new();
        for chunk in chunks {
            append_join(&mut out, &chunk.display_text());
        }
        out
    }

    #[test]
    fn a_merge_leaves_the_chunks_before_it_untouched() {
        // The property the DirectStreaming output path rests on. The coordinator
        // pushes the session's text as the writer's target, and a merge rewrites
        // one closed chunk inside it; the writer reaches a rewritten target by
        // backspacing the divergence and retyping the rest. If the chunks before
        // the corrected one were not a prefix of the new text, that backspace
        // would start at the first character of the session and retype all of it,
        // into the user's document, at typing speed. Because they are a prefix,
        // the backspace covers the corrected chunk's remainder and the chunks
        // after it, and nothing else.
        let mut chunks = vec![
            closed_chunk(1, "the cat sat on the mat.", 3_000),
            closed_chunk(2, "the dog barked at the door.", 3_000),
            closed_chunk(3, "and then it ran off", 2_000),
        ];
        let before = session_text(&chunks);

        chunks[1].apply_merge("the dog barked loudly at the door.".to_string(), false);
        let after = session_text(&chunks);

        // What both share: the first chunk whole, the join, and the corrected
        // chunk up to the word the merge changed.
        let kept = "the cat sat on the mat. the dog barked ";
        assert!(before.starts_with(kept));
        assert!(after.starts_with(kept));
        // And what is left to backspace is the rest of the corrected chunk plus
        // every chunk after it — never anything before it.
        assert_eq!(&before[kept.len()..], "at the door. and then it ran off");
        assert_eq!(
            &after[kept.len()..],
            "loudly at the door. and then it ran off"
        );
    }

    #[test]
    fn a_merge_that_shortens_a_chunk_deletes_the_block_it_replaces() {
        // The other half of the same mechanism, and the one a reader of the
        // direct-streaming path asks about first: a merge is under no obligation
        // to be the same length as the text it replaces. Here it drops a stutter,
        // so the writer has to backspace *more* than it retypes — the wrong block
        // is deleted, not overwritten. Nothing about the mechanism changes: the
        // divergence is still found by the common prefix, so the deletion starts
        // at the word the merge dropped and reaches exactly as far as the old
        // text ran.
        let mut chunks = vec![
            closed_chunk(1, "and then and then", 2_000),
            closed_chunk(2, "we went home", 2_000),
        ];
        let before = session_text(&chunks);

        chunks[0].apply_merge("and then".to_string(), false);
        let after = session_text(&chunks);

        let kept = "and then ";
        assert!(before.starts_with(kept));
        assert!(after.starts_with(kept));
        // Before / after the divergence: 21 characters backspaced away and 12
        // typed back, so the block that was wrong is gone rather than covered.
        // The chunk after the merged one is retyped only because it follows.
        assert_eq!(&before[kept.len()..], "and then we went home");
        assert_eq!(&after[kept.len()..], "we went home");
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
