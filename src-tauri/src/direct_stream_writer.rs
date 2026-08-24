use crate::clipboard::{backspace_direct, paste_direct};
use crate::settings::AppSettings;
use log::{debug, warn};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tauri::AppHandle;

enum DirectStreamCmd {
    UpdateTarget(String),
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

    pub fn update_target(&self, text: String) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(DirectStreamCmd::UpdateTarget(text));
        }
    }

    pub fn flush(mut self, final_text: Option<String>) {
        if let Some(tx) = self.tx.take() {
            let (reply_tx, reply_rx) = mpsc::channel();
            if tx
                .send(DirectStreamCmd::Flush(final_text, reply_tx))
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

fn common_prefix_char_len(a: &str, b: &str) -> usize {
    a.chars()
        .zip(b.chars())
        .take_while(|(c1, c2)| c1 == c2)
        .count()
}

fn step_typewriter(
    target: &str,
    typed: &mut String,
    threshold: usize,
    app_handle: &AppHandle,
    #[cfg(target_os = "linux")] typing_tool: crate::settings::TypingTool,
) {
    if typed.as_str() == target {
        return;
    }

    let common_chars = common_prefix_char_len(typed, target);
    let typed_char_count = typed.chars().count();
    let target_char_count = target.chars().count();

    // If typed has characters beyond the common prefix, backspace them out
    if typed_char_count > common_chars {
        let backspaces = typed_char_count - common_chars;
        if let Err(e) = backspace_direct(
            backspaces,
            app_handle,
            #[cfg(target_os = "linux")]
            typing_tool,
        ) {
            warn!("DirectStreamWriter: failed to backspace: {}", e);
        }
        let byte_pos = typed
            .char_indices()
            .nth(common_chars)
            .map(|(idx, _)| idx)
            .unwrap_or(typed.len());
        typed.truncate(byte_pos);
    }

    // Advance 1, 2, or 3 characters from target
    let current_char_count = typed.chars().count();
    let remaining_chars = target_char_count.saturating_sub(current_char_count);
    if remaining_chars > 0 {
        let step = if remaining_chars > threshold * 2 {
            3
        } else if remaining_chars > threshold {
            2
        } else {
            1
        };

        let chars_to_take = step.min(remaining_chars);
        let chunk: String = target
            .chars()
            .skip(current_char_count)
            .take(chars_to_take)
            .collect();

        if !chunk.is_empty() {
            if let Err(e) = paste_direct(
                &chunk,
                app_handle,
                #[cfg(target_os = "linux")]
                typing_tool,
            ) {
                warn!("DirectStreamWriter: failed to type chunk: {}", e);
            }
            typed.push_str(&chunk);
        }
    }
}

fn sync_to_target(
    target: &str,
    typed: &mut String,
    app_handle: &AppHandle,
    #[cfg(target_os = "linux")] typing_tool: crate::settings::TypingTool,
) {
    if typed.as_str() == target {
        return;
    }

    let common_chars = common_prefix_char_len(typed, target);
    let typed_char_count = typed.chars().count();

    if typed_char_count > common_chars {
        let backspaces = typed_char_count - common_chars;
        if let Err(e) = backspace_direct(
            backspaces,
            app_handle,
            #[cfg(target_os = "linux")]
            typing_tool,
        ) {
            warn!("DirectStreamWriter: failed to backspace on sync: {}", e);
        }
        let byte_pos = typed
            .char_indices()
            .nth(common_chars)
            .map(|(idx, _)| idx)
            .unwrap_or(typed.len());
        typed.truncate(byte_pos);
    }

    let current_char_count = typed.chars().count();
    let chunk: String = target.chars().skip(current_char_count).collect();
    if !chunk.is_empty() {
        if let Err(e) = paste_direct(
            &chunk,
            app_handle,
            #[cfg(target_os = "linux")]
            typing_tool,
        ) {
            warn!("DirectStreamWriter: failed to type sync chunk: {}", e);
        }
        typed.push_str(&chunk);
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

    let mut target_text = String::new();
    let mut typed_text = String::new();
    let flush_reply: Option<Sender<()>>;

    'worker: loop {
        // If caught up with target, block until next command; otherwise wait up to tick_duration
        let cmd = if typed_text == target_text {
            match rx.recv() {
                Ok(c) => Some(c),
                Err(_) => return,
            }
        } else {
            match rx.recv_timeout(tick_duration) {
                Ok(c) => Some(c),
                Err(mpsc::RecvTimeoutError::Timeout) => None,
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
        };

        if let Some(cmd) = cmd {
            match cmd {
                DirectStreamCmd::UpdateTarget(new_target) => {
                    target_text = new_target;
                }
                DirectStreamCmd::Flush(final_text, reply) => {
                    if let Some(text) = final_text {
                        target_text = text;
                    }
                    flush_reply = Some(reply);
                    break 'worker;
                }
                DirectStreamCmd::Cancel => {
                    return;
                }
            }
        }

        // Advance typewriter step towards target_text
        step_typewriter(
            &target_text,
            &mut typed_text,
            threshold,
            &app_handle,
            #[cfg(target_os = "linux")]
            settings.typing_tool,
        );
    }

    // Flush any remaining characters immediately so output is 100% synchronized
    sync_to_target(
        &target_text,
        &mut typed_text,
        &app_handle,
        #[cfg(target_os = "linux")]
        settings.typing_tool,
    );

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
