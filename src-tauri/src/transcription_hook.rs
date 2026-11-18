//! Optional user hook (upstream `033c627`, decision D4): an executable in
//! `<app data dir>/hooks/` receives the text about to be pasted on stdin, and
//! its stdout is pasted instead.
//!
//! Opt-in by the file existing — there is no setting. It runs after
//! post-processing and the Multi-STT merge, only on text that is about to be
//! pasted: History keeps the text as produced, and text that is already in
//! the document (direct streaming, the streaming-first session's typing) or
//! that goes to the Recall editor never reaches it. A hook that fails, exits
//! non-zero or runs past [`HOOK_TIMEOUT`] leaves the text unchanged; one that
//! prints nothing means "paste nothing" (a hook may do its own pasting).
//!
//! Windows has no shebang, so the hook is looked up under the extensions it
//! can run — `transcription.exe`, `.cmd`, `.bat`, `.ps1` — before the bare
//! name upstream documents. A `.ps1` runs through Windows PowerShell with
//! UTF-8 on both pipes and the text as its pipeline input (`-join $input`).
//! No console window is opened for any of them.

use anyhow::Context;
use log::{debug, error};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tauri::AppHandle;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

/// Hung hooks are killed so they can't leave the pipeline stuck in "transcribing".
const HOOK_TIMEOUT: Duration = Duration::from_secs(30);

/// The file names a hook is looked up under, first match wins.
#[cfg(windows)]
const HOOK_NAMES: &[&str] = &[
    "transcription.exe",
    "transcription.cmd",
    "transcription.bat",
    "transcription.ps1",
    // Upstream's spelling: only runs if it is itself an executable image, and
    // a failure to start it is logged rather than ignored.
    "transcription",
];
#[cfg(not(windows))]
const HOOK_NAMES: &[&str] = &["transcription"];

/// Returns the hook's output, or `text` unchanged if there's no hook or it
/// fails. Dropping the future kills the hook, so callers can cancel it.
pub async fn apply(app: &AppHandle, text: String) -> String {
    if text.is_empty() {
        return text;
    }
    let Ok(dir) = crate::portable::app_data_dir(app) else {
        return text;
    };
    let Some(hook) = find_hook(&dir.join("hooks")) else {
        return text;
    };

    let started = std::time::Instant::now();
    match run(&hook, &text, HOOK_TIMEOUT).await {
        Ok(output) => {
            debug!(
                "Transcription hook {} ran in {:?} ({} → {} chars)",
                hook.display(),
                started.elapsed(),
                text.chars().count(),
                output.chars().count()
            );
            output
        }
        Err(e) => {
            error!(
                "Transcription hook {} failed, using original text: {:#}",
                hook.display(),
                e
            );
            text
        }
    }
}

/// The first hook file present in `hooks_dir`, if any. A few metadata calls,
/// on the paste path only.
fn find_hook(hooks_dir: &Path) -> Option<PathBuf> {
    HOOK_NAMES
        .iter()
        .map(|name| hooks_dir.join(name))
        .find(|path| path.is_file())
}

/// Builds the command that runs `hook`.
fn hook_command(hook: &Path) -> Command {
    #[cfg(windows)]
    if hook
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("ps1"))
    {
        // Windows PowerShell reads and writes the console code page on
        // redirected pipes, which would mangle every accented character; the
        // wrapper switches both to UTF-8 (no BOM) and pipes stdin into the
        // script. The path is a single-quoted literal, so only `'` needs
        // escaping.
        let literal = hook.to_string_lossy().replace('\'', "''");
        let script = format!(
            "$utf8 = New-Object System.Text.UTF8Encoding $false; \
             [Console]::InputEncoding = $utf8; [Console]::OutputEncoding = $utf8; \
             [Console]::In.ReadToEnd() | & '{literal}'"
        );
        let mut command = Command::new("powershell.exe");
        command.args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ]);
        return command;
    }
    // `.exe` directly; `.cmd` / `.bat` through cmd.exe, which the standard
    // library does itself (with its own argument escaping).
    Command::new(hook)
}

async fn run(hook: &Path, text: &str, timeout: Duration) -> anyhow::Result<String> {
    let mut command = hook_command(hook);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    {
        // CREATE_NO_WINDOW: a console hook would otherwise flash a window
        // over the app the text is about to be pasted into.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command.spawn().context("could not start it")?;

    // Write input while reading output so neither side blocks on a full pipe.
    // Write errors are ignored: a hook may exit without reading its input.
    let mut stdin = child.stdin.take().context("stdin not piped")?;
    let input = text.to_string();
    let write = async move {
        let _ = stdin.write_all(input.as_bytes()).await;
        // Dropping `stdin` here closes it, so a hook reading to the end returns.
    };
    let (_, output) = tokio::time::timeout(timeout, async {
        tokio::join!(write, child.wait_with_output())
    })
    .await
    .map_err(|_| anyhow::anyhow!("timed out after {timeout:?}"))?;
    let output = output?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    anyhow::ensure!(
        output.status.success(),
        "exited with {}: {}",
        output.status,
        stderr.trim()
    );

    Ok(strip_one_line_ending(&String::from_utf8_lossy(&output.stdout)).to_string())
}

/// Drops one trailing line ending (`\n` or `\r\n`), so `echo` output does not
/// submit a terminal line; any other whitespace is the hook's to keep.
fn strip_one_line_ending(output: &str) -> &str {
    output
        .strip_suffix("\r\n")
        .or_else(|| output.strip_suffix('\n'))
        .unwrap_or(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_line_ending_is_dropped_and_no_more() {
        assert_eq!(strip_one_line_ending("hello\n"), "hello");
        assert_eq!(strip_one_line_ending("hello\r\n"), "hello");
        assert_eq!(strip_one_line_ending("hello\n\n"), "hello\n");
        assert_eq!(strip_one_line_ending("hello "), "hello ");
        assert_eq!(strip_one_line_ending(""), "");
    }

    #[test]
    fn the_first_present_name_is_the_hook() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(find_hook(dir.path()), None);
        let last = dir.path().join(HOOK_NAMES[HOOK_NAMES.len() - 1]);
        std::fs::write(&last, "").unwrap();
        assert_eq!(find_hook(dir.path()), Some(last));
        if HOOK_NAMES.len() > 1 {
            let first = dir.path().join(HOOK_NAMES[0]);
            std::fs::write(&first, "").unwrap();
            assert_eq!(find_hook(dir.path()), Some(first));
        }
        // A directory under the name is not a hook.
        let other = tempfile::tempdir().unwrap();
        std::fs::create_dir(other.path().join(HOOK_NAMES[0])).unwrap();
        assert_eq!(find_hook(other.path()), None);
    }

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    /// Accented text through each kind of hook: the pipes must stay UTF-8.
    const SAMPLE: &str = "Ça marche — déjà vu, 東京";

    #[cfg(windows)]
    #[test]
    fn a_powershell_hook_rewrites_the_text_in_utf8() {
        let dir = tempfile::tempdir().unwrap();
        let hook = dir.path().join("transcription.ps1");
        std::fs::write(&hook, "$text = -join $input\n\"[$text]\"\n").unwrap();
        let out = runtime()
            .block_on(run(&hook, SAMPLE, Duration::from_secs(30)))
            .unwrap();
        assert_eq!(out, format!("[{SAMPLE}]"));
    }

    #[cfg(windows)]
    #[test]
    fn a_cmd_hook_passes_the_bytes_through() {
        let dir = tempfile::tempdir().unwrap();
        let hook = dir.path().join("transcription.cmd");
        std::fs::write(&hook, "@echo off\r\nfindstr \"^\"\r\n").unwrap();
        let out = runtime()
            .block_on(run(&hook, SAMPLE, Duration::from_secs(30)))
            .unwrap();
        assert_eq!(out, SAMPLE);
    }

    #[cfg(windows)]
    #[test]
    fn a_failing_hook_is_an_error_with_its_stderr() {
        let dir = tempfile::tempdir().unwrap();
        let hook = dir.path().join("transcription.cmd");
        std::fs::write(&hook, "@echo off\r\necho broken 1>&2\r\nexit /b 3\r\n").unwrap();
        let err = runtime()
            .block_on(run(&hook, SAMPLE, Duration::from_secs(30)))
            .unwrap_err()
            .to_string();
        assert!(err.contains("broken"), "{err}");
    }

    #[cfg(windows)]
    #[test]
    fn a_hung_hook_times_out() {
        let dir = tempfile::tempdir().unwrap();
        let hook = dir.path().join("transcription.ps1");
        std::fs::write(&hook, "Start-Sleep -Seconds 20\n").unwrap();
        let err = runtime()
            .block_on(run(&hook, SAMPLE, Duration::from_secs(3)))
            .unwrap_err()
            .to_string();
        assert!(err.contains("timed out"), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn a_shell_hook_rewrites_the_text() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let hook = dir.path().join("transcription");
        std::fs::write(&hook, "#!/bin/sh\nprintf '[%s]\\n' \"$(cat)\"\n").unwrap();
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
        let out = runtime()
            .block_on(run(&hook, SAMPLE, Duration::from_secs(30)))
            .unwrap();
        assert_eq!(out, format!("[{SAMPLE}]"));
    }
}
