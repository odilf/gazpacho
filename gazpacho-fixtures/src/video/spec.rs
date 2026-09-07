//! The synthetic `Spec` ground truth, mirrored from `scripts/specs.py`.
//!
//! Every generated synthetic clip stamps its frame index into its pixels, so
//! everything a test needs to predict what a reader should return is derivable
//! from a [`Spec`] — never from ffmpeg's opinion of its own output. Rationals
//! are `[numer, denom]` pairs in the manifest and `Ratio<i64>` here.

use std::ops::Range;

use num_rational::Ratio;
use serde::Deserialize;

use super::Resolution;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Codec {
    H264,
    Hevc,
    Vp9,
    Ffv1,
    Av1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PixFmt {
    Yuv420p,
    Yuv444p,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Container {
    Mp4,
    Mkv,
    #[serde(rename = "webm")]
    Webm,
    #[serde(rename = "ts")]
    Mpegts,
}

/// Frame timing of a spec. Ground-truth tests assert against this.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Timing {
    /// Constant frame rate.
    Cfr { fps: Ratio<i64> },
    /// Variable frame rate: exact per-frame durations (one per frame).
    /// Timestamps are prefix sums. Durations are whole milliseconds so mp4's
    /// 1/1000 track timescale stores them losslessly.
    Vfr { durations: Vec<Ratio<i64>> },
}

impl Timing {
    /// The exact time of frame `frame_index` relative to the first frame, in
    /// seconds (does *not* include `start_offset`).
    fn frame_length(&self, frame_index: u32) -> Ratio<i64> {
        match self {
            Self::Cfr { fps } => Ratio::from_integer(i64::from(frame_index)) / *fps,
            Self::Vfr { durations } => durations.iter().take(frame_index as usize).sum(),
        }
    }
}

/// Full description of one synthetic clip.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Spec {
    pub codec: Codec,
    pub container: Container,
    pub pix_fmt: PixFmt,
    pub timing: Timing,
    pub frames: u32,
    pub resolution: Resolution,
    /// Forced keyframe interval; 1 = all-intra. Scene-cut detection is
    /// disabled at encode time, so keyframes land exactly every `gop` frames.
    pub gop: u32,
    /// Max consecutive B-frames (H.264/HEVC only). Nonzero means decode
    /// order differs from presentation order.
    pub bframes: u32,
    /// Timestamp of the first frame, in seconds. Nonzero for the mpegts-style
    /// fixtures where the stream does not start at t = 0.
    pub start_offset: Ratio<i64>,
}

impl Spec {
    /// Exact presentation timestamp of frame `index`, in seconds (includes
    /// `start_offset`).
    pub fn timestamp_of(&self, index: u32) -> Ratio<i64> {
        assert!(index < self.frames, "frame {index} out of range");
        self.start_offset + self.timing.frame_length(index)
    }

    /// Exact display duration of frame `index`, in seconds.
    pub fn duration_of(&self, index: u32) -> Ratio<i64> {
        assert!(index < self.frames, "frame {index} out of range");
        match &self.timing {
            Timing::Cfr { fps } => fps.recip(),
            #[expect(
                clippy::indexing_slicing,
                reason = "index is asserted below `self.frames`, which is the durations length"
            )]
            Timing::Vfr { durations } => durations[index as usize],
        }
    }

    /// The exact time range the clip covers, in seconds: first timestamp to
    /// the end of the last frame.
    pub fn extent(&self) -> Range<Ratio<i64>> {
        let last = self.frames - 1;
        self.start_offset..self.timestamp_of(last) + self.duration_of(last)
    }
}
