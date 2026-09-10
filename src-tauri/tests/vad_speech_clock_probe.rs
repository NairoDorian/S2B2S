//! End-to-end check of the real VAD chain (Earshot + SmoothedVad) over a real
//! recording, asserting that the raw per-frame verdict the speech clock depends
//! on actually tracks speech.
//!
//!   HANDY_PROBE_WAV=<path> cargo test --test vad_speech_clock_probe -- --nocapture

use handy_app_lib::audio_toolkit::{
    EarshotVad, VoiceActivityDetector,
    audio::read_wav_samples,
    constants::VAD_FRAME_SAMPLES,
    vad::{
        SmoothedVad, VAD_ONSET_MS, VAD_PREFILL_MS, VAD_STREAMING_HANGOVER_MS,
        frames_for_duration_ms,
    },
};

#[test]
fn real_vad_chain_reports_speech_over_real_audio() {
    let Ok(wav) = std::env::var("HANDY_PROBE_WAV") else {
        eprintln!("skipped: set HANDY_PROBE_WAV to a 16kHz mono speech recording");
        return;
    };
    let samples = read_wav_samples(&wav).expect("read wav");
    let earshot = EarshotVad::new(0.5).expect("construct earshot");
    let prefill_frames = frames_for_duration_ms(VAD_PREFILL_MS, VAD_FRAME_SAMPLES);
    let streaming_hangover_frames =
        frames_for_duration_ms(VAD_STREAMING_HANGOVER_MS, VAD_FRAME_SAMPLES);
    let onset_frames = frames_for_duration_ms(VAD_ONSET_MS, VAD_FRAME_SAMPLES);
    let mut vad: Box<dyn VoiceActivityDetector> = Box::new(SmoothedVad::new(
        Box::new(earshot),
        prefill_frames,
        streaming_hangover_frames,
        onset_frames,
    ));

    let (mut frames, mut kept, mut raw_voiced, mut errors) = (0usize, 0usize, 0usize, 0usize);
    for chunk in samples.chunks(VAD_FRAME_SAMPLES) {
        if chunk.len() != VAD_FRAME_SAMPLES {
            break;
        }
        frames += 1;
        match vad.push_frame(chunk) {
            Ok(f) => {
                if f.is_speech() {
                    kept += 1;
                }
            }
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
    assert!(
        raw_voiced * 100 / frames.max(1) >= 30,
        "raw voiced frames far too low ({raw_voiced}/{frames}) for continuous speech"
    );
    assert!(
        raw_voiced < frames,
        "every frame voiced - VAD is not discriminating"
    );
}
