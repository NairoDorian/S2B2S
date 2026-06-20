// Adapted from AivoRelay (MaxITService/AIVORelay), MIT License.
// Source: src-tauri/src/temp_artifacts.rs — Sandboxing & Temp Cleanup (2026-06-19).

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const ARTIFACT_TTL: Duration = Duration::from_secs(60 * 60 * 24); // 24 hours

/// Validates that a requested file path is safe and contained entirely within a base directory.
/// This prevents path traversal attacks (e.g. "../../../Windows/System32/...")
pub fn validate_artifact_path(requested: &Path, base_dir: &Path) -> Result<PathBuf, String> {
    let canonical_dir = base_dir
        .canonicalize()
        .map_err(|e| format!("Base directory canonicalization failed: {}", e))?;

    let parent = requested
        .parent()
        .ok_or_else(|| "Requested path has no parent directory".to_string())?;

    let canonical_parent = parent
        .canonicalize()
        .map_err(|e| format!("Requested path parent canonicalization failed: {}", e))?;

    if !canonical_parent.starts_with(&canonical_dir) {
        return Err(
            "Path traversal detected: requested path is outside the allowed directory".to_string(),
        );
    }

    Ok(requested.to_path_buf())
}

/// Sweeps the directory for files older than 24 hours and deletes them.
pub fn cleanup_old_artifacts(dir: &Path) {
    let cutoff = SystemTime::now()
        .checked_sub(ARTIFACT_TTL)
        .unwrap_or(UNIX_EPOCH);

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Ok(metadata) = entry.metadata() {
                    if let Ok(modified) = metadata.modified() {
                        if modified < cutoff {
                            let _ = fs::remove_file(&path);
                        }
                    }
                }
            }
        }
    }
}
