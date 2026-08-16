//! Shared resumable download transport for all model-hub collections.
//!
//! This is a decoupled generalization of the transport that powers STT
//! mirror downloads (`managers/model/download.rs`): resume via `Range`,
//! stall watchdog, server-misbehavior guards and optional SHA256
//! verification — exposed as a free function any manager can call,
//! with progress delivered through a callback (testable without Tauri).

use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use hf_hub::api::tokio::CancellationToken;
use log::info;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;
use std::time::{Duration, Instant};

/// Bound on connection setup.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// No body bytes for this long means the transfer is wedged, not slow:
/// error out (keeping the partial for resume) so retry logic can react.
pub const STALL_TIMEOUT: Duration = Duration::from_secs(60);

/// Progress snapshot handed to the caller's callback.
pub struct TransportProgress {
    pub downloaded: u64,
    pub total: u64,
    pub speed_mbps: f64,
}

/// Side-channel notifications, decoupled from Tauri.
pub enum TransportEvent {
    Progress(TransportProgress),
    VerifyStart,
    VerifyEnd,
}

/// Cancellation is an outcome, not a failure: the partial file is kept
/// for resume.
#[derive(Debug, PartialEq, Eq)]
pub enum TransportOutcome {
    Completed,
    Cancelled,
}

/// Start offset of a `Content-Range: bytes <start>-<end>/<total>` header.
fn content_range_start(value: &str) -> Option<u64> {
    let range = value.trim().strip_prefix("bytes")?.trim_start();
    range.split('-').next()?.trim().parse().ok()
}

fn compute_sha256(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect())
}

/// Fetch `url` into `partial_path`, resuming whatever partial is already
/// there, and leave verified bytes in `partial_path` on success — renaming
////moving to the final destination is the caller's job.
///
/// Robustness contract (mirrors `managers/model/download.rs`):
/// - resumes via `Range`, restarts clean on a 200-to-Range or misaligned 206
/// - every chunk races the cancel token and a stall timeout
/// - a server sending more than the advertised total is cut off and the
///   partial deleted
/// - optional SHA256 verification; on mismatch the partial is deleted
pub async fn download_file_resumable(
    label: &str,
    url: &str,
    partial_path: &Path,
    expected_sha256: Option<&str>,
    cancel_token: &CancellationToken,
    emit: &(dyn Fn(TransportEvent) + Send + Sync),
) -> Result<TransportOutcome> {
    let mut resume_from = partial_path.metadata().map(|m| m.len()).unwrap_or(0);

    if resume_from > 0 {
        info!("[HubTransport] Resuming {label} from byte {resume_from}");
    } else {
        info!("[HubTransport] Starting {label} from {url}");
    }

    let client = reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .build()?;
    let mut request = client.get(url);
    if resume_from > 0 {
        request = request.header("Range", format!("bytes={resume_from}-"));
    }
    let response = tokio::select! {
        r = tokio::time::timeout(STALL_TIMEOUT, request.send()) => r
            .map_err(|_| anyhow!("no response within {}s from {url}", STALL_TIMEOUT.as_secs()))??,
        _ = cancel_token.cancelled() => return Ok(TransportOutcome::Cancelled),
    };

    // A 200 to a Range request means the server ignored it and is sending
    // the whole file; appending would corrupt the result — restart clean.
    if resume_from > 0 && response.status() == reqwest::StatusCode::OK {
        let _ = fs::remove_file(partial_path);
        resume_from = 0;
    }
    if !response.status().is_success() {
        return Err(anyhow!("server returned HTTP {}", response.status()));
    }
    // A 206 must start exactly at our partial's end.
    if resume_from > 0 && response.status() == reqwest::StatusCode::PARTIAL_CONTENT {
        let starts_at = response
            .headers()
            .get(reqwest::header::CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            .and_then(content_range_start);
        if starts_at != Some(resume_from) {
            let _ = fs::remove_file(partial_path);
            return Err(anyhow!(
                "server returned Content-Range starting at {starts_at:?}, expected {resume_from}"
            ));
        }
    }

    let content_length = response.content_length();
    let known_total = content_length.map(|l| resume_from + l);
    let total_size = known_total.unwrap_or(0);
    let mut downloaded = resume_from;
    let mut file = if resume_from > 0 {
        std::fs::OpenOptions::new()
            .append(true)
            .open(partial_path)?
    } else {
        File::create(partial_path)?
    };

    let start_time = Instant::now();
    let mut last_emit = Instant::now();
    let emit_progress = |downloaded: u64| {
        let elapsed = start_time.elapsed().as_secs_f64();
        let speed_mbps = if elapsed > 0.0 {
            (downloaded as f64 / 1024.0 / 1024.0) / elapsed
        } else {
            0.0
        };
        emit(TransportEvent::Progress(TransportProgress {
            downloaded,
            total: total_size,
            speed_mbps,
        }));
    };
    emit_progress(downloaded);

    let mut stream = response.bytes_stream();
    loop {
        let chunk = tokio::select! {
            c = tokio::time::timeout(STALL_TIMEOUT, stream.next()) => match c {
                Err(_) => return Err(anyhow!(
                    "transfer stalled: no data for {}s",
                    STALL_TIMEOUT.as_secs()
                )),
                Ok(None) => break,
                Ok(Some(chunk)) => chunk?,
            },
            _ = cancel_token.cancelled() => {
                // Keep the partial for resume; caller handles state cleanup.
                return Ok(TransportOutcome::Cancelled);
            }
        };
        // Don't let a misbehaving server fill the disk.
        if let Some(cap) = known_total {
            if downloaded + chunk.len() as u64 > cap {
                drop(file);
                let _ = fs::remove_file(partial_path);
                return Err(anyhow!("server sent more than the expected {cap} bytes"));
            }
        }
        file.write_all(&chunk)?;
        downloaded += chunk.len() as u64;
        if last_emit.elapsed() >= Duration::from_millis(200) {
            emit_progress(downloaded);
            last_emit = Instant::now();
        }
    }
    file.flush()?;
    drop(file);
    emit_progress(downloaded);

    if let Some(expected) = known_total {
        let actual = partial_path.metadata()?.len();
        if actual != expected {
            let _ = fs::remove_file(partial_path);
            return Err(anyhow!(
                "download incomplete: expected {expected} bytes, got {actual}"
            ));
        }
    }

    // Optional integrity anchor: catalogs with sha256 pins verify here.
    if let Some(expected) = expected_sha256 {
        emit(TransportEvent::VerifyStart);
        let path = partial_path.to_path_buf();
        let actual = tokio::task::spawn_blocking(move || compute_sha256(&path))
            .await
            .map_err(|e| anyhow!("SHA256 task panicked: {e}"))??;
        if actual != expected {
            let _ = fs::remove_file(partial_path);
            return Err(anyhow!(
                "SHA256 mismatch for {label}: expected {expected}, got {actual}"
            ));
        }
        emit(TransportEvent::VerifyEnd);
    }

    Ok(TransportOutcome::Completed)
}
