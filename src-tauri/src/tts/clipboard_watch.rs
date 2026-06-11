//! CopySpeak double-copy trigger: copy the same text twice within a short
//! window and it is read aloud — no hotkey needed.
//!
//! On Windows uses `GetClipboardSequenceNumber` (no clipboard content read
//! needed). On macOS uses NSPasteboard.changeCount. On Linux polls the
//! clipboard text content and compares strings for cross-platform fallback.

use crate::settings::get_settings;
use std::time::Duration;
use tauri::AppHandle;

/// Maximum text length the double-copy trigger will speak.
const MAX_TEXT_CHARS: usize = 100_000;
/// Configurable max text length for truncation events
#[allow(dead_code)]
const TRUNCATION_ACTIVE: bool = true;

/// Start the watcher thread. Cheap when the feature is disabled (slow idle poll).
pub fn start(app: AppHandle) {
    std::thread::spawn(move || watch_loop(app));
}

#[cfg(windows)]
fn get_clipboard_change_id() -> u64 {
    use windows::Win32::System::DataExchange::GetClipboardSequenceNumber;
    unsafe { GetClipboardSequenceNumber() as u64 }
}

#[cfg(target_os = "macos")]
fn get_clipboard_change_id() -> u64 {
    use objc::{class, msg_send, sel, sel_impl};
    use objc::runtime::Object;
    static mut LAST_CHANGE_COUNT: u64 = 0;
    #[link(name = "AppKit", kind = "framework")]
    extern "C" {
        fn NSPasteboard_generalPasteboard() -> *mut Object;
    }
    unsafe {
        let pb = NSPasteboard_generalPasteboard();
        if pb.is_null() {
            return LAST_CHANGE_COUNT;
        }
        let change_count: isize = msg_send![pb, changeCount];
        let new_count = change_count as u64;
        if new_count != LAST_CHANGE_COUNT {
            LAST_CHANGE_COUNT = new_count;
            new_count
        } else {
            LAST_CHANGE_COUNT
        }
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
fn get_clipboard_change_id() -> u64 {
    // On Linux/Wayland, poll clipboard text content and detect changes
    static mut LAST_TEXT_CHECK: String = String::new();
    unsafe {
        let current = get_clipboard_text();
        if current != LAST_TEXT_CHECK {
            LAST_TEXT_CHECK = current.clone();
            current.len() as u64
        } else {
            0
        }
    }
}

fn get_clipboard_text() -> String {
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        // Try xclip first, then wl-paste
        use std::process::Command;
        for cmd in &[("xclip", &["-selection", "clipboard", "-o"] as &[&str]),
                      ("wl-paste", &[] as &[&str])] {
            if let Ok(out) = Command::new(cmd.0).args(cmd.1).output() {
                if out.status.success() {
                    return String::from_utf8_lossy(&out.stdout).trim().to_string();
                }
            }
        }
    }
    String::new()
}

fn watch_loop(app: AppHandle) {
    use crate::tts::manager::TtsManager;
    use std::sync::Arc;
    use std::time::Instant;
    use tauri::Manager;
    use tauri_plugin_clipboard_manager::ClipboardExt;

    log::info!("[DoubleCopy] clipboard watcher started");
    let mut last_seq = get_clipboard_change_id();
    let mut last_text: Option<String> = None;
    let mut last_change = Instant::now();
    let mut warned = false;

    loop {
        let settings = get_settings(&app);
        if !(settings.tts.enabled && settings.tts.double_copy_enabled) {
            last_text = None;
            warned = false;
            std::thread::sleep(Duration::from_millis(1000));
            continue;
        }

        if !warned {
            #[cfg(not(windows))]
            log::info!("[DoubleCopy] running on this platform (content-based detection)");
            warned = true;
        }

        let window = Duration::from_millis(u64::from(settings.tts.double_copy_window_ms.max(200)));

        let seq = get_clipboard_change_id();
        let changed = seq != last_seq;
        last_seq = seq;

        // On non-Windows, also check clipboard content to detect changes
        #[cfg(not(windows))]
        let should_check = changed || {
            // Poll text every 500ms as fallback on macOS/Linux
            std::thread::sleep(Duration::from_millis(300));
            true
        };
        #[cfg(windows)]
        let should_check = changed;
        #[cfg(not(windows))]
        let _ = changed; // suppress unused

        if should_check {
            if let Ok(text) = app.clipboard().read_text() {
                let trimmed_len = text.trim().len();
                if trimmed_len > 0 && text.chars().count() <= MAX_TEXT_CHARS {
                    let now = Instant::now();
                    let is_double = last_text.as_deref() == Some(text.as_str())
                        && now.duration_since(last_change) <= window;
                    if is_double {
                        log::info!("[DoubleCopy] triggered ({} chars)", text.chars().count());
                        if let Some(tts) = app.try_state::<Arc<TtsManager>>() {
                            tts.speak(text);
                        }
                        last_text = None;
                    } else {
                        last_text = Some(text);
                        last_change = now;
                    }
                } else {
                    last_text = None;
                }
            }
        }

        std::thread::sleep(Duration::from_millis(200));
    }
}
