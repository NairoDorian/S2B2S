//! Native streaming latency presets — maps a user-facing latency preset
//! (`Fastest` → `Accurate`) onto the concrete `transcribe_cpp` `StreamExtension`
//! each model family supports, when the model advertises one.
//!
//! The settings field `native_streaming_latency_presets` stores the per-model
//! choice (keyed by model id); this module just translates kind + preset into
//! the extension struct the session's `stream()` call consumes.

use super::model::NativeStreamingLatencyKind;
use crate::settings::NativeStreamingLatencyPreset;
use log::{error, info, warn};
use transcribe_cpp::{
    ExtSlot, Model, ParakeetBufferedStreamOptions, ParakeetStreamOptions, R2T2StreamOptions,
    StreamExtension, sys,
};

/// Inclusive bounds of the R2T2 native streaming chunk size, in whole
/// milliseconds.
///
/// These mirror `k_r2t2_chunk_ms_min` / `k_r2t2_chunk_ms_max` in the fork's
/// `src/arch/qwen3_asr/r2t2-package.h` and are the ONLY place in the app that
/// knows them. They exist so the settings command can reject an out-of-range
/// value at the input boundary and so the UI can clamp its slider; the
/// authoritative check is still the native `stream_begin`, which rejects rather
/// than clamps (an out-of-range value there fails the stream and leaves the
/// previous transcript untouched). Keeping a copy here is a deliberate
/// duplication: the app must not send a value it already knows is invalid, but
/// it must also not become the source of truth for the C side's range.
pub const R2T2_CHUNK_MS_MIN: u32 = 80;
pub const R2T2_CHUNK_MS_MAX: u32 = 2000;
/// Matches `k_r2t2_chunk_ms_default` in the same header. Applied when a model
/// has no persisted value, so an absent settings entry behaves exactly like the
/// pre-existing users' installs (which have no entry at all).
pub const R2T2_CHUNK_MS_DEFAULT: u32 = 320;

/// Whether `ms` is a chunk size the native side will accept. Used by the
/// settings command (reject) and the UI (clamp); see the constants above.
pub fn r2t2_chunk_ms_is_valid(ms: u32) -> bool {
    (R2T2_CHUNK_MS_MIN..=R2T2_CHUNK_MS_MAX).contains(&ms)
}

/// Encoder frame of every shipped FastConformer streaming variant (Nemotron
/// 3.5, Nemotron Speech, Parakeet Unified): 10 ms mel hop × 8× subsampling.
/// The native side converts every latency knob of those families through it.
const FASTCONFORMER_FRAME_MS: u32 = 80;

/// What one latency setting means in milliseconds of audio, so the UI and the
/// benchmark can state the setting in the unit R2T2 already uses instead of a
/// word.
///
/// * `latency_ms` is the audio the model must have before it can emit a
///   chunk's text: R2T2's chunk, Nemotron's chunk `(right + 1) × 80 ms`,
///   Parakeet Unified's `chunk + right`. It is the number NVIDIA's model cards
///   call the latency of each trained setting.
/// * `lookahead_ms` is the part of it that is future context
///   (`att_context_right × 80 ms`, or the buffered `right_ms`); 0 for R2T2,
///   whose chunk has no separate lookahead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LatencyPoint {
    pub latency_ms: u32,
    pub lookahead_ms: u32,
}

/// Right-context menus the Nemotron checkpoints were trained on, in encoder
/// frames, ordered like the presets (`Fastest`, `Fast`, `Balanced`,
/// `Accurate`). `Accurate` is the model default (the first entry of
/// `att_context_size_choices`, R = 13) that `extension_for_kind` leaves to the
/// runtime. Sources: the fork's `docs/models/nemotron-3.5-asr-streaming-0.6b.md`
/// (`{0, 3, 6, 13}`) and `nemotron-speech-streaming-en-0.6b.md` (`{0, 1, 6, 13}`).
const fn nemotron_right_frames(
    kind: NativeStreamingLatencyKind,
    preset: NativeStreamingLatencyPreset,
) -> u32 {
    match preset {
        NativeStreamingLatencyPreset::Fastest => 0,
        NativeStreamingLatencyPreset::Fast => match kind {
            NativeStreamingLatencyKind::NemotronSpeechCacheAware => 1,
            _ => 3,
        },
        NativeStreamingLatencyPreset::Balanced => 6,
        NativeStreamingLatencyPreset::Accurate => 13,
    }
}

/// Parakeet Unified `(chunk_ms, right_ms)` per preset. `Accurate` is the model
/// default tuple `(70, 13, 13)` frames = 1040 / 1040 ms that the runtime picks
/// when no extension is attached.
const fn parakeet_buffered_chunk_right_ms(preset: NativeStreamingLatencyPreset) -> (u32, u32) {
    match preset {
        NativeStreamingLatencyPreset::Fastest => (160, 160),
        NativeStreamingLatencyPreset::Fast => (160, 320),
        NativeStreamingLatencyPreset::Balanced => (560, 560),
        NativeStreamingLatencyPreset::Accurate => (1040, 1040),
    }
}

/// The millisecond meaning of a latency setting. `chunk_ms` is read only for
/// R2T2 and `preset` only for the preset families, like
/// [`stream_extension_for`]; an out-of-range R2T2 value reports the native
/// default it falls back to.
pub fn latency_point(
    kind: NativeStreamingLatencyKind,
    preset: NativeStreamingLatencyPreset,
    chunk_ms: u32,
) -> LatencyPoint {
    match kind {
        NativeStreamingLatencyKind::R2T2ChunkMs => LatencyPoint {
            latency_ms: if r2t2_chunk_ms_is_valid(chunk_ms) {
                chunk_ms
            } else {
                R2T2_CHUNK_MS_DEFAULT
            },
            lookahead_ms: 0,
        },
        NativeStreamingLatencyKind::ParakeetBuffered => {
            let (chunk, right) = parakeet_buffered_chunk_right_ms(preset);
            LatencyPoint {
                latency_ms: chunk + right,
                lookahead_ms: right,
            }
        }
        NativeStreamingLatencyKind::Nemotron35CacheAware
        | NativeStreamingLatencyKind::NemotronSpeechCacheAware => {
            let right = nemotron_right_frames(kind, preset);
            LatencyPoint {
                latency_ms: (right + 1) * FASTCONFORMER_FRAME_MS,
                lookahead_ms: right * FASTCONFORMER_FRAME_MS,
            }
        }
    }
}

/// Translate a `(model_kind, preset)` pair into the raw extension kind + options
/// the model should receive.
///
/// `None` from `extension_for_kind` means "don't attach an extension at all" —
/// currently only the `Accurate` preset, which defers to the model's own
/// runtime default, produces no extension.
fn extension_for_kind(
    kind: NativeStreamingLatencyKind,
    preset: NativeStreamingLatencyPreset,
) -> Option<(u32, StreamExtension)> {
    if preset == NativeStreamingLatencyPreset::Accurate {
        return None;
    }

    match kind {
        // R2T2 has no presets: it cannot be reached through this function's
        // (kind, preset) contract at all. Resolving it here would mean inventing
        // a preset -> millisecond table, which is exactly what the R2T2
        // integration contract forbids. `stream_extension_for` routes R2T2 to
        // `r2t2_stream_extension` before it ever gets here, so reaching this arm
        // is a programming error, not a user-reachable state.
        NativeStreamingLatencyKind::R2T2ChunkMs => unreachable!(
            "R2T2 chunk size is a free millisecond value; route through \
             stream_extension_for / r2t2_stream_extension"
        ),
        // Both families read the tables `latency_point` reports from, so the
        // milliseconds the UI and the benchmark show are the values sent.
        NativeStreamingLatencyKind::ParakeetBuffered => {
            let (chunk_ms, right_ms) = parakeet_buffered_chunk_right_ms(preset);
            Some((
                sys::TRANSCRIBE_EXT_KIND_PARAKEET_BUFFERED_STREAM,
                StreamExtension::ParakeetBuffered(ParakeetBufferedStreamOptions {
                    left_ms: Some(5600),
                    chunk_ms: Some(chunk_ms as i32),
                    right_ms: Some(right_ms as i32),
                }),
            ))
        }
        NativeStreamingLatencyKind::Nemotron35CacheAware
        | NativeStreamingLatencyKind::NemotronSpeechCacheAware => Some((
            sys::TRANSCRIBE_EXT_KIND_PARAKEET_STREAM,
            StreamExtension::ParakeetStream(ParakeetStreamOptions {
                att_context_right: Some(nemotron_right_frames(kind, preset) as i32),
            }),
        )),
    }
}

/// Resolve the stream extension for a given model + model id + preset lookup.
///
/// * `kind` comes from `ModelInfo::native_streaming_latency_kind` (catalog
///   hint — `None` for non-streaming models).
/// * `preset` is read from `settings.native_streaming_latency_presets`.
///
/// Returns `None` when the model has no latency extension, when the user is on
/// the `Accurate` preset, or when the runtime rejects the extension kind for
/// this particular model build.
///
/// Preset families only: [`stream_extension_for`] routes R2T2 to
/// `r2t2_stream_extension` before reaching here.
fn stream_extension(
    model: &Model,
    model_id: &str,
    kind: Option<NativeStreamingLatencyKind>,
    preset: NativeStreamingLatencyPreset,
) -> Option<StreamExtension> {
    let kind = kind?;
    let (extension_kind, extension) = extension_for_kind(kind, preset)?;
    if model.accepts_ext(ExtSlot::Stream, extension_kind) {
        Some(extension)
    } else {
        warn!(
            "Native streaming latency preset {:?} ignored for model '{}': runtime rejected extension kind {:#x}",
            preset, model_id, extension_kind
        );
        None
    }
}

/// Resolve the R2T2 streaming chunk size into its native extension.
///
/// Unlike the preset families this carries the *exact* millisecond value the
/// user chose — no tier lookup, no rounding, no substitution of a preset. The
/// value is copied by the native `stream_begin`, so it applies to the next
/// stream and cannot disturb decoder state already accumulated.
///
/// An out-of-range value is refused here rather than clamped or mapped: the
/// extension is omitted, an error naming the offending value is logged, and the
/// session falls back to the native default (`R2T2_CHUNK_MS_DEFAULT`), which is
/// itself in range. This path is only reachable from a settings file that was
/// hand-edited past the command's validation — a stream that fails outright
/// would take the user's live session down over a stale config, so the safe
/// floor is preferred to a hard failure. The native side still rejects the
/// value independently if it ever does arrive (see `r2t2_chunk_ms_is_valid`).
fn r2t2_stream_extension(model: &Model, model_id: &str, chunk_ms: u32) -> Option<StreamExtension> {
    if !model.accepts_ext(ExtSlot::Stream, sys::TRANSCRIBE_EXT_KIND_R2T2_STREAM) {
        warn!(
            "R2T2 chunk size {} ms ignored for model '{}': runtime rejected extension kind {:#x}",
            chunk_ms,
            model_id,
            sys::TRANSCRIBE_EXT_KIND_R2T2_STREAM
        );
        return None;
    }
    if !r2t2_chunk_ms_is_valid(chunk_ms) {
        error!(
            "R2T2 chunk size {} ms for model '{}' is outside {}..={} ms; using the native default of {} ms",
            chunk_ms, model_id, R2T2_CHUNK_MS_MIN, R2T2_CHUNK_MS_MAX, R2T2_CHUNK_MS_DEFAULT
        );
        return None;
    }
    // Log requested *and* resolved on the success path too, not just the failure
    // paths above. The integration contract requires the resolved cadence to be
    // visible in the app log alongside the native-side timing line, because the
    // only way to tell "the slider did nothing" from "the model could not keep
    // up with the cadence" is to see what the decoder was actually asked for.
    // One line per stream begin, so it cannot spam a live session.
    info!(
        "R2T2 streaming chunk size: requested={} ms resolved={} ms for model '{}'",
        chunk_ms, chunk_ms, model_id
    );
    Some(StreamExtension::R2T2(R2T2StreamOptions {
        chunk_size_ms: Some(chunk_ms),
    }))
}

/// Resolve the stream extension for *any* native-streaming model — the single
/// entry point both the primary and the Multi-STT streaming paths call.
///
/// Keeping one dispatcher is what guarantees the two paths agree: the R2T2
/// branch cannot be wired into one call site and forgotten in the other, which
/// would silently run the second path at the native default. `chunk_ms` is
/// ignored for the preset families and read only for `R2T2ChunkMs`.
pub fn stream_extension_for(
    model: &Model,
    model_id: &str,
    kind: Option<NativeStreamingLatencyKind>,
    preset: NativeStreamingLatencyPreset,
    chunk_ms: u32,
) -> Option<StreamExtension> {
    match kind? {
        NativeStreamingLatencyKind::R2T2ChunkMs => r2t2_stream_extension(model, model_id, chunk_ms),
        preset_kind => stream_extension(model, model_id, Some(preset_kind), preset),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accurate_uses_the_runtime_default() {
        assert!(
            extension_for_kind(
                NativeStreamingLatencyKind::ParakeetBuffered,
                NativeStreamingLatencyPreset::Accurate,
            )
            .is_none()
        );
    }

    #[test]
    fn fast_tier_is_model_specific_for_nemotron() {
        let (_, nemotron_35) = extension_for_kind(
            NativeStreamingLatencyKind::Nemotron35CacheAware,
            NativeStreamingLatencyPreset::Fast,
        )
        .unwrap();
        let (_, nemotron_speech) = extension_for_kind(
            NativeStreamingLatencyKind::NemotronSpeechCacheAware,
            NativeStreamingLatencyPreset::Fast,
        )
        .unwrap();

        assert_eq!(
            nemotron_35,
            StreamExtension::ParakeetStream(ParakeetStreamOptions {
                att_context_right: Some(3),
            })
        );
        assert_eq!(
            nemotron_speech,
            StreamExtension::ParakeetStream(ParakeetStreamOptions {
                att_context_right: Some(1),
            })
        );
    }

    /// The milliseconds the UI and the benchmark show for each preset are the
    /// latencies of the trained settings on the model cards, and they agree
    /// with the extension actually sent.
    #[test]
    fn latency_points_match_the_trained_menus() {
        use NativeStreamingLatencyKind as K;
        use NativeStreamingLatencyPreset as P;
        let presets = [P::Fastest, P::Fast, P::Balanced, P::Accurate];
        let ms = |kind| presets.map(|p| latency_point(kind, p, 0).latency_ms);
        let ahead = |kind| presets.map(|p| latency_point(kind, p, 0).lookahead_ms);

        assert_eq!(ms(K::Nemotron35CacheAware), [80, 320, 560, 1120]);
        assert_eq!(ahead(K::Nemotron35CacheAware), [0, 240, 480, 1040]);
        assert_eq!(ms(K::NemotronSpeechCacheAware), [80, 160, 560, 1120]);
        assert_eq!(ahead(K::NemotronSpeechCacheAware), [0, 80, 480, 1040]);
        assert_eq!(ms(K::ParakeetBuffered), [320, 480, 1120, 2080]);

        for kind in [K::Nemotron35CacheAware, K::NemotronSpeechCacheAware] {
            for preset in [P::Fastest, P::Fast, P::Balanced] {
                let (_, ext) = extension_for_kind(kind, preset).unwrap();
                let StreamExtension::ParakeetStream(options) = ext else {
                    panic!("Nemotron must use the cache-aware extension");
                };
                let right = options.att_context_right.unwrap() as u32;
                assert_eq!(latency_point(kind, preset, 0).lookahead_ms, right * 80);
            }
        }
    }

    #[test]
    fn r2t2_latency_point_is_the_chunk_or_the_native_default() {
        let point = |ms| {
            latency_point(
                NativeStreamingLatencyKind::R2T2ChunkMs,
                NativeStreamingLatencyPreset::Fastest,
                ms,
            )
        };
        assert_eq!(point(160).latency_ms, 160);
        assert_eq!(point(160).lookahead_ms, 0);
        assert_eq!(point(5000).latency_ms, R2T2_CHUNK_MS_DEFAULT);
    }

    /// The point of R2T2's control is that the user can reach the *lowest*
    /// latency the model supports, so the inclusive lower bound is a contract,
    /// not an implementation detail: `80` must be accepted and `79` rejected.
    /// This also pins the default inside the range, since a default the resolver
    /// refuses would silently fall back to the native default on every stream.
    ///
    /// The bounds are asserted literally as well as relationally: the range is
    /// mirrored from the fork's `k_r2t2_chunk_ms_*` and duplicated in the UI, so
    /// a one-sided edit in any of the three places should fail a test here rather
    /// than quietly narrow the range users can select.
    #[test]
    fn r2t2_chunk_bounds_admit_the_lowest_latency_and_reject_outside() {
        assert_eq!(R2T2_CHUNK_MS_MIN, 80);
        assert_eq!(R2T2_CHUNK_MS_MAX, 2000);
        assert_eq!(R2T2_CHUNK_MS_DEFAULT, 320);

        assert!(r2t2_chunk_ms_is_valid(R2T2_CHUNK_MS_MIN), "80 ms must work");
        assert!(
            r2t2_chunk_ms_is_valid(R2T2_CHUNK_MS_MAX),
            "2000 ms must work"
        );
        assert!(
            r2t2_chunk_ms_is_valid(R2T2_CHUNK_MS_DEFAULT),
            "the fallback default must itself be in range"
        );
        assert!(
            (R2T2_CHUNK_MS_MIN..=R2T2_CHUNK_MS_MAX).contains(&R2T2_CHUNK_MS_DEFAULT),
            "the default must lie inside the range"
        );

        assert!(!r2t2_chunk_ms_is_valid(R2T2_CHUNK_MS_MIN - 1));
        assert!(!r2t2_chunk_ms_is_valid(R2T2_CHUNK_MS_MAX + 1));
        assert!(!r2t2_chunk_ms_is_valid(0));
    }
}
