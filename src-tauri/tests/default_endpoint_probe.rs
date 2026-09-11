//! Opt-in probe of the concrete default microphone endpoint: resolves it,
//! reads its default config and captures from it for a moment. Needs a
//! microphone, so it skips unless asked for:
//!
//! ```text
//! HANDY_PROBE_MIC=1 cargo test --test default_endpoint_probe -- --nocapture
//! ```
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use handy_app_lib::audio_toolkit::audio::default_input_endpoint;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

#[test]
fn concrete_default_input_resolves_and_captures() {
    if std::env::var_os("HANDY_PROBE_MIC").is_none() {
        eprintln!("HANDY_PROBE_MIC not set; skipping");
        return;
    }
    let endpoint = default_input_endpoint().expect("a default input endpoint");
    let handle_name = cpal::default_host()
        .default_input_device()
        .and_then(|d| d.description().ok())
        .map(|d| d.name().to_string());
    eprintln!(
        "endpoint: index={} name={:?} (default handle name {:?})",
        endpoint.index, endpoint.name, handle_name
    );
    assert_eq!(Some(endpoint.name.clone()), handle_name);
    assert_ne!(endpoint.index, "default", "expected an enumerated endpoint");

    let config = endpoint
        .device
        .default_input_config()
        .expect("default input config of the concrete endpoint");
    eprintln!(
        "config: {} Hz, {} ch, {:?}",
        config.sample_rate(),
        config.channels(),
        config.sample_format()
    );
    let samples = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&samples);
    let stream = endpoint
        .device
        .build_input_stream(
            config.into(),
            move |data: &[f32], _| {
                counter.fetch_add(data.len(), Ordering::Relaxed);
            },
            |e| eprintln!("stream error: {e}"),
            None,
        )
        .expect("input stream on the concrete endpoint");
    stream.play().expect("play");
    let started = Instant::now();
    while samples.load(Ordering::Relaxed) == 0 && started.elapsed() < Duration::from_secs(3) {
        std::thread::sleep(Duration::from_millis(20));
    }
    let n = samples.load(Ordering::Relaxed);
    eprintln!("captured {n} samples in {:?}", started.elapsed());
    assert!(
        n > 0,
        "no samples arrived from the concrete default endpoint"
    );
}
