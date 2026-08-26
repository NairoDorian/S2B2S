//! Side-by-side measurement of the two VAD backends over real recordings.
//!
//!   HANDY_BENCH_WAV_DIR=<dir with 16 kHz mono WAVs> \
//!     cargo test --test vad_backend_bench -- --nocapture
//!
//! For every WAV it runs the raw detectors (Silero v6.2 through `ort`, Earshot
//! pure Rust) and the smoothed chains the app actually uses (streaming
//! policy: prefill / hangover / onset), and reports:
//!   - compute cost per frame and real-time factor (RTF)
//!   - raw voiced fraction and smoothed "kept" seconds per backend
//!   - frame-level agreement between the two raw verdicts (each 32 ms Silero
//!     frame is compared with the OR of the two 16 ms Earshot frames it covers)
//!
//! Skipped when the variable is unset. Thresholds match `managers/audio.rs`.
//! Note: `[profile.dev.package.earshot]` pins Earshot to opt-level 3 so a
//! debug test binary compares an optimised Earshot against the prebuilt ONNX
//! Runtime rather than an unoptimised one.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use handy_app_lib::audio_toolkit::{
    EarshotVad, SileroVad, VoiceActivityDetector,
    audio::read_wav_samples,
    constants::VAD_FRAME_SAMPLES,
    vad::{
        SmoothedVad, VAD_ONSET_MS, VAD_PREFILL_MS, VAD_STREAMING_HANGOVER_MS,
        earshot::EARSHOT_FRAME_SAMPLES, frames_for_duration_ms,
    },
};

const SILERO_THRESHOLD: f32 = 0.3;
const EARSHOT_THRESHOLD: f32 = 0.5;

struct RunStats {
    frames: usize,
    raw_voiced: usize,
    kept: usize,
    compute: Duration,
    raw_verdicts: Vec<bool>,
}

/// Run `raw` (for the per-frame verdict/timing) and a smoothed chain built on
/// `smoothed_inner` (for the kept seconds) over `samples`.
fn run_backend(
    samples: &[f32],
    frame_samples: usize,
    mut raw: Box<dyn VoiceActivityDetector>,
    smoothed_inner: Box<dyn VoiceActivityDetector>,
) -> RunStats {
    let mut smoothed = SmoothedVad::new(
        smoothed_inner,
        frames_for_duration_ms(VAD_PREFILL_MS, frame_samples),
        frames_for_duration_ms(VAD_STREAMING_HANGOVER_MS, frame_samples),
        frames_for_duration_ms(VAD_ONSET_MS, frame_samples),
    );
    let mut stats = RunStats {
        frames: 0,
        raw_voiced: 0,
        kept: 0,
        compute: Duration::ZERO,
        raw_verdicts: Vec::new(),
    };
    for chunk in samples.chunks(frame_samples) {
        if chunk.len() != frame_samples {
            break;
        }
        stats.frames += 1;
        let started = Instant::now();
        let raw_result = raw.push_frame(chunk);
        stats.compute += started.elapsed();
        let voiced = match raw_result {
            Ok(frame) => frame.is_speech(),
            Err(e) => panic!("raw VAD failed on frame {}: {e}", stats.frames),
        };
        stats.raw_verdicts.push(voiced);
        if voiced {
            stats.raw_voiced += 1;
        }
        if let Ok(frame) = smoothed.push_frame(chunk)
            && frame.is_speech()
        {
            stats.kept += 1;
        }
    }
    stats
}

/// Agreement between Silero's 32 ms verdicts and the OR of the two 16 ms
/// Earshot verdicts covering the same audio.
fn agreement(silero: &[bool], earshot: &[bool]) -> (usize, usize, usize, usize) {
    let (mut agree, mut silero_only, mut earshot_only, mut total) = (0, 0, 0, 0);
    for (i, &s) in silero.iter().enumerate() {
        let Some(&a) = earshot.get(2 * i) else { break };
        let b = earshot.get(2 * i + 1).copied().unwrap_or(false);
        let e = a || b;
        total += 1;
        match (s, e) {
            (true, true) | (false, false) => agree += 1,
            (true, false) => silero_only += 1,
            (false, true) => earshot_only += 1,
        }
    }
    (agree, silero_only, earshot_only, total)
}

#[test]
fn compare_silero_and_earshot_over_real_recordings() {
    let Ok(dir) = std::env::var("HANDY_BENCH_WAV_DIR") else {
        eprintln!("skipped: set HANDY_BENCH_WAV_DIR to a directory of 16 kHz mono WAVs");
        return;
    };
    let model = std::env::var("HANDY_PROBE_VAD_MODEL")
        .unwrap_or_else(|_| "target/debug/resources/models/silero_vad_v6.2.onnx".to_string());

    let mut wavs: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("read bench dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("wav")))
        .collect();
    wavs.sort();
    assert!(!wavs.is_empty(), "no WAV files in {dir}");

    let load_started = Instant::now();
    let _warm = SileroVad::new(&model, SILERO_THRESHOLD).expect("load silero");
    let silero_load = load_started.elapsed();
    let load_started = Instant::now();
    let _warm = EarshotVad::new(EARSHOT_THRESHOLD).expect("build earshot");
    let earshot_load = load_started.elapsed();
    eprintln!(
        "backend construction: silero(ort session) {:?}   earshot {:?}",
        silero_load, earshot_load
    );

    let sec = |frames: usize, frame_samples: usize| frames as f64 * frame_samples as f64 / 16000.0;
    let (mut tot_audio, mut tot_s_compute, mut tot_e_compute) =
        (0.0f64, Duration::ZERO, Duration::ZERO);
    let (mut tot_agree, mut tot_s_only, mut tot_e_only, mut tot_cmp) = (0, 0, 0, 0);
    let (mut tot_s_raw, mut tot_s_kept, mut tot_e_raw, mut tot_e_kept) = (0.0, 0.0, 0.0, 0.0);

    eprintln!(
        "\n{:<26} {:>7} | {:>9} {:>8} {:>7} {:>8} | {:>9} {:>8} {:>7} {:>8} | {:>6} {:>6} {:>6}",
        "file",
        "audio",
        "sil µs/f",
        "sil RTF",
        "sil %v",
        "sil kept",
        "ear µs/f",
        "ear RTF",
        "ear %v",
        "ear kept",
        "agree",
        "S-only",
        "E-only"
    );
    for wav in &wavs {
        let samples = read_wav_samples(wav).expect("read wav");
        let audio_secs = samples.len() as f64 / 16000.0;

        let s = run_backend(
            &samples,
            VAD_FRAME_SAMPLES,
            Box::new(SileroVad::new(&model, SILERO_THRESHOLD).unwrap()),
            Box::new(SileroVad::new(&model, SILERO_THRESHOLD).unwrap()),
        );
        let e = run_backend(
            &samples,
            EARSHOT_FRAME_SAMPLES,
            Box::new(EarshotVad::new(EARSHOT_THRESHOLD).unwrap()),
            Box::new(EarshotVad::new(EARSHOT_THRESHOLD).unwrap()),
        );
        let (agree, s_only, e_only, cmp) = agreement(&s.raw_verdicts, &e.raw_verdicts);

        let s_us = s.compute.as_secs_f64() * 1e6 / s.frames as f64;
        let e_us = e.compute.as_secs_f64() * 1e6 / e.frames as f64;
        let s_kept = sec(s.kept, VAD_FRAME_SAMPLES);
        let e_kept = sec(e.kept, EARSHOT_FRAME_SAMPLES);
        eprintln!(
            "{:<26} {:>6.1}s | {:>9.1} {:>8.5} {:>6.0}% {:>7.1}s | {:>9.2} {:>8.5} {:>6.0}% {:>7.1}s | {:>5.0}% {:>5.0}% {:>5.0}%",
            wav.file_name().unwrap().to_string_lossy(),
            audio_secs,
            s_us,
            s.compute.as_secs_f64() / audio_secs,
            100.0 * s.raw_voiced as f64 / s.frames as f64,
            s_kept,
            e_us,
            e.compute.as_secs_f64() / audio_secs,
            100.0 * e.raw_voiced as f64 / e.frames as f64,
            e_kept,
            100.0 * agree as f64 / cmp.max(1) as f64,
            100.0 * s_only as f64 / cmp.max(1) as f64,
            100.0 * e_only as f64 / cmp.max(1) as f64,
        );

        tot_audio += audio_secs;
        tot_s_compute += s.compute;
        tot_e_compute += e.compute;
        tot_agree += agree;
        tot_s_only += s_only;
        tot_e_only += e_only;
        tot_cmp += cmp;
        tot_s_raw += sec(s.raw_voiced, VAD_FRAME_SAMPLES);
        tot_s_kept += s_kept;
        tot_e_raw += sec(e.raw_voiced, EARSHOT_FRAME_SAMPLES);
        tot_e_kept += e_kept;
    }

    eprintln!(
        "\nTOTAL {:.1}s of audio\n  silero : compute {:?}  RTF {:.5}  raw speech {:.1}s  kept {:.1}s\n  earshot: compute {:?}  RTF {:.5}  raw speech {:.1}s  kept {:.1}s\n  speedup earshot/silero: {:.1}x\n  agreement (32 ms frames): {:.1}%  silero-only {:.1}%  earshot-only {:.1}%",
        tot_audio,
        tot_s_compute,
        tot_s_compute.as_secs_f64() / tot_audio,
        tot_s_raw,
        tot_s_kept,
        tot_e_compute,
        tot_e_compute.as_secs_f64() / tot_audio,
        tot_e_raw,
        tot_e_kept,
        tot_s_compute.as_secs_f64() / tot_e_compute.as_secs_f64().max(1e-9),
        100.0 * tot_agree as f64 / tot_cmp.max(1) as f64,
        100.0 * tot_s_only as f64 / tot_cmp.max(1) as f64,
        100.0 * tot_e_only as f64 / tot_cmp.max(1) as f64,
    );
}
