//! TDD spec for the gazpacho-media reader, driven by the spec-backed test
//! videos (the fixed synthetic matrix plus the seeded random clips).
//!
//! Run with: `cargo test -p gazpacho-media --features fixtures`
//! (plain `cargo test` skips this file entirely).
//!
//! Every generated clip stamps its frame index into its pixels, so assertions
//! here are against `Spec` math (which frame *should* be visible at time `t`),
//! never against ffmpeg's opinion of its own output.
//!
//! Universal properties that don't need a spec (metadata differential,
//! random-vs-sequential access, out-of-extent errors, ...) live in
//! `tests/properties.rs` and run over every kind of video, not just these.
//!
//! Note on containers: mkv/webm store timestamps in milliseconds, so for
//! NTSC rates the *container's* timestamps are rounded. The reader is
//! expected to reconstruct exact times from the rational frame rate — that
//! discrepancy is part of what these tests exist to surface. (The exact
//! rational-fps reconstruction itself is unit-tested next to
//! `classify_timing` in `src/metadata.rs`.)

#![cfg(feature = "fixtures")]

use gazpacho_media::fixtures::{self, Timing, reader, recovered};
use gazpacho_media::metadata::{self, MediaMetadata};
use gazpacho_media::read::{AccessPattern, Resolution, ResolutionRequest};

#[test]
fn metadata_matches_spec() {
    fixtures::init_tracing();
    for (video, spec) in fixtures::videos().spec_backed() {
        let name = &video.name;
        let meta =
            MediaMetadata::load(video.path_str()).unwrap_or_else(|err| panic!("{name}: {err}"));
        let stream = &meta.video[0];

        assert_eq!(stream.resolution, spec.resolution, "{name}");
        assert_eq!(stream.frame_count, spec.frames, "{name}");
        assert_eq!(stream.start, spec.start_offset, "{name}: start");
        assert_eq!(stream.extent(), spec.extent(), "{name}: extent");

        match &spec.timing {
            Timing::Cfr { .. } => {
                assert!(
                    matches!(stream.timing, metadata::Timing::Constant(_)),
                    "{name}: CFR fixture probed as variable frame rate"
                );
            }
            Timing::Vfr { .. } => {
                let metadata::Timing::Variable(timestamps) = &stream.timing else {
                    panic!("{name}: VFR fixture probed as constant frame rate");
                };
                assert_eq!(timestamps.len(), spec.frames as usize, "{name}");
                for (i, &ts) in timestamps.iter().enumerate() {
                    assert_eq!(ts, spec.timestamp_of(i as u32), "{name} frame {i}");
                }
            }
        }
    }
}

/// The core sweep: for every spec-backed video, every frame queried at its
/// exact timestamp identifies itself. This is what makes seek +
/// rational-time math correct by construction.
#[test]
fn every_frame_recovers_its_index() {
    for (video, spec) in fixtures::videos().spec_backed() {
        let reader = reader();
        for i in 0..spec.frames {
            let t = spec.timestamp_of(i);
            let frame = reader
                .frame(
                    video.path_str(),
                    t,
                    ResolutionRequest::auto(),
                    // TODO: Test random access pattern.
                    AccessPattern::Sequential,
                )
                .unwrap_or_else(|err| panic!("{} frame {i} at t={t}: {err}", video.name));
            assert_eq!(recovered(video, &frame), i, "{} at t={t}", video.name);
        }
    }
}

/// Times strictly inside a frame's display window still return that frame —
/// callers sample at arbitrary times, not only on boundaries.
#[test]
fn mid_frame_times_return_the_covering_frame() {
    let reader = reader();
    let videos = fixtures::videos();
    for video in [videos.baseline(), videos.expect("vfr_h264")] {
        let spec = video.expect_spec();
        for i in 0..spec.frames {
            let t = spec.timestamp_of(i).advance_secs(spec.duration_of(i) / 3);
            let frame = reader
                .frame(
                    video.path_str(),
                    t,
                    // TODO: Test manual resolution requests.
                    ResolutionRequest::auto(),
                    // TODO: Test random access pattern.
                    AccessPattern::Sequential,
                )
                .unwrap_or_else(|err| panic!("{} frame {i}: {err}", video.name));
            assert_eq!(recovered(video, &frame), i, "{} at t={t}", video.name);
        }
    }
}

/// Requested resolution is honored exactly, and the stamp survives scaling
/// (it's read by relative position).
#[test]
fn downscaling_preserves_identity() {
    let reader = reader();
    let video = fixtures::videos().baseline();
    let t = video.expect_spec().timestamp_of(7);
    for (width, height) in [(80, 60), (64, 48), (24, 18)] {
        let resolution = Resolution { width, height };
        let frame = reader
            .frame(
                video.path_str(),
                t,
                // TODO: Test auto downsampling.
                ResolutionRequest::Manual(resolution),
                // TODO: Test different access patterns.
                AccessPattern::Sequential,
            )
            .unwrap();

        assert_eq!(frame.resolution(), resolution);
        assert_eq!(recovered(video, &frame), 7, "at {resolution:?}");
    }
}

/// B-frame files store frames out of order (decode order != presentation
/// order, negative DTS, mp4 edit lists). None of that may leak: a forward
/// sweep still yields 0, 1, 2, ...
#[test]
fn bframe_reordering_is_invisible() {
    let videos = fixtures::videos();
    for name in ["h264_bf2", "h264_bf2_offset", "h264_bf2_ts"] {
        let video = videos.expect(name);
        let spec = video.expect_spec();
        let reader = reader();
        for i in 0..spec.frames {
            let t = spec.timestamp_of(i);
            let frame = reader
                .frame(
                    video.path_str(),
                    t,
                    ResolutionRequest::auto(),
                    // TODO: Test non-sequential access patterns.
                    AccessPattern::Sequential,
                )
                .unwrap_or_else(|err| panic!("{name} frame {i}: {err}"));
            assert_eq!(recovered(video, &frame), i, "{name} at t={t}");
        }
    }
}
