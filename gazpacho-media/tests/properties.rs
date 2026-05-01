//! Universal property suite: these hold for *every* registered test video —
//! synthetic, random, derived, and real-world alike — using only probed
//! metadata as ground truth (no spec required). Spec-exact assertions live in
//! `tests/synthetic.rs`.
//!
//! Custom libtest-mimic harness: `main` enumerates the registry up front
//! (generating any missing fixtures — the build step) and emits one test
//! case per (property × video), run in parallel and filterable by name, e.g.
//! `cargo test -p gazpacho-media --test properties vfr_h264`. Works the same
//! under `cargo nextest run`, where each case gets its own process.
//!
//! Cut the video set down while iterating with
//! `GAZPACHO_TEST_VIDEOS=sample:<N>[:<seed>]`; point
//! `GAZPACHO_REAL_VIDEOS_DIR` at a directory of your own files to include
//! them.

mod common;

use std::hash::{DefaultHasher, Hasher};

use common::{assert_frames_eq, fixture_resolution, probed_timestamps, reader};
use eyre::{WrapErr as _, ensure};
use gazpacho_fixtures::{self as fixtures, TestVideo};
use gazpacho_media::metadata::MediaMetadata;
use gazpacho_media::read::{AccessPattern, Frame, ResolutionRequest};
use libtest_mimic::{Arguments, Trial};
use num_rational::Ratio;

/// Cap on per-video frame sweeps so arbitrarily long real-world files stay
/// tractable. A no-op for the generated clips.
const FRAME_CAP: usize = 240;
/// Tighter cap where a decoded reference is held in memory (a cap of 240
/// 1080p RGBA frames would be gigabytes).
const REFERENCE_CAP: usize = 60;

type Property = (&'static str, fn(&TestVideo) -> eyre::Result<()>);

fn main() {
    let args = Arguments::from_args();
    fixtures::init_tracing_stderr();
    let registry = fixtures::videos();

    let properties: &[Property] = &[
        ("metadata_loads", metadata_loads),
        (
            "fast_load_agrees_with_full_decode",
            fast_load_agrees_with_full_decode,
        ),
        ("extent_is_self_consistent", extent_is_self_consistent),
        (
            "sequential_read_matches_reference_decode",
            sequential_read_matches_reference_decode,
        ),
        (
            "random_access_matches_sequential",
            random_access_matches_sequential,
        ),
        ("out_of_extent_is_an_error", out_of_extent_is_an_error),
    ];

    let mut trials = Vec::new();
    for video in registry.all() {
        for &(name, property) in properties {
            trials.push(Trial::test(format!("{name}::{}", video.name), move || {
                property(video).map_err(|err| format!("{err:?}").into())
            }));
        }
    }
    libtest_mimic::run(&args, trials).exit();
}

fn metadata_loads(video: &TestVideo) -> eyre::Result<()> {
    let name = &video.name;
    let meta = MediaMetadata::load(video.path_str()).wrap_err_with(|| name.clone())?;
    ensure!(!meta.video.is_empty(), "{name}: no video stream probed");
    for stream in &meta.video {
        ensure!(stream.frame_count > 0, "{name}: empty stream");
        ensure!(stream.start < stream.end, "{name}: degenerate extent");
    }
    Ok(())
}

/// The fast packet-based `load` must agree with a full decode
/// (`load_by_decode`, using the decoder's `best_effort_timestamp`) on every
/// video. This guards the packet shortcut — including discard-flag handling —
/// against silently drifting from the ground truth the decoder sees.
fn fast_load_agrees_with_full_decode(video: &TestVideo) -> eyre::Result<()> {
    let path = video.path_str();
    let fast = MediaMetadata::load(path)?;
    let slow = MediaMetadata::load_by_decode(path)?;
    assert_agree(&video.name, &fast, &slow);
    Ok(())
}

/// Assert two probes produced identical metadata, field by field.
#[track_caller]
fn assert_agree(label: &str, fast: &MediaMetadata, slow: &MediaMetadata) {
    assert_eq!(
        fast.video.len(),
        slow.video.len(),
        "{label}: video stream count"
    );
    for (i, (a, b)) in fast.video.iter().zip(&slow.video).enumerate() {
        assert_eq!(a.resolution, b.resolution, "{label} v{i}: resolution");
        assert_eq!(a.frame_count, b.frame_count, "{label} v{i}: frame_count");
        assert_eq!(a.start, b.start, "{label} v{i}: start");
        assert_eq!(a.end, b.end, "{label} v{i}: end");
        assert_eq!(a.timing, b.timing, "{label} v{i}: timing");
        // `keyframes` is deliberately NOT compared: container sync flags lie
        // in both directions (the Chromium corpus has fragmented mp4s whose
        // keyframe is marked non-sync and vice versa), so the packet path
        // can't always match decoder truth. Keyframes are a seek hint, and
        // whatever consumes them must tolerate lying containers anyway.
        assert_eq!(a.stream_index, b.stream_index, "{label} v{i}: stream_index");
        assert_eq!(
            a.parent_stream_index, b.parent_stream_index,
            "{label} v{i}: parent_stream_index"
        );
    }

    assert_eq!(
        fast.audio.len(),
        slow.audio.len(),
        "{label}: audio stream count"
    );
    for (i, (a, b)) in fast.audio.iter().zip(&slow.audio).enumerate() {
        assert_eq!(a.sample_rate, b.sample_rate, "{label} a{i}: sample_rate");
        assert_eq!(a.stream_index, b.stream_index, "{label} a{i}: stream_index");
    }
}

/// The reader's extent must equal the probed metadata's extent.
fn extent_is_self_consistent(video: &TestVideo) -> eyre::Result<()> {
    let name = &video.name;
    let extent = reader()
        .extent(video.path_str())
        .wrap_err_with(|| name.clone())?;
    let meta = MediaMetadata::load(video.path_str())?;
    let stream = meta.video.first().ok_or_else(|| eyre::eyre!("{name}: no video stream probed"))?;
    ensure!(extent == stream.extent(), "{name}");
    Ok(())
}

/// The reader, queried at each probed frame timestamp in order, must return
/// exactly what an independent ffmpeg pipe decodes — for any video, spec or
/// not.
fn sequential_read_matches_reference_decode(video: &TestVideo) -> eyre::Result<()> {
    let name = &video.name;
    let meta = MediaMetadata::load(video.path_str())?;
    let stream = meta.video.first().ok_or_else(|| eyre::eyre!("{name}: no video stream probed"))?;
    let reference = fixtures::decode_rgba_prefix(
        &video.path,
        stream.stream_index,
        fixture_resolution(stream.resolution),
        REFERENCE_CAP,
    )
    .wrap_err_with(|| format!("{name}: reference decode"))?;
    let times = probed_timestamps(stream, REFERENCE_CAP);
    ensure!(
        times.len() == reference.len(),
        "{name}: reference frame count"
    );

    let reader = reader();
    for (i, (t, expected)) in times.iter().zip(&reference).enumerate() {
        let frame = reader
            .frame(
                video.path_str(),
                *t,
                ResolutionRequest::Manual(stream.resolution),
                AccessPattern::Sequential,
            )
            .wrap_err_with(|| format!("{name} frame {i} at t={t}"))?;
        assert_frames_eq(&format!("{name} frame {i} at t={t}"), &frame, expected);
    }
    Ok(())
}

/// Scrambled access order must not change what a time maps to — a
/// reader-vs-reader property needing no ground truth. Exercises chunking and
/// caching across every kind of file.
fn random_access_matches_sequential(video: &TestVideo) -> eyre::Result<()> {
    let name = &video.name;
    let meta = MediaMetadata::load(video.path_str())?;
    let stream = meta.video.first().ok_or_else(|| eyre::eyre!("{name}: no video stream probed"))?;
    let times = probed_timestamps(stream, FRAME_CAP);
    let n = times.len();

    let read = |reader: &gazpacho_media::MediaReader, i: usize| -> eyre::Result<Frame> {
        let t = times.get(i).expect("i is always < times.len()");
        reader
            .frame(
                video.path_str(),
                *t,
                ResolutionRequest::auto(),
                AccessPattern::Sequential,
            )
            .wrap_err_with(|| format!("{name} frame {i}"))
    };

    // Hashes, not frames: real-world files would not fit in memory.
    let first_pass = reader();
    let sequential: Vec<u64> = (0..n)
        .map(|i| Ok(frame_hash(&read(&first_pass, i)?)))
        .collect::<eyre::Result<_>>()?;

    // 37 is coprime with most frame counts: visits every index, scrambled.
    let second_pass = reader();
    for k in 0..n {
        let i = (k * 37) % n;
        let expected = sequential
            .get(i)
            .expect("i = (k * 37) % n is always < n == sequential.len()");
        ensure!(
            frame_hash(&read(&second_pass, i)?) == *expected,
            "{name} frame {i}"
        );
    }
    Ok(())
}

fn frame_hash(frame: &Frame) -> u64 {
    use std::hash::Hash;
    let mut hasher = DefaultHasher::new();
    frame.hash(&mut hasher);
    hasher.finish()
}

fn out_of_extent_is_an_error(video: &TestVideo) -> eyre::Result<()> {
    let name = &video.name;
    let reader = reader();
    let extent = reader.extent(video.path_str())?;
    // The extent is half-open: `end` itself is already outside.
    for t in [extent.end, extent.end.advance_secs(Ratio::from_integer(1))] {
        let result = reader.frame(
            video.path_str(),
            t,
            ResolutionRequest::auto(),
            AccessPattern::Sequential,
        );
        ensure!(result.is_err(), "{name}: t={t} should be out of extent");
    }
    Ok(())
}
