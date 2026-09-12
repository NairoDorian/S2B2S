//! A drainable mid-recording copy of the frames a model is decoding.
//!
//! The experimental Multi-STT streaming mode runs the extra models *during* the
//! recording, so it needs audio the normal path only hands out once
//! `Cmd::Stop` completes. This tap is that audio: the recorder pushes every
//! frame it hands to the decoder and the consumer drains it on its own schedule.
//!
//! **The push site is what makes the timestamps work.** `handle_frame` hands the
//! same VAD-kept frame to the batch buffer (`RecordedAudio::stt_samples`) and to
//! the streaming worker's feed callback, and this tap sits in the same place — so
//! the tapped samples, the batch recording and the streaming model's input are
//! one and the same signal in one and the same order. That is what lets
//! `StreamUpdate::audio_committed_ms`, which is stated in *stream input* time,
//! be used as an index into the tapped buffer (`ms * 16` at 16 kHz) without any
//! resampling or silence-compression correction.
//!
//! A process-wide instance, like the Live FFT tap, so the always-on microphone
//! (opened on its own thread during startup) is wired to it before any manager
//! exists. It is inert (`active` false, one relaxed atomic load per frame)
//! whenever the mode is off.
//!
//! **A session is identified by a token, not by a lock.** [`ChunkTap::begin`]
//! always succeeds and bumps a generation, invalidating whoever held the tap
//! before; a session that notices its token is stale stops touching the tap
//! (see `multi_stt_stream`). Owning the tap by exclusion would mean a
//! coordinator that outlives its recording — a cancel that never reaches the
//! action, a panic in the merge job — silently swallows the *next* recording's
//! audio instead of failing loudly.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

/// A drainable copy of the VAD-filtered 16 kHz frames of the active recording.
pub struct ChunkTap {
    /// Samples pushed since the current `begin` and not yet drained. Only the
    /// holding session takes this lock, once per tick, so the audio consumer
    /// thread never waits on a contended mutex — the same posture as the Live
    /// FFT ring.
    buf: Mutex<Vec<f32>>,
    active: AtomicBool,
    /// Identifies the holding session. Bumped by `begin`, so a session can tell
    /// in one atomic load whether the tap is still its own.
    generation: AtomicU64,
    /// Monotonic count of samples pushed since `begin`, drained or not.
    pushed: AtomicU64,
}

static TAP: LazyLock<Arc<ChunkTap>> = LazyLock::new(|| Arc::new(ChunkTap::new()));

/// The process-wide tap the recorder is built with.
pub fn tap() -> Arc<ChunkTap> {
    Arc::clone(&TAP)
}

impl ChunkTap {
    fn new() -> Self {
        Self {
            buf: Mutex::new(Vec::new()),
            active: AtomicBool::new(false),
            generation: AtomicU64::new(0),
            pushed: AtomicU64::new(0),
        }
    }

    /// Take the tap for a session, dropping anything left over from a previous
    /// one, and return this session's token.
    ///
    /// Always succeeds: a previous holder is invalidated rather than refused,
    /// because the previous holder is the suspect one (it should have called
    /// [`Self::end`]). It notices within a tick and stops draining, so the new
    /// recording's frames are never mixed into the old session's.
    pub fn begin(&self) -> u64 {
        let token = self.generation.fetch_add(1, Ordering::AcqRel) + 1;
        let mut buf = self.buf.lock().unwrap();
        buf.clear();
        buf.shrink_to_fit();
        self.pushed.store(0, Ordering::Release);
        self.active.store(true, Ordering::Release);
        token
    }

    /// Release the tap, but only if `token` still holds it: a session that was
    /// superseded must not clear the buffer the *current* one is filling.
    ///
    /// The generation is bumped here too, so a released token reads as not
    /// current (`is_current`) and can never be handed out again — a session that
    /// ended is not a holder, and the next `begin` must not look like its
    /// continuation.
    pub fn end(&self, token: u64) {
        let mut buf = self.buf.lock().unwrap();
        if self.generation.load(Ordering::Acquire) != token {
            return;
        }
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.active.store(false, Ordering::Release);
        buf.clear();
        self.pushed.store(0, Ordering::Release);
    }

    /// Whether `token` is still the holding session. Checked once per tick by
    /// the coordinator; a stale token means another session took the tap.
    pub fn is_current(&self, token: u64) -> bool {
        self.generation.load(Ordering::Acquire) == token
    }

    /// Whether any session holds the tap. Read once per frame on the audio
    /// consumer thread; a plain relaxed load, like the Live FFT `wants_*` gates.
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Relaxed)
    }

    /// Producer side: append one 16 ms frame. Called on the audio consumer
    /// thread, so it must not block — the mutex is uncontended in practice and
    /// the append is one memcpy.
    pub fn push(&self, frames: &[f32]) {
        self.buf.lock().unwrap().extend_from_slice(frames);
        self.pushed
            .fetch_add(frames.len() as u64, Ordering::Relaxed);
    }

    /// Consumer side: append everything buffered since the last drain to `out`,
    /// returning how many samples were added. `out` is the coordinator's own
    /// buffer, so the steady state allocates nothing on either side.
    pub fn take_into(&self, out: &mut Vec<f32>) -> usize {
        let mut buf = self.buf.lock().unwrap();
        if buf.is_empty() {
            return 0;
        }
        let n = buf.len();
        out.extend_from_slice(&buf);
        buf.clear();
        n
    }

    /// Samples pushed since `begin`, including those already drained. The
    /// difference against the stream's own `input_received_ms` is how the
    /// coordinator checks the two timelines still line up (see
    /// `multi_stt_stream`).
    pub fn pushed_samples(&self) -> u64 {
        self.pushed.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh instance per test: the process-wide one is shared.
    fn tap() -> ChunkTap {
        ChunkTap::new()
    }

    #[test]
    fn push_then_take_moves_samples_once() {
        let tap = tap();
        let token = tap.begin();
        assert!(tap.is_current(token));
        tap.push(&[1.0, 2.0, 3.0]);
        tap.push(&[4.0]);

        let mut out = Vec::new();
        assert_eq!(tap.take_into(&mut out), 4);
        assert_eq!(out, vec![1.0, 2.0, 3.0, 4.0]);
        // Drained, not duplicated: a second drain adds nothing.
        assert_eq!(tap.take_into(&mut out), 0);
        assert_eq!(out.len(), 4);
    }

    #[test]
    fn take_appends_and_reuses_the_callers_buffer() {
        let tap = tap();
        tap.begin();
        let mut out = vec![0.5];
        tap.push(&[1.0]);
        assert_eq!(tap.take_into(&mut out), 1);
        assert_eq!(out, vec![0.5, 1.0]);
    }

    #[test]
    fn pushed_counts_drained_samples_too() {
        let tap = tap();
        tap.begin();
        tap.push(&[0.0; 16]);
        let mut out = Vec::new();
        tap.take_into(&mut out);
        tap.push(&[0.0; 8]);
        // Still the whole session's total, which is what the alignment probe
        // compares against the stream's `input_received_ms`.
        assert_eq!(tap.pushed_samples(), 24);
    }

    #[test]
    fn begin_drops_leftovers_and_end_releases() {
        let tap = tap();
        let token = tap.begin();
        tap.push(&[1.0, 2.0]);
        tap.end(token);
        assert!(!tap.is_active());
        assert!(!tap.is_current(token));
        assert_eq!(tap.pushed_samples(), 0);

        // A new session starts clean rather than inheriting the last one's tail.
        tap.begin();
        let mut out = Vec::new();
        assert_eq!(tap.take_into(&mut out), 0);
        assert!(out.is_empty());
    }

    #[test]
    fn a_new_session_supersedes_a_holder_that_never_released_it() {
        let tap = tap();
        let stale = tap.begin();
        tap.push(&[1.0; 4]);

        // The new recording seizes the tap rather than being refused, and starts
        // from its own first sample — the stale session's leftovers are gone.
        let current = tap.begin();
        assert!(!tap.is_current(stale));
        assert!(tap.is_current(current));
        assert_eq!(tap.pushed_samples(), 0);

        // And the stale session releasing later must not disturb the new one.
        tap.push(&[2.0; 3]);
        tap.end(stale);
        assert!(tap.is_active());
        assert!(tap.is_current(current));
        let mut out = Vec::new();
        assert_eq!(tap.take_into(&mut out), 3);
        assert_eq!(out, vec![2.0; 3]);
    }

    #[test]
    fn an_inactive_tap_still_buffers_nothing_when_not_pushed() {
        let tap = tap();
        assert!(!tap.is_active());
        let mut out = Vec::new();
        assert_eq!(tap.take_into(&mut out), 0);
    }
}
