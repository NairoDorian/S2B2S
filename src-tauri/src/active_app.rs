//! Foreground application detection.
//! Uses Win32 GetForegroundWindow on Windows; returns None on other platforms.

#[cfg(target_os = "windows")]
pub fn get_frontmost_app_name() -> Option<String> {
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW};

    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return None;
        }

        let mut buffer = [0u16; 512];
        let len = GetWindowTextW(hwnd, &mut buffer);
        if len > 0 {
            let title = String::from_utf16_lossy(&buffer[..len as usize]);
            return Some(title);
        }
    }
    None
}

#[cfg(not(target_os = "windows"))]
pub fn get_frontmost_app_name() -> Option<String> {
    None
}
