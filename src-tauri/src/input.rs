use enigo::{Enigo, Key, Keyboard, Mouse, Settings};
use log::warn;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

#[cfg(target_os = "macos")]
mod macos {
    use super::Key;
    use log::{debug, warn};
    use std::ffi::c_void;

    type TisInputSourceRef = *const c_void;
    type CfDataRef = *const c_void;
    type CfStringRef = *const c_void;

    // kVK_ANSI_V. This is the behavior used before layout-aware
    // resolution and remains the safest fallback if macOS cannot expose the
    // active layout.
    const ANSI_V_KEYCODE: u16 = 9;
    const KEYCODE_COUNT: u16 = 128;
    const UC_KEY_ACTION_DISPLAY: u16 = 3;
    const UC_KEY_TRANSLATE_NO_DEAD_KEYS_MASK: u32 = 1;
    // Carbon's cmdKey is bit 8. UCKeyTranslate expects Carbon modifiers shifted
    // right by 8, so Command is represented by bit 0 here.
    const COMMAND_MODIFIER_STATE: u32 = 1;

    #[link(name = "Carbon", kind = "framework")]
    unsafe extern "C" {
        fn TISCopyCurrentKeyboardLayoutInputSource() -> TisInputSourceRef;
        fn TISGetInputSourceProperty(
            input_source: TisInputSourceRef,
            property_key: CfStringRef,
        ) -> CfDataRef;
        static kTISPropertyUnicodeKeyLayoutData: CfStringRef;
        fn UCKeyTranslate(
            key_layout: *const u8,
            virtual_key_code: u16,
            key_action: u16,
            modifier_key_state: u32,
            keyboard_type: u32,
            key_translate_options: u32,
            dead_key_state: *mut u32,
            max_string_length: usize,
            actual_string_length: *mut usize,
            unicode_string: *mut u16,
        ) -> i32;
        fn LMGetKbdType() -> u8;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFDataGetBytePtr(data: CfDataRef) -> *const u8;
        fn CFRelease(value: *const c_void);
    }

    struct InputSource(TisInputSourceRef);

    impl Drop for InputSource {
        fn drop(&mut self) {
            if !self.0.is_null() {
                // SAFETY: TISCopyCurrentKeyboardLayoutInputSource returned this
                // retained reference, so this balances that ownership.
                unsafe { CFRelease(self.0) };
            }
        }
    }

    fn find_keycode(mut matches: impl FnMut(u16) -> bool) -> Option<u16> {
        (0..KEYCODE_COUNT).find(|&keycode| matches(keycode))
    }

    /// Resolves the physical key that macOS interprets as `v` while Command is
    /// held. Including Command is important: non-Latin layouts commonly map
    /// Cmd shortcuts to their ANSI equivalents, while standard Dvorak does not.
    ///
    /// TIS APIs must run on the main thread. The paste path already enters
    /// through `AppHandle::run_on_main_thread` before reaching this function.
    fn resolve_command_v_keycode() -> Result<u16, String> {
        // SAFETY: This function is called on the macOS main thread. The returned
        // source follows the Create Rule and is released by InputSource::drop.
        let source = InputSource(unsafe { TISCopyCurrentKeyboardLayoutInputSource() });
        if source.0.is_null() {
            return Err("macOS returned no current keyboard layout input source".into());
        }

        // SAFETY: The source remains retained for the duration of the scan and
        // the property constant is provided by Carbon.
        let layout_data =
            unsafe { TISGetInputSourceProperty(source.0, kTISPropertyUnicodeKeyLayoutData) };
        if layout_data.is_null() {
            return Err("current macOS keyboard layout has no Unicode layout data".into());
        }

        // SAFETY: layout_data is a CFData owned by the retained input source and
        // remains valid until source is dropped after the scan.
        let layout = unsafe { CFDataGetBytePtr(layout_data) };
        if layout.is_null() {
            return Err("current macOS keyboard layout data is empty".into());
        }

        // SAFETY: LMGetKbdType has no arguments and returns the current physical
        // keyboard type used by UCKeyTranslate.
        let keyboard_type = unsafe { LMGetKbdType() } as u32;
        let keycode = find_keycode(|keycode| {
            let mut dead_key_state = 0;
            let mut chars = [0_u16; 4];
            let mut length = 0_usize;

            // SAFETY: layout points to valid UCKeyboardLayout bytes while source
            // is retained. All output pointers reference initialized local
            // storage of the declared sizes.
            let status = unsafe {
                UCKeyTranslate(
                    layout,
                    keycode,
                    UC_KEY_ACTION_DISPLAY,
                    COMMAND_MODIFIER_STATE,
                    keyboard_type,
                    UC_KEY_TRANSLATE_NO_DEAD_KEYS_MASK,
                    &mut dead_key_state,
                    chars.len(),
                    &mut length,
                    chars.as_mut_ptr(),
                )
            };

            status == 0 && length == 1 && chars[0] == u16::from(b'v')
        })
        .ok_or_else(|| "could not map Cmd+V in the current macOS keyboard layout".to_string())?;

        Ok(keycode)
    }

    pub(super) fn command_v_key() -> Key {
        match resolve_command_v_keycode() {
            Ok(keycode) => {
                debug!("Resolved Cmd+V for the active macOS layout to keycode {keycode}");
                Key::Other(u32::from(keycode))
            }
            Err(error) => {
                warn!(
                    "Could not resolve Cmd+V for the active macOS layout ({error}); using ANSI V keycode {ANSI_V_KEYCODE}"
                );
                Key::Other(u32::from(ANSI_V_KEYCODE))
            }
        }
    }
}

/// Wrapper for Enigo to store in Tauri's managed state.
/// Enigo is wrapped in a Mutex since it requires mutable access.
pub struct EnigoState(pub Mutex<Enigo>);

impl EnigoState {
    pub fn new() -> Result<Self, String> {
        let enigo = Enigo::new(&Settings::default())
            .map_err(|e| format!("Failed to initialize Enigo: {}", e))?;
        Ok(Self(Mutex::new(enigo)))
    }
}

/// Get the current mouse cursor position using the managed Enigo instance.
/// Returns None if the state is not available or if getting the location fails.
pub fn get_cursor_position(app_handle: &AppHandle) -> Option<(i32, i32)> {
    let enigo_state = app_handle.try_state::<EnigoState>()?;
    let enigo = enigo_state.0.lock().ok()?;
    enigo.location().ok()
}

/// Sends a Ctrl+V or Cmd+V paste command using platform-specific virtual key codes.
/// This ensures the paste works regardless of keyboard layout (e.g., Russian, AZERTY, DVORAK).
/// Note: On Wayland, this may not work - callers should check for Wayland and use alternative methods.
///
/// `hold_ms` is how long the modifier stays held after the V click before being
/// released. Most applications read the modifier from the V event's flags and
/// need no hold at all, but applications that poll global keyboard state when
/// handling the key need the modifier to still be down — the hold insures
/// against those. Callers that can detect a failed chord (e.g. the
/// receipt-sequenced paste path) may use a much shorter hold.
pub fn send_paste_ctrl_v(enigo: &mut Enigo, hold_ms: u64) -> Result<(), String> {
    // Platform-specific key definitions
    #[cfg(target_os = "macos")]
    let (modifier_key, v_key_code) = (Key::Meta, macos::command_v_key());
    #[cfg(target_os = "windows")]
    let (modifier_key, v_key_code) = (Key::Control, Key::Other(0x56)); // VK_V
    #[cfg(target_os = "linux")]
    let (modifier_key, v_key_code) = (Key::Control, Key::Unicode('v'));

    // Press modifier + V
    send_chord(
        enigo,
        &[(modifier_key, "modifier key")],
        &[(v_key_code, "V key")],
        Some(std::time::Duration::from_millis(hold_ms)),
    )
}

/// Sends a Ctrl+Shift+V paste command.
/// This is commonly used in terminal applications on Linux to paste without formatting.
/// Note: On Wayland, this may not work - callers should check for Wayland and use alternative methods.
pub fn send_paste_ctrl_shift_v(enigo: &mut Enigo, hold_ms: u64) -> Result<(), String> {
    // Platform-specific key definitions
    #[cfg(target_os = "macos")]
    let (modifier_key, v_key_code) = (Key::Meta, macos::command_v_key());
    #[cfg(target_os = "windows")]
    let (modifier_key, v_key_code) = (Key::Control, Key::Other(0x56)); // VK_V
    #[cfg(target_os = "linux")]
    let (modifier_key, v_key_code) = (Key::Control, Key::Unicode('v'));

    // Press Ctrl/Cmd + Shift + V
    send_chord(
        enigo,
        &[(modifier_key, "modifier key"), (Key::Shift, "Shift key")],
        &[(v_key_code, "V key")],
        Some(std::time::Duration::from_millis(hold_ms)),
    )
}

/// Sends a Shift+Insert paste command (Windows and Linux only).
/// This is more universal for terminal applications and legacy software.
/// Note: On Wayland, this may not work - callers should check for Wayland and use alternative methods.
pub fn send_paste_shift_insert(enigo: &mut Enigo, hold_ms: u64) -> Result<(), String> {
    // VK_INSERT on Windows, the XK_Insert keysym on Linux (enigo reads
    // `Key::Other` as a keysym there, where 0x76 would be XK_v).
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    let insert_key_code = Key::Insert;
    #[cfg(target_os = "macos")]
    let insert_key_code = Key::Other(0x76); // unused: Shift+Insert is not offered on macOS

    // Press Shift + Insert
    send_chord(
        enigo,
        &[(Key::Shift, "Shift key")],
        &[(insert_key_code, "Insert key")],
        Some(std::time::Duration::from_millis(hold_ms)),
    )
}

/// Presses `modifiers` in order, clicks `keys`, waits `hold` (if any), then
/// releases the modifiers that were pressed, in reverse order.
///
/// The releases run even when a press or a click fails, so an error never
/// leaves Ctrl, Cmd or Shift latched system-wide. Returns the first error.
fn send_chord(
    enigo: &mut Enigo,
    modifiers: &[(Key, &str)],
    keys: &[(Key, &str)],
    hold: Option<std::time::Duration>,
) -> Result<(), String> {
    let mut first_error: Option<String> = None;
    let mut pressed = 0;
    for (key, name) in modifiers {
        match enigo.key(*key, enigo::Direction::Press) {
            Ok(()) => pressed += 1,
            Err(e) => {
                first_error = Some(format!("Failed to press {name}: {e}"));
                break;
            }
        }
    }

    if first_error.is_none() {
        for (key, name) in keys {
            if let Err(e) = enigo.key(*key, enigo::Direction::Click) {
                first_error = Some(format!("Failed to click {name}: {e}"));
                break;
            }
        }
        if first_error.is_none()
            && let Some(hold) = hold
        {
            std::thread::sleep(hold);
        }
    }

    for (key, name) in modifiers[..pressed].iter().rev() {
        if let Err(e) = enigo.key(*key, enigo::Direction::Release)
            && first_error.is_none()
        {
            first_error = Some(format!("Failed to release {name}: {e}"));
        }
    }

    first_error.map_or(Ok(()), Err)
}

/// Pastes text directly using the enigo text method.
/// This tries to use system input methods if possible, otherwise simulates keystrokes one by one.
pub fn paste_text_direct(enigo: &mut Enigo, text: &str) -> Result<(), String> {
    enigo
        .text(text)
        .map_err(|e| format!("Failed to send text directly: {}", e))?;

    Ok(())
}

/// Parse a single key token (e.g. "ctrl", "space", "f1") into an enigo `Key`.
fn parse_key_token(token: &str) -> Option<Key> {
    let token = token.to_lowercase();
    match token.as_str() {
        "ctrl" | "control" => Some(Key::Control),
        "shift" => Some(Key::Shift),
        "alt" | "option" => Some(Key::Alt),
        "cmd" | "command" | "super" | "win" | "windows" | "meta" => Some(Key::Meta),
        "space" => Some(Key::Space),
        "enter" | "return" => Some(Key::Return),
        "esc" | "escape" => Some(Key::Escape),
        "tab" => Some(Key::Tab),
        "backspace" => Some(Key::Backspace),
        "delete" => Some(Key::Delete),
        "up" => Some(Key::UpArrow),
        "down" => Some(Key::DownArrow),
        "left" => Some(Key::LeftArrow),
        "right" => Some(Key::RightArrow),
        "capslock" | "caps_lock" | "caps lock" => Some(Key::CapsLock),
        "f1" => Some(Key::F1),
        "f2" => Some(Key::F2),
        "f3" => Some(Key::F3),
        "f4" => Some(Key::F4),
        "f5" => Some(Key::F5),
        "f6" => Some(Key::F6),
        "f7" => Some(Key::F7),
        "f8" => Some(Key::F8),
        "f9" => Some(Key::F9),
        "f10" => Some(Key::F10),
        "f11" => Some(Key::F11),
        "f12" => Some(Key::F12),
        "f13" => Some(Key::F13),
        "f14" => Some(Key::F14),
        "f15" => Some(Key::F15),
        "f16" => Some(Key::F16),
        "f17" => Some(Key::F17),
        "f18" => Some(Key::F18),
        "f19" => Some(Key::F19),
        "f20" => Some(Key::F20),
        "f21" => Some(Key::F21),
        "f22" => Some(Key::F22),
        "f23" => Some(Key::F23),
        "f24" => Some(Key::F24),
        _ if token.len() == 1 => token.chars().next().map(Key::Unicode),
        _ => None,
    }
}

/// Check if a Key is a modifier key (Control, Shift, Alt/Meta).
fn is_modifier_key(key: &Key) -> bool {
    matches!(key, Key::Control | Key::Shift | Key::Alt | Key::Meta)
}

/// Simulate a keyboard shortcut (e.g. "ctrl+space", "ctrl+alt+space") by
/// pressing the modifier keys, clicking the main key, and releasing the
/// modifiers in reverse order.
pub fn simulate_shortcut(enigo: &mut Enigo, shortcut: &str) -> Result<(), String> {
    let parts: Vec<&str> = shortcut.split('+').map(|p| p.trim()).collect();

    let mut modifiers: Vec<(Key, &str)> = Vec::new();
    let mut main_keys: Vec<(Key, &str)> = Vec::new();

    for part in &parts {
        if part.is_empty() {
            continue;
        }
        match parse_key_token(part) {
            Some(key) if is_modifier_key(&key) => modifiers.push((key, "modifier key")),
            Some(key) => main_keys.push((key, "key")),
            None => warn!("simulate_shortcut: unknown key token '{}'", part),
        }
    }

    if main_keys.is_empty() {
        return Err("Shortcut must contain at least one non-modifier key".into());
    }

    // Press the modifiers, click the main keys, release the modifiers in
    // reverse order — also when a press or a click fails.
    send_chord(enigo, &modifiers, &main_keys, None)
}
