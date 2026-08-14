//! Speculative turn tracking for continuous-voice conversation.
//!
//! Ported conceptually from speech-to-speech's `pipeline/speculative_turns.py`:
//! every utterance is a revisable unit of work identified by `(turn_id,
//! revision)`. The recorder owns turn *timing* (endpoint + reopen grace) while
//! this tracker owns turn *versions*: pipeline stages call `is_latest` before
//! publishing side effects and `commit_if_latest` before irreversible ones
//! (TTS playback, conversation history), so a resumed utterance invalidates
//! stale in-flight work at the earliest gate.

use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, Default)]
struct Inner {
    next_turn_id: u64,
    latest_revision: HashMap<u64, u32>,
    committed_revision: HashMap<u64, u32>,
}

pub struct SpeculativeTurnTracker {
    inner: Mutex<Inner>,
}

impl Default for SpeculativeTurnTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl SpeculativeTurnTracker {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner::default()),
        }
    }

    /// Allocate a fresh turn; returns `(turn_id, revision)`.
    pub fn new_turn(&self) -> (u64, u32) {
        let id = {
            let mut inner = self.inner.lock().unwrap();
            inner.next_turn_id += 1;
            let id = inner.next_turn_id;
            inner.latest_revision.insert(id, 0);
            id
        };
        // Opportunistic bookkeeping: every 32nd turn, forget old committed
        // turns so long sessions don't grow the maps.
        if id % 32 == 0 {
            self.prune(64);
        }
        (id, 0)
    }

    /// Reopen an existing turn (speech resumed inside the reopen grace):
    /// bumps the revision so every stage tagged with the old revision goes
    /// stale. Returns the new revision. No-op (defensive) once the turn has
    /// been committed — the recorder only reopens pre-commit, but a committed
    /// turn must never be invalidated.
    pub fn reopen(&self, turn_id: u64) -> u32 {
        let mut inner = self.inner.lock().unwrap();
        if inner.committed_revision.contains_key(&turn_id) {
            return inner.latest_revision.get(&turn_id).copied().unwrap_or(0);
        }
        let next = inner.latest_revision.get(&turn_id).copied().unwrap_or(0) + 1;
        inner.latest_revision.insert(turn_id, next);
        next
    }

    /// True while `revision` is the newest known revision for this turn.
    pub fn is_latest(&self, turn_id: u64, revision: u32) -> bool {
        self.inner
            .lock()
            .unwrap()
            .latest_revision
            .get(&turn_id)
            .copied()
            .is_some_and(|r| r == revision)
    }

    /// Commit (mark irreversible work started) only if this revision is still
    /// the latest. Returns true when the commit was recorded now.
    pub fn commit_if_latest(&self, turn_id: u64, revision: u32) -> bool {
        let mut inner = self.inner.lock().unwrap();
        if inner.latest_revision.get(&turn_id).copied() == Some(revision) {
            inner.committed_revision.insert(turn_id, revision);
            true
        } else {
            false
        }
    }

    /// Mark a turn superseded (barge-in / cancel): every in-flight stage
    /// tagged with any revision of this turn becomes stale.
    pub fn cancel(&self, turn_id: u64) {
        let mut inner = self.inner.lock().unwrap();
        let next = inner.latest_revision.get(&turn_id).copied().unwrap_or(0) + 1;
        inner.latest_revision.insert(turn_id, next);
    }

    /// Drop bookkeeping for old turns so long sessions don't grow the map.
    /// Keeps the most recent `keep` turns (uncommitted ones are never pruned).
    pub fn prune(&self, keep: usize) {
        let mut inner = self.inner.lock().unwrap();
        if inner.latest_revision.len() <= keep {
            return;
        }
        let mut ids: Vec<u64> = inner
            .latest_revision
            .keys()
            .copied()
            .filter(|id| {
                inner
                    .committed_revision
                    .get(id)
                    .copied()
                    .map(|r| inner.latest_revision.get(id).copied() == Some(r))
                    .unwrap_or(false)
            })
            .collect();
        ids.sort_unstable();
        let drop_count = ids.len().saturating_sub(keep);
        for id in ids.into_iter().take(drop_count) {
            inner.latest_revision.remove(&id);
            inner.committed_revision.remove(&id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_turn_allocates_increasing_ids() {
        let tracker = SpeculativeTurnTracker::new();
        let (id1, rev1) = tracker.new_turn();
        let (id2, rev2) = tracker.new_turn();
        assert_eq!((rev1, rev2), (0, 0));
        assert!(id2 > id1);
    }

    #[test]
    fn reopen_makes_old_revision_stale() {
        let tracker = SpeculativeTurnTracker::new();
        let (id, rev) = tracker.new_turn();
        assert!(tracker.is_latest(id, rev));
        let new_rev = tracker.reopen(id);
        assert_eq!(new_rev, rev + 1);
        assert!(!tracker.is_latest(id, rev));
        assert!(tracker.is_latest(id, new_rev));
    }

    #[test]
    fn commit_only_succeeds_for_latest() {
        let tracker = SpeculativeTurnTracker::new();
        let (id, rev) = tracker.new_turn();
        let next = tracker.reopen(id);
        assert!(!tracker.commit_if_latest(id, rev));
        assert!(tracker.commit_if_latest(id, next));
        // A committed turn is immune to further reopens.
        assert_eq!(tracker.reopen(id), next);
    }

    #[test]
    fn cancel_supersedes_all_revisions() {
        let tracker = SpeculativeTurnTracker::new();
        let (id, rev) = tracker.new_turn();
        tracker.cancel(id);
        assert!(!tracker.is_latest(id, rev));
        // The turn can still be reopened/committed as a fresh revision.
        let next = tracker.reopen(id);
        assert!(tracker.is_latest(id, next));
    }

    #[test]
    fn prune_keeps_uncommitted_turns() {
        let tracker = SpeculativeTurnTracker::new();
        for _ in 0..10 {
            let (id, rev) = tracker.new_turn();
            tracker.commit_if_latest(id, rev);
        }
        let (uncommitted, _) = tracker.new_turn();
        tracker.prune(2);
        assert!(tracker.is_latest(uncommitted, 0));
    }
}
