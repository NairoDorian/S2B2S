//! GENERATED FILE — DO NOT EDIT.
//!
//! Source: `scripts/app-meta.ts` — run `bun run meta:sync` to regenerate.
//!
//! Do not add a literal of the project's identity anywhere else in this crate.
//! Import from here instead, so a rename stays a one-line change.

/// Product name, for anything the user sees: window titles, tray tooltip,
/// log lines, the about page.
pub const NAME: &str = "ZER0";

/// Machine slug: the crate, the binary, `zer0.exe`, the `package.json`
/// name. Never shown to the user.
pub const SLUG: &str = "zer0";

/// Bundle identifier — the OS-level application identity.
pub const IDENTIFIER: &str = "com.nairodorian.zer0";

/// Repository URL, for links out of the app.
pub const REPO_URL: &str = "https://github.com/NairoDorian/S2B2S";

/// Releases page — where an update or a portable installer link points.
pub const RELEASES_URL: &str = "https://github.com/NairoDorian/S2B2S/releases/latest";

/// Who this application says it is on outbound HTTP requests: model downloads,
/// GitHub API calls, LLM provider calls. `name/version` is the shape every one
/// of those services expects.
pub const USER_AGENT: &str = "ZER0/0.9.7";

/// The folder under the OS app-data directory that holds models, history and
/// settings. Kept separate from `IDENTIFIER` because the folder name is also
/// what the user sees in `%APPDATA%` / `~/.local/share`.
pub const DATA_DIR_NAME: &str = "ZER0";

/// The folder name a pre-rename install used. Read at startup by the one-shot
/// app-data migration; never written.
pub const LEGACY_DATA_DIR_NAME: &str = "Handy";

/// The bundle identifier a pre-rename install used, named in the migration log
/// and in the portable-install notes.
pub const LEGACY_IDENTIFIER: &str = "com.pais.handy";

/// Prefix of every environment flag this application reads.
pub const ENV_PREFIX: &str = "ZER0_";

/// The previous environment-flag prefix. Every flag lookup falls back to it and
/// warns, so an existing shell profile or Nix wrapper keeps working.
pub const LEGACY_ENV_PREFIX: &str = "HANDY_";

/// Magic string written into the `portable` marker beside the executable.
pub const PORTABLE_MARKER: &str = "ZER0 Portable Mode";

/// The marker a pre-rename release wrote. Accepted on read, never written
/// unless an older empty marker is being upgraded.
pub const LEGACY_PORTABLE_MARKER: &str = "Handy Portable Mode";

/// Base name of the WAV files a recording is written to before transcription.
pub const RECORDING_BASENAME: &str = "zer0";

/// File name a single recording's WAV is saved under, in the recordings
/// directory.
///
/// A function rather than a `format!` at each call site so the shape of the
/// name lives in one place: the history row a transcription writes must name
/// the same file the recorder wrote.
pub fn recording_file_name(timestamp: i64) -> String {
    format!("{RECORDING_BASENAME}-{timestamp}.wav")
}

/// File name a Multi-STT recording's WAV is saved under. The `-multi` infix
/// keeps a parallel recording distinguishable from a plain one in the
/// recordings directory and in history.
pub fn multi_recording_file_name(timestamp: i64) -> String {
    format!("{RECORDING_BASENAME}-multi-{timestamp}.wav")
}
