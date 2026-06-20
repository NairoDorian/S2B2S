use log::{debug, warn};
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use specta::Type;
use std::collections::HashMap;
use std::fmt;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

pub const APPLE_INTELLIGENCE_PROVIDER_ID: &str = "apple_intelligence";
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

/// A saved language model: a (provider, model) pair the user added in the
/// Models > Language Models tab. Referenced by post-process actions.
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct LLMModel {
    pub id: String,
    pub provider_id: String,
    pub model: String,
    pub label: String,
}

/// A post-processing action: a prompt applied to the transcription through a
/// saved language model. Can be triggered by a dedicated global shortcut
/// (stored in `bindings` under `ppa_<id>`) or by pressing `trigger_key`
/// while a recording is in progress.
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct PostProcessAction {
    pub id: String,
    pub name: String,
    pub prompt: String,
    #[serde(default)]
    pub llm_model_id: Option<String>,
    #[serde(default = "default_action_icon")]
    pub icon: String,
    #[serde(default)]
    pub trigger_key: Option<u8>,
}

pub fn default_action_icon() -> String {
    "sparkles".to_string()
}

/// Prefix for per-action global shortcut binding ids stored in `bindings`.
pub const ACTION_BINDING_PREFIX: &str = "ppa_";

pub fn action_binding_id(action_id: &str) -> String {
    format!("{}{}", ACTION_BINDING_PREFIX, action_id)
}

#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct PostProcessProvider {
    pub id: String,
    pub label: String,
    pub base_url: String,
    #[serde(default)]
    pub allow_base_url_edit: bool,
    #[serde(default)]
    pub allow_insecure_http: bool,
    #[serde(default)]
    pub models_endpoint: Option<String>,
    #[serde(default)]
    pub supports_structured_output: bool,
}

// ===== S2B2S: Text-to-Speech (CopySpeak "Read Anywhere") + Brain configuration =====

/// Pagination config: split long text into sentence-bounded fragments before synthesis.
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct PaginationConfig {
    pub enabled: bool,
    /// Target maximum characters per fragment.
    pub fragment_size: u32,
}

impl Default for PaginationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            fragment_size: 600,
        }
    }
}

/// Pre-TTS text sanitization toggles (markdown stripping + speech normalization).
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct SanitizationConfig {
    pub enabled: bool,
    /// Strip markdown syntax (code blocks, links, headers, lists, quotes).
    pub markdown: bool,
    /// Expand abbreviations/symbols/units for speech (e.g. "$50" -> "50 dollars").
    pub tts_normalization: bool,
}

impl Default for SanitizationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            markdown: true,
            tts_normalization: true,
        }
    }
}

/// Which TTS engine synthesizes speech.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default, Type)]
#[serde(rename_all = "snake_case")]
pub enum TtsEngine {
    #[default]
    Piper,
    Kokoro,
    Kitten,
    Pocket,
    Sapi,
    Openai,
    Elevenlabs,
    Cartesia,
}

/// Piper (persistent local HTTP server) configuration.
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct PiperConfig {
    /// Path to the python executable that hosts `piper.http_server`.
    pub python_path: String,
    /// Directory containing Piper `.onnx` voices (defaults to ~/piper-voices when empty).
    pub data_dir: String,
    /// Use the CUDA execution provider when starting the server.
    pub cuda: bool,
}

impl Default for PiperConfig {
    fn default() -> Self {
        Self {
            python_path: "python".to_string(),
            data_dir: String::new(),
            cuda: true,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type)]
pub struct OpenAIConfig {
    pub api_key: String,
    pub model: String,
    pub voice: String,
}

impl Default for OpenAIConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            model: "tts-1".to_string(),
            voice: "alloy".to_string(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default, Type)]
pub enum ElevenLabsOutputFormat {
    #[serde(rename = "mp3_44100_128")]
    #[default]
    Mp344100_128,
    #[serde(rename = "mp3_44100_192")]
    Mp344100_192,
    #[serde(rename = "mp3_44100_32")]
    Mp344100_32,
    #[serde(rename = "mp3_22050_32")]
    Mp322050_32,
    #[serde(rename = "pcm_44100")]
    Pcm44100,
    #[serde(rename = "pcm_22050")]
    Pcm22050,
    #[serde(rename = "pcm_16000")]
    Pcm16000,
    #[serde(rename = "ogg_vorbis_44100")]
    OggVorbis44100,
    #[serde(rename = "ogg_vorbis_22050")]
    OggVorbis22050,
    #[serde(rename = "flac_44100")]
    Flac44100,
    #[serde(rename = "mulaw_8000")]
    Mulaw8000,
}

impl ElevenLabsOutputFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            ElevenLabsOutputFormat::Mp344100_128 => "mp3_44100_128",
            ElevenLabsOutputFormat::Mp344100_192 => "mp3_44100_192",
            ElevenLabsOutputFormat::Mp344100_32 => "mp3_44100_32",
            ElevenLabsOutputFormat::Mp322050_32 => "mp3_22050_32",
            ElevenLabsOutputFormat::Pcm44100 => "pcm_44100",
            ElevenLabsOutputFormat::Pcm22050 => "pcm_22050",
            ElevenLabsOutputFormat::Pcm16000 => "pcm_16000",
            ElevenLabsOutputFormat::OggVorbis44100 => "ogg_vorbis_44100",
            ElevenLabsOutputFormat::OggVorbis22050 => "ogg_vorbis_22050",
            ElevenLabsOutputFormat::Flac44100 => "flac_44100",
            ElevenLabsOutputFormat::Mulaw8000 => "mulaw_8000",
        }
    }

    pub fn mime_type(self) -> &'static str {
        match self {
            ElevenLabsOutputFormat::Mp344100_128
            | ElevenLabsOutputFormat::Mp344100_192
            | ElevenLabsOutputFormat::Mp344100_32
            | ElevenLabsOutputFormat::Mp322050_32 => "audio/mpeg",
            ElevenLabsOutputFormat::Pcm44100
            | ElevenLabsOutputFormat::Pcm22050
            | ElevenLabsOutputFormat::Pcm16000 => "audio/pcm",
            ElevenLabsOutputFormat::OggVorbis44100 | ElevenLabsOutputFormat::OggVorbis22050 => {
                "audio/ogg"
            }
            ElevenLabsOutputFormat::Flac44100 => "audio/flac",
            ElevenLabsOutputFormat::Mulaw8000 => "audio/mulaw",
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type)]
pub struct ElevenLabsConfig {
    pub api_key: String,
    pub voice_id: String,
    pub voice_name: Option<String>,
    pub model_id: String,
    pub output_format: ElevenLabsOutputFormat,
    #[serde(default = "default_elevenlabs_stability")]
    pub voice_stability: f32,
    #[serde(default = "default_elevenlabs_similarity")]
    pub voice_similarity_boost: f32,
    pub voice_style: Option<f32>,
    pub use_speaker_boost: Option<bool>,
    pub use_manual_voice_id: bool,
}

fn default_elevenlabs_stability() -> f32 {
    0.5
}

fn default_elevenlabs_similarity() -> f32 {
    0.75
}

impl Default for ElevenLabsConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            voice_id: "21m00Tcm4TlvDq8ikWAM".to_string(), // Rachel
            voice_name: Some("Rachel".to_string()),
            model_id: "eleven_turbo_v2_5".to_string(),
            output_format: ElevenLabsOutputFormat::default(),
            voice_stability: default_elevenlabs_stability(),
            voice_similarity_boost: default_elevenlabs_similarity(),
            voice_style: None,
            use_speaker_boost: None,
            use_manual_voice_id: false,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type)]
pub struct CartesiaConfig {
    pub api_key: String,
    pub model_id: String,
    pub voice_id: String,
    pub voice_name: Option<String>,
    pub output_format: String,
    pub use_manual_voice_id: bool,
}

impl Default for CartesiaConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            model_id: "sonic-3.5".to_string(),
            voice_id: "f786b574-daa5-4673-aa0c-cbe3e8534c02".to_string(),
            voice_name: Some("Katie".to_string()),
            output_format: "wav".to_string(),
            use_manual_voice_id: false,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct TtsGreetingConfig {
    #[serde(default = "default_greeting_text")]
    pub text: String,
    #[serde(default = "default_greeting_speed")]
    pub speed: f32,
    #[serde(default = "default_greeting_voice")]
    pub voice: String,
    #[serde(default = "default_greeting_engine")]
    pub engine: TtsEngine,
    /// Speaking variability (Piper HTTP: noise_scale). 0=monotone, 0.667=Piper default.
    #[serde(default = "default_noise_scale")]
    pub noise_scale: f32,
    /// Phoneme width variability (Piper HTTP: noise_w_scale). 0=precise, 0.8=Piper default.
    #[serde(default = "default_noise_w_scale")]
    pub noise_w_scale: f32,
}

fn default_greeting_text() -> String {
    "Hello, how can I help?".to_string()
}

fn default_greeting_speed() -> f32 {
    1.0
}

fn default_greeting_voice() -> String {
    String::new()
}

fn default_greeting_engine() -> TtsEngine {
    TtsEngine::Piper
}

fn default_noise_scale() -> f32 {
    0.667
}

fn default_noise_w_scale() -> f32 {
    0.8
}

impl Default for TtsGreetingConfig {
    fn default() -> Self {
        Self {
            text: default_greeting_text(),
            speed: default_greeting_speed(),
            voice: default_greeting_voice(),
            engine: default_greeting_engine(),
            noise_scale: default_noise_scale(),
            noise_w_scale: default_noise_w_scale(),
        }
    }
}

/// Text-to-speech ("Read Anywhere" / CopySpeak) configuration.
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct TtsConfig {
    pub enabled: bool,
    pub engine: TtsEngine,
    /// Engine-specific voice id (e.g. a Piper voice filename without extension).
    pub voice: String,
    /// Playback rate; one owner — passed to the engine, never applied twice (CopySpeak C1).
    pub speed: f32,
    /// Playback volume 0-100.
    pub volume: u8,
    pub pagination: PaginationConfig,
    #[serde(default)]
    pub sanitization: SanitizationConfig,
    pub piper: PiperConfig,
    /// CopySpeak double-copy trigger: copy the same text twice within the
    /// window to speak it automatically (currently Windows-only detection).
    #[serde(default)]
    pub double_copy_enabled: bool,
    #[serde(default = "default_double_copy_window_ms")]
    pub double_copy_window_ms: u32,
    #[serde(default = "default_play_startup_greeting")]
    pub play_startup_greeting: bool,
    #[serde(default)]
    pub greeting: TtsGreetingConfig,
    #[serde(default)]
    pub openai: OpenAIConfig,
    #[serde(default)]
    pub elevenlabs: ElevenLabsConfig,
    #[serde(default)]
    pub cartesia: CartesiaConfig,
    /// Number of parallel Kokoro synthesis workers (auto-tuned from CPU count, min 1, max 8).
    #[serde(default = "default_tts_workers")]
    pub tts_workers: u32,
    /// Shorten the first chunk to reduce time-to-first-audio (Parrot pattern).
    #[serde(default = "default_tts_shorten_first_chunk")]
    pub tts_shorten_first_chunk: bool,
    /// Audio format for saved TTS output.
    #[serde(default)]
    pub tts_save_format: crate::tts::audio_format::AudioFormat,
    /// Wake word / always-listening keyword detection.
    #[serde(default)]
    pub wake_word: WakeWordConfig,
}

/// User-facing configuration for wake word detection.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct WakeWordConfig {
    pub enabled: bool,
    pub keyword: String,
    pub threshold: f32,
    pub show_indicator: bool,
}

impl Default for WakeWordConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            keyword: "hey s2b2s".to_string(),
            threshold: 0.6,
            show_indicator: true,
        }
    }
}

fn default_double_copy_window_ms() -> u32 {
    1500
}

fn default_play_startup_greeting() -> bool {
    true
}

fn default_tts_workers() -> u32 {
    // Default to half the logical cores, capped between 1 and 4.
    let cpus = std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(4);
    (cpus / 2).clamp(1, 4)
}

fn default_tts_shorten_first_chunk() -> bool {
    true
}

impl Default for TtsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            engine: TtsEngine::default(),
            voice: String::new(),
            speed: 1.0,
            volume: 100,
            pagination: PaginationConfig::default(),
            sanitization: SanitizationConfig::default(),
            piper: PiperConfig::default(),
            double_copy_enabled: false,
            double_copy_window_ms: default_double_copy_window_ms(),
            play_startup_greeting: true,
            greeting: TtsGreetingConfig::default(),
            openai: OpenAIConfig::default(),
            elevenlabs: ElevenLabsConfig::default(),
            cartesia: CartesiaConfig::default(),
            tts_workers: default_tts_workers(),
            tts_shorten_first_chunk: default_tts_shorten_first_chunk(),
            tts_save_format: crate::tts::audio_format::AudioFormat::default(),
            wake_word: WakeWordConfig::default(),
        }
    }
}

/// The "Brain": a streaming LLM subsystem, independent of dictation post-processing.
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct BrainConfig {
    pub enabled: bool,
    pub provider_id: String,
    pub providers: Vec<PostProcessProvider>,
    pub api_keys: SecretMap,
    pub models: HashMap<String, String>,
    pub system_prompt: String,
    /// How many prior turns to keep in the context window (0 = stateless).
    pub context_turns: u32,
    /// Speak the Brain's reply aloud via the TTS subsystem.
    pub read_aloud: bool,
    /// Separate system prompt appended when read-aloud is ON.
    /// Instructs the model to answer conversationally for listening.
    #[serde(default = "default_speakable_output_prompt")]
    pub speakable_output_prompt: String,
    /// Dummy prompt sent to warm up the Brain model when it loads into VRAM.
    #[serde(default = "default_warmup_prompt")]
    pub warmup_prompt: String,
    /// Conversation mode: push_to_talk | toggle | hands_free
    #[serde(default = "default_conversation_mode")]
    pub conversation_mode: String,
    /// Endpoint silence preset: snappy(300ms) | balanced(600ms) | patient(1200ms)
    #[serde(default = "default_endpoint_preset")]
    pub endpoint_preset: String,
    /// Headphone mode — enables barge-in during TTS playback
    #[serde(default)]
    pub headphone_mode: bool,
    /// Auto-rearm mic after reply in hands-free mode
    #[serde(default)]
    pub auto_listen: bool,
    /// Send the WAV audio recording as `input_audio` to the multimodal Brain model
    /// (Gemma 4 supports native audio transcription as an extra STT pass).
    #[serde(default)]
    pub multimodal_audio_enabled: bool,
    /// Send a screenshot/image as `image_url` to the multimodal Brain model.
    /// When enabled, images can be passed alongside text prompts for vision understanding.
    #[serde(default)]
    pub multimodal_image_enabled: bool,
}

fn default_speakable_output_prompt() -> String {
    "Answer conversationally in short sentences suitable for being read aloud. Avoid markdown tables, bullet lists, code blocks, and emoji unless asked. Expand abbreviations. Put any code in a short spoken summary.".to_string()
}

fn default_conversation_mode() -> String {
    "push_to_talk".to_string()
}

fn default_warmup_prompt() -> String {
    "Count from 1 to 10".to_string()
}

fn default_endpoint_preset() -> String {
    "balanced".to_string()
}

impl BrainConfig {
    pub fn active_provider(&self) -> Option<&PostProcessProvider> {
        self.providers.iter().find(|p| p.id == self.provider_id)
    }

    pub fn active_model(&self) -> String {
        self.models
            .get(&self.provider_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn active_api_key(&self) -> String {
        self.api_keys
            .get(&self.provider_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn active_base_url(&self) -> String {
        self.active_provider()
            .map(|p| p.base_url.clone())
            .unwrap_or_default()
    }

    pub fn provider_mut(&mut self, id: &str) -> Option<&mut PostProcessProvider> {
        self.providers.iter_mut().find(|p| p.id == id)
    }
}

impl Default for BrainConfig {
    fn default() -> Self {
        let providers = default_post_process_providers();
        let mut api_keys = HashMap::new();
        let mut models = HashMap::new();
        for provider in &providers {
            api_keys.insert(provider.id.clone(), String::new());
            models.insert(provider.id.clone(), String::new());
        }
        // Set default model for llama_cpp (local Gemma-4 engine)
        models.insert(
            "llama_cpp".to_string(),
            "unsloth/gemma-4-e2b-it-qat-GGUF".to_string(),
        );

        Self {
            enabled: true,
            provider_id: "llama_cpp".to_string(), // Default to Llama.cpp (Gemma-4 + MTP)
            providers,
            api_keys: SecretMap(api_keys),
            models,
            system_prompt: "You are a helpful, concise voice assistant. Answer conversationally in short sentences suitable for being read aloud. Avoid markdown tables, bullet lists, and emoji unless asked.".to_string(),
            context_turns: 20,
            read_aloud: true,
            speakable_output_prompt: default_speakable_output_prompt(),
            warmup_prompt: default_warmup_prompt(),
            conversation_mode: default_conversation_mode(),
            endpoint_preset: default_endpoint_preset(),
            headphone_mode: false,
            auto_listen: false,
            multimodal_audio_enabled: false,
            multimodal_image_enabled: false,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "lowercase")]
pub enum OverlayPosition {
    None,
    Top,
    Bottom,
}

/// Overlay approach: Tauri-only (CopySpeak HUD style — alwaysOnTop + transparent)
/// or OS-native (NSPanel/Win32 HWND_TOPMOST/GTK layer-shell — Handy style).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum OverlayMode {
    /// Tauri `always_on_top(true)` + `transparent(true)` only — simpler, fewer deps.
    Tauri,
    /// Per-OS native window APIs (NSPanel, Win32 topmost, GTK layer-shell).
    OsNative,
}

impl Default for OverlayMode {
    fn default() -> Self {
        OverlayMode::OsNative
    }
}

/// Configuration for the recording overlay window (the pill shown during dictation/TTS).
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct OverlayWindowConfig {
    /// Which overlay approach to use.
    #[serde(default)]
    pub mode: OverlayMode,
    /// Position on screen.
    #[serde(default = "default_overlay_position")]
    pub position: OverlayPosition,
    /// Overlay width in logical pixels.
    #[serde(default = "default_overlay_pill_width")]
    pub width: u32,
    /// Overlay height in logical pixels.
    #[serde(default = "default_overlay_pill_height")]
    pub height: u32,
    /// Overlay background opacity (0.0–1.0).
    #[serde(default = "default_overlay_opacity")]
    pub opacity: f32,
    /// Round the corners with this radius (0 = square).
    #[serde(default = "default_overlay_corner_radius")]
    pub corner_radius: f32,
    /// Show a text reply bubble next to the cursor during Brain conversation.
    #[serde(default = "default_overlay_reply_bubble")]
    pub reply_bubble: bool,
    /// Fade-out time in milliseconds.
    #[serde(default = "default_overlay_fade_ms")]
    pub fade_ms: u32,
}

fn default_overlay_pill_width() -> u32 {
    172
}
fn default_overlay_pill_height() -> u32 {
    36
}
fn default_overlay_opacity() -> f32 {
    0.8
}
fn default_overlay_corner_radius() -> f32 {
    18.0
}
fn default_overlay_reply_bubble() -> bool {
    false
}
fn default_overlay_fade_ms() -> u32 {
    300
}

impl Default for OverlayWindowConfig {
    fn default() -> Self {
        Self {
            mode: OverlayMode::default(),
            position: default_overlay_position(),
            width: default_overlay_pill_width(),
            height: default_overlay_pill_height(),
            opacity: default_overlay_opacity(),
            corner_radius: default_overlay_corner_radius(),
            reply_bubble: default_overlay_reply_bubble(),
            fade_ms: default_overlay_fade_ms(),
        }
    }
}

/// Cursor trail / wgpu overlay effect configuration (CursorFX + TD_Web_Trail).
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct WgpuTrailConfig {
    /// Master enable for the native wgpu cursor trail.
    #[serde(default)]
    pub enabled: bool,
    /// Trail colour as hex string (e.g. "#7c3aed").
    #[serde(default = "default_trail_color")]
    pub color: String,
    /// Number of trail segments (chain length).
    #[serde(default = "default_trail_segments")]
    pub segments: u32,
    /// Spring stiffness for the physics chain (0.0–1.0).
    #[serde(default = "default_trail_spring")]
    pub spring: f32,
    /// Velocity friction / damping (0.0–1.0).
    #[serde(default = "default_trail_friction")]
    pub friction: f32,
    /// Base trail width in logical pixels at the head.
    #[serde(default = "default_trail_width")]
    pub width: f32,
    /// Width taper exponent (e.g. 1.5 = trail tapers toward tail).
    #[serde(default = "default_trail_taper")]
    pub taper: f32,
    /// Opacity of the glow pass (0.0–1.0).
    #[serde(default = "default_trail_glow")]
    pub glow: f32,
    /// Lazy-brush dead-zone radius in logical pixels.
    #[serde(default = "default_trail_lazy_radius")]
    pub lazy_radius: f32,
    /// Lazy-brush friction factor (0.0–1.0).
    #[serde(default = "default_trail_lazy_friction")]
    pub lazy_friction: f32,
    /// Enable cursor click ripple effect.
    #[serde(default)]
    pub click_ripple: bool,
}

fn default_trail_color() -> String {
    "#7c3aed".to_string()
}
fn default_trail_segments() -> u32 {
    50
}
fn default_trail_spring() -> f32 {
    0.39
}
fn default_trail_friction() -> f32 {
    0.5
}
fn default_trail_width() -> f32 {
    3.0
}
fn default_trail_taper() -> f32 {
    1.5
}
fn default_trail_glow() -> f32 {
    0.6
}
fn default_trail_lazy_radius() -> f32 {
    8.0
}
fn default_trail_lazy_friction() -> f32 {
    0.4
}

impl Default for WgpuTrailConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            color: default_trail_color(),
            segments: default_trail_segments(),
            spring: default_trail_spring(),
            friction: default_trail_friction(),
            width: default_trail_width(),
            taper: default_trail_taper(),
            glow: default_trail_glow(),
            lazy_radius: default_trail_lazy_radius(),
            lazy_friction: default_trail_lazy_friction(),
            click_ripple: false,
        }
    }
}

#[derive(Default, Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
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
    None,
    ShiftInsert,
    CtrlShiftV,
    ExternalScript,
}

#[derive(Default, Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum ClipboardHandling {
    #[default]
    DontModify,
    CopyToClipboard,
}

#[derive(Default, Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
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
    KeyListener,
}

impl Default for KeyboardImplementation {
    fn default() -> Self {
        #[cfg(target_os = "linux")]
        return KeyboardImplementation::Tauri;
        #[cfg(not(target_os = "linux"))]
        return KeyboardImplementation::KeyListener;
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

    pub fn start_path(&self) -> String {
        format!("resources/{}_start.wav", self.as_str())
    }

    pub fn stop_path(&self) -> String {
        format!("resources/{}_stop.wav", self.as_str())
    }
}

#[derive(Default, Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
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

#[derive(Default, Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum WhisperAcceleratorSetting {
    #[default]
    Auto,
    Cpu,
    Gpu,
}

#[derive(Default, Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum OrtAcceleratorSetting {
    #[default]
    Auto,
    Cpu,
    Cuda,
    #[serde(rename = "directml")]
    DirectMl,
    Rocm,
}

#[derive(Clone, Serialize, Deserialize, Type)]
#[serde(transparent)]
pub(crate) struct SecretMap(HashMap<String, String>);

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

/* still s2b2s for composing the initial JSON in the store ------------- */
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct AppSettings {
    pub bindings: HashMap<String, ShortcutBinding>,
    pub push_to_talk: bool,
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
    #[serde(default = "default_model")]
    pub selected_model: String,
    #[serde(default = "default_always_on_microphone")]
    pub always_on_microphone: bool,
    #[serde(default)]
    pub selected_microphone: Option<String>,
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
    /// Overlay window behaviour + visual customization.
    #[serde(default)]
    pub overlay_window: OverlayWindowConfig,
    /// Native wgpu cursor trail + click ripple effects.
    #[serde(default)]
    pub wgpu_trail: WgpuTrailConfig,
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
    // specta-typescript 0.0.12 forbids exporting 64-bit ints by default; these
    // values are small, so exporting as TS `number` is lossless in practice.
    #[specta(type = u32)]
    pub history_limit: usize,
    #[serde(default = "default_recording_retention_period")]
    pub recording_retention_period: RecordingRetentionPeriod,
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
    pub llm_models: Vec<LLMModel>,
    #[serde(default)]
    pub post_process_actions: Vec<PostProcessAction>,
    #[serde(default)]
    pub post_process_actions_initialized: bool,
    #[serde(default)]
    pub mute_while_recording: bool,
    #[serde(default)]
    pub append_trailing_space: bool,
    #[serde(default = "default_app_language")]
    pub app_language: String,
    #[serde(default)]
    pub experimental_enabled: bool,
    #[serde(default)]
    pub lazy_stream_close: bool,
    #[serde(default)]
    pub keyboard_implementation: KeyboardImplementation,
    #[serde(default = "default_show_tray_icon")]
    pub show_tray_icon: bool,
    #[serde(default = "default_paste_delay_ms")]
    #[specta(type = u32)]
    pub paste_delay_ms: u64,
    #[serde(default = "default_typing_tool")]
    pub typing_tool: TypingTool,
    pub external_script_path: Option<String>,
    #[serde(default)]
    pub custom_filler_words: Option<Vec<String>>,
    #[serde(default)]
    pub whisper_accelerator: WhisperAcceleratorSetting,
    #[serde(default)]
    pub ort_accelerator: OrtAcceleratorSetting,
    #[serde(default = "default_whisper_gpu_device")]
    pub whisper_gpu_device: i32,
    #[serde(default)]
    #[specta(type = u32)]
    pub extra_recording_buffer_ms: u64,
    /// Text-to-speech ("Read Anywhere" / CopySpeak) settings.
    #[serde(default)]
    pub tts: TtsConfig,
    /// Streaming LLM "Brain" subsystem settings (separate from post-processing).
    #[serde(default)]
    pub brain: BrainConfig,
    #[serde(default)]
    pub long_audio_model: Option<String>,
    #[serde(default = "default_long_audio_threshold_seconds")]
    pub long_audio_threshold_seconds: f64,
    #[serde(default = "default_noise_suppression_enabled")]
    pub noise_suppression_enabled: bool,
    #[serde(default = "default_vad_mode")]
    pub vad_mode: String,
    #[serde(default = "default_rnnoise_voice_threshold")]
    pub rnnoise_voice_threshold: f64,
    #[serde(default)]
    pub llama_server: crate::llama_server::manager::LlamaServerConfig,
    /// Multi-STT: run multiple transcription models in parallel and merge results.
    #[serde(default)]
    pub multi_stt_enabled: bool,
    #[serde(default)]
    pub multi_stt_models: Vec<String>,
    /// Multi-STT post-processing prompt. {transcriptions} is replaced with the model results.
    #[serde(default = "default_multi_stt_prompt")]
    pub multi_stt_prompt: String,
    /// Parakeet streaming toggle: when enabled, all UnifiedParakeet models
    /// (Unified 0.6B + EOU 120M) use the streaming API for progressive partial
    /// results with stateful RNNT decoder. When disabled, uses offline /transcribe.
    #[serde(default = "default_parakeet_streaming_enabled")]
    pub parakeet_streaming_enabled: bool,
    #[serde(default)]
    pub control_server_token: Option<String>,
    #[serde(default)]
    pub recording_auto_stop_enabled: bool,
    #[serde(default = "default_recording_auto_stop_timeout_seconds")]
    pub recording_auto_stop_timeout_seconds: u32,
    #[serde(default)]
    pub recording_auto_stop_paste: bool,
    #[serde(default)]
    pub text_replacement_decapitalize_after_edit_key_enabled: bool,
    #[serde(default = "default_text_replacement_decapitalize_after_edit_key")]
    pub text_replacement_decapitalize_after_edit_key: String,
    #[serde(default)]
    pub text_replacement_decapitalize_after_edit_secondary_key_enabled: bool,
    #[serde(default = "default_text_replacement_decapitalize_after_edit_secondary_key")]
    pub text_replacement_decapitalize_after_edit_secondary_key: String,
    #[serde(default = "default_text_replacement_decapitalize_timeout_ms")]
    pub text_replacement_decapitalize_timeout_ms: u32,
    #[serde(default = "default_text_replacement_decapitalize_standard_post_recording_monitor_ms")]
    pub text_replacement_decapitalize_standard_post_recording_monitor_ms: u32,
}

fn default_recording_auto_stop_timeout_seconds() -> u32 {
    1800
}

fn default_text_replacement_decapitalize_after_edit_key() -> String {
    "backspace".to_string()
}

fn default_text_replacement_decapitalize_after_edit_secondary_key() -> String {
    "delete".to_string()
}

fn default_text_replacement_decapitalize_timeout_ms() -> u32 {
    5000
}

fn default_text_replacement_decapitalize_standard_post_recording_monitor_ms() -> u32 {
    5000
}

fn default_long_audio_threshold_seconds() -> f64 {
    10.0
}

fn default_noise_suppression_enabled() -> bool {
    false
}

fn default_vad_mode() -> String {
    "triple".to_string()
}

fn default_rnnoise_voice_threshold() -> f64 {
    0.2
}

fn default_multi_stt_prompt() -> String {
    "You are a speech transcription corrector. Given multiple independent transcriptions of the same audio recording, produce the most accurate final transcription. Cross-reference the transcriptions to resolve disagreements. Fix obvious errors, remove repetitions, and ensure the output reads naturally.\n\nTranscriptions:\n{transcriptions}\n\nReturn only the corrected transcription text, nothing else.".to_string()
}

fn default_parakeet_streaming_enabled() -> bool {
    true
}

fn default_model() -> String {
    "".to_string()
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

fn default_selected_language() -> String {
    "auto".to_string()
}

fn default_overlay_position() -> OverlayPosition {
    #[cfg(target_os = "linux")]
    return OverlayPosition::None;
    #[cfg(not(target_os = "linux"))]
    return OverlayPosition::Bottom;
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

fn default_paste_delay_ms() -> u64 {
    60
}

fn default_auto_submit() -> bool {
    false
}

fn default_history_limit() -> usize {
    5
}

fn default_recording_retention_period() -> RecordingRetentionPeriod {
    RecordingRetentionPeriod::PreserveLimit
}

fn default_audio_feedback_volume() -> f32 {
    1.0
}

fn default_sound_theme() -> SoundTheme {
    SoundTheme::Marimba
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
            allow_insecure_http: false,
            models_endpoint: Some("/models".to_string()),
            supports_structured_output: true,
        },
        PostProcessProvider {
            id: "zai".to_string(),
            label: "Z.AI".to_string(),
            base_url: "https://api.z.ai/api/paas/v4".to_string(),
            allow_base_url_edit: false,
            allow_insecure_http: false,
            models_endpoint: Some("/models".to_string()),
            supports_structured_output: true,
        },
        PostProcessProvider {
            id: "gemini".to_string(),
            label: "Google Gemini".to_string(),
            base_url: "https://generativelanguage.googleapis.com/v1beta/openai".to_string(),
            allow_base_url_edit: false,
            allow_insecure_http: false,
            models_endpoint: Some("/models".to_string()),
            supports_structured_output: true,
        },
        PostProcessProvider {
            id: "google_ai_studio".to_string(),
            label: "Google AI Studio".to_string(),
            base_url: "https://generativelanguage.googleapis.com/v1beta/openai/".to_string(),
            allow_base_url_edit: false,
            allow_insecure_http: false,
            models_endpoint: Some("/models".to_string()),
            supports_structured_output: false,
        },
        PostProcessProvider {
            id: "openrouter".to_string(),
            label: "OpenRouter".to_string(),
            base_url: "https://openrouter.ai/api/v1".to_string(),
            allow_base_url_edit: false,
            allow_insecure_http: false,
            models_endpoint: Some("/models".to_string()),
            supports_structured_output: true,
        },
        PostProcessProvider {
            id: "anthropic".to_string(),
            label: "Anthropic".to_string(),
            base_url: "https://api.anthropic.com/v1".to_string(),
            allow_base_url_edit: false,
            allow_insecure_http: false,
            models_endpoint: Some("/models".to_string()),
            supports_structured_output: false,
        },
        PostProcessProvider {
            id: "groq".to_string(),
            label: "Groq".to_string(),
            base_url: "https://api.groq.com/openai/v1".to_string(),
            allow_base_url_edit: false,
            allow_insecure_http: false,
            models_endpoint: Some("/models".to_string()),
            supports_structured_output: false,
        },
        PostProcessProvider {
            id: "cerebras".to_string(),
            label: "Cerebras".to_string(),
            base_url: "https://api.cerebras.ai/v1".to_string(),
            allow_base_url_edit: false,
            allow_insecure_http: false,
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
            allow_insecure_http: false,
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
        allow_insecure_http: false,
        models_endpoint: Some("/models".to_string()),
        supports_structured_output: true,
    });

    // Llama.cpp (Local) provider
    providers.push(PostProcessProvider {
        id: "llama_cpp".to_string(),
        label: "Llama.cpp (Local)".to_string(),
        base_url: "http://localhost:8001/v1".to_string(),
        allow_base_url_edit: true,
        allow_insecure_http: true,
        models_endpoint: Some("/models".to_string()),
        supports_structured_output: true,
    });

    // Custom provider always comes last
    providers.push(PostProcessProvider {
        id: "custom".to_string(),
        label: "Custom".to_string(),
        base_url: "http://localhost:11434/v1".to_string(),
        allow_base_url_edit: true,
        allow_insecure_http: true,
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
    if provider_id == "llama_cpp" {
        return "unsloth/gemma-4-e2b-it-qat-GGUF".to_string();
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
        prompt: "Clean this transcript:\n1. Fix spelling, capitalization, and punctuation errors\n2. Convert number words to digits (twenty-five → 25, ten percent → 10%, five dollars → $5)\n3. Replace spoken punctuation with symbols (period → ., comma → ,, question mark → ?)\n4. Remove filler words (um, uh, like as filler)\n5. Keep the language in the original version (if it was french, keep it in french for example)\n\nPreserve exact meaning and word order. Do not paraphrase or reorder content.\n\nReturn only the cleaned transcript.\n\nTranscript:\n${output}".to_string(),
    }]
}

fn default_whisper_gpu_device() -> i32 {
    -1 // auto
}

fn default_typing_tool() -> TypingTool {
    TypingTool::Auto
}

fn ensure_post_process_defaults(settings: &mut AppSettings) -> bool {
    let mut changed = false;
    let default_providers = default_post_process_providers();

    // 1. Post Process Sync
    for provider in &default_providers {
        match settings
            .post_process_providers
            .iter_mut()
            .find(|p| p.id == provider.id)
        {
            Some(existing) => {
                if existing.supports_structured_output != provider.supports_structured_output {
                    existing.supports_structured_output = provider.supports_structured_output;
                    changed = true;
                }
                if existing.base_url != provider.base_url
                    && existing.id != "custom"
                    && existing.id != "llama_cpp"
                {
                    existing.base_url = provider.base_url.clone();
                    changed = true;
                }
            }
            None => {
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

    // 2. Brain Providers Sync
    for provider in &default_providers {
        match settings
            .brain
            .providers
            .iter_mut()
            .find(|p| p.id == provider.id)
        {
            Some(existing) => {
                if existing.supports_structured_output != provider.supports_structured_output {
                    existing.supports_structured_output = provider.supports_structured_output;
                    changed = true;
                }
                if existing.base_url != provider.base_url
                    && existing.id != "custom"
                    && existing.id != "llama_cpp"
                {
                    existing.base_url = provider.base_url.clone();
                    changed = true;
                }
            }
            None => {
                settings.brain.providers.push(provider.clone());
                changed = true;
            }
        }

        if !settings.brain.api_keys.contains_key(&provider.id) {
            settings
                .brain
                .api_keys
                .insert(provider.id.clone(), String::new());
            changed = true;
        }

        let default_model = default_model_for_provider(&provider.id);
        match settings.brain.models.get_mut(&provider.id) {
            Some(existing) => {
                if existing.is_empty() && !default_model.is_empty() {
                    *existing = default_model.clone();
                    changed = true;
                }
            }
            None => {
                settings
                    .brain
                    .models
                    .insert(provider.id.clone(), default_model);
                changed = true;
            }
        }
    }

    changed
}

/// One-time migration: build saved language models and post-process actions
/// from the legacy prompt/provider configuration. Returns true when settings
/// were modified and need to be persisted.
fn ensure_post_process_actions(settings: &mut AppSettings) -> bool {
    if settings.post_process_actions_initialized {
        return false;
    }

    let mut default_model_id: Option<String> = settings.llm_models.first().map(|m| m.id.clone());
    if default_model_id.is_none() {
        if let Some(provider) = settings.active_post_process_provider().cloned() {
            if provider.id != APPLE_INTELLIGENCE_PROVIDER_ID {
                if let Some(model) = settings.post_process_models.get(&provider.id).cloned() {
                    if !model.trim().is_empty() {
                        let id = format!("llm_{}", chrono::Utc::now().timestamp_millis());
                        settings.llm_models.push(LLMModel {
                            id: id.clone(),
                            provider_id: provider.id.clone(),
                            model: model.clone(),
                            label: model,
                        });
                        default_model_id = Some(id);
                    }
                }
            }
        }
    }

    if settings.post_process_actions.is_empty() {
        for (index, prompt) in settings.post_process_prompts.iter().enumerate() {
            let trigger_key = u8::try_from(index + 1).ok().filter(|key| *key <= 9);
            settings.post_process_actions.push(PostProcessAction {
                id: format!("act_migrated_{}", index),
                name: prompt.name.clone(),
                prompt: prompt.prompt.clone(),
                llm_model_id: default_model_id.clone(),
                icon: default_action_icon(),
                trigger_key,
            });
        }
    }

    settings.post_process_actions_initialized = true;
    true
}

/// Ensure every post-process action has a matching `ppa_<id>` shortcut binding
/// (created empty so the binding UI can manage it) and prune any orphan
/// `ppa_` bindings whose action no longer exists. Returns true when settings
/// were modified. Runs on every load so migrated actions stay consistent.
fn ensure_action_bindings(settings: &mut AppSettings) -> bool {
    let mut changed = false;

    let to_create: Vec<(String, String)> = settings
        .post_process_actions
        .iter()
        .map(|action| (action_binding_id(&action.id), action.name.clone()))
        .filter(|(binding_id, _)| !settings.bindings.contains_key(binding_id))
        .collect();
    for (binding_id, name) in to_create {
        settings.bindings.insert(
            binding_id.clone(),
            ShortcutBinding {
                id: binding_id,
                name,
                description: "Starts a transcription processed with this action.".to_string(),
                default_binding: String::new(),
                current_binding: String::new(),
            },
        );
        changed = true;
    }

    let valid_ids: std::collections::HashSet<String> = settings
        .post_process_actions
        .iter()
        .map(|action| action_binding_id(&action.id))
        .collect();
    let orphans: Vec<String> = settings
        .bindings
        .keys()
        .filter(|key| key.starts_with(ACTION_BINDING_PREFIX) && !valid_ids.contains(*key))
        .cloned()
        .collect();
    for key in orphans {
        settings.bindings.remove(&key);
        changed = true;
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

    let mut bindings = HashMap::new();
    bindings.insert(
        "transcribe".to_string(),
        ShortcutBinding {
            id: "transcribe".to_string(),
            name: "Transcribe".to_string(),
            description: "Converts your speech into text.".to_string(),
            default_binding: default_shortcut.to_string(),
            current_binding: default_shortcut.to_string(),
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

    #[cfg(target_os = "macos")]
    let default_speak_selection_shortcut = "option+shift+r";
    #[cfg(not(target_os = "macos"))]
    let default_speak_selection_shortcut = "alt+shift+r";

    bindings.insert(
        "speak_selection".to_string(),
        ShortcutBinding {
            id: "speak_selection".to_string(),
            name: "Speak Selection".to_string(),
            description: "Reads the selected text (or clipboard) aloud; press again to stop."
                .to_string(),
            default_binding: default_speak_selection_shortcut.to_string(),
            current_binding: default_speak_selection_shortcut.to_string(),
        },
    );

    // "B" for Brain — Space variants collide with the transcribe bindings.
    #[cfg(target_os = "macos")]
    let default_converse_shortcut = "option+shift+b";
    #[cfg(not(target_os = "macos"))]
    let default_converse_shortcut = "alt+shift+b";

    bindings.insert(
        "converse".to_string(),
        ShortcutBinding {
            id: "converse".to_string(),
            name: "Talk to the Brain".to_string(),
            description:
                "Records your speech, sends it to the Brain, and streams (and speaks) the reply."
                    .to_string(),
            default_binding: default_converse_shortcut.to_string(),
            current_binding: default_converse_shortcut.to_string(),
        },
    );
    bindings.insert(
        "pause".to_string(),
        ShortcutBinding {
            id: "pause".to_string(),
            name: "Pause / Resume".to_string(),
            description: "Pauses or resumes the current recording.".to_string(),
            default_binding: "f6".to_string(),
            current_binding: "f6".to_string(),
        },
    );
    bindings.insert(
        "show_history".to_string(),
        ShortcutBinding {
            id: "show_history".to_string(),
            name: "Show History".to_string(),
            description: "Opens the app window and navigates to the History tab.".to_string(),
            default_binding: "".to_string(),
            current_binding: "".to_string(),
        },
    );
    bindings.insert(
        "copy_latest_history".to_string(),
        ShortcutBinding {
            id: "copy_latest_history".to_string(),
            name: "Copy Latest History".to_string(),
            description: "Copies the latest transcription entry to your clipboard.".to_string(),
            default_binding: "".to_string(),
            current_binding: "".to_string(),
        },
    );
    bindings.insert(
        "toggle_pause".to_string(),
        ShortcutBinding {
            id: "toggle_pause".to_string(),
            name: "Pause/Resume Recording".to_string(),
            description: "Pauses or resumes the current recording session.".to_string(),
            default_binding: "F6".to_string(),
            current_binding: "F6".to_string(),
        },
    );

    #[cfg(target_os = "macos")]
    let default_ai_replace_shortcut = "option+shift+r";
    #[cfg(not(target_os = "macos"))]
    let default_ai_replace_shortcut = "ctrl+alt+space";

    bindings.insert(
        "ai_replace".to_string(),
        ShortcutBinding {
            id: "ai_replace".to_string(),
            name: "AI Replace Selection".to_string(),
            description: "Select text, press this hotkey, speak an instruction — the Brain rewrites the selection in place.".to_string(),
            default_binding: default_ai_replace_shortcut.to_string(),
            current_binding: default_ai_replace_shortcut.to_string(),
        },
    );

    AppSettings {
        bindings,
        push_to_talk: true,
        audio_feedback: false,
        audio_feedback_volume: default_audio_feedback_volume(),
        sound_theme: default_sound_theme(),
        start_hidden: default_start_hidden(),
        autostart_enabled: default_autostart_enabled(),
        update_checks_enabled: default_update_checks_enabled(),
        selected_model: "".to_string(),
        always_on_microphone: false,
        selected_microphone: None,
        clamshell_microphone: None,
        selected_output_device: None,
        translate_to_english: false,
        selected_language: "auto".to_string(),
        overlay_position: default_overlay_position(),
        overlay_window: OverlayWindowConfig::default(),
        wgpu_trail: WgpuTrailConfig::default(),
        debug_mode: false,
        log_level: default_log_level(),
        custom_words: Vec::new(),
        model_unload_timeout: ModelUnloadTimeout::default(),
        word_correction_threshold: default_word_correction_threshold(),
        history_limit: default_history_limit(),
        recording_retention_period: default_recording_retention_period(),
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
        llm_models: Vec::new(),
        post_process_actions: Vec::new(),
        post_process_actions_initialized: false,
        mute_while_recording: false,
        append_trailing_space: false,
        app_language: default_app_language(),
        experimental_enabled: false,
        lazy_stream_close: false,
        keyboard_implementation: KeyboardImplementation::default(),
        show_tray_icon: default_show_tray_icon(),
        paste_delay_ms: default_paste_delay_ms(),
        typing_tool: default_typing_tool(),
        external_script_path: None,
        custom_filler_words: None,
        whisper_accelerator: WhisperAcceleratorSetting::default(),
        ort_accelerator: OrtAcceleratorSetting::default(),
        whisper_gpu_device: default_whisper_gpu_device(),
        extra_recording_buffer_ms: 0,
        tts: TtsConfig::default(),
        brain: BrainConfig::default(),
        long_audio_model: None,
        long_audio_threshold_seconds: default_long_audio_threshold_seconds(),
        noise_suppression_enabled: default_noise_suppression_enabled(),
        vad_mode: default_vad_mode(),
        rnnoise_voice_threshold: default_rnnoise_voice_threshold(),
        llama_server: crate::llama_server::manager::LlamaServerConfig::default(),
        multi_stt_enabled: false,
        multi_stt_models: Vec::new(),
        multi_stt_prompt: default_multi_stt_prompt(),
        parakeet_streaming_enabled: default_parakeet_streaming_enabled(),
        control_server_token: None,
        recording_auto_stop_enabled: false,
        recording_auto_stop_timeout_seconds: default_recording_auto_stop_timeout_seconds(),
        recording_auto_stop_paste: false,
        text_replacement_decapitalize_after_edit_key_enabled: false,
        text_replacement_decapitalize_after_edit_key:
            default_text_replacement_decapitalize_after_edit_key(),
        text_replacement_decapitalize_after_edit_secondary_key_enabled: false,
        text_replacement_decapitalize_after_edit_secondary_key:
            default_text_replacement_decapitalize_after_edit_secondary_key(),
        text_replacement_decapitalize_timeout_ms: default_text_replacement_decapitalize_timeout_ms(
        ),
        text_replacement_decapitalize_standard_post_recording_monitor_ms:
            default_text_replacement_decapitalize_standard_post_recording_monitor_ms(),
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

    pub fn llm_model(&self, id: &str) -> Option<&LLMModel> {
        self.llm_models.iter().find(|model| model.id == id)
    }

    pub fn post_process_action(&self, id: &str) -> Option<&PostProcessAction> {
        self.post_process_actions
            .iter()
            .find(|action| action.id == id)
    }

    pub fn post_process_action_by_trigger_key(&self, key: u8) -> Option<&PostProcessAction> {
        self.post_process_actions
            .iter()
            .find(|action| action.trigger_key == Some(key))
    }

    pub fn default_post_process_action(&self) -> Option<&PostProcessAction> {
        self.post_process_actions.first()
    }
}

fn encrypt_settings_keys(mut settings: AppSettings) -> AppSettings {
    if !settings.tts.openai.api_key.is_empty()
        && !settings.tts.openai.api_key.starts_with("enc:v1:")
    {
        if let Ok(enc) = crate::crypto::encrypt_str(&settings.tts.openai.api_key) {
            settings.tts.openai.api_key = enc;
        }
    }
    if !settings.tts.elevenlabs.api_key.is_empty()
        && !settings.tts.elevenlabs.api_key.starts_with("enc:v1:")
    {
        if let Ok(enc) = crate::crypto::encrypt_str(&settings.tts.elevenlabs.api_key) {
            settings.tts.elevenlabs.api_key = enc;
        }
    }
    if !settings.tts.cartesia.api_key.is_empty()
        && !settings.tts.cartesia.api_key.starts_with("enc:v1:")
    {
        if let Ok(enc) = crate::crypto::encrypt_str(&settings.tts.cartesia.api_key) {
            settings.tts.cartesia.api_key = enc;
        }
    }
    for (_, val) in settings.post_process_api_keys.0.iter_mut() {
        if !val.is_empty() && !val.starts_with("enc:v1:") {
            if let Ok(enc) = crate::crypto::encrypt_str(val) {
                *val = enc;
            }
        }
    }
    for (_, val) in settings.brain.api_keys.0.iter_mut() {
        if !val.is_empty() && !val.starts_with("enc:v1:") {
            if let Ok(enc) = crate::crypto::encrypt_str(val) {
                *val = enc;
            }
        }
    }
    settings
}

fn decrypt_settings_keys(mut settings: AppSettings) -> AppSettings {
    if settings.tts.openai.api_key.starts_with("enc:v1:") {
        if let Ok(dec) = crate::crypto::decrypt_str(&settings.tts.openai.api_key) {
            settings.tts.openai.api_key = dec;
        }
    }
    if settings.tts.elevenlabs.api_key.starts_with("enc:v1:") {
        if let Ok(dec) = crate::crypto::decrypt_str(&settings.tts.elevenlabs.api_key) {
            settings.tts.elevenlabs.api_key = dec;
        }
    }
    if settings.tts.cartesia.api_key.starts_with("enc:v1:") {
        if let Ok(dec) = crate::crypto::decrypt_str(&settings.tts.cartesia.api_key) {
            settings.tts.cartesia.api_key = dec;
        }
    }
    for (_, val) in settings.post_process_api_keys.0.iter_mut() {
        if val.starts_with("enc:v1:") {
            if let Ok(dec) = crate::crypto::decrypt_str(val) {
                *val = dec;
            }
        }
    }
    for (_, val) in settings.brain.api_keys.0.iter_mut() {
        if val.starts_with("enc:v1:") {
            if let Ok(dec) = crate::crypto::decrypt_str(val) {
                *val = dec;
            }
        }
    }
    settings
}

pub fn load_or_create_app_settings(app: &AppHandle) -> AppSettings {
    // Initialize store
    let store = app
        .store(crate::portable::store_path(SETTINGS_STORE_PATH))
        .expect("Failed to initialize store");

    let mut settings = if let Some(settings_value) = store.get("settings") {
        // Parse the entire settings object
        match serde_json::from_value::<AppSettings>(settings_value) {
            Ok(mut settings) => {
                debug!("Found existing settings: {:?}", settings);
                settings = decrypt_settings_keys(settings);
                let default_settings = get_default_settings();
                let mut updated = false;

                // Merge default bindings into existing settings
                for (key, value) in default_settings.bindings {
                    if let std::collections::hash_map::Entry::Vacant(e) =
                        settings.bindings.entry(key)
                    {
                        debug!("Adding missing binding: {}", e.key());
                        e.insert(value);
                        updated = true;
                    }
                }

                if updated {
                    debug!("Settings updated with new bindings");
                    let encrypted = encrypt_settings_keys(settings.clone());
                    store.set("settings", serde_json::to_value(&encrypted).unwrap());
                }

                settings
            }
            Err(e) => {
                warn!("Failed to parse settings: {}", e);
                // Fall back to default settings if parsing fails
                let default_settings = get_default_settings();
                let encrypted = encrypt_settings_keys(default_settings.clone());
                store.set("settings", serde_json::to_value(&encrypted).unwrap());
                default_settings
            }
        }
    } else {
        let default_settings = get_default_settings();
        let encrypted = encrypt_settings_keys(default_settings.clone());
        store.set("settings", serde_json::to_value(&encrypted).unwrap());
        default_settings
    };

    let mut changed = ensure_post_process_defaults(&mut settings);
    changed |= ensure_post_process_actions(&mut settings);
    changed |= ensure_action_bindings(&mut settings);
    if changed {
        let encrypted = encrypt_settings_keys(settings.clone());
        store.set("settings", serde_json::to_value(&encrypted).unwrap());
    }

    settings
}

pub fn get_settings(app: &AppHandle) -> AppSettings {
    let store = app
        .store(crate::portable::store_path(SETTINGS_STORE_PATH))
        .expect("Failed to initialize store");

    let mut settings = if let Some(settings_value) = store.get("settings") {
        serde_json::from_value::<AppSettings>(settings_value)
            .map(|s| decrypt_settings_keys(s))
            .unwrap_or_else(|_| {
                let default_settings = get_default_settings();
                let encrypted = encrypt_settings_keys(default_settings.clone());
                store.set("settings", serde_json::to_value(&encrypted).unwrap());
                default_settings
            })
    } else {
        let default_settings = get_default_settings();
        let encrypted = encrypt_settings_keys(default_settings.clone());
        store.set("settings", serde_json::to_value(&encrypted).unwrap());
        default_settings
    };

    let mut updated = false;

    if settings.control_server_token.is_none() {
        let mut token_bytes = [0u8; 16];
        if getrandom::getrandom(&mut token_bytes).is_ok() {
            let token = token_bytes
                .iter()
                .map(|b| format!("{:02x}", b))
                .collect::<String>();
            log::info!("Generated control server token: {}", token);
            settings.control_server_token = Some(token);
            updated = true;
        }
    }

    let default_bindings = get_default_settings().bindings;
    for (key, value) in default_bindings {
        if !settings.bindings.contains_key(&key) {
            debug!("Adding missing binding: {}", key);
            settings.bindings.insert(key, value);
            updated = true;
        }
    }

    if ensure_post_process_defaults(&mut settings) {
        updated = true;
    }

    if ensure_post_process_actions(&mut settings) {
        updated = true;
    }

    if ensure_action_bindings(&mut settings) {
        updated = true;
    }

    if updated {
        let encrypted = encrypt_settings_keys(settings.clone());
        store.set("settings", serde_json::to_value(&encrypted).unwrap());
    }

    settings
}

pub fn write_settings(app: &AppHandle, settings: AppSettings) {
    let store = app
        .store(crate::portable::store_path(SETTINGS_STORE_PATH))
        .expect("Failed to initialize store");

    let encrypted = encrypt_settings_keys(settings);
    store.set("settings", serde_json::to_value(&encrypted).unwrap());
}

pub fn get_bindings(app: &AppHandle) -> HashMap<String, ShortcutBinding> {
    let settings = get_settings(app);

    settings.bindings
}

pub fn get_stored_binding(app: &AppHandle, id: &str) -> ShortcutBinding {
    let bindings = get_bindings(app);

    let binding = bindings.get(id).unwrap().clone();

    binding
}

pub fn get_history_limit(app: &AppHandle) -> usize {
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

    #[test]
    fn default_settings_disable_auto_submit() {
        let settings = get_default_settings();
        assert!(!settings.auto_submit);
        assert_eq!(settings.auto_submit_key, AutoSubmitKey::Enter);
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
}
