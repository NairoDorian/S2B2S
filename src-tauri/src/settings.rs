use crate::utils;
use log::{debug, info, trace, warn};
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use specta::Type;
use std::collections::HashMap;
use std::fmt;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

pub const APPLE_INTELLIGENCE_PROVIDER_ID: &str = "apple_intelligence";

/// User-facing latency preset for native streaming models (Parakeet Buffered,
/// Nemotron cache-aware). Stored per-model-id in
/// [`AppSettings::native_streaming_latency_presets`]. `Accurate` is the default
/// (runtime default â€” no stream extension attached), so it is the unit value.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum NativeStreamingLatencyPreset {
    Fastest,
    Fast,
    Balanced,
    #[default]
    Accurate,
}

pub const APPLE_INTELLIGENCE_DEFAULT_MODEL_ID: &str = "Apple Intelligence";

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

// Custom deserializer to handle both old numeric format (1-5) and new string format ("trace", "debug", etc.)
impl<'de> Deserialize<'de> for LogLevel {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct LogLevelVisitor;

        impl<'de> Visitor<'de> for LogLevelVisitor {
            type Value = LogLevel;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a string or integer representing log level")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<LogLevel, E> {
                match value.to_lowercase().as_str() {
                    "trace" => Ok(LogLevel::Trace),
                    "debug" => Ok(LogLevel::Debug),
                    "info" => Ok(LogLevel::Info),
                    "warn" => Ok(LogLevel::Warn),
                    "error" => Ok(LogLevel::Error),
                    _ => Err(E::unknown_variant(
                        value,
                        &["trace", "debug", "info", "warn", "error"],
                    )),
                }
            }

            fn visit_u64<E: de::Error>(self, value: u64) -> Result<LogLevel, E> {
                match value {
                    1 => Ok(LogLevel::Trace),
                    2 => Ok(LogLevel::Debug),
                    3 => Ok(LogLevel::Info),
                    4 => Ok(LogLevel::Warn),
                    5 => Ok(LogLevel::Error),
                    _ => Err(E::invalid_value(de::Unexpected::Unsigned(value), &"1-5")),
                }
            }
        }

        deserializer.deserialize_any(LogLevelVisitor)
    }
}

impl From<LogLevel> for tauri_plugin_log::LogLevel {
    fn from(level: LogLevel) -> Self {
        match level {
            LogLevel::Trace => tauri_plugin_log::LogLevel::Trace,
            LogLevel::Debug => tauri_plugin_log::LogLevel::Debug,
            LogLevel::Info => tauri_plugin_log::LogLevel::Info,
            LogLevel::Warn => tauri_plugin_log::LogLevel::Warn,
            LogLevel::Error => tauri_plugin_log::LogLevel::Error,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct ShortcutBinding {
    pub id: String,
    pub name: String,
    pub description: String,
    pub default_binding: String,
    pub current_binding: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct LLMPrompt {
    pub id: String,
    pub name: String,
    pub prompt: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct PostProcessProvider {
    pub id: String,
    pub label: String,
    pub base_url: String,
    #[serde(default)]
    pub allow_base_url_edit: bool,
    #[serde(default)]
    pub models_endpoint: Option<String>,
    #[serde(default)]
    pub supports_structured_output: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "lowercase")]
pub enum OverlayPosition {
    Top,
    // `none` is retired: overlay visibility is owned by `OverlayStyle` now. The
    // alias keeps legacy stores (`"overlay_position": "none"`) deserializing
    // instead of failing the whole load; the one-time overlay migration reads the
    // raw stored string to recover the old "hidden" intent as `OverlayStyle::None`.
    #[serde(alias = "none")]
    Bottom,
}

/// Which recording overlay to display. `Minimal` and `Live` share one base
/// (the pill); `Live` grows into the panel that shows live transcription text.
/// `None` hides the overlay entirely. Decoupled from whether the model runs in
/// streaming mode (that is driven purely by model capability).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "lowercase")]
pub enum OverlayStyle {
    None,
    Minimal,
    Live,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum ModelUnloadTimeout {
    Never,
    Immediately,
    Min2,
    #[default]
    Min5,
    Min10,
    Min15,
    Hour1,
    Sec15, // Debug mode only
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum PasteMethod {
    CtrlV,
    Direct,
    DirectStreaming,
    None,
    ShiftInsert,
    CtrlShiftV,
    ExternalScript,
}

/// How the transcribe shortcut's key events drive a recording.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum ShortcutActivation {
    /// Press to start, press again to stop.
    Toggle,
    /// Hold to record, release to stop.
    PushToTalk,
    /// Hold to record and release to stop, or tap to keep recording until the
    /// next press. Which one it was is decided by how long the key was held
    /// (`hold_threshold_ms`).
    #[default]
    HoldOrToggle,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum ClipboardHandling {
    #[default]
    DontModify,
    CopyToClipboard,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum AutoSubmitKey {
    #[default]
    Enter,
    CtrlEnter,
    CmdEnter,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum RecordingRetentionPeriod {
    Never,
    PreserveLimit,
    Days3,
    Weeks2,
    Months3,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum KeyboardImplementation {
    Tauri,
    /// The native backend (`shortcut::native_keys`, on the `handy-keys` crate).
    ///
    /// The serialized value stays `"handy_keys"` and must not follow a rename:
    /// it is already written into every user's `settings_store.json`, and a new
    /// spelling would read as "unknown variant" and drop the setting. The
    /// `rename` below is the frozen wire value, not a name.
    #[serde(rename = "handy_keys")]
    NativeKeys,
}

impl Default for KeyboardImplementation {
    fn default() -> Self {
        #[cfg(target_os = "linux")]
        return KeyboardImplementation::Tauri;
        #[cfg(not(target_os = "linux"))]
        return KeyboardImplementation::NativeKeys;
    }
}

impl Default for PasteMethod {
    fn default() -> Self {
        // Default to CtrlV for macOS and Windows, Direct for Linux
        #[cfg(target_os = "linux")]
        return PasteMethod::Direct;
        #[cfg(not(target_os = "linux"))]
        return PasteMethod::CtrlV;
    }
}

impl ModelUnloadTimeout {
    pub fn to_minutes(self) -> Option<u64> {
        match self {
            ModelUnloadTimeout::Never => None,
            ModelUnloadTimeout::Immediately => Some(0), // Special case for immediate unloading
            ModelUnloadTimeout::Min2 => Some(2),
            ModelUnloadTimeout::Min5 => Some(5),
            ModelUnloadTimeout::Min10 => Some(10),
            ModelUnloadTimeout::Min15 => Some(15),
            ModelUnloadTimeout::Hour1 => Some(60),
            ModelUnloadTimeout::Sec15 => Some(0), // Special case for debug - handled separately
        }
    }

    pub fn to_seconds(self) -> Option<u64> {
        match self {
            ModelUnloadTimeout::Never => None,
            ModelUnloadTimeout::Immediately => Some(0), // Special case for immediate unloading
            ModelUnloadTimeout::Sec15 => Some(15),
            _ => self.to_minutes().map(|m| m * 60),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum SoundTheme {
    Marimba,
    Pop,
    Custom,
}

impl SoundTheme {
    fn as_str(&self) -> &'static str {
        match self {
            SoundTheme::Marimba => "marimba",
            SoundTheme::Pop => "pop",
            SoundTheme::Custom => "custom",
        }
    }

    pub fn to_start_path(self) -> String {
        format!("resources/{}_start.wav", self.as_str())
    }

    pub fn to_stop_path(self) -> String {
        format!("resources/{}_stop.wav", self.as_str())
    }
}

/// UI appearance mode. `System` follows the OS `prefers-color-scheme`; `Light`
/// and `Dark` force one of the two palettes the app already ships.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    System,
    Light,
    Dark,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum TypingTool {
    #[default]
    Auto,
    Wtype,
    Kwtype,
    Dotool,
    Ydotool,
    Xdotool,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum TranscribeAcceleratorSetting {
    #[default]
    Auto,
    Cpu,
    Gpu,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum MicIdleTimeoutUnit {
    #[default]
    Seconds,
    Minutes,
}

/// Default speech-probability threshold of the Earshot VAD: 0.5 is the
/// neutral point of its 0â€“1 score. User-adjustable through
/// `vad_threshold_earshot` (see `VadSensitivity` in the Advanced page): lower
/// values keep more borderline audio, higher values drop more background noise.
pub const DEFAULT_VAD_THRESHOLD_EARSHOT: f32 = 0.5;
/// Hard bounds for a stored threshold. Below 0.05 the exit hysteresis floor
/// (0.01) makes everything speech; above 0.95 nothing ever is.
pub const MIN_VAD_THRESHOLD: f32 = 0.05;
pub const MAX_VAD_THRESHOLD: f32 = 0.95;

pub fn clamp_vad_threshold(threshold: f32) -> f32 {
    if threshold.is_finite() {
        threshold.clamp(MIN_VAD_THRESHOLD, MAX_VAD_THRESHOLD)
    } else {
        DEFAULT_VAD_THRESHOLD_EARSHOT
    }
}

/// How the "Transcribe Files" page turns one audio file into text.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum FileTranscriptionMode {
    /// Primary model only, no LLM.
    #[default]
    Simple,
    /// Primary model, then the selected post-processing prompt.
    PostProcess,
    /// Primary + configured extra models, merged with the Multi-STT prompt
    /// (concatenated when no merge prompt / provider is configured).
    MultiStt,
    /// Multi-STT merge, then the post-processing prompt on the merged text.
    MultiSttPostProcess,
}

/// Text file flavour written by the file-transcription and Live Mode pages.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptOutputFormat {
    #[default]
    Txt,
    Md,
}

impl TranscriptOutputFormat {
    pub fn extension(self) -> &'static str {
        match self {
            TranscriptOutputFormat::Txt => "txt",
            TranscriptOutputFormat::Md => "md",
        }
    }
}

/// Settings of the "Transcribe Files" page. Grouped into one struct so the
/// page persists through a single command instead of one per field.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type)]
#[serde(default)]
pub struct FileTranscriptionSettings {
    pub mode: FileTranscriptionMode,
    /// Folder the transcripts are written to. `None` writes each transcript
    /// next to its source audio file.
    pub output_dir: Option<String>,
    pub output_format: TranscriptOutputFormat,
    /// Replace an existing transcript instead of appending `-2`, `-3`, â€¦.
    pub overwrite_existing: bool,
    /// When a folder is added, also queue audio files from its sub-folders.
    pub include_subfolders: bool,
    /// Long recordings are decoded in segments of at most this many minutes,
    /// cut at the quietest point near the boundary, so one file never holds
    /// the engine (or memory) for an hour at a time. 1â€“60.
    pub max_segment_minutes: u32,
}

impl Default for FileTranscriptionSettings {
    fn default() -> Self {
        Self {
            mode: FileTranscriptionMode::Simple,
            output_dir: None,
            output_format: TranscriptOutputFormat::Txt,
            overwrite_existing: false,
            include_subfolders: true,
            max_segment_minutes: 10,
        }
    }
}

impl FileTranscriptionSettings {
    pub fn normalized(mut self) -> Self {
        self.max_segment_minutes = self.max_segment_minutes.clamp(1, 60);
        self.output_dir = self
            .output_dir
            .filter(|dir| !dir.trim().is_empty())
            .map(|dir| dir.trim().to_string());
        self
    }
}

/// Settings of the "Recall" page (fork feature): the note vault. Grouped
/// into one struct so the page persists through a single command.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default, Type)]
#[serde(default)]
pub struct RecallSettings {
    /// Folder the vault lives in. `None` means the default
    /// `<app data>/recall`.
    pub output_dir: Option<String>,
}

impl RecallSettings {
    pub fn normalized(mut self) -> Self {
        self.output_dir = self
            .output_dir
            .filter(|dir| !dir.trim().is_empty())
            .map(|dir| dir.trim().to_string());
        self
    }
}

/// The in-app llama.cpp server ("brain") behind post-processing and the
/// Multi-STT merge. Defaults reproduce `launch_server_E2B_Q4.ps1`: Gemma 4
/// E2B Q4 with its MTP draft, 8k context, sampling tuned for the merge/clean
/// prompts (temp 0.05, top-p 0.35), reasoning off, alias
/// `gemma-4-E2B-Q4-MTP`. See `llama_server::build_args`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type)]
#[serde(default)]
pub struct LlamaSettings {
    /// Folder that contains `llama-server(.exe)`: an install made by the app
    /// (`<app data>/llama_cpp/<backend>-<tag>`) or any existing one.
    pub server_dir: Option<String>,
    pub model_path: Option<String>,
    /// MTP / speculative draft model (`--model-draft`, `--spec-type draft-mtp`).
    pub draft_model_path: Option<String>,
    pub mmproj_path: Option<String>,
    /// Load the mmproj (vision/audio). Off saves ~1 GB of VRAM for text use.
    pub mmproj_enabled: bool,
    pub port: u16,
    pub context_size: u32,
    /// `-ngl`; -1 = all layers on the GPU.
    pub gpu_layers: i32,
    /// `--threads`; -1 = let llama.cpp choose.
    pub threads: i32,
    pub flash_attn: bool,
    /// `--reasoning on|off` (Gemma 4 thinking mode; off is much faster).
    pub reasoning: bool,
    pub temperature: f32,
    pub top_p: f32,
    pub top_k: u32,
    pub min_p: f32,
    pub spec_draft_n_max: u32,
    /// `--alias`; also the model name the post-processing provider sends.
    pub alias: String,
    /// Appended verbatim to the generated arguments.
    pub extra_args: String,
    /// When set, replaces the generated arguments entirely (everything after
    /// the executable).
    pub custom_args: Option<String>,
    /// `LLAMA_ATTN_ROT_DISABLE=1` in the server environment (+3â€“4 % on short
    /// prompts in the S2B2S benchmarks).
    pub attn_rot_disable: bool,
    /// Start the server when the app starts.
    pub autostart: bool,
    /// Start the server when a request targets it and it is not running.
    pub start_on_demand: bool,
    pub stop_on_exit: bool,
    /// Preferred release asset: `auto`, `cuda-13.4`, `cuda-12.4`, `vulkan`, `cpu`.
    pub backend: String,
    /// `latest`, `stable` or `nightly` for the release list.
    pub channel: String,
    /// Also download the ~500 MB CUDA runtime package (cudart / cuBLAS) with
    /// a CUDA build. Off, like the download script without `-IncludeCudart`:
    /// a machine with the CUDA toolkit installed already has those DLLs.
    pub include_cudart: bool,
}

/// Default port for the in-app llama-server.
///
/// Deliberately in the registered range (1024-49151) rather than the ephemeral
/// one: Windows hands out dynamic port reservations to Hyper-V, WSL and Docker
/// from 49152-65535, and a port inside a reservation cannot be bound by
/// anything. The previous default, 62966, sat in the middle of one such block
/// (62940-63039) and made the server fail on startup with nothing to explain
/// it â€” no process is listening, so the health check correctly reports "not
/// running" and only llama.cpp's stderr hints at the cause.
pub const DEFAULT_LLAMA_PORT: u16 = 18080;

impl Default for LlamaSettings {
    fn default() -> Self {
        Self {
            server_dir: None,
            model_path: None,
            draft_model_path: None,
            mmproj_path: None,
            mmproj_enabled: false,
            port: DEFAULT_LLAMA_PORT,
            context_size: 8192,
            gpu_layers: -1,
            threads: -1,
            flash_attn: true,
            reasoning: false,
            temperature: 0.05,
            top_p: 0.35,
            top_k: 64,
            min_p: 0.0,
            spec_draft_n_max: 4,
            alias: "gemma-4-E2B-Q4-MTP".to_string(),
            extra_args: String::new(),
            custom_args: None,
            attn_rot_disable: true,
            autostart: false,
            start_on_demand: true,
            stop_on_exit: true,
            backend: "auto".to_string(),
            channel: "latest".to_string(),
            include_cudart: false,
        }
    }
}

impl LlamaSettings {
    pub fn normalized(mut self) -> Self {
        let clean = |v: Option<String>| v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        self.server_dir = clean(self.server_dir);
        self.model_path = clean(self.model_path);
        self.draft_model_path = clean(self.draft_model_path);
        self.mmproj_path = clean(self.mmproj_path);
        self.custom_args = clean(self.custom_args);
        if self.port < 1024 {
            self.port = DEFAULT_LLAMA_PORT;
        }
        self.context_size = self.context_size.clamp(512, 262_144);
        self.gpu_layers = self.gpu_layers.max(-1);
        self.threads = self.threads.max(-1);
        self.temperature = if self.temperature.is_finite() {
            self.temperature.clamp(0.0, 2.0)
        } else {
            0.05
        };
        self.top_p = if self.top_p.is_finite() {
            self.top_p.clamp(0.0, 1.0)
        } else {
            0.35
        };
        self.min_p = if self.min_p.is_finite() {
            self.min_p.clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.top_k = self.top_k.min(1000);
        self.spec_draft_n_max = self.spec_draft_n_max.clamp(1, 16);
        let alias = self.alias.trim();
        self.alias = if alias.is_empty() {
            "gemma-4-E2B-Q4-MTP".into()
        } else {
            alias.into()
        };
        if !["auto", "cuda-13.4", "cuda-12.4", "vulkan", "cpu"].contains(&self.backend.as_str()) {
            self.backend = "auto".into();
        }
        if !["latest", "stable", "nightly"].contains(&self.channel.as_str()) {
            self.channel = "latest".into();
        }
        self
    }
}

/// How Live Mode grows the transcript file while you speak.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum LiveTranscriptGranularity {
    /// Mirror the live stream exactly, including the model's tentative tail.
    #[default]
    Character,
    /// Only write text up to the last completed word.
    Word,
}

/// Settings of the "Live Mode" page (continuous recording + live transcript).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type)]
#[serde(default)]
pub struct LiveModeSettings {
    /// Session folders are created under this directory. `None` uses
    /// `<app data>/live_mode`.
    pub output_dir: Option<String>,
    /// Target length of one audio chunk / transcript segment in minutes. 1â€“60.
    pub chunk_minutes: u32,
    pub transcript_format: TranscriptOutputFormat,
    pub granularity: LiveTranscriptGranularity,
    /// Write the raw microphone signal to `chunk_NNNN.wav` files.
    pub save_audio: bool,
    /// Rotate chunks on the first pause once 80 % of `chunk_minutes` has
    /// elapsed, so a cut never lands mid-word.
    pub prefer_silence_boundary: bool,
}

impl Default for LiveModeSettings {
    fn default() -> Self {
        Self {
            output_dir: None,
            chunk_minutes: 5,
            transcript_format: TranscriptOutputFormat::Txt,
            granularity: LiveTranscriptGranularity::Character,
            save_audio: true,
            prefer_silence_boundary: true,
        }
    }
}

impl LiveModeSettings {
    pub fn normalized(mut self) -> Self {
        self.chunk_minutes = self.chunk_minutes.clamp(1, 60);
        self.output_dir = self
            .output_dir
            .filter(|dir| !dir.trim().is_empty())
            .map(|dir| dir.trim().to_string());
        self
    }
}

/* â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€ Live FFT (fork) â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€ */

/// Which signal the Live FFT page analyses.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum FftSource {
    /// The microphone at its native rate, before noise suppression â€” the
    /// full bandwidth the device delivers (24 kHz at 48 kHz).
    #[default]
    Microphone,
    /// The 48 kHz frames as they leave RNNoise (full bandwidth), or the
    /// untouched microphone while suppression is off, so toggling it is a
    /// direct before/after.
    Denoised,
    /// The 16 kHz frames a model hears: after resampling and noise
    /// suppression, before the VAD. Bandwidth stops at 8 kHz.
    Processed,
}

/// Frequency axis of the spectrum.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum FftScale {
    #[default]
    Log,
    Mel,
    Erb,
    Bark,
    Chroma,
    Linear,
    /// Mel + Log blend.
    Melog,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum FftWarpInterp {
    /// Two taps.
    #[default]
    Linear,
    /// Catmull-Rom, four taps: a 16K FFT with cubic looks like 32K with linear.
    Cubic,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum FftWindowLengthMode {
    #[default]
    Samples,
    Milliseconds,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum FftWindowType {
    #[default]
    Kaiser,
    Hann,
    Hamming,
    Blackman,
    BlackmanHarris,
    Rectangular,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum FftWeighting {
    #[default]
    Off,
    /// IEC 61672 A-weighting.
    A,
    /// C-weighting.
    C,
    /// ITU-R 468 noise weighting.
    Itu468,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum FftMagnitudeNorm {
    /// `mean(window) == 1`; a full-scale sine peaks at `window / 2`.
    #[default]
    CoherentGain,
    /// `sum(window) == 2`; a sine of amplitude 1 reads 1.0.
    FullScale,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum FftLoudnessMode {
    /// Linear magnitude.
    Off,
    /// Decibels relative to the reference.
    #[default]
    Db,
    /// Decibels mapped onto 0â€¦1 over `db_range`.
    DbNormalized,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum FftDbReference {
    /// 0 dB = the loudest bin of this frame.
    FramePeak,
    /// 0 dB = digital full scale.
    #[default]
    Dbfs,
    /// 0 dB = a slow peak follower (1.5 s release).
    Agc,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum FftBallisticsMode {
    /// Per-frame smoothing coefficients (0 = follow instantly).
    Coefficient,
    /// Time constants in milliseconds, independent of the update rate.
    #[default]
    Milliseconds,
}

/// Zero-padded FFT lengths the page offers.
pub const FFT_SIZES: [u32; 7] = [1024, 2048, 4096, 8192, 16384, 32768, 65536];
pub const MIN_FFT_OUTPUT_BINS: u32 = 32;
/// Upper bound of the per-frame event payload (8192 floats â‰ˆ 70 KB of JSON).
pub const MAX_FFT_OUTPUT_BINS: u32 = 8192;
pub const MIN_FFT_WINDOW_SAMPLES: u32 = 16;
pub const MAX_FFT_WINDOW_SAMPLES: u32 = 65536;
pub const MIN_FFT_UPDATE_RATE_HZ: u32 = 5;
pub const MAX_FFT_UPDATE_RATE_HZ: u32 = 60;

/// How the recording overlay's spectrum is drawn.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum OverlayScopeStyle {
    #[default]
    Area,
    Line,
    Bars,
}

pub const MIN_OVERLAY_WAVE_SAMPLES: u32 = 256;
pub const MAX_OVERLAY_WAVE_SAMPLES: u32 = 16_384;
pub const MIN_OVERLAY_VIEW_WIDTH: u32 = 32;
pub const MAX_OVERLAY_VIEW_WIDTH: u32 = 160;
pub const MIN_OVERLAY_VIEW_HEIGHT: u32 = 14;
pub const MAX_OVERLAY_VIEW_HEIGHT: u32 = 48;
pub const MIN_OVERLAY_CIRCULAR_BINS: u32 = 12;
pub const MAX_OVERLAY_CIRCULAR_BINS: u32 = 240;
pub const MIN_OVERLAY_CIRCULAR_GAIN: f32 = 0.05;
pub const MAX_OVERLAY_CIRCULAR_GAIN: f32 = 8.0;
pub const MIN_OVERLAY_CIRCULAR_FLOOR: f32 = 0.0;
pub const MAX_OVERLAY_CIRCULAR_FLOOR: f32 = 0.9;
pub const MIN_OVERLAY_CIRCULAR_SIZE: u32 = 32;
pub const MAX_OVERLAY_CIRCULAR_SIZE: u32 = 400;
pub const MIN_OVERLAY_VIEW_SCALE: u32 = 50;
pub const MAX_OVERLAY_VIEW_SCALE: u32 = 400;
pub const MIN_OVERLAY_SIGNAL_SCALE: f32 = 0.1;
pub const MAX_OVERLAY_SIGNAL_SCALE: f32 = 10.0;

/// The picture the recording overlay draws of the microphone (see
/// `live_fft::scope` and `overlay/OverlayScope.tsx`). The analysis behind it
/// follows `live_fft`; this only shapes the display. Mirrored by
/// `src/lib/overlayScope.ts`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type)]
#[serde(default)]
pub struct OverlayScopeSettings {
    /// Draw the spectrum view.
    pub show_spectrum: bool,
    /// Draw the waveform view.
    pub show_wave: bool,
    pub spectrum_style: OverlayScopeStyle,
    /// Draw the spectrum rising from the centre line with its negative
    /// mirrored below, so it reads like the centred waveform beside it.
    pub spectrum_mirror: bool,
    /// Keep a slowly falling marker at each column's recent peak, like the
    /// Live FFT page's peak hold.
    pub peak_hold: bool,
    /// Samples of raw audio the waveform view covers (256â€¦16384).
    pub wave_samples: u32,
    /// Raised-cosine fade at each end of that window, in samples
    /// (0â€¦half the window), so the trace starts and ends at zero.
    pub wave_taper_samples: u32,
    /// Auto-gain floor of the waveform as a full-scale fraction: quieter
    /// signals are not blown up to full height (0.001â€¦0.5).
    pub wave_gain_floor: f32,
    /// Width of each view in logical pixels (32â€¦160).
    pub view_width: u32,
    /// Height of the views in logical pixels (14â€¦48).
    pub view_height: u32,
    /// Draw the circular-spectrum view: a third view beside the linear
    /// spectrum and the waveform. The bins are combined with their own
    /// inversion â€” appended and prepended â€” and the two symmetric signals
    /// are added into the input signal (the cross-sum of each bin with its
    /// mirror partner, halved into display units). The two branches ride at
    /// radius 1+s and 1-s around a full 2Ï€ sweep, the whole figure rotated
    /// 90Â° so the seam straddles the right of the ring.
    pub show_circular: bool,
    /// Circular style: radial bars between the inner and outer loop, or the
    /// two loops drawn as lines.
    pub circular_bars: bool,
    /// Display bins of the circular loop (12â€¦240). The pipeline's bins are
    /// peak-pooled down to this many, so fewer bins means chunkier bars.
    pub circular_bins: u32,
    /// Fixed display gain of the circular loop (0.05â€¦8). The pooled bins are
    /// multiplied by this and clamped to 0â€¦1 â€” deliberately a fixed scale,
    /// not a dynamic normalisation, so the loop's size breathes with the
    /// signal instead of always filling the ring.
    pub circular_gain: f32,
    /// Floor of the circular loop as a fraction of full scale (0â€¦0.9). Bars
    /// below it are not drawn: without a floor the ambient room tone paints
    /// the whole ring and the loop reads as a filled disc.
    pub circular_floor: f32,
    /// Side of the square circular-spectrum view, in logical pixels
    /// (32â€¦400).
    pub circular_size: u32,
    /// Draw the circular spectrum as a full-window background layer behind
    /// the card instead of as its own view in the block.
    pub circular_background: bool,
    /// Linear spectrum view scale, percent of its base size (50â€¦400).
    pub spectrum_scale: u32,
    /// Waveform view scale, percent of its base size (50â€¦400).
    pub wave_scale: u32,
    /// Signal-intensity scale for the linear spectrum (0.1â€¦10). Multiplies the
    /// display units before drawing, so the spectrum reads taller without
    /// changing the view's pixel width.
    pub spectrum_signal_scale: f32,
    /// Signal-intensity scale for the raw-audio waveform (0.1â€¦10). Multiplies
    /// the sample values before drawing, so the trace swings taller without
    /// changing the view's pixel width.
    pub wave_signal_scale: f32,
    /// Signal-intensity scale for the circular spectrum (0.1â€¦10). Multiplies
    /// the pooled display units before drawing, so the ring breathes more
    /// dramatically without changing its pixel size.
    pub circular_signal_scale: f32,
    /// Draw the raw-audio waveform *inside* the circular spectrum â€” a centred
    /// horizontal trace from the ring's left edge to its right edge â€” instead
    /// of as its own view beside it.
    pub wave_inside_circular: bool,
    /// Draw the inner (negative-contracting) loop of the circular spectrum.
    /// When off, only the outer (positive-expanding) loop is shown. The outer
    /// line is always visible.
    pub circular_show_inner: bool,
}

impl Default for OverlayScopeSettings {
    fn default() -> Self {
        Self {
            show_spectrum: true,
            show_wave: true,
            spectrum_style: OverlayScopeStyle::Area,
            spectrum_mirror: true,
            peak_hold: false,
            wave_samples: 4096,
            wave_taper_samples: 512,
            wave_gain_floor: 0.02,
            view_width: 48,
            view_height: 22,
            show_circular: false,
            circular_bars: true,
            circular_bins: 48,
            circular_gain: 2.0,
            circular_floor: 0.15,
            circular_size: 64,
            circular_background: false,
            spectrum_scale: 100,
            wave_scale: 100,
            spectrum_signal_scale: 1.0,
            wave_signal_scale: 1.0,
            circular_signal_scale: 1.0,
            wave_inside_circular: false,
            circular_show_inner: true,
        }
    }
}

impl OverlayScopeSettings {
    pub fn normalized(mut self) -> Self {
        let d = Self::default();
        self.wave_samples = self
            .wave_samples
            .clamp(MIN_OVERLAY_WAVE_SAMPLES, MAX_OVERLAY_WAVE_SAMPLES);
        self.wave_taper_samples = self.wave_taper_samples.min(self.wave_samples / 2);
        self.wave_gain_floor = if self.wave_gain_floor.is_finite() {
            self.wave_gain_floor.clamp(0.001, 0.5)
        } else {
            d.wave_gain_floor
        };
        self.view_width = self
            .view_width
            .clamp(MIN_OVERLAY_VIEW_WIDTH, MAX_OVERLAY_VIEW_WIDTH);
        self.view_height = self
            .view_height
            .clamp(MIN_OVERLAY_VIEW_HEIGHT, MAX_OVERLAY_VIEW_HEIGHT);
        self.circular_bins = self
            .circular_bins
            .clamp(MIN_OVERLAY_CIRCULAR_BINS, MAX_OVERLAY_CIRCULAR_BINS);
        self.circular_gain = if self.circular_gain.is_finite() {
            self.circular_gain
                .clamp(MIN_OVERLAY_CIRCULAR_GAIN, MAX_OVERLAY_CIRCULAR_GAIN)
        } else {
            d.circular_gain
        };
        self.circular_floor = if self.circular_floor.is_finite() {
            self.circular_floor
                .clamp(MIN_OVERLAY_CIRCULAR_FLOOR, MAX_OVERLAY_CIRCULAR_FLOOR)
        } else {
            d.circular_floor
        };
        self.circular_size = self
            .circular_size
            .clamp(MIN_OVERLAY_CIRCULAR_SIZE, MAX_OVERLAY_CIRCULAR_SIZE);
        self.spectrum_scale = self
            .spectrum_scale
            .clamp(MIN_OVERLAY_VIEW_SCALE, MAX_OVERLAY_VIEW_SCALE);
        self.wave_scale = self
            .wave_scale
            .clamp(MIN_OVERLAY_VIEW_SCALE, MAX_OVERLAY_VIEW_SCALE);
        self.spectrum_signal_scale = if self.spectrum_signal_scale.is_finite() {
            self.spectrum_signal_scale
                .clamp(MIN_OVERLAY_SIGNAL_SCALE, MAX_OVERLAY_SIGNAL_SCALE)
        } else {
            d.spectrum_signal_scale
        };
        self.wave_signal_scale = if self.wave_signal_scale.is_finite() {
            self.wave_signal_scale
                .clamp(MIN_OVERLAY_SIGNAL_SCALE, MAX_OVERLAY_SIGNAL_SCALE)
        } else {
            d.wave_signal_scale
        };
        self.circular_signal_scale = if self.circular_signal_scale.is_finite() {
            self.circular_signal_scale
                .clamp(MIN_OVERLAY_SIGNAL_SCALE, MAX_OVERLAY_SIGNAL_SCALE)
        } else {
            d.circular_signal_scale
        };
        self
    }

    /// Width of the linear spectrum view: its base width times the scale.
    pub fn spectrum_view_w(&self) -> u32 {
        self.view_width * self.spectrum_scale / 100
    }

    /// Width of the waveform view: its base width times the scale.
    pub fn wave_view_w(&self) -> u32 {
        self.view_width * self.wave_scale / 100
    }

    /// Height the scope views need: the tallest visible view. The pill's row
    /// grows to fit it. The background circular layer is not counted — it is
    /// an absolute layer over the window, not a block view.
    pub fn view_height_px(&self) -> u32 {
        let mut h = 0u32;
        if self.show_spectrum {
            h = h.max(self.view_height * self.spectrum_scale / 100);
        }
        // wave_inside_circular draws inside the ring — no block height.
        let wave_inside =
            self.wave_inside_circular && self.show_circular && !self.circular_background;
        if self.show_wave && !wave_inside {
            h = h.max(self.view_height * self.wave_scale / 100);
        }
        if self.show_circular && !self.circular_background {
            h = h.max(self.circular_size);
        }
        h
    }

    /// Width of the scope block inside the pill: the views at their scaled
    /// widths, the 6 px gap between them and the 8 px trailing padding; zero
    /// without views. `overlayScopeBlockWidth` in `src/lib/overlayScope.ts`
    /// is the same sum.
    pub fn block_width_px(&self) -> u32 {
        let mut views = 0u32;
        let mut w = 0u32;
        if self.show_spectrum {
            views += 1;
            w += self.spectrum_view_w();
        }
        // wave_inside_circular draws inside the ring — no block width.
        let wave_inside =
            self.wave_inside_circular && self.show_circular && !self.circular_background;
        if self.show_wave && !wave_inside {
            views += 1;
            w += self.wave_view_w();
        }
        if self.show_circular && !self.circular_background {
            views += 1;
            w += self.circular_size;
        }
        if views == 0 {
            0
        } else {
            w + 6 * (views - 1) + 8
        }
    }
}

/// Settings of the "Live FFT" page, grouped the way the page shows them
/// (Spectrum, EQ, Window & Weighting, Loudness & Ballistics, Performance).
/// `update_rate_hz` is the analysis frame rate; there is no planner policy
/// or poll interval to configure, since rustfft plans are instant and the
/// settings are pushed, not polled.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type)]
#[serde(default)]
pub struct LiveFftSettings {
    pub source: FftSource,
    // --- Spectrum ---
    pub scale: FftScale,
    pub warp_interpolation: FftWarpInterp,
    /// Highest frequency on the axis; clamped to Nyquist at run time.
    pub display_max_hz: f32,
    /// Size of the warped spectrum handed to the page (32â€¦8192).
    pub output_bins: u32,
    /// 0 = linear grid, 1 = fully perceptual.
    pub warp_blend: f32,
    /// Lowest frequency of the Log / Melog grid.
    pub log_floor_hz: f32,
    pub window_length_mode: FftWindowLengthMode,
    /// Analysis window in samples (3175 = 72 ms at 44.1 kHz).
    pub window_samples: u32,
    /// Analysis window in milliseconds, used when the mode says so.
    pub window_ms: f32,
    /// Zero-padded transform length; grown to the next power of two above
    /// the window when that is larger.
    pub fft_size: u32,
    // --- EQ (applied at ingest to new samples; stateful) ---
    pub eq_enabled: bool,
    pub high_shelf: bool,
    pub low_shelf: bool,
    pub high_gain_db: f32,
    pub high_cutoff_hz: f32,
    pub low_gain_db: f32,
    pub low_cutoff_hz: f32,
    pub eq_q: f32,
    /// Wet/dry blend of the EQ (0â€¦5).
    pub eq_amount: f32,
    // --- Window & Weighting ---
    pub window_type: FftWindowType,
    pub kaiser_beta: f32,
    pub weighting: FftWeighting,
    pub magnitude_norm: FftMagnitudeNorm,
    // --- Loudness & Ballistics ---
    pub loudness_mode: FftLoudnessMode,
    pub db_reference: FftDbReference,
    /// Floor of the dB display, in dB below the reference.
    pub db_range: f32,
    pub ballistics_enabled: bool,
    pub ballistics_mode: FftBallisticsMode,
    /// Per-frame attack coefficient (0â€¦0.99).
    pub attack: f32,
    /// Per-frame release coefficient (0â€¦0.99).
    pub release: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    // --- Performance ---
    /// Run the transform on the analysis worker thread (on) or inline on the
    /// audio consumer thread (off).
    pub async_analysis: bool,
    /// Spectrum frames per second sent to the page (5â€¦60).
    pub update_rate_hz: u32,
    // --- Voice detection ---
    /// Run the speech detector on the analysed session and report its
    /// per-frame verdicts to the page (`VadTestEvent`), so the threshold and
    /// noise suppression can be tuned against the spectrum. Changing it
    /// restarts a running session (the VAD policy is fixed per recording).
    pub show_vad: bool,
}

impl Default for LiveFftSettings {
    fn default() -> Self {
        Self {
            source: FftSource::Microphone,
            scale: FftScale::Log,
            warp_interpolation: FftWarpInterp::Linear,
            display_max_hz: 24_000.0,
            output_bins: 1024,
            warp_blend: 0.963,
            log_floor_hz: 20.0,
            window_length_mode: FftWindowLengthMode::Samples,
            window_samples: 3175,
            window_ms: 72.0,
            fft_size: 32_768,
            eq_enabled: false,
            high_shelf: true,
            low_shelf: true,
            high_gain_db: 6.0,
            high_cutoff_hz: 1000.0,
            low_gain_db: 0.0,
            low_cutoff_hz: 200.0,
            eq_q: 0.707,
            eq_amount: 1.0,
            window_type: FftWindowType::Kaiser,
            kaiser_beta: 15.0,
            weighting: FftWeighting::Off,
            magnitude_norm: FftMagnitudeNorm::CoherentGain,
            loudness_mode: FftLoudnessMode::Db,
            db_reference: FftDbReference::Dbfs,
            db_range: 90.0,
            ballistics_enabled: true,
            ballistics_mode: FftBallisticsMode::Milliseconds,
            attack: 0.0,
            release: 0.0,
            attack_ms: 15.0,
            release_ms: 250.0,
            async_analysis: true,
            update_rate_hz: 30,
            show_vad: false,
        }
    }
}

impl LiveFftSettings {
    /// The page's "Raw" preset: linear magnitude, frame-peak reference,
    /// ballistics off.
    pub fn raw_defaults() -> Self {
        Self {
            loudness_mode: FftLoudnessMode::Off,
            db_reference: FftDbReference::FramePeak,
            db_range: 80.0,
            ballistics_enabled: false,
            ballistics_mode: FftBallisticsMode::Coefficient,
            attack_ms: 50.0,
            release_ms: 200.0,
            ..Self::default()
        }
    }

    pub fn normalized(mut self) -> Self {
        let finite = |v: f32, fallback: f32| if v.is_finite() { v } else { fallback };
        let d = Self::default();
        self.display_max_hz = finite(self.display_max_hz, d.display_max_hz).clamp(100.0, 192_000.0);
        self.output_bins = self
            .output_bins
            .clamp(MIN_FFT_OUTPUT_BINS, MAX_FFT_OUTPUT_BINS);
        self.warp_blend = finite(self.warp_blend, d.warp_blend).clamp(0.0, 1.0);
        self.log_floor_hz = finite(self.log_floor_hz, d.log_floor_hz).clamp(1.0, 5000.0);
        self.window_samples = self
            .window_samples
            .clamp(MIN_FFT_WINDOW_SAMPLES, MAX_FFT_WINDOW_SAMPLES);
        self.window_ms = finite(self.window_ms, d.window_ms).clamp(1.0, 5000.0);
        if !FFT_SIZES.contains(&self.fft_size) {
            self.fft_size = d.fft_size;
        }
        self.high_gain_db = finite(self.high_gain_db, d.high_gain_db).clamp(-24.0, 24.0);
        self.high_cutoff_hz = finite(self.high_cutoff_hz, d.high_cutoff_hz).clamp(20.0, 20_000.0);
        self.low_gain_db = finite(self.low_gain_db, d.low_gain_db).clamp(-24.0, 24.0);
        self.low_cutoff_hz = finite(self.low_cutoff_hz, d.low_cutoff_hz).clamp(20.0, 5000.0);
        self.eq_q = finite(self.eq_q, d.eq_q).clamp(0.1, 4.0);
        self.eq_amount = finite(self.eq_amount, d.eq_amount).clamp(0.0, 5.0);
        self.kaiser_beta = finite(self.kaiser_beta, d.kaiser_beta).clamp(0.0, 100.0);
        self.db_range = finite(self.db_range, d.db_range).clamp(10.0, 160.0);
        self.attack = finite(self.attack, d.attack).clamp(0.0, 0.99);
        self.release = finite(self.release, d.release).clamp(0.0, 0.99);
        self.attack_ms = finite(self.attack_ms, d.attack_ms).clamp(0.0, 2000.0);
        self.release_ms = finite(self.release_ms, d.release_ms).clamp(0.0, 5000.0);
        self.update_rate_hz = self
            .update_rate_hz
            .clamp(MIN_FFT_UPDATE_RATE_HZ, MAX_FFT_UPDATE_RATE_HZ);
        self
    }
}

#[derive(Clone, Serialize, Deserialize, Type)]
#[serde(transparent)]
pub struct SecretMap(pub(crate) HashMap<String, String>);

impl fmt::Debug for SecretMap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let redacted: HashMap<&String, &str> = self
            .0
            .iter()
            .map(|(k, v)| (k, if v.is_empty() { "" } else { "[REDACTED]" }))
            .collect();
        redacted.fmt(f)
    }
}

impl std::ops::Deref for SecretMap {
    type Target = HashMap<String, String>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for SecretMap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/* still needed for composing the initial JSON in the store ------------- */
/// The container-level `serde(default)` (backed by the `Default` impl below)
/// guarantees every field â€” including ones added in the future â€” falls back to
/// its `get_default_settings()` value when missing from a stored settings
/// object, so a partial store can never fail the whole load (#1619).
/// Field-level defaults below take precedence where present.
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
#[serde(default)]
pub struct AppSettings {
    /// Internal settings schema marker for one-time migrations. Fresh installs
    /// start at the current version; existing stores missing this key are
    /// treated as version 0 and migrated forward.
    #[serde(default = "default_settings_schema_version")]
    pub settings_schema_version: u32,
    /// Defaults to empty on partial stores; the load path merges in the
    /// default bindings for any missing keys before the settings are used.
    #[serde(default)]
    pub bindings: HashMap<String, ShortcutBinding>,
    /// Replaces the pre-0.10 `push_to_talk` bool; stores missing this key are
    /// migrated from it in `apply_settings_migrations`.
    #[serde(default)]
    pub shortcut_activation: ShortcutActivation,
    /// Hold-or-toggle only: a press held at least this long is push-to-talk,
    /// anything shorter is a tap that locks recording on.
    #[serde(default = "default_hold_threshold_ms")]
    pub hold_threshold_ms: u64,
    #[serde(default)]
    pub audio_feedback: bool,
    #[serde(default = "default_audio_feedback_volume")]
    pub audio_feedback_volume: f32,
    #[serde(default = "default_sound_theme")]
    pub sound_theme: SoundTheme,
    #[serde(default = "default_start_hidden")]
    pub start_hidden: bool,
    #[serde(default = "default_autostart_enabled")]
    pub autostart_enabled: bool,
    #[serde(default = "default_update_checks_enabled")]
    pub update_checks_enabled: bool,
    #[serde(default = "default_show_whats_new_on_update")]
    pub show_whats_new_on_update: bool,
    /// The app version whose What's New the user has already seen. Fresh installs
    /// default to the current version (nothing is "new" to them). Existing users
    /// upgrading from before this key existed are blanked by the migration so they
    /// see the current release's notes â€” see `apply_settings_migrations`.
    #[serde(default = "default_whats_new_last_seen_version")]
    pub whats_new_last_seen_version: String,
    #[serde(default = "default_model")]
    pub selected_model: String,
    #[serde(default)]
    pub onboarding_completed: bool,
    #[serde(default = "default_always_on_microphone")]
    pub always_on_microphone: bool,
    #[serde(default)]
    pub selected_microphone: Option<String>,
    /// Which input channel to use on the selected microphone device.
    /// None means "average all channels" (original behavior).
    #[serde(default)]
    pub selected_channel: Option<u16>,
    #[serde(default)]
    pub clamshell_microphone: Option<String>,
    #[serde(default)]
    pub selected_output_device: Option<String>,
    #[serde(default = "default_translate_to_english")]
    pub translate_to_english: bool,
    #[serde(default = "default_selected_language")]
    pub selected_language: String,
    #[serde(default = "default_overlay_position")]
    pub overlay_position: OverlayPosition,
    /// When true the overlay is placed at the saved `recording_overlay_custom_x_px` /
    /// `recording_overlay_custom_y_px` instead of the auto Top/Bottom anchor.
    /// Set by the drag-grip on the recording overlay and persists across restarts.
    #[serde(default)]
    pub recording_overlay_use_manual_position: bool,
    #[serde(default)]
    pub recording_overlay_has_saved_custom_position: bool,
    #[serde(default)]
    pub recording_overlay_manual_position_uses_physical_px: bool,
    #[serde(default)]
    pub recording_overlay_custom_x_px: i32,
    #[serde(default)]
    pub recording_overlay_custom_y_px: i32,
    #[serde(default = "default_overlay_window_fade_ms")]
    pub overlay_window_fade_ms: u32,
    #[serde(default = "default_overlay_window_corner_radius")]
    pub overlay_window_corner_radius: f32,
    #[serde(default = "default_debug_mode")]
    pub debug_mode: bool,
    #[serde(default = "default_log_level")]
    pub log_level: LogLevel,
    #[serde(default)]
    pub custom_words: Vec<String>,
    #[serde(default)]
    pub model_unload_timeout: ModelUnloadTimeout,
    #[serde(default = "default_word_correction_threshold")]
    pub word_correction_threshold: f64,
    #[serde(default = "default_history_limit")]
    pub history_limit: u32,
    #[serde(default = "default_recording_retention_period")]
    pub recording_retention_period: RecordingRetentionPeriod,
    #[serde(default = "default_save_raw_audio")]
    pub save_raw_audio: bool,
    #[serde(default)]
    pub paste_method: PasteMethod,
    #[serde(default)]
    pub clipboard_handling: ClipboardHandling,
    #[serde(default = "default_auto_submit")]
    pub auto_submit: bool,
    #[serde(default)]
    pub auto_submit_key: AutoSubmitKey,
    #[serde(default = "default_post_process_enabled")]
    pub post_process_enabled: bool,
    #[serde(default = "default_post_process_provider_id")]
    pub post_process_provider_id: String,
    #[serde(default = "default_post_process_providers")]
    pub post_process_providers: Vec<PostProcessProvider>,
    #[serde(default = "default_post_process_api_keys")]
    pub post_process_api_keys: SecretMap,
    #[serde(default = "default_post_process_models")]
    pub post_process_models: HashMap<String, String>,
    #[serde(default = "default_post_process_prompts")]
    pub post_process_prompts: Vec<LLMPrompt>,
    #[serde(default)]
    pub post_process_selected_prompt_id: Option<String>,
    #[serde(default)]
    pub mute_while_recording: bool,
    #[serde(default)]
    pub append_trailing_space: bool,
    #[serde(default)]
    pub append_trailing_newline: bool,
    #[serde(default = "default_app_language")]
    pub app_language: String,
    #[serde(default = "default_theme")]
    pub theme: Theme,
    #[serde(default)]
    pub custom_accent_color: Option<String>,
    /// Zoom of the settings window (0.7â€“1.6, 1.0 = native), for screens whose
    /// OS scaling makes the UI too small or too large. Applied as CSS zoom.
    #[serde(default = "default_ui_scale")]
    pub ui_scale: f32,
    #[serde(default)]
    pub experimental_enabled: bool,
    #[serde(default)]
    pub lazy_stream_close: bool,
    #[serde(default)]
    pub keyboard_implementation: KeyboardImplementation,
    #[serde(default = "default_show_tray_icon")]
    pub show_tray_icon: bool,
    #[serde(default = "default_paste_delay_ms")]
    pub paste_delay_ms: u32,
    #[serde(default = "default_paste_delay_after_ms")]
    pub paste_delay_after_ms: u32,
    /// Debug-gated ("beta") receipt-sequenced paste: restore the clipboard only
    /// after the target app actually reads the transcript, instead of after a
    /// fixed delay. See `paste_tx`. macOS and Windows only.
    #[serde(default)]
    pub reliable_paste: bool,
    #[serde(default = "default_typing_tool")]
    pub typing_tool: TypingTool,
    #[serde(default)]
    pub external_script_path: Option<String>,
    #[serde(default = "default_filler_word_removal_enabled")]
    pub filler_word_removal_enabled: bool,
    #[serde(default)]
    pub custom_filler_words: Option<Vec<String>>,
    #[serde(default)]
    pub transcribe_accelerator: TranscribeAcceleratorSetting,
    /// Stable transcribe.cpp device selector. This is derived from the backend's
    /// `device_id` when available (or its name for backends such as Metal),
    /// never from the process-local device registry index.
    #[serde(
        default = "default_transcribe_gpu_device",
        deserialize_with = "deserialize_transcribe_gpu_device"
    )]
    #[specta(type = Option<String>)]
    pub transcribe_gpu_device: Option<String>,
    #[serde(default)]
    pub extra_recording_buffer_ms: u32,
    #[serde(default = "default_vad_enabled")]
    pub vad_enabled: bool,
    /// RNNoise noise suppression on the microphone path, before the VAD and
    /// the model (`audio_toolkit::audio::DenoiseChain`). Off by default: it
    /// adds a little latency and can make some voices sound processed; the
    /// live VAD test in Settings â†’ Advanced shows its effect.
    #[serde(default)]
    pub denoise_enabled: bool,
    /// RNNoise wet/dry mix (0â€“1): 1 = the suppressor's output, 0 = the input
    /// untouched.
    #[serde(default = "default_denoise_strength")]
    pub denoise_strength: f32,
    /// RNNoise's own speech probability below which the suppressor mutes
    /// the frame (0â€“1); 0 turns the gate off.
    #[serde(default)]
    pub denoise_vad_threshold: f32,
    /// How long audio keeps passing after the last frame above that
    /// threshold, in milliseconds.
    #[serde(default = "default_denoise_vad_grace_ms")]
    pub denoise_vad_grace_ms: u32,
    /// Speech-probability threshold of the Earshot detector (0.05â€“0.95).
    #[serde(default = "default_vad_threshold_earshot")]
    pub vad_threshold_earshot: f32,
    /// Which recording overlay to show: None / Minimal / Live. Streaming mode is
    /// not gated on this â€” that follows model capability. Migrated from the old
    /// `overlay_position` (position `none` â†’ style `None`).
    #[serde(default = "default_overlay_style")]
    pub overlay_style: OverlayStyle,
    /// Whether the live streaming overlay should reveal text character-by-character
    /// in direct/typewriter mode instead of chunk-by-chunk / word-by-word.
    #[serde(default)]
    pub overlay_direct_mode: bool,
    /// Speed at which characters appear in direct streaming mode (characters per second).
    #[serde(default = "default_overlay_direct_speed")]
    pub overlay_direct_speed: u32,
    /// Whether the live preview may *rewrite* text it has already shown —
    /// "back correction".
    ///
    /// Off (the default): the preview only ever reveals forward, and a revision
    /// that is not an extension of what is displayed is applied as one block,
    /// exactly as it already is when direct mode is off.
    ///
    /// On: the preview retypes corrections, rewinding the revealed text to the
    /// longest common prefix and revealing the new wording again. That is
    /// pleasant for a model that revises rarely, and unpleasant for one that
    /// revises on every chunk — a model that re-attends over the growing audio
    /// context (see `transcribe_stream_text` in the fork's public header)
    /// replaces its volatile `tentative_text` tail on each decode, so the
    /// rewind fires continuously and the preview appears to stutter backwards.
    ///
    /// Deliberately independent of `multi_stt_streaming_first_enabled`: that
    /// flag decides whether a batch pass re-transcribes each pause, this one
    /// decides only whether the display is allowed to move backwards. Neither
    /// implies the other, so a user who wants steady text keeps it steady in
    /// both modes.
    #[serde(default)]
    pub overlay_back_correction: bool,
    /// Speed at which characters are typed in direct streaming paste method (characters per second).
    #[serde(default = "default_direct_streaming_speed")]
    pub direct_streaming_speed: u32,
    /// Whether the overlay shows live speech statistics: a speech/silence
    /// indicator, a timer that counts only while you are actually speaking, and
    /// the running average words per minute. Applies to both the Minimal and
    /// Live overlays; ignored when the overlay is off.
    #[serde(default = "default_overlay_speech_stats")]
    pub overlay_speech_stats: bool,
    /// How long silence must last before the speech timer stops counting.
    /// Shorter gaps are treated as part of the same utterance, so the timer does
    /// not stall on the pauses between words.
    #[serde(default = "default_speech_pause_hold_ms")]
    pub speech_pause_hold_ms: u32,
    // Multi STT settings
    #[serde(default)]
    pub multi_stt_enabled: bool,
    #[serde(default)]
    pub multi_stt_model_2: Option<String>,
    #[serde(default)]
    pub multi_stt_model_3: Option<String>,
    #[serde(default)]
    pub multi_stt_model_4: Option<String>,
    #[serde(default)]
    pub multi_stt_language_model_2: Option<String>,
    #[serde(default)]
    pub multi_stt_language_model_3: Option<String>,
    #[serde(default)]
    pub multi_stt_language_model_4: Option<String>,
    #[serde(default = "default_multi_stt_translate")]
    pub multi_stt_translate_model_2: bool,
    #[serde(default = "default_multi_stt_translate")]
    pub multi_stt_translate_model_3: bool,
    #[serde(default = "default_multi_stt_translate")]
    pub multi_stt_translate_model_4: bool,
    #[serde(default = "default_multi_stt_keep_models")]
    pub multi_stt_keep_extra_models_loaded: bool,
    #[serde(default)]
    pub multi_stt_merge_prompt: Option<LLMPrompt>,
    /// Multi-STT Performance Mode: when enabled, simulate a keyboard shortcut
    /// to boost CPU performance before transcription starts (FULL POWER) and
    /// restore normal power after the merge/paste completes.
    #[serde(default)]
    pub multi_stt_performance_mode_enabled: bool,
    #[serde(default)]
    pub multi_stt_performance_mode_trigger_on_start: bool,
    #[serde(default = "default_multi_stt_full_power_shortcut")]
    pub multi_stt_performance_mode_full_power_shortcut: String,
    #[serde(default = "default_multi_stt_normal_shortcut")]
    pub multi_stt_performance_mode_normal_shortcut: String,
    /// Experimental: run the primary streaming model as the live 1st model and
    /// replace its rough text in place, chunk by chunk, as the extras and the
    /// merge land (`multi_stt_stream`). Needs a streaming-capable primary model
    /// and a merge prompt; without either, Multi-STT takes its normal batch path.
    #[serde(default)]
    pub multi_stt_streaming_first_enabled: bool,
    /// How long the speaker has to pause before the chunk being spoken closes
    /// and is merged â€” what divides the session into chunks (100â€“10000 ms).
    /// Same test Live Mode uses for its silence boundary.
    #[serde(default = "default_multi_stt_streaming_pause_ms")]
    pub multi_stt_streaming_pause_ms: u32,
    /// How many already-closed chunks a merge window of the experimental
    /// streaming-first mode carries in front of the one it merges. The window is
    /// then `[the previous chunks .. the closed one]` instead of everything
    /// accumulated since the recording began, so its size is flat in the length
    /// of the session. 0 sends only the chunk that just closed. See
    /// `multi_stt_stream::strip_context_prefix` for what the extras' decodes of
    /// the context are cropped back against.
    #[serde(default = "default_multi_stt_streaming_context_chunks")]
    pub multi_stt_streaming_context_chunks: u32,
    // Microphone idle timeout
    #[serde(default = "default_mic_idle_timeout_value")]
    pub mic_idle_timeout_value: u32,
    #[serde(default)]
    pub mic_idle_timeout_unit: MicIdleTimeoutUnit,
    #[serde(default)]
    pub mic_idle_infinite: bool,
    #[serde(default)]
    pub native_streaming_latency_presets: HashMap<String, NativeStreamingLatencyPreset>,
    /// Per-model R2T2 (Confucius4-R2T2) native streaming chunk size, in whole
    /// milliseconds, keyed by model id.
    ///
    /// Deliberately a *separate* map from
    /// [`AppSettings::native_streaming_latency_presets`]: R2T2 is the one
    /// streaming family whose latency control is a continuous millisecond value
    /// rather than a four-value preset, so folding it into the preset enum
    /// would either lose the user's exact choice or force a fake preset tier.
    /// The two maps never apply to the same model — a given model has one
    /// latency extension kind or the other.
    ///
    /// `#[serde(default)]` on an empty map is the backward-compatibility
    /// story: an existing install has no entry, the resolver falls back to
    /// `R2T2_CHUNK_MS_DEFAULT` (320 ms), and behaviour is exactly what it was.
    /// Values are validated against the native 80..=2000 ms range by
    /// `change_native_streaming_chunk_ms_setting`; the resolver still
    /// re-validates because this file is user-editable.
    #[serde(default)]
    pub native_streaming_chunk_ms: HashMap<String, u32>,
    /// "Transcribe Files" page (fork feature).
    #[serde(default)]
    pub file_transcription: FileTranscriptionSettings,
    /// "Live Mode" page (fork feature).
    #[serde(default)]
    pub live_mode: LiveModeSettings,
    /// "Recall" page (fork feature): the note vault.
    #[serde(default)]
    pub recall: RecallSettings,
    /// "Live FFT" page (fork feature).
    #[serde(default)]
    pub live_fft: LiveFftSettings,
    /// The recording overlay's picture of the microphone (which views, style,
    /// waveform window, size); the analysis follows `live_fft`.
    #[serde(default)]
    pub overlay_scope: OverlayScopeSettings,
    /// In-app llama.cpp server (fork feature).
    #[serde(default)]
    pub llama: LlamaSettings,
}

pub const MIN_UI_SCALE: f32 = 0.7;
pub const MAX_UI_SCALE: f32 = 1.6;

fn default_ui_scale() -> f32 {
    1.0
}

pub fn clamp_ui_scale(scale: f32) -> f32 {
    if scale.is_finite() {
        scale.clamp(MIN_UI_SCALE, MAX_UI_SCALE)
    } else {
        1.0
    }
}

fn default_vad_threshold_earshot() -> f32 {
    DEFAULT_VAD_THRESHOLD_EARSHOT
}

fn default_denoise_strength() -> f32 {
    1.0
}

fn default_denoise_vad_grace_ms() -> u32 {
    200
}

fn default_model() -> String {
    "".to_string()
}

const CURRENT_SETTINGS_SCHEMA_VERSION: u32 = 6;

fn default_settings_schema_version() -> u32 {
    CURRENT_SETTINGS_SCHEMA_VERSION
}

fn default_multi_stt_translate() -> bool {
    false
}

fn default_multi_stt_keep_models() -> bool {
    true
}

fn default_multi_stt_full_power_shortcut() -> String {
    "ctrl+space".to_string()
}

fn default_multi_stt_normal_shortcut() -> String {
    "ctrl+alt+space".to_string()
}

/// One second: long enough that a breath does not close a chunk, short enough
/// that the correction lands while the speaker still remembers the words.
fn default_multi_stt_streaming_pause_ms() -> u32 {
    1000
}

/// One chunk: a window the extras re-decode in a second or two, carrying the
/// chunk before the new one so a word the stream cut at the break still reaches
/// the merge with what surrounds it.
fn default_multi_stt_streaming_context_chunks() -> u32 {
    1
}

fn default_hold_threshold_ms() -> u64 {
    300
}

fn default_always_on_microphone() -> bool {
    false
}

fn default_translate_to_english() -> bool {
    false
}

fn default_start_hidden() -> bool {
    false
}

fn default_autostart_enabled() -> bool {
    false
}

fn default_update_checks_enabled() -> bool {
    true
}

fn default_show_whats_new_on_update() -> bool {
    true
}

fn default_whats_new_last_seen_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn default_selected_language() -> String {
    "auto".to_string()
}

fn default_overlay_position() -> OverlayPosition {
    // Position only matters when the overlay is shown; whether it shows at all is
    // `overlay_style` (Linux defaults that to None). So a single default suffices.
    OverlayPosition::Bottom
}

fn default_overlay_style() -> OverlayStyle {
    // Linux hides the overlay by default; other platforms show the live overlay.
    // Position is independent and only selects top vs. bottom placement.
    #[cfg(target_os = "linux")]
    return OverlayStyle::None;
    #[cfg(not(target_os = "linux"))]
    return OverlayStyle::Live;
}

fn default_overlay_window_fade_ms() -> u32 {
    300
}

/// Clamp boundary for `overlay_window_corner_radius` — capped to keep the
/// 280×50 overlay readable (half of 50 = 25).
pub(crate) const OVERLAY_CORNER_RADIUS_MAX: f32 = 25.0;

fn default_overlay_window_corner_radius() -> f32 {
    0.0
}

fn default_overlay_direct_speed() -> u32 {
    30
}

fn default_direct_streaming_speed() -> u32 {
    30
}

fn default_overlay_speech_stats() -> bool {
    true
}

fn default_speech_pause_hold_ms() -> u32 {
    crate::audio_toolkit::DEFAULT_SPEECH_PAUSE_HOLD_MS
}

fn default_mic_idle_timeout_value() -> u32 {
    30
}

fn default_vad_enabled() -> bool {
    true
}

fn default_filler_word_removal_enabled() -> bool {
    true
}

fn default_debug_mode() -> bool {
    false
}

fn default_log_level() -> LogLevel {
    LogLevel::Debug
}

fn default_word_correction_threshold() -> f64 {
    0.18
}

fn default_paste_delay_ms() -> u32 {
    60
}

fn default_paste_delay_after_ms() -> u32 {
    60
}

fn default_auto_submit() -> bool {
    false
}

fn default_history_limit() -> u32 {
    5
}

fn default_recording_retention_period() -> RecordingRetentionPeriod {
    RecordingRetentionPeriod::PreserveLimit
}

fn default_save_raw_audio() -> bool {
    false
}

fn default_audio_feedback_volume() -> f32 {
    1.0
}

fn default_sound_theme() -> SoundTheme {
    SoundTheme::Marimba
}

fn default_theme() -> Theme {
    Theme::System
}

fn default_post_process_enabled() -> bool {
    false
}

fn default_app_language() -> String {
    tauri_plugin_os::locale()
        .map(|l| l.replace('_', "-"))
        .unwrap_or_else(|| "en".to_string())
}

fn default_show_tray_icon() -> bool {
    true
}

fn default_post_process_provider_id() -> String {
    "openai".to_string()
}

fn default_post_process_providers() -> Vec<PostProcessProvider> {
    let mut providers = vec![
        PostProcessProvider {
            id: "openai".to_string(),
            label: "OpenAI".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            allow_base_url_edit: false,
            models_endpoint: Some("/models".to_string()),
            supports_structured_output: true,
        },
        PostProcessProvider {
            id: "zai".to_string(),
            label: "Z.AI".to_string(),
            base_url: "https://api.z.ai/api/paas/v4".to_string(),
            allow_base_url_edit: false,
            models_endpoint: Some("/models".to_string()),
            supports_structured_output: true,
        },
        PostProcessProvider {
            id: "openrouter".to_string(),
            label: "OpenRouter".to_string(),
            base_url: "https://openrouter.ai/api/v1".to_string(),
            allow_base_url_edit: false,
            models_endpoint: Some("/models".to_string()),
            supports_structured_output: true,
        },
        PostProcessProvider {
            id: "anthropic".to_string(),
            label: "Anthropic".to_string(),
            base_url: "https://api.anthropic.com/v1".to_string(),
            allow_base_url_edit: false,
            models_endpoint: Some("/models".to_string()),
            supports_structured_output: false,
        },
        PostProcessProvider {
            id: "groq".to_string(),
            label: "Groq".to_string(),
            base_url: "https://api.groq.com/openai/v1".to_string(),
            allow_base_url_edit: false,
            models_endpoint: Some("/models".to_string()),
            supports_structured_output: false,
        },
        PostProcessProvider {
            id: "cerebras".to_string(),
            label: "Cerebras".to_string(),
            base_url: "https://api.cerebras.ai/v1".to_string(),
            allow_base_url_edit: false,
            models_endpoint: Some("/models".to_string()),
            supports_structured_output: true,
        },
    ];

    // Note: We always include Apple Intelligence on macOS ARM64 without checking availability
    // at startup. The availability check is deferred to when the user actually tries to use it
    // (in actions.rs). This prevents crashes on macOS 26.x beta where accessing
    // SystemLanguageModel.default during early app initialization causes SIGABRT.
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        providers.push(PostProcessProvider {
            id: APPLE_INTELLIGENCE_PROVIDER_ID.to_string(),
            label: "Apple Intelligence".to_string(),
            base_url: "apple-intelligence://local".to_string(),
            allow_base_url_edit: false,
            models_endpoint: None,
            supports_structured_output: true,
        });
    }

    // AWS Bedrock via Mantle (OpenAI-compatible endpoint)
    providers.push(PostProcessProvider {
        id: "bedrock_mantle".to_string(),
        label: "AWS Bedrock (Mantle)".to_string(),
        base_url: "https://bedrock-mantle.us-east-1.api.aws/v1".to_string(),
        allow_base_url_edit: false,
        models_endpoint: Some("/models".to_string()),
        supports_structured_output: true,
    });

    // Custom provider always comes last
    providers.push(PostProcessProvider {
        id: "custom".to_string(),
        label: "Custom".to_string(),
        base_url: "http://localhost:11434/v1".to_string(),
        allow_base_url_edit: true,
        models_endpoint: Some("/models".to_string()),
        supports_structured_output: false,
    });

    providers
}

fn default_post_process_api_keys() -> SecretMap {
    let mut map = HashMap::new();
    for provider in default_post_process_providers() {
        map.insert(provider.id, String::new());
    }
    SecretMap(map)
}

fn default_model_for_provider(provider_id: &str) -> String {
    if provider_id == APPLE_INTELLIGENCE_PROVIDER_ID {
        return APPLE_INTELLIGENCE_DEFAULT_MODEL_ID.to_string();
    }
    String::new()
}

fn default_post_process_models() -> HashMap<String, String> {
    let mut map = HashMap::new();
    for provider in default_post_process_providers() {
        map.insert(
            provider.id.clone(),
            default_model_for_provider(&provider.id),
        );
    }
    map
}

fn default_post_process_prompts() -> Vec<LLMPrompt> {
    vec![LLMPrompt {
        id: "default_improve_transcriptions".to_string(),
        name: "Improve Transcriptions".to_string(),
        prompt: "Role: You are an expert audio transcript cleaning and post-processing engine. Your task is to clean and refine a provided speech-to-text transcript into clear, grammatically correct text while preserving the speaker's exact meaning and original language.\n\nCore Instructions:\n1. Language Retention: Maintain the original language strictly (French in French, English in English, even if mixed in the same audio). Never translate.\n2. Grammar & Misrecognitions: Fix spelling, capitalization, missing commas, and sentence boundaries. Fix obvious speech-to-text misrecognitions and phonetic errors contextually to make the text completely coherent.\n3. Remove Speech Artifacts: Strip out filler words (e.g., \"um,\" \"uh,\" \"like\" as filler, \"euh\", \"genre\"), stutters, and false starts.\n4. Fidelity: Preserve the original speaker's exact sentence structure, tone, and word order as closely as possible. Do NOT paraphrase, summarize, or rewrite valid spoken content.\n\nMandatory Transformations:\n- Numbers to Digits (STRICT - ALL NUMBERS MUST BE NUMERIC DIGITS, NEVER LETTERS):\n  - Replace every number, count, or quantity word with its numeric digits without exception:\n    - English: \"one\" → \"1\", \"two\" → \"2\", \"three\" → \"3\", \"four\" → \"4\", \"five\" → \"5\", \"six\" → \"6\", \"seven\" → \"7\", \"eight\" → \"8\", \"nine\" → \"9\"\n    - French: \"un\" / \"une\" → \"1\", \"deux\" → \"2\", \"trois\" → \"3\", \"quatre\" → \"4\", \"cinq\" → \"5\", \"six\" → \"6\", \"sept\" → \"7\", \"huit\" → \"8\", \"neuf\" → \"9\"\n    - Larger & compound numbers: \"ten\" → \"10\", \"twelve\" → \"12\", \"twenty-four\" → \"24\", \"vingt-quatre\" → \"24\", \"quatre-vingt-cinq\" → \"85\", \"sixty thousand\" → \"60,000\", \"cinquante mille\" → \"50 000\"\n  - Currencies to symbols: \"dollars\" → \"$\", \"euros\" → \"€\" (e.g., \"sixty thousand dollars\" → \"$60,000\", \"cinquante euros\" → \"50 €\")\n  - Percentages to symbols: \"percent\" / \"pour cent\" → \"%\" (e.g., \"twelve percent\" → \"12%\", \"85 pour cent\" → \"85%\")\n  - Times to digits: \"ten thirty AM\" → \"10:30 AM\", \"quatorze heures trente\" → \"14h30\"\n- Spoken Punctuation to Marks (MANDATORY):\n  - Convert ALL spoken punctuation words directly into punctuation marks:\n    - \"point\" → \".\"\n    - \"virgule\" → \",\"\n    - \"point d'interrogation\" → \"?\"\n    - \"point d'exclamation\" → \"!\"\n    - \"period\" → \".\"\n    - \"comma\" → \",\"\n    - \"question mark\" → \"?\"\n    - \"exclamation mark\" → \"!\"\n  - NEVER leave spoken punctuation words in the final output text.\n\nOutput Rules:\n- Return ONLY the cleaned transcript.\n- Do NOT include any preamble, introductory text, markdown code blocks, quotes, backticks, or commentary (e.g., do NOT write \"Here is the cleaned transcript:\").\n- Never put quotes, backticks, or decorators around the output text.\n\n---\n\nTranscript:\n\"\"\"\n${output}\n\"\"\"".to_string(),
    }]
}

fn default_multi_stt_merge_prompt() -> Option<LLMPrompt> {
    Some(LLMPrompt {
        id: "default_merge_and_clean".to_string(),
        name: "Merge and Clean".to_string(),
        prompt: "Role: You are an expert multi-source Speech-to-Text (STT) consensus and transcript refinement engine. Your task is to compare  up to 4 different STT transcripts of the exact same audio, merge them into a single accurate transcript, and clean the text according to strict formatting rules.\n\nCore Objective:\nAnalyze Transcriptions 1, 2, 3 and 4. Reconcile differences between them using contextual logic, phonetic similarity, and majority consensus to reconstruct the single most accurate version of what was spoken.\n\n1. Consensus & Merge Logic:\n- Discrepancy Resolution: When the (up to) 4 transcripts disagree on a word or phrase, select the version that makes the most sense grammatically and contextually in the original language.\n- Majority Voting: If 2 of the 4 transcripts agree on a word/phrase and it fits logically, favor these readings unless it is an obvious shared STT misrecognition.\n- Hallucinations & Omissions: Ignore individual model hallucinations, random character glitches, or missing words if the other transcripts provide a coherent sentence.\n\n2. Mandatory Transformations:\n- Numbers to Digits (STRICT - ALL NUMBERS MUST BE NUMERIC DIGITS, NEVER LETTERS):\n  - Replace every number, count, or quantity word with its numeric digits without exception:\n    - English: \"one\" → \"1\", \"two\" → \"2\", \"three\" → \"3\", \"four\" → \"4\", \"five\" → \"5\", \"six\" → \"6\", \"seven\" → \"7\", \"eight\" → \"8\", \"nine\" → \"9\"\n    - French: \"un\" / \"une\" → \"1\", \"deux\" → \"2\", \"trois\" → \"3\", \"quatre\" → \"4\", \"cinq\" → \"5\", \"six\" → \"6\", \"sept\" → \"7\", \"huit\" → \"8\", \"neuf\" → \"9\"\n    - Larger & compound numbers: \"ten\" → \"10\", \"twelve\" → \"12\", \"twenty-four\" → \"24\", \"vingt-quatre\" → \"24\", \"quatre-vingt-cinq\" → \"85\", \"sixty thousand\" → \"60,000\", \"cinquante mille\" → \"50 000\"\n  - Currencies to symbols: \"dollars\" → \"$\", \"euros\" → \"€\" (e.g., \"sixty thousand dollars\" → \"$60,000\", \"50 euros\" → \"50 €\")\n  - Percentages to symbols: \"percent\" / \"pour cent\" → \"%\" (e.g., \"twelve percent\" → \"12%\", \"85 pour cent\" → \"85%\")\n  - Times to digits: \"ten thirty AM\" → \"10:30 AM\", \"quatorze heures\" → \"14h00\"\n- Spoken Punctuation to Marks:\n  - Convert spoken punctuation words directly to punctuation marks: \"point\" → \".\", \"virgule\" → \",\", \"point d'interrogation\" → \"?\", \"point d'exclamation\" → \"!\", \"period\" → \".\", \"comma\" → \",\", \"question mark\" → \"?\", \"exclamation mark\" → \"!\"\n  - NEVER leave spoken punctuation words in the final output text.\n- Language Retention:\n  - Keep French sentences strictly in French and English sentences strictly in English. Never translate.\n- Output Constraints:\n  - Return ONLY the final merged and cleaned transcript.\n  - Do NOT output preamble, markdown quotes, backticks, or commentary.\n\nWord Error Rate = WER\n\n---\n\nTranscription 1 ( up to 8% WER WORST SCORE, TRUST LAST ) :\n\"\"\"\n${output1}\n\"\"\"\n\nTranscription 2 ( 1.3% WER BEST SCORE, TRUST 1ST ) :\n\"\"\"\n${output2}\n\"\"\"\n\nTranscription 3 ( 1.6% WER SCORE, TRUST 2ND ) :\n\"\"\"\n${output3}\n\"\"\"\n\nTranscription 4 ( 1.9% WER SCORE, TRUST 3RD ) :\n\"\"\"\n${output4}\n\"\"\"\n\nALWAYS MAKE A MIX OF THE 4 Transcripts, don't keep 1 specifically, make a union of them all, while keeping the most logical output for the language chosen, it needs to make sense. Do not include the mistakes or the misspelling or some random words that were miss-transcribed or miss heard.".to_string(),
    })
}

fn default_transcribe_gpu_device() -> Option<String> {
    None // automatic device selection
}

/// Accept the 0.1-era integer registry index long enough for the schema
/// migration to clear it. Device indices are process-local in transcribe.cpp
/// 0.2 and must never be carried across launches.
fn deserialize_transcribe_gpu_device<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    match Option::<serde_json::Value>::deserialize(deserializer)? {
        None => Ok(None),
        Some(serde_json::Value::String(value)) => Ok(Some(value)),
        Some(serde_json::Value::Number(_)) => Ok(None),
        Some(_) => Err(de::Error::custom(
            "transcribe GPU device must be a string, integer, or null",
        )),
    }
}

fn default_typing_tool() -> TypingTool {
    TypingTool::Auto
}

fn ensure_post_process_defaults(settings: &mut AppSettings) -> bool {
    let mut changed = false;
    for provider in default_post_process_providers() {
        // Use match to do a single lookup - either sync existing or add new
        match settings
            .post_process_providers
            .iter_mut()
            .find(|p| p.id == provider.id)
        {
            Some(existing) => {
                // Sync supports_structured_output field for existing providers (migration)
                if existing.supports_structured_output != provider.supports_structured_output {
                    debug!(
                        "Updating supports_structured_output for provider '{}' from {} to {}",
                        provider.id,
                        existing.supports_structured_output,
                        provider.supports_structured_output
                    );
                    existing.supports_structured_output = provider.supports_structured_output;
                    changed = true;
                }
            }
            None => {
                // Provider doesn't exist, add it
                settings.post_process_providers.push(provider.clone());
                changed = true;
            }
        }

        if !settings.post_process_api_keys.contains_key(&provider.id) {
            settings
                .post_process_api_keys
                .insert(provider.id.clone(), String::new());
            changed = true;
        }

        let default_model = default_model_for_provider(&provider.id);
        match settings.post_process_models.get_mut(&provider.id) {
            Some(existing) => {
                if existing.is_empty() && !default_model.is_empty() {
                    *existing = default_model.clone();
                    changed = true;
                }
            }
            None => {
                settings
                    .post_process_models
                    .insert(provider.id.clone(), default_model);
                changed = true;
            }
        }
    }

    changed
}

pub const SETTINGS_STORE_PATH: &str = "settings_store.json";

pub fn get_default_settings() -> AppSettings {
    #[cfg(target_os = "windows")]
    let default_shortcut = "ctrl+space";
    #[cfg(target_os = "macos")]
    let default_shortcut = "option+space";
    #[cfg(target_os = "linux")]
    let default_shortcut = "ctrl+space";
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    let default_shortcut = "alt+space";

    #[cfg(target_os = "windows")]
    let default_multi_stt_shortcut = "ctrl+alt+space";
    // `option` and `alt` are the same modifier on macOS, so the old
    // "option+alt+space" default was really just "option+space" â€” identical to
    // the primary transcribe shortcut.
    #[cfg(target_os = "macos")]
    let default_multi_stt_shortcut = "ctrl+option+space";
    #[cfg(target_os = "linux")]
    let default_multi_stt_shortcut = "ctrl+alt+space";
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    let default_multi_stt_shortcut = "alt+shift+space";

    let mut bindings = HashMap::new();
    bindings.insert(
        "transcribe".to_string(),
        ShortcutBinding {
            id: "transcribe".to_string(),
            name: "Transcribe".to_string(),
            description: "Converts your speech into text.".to_string(),
            default_binding: default_shortcut.to_string(),
            current_binding: String::new(),
        },
    );
    #[cfg(target_os = "windows")]
    let default_post_process_shortcut = "ctrl+shift+space";
    #[cfg(target_os = "macos")]
    let default_post_process_shortcut = "option+shift+space";
    #[cfg(target_os = "linux")]
    let default_post_process_shortcut = "ctrl+shift+space";
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    let default_post_process_shortcut = "alt+shift+space";

    bindings.insert(
        "transcribe_with_post_process".to_string(),
        ShortcutBinding {
            id: "transcribe_with_post_process".to_string(),
            name: "Transcribe with Post-Processing".to_string(),
            description: "Converts your speech into text and applies AI post-processing."
                .to_string(),
            default_binding: default_post_process_shortcut.to_string(),
            current_binding: default_post_process_shortcut.to_string(),
        },
    );
    bindings.insert(
        "cancel".to_string(),
        ShortcutBinding {
            id: "cancel".to_string(),
            name: "Cancel".to_string(),
            description: "Cancels the current recording.".to_string(),
            default_binding: "escape".to_string(),
            current_binding: "escape".to_string(),
        },
    );
    bindings.insert(
        "multi_stt_transcribe".to_string(),
        ShortcutBinding {
            id: "multi_stt_transcribe".to_string(),
            name: "Multi STT Transcribe".to_string(),
            description:
                "Transcribes with multiple STT models simultaneously and merges the results."
                    .to_string(),
            default_binding: default_multi_stt_shortcut.to_string(),
            current_binding: String::new(),
        },
    );

    AppSettings {
        settings_schema_version: default_settings_schema_version(),
        bindings,
        shortcut_activation: ShortcutActivation::default(),
        hold_threshold_ms: default_hold_threshold_ms(),
        audio_feedback: false,
        audio_feedback_volume: default_audio_feedback_volume(),
        sound_theme: default_sound_theme(),
        start_hidden: default_start_hidden(),
        autostart_enabled: default_autostart_enabled(),
        update_checks_enabled: default_update_checks_enabled(),
        show_whats_new_on_update: default_show_whats_new_on_update(),
        whats_new_last_seen_version: default_whats_new_last_seen_version(),
        selected_model: "".to_string(),
        onboarding_completed: false,
        always_on_microphone: false,
        selected_microphone: None,
        selected_channel: None,
        clamshell_microphone: None,
        selected_output_device: None,
        translate_to_english: false,
        selected_language: "auto".to_string(),
        overlay_position: default_overlay_position(),
        recording_overlay_use_manual_position: false,
        recording_overlay_has_saved_custom_position: false,
        recording_overlay_manual_position_uses_physical_px: false,
        recording_overlay_custom_x_px: 0,
        recording_overlay_custom_y_px: 0,
        overlay_window_fade_ms: default_overlay_window_fade_ms(),
        overlay_window_corner_radius: default_overlay_window_corner_radius(),
        debug_mode: false,
        log_level: default_log_level(),
        custom_words: Vec::new(),
        model_unload_timeout: ModelUnloadTimeout::default(),
        word_correction_threshold: default_word_correction_threshold(),
        history_limit: default_history_limit(),
        recording_retention_period: default_recording_retention_period(),
        save_raw_audio: default_save_raw_audio(),
        paste_method: PasteMethod::default(),
        clipboard_handling: ClipboardHandling::default(),
        auto_submit: default_auto_submit(),
        auto_submit_key: AutoSubmitKey::default(),
        post_process_enabled: default_post_process_enabled(),
        post_process_provider_id: default_post_process_provider_id(),
        post_process_providers: default_post_process_providers(),
        post_process_api_keys: default_post_process_api_keys(),
        post_process_models: default_post_process_models(),
        post_process_prompts: default_post_process_prompts(),
        post_process_selected_prompt_id: None,
        mute_while_recording: false,
        append_trailing_space: false,
        append_trailing_newline: false,
        app_language: default_app_language(),
        theme: default_theme(),
        custom_accent_color: None,
        ui_scale: default_ui_scale(),
        experimental_enabled: false,
        lazy_stream_close: false,
        keyboard_implementation: KeyboardImplementation::default(),
        show_tray_icon: default_show_tray_icon(),
        paste_delay_ms: default_paste_delay_ms(),
        paste_delay_after_ms: default_paste_delay_after_ms(),
        reliable_paste: false,
        typing_tool: default_typing_tool(),
        external_script_path: None,
        filler_word_removal_enabled: default_filler_word_removal_enabled(),
        custom_filler_words: None,
        transcribe_accelerator: TranscribeAcceleratorSetting::default(),
        transcribe_gpu_device: default_transcribe_gpu_device(),
        extra_recording_buffer_ms: 0,
        vad_enabled: default_vad_enabled(),
        denoise_enabled: false,
        denoise_strength: default_denoise_strength(),
        denoise_vad_threshold: 0.0,
        denoise_vad_grace_ms: default_denoise_vad_grace_ms(),
        vad_threshold_earshot: default_vad_threshold_earshot(),
        overlay_style: default_overlay_style(),
        overlay_direct_mode: false,
        overlay_direct_speed: default_overlay_direct_speed(),
        overlay_back_correction: false,
        direct_streaming_speed: default_direct_streaming_speed(),
        overlay_speech_stats: default_overlay_speech_stats(),
        speech_pause_hold_ms: default_speech_pause_hold_ms(),
        multi_stt_enabled: false,
        multi_stt_model_2: None,
        multi_stt_model_3: None,
        multi_stt_model_4: None,
        multi_stt_language_model_2: None,
        multi_stt_language_model_3: None,
        multi_stt_language_model_4: None,
        multi_stt_translate_model_2: false,
        multi_stt_translate_model_3: false,
        multi_stt_translate_model_4: false,
        multi_stt_keep_extra_models_loaded: true,
        multi_stt_merge_prompt: default_multi_stt_merge_prompt(),
        multi_stt_performance_mode_enabled: false,
        multi_stt_performance_mode_trigger_on_start: false,
        multi_stt_performance_mode_full_power_shortcut: default_multi_stt_full_power_shortcut(),
        multi_stt_performance_mode_normal_shortcut: default_multi_stt_normal_shortcut(),
        multi_stt_streaming_first_enabled: false,
        multi_stt_streaming_pause_ms: default_multi_stt_streaming_pause_ms(),
        multi_stt_streaming_context_chunks: default_multi_stt_streaming_context_chunks(),
        mic_idle_timeout_value: default_mic_idle_timeout_value(),
        mic_idle_timeout_unit: MicIdleTimeoutUnit::default(),
        mic_idle_infinite: false,
        native_streaming_latency_presets: HashMap::new(),
        native_streaming_chunk_ms: HashMap::new(),
        file_transcription: FileTranscriptionSettings::default(),
        llama: LlamaSettings::default(),
        live_mode: LiveModeSettings::default(),
        recall: RecallSettings::default(),
        live_fft: LiveFftSettings::default(),
        overlay_scope: OverlayScopeSettings::default(),
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        get_default_settings()
    }
}

impl AppSettings {
    pub fn active_post_process_provider(&self) -> Option<&PostProcessProvider> {
        self.post_process_providers
            .iter()
            .find(|provider| provider.id == self.post_process_provider_id)
    }

    pub fn post_process_provider(&self, provider_id: &str) -> Option<&PostProcessProvider> {
        self.post_process_providers
            .iter()
            .find(|provider| provider.id == provider_id)
    }

    pub fn post_process_provider_mut(
        &mut self,
        provider_id: &str,
    ) -> Option<&mut PostProcessProvider> {
        self.post_process_providers
            .iter_mut()
            .find(|provider| provider.id == provider_id)
    }
}

/// Startup entry point. Same load-or-create/salvage/migrate behavior as
/// `get_settings`; kept as a named alias for call-site clarity, plus a
/// one-time debug dump of the loaded settings.
pub fn load_or_create_app_settings(app: &AppHandle) -> AppSettings {
    let settings = get_settings(app);
    // The struct is several kilobytes on one line â€” every binding, provider,
    // prompt and nested settings group â€” so dumping it whole buries the startup
    // console under two walls of text and the lines that matter scroll away.
    // The summary is what a reader wants at DEBUG; the dump is one level down,
    // for when one field's value is the actual question.
    debug!(
        "Loaded settings: schema {}, model '{}', log level {:?}, {} binding(s), multi-STT {}, \
         streaming-first {}",
        settings.settings_schema_version,
        settings.selected_model,
        settings.log_level,
        settings.bindings.len(),
        settings.multi_stt_enabled,
        settings.multi_stt_streaming_first_enabled
    );
    trace!("Loaded settings: {:?}", settings);
    settings
}

pub fn get_settings(app: &AppHandle) -> AppSettings {
    let store = app
        .store(crate::portable::store_path(SETTINGS_STORE_PATH))
        .expect("Failed to initialize store");

    // Settings reads also persist one-time migrations. Migration helpers are
    // idempotent, so this converges after the first read of an older store.
    let mut settings = if let Some(settings_value) = store.get("settings") {
        let (mut settings, mut updated) =
            match serde_json::from_value::<AppSettings>(settings_value.clone()) {
                Ok(settings) => (settings, false),
                Err(e) => {
                    warn!("Failed to parse stored settings ({e}); salvaging valid fields");
                    (salvage_settings(&settings_value), true)
                }
            };

        if apply_settings_migrations(&mut settings, &settings_value) {
            updated = true;
        }

        // Merge in any bindings added since this store was written.
        for (key, value) in get_default_settings().bindings {
            if let std::collections::hash_map::Entry::Vacant(entry) = settings.bindings.entry(key) {
                debug!("Adding missing binding: {}", entry.key());
                entry.insert(value);
                updated = true;
            }
        }

        if updated {
            store.set("settings", serde_json::to_value(&settings).unwrap());
        }

        settings
    } else {
        let default_settings = get_default_settings();
        store.set("settings", serde_json::to_value(&default_settings).unwrap());
        default_settings
    };

    if ensure_post_process_defaults(&mut settings) {
        store.set("settings", serde_json::to_value(&settings).unwrap());
    }

    settings
}

/// Rebuilds settings from a store value that failed to deserialize as a whole.
/// Every stored field that is individually valid is kept; only broken values
/// (e.g. an enum variant written by a newer or older version) fall back to
/// their default. This means one bad field can never reset the rest of the
/// user's configuration (#1619).
fn salvage_settings(stored: &serde_json::Value) -> AppSettings {
    let Some(stored_map) = stored.as_object() else {
        warn!("Stored settings are not a JSON object; falling back to defaults");
        return get_default_settings();
    };

    let mut merged = serde_json::to_value(get_default_settings())
        .expect("default settings serialize to a JSON object");

    for (key, value) in stored_map {
        let previous = merged
            .as_object_mut()
            .expect("merged settings stay an object")
            .insert(key.clone(), value.clone());
        if serde_json::from_value::<AppSettings>(merged.clone()).is_err() {
            // Log only the key: values may hold secrets (e.g. API keys).
            warn!("Dropping invalid settings field '{key}', keeping its default");
            let map = merged
                .as_object_mut()
                .expect("merged settings stay an object");
            match previous {
                Some(previous) => map.insert(key.clone(), previous),
                None => map.remove(key),
            };
        }
    }

    serde_json::from_value(merged).unwrap_or_else(|e| {
        warn!("Failed to reassemble salvaged settings ({e}); falling back to defaults");
        get_default_settings()
    })
}

fn apply_settings_migrations(
    settings: &mut AppSettings,
    settings_value: &serde_json::Value,
) -> bool {
    let mut updated = false;

    // One-time onboarding migration: users with an explicit selected model have
    // already made it through model selection. Users who merely have compatible
    // files on disk should still see onboarding.
    if settings_value.get("onboarding_completed").is_none() {
        settings.onboarding_completed = !settings.selected_model.is_empty();
        updated = true;
    }

    // One-time What's New migration: migrations only run on an existing store
    // (fresh installs stamp the current version via get_default_settings). A
    // missing key here means a user upgrading from before it existed â€” blank it
    // so they see the current release's What's New, mirroring the onboarding
    // migration's explicit first-run-vs-upgrade decision.
    if settings_value.get("whats_new_last_seen_version").is_none() {
        settings.whats_new_last_seen_version = String::new();
        updated = true;
    }

    // One-time shortcut activation migration (only while the new key is
    // absent): the retired `push_to_talk` bool maps onto the two legacy modes so
    // upgrading users keep exactly the behavior they had. Only fresh installs
    // get the hold-or-toggle default.
    if settings_value.get("shortcut_activation").is_none()
        && let Some(push_to_talk) = settings_value.get("push_to_talk").and_then(|v| v.as_bool())
    {
        settings.shortcut_activation = if push_to_talk {
            ShortcutActivation::PushToTalk
        } else {
            ShortcutActivation::Toggle
        };
        updated = true;
    }

    let stored_schema_version = settings_value
        .get("settings_schema_version")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    if stored_schema_version < 1 {
        // Before schema 1 this was a UI ordinal. Preserve the original safety
        // migration: a positive selection was ambiguous even in 0.1.
        let had_positive_legacy_selection = settings_value
            .get("transcribe_gpu_device")
            .and_then(|value| value.as_i64())
            .is_some_and(|value| value > 0);
        if had_positive_legacy_selection {
            settings.transcribe_accelerator = TranscribeAcceleratorSetting::Auto;
        }
    }
    if stored_schema_version < 2 {
        // transcribe.cpp 0.2 replaced integer registry indices with opaque
        // process-local handles. Clear every old index once; keeping the GPU
        // accelerator preference for schema-1 users preserves their intent and
        // lets the backend choose a valid GPU automatically.
        settings.transcribe_gpu_device = default_transcribe_gpu_device();
        settings.settings_schema_version = 2;
        updated = true;
    }

    // One-time overlay migration (only while the new key is absent): the retired
    // overlay_position `none` meant "hide the overlay" â†’ OverlayStyle::None; any
    // other position had it visible â†’ Live. The position enum no longer has a
    // `none` variant (legacy "none" deserializes to Bottom via a serde alias), so
    // read the raw stored string to recover the old intent.
    if settings_value.get("overlay_style").is_none() {
        let was_hidden = settings_value
            .get("overlay_position")
            .and_then(|v| v.as_str())
            == Some("none");
        settings.overlay_style = if was_hidden {
            OverlayStyle::None
        } else {
            OverlayStyle::Live
        };
        updated = true;
    }

    // Schema 3: Clear the default global shortcuts for the regular transcription
    // and Multi-STT triggers ("transcribe" and "multi_stt_transcribe"). These
    // were registered globally by default and now conflict with the Multi-STT
    // performance-mode simulated shortcuts (Ctrl+Space / Ctrl+Alt+Space).
    // The default_binding is preserved so the user can manually restore them,
    // but no shortcut is registered until the user explicitly sets one.
    if stored_schema_version < 3 {
        for binding_id in ["transcribe", "multi_stt_transcribe"] {
            if let Some(binding) = settings.bindings.get_mut(binding_id)
                && !binding.current_binding.is_empty()
            {
                debug!(
                    "Schema 3 migration: clearing default binding for '{}'",
                    binding_id
                );
                binding.current_binding = String::new();
                updated = true;
            }
        }
        settings.settings_schema_version = 3;
    }

    // Schema 4: Catch users who already had schema_version 3 (stamped by a
    // build that bumped CURRENT_SETTINGS_SCHEMA_VERSION before adding the
    // schema 3 migration body). For those users the < 3 check above was
    // skipped, so their "transcribe" and "multi_stt_transcribe" bindings
    // still carried the old default shortcuts and would shadow the
    // performance-mode simulated keys. This re-applies the same clearing
    // on top of schema 3, then stamps the version to 4.
    if stored_schema_version < 4 {
        for binding_id in ["transcribe", "multi_stt_transcribe"] {
            if let Some(binding) = settings.bindings.get_mut(binding_id)
                && !binding.current_binding.is_empty()
            {
                debug!(
                    "Schema 4 migration: clearing leftover binding for '{}'",
                    binding_id
                );
                binding.current_binding = String::new();
                updated = true;
            }
        }
        settings.settings_schema_version = 4;
    }

    // Schema 5: Clear transcribe_with_post_process.current_binding when it
    // conflicts with the Multi-STT performance-mode simulated shortcuts
    // (ctrl+space / ctrl+alt+space). The performance mode simulates these
    // keystrokes, so any global shortcut matching them causes recursive
    // triggering. We skip transcribe_with_post_process entirely in shortcut
    // registration now, but we also clear the stored value so the UI doesn't
    // show a misleading binding and so that future registrations (via
    // change_binding_setting) default to a non-conflicting slot.
    if stored_schema_version < 5 {
        let perf_full = settings
            .multi_stt_performance_mode_full_power_shortcut
            .clone();
        let perf_normal = settings.multi_stt_performance_mode_normal_shortcut.clone();
        if let Some(binding) = settings.bindings.get_mut("transcribe_with_post_process") {
            let normalized = normalize_binding(&binding.current_binding);
            if normalized == normalize_binding(&perf_full)
                || normalized == normalize_binding(&perf_normal)
            {
                debug!(
                    "Schema 5 migration: clearing 'transcribe_with_post_process' binding '{}' (conflicts with performance-mode shortcut)",
                    binding.current_binding
                );
                binding.current_binding = String::new();
                updated = true;
            }
        }
        settings.settings_schema_version = 5;
    }

    // Schema 6: the ONNX runtime (transcribe-rs / ort) is gone, and with it
    // the 11 hard-coded ONNX models. Every one of them has a GGUF successor in
    // the bundled catalog, so a selection pointing at a retired id is remapped
    // to that successor instead of silently falling back to "no model". The
    // replacement may not be downloaded yet; the normal "model not downloaded"
    // flow handles that. Model files on disk are never touched.
    if stored_schema_version < 6 {
        if let Some(replacement) = legacy_onnx_model_replacement(&settings.selected_model) {
            info!(
                "Schema 6 migration: selected model '{}' is no longer supported; switching to '{}'",
                settings.selected_model, replacement
            );
            settings.selected_model = replacement;
            updated = true;
        }
        for slot in [
            &mut settings.multi_stt_model_2,
            &mut settings.multi_stt_model_3,
            &mut settings.multi_stt_model_4,
        ] {
            if let Some(replacement) = slot.as_deref().and_then(legacy_onnx_model_replacement) {
                info!(
                    "Schema 6 migration: Multi-STT model '{}' is no longer supported; switching to '{}'",
                    slot.as_deref().unwrap_or_default(),
                    replacement
                );
                *slot = Some(replacement);
                updated = true;
            }
        }
        settings.settings_schema_version = CURRENT_SETTINGS_SCHEMA_VERSION;
    }

    updated
}

/// Catalog successor for a retired hard-coded ONNX model id, or `None` for
/// any other id. The mapping is by architecture: each legacy ONNX bundle has a
/// GGUF conversion of the same weights in the catalog.
pub fn legacy_onnx_model_replacement(model_id: &str) -> Option<String> {
    let repo = match model_id {
        "parakeet-tdt-0.6b-v2" => "handy-computer/parakeet-tdt-0.6b-v2-gguf",
        "parakeet-tdt-0.6b-v3" => "handy-computer/parakeet-tdt-0.6b-v3-gguf",
        "moonshine-base" => "handy-computer/moonshine-base-gguf",
        "moonshine-tiny-streaming-en" => "handy-computer/moonshine-streaming-tiny-gguf",
        "moonshine-small-streaming-en" => "handy-computer/moonshine-streaming-small-gguf",
        "moonshine-medium-streaming-en" => "handy-computer/moonshine-streaming-medium-gguf",
        "sense-voice-int8" => "handy-computer/SenseVoiceSmall-gguf",
        "gigaam-v3-e2e-ctc" => "handy-computer/gigaam-v3-e2e-ctc-gguf",
        "canary-180m-flash" => "handy-computer/canary-180m-flash-gguf",
        "canary-1b-v2" => "handy-computer/canary-1b-v2-gguf",
        "cohere-int8" => "handy-computer/cohere-transcribe-03-2026-gguf",
        _ => return None,
    };
    crate::catalog::default_id_for_repo(repo)
}

/// Normalize a hotkey string for comparison: lowercase, sort modifier
/// tokens, strip `_left`/`_right` suffixes. E.g. "ctrl_left+alt_left+space"
/// becomes "alt+ctrl+space" â€” same as "ctrl+alt+space".
pub fn normalize_binding(s: &str) -> String {
    let parts: Vec<&str> = s.split('+').map(|p| p.trim()).collect();
    let mut normalized: Vec<String> = parts
        .iter()
        .map(|p| {
            let lower = p.to_lowercase();
            if lower.ends_with("_left") {
                lower.trim_end_matches("_left").to_string()
            } else if lower.ends_with("_right") {
                lower.trim_end_matches("_right").to_string()
            } else {
                lower
            }
        })
        .collect();
    normalized.sort();
    normalized.join("+")
}

/// Update checks are forced off (without touching the persisted setting) when
/// the `DISABLE_UPDATER` flag is set â€” e.g. by the Nix package, since
/// self-update can't work against an immutable /nix/store install. The full
/// variable name is `{ENV_PREFIX}DISABLE_UPDATER`; the pre-0.9.7
/// `HANDY_DISABLE_UPDATER` is still honoured.
pub fn update_checks_forced_disabled() -> bool {
    use std::sync::OnceLock;
    static IS_UPDATER_DISABLED: OnceLock<bool> = OnceLock::new();
    *IS_UPDATER_DISABLED.get_or_init(|| utils::app_env_flag("DISABLE_UPDATER"))
}

/// Effective updater state: the user's stored preference, overridden to `false`
/// while the disable-updater flag is set. Callers deciding whether to actually
/// check for updates must use this rather than reading `update_checks_enabled`
/// directly, so the forced-off state never leaks into the persisted setting.
pub fn update_checks_effectively_enabled(settings: &AppSettings) -> bool {
    settings.update_checks_enabled && !update_checks_forced_disabled()
}

pub fn write_settings(app: &AppHandle, settings: AppSettings) {
    let store = app
        .store(crate::portable::store_path(SETTINGS_STORE_PATH))
        .expect("Failed to initialize store");

    store.set("settings", serde_json::to_value(&settings).unwrap());
}

pub fn get_bindings(app: &AppHandle) -> HashMap<String, ShortcutBinding> {
    let settings = get_settings(app);

    settings.bindings
}

pub fn get_stored_binding(app: &AppHandle, id: &str) -> ShortcutBinding {
    let bindings = get_bindings(app);
    bindings.get(id).unwrap().clone()
}

pub fn get_history_limit(app: &AppHandle) -> u32 {
    let settings = get_settings(app);
    settings.history_limit
}

pub fn get_recording_retention_period(app: &AppHandle) -> RecordingRetentionPeriod {
    let settings = get_settings(app);
    settings.recording_retention_period
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_settings_json() -> serde_json::Value {
        serde_json::to_value(get_default_settings()).unwrap()
    }

    /// Every field must survive a partial store: a missing key must never fail
    /// the whole-settings parse (#1619). `json!({})` is the extreme case.
    #[test]
    fn empty_store_parses_with_defaults() {
        let settings: AppSettings = serde_json::from_value(serde_json::json!({}))
            .expect("all AppSettings fields need serde defaults");
        assert_eq!(
            settings.shortcut_activation,
            ShortcutActivation::HoldOrToggle
        );
        assert_eq!(settings.hold_threshold_ms, default_hold_threshold_ms());
        assert!(!settings.audio_feedback);
        assert!(settings.filler_word_removal_enabled);
        // Bindings default to empty; the load path merges the real defaults in.
        assert!(settings.bindings.is_empty());
    }

    /// Frozen snapshot of a real v0.9.0-era settings store, as written to
    /// disk. This pins backwards compatibility: it must always parse strictly
    /// (no salvage). Schema migrations may then rewrite fields whose native
    /// meaning changed.
    ///
    /// If a schema change breaks this test, do NOT just update the fixture â€”
    /// it stands in for the stores on users' machines. Add a
    /// `#[serde(alias)]`/`#[serde(other)]` or a one-time migration in
    /// `apply_settings_migrations` so old values keep loading, and only extend
    /// the fixture alongside that.
    #[test]
    fn frozen_v0_9_store_parses_strictly_then_migrates_device_index() {
        // Note "log_level": 2 â€” the legacy numeric format, kept deliberately.
        let stored: serde_json::Value = serde_json::from_str(
            r##"{
            "settings_schema_version": 1,
            "bindings": {
                "transcribe": {
                    "id": "transcribe",
                    "name": "Transcribe",
                    "description": "Converts your speech into text.",
                    "default_binding": "option+space",
                    "current_binding": "f13"
                },
                "transcribe_with_post_process": {
                    "id": "transcribe_with_post_process",
                    "name": "Transcribe with Post-Processing",
                    "description": "Converts your speech into text and applies AI post-processing.",
                    "default_binding": "option+shift+space",
                    "current_binding": "option+shift+space"
                },
                "cancel": {
                    "id": "cancel",
                    "name": "Cancel",
                    "description": "Cancels the current recording.",
                    "default_binding": "escape",
                    "current_binding": "escape"
                }
            },
            "push_to_talk": false,
            "audio_feedback": true,
            "audio_feedback_volume": 0.8,
            "sound_theme": "pop",
            "start_hidden": false,
            "autostart_enabled": true,
            "update_checks_enabled": true,
            "show_whats_new_on_update": true,
            "whats_new_last_seen_version": "0.9.0",
            "selected_model": "whisper-large-v3-turbo",
            "onboarding_completed": true,
            "always_on_microphone": false,
            "selected_microphone": "MacBook Pro Microphone",
            "clamshell_microphone": null,
            "selected_output_device": null,
            "translate_to_english": false,
            "selected_language": "en",
            "overlay_position": "bottom",
            "debug_mode": false,
            "log_level": 2,
            "custom_words": ["NeMo", "parakeet"],
            "model_unload_timeout": "min5",
            "word_correction_threshold": 0.18,
            "history_limit": 5,
            "recording_retention_period": "preserve_limit",
            "paste_method": "ctrl_v",
            "clipboard_handling": "dont_modify",
            "auto_submit": false,
            "auto_submit_key": "enter",
            "post_process_enabled": false,
            "post_process_provider_id": "openai",
            "post_process_providers": [
                {
                    "id": "openai",
                    "label": "OpenAI",
                    "base_url": "https://api.openai.com/v1",
                    "allow_base_url_edit": false,
                    "models_endpoint": null,
                    "supports_structured_output": true
                }
            ],
            "post_process_api_keys": { "openai": "" },
            "post_process_models": { "openai": "gpt-4o-mini" },
            "post_process_prompts": [
                { "id": "default", "name": "Default", "prompt": "Clean up the transcript." }
            ],
            "post_process_selected_prompt_id": null,
            "mute_while_recording": false,
            "append_trailing_space": false,
            "app_language": "en",
            "experimental_enabled": false,
            "lazy_stream_close": false,
            "keyboard_implementation": "handy_keys",
            "show_tray_icon": true,
            "paste_delay_ms": 60,
            "typing_tool": "auto",
            "external_script_path": null,
            "custom_filler_words": null,
            "transcribe_accelerator": "gpu",
            "ort_accelerator": "auto",
            "transcribe_gpu_device": 0,
            "extra_recording_buffer_ms": 0,
            "vad_enabled": true,
            "overlay_style": "live"
        }"##,
        )
        .expect("fixture is valid JSON");

        let mut settings: AppSettings = serde_json::from_value(stored.clone())
            .expect("a stored v0.9.0 settings object must keep parsing strictly");

        assert_eq!(settings.selected_model, "whisper-large-v3-turbo");
        assert_eq!(settings.bindings["transcribe"].current_binding, "f13");
        assert_eq!(settings.log_level, LogLevel::Debug);
        assert_eq!(settings.sound_theme, SoundTheme::Pop);
        assert!(settings.filler_word_removal_enabled);

        // The 0.1 integer device index is cleared once for transcribe.cpp 0.2,
        // while preserving this user's explicit GPU accelerator preference.
        assert!(apply_settings_migrations(&mut settings, &stored));
        assert_eq!(
            settings.settings_schema_version,
            CURRENT_SETTINGS_SCHEMA_VERSION
        );
        assert_eq!(
            settings.transcribe_accelerator,
            TranscribeAcceleratorSetting::Gpu
        );
        // The retired push_to_talk bool (false in this fixture) becomes the
        // matching legacy mode rather than the new hold-or-toggle default.
        assert_eq!(settings.shortcut_activation, ShortcutActivation::Toggle);
        assert_eq!(settings.transcribe_gpu_device, None);

        // Schema 3 clears the default global shortcuts for "transcribe" and
        // "multi_stt_transcribe" to avoid conflicts with the Multi-STT
        // performance-mode simulated shortcuts.
        assert_eq!(settings.bindings["transcribe"].current_binding, "");
    }

    #[test]
    fn schema_6_remaps_retired_onnx_models_to_catalog_gguf() {
        let mut stored = default_settings_json();
        let map = stored.as_object_mut().unwrap();
        map.insert("settings_schema_version".into(), serde_json::json!(5));
        map.insert(
            "selected_model".into(),
            serde_json::json!("sense-voice-int8"),
        );
        map.insert(
            "multi_stt_model_2".into(),
            serde_json::json!("moonshine-base"),
        );
        map.insert(
            "multi_stt_model_3".into(),
            serde_json::json!("canary-1b-v2"),
        );
        map.insert(
            "multi_stt_model_4".into(),
            serde_json::json!(
                "handy-computer/whisper-large-v3-turbo-gguf/whisper-large-v3-turbo-Q8_0.gguf"
            ),
        );

        let mut settings: AppSettings = serde_json::from_value(stored.clone()).unwrap();
        assert!(apply_settings_migrations(&mut settings, &stored));
        assert_eq!(
            settings.settings_schema_version,
            CURRENT_SETTINGS_SCHEMA_VERSION
        );
        assert_eq!(
            settings.selected_model,
            "handy-computer/SenseVoiceSmall-gguf/SenseVoiceSmall-Q8_0.gguf"
        );
        assert_eq!(
            settings.multi_stt_model_2.as_deref(),
            Some("handy-computer/moonshine-base-gguf/moonshine-base-Q8_0.gguf")
        );
        assert_eq!(
            settings.multi_stt_model_3.as_deref(),
            Some("handy-computer/canary-1b-v2-gguf/canary-1b-v2-Q5_K_M.gguf")
        );
        // Catalog ids are left alone.
        assert_eq!(
            settings.multi_stt_model_4.as_deref(),
            Some("handy-computer/whisper-large-v3-turbo-gguf/whisper-large-v3-turbo-Q8_0.gguf")
        );
        // Every retired id resolves to a catalog entry that actually exists.
        for legacy in [
            "parakeet-tdt-0.6b-v2",
            "parakeet-tdt-0.6b-v3",
            "moonshine-base",
            "moonshine-tiny-streaming-en",
            "moonshine-small-streaming-en",
            "moonshine-medium-streaming-en",
            "sense-voice-int8",
            "gigaam-v3-e2e-ctc",
            "canary-180m-flash",
            "canary-1b-v2",
            "cohere-int8",
        ] {
            let replacement = legacy_onnx_model_replacement(legacy)
                .unwrap_or_else(|| panic!("{legacy} has no catalog replacement"));
            assert!(
                crate::catalog::CATALOG.iter().any(|d| d.id == replacement),
                "{legacy} -> {replacement} is not a catalog id"
            );
        }
        assert_eq!(
            legacy_onnx_model_replacement("whisper-large-v3-turbo"),
            None
        );
    }

    #[test]
    fn salvage_preserves_valid_fields_when_one_value_is_invalid() {
        let mut stored = default_settings_json();
        let map = stored.as_object_mut().unwrap();
        map.insert(
            "selected_model".into(),
            serde_json::json!("parakeet-tdt-0.6b-v3"),
        );
        map.insert("onboarding_completed".into(), serde_json::json!(true));
        // An enum variant this build doesn't know, e.g. written by a newer
        // version before a downgrade.
        map.insert("sound_theme".into(), serde_json::json!("theremin"));
        stored["bindings"]["transcribe"]["current_binding"] = serde_json::json!("f13");

        // Precondition: this is exactly the whole-store parse failure from
        // #1619 that used to reset everything to defaults.
        assert!(serde_json::from_value::<AppSettings>(stored.clone()).is_err());

        let salvaged = salvage_settings(&stored);
        assert_eq!(salvaged.selected_model, "parakeet-tdt-0.6b-v3");
        assert!(salvaged.onboarding_completed);
        assert_eq!(salvaged.bindings["transcribe"].current_binding, "f13");
        assert_eq!(salvaged.sound_theme, default_sound_theme());
    }

    #[test]
    fn salvage_drops_only_wrong_typed_fields() {
        let mut stored = default_settings_json();
        let map = stored.as_object_mut().unwrap();
        map.insert("paste_delay_ms".into(), serde_json::json!("sixty"));
        map.insert("sound_theme".into(), serde_json::json!(42));
        map.insert("custom_words".into(), serde_json::json!(["nemo"]));

        assert!(serde_json::from_value::<AppSettings>(stored.clone()).is_err());

        let salvaged = salvage_settings(&stored);
        assert_eq!(salvaged.paste_delay_ms, default_paste_delay_ms());
        assert_eq!(salvaged.sound_theme, default_sound_theme());
        assert_eq!(salvaged.custom_words, vec!["nemo".to_string()]);
    }

    #[test]
    fn salvage_of_poisoned_bindings_keeps_other_fields() {
        let mut stored = default_settings_json();
        let map = stored.as_object_mut().unwrap();
        // One malformed entry poisons the whole bindings map, but must not
        // take the rest of the settings down with it.
        map.insert(
            "bindings".into(),
            serde_json::json!({ "transcribe": { "id": 42 } }),
        );
        map.insert("selected_model".into(), serde_json::json!("whisper-small"));

        assert!(serde_json::from_value::<AppSettings>(stored.clone()).is_err());

        let salvaged = salvage_settings(&stored);
        assert_eq!(salvaged.selected_model, "whisper-small");
        let defaults = get_default_settings();
        assert_eq!(
            salvaged.bindings["transcribe"].current_binding,
            defaults.bindings["transcribe"].current_binding
        );
    }

    #[test]
    fn salvage_tolerates_unknown_keys() {
        let mut stored = default_settings_json();
        let map = stored.as_object_mut().unwrap();
        map.insert(
            "field_from_the_future".into(),
            serde_json::json!({ "nested": true }),
        );
        map.insert("selected_model".into(), serde_json::json!("kept"));
        map.insert("sound_theme".into(), serde_json::json!("theremin"));

        let salvaged = salvage_settings(&stored);
        assert_eq!(salvaged.selected_model, "kept");
        assert_eq!(salvaged.sound_theme, default_sound_theme());
    }

    #[test]
    fn salvage_of_non_object_store_falls_back_to_defaults() {
        for stored in [
            serde_json::json!("corrupt"),
            serde_json::json!(null),
            serde_json::json!([1, 2, 3]),
        ] {
            let salvaged = salvage_settings(&stored);
            assert_eq!(
                serde_json::to_value(&salvaged).unwrap(),
                default_settings_json()
            );
        }
    }

    #[test]
    fn default_settings_disable_auto_submit() {
        let settings = get_default_settings();
        assert!(!settings.auto_submit);
        assert_eq!(settings.auto_submit_key, AutoSubmitKey::Enter);
        assert_eq!(
            settings.settings_schema_version,
            CURRENT_SETTINGS_SCHEMA_VERSION
        );
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn default_overlay_style_is_live_when_overlay_defaults_on() {
        let settings = get_default_settings();
        assert_eq!(settings.overlay_style, OverlayStyle::Live);
    }

    #[test]
    fn overlay_migration_keeps_disabled_overlay_off() {
        let mut settings = get_default_settings();

        // Legacy store: overlay was hidden via the retired position "none".
        let raw = serde_json::json!({
            "selected_model": "",
            "overlay_position": "none"
        });

        assert!(apply_settings_migrations(&mut settings, &raw));
        assert_eq!(settings.overlay_style, OverlayStyle::None);
    }

    #[test]
    fn legacy_none_overlay_position_deserializes_to_bottom() {
        // A persisted "none" must not fail the whole settings load; the serde
        // alias folds it onto Bottom (visibility is owned by overlay_style).
        let raw = serde_json::json!({ "overlay_position": "none" });
        let position: OverlayPosition =
            serde_json::from_value(raw.get("overlay_position").unwrap().clone())
                .expect("legacy \"none\" should deserialize, not error");
        assert_eq!(position, OverlayPosition::Bottom);
    }

    #[test]
    fn overlay_migration_promotes_enabled_overlay_to_live() {
        let mut settings = get_default_settings();
        settings.overlay_position = OverlayPosition::Top;
        settings.overlay_style = OverlayStyle::Minimal;

        let raw = serde_json::json!({
            "selected_model": "",
            "overlay_position": "top"
        });

        assert!(apply_settings_migrations(&mut settings, &raw));
        assert_eq!(settings.overlay_style, OverlayStyle::Live);
        assert_eq!(settings.overlay_position, OverlayPosition::Top);
    }

    #[test]
    fn shortcut_activation_migration_maps_push_to_talk_true() {
        let mut settings = get_default_settings();
        let raw = serde_json::json!({
            "selected_model": "",
            "push_to_talk": true
        });

        assert!(apply_settings_migrations(&mut settings, &raw));
        assert_eq!(settings.shortcut_activation, ShortcutActivation::PushToTalk);
    }

    #[test]
    fn shortcut_activation_migration_maps_push_to_talk_false() {
        let mut settings = get_default_settings();
        let raw = serde_json::json!({
            "selected_model": "",
            "push_to_talk": false
        });

        assert!(apply_settings_migrations(&mut settings, &raw));
        assert_eq!(settings.shortcut_activation, ShortcutActivation::Toggle);
    }

    #[test]
    fn shortcut_activation_migration_respects_explicit_new_key() {
        let mut settings = get_default_settings();
        settings.shortcut_activation = ShortcutActivation::HoldOrToggle;
        let raw = serde_json::json!({
            "selected_model": "",
            "push_to_talk": true,
            "shortcut_activation": "hold_or_toggle"
        });

        apply_settings_migrations(&mut settings, &raw);
        assert_eq!(
            settings.shortcut_activation,
            ShortcutActivation::HoldOrToggle
        );
    }

    #[test]
    fn shortcut_activation_defaults_to_hold_or_toggle_without_legacy_key() {
        let mut settings = get_default_settings();
        let raw = serde_json::json!({ "selected_model": "" });

        apply_settings_migrations(&mut settings, &raw);
        assert_eq!(
            settings.shortcut_activation,
            ShortcutActivation::HoldOrToggle
        );
    }

    #[test]
    fn gpu_device_migration_resets_legacy_positive_selection_to_auto() {
        let mut settings = get_default_settings();
        settings.transcribe_accelerator = TranscribeAcceleratorSetting::Gpu;

        let raw = serde_json::json!({
            "transcribe_accelerator": "gpu",
            "transcribe_gpu_device": 2
        });

        assert!(apply_settings_migrations(&mut settings, &raw));
        assert_eq!(
            settings.transcribe_accelerator,
            TranscribeAcceleratorSetting::Auto
        );
        assert_eq!(settings.transcribe_gpu_device, None);
        assert_eq!(
            settings.settings_schema_version,
            CURRENT_SETTINGS_SCHEMA_VERSION
        );
    }

    #[test]
    fn gpu_device_migration_clears_v1_index_but_keeps_gpu_preference() {
        let raw = serde_json::json!({
            "settings_schema_version": 1,
            "transcribe_accelerator": "gpu",
            "transcribe_gpu_device": 2
        });
        let mut settings: AppSettings = serde_json::from_value(raw.clone()).unwrap();

        assert!(apply_settings_migrations(&mut settings, &raw));
        assert_eq!(
            settings.transcribe_accelerator,
            TranscribeAcceleratorSetting::Gpu
        );
        assert_eq!(settings.transcribe_gpu_device, None);
    }

    #[test]
    fn gpu_device_migration_keeps_current_stable_selection() {
        let mut settings = get_default_settings();
        settings.transcribe_accelerator = TranscribeAcceleratorSetting::Gpu;
        settings.transcribe_gpu_device = Some("[\"vulkan\",\"id\",\"0000:01:00.0\"]".into());

        let raw = serde_json::json!({
            "settings_schema_version": CURRENT_SETTINGS_SCHEMA_VERSION,
            "onboarding_completed": false,
            "whats_new_last_seen_version": default_whats_new_last_seen_version(),
            "overlay_style": "live",
            "transcribe_accelerator": "gpu",
            "transcribe_gpu_device": settings.transcribe_gpu_device
        });

        assert!(!apply_settings_migrations(&mut settings, &raw));
        assert_eq!(
            settings.transcribe_gpu_device.as_deref(),
            Some("[\"vulkan\",\"id\",\"0000:01:00.0\"]")
        );
    }

    #[test]
    fn debug_output_redacts_api_keys() {
        let mut settings = get_default_settings();
        settings
            .post_process_api_keys
            .insert("openai".to_string(), "sk-proj-secret-key-12345".to_string());
        settings.post_process_api_keys.insert(
            "anthropic".to_string(),
            "sk-ant-secret-key-67890".to_string(),
        );
        settings
            .post_process_api_keys
            .insert("empty_provider".to_string(), "".to_string());

        let debug_output = format!("{:?}", settings);

        assert!(!debug_output.contains("sk-proj-secret-key-12345"));
        assert!(!debug_output.contains("sk-ant-secret-key-67890"));
        assert!(debug_output.contains("[REDACTED]"));
    }

    #[test]
    fn secret_map_debug_redacts_values() {
        let map = SecretMap(HashMap::from([("key".into(), "secret".into())]));
        let out = format!("{:?}", map);
        assert!(!out.contains("secret"));
        assert!(out.contains("[REDACTED]"));
    }

    #[test]
    fn schema_3_migration_clears_transcribe_and_multi_stt_bindings() {
        let mut stored = default_settings_json();
        let map = stored.as_object_mut().unwrap();
        map.insert("settings_schema_version".into(), serde_json::json!(2));
        map.insert(
            "multi_stt_transcribe_binding".into(),
            serde_json::json!("ctrl+alt+space"),
        );
        // Simulate a user who has manually set the global transcribe and
        // multi-stt bindings â€” these should be cleared by the schema 3
        // migration to avoid conflicts with performance-mode simulated
        // shortcuts (ctrl+space / ctrl+alt+space).
        stored["bindings"]["transcribe"]["current_binding"] = serde_json::json!("ctrl+space");
        stored["bindings"]["multi_stt_transcribe"] = serde_json::json!({
            "id": "multi_stt_transcribe",
            "name": "Multi STT Transcribe",
            "description": "Transcribes with multiple STT models.",
            "default_binding": "ctrl+alt+space",
            "current_binding": "ctrl+alt+space"
        });

        let mut settings: AppSettings = serde_json::from_value(stored.clone())
            .expect("valid settings parse with extra bindings");

        assert_eq!(
            settings.bindings["transcribe"].current_binding,
            "ctrl+space"
        );
        assert_eq!(
            settings.bindings["multi_stt_transcribe"].current_binding,
            "ctrl+alt+space"
        );

        assert!(apply_settings_migrations(&mut settings, &stored));
        assert_eq!(
            settings.settings_schema_version,
            CURRENT_SETTINGS_SCHEMA_VERSION
        );

        // Both bindings should have their current_binding cleared
        assert_eq!(settings.bindings["transcribe"].current_binding, "");
        assert_eq!(
            settings.bindings["multi_stt_transcribe"].current_binding,
            ""
        );

        // But default_binding must be preserved for manual restore
        assert!(!settings.bindings["transcribe"].default_binding.is_empty());
        assert!(
            !settings.bindings["multi_stt_transcribe"]
                .default_binding
                .is_empty()
        );
    }

    #[test]
    fn schema_4_migration_clears_bindings_for_users_already_on_schema_3() {
        // Users who ran a build that bumped CURRENT_SETTINGS_SCHEMA_VERSION
        // to 3 before the schema 3 migration body existed would have
        // settings_schema_version: 3 in their store but with the old
        // default shortcuts still set. The schema 4 migration must catch
        // these users and clear the bindings.
        let mut stored = default_settings_json();
        stored["settings_schema_version"] = serde_json::json!(3);
        stored["bindings"]["transcribe"]["current_binding"] = serde_json::json!("ctrl+space");
        stored["bindings"]["multi_stt_transcribe"] = serde_json::json!({
            "id": "multi_stt_transcribe",
            "name": "Multi STT Transcribe",
            "description": "Transcribes with multiple STT models.",
            "default_binding": "ctrl+alt+space",
            "current_binding": "ctrl+alt+space"
        });

        let mut settings: AppSettings =
            serde_json::from_value(stored.clone()).expect("valid settings parse with schema 3");

        assert!(apply_settings_migrations(&mut settings, &stored));
        assert_eq!(
            settings.settings_schema_version,
            CURRENT_SETTINGS_SCHEMA_VERSION
        );
        assert_eq!(settings.bindings["transcribe"].current_binding, "");
        assert_eq!(
            settings.bindings["multi_stt_transcribe"].current_binding,
            ""
        );
    }

    #[test]
    fn schema_5_migration_clears_post_process_binding_conflicting_with_performance_mode() {
        // Users on schema 4 may still have transcribe_with_post_process
        // bound to ctrl+alt+space (or ctrl_left+alt_left+space), which
        // conflicts with the performance-mode simulated keystrokes.
        let mut stored = default_settings_json();
        stored["settings_schema_version"] = serde_json::json!(4);
        stored["bindings"]["transcribe_with_post_process"]["current_binding"] =
            serde_json::json!("ctrl_left+alt_left+space");

        let mut settings: AppSettings =
            serde_json::from_value(stored.clone()).expect("valid settings parse with schema 4");

        assert!(apply_settings_migrations(&mut settings, &stored));
        assert_eq!(
            settings.settings_schema_version,
            CURRENT_SETTINGS_SCHEMA_VERSION
        );
        assert_eq!(
            settings.bindings["transcribe_with_post_process"].current_binding,
            ""
        );
    }

    #[test]
    fn schema_5_migration_preserves_non_conflicting_post_process_binding() {
        let mut stored = default_settings_json();
        stored["settings_schema_version"] = serde_json::json!(4);
        stored["bindings"]["transcribe_with_post_process"]["current_binding"] =
            serde_json::json!("ctrl+shift+space");

        let mut settings: AppSettings =
            serde_json::from_value(stored.clone()).expect("valid settings parse with schema 4");

        assert!(!apply_settings_migrations(&mut settings, &stored));
        assert_eq!(
            settings.bindings["transcribe_with_post_process"].current_binding,
            "ctrl+shift+space"
        );
    }

    #[test]
    fn test_multi_stt_performance_mode_trigger_on_start_defaults_to_false() {
        let default_settings = get_default_settings();
        assert!(!default_settings.multi_stt_performance_mode_trigger_on_start);

        let parsed: AppSettings = serde_json::from_str("{}").expect("deserializes empty json");
        assert!(!parsed.multi_stt_performance_mode_trigger_on_start);
    }
}
