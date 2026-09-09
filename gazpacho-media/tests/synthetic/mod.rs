//! Correctness properties that only hold when a ground-truth `Spec` is
//! available, so they run over the spec-backed synthetic clips (the fixed
//! matrix plus the seeded random clips) rather than every kind of video.
//!
//! Every generated clip stamps its frame index into its pixels, so assertions
//! here are against `Spec` math (which frame *should* be visible at time `t`),
//! never against ffmpeg's opinion of its own output. These are the seek +
//! rational-time properties: metadata matches the spec, every frame recovers
//! its own index, mid-frame samples land on the covering frame, downscaling
//! preserves identity, and B-frame reordering never leaks.
//!
//! Universal properties that hold for *any* video — the metadata differential,
//! random-vs-sequential access, out-of-extent errors — live next to these in
//! `tests/properties.rs` and run over every kind of video, not just the
//! spec-backed ones.
//!
//! Note on containers: mkv/webm store timestamps in milliseconds, so for
//! NTSC rates the *container's* timestamps are rounded. The reader is expected
//! to reconstruct exact times from the rational frame rate — that discrepancy
//! is part of what these tests exist to surface. (The exact rational-fps
//! reconstruction itself is unit-tested next to `classify_timing` in
//! `src/metadata.rs`.)

use crate::{fixture_resolution, media_time, reader, recovered};
use eyre::{WrapErr as _, ensure};
use gazpacho_datatypes::Resolution;
use gazpacho_fixtures::video::{SyntheticVideo, Timing as SpecTiming};
use gazpacho_media::metadata::{self, MediaMetadata};
use gazpacho_media::read::{AccessPattern, ResolutionRequest};

/// Every probed field of the spec-backed clip agrees with the `Spec` that
/// generated it: resolution, frame count, start, extent, and timing (CFR vs
/// exact VFR timestamps).
pub(crate) fn metadata_matches_spec(video: &SyntheticVideo) -> eyre::Result<()> {
    let name = &video.name;
    let spec = &video.meta;
    let meta = MediaMetadata::load(&video.path).wrap_err_with(|| name.clone())?;
    let stream = meta
        .video
        .first()
        .ok_or_else(|| eyre::eyre!("{name}: no video stream probed"))?;

    ensure!(
        fixture_resolution(stream.resolution) == spec.resolution,
        "{name}: resolution"
    );
    ensure!(stream.frame_count == spec.frames, "{name}: frame_count");
    ensure!(
        stream.extent.start == media_time(spec.start_offset),
        "{name}: start"
    );
    let extent = spec.extent();
    ensure!(
        *stream.extent == (media_time(extent.start)..media_time(extent.end)).into(),
        "{name}: extent"
    );

    match &spec.timing {
        SpecTiming::Cfr { .. } => {
            ensure!(
                matches!(stream.timing, metadata::Timing::Constant(_)),
                "{name}: CFR fixture probed as variable frame rate"
            );
        }
        SpecTiming::Vfr { .. } => {
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

/// The core sweep: every frame queried at its exact timestamp identifies
/// itself. This is what makes seek + rational-time math correct by
/// construction.
pub(crate) fn every_frame_recovers_its_index(video: &SyntheticVideo) -> eyre::Result<()> {
    let mut reader = reader();
    for i in 0..video.meta.frames {
        let t = media_time(video.meta.timestamp_of(i));
        let frame = reader
            .frame(
                &video.path,
                t,
                ResolutionRequest::auto(),
                AccessPattern::Sequential,
            )
            .wrap_err_with(|| format!("{} frame {i} at t={t}", video.name))?;
        ensure!(recovered(video, &frame)? == i, "{} at t={t}", video.name);
    }
    Ok(())
}

/// Times strictly inside a frame's display window still return that frame —
/// callers sample at arbitrary times, not only on boundaries.
pub(crate) fn mid_frame_times_return_the_covering_frame(
    video: &SyntheticVideo,
) -> eyre::Result<()> {
    let mut reader = reader();
    let spec = &video.meta;
    for i in 0..spec.frames {
        let t = media_time(spec.timestamp_of(i) + spec.duration_of(i) / 3);
        let frame = reader
            .frame(
                &video.path,
                t,
                ResolutionRequest::auto(),
                AccessPattern::Sequential,
            )
            .wrap_err_with(|| format!("{} frame {i}", video.name))?;
        ensure!(recovered(video, &frame)? == i, "{} at t={t}", video.name);
    }
    Ok(())
}

/// Requested resolution is honored exactly, and the stamp survives scaling
/// (it's read by relative position).
pub(crate) fn downscaling_preserves_identity(video: &SyntheticVideo) -> eyre::Result<()> {
    let mut reader = reader();
    let t = media_time(video.meta.timestamp_of(7));
    for (width, height) in [(80, 60), (64, 48), (24, 18)] {
        let resolution = Resolution { width, height };
        let frame = reader.frame(
            &video.path,
            t,
            ResolutionRequest::Manual(resolution),
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
pub(crate) fn bframe_reordering_is_invisible(video: &SyntheticVideo) -> eyre::Result<()> {
    let name = &video.name;
    let spec = &video.meta;
    let mut reader = reader();
    for i in 0..spec.frames {
        let t = media_time(spec.timestamp_of(i));
        let frame = reader
            .frame(
                &video.path,
                t,
                ResolutionRequest::auto(),
                AccessPattern::Sequential,
            )
            .wrap_err_with(|| format!("{name} frame {i}"))?;
        ensure!(recovered(video, &frame)? == i, "{name} at t={t}");
    }
    Ok(())
}
