//! Local TCP HTTP API server on 127.0.0.1:43117.
//! Provides health check, speak, brain, and command endpoints
//! for remote control and integration with external scripts.

use crate::brain::manager::BrainManager;
use crate::transcription_coordinator::TranscriptionCoordinator;
use crate::tts::backends::piper_server;
use crate::tts::manager::TtsManager;
use serde::Deserialize;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Manager};

const DEFAULT_ADDR: &str = "127.0.0.1:43117";
const MAX_BODY_BYTES: usize = 200_000;
const READ_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Deserialize)]
struct SpeakRequest {
    text: String,
}

#[derive(Debug, Deserialize)]
struct BrainRequest {
    text: String,
}

#[derive(Debug, Deserialize)]
struct CommandRequest {
    command: String,
}

enum ControlRequest {
    Health,
    PiperStatus,
    Speak(SpeakRequest),
    Brain(BrainRequest),
    Command(CommandRequest),
}

pub fn start(app: AppHandle) {
    let addr = std::env::var("S2B2S_CONTROL_ADDR").unwrap_or_else(|_| DEFAULT_ADDR.to_string());
    std::thread::spawn(move || {
        let listener = match TcpListener::bind(&addr) {
            Ok(listener) => listener,
            Err(error) => {
                log::warn!("[Control] Failed to bind {}: {}", addr, error);
                return;
            }
        };

        log::info!("[Control] Listening on http://{}", addr);
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => handle_connection(stream, app.clone()),
                Err(error) => log::warn!("[Control] Connection failed: {}", error),
            }
        }
    });
}

fn handle_connection(mut stream: TcpStream, app: AppHandle) {
    let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
    let _ = stream.set_write_timeout(Some(READ_TIMEOUT));

    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 4096];

    let read_result = loop {
        match stream.read(&mut chunk) {
            Ok(0) => break Ok(()),
            Ok(n) => {
                buffer.extend_from_slice(&chunk[..n]);
                match request_state(&buffer) {
                    RequestState::Incomplete => continue,
                    RequestState::Complete => break Ok(()),
                    RequestState::TooLarge => break Err((413, "request too large".to_string())),
                }
            }
            Err(error) => {
                log::warn!("[Control] Read failed: {}", error);
                return;
            }
        }
    };

    let response = match read_result.and_then(|()| parse_request(&buffer)) {
        Ok(ControlRequest::Health) => http_response(200, "OK", r#"{"ok":true,"app":"S2B2S"}"#),
        Ok(ControlRequest::PiperStatus) => {
            let status = piper_server::get_piper_server_status();
            let body = serde_json::to_string(&status)
                .unwrap_or_else(|_| r#"{"error":"serialization failed"}"#.to_string());
            http_response(200, "OK", &body)
        }
        Ok(ControlRequest::Speak(request)) => {
            if let Some(tts) = app
                .try_state::<Arc<TtsManager>>()
                .map(|s| s.inner().clone())
            {
                tts.speak(request.text);
                http_response(200, "OK", r#"{"ok":true}"#)
            } else {
                http_response(500, "Error", r#"{"error":"TtsManager not available"}"#)
            }
        }
        Ok(ControlRequest::Brain(request)) => {
            if let Some(brain) = app
                .try_state::<Arc<BrainManager>>()
                .map(|s| s.inner().clone())
            {
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = brain.ask(request.text).await {
                        log::error!("[Control] Brain ask failed: {}", e);
                    }
                });
                http_response(200, "OK", r#"{"ok":true}"#)
            } else {
                http_response(500, "Error", r#"{"error":"BrainManager not available"}"#)
            }
        }
        Ok(ControlRequest::Command(request)) => {
            if let Some(coordinator) = app
                .try_state::<Arc<TranscriptionCoordinator>>()
                .map(|s| s.inner().clone())
            {
                coordinator.send_input(&request.command, "API", true, false);
                http_response(200, "OK", r#"{"ok":true}"#)
            } else {
                http_response(
                    500,
                    "Error",
                    r#"{"error":"TranscriptionCoordinator not available"}"#,
                )
            }
        }
        Err((status, message)) => http_response(status, "Error", &json_error(&message)),
    };

    let _ = stream.write_all(response.as_bytes());
}

enum RequestState {
    Incomplete,
    Complete,
    TooLarge,
}

fn request_state(buffer: &[u8]) -> RequestState {
    let Some(header_end) = find_header_end(buffer) else {
        if buffer.len() > MAX_BODY_BYTES {
            return RequestState::TooLarge;
        }
        return RequestState::Incomplete;
    };
    let headers = String::from_utf8_lossy(&buffer[..header_end]);
    let content_length = content_length(&headers).unwrap_or(0);
    if content_length > MAX_BODY_BYTES {
        return RequestState::TooLarge;
    }
    if buffer.len() >= header_end + 4 + content_length {
        RequestState::Complete
    } else {
        RequestState::Incomplete
    }
}

fn json_error(message: &str) -> String {
    let value = serde_json::json!({ "error": message });
    value.to_string()
}

fn content_length(headers: &str) -> Option<usize> {
    headers
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse::<usize>().ok())
}

fn parse_request(buffer: &[u8]) -> Result<ControlRequest, (u16, String)> {
    let header_end = find_header_end(buffer).ok_or((400, "missing HTTP headers".to_string()))?;
    let headers = String::from_utf8_lossy(&buffer[..header_end]);

    let mut lines = headers.lines();
    let request_line = lines.next().unwrap_or_default();

    if request_line.starts_with("GET /health ") {
        return Ok(ControlRequest::Health);
    }
    if request_line.starts_with("GET /piper-status ") {
        return Ok(ControlRequest::PiperStatus);
    }

    if request_line.starts_with("POST /speak ") {
        let body = get_body(buffer, header_end, &headers)?;
        let request: SpeakRequest = serde_json::from_slice(&body)
            .map_err(|error| (400, format!("invalid JSON: {}", error)))?;
        if request.text.trim().is_empty() {
            return Err((400, "text is required".to_string()));
        }
        return Ok(ControlRequest::Speak(request));
    }

    if request_line.starts_with("POST /brain ") {
        let body = get_body(buffer, header_end, &headers)?;
        let request: BrainRequest = serde_json::from_slice(&body)
            .map_err(|error| (400, format!("invalid JSON: {}", error)))?;
        if request.text.trim().is_empty() {
            return Err((400, "text is required".to_string()));
        }
        return Ok(ControlRequest::Brain(request));
    }

    if request_line.starts_with("POST /command ") {
        let body = get_body(buffer, header_end, &headers)?;
        let request: CommandRequest = serde_json::from_slice(&body)
            .map_err(|error| (400, format!("invalid JSON: {}", error)))?;
        if request.command.trim().is_empty() {
            return Err((400, "command is required".to_string()));
        }
        return Ok(ControlRequest::Command(request));
    }

    Err((
        404,
        "expected GET /health, GET /piper-status, POST /speak, POST /brain, or POST /command"
            .to_string(),
    ))
}

fn get_body(buffer: &[u8], header_end: usize, headers: &str) -> Result<Vec<u8>, (u16, String)> {
    let content_length = content_length(headers).unwrap_or(0);
    let body_start = header_end + 4;
    let body_end = body_start + content_length;
    if buffer.len() < body_end {
        return Err((400, "incomplete body".to_string()));
    }
    Ok(buffer[body_start..body_end].to_vec())
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn http_response(status: u16, reason: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        status,
        reason,
        body.len(),
        body
    )
}
