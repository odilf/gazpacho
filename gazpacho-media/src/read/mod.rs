//! Reading of media files via [`MediaReader`].

use std::collections::HashMap;
use std::num::NonZeroU8;
use std::ops::Range;
use std::sync::Mutex;
use std::time::SystemTime;
use std::{fmt, fs};

use crate::metadata::{MediaMetadata, MediaTime};
use crate::read::sequential::SequentialReader;

mod random;
mod sequential;

/// A CPU frame: RGBA8, row-major, tightly packed.
#[derive(Clone, PartialEq, Eq)]
pub struct Frame {
    resolution: Resolution,
    data: Box<[u8]>,
}

impl Frame {
    pub fn new(resolution: Resolution, data: impl Into<Box<[u8]>>) -> Self {
        let data = data.into();
        let area = resolution.width * resolution.height;
        assert_eq!(data.len() as u32, area * 4);

        Self { resolution, data }
    }

    pub fn resolution(&self) -> Resolution {
        self.resolution
    }

    pub fn get(&self, x: u32, y: u32) -> [u8; 4] {
        let i = 4 * (y * self.resolution.width + x) as usize;
        self.data[i..i + 4].try_into().unwrap()
    }

    pub(crate) fn data(&self) -> &[u8] {
        &self.data
    }
}

impl fmt::Debug for Frame {
    // Manual impl: dumping megabytes of pixels into assert messages helps no one.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Frame")
            .field("resolution", &self.resolution)
            // NIT: Allocation can be avoided.
            .field("data", &format!("<{}-byte array>", self.data.len()))
            .finish()
    }
}

pub enum AccessPattern {
    Sequential,
    Random,
}

/// Reading of media (i.e., audio and video).
///
/// Target behavior (see `tests/synthetic.rs`):
/// - Frames are addressed by exact rational time; `frame(t)` returns the
///   frame whose display window `[pts, pts + duration)` contains `t`,
///   `t` does not need to lie exactly on a frame boundary.
/// - Streams do not necessarily start at `t = 0` (mpegts preload, edit
///   lists).
/// - B-frame decode order is never visible to the caller: results are in
///   presentation order.
#[derive(Debug, Default)]
pub struct MediaReader {
    metadata_cache: Mutex<HashMap<String, (MediaMetadata, Option<SystemTime>)>>,
    sequential: SequentialReader,
}

impl MediaReader {
    /// The time range covered by the first video stream: `start` is the
    /// first frame's timestamp (not necessarily zero), `end` the end of the
    /// last frame's display window.
    pub fn extent(&self, path: &str) -> eyre::Result<Range<MediaTime>> {
        let meta = fs::metadata(path)?;
        let mtime = meta.modified().ok();

        let mut cache = self.metadata_cache.lock().unwrap();

        // We could also do it with entries, but that forces to re-allocate the
        // string and I guess a lookup is cheaper than an allocation, especially
        // since the second will be almost certainly a cache hit.
        let stale = !matches!(cache.get(path), Some((_, cached_mtime)) if *cached_mtime == mtime);
        if stale {
            let metadata = MediaMetadata::load(path)?;
            cache.insert(path.to_string(), (metadata, mtime));
        }

        Ok(cache[path].0.video[0].extent())
    }

    /// Decode the frame visible at `time`, scaled to `resolution`.
    ///
    /// `time` must lie within [`extent`](Self::extent); the frame whose
    /// display window contains `time` is returned.
    pub fn frame(
        &self,
        path: &str,
        time: MediaTime,
        resolution: ResolutionRequest,
        access_pattern: AccessPattern,
    ) -> eyre::Result<Frame> {
        match access_pattern {
            AccessPattern::Sequential => self.sequential.frame(path, time, resolution),
            AccessPattern::Random => todo!(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ResolutionRequest {
    /// Get the native resolution, optionally downsampling by some factor (useful for preview)
    Auto {
        downsample: NonZeroU8,
    },
    Manual(Resolution),
}

impl ResolutionRequest {
    pub const fn auto() -> Self {
        Self::Auto {
            downsample: NonZeroU8::new(1).unwrap(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resolution {
    pub width: u32,
    pub height: u32,
}

impl fmt::Display for Resolution {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}x{}", self.width, self.height)
    }
}

#[cfg(test)]
mod tests {
    use num_rational::Ratio;

    use crate::{
        MediaReader,
        fixtures::{self, recovered},
        read::AccessPattern,
    };

    use super::*;

    /// Streams that don't start at t = 0: the extent begins at the true first
    /// PTS, frame 0 lives *there*, and t = 0 is out of range.
    #[test]
    fn nonzero_start_is_respected() {
        let corpus = fixtures::corpus();
        let reader = MediaReader::default();
        for name in ["h264_bf2_offset", "h264_bf2_ts"] {
            let fixture = corpus.expect(name);
            let extent = reader.extent(fixture.path_str()).unwrap();
            assert_eq!(extent.start, fixture.spec.start_offset, "{name}");

            // Frame 0 is at the offset, not at zero.
            let frame = reader
                .frame(
                    fixture.path_str(),
                    extent.start,
                    ResolutionRequest::auto(),
                    // TODO: Test non-sequential access pattern.
                    AccessPattern::Sequential,
                )
                .unwrap_or_else(|err| panic!("{name} at extent.start: {err}"));
            assert_eq!(recovered(fixture, &frame), 0, "{name}");

            // t = 0 is before the stream exists.
            let before = reader.frame(
                fixture.path_str(),
                // todo!(""),
                MediaTime(Ratio::from_integer(0)),
                ResolutionRequest::auto(),
                // TODO: Test non-sequential access pattern.
                AccessPattern::Sequential,
            );
            assert!(before.is_err(), "{name}: t=0 should be out of extent");
        }
    }
}
