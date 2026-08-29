//! TDD spec for the gazpacho-media reader, driven by the spec-backed test
//! videos (the fixed synthetic matrix plus the seeded random clips).
//!
//! Every generated clip stamps its frame index into its pixels, so assertions
//! here are against `Spec` math (which frame *should* be visible at time `t`),
//! never against ffmpeg's opinion of its own output.
//!
//! Custom libtest-mimic harness: `main` enumerates the registry up front
//! (generating any missing fixtures — the build step) and emits one test
//! case per (property × video), run in parallel and filterable by name, e.g.
//! `cargo test -p gazpacho-media --test synthetic bf2`. Works the same under
//! `cargo nextest run`, where each case gets its own process.
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

mod common;

use common::{media_time, reader, recovered};
use eyre::{WrapErr as _, ensure};
use gazpacho_datatypes::Resolution;
use gazpacho_fixtures::{self as fixtures, Spec, TestVideo, Timing};
use gazpacho_media::metadata::{self, MediaMetadata};
use gazpacho_media::read::{AccessPattern, ResolutionRequest};
use libtest_mimic::{Arguments, Trial};

fn main() -> eyre::Result<()> {
    let args = Arguments::from_args();
    fixtures::init_tracing_stderr();
    let registry = fixtures::videos();

    let mut trials = Vec::new();
    for (video, spec) in registry.spec_backed() {
        trials.push(trial("metadata_matches_spec", video, move || {
            metadata_matches_spec(video, spec)
        }));
        trials.push(trial("every_frame_recovers_its_index", video, move || {
            every_frame_recovers_its_index(video, spec)
        }));
    }
    for video in [registry.baseline()?, registry.expect("vfr_h264")?] {
        trials.push(trial(
            "mid_frame_times_return_the_covering_frame",
            video,
            move || mid_frame_times_return_the_covering_frame(video),
        ));
    }
    let baseline = registry.baseline()?;
    trials.push(trial(
        "downscaling_preserves_identity",
        baseline,
        move || downscaling_preserves_identity(baseline),
    ));
    for name in ["h264_bf2", "h264_bf2_offset", "h264_bf2_ts"] {
        let video = registry.expect(name)?;
        trials.push(trial("bframe_reordering_is_invisible", video, move || {
            bframe_reordering_is_invisible(video)
        }));
    }
    libtest_mimic::run(&args, trials).exit();
}

fn trial(
    family: &str,
    video: &TestVideo,
    run: impl FnOnce() -> eyre::Result<()> + Send + 'static,
) -> Trial {
    Trial::test(format!("{family}::{}", video.name), move || {
        run().map_err(|err| format!("{err:?}").into())
    })
}

fn metadata_matches_spec(video: &TestVideo, spec: &Spec) -> eyre::Result<()> {
    let name = &video.name;
    let meta = MediaMetadata::load(video.path_str()).wrap_err_with(|| name.clone())?;
    let stream = meta
        .video
        .first()
        .ok_or_else(|| eyre::eyre!("{name}: no video stream probed"))?;

    ensure!(
        common::fixture_resolution(stream.resolution) == spec.resolution,
        "{name}: resolution"
    );
    ensure!(stream.frame_count == spec.frames, "{name}: frame_count");
    ensure!(
        stream.start == media_time(spec.start_offset),
        "{name}: start"
    );
    let extent = spec.extent();
    ensure!(
        *stream.extent() == (media_time(extent.start)..media_time(extent.end)).into(),
        "{name}: extent"
    );

    match &spec.timing {
        Timing::Cfr { .. } => {
            ensure!(
                matches!(stream.timing, metadata::Timing::Constant(_)),
                "{name}: CFR fixture probed as variable frame rate"
            );
        }
        Timing::Vfr { .. } => {
            let metadata::Timing::Variable(timestamps) = &stream.timing else {
                eyre::bail!("{name}: VFR fixture probed as constant frame rate");
            };
            ensure!(timestamps.len() == spec.frames as usize, "{name}");
            for (i, &ts) in timestamps.iter().enumerate() {
                ensure!(
                    ts == media_time(spec.timestamp_of(i as u32)),
                    "{name} frame {i}"
                );
            }
        }
    }
    Ok(())
}

/// The core sweep: for every spec-backed video, every frame queried at its
/// exact timestamp identifies itself. This is what makes seek +
/// rational-time math correct by construction.
fn every_frame_recovers_its_index(video: &TestVideo, spec: &Spec) -> eyre::Result<()> {
    let reader = reader();
    for i in 0..spec.frames {
        let t = media_time(spec.timestamp_of(i));
        let frame = reader
            .frame(
                video.path_str(),
                t,
                ResolutionRequest::auto(),
                // TODO: Test random access pattern.
                AccessPattern::Sequential,
            )
            .wrap_err_with(|| format!("{} frame {i} at t={t}", video.name))?;
        ensure!(recovered(video, &frame)? == i, "{} at t={t}", video.name);
    }
    Ok(())
}

/// Times strictly inside a frame's display window still return that frame —
/// callers sample at arbitrary times, not only on boundaries.
fn mid_frame_times_return_the_covering_frame(video: &TestVideo) -> eyre::Result<()> {
    let reader = reader();
    let spec = video.expect_spec()?;
    for i in 0..spec.frames {
        let t = media_time(spec.timestamp_of(i) + spec.duration_of(i) / 3);
        let frame = reader
            .frame(
                video.path_str(),
                t,
                // TODO: Test manual resolution requests.
                ResolutionRequest::auto(),
                // TODO: Test random access pattern.
                AccessPattern::Sequential,
            )
            .wrap_err_with(|| format!("{} frame {i}", video.name))?;
        ensure!(recovered(video, &frame)? == i, "{} at t={t}", video.name);
    }
    Ok(())
}

/// Requested resolution is honored exactly, and the stamp survives scaling
/// (it's read by relative position).
fn downscaling_preserves_identity(video: &TestVideo) -> eyre::Result<()> {
    let reader = reader();
    let t = media_time(video.expect_spec()?.timestamp_of(7));
    for (width, height) in [(80, 60), (64, 48), (24, 18)] {
        let resolution = Resolution { width, height };
        let frame = reader.frame(
            video.path_str(),
            t,
            // TODO: Test auto downsampling.
            ResolutionRequest::Manual(resolution),
            // TODO: Test different access patterns.
            AccessPattern::Sequential,
        )?;

        ensure!(frame.resolution() == resolution, "at {resolution:?}");
        ensure!(recovered(video, &frame)? == 7, "at {resolution:?}");
    }
    Ok(())
}

/// B-frame files store frames out of order (decode order != presentation
/// order, negative DTS, mp4 edit lists). None of that may leak: a forward
/// sweep still yields 0, 1, 2, ...
fn bframe_reordering_is_invisible(video: &TestVideo) -> eyre::Result<()> {
    let name = &video.name;
    let spec = video.expect_spec()?;
    let reader = reader();
    for i in 0..spec.frames {
        let t = media_time(spec.timestamp_of(i));
        let frame = reader
            .frame(
                video.path_str(),
                t,
                ResolutionRequest::auto(),
                // TODO: Test non-sequential access patterns.
                AccessPattern::Sequential,
            )
            .wrap_err_with(|| format!("{name} frame {i}"))?;
        ensure!(recovered(video, &frame)? == i, "{name} at t={t}");
    }
    Ok(())
}
