// Fragment queue: ordered text fragments + their synthesized audio, for
// sequential streaming TTS playback (synthesize fragment i+1 while playing i).
//
// NOTE: Currently unused - the TTS manager (TtsManager::new) uses TtsPlayer
// directly for gapless streaming. This module is preserved from the AgentZero
// prototype for potential future pre-synthesis optimization.
#![allow(dead_code)]

use crate::tts::pagination::TextFragment;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// Status of the fragment queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum QueueStatus {
    Idle,
    Playing,
    Paused,
    Stopped,
}

/// A queued fragment with its synthesized audio.
#[derive(Clone)]
pub struct QueuedFragment {
    pub fragment: TextFragment,
    /// Synthesized audio bytes (None until synthesized).
    pub audio: Option<Vec<u8>>,
}

/// Fragment queue for sequential TTS playback.
pub struct FragmentQueue {
    fragments: Arc<Mutex<Vec<QueuedFragment>>>,
    current_index: Arc<AtomicUsize>,
    status: Arc<AtomicUsize>,
    stop_flag: Arc<AtomicBool>,
}

impl FragmentQueue {
    pub fn new() -> Self {
        Self {
            fragments: Arc::new(Mutex::new(Vec::new())),
            current_index: Arc::new(AtomicUsize::new(usize::MAX)),
            status: Arc::new(AtomicUsize::new(QueueStatus::Idle as usize)),
            stop_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn add_fragment(&self, fragment: TextFragment) {
        let mut fragments = self.fragments.lock().unwrap();
        fragments.push(QueuedFragment {
            fragment,
            audio: None,
        });
    }

    pub fn add_fragments(&self, fragments: Vec<TextFragment>) {
        let mut queue = self.fragments.lock().unwrap();
        for fragment in fragments {
            queue.push(QueuedFragment {
                fragment,
                audio: None,
            });
        }
    }

    pub fn clear(&self) {
        let mut fragments = self.fragments.lock().unwrap();
        fragments.clear();
        self.current_index.store(usize::MAX, Ordering::SeqCst);
    }

    pub fn len(&self) -> usize {
        self.fragments.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn status(&self) -> QueueStatus {
        match self.status.load(Ordering::SeqCst) {
            1 => QueueStatus::Playing,
            2 => QueueStatus::Paused,
            3 => QueueStatus::Stopped,
            _ => QueueStatus::Idle,
        }
    }

    fn set_status(&self, status: QueueStatus) {
        self.status.store(status as usize, Ordering::SeqCst);
    }

    pub fn current_index(&self) -> Option<usize> {
        let idx = self.current_index.load(Ordering::SeqCst);
        if idx == usize::MAX {
            None
        } else {
            Some(idx)
        }
    }

    pub fn set_current_index(&self, index: usize) {
        self.current_index.store(index, Ordering::SeqCst);
    }

    pub fn current_fragment(&self) -> Option<TextFragment> {
        let idx = self.current_index()?;
        let fragments = self.fragments.lock().unwrap();
        fragments.get(idx).map(|q| q.fragment.clone())
    }

    pub fn fragments(&self) -> Vec<TextFragment> {
        self.fragments
            .lock()
            .unwrap()
            .iter()
            .map(|q| q.fragment.clone())
            .collect()
    }

    pub fn get_fragment(&self, index: usize) -> Option<TextFragment> {
        self.fragments
            .lock()
            .unwrap()
            .get(index)
            .map(|q| q.fragment.clone())
    }

    pub fn set_audio(&self, index: usize, audio: Vec<u8>) {
        let mut queue = self.fragments.lock().unwrap();
        if let Some(queued) = queue.get_mut(index) {
            queued.audio = Some(audio);
        }
    }

    pub fn get_audio(&self, index: usize) -> Option<Vec<u8>> {
        self.fragments
            .lock()
            .unwrap()
            .get(index)
            .and_then(|q| q.audio.clone())
    }

    pub fn has_audio(&self, index: usize) -> bool {
        self.fragments
            .lock()
            .unwrap()
            .get(index)
            .map(|q| q.audio.is_some())
            .unwrap_or(false)
    }

    /// Index of the next fragment to play, or None at the end.
    /// Does NOT mutate current_index — use [`Self::set_current_index`] with the result.
    pub fn next(&self) -> Option<usize> {
        let current_idx = self.current_index.load(Ordering::SeqCst);
        let queue_len = self.len();
        if queue_len == 0 {
            None
        } else if current_idx == usize::MAX {
            Some(0)
        } else if current_idx + 1 < queue_len {
            Some(current_idx + 1)
        } else {
            None
        }
    }

    pub fn previous(&self) -> Option<usize> {
        let current_idx = self.current_index.load(Ordering::SeqCst);
        if current_idx == 0 || current_idx == usize::MAX {
            None
        } else {
            Some(current_idx.saturating_sub(1))
        }
    }

    pub fn skip_to(&self, index: usize) -> Result<(), String> {
        let queue_len = self.len();
        if index >= queue_len {
            return Err(format!(
                "Invalid fragment index: {} (queue size: {})",
                index, queue_len
            ));
        }
        self.current_index.store(index, Ordering::SeqCst);
        Ok(())
    }

    pub fn stop(&self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        self.set_status(QueueStatus::Stopped);
    }

    pub fn should_stop(&self) -> bool {
        self.stop_flag.load(Ordering::SeqCst)
    }

    pub fn clear_stop_flag(&self) {
        self.stop_flag.store(false, Ordering::SeqCst);
    }

    pub fn pause(&self) {
        if self.status() == QueueStatus::Playing {
            self.set_status(QueueStatus::Paused);
        }
    }

    pub fn resume(&self) {
        if self.status() == QueueStatus::Paused {
            self.set_status(QueueStatus::Playing);
        }
    }

    pub fn start(&self) {
        if self.is_empty() {
            log::warn!("[FragmentQueue] Cannot start: queue is empty");
            return;
        }
        self.clear_stop_flag();
        if self.current_index.load(Ordering::SeqCst) == usize::MAX {
            self.current_index.store(0, Ordering::SeqCst);
        }
        self.set_status(QueueStatus::Playing);
    }
}

impl Default for FragmentQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frag(text: &str, index: usize, total: usize) -> TextFragment {
        TextFragment::new(text.to_string(), index, total)
    }

    #[test]
    fn test_empty_queue() {
        let queue = FragmentQueue::new();
        assert!(queue.is_empty());
        assert_eq!(queue.status(), QueueStatus::Idle);
    }

    #[test]
    fn test_add_and_clear() {
        let queue = FragmentQueue::new();
        queue.add_fragments(vec![frag("a", 0, 2), frag("b", 1, 2)]);
        assert_eq!(queue.len(), 2);
        queue.clear();
        assert!(queue.is_empty());
    }

    #[test]
    fn test_navigation() {
        let queue = FragmentQueue::new();
        queue.add_fragments(vec![frag("a", 0, 3), frag("b", 1, 3), frag("c", 2, 3)]);
        assert_eq!(queue.next(), Some(0));
        queue.skip_to(0).unwrap();
        assert_eq!(queue.next(), Some(1));
        queue.skip_to(2).unwrap();
        assert_eq!(queue.next(), None);
        assert_eq!(queue.previous(), Some(1));
    }

    #[test]
    fn test_audio_storage_ordered() {
        let queue = FragmentQueue::new();
        queue.add_fragments(vec![frag("a", 0, 2), frag("b", 1, 2)]);
        assert!(!queue.has_audio(0));
        // Out-of-order set: store fragment 1's audio first, then 0.
        queue.set_audio(1, vec![9, 9, 9]);
        queue.set_audio(0, vec![1, 2, 3]);
        assert_eq!(queue.get_audio(0), Some(vec![1, 2, 3]));
        assert_eq!(queue.get_audio(1), Some(vec![9, 9, 9]));
    }

    #[test]
    fn test_status_transitions() {
        let queue = FragmentQueue::new();
        queue.add_fragment(frag("a", 0, 1));
        assert_eq!(queue.status(), QueueStatus::Idle);
        queue.start();
        assert_eq!(queue.status(), QueueStatus::Playing);
        queue.pause();
        assert_eq!(queue.status(), QueueStatus::Paused);
        queue.resume();
        assert_eq!(queue.status(), QueueStatus::Playing);
        queue.stop();
        assert_eq!(queue.status(), QueueStatus::Stopped);
    }

    #[test]
    fn test_stop_flag() {
        let queue = FragmentQueue::new();
        assert!(!queue.should_stop());
        queue.stop();
        assert!(queue.should_stop());
        queue.clear_stop_flag();
        assert!(!queue.should_stop());
    }
}
