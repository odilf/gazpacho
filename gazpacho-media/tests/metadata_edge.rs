//! Metadata probing on files the synthetic `Spec` corpus can't express:
//! a trimming edit list (discard-flagged packets), an audio track, and cover
//! art. Run with: `cargo test -p gazpacho-media --features fixtures`.

#![cfg(feature = "fixtures")]

use std::path::Path;

use gazpacho_media::fixtures;
use gazpacho_media::metadata::MediaMetadata;

/// A trimming edit list keeps the frames before the cut in the container (as
/// discard-flagged, pre-roll packets) even though they never present. Metadata
/// must reflect what *plays*: the discarded packets are excluded from the frame
/// count, the start is the presentation zero (not the negative pre-roll), and
/// keyframe indices are renumbered against the presented frames.
#[test]
fn trimming_edit_list_excludes_discarded_frames() {
    fixtures::init_tracing();
    let baseline = fixtures::corpus().baseline();
    let resolution = baseline.spec.resolution;
    let total = baseline.spec.frames;

    let path = fixtures::trimmed_baseline();
    let path = path.to_str().unwrap();

    // Ground truth via an independent decode (the edit list is applied, so only
    // presented frames come out); every frame announces its original index.
    let frames = fixtures::decode_all_rgba(Path::new(path), resolution).unwrap();
    let presented: Vec<u32> = frames
        .iter()
        .map(|f| fixtures::recover_index(f).unwrap())
        .collect();
    let first = presented[0];
    assert!(first > 0, "seek should trim into the stream, past frame 0");
    assert!((frames.len() as u32) < total, "trim should drop whole frames");
    assert_eq!(
        presented,
        (first..total).collect::<Vec<_>>(),
        "presented frames are a contiguous tail of the original"
    );

    let meta = MediaMetadata::load(path).unwrap();
    let video = &meta.video[0];

    // The whole point: discarded packets are NOT counted. Without discard
    // handling this would be the raw packet count (== `total`), not `frames.len()`.
    assert_eq!(video.frame_count as usize, frames.len());
    // Presentation starts at zero, not at the discarded packets' negative pre-roll.
    assert_eq!(video.start, baseline.spec.start_offset);

    // Keyframes renumber against presented frames: the original keyframes at or
    // after the cut, shifted down by `first`. (The first presented frame is
    // mid-GOP, so index 0 is deliberately not a keyframe here.)
    let expected_keyframes: Vec<u32> = (0..total)
        .step_by(baseline.spec.gop as usize)
        .filter(|&k| k >= first)
        .map(|k| k - first)
        .collect();
    assert_eq!(&*video.keyframes, expected_keyframes.as_slice());
    assert_ne!(video.keyframes.first(), Some(&0));
}

/// An audio track is probed into `AudioMetadata` alongside the video.
#[test]
fn audio_stream_is_probed() {
    fixtures::init_tracing();
    let path = fixtures::baseline_with_audio();
    let meta = MediaMetadata::load(path.to_str().unwrap()).unwrap();

    assert_eq!(meta.video.len(), 1);
    assert_eq!(meta.audio.len(), 1);
    let audio = &meta.audio[0];
    assert_eq!(audio.sample_rate, 44100);
    assert_eq!(audio.audio_stream_index, 0);
    assert_eq!(audio.stream_index, 1, "audio is the second container stream");
    assert!(audio.length > 0.0, "audio length should be probed");
}

/// Cover art is an `attached_pic` video stream — a single embedded still, not a
/// real track — and must not surface as a `VideoMetadata`.
#[test]
fn attached_picture_is_skipped() {
    fixtures::init_tracing();
    let path = fixtures::baseline_with_cover_art();
    let meta = MediaMetadata::load(path.to_str().unwrap()).unwrap();

    assert_eq!(
        meta.video.len(),
        1,
        "the attached_pic stream should be ignored, leaving one real video stream"
    );
    assert_eq!(meta.video[0].frame_count, 60);
    assert_eq!(meta.video[0].video_stream_index, 0);
}
