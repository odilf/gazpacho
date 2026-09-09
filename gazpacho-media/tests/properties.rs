//! Helpers shared by the integration-test harnesses: the glue that needs
//! gazpacho-media types and therefore can't live in the (deliberately
//! independent) fixtures crate.

mod synthetic;
mod universal;

use eyre::WrapErr as _;
use gazpacho_datatypes::{Frame, Resolution, Time};
use gazpacho_fixtures::video::{self as fixtures, SyntheticVideo};
use gazpacho_fixtures::{init_tracing_stderr, props, test_video_properties, videos};
use gazpacho_media::MediaReader;
use gazpacho_media::metadata::{Timing, VideoMetadata};
use num_rational::Ratio;

test_video_properties! {
    props!([
        universal::metadata_loads, 1;
        universal::fast_load_agrees_with_full_decode, 3;
        universal::extent_is_self_consistent, 1;
        universal::sequential_read_matches_reference_decode, 4;
        universal::random_access_matches_sequential, 4;
        universal::out_of_extent_is_an_error, 1;
    ]);

    props!([
        synthetic::metadata_matches_spec, 1;
        synthetic::every_frame_recovers_its_index, 3;
        synthetic::mid_frame_times_return_the_covering_frame, 3;
        synthetic::downscaling_preserves_identity, 2;
        synthetic::bframe_reordering_is_invisible, 3;
    ], videos().synthetic.iter().collect());
}

// == Helpers ==

fn reader() -> MediaReader {
    init_tracing_stderr();
    MediaReader::default()
}

/// Seconds from the fixtures' spec math as a reader timestamp.
fn media_time(seconds: Ratio<i64>) -> Time {
    Time::from_secs(seconds)
}

/// A reader resolution as the fixtures type (for reference decodes).
pub fn fixture_resolution(resolution: Resolution) -> fixtures::Resolution {
    fixtures::Resolution {
        width: resolution.width,
        height: resolution.height,
    }
}

/// The index stamped in a decoded frame; errors with context if unreadable.
fn recovered(video: &SyntheticVideo, frame: &Frame) -> eyre::Result<u32> {
    fixtures::recover_index(fixture_resolution(frame.resolution()), frame.bytes())
        .wrap_err_with(|| format!("{}: unreadable stamp", video.name))
}

/// Assert a reader frame and a reference-decoded fixtures frame are
/// pixel-identical.
#[track_caller]
fn assert_frames_eq(label: &str, frame: &Frame, reference: &fixtures::Frame) {
    let resolution = fixture_resolution(frame.resolution());
    assert_eq!(
        resolution,
        reference.resolution(),
        "{label}: resolution differs from reference"
    );
    assert!(
        frame.bytes() == reference.bytes(),
        "{label}: pixels differ from reference"
    );
}

/// The first `limit` presentation timestamps derived from *probed* metadata,
/// so tests can enumerate frame times for videos without a spec.
fn probed_timestamps(video: &VideoMetadata, limit: usize) -> Vec<Time> {
    match &video.timing {
        Timing::Constant(fps) => (0..video.frame_count)
            .take(limit)
            .map(|i| {
                video
                    .extent
                    .start
                    .advance_secs(Ratio::from_integer(u64::from(i)) * fps.frame_length())
            })
            .collect(),
        Timing::Variable(timestamps) => timestamps.iter().take(limit).copied().collect(),
    }
}
