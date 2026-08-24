use crate::clipboard::paste_direct;
use crate::settings::AppSettings;
use log::{debug, warn};
use std::collections::VecDeque;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tauri::AppHandle;

enum DirectStreamCmd {
    Feed(String),
    Flush(Option<String>, Sender<()>),
    Cancel,
}

pub struct DirectStreamWriter {
    tx: Option<Sender<DirectStreamCmd>>,
    worker_handle: Option<JoinHandle<()>>,
}

impl DirectStreamWriter {
    pub fn new(app_handle: AppHandle, speed: u32, settings: AppSettings) -> Self {
        let (tx, rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            run_direct_stream_worker(app_handle, rx, speed, settings);
        });

        Self {
            tx: Some(tx),
            worker_handle: Some(handle),
        }
    }

    pub fn feed(&self, text: String) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(DirectStreamCmd::Feed(text));
        }
    }

    pub fn flush(mut self, remaining_tail: Option<String>) {
        if let Some(tx) = self.tx.take() {
            let (reply_tx, reply_rx) = mpsc::channel();
            if tx
                .send(DirectStreamCmd::Flush(remaining_tail, reply_tx))
                .is_ok()
            {
                // Wait briefly for flush to complete (up to 3 seconds)
                let _ = reply_rx.recv_timeout(Duration::from_secs(3));
            }
        }
        if let Some(handle) = self.worker_handle.take() {
            let _ = handle.join();
        }
    }

    pub fn cancel(mut self) {
        if let Some(tx) = self.tx.take() {
            let _ = tx.send(DirectStreamCmd::Cancel);
        }
        if let Some(handle) = self.worker_handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for DirectStreamWriter {
    fn drop(&mut self) {
        if let Some(tx) = self.tx.take() {
            let _ = tx.send(DirectStreamCmd::Cancel);
        }
    }
}

fn run_direct_stream_worker(
    app_handle: AppHandle,
    rx: Receiver<DirectStreamCmd>,
    speed: u32,
    settings: AppSettings,
) {
    let speed = speed.clamp(10, 60);
    let interval_ms = (1000 / speed).clamp(8, 100) as u64;
    let tick_duration = Duration::from_millis(interval_ms);
    let threshold = 15.max(((speed as f32) * 0.6).round() as usize);

    let mut queue: VecDeque<char> = VecDeque::new();
    let flush_reply: Option<Sender<()>>;

    'worker: loop {
        // Process any pending commands without blocking if queue is not empty,
        // or block for tick_duration if queue has items, or block indefinitely if queue is empty.
        let cmd = if queue.is_empty() {
            match rx.recv() {
                Ok(c) => c,
                Err(_) => return,
            }
        } else {
            match rx.recv_timeout(tick_duration) {
                Ok(c) => c,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    // Time to type the next chunk
                    let step = if queue.len() > threshold * 2 {
                        3
                    } else if queue.len() > threshold {
                        2
                    } else {
                        1
                    };

                    let chunk: String = (0..step).filter_map(|_| queue.pop_front()).collect();
                    if !chunk.is_empty() {
                        if let Err(e) = paste_direct(
                            &chunk,
                            &app_handle,
                            #[cfg(target_os = "linux")]
                            settings.typing_tool,
                        ) {
                            warn!("DirectStreamWriter: failed to type chunk: {}", e);
                        }
                    }
                    continue 'worker;
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
        };

        match cmd {
            DirectStreamCmd::Feed(text) => {
                queue.extend(text.chars());
            }
            DirectStreamCmd::Flush(tail, reply) => {
                if let Some(tail_text) = tail {
                    queue.extend(tail_text.chars());
                }
                flush_reply = Some(reply);
                break 'worker;
            }
            DirectStreamCmd::Cancel => {
                queue.clear();
                return;
            }
        }

        // Type the next characters if queue has items
        if !queue.is_empty() {
            let step = if queue.len() > threshold * 2 {
                3
            } else if queue.len() > threshold {
                2
            } else {
                1
            };

            let chunk: String = (0..step).filter_map(|_| queue.pop_front()).collect();
            if !chunk.is_empty() {
                if let Err(e) = paste_direct(
                    &chunk,
                    &app_handle,
                    #[cfg(target_os = "linux")]
                    settings.typing_tool,
                ) {
                    warn!("DirectStreamWriter: failed to type chunk: {}", e);
                }
            }
        }
    }

    // Flush any remaining characters at once
    if !queue.is_empty() {
        let remaining: String = queue.drain(..).collect();
        if let Err(e) = paste_direct(
            &remaining,
            &app_handle,
            #[cfg(target_os = "linux")]
            settings.typing_tool,
        ) {
            warn!("DirectStreamWriter: failed to flush remaining text: {}", e);
        }
    }

    // Trailing space
    if settings.append_trailing_space {
        let _ = paste_direct(
            " ",
            &app_handle,
            #[cfg(target_os = "linux")]
            settings.typing_tool,
        );
    }

    // Trailing newline
    if settings.append_trailing_newline {
        let _ = paste_direct(
            "\n",
            &app_handle,
            #[cfg(target_os = "linux")]
            settings.typing_tool,
        );
    }

    // Auto submit
    if settings.auto_submit {
        thread::sleep(Duration::from_millis(50));
        let _ = crate::clipboard::with_enigo(&app_handle, |enigo| {
            crate::clipboard::send_return_key(enigo, settings.auto_submit_key)
        });
    }

    if let Some(reply) = flush_reply {
        let _ = reply.send(());
    }

    debug!("DirectStreamWriter finished writing");
}
