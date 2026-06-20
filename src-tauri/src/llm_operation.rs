// Adapted from AivoRelay (MaxITService/AIVORelay), MIT License.
// Source: src-tauri/src/llm_operation.rs — SSE/LLM Async Cancellation Tracker (2026-06-19).

use std::sync::atomic::{AtomicU64, Ordering};

pub struct LlmOperationTracker {
    current_operation_id: AtomicU64,
    cancelled_before_id: AtomicU64,
}

impl Default for LlmOperationTracker {
    fn default() -> Self {
        Self {
            current_operation_id: AtomicU64::new(0),
            cancelled_before_id: AtomicU64::new(0),
        }
    }
}

impl LlmOperationTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts a new LLM operation and returns its unique sequential ID.
    pub fn start_operation(&self) -> u64 {
        self.current_operation_id.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Cancels all LLM operations started before this moment.
    pub fn cancel(&self) {
        let cur = self.current_operation_id.load(Ordering::SeqCst);
        self.cancelled_before_id.store(cur + 1, Ordering::SeqCst);
    }

    /// Checks if a specific operation ID has been cancelled.
    pub fn is_cancelled(&self, id: u64) -> bool {
        id < self.cancelled_before_id.load(Ordering::SeqCst)
    }
}
