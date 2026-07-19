/// Frames per clip.
const FRAMES: u32 = 60;
const RESOLUTION: Resolution = Resolution {
    width: 160,
    height: 120,
};

use std::ops::Range;

use num_rational::Ratio;

use crate::{metadata::MediaTime, read::Resolution};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    H264,
    Hevc,
    Vp9,
    Ffv1,
}

impl Codec {
    pub fn encoder(self) -> &'static str {
        match self {
            Codec::H264 => "libx264",
            Codec::Hevc => "libx265",
            Codec::Vp9 => "libvpx-vp9",
            Codec::Ffv1 => "ffv1",
        }
    }

    fn tag(self) -> &'static str {
        match self {
            Codec::H264 => "h264",
            Codec::Hevc => "hevc",
            Codec::Vp9 => "vp9",
            Codec::Ffv1 => "ffv1",
        }
    }

    fn default_container(self) -> Container {
        match self {
            Codec::H264 | Codec::Hevc => Container::Mp4,
            Codec::Vp9 => Container::WebM,
            Codec::Ffv1 => Container::Mkv,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixFmt {
    Yuv420p,
    Yuv444p,
}

impl PixFmt {
    pub fn ffmpeg_name(self) -> &'static str {
        match self {
            PixFmt::Yuv420p => "yuv420p",
            PixFmt::Yuv444p => "yuv444p",
        }
    }

    fn tag(self) -> &'static str {
        match self {
            PixFmt::Yuv420p => "420p",
            PixFmt::Yuv444p => "444p",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Container {
    Mp4,
    Mkv,
    WebM,
    MpegTs,
}

impl Container {
    pub fn ext(self) -> &'static str {
        match self {
            Container::Mp4 => "mp4",
            Container::Mkv => "mkv",
            Container::WebM => "webm",
            Container::MpegTs => "ts",
        }
    }
}

/// Frame timing of a spec. Ground truth tests assert against.
///
/// Different way of storing VFR than [`crate::metadata::Timing`] (although I
/// don't know if the difference is justifiable...)
#[derive(Debug, Clone, PartialEq)]
pub enum Timing {
    /// Constant frame rate.
    Cfr { fps: Ratio<u64> },
    /// Variable frame rate: exact per-frame durations (one per frame).
    /// Timestamps are prefix sums. Durations are whole milliseconds so mp4's
    /// 1/1000 track timescale stores them losslessly.
    Vfr { durations: Vec<Ratio<u64>> },
}

impl Timing {
    pub fn frame_length(&self, frame_index: u32) -> Ratio<u64> {
        match self {
            Self::Cfr { fps } => Ratio::from_integer(u64::from(frame_index)) / fps,
            Self::Vfr { durations } => durations.iter().take(frame_index as usize).sum(),
        }
    }
}

/// Full description of one synthetic clip. Everything a test needs to predict
/// what the reader should return is derivable from here.
#[derive(Debug, Clone)]
pub struct Spec {
    pub name: String,
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
    /// Timestamp of the first frame. Nonzero for the mpegts-style fixtures
    /// where the stream does not start at t = 0.
    pub start_offset: MediaTime,
}

impl Spec {
    /// Exact presentation timestamp of frame `index` (includes `start_offset`).
    pub fn timestamp_of(&self, index: u32) -> MediaTime {
        assert!(index < self.frames, "frame {index} out of range");
        self.start_offset
            .advance_secs(self.timing.frame_length(index))
    }

    /// Exact display duration of frame `index`.
    pub fn duration_of(&self, index: u32) -> Ratio<u64> {
        assert!(index < self.frames, "frame {index} out of range");
        match &self.timing {
            Timing::Cfr { fps } => fps.recip(),
            Timing::Vfr { durations } => durations[index as usize],
        }
    }

    /// The exact time range the clip covers: first timestamp to the end of
    /// the last frame.
    pub fn extent(&self) -> Range<MediaTime> {
        let last = self.frames - 1;
        self.start_offset..self.timestamp_of(last).advance_secs(self.duration_of(last))
    }

    pub fn file_name(&self) -> String {
        format!("{}.{}", self.name, self.container.ext())
    }
}

fn fps_tag(fps: Ratio<u64>) -> String {
    if *fps.denom() == 1 {
        fps.numer().to_string()
    } else if fps == Ratio::new(24000, 1001) {
        "ntsc".to_owned()
    } else {
        format!("{}-{}", fps.numer(), fps.denom())
    }
}

/// The whole corpus: a full sweep of codec x GOP x fps x pixel format, plus
/// targeted VFR, B-frame, and nonzero-start fixtures.
pub fn all_specs() -> Vec<Spec> {
    let r30 = Ratio::from_integer(30);
    let ntsc = Ratio::new(24000, 1001);
    let zero = Ratio::from_integer(0);

    let base = |codec: Codec, gop: u32, fps: Ratio<u64>, pix_fmt: PixFmt| Spec {
        name: format!(
            "{}_{}_g{}_{}",
            codec.tag(),
            pix_fmt.tag(),
            gop,
            fps_tag(fps)
        ),
        codec,
        container: codec.default_container(),
        pix_fmt,
        timing: Timing::Cfr { fps },
        frames: FRAMES,
        resolution: RESOLUTION,
        gop,
        bframes: 0,
        start_offset: MediaTime(zero),
    };

    let mut specs = Vec::new();

    // Inter codecs: sweep GOP structure (all-intra / normal / longer than the
    // whole clip) against both fps (integer and NTSC rational) and both chroma
    // samplings.
    for codec in [Codec::H264, Codec::Hevc, Codec::Vp9] {
        for gop in [1, 12, 250] {
            for fps in [r30, ntsc] {
                for pix_fmt in [PixFmt::Yuv420p, PixFmt::Yuv444p] {
                    specs.push(base(codec, gop, fps, pix_fmt));
                }
            }
        }
    }

    // FFV1 is intra-only (well, we keep it that way): lossless reference.
    for fps in [r30, ntsc] {
        for pix_fmt in [PixFmt::Yuv420p, PixFmt::Yuv444p] {
            specs.push(base(Codec::Ffv1, 1, fps, pix_fmt));
        }
    }

    // Variable frame rate: irregular but exact millisecond durations.
    let pattern = [33u64, 21, 100, 40, 15, 67];
    let durations = (0..FRAMES)
        .map(|i| Ratio::new(pattern[i as usize % pattern.len()], 1000))
        .collect();
    specs.push(Spec {
        name: "vfr_h264".to_owned(),
        timing: Timing::Vfr { durations },
        ..base(Codec::H264, 12, r30, PixFmt::Yuv420p)
    });

    // B-frames: decode order != presentation order (negative DTS / mp4 edit
    // list). The reader must hand frames back in presentation order.
    specs.push(Spec {
        name: "h264_bf2".to_owned(),
        bframes: 2,
        ..base(Codec::H264, 12, r30, PixFmt::Yuv420p)
    });

    // B-frames plus a first PTS of 0.7s in mp4.
    specs.push(Spec {
        name: "h264_bf2_offset".to_owned(),
        bframes: 2,
        start_offset: MediaTime(Ratio::new(7, 10)),
        ..base(Codec::H264, 12, r30, PixFmt::Yuv420p)
    });

    // The classic broadcast-style case: mpegts starting at 1.4s, with
    // B-frames. `t = 0` is *before* this stream exists.
    specs.push(Spec {
        name: "h264_bf2_ts".to_owned(),
        container: Container::MpegTs,
        bframes: 2,
        start_offset: MediaTime(Ratio::new(7, 5)),
        ..base(Codec::H264, 12, r30, PixFmt::Yuv420p)
    });

    specs
}
