//! Test videos for the reader, organized as a registry of several [`Kind`]s:
//! the fixed synthetic matrix, seeded random specs, derived edge cases, the
//! downloaded Chromium media test corpus (`GAZPACHO_CHROMIUM_VIDEOS=0` to
//! disable), and optional real-world files (`GAZPACHO_REAL_VIDEOS_DIR`).
//!
//! Every *generated* clip encodes its own ground truth: frame `i`'s pixels are
//! a 4x4 grid of black/white blocks spelling `i` in binary ([`stamp`]), so
//! after decoding through *any* path, [`recover_index`] tells you exactly
//! which frame you got, at any resolution, through any lossy codec. Such
//! videos carry `spec: Some(..)`; tests assert against [`Spec`] math (which
//! frame *should* be visible at time `t`), never against ffmpeg's opinion of
//! its own output, so we don't end up testing ffmpeg with ffmpeg. Spec-less
//! videos (derived, real-world) get self-consistency properties only.
//!
//! No special machinery is needed to generate before tests run: [`videos`]
//! is lazy (`OnceLock`) and idempotent on disk. Files are stored in
//! `target/synthetic-fixtures/<version>/`. Concurrent test binaries are safe
//! because generation writes to a temp name and `rename`s into place.
//!
//! Iteration can be cut down for quick local runs with
//! `GAZPACHO_TEST_VIDEOS=sample:<N>[:<seed>]` (see [`Registry::all`]).
//!
//! [`VERSION`] needs to be bumped to invalidate the cache if generation logic
//! changes.

use std::path::Path;
use std::process::{Command, Stdio};

use eyre::{WrapErr as _, ensure};
use ffmpeg_sidecar::paths::ffmpeg_path;
use num_rational::Ratio;

use crate::MediaReader;
use crate::metadata::MediaTime;
use crate::read::{Frame, Resolution};

/// To invalidate the cache.
const VERSION: &str = "v6";

mod chromium;
mod generation;
mod random;
mod registry;
mod spec;

pub use generation::{BASELINE, recover_index, stamp};
pub use registry::{
    Kind, Registry, TestVideo, baseline_with_audio, baseline_with_cover_art, trimmed_baseline,
    videos,
};
pub use spec::{Codec, Container, PixFmt, Spec, Timing};

// === Helpers for tests ======================================================

/// Decode *every* frame of a file to RGBA via a plain ffmpeg pipe, in
/// presentation order.
///
/// This is a decode path independent of [`crate::read`], used to validate the
/// fixtures themselves and as a reference to compare the reader against.
pub fn decode_all_rgba(path: &Path, resolution: Resolution) -> eyre::Result<Vec<Frame>> {
    decode_rgba(path, None, resolution, None)
}

/// Like [`decode_all_rgba`], but reads the stream at container index
/// `stream_index` and stops after `limit` frames — for comparing against a
/// reader on multi-track or real-world files too large to hold decoded in
/// memory.
pub fn decode_rgba_prefix(
    path: &Path,
    stream_index: u8,
    resolution: Resolution,
    limit: usize,
) -> eyre::Result<Vec<Frame>> {
    decode_rgba(path, Some(stream_index), resolution, Some(limit))
}

fn decode_rgba(
    path: &Path,
    stream_index: Option<u8>,
    resolution: Resolution,
    limit: Option<usize>,
) -> eyre::Result<Vec<Frame>> {
    let Resolution { width, height } = resolution;
    let mut cmd = Command::new(ffmpeg_path());
    cmd.args(["-hide_banner", "-loglevel", "error"])
        .arg("-i")
        .arg(path)
        // One output frame per coded frame — otherwise ffmpeg CFR-izes VFR
        // files by duplicating frames to fill the timestamp gaps.
        .args(["-fps_mode", "passthrough"]);
    if let Some(index) = stream_index {
        cmd.args(["-map", &format!("0:{index}")]);
    }
    if let Some(limit) = limit {
        cmd.args(["-frames:v", &limit.to_string()]);
    }
    let output = cmd
        .args(["-f", "rawvideo", "-pix_fmt", "rgba", "-"])
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .wrap_err("running ffmpeg to decode")?;
    ensure!(
        output.status.success(),
        "ffmpeg decode failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );

    let frame_size = (width * height * 4) as usize;
    ensure!(
        output.stdout.len() % frame_size == 0,
        "decoded byte count {} is not a whole number of {width}x{height} RGBA frames",
        output.stdout.len()
    );
    Ok(output
        .stdout
        .chunks_exact(frame_size)
        .map(|data| Frame::new(resolution, data))
        .collect())
}

/// Install a `RUST_LOG`-aware tracing subscriber for tests. Defaults to
/// `gazpacho_media=debug` so fixture generation is visible under `cargo test
/// -- --nocapture`.
#[track_caller]
pub fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("gazpacho_media=debug"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_test_writer()
        .try_init()
        .unwrap_or_else(|err| tracing::warn!(err, "Tracer already initialized."));
}

pub fn reader() -> MediaReader {
    init_tracing();
    MediaReader::default()
}

/// The index stamped in a decoded frame; panics with context if unreadable.
pub fn recovered(video: &TestVideo, frame: &Frame) -> u32 {
    assert!(
        video.spec.is_some(),
        "{}: only spec-backed videos carry frame stamps",
        video.name
    );
    recover_index(&frame).unwrap_or_else(|err| panic!("{}: unreadable stamp: {err}", video.name))
}

/// The first `limit` presentation timestamps derived from *probed* metadata,
/// so tests can enumerate frame times for videos without a spec.
pub fn probed_timestamps(video: &crate::metadata::VideoMetadata, limit: usize) -> Vec<MediaTime> {
    match &video.timing {
        crate::metadata::Timing::Constant(fps) => (0..video.frame_count)
            .take(limit)
            .map(|i| {
                video
                    .start
                    .advance_secs(Ratio::from_integer(u64::from(i)) * fps.frame_length())
            })
            .collect(),
        crate::metadata::Timing::Variable(timestamps) => {
            timestamps.iter().take(limit).copied().collect()
        }
    }
}

// === Self-tests: validate the oracle without involving the reader ==========

#[cfg(test)]
mod tests {
    use crate::read::Resolution;

    use super::*;

    #[test]
    fn stamp_roundtrips_in_memory() {
        let res = Resolution {
            width: 160,
            height: 120,
        };
        for index in [0, 1, 42, 0x5555, 0xAAAA, 0xFFFF] {
            let stamped_frame = stamp(res, index);
            assert_eq!(recover_index(&stamped_frame).unwrap(), index);
        }
    }

    #[test]
    fn registry_generates() {
        init_tracing();
        let registry = videos();
        let generated: Vec<_> = registry
            .all_full()
            .iter()
            .filter(|v| v.spec.is_some())
            .collect();
        assert!(
            generated.len() >= 40,
            "expected the full matrix, got {} fixtures",
            generated.len()
        );
        for video in registry.all_full() {
            let size = std::fs::metadata(&video.path)
                .unwrap_or_else(|_| panic!("{} missing", video.path.display()))
                .len();
            assert!(size > 0, "{} is empty", video.name);
        }
        // Names must be unique: they key lookups and label failures.
        let mut names: Vec<_> = registry.all_full().iter().map(|v| &v.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), registry.all_full().len(), "duplicate names");
        // Targeted lookups tests rely on must always exist, even in samples.
        for name in [
            BASELINE,
            "h264_420p_g250_30",
            "vfr_h264",
            "h264_bf2",
            "h264_bf2_offset",
            "h264_bf2_ts",
            "trimmed",
            "with_audio",
            "with_cover",
        ] {
            registry.expect(name);
        }
    }

    /// The core oracle check: after a full encode → decode round trip through
    /// an *independent* ffmpeg pipe, every frame still announces its index,
    /// in presentation order. Covers the lossiest codec, VFR, B-frame
    /// reordering, and the seeded random specs.
    #[test]
    fn stamp_survives_encode_and_decode() {
        init_tracing();
        for video in videos().all_full() {
            let Some(spec) = &video.spec else { continue };
            let name = &video.name;
            let frames = decode_all_rgba(&video.path, spec.resolution).unwrap();
            assert_eq!(frames.len(), spec.frames as usize, "{name}");
            for (i, frame) in frames.iter().enumerate() {
                let recovered =
                    recover_index(&frame).unwrap_or_else(|err| panic!("{name} frame {i}: {err}"));
                assert_eq!(recovered, i as u32, "{name} frame {i}");
            }
        }
    }
}
