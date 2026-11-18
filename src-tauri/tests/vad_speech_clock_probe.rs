//! End-to-end check of the real VAD chain (the engine's Earshot speech gate,
//! with the streaming hangover) over a real recording, asserting that the raw
//! per-frame verdict the speech clock depends on actually tracks speech.
//!
//!   <PROBE_WAV flag>=<path> cargo test --test vad_speech_clock_probe -- --nocapture
//!
//! `app_lib::app_env_var("PROBE_WAV")` resolves the flag's full name, so the
//! command line above is the one the run itself would print.

use app_lib::app_env_var;
use app_lib::audio_toolkit::{
    SpeechGate, VoiceActivityDetector,
    audio::read_wav_samples,
    constants::VAD_FRAME_SAMPLES,
    vad::{VAD_STREAMING_HANGOVER_MS, VadFrame, exit_threshold},
};

#[test]
fn real_vad_chain_reports_speech_over_real_audio() {
    // Read through the app's own accessor rather than `env::var` with a literal
    // name: the prefix is part of the identity, and a probe that spelled it
    // would be one more place a rename has to find.
    let Some(wav) = app_env_var("PROBE_WAV") else {
        eprintln!("skipped: set the PROBE_WAV flag to a 16kHz mono speech recording");
        return;
    };
    let samples = read_wav_samples(&wav).expect("read wav");
    let mut vad: Box<dyn VoiceActivityDetector> = Box::new(
        SpeechGate::new(0.5, VAD_STREAMING_HANGOVER_MS).expect("construct the speech gate"),
    );

    let (mut frames, mut kept, mut raw_voiced, mut errors) = (0usize, 0usize, 0usize, 0usize);
    for chunk in samples.chunks(VAD_FRAME_SAMPLES) {
        if chunk.len() != VAD_FRAME_SAMPLES {
            break;
        }
        frames += 1;
        match vad.push_frame(chunk) {
            // An onset returns its pre-roll together with the current
            // frame, so count the frames it carries, not the call.
            Ok(VadFrame::Speech(buf)) => kept += buf.len() / VAD_FRAME_SAMPLES,
            Ok(VadFrame::Noise) => {}
            Err(e) => {
                if errors == 0 {
                    eprintln!("first push_frame error: {e}");
                }
                errors += 1;
            }
        }
        if vad.last_frame_voiced() {
            raw_voiced += 1;
        }
    }

    let secs = |n: usize| n as f64 * VAD_FRAME_SAMPLES as f64 / 16000.0;
    eprintln!(
        "frames={frames} errors={errors} kept(smoothed)={kept} raw_voiced={raw_voiced}\n\
         total={:.2}s  kept={:.2}s  raw speech={:.2}s ({:.0}%)",
        secs(frames),
        secs(kept),
        secs(raw_voiced),
        100.0 * raw_voiced as f64 / frames as f64
    );

    assert_eq!(errors, 0, "VAD errored on {errors}/{frames} frames");
    assert!(kept <= frames, "the gate released a frame twice");
    assert!(
        raw_voiced * 100 / frames.max(1) >= 30,
        "raw voiced frames far too low ({raw_voiced}/{frames}) for continuous speech"
    );
    assert!(
        raw_voiced < frames,
        "every frame voiced - VAD is not discriminating"
    );
}

/// The VAD slider moved mid-recording: both edges of the two-threshold rule
/// must move with it. Raised to 0.7, the gate leaves speech below 0.55 (not
/// the 0.35 it started with); lowered to 0.3, it leaves below 0.15 (not at
/// 0.3, which would collapse the hysteresis into a single edge).
#[test]
fn threshold_changes_move_both_edges_over_real_audio() {
    let Some(wav) = app_env_var("PROBE_WAV") else {
        eprintln!("skipped: set the PROBE_WAV flag to a 16kHz mono speech recording");
        return;
    };
    let samples = read_wav_samples(&wav).expect("read wav");
    let (frames, _) = samples.as_chunks::<VAD_FRAME_SAMPLES>();
    let mut vad = SpeechGate::new(0.5, VAD_STREAMING_HANGOVER_MS).expect("speech gate");
    let (mut threshold, mut in_speech) = (0.5f32, false);
    // Frames a stale exit edge (the construction's 0.35, kept below the new
    // threshold) would have decided differently: the case under test.
    let mut discriminating = 0usize;
    let third = frames.len() / 3;
    for (i, frame) in frames.iter().enumerate() {
        if i == third || i == 2 * third {
            threshold = if i == third { 0.7 } else { 0.3 };
            vad.set_threshold(threshold);
            in_speech = false;
        }
        vad.push_frame(frame).expect("frame");
        let score = vad.last_frame_score().expect("score");
        if in_speech {
            let stale = exit_threshold(0.5).min(threshold);
            discriminating += usize::from((score >= stale) != (score >= exit_threshold(threshold)));
        }
        in_speech = if in_speech {
            score >= exit_threshold(threshold)
        } else {
            score >= threshold
        };
        assert_eq!(
            vad.last_frame_voiced(),
            in_speech,
            "frame {i}: score {score}, threshold {threshold}"
        );
    }
    eprintln!(
        "{} frames, {discriminating} decided by the moved exit edge",
        frames.len()
    );
}
