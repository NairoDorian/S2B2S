use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug, Clone, Default)]
// `env!` rather than the `app_identity` constants: an attribute needs a literal
// or a macro that expands to one, and these two are already derived from
// `Cargo.toml`, which `scripts/app-meta.ts` mirrors from the same source. The
// name is the binary name, which is what `--help` should show.
#[command(
    name = env!("CARGO_PKG_NAME"),
    about = env!("CARGO_PKG_DESCRIPTION")
)]
pub struct CliArgs {
    /// Start with the main window hidden
    #[arg(long)]
    pub start_hidden: bool,

    /// Disable the system tray icon
    #[arg(long)]
    pub no_tray: bool,

    /// Toggle transcription on/off (sent to running instance)
    #[arg(long)]
    pub toggle_transcription: bool,

    /// Toggle transcription with post-processing on/off (sent to running instance)
    #[arg(long)]
    pub toggle_post_process: bool,

    /// Cancel the current operation (sent to running instance)
    #[arg(long)]
    pub cancel: bool,

    /// Enable debug mode with verbose logging
    #[arg(long)]
    pub debug: bool,

    /// Transcribe this mono WAV (16/24-bit PCM or 32-bit float, any sample
    /// rate — resampled to 16 kHz) headlessly and exit. Runs the same batch
    /// transcription path as the app — no mic, no VAD, no download (the model
    /// must already be installed).
    #[arg(short = 'f', long, value_name = "WAV")]
    pub transcribe_file: Option<PathBuf>,

    /// Model id to load for --transcribe-file (default: the selected model).
    #[arg(long)]
    pub model: Option<String>,

    /// Hard-select the compute device for --transcribe-file by its registry
    /// index (see --list-devices). Omit to use the persisted accelerator
    /// setting. transcribe-cpp (whisper-family) models only.
    #[arg(long, value_name = "N")]
    pub device_index: Option<usize>,

    /// Replay a WAV through native streaming, with N-ms feeds (no microphone/UI).
    #[arg(long, requires = "transcribe_file", value_parser = clap::value_parser!(u32).range(1..=10000))]
    pub stream_chunk_ms: Option<u32>,

    /// Nemotron right context for headless streaming (otherwise use app preset).
    #[arg(long, requires = "stream_chunk_ms")]
    pub stream_att_right: Option<i32>,

    /// List the transcribe-cpp compute devices (with indices) and exit.
    #[arg(long)]
    pub list_devices: bool,

    /// List the available models (with ids) and exit. Pass an id to --model.
    /// Honors --json for machine-readable output.
    #[arg(long)]
    pub list_models: bool,

    /// Benchmark runs (must be 3): discard warm-up, average runs 2 and 3.
    #[arg(long, value_name = "N")]
    pub repeat: Option<usize>,

    /// Emit --transcribe-file results as JSON.
    #[arg(long)]
    pub json: bool,
}
