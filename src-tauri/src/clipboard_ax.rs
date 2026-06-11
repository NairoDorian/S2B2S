/// Platform-specific selection capture (macOS AX API).
/// On other platforms falls back to sentinel-clipboard capture.
///
/// macOS AX API path (from Parrot's proven pattern):
///   AXUIElementCreateSystemWide → AXFocusedUIElement → AXSelectedText
///   Retries 3 times at 0/40/90ms because some apps expose selection late.

#[cfg(target_os = "macos")]
mod macos_ax {
    use objc::{class, msg_send, sel, sel_impl};
    use objc::runtime::{Object, BOOL, YES};
    use std::ffi::CString;

    extern "C" {
        fn AXUIElementCreateSystemWide() -> *mut Object;
        fn AXUIElementCopyAttributeValue(
            element: *mut Object,
            attribute: *mut Object,
            value: *mut *mut Object,
        ) -> i32;
        fn CFStringGetLength(theString: *mut Object) -> usize;
        fn CFStringGetCString(
            theString: *mut Object,
            buffer: *mut i8,
            bufferSize: usize,
            encoding: usize,
        ) -> BOOL;
        fn CFRelease(cf: *mut Object);
    }

    const kAXFocusedUIElementAttribute: &str = "AXFocusedUIElement";
    const kAXSelectedTextAttribute: &str = "AXSelectedText";
    const kCFStringEncodingUTF8: usize = 0x08000100;
    const kAXErrorSuccess: i32 = 0;

    fn get_attribute_value(element: *mut Object, attribute: &str) -> Option<String> {
        unsafe {
            let attr = CString::new(attribute).ok()?;
            let attr_cf = CFStringCreateWithCString(attr.as_ptr());
            if attr_cf.is_null() { return None; }
            let mut value: *mut Object = std::ptr::null_mut();
            let result = AXUIElementCopyAttributeValue(element, attr_cf, &mut value);
            CFRelease(attr_cf);
            if result != kAXErrorSuccess || value.is_null() {
                return None;
            }
            let len = CFStringGetLength(value);
            if len == 0 {
                CFRelease(value);
                return None;
            }
            let mut buf = vec![0i8; len * 4 + 64];
            let ok = CFStringGetCString(value, buf.as_mut_ptr(), buf.len(), kCFStringEncodingUTF8);
            CFRelease(value);
            if ok == YES {
                Some(CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned())
            } else {
                None
            }
        }
    }

    unsafe fn CFStringCreateWithCString(cstr: *const i8) -> *mut Object {
        extern "C" {
            fn CFStringCreateWithCString(
                alloc: *mut Object,
                cStr: *const i8,
                encoding: usize,
            ) -> *mut Object;
        }
        CFStringCreateWithCString(std::ptr::null_mut(), cstr, kCFStringEncodingUTF8)
    }

    /// Read selected text via macOS Accessibility API.
    /// Returns `None` if no selection or AX is unavailable.
    pub fn read_selected_text() -> Option<String> {
        unsafe {
            let system = AXUIElementCreateSystemWide();
            if system.is_null() { return None; }

            let focused = get_attribute_value(system, kAXFocusedUIElementAttribute);
            CFRelease(system);

            if let Some(focused_str) = &focused {
                // Focused element is a string representation; use system-wide element directly
                let system2 = AXUIElementCreateSystemWide();
                let text = get_attribute_value(system2, kAXSelectedTextAttribute);
                CFRelease(system2);
                text
            } else {
                None
            }
        }
    }
}

use crate::settings::get_settings;
use tauri::AppHandle;

/// Capture selected text from the currently focused application.
///
/// Tier 1 (macOS): Accessibility API — no clipboard touch.
/// Tier 2 (all platforms): Sentinel clipboard — write unique string, Ctrl+C, read, restore.
pub fn capture_selection(app: &AppHandle) -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        // Try AX API first (no clipboard touch)
        for delay_ms in &[0u64, 40, 90] {
            if *delay_ms > 0 {
                std::thread::sleep(std::time::Duration::from_millis(*delay_ms));
            }
            if let Some(text) = macos_ax::read_selected_text() {
                if !text.is_empty() {
                    log::info!("[Selection] macOS AX API read {} chars", text.len());
                    return Ok(text);
                }
            }
        }
        log::debug!("[Selection] AX API failed, falling back to sentinel clipboard");
    }

    // Fallback: sentinel clipboard for all platforms
    let clipboard = app.clipboard();
    let previous = clipboard.read_text().ok();

    let sentinel = format!("__S2B2S_SEL_{}__", std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0));
    let _ = clipboard.write_text(sentinel.clone());

    // Send Ctrl+C
    crate::input::send_copy_ctrl_c();

    std::thread::sleep(std::time::Duration::from_millis(150));

    let captured = clipboard.read_text().ok();

    // Restore previous clipboard content
    if let Some(prev) = previous {
        let _ = clipboard.write_text(prev);
    } else if let Ok(captured_text) = &captured {
        if captured_text != &sentinel {
            let _ = clipboard.write_text(captured_text.clone());
        }
    }

    match captured {
        Some(text) if text != sentinel && !text.trim().is_empty() => {
            Ok(text)
        }
        _ => Err("No text selected".to_string()),
    }
}
