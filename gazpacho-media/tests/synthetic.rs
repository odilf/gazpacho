//! TDD spec for the gazpacho-media reader, driven by the synthetic corpus.
//!
//! Run with: `cargo test -p gazpacho-media --features fixtures`
//! (plain `cargo test` skips this file entirely).
//!
//! The corpus generates itself on first use into `target/synthetic-fixtures/`
//! and is cached across runs — no build step or ordering needed. Every
//! fixture stamps its frame index into its pixels, so assertions here are
//! against `Spec` math (which frame *should* be visible at time `t`), never
//! against ffmpeg's opinion of its own output.
//!
//! Note on containers: mkv/webm store timestamps in milliseconds, so for
//! NTSC rates the *container's* timestamps are rounded. The reader is
//! expected to reconstruct exact times from the rational frame rate — that
//! discrepancy is part of what these tests exist to surface.

#![cfg(feature = "fixtures")]

use gazpacho_media::fixtures::{self, Timing, reader, recovered};
use gazpacho_media::metadata::{self, MediaMetadata};
use gazpacho_media::read::{AccessPattern, Resolution, ResolutionRequest};
use num_rational::Ratio;

#[test]
fn metadata_matches_spec() {
    fixtures::init_tracing();
    for fixture in fixtures::corpus().all() {
        let name = &fixture.spec.name;
        let meta =
            MediaMetadata::load(fixture.path_str()).unwrap_or_else(|err| panic!("{name}: {err}"));
        let video = &meta.video[0];

        assert_eq!(video.resolution, fixture.spec.resolution, "{name}");
        assert_eq!(video.frame_count, fixture.spec.frames, "{name}");
        assert_eq!(video.start, fixture.spec.start_offset, "{name}: start");
        assert_eq!(video.extent(), fixture.spec.extent(), "{name}: extent");

        match &fixture.spec.timing {
            Timing::Cfr { fps } => {
                let metadata::Timing::Constant(probed) = &video.timing else {
                    panic!("{name}: CFR fixture probed as variable frame rate");
                };
                assert_eq!(probed.get(), *fps, "{name}: exact rational fps");
            }
            Timing::Vfr { .. } => {
                let metadata::Timing::Variable(timestamps) = &video.timing else {
                    panic!("{name}: VFR fixture probed as constant frame rate");
                };
                assert_eq!(timestamps.len(), fixture.spec.frames as usize, "{name}");
                for (i, &ts) in timestamps.iter().enumerate() {
                    assert_eq!(ts, fixture.spec.timestamp_of(i as u32), "{name} frame {i}");
                }
            }
        }
    }
}

#[test]
fn extent_covers_the_spec() {
    let reader = reader();
    for fixture in fixtures::corpus().all() {
        let extent = reader
            .extent(fixture.path_str())
            .unwrap_or_else(|err| panic!("{}: {err}", fixture.spec.name));
        assert_eq!(extent, fixture.spec.extent(), "{}", fixture.spec.name);
    }
}

/// The core sweep: for every fixture, every frame queried at its exact
/// timestamp identifies itself. This is what makes seek + rational-time math
/// correct by construction.
#[test]
fn every_frame_recovers_its_index() {
    for fixture in fixtures::corpus().all() {
        let reader = reader();
        for i in 0..fixture.spec.frames {
            let t = fixture.spec.timestamp_of(i);
            let frame = reader
                .frame(
                    fixture.path_str(),
                    t,
                    ResolutionRequest::auto(),
                    // TODO: Test random access pattern.
                    AccessPattern::Sequential,
                )
                .unwrap_or_else(|err| panic!("{} frame {i} at t={t}: {err}", fixture.spec.name));
            assert_eq!(
                recovered(fixture, &frame),
                i,
                "{} at t={t}",
                fixture.spec.name
            );
        }
    }
}

/// Times strictly inside a frame's display window still return that frame —
/// callers sample at arbitrary times, not only on boundaries.
#[test]
fn mid_frame_times_return_the_covering_frame() {
    let reader = reader();
    let corpus = fixtures::corpus();
    for fixture in [corpus.baseline(), corpus.expect("vfr_h264")] {
        for i in 0..fixture.spec.frames {
            let t = fixture
                .spec
                .timestamp_of(i)
                .advance_secs(fixture.spec.duration_of(i) / 3);
            let frame = reader
                .frame(
                    fixture.path_str(),
                    t,
                    // TODO: Test manual resolution requests.
                    ResolutionRequest::auto(),
                    // TODO: Test random access pattern.
                    AccessPattern::Sequential,
                )
                .unwrap_or_else(|err| panic!("{} frame {i}: {err}", fixture.spec.name));
            assert_eq!(
                recovered(fixture, &frame),
                i,
                "{} at t={t}",
                fixture.spec.name
            );
        }
    }
}

/// Scrambled access order: crossing chunk boundaries backwards and forwards
/// must not change what a time maps to. Exercises chunking + the LRU cache,
/// including a long-GOP file where a chunk never ends on a keyframe.
#[test]
fn random_access_matches_sequential() {
    let corpus = fixtures::corpus();
    for name in [fixtures::BASELINE, "h264_420p_g250_30", "vfr_h264"] {
        let fixture = corpus.expect(name);
        let reader = reader();
        let n = fixture.spec.frames;
        // 37 is coprime with the frame count: visits every index, scrambled.
        for k in 0..n {
            let i = (k * 37) % n;
            let t = fixture.spec.timestamp_of(i);
            let frame = reader
                .frame(
                    fixture.path_str(),
                    t,
                    // TODO: Test manual resolution requests.
                    ResolutionRequest::auto(),
                    AccessPattern::Sequential,
                )
                .unwrap_or_else(|err| panic!("{name} frame {i}: {err}"));
            assert_eq!(recovered(fixture, &frame), i, "{name} at t={t}");
        }
    }
}

/// Requested resolution is honored exactly, and the stamp survives scaling
/// (it's read by relative position).
#[test]
fn downscaling_preserves_identity() {
    let reader = reader();
    let fixture = fixtures::corpus().baseline();
    let t = fixture.spec.timestamp_of(7);
    for (width, height) in [(80, 60), (64, 48), (24, 18)] {
        let resolution = Resolution { width, height };
        let frame = reader
            .frame(
                fixture.path_str(),
                t,
                // TODO: Test auto downsampling.
                ResolutionRequest::Manual(resolution),
                // TODO: Test different access patterns.
                AccessPattern::Sequential,
            )
            .unwrap();

        assert_eq!(frame.resolution(), resolution);
        assert_eq!(recovered(fixture, &frame), 7, "at {resolution:?}");
    }
}

/// B-frame files store frames out of order (decode order != presentation
/// order, negative DTS, mp4 edit lists). None of that may leak: a forward
/// sweep still yields 0, 1, 2, ...
#[test]
fn bframe_reordering_is_invisible() {
    let corpus = fixtures::corpus();
    for name in ["h264_bf2", "h264_bf2_offset", "h264_bf2_ts"] {
        let fixture = corpus.expect(name);
        let reader = reader();
        for i in 0..fixture.spec.frames {
            let t = fixture.spec.timestamp_of(i);
            let frame = reader
                .frame(
                    fixture.path_str(),
                    t,
                    ResolutionRequest::auto(),
                    // TODO: Test non-sequential access patterns.
                    AccessPattern::Sequential,
                )
                .unwrap_or_else(|err| panic!("{name} frame {i}: {err}"));
            assert_eq!(recovered(fixture, &frame), i, "{name} at t={t}");
        }
    }
}

#[test]
fn out_of_extent_is_an_error() {
    let reader = reader();
    let fixture = fixtures::corpus().baseline();
    let end = fixture.spec.extent().end;
    // The extent is half-open: `end` itself is already outside.
    for t in [end, end.advance_secs(Ratio::from_integer(1))] {
        let result = reader.frame(
            fixture.path_str(),
            t,
            ResolutionRequest::auto(),
            // TODO: Test non-sequential access pattern.
            AccessPattern::Sequential,
        );
        assert!(result.is_err(), "t={t} should be out of extent");
    }
}
