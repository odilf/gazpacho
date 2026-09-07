//! Reading of media files via [`MediaReader`].

use std::collections::HashMap;
use std::fs;
use std::num::NonZeroU8;
use std::time::SystemTime;

use eyre::{Context, OptionExt as _};
use gazpacho_datatypes::{Extent, Frame, Resolution, Time};

use crate::metadata::MediaMetadata;
use crate::read::sequential::SequentialReader;

mod pipe;
mod random;
mod sequential;

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
    metadata_cache: MetadataCache,
    sequential: SequentialReader,
}

type MetadataCache = HashMap<String, (MediaMetadata, Option<SystemTime>)>;

impl MediaReader {
    pub fn new() -> Self {
        MediaReader {
            metadata_cache: MetadataCache::new(),
            sequential: SequentialReader::new(),
        }
    }

    pub fn ensure_metadata_cached(&mut self, path: &str) -> eyre::Result<()> {
        let meta = fs::metadata(path)?;
        let mtime = meta.modified().ok();

        // We could also do it with entries, but that forces to re-allocate the
        // string and I guess a lookup is cheaper than an allocation, especially
        // since the second will be almost certainly a cache hit.
        let stale = !matches!(self.metadata_cache.get(path), Some((_, cached_mtime)) if *cached_mtime == mtime);
        if stale {
            let metadata = MediaMetadata::load(path)?;
            self.metadata_cache
                .insert(path.to_string(), (metadata, mtime));
        }

        Ok(())
    }

    pub fn metadata(&mut self, path: &str) -> eyre::Result<&MediaMetadata> {
        self.ensure_metadata_cached(path)?;
        #[expect(clippy::indexing_slicing, reason = "just ensured above")]
        let (meta, _time) = &self.metadata_cache[path];
        Ok(meta)
    }

    pub fn extent(&mut self, path: &str) -> eyre::Result<Extent> {
        Ok(self
            .metadata(path)?
            .video
            .first()
            .ok_or_eyre("Expected at least one video stream")?
            .extent)
    }

    pub fn frame(
        &mut self,
        path: &str,
        time: Time,
        resolution: ResolutionRequest,
        access_pattern: AccessPattern,
    ) -> eyre::Result<Frame> {
        self.frame_of_stream(path, time, resolution, access_pattern, None)
    }

    /// Like [`frame`](Self::frame), but reading the video stream at container
    /// index `stream_index` (`None` picks the first video stream).
    pub fn frame_of_stream(
        &mut self,
        path: &str,
        time: Time,
        resolution: ResolutionRequest,
        access_pattern: AccessPattern,
        stream_index: Option<u8>,
    ) -> eyre::Result<Frame> {
        self.ensure_metadata_cached(path)?;
        #[expect(clippy::indexing_slicing, reason = "just ensured above")]
        let (metadata, _path) = &self.metadata_cache[path];

        let video = match stream_index {
            None => metadata.video.first().ok_or_eyre("no video streams")?,
            Some(index) => metadata
                .video
                .iter()
                .find(|v| v.stream_index == index)
                .ok_or_else(|| eyre::eyre!("no video stream at container index {index}"))?,
        };

        let extent = video.extent;
        eyre::ensure!(
            extent.contains(&time),
            "t={time} is outside the stream extent {}..{}",
            extent.start,
            extent.end,
        );

        let resolution = resolution.resolve(video.resolution);
        match access_pattern {
            AccessPattern::Sequential => self.sequential.frame(path, time, resolution, video),
            AccessPattern::Random => {
                tracing::warn!("random access pattern not implemented yet, using sequential");
                self.sequential.frame(path, time, resolution, video)
            }
        }
        .wrap_err_with(|| format!("Failed to fetch frame at {time} from '{path}'"))
    }
}

impl Clone for MediaReader {
    /// This clone implementation closes the ffmpeg readers (but keeps the caches).
    fn clone(&self) -> Self {
        Self {
            metadata_cache: self.metadata_cache.clone(),
            sequential: SequentialReader::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
            downsample: NonZeroU8::MIN,
        }
    }

    /// The concrete resolution to decode at, given the stream's native one.
    pub fn resolve(self, native: Resolution) -> Resolution {
        match self {
            Self::Auto { downsample } => {
                let downsample = u32::from(downsample.get());
                Resolution {
                    width: (native.width / downsample).max(1),
                    height: (native.height / downsample).max(1),
                }
            }
            Self::Manual(resolution) => resolution,
        }
    }
}

#[cfg(test)]
mod tests {
    use gazpacho_fixtures::video::{self as fixtures, recover_index};
    use num_traits::Zero;

    use crate::{MediaReader, read::AccessPattern};

    use super::*;

    /// The stamped index in a reader-produced frame, via the fixtures oracle.
    fn recovered(frame: &Frame) -> u32 {
        let Resolution { width, height } = frame.resolution();
        // TODO: It would be better to change `recover_index` to use `&[[u8; 4]]`.
        recover_index(fixtures::Resolution { width, height }, frame.bytes()).unwrap()
    }

    /// Streams that don't start at t = 0: the extent begins at the true first
    /// PTS, frame 0 lives *there*, and t = 0 is out of range.
    #[test]
    fn nonzero_start_is_respected() {
        let fixtures = fixtures::videos();
        let mut reader = MediaReader::default();
        for (video, spec) in fixtures
            .spec_backed()
            .filter(|(_video, spec)| !spec.start_offset.is_zero())
        {
            let extent = reader.extent(&video.path).unwrap();
            assert_eq!(
                extent.start,
                Time::from_secs(spec.start_offset),
                "{}",
                video.name
            );

            // Frame 0 is at the offset, not at zero.
            let frame = reader
                .frame(
                    &video.path,
                    extent.start,
                    ResolutionRequest::auto(),
                    // TODO: Test non-sequential access pattern.
                    AccessPattern::Sequential,
                )
                .unwrap_or_else(|err| panic!("{} at extent.start: {err}", video.name));
            assert_eq!(recovered(&frame), 0, "{}", video.name);

            // t = 0 is before the stream exists.
            let before = reader.frame(
                &video.path,
                Time::ZERO,
                ResolutionRequest::auto(),
                // TODO: Test non-sequential access pattern.
                AccessPattern::Sequential,
            );
            assert!(
                before.is_err(),
                "{}: t=0 should be out of extent",
                video.name
            );
        }
    }
}
