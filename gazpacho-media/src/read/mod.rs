//! Reading of media files via [`MediaReader`].

use std::collections::HashMap;
use std::fs;
use std::num::NonZeroU8;
use std::sync::{Mutex, MutexGuard};
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
    metadata_cache: Mutex<MetadataCache>,
    sequential: SequentialReader,
}

type MetadataCache = HashMap<String, (MediaMetadata, Option<SystemTime>)>;

impl MediaReader {
    pub fn new() -> Self {
        MediaReader {
            metadata_cache: Mutex::new(MetadataCache::new()),
            sequential: SequentialReader::new(),
        }
    }
    /// The metadata cache with `path` guaranteed present and fresh,
    /// (re)probing if the file changed on disk. Index the guard with `path`.
    fn metadata(&self, path: &str) -> eyre::Result<MutexGuard<'_, MetadataCache>> {
        let meta = fs::metadata(path)?;
        let mtime = meta.modified().ok();

        let mut cache = self
            .metadata_cache
            .lock()
            .map_err(|_poison| eyre::eyre!("poisoned lock"))?;

        // We could also do it with entries, but that forces to re-allocate the
        // string and I guess a lookup is cheaper than an allocation, especially
        // since the second will be almost certainly a cache hit.
        let stale = !matches!(cache.get(path), Some((_, cached_mtime)) if *cached_mtime == mtime);
        if stale {
            let metadata = MediaMetadata::load(path)?;
            cache.insert(path.to_string(), (metadata, mtime));
        }

        Ok(cache)
    }

    /// The time range covered by the first video stream: `start` is the
    /// first frame's timestamp (not necessarily zero), `end` the end of the
    /// last frame's display window.
    pub fn extent(&self, path: &str) -> eyre::Result<Extent> {
        let cache = self.metadata(path)?;
        let (metadata, _) = cache
            .get(path)
            .expect("metadata(path) always inserts path before returning");
        Ok(metadata
            .video
            .first()
            .ok_or_eyre("no video streams")?
            .extent())
    }

    /// Decode the frame of the first video stream visible at `time`, scaled
    /// to `resolution`.
    ///
    /// `time` must lie within [`extent`](Self::extent); the frame whose
    /// display window contains `time` is returned.
    pub fn frame(
        &self,
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
        &self,
        path: &str,
        time: Time,
        resolution: ResolutionRequest,
        access_pattern: AccessPattern,
        stream_index: Option<u8>,
    ) -> eyre::Result<Frame> {
        let cache = self.metadata(path)?;
        let (metadata, _) = cache
            .get(path)
            .expect("metadata(path) always inserts path before returning");
        let video = match stream_index {
            None => metadata.video.first().ok_or_eyre("no video streams")?,
            Some(index) => metadata
                .video
                .iter()
                .find(|v| v.stream_index == index)
                .ok_or_else(|| eyre::eyre!("no video stream at container index {index}"))?,
        };

        let extent = video.extent();
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
    use gazpacho_fixtures as fixtures;

    use crate::{MediaReader, read::AccessPattern};

    use super::*;

    /// The stamped index in a reader-produced frame, via the fixtures oracle.
    fn recovered(frame: &Frame) -> u32 {
        let Resolution { width, height } = frame.resolution();
        // TODO: It would be better to change `recover_index` to use `&[[u8; 4]]`.
        fixtures::recover_index(fixtures::Resolution { width, height }, frame.bytes()).unwrap()
    }

    /// Streams that don't start at t = 0: the extent begins at the true first
    /// PTS, frame 0 lives *there*, and t = 0 is out of range.
    #[test]
    fn nonzero_start_is_respected() {
        let videos = fixtures::videos();
        let reader = MediaReader::default();
        for name in ["h264_bf2_offset", "h264_bf2_ts"] {
            let video = videos.expect(name).unwrap();
            let extent = reader.extent(video.path_str()).unwrap();
            assert_eq!(
                extent.start,
                Time::from_secs(video.expect_spec().unwrap().start_offset),
                "{name}"
            );

            // Frame 0 is at the offset, not at zero.
            let frame = reader
                .frame(
                    video.path_str(),
                    extent.start,
                    ResolutionRequest::auto(),
                    // TODO: Test non-sequential access pattern.
                    AccessPattern::Sequential,
                )
                .unwrap_or_else(|err| panic!("{name} at extent.start: {err}"));
            assert_eq!(recovered(&frame), 0, "{name}");

            // t = 0 is before the stream exists.
            let before = reader.frame(
                video.path_str(),
                Time::ZERO,
                ResolutionRequest::auto(),
                // TODO: Test non-sequential access pattern.
                AccessPattern::Sequential,
            );
            assert!(before.is_err(), "{name}: t=0 should be out of extent");
        }
    }
}
