//! Differential ("property") test: the fast packet-based `load` must agree with
//! a full decode (`load_by_decode`, using the decoder's `best_effort_timestamp`)
//! on every fixture and edge case. This guards the packet shortcut — including
//! discard-flag handling — against silently drifting from the ground truth the
//! decoder sees. Run with: `cargo test -p gazpacho-media --features fixtures`.

#![cfg(feature = "fixtures")]

use gazpacho_media::fixtures;
use gazpacho_media::metadata::MediaMetadata;

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
        assert_eq!(a.keyframes, b.keyframes, "{label} v{i}: keyframes");
        assert_eq!(a.stream_index, b.stream_index, "{label} v{i}: stream_index");
        assert_eq!(
            a.video_stream_index, b.video_stream_index,
            "{label} v{i}: video_stream_index"
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

/// Every fixture in the corpus: packets vs. full decode.
#[test]
fn packets_agree_with_full_decode_across_corpus() {
    fixtures::init_tracing();
    for fixture in fixtures::corpus().all() {
        let path = fixture.path_str();
        let fast = MediaMetadata::load(path).unwrap();
        let slow = MediaMetadata::load_by_decode(path).unwrap();
        assert_agree(&fixture.spec.name, &fast, &slow);
    }
}

/// The derived edge cases — trimming edit list, audio, cover art — where the two
/// paths are most likely to diverge if discard/attached-pic handling regressed.
#[test]
fn packets_agree_with_full_decode_on_edge_cases() {
    fixtures::init_tracing();
    let files = [
        ("trimmed", fixtures::trimmed_baseline()),
        ("with_audio", fixtures::baseline_with_audio()),
        ("with_cover", fixtures::baseline_with_cover_art()),
    ];
    for (label, path) in &files {
        let path = path.to_str().unwrap();
        let fast = MediaMetadata::load(path).unwrap();
        let slow = MediaMetadata::load_by_decode(path).unwrap();
        assert_agree(label, &fast, &slow);
    }
}
