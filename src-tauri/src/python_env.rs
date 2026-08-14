//! Python Environment Manager for S2B2S.
//!
//! Provides cross-platform utilities for:
//! - Detecting and installing `uv` (the Python package manager)
//! - Creating and inspecting the shared S2B2S venv (Python 3.12)
//! - Installing / checking individual TTS/STT backend packages
//! - Streaming progress lines back to the frontend via Tauri events

use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::PathBuf;
use std::process::Command;
use tauri::Emitter;

#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

// ---------------------------------------------------------------------------
// Public types (specta-exported so TS bindings are generated)
// ---------------------------------------------------------------------------

/// Overall status of the Python environment.
#[derive(Serialize, Deserialize, Debug, Clone, Type, tauri_specta::Event)]
pub struct PythonEnvStatus {
    /// `uv` version string, e.g. "uv 0.7.12", or `None` if not installed.
    pub uv_version: Option<String>,
    /// Python version inside the venv, e.g. "3.12.8", or `None`.
    pub python_version: Option<String>,
    /// Absolute path to the venv directory.
    pub venv_path: String,
    /// Whether the venv directory exists.
    pub venv_exists: bool,
    /// Per-backend installation status.
    pub backends: Vec<BackendStatus>,
}

/// Status of one TTS / STT backend.
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct BackendStatus {
    /// Short identifier, e.g. "piper", "kokoro", "kitten".
    pub id: String,
    /// Human-readable label.
    pub label: String,
    /// Whether the required Python packages are importable.
    pub installed: bool,
    /// Category for grouping in the UI.
    pub category: BackendCategory,
}

#[derive(Serialize, Deserialize, Debug, Clone, Type, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BackendCategory {
    Tts,
    Stt,
}

/// Payload emitted on the `python-env-progress` event.
#[derive(Serialize, Deserialize, Clone, Debug, Type, tauri_specta::Event)]
pub struct EnvProgressEvent {
    /// Short identifier of the operation context (e.g. "uv", "kokoro", "venv").
    pub context: String,
    /// The log line text.
    pub line: String,
    /// Severity: "info", "warn", "error".
    pub level: String,
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Build a `Command` with no console window on Windows.
fn make_cmd(program: &str) -> Command {
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

/// Emit a progress line to the frontend.
pub fn emit_progress(app: &tauri::AppHandle, context: &str, line: &str, level: &str) {
    let _ = app.emit(
        "python-env-progress",
        EnvProgressEvent {
            context: context.to_string(),
            line: line.to_string(),
            level: level.to_string(),
        },
    );
}

/// Run a command and stream its stdout+stderr lines via `emit_progress`.
/// Returns `Ok(())` if the exit code is 0, `Err(last_error_line)` otherwise.
fn run_streaming(app: &tauri::AppHandle, context: &str, mut cmd: Command) -> Result<(), String> {
    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| format!("spawn error: {e}"))?;

    // Drain stdout
    let stdout_handle = {
        let app2 = app.clone();
        let ctx = context.to_string();
        if let Some(stdout) = child.stdout.take() {
            Some(std::thread::spawn(move || {
                use std::io::BufRead;
                for line in std::io::BufReader::new(stdout).lines().flatten() {
                    log::debug!("[python_env/{ctx}] {line}");
                    emit_progress(&app2, &ctx, &line, "info");
                }
            }))
        } else {
            None
        }
    };

    // Drain stderr
    let mut last_err = String::new();
    {
        let app2 = app.clone();
        let ctx = context.to_string();
        if let Some(stderr) = child.stderr.take() {
            use std::io::BufRead;
            for line in std::io::BufReader::new(stderr).lines().flatten() {
                log::debug!("[python_env/{ctx}] STDERR: {line}");
                let level = if line.to_ascii_lowercase().contains("error") {
                    "error"
                } else if line.to_ascii_lowercase().contains("warn") {
                    "warn"
                } else {
                    "info"
                };
                emit_progress(&app2, &ctx, &line, level);
                last_err = line;
            }
        }
    }

    if let Some(h) = stdout_handle {
        let _ = h.join();
    }

    let status = child.wait().map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(last_err)
    }
}

// ---------------------------------------------------------------------------
// uv detection & installation
// ---------------------------------------------------------------------------

/// Resolve the `uv` executable path, checking common install locations.
pub fn find_uv() -> Option<String> {
    // 1. Already on PATH
    if which("uv").is_some() {
        return Some("uv".to_string());
    }

    // 2. Common user-local install locations
    let candidates = uv_candidate_paths();
    for p in &candidates {
        if p.exists() {
            return Some(p.to_string_lossy().to_string());
        }
    }

    None
}

fn uv_candidate_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    #[cfg(windows)]
    {
        if let Ok(userprofile) = std::env::var("USERPROFILE") {
            let base = PathBuf::from(&userprofile);
            paths.push(base.join(".cargo").join("bin").join("uv.exe"));
            paths.push(
                base.join("AppData")
                    .join("Roaming")
                    .join("uv")
                    .join("bin")
                    .join("uv.exe"),
            );
            paths.push(base.join(".local").join("bin").join("uv.exe"));
        }
        if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
            paths.push(
                PathBuf::from(&local_app_data)
                    .join("uv")
                    .join("bin")
                    .join("uv.exe"),
            );
        }
    }

    #[cfg(not(windows))]
    {
        if let Ok(home) = std::env::var("HOME") {
            let base = PathBuf::from(&home);
            paths.push(base.join(".cargo").join("bin").join("uv"));
            paths.push(base.join(".local").join("bin").join("uv"));
        }
        paths.push(PathBuf::from("/usr/local/bin/uv"));
        paths.push(PathBuf::from("/opt/homebrew/bin/uv"));
    }

    paths
}

/// Get the version string of the installed `uv`, or `None`.
pub fn uv_version(uv: &str) -> Option<String> {
    let output = make_cmd(uv).arg("--version").output().ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

/// Install `uv` using the official installer script (cross-platform).
/// Streams progress to the frontend. Returns the path to `uv` on success.
pub fn install_uv(app: &tauri::AppHandle) -> Result<String, String> {
    emit_progress(app, "uv", "Installing uv package manager…", "info");

    #[cfg(windows)]
    {
        // Windows: PowerShell one-liner installer
        let mut cmd = make_cmd("powershell");
        cmd.args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            "irm https://astral.sh/uv/install.ps1 | iex",
        ]);
        run_streaming(app, "uv", cmd)?;
    }

    #[cfg(not(windows))]
    {
        // macOS / Linux: curl | sh
        let mut cmd = make_cmd("sh");
        cmd.args(["-c", "curl -LsSf https://astral.sh/uv/install.sh | sh"]);
        run_streaming(app, "uv", cmd)?;
    }

    // Locate the freshly installed uv
    let uv_path = find_uv().ok_or_else(|| {
        "uv installed but still not found on PATH. Restart the app and try again.".to_string()
    })?;

    let version = uv_version(&uv_path).unwrap_or_else(|| "unknown".to_string());
    emit_progress(app, "uv", &format!("✅ uv installed: {version}"), "info");

    Ok(uv_path)
}

// ---------------------------------------------------------------------------
// Venv management
// ---------------------------------------------------------------------------

/// Resolve the canonical venv directory path.
pub fn venv_dir() -> PathBuf {
    crate::portable::get_venv_dir()
}

/// Whether the venv looks like a *working* venv (has the `pyvenv.cfg` marker).
/// A leftover hollow directory (only Scripts/, no Lib/, no cfg — e.g. after an
/// interrupted recreation) reports false so callers recreate it safely.
pub fn venv_is_valid() -> bool {
    let venv = venv_dir();
    venv.join("pyvenv.cfg").exists()
}

/// Python executable inside the venv (cross-platform).
pub fn venv_python() -> PathBuf {
    let venv = venv_dir();
    if cfg!(windows) {
        venv.join("Scripts").join("python.exe")
    } else {
        venv.join("bin").join("python3")
    }
}

/// Get the Python version inside the venv, e.g. `"3.12.8"`.
pub fn python_version_in_venv() -> Option<String> {
    let python = venv_python();
    if !python.exists() {
        return None;
    }
    let output = make_cmd(python.to_str().unwrap_or("python"))
        .args(["--version"])
        .output()
        .ok()?;

    // Python prints "Python 3.12.8" to stdout (or stderr on older versions)
    let combined = format!(
        "{} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    for part in combined.split_whitespace() {
        if part.contains('.') && part.chars().next().map_or(false, |c| c.is_ascii_digit()) {
            return Some(part.to_string());
        }
    }
    None
}

/// Create (or recreate) the venv using `uv venv --python 3.12`.
///
/// Safety: an existing venv is never deleted in place. It is renamed aside
/// (`venv.old-<timestamp>`), the fresh venv is created, and the backup is only
/// removed once creation succeeded — a failed run restores the old venv so
/// installed packages can never be lost.
pub fn create_venv(app: &tauri::AppHandle, uv: &str) -> Result<(), String> {
    let venv = venv_dir();
    emit_progress(
        app,
        "venv",
        &format!("Creating venv at {}…", venv.display()),
        "info",
    );

    // Move an existing venv aside instead of deleting it.
    let backup: Option<std::path::PathBuf> = if venv.exists() {
        let backup = venv.with_extension(format!(
            "old-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
        ));
        emit_progress(
            app,
            "venv",
            &format!("Backing up existing venv to {}…", backup.display()),
            "info",
        );
        std::fs::rename(&venv, &backup)
            .map_err(|e| format!("Failed to move old venv aside: {e}"))?;
        Some(backup)
    } else {
        None
    };

    let mut cmd = make_cmd(uv);
    cmd.args([
        "venv",
        "--python",
        "3.12",
        "--allow-existing",
        venv.to_str().unwrap_or("venv"),
    ]);

    let result = run_streaming(app, "venv", cmd);

    match result {
        Ok(()) => {
            if let Some(ref backup) = backup {
                // Fresh venv is live — drop the backup.
                let _ = std::fs::remove_dir_all(backup);
                emit_progress(app, "venv", "Removed previous venv backup", "info");
            }
            emit_progress(app, "venv", "✅ venv created with Python 3.12", "info");
            Ok(())
        }
        Err(e) => {
            // Creation failed — restore the previous venv so packages survive.
            if let Some(ref backup) = backup {
                if backup.exists() && !venv.exists() {
                    let _ = std::fs::rename(backup, &venv);
                    emit_progress(app, "venv", "Restored previous venv after failure", "warn");
                }
            }
            Err(e)
        }
    }
}

// ---------------------------------------------------------------------------
// Backend package definitions
// ---------------------------------------------------------------------------

/// All backends that S2B2S manages.
pub fn all_backends() -> Vec<&'static str> {
    vec![
        "piper",
        "kokoro",
        "kitten",
        "pocket",
        "qwen3",
        "sherpa_onnx",
    ]
}

fn backend_label(id: &str) -> &'static str {
    match id {
        "piper" => "Piper TTS",
        "kokoro" => "Kokoro TTS",
        "kitten" => "Kitten TTS",
        "pocket" => "Pocket TTS",
        "qwen3" => "Qwen3 TTS",
        "sherpa_onnx" => "Sherpa-ONNX (STT)",
        _ => "Unknown",
    }
}

fn backend_category(id: &str) -> BackendCategory {
    match id {
        "sherpa_onnx" => BackendCategory::Stt,
        _ => BackendCategory::Tts,
    }
}

/// Python import to check for each backend.
fn backend_import_check(id: &str) -> &'static str {
    match id {
        "piper" => "piper",
        "kokoro" => "kokoro_tts",
        "kitten" => "kittentts",
        "pocket" => "pocket_tts",
        "qwen3" => "faster_qwen3_tts",
        "sherpa_onnx" => "sherpa_onnx",
        _ => "",
    }
}

/// Check whether a backend's packages are importable in the venv.
pub fn is_backend_installed(id: &str) -> bool {
    let import = backend_import_check(id);
    if import.is_empty() {
        return false;
    }
    let python = venv_python();
    if !python.exists() {
        return false;
    }
    let mut cmd = make_cmd(python.to_str().unwrap_or("python"));
    cmd.args(["-c", &format!("import {import}")]);
    cmd.stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    cmd.status().map(|s| s.success()).unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Package installation sequences (mirrors setup_venv_uv.ps1 + CopySpeak)
// ---------------------------------------------------------------------------

fn uv_pip_install(
    app: &tauri::AppHandle,
    context: &str,
    uv: &str,
    packages: &[&str],
) -> Result<(), String> {
    let python = venv_python();
    let python_str = python.to_str().unwrap_or("python");

    let mut cmd = make_cmd(uv);
    cmd.arg("pip")
        .arg("install")
        .arg("--python")
        .arg(python_str)
        .arg("--no-cache");

    for pkg in packages {
        cmd.arg(pkg);
    }

    run_streaming(app, context, cmd)
}

fn uv_pip_uninstall(uv: &str, package: &str) {
    let python = venv_python();
    let python_str = python.to_str().unwrap_or("python");
    let _ = make_cmd(uv)
        .args(["pip", "uninstall", "--python", python_str, package, "--yes"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

/// Install the GPU (CUDA 13) runtime libraries into the venv.
fn install_cuda_runtime(app: &tauri::AppHandle, uv: &str) -> Result<(), String> {
    emit_progress(app, "cuda", "Removing CPU-only onnxruntime…", "info");
    uv_pip_uninstall(uv, "onnxruntime");

    // Purge leftover directories (mirrors CopySpeak's setup-venv-cuda-v2.ps1)
    let venv = venv_dir();
    let site_pkgs = if cfg!(windows) {
        venv.join("Lib").join("site-packages")
    } else {
        // Approximate — actual path depends on Python minor version
        venv.join("lib")
    };
    for name in &["onnxruntime", "onnxruntime_gpu"] {
        let p = site_pkgs.join(name);
        if p.exists() {
            let _ = std::fs::remove_dir_all(&p);
        }
    }

    emit_progress(app, "cuda", "Installing onnxruntime-gpu…", "info");
    uv_pip_install(app, "cuda", uv, &["--force-reinstall", "onnxruntime-gpu"])?;

    emit_progress(
        app,
        "cuda",
        "Installing NVIDIA CUDA 13 runtime DLL packages…",
        "info",
    );
    let cuda_pkgs = [
        "nvidia-cuda-runtime",
        "nvidia-cudnn-cu13",
        "nvidia-cublas",
        "nvidia-cufft",
        "nvidia-cusolver",
        "nvidia-cusparse",
        "nvidia-nvjitlink",
    ];
    for pkg in &cuda_pkgs {
        uv_pip_install(app, "cuda", uv, &[pkg])?;
    }

    emit_progress(app, "cuda", "✅ CUDA 13 runtime packages installed", "info");
    Ok(())
}

/// Install all packages required by a single backend.
pub fn install_backend(
    app: &tauri::AppHandle,
    backend_id: &str,
    uv: &str,
    gpu: bool,
) -> Result<(), String> {
    emit_progress(
        app,
        backend_id,
        &format!(
            "Installing {} ({})…",
            backend_label(backend_id),
            if gpu { "GPU" } else { "CPU" }
        ),
        "info",
    );

    match backend_id {
        "piper" => {
            uv_pip_install(app, "piper", uv, &["piper-tts[http]"])?;
            uv_pip_install(
                app,
                "piper",
                uv,
                &[
                    "coloredlogs",
                    "flatbuffers",
                    "packaging",
                    "protobuf",
                    "sympy",
                    "sentencepiece",
                ],
            )?;

            if gpu {
                install_cuda_runtime(app, uv)?;
            } else {
                // Ensure CPU onnxruntime is present (piper pulls it but let's be explicit)
                uv_pip_install(app, "piper", uv, &["onnxruntime"])?;
            }
        }

        "kokoro" => {
            uv_pip_install(app, "kokoro", uv, &["kokoro-tts", "soundfile", "numpy"])?;
            if gpu {
                install_cuda_runtime(app, uv)?;
            } else {
                uv_pip_install(app, "kokoro", uv, &["onnxruntime"])?;
            }
        }

        "kitten" => {
            uv_pip_install(
                app,
                "kitten",
                uv,
                &[
                    "https://github.com/KittenML/KittenTTS/releases/download/0.8.1/kittentts-0.8.1-py3-none-any.whl",
                    "soundfile",
                    "numpy",
                ],
            )?;
            if gpu {
                install_cuda_runtime(app, uv)?;
            } else {
                uv_pip_install(app, "kitten", uv, &["onnxruntime"])?;
            }
        }

        "pocket" => {
            uv_pip_install(app, "pocket", uv, &["pocket-tts", "soundfile", "numpy"])?;
            if gpu {
                install_cuda_runtime(app, uv)?;
            } else {
                uv_pip_install(app, "pocket", uv, &["onnxruntime"])?;
            }
        }

        "qwen3" => {
            // Install torch (CUDA nightly or CPU wheel)
            if gpu {
                uv_pip_install(
                    app,
                    "qwen3",
                    uv,
                    &[
                        "--pre",
                        "torch",
                        "torchvision",
                        "torchaudio",
                        "--index-url",
                        "https://download.pytorch.org/whl/nightly/cu132",
                    ],
                )?;
            } else {
                uv_pip_install(
                    app,
                    "qwen3",
                    uv,
                    &[
                        "torch",
                        "torchvision",
                        "torchaudio",
                        "--index-url",
                        "https://download.pytorch.org/whl/cpu",
                    ],
                )?;
            }
            uv_pip_install(
                app,
                "qwen3",
                uv,
                &[
                    "transformers>=4.57,<5",
                    "huggingface-hub>=0.36.0,<1.0",
                    "accelerate",
                    "soundfile",
                    "numpy>=2.4.0,<2.5.0",
                ],
            )?;
            // qwen-tts (no deps) + faster-qwen3-tts
            uv_pip_install(app, "qwen3", uv, &["qwen-tts", "--no-deps"])?;
            uv_pip_install(
                app,
                "qwen3",
                uv,
                &[
                    "--no-deps",
                    "git+https://github.com/andimarafioti/faster-qwen3-tts.git",
                ],
            )?;
        }

        "sherpa_onnx" => {
            uv_pip_install(app, "sherpa_onnx", uv, &["sherpa-onnx"])?;
        }

        other => {
            return Err(format!("Unknown backend: {other}"));
        }
    }

    emit_progress(
        app,
        backend_id,
        &format!("✅ {} installed", backend_label(backend_id)),
        "info",
    );
    Ok(())
}

/// One pip step for the "Install All" flow: mirrors `setup_venv_uv.ps1`'s
/// Install-Pkg helper (force-reinstall, cached disabled, targeted at the venv).
fn uv_pip_install_step(
    app: &tauri::AppHandle,
    uv: &str,
    label: &str,
    packages: &[&str],
) -> Result<(), String> {
    emit_progress(app, "all", &format!("→ {label}"), "info");
    let python = venv_python();
    let python_str = python.to_str().unwrap_or("python");

    let mut cmd = make_cmd(uv);
    cmd.arg("pip")
        .arg("install")
        .arg("--python")
        .arg(python_str)
        .arg("--no-cache")
        .arg("--force-reinstall");
    for pkg in packages {
        cmd.arg(pkg);
    }
    run_streaming(app, "all", cmd)
}

/// Install all backends plus common deps in one go — the default "Install All"
/// action. Mirrors `scripts/setup_venv_uv.ps1` exactly:
///   1. STT/TTS backends in canonical order (kokoro → pocket → kitten → sherpa)
///   2. torch (CUDA 13.2 nightly or CPU wheel)
///   3. audio/ML base deps (soundfile, numpy, sox, librosa, transformers…)
///   4. qwen-tts + faster-qwen3-tts (no deps)
///   5. piper-tts LAST (it pulls CPU onnxruntime)
///   6. CUDA runtime at the very end (GPU mode) or explicit CPU onnxruntime
pub fn install_all_backends(app: &tauri::AppHandle, uv: &str, gpu: bool) -> Result<(), String> {
    emit_progress(
        app,
        "all",
        &format!(
            "Installing all backends in one go ({} mode) — CUDA at the end…",
            if gpu { "GPU" } else { "CPU" }
        ),
        "info",
    );

    // 1) Backends in the canonical order
    uv_pip_install_step(app, uv, "kokoro-tts", &["kokoro-tts"])?;
    uv_pip_install_step(app, uv, "pocket-tts", &["pocket-tts"])?;
    uv_pip_install_step(
        app,
        uv,
        "kittentts (wheel)",
        &["https://github.com/KittenML/KittenTTS/releases/download/0.8.1/kittentts-0.8.1-py3-none-any.whl"],
    )?;
    uv_pip_install_step(app, uv, "sherpa-onnx", &["sherpa-onnx"])?;

    // 2) torch — CUDA nightly or CPU wheel (one step, before piper)
    if gpu {
        uv_pip_install_step(
            app,
            uv,
            "torch (CUDA 13.2 Nightly)",
            &[
                "--pre",
                "torch",
                "torchvision",
                "torchaudio",
                "--index-url",
                "https://download.pytorch.org/whl/nightly/cu132",
            ],
        )?;
    } else {
        uv_pip_install_step(
            app,
            uv,
            "torch (CPU)",
            &[
                "torch",
                "torchvision",
                "torchaudio",
                "--index-url",
                "https://download.pytorch.org/whl/cpu",
            ],
        )?;
    }

    // 3) Audio & ML base deps
    uv_pip_install_step(
        app,
        uv,
        "soundfile, numpy 2.4.x",
        &["soundfile", "numpy>=2.4.0,<2.5.0"],
    )?;
    uv_pip_install_step(app, uv, "sox, soxr", &["sox", "soxr"])?;
    uv_pip_install_step(
        app,
        uv,
        "librosa, numba",
        &["librosa>=0.10.0", "numba>=0.59.0"],
    )?;
    uv_pip_install_step(
        app,
        uv,
        "transformers, hub, accelerate",
        &[
            "transformers>=4.57,<5",
            "huggingface-hub>=0.36.0,<1.0",
            "accelerate",
        ],
    )?;

    // 4) qwen-tts + faster-qwen3-tts (no deps)
    uv_pip_install_step(app, uv, "qwen-tts (no deps)", &["--no-deps", "qwen-tts"])?;
    uv_pip_install_step(
        app,
        uv,
        "faster-qwen3-tts (no deps)",
        &[
            "--no-deps",
            "git+https://github.com/andimarafioti/faster-qwen3-tts.git",
        ],
    )?;

    // 5) piper LAST (pulls CPU onnxruntime), then build/runtime deps
    uv_pip_install_step(app, uv, "piper-tts[http]", &["piper-tts[http]"])?;
    uv_pip_install_step(
        app,
        uv,
        "build/runtime deps",
        &[
            "coloredlogs",
            "flatbuffers",
            "packaging",
            "protobuf",
            "sympy",
        ],
    )?;
    uv_pip_install_step(app, uv, "sentencepiece", &["sentencepiece"])?;

    // 6) CUDA at the very end (GPU mode) — swap CPU onnxruntime for GPU + NVIDIA DLLs
    if gpu {
        install_cuda_runtime(app, uv)?;
        emit_progress(app, "all", "Final purge of CPU onnxruntime…", "info");
        uv_pip_uninstall(uv, "onnxruntime");
    } else {
        uv_pip_install_step(app, uv, "onnxruntime (CPU)", &["onnxruntime"])?;
    }

    emit_progress(app, "all", "✅ All backends installed!", "info");
    Ok(())
}

// ---------------------------------------------------------------------------
// Full status snapshot
// ---------------------------------------------------------------------------

/// Build the current `PythonEnvStatus` snapshot (non-blocking).
pub fn get_env_status() -> PythonEnvStatus {
    let uv = find_uv();
    let uv_version = uv.as_deref().and_then(|u| uv_version(u));
    let venv = venv_dir();
    let venv_exists = venv.exists();
    let python_version = if venv_exists {
        python_version_in_venv()
    } else {
        None
    };

    let backends = all_backends()
        .iter()
        .map(|&id| BackendStatus {
            id: id.to_string(),
            label: backend_label(id).to_string(),
            installed: if venv_exists {
                is_backend_installed(id)
            } else {
                false
            },
            category: backend_category(id),
        })
        .collect();

    PythonEnvStatus {
        uv_version,
        python_version,
        venv_path: venv.to_string_lossy().to_string(),
        venv_exists,
        backends,
    }
}

// ---------------------------------------------------------------------------
// Utilities
// ---------------------------------------------------------------------------

/// Check if a program is on the system PATH.
fn which(program: &str) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let output = make_cmd("where.exe").arg(program).output().ok()?;
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            if !path.is_empty() {
                return Some(PathBuf::from(path));
            }
        }
        None
    }
    #[cfg(not(windows))]
    {
        let output = std::process::Command::new("which")
            .arg(program)
            .output()
            .ok()?;
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Some(PathBuf::from(path));
            }
        }
        None
    }
}
