//! CopySpeak double-copy trigger: copy the same text twice within a short
//! window and it is read aloud — no hotkey needed.
//!
//! Detection requires noticing a clipboard *write* even when the content is
//! unchanged. On Windows this is cheap and reliable via
//! `GetClipboardSequenceNumber` (no clipboard open, no events). On macOS/Linux
//! an equivalent change counter needs platform plumbing we don't have yet, so
//! the watcher idles there.
//! TODO(cross-platform): macOS NSPasteboard.changeCount and X11/Wayland
//! ownership events for double-copy detection.

use crate::settings::get_settings;
use std::time::Duration;
use tauri::AppHandle;

/// Maximum text length the double-copy trigger will speak.
#[cfg(windows)]
const MAX_TEXT_CHARS: usize = 100_000;

/// Start the watcher thread. Cheap when the feature is disabled (slow idle poll).
pub fn start(app: AppHandle) {
    std::thread::spawn(move || watch_loop(app));
}

#[cfg(windows)]
fn watch_loop(app: AppHandle) {
    use crate::tts::manager::TtsManager;
    use std::sync::Arc;
    use std::time::Instant;
    use tauri::Manager;
    use tauri_plugin_clipboard_manager::ClipboardExt;
    use windows::Win32::System::DataExchange::GetClipboardSequenceNumber;

    log::info!("[DoubleCopy] clipboard watcher started");
    let mut last_seq = unsafe { GetClipboardSequenceNumber() };
    // The text seen at the previous clipboard change, and when it appeared.
    let mut last_text: Option<String> = None;
    let mut last_change = Instant::now();

    loop {
        let settings = get_settings(&app);
        if !(settings.tts.enabled && settings.tts.double_copy_enabled) {
            last_text = None;
            std::thread::sleep(Duration::from_millis(1000));
            continue;
        }
        let window = Duration::from_millis(u64::from(settings.tts.double_copy_window_ms.max(200)));

        let seq = unsafe { GetClipboardSequenceNumber() };
        if seq != last_seq {
            last_seq = seq;
            let now = Instant::now();
            if let Ok(text) = app.clipboard().read_text() {
                let trimmed_len = text.trim().len();
                if trimmed_len > 0 && text.chars().count() <= MAX_TEXT_CHARS {
                    let is_double = last_text.as_deref() == Some(text.as_str())
                        && now.duration_since(last_change) <= window;
                    if is_double {
                        log::info!("[DoubleCopy] triggered ({} chars)", text.chars().count());
                        if let Some(tts) = app.try_state::<Arc<TtsManager>>() {
                            tts.speak(text);
                        }
                        // Reset so a third copy starts a fresh sequence.
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

#[cfg(not(windows))]
fn watch_loop(app: AppHandle) {
    // Graceful degradation: the setting exists but detection is not yet
    // implemented on this OS (needs a clipboard change counter, see module docs).
    loop {
        let settings = get_settings(&app);
        if settings.tts.enabled && settings.tts.double_copy_enabled {
            log::warn!(
                "[DoubleCopy] double-copy trigger is not yet supported on this platform; \
                 use the speak-selection shortcut instead"
            );
            // Log once per enable; then idle until toggled off and on again.
            while get_settings(&app).tts.double_copy_enabled {
                std::thread::sleep(Duration::from_millis(2000));
            }
        }
        std::thread::sleep(Duration::from_millis(2000));
    }
}
