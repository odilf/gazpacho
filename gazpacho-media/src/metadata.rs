//! Media metadata probing.
//!
//! Rewrite in progress: the types below are the target shape, driven by
//! `tests/synthetic.rs` (run with `--features fixtures`). Pure time math is
//! implemented; probing is `todo!()`.

use std::{fmt, ops::Range};

use num_rational::Ratio;

use crate::read::Resolution;

/// Media-local time in seconds, exact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct MediaTime(pub(crate) Ratio<i64>);
impl MediaTime {
    fn from_duration_secs(secs: Ratio<u64>) -> MediaTime {
        MediaTime(Ratio::new(*secs.numer() as i64, *secs.denom() as i64))
    }

    pub fn advance_secs(&self, delta: Ratio<u64>) -> MediaTime {
        MediaTime(self.0 + MediaTime::from_duration_secs(delta).0)
    }
}

impl fmt::Display for MediaTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}s", self.0)
    }
}

/// Frame rate in frames per second, as an exact rational.
///
/// Exactness matters: NTSC rates like `24000/1001` accumulate drift if held
/// as floats, and frame-index math in the reader must be exact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fps(Ratio<u64>);

impl Fps {
    /// `None` unless `numer/denom` is a positive, finite rate.
    pub fn new(numer: u64, denom: u64) -> Option<Self> {
        (numer != 0 && denom != 0).then(|| Fps(Ratio::new(numer, denom)))
    }

    pub fn get(self) -> Ratio<u64> {
        self.0
    }

    /// Exact timestamp of frame `index`, relative to the stream's start.
    pub fn time_at(self, index: u32) -> Ratio<u64> {
        Ratio::from_integer(u64::from(index)) / self.0
    }

    /// Exact display duration of one frame.
    pub fn frame_length(self) -> Ratio<u64> {
        self.0.recip()
    }

    /// The frame on screen at `time` (relative to stream start): the largest
    /// `i` with `time_at(i) <= time`.
    pub fn frame_index_at(self, time: Ratio<u64>) -> u32 {
        u32::try_from((time * self.0).to_integer()).expect("frame index fits u32")
    }

    /// Frame index if `time` lies exactly on a frame boundary; errors
    /// otherwise. No float tolerance — rationals make "exact" meaningful.
    pub fn exact_frame_index(self, time: Ratio<u64>) -> eyre::Result<u32> {
        let index = time * self.0;
        eyre::ensure!(
            index.is_integer(),
            "time {time} is not on a frame boundary (frame {index})"
        );
        Ok(u32::try_from(index.to_integer()).expect("frame index fits u32"))
    }
}

/// Per-frame timing of a video stream.
#[derive(Debug, Clone, PartialEq)]
pub enum Timing {
    /// Constant frame rate: frame `i` is presented at `start + i / fps`.
    Constant(Fps),
    /// Variable frame rate: the exact absolute presentation timestamp of
    /// every frame, ascending (`timestamps[0] == start`).
    Variable(Box<[MediaTime]>),
}

#[derive(Debug, Clone)]
pub struct VideoMetadata {
    pub resolution: Resolution,
    pub timing: Timing,
    /// Presentation timestamp of the first frame (not always zero!, mpegts
    /// preload, edit lists, trimmed streams).
    pub start: MediaTime,
    /// Total number of frames.
    pub frame_count: u32,
    /// End of the last frame's display window, so the stream covers
    /// `start..end`.
    pub end: MediaTime,
    /// Keyframe frame indices. Ascending, starting at 0. Can be empty if
    /// keyframe info couldn't be retrieved.
    pub keyframes: Box<[u32]>,
}

impl VideoMetadata {
    /// The time range this stream covers.
    pub fn extent(&self) -> Range<MediaTime> {
        self.start..self.end
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AudioMetadata {
    pub sample_rate: u32,
    pub length: f64,
}

// TODO(streams rework): metadata is really a tree.
#[derive(Debug, Clone)]
pub struct MediaMetadata {
    pub video: Vec<VideoMetadata>,
    pub audio: Vec<AudioMetadata>,
}

impl MediaMetadata {
    pub fn load(path: &str) -> eyre::Result<Self> {
        todo!("Load metadata")
    }
}
