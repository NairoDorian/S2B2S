//! The OS's preferred languages, for suggesting "Languages I speak": the
//! onboarding step and the empty list on the General page.
//!
//! Port of upstream's `system_languages.rs` (`0e5e9ca`) without its settings
//! backfill. Upstream fills `spoken_languages` from these for users who never
//! set it; here that list is the engine's language allowlist
//! (`RunOptions::allowed_languages`), so a guess would hold every model to
//! languages the user never confirmed. It is only ever set by the user; this
//! module only suggests. The picker's spelling and the "up to three plus
//! English" rule live in `src/lib/languageSuggestions.ts`, beside the picker's
//! language list.

use once_cell::sync::Lazy;

/// Read once: the list rarely changes while the app runs.
static LANGUAGES: Lazy<Vec<String>> = Lazy::new(|| {
    let languages = read();
    log::info!("System languages: {languages:?}");
    languages
});

/// Windows: Settings → Time & Language's list, which also holds the
/// keyboard-only languages that the display languages (`sys-locale`) miss.
#[cfg(windows)]
fn read() -> Vec<String> {
    use windows::System::UserProfile::GlobalizationPreferences;
    let languages = GlobalizationPreferences::Languages()
        .map(|list| list.into_iter().map(|l| l.to_string()).collect::<Vec<_>>());
    match languages {
        Ok(languages) if !languages.is_empty() => languages,
        Ok(_) => {
            log::warn!("Windows listed no preferred languages; using the display languages");
            sys_locale::get_locales().collect()
        }
        Err(e) => {
            log::warn!(
                "Couldn't read Windows' preferred languages ({e}); using the display languages"
            );
            sys_locale::get_locales().collect()
        }
    }
}

/// macOS: System Settings → Language & Region's preferred languages; Linux:
/// `LANGUAGE`, else the locale.
#[cfg(not(windows))]
fn read() -> Vec<String> {
    sys_locale::get_locales().collect()
}

/// The OS's preferred languages, most preferred first, as BCP-47 tags
/// (`fr-FR`, `zh-Hant-TW`). Async so the first read (a WinRT call on
/// Windows) never runs on the main thread.
#[tauri::command]
#[specta::specta]
pub async fn get_system_languages() -> Result<Vec<String>, String> {
    Ok(LANGUAGES.clone())
}
