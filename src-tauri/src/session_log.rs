//! Per-session durable log file naming.
//!
//! Each app open writes to its own file so the debug console never mixes
//! records from a previous run with the current one. The basename is fixed
//! once at process start (before the Tauri log plugin opens its file) and is
//! shared with `commands::current_log_file` so reader and writer always agree.

use std::sync::OnceLock;

use crate::app_identity::RECORDING_BASENAME;

static SESSION_BASENAME: OnceLock<String> = OnceLock::new();

/// `zer0-YYYYMMDD-HHMMSS-mmm` — unique per process open (millisecond stamp
/// guards two launches inside the same second).
fn new_session_stem() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    // Human-readable UTC date from the unix millis without a chrono dependency.
    let (y, mo, d, h, mi, s, ms) = civil_from_unix_ms(now);
    format!("{RECORDING_BASENAME}-{y:04}{mo:02}{d:02}-{h:02}{mi:02}{s:02}-{ms:03}")
}

/// Idempotent: first call freezes the stem for this process.
pub fn init() -> &'static str {
    SESSION_BASENAME.get_or_init(new_session_stem)
}

/// The frozen stem (`zer0-…`), or `None` if `init` has not run yet.
pub fn basename() -> Option<&'static str> {
    SESSION_BASENAME.get().map(|s| s.as_str())
}

/// Howard Hinnant's civil_from_days algorithm, unpacked for unix millis.
fn civil_from_unix_ms(ms: u128) -> (i64, u32, u32, u32, u32, u32, u32) {
    let secs = (ms / 1000) as i64;
    let millis = (ms % 1000) as u32;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (h, mi, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);

    // days -> y/m/d (civil_from_days)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    (y, m as u32, d as u32, h as u32, mi as u32, s as u32, millis)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stem_is_stable_after_init() {
        let a = init();
        let b = init();
        assert_eq!(a, b);
        assert!(a.starts_with(&format!("{RECORDING_BASENAME}-")));
        assert!(!a.ends_with(".log"));
    }

    #[test]
    fn civil_conversion_is_plausible() {
        // 2026-09-22 12:00:00 UTC
        let (y, mo, d, h, mi, s, ms) = civil_from_unix_ms(1_790_078_400_000);
        assert_eq!((y, mo, d), (2026, 9, 22));
        assert_eq!((h, mi, s, ms), (12, 0, 0, 0));
    }
}
