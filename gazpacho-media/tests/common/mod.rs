//! Helpers shared by the integration-test harnesses: the glue that needs
//! gazpacho-media types and therefore can't live in the (deliberately
//! independent) fixtures crate.

#![allow(dead_code)] // each harness uses a different subset

use gazpacho_fixtures::{self as fixtures, TestVideo};
use gazpacho_media::MediaReader;
use gazpacho_media::metadata::{MediaTime, Timing, VideoMetadata};
use gazpacho_media::read::{Frame, Resolution};
use num_rational::Ratio;

pub fn reader() -> MediaReader {
    fixtures::init_tracing();
    MediaReader::default()
}

/// Seconds from the fixtures' spec math as a reader timestamp.
pub fn media_time(seconds: Ratio<i64>) -> MediaTime {
    MediaTime::from_secs(seconds)
}

/// A reader resolution as the fixtures type (for reference decodes).
pub fn fixture_resolution(resolution: Resolution) -> fixtures::Resolution {
    fixtures::Resolution {
        width: resolution.width,
        height: resolution.height,
    }
}

/// The index stamped in a decoded frame; panics with context if unreadable.
#[track_caller]
pub fn recovered(video: &TestVideo, frame: &Frame) -> u32 {
    assert!(
        video.spec.is_some(),
        "{}: only spec-backed videos carry frame stamps",
        video.name
    );
    fixtures::recover_index(fixture_resolution(frame.resolution()), frame.data())
        .unwrap_or_else(|err| panic!("{}: unreadable stamp: {err}", video.name))
}

/// Assert a reader frame and a reference-decoded fixtures frame are
/// pixel-identical.
#[track_caller]
pub fn assert_frames_eq(label: &str, frame: &Frame, reference: &fixtures::Frame) {
    let resolution = fixture_resolution(frame.resolution());
    assert_eq!(
        resolution,
        reference.resolution(),
        "{label}: resolution differs from reference"
    );
    assert!(
        frame.data() == reference.data(),
        "{label}: pixels differ from reference"
    );
}

/// The first `limit` presentation timestamps derived from *probed* metadata,
/// so tests can enumerate frame times for videos without a spec.
pub fn probed_timestamps(video: &VideoMetadata, limit: usize) -> Vec<MediaTime> {
    match &video.timing {
        Timing::Constant(fps) => (0..video.frame_count)
            .take(limit)
            .map(|i| {
                video
                    .start
                    .advance_secs(Ratio::from_integer(u64::from(i)) * fps.frame_length())
            })
            .collect(),
        Timing::Variable(timestamps) => timestamps.iter().take(limit).copied().collect(),
    }
}
